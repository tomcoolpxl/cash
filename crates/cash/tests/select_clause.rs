//! `select` — a bash construct cash could not parse at all.
//!
//! `select` was a reserved word with no grammar rule behind it, so
//! `select x in a b; do ...; done` was a syntax error. It is the one interactive-menu
//! construct bash has, and a script using it did not fail at the menu — it failed to
//! parse, taking the whole file with it.
//!
//! Everything here drives it through a pipe rather than a terminal, which is also how
//! `select` behaves when a script feeds it: the menu and prompt go to standard error, the
//! body's output to standard output.

#![cfg(windows)]
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

use std::io::Write;
use std::process::{Command, Stdio};

const CASH: &str = env!("CARGO_BIN_EXE_cash");

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

/// Run a script with `input` on standard input.
fn cash_with_input(script: &str, input: &str) -> Output {
    let mut child = Command::new(CASH)
        .args(["-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run cash");

    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");

    let out = child.wait_with_output().expect("wait");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

// ---------------------------------------------------------------------------
// It parses
// ---------------------------------------------------------------------------

#[test]
fn a_select_clause_parses() {
    // The original symptom was `syntax error at line 1 col 8`.
    let out = cash_with_input("select x in a b; do break; done", "1\n");
    assert!(
        !out.stderr.contains("syntax error"),
        "select still does not parse: {}",
        out.stderr
    );
    assert_eq!(out.code, 0);
}

#[test]
fn a_multi_line_select_parses() {
    let out = cash_with_input(
        "select choice in alpha beta\ndo\n  echo \"picked $choice\"\n  break\ndone",
        "2\n",
    );
    assert_eq!(out.stdout, "picked beta", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// The menu and the choice
// ---------------------------------------------------------------------------

#[test]
fn the_menu_is_numbered_from_one_and_goes_to_stderr() {
    let out = cash_with_input("select x in a b c; do break; done", "1\n");
    assert!(out.stderr.contains("1) a"), "no menu: {}", out.stderr);
    assert!(
        out.stderr.contains("2) b"),
        "menu not numbered: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("3) c"),
        "menu incomplete: {}",
        out.stderr
    );
    assert!(
        !out.stdout.contains("1) a"),
        "the menu leaked onto stdout: {}",
        out.stdout
    );
}

#[test]
fn a_number_selects_the_matching_word() {
    let out = cash_with_input(r#"select x in a b c; do echo "got=$x"; break; done"#, "2\n");
    assert_eq!(out.stdout, "got=b");
}

#[test]
fn an_out_of_range_number_yields_an_empty_variable() {
    // bash sets the variable empty rather than refusing, which is what a body testing
    // `[ -z "$x" ]` depends on.
    let out = cash_with_input(r#"select x in a b; do echo "[$x]"; break; done"#, "9\n");
    assert_eq!(out.stdout, "[]");
}

#[test]
fn a_non_numeric_answer_yields_an_empty_variable() {
    let out = cash_with_input(r#"select x in a b; do echo "[$x]"; break; done"#, "quit\n");
    assert_eq!(out.stdout, "[]");
}

#[test]
fn reply_holds_the_raw_answer() {
    // How a body tells "3" from "quit" when both leave the variable empty.
    let out = cash_with_input(
        r#"select x in a b; do echo "[$x][$REPLY]"; break; done"#,
        "quit\n",
    );
    assert_eq!(out.stdout, "[][quit]");
}

#[test]
fn a_blank_line_reprints_the_menu_without_running_the_body() {
    let out = cash_with_input(r#"select x in a b; do echo "ran=$x"; break; done"#, "\n1\n");
    assert_eq!(
        out.stdout, "ran=a",
        "the body ran on a blank line: {}",
        out.stdout
    );
    // The menu appears twice: once at the start, once after the blank line.
    assert_eq!(
        out.stderr.matches("1) a").count(),
        2,
        "the menu was not reprinted: {}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// The loop
// ---------------------------------------------------------------------------

#[test]
fn it_loops_until_break() {
    let out = cash_with_input(
        r#"n=0; select x in a b; do n=$((n+1)); echo "iteration $n: $x"; [ "$n" -eq 2 ] && break; done"#,
        "1\n2\n",
    );
    assert_eq!(out.stdout, "iteration 1: a\niteration 2: b");
}

#[test]
fn end_of_input_ends_the_loop() {
    // No answer at all: the loop stops rather than spinning.
    let out = cash_with_input(
        r#"select x in a b; do echo "body ran"; done; echo after"#,
        "",
    );
    assert_eq!(
        out.stdout, "after",
        "the body ran with no input: {}",
        out.stdout
    );
    assert_eq!(out.code, 0);
}

#[test]
fn an_empty_word_list_exits_immediately() {
    // bash does not prompt for a menu with nothing in it.
    let out = cash_with_input(
        r#"select x in; do echo "body ran"; done; echo after"#,
        "1\n",
    );
    assert_eq!(out.stdout, "after", "stderr: {}", out.stderr);
}

#[test]
fn continue_asks_again() {
    let out = cash_with_input(
        r#"n=0; select x in a b; do n=$((n+1)); [ "$n" -lt 2 ] && continue; echo "stopped at $n"; break; done"#,
        "1\n2\n",
    );
    assert_eq!(out.stdout, "stopped at 2", "stderr: {}", out.stderr);
}

#[test]
fn the_positional_parameters_are_the_default_list() {
    // `select x; do ...; done` offers "$@", exactly as `for` does.
    let out = cash_with_input(
        r#"set -- one two; select x; do echo "got=$x"; break; done"#,
        "2\n",
    );
    assert_eq!(out.stdout, "got=two", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// The prompt
// ---------------------------------------------------------------------------

#[test]
fn ps3_is_the_prompt() {
    let out = cash_with_input(r#"PS3="pick: "; select x in a; do break; done"#, "1\n");
    // Trailing whitespace is trimmed out of the captured stderr, so match without it.
    assert!(
        out.stderr.contains("pick:"),
        "PS3 was ignored: {}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("#?"),
        "the default prompt was used despite PS3: {}",
        out.stderr
    );
}

#[test]
fn the_default_prompt_is_used_when_ps3_is_unset() {
    let out = cash_with_input("select x in a; do break; done", "1\n");
    assert!(
        out.stderr.contains("#?"),
        "no default prompt: {}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// Composition
// ---------------------------------------------------------------------------

#[test]
fn a_select_can_be_nested_in_a_function() {
    let out = cash_with_input(
        r#"choose() { select x in a b; do echo "fn got $x"; break; done; }; choose"#,
        "2\n",
    );
    assert_eq!(out.stdout, "fn got b", "stderr: {}", out.stderr);
}

#[test]
fn the_variable_survives_the_loop() {
    let out = cash_with_input(
        r#"select x in a b; do break; done; echo "after=[$x]""#,
        "2\n",
    );
    assert_eq!(out.stdout, "after=[b]");
}

#[test]
fn a_word_list_is_expanded() {
    let out = cash_with_input(
        r#"opts="a b"; select x in $opts; do echo "got=$x"; break; done"#,
        "2\n",
    );
    assert_eq!(out.stdout, "got=b", "stderr: {}", out.stderr);
}
