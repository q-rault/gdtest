//! HTML test report with history, flaky detection, and searchable output (internal).
//!
//! Generates a standalone HTML file (no CDN dependencies) after each run.
//! History is stored in `.gdtest-history/` as JSON files — gitignore-able.
//! Flaky tests are detected by comparing pass/fail outcomes across the last N runs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// ── Data types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    Pass,
    Fail,
    Error,
    Skip,
    Pending,
}

impl TestStatus {
    fn css_class(&self) -> &'static str {
        match self {
            TestStatus::Pass => "pass",
            TestStatus::Fail => "fail",
            TestStatus::Error => "error",
            TestStatus::Skip => "skip",
            TestStatus::Pending => "pending",
        }
    }
    fn label(&self) -> &'static str {
        match self {
            TestStatus::Pass => "✓ PASS",
            TestStatus::Fail => "✗ FAIL",
            TestStatus::Error => "! ERR",
            TestStatus::Skip => "- SKIP",
            TestStatus::Pending => "… PEND",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub file: String,
    pub name: String,
    pub status: TestStatus,
    pub duration_us: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunRecord {
    /// Unix timestamp (seconds).
    pub timestamp: u64,
    /// Git commit SHA, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,
    pub results: Vec<TestResult>,
}

// ── History I/O ───────────────────────────────────────────────────────────────

/// Return the path to the `.gdtest-history` directory under `project_abs`.
pub fn history_dir(project_abs: &Path) -> PathBuf {
    project_abs.join(".gdtest-history")
}

/// Write this run's record to `.gdtest-history/run-<timestamp>.json`.
pub fn write_run_record(record: &RunRecord, project_abs: &Path) -> Result<()> {
    let dir = history_dir(project_abs);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(format!("run-{}.json", record.timestamp));
    let json = serde_json::to_string(record)?;
    std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Load the last `max_runs` run records from history, oldest first.
pub fn load_history(project_abs: &Path, max_runs: usize) -> Vec<RunRecord> {
    let dir = history_dir(project_abs);
    if !dir.exists() {
        return Vec::new();
    }

    let mut entries: Vec<(u64, PathBuf)> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("run-"))
        .filter_map(|e| {
            let name = e.file_name();
            let ts: u64 = name
                .to_string_lossy()
                .trim_start_matches("run-")
                .trim_end_matches(".json")
                .parse()
                .ok()?;
            Some((ts, e.path()))
        })
        .collect();

    // Keep only the most recent `max_runs` files (oldest first in the return value).
    entries.sort_by_key(|(ts, _)| *ts);
    let trim_from = entries.len().saturating_sub(max_runs);
    entries = entries.into_iter().skip(trim_from).collect();

    entries
        .into_iter()
        .filter_map(|(_, path)| {
            let content = std::fs::read_to_string(&path).ok()?;
            serde_json::from_str(&content).ok()
        })
        .collect()
}

/// Prune history to keep at most `max_keep` run files.
pub fn prune_history(project_abs: &Path, max_keep: usize) {
    let dir = history_dir(project_abs);
    if !dir.exists() {
        return;
    }
    let mut entries: Vec<(u64, PathBuf)> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("run-"))
        .filter_map(|e| {
            let name = e.file_name();
            let ts: u64 = name
                .to_string_lossy()
                .trim_start_matches("run-")
                .trim_end_matches(".json")
                .parse()
                .ok()?;
            Some((ts, e.path()))
        })
        .collect();

    entries.sort_by_key(|(ts, _)| *ts);
    if entries.len() > max_keep {
        let to_remove = entries.len() - max_keep;
        for (_, path) in entries.into_iter().take(to_remove) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

// ── Flaky detection ───────────────────────────────────────────────────────────

/// A test's flaky score: how often it flipped between pass/fail in `window` runs.
#[derive(Debug)]
pub struct FlakyInfo {
    pub key: String,
    pub flip_count: usize,
    pub run_count: usize,
    /// Fraction of runs where the test failed or errored.
    pub fail_rate: f32,
}

/// Detect flaky tests from the last `window` runs (not including current run).
/// Returns entries sorted by flip_count descending.
pub fn detect_flaky(history: &[RunRecord], window: usize) -> Vec<FlakyInfo> {
    // last `window` runs
    let runs = if history.len() > window {
        &history[history.len() - window..]
    } else {
        history
    };

    // key → [status_per_run]
    let mut outcomes: HashMap<String, Vec<bool>> = HashMap::new();
    for run in runs {
        for r in &run.results {
            let key = format!("{} :: {}", r.file, r.name);
            let passed = matches!(r.status, TestStatus::Pass);
            outcomes.entry(key).or_default().push(passed);
        }
    }

    let mut flaky: Vec<FlakyInfo> = outcomes
        .into_iter()
        .filter_map(|(key, statuses)| {
            if statuses.len() < 2 {
                return None;
            }
            let flip_count = statuses.windows(2).filter(|w| w[0] != w[1]).count();
            if flip_count == 0 {
                return None;
            }
            let fail_count = statuses.iter().filter(|&&p| !p).count();
            Some(FlakyInfo {
                key,
                flip_count,
                run_count: statuses.len(),
                fail_rate: fail_count as f32 / statuses.len() as f32,
            })
        })
        .collect();

    flaky.sort_by(|a, b| b.flip_count.cmp(&a.flip_count));
    flaky
}

// ── HTML generation ───────────────────────────────────────────────────────────

pub struct ReportOptions {
    pub flaky_window: usize,
    pub slow_threshold_ms: u64,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            flaky_window: 20,
            slow_threshold_ms: 5000,
        }
    }
}

