//! Worker pool + MPSC test dispatch (M2/M3: internal/70/71/73/77).
//!
//! Work item granularity: a `(script, func)` pair. Tree-sitter discovery
//! (gdtest-analysis) produces the full list; the pool dispatches each to
//! whichever worker grabs it off the shared queue first — load-balanced
//! for free, no shard pre-computation.
//!
//! Per-test internal correlation:
//! - Each submit is followed by exactly one result event (TestPass /
//!   TestFail / TestError), preceded by a TestStart. Between TestStart
//!   and the result we buffer stderr `SCRIPT ERROR` lines and escalate
//!   a silent-pass to TestError when the test completed without failing
//!   any expect() but still logged a runtime error.
//! - A mid-test worker crash (stdout close) is attributed to the
//!   in-flight test and the queue item is retried on a fresh worker.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::{mpsc, Mutex};

use crate::protocol::{Command, Event};
use crate::reporter::{Reporter, Summary};
use crate::worker::{Config, Worker};

const SCRIPT_ERROR_MARKER: &str = "SCRIPT ERROR";
const MAX_DIAGNOSTIC_LINES: usize = 8;

#[derive(Debug, Clone)]
pub struct TestItem {
    /// res:// path sent to Godot (may point to a rewritten copy).
    pub script: String,
    pub func: String,
    /// Original res:// path shown to the user in all reporter output.
    pub display_script: String,
}

impl TestItem {
    pub fn new(script: String, func: String) -> Self {
        let display_script = script.clone();
        Self {
            script,
            func,
            display_script,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum PoolEvent {
    TestPass {
        worker: usize,
        file: String,
        name: String,
        us: u64,
    },
    TestFail {
        worker: usize,
        file: String,
        name: String,
        us: u64,
        failures: Vec<String>,
    },
    TestError {
        worker: usize,
        file: String,
        name: String,
        message: String,
    },
    TestPending {
        worker: usize,
        file: String,
        name: String,
        message: String,
    },
    TestSkip {
        worker: usize,
        file: String,
        name: String,
        message: String,
    },
    WorkerCrashed {
        worker: usize,
        current_file: Option<String>,
        message: String,
    },
    StderrNoise {
        worker: usize,
        line: String,
    },
}

/// How aggressively the pool recycles workers between tests.
///
/// `File` (default) keeps a worker alive across every test it picks up — fast,
/// matches the M2 behaviour, but autoload mutations and `Engine.set_meta` calls
/// leak from one test to the next.
///
/// `Test` shuts the worker down after every test result and spawns a fresh
/// Godot for the next one. Slow (~150–300 ms startup × N tests) but each test
/// runs in a clean process. Use it for autoload-heavy suites (`GameClock` etc.)
/// where state leak would otherwise cascade failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationMode {
    File,
    Test,
}

pub struct PoolOptions {
    pub jobs: usize,
    pub cfg: Config,
    pub reporter: Reporter,
    pub isolation: IsolationMode,
    /// Per-test timeout in milliseconds. A test that doesn't produce a result
    /// event within this window is reported as an error and its worker respawned.
    pub timeout_ms: u64,
    /// Stop dispatching new tests after the first failure or error.
    pub fail_fast: bool,
    /// Print a warning when a test exceeds this duration without killing it.
    /// `None` disables the warning tier.
    pub timeout_warn_ms: Option<u64>,
    /// Number of times to re-run a failing or erroring test before reporting
    /// the failure. `0` means no retries (report on first failure). Timeouts
    /// and crashes are not retried — only TestFail / TestError outcomes are.
    pub retries: u32,
}

pub async fn run(items: Vec<TestItem>, opts: PoolOptions) -> Result<(Summary, Vec<PoolEvent>)> {
    let PoolOptions {
        jobs,
        cfg,
        mut reporter,
        isolation,
        timeout_ms,
        fail_fast,
        timeout_warn_ms,
        retries,
    } = opts;

    let mut summary = Summary::default();
    let mut event_log: Vec<PoolEvent> = Vec::new();

    // Queue stores (item, retries_remaining) so each item carries its own
    // retry budget without a shared map.
    let total = items.len() as u32;
    reporter.set_total(total);
    let queue: Arc<Mutex<VecDeque<(TestItem, u32)>>> = Arc::new(Mutex::new(
        items.into_iter().map(|it| (it, retries)).collect(),
    ));
    let cancel = Arc::new(AtomicBool::new(false));
    let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<PoolEvent>();

    let cancel_sig = cancel.clone();
    // Held until the end of run() so it doesn't outlive a single pool
    // iteration. Without the abort, every watch-mode iteration would
    // leave a SIGINT handler running forever, and Ctrl-C would print
    // the same message N times (once per accumulated handler).
    let sigint_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            cancel_sig.store(true, Ordering::SeqCst);
            eprintln!("\ngdtest: SIGINT received, finishing in-flight tests...");
        }
    });

    let mut worker_tasks = Vec::with_capacity(jobs);
    for id in 0..jobs {
        let queue = queue.clone();
        let cfg = cfg.clone();
        let cancel = cancel.clone();
        let ev_tx = ev_tx.clone();
        worker_tasks.push(tokio::spawn(async move {
            worker_loop(
                id,
                cfg,
                queue,
                cancel,
                ev_tx,
                isolation,
                timeout_ms,
                timeout_warn_ms,
            )
            .await
        }));
    }
    drop(ev_tx);

    // Ticker drives the spinner animation in PrettyReporter.
    // Starts 80 ms after pool launch so the first draw is a real event.
    let tick_start = tokio::time::Instant::now() + Duration::from_millis(80);
    let mut ticker = tokio::time::interval_at(tick_start, Duration::from_millis(80));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            ev = ev_rx.recv() => {
                match ev {
                    Some(ev) => {
                        reporter.on_event(&ev);
                        summary.apply(&ev);
                        if fail_fast
                            && matches!(ev, PoolEvent::TestFail { .. } | PoolEvent::TestError { .. })
                        {
                            cancel.store(true, Ordering::SeqCst);
                        }
                        if matches!(
                            ev,
                            PoolEvent::TestPass { .. }
                                | PoolEvent::TestFail { .. }
                                | PoolEvent::TestError { .. }
                                | PoolEvent::TestPending { .. }
                                | PoolEvent::TestSkip { .. }
                        ) {
                            event_log.push(ev);
                        }
                    }
                    None => break,
                }
            }
            _ = ticker.tick() => {
                reporter.tick();
            }
        }
    }
    reporter.finish(&summary);

    for t in worker_tasks {
        let _ = t.await;
    }
    sigint_task.abort();
    Ok((summary, event_log))
}

