//! Reporters: Pretty (default), TAP, JSON.
//!
//! All three consume the same `PoolEvent` stream from `pool.rs`. Pretty
//! streams human-readable lines, TAP buffers and emits canonical TAP v14
//! at the end, JSON streams one ndjson record per event for downstream
//! tooling (custom CI scripts, the planned VSCode extension internal).

use std::io::{self, Write};

use owo_colors::OwoColorize;
use serde::Serialize;

use crate::pool::PoolEvent;

#[derive(clap::ValueEnum, serde::Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ReporterKind {
    Pretty,
    Tap,
    Json,
    Junit,
}

pub enum Reporter {
    Pretty(PrettyReporter),
    Tap(TapReporter),
    Json(JsonReporter),
    Junit(JunitReporter),
    /// Suppresses all output — used by mutation testing inner loops.
    Silent,
}

impl Reporter {
    pub fn new(kind: ReporterKind) -> Self {
        match kind {
            ReporterKind::Pretty => Reporter::Pretty(PrettyReporter::default()),
            ReporterKind::Tap => Reporter::Tap(TapReporter::default()),
            ReporterKind::Json => Reporter::Json(JsonReporter::default()),
            ReporterKind::Junit => Reporter::Junit(JunitReporter::default()),
        }
    }

    /// Like `new`, but the JSON reporter writes each event as a numbered file
    /// under `dir/` instead of stdout. Non-JSON reporters are unaffected.
    pub fn new_to_dir(kind: ReporterKind, dir: &std::path::Path) -> std::io::Result<Self> {
        match kind {
            ReporterKind::Json => Ok(Reporter::Json(JsonReporter::to_dir(dir)?)),
            _ => Ok(Self::new(kind)),
        }
    }

    pub fn set_total(&mut self, n: u32) {
        if let Reporter::Pretty(r) = self {
            r.set_total(n);
        }
    }

    pub fn on_event(&mut self, ev: &PoolEvent) {
        match self {
            Reporter::Pretty(r) => r.on_event(ev),
            Reporter::Tap(r) => r.on_event(ev),
            Reporter::Json(r) => r.on_event(ev),
            Reporter::Junit(r) => r.on_event(ev),
            Reporter::Silent => {}
        }
    }

    pub fn finish(&mut self, summary: &Summary) {
        match self {
            Reporter::Pretty(r) => r.finish(summary),
            Reporter::Tap(r) => r.finish(summary),
            Reporter::Json(r) => r.finish(summary),
            Reporter::Junit(r) => r.finish(summary),
            Reporter::Silent => {}
        }
    }

