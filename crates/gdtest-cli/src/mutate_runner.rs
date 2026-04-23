//! Mutation testing orchestration (internal).
//!
//! For each mutation in a target GDScript file:
//!   1. Backup original → write mutant in-place
//!   2. Run the test pool (fail-fast, silent reporter)
//!   3. Restore original
//!   4. Record killed (any failure/error/crash) vs survived (all passed)

use std::path::PathBuf;

use anyhow::{Context as _, Result};

use crate::{pool, reporter, worker};

pub struct MutOpts {
    pub cfg: worker::Config,
    pub jobs: usize,
    pub isolation: pool::IsolationMode,
    pub timeout_ms: u64,
}

pub struct MutationReport {
    pub total: usize,
    pub killed: usize,
    /// Human-readable descriptions of surviving mutations.
    pub survived: Vec<String>,
}

/// Run mutation testing against `mutate_file` using `items` as the test suite.
pub async fn run(
    mutate_file: &str,
    items: Vec<pool::TestItem>,
    opts: MutOpts,
) -> Result<MutationReport> {
    let project_abs = PathBuf::from(&opts.cfg.project_path)
        .canonicalize()
        .context("project path")?;

    let res_path = if mutate_file.starts_with("res://") {
        mutate_file.to_string()
    } else {
        let p = PathBuf::from(mutate_file);
        let abs = if p.is_absolute() {
            p
        } else {
            project_abs.join(p)
        };
        gdtest_discovery::to_res_path(&abs, &project_abs)?
    };
    let abs_path = gdtest_discovery::res_to_abs(&res_path, &project_abs)?;

    let source =
        std::fs::read(&abs_path).with_context(|| format!("reading {}", abs_path.display()))?;

    let mutations = gdtest_discovery::mutate::find_mutations(&source);
    let total = mutations.len();

    if total == 0 {
        eprintln!("gdtest mutate: no mutations found in {res_path}");
        return Ok(MutationReport {
            total: 0,
            killed: 0,
            survived: Vec::new(),
        });
    }

    eprintln!("gdtest mutate: {total} mutations in {res_path}");

    let mut killed = 0usize;
    let mut survived: Vec<String> = Vec::new();

    for (idx, mutation) in mutations.iter().enumerate() {
        let desc = mutation.description();
        eprint!(
            "  [{:>width$}/{}] {} … ",
            idx + 1,
            total,
            desc,
            width = total.to_string().len()
        );

        let mutated = mutation.apply(&source);
        let backup = abs_path.with_extension("gd.mut_bak");

        // Write backup then mutant
        std::fs::write(&backup, &source)
            .with_context(|| format!("writing backup {}", backup.display()))?;
        if let Err(e) = std::fs::write(&abs_path, &mutated) {
            std::fs::remove_file(&backup).ok();
            return Err(e).with_context(|| format!("writing mutant {}", abs_path.display()));
        }

        let pool_opts = pool::PoolOptions {
            jobs: opts.jobs.min(items.len()).max(1),
            cfg: opts.cfg.clone(),
            reporter: reporter::Reporter::Silent,
            isolation: opts.isolation,
            timeout_ms: opts.timeout_ms,
            fail_fast: true,
            timeout_warn_ms: None,
            retries: 0,
        };

        let result = pool::run(items.clone(), pool_opts).await;

        // Restore original unconditionally before inspecting result
        if let Err(e) = std::fs::write(&abs_path, &source) {
            eprintln!(
                "\ngdtest mutate: FATAL: could not restore {}: {e}",
                abs_path.display()
            );
            std::fs::remove_file(&backup).ok();
            return Err(anyhow::anyhow!("source restore failed: {e}"));
        }
        std::fs::remove_file(&backup).ok();

        match result {
            Ok((summary, _)) if summary.fail > 0 || summary.error > 0 => {
                eprintln!("KILLED");
                killed += 1;
            }
            Ok(_) => {
                eprintln!("survived");
                survived.push(desc);
            }
            Err(_) => {
                // Worker crash counts as killed — mutant broke the harness
                eprintln!("KILLED (crash)");
                killed += 1;
            }
        }
    }

    Ok(MutationReport {
        total,
        killed,
        survived,
    })
}
