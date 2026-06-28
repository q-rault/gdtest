use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GdUnit4MigrationStatus {
    CandidateForExecution,
    RewriteIncomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GdUnit4MigrationAction {
    pub line: usize,
    pub pattern: String,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GdUnit4MigrationIssue {
    pub line: usize,
    pub pattern: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GdUnit4MigrationOutput {
    pub source: Vec<u8>,
    pub changed: bool,
    pub status: GdUnit4MigrationStatus,
    pub rewritten: Vec<GdUnit4MigrationAction>,
    pub blocked: Vec<GdUnit4MigrationIssue>,
    pub manual_followups: Vec<GdUnit4MigrationIssue>,
}

pub fn is_gdunit4_test_file(path: &Path) -> Result<bool> {
    let source = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(is_gdunit4_test_source(&source))
}

pub fn migrate_gdunit4_file(path: &Path, res_path: &str) -> Result<GdUnit4MigrationOutput> {
    let source = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(migrate_gdunit4_source(&source, res_path))
}

pub fn is_gdunit4_test_source(source: &[u8]) -> bool {
    let text = String::from_utf8_lossy(source);
    text.lines()
        .any(|line| line.trim_start().starts_with("extends GdUnitTestSuite"))
}

pub fn migrate_gdunit4_source(source: &[u8], _res_path: &str) -> GdUnit4MigrationOutput {
    let text = String::from_utf8_lossy(source);
    let mut rewritten = Vec::new();
    let mut blocked = Vec::new();
    let mut manual_followups = Vec::new();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let had_trailing_newline = text.ends_with('\n');

    for (idx, line) in lines.iter_mut().enumerate() {
        let line_no = idx + 1;
        let original_line = line.clone();
        let trimmed = original_line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }

        if replace_extends(line) && original_line != *line {
            rewritten.push(GdUnit4MigrationAction {
                line: line_no,
                pattern: "extends GdUnitTestSuite".to_string(),
                replacement: "extends GdTestCompatGdUnit".to_string(),
            });
        }

        if remove_class_name(line) && original_line != *line {
            rewritten.push(GdUnit4MigrationAction {
                line: line_no,
                pattern: "class_name ...".to_string(),
                replacement: "(removed)".to_string(),
            });
        }

        rewrite_call_name(
            line,
            line_no,
            "simulate_action_release",
            "simulate_action_released",
            &mut rewritten,
        );
        rewrite_call_name(
            line,
            line_no,
            "simulate_mouse_button_release",
            "simulate_mouse_button_released",
            &mut rewritten,
        );
        rewrite_call_name(
            line,
            line_no,
            "simulate_key_release",
            "simulate_key_released",
            &mut rewritten,
        );
        rewrite_call_name(
            line,
            line_no,
            "await_input_processed",
            "wait_process_frame",
            &mut rewritten,
        );
        rewrite_scene_runner(line, line_no, &mut rewritten, &mut manual_followups);
        rewrite_verify_chain(line, line_no, &mut rewritten, &mut manual_followups);

        if original_line.contains("clean_temp_dir(") {
            manual_followups.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "clean_temp_dir(...)".to_string(),
                message: "clean_temp_dir() has no safe generic rewrite in v1; convert it manually to delete_temp_dir(name) when the temp dir name is explicit.".to_string(),
            });
        }
        if original_line.contains("monitor_signals(") {
            manual_followups.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "monitor_signals(...)".to_string(),
                message: "monitor_signals() is not auto-migrated; expand it to explicit per-signal gdtest watchers by hand.".to_string(),
            });
        }
        if original_line.contains("verify_signal(") {
            manual_followups.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "verify_signal(...)".to_string(),
                message: "verify_signal() is not auto-migrated in v1; inspect the watcher state manually.".to_string(),
            });
        }
        if original_line.contains("@GdUnitTestParameters") {
            blocked.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "@GdUnitTestParameters".to_string(),
                message: "Annotation-driven parameterized tests are not auto-migrated in v1."
                    .to_string(),
            });
        }
        if original_line.contains("GdUnitTuple") || original_line.contains("tuple(") {
            blocked.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "GdUnitTuple / tuple(...)".to_string(),
                message: "GdUnitTuple-based parameterized tests are not auto-migrated in v1."
                    .to_string(),
            });
        }
        if original_line.contains("@GdUnitMock") {
            blocked.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "@GdUnitMock".to_string(),
                message: "Annotation-driven mock injection is not auto-migrated in v1.".to_string(),
            });
        }
        if original_line.contains("RETURN_DEEP_STUB") {
            blocked.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "RETURN_DEEP_STUB".to_string(),
                message: "Deep stubs have no gdtest equivalent in v1.".to_string(),
            });
        }
        if original_line.contains("verify_no_more_interactions(") {
            blocked.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "verify_no_more_interactions(...)".to_string(),
                message: "verify_no_more_interactions() has no gdtest equivalent in v1."
                    .to_string(),
            });
        }
        if original_line.contains("reset(") {
            blocked.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "reset(...)".to_string(),
                message: "reset(mock) has no gdtest equivalent in v1.".to_string(),
            });
        }
        if original_line.contains("InputFactory.") {
            blocked.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "InputFactory.*".to_string(),
                message: "InputFactory helpers are not auto-migrated in v1.".to_string(),
            });
        }
        if original_line.contains("simulate_mouse_move_relative(") {
            blocked.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "simulate_mouse_move_relative(...)".to_string(),
                message: "Relative mouse movement has no gdtest equivalent in v1.".to_string(),
            });
        }
        if original_line.contains("simulate_screen_touch_") {
            blocked.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "simulate_screen_touch_*".to_string(),
                message: "Touch simulation helpers have no gdtest equivalent in v1.".to_string(),
            });
        }
        if original_line.contains("override_failure_message(") {
            blocked.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "override_failure_message(...)".to_string(),
                message: "Fluent failure-message overrides are not supported by the compat layer."
                    .to_string(),
            });
        }
        if original_line.contains("append_failure_message(") {
            blocked.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "append_failure_message(...)".to_string(),
                message: "Fluent failure-message appends are not supported by the compat layer."
                    .to_string(),
            });
        }
        if original_line.contains(".matches(") && original_line.contains("assert_str(") {
            blocked.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "assert_str(...).matches(...)".to_string(),
                message: "String regex assertions have no direct gdtest rewrite in v1.".to_string(),
            });
        }
        if original_line.contains(".contains_same(") && original_line.contains("assert_array(") {
            blocked.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "assert_array(...).contains_same(...)".to_string(),
                message:
                    "Reference-identity collection assertions have no gdtest equivalent in v1."
                        .to_string(),
            });
        }
        if (original_line.contains(".extract(") || original_line.contains(".extractv("))
            && original_line.contains("assert_array(")
        {
            blocked.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "assert_array(...).extract(...)".to_string(),
                message: "Extractor helpers are not auto-migrated in v1.".to_string(),
            });
        }
    }

    dedup_issues(&mut blocked);
    dedup_issues(&mut manual_followups);

    if !blocked.is_empty() || !manual_followups.is_empty() {
        let mut header: Vec<String> =
            vec!["# gdtest migrate gdunit4: manual follow-up required".to_string()];
        for issue in blocked.iter().chain(manual_followups.iter()) {
            header.push(format!("# line {}: {}", issue.line, issue.message));
        }
        header.push(String::new());
        let mut merged = header;
        merged.extend(lines);
        lines = merged;
    }

    let mut rendered = lines.join("\n");
    if had_trailing_newline {
        rendered.push('\n');
    }

    let changed = rendered.as_bytes() != source;
    let status = if blocked.is_empty() && manual_followups.is_empty() {
        GdUnit4MigrationStatus::CandidateForExecution
    } else {
        GdUnit4MigrationStatus::RewriteIncomplete
    };

    GdUnit4MigrationOutput {
        source: rendered.into_bytes(),
        changed,
        status,
        rewritten,
        blocked,
        manual_followups,
    }
}

