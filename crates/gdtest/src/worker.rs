//! Single Godot worker: spawn, framed I/O over the `GDTEST_OUT:` protocol.

use std::path::PathBuf;
use std::process::Stdio;
#[cfg(windows)]
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{anyhow, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command as TokioCommand};
use tokio::sync::mpsc;

use crate::protocol::{Command, Event, GDTEST_OUT_PREFIX};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    Headless,
    Windowed,
}

#[derive(Clone)]
pub struct Config {
    pub godot_exe: String,
    pub project_path: String,
    pub harness: String,
    pub display_mode: DisplayMode,
    pub update_snapshots: bool,
    pub seed: Option<u64>,
    /// `res://` paths of plugin scripts to load in the worker (internal).
    pub plugins: Vec<String>,
}

pub struct Worker {
    pub child: Child,
    pub stdin: ChildStdin,
    pub stdout: tokio::io::Lines<BufReader<ChildStdout>>,
    /// Path to the Godot log file when the platform uses `--log-file`.
    pub log_path: Option<PathBuf>,
    /// Stderr lines delivered via a channel.
    ///
    /// On Unix the producer runs in a blocking thread reading from a PTY
    /// master, so Godot sees a TTY and flushes stderr on every `\n` —
    /// SCRIPT ERRORs arrive in the same event-loop tick as the test they
    /// belong to instead of being delayed by pipe-buffering.
    /// On non-Unix it's a tokio task over a regular pipe (best-effort).
    ///
    /// `None` once taken by the pool for in-flight correlation.
    pub stderr: Option<mpsc::UnboundedReceiver<String>>,
}

#[cfg(windows)]
static WORKER_LOG_SEQ: AtomicU64 = AtomicU64::new(0);

impl Worker {
    pub fn spawn(cfg: &Config) -> Result<Self> {
        #[cfg(unix)]
        let (stderr_stdio, stderr_rx) =
            pty_stderr().context("failed to open PTY for Godot stderr")?;

        #[cfg(windows)]
        let stderr_stdio = Stdio::inherit();

        #[cfg(all(not(unix), not(windows)))]
        let stderr_stdio = Stdio::piped();

        let mut cmd = TokioCommand::new(&cfg.godot_exe);
        cmd.args(base_args(cfg))
            .current_dir(&cfg.project_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr_stdio)
            .kill_on_drop(true);
        #[cfg(windows)]
        let log_path: Option<PathBuf> = {
            let seq = WORKER_LOG_SEQ.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "gdtest-godot-{}-{}.log",
                std::process::id(),
                seq
            ));
            cmd.arg("--log-file").arg(&path);
            Some(path)
        };
        #[cfg(not(windows))]
        let log_path: Option<PathBuf> = None;
        cmd.arg("-s").arg(&cfg.harness);
        if cfg.update_snapshots {
            cmd.env("GDTEST_UPDATE_SNAPSHOTS", "1");
        }
        if let Some(s) = cfg.seed {
            cmd.env("GDTEST_SEED", s.to_string());
        }
        if !cfg.plugins.is_empty() {
            cmd.env("GDTEST_PLUGINS", cfg.plugins.join(","));
        }
        let mut child = cmd
            .spawn()
            .with_context(|| format!("failed to spawn Godot at {}", cfg.godot_exe))?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
        let stdout = BufReader::new(stdout).lines();

        // On Unix stderr is the PTY slave — child.stderr is None (no pipe).
        // On other platforms we set up a tokio task over the pipe here.
        #[cfg(windows)]
        let stderr_rx = {
            // Godot 4.6.x on Windows crashes very early when stderr is redirected
            // to a pipe. Inherit stderr instead of capturing it so the worker stays
            // alive; the downside is we lose programmatic stderr correlation.
            let (_tx, rx) = mpsc::unbounded_channel::<String>();
            rx
        };

        #[cfg(all(not(unix), not(windows)))]
        let stderr_rx = {
            use tokio::process::ChildStderr;
            let stderr: ChildStderr = child
                .stderr
                .take()
                .ok_or_else(|| anyhow!("no stderr pipe"))?;
            let mut lines = BufReader::new(stderr).lines();
            let (tx, rx) = mpsc::unbounded_channel::<String>();
            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
            rx
        };

        Ok(Self {
            child,
            stdin,
            stdout,
            log_path,
            stderr: Some(stderr_rx),
        })
    }

    /// Spawn a background task that eprintln!s every Godot stderr line
    /// prefixed `[godot]`. Useful for single-test CLI modes that want
    /// diagnostics but don't need to inspect stderr programmatically.
    /// Consumes `self.stderr` — after this call the field is None.
    pub fn drain_stderr_to_log(&mut self) {
        if let Some(mut rx) = self.stderr.take() {
            tokio::spawn(async move {
                while let Some(line) = rx.recv().await {
                    eprintln!("[godot] {line}");
                }
            });
        }
    }

    pub async fn send<T: serde::Serialize>(&mut self, msg: &T) -> Result<()> {
        let line = serde_json::to_string(msg)? + "\n";
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Reads worker stdout until a protocol event is found. Non-protocol lines
    /// (Godot's own stdout noise) are silently dropped. Returns an error when
    /// the worker's stdout closes — callers treat that as a crash.
    pub async fn next_event(&mut self) -> Result<Event> {
        while let Some(line) = self.stdout.next_line().await? {
            let Some(payload) = line.strip_prefix(GDTEST_OUT_PREFIX) else {
                continue;
            };
            let payload = payload.trim();
            if payload.is_empty() {
                continue;
            }
            return serde_json::from_str::<Event>(payload)
                .map_err(|e| anyhow!("invalid event `{payload}`: {e}"));
        }
        Err(anyhow!("worker stdout closed"))
    }

    pub async fn wait_for<F: Fn(&Event) -> bool>(&mut self, pred: F) -> Result<Event> {
        loop {
            let ev = self.next_event().await?;
            if pred(&ev) {
                return Ok(ev);
            }
        }
    }

    pub async fn shutdown(mut self) -> Result<std::process::ExitStatus> {
        let line = serde_json::to_string(&Command::Shutdown)? + "\n";
        self.stdin.write_all(line.as_bytes()).await.ok();
        drop(self.stdin);
        tokio::time::timeout(std::time::Duration::from_secs(5), self.child.wait())
            .await
            .context("worker did not exit within 5s")?
            .map_err(Into::into)
    }
}

