//! The acceptance corpus of real scripts — **M2**, and the executable form of **D2**.
//!
//! D2 promises that unmodified POSIX-shaped scripts run as-is. `acceptance.rs` proves
//! individual decisions; this proves the thing the decisions are *for*, by running
//! scripts written the way they are actually written on Linux — no Windows
//! accommodations, no cash-specific spellings.

#![cfg(windows)]

use std::path::PathBuf;
use std::process::Command;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

fn corpus_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/cash; the corpus lives at the repository root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("corpus")
}

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

fn run_script(name: &str) -> Run {
    let script = corpus_dir().join(name);
    assert!(script.is_file(), "corpus script missing: {}", script.display());

    let out = Command::new(CASH)
        .arg(script.to_string_lossy().replace('\\', "/"))
        .output()
        .expect("failed to run cash");

    Run {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

#[test]
fn the_terraform_wrapper_runs_unmodified() {
    // Every construct in this script is one that appears in real infrastructure
    // wrappers: set -euo pipefail, an EXIT trap, mktemp, command substitution over a
    // CRLF file, $(pwd) composed into an argument, case, functions with locals, arrays,
    // parameter expansion with defaults, arithmetic, a here-document, a read loop,
    // process substitution, and a status checked against a conditional.
    //
    // The script self-checks: any mismatch exits non-zero with a diagnostic, so the
    // assertions here are about the script completing, not about matching a golden file.
    let run = run_script("terraform-wrapper.sh");

    assert_eq!(
        run.code, 0,
        "the wrapper failed.\nstdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );

    for expected in [
        "environment=staging region=eu-west-1 parallelism=10",
        "version=v1.4.2 modules=4 changed=1",
        "approve=-auto-approve",
        "ok",
    ] {
        assert!(
            run.stdout.contains(expected),
            "missing {expected:?} in:\n{}",
            run.stdout
        );
    }
}

#[test]
fn the_terraform_wrapper_matches_real_bash() {
    // D43 keeps the differential suite on Linux because §4's divergences would make a
    // bash reference report false failures. This script is deliberately written to avoid
    // every one of them, so its output *should* match bash exactly — which makes it a
    // genuine cross-check rather than a restatement of cash's own behaviour.
    //
    // Skipped where no reference bash exists.
    let bash = PathBuf::from(r"C:\Program Files\Git\bin\bash.exe");
    if !bash.is_file() {
        eprintln!("skipped: no reference bash available");
        return;
    }

    let script = corpus_dir().join("terraform-wrapper.sh");
    let reference = Command::new(&bash)
        .arg(script.to_string_lossy().replace('\\', "/"))
        .output()
        .expect("failed to run reference bash");

    let bash_stdout = String::from_utf8_lossy(&reference.stdout).trim_end().to_string();
    let cash_stdout = run_script("terraform-wrapper.sh").stdout;

    assert_eq!(
        cash_stdout, bash_stdout,
        "cash and bash disagree on a script written to avoid every §4 divergence"
    );
    assert_eq!(reference.status.code().unwrap_or(-1), 0, "the reference run itself failed");
}

#[test]
fn a_trap_can_remove_a_directory_it_moved_out_of() {
    // §4: a bundled builtin re-enters the binary as a child process and inherits the
    // shell's working directory, so `cd "$d"; rm -rf "$d"` is a process deleting its own
    // current directory — which Windows refuses.
    //
    // The workaround is to leave the directory first, which is good practice regardless.
    // Asserting it here means the recommended form stays working.
    let out = Command::new(CASH)
        .args([
            "-c",
            r#"d=$(mktemp -d); trap 'cd /; rm -rf "$d"' EXIT; cd "$d"; echo worked"#,
        ])
        .output()
        .expect("failed to run cash");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stdout.contains("worked"), "script failed: {stdout} {stderr}");
    assert!(
        !stderr.contains("cannot remove"),
        "cleanup failed even after leaving the directory: {stderr}"
    );
}
