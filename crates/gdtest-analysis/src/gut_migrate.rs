use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GutMigrationStatus {
    CandidateForExecution,
    RewriteIncomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GutMigrationAction {
    pub line: usize,
    pub pattern: String,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GutMigrationIssue {
    pub line: usize,
    pub pattern: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GutMigrationOutput {
    pub source: Vec<u8>,
    pub changed: bool,
    pub status: GutMigrationStatus,
    pub rewritten: Vec<GutMigrationAction>,
    pub blocked: Vec<GutMigrationIssue>,
    pub manual_followups: Vec<GutMigrationIssue>,
}

pub fn is_gut_test_file(path: &Path) -> Result<bool> {
    let source = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(is_gut_test_source(&source))
}

pub fn migrate_gut_file(path: &Path, res_path: &str) -> Result<GutMigrationOutput> {
    let source = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(migrate_gut_source(&source, res_path))
}

pub fn is_gut_test_source(source: &[u8]) -> bool {
    let text = String::from_utf8_lossy(source);
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("extends GutTest")
            || trimmed.starts_with("extends \"res://addons/gut/test.gd\"")
            || trimmed.starts_with("extends 'res://addons/gut/test.gd'")
    })
}

pub fn migrate_gut_source(source: &[u8], _res_path: &str) -> GutMigrationOutput {
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
            rewritten.push(GutMigrationAction {
                line: line_no,
                pattern: "extends GutTest".to_string(),
                replacement: "extends GdTest".to_string(),
            });
        }

        rewrite_constructor_chain(
            line,
            line_no,
            "partial_double",
            "partial_mock",
            &mut rewritten,
        );
        rewrite_constructor_chain(line, line_no, "double", "mock", &mut rewritten);
        rewrite_stub_to_return(line, line_no, &mut rewritten);
        rewrite_call_name(
            line,
            line_no,
            "yield_for",
            "wait_physics_seconds",
            &mut rewritten,
        );
        rewrite_call_name(
            line,
            line_no,
            "yield_frames",
            "wait_physics_frames",
            &mut rewritten,
        );
        rewrite_call_name(
            line,
            line_no,
            "wait_frames",
            "wait_physics_frames",
            &mut rewritten,
        );
        rewrite_wait_process_frames(line, line_no, &mut rewritten, &mut manual_followups);
        rewrite_wait_idle_frames(line, line_no, &mut rewritten, &mut manual_followups);
        rewrite_signal_assertions(line, line_no, &mut rewritten, &mut manual_followups);
        rewrite_yield_to(line, line_no, &mut rewritten, &mut manual_followups);

        if original_line.contains("watch_signals(") {
            manual_followups.push(GutMigrationIssue {
                line: line_no,
                pattern: "watch_signals(...)".to_string(),
                message: "watch_signals() is not auto-migrated; expand to explicit gdtest per-signal assertions by hand.".to_string(),
            });
        }
        if original_line.contains("use_parameters(") {
            blocked.push(GutMigrationIssue {
                line: line_no,
                pattern: "use_parameters(...)".to_string(),
                message:
                    "Parameterized GUT tests need manual restructuring; v1 does not rewrite them."
                        .to_string(),
            });
        }
        if original_line.contains("assert_engine_error(")
            || original_line.contains("assert_engine_error_count(")
        {
            blocked.push(GutMigrationIssue {
                line: line_no,
                pattern: "assert_engine_error*".to_string(),
                message: "Engine error assertions have no safe canonical gdtest rewrite in v1."
                    .to_string(),
            });
        }
        if trimmed.starts_with("func should_skip_script(") {
            blocked.push(GutMigrationIssue {
                line: line_no,
                pattern: "should_skip_script()".to_string(),
                message: "Script-level skip behavior is not auto-migrated; convert it manually."
                    .to_string(),
            });
        }
        if original_line.contains("partial_double_singleton(")
            || original_line.contains("double_singleton(")
        {
            blocked.push(GutMigrationIssue {
                line: line_no,
                pattern: "double_singleton()/partial_double_singleton()".to_string(),
                message:
                    "Singleton doubles need manual migration; no safe gdtest rewrite is applied."
                        .to_string(),
            });
        }
        if original_line.contains("double(")
            && !original_line.contains(".new()")
            && line == &original_line
        {
            blocked.push(GutMigrationIssue {
                line: line_no,
                pattern: "double(...)".to_string(),
                message: "Only `double(...).new()` is auto-rewritten in v1. Other double() forms need manual review.".to_string(),
            });
        }
        if original_line.contains("partial_double(")
            && !original_line.contains(".new()")
            && line == &original_line
        {
            blocked.push(GutMigrationIssue {
                line: line_no,
                pattern: "partial_double(...)".to_string(),
                message: "Only `partial_double(...).new()` is auto-rewritten in v1. Other partial_double() forms need manual review.".to_string(),
            });
        }
        if original_line.contains("stub(")
            && original_line.contains(".to_")
            && line == &original_line
        {
            manual_followups.push(GutMigrationIssue {
                line: line_no,
                pattern: "stub(...).to_*".to_string(),
                message: "This stub chain was not recognized as a safe direct-value rewrite; convert it manually.".to_string(),
            });
        }
    }

    dedup_issues(&mut blocked);
    dedup_issues(&mut manual_followups);

    if !blocked.is_empty() || !manual_followups.is_empty() {
        let mut header: Vec<String> =
            vec!["# gdtest migrate gut: manual follow-up required".to_string()];
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
        GutMigrationStatus::CandidateForExecution
    } else {
        GutMigrationStatus::RewriteIncomplete
    };

    GutMigrationOutput {
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
    *line = line.replace("extends GutTest", "extends GdTest");
    *line = line.replace("extends \"res://addons/gut/test.gd\"", "extends GdTest");
    *line = line.replace("extends 'res://addons/gut/test.gd'", "extends GdTest");
    *line != original
}

