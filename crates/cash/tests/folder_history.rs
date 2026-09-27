//! fish's folder history (spec D62): `prevd`, `nextd` and `cdh`, run as a script. The
//! Alt-← and Alt-→ keys are in `conpty_interactive_tests.rs`.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::process::{Command, Stdio};

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// Runs `script` under cash in a fresh folder holding `a`, `b` and `c`; standard output
/// and standard error.
fn run_in_folders(script: &str) -> (String, String) {
    let root = tempfile::tempdir().expect("temp dir");
    for name in ["a", "b", "c"] {
        std::fs::create_dir(root.path().join(name)).expect("create folder");
    }
    let out = Command::new(CASH)
        .args(["--norc", "--noprofile", "-c", script])
        .current_dir(root.path())
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    (
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
    )
}

/// `cd a`, `cd ../b`, `cd ../c`, then the given commands, printing where each leaves the
/// shell as its folder's name.
fn after_visiting_a_b_c(commands: &str) -> (String, String) {
    run_in_folders(&format!(
        "here() {{ printf '%s\\n' \"${{PWD##*/}}\"; }}\ncd a; cd ../b; cd ../c\n{commands}"
    ))
}

#[test]
fn prevd_and_nextd_retrace_the_route_without_adding_to_it() {
    let (out, err) = after_visiting_a_b_c(concat!(
        "prevd; here\n",
        "prevd; here\n",
        "nextd 2; here\n",
        "prevd 2; here\n",
    ));
    assert_eq!(err, "");
    assert_eq!(out, "b\na\nc\na\n");
}

#[test]
fn cd_forgets_the_way_forward() {
    let (out, err) =
        after_visiting_a_b_c(concat!("prevd; cd ../a; here\n", "nextd; echo status=$?\n",));
    assert_eq!(out, "a\nstatus=1\n");
    assert_eq!(err, "nextd: no folder ahead of this one\n");
}

#[test]
fn the_ends_of_the_history_are_reported() {
    let (out, err) = run_in_folders("prevd; echo status=$?; prevd x; echo status=$?");
    assert_eq!(out, "status=1\nstatus=2\n");
    assert_eq!(
        err,
        "prevd: no folder behind this one\nprevd: usage: prevd [-l] [COUNT]\n"
    );
}

#[test]
fn prevd_l_lists_the_history_around_the_current_folder() {
    let (out, _) = after_visiting_a_b_c("prevd -l | sed 's/[^ ]*[/\\\\]//'");
    // Behind: the start folder, then a; current: b; ahead: c.
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 4, "{out}");
    assert!(lines[0].starts_with("  2) "), "{out}");
    assert_eq!(lines[1], "  1) a", "{out}");
    assert_eq!(lines[2].trim(), "b", "{out}");
    assert_eq!(lines[3].trim(), "1) c", "{out}");
}

#[test]
fn cdh_offers_recent_folders_most_recent_first() {
    // The listing goes to a file, with each folder shortened to its name.
    let (out, err) = after_visiting_a_b_c(concat!(
        "cdh <<< 2 > ../listing; here\n",
        "sed 's/) .*[/\\\\]/) /' ../listing\n",
    ));
    assert_eq!(err, "");
    // Behind c: b (1), a (2) and the start folder (3), oldest at the top.
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "a", "{out}");
    assert!(lines[1].starts_with("  3) "), "{out}");
    assert_eq!(
        &lines[2..],
        ["  2) a", "  1) b", "Select a folder by number: "]
    );
}

#[test]
fn cdh_with_a_folder_goes_there_and_records_it() {
    let (out, err) = after_visiting_a_b_c("cdh ../a; here; prevd; here");
    assert_eq!(err, "");
    assert_eq!(out, "a\nc\n");
}
