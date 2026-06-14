//! Discovery helpers for gdtest.
//!
//! Two layers:
//! 1. [`resolve`] turns user-supplied paths (files, directories, `res://…`)
//!    into a flat list of `res://…` paths, filtered by discovery patterns.
//! 2. [`list_tests_in`] parses one GDScript file with tree-sitter-gdscript
//!    and returns every `func test_*()` name in source order.
//!
//! Default patterns (internal):
//!   - `test/**/*.gd`  — any `.gd` file inside a `test/` directory tree
//!   - `**/*.test.gd`  — co-located test files anywhere in the project
//!
//! Users can override via `gdtest.toml`:  `patterns = ["test/**/*.gd", "**/*.test.gd"]`
//!
//! Target: discovering 500 tests across 50 files in <100 ms on a warm cache
//! (internal success criterion).

/// Default discovery patterns used when no `patterns` key is in `gdtest.toml`.
pub const DEFAULT_PATTERNS: &[&str] = &["test/**/*.gd", "**/*.test.gd"];

pub mod coverage;
pub mod deps;
pub mod gdunit4_migrate;
pub mod gut_migrate;
pub mod mutate;
pub mod rewrite;
pub use gdunit4_migrate::{
    is_gdunit4_test_file, is_gdunit4_test_source, migrate_gdunit4_file, migrate_gdunit4_source,
    GdUnit4MigrationAction, GdUnit4MigrationIssue, GdUnit4MigrationOutput, GdUnit4MigrationStatus,
};
pub use gut_migrate::{
    is_gut_test_file, is_gut_test_source, migrate_gut_file, migrate_gut_source, GutMigrationAction,
    GutMigrationIssue, GutMigrationOutput, GutMigrationStatus,
};
pub use rewrite::{rewrite_file, rewrite_source, RewriteOutput};

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use tree_sitter::Parser;
use walkdir::WalkDir;

/// Normalise every input path into a list of `res://…` test-file paths.
///
/// `patterns` is a slice of glob strings (relative to project root) used when
/// walking directories. Pass [`DEFAULT_PATTERNS`] when no custom config is set.
pub fn resolve(inputs: &[String], project_path: &Path, patterns: &[String]) -> Result<Vec<String>> {
    let project_abs = project_path
        .canonicalize()
        .with_context(|| format!("project path does not exist: {}", project_path.display()))?;
    let mut out: Vec<String> = Vec::new();
    for input in inputs {
        if let Some(res) = input.strip_prefix("res://") {
            out.push(format!("res://{res}"));
            continue;
        }
        let path = PathBuf::from(input);
        let abs = if path.is_absolute() {
            path.clone()
        } else {
            project_abs.join(&path)
        };
        // Canonicalize so the \\?\ UNC prefix (Windows) matches project_abs.
        let abs = abs.canonicalize().unwrap_or(abs);
        if abs.is_file() {
            if abs.extension().and_then(|e| e.to_str()) != Some("gd") {
                return Err(anyhow!("not a .gd file: {}", abs.display()));
            }
            out.push(to_res_path(&abs, &project_abs)?);
        } else if abs.is_dir() {
            for entry in WalkDir::new(&abs).follow_links(false) {
                let entry = entry.with_context(|| format!("walking {}", abs.display()))?;
                if !entry.file_type().is_file() {
                    continue;
                }
                if entry.path().extension().and_then(|e| e.to_str()) != Some("gd") {
                    continue;
                }
                let ep = entry
                    .path()
                    .canonicalize()
                    .unwrap_or_else(|_| entry.path().to_path_buf());
                // Match against project-root-relative path with forward slashes.
                let rel = ep.strip_prefix(&project_abs).unwrap_or(&ep);
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                if !patterns.iter().any(|p| glob_match(p, &rel_str)) {
                    continue;
                }
                out.push(to_res_path(&ep, &project_abs)?);
            }
        } else {
            return Err(anyhow!("path not found: {}", abs.display()));
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Match a relative path (forward-slash separated) against a simple glob pattern.
/// Supports `**` (zero or more path segments) and `*` (any chars within one segment).
pub fn glob_match(pattern: &str, path: &str) -> bool {
    let pat: Vec<&str> = pattern.split('/').collect();
    let seg: Vec<&str> = path.split('/').collect();
    glob_seg(&pat, &seg)
}

fn glob_seg(pat: &[&str], seg: &[&str]) -> bool {
    if pat.is_empty() {
        return seg.is_empty();
    }
    if pat[0] == "**" {
        for i in 0..=seg.len() {
            if glob_seg(&pat[1..], &seg[i..]) {
                return true;
            }
        }
        return false;
    }
    if seg.is_empty() {
        return false;
    }
    if seg_matches(pat[0], seg[0]) {
        return glob_seg(&pat[1..], &seg[1..]);
    }
    false
}

/// Match a single path segment against a pattern that may contain `*` wildcards.
fn seg_matches(pattern: &str, s: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == s;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !s.starts_with(part) {
                return false;
            }
            pos += part.len();
        } else if i == parts.len() - 1 {
            return s[pos..].ends_with(part);
        } else {
            match s[pos..].find(part) {
                None => return false,
                Some(idx) => pos += idx + part.len(),
            }
        }
    }
    true
}

/// If `path` contains a component named `describe_<Name>`, return that component name.
/// Used to prefix test function names for directory-based sub-suites (internal).
pub fn extract_describe_dir(path: &Path) -> Option<String> {
    path.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .filter(|s| s.starts_with("describe_"))
        .last()
        .map(|s| s.to_string())
}

/// Classification of a discovered test function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestKind {
    /// `test_*` — runs normally.
    Normal,
    /// `bench_*` — benchmark; only runs when `--bench` is passed.
    Bench,
}

