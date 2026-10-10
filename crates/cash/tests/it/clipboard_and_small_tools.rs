//! `uuidgen`, `xdg-open`, `pbcopy` and `pbpaste`.
//!
//! `uuidgen` is checked against util-linux 2.42.3 case by case: the script in
//! `tests/oracle` ran under the real tool to make the `.out` file, and runs here under
//! cash. `xdg-open` is checked for its syntax, its exit codes and its version; nothing is
//! opened. The clipboard is one per desktop, so one test does the whole round trip and
//! puts back what it found.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::io::Write;
use std::process::Stdio;

use crate::common::{Scratch, cash_command, golden, run, run_in, run_oracle_script};

#[test]
fn uuidgen_matches_util_linux() {
    assert_eq!(run_oracle_script("uuidgen_cases"), golden("uuidgen_cases"));
}

#[test]
fn uuidgen_names_itself_and_prints_util_linux_s_help() {
    let out = run("uuidgen -V");
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, "uuidgen (cash): util-linux 2.42.3's options");

    let out = run("uuidgen --help");
    assert_eq!(out.code, 0, "{}", out.stderr);
    // util-linux's help begins with a blank line, and so does this.
    assert!(
        out.stdout.starts_with("\nUsage:\n uuidgen [options]"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout
            .contains(" -n, --namespace <ns>  generate hash-based uuid in this namespace")
    );
    assert!(out.stdout.contains(" -V, --version       display version"));

    let page = run("help uuidgen");
    assert_eq!(page.code, 0, "{}", page.stderr);
    assert!(page.stdout.contains("--sha1"), "{}", page.stdout);
}

#[test]
fn xdg_open_takes_one_target_and_exits_as_the_real_one_does() {
    let scratch = Scratch::new("xdg-open");
    let dir = scratch.path();

    let out = run_in(dir, "xdg-open; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert!(
        out.stderr.contains("xdg-open { file | URL }"),
        "{}",
        out.stderr
    );

    let out = run_in(dir, "xdg-open a b; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert_eq!(
        out.stderr,
        "xdg-open: unexpected argument 'b'\nTry 'xdg-open --help' for more information."
    );

    let out = run_in(dir, "xdg-open --bogus; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert_eq!(
        out.stderr,
        "xdg-open: unexpected option '--bogus'\nTry 'xdg-open --help' for more information."
    );

    let out = run_in(dir, "xdg-open no-such-file.txt; echo rc=$?");
    assert_eq!(out.stdout, "rc=2");
    assert_eq!(
        out.stderr,
        "xdg-open: file 'no-such-file.txt' does not exist"
    );

    // A drive letter is a path, not a scheme.
    let out = run_in(dir, "xdg-open C:/no/such/folder/x.txt; echo rc=$?");
    assert_eq!(out.stdout, "rc=2");
    assert_eq!(
        out.stderr,
        "xdg-open: file 'C:/no/such/folder/x.txt' does not exist"
    );

    // Status 4, Windows refusing, is not provoked here: an unknown scheme makes Windows
    // offer to find a program for it rather than fail, and nothing may open in a test.

    let out = run_in(dir, "xdg-open --version");
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        "xdg-open (cash): start, under the name cross-platform scripts try first"
    );

    let out = run_in(dir, "xdg-open --help");
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("Synopsis"), "{}", out.stdout);
    assert!(
        out.stdout
            .contains("xdg-open { --help | --manual | --version }")
    );
    let manual = run_in(dir, "xdg-open --manual");
    assert_eq!(manual.code, 0);
    assert!(manual.stdout.contains("Exit Codes"), "{}", manual.stdout);

    // What a script does to find it.
    let out = run_in(dir, "command -v xdg-open; type -t xdg-open");
    assert_eq!(out.stdout, "xdg-open\nbuiltin");

    let page = run("help xdg-open");
    assert_eq!(page.code, 0, "{}", page.stderr);
    assert!(
        page.stdout.contains("it is start under the name"),
        "{}",
        page.stdout
    );
    assert!(page.stdout.contains("EXIT CODES"), "{}", page.stdout);
}