fn base_args(cfg: &Config) -> Vec<String> {
    let mut args = Vec::new();
    if cfg.display_mode == DisplayMode::Headless {
        args.push("--headless".to_string());
    }
    args.push("--path".to_string());
    args.push(cfg.project_path.clone());
    args.push("-s".to_string());
    args.push(cfg.harness.clone());
    args
}

/// On Unix, open a PTY pair and return (slave as Stdio, stderr line receiver).
///
/// Godot inherits the slave fd as its stderr. Because the slave is a real
/// TTY, Godot's C++ runtime uses line-buffered (not fully-buffered) stdio,
/// so every SCRIPT ERROR line is flushed on the `\n` and arrives in the
/// same event-loop tick as the test that triggered it.
///
/// The master side is read in a blocking thread and forwarded to the
/// caller via an unbounded channel.
#[cfg(unix)]
fn pty_stderr() -> Result<(Stdio, mpsc::UnboundedReceiver<String>)> {
    use std::os::unix::io::FromRawFd;

    let mut master_fd: libc::c_int = -1;
    let mut slave_fd: libc::c_int = -1;

    // SAFETY: openpty is a POSIX call; we check the return value and own
    // both fds exclusively after this point.
    let ret = unsafe {
        libc::openpty(
            &mut master_fd,
            &mut slave_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ret != 0 {
        return Err(anyhow!(
            "openpty failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    // SAFETY: openpty returned valid, exclusively-owned fds.
    let master_file = unsafe { std::fs::File::from_raw_fd(master_fd) };
    let slave_file = unsafe { std::fs::File::from_raw_fd(slave_fd) };
    let slave_stdio = Stdio::from(slave_file);

    let (tx, rx) = mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(master_file).lines() {
            match line {
                Ok(l) => {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
                // EIO when the slave fd closes (Godot exited) — normal EOF on macOS.
                Err(_) => break,
            }
        }
    });

    Ok((slave_stdio, rx))
}

#[cfg(test)]
mod tests {
    use super::{base_args, Config, DisplayMode};

    fn test_config(display_mode: DisplayMode) -> Config {
        Config {
            godot_exe: "godot".to_string(),
            project_path: ".".to_string(),
            harness: "addons/gdtest/gdtest_worker.gd".to_string(),
            display_mode,
            update_snapshots: false,
            seed: None,
            plugins: Vec::new(),
        }
    }

    #[test]
    fn headless_mode_includes_headless_flag() {
        let args = base_args(&test_config(DisplayMode::Headless));
        assert_eq!(
            args,
            vec![
                "--headless",
                "--path",
                ".",
                "-s",
                "addons/gdtest/gdtest_worker.gd"
            ]
        );
    }

    #[test]
    fn windowed_mode_omits_headless_flag() {
        let args = base_args(&test_config(DisplayMode::Windowed));
        assert_eq!(args, vec!["--path", ".", "-s", "addons/gdtest/gdtest_worker.gd"]);
    }
}
