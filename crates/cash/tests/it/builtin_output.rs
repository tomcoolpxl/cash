//! A builtin's standard output, gathered into larger writes (`cash_core::output`): what
//! is gathered keeps its place among the builtin's messages, the programs and commands it
//! runs, and the redirections they make.

#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, run as cash, run_in};

#[test]
fn a_message_keeps_its_place_among_a_builtins_output() {
    let out = cash("type echo nosuch cd 2>&1");
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{}", out.stdout);
    assert!(lines[0].starts_with("echo is"), "{}", out.stdout);
    assert!(
        lines[1].ends_with("type: nosuch: not found"),
        "{}",
        out.stdout
    );
    assert!(lines[2].starts_with("cd is"), "{}", out.stdout);
}

#[test]
fn a_program_a_builtin_runs_writes_in_its_turn() {
    let out = cash("eval 'echo a; cmd.exe /d /c echo b; echo c' | cat");
    assert_eq!(out.stdout.replace('\r', ""), "a\nb\nc", "{}", out.stderr);
}

#[test]
fn exec_inside_eval_or_a_sourced_file_still_moves_the_output() {
    let scratch = Scratch::new("exec-moves-output");
    std::fs::write(
        scratch.path().join("s.sh"),
        "exec >from-source.txt\necho sourced\n",
    )
    .unwrap();
    let out = run_in(
        scratch.path(),
        "eval 'exec 3>&1 >from-eval.txt; echo evaluated; exec >&3'; echo back; ( . ./s.sh )",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "back");
    let read = |name: &str| std::fs::read_to_string(scratch.path().join(name)).unwrap();
    assert_eq!(read("from-eval.txt"), "evaluated\n");
    assert_eq!(read("from-source.txt"), "sourced\n");
}

#[test]
fn a_large_output_arrives_whole_and_in_order() {
    let scratch = Scratch::new("large-output");
    let out = run_in(scratch.path(), "printf '%s\\n' {1..200000} > numbers.txt");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let text = std::fs::read_to_string(scratch.path().join("numbers.txt")).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let expected: Vec<String> = (1..=200_000).map(|n| n.to_string()).collect();
    assert!(
        text.ends_with('\n') && lines == expected,
        "{} lines, not {}",
        lines.len(),
        expected.len()
    );
}

#[test]
fn a_prompt_shows_before_the_builtin_reads_its_answer() {
    let out = cash("printf 'answer\\n' | { printf 'q? '; read r; echo \"got $r\"; }");
    assert_eq!(out.stdout, "q? got answer");
}
