//! Whether a process still runs is asked of the process, not of its pid — **D22**.
//!
//! Windows hands a pid out again soon after its process ends, so "is some process with
//! this number running?" is a question about whoever has the number now. The registry of
//! spawned trees asked it that way, to list the roots `kill 0` signals and to drop the
//! entries of commands that had finished: a root that had ended counted as running once
//! another process had its pid. Each entry now holds its root open, which keeps the pid
//! the root's and lets the registry ask the process.
//!
//! These tests do not wait for Windows to reuse a pid, which is luck. They check what
//! decides it: that the root's pid stays its own while the entry exists, and that a root
//! which has ended is not taken for a running one even when the number says otherwise. A
//! process that exits with 259, the status that reads as "still running", says so for as
//! long as anything holds it.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result."
)]

use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use cash_win32::jobreg;
use cash_win32::process::{is_pid_alive, started};

/// A `cmd.exe` that waits for its input to close and then exits with `status`, so the
/// test decides when it ends.
fn waits_then_exits(status: u32) -> Child {
    Command::new("cmd.exe")
        .args(["/d", "/c", &format!("set /p line= & exit {status}")])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("cmd.exe")
}

fn end(child: &mut Child) -> Option<i32> {
    drop(child.stdin.take());
    child.wait().expect("cmd.exe exits").code()
}

/// How long it takes the process that had `pid` and started at `at` to stop being a
/// process, or `None` if it still is one after `patience`.
///
/// Windows frees a process once the last handle to it is closed. Others (the console
/// host, a virus scanner) may hold one a moment longer than this test does, so "let go"
/// and "kept" are both told by watching for a while.
fn let_go_within(pid: u32, at: u64, patience: Duration) -> Option<Duration> {
    let began = Instant::now();
    while started(pid) == Some(at) {
        if began.elapsed() > patience {
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Some(began.elapsed())
}

#[test]
fn a_process_that_exited_with_259_is_not_alive() {
    let mut child = waits_then_exits(259);
    assert!(is_pid_alive(child.id()));
    assert_eq!(end(&mut child), Some(259));
    // `child` is still held, so the pid is still this process's.
    assert!(!is_pid_alive(child.id()));
}

/// The registry is one per process, and a sweep drops every entry whose root has ended:
/// under `cargo test`, which runs these on threads of one process, they take turns.
static REGISTRY: Mutex<()> = Mutex::new(());

#[test]
fn a_contained_root_keeps_its_pid_until_its_entry_is_swept() {
    let _turn = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    let mut child = waits_then_exits(0);
    let (pid, at) = (child.id(), started(child.id()).expect("its start time"));
    jobreg::contain(pid);
    assert!(jobreg::roots().contains(&pid));

    end(&mut child);
    drop(child);

    // Nothing but the registry's entry holds the root now. Its pid is still its own: a
    // process that has ended, which no other can replace while the entry names it.
    if let Some(after) = let_go_within(pid, at, Duration::from_secs(1)) {
        panic!("the pid was no longer the root's after {after:?}");
    }
    assert!(!is_pid_alive(pid));
    assert!(!jobreg::roots().contains(&pid));

    jobreg::sweep();
    assert!(
        let_go_within(pid, at, Duration::from_secs(10)).is_some(),
        "a swept entry still holds its root"
    );
    assert!(
        !jobreg::terminate_tree(pid, 1).expect("no job to terminate"),
        "the entry was not swept"
    );
}

#[test]
fn a_root_that_exited_with_259_is_not_a_root_still() {
    let _turn = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    let mut child = waits_then_exits(259);
    let pid = child.id();
    jobreg::contain(pid);
    assert!(jobreg::roots().contains(&pid));

    assert_eq!(end(&mut child), Some(259));

    // `child` is still held: the pid names this process, ended, with the status that
    // reads as running. `kill 0` must not be handed it, and its entry must not stay.
    assert!(!jobreg::roots().contains(&pid), "`kill 0` would signal it");
    jobreg::sweep();
    assert!(
        !jobreg::terminate_tree(pid, 1).expect("no job to terminate"),
        "the entry was not swept"
    );
}