/// Generate the full standalone HTML report and write it to `out_path`.
pub fn generate_report(
    current: &RunRecord,
    history: &[RunRecord],
    out_path: &Path,
    opts: &ReportOptions,
) -> Result<()> {
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let html = build_html(current, history, opts);
    std::fs::write(out_path, html.as_bytes())
        .with_context(|| format!("writing {}", out_path.display()))?;
    Ok(())
}

fn build_html(current: &RunRecord, history: &[RunRecord], opts: &ReportOptions) -> String {
    let pass = current
        .results
        .iter()
        .filter(|r| r.status == TestStatus::Pass)
        .count();
    let fail = current
        .results
        .iter()
        .filter(|r| r.status == TestStatus::Fail)
        .count();
    let error = current
        .results
        .iter()
        .filter(|r| r.status == TestStatus::Error)
        .count();
    let skip = current
        .results
        .iter()
        .filter(|r| matches!(r.status, TestStatus::Skip | TestStatus::Pending))
        .count();
    let total = current.results.len();
    let total_us: u64 = current.results.iter().map(|r| r.duration_us).sum();

    let flaky = detect_flaky(history, opts.flaky_window);

    // Build per-run trend: last 10 runs pass rates for the mini trend bar.
    let trend_runs: Vec<(f32, u64)> = history
        .iter()
        .rev()
        .take(10)
        .rev()
        .map(|r| {
            let n = r.results.len();
            let p = r
                .results
                .iter()
                .filter(|t| t.status == TestStatus::Pass)
                .count();
            let t: u64 = r.results.iter().map(|t| t.duration_us).sum::<u64>() / 1_000_000;
            (if n == 0 { 1.0 } else { p as f32 / n as f32 }, t)
        })
        .collect();

    let ts_display = {
        let ts = current.timestamp;
        format!(
            "{}-{:02}-{:02} {:02}:{:02}",
            1970 + ts / 31557600,
            ((ts % 31557600) / 2629800) + 1,
            ((ts % 2629800) / 86400) + 1,
            (ts % 86400) / 3600,
            (ts % 3600) / 60,
        )
    };

    let mut html = String::with_capacity(64 * 1024);

    // ── HEAD ─────────────────────────────────────────────────────────────────
    html.push_str(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>gdtest report</title>
<style>
*{box-sizing:border-box;margin:0;padding:0}
body{font-family:'Segoe UI',system-ui,sans-serif;background:#0d1117;color:#c9d1d9;font-size:14px}
a{color:#58a6ff}
header{background:#161b22;border-bottom:1px solid #30363d;padding:12px 20px;display:flex;align-items:center;gap:16px}
header h1{font-size:18px;font-weight:600;color:#f0f6fc}
.badge{display:inline-flex;align-items:center;gap:4px;padding:3px 10px;border-radius:12px;font-size:12px;font-weight:600}
.badge.pass{background:#1a4731;color:#3fb950}
.badge.fail{background:#4d1a1a;color:#f85149}
.badge.error{background:#3d2a00;color:#d29922}
.badge.skip{background:#21262d;color:#8b949e}
.badge.total{background:#1c2128;color:#c9d1d9}
.summary{display:flex;gap:12px;padding:16px 20px;background:#0d1117;border-bottom:1px solid #21262d;flex-wrap:wrap}
.card{background:#161b22;border:1px solid #30363d;border-radius:8px;padding:12px 18px;min-width:100px;text-align:center}
.card .num{font-size:28px;font-weight:700;line-height:1}
.card .lbl{font-size:11px;color:#8b949e;margin-top:4px}
.card.green .num{color:#3fb950}
.card.red .num{color:#f85149}
.card.orange .num{color:#d29922}
.card.gray .num{color:#8b949e}
.trend{display:flex;align-items:flex-end;gap:2px;height:32px}
.trend-bar{width:8px;background:#238636;border-radius:2px 2px 0 0;transition:opacity .15s}
.section{padding:16px 20px}
.section h2{font-size:14px;font-weight:600;color:#f0f6fc;margin-bottom:10px;display:flex;align-items:center;gap:8px}
.flaky-list{list-style:none;background:#161b22;border:1px solid #30363d;border-radius:6px;overflow:hidden}
.flaky-item{padding:8px 14px;border-bottom:1px solid #21262d;display:flex;gap:10px;align-items:center}
.flaky-item:last-child{border-bottom:none}
.flaky-name{font-family:monospace;font-size:12px;flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.flaky-rate{font-size:11px;color:#d29922;white-space:nowrap}
.controls{padding:8px 20px;display:flex;gap:10px;align-items:center;background:#0d1117;border-bottom:1px solid #21262d}
#search{background:#161b22;border:1px solid #30363d;color:#c9d1d9;padding:6px 10px;border-radius:6px;width:280px;font-size:13px}
#search::placeholder{color:#484f58}
.filter-btn{background:#21262d;border:1px solid #30363d;color:#c9d1d9;padding:5px 12px;border-radius:6px;cursor:pointer;font-size:12px}
.filter-btn.active{background:#1f6feb;border-color:#1f6feb;color:#fff}
table{width:100%;border-collapse:collapse;table-layout:fixed}
thead{background:#161b22;position:sticky;top:0;z-index:1}
th{padding:8px 12px;text-align:left;font-size:11px;font-weight:600;color:#8b949e;border-bottom:1px solid #30363d}
th:first-child{width:80px}
th:last-child{width:90px;text-align:right}
th:nth-child(2){width:45%}
td{padding:7px 12px;border-bottom:1px solid #21262d;font-size:12px;vertical-align:top}
td:first-child{font-size:11px;font-weight:600}
td:last-child{text-align:right;color:#8b949e;font-family:monospace;font-size:11px}
tr.pass td:first-child{color:#3fb950}
tr.fail td:first-child{color:#f85149}
tr.error td:first-child{color:#d29922}
tr.skip td:first-child{color:#8b949e}
tr:hover td{background:#161b22}
.file-cell{color:#8b949e;font-family:monospace;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.name-cell{font-family:monospace}
.failures{margin-top:4px;padding:4px 8px;background:#1a0a0a;border-left:2px solid #f85149;border-radius:0 4px 4px 0}
.fail-line{color:#ffa198;font-size:11px;line-height:1.5}
.slow{color:#d29922}
#results{overflow-x:auto}
</style>
</head>
<body>
"#);

    // ── HEADER ────────────────────────────────────────────────────────────────
    html.push_str(&format!(
        r#"<header>
<h1>gdtest report</h1>
<span style="color:#8b949e;font-size:12px">{ts_display}</span>
"#
    ));
    if let Some(sha) = &current.git_sha {
        let short = if sha.len() > 8 { &sha[..8] } else { sha };
        html.push_str(&format!(
            r#"<span style="color:#8b949e;font-size:12px;font-family:monospace">{short}</span>"#
        ));
    }
    // Trend mini-bar
    if !trend_runs.is_empty() {
        html.push_str(r#"<div class="trend" title="Last runs pass rate">"#);
        for (rate, _) in &trend_runs {
            let h = ((*rate) * 32.0).round() as u32;
            let opacity = (0.4 + rate * 0.6 * 10.0).min(1.0);
            let color = if *rate >= 0.9 {
                "#238636"
            } else if *rate >= 0.5 {
                "#d29922"
            } else {
                "#da3633"
            };
            html.push_str(&format!(
                r#"<div class="trend-bar" style="height:{h}px;background:{color};opacity:{opacity:.2}"></div>"#
            ));
        }
        html.push_str("</div>");
    }
    html.push_str("</header>\n");

    // ── SUMMARY CARDS ────────────────────────────────────────────────────────
    html.push_str(r#"<div class="summary">"#);
    html.push_str(&format!(r#"<div class="card total"><div class="num">{total}</div><div class="lbl">total</div></div>"#));
    html.push_str(&format!(r#"<div class="card green"><div class="num">{pass}</div><div class="lbl">passed</div></div>"#));
    let fail_cls = if fail + error > 0 { "red" } else { "green" };
    html.push_str(&format!(r#"<div class="card {fail_cls}"><div class="num">{}</div><div class="lbl">failed</div></div>"#, fail + error));
    html.push_str(&format!(r#"<div class="card gray"><div class="num">{skip}</div><div class="lbl">skipped</div></div>"#));
    html.push_str(&format!(
        r#"<div class="card gray"><div class="num">{}</div><div class="lbl">duration</div></div>"#,
        fmt_us(total_us)
    ));
    html.push_str("</div>\n");

    // ── FLAKY SECTION ────────────────────────────────────────────────────────
    if !flaky.is_empty() {
        html.push_str(r#"<div class="section"><h2>⚠ Flaky tests <span class="badge fail">"#);
        html.push_str(&flaky.len().to_string());
        html.push_str(r#"</span></h2><ul class="flaky-list">"#);
        for fi in flaky.iter().take(10) {
            let esc = html_escape(&fi.key);
            html.push_str(&format!(
                r#"<li class="flaky-item"><span class="flaky-name" title="{esc}">{esc}</span><span class="flaky-rate">{} flips / {} runs ({:.0}% fail rate)</span></li>"#,
                fi.flip_count, fi.run_count, fi.fail_rate * 100.0
            ));
        }
        html.push_str("</ul></div>\n");
    }

    // ── CONTROLS ─────────────────────────────────────────────────────────────
    html.push_str(
        r#"<div class="controls">
<input id="search" type="text" placeholder="Filter by name, file…" oninput="applyFilters()">
<button class="filter-btn active" onclick="setStatus('all',this)">All</button>
<button class="filter-btn" onclick="setStatus('fail',this)">Failed</button>
<button class="filter-btn" onclick="setStatus('pass',this)">Passed</button>
<button class="filter-btn" onclick="setStatus('skip',this)">Skipped</button>
</div>
"#,
    );

    // ── RESULTS TABLE ────────────────────────────────────────────────────────
    html.push_str(
        r#"<div id="results"><table><thead><tr>
<th>Status</th><th>Test</th><th>File</th><th>Time</th>
</tr></thead><tbody id="tbody">
"#,
    );

    for r in &current.results {
        let cls = r.status.css_class();
        let label = r.status.label();
        let name_esc = html_escape(&r.name);
        let file_esc = html_escape(&r.file);
        let anchor = make_anchor(&r.file, &r.name);
        let time_cls = if r.duration_us > opts.slow_threshold_ms * 1000 {
            " slow"
        } else {
            ""
        };
        let time_str = fmt_us(r.duration_us);

        html.push_str(&format!(
            r##"<tr class="{cls}" id="{anchor}" data-name="{name_esc}" data-file="{file_esc}" data-status="{cls}">
<td>{label}</td>
<td><div class="name-cell"><a href="#{anchor}">{name_esc}</a>"##
        ));
        if !r.failures.is_empty() {
            html.push_str(r#"<div class="failures">"#);
            for f in &r.failures {
                html.push_str(&format!(
                    r#"<div class="fail-line">{}</div>"#,
                    html_escape(f)
                ));
            }
            html.push_str("</div>");
        }
        html.push_str(&format!(
            r#"</div></td>
<td class="file-cell" title="{file_esc}">{file_esc}</td>
<td class="{time_cls}">{time_str}</td>
</tr>
"#
        ));
    }

    html.push_str("</tbody></table></div>\n");

    // ── INLINE JS ────────────────────────────────────────────────────────────
    html.push_str(
        r#"<script>
var currentStatus='all';
function setStatus(s,btn){
  currentStatus=s;
  document.querySelectorAll('.filter-btn').forEach(b=>b.classList.remove('active'));
  btn.classList.add('active');
  applyFilters();
}
function applyFilters(){
  var q=document.getElementById('search').value.toLowerCase();
  document.querySelectorAll('#tbody tr').forEach(function(row){
    var name=row.dataset.name||'';
    var file=row.dataset.file||'';
    var status=row.dataset.status||'';
    var matchText=!q||name.toLowerCase().includes(q)||file.toLowerCase().includes(q);
    var matchStatus=currentStatus==='all'||
      (currentStatus==='fail'&&(status==='fail'||status==='error'))||
      (currentStatus==='pass'&&status==='pass')||
      (currentStatus==='skip'&&(status==='skip'||status==='pending'));
    row.style.display=(matchText&&matchStatus)?'':'none';
  });
}
</script>
</body></html>"#,
    );

    html
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn make_anchor(file: &str, name: &str) -> String {
    let combined = format!("{file}::{name}");
    combined
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn fmt_us(us: u64) -> String {
    if us < 1_000 {
        format!("{us}µs")
    } else if us < 1_000_000 {
        format!("{:.1}ms", us as f64 / 1_000.0)
    } else {
        format!("{:.2}s", us as f64 / 1_000_000.0)
    }
}

pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Try to read the current git HEAD SHA (first 40 chars).
pub fn git_sha(project_abs: &Path) -> Option<String> {
    let head = project_abs.join(".git").join("HEAD");
    let content = std::fs::read_to_string(&head).ok()?;
    let content = content.trim();
    if content.starts_with("ref: ") {
        let ref_path = content.trim_start_matches("ref: ");
        let ref_file = project_abs.join(".git").join(ref_path);
        let sha = std::fs::read_to_string(ref_file).ok()?;
        Some(sha.trim().to_string())
    } else if content.len() >= 40 {
        Some(content[..40].to_string())
    } else {
        None
    }
}