async fn worker_loop(
    id: usize,
    cfg: Config,
    queue: Arc<Mutex<VecDeque<(TestItem, u32)>>>,
    cancel: Arc<AtomicBool>,
    ev_tx: mpsc::UnboundedSender<PoolEvent>,
    isolation: IsolationMode,
    timeout_ms: u64,
    timeout_warn_ms: Option<u64>,
) {
    let mut worker = match Worker::spawn(&cfg) {
        Ok(w) => w,
        Err(e) => {
            let _ = ev_tx.send(PoolEvent::WorkerCrashed {
                worker: id,
                current_file: None,
                message: format!("spawn failed: {e}"),
            });
            return;
        }
    };
    let mut stderr_rx = worker.stderr.take().expect("worker stderr channel");

    match wait_for_ready(
        &mut worker,
        &mut stderr_rx,
        Duration::from_secs(30),
        "initial",
    )
    .await
    {
        Ok(()) => {}
        Err(message) => {
            let _ = ev_tx.send(PoolEvent::WorkerCrashed {
                worker: id,
                current_file: None,
                message,
            });
            return;
        }
    }

    loop {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let entry = {
            let mut q = queue.lock().await;
            q.pop_front()
        };
        let Some((item, retries_left)) = entry else {
            break;
        };

        if let Err(e) = worker
            .send(&Command::RunTest {
                script: item.script.clone(),
                func: item.func.clone(),
            })
            .await
        {
            let _ = ev_tx.send(PoolEvent::WorkerCrashed {
                worker: id,
                current_file: Some(item.display_script.clone()),
                message: format!("send RunTest: {e}"),
            });
            queue.lock().await.push_front((item, retries_left));
            if !try_respawn(&mut worker, &cfg, id, &ev_tx).await {
                return;
            }
            stderr_rx = worker.stderr.take().expect("worker stderr channel");
            continue;
        }

        let (crashed, maybe_result, new_rx) = run_one_test(
            id,
            &mut worker,
            &item,
            stderr_rx,
            &ev_tx,
            timeout_ms,
            timeout_warn_ms,
        )
        .await;
        stderr_rx = new_rx;
        let had_result = maybe_result.is_some();

        if let Some(result) = maybe_result {
            // Test completed normally. Check retry eligibility:
            // only TestFail / TestError are retried, not Pending / Skip / Pass.
            let retryable = retries_left > 0
                && matches!(
                    result,
                    PoolEvent::TestFail { .. } | PoolEvent::TestError { .. }
                );
            if retryable {
                eprintln!(
                    "[gdtest] retrying {} :: {} ({} retries left)",
                    item.display_script,
                    item.func,
                    retries_left - 1,
                );
                queue.lock().await.push_front((item, retries_left - 1));
                if isolation == IsolationMode::Test {
                    if !try_respawn(&mut worker, &cfg, id, &ev_tx).await {
                        return;
                    }
                    stderr_rx = worker.stderr.take().expect("worker stderr channel");
                }
                continue;
            }
            let _ = ev_tx.send(result);
        }

        if crashed {
            // Only re-enqueue on a true infrastructure crash (stdout closed mid-test, not a timeout).
            // Timeouts return had_result=true so they use the normal retries budget above.
            if !had_result {
                queue.lock().await.push_front((item, retries_left));
            }
            if !try_respawn(&mut worker, &cfg, id, &ev_tx).await {
                return;
            }
            stderr_rx = worker.stderr.take().expect("worker stderr channel");
            continue;
        }

        // internal: in --isolation=test, force a worker recycle after every
        // test. The old worker's Child has kill_on_drop=true so swapping
        // worker via try_respawn drops the previous instance and Godot
        // exits. We don't bother sending Shutdown first: the kill is
        // immediate and the test result has already been emitted.
        if isolation == IsolationMode::Test {
            if !try_respawn(&mut worker, &cfg, id, &ev_tx).await {
                return;
            }
            stderr_rx = worker.stderr.take().expect("worker stderr channel");
        }
    }

    drop(stderr_rx);
    let _ = worker.shutdown().await;
}

