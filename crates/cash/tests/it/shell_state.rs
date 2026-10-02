//! The shell's state is the shell's, not the process's (D10).
//!
//! cash never changes its own working directory, and `export` never touches the process
//! environment: both live in the shell, so that a subshell or a background job can have
//! its own. Anything that reaches for the process's copy instead gets the folder cash was
//! started in and the environment it was started with (`REVIEW_REPORT.md` §4.1).

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly"
)]

use std::path::Path;
use std::process::Command;

use crate::common::{Scratch, run_in};

/// What `program` from System32 prints, run directly.
fn output_of_system_program(program: &str) -> String {
    let path = Path::new(r"C:\Windows\System32").join(program);
    let out = Command::new(path).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim_end().to_owned()
}

#[test]
fn a_relative_program_runs_from_the_shells_folder() {
    // Two programs of one name, `tool.exe` in the folder cash starts in and in `sub`.
    // They print different things, so the output says which one ran.
    let scratch = Scratch::new("relative-program");
    std::fs::create_dir(scratch.path().join("sub")).unwrap();
    std::fs::copy(
        r"C:\Windows\System32\whoami.exe",
        scratch.path().join("tool.exe"),
    )
    .unwrap();
    std::fs::copy(
        r"C:\Windows\System32\hostname.exe",
        scratch.path().join("sub").join("tool.exe"),
    )
    .unwrap();
    std::fs::copy(
        r"C:\Windows\System32\hostname.exe",
        scratch.path().join("sub").join("only-here.exe"),
    )
    .unwrap();
    let hostname = output_of_system_program("hostname.exe");

    // `cd sub && ./tool.exe` ran the start folder's `tool.exe`, and `./only-here.exe` was
    // "command not found", because Windows resolved them against cash's own folder.
    let out = run_in(
        scratch.path(),
        "cd sub && ./tool.exe && ./tool && ./only-here.exe",
    );
    let lines: Vec<&str> = out.stdout.lines().map(str::trim_end).collect();
    assert_eq!(lines, [hostname.as_str(); 3], "{}", out.stderr);

    // A program named with forward slashes is started by its Windows spelling: cmd.exe
    // read the `/` in its own command line as a switch.
    let out = run_in(
        scratch.path(),
        "C:/Windows/System32/cmd.exe /d /c exit 4; echo $?",
    );
    assert_eq!(out.stdout, "4", "{}", out.stderr);

    // A relative entry in PATH is the shell's folder's too.
    let out = run_in(scratch.path(), r#"PATH=".:$PATH"; cd sub && only-here"#);
    assert_eq!(out.stdout, hostname, "{}", out.stderr);
}
