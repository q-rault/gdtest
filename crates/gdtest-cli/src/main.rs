use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use clap::{Parser, Subcommand};

mod aggregate;
mod bench;
mod config;
mod coverage;
mod html_report;
mod migrate;
mod mutate_runner;
mod ping;
mod pool;
mod reporter;
mod run;
mod watch;
mod worker;

#[derive(Parser)]
#[command(name = "gdtest", version, about = "Fast test runner for Godot 4")]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    /// Test targets. Each argument is one of:
    ///   - `path/to/file.gd`                             → run every `test_*` in the file
    ///   - `path/to/file.gd::test_bar`                   → run exactly that one test
    ///   - `path/to/dir`                                 → walk for `test_*.gd` and run them all
    ///   - `res://…`                                     → same, with project-relative prefix
    targets: Vec<String>,

    /// Number of parallel workers. Defaults to the CPU count (clamped by test count).
    #[arg(short = 'j', long, env = "GDTEST_JOBS", global = true)]
    jobs: Option<usize>,

    /// Output format. [default: pretty] [env: GDTEST_REPORTER]
    #[arg(long, value_enum, env = "GDTEST_REPORTER", global = true)]
    reporter: Option<reporter::ReporterKind>,

    /// Isolation: `file` keeps the worker alive across tests (fast, default),
    /// `test` respawns the worker between every test (slow, ~200 ms × N
    /// extra, but cleans autoload state and `Engine.set_meta` between tests).
    /// [default: file] [env: GDTEST_ISOLATION]
    #[arg(long, value_enum, env = "GDTEST_ISOLATION", global = true)]
    isolation: Option<pool::IsolationMode>,

    /// Only run tests whose `file :: func` form matches the regex. (internal)
    #[arg(long)]
    filter: Option<String>,

    /// Only run tests that have this tag (repeatable; OR logic). [env: GDTEST_INCLUDE_TAGS]
    #[arg(long = "tag", value_delimiter = ',', env = "GDTEST_INCLUDE_TAGS")]
    tag: Vec<String>,

    /// Exclude tests that have this tag (repeatable). [env: GDTEST_EXCLUDE_TAGS]
    #[arg(
        long = "exclude-tag",
        value_delimiter = ',',
        env = "GDTEST_EXCLUDE_TAGS"
    )]
    exclude_tag: Vec<String>,

    /// List every discovered test as `file :: func` and exit without running.
    #[arg(long)]
    list: bool,

    /// Skip the auto `--import` cache check on startup. The auto-import
    /// runs `godot --headless --import` once when `.godot/global_script_class_cache.cfg`
    /// is missing, so `class_name Gdt` and the user's class_names resolve in
    /// the worker. Disable for CI runs where the cache is already populated
    /// or when invoking gdtest from a non-project directory.
    #[arg(long, global = true)]
    no_auto_import: bool,

    /// Per-test timeout in milliseconds. A test that produces no result
    /// within this window is reported as an error and its worker is respawned.
    /// [default: 5000] [env: GDTEST_TIMEOUT]
    #[arg(long, env = "GDTEST_TIMEOUT", global = true)]
    timeout: Option<u64>,

    /// Print a slow-test warning when a test exceeds this duration (ms) without
    /// killing it. Disabled by default. Must be less than --timeout.
    /// [env: GDTEST_TIMEOUT_WARN]
    #[arg(long, env = "GDTEST_TIMEOUT_WARN", global = true)]
    timeout_warn: Option<u64>,

    /// Stop after the first test failure or error. [env: GDTEST_FAIL_FAST]
    #[arg(long, env = "GDTEST_FAIL_FAST", global = true)]
    fail_fast: bool,

    /// Re-run a failing or erroring test up to N additional times before
    /// reporting the failure. Useful for quarantining flaky tests. `0`
    /// means no retries (default). Timeouts and crashes are not retried.
    /// [env: GDTEST_RETRIES]
    #[arg(long, env = "GDTEST_RETRIES", global = true)]
    retries: Option<u32>,

    /// gdtest.toml profile to activate (e.g. `--profile ci`).
    #[arg(long, global = true)]
    profile: Option<String>,

    /// Partition the test suite for CI sharding. Format: `count:N/M` or `hash:N/M`.
    /// `count:N/M` takes every M-th test starting at position N-1 (round-robin).
    /// `hash:N/M` uses a stable FNV hash so the same test always lands on the same shard.
    /// Example: `gdtest test/ --partition count:2/3` runs shard 2 of 3.
    #[arg(long, global = true)]
    partition: Option<String>,

    /// Accept new or changed snapshot values and write them to disk.
    /// Run once after intentional changes to bring stored snapshots in sync.
    #[arg(long)]
    update_snapshots: bool,

    /// Watch the project for `.gd` file changes and rerun the targets
    /// after every change (internal). Initial run + watch loop until SIGINT.
    #[arg(long)]
    watch: bool,

    /// Skip assert rewriting. By default, `assert(a == b)` calls are rewritten
    /// to `Gdt._rewrite_eq(a, b, ...)` for rich failure diffs (internal).
    /// Use this flag to run the original source without rewriting.
    #[arg(long)]
    no_rewrite: bool,

    /// Instrument source files for coverage and write `coverage/lcov.info` after the run.
    #[arg(long)]
    coverage: bool,

    /// Fail the run if total line coverage is below N%. Requires --coverage.
    #[arg(long, value_name = "PCT")]
    min_coverage: Option<f32>,

    /// Glob patterns to exclude from coverage instrumentation (repeatable).
    /// [env: GDTEST_COVERAGE_EXCLUDE]
    #[arg(
        long = "coverage-exclude",
        value_delimiter = ',',
        env = "GDTEST_COVERAGE_EXCLUDE"
    )]
    coverage_exclude: Vec<String>,

    /// Write a standalone HTML report to PATH after the run.
    /// Omit PATH to use the default (`gdtest-report.html`).
    #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = "gdtest-report.html")]
    html_report: Option<String>,

    /// Run only benchmark functions (named `bench_*`). Skips all `test_*` functions.
    #[arg(long)]
    bench: bool,

    /// Fix the RNG seed for forall() property tests, enabling reproduction of
    /// a specific failing case. The seed is printed on failure. [env: GDTEST_SEED]
    #[arg(long, env = "GDTEST_SEED")]
    seed: Option<u64>,

    /// Run only tests affected by uncommitted changes in the working tree
    /// (uses `git diff HEAD` to find modified files). Implies dependency graph build.
    #[arg(long, conflicts_with = "changed_since")]
    changed_only: bool,

    /// Run only tests affected by changes since <REF> (e.g. `origin/main`).
    /// Uses `git diff <REF> HEAD --name-only`.
    #[arg(long, value_name = "REF")]
    changed_since: Option<String>,

    /// Run mutation testing on a GDScript source file (internal).
    /// For each single-point mutation (operator/literal replacement), the test
    /// suite is re-run to check whether a test catches the change ("killed").
    /// Prints a mutation score at the end. Cannot be combined with --watch.
    #[arg(long, value_name = "FILE", conflicts_with = "watch")]
    mutate: Option<String>,

    #[command(flatten)]
    worker: WorkerArgs,

    #[command(subcommand)]
    command: Option<Subcmd>,
}