/// Drive one RunTest round-trip.
///
/// Returns `(crashed, result, stderr_rx)`:
/// - `crashed` — true when the worker process died and must be respawned.
/// - `result` — `Some(event)` when the test completed normally (not yet sent
///   to the reporter channel — caller decides to emit or retry); `None` when
///   the worker crashed/timed-out (crash events already sent directly).
async fn run_one_test(
    id: usize,
    worker: &mut Worker,
    item: &TestItem,
    mut stderr_rx: mpsc::UnboundedReceiver<String>,
    ev_tx: &mpsc::UnboundedSender<PoolEvent>,
    timeout_ms: u64,
    timeout_warn_ms: Option<u64>,
) -> (bool, Option<PoolEvent>, mpsc::UnboundedReceiver<String>) {
    let mut runtime_errors: Vec<String> = Vec::new();
    let mut pre_start_errors: Vec<String> = Vec::new();
    let mut saw_start = false;
    let now = tokio::time::Instant::now();
    let deadline = now + Duration::from_millis(timeout_ms);
    let warn_deadline = timeout_warn_ms.map(|ms| now + Duration::from_millis(ms));
    let mut warn_fired = false;

    loop {
        tokio::select! {
            biased;
            ev_result = worker.next_event() => {
                match ev_result {
                    Ok(Event::TestStart { .. }) => { saw_start = true; }
                    Ok(Event::TestPass { name, us }) => {
                        let ev = if runtime_errors.is_empty() {
                            PoolEvent::TestPass { worker: id, file: item.display_script.clone(), name: name.clone(), us }
                        } else {
                            let msg = format!(
                                "passed expectations but observed {} runtime error(s): {}",
                                runtime_errors.len(),
                                runtime_errors.join(" | "),
                            );
                            PoolEvent::TestError { worker: id, file: item.display_script.clone(), name: name.clone(), message: msg }
                        };
                        // Parameterized case result: forward and keep looping.
                        if name.contains('[') {
                            let _ = ev_tx.send(ev);
                            runtime_errors.clear();
                        } else {
                            return (false, Some(ev), stderr_rx);
                        }
                    }
                    Ok(Event::TestFail { name, us, mut failures }) => {
                        for e in runtime_errors.drain(..) {
                            failures.push(format!("runtime error: {e}"));
                        }
                        let ev = PoolEvent::TestFail {
                            worker: id, file: item.display_script.clone(), name: name.clone(), us, failures,
                        };
                        // Parameterized case result: forward and keep looping.
                        if name.contains('[') {
                            let _ = ev_tx.send(ev);
                        } else {
                            return (false, Some(ev), stderr_rx);
                        }
                    }
                    Ok(Event::TestError { name, message }) => {
                        let mut stderr_context: Vec<String> = Vec::new();
                        stderr_context.extend(pre_start_errors.iter().cloned());
                        stderr_context.extend(runtime_errors.iter().cloned());
                        let message = if stderr_context.is_empty() {
                            message
                        } else {
                            format!("{message} (stderr: {})", stderr_context.join(" | "))
                        };
                        let ev = PoolEvent::TestError {
                            worker: id, file: item.display_script.clone(), name: name.clone(), message,
                        };
                        // Parameterized case result: forward and keep looping.
                        if name.contains('[') {
                            let _ = ev_tx.send(ev);
                        } else {
                            return (false, Some(ev), stderr_rx);
                        }
                    }
                    Ok(Event::TestPending { name, message }) => {
                        let ev = PoolEvent::TestPending {
                            worker: id, file: item.display_script.clone(), name, message,
                        };
                        return (false, Some(ev), stderr_rx);
                    }
                    Ok(Event::TestSkip { name, message }) => {
                        let ev = PoolEvent::TestSkip {
                            worker: id, file: item.display_script.clone(), name, message,
                        };
                        return (false, Some(ev), stderr_rx);
                    }
                    // Parameterized batch: individual case results are forwarded
                    // directly; TestBatchDone signals the RunTest is complete.
                    Ok(Event::TestBatchDone) => {
                        return (false, None, stderr_rx);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        let name = if saw_start { item.func.clone() } else {
                            format!("{} (before TestStart)", item.func)
                        };
                        let mut stderr_context: Vec<String> = Vec::new();
                        stderr_context.extend(pre_start_errors.iter().cloned());
                        stderr_context.extend(runtime_errors.iter().cloned());
                        let msg = if stderr_context.is_empty() {
                            format!("worker crashed during test: {e}")
                        } else {
                            format!(
                                "worker crashed during test (stderr: {}): {e}",
                                stderr_context.join(" | "),
                            )
                        };
                        // Crash events sent directly — not subject to retry.
                        let _ = ev_tx.send(PoolEvent::TestError {
                            worker: id, file: item.display_script.clone(), name, message: msg,
                        });
                        let _ = ev_tx.send(PoolEvent::WorkerCrashed {
                            worker: id,
                            current_file: Some(item.display_script.clone()),
                            message: format!("stdout closed mid-test: {e}"),
                        });
                        return (true, None, stderr_rx);
                    }
                }
            }
            maybe_line = stderr_rx.recv() => {
                if let Some(line) = maybe_line {
                    if line.contains(SCRIPT_ERROR_MARKER) {
                        if saw_start {
                            push_recent_line(&mut runtime_errors, line.trim().to_string());
                        } else {
                            push_recent_line(&mut pre_start_errors, line.trim().to_string());
                        }
                    }
                    let _ = ev_tx.send(PoolEvent::StderrNoise {
                        worker: id, line,
                    });
                }
            }
            _ = async {
                match (warn_deadline, warn_fired) {
                    (Some(d), false) => tokio::time::sleep_until(d).await,
                    _ => std::future::pending::<()>().await,
                }
            } => {
                warn_fired = true;
                eprintln!(
                    "[gdtest] slow test: {} :: {} (>{}ms)",
                    item.display_script, item.func,
                    timeout_warn_ms.unwrap_or(0),
                );
            }
            _ = tokio::time::sleep_until(deadline) => {
                let name = item.func.clone();
                let msg = format!("timed out after {}ms", timeout_ms);
                // Return as a result (not a crash re-enqueue) so the normal
                // retries budget applies. Worker still needs respawning (crashed=true)
                // but the item is NOT silently re-queued.
                let _ = ev_tx.send(PoolEvent::WorkerCrashed {
                    worker: id,
                    current_file: Some(item.display_script.clone()),
                    message: msg.clone(),
                });
                return (true, Some(PoolEvent::TestError {
                    worker: id, file: item.display_script.clone(), name, message: msg,
                }), stderr_rx);
            }
        }
    }
}

