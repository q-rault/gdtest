//! `--watch` mode: re-run on `.gd` file changes.
//!
//! Selective rerun: each change reruns only the test files it can affect,
//! as the project's dependency graph tells (see `gdtest_analysis::deps`):
//! a test file reruns itself, a script reruns the tests that use it,
//! directly or through other scripts, scenes and resources.
//!
//! Ctrl-C: the loop selects on `tokio::signal::ctrl_c()` alongside the
//! file-event channel. First SIGINT exits cleanly within ~1s, even
//! mid-iteration (the pool spawned per iteration has its own SIGINT
//! handler that's aborted at iteration end so we don't accumulate).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::pool::{self, IsolationMode, TestItem};
use crate::reporter::{self, Reporter};
use crate::worker::Config;

const DEBOUNCE: Duration = Duration::from_millis(200);

pub struct WatchOptions {
    pub project_path: PathBuf,
    pub jobs: usize,
    pub reporter_kind: reporter::ReporterKind,
    pub isolation: IsolationMode,
    pub cfg: Config,
    pub filter: Option<String>,
    pub include_tags: Vec<String>,
    pub exclude_tags: Vec<String>,
    pub explicit_targets: Vec<String>,
    /// Pre-resolved file targets (post --filter, pre per-test discovery).
    /// We re-discover on each iteration so newly-added tests show up.
    pub resolved_files: Vec<String>,
    pub explicit_tests: Vec<TestItem>,
    pub timeout_ms: u64,
    pub fail_fast: bool,
    pub timeout_warn_ms: Option<u64>,
    pub retries: u32,
}

#[tokio::main(flavor = "multi_thread")]
pub async fn run(opts: WatchOptions) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<notify::Result<notify::Event>>();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |ev| {
        let _ = tx.send(ev);
    })?;
    watcher.watch(&opts.project_path, RecursiveMode::Recursive)?;

    eprintln!(
        "gdtest: watching {} for *.gd changes (Ctrl-C to exit)",
        opts.project_path.display(),
    );

    // Initial run with no narrowing.
    run_iteration(&opts, &[]).await?;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\ngdtest: SIGINT, exiting watch");
                return Ok(());
            }
            maybe_ev = rx.recv() => {
                let Some(ev) = maybe_ev else { return Ok(()) };
                if !is_relevant(&ev) { continue; }
                let mut changed: Vec<PathBuf> = collect_gd_paths(&ev);
                // Debounce: keep draining the channel for a fixed window
                // and accumulate changed paths so the next iteration
                // sees the full burst.
                let deadline = Instant::now() + DEBOUNCE;
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline) => break,
                        more = rx.recv() => {
                            match more {
                                Some(more_ev) if is_relevant(&more_ev) => {
                                    changed.extend(collect_gd_paths(&more_ev));
                                }
                                Some(_) => continue,
                                None => return Ok(()),
                            }
                        }
                    }
                }
                let _ = run_iteration(&opts, &changed).await;
            }
        }
    }
}

fn is_relevant(ev: &notify::Result<notify::Event>) -> bool {
    let Ok(ev) = ev else { return false };
    if !matches!(
        ev.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) {
        return false;
    }
    ev.paths.iter().any(|p| is_project_file(p))
}

fn collect_gd_paths(ev: &notify::Result<notify::Event>) -> Vec<PathBuf> {
    let Ok(ev) = ev else { return Vec::new() };
    ev.paths
        .iter()
        .filter(|p| is_project_file(p))
        .cloned()
        .collect()
}

/// A file a test may depend on: not in a hidden folder such as `.godot/`,
/// and not one Godot writes beside a file (`.import`, `.uid`).
fn is_project_file(path: &Path) -> bool {
    let hidden = path.components().any(|c| {
        let name = c.as_os_str().to_string_lossy();
        name.starts_with('.') && name != "." && name != ".."
    });
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    !hidden && !matches!(ext, "import" | "uid" | "tmp" | "")
}

