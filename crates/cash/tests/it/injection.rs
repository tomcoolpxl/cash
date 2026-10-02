//! Text that must never become a command.
//!
//! Each case here once ran something its author did not write: a URL handed to `start`,
//! whose `&` reached `cmd.exe` unquoted (`REVIEW_REPORT.md` BI-06); a key that `unset`,
//! `read` or `[[ -v ]]` expands a second time, spelt in a form the hardening of spec §4
//! row 37 did not recognise (LANG-04). Each test watches for a side effect — a file that
//! must not appear — rather than for output, which a command substitution would swallow.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result. Shell snippets are spelled with hashes \
              throughout, including where they are not strictly needed."
)]

use std::path::{Path, PathBuf};
use std::process::Command;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

/// Runs `script` with `dir` as cash's working directory.
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

/// A scratch folder of the test's own, empty.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cash-injection-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn start_hands_an_ampersand_to_no_command_processor() {
    let dir = scratch("start");
    // No such file, so nothing opens; with `cmd /c start` in between, `&` ran the rest.
    let out = cash_in(
        &dir,
        "start 'no-such-file.txt&echo x>pwned'; echo \"rc=$?\"",
    );
    assert!(
        !dir.join("pwned").exists(),
        "the text after & ran: {}",
        out.stderr
    );
    assert_eq!(out.stdout, "rc=1", "{}", out.stderr);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stderr
            .starts_with("start: no-such-file.txt&echo x>pwned:"),
        "{}",
        out.stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}
