use anyhow::{anyhow, Result};
use gdtest_protocol::{Command, Event};
use owo_colors::OwoColorize;

use crate::worker::{Config, Worker};

/// `script::func` — exactly one test.
pub fn single(target: &str, cfg: &Config) -> Result<()> {
    let (script, func) = parse_target(target)?;
    run_single(script, func, cfg)
}

fn parse_target(target: &str) -> Result<(&str, &str)> {
    target
        .split_once("::")
        .filter(|(s, f)| !s.is_empty() && !f.is_empty())
        .ok_or_else(|| anyhow!("target must be `script::func`, got `{target}`"))
}

#[tokio::main(flavor = "current_thread")]
async fn run_single(script: &str, func: &str, cfg: &Config) -> Result<()> {
    let mut w = Worker::spawn(cfg)?;
    w.drain_stderr_to_log();
    w.wait_for(|ev| matches!(ev, Event::Ready)).await?;
    w.send(&Command::RunTest {
        script: script.to_string(),
        func: func.to_string(),
    })
    .await?;
    let ev = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        w.wait_for(|ev| {
            matches!(
                ev,
                Event::TestPass { .. } | Event::TestFail { .. } | Event::TestError { .. }
            )
        }),
    )
    .await
    .map_err(|_| anyhow!("timed out after 30s waiting for test result"))??;
    w.shutdown().await?;

    let exit_code = print_result(&ev);
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

fn print_result(ev: &Event) -> i32 {
    match ev {
        Event::TestPass { name, us } => {
            println!("{} {name} ({})", "✓".green(), fmt_duration(*us));
            0
        }
        Event::TestFail { name, us, failures } => {
            println!("{} {name} ({})", "✗".red(), fmt_duration(*us));
            for f in failures {
                println!("    {} {f}", "→".red());
            }
            1
        }
        Event::TestError { name, message } => {
            println!("{} {name}: {message}", "!".yellow());
            1
        }
        _ => unreachable!("wait_for filtered"),
    }
}

fn fmt_duration(us: u64) -> String {
    if us < 1_000 {
        format!("{us} µs")
    } else if us < 1_000_000 {
        format!("{:.2} ms", us as f64 / 1_000.0)
    } else {
        format!("{:.2} s", us as f64 / 1_000_000.0)
    }
}