#[derive(clap::Args, Clone)]
struct WorkerArgs {
    #[arg(long, env = "GODOT_EXE", global = true)]
    godot_exe: Option<String>,

    #[arg(long, default_value = ".", global = true)]
    project_path: String,

    #[arg(
        long,
        env = "GDTEST_HARNESS",
        default_value = "addons/gdtest/gdtest_worker.gd",
        global = true
    )]
    harness: String,
}

#[derive(Subcommand)]
enum Subcmd {
    /// Internal: spawn a worker and time N round-trip pings (M0 sanity check).
    #[command(name = "__ping", hide = true)]
    Ping {
        #[arg(long, default_value_t = 10)]
        count: u32,
    },
    /// Internal: run a fixed list of tests in one persistent worker (M1.5 bench).
    #[command(name = "__bench", hide = true)]
    Bench {
        #[arg(long)]
        script: String,
        #[arg(long, value_delimiter = ',')]
        funcs: Vec<String>,
    },
    /// Merge JUnit XML shards from CI matrix jobs into a single report (internal).
    ///
    /// Example:
    ///   gdtest aggregate shard-1.xml shard-2.xml shard-3.xml --output combined.xml
    ///
    /// In GitHub Actions matrix:
    ///   gdtest test/ --partition count:${{ matrix.shard }}/4 --reporter junit > shard.xml
    ///   # (after all shards) gdtest aggregate shard-*.xml --output combined.xml
    Aggregate {
        /// JUnit XML files to merge (produced by `--reporter junit`).
        files: Vec<String>,
        /// Write merged XML to FILE instead of stdout.
        #[arg(long, value_name = "FILE")]
        output: Option<String>,
    },
    /// Rewrite framework-specific tests into parallel gdtest files without touching the originals.
    Migrate {
        #[command(subcommand)]
        command: MigrateSubcmd,
    },
}

#[derive(Subcommand)]
enum MigrateSubcmd {
    /// Rewrite GUT tests into parallel gdtest files and emit migration reports.
    Gut {
        /// One or more GUT test files or directories to rewrite.
        inputs: Vec<String>,
        /// Output directory for generated gdtest files and reports.
        #[arg(long, value_name = "DIR", default_value = ".gdtest_rewrite/gut")]
        out: String,
    },
    /// Rewrite gdUnit4 tests into parallel gdtest files and emit migration reports.
    Gdunit4 {
        /// One or more gdUnit4 test files or directories to rewrite.
        inputs: Vec<String>,
        /// Output directory for generated gdtest files and reports.
        #[arg(long, value_name = "DIR", default_value = ".gdtest_rewrite/gdunit4")]
        out: String,
    },
}

