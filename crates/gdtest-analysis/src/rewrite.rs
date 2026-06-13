//! GDScript assert rewriter — tree-sitter preprocessing for pytest-style rich diffs.
//!
//! Rewrites `assert(expr)` calls into `Gdt._rewrite_*(...)` calls that carry
//! operand values AND the source-text of the condition to the failure formatter,
//! giving messages like:
//!
//! ```text
//! FAIL  assert(player.hp == 100)  (test_player.gd:42)
//!   left:  87
//!   right: 100
//! ```
//!
//! Only single-line assert calls are rewritten; multi-line ones are left unchanged
//! so line counts are never disturbed.
//!
//! # Tree-sitter structure (verified via dump_assert_tree test)
//!
//! ```text
//! expression_statement
//!   call
//!     identifier "assert"          ← function name
//!     arguments
//!       (
//!       binary_operator            ← condition (may be any expression)
//!         <left>
//!         ==                       ← unnamed operator token
//!         <right>
//!       [, string]                 ← optional message
//!       )
//! ```

use std::path::Path;

use anyhow::{Context, Result};
use tree_sitter::Parser;

// ── Public API ────────────────────────────────────────────────────────────────

pub struct RewriteOutput {
    /// Modified source bytes. Identical to input when nothing was rewritten.
    pub source: Vec<u8>,
    /// Whether any assert calls were actually rewritten.
    pub changed: bool,
}

pub fn rewrite_file(path: &Path, original_res_path: &str) -> Result<RewriteOutput> {
    let source = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(rewrite_source(&source, original_res_path))
}

pub fn rewrite_source(source: &[u8], original_res_path: &str) -> RewriteOutput {
    let Some(asserts) = find_asserts(source) else {
        return RewriteOutput {
            source: source.to_vec(),
            changed: false,
        };
    };
    if asserts.is_empty() {
        return RewriteOutput {
            source: source.to_vec(),
            changed: false,
        };
    }

    // Collect replacements in reverse byte order so earlier positions stay valid.
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    for a in &asserts {
        if let Some(repl) = build_replacement(a, source, original_res_path) {
            replacements.push((a.start_byte, a.end_byte, repl));
        }
    }
    if replacements.is_empty() {
        return RewriteOutput {
            source: source.to_vec(),
            changed: false,
        };
    }

    replacements.sort_by(|a, b| b.0.cmp(&a.0));
    let mut result = source.to_vec();
    for (start, end, replacement) in &replacements {
        result.splice(start..end, replacement.bytes());
    }
    RewriteOutput {
        source: result,
        changed: true,
    }
}

// ── Internal types ────────────────────────────────────────────────────────────

struct AssertInfo {
    /// Byte range of the `call` node ("assert(...)").
    start_byte: usize,
    end_byte: usize,
    /// 1-based original line number.
    line: u32,
    /// Full text of the condition expression.
    condition_text: String,
    /// Parsed binary comparison, if the condition is one.
    binary: Option<BinaryInfo>,
}

struct BinaryInfo {
    op: BinaryOp,
    left_text: String,
    right_text: String,
}

#[derive(Clone, Copy)]
enum BinaryOp {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
}

impl BinaryOp {
    fn gdt_suffix(self) -> &'static str {
        match self {
            BinaryOp::Eq => "eq",
            BinaryOp::Ne => "ne",
            BinaryOp::Lt => "lt",
            BinaryOp::Lte => "lte",
            BinaryOp::Gt => "gt",
            BinaryOp::Gte => "gte",
        }
    }
    fn from_text(op: &str) -> Option<Self> {
        match op {
            "==" => Some(BinaryOp::Eq),
            "!=" => Some(BinaryOp::Ne),
            "<" => Some(BinaryOp::Lt),
            "<=" => Some(BinaryOp::Lte),
            ">" => Some(BinaryOp::Gt),
            ">=" => Some(BinaryOp::Gte),
            _ => None,
        }
    }
}

// ── Tree-sitter traversal ─────────────────────────────────────────────────────

fn find_asserts(source: &[u8]) -> Option<Vec<AssertInfo>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_gdscript::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(source, None)?;
    let mut out = Vec::new();
    collect_assert_calls(tree.root_node(), source, &mut out);
    Some(out)
}

fn collect_assert_calls(node: tree_sitter::Node, source: &[u8], out: &mut Vec<AssertInfo>) {
    if node.kind() == "call" {
        if let Some(info) = parse_assert_call(node, source) {
            out.push(info);
            return; // don't descend further into this call
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            collect_assert_calls(child, source, out);
        }
    }
}

