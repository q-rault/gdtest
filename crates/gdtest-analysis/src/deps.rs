//! Test impact analysis: which test files a change can affect.
//!
//! Which file uses which comes from [`gdsyntax::project`], which reads it
//! from the source. A test file is affected by a change when it reaches a
//! changed file through those links. What the source cannot tell is handled
//! cautiously:
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

pub use gdsyntax::project::{DepGraph, FileDeps, in_folder, parse_script, parse_text_resource};

/// Which test files a change affects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Impact {
    /// Every test: `reason` says why.
    All { reason: String },
    /// These test files, and no other.
    Files(BTreeSet<String>),
}

/// The project's dependency graph, with what the tests were seen using.
#[derive(Debug, Default)]
pub struct Graph {
    /// Which file uses which, from the source and from what the tests were
    /// seen using.
    pub project: gdsyntax::project::Graph,
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
    /// Reads the project's scripts, scenes and resources, and what its tests
    /// were seen using.
    pub fn build(project_abs: &Path) -> Graph {
        let mut graph = Graph {
            project: gdsyntax::project::Graph::build(project_abs),
            ..Graph::default()
        };
        for (test, paths) in read_uses(project_abs) {
            for path in paths {
                graph.project.add_use(&test, &path);
            }
            graph.seen.insert(test);
        }
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
        for (name, path) in graph.project.autoloads().clone() {
            graph
                .global
                .insert(path, format!("the autoload {name} changed"));
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
        let start = self.project.with_folder_loaders(changed);
        let reached = self.project.users_of(&start);

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
        if !self.project.has_unknown_loads() || self.seen.contains(file) {
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
            if self.in_global_folder(current) {
                continue;
            }
            if self.project.loads_unknown(current) {
                return true;
            }
            if let Some(uses) = self.project.forward.get(current) {
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
            if !seen.insert(current) || self.in_global_folder(current) {
                continue;
            }
            if self.project.loads_unknown(current) {
                found.insert(current.to_string());
            }
            if let Some(uses) = self.project.forward.get(current) {
                stack.extend(uses.iter().map(String::as_str));
            }
        }
        found
    }

    fn in_global_folder(&self, file: &str) -> bool {
        self.global_folders
            .iter()
            .any(|(folder, _)| file.starts_with(folder.as_str()))
    }
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
    (graph.project.forward, graph.project.reverse)
}

/// Every file that uses one of `changed`, directly or not, without the
/// changed files themselves.
pub fn affected_files(changed: &HashSet<String>, reverse: &DepGraph) -> HashSet<String> {
    let mut reached: HashSet<String> = HashSet::new();
    let mut frontier: Vec<&String> = changed.iter().collect();
    while let Some(file) = frontier.pop() {
        for user in reverse.get(file).into_iter().flatten() {
            if reached.insert(user.clone()) {
                frontier.push(user);
            }
        }
    }
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
        assert!(graph.project.forward["res://src/timer.gd"].contains("res://core/clock.gd"));
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
