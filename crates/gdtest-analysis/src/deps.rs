//! Test impact analysis: which test files a change can affect.
//!
//! The graph links every file of the project to the files it uses, as far as
//! they can be read from the source:
//!
//! - in a script: `extends`, `preload`/`load` and any other `"res://…"` or
//!   `"uid://…"` string, the `class_name` of another script used by name, and
//!   an autoload used by name;
//! - in a scene or text resource (`.tscn`, `.tres`): its `ext_resource`s.
//!
//! A test file is affected by a change when it reaches a changed file through
//! these links. What the source cannot tell is handled cautiously:
//!
//! - a script that loads a computed path (`load(path)`) may load any file, so
//!   the tests that reach it always run; with a known folder
//!   (`load("res://items/" + id)`), it depends on every file of that folder;
//! - `project.godot`, the autoloads and the test runner's own files change
//!   what every test runs with, so a change to one of them affects every test.
//!
//! What the tests did when they last ran adds to that, as Vitest learns a
//! test's dynamic imports by running it: the worker reports the scenes and
//! scripts of the nodes each test added to the tree, and [`record_uses`]
//! keeps them per test file. A test file uses what it was seen to use, and
//! once seen, a computed `load()` it reaches no longer makes it run on every
//! change.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use gdsyntax::SyntaxKind;
use walkdir::WalkDir;

/// Maps res:// path → set of res:// paths.
pub type DepGraph = HashMap<String, HashSet<String>>;

/// What one file uses, read from its source.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FileDeps {
    /// `res://` paths, and `uid://` ids still to resolve.
    pub paths: BTreeSet<String>,
    /// Names that may be a `class_name` or an autoload.
    pub names: BTreeSet<String>,
    /// Folders it loads computed paths from: `res://items/`.
    pub folders: BTreeSet<String>,
    /// It loads a path the source does not tell.
    pub loads_unknown: bool,
    /// The `class_name` it declares.
    pub class_name: Option<String>,
}

/// Which test files a change affects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Impact {
    /// Every test: `reason` says why.
    All { reason: String },
    /// These test files, and no other.
    Files(BTreeSet<String>),
}

/// The project's dependency graph.
#[derive(Debug, Default)]
pub struct Graph {
    /// `forward[a]`: the files `a` uses.
    pub forward: DepGraph,
    /// `reverse[b]`: the files that use `b`.
    pub reverse: DepGraph,
    /// file → folders it loads computed paths from.
    folders: HashMap<String, BTreeSet<String>>,
    /// Files that load paths the source does not tell.
    unknown: HashSet<String>,
    /// Files whose change affects every test, with the reason.
    global: HashMap<String, String>,
    /// Folders whose files affect every test (the test runner's addon).
    global_folders: Vec<(String, String)>,
    /// Test files seen running, with what they used: see [`record_uses`].
    seen: HashSet<String>,
}

/// Where gdtest keeps what each test file used when it last ran.
pub fn uses_path(project_abs: &Path) -> std::path::PathBuf {
    project_abs.join(".godot").join("gdtest").join("uses.json")
}