/// Returns Some if this `call` node is a single-line `assert(...)` call.
fn parse_assert_call(node: tree_sitter::Node, source: &[u8]) -> Option<AssertInfo> {
    // First child of `call` is the function expression (identifier "assert").
    let func_node = node.child(0)?;
    if func_node.kind() != "identifier" {
        return None;
    }
    if func_node.utf8_text(source).ok()? != "assert" {
        return None;
    }

    // Only rewrite single-line calls.
    if node.start_position().row != node.end_position().row {
        return None;
    }

    // Second child is `arguments`.
    let args_node = node.child(1)?;
    if args_node.kind() != "arguments" {
        return None;
    }

    // Walk arguments children: "(" [condition] ["," message] ")"
    // Named children are the actual arguments; unnamed are punctuation.
    let mut condition_node: Option<tree_sitter::Node> = None;
    for i in 0..args_node.child_count() {
        let child = args_node.child(i as u32)?;
        if child.kind() == "(" || child.kind() == ")" || child.kind() == "," {
            continue;
        }
        if condition_node.is_none() {
            condition_node = Some(child);
            break; // stop at first arg (condition); ignore message
        }
    }

    let condition_node = condition_node?;
    let condition_text = condition_node.utf8_text(source).ok()?.to_string();
    let binary = try_parse_binary(condition_node, source);

    Some(AssertInfo {
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        line: node.start_position().row as u32 + 1,
        condition_text,
        binary,
    })
}

/// If node is a `binary_operator` with a comparison op, extract both sides.
fn try_parse_binary(node: tree_sitter::Node, source: &[u8]) -> Option<BinaryInfo> {
    if node.kind() != "binary_operator" {
        return None;
    }

    // Children: <left> <op_token> <right>
    // Named children are left and right; unnamed is the operator token.
    let mut left: Option<tree_sitter::Node> = None;
    let mut right: Option<tree_sitter::Node> = None;
    let mut op: Option<BinaryOp> = None;

    for i in 0..node.child_count() {
        let child = node.child(i as u32)?;
        if child.is_named() {
            if left.is_none() {
                left = Some(child);
            } else {
                right = Some(child);
            }
        } else if let Ok(txt) = child.utf8_text(source) {
            if let Some(parsed) = BinaryOp::from_text(txt) {
                op = Some(parsed);
            }
        }
    }

    Some(BinaryInfo {
        op: op?,
        left_text: left?.utf8_text(source).ok()?.to_string(),
        right_text: right?.utf8_text(source).ok()?.to_string(),
    })
}

// ── Code generation ───────────────────────────────────────────────────────────

fn build_replacement(info: &AssertInfo, source: &[u8], original_res_path: &str) -> Option<String> {
    let indent = leading_whitespace(info.start_byte, source).to_string();
    let file = escape_gdstring(original_res_path);
    let line = info.line;

    let repl = if let Some(bin) = &info.binary {
        let expr = escape_gdstring(&info.condition_text);
        format!(
            r#"{indent}Gdt._rewrite_{suffix}({lhs}, {rhs}, "{expr}", "{file}", {line})"#,
            suffix = bin.op.gdt_suffix(),
            lhs = bin.left_text,
            rhs = bin.right_text,
        )
    } else {
        let expr = escape_gdstring(&info.condition_text);
        format!(
            r#"{indent}Gdt._rewrite_true({cond}, "{expr}", "{file}", {line})"#,
            cond = info.condition_text,
        )
    };

    Some(repl)
}

fn leading_whitespace(pos: usize, source: &[u8]) -> &str {
    let line_start = source[..pos]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0);
    let prefix = &source[line_start..pos];
    let ws_end = prefix
        .iter()
        .take_while(|&&b| b == b' ' || b == b'\t')
        .count();
    std::str::from_utf8(&prefix[..ws_end]).unwrap_or("")
}

