//! D13's escalation state machine and D19's suspend/resume.

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
fn a_finished_thread_someone_still_holds_is_not_suspended() {
    // On GitHub's runner, suspending an exited process once reported 1 thread. The likely
    // cause is a finished thread kept in the snapshot by a handle someone else held (an
    // antivirus scanner, say), which SuspendThread then "succeeds" on. Windows 11 drops
    // such threads from the snapshot, so here this passes with or without the exit-code
    // check in `for_each_thread`; it holds the handles so that a Windows which keeps
    // them is tested, not left to chance.
    use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, THREAD_QUERY_LIMITED_INFORMATION};

    // `findstr` waits for its input, so its threads can be found before it exits.
    let mut child = Command::new("findstr.exe")
        .arg("x")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn child");
    let pid = child.id();
    std::thread::sleep(Duration::from_millis(200));

    let mut held: Vec<HANDLE> = Vec::new();
    // SAFETY: TH32CS_SNAPTHREAD snapshots every thread; the pid argument is ignored.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    // SAFETY: a zeroed THREADENTRY32 is valid once `dwSize` is set, as below.
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = u32::try_from(size_of::<THREADENTRY32>()).unwrap();
    // SAFETY: the snapshot is valid and the entry correctly sized.
    let mut ok = unsafe { Thread32First(snapshot, &raw mut entry) };
    while ok != 0 {
        if entry.th32OwnerProcessID == pid {
            // SAFETY: opening a thread by id; null on failure.
            let handle =
                unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, FALSE, entry.th32ThreadID) };
            if !handle.is_null() {
                held.push(handle);
            }
        }
        // SAFETY: as above.
        ok = unsafe { Thread32Next(snapshot, &raw mut entry) };
    }
    // SAFETY: closing the snapshot once.
    unsafe { CloseHandle(snapshot) };
    assert!(!held.is_empty(), "found none of findstr's threads");

    drop(child.stdin.take());
    let _ = child.wait();

    let affected = suspend_process(pid).expect("suspend of a dead pid should not error");
    for handle in held {
        // SAFETY: each handle was opened above and is closed once.
        unsafe { CloseHandle(handle) };
    }
    assert_eq!(affected, 0, "a finished thread was counted as suspended");
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