/// Runs `script` in `dir` with `stdin` fed to it; standard output byte for byte.
fn cash_with_stdin(dir: &std::path::Path, script: &str, stdin: &[u8]) -> (Vec<u8>, String, i32) {
    let mut child = cash_command()
        .args(["-c", script])
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run cash");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin)
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait for cash");
    (
        out.stdout,
        String::from_utf8_lossy(&out.stderr).trim_end().to_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn pbcopy_and_pbpaste_round_trip_through_the_clipboard() {
    // One test for everything the clipboard is used for: it is one per desktop, and
    // nextest runs tests in parallel. What was there is put back at the end.
    let before = cash_win32::clipboard::get_text().expect("read the clipboard");
    let scratch = Scratch::new("pbcopy");
    let dir = scratch.path();

    // LF in, CRLF on the clipboard, LF out; UTF-8 both ways.
    let (stdout, stderr, code) = cash_with_stdin(
        dir,
        "pbcopy && pbpaste",
        "one\ntwo h\u{e9}llo \u{20ac}\n".as_bytes(),
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stderr, "");
    assert_eq!(
        String::from_utf8_lossy(&stdout),
        "one\ntwo h\u{e9}llo \u{20ac}\n"
    );
    assert_eq!(
        cash_win32::clipboard::get_text().expect("read the clipboard"),
        Some("one\r\ntwo h\u{e9}llo \u{20ac}\r\n".to_owned())
    );

    // A file with LF comes back the same through a file.
    let text = "line 1\nline 2\n\nlast, no newline";
    std::fs::write(dir.join("in.txt"), text).expect("write in.txt");
    let out = run_in(dir, "pbcopy < in.txt && pbpaste > out.txt; echo rc=$?");
    assert_eq!(out.stdout, "rc=0", "{}", out.stderr);
    assert_eq!(
        std::fs::read(dir.join("out.txt")).expect("read out.txt"),
        text.as_bytes()
    );

    // CRLF already there is left alone; a byte order mark is dropped; `pbpaste | wc -l`
    // counts lines.
    let (stdout, _, code) = cash_with_stdin(
        dir,
        "pbcopy -pboard general && pbpaste -pboard general -Prefer txt | wc -l",
        b"\xEF\xBB\xBFa\r\nb\nc\r\n",
    );
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "3");
    assert_eq!(
        cash_win32::clipboard::get_text().expect("read the clipboard"),
        Some("a\r\nb\r\nc\r\n".to_owned())
    );

    // An empty input empties the clipboard, and an empty clipboard pastes nothing.
    let out = run_in(dir, "pbcopy </dev/null; echo rc=$?; pbpaste; echo rc=$?");
    assert_eq!(out.stdout, "rc=0\nrc=0", "{}", out.stderr);
    assert_eq!(out.stderr, "");
    assert_eq!(
        cash_win32::clipboard::get_text().expect("read the clipboard"),
        None
    );

    // The options: macOS's accepted, others refused with the usage.
    let out = run_in(dir, "pbcopy -h; pbpaste -h");
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        "Usage: pbcopy [-pboard {general | ruler | find | font}]\n\
         Usage: pbpaste [-pboard {general | ruler | find | font}] [-Prefer {txt | rtf | ps}]"
    );
    let out = run_in(dir, "pbcopy -x </dev/null; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert_eq!(
        out.stderr,
        "pbcopy: unknown option '-x'\nUsage: pbcopy [-pboard {general | ruler | find | font}]"
    );
    let out = run_in(dir, "pbpaste -pboard other; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert!(
        out.stderr
            .starts_with("pbpaste: unknown pasteboard 'other'\n"),
        "{}",
        out.stderr
    );

    let page = run("help pbpaste");
    assert_eq!(page.code, 0, "{}", page.stderr);
    assert!(page.stdout.contains("CRLF"), "{}", page.stdout);

    cash_win32::clipboard::set_text(before.as_deref().unwrap_or_default())
        .expect("put the clipboard back");
}
