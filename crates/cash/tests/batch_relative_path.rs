//! A batch file run by a path with forward slashes — **D8**.
//!
//! cash runs `.cmd` and `.bat` files through `cmd.exe /d /s /c`. Handed the command as
//! typed, `./showargs.cmd .` reached `cmd` as `./showargs.cmd`, which it reads as the
//! command `.` followed by the switch `/showargs.cmd`: "'.' is not recognized". Git Bash
//! prints `all=[.]`. The same holds for `../x.cmd`, `sub/x.cmd` and paths with spaces.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result."
)]

use std::path::Path;
use std::process::Command;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

const SHOWARGS: &str = "@echo off\r\necho all=[%*]\r\necho dp0=[%~dp0]\r\n";

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

fn cash_in(dir: &Path, script: &str) -> Output {
    let out = Command::new(CASH)
        .args(["-c", script])
        .current_dir(dir)
        .output()
        .expect("failed to run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// `root/sub dir/showargs.cmd`, plus `root/sub dir/inner/`, to run it from below.
fn layout() -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("scratch dir");
    let sub = root.path().join("sub dir");
    std::fs::create_dir_all(sub.join("inner")).expect("mkdir");
    std::fs::write(sub.join("showargs.cmd"), SHOWARGS).expect("write script");
    let sub = sub.canonicalize().expect("canonicalize");
    (root, sub)
}

/// The folder a `dp0=[…\\]` line names, resolved. `cmd` spells `%~dp0` the way the path
/// reached it, which under an 8.3 `TEMP` (`C:\\Users\\RUNNER~1`, as GitHub's runner has)
/// is the short form; resolving both sides compares the folders, not their spellings.
fn dp0_folder(line: &str) -> std::path::PathBuf {
    let folder = line
        .strip_prefix("dp0=[")
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or_else(|| panic!("not a dp0 line: {line}"));
    assert!(
        folder.ends_with('\\'),
        "%~dp0 ends with a backslash: {line}"
    );
    Path::new(folder)
        .canonicalize()
        .unwrap_or_else(|e| panic!("{folder}: {e}"))
}

fn check(dir: &Path, script: &str, sub: &Path) {
    let out = cash_in(dir, script);
    assert_eq!(out.code, 0, "{script}: stderr: {}", out.stderr);
    let mut lines = out.stdout.lines();
    assert_eq!(
        lines.next(),
        Some("all=[. a b]"),
        "{script}: {}",
        out.stdout
    );
    let dp0 = lines.next().unwrap_or_default();
    assert_eq!(dp0_folder(dp0), sub, "{script}: {dp0}");
}

#[test]
fn dot_slash_batch_file_receives_its_arguments() {
    let (_root, sub) = layout();
    check(&sub, "./showargs.cmd . a b", &sub);
}

#[test]
fn dot_dot_slash_batch_file_receives_its_arguments() {
    let (_root, sub) = layout();
    check(&sub.join("inner"), "../showargs.cmd . a b", &sub);
}

#[test]
fn subdirectory_batch_file_with_a_space_receives_its_arguments() {
    let (root, sub) = layout();
    check(root.path(), "'sub dir/showargs.cmd' . a b", &sub);
}

#[test]
fn absolute_forward_slash_batch_file_receives_its_arguments() {
    let (root, sub) = layout();
    let abs = sub
        .join("showargs.cmd")
        .display()
        .to_string()
        .replace('\\', "/");
    let abs = abs.strip_prefix("//?/").unwrap_or(&abs).to_string();
    check(root.path(), &format!("'{abs}' . a b"), &sub);
}
