//! GDScript mutation testing operators.
//!
//! Generates single-point mutations by replacing operators/literals in
//! GDScript source text. Mutations inside string literals and comments are
//! skipped via lightweight string-state tracking.

/// One mutation: byte range in the original source → replacement text.
#[derive(Debug, Clone)]
pub struct Mutation {
    /// Byte range in the original source to replace.
    pub start: usize,
    pub end: usize,
    /// What was there before.
    pub from: &'static str,
    /// What it becomes.
    pub to: &'static str,
    /// 1-based source line number (for reporting).
    pub line: u32,
}

impl Mutation {
    /// Apply this mutation to `source`, returning the mutated bytes.
    pub fn apply(&self, source: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(source.len() + self.to.len());
        out.extend_from_slice(&source[..self.start]);
        out.extend_from_slice(self.to.as_bytes());
        out.extend_from_slice(&source[self.end..]);
        out
    }

    /// Human-readable description for reports.
    pub fn description(&self) -> String {
        format!("line {}: `{}` → `{}`", self.line, self.from, self.to)
    }
}

/// Mutation operator pairs: (from, to). Longer tokens must come before
/// shorter prefixes so `>=` is matched before `>`.
static OPERATORS: &[(&str, &str)] = &[
    (">=", ">"),
    ("<=", "<"),
    ("!=", "=="),
    ("==", "!="),
    (" and ", " or "),
    (" or ", " and "),
    (" + ", " - "),
    (" - ", " + "),
    (" * ", " / "),
    (" / ", " * "),
    (">", ">="),
    ("<", "<="),
    ("true", "false"),
    ("false", "true"),
];

/// Find all single-point mutations in `source`. Returns them in source order.
pub fn find_mutations(source: &[u8]) -> Vec<Mutation> {
    let text = match std::str::from_utf8(source) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut mutations: Vec<Mutation> = Vec::new();
    let mut byte_offset: usize = 0;

    for (line_idx, raw_line) in text.lines().enumerate() {
        let line_num = (line_idx + 1) as u32;

        // Strip trailing \r if present (CRLF files)
        let line = raw_line.trim_end_matches('\r');

        // Find the effective end of code (before any # comment, respecting strings)
        let code_end = comment_start(line);
        let code = &line[..code_end];

        // Collect candidate mutations for this line, tracking used ranges to
        // avoid overlapping mutations (e.g. `!=` and `==` both matching `!=`).
        let mut used: Vec<(usize, usize)> = Vec::new();

        for &(from, to) in OPERATORS {
            let mut scan = 0usize;
            while scan < code.len() {
                let Some(rel) = code[scan..].find(from) else {
                    break;
                };
                let abs = scan + rel;
                scan = abs + 1; // advance past this position

                // Skip if overlapping an already-emitted mutation.
                if used.iter().any(|&(s, e)| s < abs + from.len() && e > abs) {
                    continue;
                }

                // Skip if inside a string literal.
                if inside_string(code, abs) {
                    continue;
                }

                // Guard single-char `>` / `<` against matching inside `>=`, `<=`, etc.
                if from == ">" || from == "<" {
                    let prev = abs.checked_sub(1).map(|i| code.as_bytes()[i]);
                    let next = code.as_bytes().get(abs + from.len()).copied();
                    if matches!(prev, Some(b'>' | b'<' | b'=' | b'!')) || matches!(next, Some(b'='))
                    {
                        continue;
                    }
                }

                // Guard `true` / `false` to match only at word boundaries.
                if from == "true" || from == "false" {
                    let prev = abs.checked_sub(1).map(|i| code.as_bytes()[i]);
                    let next = code.as_bytes().get(abs + from.len()).copied();
                    let prev_ok = prev.is_none_or(|b| !b.is_ascii_alphanumeric() && b != b'_');
                    let next_ok = next.is_none_or(|b| !b.is_ascii_alphanumeric() && b != b'_');
                    if !prev_ok || !next_ok {
                        continue;
                    }
                }

                used.push((abs, abs + from.len()));
                mutations.push(Mutation {
                    start: byte_offset + abs,
                    end: byte_offset + abs + from.len(),
                    from,
                    to,
                    line: line_num,
                });
                scan = abs + from.len();
            }
        }

        byte_offset += raw_line.len() + 1; // +1 for the \n stripped by lines()
    }

    mutations
}

