//! `cd`'s failures, worded as bash words them.
//!
//! Every failed `cd` printed "cd: i/o error: The system cannot find the file specified.
//! (os error 2)", which names neither the directory nor the reason. That mattered most
//! for the commonest failure on Windows: an unquoted `cd C:\Users\me\src`, which bash's
//! lexer turns into `C:Usersmesrc` before `cd` sees it, so the message has to show that
//! word for the user to understand what happened.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::process::Command;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// Standard error and the exit status of `script`.
fn stderr_and_status(script: &str) -> (String, i32) {
    let out = Command::new(CASH)
        .args(["-c", script])
        .output()
        .expect("run cash");
    (
        String::from_utf8_lossy(&out.stderr).trim_end().to_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn a_missing_directory_is_named_with_bash_wording() {
    let (stderr, status) = stderr_and_status("cd no-such-directory-here");
    assert_eq!(
        stderr,
        "cd: no-such-directory-here: No such file or directory"
    );
    assert_eq!(status, 1);
}

#[test]
fn a_file_is_not_a_directory() {
    let (stderr, status) = stderr_and_status("cd Cargo.toml");
    assert_eq!(stderr, "cd: Cargo.toml: Not a directory");
    assert_eq!(status, 1);
}

#[test]
fn physical_mode_reports_the_same_way() {
    let (stderr, _) = stderr_and_status("cd -P no-such-directory-here");
    assert_eq!(
        stderr,
        "cd: no-such-directory-here: No such file or directory"
    );
}

#[test]
fn a_path_that_lost_its_backslashes_gets_a_hint() {
    let (stderr, status) = stderr_and_status(r"cd C:\no\such\place");
    let mut lines = stderr.lines();
    assert_eq!(
        lines.next(),
        Some("cd: C:nosuchplace: No such file or directory")
    );
    let hint = lines.next().unwrap_or_default();
    assert!(
        hint.starts_with("cd: hint:") && hint.contains("C:/dir/sub"),
        "{stderr}"
    );
    assert_eq!(status, 1);
}

#[test]
fn an_ordinary_failure_gets_no_hint() {
    for script in [
        "cd C:/no/such/place",
        "cd nowhere",
        r"cd 'C:\no\such\place'",
    ] {
        let (stderr, _) = stderr_and_status(script);
        assert!(!stderr.contains("hint"), "{script}: {stderr}");
    }
}
