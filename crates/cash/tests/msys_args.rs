//! Arguments reach MSYS2 programs exactly as the script wrote them.
//!
//! Git for Windows' `usr/bin` tools split their command line by Cygwin's rules, not the
//! Microsoft C runtime's, and glob any word with a wildcard in it. Encoding their
//! arguments the Microsoft way turned `JSON.sh`'s `"[^[:cntrl:]"\\]*"|[[:space:]]+` into
//! `\[^[:cntrl:]"\]*"|[[:space:]]+` on the way to `grep`, which then refused it.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// Git's MSYS2 `printf`, or `None` where Git for Windows is not installed.
fn msys_printf() -> Option<PathBuf> {
    let path = Path::new(r"C:\Program Files\Git\usr\bin\printf.exe");
    path.is_file().then(|| path.to_path_buf())
}

/// `value` as one single-quoted shell word.
fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

const HOSTILE: &[&str] = &[
    r#""[^[:cntrl:]"\\]*"|[[:space:]]+"#,
    "^[[:space:]]+$",
    "a b",
    "it's",
    "\"",
    "\\",
    r"\\",
    r"a\",
    r#"x\"y"#,
    "*",
    "*.rs",
    "{a,b}",
    "(x)",
    "~",
    "~/x",
    "@file",
    "",
    r"C:\Program Files\",
    r"C:\*",
    r#"C:\a "b" c"#,
    r"C:\Windows\System32",
    r"a\b\c",
    "tab\there",
    "new\nline",
    "ünïcødé",
    "[",
    "]",
];

#[test]
fn hostile_arguments_survive_the_trip_to_an_msys_program() {
    let Some(printf) = msys_printf() else {
        eprintln!("skipping: Git for Windows' printf.exe is not installed");
        return;
    };

    // Files for a glob to find and a response file for `@file` to read, so that either
    // mistake changes the output rather than passing by luck.
    let dir = std::env::temp_dir().join("cash-msys-args");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("one.rs"), "").unwrap();
    std::fs::write(dir.join("two.rs"), "").unwrap();
    std::fs::write(dir.join("file"), "INJECTED").unwrap();
    std::fs::write(dir.join("a"), "").unwrap();

    let printf = printf.to_string_lossy().replace('\\', "/");
    let args: Vec<String> = HOSTILE.iter().map(|arg| sh_quote(arg)).collect();
    let script = format!("{} '<%s>' {}", sh_quote(&printf), args.join(" "));
    let out = Command::new(CASH)
        .args(["-c", &script])
        .current_dir(&dir)
        .output()
        .expect("run cash");
    let _ = std::fs::remove_dir_all(&dir);

    let stdout = String::from_utf8_lossy(&out.stdout);
    let expected: String = HOSTILE.iter().flat_map(|arg| ["<", arg, ">"]).collect();
    assert_eq!(
        stdout,
        expected,
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// What Git's `echo.exe` prints for `ARG` when cash reaches it by `how`, a script
/// fragment that runs `$E "$A"` some way; `None` where Git for Windows is absent.
fn echoed_via(how: &str) -> Option<String> {
    let echo = Path::new(r"C:\Program Files\Git\usr\bin\echo.exe");
    if !echo.is_file() {
        eprintln!("skipping: Git for Windows' echo.exe is not installed");
        return None;
    }
    let script = format!(
        r#"E='C:/Program Files/Git/usr/bin/echo.exe'; A='"x\\"|[[:space:]]+'
        PATH="C:/Program Files/Git/usr/bin:$PATH"
        {how}"#
    );
    let out = Command::new(CASH)
        .args(["-c", &script])
        .output()
        .expect("run cash");
    Some(
        String::from_utf8_lossy(&out.stdout).trim_end().to_owned()
            + &String::from_utf8_lossy(&out.stderr),
    )
}

// No space in it: a space made Rust wrap the argument in quotes, which Cygwin happens to
// decode correctly, so it hid the bug.
const SENT: &str = r#""x\\"|[[:space:]]+"#;

#[test]
fn exec_by_bare_name_encodes_for_msys() {
    // `exec grep -E "$@"` is Git's own `egrep`; the name is resolved late, by
    // `Command::new`, which is where the encoding used to be decided without looking.
    if let Some(out) = echoed_via(r#"exec echo.exe "$A""#) {
        assert_eq!(out, SENT);
    }
}

#[test]
fn xargs_encodes_for_msys() {
    if let Some(out) = echoed_via(r#"printf '%s' "$A" | xargs -0 "$E""#) {
        assert_eq!(out, SENT);
    }
}

#[test]
fn find_exec_encodes_for_msys() {
    if let Some(out) = echoed_via(r#"find . -maxdepth 0 -exec "$E" "$A" \;"#) {
        assert_eq!(out, SENT);
    }
}

#[test]
fn git_egrep_script_gets_json_sh_pattern_intact() {
    // The exact failure on GitHub's runner: Git's extensionless `egrep` script, reached
    // by full path so that no native egrep.exe earlier on PATH can stand in for it.
    let egrep = Path::new(r"C:\Program Files\Git\usr\bin\egrep");
    if !egrep.is_file() {
        eprintln!("skipping: Git for Windows' egrep is not installed");
        return;
    }
    let script = r#"CHAR='[^[:cntrl:]"\\]'; SPACE='[[:space:]]+'
        PATH="C:/Program Files/Git/usr/bin:$PATH"
        printf '"ab" c\n' | 'C:/Program Files/Git/usr/bin/egrep' -ao "\"$CHAR*\"|$SPACE" | od -An -c | tr -s ' '"#;
    let out = Command::new(CASH)
        .args(["-c", script])
        .output()
        .expect("run cash");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        r#"" a b " \n \n"#,
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn json_sh_patterns_reach_git_grep_intact() {
    let grep = Path::new(r"C:\Program Files\Git\usr\bin\grep.exe");
    if !grep.is_file() {
        eprintln!("skipping: Git for Windows' grep.exe is not installed");
        return;
    }
    let grep = grep.to_string_lossy().replace('\\', "/");
    let script = format!(
        r#"CHAR='[^[:cntrl:]"\\]'; SPACE='[[:space:]]+'
        printf '"ab" c\n' | '{grep}' -Eo "\"$CHAR*\"|$SPACE" | od -An -c | tr -s ' '"#
    );
    let out = Command::new(CASH)
        .args(["-c", &script])
        .output()
        .expect("run cash");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        r#"" a b " \n \n"#,
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