fn main() -> anyhow::Result<()> {
    // Load gdtest.toml and inject values as env vars before clap parses.
    // Priority: CLI flag > env var > gdtest.toml > built-in default.
    config::apply_env_defaults()?;

    let cli = Cli::parse();

    match &cli.command {
        Some(Subcmd::Ping { count }) => {
            let cfg = cfg_or_die(&cli)?;
            return ping::run(*count, &cfg);
        }
        Some(Subcmd::Bench { script, funcs }) => {
            let cfg = cfg_or_die(&cli)?;
            return bench::run(script, funcs, &cfg);
        }
        Some(Subcmd::Aggregate { files, output }) => {
            return aggregate::run(files, output.as_deref());
        }
        Some(Subcmd::Migrate { command }) => match command {
            MigrateSubcmd::Gut { inputs, out } => {
                let targets = if inputs.is_empty() {
                    vec![".".to_string()]
                } else {
                    inputs.clone()
                };
                let project_abs = PathBuf::from(&cli.worker.project_path)
                    .canonicalize()
                    .with_context(|| {
                        format!("project path does not exist: {}", cli.worker.project_path)
                    })?;
                let out_dir = if Path::new(out).is_absolute() {
                    PathBuf::from(out)
                } else {
                    project_abs.join(out)
                };
                return migrate::run_gut(&targets, &project_abs, &out_dir);
            }
            MigrateSubcmd::Gdunit4 { inputs, out } => {
                let targets = if inputs.is_empty() {
                    vec![".".to_string()]
                } else {
                    inputs.clone()
                };
                let project_abs = PathBuf::from(&cli.worker.project_path)
                    .canonicalize()
                    .with_context(|| {
                        format!("project path does not exist: {}", cli.worker.project_path)
                    })?;
                let out_dir = if Path::new(out).is_absolute() {
                    PathBuf::from(out)
                } else {
                    project_abs.join(out)
                };
                return migrate::run_gdunit4(&targets, &project_abs, &out_dir);
            }
        },
        None => {}
    }

    // Default to scanning from cwd when no targets are given (internal).
    let mut targets = cli.targets.clone();
    if targets.is_empty() {
        targets.push(".".to_string());
    }

    // Single-test short-circuit (M1 mode): `script::func` on its own. Pool
    // adds frame-tick latency that single-test focus runs don't need.
    if targets.len() == 1 && targets[0].contains("::") && !cli.list && cli.filter.is_none() {
        let cfg = cfg_or_die(&cli)?;
        if !cli.no_auto_import {
            let single = targets[0]
                .split_once("::")
                .map(|(s, _)| s.to_string())
                .unwrap_or_default();
            ensure_class_cache(&cfg, &[single])?;
        }
        return run::single(&targets[0], &cfg);
    }

    let cfg = cfg_or_die(&cli)?;
    if !cli.no_auto_import && !cli.list {
        // Skip when --list: discovery doesn't touch Godot, no need to import.
        ensure_class_cache(&cfg, &targets)?;
    }
    let jobs = cli.jobs.unwrap_or_else(num_cpus::get).max(1);
    let reporter = cli.reporter.unwrap_or(reporter::ReporterKind::Pretty);
    let isolation = cli.isolation.unwrap_or(pool::IsolationMode::File);
    let timeout = cli.timeout.unwrap_or(5000);
    let retries = cli.retries.unwrap_or(0);
    if cli.watch {
        return run_watch(
            targets,
            cli.filter,
            cli.tag,
            cli.exclude_tag,
            jobs,
            reporter,
            isolation,
            cfg,
            timeout,
            cli.fail_fast,
            cli.timeout_warn,
            retries,
        );
    }
    let html_report_path = cli.html_report.as_deref().map(PathBuf::from);
    if let Some(mutate_file) = cli.mutate {
        return run_mutate(
            mutate_file,
            targets,
            cli.filter,
            jobs,
            reporter,
            isolation,
            cfg,
            timeout,
        );
    }
    run_pool(
        targets,
        cli.filter,
        cli.tag,
        cli.exclude_tag,
        cli.partition,
        cli.list,
        jobs,
        reporter,
        isolation,
        cfg,
        timeout,
        cli.fail_fast,
        cli.timeout_warn,
        retries,
        cli.no_rewrite,
        cli.coverage,
        cli.min_coverage,
        cli.coverage_exclude,
        html_report_path,
        cli.bench,
        cli.changed_only,
        cli.changed_since,
    )
}