fn rewrite_call_name(
    line: &mut String,
    line_no: usize,
    old_name: &str,
    new_name: &str,
    rewritten: &mut Vec<GutMigrationAction>,
) {
    let mut cursor = 0usize;
    while let Some((start, end, args)) = find_named_call(line, old_name, cursor) {
        line.replace_range(start..end, &format!("{new_name}({args})"));
        rewritten.push(GutMigrationAction {
            line: line_no,
            pattern: format!("{old_name}(...)"),
            replacement: format!("{new_name}(...)"),
        });
        cursor = start + new_name.len();
    }
}

fn rewrite_wait_process_frames(
    line: &mut String,
    line_no: usize,
    rewritten: &mut Vec<GutMigrationAction>,
    manual_followups: &mut Vec<GutMigrationIssue>,
) {
    rewrite_wait_single_frame(
        line,
        line_no,
        "wait_process_frames",
        "wait_process_frame",
        rewritten,
        manual_followups,
    );
}

fn rewrite_wait_idle_frames(
    line: &mut String,
    line_no: usize,
    rewritten: &mut Vec<GutMigrationAction>,
    manual_followups: &mut Vec<GutMigrationIssue>,
) {
    rewrite_wait_single_frame(
        line,
        line_no,
        "wait_idle_frames",
        "wait_process_frame",
        rewritten,
        manual_followups,
    );
}

fn rewrite_wait_single_frame(
    line: &mut String,
    line_no: usize,
    old_name: &str,
    new_name: &str,
    rewritten: &mut Vec<GutMigrationAction>,
    manual_followups: &mut Vec<GutMigrationIssue>,
) {
    let mut cursor = 0usize;
    while let Some((start, end, args)) = find_named_call(line, old_name, cursor) {
        let parsed = split_top_level_args(&args);
        if parsed.len() == 1 && is_literal_one(parsed[0].as_str()) {
            line.replace_range(start..end, &format!("{new_name}()"));
            rewritten.push(GutMigrationAction {
                line: line_no,
                pattern: format!("{old_name}(1)"),
                replacement: format!("{new_name}()"),
            });
            cursor = start + new_name.len();
        } else {
            manual_followups.push(GutMigrationIssue {
                line: line_no,
                pattern: format!("{old_name}(...)"),
                message: format!("{old_name} only auto-rewrites when waiting for exactly one frame; larger counts need manual expansion."),
            });
            cursor = end;
        }
    }
}

fn rewrite_constructor_chain(
    line: &mut String,
    line_no: usize,
    old_name: &str,
    new_name: &str,
    rewritten: &mut Vec<GutMigrationAction>,
) {
    let mut cursor = 0usize;
    while let Some((call_start, call_end, args)) = find_named_call(line, old_name, cursor) {
        let suffix = &line[call_end..];
        let Some(new_end) = parse_dot_new_suffix(suffix).map(|offset| call_end + offset) else {
            cursor = call_end;
            continue;
        };
        line.replace_range(call_start..new_end, &format!("{new_name}({args})"));
        rewritten.push(GutMigrationAction {
            line: line_no,
            pattern: format!("{old_name}(...).new()"),
            replacement: format!("{new_name}(...)"),
        });
        cursor = call_start + new_name.len();
    }
}