/// The test files `changed` (absolute paths) can affect, or `None` to run
/// every test: on the first run, or when the change affects every test.
fn narrow_targets(
    changed: &[PathBuf],
    project: &Path,
    resolved_files: &[String],
) -> Option<HashSet<String>> {
    if changed.is_empty() {
        return None;
    }
    let project_abs = project.canonicalize().ok()?;
    let mut res_paths = Vec::new();
    for p in changed {
        // A deleted file no longer resolves: build its path by hand.
        let abs = p.canonicalize().unwrap_or_else(|_| p.clone());
        let rel = abs
            .strip_prefix(&project_abs)
            .or_else(|_| abs.strip_prefix(project))
            .ok()?;
        res_paths.push(format!(
            "res://{}",
            rel.to_string_lossy().replace('\\', "/")
        ));
    }
    let graph = gdtest_analysis::deps::Graph::build(&project_abs);
    match graph.impact(&res_paths, resolved_files) {
        gdtest_analysis::deps::Impact::All { reason } => {
            eprintln!("\ngdtest: {reason}: rerunning every test");
            None
        }
        gdtest_analysis::deps::Impact::Files(files) => Some(files.into_iter().collect()),
    }
}

async fn run_iteration(opts: &WatchOptions, changed: &[PathBuf]) -> Result<()> {
    let narrow = narrow_targets(changed, &opts.project_path, &opts.resolved_files);

    // The first run has no change to report.
    if !changed.is_empty()
        && let Some(files) = &narrow
    {
        eprintln!(
            "\ngdtest: change detected — rerunning {} test file(s)",
            files.len(),
        );
        if files.is_empty() {
            return Ok(());
        }
    }

    // Re-discover so newly-added tests appear and removed ones disappear.
    let mut items: Vec<TestItem> = opts.explicit_tests.clone();
    for file in &opts.resolved_files {
        let abs = gdtest_analysis::res_to_abs(file, &opts.project_path)?;
        let mut entries = gdtest_analysis::list_test_entries_in(&abs).unwrap_or_default();
        if !opts.include_tags.is_empty() || !opts.exclude_tags.is_empty() {
            entries.retain(|e| {
                if !opts.include_tags.is_empty()
                    && !e.tags.iter().any(|t| opts.include_tags.contains(t))
                {
                    return false;
                }
                if !opts.exclude_tags.is_empty()
                    && e.tags.iter().any(|t| opts.exclude_tags.contains(t))
                {
                    return false;
                }
                true
            });
        }
        for e in entries {
            if e.kind == gdtest_analysis::TestKind::Normal {
                let item_isolation = match e.isolation {
                    Some(gdtest_analysis::IsolationDirective::File) => IsolationMode::File,
                    Some(gdtest_analysis::IsolationDirective::Test) => IsolationMode::Test,
                    None => opts.isolation,
                };
                items.push(TestItem::new(file.clone(), e.name, item_isolation));
            }
        }
    }
    if let Some(pat) = &opts.filter {
        let re = regex::Regex::new(pat)?;
        items.retain(|it| re.is_match(&format!("{} :: {}", it.display_script, it.func)));
    }
    if let Some(touched) = &narrow {
        items.retain(|it| touched.contains(&it.display_script));
    }
    if items.is_empty() {
        eprintln!("gdtest: no tests matched after re-discovery");
        return Ok(());
    }
    let jobs = opts.jobs.min(items.len().max(1)).max(1);
    let _ = pool::run(
        items,
        pool::PoolOptions {
            jobs,
            cfg: opts.cfg.clone(),
            reporter: Reporter::new(opts.reporter_kind),
            isolation: opts.isolation,
            timeout_ms: opts.timeout_ms,
            fail_fast: opts.fail_fast,
            timeout_warn_ms: opts.timeout_warn_ms,
            retries: opts.retries,
        },
    )
    .await?;
    let _ = &opts.explicit_targets; // reserved for future selective-rerun work
    Ok(())
}