async fn try_respawn(
    worker: &mut Worker,
    cfg: &Config,
    id: usize,
    ev_tx: &mpsc::UnboundedSender<PoolEvent>,
) -> bool {
    match Worker::spawn(cfg) {
        Ok(mut w) => {
            let mut stderr_rx = w.stderr.take().expect("worker stderr channel");
            match wait_for_ready(&mut w, &mut stderr_rx, Duration::from_secs(30), "respawn").await {
                Ok(()) => {
                    w.stderr = Some(stderr_rx);
                }
                Err(message) => {
                    let _ = ev_tx.send(PoolEvent::WorkerCrashed {
                        worker: id,
                        current_file: None,
                        message,
                    });
                    return false;
                }
            }
            *worker = w;
            true
        }
        Err(e) => {
            let _ = ev_tx.send(PoolEvent::WorkerCrashed {
                worker: id,
                current_file: None,
                message: format!("respawn failed: {e}"),
            });
            false
        }
    }
}

async fn wait_for_ready(
    worker: &mut Worker,
    stderr_rx: &mut mpsc::UnboundedReceiver<String>,
    timeout: Duration,
    phase: &str,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut startup_stderr: Vec<String> = Vec::new();
    loop {
        tokio::select! {
            ev = worker.next_event() => {
                match ev {
                    Ok(Event::Ready) => return Ok(()),
                    Ok(_) => {}
                    Err(e) => {
                        return Err(format_ready_failure(
                            phase,
                            format!("{phase} Ready never arrived: {e}"),
                            &startup_stderr,
                            worker.log_path.as_deref(),
                        ));
                    }
                }
            }
            maybe_line = stderr_rx.recv() => {
                if let Some(line) = maybe_line {
                    push_recent_line(&mut startup_stderr, line.trim().to_string());
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                return Err(format_ready_failure(
                    phase,
                    format!(
                        "{phase} Ready timed out after {}s (Godot worker hung during startup)",
                        timeout.as_secs()
                    ),
                    &startup_stderr,
                    worker.log_path.as_deref(),
                ));
            }
        }
    }
}

fn push_recent_line(lines: &mut Vec<String>, line: String) {
    if line.is_empty() {
        return;
    }
    if lines.len() == MAX_DIAGNOSTIC_LINES {
        lines.remove(0);
    }
    lines.push(line);
}

fn format_ready_failure(
    _phase: &str,
    base: String,
    startup_stderr: &[String],
    log_path: Option<&std::path::Path>,
) -> String {
    let mut parts: Vec<String> = vec![base];
    if !startup_stderr.is_empty() {
        parts.push(format!("recent stderr: {}", startup_stderr.join(" | ")));
    }
    if let Some(path) = log_path {
        parts.push(format!("godot log: {}", path.display()));
    }
    parts.join(" | ")
}
