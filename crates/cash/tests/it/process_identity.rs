//! Telling a process from a later one that was given its pid.
//!
//! Windows hands a pid out again soon after its process exits, so a pid alone does not
//! say whether a process is gone: `kill -0 $pid` asks only whether some process has that
//! pid now. Twelve tests killed a `ping.exe`, slept a second and asked that way, and
//! about one full run in two had one of them fail, a different one each time. Measured
//! on 2026-09-30 with their script repeated all through three full runs: 8 times in 360,
//! `kill -0` succeeded a second after the kill, though `wait` then reported the ping's
//! 137. Each time the pid named a `cash.exe` or a test executable that another test had
//! started half a second to a second after the ping died. Asked ten times a second for
//! three seconds, it succeeded in 68 tries of 240.
//!
//! So a test asks in one of three ways, none of which is the pid alone:
//!
//! - of the shell's own child, `wait "$pid"; echo "status=$?"`: the status comes from the
//!   handle cash holds, and says how the child ended (128 + the signal that ended it);
//! - of a process only a script can name, by a name no other process has
//!   ([`OwnPing`]);
//! - of a pid the test itself saw alive, by the pid and a time that process was running
//!   ([`still_running`]).
//!
//! A pid the test holds a handle to (a [`std::process::Child`]) stays its process's:
//! Windows does not hand it out again while the handle is open.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result."
)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use cash_win32::process::{is_pid_alive, started};

/// Whether the process that had `pid` at `seen` is still running; `seen` is a
/// [`cash_win32::process::now_filetime`] count taken while that process was running, or
/// as it ended.
///
/// A pid is handed out again only after its process has ended, so a process with this pid
/// that started after `seen` is another one.
pub(crate) fn still_running(pid: u32, seen: u64) -> bool {
    started(pid).is_some_and(|at| at <= seen) && is_pid_alive(pid)
}

/// A copy of `ping.exe` under a name no other process has.
///
/// `pkill`, `killall` and `pidof` name processes by their program, and `ping` names every
/// ping on the machine: the ones other tests run at the same time, and the user's own. A
/// `pkill -x ping` in one test ended the pings of the others (a bystander started beside
/// it exited 143). Under its own name, a test's target is the only process it can reach.
///
/// The copy has no message file beside it, so it prints blank lines where `ping.exe`
/// prints text; tests that read ping's output run `ping.exe` itself.
pub(crate) struct OwnPing {
    dir: tempfile::TempDir,
    name: String,
}

impl OwnPing {
    pub(crate) fn new() -> Self {
        static COPIES: AtomicU32 = AtomicU32::new(0);
        // Unique among running processes: the test's own pid, and a count within it.
        let name = format!(
            "cash-test-ping-{}-{}",
            std::process::id(),
            COPIES.fetch_add(1, Ordering::Relaxed)
        );
        let dir = tempfile::tempdir().expect("a scratch directory");
        let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        let ping = PathBuf::from(system_root).join("System32").join("PING.EXE");
        std::fs::copy(&ping, dir.path().join(format!("{name}.exe")))
            .unwrap_or_else(|error| panic!("copying {}: {error}", ping.display()));
        Self { dir, name }
    }

    /// The program's name, as `pkill -x`, `killall` and `pidof` take it.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// The program's path as a script spells it, with forward slashes.
    pub(crate) fn path(&self) -> String {
        self.dir
            .path()
            .join(format!("{}.exe", self.name))
            .to_string_lossy()
            .replace('\\', "/")
    }
}

#[test]
fn a_running_process_is_still_running() {
    let now = cash_win32::process::now_filetime();
    assert!(still_running(std::process::id(), now));
}

#[test]
fn a_process_that_started_after_the_pid_was_seen_is_another_one() {
    // This process has the pid, and is running: only its start time says it is not the
    // one that had the pid a moment before it started.
    let me = std::process::id();
    let before_i_started = started(me).expect("my own start time") - 1;
    assert!(is_pid_alive(me));
    assert!(!still_running(me, before_i_started));
}

#[test]
fn a_process_that_has_exited_is_not_running() {
    let mut child = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "exit"])
        .spawn()
        .expect("cmd.exe");
    let seen = cash_win32::process::now_filetime();
    child.wait().expect("cmd.exe exits");
    // `child` is still held, so the pid is still this process's.
    assert!(!still_running(child.id(), seen));
}

#[test]
fn an_own_ping_runs_under_its_own_name() {
    let target = OwnPing::new();
    let mut child = std::process::Command::new(target.path())
        .args(["-n", "5", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("the copy of ping.exe runs");
    let image = format!("{}.exe", target.name());
    let mine: Vec<u32> = cash_win32::process::list()
        .into_iter()
        .filter(|process| process.name.eq_ignore_ascii_case(&image))
        .map(|process| process.pid)
        .collect();
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(mine, [child.id()]);
}
