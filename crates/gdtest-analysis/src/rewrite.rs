//! GDScript assert rewriter: preprocessing for pytest-style rich diffs.
//!
//! Rewrites `assert(expr)` statements into `Gdt._rewrite_*(...)` calls that carry
//! operand values AND the source-text of the condition to the failure formatter,
//! giving messages like:
//!
//! ```text
//! FAIL  assert(player.hp == 100)  (test_player.gd:42)
//!   left:  87
//!   right: 100
//! ```
//!
//! Only single-line asserts are rewritten; multi-line ones are left unchanged
//! so line counts are never disturbed.
//!
//! In gdsyntax's tree an assert is an `ASSERT_STMT`: the `assert` keyword and
//! an `ARG_LIST` holding the condition and an optional message. The replaced
//! range runs from the keyword to the closing parenthesis.

use std::path::Path;

use anyhow::{Context, Result};
use gdsyntax::SyntaxKind;
use gdsyntax::ast::{AssertStmt, AstNode, BinaryOp as AstBinaryOp, Expr};

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

    replacements.sort_by_key(|r| std::cmp::Reverse(r.0));
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
    /// Byte range of the statement, from `assert` to `)`.
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
    fn from_ast(op: AstBinaryOp) -> Option<Self> {
        match op {
            AstBinaryOp::Equal => Some(BinaryOp::Eq),
            AstBinaryOp::NotEqual => Some(BinaryOp::Ne),
            AstBinaryOp::Less => Some(BinaryOp::Lt),
            AstBinaryOp::LessEqual => Some(BinaryOp::Lte),
            AstBinaryOp::Greater => Some(BinaryOp::Gt),
            AstBinaryOp::GreaterEqual => Some(BinaryOp::Gte),
            _ => None,
        }
    }
}

// ── Tree traversal ────────────────────────────────────────────────────────────

fn find_asserts(source: &[u8]) -> Option<Vec<AssertInfo>> {
    // Byte offsets index into `source`, so it must be the parsed text as is.
    let text = std::str::from_utf8(source).ok()?;
    let tree = gdsyntax::parse(text);
    Some(
        tree.root()
            .descendants()
            .filter_map(AssertStmt::cast)
            .filter_map(|stmt| parse_assert(&tree, &stmt))
            .collect(),
    )
}

/// Returns Some if this is a complete, single-line `assert(...)`.
fn parse_assert(tree: &gdsyntax::SyntaxTree, stmt: &AssertStmt) -> Option<AssertInfo> {
    let keyword = stmt
        .syntax()
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == SyntaxKind::ASSERT_KW)?;
    let args = stmt.arguments()?;
    // An unclosed argument list is a syntax error; leave it alone.
    let close = args.syntax().last_token()?;
    if close.kind() != SyntaxKind::PARENTHESIS_CLOSE {
        return None;
    }
    let start_byte: usize = keyword.text_range().start().into();
    let end_byte: usize = close.text_range().end().into();

    // Only rewrite single-line asserts.
    let line = tree.position(start_byte).line;
    if tree.position(end_byte).line != line {
        return None;
    }

    // The first argument is the condition; the message is ignored.
    let condition = args.arguments().next()?;
    let binary = match &condition {
        Expr::Binary(binary) => (|| {
            Some(BinaryInfo {
                op: BinaryOp::from_ast(binary.op()?)?,
                left_text: node_text(tree, binary.lhs()?.syntax()),
                right_text: node_text(tree, binary.rhs()?.syntax()),
            })
        })(),
        _ => None,
    };

    Some(AssertInfo {
        start_byte,
        end_byte,
        line: line as u32,
        condition_text: node_text(tree, condition.syntax()),
        binary,
    })
}

/// The source text of a node, without surrounding whitespace or comments.
fn node_text(tree: &gdsyntax::SyntaxTree, node: &gdsyntax::SyntaxNode) -> String {
    tree.source()[node.text_range()].trim().to_string()
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
    fn keeps_message_out_and_comment_after() {
        let src = b"func test_foo():\n\tassert(a == b, \"msg\")  # note\n";
        let out = rewrite_source(src, "res://test.gd");
        let s = std::str::from_utf8(&out.source).unwrap();
        assert!(
            s.contains("\tGdt._rewrite_eq(a, b, \"a == b\", \"res://test.gd\", 2)  # note\n"),
            "{s}"
        );
    }

    #[test]
    fn rewrites_asserts_in_lambdas() {
        let src = b"func test_foo():\n\tvar f := func(): assert(x)\n";
        let out = rewrite_source(src, "res://test.gd");
        assert!(out.changed);
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
