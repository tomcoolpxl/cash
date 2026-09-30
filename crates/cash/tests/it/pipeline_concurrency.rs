//! Pipelines that actually run concurrently, and `read -t` — **D11**, **D26**.
//!
//! A compound command on the left of a pipeline was `await`ed inline, so it ran to
//! *completion* before the next member was created. Nothing was draining the pipe, and
//! above the buffer — 4096 bytes on Windows — everything it wrote was lost:
//!
//! ```text
//! { seq 1 5000; } | wc -l     ->  0      (bash: 5000)
//! ```
//!
//! Total, silent data loss in an everyday construct: `for f in *; do ...; done | tee log`
//! is not an exotic shape. Under 4 KiB it worked, which is what let it go unnoticed.
//!
//! The same investigation found `read -t` unimplemented — it failed outright with
//! "poll-based timeout is not supported on this platform", because the implementation was
//! `poll(2)`.
//!
//! Later, `read -t` turned out to time out on the pipe cash itself was started with,
//! with the line it waited for already read; see the last section.
//!
//! The sizes here are deliberate, not arbitrary: each one straddles the pipe buffer.

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

use std::io::{BufRead as _, BufReader, Write as _};
use std::process::{Command, Stdio};

const CASH: &str = env!("CARGO_BIN_EXE_cash");

struct Output {
    stdout: String,
    stderr: String,
}

fn cash(script: &str) -> Output {
    let out = Command::new(CASH)
        .args(["-c", script])
        .output()
        .expect("failed to run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
    }
}

/// Count lines out of a pipeline, trimmed of `wc`'s padding.
fn lines(script: &str) -> String {
    cash(&format!("{script} | wc -l | tr -d ' '")).stdout
}

// ---------------------------------------------------------------------------
// Every compound kind, above and below the pipe buffer
// ---------------------------------------------------------------------------

#[test]
fn a_brace_group_does_not_lose_output_above_the_pipe_buffer() {
    assert_eq!(lines("{ seq 1 5000; }"), "5000");
}

#[test]
fn a_subshell_does_not_lose_output() {
    assert_eq!(lines("( seq 1 5000 )"), "5000");
}

