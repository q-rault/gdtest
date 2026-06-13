//! JSON line-delimited protocol between the gdtest orchestrator and the Godot worker.
//!
//! Every worker stdout line that carries a protocol payload is prefixed with
//! [`GDTEST_OUT_PREFIX`] so the orchestrator can ignore Godot's own stdout chatter
//! (shader warnings, etc.). Worker stdin lines are plain JSON; the orchestrator
//! writes them directly with a trailing newline.

use serde::{Deserialize, Serialize};

pub const GDTEST_OUT_PREFIX: &str = "GDTEST_OUT:";

/// Orchestrator -> worker. Tag field is `cmd`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Ping,
    RunTest {
        script: String,
        func: String,
    },
    /// Run every `func test_*()` in the script in file-definition order.
    /// Worker emits one TestPass/TestFail/TestError per test, then Ready.
    RunFile {
        script: String,
    },
    Reset,
    Shutdown,
}

/// Worker -> orchestrator. Tag field is `event`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Ready {
        /// Harness version from `addons/gdtest/gdtest_worker.gd`. Present in
        /// harness 0.1.0+; absent (None) when an older harness is used.
        #[serde(default)]
        harness_version: Option<String>,
        /// Godot engine version (major.minor.patch). Present in harness 0.1.0+.
        #[serde(default)]
        godot_version: Option<String>,
    },
    Pong,
    /// Emitted by the worker immediately before calling a test function.
    /// Used by the pool to correlate a mid-test worker crash or SCRIPT
    /// ERROR with the specific test that was in flight.
    TestStart {
        name: String,
    },
    TestPass {
        name: String,
        us: u64,
    },
    TestFail {
        name: String,
        us: u64,
        failures: Vec<String>,
    },
    TestError {
        name: String,
        message: String,
    },
    /// Test called `pending()` - intentionally incomplete, not a failure.
    TestPending {
        name: String,
        message: String,
    },
    /// Test called `skip()` - intentionally excluded from this run.
    TestSkip {
        name: String,
        message: String,
    },
    /// Signals the end of a parameterized test batch. Sent after all
    /// per-case TestPass/Fail/Error events for one `func test_*()` that
    /// had a companion `func get_test_*_cases()`.
    TestBatchDone,
    Log {
        level: String,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip_cmd(cmd: Command, expected: &str) {
        let s = serde_json::to_string(&cmd).unwrap();
        assert_eq!(s, expected);
        let back: Command = serde_json::from_str(&s).unwrap();
        assert_eq!(back, cmd);
    }

    fn round_trip_event(ev: Event, expected: &str) {
        let s = serde_json::to_string(&ev).unwrap();
        assert_eq!(s, expected);
        let back: Event = serde_json::from_str(&s).unwrap();
        assert_eq!(back, ev);
    }

    #[test]
    fn ping_serializes() {
        round_trip_cmd(Command::Ping, r#"{"cmd":"ping"}"#);
    }

    #[test]
    fn run_test_serializes() {
        round_trip_cmd(
            Command::RunTest {
                script: "res://test/foo.gd".into(),
                func: "test_bar".into(),
            },
            r#"{"cmd":"run_test","script":"res://test/foo.gd","func":"test_bar"}"#,
        );
    }

    #[test]
    fn shutdown_serializes() {
        round_trip_cmd(Command::Shutdown, r#"{"cmd":"shutdown"}"#);
    }

    #[test]
    fn test_start_serializes() {
        round_trip_event(
            Event::TestStart {
                name: "test_foo".into(),
            },
            r#"{"event":"test_start","name":"test_foo"}"#,
        );
    }

    #[test]
    fn run_file_serializes() {
        round_trip_cmd(
            Command::RunFile {
                script: "res://test/foo.gd".into(),
            },
            r#"{"cmd":"run_file","script":"res://test/foo.gd"}"#,
        );
    }

    #[test]
    fn pong_serializes() {
        round_trip_event(Event::Pong, r#"{"event":"pong"}"#);
    }

    #[test]
    fn test_pass_serializes() {
        round_trip_event(
            Event::TestPass {
                name: "test_foo".into(),
                us: 1234,
            },
            r#"{"event":"test_pass","name":"test_foo","us":1234}"#,
        );
    }

    #[test]
    fn test_fail_serializes() {
        round_trip_event(
            Event::TestFail {
                name: "test_foo".into(),
                us: 42,
                failures: vec!["expected 1 got 2".into()],
            },
            r#"{"event":"test_fail","name":"test_foo","us":42,"failures":["expected 1 got 2"]}"#,
        );
    }
}