/// What each test file was seen to use: test file → res:// paths.
pub fn read_uses(project_abs: &Path) -> HashMap<String, BTreeSet<String>> {
    std::fs::read_to_string(uses_path(project_abs))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Adds what the test files of a run used to what was seen before. Kept
/// across runs, so a test that loads different scenes on different runs
/// depends on all of them.
pub fn record_uses(
    project_abs: &Path,
    uses: &HashMap<String, BTreeSet<String>>,
) -> anyhow::Result<()> {
    let mut all = read_uses(project_abs);
    for (file, paths) in uses {
        all.entry(file.clone())
            .or_default()
            .extend(paths.iter().cloned());
    }
    let path = uses_path(project_abs);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let sorted: std::collections::BTreeMap<_, _> = all.into_iter().collect();
    std::fs::write(&path, serde_json::to_string_pretty(&sorted)?)?;
    Ok(())
}

impl Graph {
    /// Reads the project's scripts, scenes and resources.
    pub fn build(project_abs: &Path) -> Graph {
        let mut sources: HashMap<String, FileDeps> = HashMap::new();
        let mut uids: HashMap<String, String> = HashMap::new();

        let walker = WalkDir::new(project_abs)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'));
        for entry in walker.flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(project_abs) else {
                continue;
            };
            let res = format!("res://{}", rel.to_string_lossy().replace('\\', "/"));
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            match ext {
                "gd" => {
                    if let Ok(text) = std::fs::read_to_string(path) {
                        sources.insert(res.clone(), parse_script(&text, &res));
                    }
                }
                "tscn" | "tres" => {
                    if let Ok(text) = std::fs::read_to_string(path) {
                        if let Some(uid) = header_uid(&text) {
                            uids.insert(uid, res.clone());
                        }
                        sources.insert(res.clone(), parse_text_resource(&text));
                    }
                }
                // `a.gd.uid` holds the uid of `a.gd`.
                "uid" => {
                    if let Ok(text) = std::fs::read_to_string(path) {
                        let uid = text.trim();
                        if uid.starts_with("uid://") {
                            uids.insert(uid.to_string(), res.trim_end_matches(".uid").to_string());
                        }
                    }
                }
                // `icon.png.import` holds the uid of `icon.png`.
                "import" => {
                    if let Ok(text) = std::fs::read_to_string(path)
                        && let Some(uid) = header_uid(&text)
                    {
                        uids.insert(uid, res.trim_end_matches(".import").to_string());
                    }
                }
                _ => {}
            }
        }

        let mut graph = Graph::default();
        for (test, paths) in read_uses(project_abs) {
            graph.seen.insert(test.clone());
            for path in paths {
                if path != test {
                    graph
                        .reverse
                        .entry(path.clone())
                        .or_default()
                        .insert(test.clone());
                    graph.forward.entry(test.clone()).or_default().insert(path);
                }
            }
        }
        let autoloads = read_autoloads(project_abs);
        graph.global.insert(
            "res://project.godot".to_string(),
            "project.godot changed".to_string(),
        );
        for config in ["res://gdtest.toml", "res://.gdtest.toml"] {
            graph
                .global
                .insert(config.to_string(), format!("{} changed", &config[6..]));
        }
        graph.global_folders.push((
            "res://addons/gdtest/".to_string(),
            "the gdtest addon changed".to_string(),
        ));
        for (name, path) in &autoloads {
            graph
                .global
                .insert(path.clone(), format!("the autoload {name} changed"));
        }

        let classes: HashMap<&str, &str> = sources
            .iter()
            .filter_map(|(file, deps)| deps.class_name.as_deref().map(|c| (c, file.as_str())))
            .collect();

        for (file, deps) in &sources {
            let mut uses: HashSet<String> = HashSet::new();
            for path in &deps.paths {
                if let Some(target) = uids.get(path) {
                    uses.insert(target.clone());
                } else if path.starts_with("res://") {
                    uses.insert(path.clone());
                }
            }
            // A folder it loads from: it may load any file there.
            for folder in &deps.folders {
                uses.extend(sources.keys().filter(|f| in_folder(f, folder)).cloned());
            }
            for name in &deps.names {
                if let Some(target) = classes.get(name.as_str()) {
                    uses.insert((*target).to_string());
                }
                if let Some(target) = autoloads.get(name) {
                    uses.insert(target.clone());
                }
            }
            uses.remove(file);
            for used in &uses {
                graph
                    .reverse
                    .entry(used.clone())
                    .or_default()
                    .insert(file.clone());
            }
            graph.forward.entry(file.clone()).or_default().extend(uses);
            if !deps.folders.is_empty() {
                graph.folders.insert(file.clone(), deps.folders.clone());
            }
            if deps.loads_unknown {
                graph.unknown.insert(file.clone());
            }
        }
        graph
    }

    /// The test files among `tests` that a change to `changed` (res://
    /// paths) affects.
    pub fn impact(&self, changed: &[String], tests: &[String]) -> Impact {
        for file in changed {
            if let Some(reason) = self.global.get(file) {
                return Impact::All {
                    reason: reason.clone(),
                };
            }
            for (folder, reason) in &self.global_folders {
                if file.starts_with(folder.as_str()) {
                    return Impact::All {
                        reason: reason.clone(),
                    };
                }
            }
        }

        // The changed files, and the files that load from their folders.
        let mut start: HashSet<String> = changed.iter().cloned().collect();
        for (file, folders) in &self.folders {
            if changed
                .iter()
                .any(|c| folders.iter().any(|f| in_folder(c, f)))
            {
                start.insert(file.clone());
            }
        }
        let reached = dependents(&start, &self.reverse);

        let mut affected = BTreeSet::new();
        for test in tests {
            if start.contains(test) || reached.contains(test) || self.loads_unknown(test) {
                affected.insert(test.clone());
            }
        }
        Impact::Files(affected)
    }

    /// Whether `file`, or a file it uses, loads a path the source does not
    /// tell: then any change may affect it.
    pub fn loads_unknown(&self, file: &str) -> bool {
        // What it loads was seen when it ran.
        if self.unknown.is_empty() || self.seen.contains(file) {
            return false;
        }
        let mut seen: HashSet<&str> = HashSet::new();
        let mut stack = vec![file];
        while let Some(current) = stack.pop() {
            if !seen.insert(current) {
                continue;
            }
            // The test runner loads whatever it runs; a change to it
            // already affects every test.
            if self
                .global_folders
                .iter()
                .any(|(folder, _)| current.starts_with(folder.as_str()))
            {
                continue;
            }
            if self.unknown.contains(current) {
                return true;
            }
            if let Some(uses) = self.forward.get(current) {
                stack.extend(uses.iter().map(String::as_str));
            }
        }
        false
    }

    /// The files that load paths the source does not tell, among the files
    /// `tests` reach.
    pub fn unknown_loaders(&self, tests: &[String]) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        let mut seen: HashSet<&str> = HashSet::new();
        let mut stack: Vec<&str> = tests.iter().map(String::as_str).collect();
        while let Some(current) = stack.pop() {
            if !seen.insert(current)
                || self
                    .global_folders
                    .iter()
                    .any(|(folder, _)| current.starts_with(folder.as_str()))
            {
                continue;
            }
            if self.unknown.contains(current) {
                found.insert(current.to_string());
            }
            if let Some(uses) = self.forward.get(current) {
                stack.extend(uses.iter().map(String::as_str));
            }
        }
        found
    }
}