fn run_watch(
    targets: Vec<String>,
    filter: Option<String>,
    include_tags: Vec<String>,
    exclude_tags: Vec<String>,
    jobs: usize,
    rkind: reporter::ReporterKind,
    isolation: pool::IsolationMode,
    cfg: worker::Config,
    timeout_ms: u64,
    fail_fast: bool,
    timeout_warn_ms: Option<u64>,
    retries: u32,
) -> anyhow::Result<()> {
    let project_abs = PathBuf::from(&cfg.project_path).canonicalize()?;
    let mut file_targets: Vec<String> = Vec::new();
    let mut explicit_tests: Vec<pool::TestItem> = Vec::new();
    for t in &targets {
        if let Some((script, func)) = t.split_once("::") {
            let s = normalize_res(script, &project_abs)?;
            explicit_tests.push(pool::TestItem::new(s, func.to_string()));
        } else {
            file_targets.push(t.clone());
        }
    }
    let patterns = config::load_patterns(&cfg.project_path);
    let resolved_files = gdtest_discovery::resolve(&file_targets, &project_abs, &patterns)?;
    watch::run(watch::WatchOptions {
        project_path: project_abs,
        jobs,
        reporter_kind: rkind,
        isolation,
        cfg,
        filter,
        include_tags,
        exclude_tags,
        explicit_targets: targets,
        resolved_files,
        explicit_tests,
        timeout_ms,
        fail_fast,
        timeout_warn_ms,
        retries,
    })
}

/// Run `godot --headless --import` if Godot's class cache is missing OR
/// any of the user's target `.gd` files lacks its sibling `.uid` (means
/// the file was added to the project after the last import — Godot
/// doesn't know about it yet, and `class_name` references and resource
/// loads referencing the new file will fail).
///
/// The check is a stat per target file plus the cache stat — zero
/// process spawn unless we actually need to import.
fn ensure_class_cache(cfg: &worker::Config, targets: &[String]) -> anyhow::Result<()> {
    let project_path = std::path::PathBuf::from(&cfg.project_path);
    let cache = project_path
        .join(".godot")
        .join("global_script_class_cache.cfg");
    let cache_exists = cache.exists();
    let stale_target = !cache_exists || targets_have_missing_uid(targets, &project_path);
    if !stale_target {
        return Ok(());
    }
    let reason = if cache_exists {
        "newly added .gd file detected (no .uid sibling)"
    } else {
        "class cache missing"
    };
    eprintln!(
        "gdtest: building Godot class cache ({reason}; one-shot {} --headless --import)",
        cfg.godot_exe,
    );
    let status = std::process::Command::new(&cfg.godot_exe)
        .arg("--headless")
        .arg("--path")
        .arg(&project_path)
        .arg("--import")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| anyhow::anyhow!("failed to spawn `{} --import`: {e}", cfg.godot_exe))?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "`{} --headless --path {} --import` exited with {status}; rerun gdtest with --no-auto-import to skip",
            cfg.godot_exe,
            project_path.display(),
        ));
    }
    Ok(())
}

/// Returns true if any user-supplied target points at a `.gd` file (or
/// directory containing `.gd` files) where the corresponding `<file>.uid`
/// is missing. We resolve targets via the same logic as the pool path so
/// directories get walked and `res://` paths normalise correctly.
fn targets_have_missing_uid(targets: &[String], project_path: &std::path::Path) -> bool {
    let Ok(project_abs) = project_path.canonicalize() else {
        return false;
    };
    let mut file_targets: Vec<String> = Vec::new();
    for t in targets {
        let path_part = t.split_once("::").map(|(s, _)| s).unwrap_or(t);
        file_targets.push(path_part.to_string());
    }
    let patterns = config::load_patterns(&project_path.to_string_lossy());
    let Ok(resolved) = gdtest_discovery::resolve(&file_targets, &project_abs, &patterns) else {
        return false;
    };
    resolved.iter().any(|res| {
        let Ok(abs) = gdtest_discovery::res_to_abs(res, &project_abs) else {
            return false;
        };
        let mut uid = abs.into_os_string();
        uid.push(".uid");
        !std::path::Path::new(&uid).exists()
    })
}

fn cfg_or_die(cli: &Cli) -> anyhow::Result<worker::Config> {
    let godot_exe = cli
        .worker
        .godot_exe
        .clone()
        .ok_or_else(|| anyhow::anyhow!("GODOT_EXE env var or --godot-exe flag required"))?;
    let plugins = config::load_plugins(&cli.worker.project_path);
    Ok(worker::Config {
        godot_exe,
        project_path: cli.worker.project_path.clone(),
        harness: cli.worker.harness.clone(),
        update_snapshots: cli.update_snapshots,
        seed: cli.seed,
        plugins,
    })
}

