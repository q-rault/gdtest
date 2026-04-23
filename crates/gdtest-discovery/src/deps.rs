//! GDScript dependency graph for test impact analysis (internal).
//!
//! Parses `extends`, `preload()`, and `load()` calls to build a reverse
//! dependency map: changed_file → test files that transitively import it.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use walkdir::WalkDir;

/// Maps res:// path → set of res:// paths.
pub type DepGraph = HashMap<String, HashSet<String>>;

/// Parse all `res://` dependencies referenced in a GDScript source file.
/// Handles `extends "res://..."`, `preload("res://...")`, `load("res://...")`.
pub fn parse_file_deps(source: &[u8]) -> Vec<String> {
    let text = match std::str::from_utf8(source) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut deps = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(path) = extract_res_path(line, "preload(") {
            deps.push(path);
        }
        // load() — skip positions that are actually part of preload()
        if let Some(path) = extract_standalone_load(line) {
            deps.push(path);
        }
        if let Some(path) = extract_res_path(line, "extends ") {
            deps.push(path);
        }
    }
    deps
}

/// Find `load("res://...")` on the line, skipping any occurrence that is
/// actually the `load(` part of `preload(`.
fn extract_standalone_load(line: &str) -> Option<String> {
    let mut offset = 0;
    while let Some(pos) = line[offset..].find("load(") {
        let abs_pos = offset + pos;
        offset = abs_pos + 5;
        // Skip if preceded by "pre"
        if abs_pos >= 3 && &line[abs_pos - 3..abs_pos] == "pre" {
            continue;
        }
        return extract_res_path(&line[abs_pos..], "load(");
    }
    None
}

fn extract_res_path(line: &str, keyword: &str) -> Option<String> {
    let kw_pos = line.find(keyword)?;
    let after_kw = &line[kw_pos + keyword.len()..];
    let (q_idx, q_char) = after_kw
        .char_indices()
        .find(|(_, c)| *c == '"' || *c == '\'')?;
    let path_start = &after_kw[q_idx + q_char.len_utf8()..];
    if !path_start.starts_with("res://") {
        return None;
    }
    let end = path_start.find(q_char)?;
    Some(path_start[..end].to_string())
}

/// Build forward and reverse dependency graphs by scanning all `.gd` files.
///
/// - `forward[A]` = files A imports
/// - `reverse[B]` = files that import B
pub fn build_dep_graph(project_abs: &Path) -> (DepGraph, DepGraph) {
    let mut forward: DepGraph = HashMap::new();
    let mut reverse: DepGraph = HashMap::new();

    for entry in WalkDir::new(project_abs).follow_links(false) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gd") {
            continue;
        }
        let Ok(rel) = path.strip_prefix(project_abs) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str.starts_with(".godot/") {
            continue;
        }
        let res_path = format!("res://{rel_str}");
        forward.entry(res_path.clone()).or_default();
        reverse.entry(res_path.clone()).or_default();

        let Ok(source) = std::fs::read(path) else {
            continue;
        };
        for dep in parse_file_deps(&source) {
            forward
                .entry(res_path.clone())
                .or_default()
                .insert(dep.clone());
            reverse.entry(dep).or_default().insert(res_path.clone());
        }
    }

    (forward, reverse)
}

/// Return all res:// paths that transitively depend on any file in `changed`.
/// The result does NOT include the changed files themselves — only their dependents.
pub fn affected_files(changed: &HashSet<String>, reverse: &DepGraph) -> HashSet<String> {
    let mut affected: HashSet<String> = HashSet::new();
    let mut frontier: Vec<String> = changed.iter().cloned().collect();

    while let Some(file) = frontier.pop() {
        if let Some(dependents) = reverse.get(&file) {
            for dep in dependents {
                if affected.insert(dep.clone()) {
                    frontier.push(dep.clone());
                }
            }
        }
    }

    affected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_preload_and_extends() {
        let src = br#"extends "res://src/base.gd"
const Util = preload("res://src/util.gd")
var x = load("res://data/config.gd")
"#;
        let deps = parse_file_deps(src);
        assert!(deps.contains(&"res://src/base.gd".to_string()), "extends");
        assert!(deps.contains(&"res://src/util.gd".to_string()), "preload");
        assert!(deps.contains(&"res://data/config.gd".to_string()), "load");
    }

    #[test]
    fn skips_comments_and_non_res() {
        let src = br#"# preload("res://commented.gd")
load("user://settings.cfg")
preload("res://valid.gd")
"#;
        let deps = parse_file_deps(src);
        assert_eq!(deps, vec!["res://valid.gd".to_string()]);
    }

    #[test]
    fn affected_files_transitive() {
        let mut reverse: DepGraph = HashMap::new();
        reverse.insert("res://a.gd".to_string(), ["res://b.gd".to_string()].into());
        reverse.insert("res://b.gd".to_string(), ["res://c.gd".to_string()].into());

        let changed: HashSet<String> = ["res://a.gd".to_string()].into();
        let affected = affected_files(&changed, &reverse);
        assert!(affected.contains("res://b.gd"), "direct dependent");
        assert!(affected.contains("res://c.gd"), "transitive dependent");
        assert!(
            !affected.contains("res://a.gd"),
            "changed file itself not in affected"
        );
    }
}