    /// Advance the spinner animation. Only has an effect in Pretty mode.
    pub fn tick(&mut self) {
        if let Reporter::Pretty(r) = self {
            r.tick();
        }
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub struct Summary {
    pub pass: u32,
    pub fail: u32,
    pub error: u32,
    pub pending: u32,
    pub skipped: u32,
    pub worker_crashes: u32,
}

impl Summary {
    pub fn apply(&mut self, ev: &PoolEvent) {
        match ev {
            PoolEvent::TestPass { .. } => self.pass += 1,
            PoolEvent::TestFail { .. } => self.fail += 1,
            PoolEvent::TestError { .. } => self.error += 1,
            PoolEvent::TestPending { .. } => self.pending += 1,
            PoolEvent::TestSkip { .. } => self.skipped += 1,
            PoolEvent::WorkerCrashed { .. } => self.worker_crashes += 1,
            _ => {}
        }
    }

    pub fn exit_code(&self) -> i32 {
        if self.fail == 0 && self.error == 0 && self.worker_crashes == 0 {
            0
        } else {
            1
        }
    }
}

// ── Pretty ───────────────────────────────────────────────────────────────

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct PrettyReporter {
    total: u32,
    completed: u32,
    pass: u32,
    fail: u32,
    error: u32,
    is_terminal: bool,
    spinner_frame: usize,
}

impl Default for PrettyReporter {
    fn default() -> Self {
        Self {
            total: 0,
            completed: 0,
            pass: 0,
            fail: 0,
            error: 0,
            is_terminal: std::io::IsTerminal::is_terminal(&std::io::stdout()),
            spinner_frame: 0,
        }
    }
}

impl PrettyReporter {
    pub fn set_total(&mut self, n: u32) {
        self.total = n;
    }

    fn erase_progress(&self, stdout: &mut impl Write) {
        if self.is_terminal && self.total > 0 {
            let _ = write!(stdout, "\r\x1b[2K");
        }
    }

    fn draw_progress(&self, stdout: &mut impl Write) {
        if !self.is_terminal || self.total == 0 {
            return;
        }
        let spinner = SPINNER[self.spinner_frame % SPINNER.len()];
        let _ = write!(
            stdout,
            "\r{} {}/{} ",
            spinner.dimmed(),
            self.completed,
            self.total,
        );
        if self.pass > 0 {
            let _ = write!(stdout, "{} ", format!("{} ✓", self.pass).green());
        }
        if self.fail > 0 {
            let _ = write!(stdout, "{} ", format!("{} ✗", self.fail).red());
        }
        if self.error > 0 {
            let _ = write!(stdout, "{} ", format!("{} !", self.error).yellow());
        }
        let _ = stdout.flush();
    }

    pub fn tick(&mut self) {
        if !self.is_terminal || self.total == 0 {
            return;
        }
        self.spinner_frame = self.spinner_frame.wrapping_add(1);
        let mut stdout = io::stdout().lock();
        self.draw_progress(&mut stdout);
    }

    fn on_event(&mut self, ev: &PoolEvent) {
        let mut stdout = io::stdout().lock();
        self.erase_progress(&mut stdout);
        match ev {
            PoolEvent::TestPass { file, name, us, .. } => {
                self.completed += 1;
                self.pass += 1;
                let _ = writeln!(
                    stdout,
                    "{} {} :: {name} ({})",
                    "✓".green(),
                    file.dimmed(),
                    fmt_us(*us),
                );
            }
            PoolEvent::TestFail {
                file,
                name,
                us,
                failures,
                ..
            } => {
                self.completed += 1;
                self.fail += 1;
                let _ = writeln!(
                    stdout,
                    "{} {} :: {name} ({})",
                    "✗".red(),
                    file.dimmed(),
                    fmt_us(*us),
                );
                for f in failures {
                    let mut lines = f.lines();
                    if let Some(first) = lines.next() {
                        let _ = writeln!(stdout, "    {} {first}", "→".red());
                        for rest in lines {
                            let _ = writeln!(stdout, "      {rest}");
                        }
                    }
                }
            }
            PoolEvent::TestError {
                file,
                name,
                message,
                ..
            } => {
                self.completed += 1;
                self.error += 1;
                let _ = writeln!(
                    stdout,
                    "{} {} :: {name}: {message}",
                    "!".yellow(),
                    file.dimmed(),
                );
            }
            PoolEvent::TestPending {
                file,
                name,
                message,
                ..
            } => {
                self.completed += 1;
                let _ = writeln!(
                    stdout,
                    "{} {} :: {name} (pending: {message})",
                    "?".cyan(),
                    file.dimmed(),
                );
            }
            PoolEvent::TestSkip { file, name, .. } => {
                self.completed += 1;
                let _ = writeln!(stdout, "{} {} :: {name}", "s".dimmed(), file.dimmed(),);
            }
            PoolEvent::WorkerCrashed {
                worker,
                current_file,
                message,
            } => {
                let file = current_file.as_deref().unwrap_or("<none>");
                let _ = writeln!(
                    stdout,
                    "{} worker {worker} crashed while running {file}: {message}",
                    "✗".red()
                );
            }
            _ => {}
        }
        self.draw_progress(&mut stdout);
    }

    fn finish(&mut self, s: &Summary) {
        let mut stdout = io::stdout().lock();
        self.erase_progress(&mut stdout);
        drop(stdout);

        println!();
        let total = s.pass + s.fail + s.error + s.pending + s.skipped;
        print!("{} tests  ", total.bold());
        if s.pass > 0 {
            print!("{} ", format!("{} passed", s.pass).green());
        }
        if s.fail > 0 {
            print!("{} ", format!("{} failed", s.fail).red());
        }
        if s.error > 0 {
            print!("{} ", format!("{} errored", s.error).yellow());
        }
        if s.pending > 0 {
            print!("{} ", format!("{} pending", s.pending).cyan());
        }
        if s.skipped > 0 {
            print!("{} ", format!("{} skipped", s.skipped).dimmed());
        }
        if s.worker_crashes > 0 {
            print!("{} ", format!("{} worker crashes", s.worker_crashes).red());
        }
        println!();
    }
}

fn fmt_us(us: u64) -> String {
    if us < 1_000 {
        format!("{us} µs")
    } else if us < 1_000_000 {
        format!("{:.2} ms", us as f64 / 1_000.0)
    } else {
        format!("{:.2} s", us as f64 / 1_000_000.0)
    }
}

// ── TAP v14 ───────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct TapReporter {
    header_emitted: bool,
    buffered: Vec<PoolEvent>,
}

impl TapReporter {
    fn on_event(&mut self, ev: &PoolEvent) {
        // TAP requires `1..N` upfront. We don't know N until discovery + run
        // complete (some tests may error-out a whole file). Buffer events and
        // flush on finish() with the real plan line.
        self.buffered.push(ev.clone());
    }

