//! Git's prompt scripts under cash, against Git Bash's output frozen in `tests/git-prompt`
//! (BIN-05).
//!
//! `tests/git-prompt-differential.sh` makes about forty fixture repositories, one per state
//! `__git_ps1` reports, and runs 115 cases of `__git_ps1`, Git for Windows'
//! `profile.d/git-prompt.sh` and `git-completion.bash` in them. It ran by hand only, with
//! Git Bash beside cash. `--freeze` writes Git Bash's output for each case and the hashes
//! of the scripts it came from; `--check`, which this runs, gives every case to cash alone
//! and compares.
//!
//! It runs in the Git Bash of the Git for Windows that provides the scripts: the fixtures
//! are made with git, and only the cases run under cash. Without Git for Windows the test
//! fails rather than passing without a word, and when Git for Windows updates its scripts
//! it fails and says to re-freeze, as decided on 2026-10-04.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a \
              failed set-up should abort it loudly, saying what it found"
)]

use std::path::PathBuf;
use std::process::Command;

/// The `bash.exe` of the Git for Windows whose `git` is on `PATH`.
fn git_bash() -> PathBuf {
    let output = Command::new("git")
        .arg("--exec-path")
        .output()
        .expect("git, from Git for Windows, which provides the prompt scripts under test");
    let exec_path = String::from_utf8_lossy(&output.stdout)
        .trim()
        .replace('\\', "/");
    let root = exec_path
        .strip_suffix("/mingw64/libexec/git-core")
        .unwrap_or_else(|| {
            panic!("`git --exec-path` is {exec_path:?}, not Git for Windows' mingw64 layout")
        });
    let bash = PathBuf::from(root).join("bin/bash.exe");
    assert!(bash.is_file(), "no Git Bash at {}", bash.display());
    bash
}

#[test]
fn git_prompt_scripts_give_git_bashs_frozen_output() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cash = env!("CARGO_BIN_EXE_cash").replace('\\', "/");
    let output = Command::new(git_bash())
        .args(["tests/git-prompt-differential.sh", "--check", &cash])
        .current_dir(&root)
        // The script and the cases are non-interactive shells, which would source it.
        .env_remove("BASH_ENV")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("run the git-prompt differential");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the git-prompt differential failed ({}):\n{stdout}\n{stderr}",
        output.status
    );
    eprintln!("{}", stdout.lines().last().unwrap_or_default());
}