/// Return the byte index where a `#` comment begins (outside strings), or
/// `line.len()` if there is no comment.
fn comment_start(line: &str) -> usize {
    let mut in_str: Option<char> = None;
    for (i, c) in line.char_indices() {
        match in_str {
            Some(q) if c == q => in_str = None,
            None if c == '"' || c == '\'' => in_str = Some(c),
            None if c == '#' => return i,
            _ => {}
        }
    }
    line.len()
}

/// Return true if byte position `pos` in `line` is inside a string literal.
fn inside_string(line: &str, pos: usize) -> bool {
    let mut in_str: Option<char> = None;
    for (i, c) in line.char_indices() {
        if i >= pos {
            break;
        }
        match in_str {
            Some(q) if c == q => in_str = None,
            None if c == '"' || c == '\'' => in_str = Some(c),
            _ => {}
        }
    }
    in_str.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_comparison_mutations() {
        let src = b"if health > 0:\n    pass\n";
        let muts = find_mutations(src);
        assert!(
            muts.iter()
                .any(|m| m.from == ">" && m.to == ">=" && m.line == 1),
            "expected > → >= mutation: {muts:?}"
        );
    }

    #[test]
    fn skips_greater_equal_for_plain_greater() {
        let src = b"if x >= 0:\n    pass\n";
        let muts = find_mutations(src);
        // >= → > is valid, but > → >= should not also appear for the same position
        let gt_to_gte: Vec<_> = muts
            .iter()
            .filter(|m| m.from == ">" && m.to == ">=")
            .collect();
        assert!(
            gt_to_gte.is_empty(),
            "bare > should not appear inside >=: {gt_to_gte:?}"
        );
    }

    #[test]
    fn skips_operators_in_strings() {
        let src = b"var s = \"a > b\"\nif x > 0:\n    pass\n";
        let muts = find_mutations(src);
        // Only line 2 should have the mutation, not line 1 (inside string)
        let line1: Vec<_> = muts.iter().filter(|m| m.line == 1).collect();
        assert!(line1.is_empty(), "no mutations inside strings: {line1:?}");
        assert!(muts.iter().any(|m| m.line == 2 && m.from == ">"));
    }

    #[test]
    fn skips_comments() {
        let src = b"# if x > 0: pass\nif y > 0:\n    pass\n";
        let muts = find_mutations(src);
        let line1: Vec<_> = muts.iter().filter(|m| m.line == 1).collect();
        assert!(line1.is_empty(), "no mutations in comments: {line1:?}");
    }

    #[test]
    fn true_false_mutation_respects_word_boundary() {
        // "true" inside identifiers like "is_true" or "retrue" must not be mutated.
        let src = b"var is_true := true\nvar retrue := 1\n";
        let muts = find_mutations(src);
        let true_muts: Vec<_> = muts
            .iter()
            .filter(|m| m.from == "true" && m.to == "false")
            .collect();
        assert_eq!(
            true_muts.len(),
            1,
            "expected exactly 1 true→false: {true_muts:?}"
        );
        // The one mutation must be the rvalue on line 1, not inside the identifier.
        assert_eq!(true_muts[0].line, 1);
        let col = true_muts[0].start;
        assert!(
            col > 10,
            "mutation should be past the identifier, got byte offset {col}"
        );
    }

    #[test]
    fn apply_mutation_replaces_bytes() {
        let src = b"if x > 0:\n    pass\n";
        let muts = find_mutations(src);
        let gt_mut = muts.iter().find(|m| m.from == ">").unwrap();
        let mutated = gt_mut.apply(src);
        let mutated_str = std::str::from_utf8(&mutated).unwrap();
        assert!(
            mutated_str.contains(">="),
            "mutation not applied: {mutated_str}"
        );
        assert!(
            !mutated_str.contains(" > ") || mutated_str.contains(">="),
            "original not replaced"
        );
    }
}