/// Per-test isolation override discovered from source comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationDirective {
    File,
    Test,
}

/// A discovered test function with its run classification.
#[derive(Debug, Clone)]
pub struct TestEntry {
    pub name: String,
    pub kind: TestKind,
    /// Tags collected from `## @tag <name>` doc comments immediately preceding the function.
    pub tags: Vec<String>,
    /// Isolation override from source comments.
    pub isolation: Option<IsolationDirective>,
}

/// Return every `func test_*()` and `func bench_*()` in the given GDScript file, in source order.
///
/// If the file lives inside a `describe_<Name>/` directory, all returned
/// entry names are prefixed with `describe_<Name>/` to match the reporting
/// format of inner-class describe blocks (internal).
pub fn list_test_entries_in(script_path: &Path) -> Result<Vec<TestEntry>> {
    let source =
        std::fs::read(script_path).with_context(|| format!("reading {}", script_path.display()))?;
    let mut entries = list_test_entries_in_source(&source)?;
    if let Some(prefix) = extract_describe_dir(script_path) {
        for entry in &mut entries {
            entry.name = format!("{prefix}/{}", entry.name);
        }
    }
    Ok(entries)
}

pub fn list_test_entries_in_source(source: &[u8]) -> Result<Vec<TestEntry>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_gdscript::LANGUAGE.into())
        .context("failed to set GDScript language")?;
    let Some(tree) = parser.parse(source, None) else {
        return Ok(Vec::new());
    };
    let file_isolation = extract_file_isolation(source);
    Ok(collect_test_entries(
        tree.root_node(),
        source,
        None,
        file_isolation,
    ))
}

/// Parse a `## @tag <name>` doc comment. Returns the tag name or `None`.
fn parse_tag_comment(text: &str) -> Option<String> {
    let body = text.trim().strip_prefix("##")?.trim_start();
    let name = body.strip_prefix("@tag")?.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    Some(name.to_string())
}

