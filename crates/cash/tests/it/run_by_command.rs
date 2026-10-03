//! A script that does not parse runs as Bash runs it: a complete command at a time, up to
//! the one that is wrong (EXE-07).
//!
//! cash parsed a script, a sourced file, a `-c` string and `eval`'s whole before running
//! any of it, so a syntax error on line 2 stopped line 1, and a self-extracting archive's
//! payload after `exit` had to parse. A script that parses still runs whole.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction, and a \
              failed set-up should abort it loudly"
)]

use crate::common::{Scratch, cash_command, output_of, run_in};

/// Runs the script `name`, written with `text`, and returns its output and status.
fn run_script(name: &str, text: &[u8]) -> (String, String, i32) {
    let scratch = Scratch::new(name);
    std::fs::write(scratch.path().join("script.sh"), text).expect("write");
    let out = output_of(cash_command().arg("script.sh").current_dir(scratch.path()));
    (out.stdout, out.stderr, out.code)
}

#[test]
fn the_commands_before_a_syntax_error_run() {
    let (stdout, stderr, code) = run_script("before-error", b"echo first\nif then\necho never\n");
    assert_eq!((stdout.as_str(), code), ("first", 2), "{stderr}");
    assert!(stderr.contains("line 2"), "{stderr}");
}

#[test]
fn a_payload_after_exit_is_never_read() {
    // A self-extracting archive: a script, `exit`, and bytes that are no shell at all.
    let mut text = b"echo unpacking\nexit 0\n".to_vec();
    text.extend_from_slice(b"\xff\xfe\x00 )( binary\n\x01 if then fi done\n");
    let (stdout, stderr, code) = run_script("payload", &text);
    assert_eq!((stdout.as_str(), code), ("unpacking", 0), "{stderr}");
}

#[test]
fn a_later_line_parses_as_an_earlier_one_left_the_shell() {
    // `shopt -s extglob` is in force for the lines after it, as Bash reads them. Line
    // numbers count from the start of the script, not of the command; a here-document's
    // are its command's, as in Bash.
    let (stdout, stderr, code) = run_script(
        "late-extglob",
        b"shopt -s extglob\nx=abc\necho \"${x/+(b)/B} $LINENO\"\ncat <<EOF\nhere $LINENO\nEOF\nif then\n",
    );
    assert_eq!((stdout.as_str(), code), ("aBc 3\nhere 4", 2), "{stderr}");
    assert!(stderr.contains("line 7"), "{stderr}");
}

#[test]
fn a_sourced_file_a_dash_c_string_and_eval_run_up_to_the_error() {
    let scratch = Scratch::new("by-command");
    std::fs::write(scratch.path().join("bad.inc"), "echo in-bad\nif then\n").expect("write");
    let out = run_in(
        scratch.path(),
        ". ./bad.inc; echo \"source $?\"; eval $'echo in-eval\\nif then'; echo \"eval $?\"",
    );
    assert_eq!(
        (out.stdout.as_str(), out.code),
        ("in-bad\nsource 2\nin-eval\neval 2", 0),
        "{}",
        out.stderr
    );

    let out = run_in(scratch.path(), "echo in-c\nif then\necho never");
    assert_eq!(
        (out.stdout.as_str(), out.code),
        ("in-c", 2),
        "{}",
        out.stderr
    );
}
