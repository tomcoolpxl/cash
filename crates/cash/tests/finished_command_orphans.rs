//! A finished command's descendants keep running — **D6**, **D22**.
//!
//! `code .` runs `code.cmd`, which starts the VS Code window and exits. cash puts every
//! external command in its own job so `kill %1` can reap the tree, and it closed that
//! job with `KILL_ON_JOB_CLOSE` set as soon as the next command started: the window
//! opened and closed. In bash a finished command's orphans live on, and so they must here
//! for as long as cash does.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result."
)]

use std::process::Command;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// Starts a process that outlives the batch file: after about two seconds it writes
/// `marker`, then the batch file exits at once, as `code.cmd` does.
const LAUNCHER: &str = "@echo off\r\n\
start \"\" /b cmd /d /c \"ping -n 3 127.0.0.1 >nul & echo alive>marker\" >nul 2>nul\r\n";

#[test]
fn a_launched_process_survives_the_next_command() {
    let dir = tempfile::tempdir().expect("scratch dir");
    std::fs::write(dir.path().join("launch.cmd"), LAUNCHER).expect("write launcher");

    // `whoami.exe` is any external command: spawning one sweeps finished jobs, which
    // is where the launcher's job used to be closed and its orphan killed.
    let out = Command::new(CASH)
        .args([
            "-c",
            "./launch.cmd; whoami.exe >/dev/null; sleep 5; cat marker",
        ])
        .current_dir(dir.path())
        .output()
        .expect("failed to run cash");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(stdout.trim_end(), "alive", "stderr: {stderr}");
}