fn parse_isolation_comment(text: &str) -> Option<IsolationDirective> {
    let trimmed = text.trim();
    let doc_body = trimmed.strip_prefix("##").map(str::trim_start);
    let plain_body = trimmed.strip_prefix('#').map(str::trim_start);

    if let Some(body) = doc_body {
        if let Some(value) = body.strip_prefix("@isolation").map(str::trim) {
            return match value {
                "file" => Some(IsolationDirective::File),
                "test" => Some(IsolationDirective::Test),
                _ => None,
            };
        }
    }

    if let Some(body) = plain_body {
        if let Some(value) = body
            .strip_prefix("gdtest:")
            .map(str::trim_start)
            .and_then(|rest| rest.strip_prefix("isolation="))
            .map(str::trim)
        {
            return match value {
                "file" => Some(IsolationDirective::File),
                "test" => Some(IsolationDirective::Test),
                _ => None,
            };
        }
    }

    None
}

/// Scan source lines immediately before `func_row` and collect `## @tag` names.
/// Stops at a blank line or any line that is neither a `## @tag` nor a plain `#` comment.
fn extract_tags_preceding(source: &[u8], func_row: usize) -> Vec<String> {
    let lines: Vec<&[u8]> = source.split(|&b| b == b'\n').collect();
    let mut tags = Vec::new();
    let mut row = func_row;
    while row > 0 {
        row -= 1;
        let line = std::str::from_utf8(lines[row]).unwrap_or("").trim();
        if line.is_empty() {
            break;
        }
        if let Some(tag) = parse_tag_comment(line) {
            tags.push(tag);
        } else if line.starts_with('#') {
            // plain comment — keep scanning upward
        } else {
            break;
        }
    }
    tags.reverse();
    tags
}

fn extract_isolation_preceding(source: &[u8], func_row: usize) -> Option<IsolationDirective> {
    let lines: Vec<&[u8]> = source.split(|&b| b == b'\n').collect();
    let mut row = func_row;
    while row > 0 {
        row -= 1;
        let line = std::str::from_utf8(lines[row]).unwrap_or("").trim();
        if line.is_empty() {
            break;
        }
        if let Some(isolation) = parse_isolation_comment(line) {
            return Some(isolation);
        }
        if line.starts_with('#') {
            continue;
        }
        break;
    }
    None
}

fn extract_file_isolation(source: &[u8]) -> Option<IsolationDirective> {
    for raw_line in source.split(|&b| b == b'\n') {
        let line = std::str::from_utf8(raw_line).unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(isolation) = parse_isolation_comment(line) {
            return Some(isolation);
        }
        if line.starts_with('#') {
            continue;
        }
        break;
    }
    None
}

/// Classify a function name into its TestKind, or `None` if it's not a test.
fn classify_func(name: &str) -> Option<TestKind> {
    if name.starts_with("bench_") {
        Some(TestKind::Bench)
    } else if name.starts_with("test_") {
        Some(TestKind::Normal)
    } else {
        None
    }
}

/// Recursively walk the AST to collect test entries.
/// `describe` is the name of the enclosing `describe_*` inner class, if any.
fn collect_test_entries<'a>(
    node: tree_sitter::Node<'a>,
    source: &[u8],
    describe: Option<&str>,
    file_isolation: Option<IsolationDirective>,
) -> Vec<TestEntry> {
    let mut out = Vec::new();
    for i in 0..node.child_count() {
        let Some(child) = node.child(i as u32) else {
            continue;
        };
        match child.kind() {
            "function_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    if let Ok(fname) = name_node.utf8_text(source) {
                        if let Some(kind) = classify_func(fname) {
                            let name = match describe {
                                Some(cls) => format!("{cls}/{fname}"),
                                None => fname.to_string(),
                            };
                            let tags = extract_tags_preceding(source, child.start_position().row);
                            let isolation =
                                extract_isolation_preceding(source, child.start_position().row)
                                    .or(file_isolation);
                            out.push(TestEntry {
                                name,
                                kind,
                                tags,
                                isolation,
                            });
                        }
                    }
                }
                // Never descend into function bodies — no nested functions in GDScript.
            }
            "class_definition" if describe.is_none() => {
                // Only look inside top-level `describe_*` inner classes.
                if let Some(name_node) = child.child_by_field_name("name") {
                    if let Ok(cname) = name_node.utf8_text(source) {
                        if cname.starts_with("describe_") {
                            out.extend(collect_test_entries(
                                child,
                                source,
                                Some(cname),
                                file_isolation,
                            ));
                        }
                        // Non-describe inner classes are ignored entirely.
                    }
                }
            }
            _ => {
                out.extend(collect_test_entries(
                    child,
                    source,
                    describe,
                    file_isolation,
                ));
            }
        }
    }
    out
}

