//! Lookups agree with execution about extensionless commands — **D8**, **D46**.
//!
//! Git for Windows ships `/usr/bin/egrep` as a two-line `#!/bin/sh` script with no
//! extension. Running `egrep` worked, because execution falls back to the extensionless
//! file and dispatches it by its shebang (D8 step 4). But `type -a egrep` said
//! "not found", and `command -v` agreed with it rather than with execution: the lookup
//! only counted files with a `PATHEXT` extension as executable.
//!
//! In bash, `type`, `type -a`, `command -v`, `hash` and `which` all report what running
//! the name would run. Windows has no execute bit, so a file's contents stand in for it:
//! a `#!` line or a PE image makes an extensionless file executable.

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

use std::path::Path;
use std::process::Command;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

/// Runs `script` with `dir` as the only `PATH` entry ahead of Windows itself, so nothing
/// installed on the machine (Git, Scoop) can answer in its place.
fn cash_with_path(dir: &Path, script: &str) -> Output {
    let path = format!(r"{};C:\WINDOWS\system32;C:\WINDOWS", dir.display());
    let out = Command::new(CASH)
        .args(["-c", script])
        .env("PATH", path)
        .output()
        .expect("failed to run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// A directory holding an extensionless `#!/bin/sh` script, the shape of Git's `egrep`.
fn shebang_tool() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("scratch dir");
    std::fs::write(
        dir.path().join("mytool"),
        "#!/bin/sh\necho ran-mytool \"$@\"\n",
    )
    .expect("write script");
    dir
}

#[test]
fn execution_runs_the_extensionless_shebang_script() {
    // The half that already worked: pinned so the lookups below are checked against it.
    let dir = shebang_tool();
    let out = cash_with_path(dir.path(), "mytool a b");
    assert_eq!(out.stdout, "ran-mytool a b", "stderr: {}", out.stderr);
    assert_eq!(out.code, 0);
}

#[test]
fn type_a_finds_an_extensionless_shebang_script() {
    let dir = shebang_tool();
    let out = cash_with_path(dir.path(), "type -a mytool");
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.starts_with("mytool is ") && out.stdout.ends_with("/mytool"),
        "unexpected: {:?}",
        out.stdout
    );
    assert!(!out.stderr.contains("not found"), "stderr: {}", out.stderr);
}

#[test]
fn every_lookup_agrees_with_execution() {
    let dir = shebang_tool();
    for script in [
        "type mytool",
        "type -a mytool",
        "type -p mytool",
        "type -P mytool",
        "command -v mytool",
        "command -V mytool",
        "hash mytool",
        "which mytool",
    ] {
        let out = cash_with_path(dir.path(), script);
        assert_eq!(
            out.code, 0,
            "`{script}` denied a command that runs: {} {}",
            out.stdout, out.stderr
        );
    }

    let out = cash_with_path(dir.path(), "command -v mytool");
    assert!(
        out.stdout.ends_with("/mytool"),
        "command -v printed {:?}",
        out.stdout
    );

    // Only the file name is checked: `hash -t` does not yet render its path per D3, and
    // that is a separate defect from whether the lookup finds the file at all.
    let out = cash_with_path(dir.path(), "hash mytool; hash -t mytool");
    assert!(
        out.stdout.ends_with("mytool"),
        "hash -t printed {:?}",
        out.stdout
    );
}

#[test]
fn test_x_counts_a_shebang_script_as_executable() {
    let dir = shebang_tool();
    let out = cash_with_path(dir.path(), r#"[ -x "$(command -v mytool)" ] && echo yes"#);
    assert_eq!(out.stdout, "yes", "stderr: {}", out.stderr);
}

#[test]
fn an_extensionless_pe_image_is_found() {
    // A PE file needs no extension to be a program; its `MZ` header says so.
    let dir = tempfile::tempdir().expect("scratch dir");
    std::fs::copy(
        r"C:\WINDOWS\system32\whoami.exe",
        dir.path().join("mywhoami"),
    )
    .expect("copy whoami.exe");
    for script in ["type -a mywhoami", "command -v mywhoami"] {
        let out = cash_with_path(dir.path(), script);
        assert_eq!(out.code, 0, "`{script}`: {} {}", out.stdout, out.stderr);
        assert!(
            out.stdout.ends_with("/mywhoami"),
            "`{script}` printed {:?}",
            out.stdout
        );
    }
}

#[test]
fn a_plain_text_file_is_still_not_a_command_to_type_a() {
    // Contents are the execute bit: text with neither a shebang nor a PE header gets none.
    let dir = tempfile::tempdir().expect("scratch dir");
    std::fs::write(dir.path().join("notes"), "just some notes\n").expect("write notes");
    let out = cash_with_path(dir.path(), "type -a notes");
    assert_ne!(out.code, 0, "type -a listed a text file: {}", out.stdout);
}

#[test]
fn pathext_match_still_beats_the_extensionless_script() {
    // Execution prefers `tool.cmd` over `tool` in the same directory (D8), so the lookup
    // has to name the same file.
    let dir = tempfile::tempdir().expect("scratch dir");
    std::fs::write(dir.path().join("tool"), "#!/bin/sh\necho from-sh\n").expect("write");
    std::fs::write(dir.path().join("tool.cmd"), "@echo from-cmd\r\n").expect("write");

    let ran = cash_with_path(dir.path(), "tool");
    assert_eq!(ran.stdout, "from-cmd", "stderr: {}", ran.stderr);

    let out = cash_with_path(dir.path(), "command -v tool");
    assert!(
        out.stdout.to_ascii_lowercase().ends_with("/tool.cmd"),
        "command -v printed {:?}",
        out.stdout
    );
    let out = cash_with_path(dir.path(), "type -a tool");
    let first = out.stdout.lines().next().unwrap_or_default();
    assert!(
        first.to_ascii_lowercase().ends_with("/tool.cmd"),
        "type -a printed {:?}",
        out.stdout
    );
}
