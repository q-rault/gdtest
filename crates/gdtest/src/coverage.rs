//! Coverage aggregation, LCOV generation, and terminal summary.
//!
//! Workers write `.gdtest_cov_<pid>.json` files in the project root when
//! `--coverage` is active. This module collects those files, merges hit counts
//! across parallel workers, generates an LCOV report, and prints a terminal
//! summary with an optional minimum-coverage gate.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;

/// File → {line → hit count}.
pub type CoverageMap = HashMap<String, HashMap<u32, u32>>;

/// File → set of executable line numbers (the "lines found" universe for LCOV).
pub type LinesFound = HashMap<String, HashSet<u32>>;

// ── Collection ────────────────────────────────────────────────────────────────

/// Read and merge all `.gdtest_cov_*.json` files in `project_abs`, then delete them.
pub fn collect_from_project(project_abs: &Path) -> Result<CoverageMap> {
    let mut merged: CoverageMap = HashMap::new();

    let read_dir = std::fs::read_dir(project_abs)
        .with_context(|| format!("reading {}", project_abs.display()))?;

    for entry in read_dir {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(".gdtest_cov_") || !name.ends_with(".json") {
            continue;
        }
        let path = entry.path();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("gdtest: coverage: could not read {}: {e}", path.display());
                continue;
            }
        };

        let data: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("gdtest: coverage: malformed {}: {e}", path.display());
                std::fs::remove_file(&path).ok();
                continue;
            }
        };

        if let Some(obj) = data.as_object() {
            for (file, lines_val) in obj {
                let file_map = merged.entry(file.clone()).or_default();
                if let Some(lines_obj) = lines_val.as_object() {
                    for (line_str, count_val) in lines_obj {
                        if let (Ok(line), Some(count)) =
                            (line_str.parse::<u32>(), count_val.as_u64())
                        {
                            *file_map.entry(line).or_insert(0) += count as u32;
                        }
                    }
                }
            }
        }
        std::fs::remove_file(&path).ok();
    }
    Ok(merged)
}

// ── LCOV generation ───────────────────────────────────────────────────────────

/// Write coverage data in LCOV format to `out_path`.
/// `lines_found` maps each file to its full set of instrumentable lines;
/// lines present in `lines_found` but absent from `cov` are treated as DA=0.
pub fn write_lcov(cov: &CoverageMap, lines_found: &LinesFound, out_path: &Path) -> Result<()> {
    if let Some(parent) = out_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut f = std::io::BufWriter::new(
        std::fs::File::create(out_path)
            .with_context(|| format!("creating {}", out_path.display()))?,
    );

    // Collect all files (union of cov keys and lines_found keys).
    let mut files: Vec<&String> = {
        let mut set: std::collections::HashSet<&String> = cov.keys().collect();
        set.extend(lines_found.keys());
        let mut v: Vec<&String> = set.into_iter().collect();
        v.sort();
        v
    };
    // Sort ascending for deterministic output
    files.sort();

    for file in &files {
        let hits = cov.get(*file);
        let lf_set = lines_found.get(*file);
        let found_count = lf_set.map(|s| s.len()).unwrap_or(0);

        // Collect all lines: those with hits + those in lf_set with 0
        let mut da_map: HashMap<u32, u32> = HashMap::new();
        if let Some(lf) = lf_set {
            for &l in lf {
                da_map.entry(l).or_insert(0);
            }
        }
        if let Some(h) = hits {
            for (&l, &c) in h {
                *da_map.entry(l).or_insert(0) += c;
            }
        }

        let hit_count = da_map.values().filter(|&&c| c > 0).count();

        writeln!(f, "TN:")?;
        writeln!(f, "SF:{file}")?;

        let mut sorted_lines: Vec<(u32, u32)> = da_map.into_iter().collect();
        sorted_lines.sort_by_key(|(l, _)| *l);
        for (line, count) in sorted_lines {
            writeln!(f, "DA:{line},{count}")?;
        }

        writeln!(f, "LH:{hit_count}")?;
        writeln!(f, "LF:{found_count}")?;
        writeln!(f, "end_of_record")?;
    }
    Ok(())
}