fn replace_extends(line: &mut String) -> bool {
    let original = line.clone();
    *line = line.replace("extends GdUnitTestSuite", "extends GdTestCompatGdUnit");
    *line != original
}

fn remove_class_name(line: &mut String) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("class_name ") {
        return false;
    }
    line.clear();
    true
}

fn rewrite_call_name(
    line: &mut String,
    line_no: usize,
    old_name: &str,
    new_name: &str,
    rewritten: &mut Vec<GdUnit4MigrationAction>,
) {
    let mut cursor = 0usize;
    while let Some((start, end, args)) = find_named_call(line, old_name, cursor) {
        line.replace_range(start..end, &format!("{new_name}({args})"));
        rewritten.push(GdUnit4MigrationAction {
            line: line_no,
            pattern: format!("{old_name}(...)"),
            replacement: format!("{new_name}(...)"),
        });
        cursor = start + new_name.len();
    }
}

fn rewrite_scene_runner(
    line: &mut String,
    line_no: usize,
    rewritten: &mut Vec<GdUnit4MigrationAction>,
    manual_followups: &mut Vec<GdUnit4MigrationIssue>,
) {
    let mut cursor = 0usize;
    while let Some((start, end, args)) = find_named_call(line, "scene_runner", cursor) {
        let parsed = split_top_level_args(&args);
        if parsed.len() != 1 {
            manual_followups.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "scene_runner(...)".to_string(),
                message:
                    "scene_runner() only auto-rewrites for a single scene path string argument."
                        .to_string(),
            });
            cursor = end;
            continue;
        }
        let arg = parsed[0].trim();
        if looks_like_string_literal(arg) {
            line.replace_range(start..end, &format!("scene_runner(load({arg}))"));
            rewritten.push(GdUnit4MigrationAction {
                line: line_no,
                pattern: "scene_runner(\"res://...\")".to_string(),
                replacement: "scene_runner(load(\"res://...\"))".to_string(),
            });
            cursor = start + "scene_runner".len();
        } else {
            manual_followups.push(GdUnit4MigrationIssue {
                line: line_no,
                pattern: "scene_runner(...)".to_string(),
                message:
                    "scene_runner() only auto-rewrites when the input is a string literal path."
                        .to_string(),
            });
            cursor = end;
        }
    }
}