fn escape_gdstring(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_assert_tree() {
        let src = b"extends RefCounted\n\nfunc test_foo():\n\tassert(x == y)\n\tassert(player.hp == 100, \"hp check\")\n\tassert(flag)\n";
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_gdscript::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        fn dump(node: tree_sitter::Node, src: &[u8], indent: usize) {
            let preview: String = node
                .utf8_text(src)
                .unwrap_or("?")
                .chars()
                .take(40)
                .collect::<String>()
                .replace('\n', "↵");
            println!(
                "{}{} [{}:{}-{}:{}] {:?}",
                "  ".repeat(indent),
                node.kind(),
                node.start_position().row + 1,
                node.start_position().column,
                node.end_position().row + 1,
                node.end_position().column,
                preview
            );
            for i in 0..node.child_count() {
                if let Some(c) = node.child(i as u32) {
                    dump(c, src, indent + 1);
                }
            }
        }
        dump(tree.root_node(), src, 0);
    }

    #[test]
    fn rewrites_eq_comparison() {
        let src = b"extends RefCounted\n\nfunc test_foo():\n\tassert(player.hp == 100)\n";
        let out = rewrite_source(src, "res://test_player.gd");
        assert!(out.changed, "expected rewrite");
        let s = std::str::from_utf8(&out.source).unwrap();
        assert!(s.contains("Gdt._rewrite_eq"), "missing _rewrite_eq:\n{s}");
        assert!(s.contains("player.hp"), "missing left operand:\n{s}");
        assert!(s.contains("100"), "missing right operand:\n{s}");
        assert!(s.contains("test_player.gd"), "missing original file:\n{s}");
        assert!(
            s.contains("\"player.hp == 100\""),
            "missing expr text:\n{s}"
        );
    }

    #[test]
    fn rewrites_ne() {
        let src = b"func test_foo():\n\tassert(a != b)\n";
        let out = rewrite_source(src, "res://test.gd");
        assert!(out.changed);
        let s = std::str::from_utf8(&out.source).unwrap();
        assert!(s.contains("_rewrite_ne"), "{s}");
    }

    #[test]
    fn rewrites_lt_lte_gt_gte() {
        for (op, suffix) in [("<", "lt"), ("<=", "lte"), (">", "gt"), (">=", "gte")] {
            let src = format!("func test_foo():\n\tassert(a {op} b)\n");
            let out = rewrite_source(src.as_bytes(), "res://test.gd");
            assert!(out.changed, "op={op}");
            let s = std::str::from_utf8(&out.source).unwrap();
            assert!(
                s.contains(&format!("_rewrite_{suffix}")),
                "op={op} missing suffix:\n{s}"
            );
        }
    }

    #[test]
    fn rewrites_bool_assert() {
        let src = b"func test_foo():\n\tassert(is_alive)\n";
        let out = rewrite_source(src, "res://test.gd");
        assert!(out.changed);
        let s = std::str::from_utf8(&out.source).unwrap();
        assert!(s.contains("Gdt._rewrite_true"), "{s}");
        assert!(s.contains("is_alive"), "{s}");
    }

    #[test]
    fn no_rewrite_multiline() {
        let src = b"func test_foo():\n\tassert(\n\t\tx == y\n\t)\n";
        let out = rewrite_source(src, "res://test.gd");
        assert!(!out.changed, "multi-line assert must not be rewritten");
    }

    #[test]
    fn preserves_indentation() {
        let src = b"func test_foo():\n\tassert(x == y)\n";
        let out = rewrite_source(src, "res://test.gd");
        let s = std::str::from_utf8(&out.source).unwrap();
        let line = s.lines().find(|l| l.contains("_rewrite_")).unwrap();
        assert!(line.starts_with('\t'), "indentation lost: {line:?}");
    }

    #[test]
    fn no_assert_unchanged() {
        let src = b"func test_foo():\n\tvar x = 1\n";
        let out = rewrite_source(src, "res://test.gd");
        assert!(!out.changed);
        assert_eq!(out.source, src);
    }

    #[test]
    fn multiple_asserts_in_one_function() {
        let src = b"func test_foo():\n\tassert(a == b)\n\tassert(c != d)\n\tassert(flag)\n";
        let out = rewrite_source(src, "res://test.gd");
        assert!(out.changed);
        let s = std::str::from_utf8(&out.source).unwrap();
        assert!(s.contains("_rewrite_eq"), "{s}");
        assert!(s.contains("_rewrite_ne"), "{s}");
        assert!(s.contains("_rewrite_true"), "{s}");
    }

    #[test]
    fn line_number_embedded() {
        // assert is on line 3 (1-based)
        let src = b"extends RefCounted\n\nfunc test_foo():\n\tassert(x == y)\n";
        let out = rewrite_source(src, "res://test.gd");
        let s = std::str::from_utf8(&out.source).unwrap();
        // Line 4 (1-based: extends=1, blank=2, func=3, assert=4)
        assert!(
            s.contains(", 4)"),
            "line number not embedded correctly:\n{s}"
        );
    }

    #[test]
    fn does_not_rewrite_non_assert_calls() {
        let src = b"func test_foo():\n\tmy_assert(x == y)\n\tassert_eq(a, b)\n";
        let out = rewrite_source(src, "res://test.gd");
        // my_assert and assert_eq are not the built-in assert keyword
        assert!(
            !out.changed,
            "should not rewrite non-assert calls:\n{}",
            std::str::from_utf8(&out.source).unwrap()
        );
    }
}
