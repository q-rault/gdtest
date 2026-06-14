//! `--watch` mode: re-run on `.gd` file changes (internal).
//!
//! Selective rerun: when every changed path resolves to a known test
//! file in the current target set, the rerun is narrowed to just those
//! files (typical TDD case: edit one test, only that test re-runs). Any
//! change to an unknown / non-test `.gd` file falls back to a full
//! rerun — conservative because tests may `preload`/`load` it. A future
//! pass (internal v1) will grep for `preload`/`load` references and only
//! invalidate the affected tests.
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
    ev.paths
        .iter()
        .any(|p| p.extension().and_then(|e| e.to_str()) == Some("gd"))
}

fn collect_gd_paths(ev: &notify::Result<notify::Event>) -> Vec<PathBuf> {
    let Ok(ev) = ev else { return Vec::new() };
    ev.paths
        .iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("gd"))
        .cloned()
        .collect()
}

/// Convert a list of changed absolute paths into a narrowed set of
/// test-file `res://…` paths, IF every change matches a known test
/// file. Returns `None` for "fall back to the full set" — either no
/// changes (initial run) or at least one change is to a non-test or
/// unknown file (could be a dependency we can't statically prove safe
/// to skip).
fn narrow_targets(
    changed: &[PathBuf],
    project: &Path,
    resolved_files: &[String],
) -> Option<HashSet<String>> {
    if changed.is_empty() {
        return None;
    }
    let project_abs = project.canonicalize().ok()?;
    let known: HashSet<&str> = resolved_files.iter().map(|s| s.as_str()).collect();
    let mut touched = HashSet::new();
    for p in changed {
        let abs = match p.canonicalize() {
            Ok(a) => a,
            Err(_) => return None, // path no longer exists (e.g. deleted) — full rerun
        };
        let res = match gdtest_analysis::to_res_path(&abs, &project_abs) {
            Ok(r) => r,
            Err(_) => return None, // outside the project — full rerun
        };
        if !known.contains(res.as_str()) {
            return None; // unknown / non-test changed → full rerun
        }
        touched.insert(res);
    }
    Some(touched)
}

async fn run_iteration(opts: &WatchOptions, changed: &[PathBuf]) -> Result<()> {
    let narrow = narrow_targets(changed, &opts.project_path, &opts.resolved_files);

    if changed.is_empty() {
        // First run — no narrowing message.
    } else {
        match &narrow {
            Some(files) => {
                eprintln!(
                    "\ngdtest: change detected — rerunning {} test file(s)",
                    files.len(),
                );
            }
            None => {
                eprintln!("\ngdtest: change detected — non-test file or unknown path, full rerun",);
            }
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
