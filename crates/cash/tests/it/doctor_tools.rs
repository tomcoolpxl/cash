//! `cash doctor` on Scoop and git, the two it names first: missing, each is a warning with
//! the way to install it; found, each is a line that passed.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::process::Command;

use crate::common::{CASH, doctor_finding, doctor_findings};

fn doctor(path: &str) -> String {
    let out = Command::new(CASH)
        .arg("doctor")
        .env("PATH", path)
        .output()
        .expect("run cash doctor");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Windows' System32, from `SystemRoot`, on whichever drive Windows is.
fn system32() -> String {
    let root = std::env::var("SystemRoot").expect("SystemRoot is set");
    format!("{root}\\System32")
}

#[test]
fn without_scoop_and_git_both_are_warned_of_first_with_the_way_to_install_them() {
    let report = doctor(&system32());
    let scoop = doctor_finding(&report, "Scoop not found").expect("a Scoop finding");
    assert_eq!(scoop.level, "WARN", "{report}");
    assert!(
        scoop
            .fix
            .as_deref()
            .is_some_and(|fix| fix.contains("irm get.scoop.sh | iex")),
        "{report}"
    );
    let git = doctor_finding(&report, "git not found").expect("a git finding");
    assert_eq!(git.level, "WARN", "{report}");
    assert!(
        git.fix
            .as_deref()
            .is_some_and(|fix| fix.contains("scoop install git") && fix.contains("Git.Git")),
        "{report}"
    );
    let warnings: Vec<String> = doctor_findings(&report)
        .into_iter()
        .filter(|finding| finding.level == "WARN")
        .map(|finding| finding.text)
        .collect();
    assert!(warnings[0].starts_with("Scoop not found"), "{report}");
    assert!(warnings[1].starts_with("git not found"), "{report}");
}

#[test]
fn found_scoop_and_git_pass() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("scoop.cmd"), "@echo off\n").unwrap();
    std::fs::write(dir.path().join("git.exe"), b"MZ").unwrap();
    let report = doctor(&format!("{};{}", dir.path().display(), system32()));
    assert_eq!(
        doctor_finding(&report, "Scoop installs").map(|f| f.level),
        Some("ok".to_owned()),
        "{report}"
    );
    assert_eq!(
        doctor_finding(&report, "git: ").map(|f| f.level),
        Some("ok".to_owned()),
        "{report}"
    );
}
