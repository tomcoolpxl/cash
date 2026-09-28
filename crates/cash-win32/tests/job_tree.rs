//! Proof of D6: children join the job automatically, and closing the job reaps the tree.
//!
//! This is the behaviour the whole project claims is better on Windows than on Linux,
//! where killing bash leaves descendants reparented to init. It deserves a real test
//! rather than a docs assertion.

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
use std::time::{Duration, Instant};

use cash_win32::JobObject;
use cash_win32::process::is_pid_alive;

/// Spawn `cmd.exe`, which in turn spawns `ping.exe` — giving us a two-level tree where
/// only the top process is ever assigned to the job.
fn spawn_tree() -> std::process::Child {
    Command::new("cmd.exe")
        .args(["/d", "/s", "/c", "ping -n 30 127.0.0.1 >nul"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn cmd.exe")
}

fn wait_until<F: FnMut() -> bool>(timeout: Duration, mut predicate: F) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn children_join_the_job_automatically() {
    let job = JobObject::for_pipeline().expect("create job");
    let mut child = spawn_tree();

    job.assign_child(&child).expect("assign child to job");

    // The grandchild (ping) is spawned by cmd *after* assignment, so its membership can
    // only come from the kernel propagating the job. That is the property D6 relies on.
    let joined = wait_until(Duration::from_secs(10), || {
        job.process_ids().is_ok_and(|ids| ids.len() >= 2)
    });

    let ids = job.process_ids().expect("query job pids");
    assert!(
        joined,
        "expected the grandchild to join the job; job held {ids:?}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn dropping_the_job_reaps_the_whole_tree() {
    let job = JobObject::for_pipeline().expect("create job");
    let mut child = spawn_tree();
    let child_pid = child.id();

    job.assign_child(&child).expect("assign child to job");

    assert!(
        wait_until(Duration::from_secs(10), || {
            job.process_ids().is_ok_and(|ids| ids.len() >= 2)
        }),
        "grandchild never joined the job, so this test would not prove anything"
    );

    // Capture the grandchild before tearing down, so we can prove it died too rather
    // than only checking the process we held a handle to.
    let pids = job.process_ids().expect("query job pids");
    let grandchild_pid = pids
        .iter()
        .copied()
        .find(|&pid| pid != child_pid)
        .expect("a grandchild pid distinct from the child");

    // The entire mechanism: closing the last handle terminates every member. No signal
    // is sent, nothing cooperates, and nothing can opt out.
    drop(job);

    assert!(
        wait_until(Duration::from_secs(10), || {
            matches!(child.try_wait(), Ok(Some(_)))
        }),
        "child {child_pid} survived the job being closed"
    );

    assert!(
        wait_until(Duration::from_secs(10), || !is_pid_alive(grandchild_pid)),
        "grandchild {grandchild_pid} survived the job being closed — D6's tree guarantee \
         is what this project claims is better than Linux, so this failing matters"
    );
}

#[test]
fn releasing_the_job_leaves_the_tree_running() {
    // What cash does with a finished command's job: `code .` leaves its editor window in
    // it, and closing the job must not close the window.
    let job = JobObject::for_pipeline().expect("create job");
    let mut child = spawn_tree();
    let child_pid = child.id();

    job.assign_child(&child).expect("assign child to job");
    assert!(
        wait_until(Duration::from_secs(10), || {
            job.process_ids().is_ok_and(|ids| ids.len() >= 2)
        }),
        "grandchild never joined the job, so this test would not prove anything"
    );

    job.release();

    std::thread::sleep(Duration::from_millis(500));
    assert!(
        matches!(child.try_wait(), Ok(None)),
        "child {child_pid} was reaped when its job was released"
    );
    assert!(is_pid_alive(child_pid));

    let _ = Command::new("taskkill")
        .args(["/f", "/t", "/pid", &child_pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.wait();
}

#[test]
fn breakaway_is_opt_in() {
    // D45 records that permitting breakaway weakens D6 for every child, not just the
    // intended one. Per-pipeline jobs must therefore not permit it.
    let pipeline = cash_win32::JobConfig::job();
    assert!(pipeline.kill_on_close);
    assert!(!pipeline.allow_breakaway);

    // The session job permits it, because `detach` (D45) needs it.
    let session = cash_win32::JobConfig::session();
    assert!(session.kill_on_close);
    assert!(session.allow_breakaway);
}
