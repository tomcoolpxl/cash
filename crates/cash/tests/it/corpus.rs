//! The acceptance corpus of real scripts — **M2**, and the executable form of **D2**.
//!
//! D2 promises that unmodified POSIX-shaped scripts run as-is. `acceptance.rs` proves
//! individual decisions; this proves the thing the decisions are *for*, by running
//! scripts written the way they are actually written on Linux — no Windows
//! accommodations, no cash-specific spellings.

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

use std::path::PathBuf;
use std::process::Command;

use crate::common::{cash_command, git_for_windows};

/// Every script in the corpus. Tests that apply to all of them iterate this, so adding a
/// script gets the cross-cutting coverage — CRLF endings, for one — without being asked.
const CORPUS_SCRIPTS: &[&str] = &["terraform-wrapper.sh", "ci-glue.sh"];

/// The reference bash, if this machine has one.
///
/// D43 rejects a bash reference for cash's own behaviour because §4's divergences would
/// make it report false failures. The corpus scripts are deliberately written to avoid
/// every one of them, so comparing against bash here is a genuine cross-check rather
/// than a restatement of cash's own behaviour.
/// Git Bash, the oracle: Git for Windows is a prerequisite (spec D35), so a machine
/// without it fails these tests rather than passing them unrun (BIN-20).
fn reference_bash() -> PathBuf {
    PathBuf::from(format!("{}/bin/bash.exe", git_for_windows()))
}

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
    assert!(
        script.is_file(),
        "corpus script missing: {}",
        script.display()
    );

    let out = cash_command()
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
    let bash = reference_bash();

    let script = corpus_dir().join("terraform-wrapper.sh");
    let reference = Command::new(&bash)
        .arg(script.to_string_lossy().replace('\\', "/"))
        .output()
        .expect("failed to run reference bash");

    let bash_stdout = String::from_utf8_lossy(&reference.stdout)
        .trim_end()
        .to_string();
    let cash_stdout = run_script("terraform-wrapper.sh").stdout;

    assert_eq!(
        cash_stdout, bash_stdout,
        "cash and bash disagree on a script written to avoid every §4 divergence"
    );
    assert_eq!(
        reference.status.code().unwrap_or(-1),
        0,
        "the reference run itself failed"
    );
}

#[test]
fn the_ci_glue_script_runs_unmodified() {
    // The other workload §1 names. Where terraform-wrapper.sh exercises shell *language*,
    // this exercises the shell as a process coordinator: find, xargs, grep, sort, uniq,
    // wc, cut, tr, pipefail, subshells, and redirection of both streams — the pieces D48
    // bundles and D8 resolves. A failure here means the userland story is broken rather
    // than the parser.
    let run = run_script("ci-glue.sh");

    assert_eq!(
        run.code, 0,
        "the ci glue failed.\nstdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );

    for expected in ["rust=3 todos=2 unique=3 envs=prod staging", "ok"] {
        assert!(
            run.stdout.contains(expected),
            "missing {expected:?} in:\n{}",
            run.stdout
        );
    }
}

#[test]
fn the_ci_glue_script_matches_real_bash() {
    let bash = reference_bash();

    let script = corpus_dir().join("ci-glue.sh");
    let reference = Command::new(&bash)
        .arg(script.to_string_lossy().replace('\\', "/"))
        .output()
        .expect("failed to run reference bash");

    let bash_stdout = String::from_utf8_lossy(&reference.stdout)
        .trim_end()
        .to_string();
    assert_eq!(
        reference.status.code().unwrap_or(-1),
        0,
        "the reference run itself failed"
    );
    assert_eq!(
        run_script("ci-glue.sh").stdout,
        bash_stdout,
        "cash and bash disagree on the ci glue script"
    );
}

#[test]
fn every_corpus_script_runs_with_crlf_endings() {
    // D7: `core.autocrlf` is `true` by default in Git for Windows, so a repository
    // checked out on this machine has CRLF scripts. `.gitattributes` keeps cash's own
    // corpus at LF, which means the CRLF case has to be manufactured here rather than
    // assumed — and it is the case most users will actually hit.
    let staging = std::env::temp_dir().join(format!("cash-corpus-crlf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).expect("create staging dir");

    for name in CORPUS_SCRIPTS {
        let lf = std::fs::read_to_string(corpus_dir().join(name)).expect("read corpus script");
        assert!(!lf.contains('\r'), "{name} is not stored with LF endings");

        let crlf_path = staging.join(name);
        std::fs::write(&crlf_path, lf.replace('\n', "\r\n").as_bytes()).expect("write crlf copy");

        let out = cash_command()
            .arg(crlf_path.to_string_lossy().replace('\\', "/"))
            .output()
            .expect("failed to run cash");

        let crlf_stdout = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
        let lf_run = run_script(name);

        assert_eq!(
            out.status.code().unwrap_or(-1),
            lf_run.code,
            "{name} exited differently with CRLF endings.\nstderr:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            crlf_stdout, lf_run.stdout,
            "{name} produced different output with CRLF endings"
        );
    }

    let _ = std::fs::remove_dir_all(&staging);
}

#[test]
fn a_trap_can_remove_a_directory_it_moved_out_of() {
    // §4: a bundled builtin re-enters the binary as a child process and inherits the
    // shell's working directory, so `cd "$d"; rm -rf "$d"` is a process deleting its own
    // current directory — which Windows refuses.
    //
    // The workaround is to leave the directory first, which is good practice regardless.
    // Asserting it here means the recommended form stays working.
    let out = cash_command()
        .args([
            "-c",
            r#"d=$(mktemp -d); trap 'cd /; rm -rf "$d"' EXIT; cd "$d"; echo worked"#,
        ])
        .output()
        .expect("failed to run cash");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("worked"),
        "script failed: {stdout} {stderr}"
    );
    assert!(
        !stderr.contains("cannot remove"),
        "cleanup failed even after leaving the directory: {stderr}"
    );
}