#[tokio::main(flavor = "multi_thread")]
#[allow(clippy::too_many_arguments)]
async fn run_pool(
    targets: Vec<String>,
    filter: Option<String>,
    include_tags: Vec<String>,
    exclude_tags: Vec<String>,
    partition: Option<String>,
    list_only: bool,
    jobs: usize,
    rkind: reporter::ReporterKind,
    isolation: pool::IsolationMode,
    cfg: worker::Config,
    timeout_ms: u64,
    fail_fast: bool,
    timeout_warn_ms: Option<u64>,
    retries: u32,
    no_rewrite: bool,
    enable_coverage: bool,
    min_coverage: Option<f32>,
    coverage_exclude: Vec<String>,
    html_report_path: Option<PathBuf>,
    bench_mode: bool,
    changed_only: bool,
    changed_since: Option<String>,
) -> anyhow::Result<()> {
    let project_abs = PathBuf::from(&cfg.project_path).canonicalize()?;

    // Split target strings into (file target, optional ::func pin).
    let mut file_targets: Vec<String> = Vec::new();
    let mut explicit_tests: Vec<pool::TestItem> = Vec::new();
    for t in &targets {
        if let Some((script, func)) = t.split_once("::") {
            let s = normalize_res(script, &project_abs)?;
            explicit_tests.push(pool::TestItem::new(s, func.to_string()));
        } else {
            file_targets.push(t.clone());
        }
    }

    // Resolve file targets to res:// paths, then tree-sitter-discover each.
    let patterns = config::load_patterns(&cfg.project_path);
    let mut files = gdtest_discovery::resolve(&file_targets, &project_abs, &patterns)?;

    // Apply test impact analysis: keep only files affected by changed sources.
    if changed_only || changed_since.is_some() {
        let since_ref = changed_since.as_deref();
        let changed = get_changed_gd_files(&project_abs, since_ref);
        if changed.is_empty() {
            eprintln!("gdtest: test impact: no changed .gd files found");
            return Ok(());
        }
        let changed_set: std::collections::HashSet<String> = changed.into_iter().collect();
        let (_, reverse) = gdtest_discovery::deps::build_dep_graph(&project_abs);
        let affected = gdtest_discovery::deps::affected_files(&changed_set, &reverse);
        let total = files.len();
        files.retain(|f| changed_set.contains(f) || affected.contains(f));
        eprintln!(
            "gdtest: test impact: {}/{} test file(s) affected by changes",
            files.len(),
            total,
        );
        if files.is_empty() {
            eprintln!("gdtest: no tests affected — nothing to run");
            return Ok(());
        }
    }

    // Rewrite assert() calls to Gdt._rewrite_*() for rich failure diffs.
    let rewrite_map = if no_rewrite {
        HashMap::new()
    } else {
        do_rewrites(&files, &project_abs).unwrap_or_else(|e| {
            eprintln!("gdtest: assert rewriting failed (falling back to original): {e}");
            HashMap::new()
        })
    };

    let mut items: Vec<pool::TestItem> = explicit_tests;
    for file in &files {
        let abs = gdtest_discovery::res_to_abs(file, &project_abs)?;
        let mut entries = gdtest_discovery::list_test_entries_in(&abs).unwrap_or_default();
        if entries.is_empty() {
            eprintln!("gdtest: no test functions found in {file} (parse error or empty file)");
            continue;
        }
        if !include_tags.is_empty() || !exclude_tags.is_empty() {
            entries.retain(|e| {
                if !include_tags.is_empty() && !e.tags.iter().any(|t| include_tags.contains(t)) {
                    return false;
                }
                if !exclude_tags.is_empty() && e.tags.iter().any(|t| exclude_tags.contains(t)) {
                    return false;
                }
                true
            });
        }
        let script = rewrite_map
            .get(file)
            .cloned()
            .unwrap_or_else(|| file.clone());
        for e in entries {
            let item = pool::TestItem {
                script: script.clone(),
                func: e.name,
                display_script: file.clone(),
            };
            match e.kind {
                gdtest_discovery::TestKind::Bench => {
                    if bench_mode {
                        items.push(item);
                    }
                }
                gdtest_discovery::TestKind::Normal => {
                    if !bench_mode {
                        items.push(item);
                    }
                }
            }
        }
    }

    // Apply --filter on the `file :: func` form.
    if let Some(pat) = &filter {
        let re = regex::Regex::new(pat)
            .map_err(|e| anyhow::anyhow!("invalid --filter regex `{pat}`: {e}"))?;
        items.retain(|it| re.is_match(&format!("{} :: {}", it.display_script, it.func)));
    }

    // Apply --partition after filtering so shard Ns are stable regardless of filter.
    if let Some(p) = &partition {
        items = apply_partition(items, p)?;
    }

    if items.is_empty() {
        eprintln!("gdtest: no test functions matched");
        std::process::exit(1);
    }

    if list_only {
        for it in &items {
            println!("{} :: {}", it.display_script, it.func);
        }
        return Ok(());
    }

    // Instrument source files for coverage before starting the pool.
    let coverage_backups = if enable_coverage {
        match instrument_for_coverage(&project_abs, &coverage_exclude) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("gdtest: coverage instrumentation failed: {e}");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // Cap worker count by work available: spawning more workers than there
    // are tests would waste Godot startups (~200 ms each) for no gain.
    let jobs = jobs.min(items.len());
    let (summary, event_log) = pool::run(
        items,
        pool::PoolOptions {
            jobs,
            cfg,
            reporter: reporter::Reporter::new(rkind),
            isolation,
            timeout_ms,
            fail_fast,
            timeout_warn_ms,
            retries,
        },
    )
    .await?;

    // Restore instrumented source files before generating the report so
    // coverage analysis reads un-modified original source if needed later.
    if enable_coverage && !coverage_backups.is_empty() {
        restore_coverage_backups(&coverage_backups);

        match coverage::collect_from_project(&project_abs) {
            Ok(cov_data) => {
                let lines_found: coverage::LinesFound = coverage_backups
                    .iter()
                    .map(|(_, _, lines, res_path)| (res_path.clone(), lines.clone()))
                    .collect();

                let lcov_path = project_abs.join("coverage").join("lcov.info");
                if let Err(e) = coverage::write_lcov(&cov_data, &lines_found, &lcov_path) {
                    eprintln!("gdtest: coverage: failed to write LCOV: {e}");
                } else {
                    eprintln!("gdtest: coverage: wrote {}", lcov_path.display());
                }

                if let Err(e) = coverage::print_summary(&cov_data, &lines_found, min_coverage) {
                    // min_coverage threshold failed — print the error and set exit code
                    eprintln!("gdtest: {e}");
                    std::process::exit(2);
                }
            }
            Err(e) => eprintln!("gdtest: coverage: failed to collect data: {e}"),
        }
    }

    // Generate HTML report if requested.
    if let Some(out_path) = html_report_path {
        let results: Vec<html_report::TestResult> = event_log
            .iter()
            .filter_map(pool_event_to_test_result)
            .collect();
        let record = html_report::RunRecord {
            timestamp: html_report::current_timestamp(),
            git_sha: html_report::git_sha(&project_abs),
            results,
        };
        if let Err(e) = html_report::write_run_record(&record, &project_abs) {
            eprintln!("gdtest: html-report: failed to write history: {e}");
        }
        html_report::prune_history(&project_abs, 50);
        let history = html_report::load_history(&project_abs, 20);
        let opts = html_report::ReportOptions::default();
        match html_report::generate_report(&record, &history, &out_path, &opts) {
            Ok(()) => eprintln!("gdtest: html report written to {}", out_path.display()),
            Err(e) => eprintln!(
                "gdtest: html-report: failed to write {}: {e}",
                out_path.display()
            ),
        }
    }

    let code = summary.exit_code();
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

fn pool_event_to_test_result(ev: &pool::PoolEvent) -> Option<html_report::TestResult> {
    match ev {
        pool::PoolEvent::TestPass { file, name, us, .. } => Some(html_report::TestResult {
            file: file.clone(),
            name: name.clone(),
            status: html_report::TestStatus::Pass,
            duration_us: *us,
            failures: Vec::new(),
        }),
        pool::PoolEvent::TestFail {
            file,
            name,
            us,
            failures,
            ..
        } => Some(html_report::TestResult {
            file: file.clone(),
            name: name.clone(),
            status: html_report::TestStatus::Fail,
            duration_us: *us,
            failures: failures.clone(),
        }),
        pool::PoolEvent::TestError {
            file,
            name,
            message,
            ..
        } => Some(html_report::TestResult {
            file: file.clone(),
            name: name.clone(),
            status: html_report::TestStatus::Error,
            duration_us: 0,
            failures: vec![message.clone()],
        }),
        pool::PoolEvent::TestPending { file, name, .. } => Some(html_report::TestResult {
            file: file.clone(),
            name: name.clone(),
            status: html_report::TestStatus::Pending,
            duration_us: 0,
            failures: Vec::new(),
        }),
        pool::PoolEvent::TestSkip { file, name, .. } => Some(html_report::TestResult {
            file: file.clone(),
            name: name.clone(),
            status: html_report::TestStatus::Skip,
            duration_us: 0,
            failures: Vec::new(),
        }),
        _ => None,
    }
}

fn apply_partition(items: Vec<pool::TestItem>, spec: &str) -> anyhow::Result<Vec<pool::TestItem>> {
    let (mode_str, rest) = spec.split_once(':').ok_or_else(|| {
        anyhow::anyhow!("--partition: expected `count:N/M` or `hash:N/M`, got `{spec}`")
    })?;
    let (n_str, m_str) = rest
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("--partition: expected `N/M`, got `{rest}`"))?;
    let n: usize = n_str
        .parse()
        .map_err(|_| anyhow::anyhow!("--partition: N must be a positive integer"))?;
    let m: usize = m_str
        .parse()
        .map_err(|_| anyhow::anyhow!("--partition: M must be a positive integer"))?;
    if m == 0 || n == 0 || n > m {
        return Err(anyhow::anyhow!(
            "--partition: N must be 1..=M and M must be >= 1, got {n}/{m}"
        ));
    }
    let result = match mode_str {
        "count" => items
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % m == n - 1)
            .map(|(_, item)| item)
            .collect(),
        "hash" => items
            .into_iter()
            .filter(|item| {
                let key = format!("{}::{}", item.script, item.func);
                (fnv1a_hash(key.as_bytes()) % m as u64) as usize == n - 1
            })
            .collect(),
        other => {
            return Err(anyhow::anyhow!(
                "--partition: unknown mode `{other}`, expected `count` or `hash`"
            ))
        }
    };
    Ok(result)
}

