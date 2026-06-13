//! GDScript coverage instrumentation.
//!
//! Inserts `Gdt._cov_hit("res://path", line)` before each executable line
//! inside function bodies. Uses tree-sitter to identify function body line
//! ranges; blank lines and comment lines are skipped via text heuristics.
//!
//! The caller is responsible for backing up original files before writing the
//! instrumented bytes and restoring them after the test run.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use tree_sitter::Parser;

// ── Public API ────────────────────────────────────────────────────────────────

/// Instrument a GDScript source file for coverage tracking.
/// Returns `(instrumented_bytes, instrumented_line_numbers)`.
/// `instrumented_line_numbers` is the set of 1-based line numbers that received
/// a hit-tracking call — this is the "lines found" (LF) count for LCOV.
pub fn instrument_file(path: &Path, res_path: &str) -> Result<(Vec<u8>, HashSet<u32>)> {
    let source = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(instrument_source(&source, res_path))
}

/// Instrument raw GDScript bytes. Returns `(instrumented_bytes, instrumented_lines)`.
pub fn instrument_source(source: &[u8], res_path: &str) -> (Vec<u8>, HashSet<u32>) {
    let body_lines = find_function_body_lines(source);
    if body_lines.is_empty() {
        return (source.to_vec(), HashSet::new());
    }

    let source_str = match std::str::from_utf8(source) {
        Ok(s) => s,
        Err(_) => return (source.to_vec(), HashSet::new()),
    };

    let escaped = res_path.replace('\\', "\\\\").replace('"', "\\\"");
    let mut result = String::with_capacity(source.len() + body_lines.len() * 60);
    let mut instrumented: HashSet<u32> = HashSet::new();

    for (i, line) in source_str.lines().enumerate() {
        let line_num = (i + 1) as u32;
        let trimmed = line.trim();

        if body_lines.contains(&line_num) && !trimmed.is_empty() && !trimmed.starts_with('#') {
            let ws = leading_whitespace(line);
            result.push_str(ws);
            result.push_str(&format!(r#"Gdt._cov_hit("{escaped}", {line_num})"#));
            result.push('\n');
            instrumented.insert(line_num);
        }
        result.push_str(line);
        result.push('\n');
    }

    (result.into_bytes(), instrumented)
}

// ── Internals ─────────────────────────────────────────────────────────────────

fn leading_whitespace(line: &str) -> &str {
    let trimmed_len = line.trim_start().len();
    &line[..line.len() - trimmed_len]
}

/// Returns all 1-based line numbers that lie inside some function body.
fn find_function_body_lines(source: &[u8]) -> HashSet<u32> {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_gdscript::LANGUAGE.into())
        .is_err()
    {
        return HashSet::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return HashSet::new();
    };
    let mut lines = HashSet::new();
    collect_body_lines(tree.root_node(), &mut lines);
    lines
}

fn collect_body_lines(node: tree_sitter::Node, lines: &mut HashSet<u32>) {
    if node.kind() == "function_definition" {
        let func_start_row = node.start_position().row as u32; // 0-based
        let func_end_row = node.end_position().row as u32;

        // Find the explicit 'body' child to get the precise body start row.
        // Look for explicit 'body' child to get the precise body start.
        // In tree-sitter-gdscript the body node begins on the same row as the
        // `func` keyword, so only use it when strictly after the header row
        // (multi-line signatures). For single-line headers fall back to
        // func_start_row + 1 so the header line is never instrumented.
        let mut body_start_row = func_start_row + 1;
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                if child.kind() == "body" {
                    let bsr = child.start_position().row as u32;
                    if bsr > func_start_row {
                        body_start_row = bsr;
                    }
                    break;
                }
            }
        }

        for row0 in body_start_row..=func_end_row {
            lines.insert(row0 + 1); // 0-based → 1-based
        }
        // Still descend so inner-class methods are also collected.
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            collect_body_lines(child, lines);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruments_function_body() {
        let src = br#"extends RefCounted

func take_damage(amount: int) -> void:
    health -= amount
    if health <= 0:
        die()

func heal(amount: int) -> void:
    pass
"#;
        let (out, instrumented) = instrument_source(src, "res://src/player.gd");
        let s = std::str::from_utf8(&out).unwrap();
        assert!(
            s.contains(r#"Gdt._cov_hit("res://src/player.gd","#),
            "missing hit call:\n{s}"
        );
        // Lines 4, 5, 6 (take_damage body) and 9 (heal body) should be instrumented.
        assert!(
            instrumented.contains(&4),
            "line 4 not instrumented: {instrumented:?}"
        );
        assert!(
            instrumented.contains(&9),
            "line 9 not instrumented: {instrumented:?}"
        );
        // `extends` (line 1) and `func` headers (lines 3, 8) must NOT be instrumented.
        assert!(
            !instrumented.contains(&1),
            "extends should not be instrumented"
        );
        assert!(
            !instrumented.contains(&3),
            "func header should not be instrumented"
        );
    }

    #[test]
    fn skips_comments_and_blanks() {
        let src = br#"func foo() -> void:
    # a comment
    var x = 1
"#;
        let (_, instrumented) = instrument_source(src, "res://test.gd");
        assert!(
            !instrumented.contains(&2),
            "comment line must not be instrumented"
        );
        assert!(instrumented.contains(&3), "var line should be instrumented");
    }

    #[test]
    fn no_instrumentation_outside_functions() {
        let src = br#"extends Node

var speed := 100.0

func _ready() -> void:
    speed = 200.0
"#;
        let (_, instrumented) = instrument_source(src, "res://player.gd");
        // Class-level var — outside any function, must NOT be instrumented.
        assert!(
            !instrumented.contains(&3),
            "class-level var should not be instrumented"
        );
        // Assignment inside _ready — should be instrumented.
        assert!(
            instrumented.contains(&6),
            "assignment inside _ready should be instrumented"
        );
    }

    #[test]
    fn hit_marker_has_correct_indentation() {
        let src = b"func foo() -> void:\n\tvar x = 1\n";
        let (out, _) = instrument_source(src, "res://test.gd");
        let s = std::str::from_utf8(&out).unwrap();
        let hit_line = s.lines().find(|l| l.contains("_cov_hit")).unwrap();
        assert!(
            hit_line.starts_with('\t'),
            "hit marker should preserve tab indentation: {hit_line:?}"
        );
    }
}