/// Whether `file` is in `folder`: `res://actors/`, or `res://actors/*.tscn`
/// for the scenes of that folder and the folders in it.
fn in_folder(file: &str, folder: &str) -> bool {
    match folder.rsplit_once("/*") {
        Some((dir, suffix)) => file.starts_with(dir) && file.ends_with(suffix),
        None => file.starts_with(folder),
    }
}

/// Every file that uses one of `start`, directly or through other files.
fn dependents(start: &HashSet<String>, reverse: &DepGraph) -> HashSet<String> {
    let mut reached: HashSet<String> = HashSet::new();
    let mut frontier: Vec<&String> = start.iter().collect();
    while let Some(file) = frontier.pop() {
        if let Some(users) = reverse.get(file) {
            for user in users {
                if reached.insert(user.clone()) {
                    frontier.push(user);
                }
            }
        }
    }
    reached
}

/// Functions whose first argument is a resource to load. Like Vitest, which
/// follows imports and not `fs.readFile`, files opened with FileAccess or
/// DirAccess are not followed, unless a `"res://…"` string names them: they
/// are mostly saves, replays and logs, read and written while running.
const LOADERS: &[&str] = &["load", "load_threaded_request", "change_scene_to_file"];

/// `# gdtest: loads res://actors/`: a script that loads computed paths says
/// where from, so that it depends on that folder instead of on everything.
/// `res://actors/*.tscn` narrows it to the scenes of that folder.
const LOADS_DIRECTIVE: &str = "gdtest: loads ";