fn fnv1a_hash(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 14695981039346656037;
    const PRIME: u64 = 1099511628211;
    bytes
        .iter()
        .fold(OFFSET, |h, &b| (h ^ b as u64).wrapping_mul(PRIME))
}

// ── Coverage helpers ──────────────────────────────────────────────────────────

/// Backup entry: (original_path, backup_path, instrumented_lines, res_path).
type CoverageBackup = (PathBuf, PathBuf, std::collections::HashSet<u32>, String);

/// Walk the project, instrument eligible .gd files in-place, and return backups.
fn instrument_for_coverage(
    project_abs: &Path,
    exclude_patterns: &[String],
) -> anyhow::Result<Vec<CoverageBackup>> {
    use walkdir::WalkDir;
    let mut backups: Vec<CoverageBackup> = Vec::new();

    for entry in WalkDir::new(project_abs).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gd") {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        // Skip test files.
        if name.starts_with("test_") {
            continue;
        }

        let rel = path.strip_prefix(project_abs).unwrap();
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        // Skip the harness itself and Godot internals.
        if rel_str.starts_with("addons/gdtest/")
            || rel_str.starts_with(".godot/")
            || rel_str.starts_with(".gdtest_")
        {
            continue;
        }

        // Skip user-configured exclude patterns.
        if exclude_patterns
            .iter()
            .any(|pat| coverage::glob_match(pat, &rel_str))
        {
            continue;
        }

        let res_path = format!("res://{rel_str}");
        let source = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let (instrumented, lines) =
            gdtest_discovery::coverage::instrument_source(&source, &res_path);

        if lines.is_empty() {
            continue; // no function bodies — nothing to track
        }

        let backup_path = path.with_extension("gd.gdtest_bak");
        std::fs::write(&backup_path, &source)
            .with_context(|| format!("writing backup {}", backup_path.display()))?;
        std::fs::write(path, &instrumented)
            .with_context(|| format!("writing instrumented {}", path.display()))?;

        backups.push((path.to_path_buf(), backup_path, lines, res_path));
    }
    Ok(backups)
}

