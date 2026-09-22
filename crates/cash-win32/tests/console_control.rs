//! D13's escalation state machine and D19's suspend/resume.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result. Shell snippets are spelled with hashes \
              throughout, including where they are not strictly needed, because \
              alternating the two forms by accident of content reads worse."
)]

use std::process::{Command, Stdio};
use std::time::Duration;

use cash_win32::console::{Escalation, InterruptState, resume_process, suspend_process};
use cash_win32::process::cpu_time;

#[test]
fn the_user_is_the_timer() {
    // D13: the grace period is not a duration, it is the second Ctrl-C. Terraform's own
    // interrupt handling is two-stage and its first stage can legitimately take minutes
    // mid-aws_rds_instance, so any fixed timeout is wrong.
    let state = InterruptState::new();

    assert_eq!(state.record(), Escalation::Interrupt);
    assert_eq!(state.record(), Escalation::Escalate);
    assert_eq!(state.record(), Escalation::Terminate);

    // Still terminate — the user is leaning on it.
    assert_eq!(state.record(), Escalation::Terminate);
}

#[test]
fn escalation_resets_per_foreground_job() {
    // Otherwise one Ctrl-C per command across three commands would terminate the third,
    // which is a genuinely alarming failure mode.
    let state = InterruptState::new();

    assert_eq!(state.record(), Escalation::Interrupt);
    state.reset();
    assert_eq!(state.count(), 0);
    assert_eq!(state.record(), Escalation::Interrupt);
}

#[test]
fn a_fresh_job_starts_gracefully() {
    let state = InterruptState::new();
    assert_eq!(state.count(), 0);
    assert_eq!(
        state.record(),
        Escalation::Interrupt,
        "first interrupt must be graceful"
    );
}

#[test]
fn suspend_and_resume_affect_real_threads() {
    // D19: Windows has no SIGSTOP for arbitrary exes, so Ctrl-Z enumerates threads and
    // suspends each — documented APIs only, no NtSuspendProcess.
    let mut child = Command::new("cmd.exe")
        .args(["/d", "/s", "/c", "ping -n 30 127.0.0.1 >nul"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn child");

    // Give the process a moment to have threads worth suspending.
    std::thread::sleep(Duration::from_millis(300));

    let suspended = suspend_process(child.id()).expect("suspend");
    assert!(suspended > 0, "expected to suspend at least one thread");

    let resumed = resume_process(child.id()).expect("resume");
    assert_eq!(resumed, suspended, "resume should reach the same threads");

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn suspending_a_dead_process_affects_nothing() {
    // A pid that has exited must not error — `kill -STOP` on a finished job is a normal
    // race, not a failure.
    let mut child = Command::new("cmd.exe")
        .args(["/d", "/s", "/c", "exit 0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn child");
    let pid = child.id();
    let _ = child.wait();

    let affected = suspend_process(pid).expect("suspend of a dead pid should not error");
    assert_eq!(affected, 0);
}

#[test]
fn suspension_actually_stops_the_cpu() {
    // The behavioural check that matters: without it, thread enumeration could "succeed"
    // while the process carried on running. CPU time is the right signal — it stops the
    // instant a process is suspended, unlike observable side effects such as file
    // writes, which are slow, buffered and racy.
    let mut child = Command::new("cmd.exe")
        .args(["/d", "/s", "/c", "for /l %i in (1,1,200000000) do @rem"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn cpu burner");

    let pid = child.id();

    // Wait until it is demonstrably burning CPU, so the test proves something.
    let mut baseline = 0;
    let mut running = false;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(50));
        if let Some(time) = cpu_time(pid)
            && time > 0
        {
            baseline = time;
            running = true;
            break;
        }
    }
    assert!(
        running,
        "child never consumed CPU, so this test proves nothing"
    );

    suspend_process(pid).expect("suspend");

    // Sample twice with a gap; a suspended process accrues nothing in between.
    std::thread::sleep(Duration::from_millis(200));
    let before = cpu_time(pid).expect("process alive");
    std::thread::sleep(Duration::from_millis(500));
    let after = cpu_time(pid).expect("process alive");

    assert_eq!(
        before, after,
        "suspended process still accrued CPU ({before} -> {after}); D19's Ctrl-Z would \
         appear to work while doing nothing"
    );
    assert!(after >= baseline);

    resume_process(pid).expect("resume");

    let mut resumed = false;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(50));
        if cpu_time(pid).is_some_and(|t| t > after) {
            resumed = true;
            break;
        }
    }
    assert!(resumed, "resumed process never started consuming CPU again");

    let _ = child.kill();
    let _ = child.wait();
}