/// Convenience wrapper: returns the names of tests that would actually run
/// (excludes `bench_*`).
pub fn list_tests_in(script_path: &Path) -> Result<Vec<String>> {
    let entries = list_test_entries_in(script_path)?;
    Ok(entries
        .into_iter()
        .filter(|e| e.kind == TestKind::Normal)
        .map(|e| e.name)
        .collect())
}

/// Translate an absolute on-disk path to the res:// form used by the
/// worker. Caller guarantees `file` is inside `project_abs`.
pub fn to_res_path(file: &Path, project_abs: &Path) -> Result<String> {
    let rel = file.strip_prefix(project_abs).with_context(|| {
        format!(
            "{} is outside project {}",
            file.display(),
            project_abs.display()
        )
    })?;
    let s = rel.to_string_lossy().replace('\\', "/");
    Ok(format!("res://{s}"))
}

/// Translate a res:// path to its on-disk absolute form.
pub fn res_to_abs(res_path: &str, project_abs: &Path) -> Result<PathBuf> {
    let rel = res_path
        .strip_prefix("res://")
        .ok_or_else(|| anyhow!("expected res:// prefix: {res_path}"))?;
    Ok(project_abs.join(rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_test_functions() {
        let src = br#"
extends RefCounted

func _helper() -> void:
    pass

func test_alpha() -> void:
    pass

func test_beta(arg: int) -> void:
    # func test_inside_comment() should NOT be picked up
    pass

func not_a_test() -> void:
    pass

# Async test
func test_gamma() -> void:
    await physics_frame
"#;
        let entries = list_test_entries_in_source(src).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["test_alpha", "test_beta", "test_gamma"]);
        assert!(entries.iter().all(|e| e.kind == TestKind::Normal));
    }

    #[test]
    fn unknown_prefixes_are_not_tests() {
        let src = br#"
func test_normal() -> void:
    pass
func xtest_skipped() -> void:
    pass
func ftest_focused() -> void:
    pass
func helper() -> void:
    pass
"#;
        let entries = list_test_entries_in_source(src).unwrap();
        // xtest_* and ftest_* are not recognized — only test_* is.
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "test_normal");
        assert_eq!(entries[0].kind, TestKind::Normal);
    }

    #[test]
    fn parses_tag_comments() {
        let src = br#"
extends GdTest

## @tag slow
## @tag integration
func test_heavy() -> void:
    pass

## regular comment, not a tag
func test_untagged() -> void:
    pass

func test_also_untagged() -> void:
    pass
"#;
        let entries = list_test_entries_in_source(src).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "test_heavy");
        assert_eq!(entries[0].tags, vec!["slow", "integration"]);
        assert!(entries[1].tags.is_empty());
        assert!(entries[2].tags.is_empty());
    }

    #[test]
    fn parses_file_header_isolation_directive() {
        let src = br#"
# gdtest: isolation=test
extends GdTest

func test_one() -> void:
    pass

func test_two() -> void:
    pass
"#;
        let entries = list_test_entries_in_source(src).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].isolation, Some(IsolationDirective::Test));
        assert_eq!(entries[1].isolation, Some(IsolationDirective::Test));
    }

    #[test]
    fn function_isolation_overrides_file_header() {
        let src = br#"
## @isolation test
extends GdTest

## @isolation file
func test_one() -> void:
    pass

func test_two() -> void:
    pass
"#;
        let entries = list_test_entries_in_source(src).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].isolation, Some(IsolationDirective::File));
        assert_eq!(entries[1].isolation, Some(IsolationDirective::Test));
    }

    #[test]
    fn classifies_bench_funcs() {
        let src = br#"
func test_normal() -> void:
    pass
func bench_pathfinding() -> void:
    pass
"#;
        let entries = list_test_entries_in_source(src).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].kind, TestKind::Normal);
        assert_eq!(entries[1].kind, TestKind::Bench);
        // bench_* excluded from list_tests_in
        let runnable: Vec<String> = entries
            .into_iter()
            .filter(|e| e.kind == TestKind::Normal)
            .map(|e| e.name)
            .collect();
        assert_eq!(runnable, vec!["test_normal"]);
    }

    #[test]
    fn empty_on_parse_error_tolerates_partial() {
        let src = br#"func test_recoverable(): pass
!@#$%garbage that breaks parsing"#;
        let _entries = list_test_entries_in_source(src).unwrap();
    }

    #[test]
    fn discovers_describe_class_tests() {
        let src = br#"
extends GdTest

func test_top_level() -> void:
    pass

class describe_Movement:
    func test_moves_right() -> void:
        pass
    func test_moves_left() -> void:
        pass
    func helper_not_a_test() -> void:
        pass

class IgnoredClass:
    func test_inside_ignored() -> void:
        pass
"#;
        let entries = list_test_entries_in_source(src).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "test_top_level",
                "describe_Movement/test_moves_right",
                "describe_Movement/test_moves_left",
            ]
        );
        assert!(entries.iter().all(|e| e.kind == TestKind::Normal));
    }

    // ── glob_match tests ──────────────────────────────────────────────────────

    #[test]
    fn glob_match_test_dir_pattern() {
        assert!(glob_match("test/**/*.gd", "test/battle.gd"));
        assert!(glob_match("test/**/*.gd", "test/docs/test_foo.gd"));
        assert!(glob_match("test/**/*.gd", "test/docs/dungeon/battle.gd"));
        assert!(!glob_match("test/**/*.gd", "src/battle.gd"));
        assert!(!glob_match("test/**/*.gd", "test/battle.tscn"));
    }

    #[test]
    fn glob_match_colocated_pattern() {
        assert!(glob_match("**/*.test.gd", "battle.test.gd"));
        assert!(glob_match("**/*.test.gd", "src/battle/battle.test.gd"));
        assert!(glob_match("**/*.test.gd", "deep/nested/dir/foo.test.gd"));
        assert!(!glob_match("**/*.test.gd", "test_battle.gd"));
        assert!(!glob_match("**/*.test.gd", "battle.gd"));
    }

    #[test]
    fn glob_match_star_prefix_legacy() {
        // Backward-compat: users may set patterns = ["test/test_*.gd"]
        assert!(glob_match("test/test_*.gd", "test/test_battle.gd"));
        assert!(!glob_match("test/test_*.gd", "test/battle.gd"));
    }

    // ── extract_describe_dir tests ────────────────────────────────────────────

    #[test]
    fn extract_describe_dir_finds_prefix() {
        use std::path::PathBuf;
        let p = PathBuf::from("test/describe_Battle/basic_hit.gd");
        assert_eq!(
            extract_describe_dir(&p),
            Some("describe_Battle".to_string())
        );
    }

    #[test]
    fn extract_describe_dir_none_for_normal() {
        use std::path::PathBuf;
        let p = PathBuf::from("test/docs/test_foo.gd");
        assert_eq!(extract_describe_dir(&p), None);
    }

    #[test]
    fn extract_describe_dir_returns_last() {
        use std::path::PathBuf;
        // Nested describe dirs: return innermost
        let p = PathBuf::from("test/describe_Outer/describe_Inner/foo.gd");
        assert_eq!(extract_describe_dir(&p), Some("describe_Inner".to_string()));
    }
}
