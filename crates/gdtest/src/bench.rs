//! Internal: run a fixed list of tests in a single persistent worker (M1.5
//! benchmark). This is a stopgap until internal/70 land the real pool + MPSC
//! dispatch. Output is one CSV line per test + a summary, so a bench script
//! can consume the data without regex-ing a pretty report.

use std::time::Instant;

use anyhow::{Result, anyhow};

use crate::protocol::{Command, Event};
use crate::worker::{Config, Worker};

#[tokio::main(flavor = "current_thread")]
pub async fn run(script: &str, funcs: &[String], cfg: &Config) -> Result<()> {
    let wall_start = Instant::now();
    let mut w = Worker::spawn(cfg)?;
    w.drain_stderr_to_log();
    w.wait_for(|ev| matches!(ev, Event::Ready { .. })).await?;
    let startup_us = wall_start.elapsed().as_micros();

    let mut pass = 0u32;
    let mut fail = 0u32;
    let mut error = 0u32;
    let mut test_us_total: u128 = 0;

    println!("# kind,name,dispatch_us,test_us");

    for func in funcs {
        let t = Instant::now();
        w.send(&Command::RunTest {
            script: script.to_string(),
            func: func.clone(),
        })
        .await?;
        let ev = w
            .wait_for(|ev| {
                matches!(
                    ev,
                    Event::TestPass { .. } | Event::TestFail { .. } | Event::TestError { .. }
                )
            })
            .await?;
        let dispatch_us = t.elapsed().as_micros();
        match ev {
            Event::TestPass { name, us } => {
                pass += 1;
                test_us_total += us as u128;
                println!("pass,{name},{dispatch_us},{us}");
            }
            Event::TestFail { name, us, failures } => {
                fail += 1;
                test_us_total += us as u128;
                println!("fail,{name},{dispatch_us},{us} # {}", failures.join(" | "));
            }
            Event::TestError { name, message } => {
                error += 1;
                println!("error,{name},{dispatch_us},0 # {message}");
            }
            _ => unreachable!(),
        }
    }

    w.shutdown().await?;

    let wall_us = wall_start.elapsed().as_micros();
    println!(
        "# summary pass={pass} fail={fail} error={error} startup_us={startup_us} test_us_total={test_us_total} wall_us={wall_us}"
    );

    if fail > 0 || error > 0 {
        return Err(anyhow!("{fail} failed, {error} errored"));
    }
    Ok(())
}