fn rewrite_verify_chain(
    line: &mut String,
    line_no: usize,
    rewritten: &mut Vec<GdUnit4MigrationAction>,
    manual_followups: &mut Vec<GdUnit4MigrationIssue>,
) {
    let re = Regex::new(
        r#"verify\s*\(\s*(?P<target>[^,\)]+?)\s*(?:,\s*(?P<count>[^,\)]+?)\s*)?\)\s*\.\s*(?P<method>[A-Za-z_][A-Za-z0-9_]*)\s*\(\s*\)"#,
    )
    .expect("valid verify regex");
    let original = line.clone();
    let mut changed = false;
    let replaced = re.replace_all(&original, |caps: &regex::Captures<'_>| {
        changed = true;
        let target = caps.name("target").unwrap().as_str().trim();
        let method = caps.name("method").unwrap().as_str().trim();
        if let Some(count) = caps.name("count") {
            rewritten.push(GdUnit4MigrationAction {
                line: line_no,
                pattern: "verify(mock, n).method()".to_string(),
                replacement: "assert_call_count(mock, \"method\", n)".to_string(),
            });
            format!(
                "assert_call_count({}, \"{}\", {})",
                target,
                method,
                count.as_str().trim()
            )
        } else {
            rewritten.push(GdUnit4MigrationAction {
                line: line_no,
                pattern: "verify(mock).method()".to_string(),
                replacement: "assert_called(mock, \"method\")".to_string(),
            });
            format!("assert_called({}, \"{}\")", target, method)
        }
    });
    if changed {
        *line = replaced.into_owned();
        return;
    }
    if original.contains("verify(") {
        manual_followups.push(GdUnit4MigrationIssue {
            line: line_no,
            pattern: "verify(...)".to_string(),
            message: "verify() only auto-rewrites for `verify(mock).method()` and `verify(mock, n).method()` with no call arguments.".to_string(),
        });
    }
}

fn find_named_call(line: &str, name: &str, from: usize) -> Option<(usize, usize, String)> {
    let needle = format!("{name}(");
    let mut search = from;
    while let Some(found) = line[search..].find(&needle) {
        let start = search + found;
        if start > 0 {
            let prev = line[..start].chars().next_back().unwrap_or(' ');
            if prev.is_ascii_alphanumeric() || prev == '_' || prev == '.' {
                search = start + needle.len();
                continue;
            }
        }
        let open = start + name.len();
        let end = find_matching_paren(line, open)?;
        return Some((start, end + 1, line[(open + 1)..end].to_string()));
    }
    None
}

