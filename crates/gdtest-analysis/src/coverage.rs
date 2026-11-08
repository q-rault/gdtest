//! GDScript coverage instrumentation.
//!
//! Inserts `Gdt._cov_hit("res://path", line)` before each executable line
//! inside function bodies. Uses gdsyntax to identify function body line
//! ranges; blank lines and comment lines are skipped via text heuristics.
//!
//! The caller is responsible for backing up original files before writing the
//! instrumented bytes and restoring them after the test run.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use gdsyntax::ast::{AstNode, Func};

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
///
/// A body runs from the line after the signature to the body's last line of
/// code. A body on the signature's line, as in `func f(): pass`, has no lines.
fn find_function_body_lines(source: &[u8]) -> HashSet<u32> {
    // Offsets must index into `source`, so invalid UTF-8 is not instrumented.
    let Ok(text) = std::str::from_utf8(source) else {
        return HashSet::new();
    };
    let tree = gdsyntax::parse(text);
    let line_of = |offset: gdsyntax::rowan::TextSize| tree.position(offset.into()).line;
    let mut lines = HashSet::new();
    // Inner-class methods are FUNC nodes too; lambdas are not.
    for func in tree.root().descendants().filter_map(Func::cast) {
        let (Some(signature), Some(body)) = (
            func.signature_range(),
            func.body().and_then(|body| body.code_range()),
        ) else {
            continue;
        };
        let start = line_of(body.start()).max(line_of(signature.end()) + 1);
        // The last token can span lines, as a multi-line string does.
        for line in start..=line_of(body.end()) {
            lines.insert(line as u32);
        }
    }
    lines
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
    fn skips_multi_line_signatures() {
        let src = b"func f(a: int,\n\t\tb: int) -> int:\n\treturn a + b\n";
        let (_, instrumented) = instrument_source(src, "res://test.gd");
        let mut lines: Vec<u32> = instrumented.into_iter().collect();
        lines.sort();
        assert_eq!(lines, vec![3]);
    }

    #[test]
    fn instruments_init_and_skips_bodiless_functions() {
        let src =
            b"@abstract\nclass A:\n\t@abstract func f() -> int\n\nfunc _init() -> void:\n\tprint(1)\n";
        let (_, instrumented) = instrument_source(src, "res://test.gd");
        let mut lines: Vec<u32> = instrumented.into_iter().collect();
        lines.sort();
        assert_eq!(lines, vec![6]);
    }

    #[test]
    fn body_ends_after_a_trailing_multi_line_string() {
        let src = b"func f() -> void:\n\tvar s = \"\"\"\n\ttext\n\t\"\"\"\n";
        let (_, instrumented) = instrument_source(src, "res://test.gd");
        assert!(instrumented.contains(&4), "{instrumented:?}");
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
