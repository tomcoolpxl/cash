//! `cash --init-rc`: a starter `~/.bashrc` for a user who has no startup file — **D69**.
//!
//! Scoop's manifest runs `cash --init-rc --once` after each install and update. Each test
//! has a home and a `%LOCALAPPDATA%` of its own, so the user's own files are never read or
//! written.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CASH: &str = env!("CARGO_BIN_EXE_cash");

struct Dirs {
    _root: tempfile::TempDir,
    home: PathBuf,
    local: PathBuf,
}

fn dirs() -> Dirs {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let local = root.path().join("local");
    std::fs::create_dir(&home).unwrap();
    std::fs::create_dir(&local).unwrap();
    Dirs {
        _root: root,
        home,
        local,
    }
}

fn init_rc(dirs: &Dirs, extra: &[&str]) -> Output {
    Command::new(CASH)
        .arg("--init-rc")
        .args(extra)
        .env("HOME", &dirs.home)
        .env("LOCALAPPDATA", &dirs.local)
        .output()
        .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn bashrc(home: &Path) -> PathBuf {
    home.join(".bashrc")
}

#[test]
fn init_rc_writes_a_starter_that_cash_reads_without_complaint() {
    let d = dirs();
    let out = init_rc(&d, &[]);
    assert!(out.status.success(), "{out:?}");
    assert!(stdout(&out).contains("wrote"), "{out:?}");
    assert!(bashrc(&d.home).is_file(), "no ~/.bashrc written");

    // An interactive cash reads it: its aliases, prompt and history settings are there,
    // and no line of it fails, which would print an error. SHLVL 2 skips the banner.
    let shell = Command::new(CASH)
        .env("HOME", &d.home)
        .env("SHLVL", "2")
        .env_remove("WT_SESSION")
        .args([
            "--noprofile",
            "-i",
            "-c",
            "alias ll; type -t __prompt_command; echo \"HISTSIZE=$HISTSIZE\"",
        ])
        .output()
        .unwrap();
    let out = stdout(&shell);
    let err = String::from_utf8_lossy(&shell.stderr);
    assert!(out.contains("alias ll='ls -alFh'"), "{out}{err}");
    assert!(out.contains("function"), "no prompt function: {out}{err}");
    assert!(out.contains("HISTSIZE=50000"), "{out}{err}");
    assert!(err.is_empty(), "the starter complained: {err}");
}

#[test]
fn init_rc_leaves_a_users_own_startup_file_alone() {
    for name in [".bashrc", ".cashrc"] {
        let d = dirs();
        std::fs::write(d.home.join(name), "# mine\n").unwrap();
        let out = init_rc(&d, &[]);
        assert!(out.status.success(), "{out:?}");
        assert!(stdout(&out).contains("exists"), "{out:?}");
        assert_eq!(
            std::fs::read_to_string(d.home.join(name)).unwrap(),
            "# mine\n"
        );
        if name == ".cashrc" {
            assert!(!bashrc(&d.home).exists(), "a starter beside ~/.cashrc");
        }
    }
}

/// Scoop runs `--once` after every update: a starter the user deleted stays deleted.
#[test]
fn init_rc_once_does_not_bring_back_a_deleted_starter() {
    let d = dirs();
    assert!(stdout(&init_rc(&d, &["--once"])).contains("wrote"));
    std::fs::remove_file(bashrc(&d.home)).unwrap();

    let again = init_rc(&d, &["--once"]);
    assert!(again.status.success(), "{again:?}");
    assert!(!bashrc(&d.home).exists(), "the deleted starter came back");
    // Scoop shows its output after every update: nothing done, nothing said.
    assert_eq!(stdout(&again), "", "{again:?}");

    // By hand, it writes one.
    assert!(stdout(&init_rc(&d, &[])).contains("wrote"));
}