/// Reads what a script uses.
pub fn parse_script(text: &str, res: &str) -> FileDeps {
    let lexed = gdsyntax::lexer::tokenize(text);
    let mut tokens: Vec<(SyntaxKind, &str)> = Vec::new();
    let mut declared_folders: BTreeSet<String> = BTreeSet::new();
    let mut offset = 0usize;
    for token in &lexed.tokens {
        let end = offset + token.len as usize;
        let token_text = &text[offset..end];
        if !token.kind.is_trivia() {
            tokens.push((token.kind, token_text));
        } else if token.kind == SyntaxKind::COMMENT
            && let Some(at) = token_text.find(LOADS_DIRECTIVE)
        {
            for folder in token_text[at + LOADS_DIRECTIVE.len()..].split([' ', ',']) {
                if folder.starts_with("res://") {
                    declared_folders.insert(folder.to_string());
                }
            }
        }
        offset = end;
    }

    let dir = res.rsplit_once('/').map(|(d, _)| d).unwrap_or("res:/");
    let constants = literal_names(&tokens);
    let mut deps = FileDeps::default();
    for (i, &(kind, token_text)) in tokens.iter().enumerate() {
        match kind {
            SyntaxKind::STRING => {
                if let Some(value) = gdsyntax::literal::string_value(token_text)
                    && (value.starts_with("res://") || value.starts_with("uid://"))
                {
                    deps.paths.insert(value);
                }
            }
            SyntaxKind::IDENTIFIER => {
                deps.names.insert(token_text.to_string());
            }
            SyntaxKind::CLASS_NAME_KW => {
                if let Some(&(SyntaxKind::IDENTIFIER, name)) = tokens.get(i + 1) {
                    deps.class_name = Some(name.to_string());
                }
            }
            _ => {}
        }
        // A call that loads a path: `preload(...)`, `load(...)`,
        // `ResourceLoader.load(...)`, `FileAccess.open(...)`.
        let is_loader = match kind {
            SyntaxKind::PRELOAD_KW => true,
            SyntaxKind::IDENTIFIER => LOADERS.contains(&token_text) && is_loader_call(&tokens, i),
            _ => false,
        };
        if is_loader && matches!(tokens.get(i + 1), Some((SyntaxKind::PARENTHESIS_OPEN, _))) {
            read_load_argument(&tokens, i + 2, dir, &constants, &mut deps);
        }
    }
    if !declared_folders.is_empty() {
        deps.loads_unknown = false;
        deps.folders.extend(declared_folders);
    }
    // `extends "a.gd"` is relative to the script.
    for (i, &(kind, _)) in tokens.iter().enumerate() {
        if kind == SyntaxKind::EXTENDS_KW
            && let Some(&(SyntaxKind::STRING, text)) = tokens.get(i + 1)
            && let Some(value) = gdsyntax::literal::string_value(text)
            && !value.contains("://")
        {
            deps.paths.insert(join_relative(dir, &value));
        }
    }
    deps
}

/// Whether the loader name at `i` is a call this analysis follows: `load`
/// on its own or on ResourceLoader, `open` on FileAccess or DirAccess, and
/// the other names on anything. `config.load(path)` is not one: it reads a
/// ConfigFile, usually from `user://`.
fn is_loader_call(tokens: &[(SyntaxKind, &str)], i: usize) -> bool {
    let name = tokens[i].1;
    let receiver = if i >= 2 && tokens[i - 1].0 == SyntaxKind::PERIOD {
        Some(tokens[i - 2].1)
    } else {
        None
    };
    match name {
        "load" | "load_threaded_request" => matches!(receiver, None | Some("ResourceLoader")),
        _ => true,
    }
}

/// Names given a string literal once, and never assigned again:
/// `const PATH := "res://a.json"`, `var path = "user://save"`.
fn literal_names(tokens: &[(SyntaxKind, &str)]) -> HashMap<String, String> {
    let mut values: HashMap<String, String> = HashMap::new();
    let mut assigned: HashMap<&str, usize> = HashMap::new();
    for (i, &(kind, text)) in tokens.iter().enumerate() {
        let assigns = matches!(tokens.get(i + 1).map(|t| t.1), Some("=") | Some("+="));
        let declared =
            i > 0 && matches!(tokens[i - 1].0, SyntaxKind::CONST_KW | SyntaxKind::VAR_KW);
        if kind == SyntaxKind::IDENTIFIER && assigns && !declared {
            *assigned.entry(text).or_default() += 1;
        }
        let declares = matches!(kind, SyntaxKind::CONST_KW | SyntaxKind::VAR_KW);
        if !declares {
            continue;
        }
        let Some(&(SyntaxKind::IDENTIFIER, name)) = tokens.get(i + 1) else {
            continue;
        };
        // `const A = "..."`, `const A := "..."`, `const A: String = "..."`.
        let mut at = i + 2;
        if tokens.get(at).map(|t| t.1) == Some(":") {
            at += 1;
            if tokens.get(at).map(|t| t.1) != Some("=") {
                at += 1;
            }
        }
        if tokens.get(at).map(|t| t.1) != Some("=") {
            continue;
        }
        if let Some(&(SyntaxKind::STRING, literal)) = tokens.get(at + 1)
            && matches!(
                tokens.get(at + 2).map(|t| t.0),
                Some(SyntaxKind::NEWLINE) | None
            )
            && let Some(value) = gdsyntax::literal::string_value(literal)
        {
            values.insert(name.to_string(), value);
        }
    }
    // A name assigned again later may hold anything.
    values.retain(|name, _| !assigned.contains_key(name.as_str()));
    values
}