    fn finish(&mut self, s: &Summary) {
        let total = s.pass + s.fail + s.error;
        if !self.header_emitted {
            println!("TAP version 14");
            println!("1..{}", total);
            self.header_emitted = true;
        }
        // Stable ordering: iterate events in the order we saw them.
        let events = std::mem::take(&mut self.buffered);
        let mut n: u32 = 0;
        for ev in &events {
            match ev {
                PoolEvent::TestPass { name, us, file, .. } => {
                    n += 1;
                    println!("ok {n} - {file} :: {name} # {} µs", us);
                }
                PoolEvent::TestFail {
                    name,
                    us,
                    failures,
                    file,
                    ..
                } => {
                    n += 1;
                    println!("not ok {n} - {file} :: {name} # {} µs", us);
                    println!("  ---");
                    println!("  failures:");
                    for f in failures {
                        println!("    - {}", tap_escape(f));
                    }
                    println!("  ...");
                }
                PoolEvent::TestError {
                    name,
                    message,
                    file,
                    ..
                } => {
                    n += 1;
                    println!("not ok {n} - {file} :: {name} # ERROR");
                    println!("  ---");
                    println!("  error: {}", tap_escape(message));
                    println!("  ...");
                }
                PoolEvent::TestPending {
                    name,
                    message,
                    file,
                    ..
                } => {
                    n += 1;
                    println!(
                        "ok {n} - {file} :: {name} # SKIP pending: {}",
                        tap_escape(message)
                    );
                }
                PoolEvent::TestSkip { name, file, .. } => {
                    n += 1;
                    println!("ok {n} - {file} :: {name} # SKIP");
                }
                PoolEvent::WorkerCrashed {
                    worker,
                    current_file,
                    message,
                } => {
                    let file = current_file.as_deref().unwrap_or("<none>");
                    println!(
                        "# worker {worker} crashed in {file}: {}",
                        tap_escape(message)
                    );
                }
                _ => {}
            }
        }
        println!(
            "# pass={} fail={} error={} pending={} skipped={} worker_crashes={}",
            s.pass, s.fail, s.error, s.pending, s.skipped, s.worker_crashes
        );
    }
}

fn tap_escape(s: &str) -> String {
    s.replace('\n', "\\n")
}

// ── JSON (ndjson) ─────────────────────────────────────────────────────────

/// Streams one JSON object per line per pool event, ending with a `summary`
/// record. Stable schema for downstream tooling: `kind` discriminator
/// matches `PoolEvent` variant in snake_case.
pub struct JsonReporter {
    sink: JsonSink,
}

enum JsonSink {
    Stdout,
    /// Each event is written to its own numbered file (`<dir>/NNNNNNNN.json`).
    /// The file is created, written, and closed before Godot ever sees it in
    /// the directory listing, so there are zero Windows file-sharing conflicts.
    Dir {
        path: std::path::PathBuf,
        seq: u32,
    },
}

impl Default for JsonReporter {
    fn default() -> Self {
        Self {
            sink: JsonSink::Stdout,
        }
    }
}

impl JsonReporter {
    pub fn to_dir(dir: &std::path::Path) -> std::io::Result<Self> {
        if dir.exists() {
            for entry in std::fs::read_dir(dir)? {
                let p = entry?.path();
                if p.extension().and_then(|e| e.to_str()) == Some("json") {
                    let _ = std::fs::remove_file(p);
                }
            }
        } else {
            std::fs::create_dir_all(dir)?;
        }
        Ok(Self {
            sink: JsonSink::Dir {
                path: dir.to_owned(),
                seq: 0,
            },
        })
    }