/// Restore original source files from backups and delete the backup files.
fn restore_coverage_backups(backups: &[CoverageBackup]) {
    for (original, backup, _, _) in backups {
        if let Err(e) = std::fs::copy(backup, original) {
            eprintln!(
                "gdtest: coverage: failed to restore {}: {e}",
                original.display()
            );
        } else {
            std::fs::remove_file(backup).ok();
        }
    }
}

/// Rewrite `assert(...)` calls in every file to `Gdt._rewrite_*(...)` variants,
/// writing copies to `<project>/.gdtest_rewrite/`. Returns a map from the original
/// `res://` path to the rewritten `res://` path for each file that changed.
fn do_rewrites(files: &[String], project_abs: &Path) -> anyhow::Result<HashMap<String, String>> {
    let rewrite_dir = project_abs.join(".gdtest_rewrite");
    if rewrite_dir.exists() {
        std::fs::remove_dir_all(&rewrite_dir)
            .with_context(|| format!("removing {}", rewrite_dir.display()))?;
    }
    let mut map = HashMap::new();
    for res_path in files {
        let abs = gdtest_discovery::res_to_abs(res_path, project_abs)?;
        let output = gdtest_discovery::rewrite_file(&abs, res_path)?;
        if output.changed {
            let rel = res_path.strip_prefix("res://").unwrap_or(res_path.as_str());
            let dest = rewrite_dir.join(rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            std::fs::write(&dest, &output.source)
                .with_context(|| format!("writing {}", dest.display()))?;
            map.insert(res_path.clone(), format!("res://.gdtest_rewrite/{rel}"));
        }
    }
    Ok(map)
}

/// Return res:// paths of .gd files changed relative to HEAD (or `since_ref`).
/// Runs `git diff --name-only` + `git ls-files --others` for untracked files.
fn get_changed_gd_files(project_abs: &Path, since_ref: Option<&str>) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();

    // Tracked changes: staged + unstaged vs HEAD (or vs `since_ref`)
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C")
        .arg(project_abs)
        .arg("diff")
        .arg("--name-only")
        .arg("--diff-filter=ACMRT");
    if let Some(r) = since_ref {
        cmd.arg(r);
    }
    cmd.arg("HEAD");
    if let Ok(out) = cmd.output() {
        if let Ok(text) = std::str::from_utf8(&out.stdout) {
            for line in text.lines().filter(|l| l.ends_with(".gd")) {
                if let Ok(rp) = gdtest_discovery::to_res_path(&project_abs.join(line), project_abs)
                {
                    paths.push(rp);
                }
            }
        }
    }

    // Untracked .gd files (only relevant for --changed-only, not --changed-since)
    if since_ref.is_none() {
        let mut cmd2 = std::process::Command::new("git");
        cmd2.arg("-C")
            .arg(project_abs)
            .arg("ls-files")
            .arg("--others")
            .arg("--exclude-standard");
        if let Ok(out2) = cmd2.output() {
            if let Ok(text2) = std::str::from_utf8(&out2.stdout) {
                for line in text2.lines().filter(|l| l.ends_with(".gd")) {
                    if let Ok(rp) =
                        gdtest_discovery::to_res_path(&project_abs.join(line), project_abs)
                    {
                        paths.push(rp);
                    }
                }
            }
        }
    }

    paths
}