#[test]
fn a_for_loop_does_not_lose_output() {
    assert_eq!(
        lines(r#"for i in $(seq 1 5000); do echo "$i"; done"#),
        "5000"
    );
}

#[test]
fn a_while_loop_does_not_lose_output() {
    // 2000 lines are 8890 bytes, twice the pipe buffer. It was 5000, as for the others,
    // but this one runs three builtins a line where they run `seq` once, and on a busy
    // machine it did not finish in nextest's fifteen seconds (2026-09-30).
    assert_eq!(
        lines(r#"i=0; while [ "$i" -lt 2000 ]; do echo "$i"; i=$((i+1)); done"#),
        "2000"
    );
}

#[test]
fn an_if_block_does_not_lose_output() {
    assert_eq!(lines("if true; then seq 1 5000; fi"), "5000");
}

#[test]
fn a_case_block_does_not_lose_output() {
    assert_eq!(lines("case x in x) seq 1 5000 ;; esac"), "5000");
}

#[test]
fn a_function_does_not_lose_output() {
    assert_eq!(lines("f() { seq 1 5000; }; f"), "5000");
}

#[test]
fn the_boundary_itself_is_covered() {
    // Just under and just over 4096 bytes of output. The old code was correct below and
    // lost everything above, so a fix that only moved the boundary would pass one of
    // these and fail the other.
    for count in [800_usize, 900, 1000, 1100, 1200] {
        assert_eq!(
            lines(&format!("{{ seq 1 {count}; }}")),
            count.to_string(),
            "lost output at {count} lines"
        );
    }
}

#[test]
fn a_very_large_compound_pipeline_is_intact() {
    assert_eq!(lines("{ seq 1 100000; }"), "100000");
}

#[test]
fn the_bytes_are_right_not_just_the_count() {
    // A line count can survive reordering or duplication; this cannot.
    let out = cash(r#"{ seq 1 20000; } | md5sum | cut -d' ' -f1"#);
    let direct = cash(r#"seq 1 20000 | md5sum | cut -d' ' -f1"#);
    assert_eq!(
        out.stdout, direct.stdout,
        "a compound pipeline produced different bytes from a plain one"
    );
    assert!(!out.stdout.is_empty(), "no checksum: {}", out.stderr);
}

#[test]
fn a_small_compound_pipeline_still_works() {
    // The case that always worked, so a fix cannot have broken it.
    let out = cash(r#"{ echo a; echo b; } | wc -l | tr -d ' '"#);
    assert_eq!(out.stdout, "2");
}

#[test]
fn a_compound_member_in_the_middle_of_a_pipeline_works() {
    let out = cash(r#"seq 1 5000 | { cat; } | wc -l | tr -d ' '"#);
    assert_eq!(out.stdout, "5000", "stderr: {}", out.stderr);
}

#[test]
fn two_compound_members_in_a_row_work() {
    let out = cash(r#"{ seq 1 5000; } | { cat; } | wc -l | tr -d ' '"#);
    assert_eq!(out.stdout, "5000", "stderr: {}", out.stderr);
}

#[test]
fn a_compound_producer_stops_when_the_consumer_leaves() {
    // `| head -1` closes the pipe early. The producer must not hang, and the pipeline
    // must still succeed.
    let out = cash(r#"{ seq 1 200000; } | head -1"#);
    assert_eq!(out.stdout, "1", "stderr: {}", out.stderr);
}

#[test]
fn lastpipe_still_sees_its_variables() {
    // The last member runs in the parent shell on purpose, so that assignments survive;
    // spawning it as a task would lose exactly that.
    let out = cash(r#"shopt -s lastpipe; set +m; echo hello | { read -r v; }; echo "v=[$v]""#);
    assert_eq!(out.stdout, "v=[hello]", "stderr: {}", out.stderr);
}

#[test]
fn a_pipelines_exit_status_is_still_the_last_members() {
    let ok = cash(r#"{ seq 1 5000; } | true; echo "rc=$?""#);
    assert_eq!(ok.stdout, "rc=0");

    let bad = cash(r#"{ seq 1 5000; } | false; echo "rc=$?""#);
    assert_eq!(bad.stdout, "rc=1");
}

#[test]
fn pipefail_still_reports_a_failing_producer() {
    let out = cash(r#"set -o pipefail; { seq 1 5000; false; } | cat > /dev/null; echo "rc=$?""#);
    assert_eq!(out.stdout, "rc=1", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// `read -t`
// ---------------------------------------------------------------------------

#[test]
fn read_with_a_timeout_is_supported_at_all() {
    // It used to fail with "poll-based timeout is not supported on this platform".
    let out = cash(r#"printf 'hello\n' | { read -t 5 -r v; echo "rc=$? v=[$v]"; }"#);
    assert_eq!(out.stdout, "rc=0 v=[hello]", "stderr: {}", out.stderr);
    assert!(
        !out.stderr.contains("not supported"),
        "read -t is still unsupported: {}",
        out.stderr
    );
}

#[test]
fn read_with_a_timeout_reads_a_regular_file() {
    // bash's `-t` has no effect on a regular file: it is always ready.
    let out = cash(
        r#"d=$(mktemp -d); printf 'from-file\n' > "$d/f"; read -t 5 -r v < "$d/f"; echo "rc=$? v=[$v]"; cd /; rm -rf "$d""#,
    );
    assert_eq!(out.stdout, "rc=0 v=[from-file]", "stderr: {}", out.stderr);
}

#[test]
fn read_with_a_timeout_reports_end_of_input() {
    let out = cash(r#"read -t 5 -r v < /dev/null; echo "rc=$?""#);
    assert_eq!(out.stdout, "rc=1", "stderr: {}", out.stderr);
}

#[test]
fn a_zero_timeout_is_a_readiness_check() {
    // `read -t 0` asks whether input is waiting, without consuming it.
    let out = cash(r#"printf 'x\n' | { read -t 0 -r v; echo "rc=$?"; }"#);
    assert!(
        out.stdout.starts_with("rc="),
        "read -t 0 produced nothing: {} {}",
        out.stdout,
        out.stderr
    );
}

#[test]
fn a_timeout_does_not_take_longer_than_it_says() {
    // The wait must be bounded even when the input never arrives.
    let started = std::time::Instant::now();
    let out = cash(r#"read -t 1 -r v < /dev/null; echo "rc=$?""#);
    let elapsed = started.elapsed();

    assert!(out.stdout.starts_with("rc="), "no result: {}", out.stderr);
    assert!(
        elapsed < std::time::Duration::from_secs(15),
        "read -t 1 took {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// The standard input cash was started with
// ---------------------------------------------------------------------------
//
// The cases above read a pipe cash made itself. The standard input cash was started with
// was read through the standard library, whose buffer took all a pipe held at the first
// read. `read -t` waits on the pipe before each byte, and after the first byte it waited
// on a pipe the library had emptied: with the writer still there, it kept `a` of a line
// that had arrived whole and timed out. And what `read` did not want reached nothing
// else, a child least of all, which inherits the pipe and not the buffer.

/// Runs `script` with, as standard input, a pipe that holds what `printf` makes of
/// `format` and whose writer stays for as long as the script runs.
///
/// The writer is another cash, which writes, says so, and then waits for the test. So the
/// input is in the pipe before the script starts, and a read of it needs no time to pass.
fn cash_reading_a_pipe_that_stays_open(format: &str, script: &str) -> Output {
    let mut writer = Command::new(CASH)
        .args([
            "-c",
            &format!("printf '{format}'; echo written >&2; read -r _"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run the writer");
    let release_writer = writer.stdin.take().expect("the writer's input");
    let pipe = writer.stdout.take().expect("the writer's output");

    let mut said = String::new();
    BufReader::new(writer.stderr.take().expect("the writer's messages"))
        .read_line(&mut said)
        .expect("failed to read from the writer");
    assert_eq!(said.trim_end(), "written", "the writer did not write");

    let out = Command::new(CASH)
        .args(["-c", script])
        .stdin(Stdio::from(pipe))
        .output()
        .expect("failed to run cash");

    drop(release_writer);
    writer.wait().expect("the writer did not end");

    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
    }
}

/// Runs cash with `args`, and `input` as its standard input, to the end.
fn cash_given(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(CASH)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run cash");
    child
        .stdin
        .take()
        .expect("cash's input")
        .write_all(input.as_bytes())
        .expect("failed to write to cash");
    let out = child.wait_with_output().expect("cash did not end");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
    }
}

#[test]
fn read_with_a_timeout_takes_each_line_of_a_pipe_that_stays_open() {
    // `(printf 'a\nb\n'; sleep 3) | cash -c '...'`. Both reads timed out, the first with
    // `a` and the second with nothing: `x=a rc=142`, `y= rc=142`.
    let out = cash_reading_a_pipe_that_stays_open(
        r"a\nb\n",
        r#"read -t 5 x; echo "x=$x rc=$?"; read -t 5 y; echo "y=$y rc=$?""#,
    );
    assert_eq!(out.stdout, "x=a rc=0\ny=b rc=0", "stderr: {}", out.stderr);
}

#[test]
fn read_with_a_timeout_keeps_all_that_came_of_an_unfinished_line() {
    // Bash assigns what arrived before the time ran out. It kept `p`.
    let out = cash_reading_a_pipe_that_stays_open("par", r#"read -t 1 x; echo "x=$x rc=$?""#);
    assert_eq!(out.stdout, "x=par rc=142", "stderr: {}", out.stderr);
}

#[test]
fn a_zero_timeout_sees_the_lines_an_earlier_read_left() {
    // `read -t 0` looked at the pipe, and the second line was no longer in it.
    let out = cash_reading_a_pipe_that_stays_open(
        r"a\nb\n",
        r#"read -r x; read -t 0; echo "x=$x ready=$?""#,
    );
    assert_eq!(out.stdout, "x=a ready=0", "stderr: {}", out.stderr);
}

#[test]
fn what_read_leaves_of_standard_input_is_the_next_commands() {
    // `cat` is a program of its own, and printed nothing: `b` and `c` were in cash.
    let out = cash_given(&["-c", r#"read -r x; echo "x=$x"; cat"#], "a\nb\nc\n");
    assert_eq!(out.stdout, "x=a\nb\nc", "stderr: {}", out.stderr);

    // Taken by count, not by line.
    let out = cash_given(&["-c", r#"read -r -N 3 x; echo "x=$x"; cat"#], "abcdef\n");
    assert_eq!(out.stdout, "x=abc\ndef", "stderr: {}", out.stderr);

    let out = cash_given(
        &["-c", r#"mapfile -t -n 1 m; echo "m=${m[0]}"; cat"#],
        "a\nb\nc\n",
    );
    assert_eq!(out.stdout, "m=a\nb\nc", "stderr: {}", out.stderr);
}

#[test]
fn a_script_on_standard_input_is_the_input_of_its_own_commands() {
    // The lines after a command are what it reads, whether the shell runs it itself or
    // starts a program for it. `hello` and `world` were run as commands instead.
    for backend in [&[][..], &["--input-backend=basic"]] {
        let out = cash_given(backend, "read -r x\nhello\necho \"x=$x\"\n");
        assert_eq!(out.stdout, "x=hello", "{backend:?}: {}", out.stderr);

        let out = cash_given(backend, "cat\nhello\nworld\n");
        assert_eq!(out.stdout, "hello\nworld", "{backend:?}: {}", out.stderr);
    }
}