    fn write_line(&mut self, line: &str) {
        match &mut self.sink {
            JsonSink::Stdout => {
                let mut out = io::stdout().lock();
                let _ = writeln!(out, "{line}");
                let _ = out.flush();
            }
            JsonSink::Dir { path, seq } => {
                *seq += 1;
                let file_path = path.join(format!("{seq:08}.json"));
                if let Ok(mut f) = std::fs::File::create(&file_path) {
                    let _ = writeln!(f, "{line}");
                }
            }
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum JsonEvent<'a> {
    TestStart {
        worker: usize,
        file: &'a str,
        name: &'a str,
    },
    TestPass {
        worker: usize,
        file: &'a str,
        name: &'a str,
        us: u64,
    },
    TestFail {
        worker: usize,
        file: &'a str,
        name: &'a str,
        us: u64,
        failures: &'a [String],
    },
    TestError {
        worker: usize,
        file: &'a str,
        name: &'a str,
        message: &'a str,
    },
    TestPending {
        worker: usize,
        file: &'a str,
        name: &'a str,
        message: &'a str,
    },
    TestSkip {
        worker: usize,
        file: &'a str,
        name: &'a str,
        message: &'a str,
    },
    WorkerCrashed {
        worker: usize,
        current_file: Option<&'a str>,
        message: &'a str,
    },
    StderrNoise {
        worker: usize,
        line: &'a str,
    },
}

#[derive(Serialize)]
struct JsonSummary {
    kind: &'static str,
    pass: u32,
    fail: u32,
    error: u32,
    pending: u32,
    skipped: u32,
    worker_crashes: u32,
}

impl JsonReporter {
    fn on_event(&mut self, ev: &PoolEvent) {
        let mapped = match ev {
            PoolEvent::TestStart { worker, file, name } => Some(JsonEvent::TestStart {
                worker: *worker,
                file,
                name,
            }),
            PoolEvent::TestPass {
                worker,
                file,
                name,
                us,
            } => Some(JsonEvent::TestPass {
                worker: *worker,
                file,
                name,
                us: *us,
            }),
            PoolEvent::TestFail {
                worker,
                file,
                name,
                us,
                failures,
            } => Some(JsonEvent::TestFail {
                worker: *worker,
                file,
                name,
                us: *us,
                failures,
            }),
            PoolEvent::TestError {
                worker,
                file,
                name,
                message,
            } => Some(JsonEvent::TestError {
                worker: *worker,
                file,
                name,
                message,
            }),
            PoolEvent::TestPending {
                worker,
                file,
                name,
                message,
            } => Some(JsonEvent::TestPending {
                worker: *worker,
                file,
                name,
                message,
            }),
            PoolEvent::TestSkip {
                worker,
                file,
                name,
                message,
            } => Some(JsonEvent::TestSkip {
                worker: *worker,
                file,
                name,
                message,
            }),
            PoolEvent::WorkerCrashed {
                worker,
                current_file,
                message,
            } => Some(JsonEvent::WorkerCrashed {
                worker: *worker,
                current_file: current_file.as_deref(),
                message,
            }),
            PoolEvent::StderrNoise { worker, line } => Some(JsonEvent::StderrNoise {
                worker: *worker,
                line,
            }),
        };
        if let Some(e) = mapped {
            if let Ok(line) = serde_json::to_string(&e) {
                self.write_line(&line);
            }
        }
    }

    fn finish(&mut self, s: &Summary) {
        let summary = JsonSummary {
            kind: "summary",
            pass: s.pass,
            fail: s.fail,
            error: s.error,
            pending: s.pending,
            skipped: s.skipped,
            worker_crashes: s.worker_crashes,
        };
        if let Ok(line) = serde_json::to_string(&summary) {
            self.write_line(&line);
        }
    }
}

// ── JUnit XML ─────────────────────────────────────────────────────────────

struct JunitCase {
    classname: String,
    name: String,
    time_s: f64,
    failure: Option<String>,
}

#[derive(Default)]
pub struct JunitReporter {
    cases: Vec<JunitCase>,
    total_time_s: f64,
}

impl JunitReporter {
    fn on_event(&mut self, ev: &PoolEvent) {
        match ev {
            PoolEvent::TestPass { file, name, us, .. } => {
                let time_s = *us as f64 / 1_000_000.0;
                self.total_time_s += time_s;
                self.cases.push(JunitCase {
                    classname: file.clone(),
                    name: name.clone(),
                    time_s,
                    failure: None,
                });
            }
            PoolEvent::TestFail {
                file,
                name,
                us,
                failures,
                ..
            } => {
                let time_s = *us as f64 / 1_000_000.0;
                self.total_time_s += time_s;
                self.cases.push(JunitCase {
                    classname: file.clone(),
                    name: name.clone(),
                    time_s,
                    failure: Some(failures.join("\n")),
                });
            }
            PoolEvent::TestError {
                file,
                name,
                message,
                ..
            } => {
                self.cases.push(JunitCase {
                    classname: file.clone(),
                    name: name.clone(),
                    time_s: 0.0,
                    failure: Some(format!("error: {message}")),
                });
            }
            PoolEvent::TestPending {
                file,
                name,
                message,
                ..
            } => {
                self.cases.push(JunitCase {
                    classname: file.clone(),
                    name: name.clone(),
                    time_s: 0.0,
                    failure: Some(format!("pending: {message}")),
                });
            }
            PoolEvent::TestSkip { file, name, .. } => {
                self.cases.push(JunitCase {
                    classname: file.clone(),
                    name: name.clone(),
                    time_s: 0.0,
                    failure: None,
                });
            }
            _ => {}
        }
    }

    fn finish(&mut self, s: &Summary) {
        let mut out = io::stdout().lock();
        let total = s.pass + s.fail + s.error;
        let _ = writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        let _ = writeln!(out, r#"<testsuites>"#);
        let _ = writeln!(
            out,
            r#"  <testsuite name="gdtest" tests="{}" failures="{}" errors="{}" time="{:.3}">"#,
            total, s.fail, s.error, self.total_time_s,
        );
        for c in &self.cases {
            let classname = xml_escape(&c.classname);
            let name = xml_escape(&c.name);
            if let Some(msg) = &c.failure {
                let _ = writeln!(
                    out,
                    r#"    <testcase classname="{classname}" name="{name}" time="{:.6}">"#,
                    c.time_s,
                );
                let _ = writeln!(out, r#"      <failure message="{}"/>"#, xml_escape(msg));
                let _ = writeln!(out, r#"    </testcase>"#);
            } else {
                let _ = writeln!(
                    out,
                    r#"    <testcase classname="{classname}" name="{name}" time="{:.6}"/>"#,
                    c.time_s,
                );
            }
        }
        let _ = writeln!(out, r#"  </testsuite>"#);
        let _ = writeln!(out, r#"</testsuites>"#);
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