fn rewrite_stub_to_return(
    line: &mut String,
    line_no: usize,
    rewritten: &mut Vec<GutMigrationAction>,
) {
    let mut cursor = 0usize;
    while let Some((call_start, call_end, args)) = find_named_call(line, "stub", cursor) {
        let suffix = &line[call_end..];
        let Some((suffix_end, value)) = parse_stub_return_suffix(suffix) else {
            cursor = call_end;
            continue;
        };
        let stub_args = split_top_level_args(&args);
        if stub_args.len() != 2 {
            cursor = call_end;
            continue;
        }
        line.replace_range(
            call_start..(call_end + suffix_end),
            &format!("stub({}, {}, {})", stub_args[0], stub_args[1], value.trim()),
        );
        rewritten.push(GutMigrationAction {
            line: line_no,
            pattern: "stub(...).to_return(...)".to_string(),
            replacement: "stub(..., value)".to_string(),
        });
        cursor = call_start + 4;
    }
}

fn rewrite_signal_assertions(
    line: &mut String,
    line_no: usize,
    rewritten: &mut Vec<GutMigrationAction>,
    manual_followups: &mut Vec<GutMigrationIssue>,
) {
    let mappings = [
        ("assert_signal_emitted", "assert_signal", 2usize, false),
        (
            "assert_signal_not_emitted",
            "assert_no_signal",
            2usize,
            false,
        ),
        (
            "assert_signal_emit_count",
            "assert_signal_count",
            3usize,
            false,
        ),
        (
            "assert_signal_emitted_with_parameters",
            "assert_signal_with_args",
            3usize,
            true,
        ),
    ];
    for (old_name, new_name, required_args, block_extra) in mappings {
        let mut cursor = 0usize;
        while let Some((start, end, args)) = find_named_call(line, old_name, cursor) {
            let parsed = split_top_level_args(&args);
            let uses_signal_ref = parsed
                .first()
                .map(|arg| !looks_like_signal_object_form(arg))
                .unwrap_or(false);
            if uses_signal_ref {
                manual_followups.push(GutMigrationIssue {
                    line: line_no,
                    pattern: format!("{old_name}(signal_ref, ...)"),
                    message: format!("{old_name} only auto-rewrites for `(object, \"signal\")` calls; signal-reference overloads need manual conversion."),
                });
                cursor = end;
                continue;
            }
            if parsed.len() < required_args
                || (block_extra && parsed.len() != required_args)
                || (!block_extra && parsed.len() > required_args)
            {
                manual_followups.push(GutMigrationIssue {
                    line: line_no,
                    pattern: format!("{old_name}(...)"),
                    message: format!("{old_name} had optional arguments or an unsupported shape; convert it manually."),
                });
                cursor = end;
                continue;
            }
            line.replace_range(start..end, &format!("{new_name}({})", parsed.join(", ")));
            rewritten.push(GutMigrationAction {
                line: line_no,
                pattern: format!("{old_name}(...)"),
                replacement: format!("{new_name}(...)"),
            });
            cursor = start + new_name.len();
        }
    }
}

