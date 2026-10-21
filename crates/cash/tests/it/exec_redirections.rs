//! `exec`, and the redirections of compound commands and function calls, as Bash 5.3 has
//! them (found by Bash's own test suite, 2026-10-20).
//!
//! A compound command's redirections are the shell's while it runs, put back as it ends;
//! `exec` with no command makes its own redirections the shell's, and only those. `exec`
//! made every descriptor of the command it ran in the shell's: `{ exec 3>x; } >y` left
//! standard output in `y` for good, and what followed the block was lost in the file.
#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    reason = "integration assertions"
)]

use std::path::Path;

use crate::common::{Scratch, cash_command};

/// Runs `script` in `dir`: its status, standard output and standard error.
fn run_in(dir: &Path, script: &str) -> (i32, String, String) {
    let result = cash_command()
        .current_dir(dir)
        .args(["--noprofile", "--norc", "-c", script])
        .output()
        .unwrap();
    let text = |bytes: Vec<u8>| String::from_utf8(bytes).unwrap().replace("\r\n", "\n");
    (
        result.status.code().unwrap_or(-1),
        text(result.stdout),
        text(result.stderr),
    )
}

#[test]
fn exec_in_a_redirected_block_keeps_its_own_descriptor_only() {
    let scratch = Scratch::new("exec-in-block");
    let (status, stdout, _) = run_in(
        scratch.path(),
        "{ echo in; exec 10>&1; echo in2; } > f; echo out; cat f; \
         { exec 3>/dev/null; } > g; echo visible; \
         h() { exec 4>four; }; h > g; echo again; echo 4 >&4; cat four; \
         { exec >e; echo a; } > e.b; echo after; cat e; echo ---; cat e.b",
    );
    assert_eq!(
        (status, stdout.as_str()),
        (0, "out\nin\nin2\nvisible\nagain\n4\nafter\na\n---\n")
    );
}

#[test]
fn a_block_puts_back_a_descriptor_it_closed() {
    // modernish's BUG_SCLOSEDFD: `exec` in a block that closed fd 8 is undone with it.
    let scratch = Scratch::new("block-fd-scope");
    let (_, stdout, _) = run_in(
        scratch.path(),
        "{ exec 8</dev/null; } 8<&-; : <&8 2>/dev/null && echo open || echo closed",
    );
    assert_eq!(stdout, "closed\n");
}

#[test]
fn a_failed_exec_ends_a_script_unless_execfail() {
    let scratch = Scratch::new("exec-fails");
    let cases = [
        (
            "shopt -s execfail; exec notthere-x; echo \"after $?\"",
            0,
            "after 127\n",
        ),
        ("exec notthere-x; echo \"after $?\"", 127, ""),
        ("exec .; echo \"after $?\"", 126, ""),
        ("exec -i x; echo \"after $?\"", 0, "after 2\n"),
        ("command exec notthere-x; echo \"after $?\"", 127, ""),
    ];
    for (script, status, stdout) in cases {
        let (got_status, got_stdout, stderr) = run_in(scratch.path(), script);
        assert_eq!(
            (got_status, got_stdout.as_str()),
            (status, stdout),
            "{script}: {stderr}"
        );
    }
    let (_, _, stderr) = run_in(scratch.path(), "exec .");
    assert!(
        stderr.contains("exec: .: cannot execute: Is a directory"),
        "{stderr}"
    );
}

#[test]
fn an_exit_is_not_negated() {
    let scratch = Scratch::new("negated-exit");
    let (_, stdout, _) = run_in(
        scratch.path(),
        "( ! exit 42 ); echo \"$?\"; ! (exit 42); echo \"$?\"",
    );
    assert_eq!(stdout, "42\n0\n");
}

#[test]
fn a_background_job_reads_the_input_redirected_around_it() {
    // Only input left as it came is `/dev/null` for `&`; a job in a loop fed by a
    // here-document, or in a stage fed by a pipe, read nothing.
    let scratch = Scratch::new("bg-input");
    let (_, stdout, _) = run_in(
        scratch.path(),
        "for x in 1 2; do { read l; echo \"[$l]\"; } & wait; done <<EOF\nab\ncd\nEOF\n\
         echo x1 | ( cat & wait ); echo x3 | { cat & wait; }; \
         { read l; echo \"[$l]\"; } & wait",
    );
    assert_eq!(stdout, "[ab]\n[cd]\nx1\nx3\n[]\n");
}

#[test]
fn a_compound_command_s_failed_redirection_fails_only_it() {
    // It ended the script, or the `( )` around it; Bash says so, fails it with 1, and
    // runs the ERR trap.
    let scratch = Scratch::new("compound-redir-fails");
    let (status, stdout, stderr) = run_in(
        scratch.path(),
        "(while false; do :; done > ./no/such; echo \"in $?\"); echo \"out $?\"; \
         trap 'echo ERR' ERR; { :; } > ./no/such; echo \"after $?\"",
    );
    assert_eq!(
        (status, stdout.as_str()),
        (0, "in 1\nout 0\nERR\nafter 1\n")
    );
    assert!(stderr.contains("./no/such"), "{stderr}");
}

#[test]
fn a_variable_takes_a_here_document_or_string_s_descriptor() {
    let scratch = Scratch::new("var-heredoc");
    let (_, stdout, stderr) = run_in(
        scratch.path(),
        "exec {v}<<EOF\nline 1\nEOF\necho \"v=$v\"; cat <&$v; \
         exec {w}<<<herestring; echo \"w=$w\"; cat <&$w",
    );
    assert_eq!(stdout, "v=10\nline 1\nw=11\nherestring\n", "{stderr}");
}

#[test]
fn eval_and_source_hold_their_redirections_as_a_function_call_does() {
    // An `exec` in what they run changes what the commands after it there see, and is
    // undone with them, as in Bash: `a` went to `out` and `b` to `log`.
    let scratch = Scratch::new("eval-source-redir");
    std::fs::write(scratch.path().join("s.sh"), "exec >log2\necho s\n").expect("write");
    let (_, stdout, stderr) = run_in(
        scratch.path(),
        "eval 'exec >log; echo a' >out; echo b; . ./s.sh >out2; echo c; \
         cat out log out2 log2",
    );
    assert_eq!(stdout, "b\nc\na\ns\n", "{stderr}");
}
