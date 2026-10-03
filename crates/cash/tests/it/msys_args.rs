//! Arguments reach MSYS2 programs exactly as the script wrote them.
//!
//! Git for Windows' `usr/bin` tools split their command line by Cygwin's rules, not the
//! Microsoft C runtime's, and glob any word with a wildcard in it. Encoding their
//! arguments the Microsoft way turned `JSON.sh`'s `"[^[:cntrl:]"\\]*"|[[:space:]]+` into
//! `\[^[:cntrl:]"\]*"|[[:space:]]+` on the way to `grep`, which then refused it.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, cash_command, git_for_windows};

/// Git for Windows' `usr/bin`, its MSYS2 tools, with forward slashes. The scripts below
/// get it as `$U`.
fn usr_bin() -> String {
    format!("{}/usr/bin", git_for_windows())
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
    let printf = format!("{}/printf.exe", usr_bin());

    // Files for a glob to find and a response file for `@file` to read, so that either
    // mistake changes the output rather than passing by luck.
    let dir = Scratch::new("msys-args");
    std::fs::write(dir.join("one.rs"), "").unwrap();
    std::fs::write(dir.join("two.rs"), "").unwrap();
    std::fs::write(dir.join("file"), "INJECTED").unwrap();
    std::fs::write(dir.join("a"), "").unwrap();

    let args: Vec<String> = HOSTILE.iter().map(|arg| sh_quote(arg)).collect();
    let script = format!("{} '<%s>' {}", sh_quote(&printf), args.join(" "));
    let out = cash_command()
        .args(["-c", &script])
        .current_dir(dir.path())
        .output()
        .expect("run cash");

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
/// fragment that runs `$E "$A"` some way, with Git's `usr/bin` as `$U` and first on `PATH`.
fn echoed_via(how: &str) -> String {
    let script = format!(
        r#"E="$U/echo.exe"; A='"x\\"|[[:space:]]+'
        PATH="$U:$PATH"
        {how}"#
    );
    let out = cash_command()
        .env("U", usr_bin())
        .args(["-c", &script])
        .output()
        .expect("run cash");
    String::from_utf8_lossy(&out.stdout).trim_end().to_owned()
        + &String::from_utf8_lossy(&out.stderr)
}

// No space in it: a space made Rust wrap the argument in quotes, which Cygwin happens to
// decode correctly, so it hid the bug.
const SENT: &str = r#""x\\"|[[:space:]]+"#;

#[test]
fn exec_by_bare_name_encodes_for_msys() {
    // `exec grep -E "$@"` is Git's own `egrep`; the name is resolved late, by
    // `Command::new`, which is where the encoding used to be decided without looking.
    let out = echoed_via(r#"exec echo.exe "$A""#);
    assert_eq!(out, SENT);
}

#[test]
fn xargs_encodes_for_msys() {
    let out = echoed_via(r#"printf '%s' "$A" | xargs -0 "$E""#);
    assert_eq!(out, SENT);
}

#[test]
fn find_exec_encodes_for_msys() {
    let out = echoed_via(r#"find . -maxdepth 0 -exec "$E" "$A" \;"#);
    assert_eq!(out, SENT);
}

// The bundled `env` and `timeout` (uutils) spawn their command with `Command` themselves;
// cash hands them itself as the command, and passes the arguments on encoded for MSYS2.

#[test]
fn env_encodes_for_msys() {
    let out = echoed_via(r#"env "$E" "$A""#);
    assert_eq!(out, SENT);
}

#[test]
fn env_with_options_and_assignments_encodes_for_msys() {
    let out = echoed_via(r#"env -u NOPE -C / FOO=1 echo.exe "$A""#);
    assert_eq!(out, SENT);
}

#[test]
fn env_split_string_encodes_for_msys() {
    let out = echoed_via(r#"env -S 'FOO=1 echo.exe' "$A""#);
    assert_eq!(out, SENT);
}

#[test]
fn env_passes_its_assignments_through_the_relay() {
    let out = echoed_via(r#"env CASH_MSYS_PROBE="$A" "$U/printenv.exe" CASH_MSYS_PROBE"#);
    assert_eq!(out, SENT);
}

#[test]
fn env_reports_the_programs_exit_status() {
    let out = echoed_via(r#"env "$U/false.exe"; echo "$?""#);
    assert_eq!(out, "1");
}

#[test]
fn timeout_encodes_for_msys() {
    let out = echoed_via(r#"timeout -s KILL 30 "$E" "$A""#);
    assert_eq!(out, SENT);
    let out = echoed_via(r#"timeout --foreground 30 echo.exe "$A""#);
    assert_eq!(out, SENT);
}

#[test]
fn timeout_still_kills_an_msys_program() {
    // In the foreground `timeout` kills only its own child, the relay, so this also shows
    // the program dying with it: a surviving `sleep` would hold the output pipe open.
    for how in ["", "--foreground"] {
        let started = std::time::Instant::now();
        let script = format!(r#"timeout {how} 1 "$U/sleep.exe" 30; echo "$?""#);
        let out = echoed_via(&script);
        assert_eq!(out, "124", "timeout {how}");
        assert!(
            started.elapsed().as_secs() < 20,
            "took {:?}",
            started.elapsed()
        );
    }
}

#[test]
fn git_egrep_script_gets_json_sh_pattern_intact() {
    // The exact failure on GitHub's runner: Git's extensionless `egrep` script, reached
    // by full path so that no native egrep.exe earlier on PATH can stand in for it.
    let script = r#"CHAR='[^[:cntrl:]"\\]'; SPACE='[[:space:]]+'
        PATH="$U:$PATH"
        printf '"ab" c\n' | "$U/egrep" -ao "\"$CHAR*\"|$SPACE" | od -An -c | tr -s ' '"#;
    let out = cash_command()
        .env("U", usr_bin())
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
    let grep = format!("{}/grep.exe", usr_bin());
    let script = format!(
        r#"CHAR='[^[:cntrl:]"\\]'; SPACE='[[:space:]]+'
        printf '"ab" c\n' | '{grep}' -Eo "\"$CHAR*\"|$SPACE" | od -An -c | tr -s ' '"#
    );
    let out = cash_command()
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