/// Reads the path a loader call gets, from the token at `at`.
fn read_load_argument(
    tokens: &[(SyntaxKind, &str)],
    at: usize,
    dir: &str,
    constants: &HashMap<String, String>,
    deps: &mut FileDeps,
) {
    let Some(&(kind, text)) = tokens.get(at) else {
        return;
    };
    let next = tokens.get(at + 1).map(|t| t.0);
    // `FileAccess.open(path, FileAccess.WRITE)` writes: it reads nothing.
    if next == Some(SyntaxKind::COMMA) && matches!(tokens.get(at + 4).map(|t| t.1), Some("WRITE")) {
        return;
    }
    let literal = match kind {
        SyntaxKind::STRING => gdsyntax::literal::string_value(text),
        SyntaxKind::IDENTIFIER => constants.get(text).cloned(),
        _ => None,
    };
    if let Some(value) = literal {
        if value.starts_with("user://") {
            return;
        }
        let path = if value.contains("://") {
            value
        } else {
            join_relative(dir, &value)
        };
        match next {
            Some(SyntaxKind::PARENTHESIS_CLOSE) | Some(SyntaxKind::COMMA) => {
                deps.paths.insert(path);
            }
            // `"res://items/" + id` or `"res://items/%s.tres" % id`.
            _ if path.starts_with("res://") => {
                let folder = match path.rfind('/') {
                    Some(slash) if slash >= "res://".len() => &path[..=slash],
                    _ => "res://",
                };
                deps.folders.insert(folder.to_string());
            }
            _ => deps.loads_unknown = true,
        }
        return;
    }
    deps.loads_unknown = true;
}

/// `res://a/b` and `../c.gd` give `res://a/c.gd`.
fn join_relative(dir: &str, relative: &str) -> String {
    let mut parts: Vec<&str> = dir.trim_start_matches("res://").split('/').collect();
    parts.retain(|p| !p.is_empty());
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    format!("res://{}", parts.join("/"))
}

/// Reads what a scene or text resource uses: its `ext_resource`s.
pub fn parse_text_resource(text: &str) -> FileDeps {
    let mut deps = FileDeps::default();
    for line in text.lines() {
        let line = line.trim_start();
        if !line.starts_with("[ext_resource") {
            continue;
        }
        if let Some(path) = attribute(line, "path") {
            deps.paths.insert(path.to_string());
        } else if let Some(uid) = attribute(line, "uid") {
            deps.paths.insert(uid.to_string());
        }
    }
    deps
}

/// The `uid="uid://…"` of a file's first `[…]` header line.
fn header_uid(text: &str) -> Option<String> {
    let header = text.lines().find(|l| l.trim_start().starts_with('['))?;
    attribute(header, "uid")
        .filter(|u| u.starts_with("uid://"))
        .map(str::to_string)
        .or_else(|| {
            // An `.import` file has its uid on a line of its own.
            text.lines()
                .find_map(|l| l.trim().strip_prefix("uid=\""))
                .and_then(|rest| rest.split('"').next())
                .filter(|u| u.starts_with("uid://"))
                .map(str::to_string)
        })
}

/// The value of `name="…"` in a header line.
fn attribute<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = line.find(&key)? + key.len();
    let end = line[start..].find('"')? + start;
    Some(&line[start..end])
}