// ── Terminal summary ──────────────────────────────────────────────────────────

/// Print per-file and aggregate coverage to stderr.
/// Returns `Err` if `min_pct` is set and total coverage is below it.
pub fn print_summary(
    cov: &CoverageMap,
    lines_found: &LinesFound,
    min_pct: Option<f32>,
) -> Result<()> {
    eprintln!("\nCoverage summary:");

    let mut all_files: Vec<&String> = {
        let mut set: std::collections::HashSet<&String> = cov.keys().collect();
        set.extend(lines_found.keys());
        let mut v: Vec<&String> = set.into_iter().collect();
        v.sort();
        v
    };
    all_files.sort();

    let mut total_found: usize = 0;
    let mut total_hit: usize = 0;

    for file in &all_files {
        let lf = lines_found.get(*file).map(|s| s.len()).unwrap_or(0);
        let lh = cov
            .get(*file)
            .map(|h| h.values().filter(|&&c| c > 0).count())
            .unwrap_or(0);
        total_found += lf;
        total_hit += lh;

        let pct = if lf == 0 {
            100.0_f32
        } else {
            lh as f32 / lf as f32 * 100.0
        };
        let pct_str = format!("{pct:.1}%");
        let colored = if pct >= 80.0 {
            pct_str.green().to_string()
        } else if pct >= 50.0 {
            pct_str.yellow().to_string()
        } else {
            pct_str.red().to_string()
        };

        // Trim long paths from the left so they fit in the column.
        let display: &str = if file.len() > 55 {
            &file[file.len() - 55..]
        } else {
            file
        };
        eprintln!("  {display:<57}  {lh:>4}/{lf:<4}  ({colored})");
    }

    let total_pct = if total_found == 0 {
        100.0_f32
    } else {
        total_hit as f32 / total_found as f32 * 100.0
    };
    let sep = "─".repeat(74);
    eprintln!("  {sep}");
    let total_pct_str = format!("{total_pct:.1}%");
    eprintln!(
        "  {:<57}  {:>4}/{:<4}  ({total_pct_str})",
        "Total", total_hit, total_found
    );

    if let Some(min) = min_pct
        && total_pct < min
    {
        anyhow::bail!("coverage {total_pct:.1}% is below minimum {min:.1}%");
    }
    Ok(())
}

// ── Glob-pattern filtering ────────────────────────────────────────────────────

/// Returns true if `path` (forward-slash-separated) matches `pattern`.
/// Supports `*` (within one path component) and `**` (across components).
pub fn glob_match(pattern: &str, path: &str) -> bool {
    let pats: Vec<&str> = pattern.split('/').collect();
    let segs: Vec<&str> = path.split('/').collect();
    glob_parts(&pats, &segs)
}

fn glob_parts(pats: &[&str], segs: &[&str]) -> bool {
    match pats.first() {
        None => segs.is_empty(),
        Some(&"**") => {
            // Match zero or more segments.
            for i in 0..=segs.len() {
                if glob_parts(&pats[1..], &segs[i..]) {
                    return true;
                }
            }
            false
        }
        Some(pat) => {
            if segs.is_empty() {
                return false;
            }
            component_match(pat, segs[0]) && glob_parts(&pats[1..], &segs[1..])
        }
    }
}

fn component_match(pat: &str, seg: &str) -> bool {
    if pat == "*" {
        return true;
    }
    if !pat.contains('*') {
        return pat == seg;
    }
    // Split on '*' and match as prefix+suffix.
    let parts: Vec<&str> = pat.splitn(2, '*').collect();
    if parts.len() != 2 {
        return pat == seg;
    }
    seg.starts_with(parts[0])
        && seg.ends_with(parts[1])
        && seg.len() >= parts[0].len() + parts[1].len()
}