fn rewrite_yield_to(
    line: &mut String,
    line_no: usize,
    rewritten: &mut Vec<GutMigrationAction>,
    manual_followups: &mut Vec<GutMigrationIssue>,
) {
    let mut cursor = 0usize;
    while let Some((start, end, args)) = find_named_call(line, "yield_to", cursor) {
        let parsed = split_top_level_args(&args);
        if parsed.len() != 3
            || !looks_like_signal_object_form(&parsed[0])
            || !looks_like_string_literal(&parsed[1])
        {
            manual_followups.push(GutMigrationIssue {
                line: line_no,
                pattern: "yield_to(...)".to_string(),
                message: "yield_to() only auto-rewrites for `(object, \"signal\", timeout_seconds)` calls without a message.".to_string(),
            });
            cursor = end;
            continue;
        }
        let timeout_expr = format!("int(ceil(({}) * 1000.0))", parsed[2].trim());
        line.replace_range(
            start..end,
            &format!(
                "assert_signal({}, {}, {})",
                parsed[0], parsed[1], timeout_expr
            ),
        );
        rewritten.push(GutMigrationAction {
            line: line_no,
            pattern: "yield_to(obj, \"signal\", timeout_seconds)".to_string(),
            replacement: "assert_signal(obj, \"signal\", timeout_ms)".to_string(),
        });
        cursor = start + "assert_signal".len();
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

fn parse_dot_new_suffix(suffix: &str) -> Option<usize> {
    let re = Regex::new(r"^\s*\.new\(\s*\)").ok()?;
    re.find(suffix).map(|m| m.end())
}

fn parse_stub_return_suffix(suffix: &str) -> Option<(usize, String)> {
    let re = Regex::new(r"^\s*\.to_return(?:_const)?\(").ok()?;
    let m = re.find(suffix)?;
    let open_idx = m.end() - 1;
    let close_idx = find_matching_paren(suffix, open_idx)?;
    Some((close_idx + 1, suffix[(open_idx + 1)..close_idx].to_string()))
}

fn looks_like_signal_object_form(arg: &str) -> bool {
    let trimmed = arg.trim();
    !trimmed.is_empty() && !trimmed.contains('.') && !trimmed.starts_with("Signal(")
}

fn looks_like_string_literal(arg: &str) -> bool {
    let trimmed = arg.trim();
    (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
}

fn is_literal_one(arg: &str) -> bool {
    matches!(arg.trim(), "1" | "1.0")
}

fn dedup_issues(issues: &mut Vec<GutMigrationIssue>) {
    issues.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.pattern.cmp(&b.pattern)));
    issues.dedup_by(|a, b| a.line == b.line && a.pattern == b.pattern && a.message == b.message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_basic_gut_extends_and_waits() {
        let src = br#"extends GutTest

func test_waits():
    await wait_frames(2)
    await yield_for(0.5)
"#;
        let out = migrate_gut_source(src, "res://test/test_waits.gd");
        let text = String::from_utf8(out.source).unwrap();
        assert!(text.contains("extends GdTest"));
        assert!(text.contains("wait_physics_frames(2)"));
        assert!(text.contains("wait_physics_seconds(0.5)"));
        assert_eq!(out.status, GutMigrationStatus::CandidateForExecution);
    }

    #[test]
    fn rewrites_signal_assertions_and_doubles() {
        let src = br#"extends GutTest

func test_signal_and_mock():
    var m = double(HealthComponent).new()
    stub(m, "get_hp").to_return(42)
    assert_signal_emitted(self, "done")
    assert_signal_emit_count(self, "done", 2)
    assert_signal_emitted_with_parameters(self, "done", [42])
"#;
        let out = migrate_gut_source(src, "res://test/test_signal.gd");
        let text = String::from_utf8(out.source).unwrap();
        assert!(text.contains("var m = mock(HealthComponent)"));
        assert!(text.contains("stub(m, \"get_hp\", 42)"));
        assert!(text.contains("assert_signal(self, \"done\")"));
        assert!(text.contains("assert_signal_count(self, \"done\", 2)"));
        assert!(text.contains("assert_signal_with_args(self, \"done\", [42])"));
        assert_eq!(out.status, GutMigrationStatus::CandidateForExecution);
    }

    #[test]
    fn flags_unsupported_patterns() {
        let src = br#"extends GutTest

func should_skip_script():
    return true

func test_unsupported():
    watch_signals(self)
    use_parameters([1, 2, 3])
    assert_engine_error("boom")
    assert_signal_emitted(self.done)
    await wait_process_frames(2)
"#;
        let out = migrate_gut_source(src, "res://test/test_blocked.gd");
        let text = String::from_utf8(out.source).unwrap();
        assert!(text.starts_with("# gdtest migrate gut: manual follow-up required"));
        assert_eq!(out.status, GutMigrationStatus::RewriteIncomplete);
        assert!(out
            .blocked
            .iter()
            .any(|i| i.pattern == "should_skip_script()"));
        assert!(out
            .blocked
            .iter()
            .any(|i| i.pattern == "use_parameters(...)"));
        assert!(out
            .blocked
            .iter()
            .any(|i| i.pattern == "assert_engine_error*"));
        assert!(out
            .manual_followups
            .iter()
            .any(|i| i.pattern == "watch_signals(...)"));
        assert!(out
            .manual_followups
            .iter()
            .any(|i| i.pattern == "assert_signal_emitted(signal_ref, ...)"));
        assert!(out
            .manual_followups
            .iter()
            .any(|i| i.pattern == "wait_process_frames(...)"));
    }

    #[test]
    fn rewrites_single_frame_wait_process_variants() {
        let src = br#"extends GutTest

func test_one_frame():
    await wait_process_frames(1)
    await wait_idle_frames(1)
"#;
        let out = migrate_gut_source(src, "res://test/test_process_frame.gd");
        let text = String::from_utf8(out.source).unwrap();
        assert!(text.contains("await wait_process_frame()"));
        assert_eq!(text.matches("wait_process_frame()").count(), 2);
    }
}