/// Run mutation testing: discover items from `targets`, then for each mutation
/// in `mutate_file` run the full test suite and report score.
#[tokio::main(flavor = "multi_thread")]
#[allow(clippy::too_many_arguments)]
async fn run_mutate(
    mutate_file: String,
    targets: Vec<String>,
    filter: Option<String>,
    jobs: usize,
    _rkind: reporter::ReporterKind,
    isolation: pool::IsolationMode,
    cfg: worker::Config,
    timeout_ms: u64,
) -> anyhow::Result<()> {
    let project_abs = PathBuf::from(&cfg.project_path).canonicalize()?;

    // Discover test items from the given targets (same logic as run_pool).
    let mut file_targets: Vec<String> = Vec::new();
    let mut items: Vec<pool::TestItem> = Vec::new();
    for t in &targets {
        if let Some((script, func)) = t.split_once("::") {
            let s = normalize_res(script, &project_abs)?;
            items.push(pool::TestItem::new(s, func.to_string()));
        } else {
            file_targets.push(t.clone());
        }
    }
    let patterns = config::load_patterns(&cfg.project_path);
    let files = gdtest_discovery::resolve(&file_targets, &project_abs, &patterns)?;
    for file in &files {
        let abs = gdtest_discovery::res_to_abs(file, &project_abs)?;
        let entries = gdtest_discovery::list_test_entries_in(&abs).unwrap_or_default();
        for e in entries {
            if e.kind == gdtest_discovery::TestKind::Normal {
                items.push(pool::TestItem {
                    script: file.clone(),
                    func: e.name,
                    display_script: file.clone(),
                });
            }
        }
    }

    // Apply --filter
    if let Some(pat) = &filter {
        let re = regex::Regex::new(pat)
            .map_err(|e| anyhow::anyhow!("invalid --filter regex `{pat}`: {e}"))?;
        items.retain(|it| re.is_match(&format!("{} :: {}", it.display_script, it.func)));
    }

    if items.is_empty() {
        eprintln!(
            "gdtest mutate: no test functions found in targets — nothing to run against mutations"
        );
        std::process::exit(1);
    }

    let opts = mutate_runner::MutOpts {
        cfg,
        jobs,
        isolation,
        timeout_ms,
    };
    let report = mutate_runner::run(&mutate_file, items, opts).await?;

    // Print summary
    let score = if report.total > 0 {
        (report.killed as f64 / report.total as f64) * 100.0
    } else {
        100.0
    };
    eprintln!(
        "\ngdtest mutate: {}/{} killed — mutation score {:.1}%",
        report.killed, report.total, score,
    );
    if !report.survived.is_empty() {
        eprintln!("survived mutations:");
        for s in &report.survived {
            eprintln!("  {s}");
        }
    }

    if report.survived.is_empty() || report.total == 0 {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn normalize_res(path: &str, project_abs: &std::path::Path) -> anyhow::Result<String> {
    if path.starts_with("res://") {
        return Ok(path.to_string());
    }
    let p = std::path::PathBuf::from(path);
    let abs = if p.is_absolute() {
        p
    } else {
        project_abs.join(p)
    };
    gdtest_discovery::to_res_path(&abs, project_abs)
}
