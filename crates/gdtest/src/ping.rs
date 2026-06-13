use std::time::Instant;

use anyhow::{anyhow, Result};

use crate::protocol::{Command, Event};
use crate::worker::{Config, Worker};

#[tokio::main(flavor = "current_thread")]
pub async fn run(count: u32, cfg: &Config) -> Result<()> {
    let t_startup = Instant::now();
    let mut w = Worker::spawn(cfg)?;
    w.drain_stderr_to_log();
    w.wait_for(|ev| matches!(ev, Event::Ready { .. })).await?;
    let startup_ms = t_startup.elapsed().as_secs_f64() * 1000.0;

    let mut rtts_us: Vec<u128> = Vec::with_capacity(count as usize);
    for i in 0..count {
        let t = Instant::now();
        w.send(&Command::Ping).await?;
        w.wait_for(|ev| matches!(ev, Event::Pong))
            .await
            .map_err(|e| anyhow!("ping {i}: {e}"))?;
        rtts_us.push(t.elapsed().as_micros());
    }

    let status = w.shutdown().await?;

    rtts_us.sort_unstable();
    let sum: u128 = rtts_us.iter().sum();
    let mean_us = if count > 0 {
        sum as f64 / count as f64
    } else {
        0.0
    };
    let p50 = rtts_us.get(rtts_us.len() / 2).copied().unwrap_or(0);
    let p99_idx = rtts_us
        .len()
        .saturating_sub(1)
        .min((rtts_us.len() * 99) / 100);
    let p99 = rtts_us.get(p99_idx).copied().unwrap_or(0);

    println!(
        "gdtest __ping: {count} round-trips, startup {startup_ms:.1} ms, mean {mean_us:.0} µs, p50 {p50} µs, p99 {p99} µs"
    );
    println!("worker exit: {status}");
    if !status.success() {
        return Err(anyhow!("worker exited with non-zero status"));
    }
    Ok(())
}