fn find_matching_paren(text: &str, open_idx: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(open_idx) != Some(&b'(') {
        return None;
    }
    let mut depth = 0usize;
    let mut in_string: Option<u8> = None;
    let mut escaped = false;
    for (idx, byte) in bytes.iter().enumerate().skip(open_idx) {
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if *byte == b'\\' {
                escaped = true;
                continue;
            }
            if *byte == quote {
                in_string = None;
            }
            continue;
        }
        match *byte {
            b'"' | b'\'' => in_string = Some(*byte),
            b'(' | b'[' | b'{' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(idx);
                }
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

fn split_top_level_args(args: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    let mut in_string: Option<char> = None;
    let mut escaped = false;
    for ch in args.chars() {
        if let Some(quote) = in_string {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                in_string = Some(ch);
                current.push(ch);
            }
            '(' => {
                paren += 1;
                current.push(ch);
            }
            ')' => {
                paren = paren.saturating_sub(1);
                current.push(ch);
            }
            '[' => {
                bracket += 1;
                current.push(ch);
            }
            ']' => {
                bracket = bracket.saturating_sub(1);
                current.push(ch);
            }
            '{' => {
                brace += 1;
                current.push(ch);
            }
            '}' => {
                brace = brace.saturating_sub(1);
                current.push(ch);
            }
            ',' if paren == 0 && bracket == 0 && brace == 0 => {
                let part = current.trim();
                if !part.is_empty() {
                    parts.push(part.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let part = current.trim();
    if !part.is_empty() {
        parts.push(part.to_string());
    }
    parts
}

fn looks_like_string_literal(arg: &str) -> bool {
    let trimmed = arg.trim();
    (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
}

fn dedup_issues(issues: &mut Vec<GdUnit4MigrationIssue>) {
    issues.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.pattern.cmp(&b.pattern)));
    issues.dedup_by(|a, b| a.line == b.line && a.pattern == b.pattern && a.message == b.message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_basic_gdunit4_suite() {
        let src = br#"class_name DamageSuite
extends GdUnitTestSuite

func test_scene() -> void:
    verify(mock_ai, 2).fire()
    verify(mock_ai).rest()
    var runner := scene_runner("res://ui/panel.tscn")
    simulate_action_release("jump")
    simulate_mouse_button_release(MOUSE_BUTTON_LEFT)
    simulate_key_release(KEY_A)
    await await_input_processed()
"#;
        let out = migrate_gdunit4_source(src, "res://test/test_damage.gd");
        let text = String::from_utf8(out.source).unwrap();
        assert!(!text.contains("class_name DamageSuite"));
        assert!(text.contains("extends GdTestCompatGdUnit"));
        assert!(text.contains("assert_call_count(mock_ai, \"fire\", 2)"));
        assert!(text.contains("assert_called(mock_ai, \"rest\")"));
        assert!(text.contains("scene_runner(load(\"res://ui/panel.tscn\"))"));
        assert!(text.contains("simulate_action_released(\"jump\")"));
        assert!(text.contains("simulate_mouse_button_released(MOUSE_BUTTON_LEFT)"));
        assert!(text.contains("simulate_key_released(KEY_A)"));
        assert!(text.contains("await wait_process_frame()"));
        assert_eq!(out.status, GdUnit4MigrationStatus::CandidateForExecution);
    }

    #[test]
    fn flags_unsupported_patterns() {
        let src = br#"extends GdUnitTestSuite

@GdUnitTestParameters([
    tuple("a", "b")
])
func test_params() -> void:
    var m = mock(MyType, RETURN_DEEP_STUB)
    monitor_signals(self)
    verify_signal(self)
    clean_temp_dir()
    InputFactory.simulate_key_pressed(KEY_A)
    verify_no_more_interactions(m)
    reset(m)
    override_failure_message("boom")
"#;
        let out = migrate_gdunit4_source(src, "res://test/test_blocked.gd");
        let text = String::from_utf8(out.source).unwrap();
        assert!(text.starts_with("# gdtest migrate gdunit4: manual follow-up required"));
        assert_eq!(out.status, GdUnit4MigrationStatus::RewriteIncomplete);
        assert!(
            out.blocked
                .iter()
                .any(|i| i.pattern == "@GdUnitTestParameters")
        );
        assert!(
            out.blocked
                .iter()
                .any(|i| i.pattern == "GdUnitTuple / tuple(...)")
        );
        assert!(out.blocked.iter().any(|i| i.pattern == "RETURN_DEEP_STUB"));
        assert!(
            out.blocked
                .iter()
                .any(|i| i.pattern == "verify_no_more_interactions(...)")
        );
        assert!(out.blocked.iter().any(|i| i.pattern == "reset(...)"));
        assert!(out.blocked.iter().any(|i| i.pattern == "InputFactory.*"));
        assert!(
            out.manual_followups
                .iter()
                .any(|i| i.pattern == "monitor_signals(...)")
        );
        assert!(
            out.manual_followups
                .iter()
                .any(|i| i.pattern == "verify_signal(...)")
        );
        assert!(
            out.manual_followups
                .iter()
                .any(|i| i.pattern == "clean_temp_dir(...)")
        );
    }

    #[test]
    fn detects_gdunit4_test_source() {
        assert!(is_gdunit4_test_source(
            br#"extends GdUnitTestSuite

func test_foo() -> void:
    pass
"#
        ));
        assert!(!is_gdunit4_test_source(
            br#"extends GdTest

func test_foo() -> void:
    pass
"#
        ));
    }
}
