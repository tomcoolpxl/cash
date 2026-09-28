//! D6's race-free containment: the child is in its job before it runs.

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

use std::time::{Duration, Instant};

use cash_win32::cmd::build_command_line;
use cash_win32::job::JobObject;
use cash_win32::process::is_pid_alive;
use cash_win32::spawn::{SpawnOptions, in_any_job, spawn};

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

fn sleeper() -> String {
    build_command_line(
        "cmd.exe",
        &[
            "/d".into(),
            "/s".into(),
            "/c".into(),
            "ping -n 30 127.0.0.1 >nul".into(),
        ],
    )
}

#[test]
fn a_child_is_in_its_job_before_it_runs() {
    // The race §6 documents: assigning after spawn() lets a fast-forking child produce a
    // grandchild that never joins. Creating suspended closes it — by the time the child
    // executes its first instruction, containment is already a fact.
    let job = JobObject::for_pipeline().expect("create job");

    let child = spawn(
        &sleeper(),
        &SpawnOptions {
            job: Some(&job),
            ..Default::default()
        },
    )
    .expect("spawn");

    let ids = job.process_ids().expect("query job");
    assert!(
        ids.contains(&child.id()),
        "child {} was not in the job immediately after spawn; ids were {ids:?}",
        child.id()
    );
}

#[test]
fn the_whole_tree_is_contained_and_reaped() {
    let job = JobObject::for_pipeline().expect("create job");
    let child = spawn(
        &sleeper(),
        &SpawnOptions {
            job: Some(&job),
            ..Default::default()
        },
    )
    .expect("spawn");

    let pid = child.id();
    assert!(
        wait_until(Duration::from_secs(10), || {
            job.process_ids().is_ok_and(|ids| ids.len() >= 2)
        }),
        "grandchild never joined, so this proves nothing"
    );

    let ids = job.process_ids().expect("query job");
    let grandchild = ids
        .iter()
        .copied()
        .find(|&p| p != pid)
        .expect("a grandchild");

    drop(job);
    drop(child);

    assert!(
        wait_until(Duration::from_secs(10), || !is_pid_alive(pid)),
        "child survived"
    );
    assert!(
        wait_until(Duration::from_secs(10), || !is_pid_alive(grandchild)),
        "grandchild {grandchild} survived"
    );
}

#[test]
fn a_new_process_group_reports_its_id() {
    // D13 needs this: GenerateConsoleCtrlEvent requires a group id, and a group only
    // exists when the child was created with CREATE_NEW_PROCESS_GROUP.
    let job = JobObject::for_pipeline().expect("create job");
    let child = spawn(
        &sleeper(),
        &SpawnOptions {
            job: Some(&job),
            new_process_group: true,
            ..Default::default()
        },
    )
    .expect("spawn");

    assert_eq!(child.group_id(), Some(child.id()));
}

#[test]
fn without_a_new_group_there_is_no_group_id() {
    let job = JobObject::for_pipeline().expect("create job");
    let child = spawn(
        &sleeper(),
        &SpawnOptions {
            job: Some(&job),
            ..Default::default()
        },
    )
    .expect("spawn");

    assert_eq!(child.group_id(), None, "a group id must not be invented");
}

#[test]
fn the_child_environment_is_what_we_gave_it() {
    // D5's translation happens in the block handed to the child, so it has to arrive.
    //
    // The child reports what it received through its *exit code* rather than by writing
    // a file. A first attempt used `echo %VAR% > file`, which fails for exactly the
    // reason D32 exists: `cmd /c` does not parse its command with CRT rules, so the
    // backslash-escaped quotes that CRT quoting produces arrive literally and corrupt
    // the path. Avoiding redirection avoids the whole hazard.
    let env = vec![
        ("CASH_TEST_CODE".to_string(), "7".to_string()),
        (
            "SystemRoot".to_string(),
            std::env::var("SystemRoot").unwrap_or_default(),
        ),
    ];

    let job = JobObject::for_pipeline().expect("create job");
    let child = spawn(
        "cmd.exe /d /s /c exit %CASH_TEST_CODE%",
        &SpawnOptions {
            job: Some(&job),
            env: Some(&env),
            ..Default::default()
        },
    )
    .expect("spawn");

    let code = child.wait().expect("wait for child");
    assert_eq!(code, 7, "child did not receive the environment we built");
}

#[test]
fn exit_codes_come_back_and_map_per_d15() {
    let job = JobObject::for_pipeline().expect("create job");

    let child = spawn(
        "cmd.exe /d /s /c exit 3",
        &SpawnOptions {
            job: Some(&job),
            ..Default::default()
        },
    )
    .expect("spawn");
    assert_eq!(child.wait().expect("wait"), 3);
    assert_eq!(child.wait_status().expect("status"), 3);

    // §9 measured this exact case in brush: cmd /c "exit 300" gives 44.
    let child = spawn(
        "cmd.exe /d /s /c exit 300",
        &SpawnOptions {
            job: Some(&job),
            ..Default::default()
        },
    )
    .expect("spawn");
    assert_eq!(
        child.wait().expect("wait"),
        300,
        "raw code should be untruncated"
    );
    assert_eq!(
        child.wait_status().expect("status"),
        44,
        "$? truncates to the low byte"
    );
}

#[test]
fn try_wait_does_not_block() {
    let job = JobObject::for_pipeline().expect("create job");
    let child = spawn(
        &sleeper(),
        &SpawnOptions {
            job: Some(&job),
            ..Default::default()
        },
    )
    .expect("spawn");

    assert_eq!(child.try_wait().expect("try_wait"), None, "still running");

    drop(job);
    assert!(
        wait_until(Duration::from_secs(10), || {
            matches!(child.try_wait(), Ok(Some(_)))
        }),
        "try_wait never observed the exit"
    );
}

#[test]
fn we_can_tell_whether_we_are_already_in_a_job() {
    // Nested jobs have worked since Windows 8, so cash running inside Windows Terminal's
    // or VS Code's job is fine. This is for cash doctor (D35) to be able to say so.
    let _ = in_any_job();
}