/// The project's autoloads: name → res:// path.
fn read_autoloads(project_abs: &Path) -> HashMap<String, String> {
    let mut autoloads = HashMap::new();
    let Ok(text) = std::fs::read_to_string(project_abs.join("project.godot")) else {
        return autoloads;
    };
    let mut in_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == "[autoload]";
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((name, value)) = line.split_once('=') {
            let path = value.trim().trim_matches('"').trim_start_matches('*');
            if path.starts_with("res://") {
                autoloads.insert(name.trim().to_string(), path.to_string());
            }
        }
    }
    autoloads
}

/// The `res://` paths a script names in `extends`, `preload` and `load`.
/// Kept for callers of the first version; [`parse_script`] reads more.
pub fn parse_file_deps(source: &[u8]) -> Vec<String> {
    let Ok(text) = std::str::from_utf8(source) else {
        return Vec::new();
    };
    parse_script(text, "res://")
        .paths
        .into_iter()
        .filter(|p| p.starts_with("res://"))
        .collect()
}

/// Forward and reverse graphs of the project. See [`Graph::build`].
pub fn build_dep_graph(project_abs: &Path) -> (DepGraph, DepGraph) {
    let graph = Graph::build(project_abs);
    (graph.forward, graph.reverse)
}

/// Every file that uses one of `changed`, directly or not, without the
/// changed files themselves.
pub fn affected_files(changed: &HashSet<String>, reverse: &DepGraph) -> HashSet<String> {
    let mut reached = dependents(changed, reverse);
    reached.retain(|f| !changed.contains(f));
    reached
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[(&str, &str)]) -> tempfile_dir::Dir {
        let dir = tempfile_dir::Dir::new();
        for (path, text) in files {
            let full = dir.path().join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }
        dir
    }

    fn files(impact: Impact) -> Vec<String> {
        match impact {
            Impact::Files(files) => files.into_iter().collect(),
            Impact::All { reason } => panic!("expected some files, got all: {reason}"),
        }
    }

    fn s(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

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

    #[test]
    fn reads_names_strings_and_loads() {
        let deps = parse_script(
            r#"class_name Player extends Actor
const Sword = preload("weapons/sword.gd")
var data = FileAccess.open("res://data/stats.json", FileAccess.READ)
var save = FileAccess.open(save_path, FileAccess.READ)  # a save: not followed
var cfg = ConfigFile.new()
func _ready():
    cfg.load(some_path)  # a ConfigFile: not followed
    var item = load("res://items/%s.tres" % id)
    var scene = "res://levels/one.tscn"
    var any = load(path)
"#,
            "res://actors/player.gd",
        );
        assert_eq!(deps.class_name.as_deref(), Some("Player"));
        assert!(deps.names.contains("Actor"));
        assert!(deps.paths.contains("res://actors/weapons/sword.gd"));
        assert!(deps.paths.contains("res://data/stats.json"));
        assert!(deps.paths.contains("res://levels/one.tscn"));
        assert!(deps.folders.contains("res://items/"));
        assert!(deps.loads_unknown);
    }

    #[test]
    fn constants_and_writes() {
        let deps = parse_script(
            "const DOC := \"res://docs/gate.tscn\"\nvar out = \"user://x.tres\"\nfunc f():\n    load(DOC)\n    load(out)\n",
            "res://f.gd",
        );
        assert!(deps.paths.contains("res://docs/gate.tscn"));
        assert!(!deps.loads_unknown);
        let reassigned = parse_script(
            "var p := \"res://a.tres\"\nfunc f():\n    p = other()\n    load(p)\n",
            "res://f.gd",
        );
        assert!(reassigned.loads_unknown);
    }

    #[test]
    fn a_literal_load_is_not_unknown() {
        let deps = parse_script(
            "func f():\n    var a = load(\"res://a.gd\")\n    var c = ConfigFile.new()\n    c.load(p)\n",
            "res://f.gd",
        );
        assert!(!deps.loads_unknown);
    }

    #[test]
    fn follows_class_names_scenes_and_uids() {
        let dir = project(&[
            ("project.godot", "[application]\n"),
            ("src/gold.gd", "class_name Gold\nvar amount := 0\n"),
            ("src/shop.gd", "class_name Shop\nvar gold := Gold.new()\n"),
            ("src/enemy.gd", "extends Node\n"),
            ("src/enemy.gd.uid", "uid://enemy123\n"),
            (
                "scenes/enemy.tscn",
                "[gd_scene format=3 uid=\"uid://scene1\"]\n\n[ext_resource type=\"Script\" uid=\"uid://enemy123\" id=\"1\"]\n",
            ),
            ("src/unrelated.gd", "class_name Unrelated\n"),
            (
                "test/test_shop.gd",
                "extends GdtTest\nfunc test_buy():\n    Shop.new()\n",
            ),
            (
                "test/test_enemy.gd",
                "extends GdtTest\nconst S = preload(\"uid://scene1\")\nfunc test_spawn():\n    pass\n",
            ),
        ]);
        let graph = Graph::build(dir.path());
        let tests = s(&["res://test/test_shop.gd", "res://test/test_enemy.gd"]);

        let gold = graph.impact(&s(&["res://src/gold.gd"]), &tests);
        assert_eq!(files(gold), s(&["res://test/test_shop.gd"]));

        let enemy = graph.impact(&s(&["res://src/enemy.gd"]), &tests);
        assert_eq!(files(enemy), s(&["res://test/test_enemy.gd"]));

        let unrelated = graph.impact(&s(&["res://src/unrelated.gd"]), &tests);
        assert!(files(unrelated).is_empty());

        let itself = graph.impact(&s(&["res://test/test_shop.gd"]), &tests);
        assert_eq!(files(itself), s(&["res://test/test_shop.gd"]));
    }

    #[test]
    fn project_settings_and_autoloads_affect_every_test() {
        let dir = project(&[
            (
                "project.godot",
                "[application]\n\n[autoload]\n\nClock=\"*res://core/clock.gd\"\n",
            ),
            ("core/clock.gd", "extends Node\nvar time := 0\n"),
            ("src/timer.gd", "func now():\n    return Clock.time\n"),
            ("test/test_a.gd", "extends GdtTest\n"),
        ]);
        let graph = Graph::build(dir.path());
        let tests = s(&["res://test/test_a.gd"]);
        assert!(matches!(
            graph.impact(&s(&["res://project.godot"]), &tests),
            Impact::All { .. }
        ));
        assert!(matches!(
            graph.impact(&s(&["res://core/clock.gd"]), &tests),
            Impact::All { .. }
        ));
        // A script that uses the autoload by name depends on it.
        assert!(graph.forward["res://src/timer.gd"].contains("res://core/clock.gd"));
    }

    #[test]
    fn unknown_and_folder_loads() {
        let dir = project(&[
            ("project.godot", ""),
            (
                "items/sword.tres",
                "[gd_resource type=\"Resource\" format=3]\n",
            ),
            (
                "src/items.gd",
                "class_name Items\nfunc get(id):\n    return load(\"res://items/\" + id)\n",
            ),
            (
                "src/loader.gd",
                "class_name Loader\nfunc get(path):\n    return load(path)\n",
            ),
            ("src/other.gd", "class_name Other\n"),
            (
                "test/test_items.gd",
                "extends GdtTest\nfunc test_a():\n    Items.new()\n",
            ),
            (
                "test/test_loader.gd",
                "extends GdtTest\nfunc test_b():\n    Loader.new()\n",
            ),
            ("test/test_plain.gd", "extends GdtTest\n"),
        ]);
        let graph = Graph::build(dir.path());
        let tests = s(&[
            "res://test/test_items.gd",
            "res://test/test_loader.gd",
            "res://test/test_plain.gd",
        ]);
        // A file in the folder Items loads from affects its tests; the
        // tests of a script that loads unknown paths run on any change.
        let sword = graph.impact(&s(&["res://items/sword.tres"]), &tests);
        assert_eq!(
            files(sword),
            s(&["res://test/test_items.gd", "res://test/test_loader.gd"])
        );
        let other = graph.impact(&s(&["res://src/other.gd"]), &tests);
        assert_eq!(files(other), s(&["res://test/test_loader.gd"]));
    }

    #[test]
    fn a_directive_says_where_computed_loads_come_from() {
        let dir = project(&[
            ("project.godot", ""),
            (
                "actors/minion.tscn",
                "[gd_scene format=3]\n\n[ext_resource type=\"Script\" path=\"res://combat/hit.gd\" id=\"1\"]\n",
            ),
            ("combat/hit.gd", "extends Node\n"),
            ("ui/menu.gd", "class_name Menu\n"),
            (
                "net/restore.gd",
                "class_name Restore\n# gdtest: loads res://actors/\nfunc f(path):\n    return load(path)\n",
            ),
            (
                "test/test_restore.gd",
                "extends GdtTest\nfunc test_a():\n    Restore.new()\n",
            ),
        ]);
        let graph = Graph::build(dir.path());
        let tests = s(&["res://test/test_restore.gd"]);
        // Through the scene it may load, it uses that scene's script.
        let hit = graph.impact(&s(&["res://combat/hit.gd"]), &tests);
        assert_eq!(files(hit), tests);
        let menu = graph.impact(&s(&["res://ui/menu.gd"]), &tests);
        assert!(files(menu).is_empty());
        assert!(in_folder("res://actors/a/b.tscn", "res://actors/*.tscn"));
        assert!(!in_folder("res://actors/a/b.gd", "res://actors/*.tscn"));
    }

    #[test]
    fn what_a_test_was_seen_using_counts() {
        let dir = project(&[
            ("project.godot", ""),
            (
                "actors/minion.tscn",
                "[gd_scene format=3]\n\n[ext_resource type=\"Script\" path=\"res://actors/minion.gd\" id=\"1\"]\n",
            ),
            ("actors/minion.gd", "extends Node\n"),
            ("actors/tower.gd", "extends Node\n"),
            (
                "net/restore.gd",
                "class_name Restore\nfunc f(path):\n    return load(path)\n",
            ),
            (
                "test/test_restore.gd",
                "extends GdtTest\nfunc test_a():\n    Restore.new()\n",
            ),
        ]);
        let tests = s(&["res://test/test_restore.gd"]);
        // Never run: it may load anything, so any change runs it.
        let graph = Graph::build(dir.path());
        assert_eq!(
            files(graph.impact(&s(&["res://actors/tower.gd"]), &tests)),
            tests
        );

        let seen: HashMap<String, BTreeSet<String>> = [(
            "res://test/test_restore.gd".to_string(),
            ["res://actors/minion.tscn".to_string()].into(),
        )]
        .into();
        record_uses(dir.path(), &seen).unwrap();
        let graph = Graph::build(dir.path());
        // Seen loading the minion scene: the scene and its script affect it.
        assert_eq!(
            files(graph.impact(&s(&["res://actors/minion.gd"]), &tests)),
            tests
        );
        assert_eq!(
            files(graph.impact(&s(&["res://actors/minion.tscn"]), &tests)),
            tests
        );
        assert!(files(graph.impact(&s(&["res://actors/tower.gd"]), &tests)).is_empty());
        // Its own source still counts.
        assert_eq!(
            files(graph.impact(&s(&["res://net/restore.gd"]), &tests)),
            tests
        );
    }

    #[test]
    fn the_gdtest_addon_affects_every_test() {
        let dir = project(&[
            ("project.godot", ""),
            ("test/test_a.gd", "extends GdtTest\n"),
        ]);
        let graph = Graph::build(dir.path());
        assert!(matches!(
            graph.impact(
                &s(&["res://addons/gdtest/gdt.gd"]),
                &s(&["res://test/test_a.gd"])
            ),
            Impact::All { .. }
        ));
    }

    /// A temporary folder, removed when dropped. The crate has no tempfile
    /// dependency, and the tests need little.
    mod tempfile_dir {
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicUsize, Ordering};

        static NEXT: AtomicUsize = AtomicUsize::new(0);

        pub struct Dir(PathBuf);

        impl Dir {
            pub fn new() -> Dir {
                let n = NEXT.fetch_add(1, Ordering::SeqCst);
                let path =
                    std::env::temp_dir().join(format!("gdtest-deps-{}-{}", std::process::id(), n));
                let _ = std::fs::remove_dir_all(&path);
                std::fs::create_dir_all(&path).unwrap();
                Dir(path)
            }

            pub fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
