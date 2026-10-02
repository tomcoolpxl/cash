//! D7: `/dev/stdin`, `/dev/stdout`, `/dev/stderr` and `/dev/fd/N` name the shell's
//! descriptors.
//!
//! `echo "message" > /dev/stderr` is `>&2` written out, and as common in scripts. Every
//! one of these failed with `failed to redirect to C:/dev/stderr: The system cannot find
//! the path specified`: a redirection resolves its word against the working directory
//! before the file is opened, and the names were only known as they are written. With a
//! folder `C:\dev` on the machine the same line made a file `C:\dev\stderr` instead.
//!
//! What Git Bash 5.3 does with each script here was measured; where cash differs (it
//! duplicates the descriptor, Git Bash opens the file again and empties it) the test
//! says so.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly"
)]

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// What a script left: each output stream apart, and its exit status.
struct Left {
    stdout: String,
    stderr: String,
    status: i32,
}

/// Runs `script` in `dir` with `input` on its standard input, which is a pipe.
fn cash_in(dir: &Path, input: &str, script: &str) -> Left {
    let mut child = Command::new(CASH)
        .current_dir(dir)
        .args(["--noprofile", "--norc", "-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run cash");
    // Dropped at the end of the statement, so the script reads what was written and
    // then the end of its input. A script that never reads has closed the pipe by then
    // or not; either way there is nothing to learn from the write failing.
    let _ = child
        .stdin
        .take()
        .expect("the pipe to cash's standard input")
        .write_all(input.as_bytes());
    let output = child.wait_with_output().expect("wait for cash");
    Left {
        stdout: String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        stderr: String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
        status: output.status.code().unwrap_or(-1),
    }
}

/// Runs `script` with `input` on its standard input, where it writes no files.
fn cash_given(input: &str, script: &str) -> Left {
    cash_in(&std::env::temp_dir(), input, script)
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name))
        .expect("the file the script wrote")
        .replace("\r\n", "\n")
}

#[test]
fn standard_input_is_read_by_name_from_a_pipe() {
    let left = cash_given(
        "first line\nsecond line\n",
        r#"read -r line < /dev/stdin; echo "read: $line, rc=$?"
read -r line < /dev/stdin; echo "read: $line, rc=$?""#,
    );

    assert_eq!(
        left.stdout,
        "read: first line, rc=0\nread: second line, rc=0\n"
    );
    assert_eq!(left.stderr, "");
    assert_eq!(left.status, 0);
}

#[test]
fn a_program_reads_standard_input_by_name() {
    let left = cash_given(
        "first line\nsecond line\n",
        r#"findstr.exe line < /dev/stdin; echo "rc=$?""#,
    );

    assert_eq!(left.stdout, "first line\nsecond line\nrc=0\n");
    assert_eq!(left.stderr, "");
}

/// The name is the descriptor the command has, a pipe of the script's own here, and not
/// the one the shell was started with.
#[test]
fn standard_input_by_name_is_the_pipe_the_command_reads() {
    let left = cash_given(
        "from outside\n",
        r#"echo from the pipeline | { read -r line < /dev/stdin; echo "read: $line"; }
printf 'a\nb\n' | while read -r line; do echo "loop: $line"; done < /dev/stdin"#,
    );

    assert_eq!(left.stdout, "read: from the pipeline\nloop: a\nloop: b\n");
    assert_eq!(left.stderr, "");
}

#[test]
fn each_output_stream_is_written_by_name() {
    let left = cash_given(
        "",
        r#"echo to-out > /dev/stdout; echo "rc=$?"; echo to-err > /dev/stderr; echo "rc=$?""#,
    );

    assert_eq!(left.stdout, "to-out\nrc=0\nrc=0\n");
    assert_eq!(left.stderr, "to-err\n");
    assert_eq!(left.status, 0);
}

#[test]
fn standard_error_is_appended_to_by_name() {
    let left = cash_given(
        "",
        r#"echo one >> /dev/stderr; echo "rc=$?"; echo two >> /dev/stderr; echo "rc=$?""#,
    );

    assert_eq!(left.stdout, "rc=0\nrc=0\n");
    assert_eq!(left.stderr, "one\ntwo\n");
}

#[test]
fn both_streams_are_sent_to_the_one_named() {
    let left = cash_given(
        "",
        r"echo both &> /dev/stderr; echo again >& /dev/stderr; echo appended &>> /dev/stderr",
    );

    assert_eq!(left.stdout, "");
    assert_eq!(left.stderr, "both\nagain\nappended\n");
}

#[test]
fn a_program_writes_to_the_stream_named() {
    let dir = tempfile::tempdir().expect("scratch dir");
    let left = cash_in(
        dir.path(),
        "",
        r#"{
    cmd.exe /d /c "echo from a program" > /dev/stderr
    cmd.exe /d /c "echo its errors 1>&2" 2> /dev/stdout
} > out.txt 2> err.txt"#,
    );

    assert_eq!(read(dir.path(), "out.txt").trim_end(), "its errors");
    assert_eq!(read(dir.path(), "err.txt").trim_end(), "from a program");
    assert_eq!(left.stderr, "");
}

/// The same where the streams are the ones the shell was started with, which a program
/// is handed by another way than a file or a pipe of the script's.
#[test]
fn a_program_writes_to_the_shells_own_stream_named() {
    let left = cash_given(
        "",
        r#"cmd.exe /d /c "echo from a program" > /dev/stderr
cmd.exe /d /c "echo its errors 1>&2" 2> /dev/stdout"#,
    );

    assert_eq!(left.stdout.trim_end(), "its errors");
    assert_eq!(left.stderr.trim_end(), "from a program");
}

/// `> /dev/stderr` in a function whose standard error the caller redirected goes where
/// the caller sent it, as `>&2` would.
#[test]
fn the_name_is_the_descriptor_a_redirection_put_there() {
    let dir = tempfile::tempdir().expect("scratch dir");
    let left = cash_in(
        dir.path(),
        "",
        r#"warn() { echo "$*" > /dev/stderr; }
warn careful 2> log.txt
{ echo piped > /dev/stdout; } | { read -r line; echo "read: $line"; }
exec 3> /dev/stdout; echo through three >&3"#,
    );

    assert_eq!(read(dir.path(), "log.txt"), "careful\n");
    assert_eq!(left.stdout, "read: piped\nthrough three\n");
    assert_eq!(left.stderr, "");
}

/// The descriptor is duplicated and nothing is opened again, so what the file already
/// has stays. Git Bash opens the file a second time and empties it: it leaves
/// `twothree` here.
#[test]
fn writing_by_name_keeps_what_the_file_already_has() {
    let dir = tempfile::tempdir().expect("scratch dir");
    let left = cash_in(
        dir.path(),
        "",
        r"( echo one >&2; echo two > /dev/stderr; echo three >&2 ) 2> err.txt
( echo one; echo two > /dev/stdout; echo three ) > out.txt",
    );

    assert_eq!(read(dir.path(), "err.txt"), "one\ntwo\nthree\n");
    assert_eq!(read(dir.path(), "out.txt"), "one\ntwo\nthree\n");
    assert_eq!(left.stderr, "");
}

#[test]
fn a_descriptor_opened_with_exec_is_written_and_read_by_number() {
    let dir = tempfile::tempdir().expect("scratch dir");
    let left = cash_in(
        dir.path(),
        "",
        r#"exec 9> nine.txt
echo first > /dev/fd/9; echo "write rc=$?"
echo second >> /dev/fd/9; echo "append rc=$?"
cmd.exe /d /c "echo third" > /dev/fd/9
exec 9>&-
exec 8< nine.txt
read -r line < /dev/fd/8; echo "read: $line"
exec {held}> held.txt; echo by a variable > /dev/fd/$held; exec {held}>&-"#,
    );

    assert_eq!(left.stdout, "write rc=0\nappend rc=0\nread: first\n");
    assert_eq!(left.stderr, "");
    assert_eq!(read(dir.path(), "nine.txt"), "first\nsecond\nthird\n");
    assert_eq!(read(dir.path(), "held.txt"), "by a variable\n");
}

/// Bash: `/dev/fd/9: No such file or directory`, and the command does not run. The name
/// is the one that was written, and no file is looked for at `C:/dev/fd/9`.
#[test]
fn a_descriptor_that_is_not_open_is_no_file() {
    let left = cash_given(
        "",
        r#"echo written > /dev/fd/9; echo "write rc=$?"
read -r line < /dev/fd/9; echo "read rc=$?"
echo appended >> /dev/fd/9; echo "append rc=$?"
echo both &> /dev/fd/9; echo "both rc=$?"
exec 0<&-; read -r line < /dev/stdin; echo "closed rc=$?""#,
    );

    assert_eq!(
        left.stdout,
        "write rc=1\nread rc=1\nappend rc=1\nboth rc=1\nclosed rc=1\n"
    );
    assert_eq!(
        left.stderr
            .matches("failed to redirect to /dev/fd/9: No such file or directory")
            .count(),
        4,
        "{}",
        left.stderr
    );
    assert!(
        left.stderr
            .contains("failed to redirect to /dev/stdin: No such file or directory"),
        "{}",
        left.stderr
    );
    assert!(!left.stderr.contains(":/dev/"), "{}", left.stderr);
}

/// gnu-bash's `source6.sub`: `echo "echo three - OK" | . /dev/stdin`.
#[test]
fn a_script_is_sourced_from_standard_input_by_name() {
    let left = cash_given(
        "echo from outside; seen=outside\n",
        r#"echo "echo three - OK; seen=pipeline" | . /dev/stdin; echo "rc=$?"
source /dev/stdin; echo "rc=$? seen=$seen""#,
    );

    assert_eq!(
        left.stdout,
        "three - OK\nrc=0\nfrom outside\nrc=0 seen=outside\n"
    );
    assert_eq!(left.stderr, "");
}

// The names as an argument of a bundled tool, which opens the file itself (D7, TODO 4.1).
// Each of these said "The system cannot find the path specified." and returned 1.

#[test]
fn a_bundled_tool_reads_standard_input_by_name() {
    let left = cash_given(
        "x\n",
        r#"cat /dev/stdin; echo "cat: $?"; echo y | cat /dev/fd/0; echo z | cat /dev/stdin -
printf 'b\na\n' | sort /dev/stdin; printf '1\n2\n' | wc -l /dev/stdin"#,
    );

    assert_eq!(left.stdout, "x\ncat: 0\ny\nz\na\nb\n2 /dev/stdin\n");
    assert_eq!(left.stderr, "");
}

#[test]
fn a_bundled_tool_writes_each_output_stream_by_name() {
    let left = cash_given(
        "",
        r#"echo to-err | tee /dev/stderr; echo to-out | tee /dev/stdout > /dev/null
echo gone | tee /dev/null; echo "rc=$?""#,
    );

    // `tee /dev/stdout > /dev/null`: tee's standard output is the null device, and so
    // is the stream it names; Bash says nothing either.
    assert_eq!(left.stdout, "to-err\ngone\nrc=0\n");
    assert_eq!(left.stderr, "to-err\n");
}

#[test]
fn a_bundled_tool_empties_a_file_from_the_null_device() {
    let dir = tempfile::tempdir().expect("a scratch folder");
    std::fs::write(dir.path().join("a"), "old\n").expect("a file to empty");
    std::fs::write(dir.path().join("b"), "old\n").expect("a file to empty");
    let left = cash_in(
        dir.path(),
        "",
        r#"cat /dev/null > a; echo "cat: $?"; cp /dev/null b; echo "cp: $?"
echo piped | cp /dev/stdin c; echo "cp stdin: $?""#,
    );

    assert_eq!(left.stdout, "cat: 0\ncp: 0\ncp stdin: 0\n");
    assert_eq!(left.stderr, "");
    assert_eq!(read(dir.path(), "a"), "");
    assert_eq!(read(dir.path(), "b"), "");
    assert_eq!(read(dir.path(), "c"), "piped\n");
}

#[test]
fn dd_writes_to_the_null_device_and_a_pipe() {
    // uutils' dd asks where its output is and truncates it there, which a pipe or `NUL`
    // refuses on Windows: `dd of=NUL` failed with "Incorrect function" before any name.
    let left = cash_given(
        "x\n",
        r#"dd if=/dev/stdin of=/dev/stdout status=none; echo y | dd of=/dev/null status=none
echo "rc=$?""#,
    );

    assert_eq!(left.stdout, "x\nrc=0\n");
    assert_eq!(left.stderr, "");
}

#[test]
fn a_bundled_tool_finds_no_descriptor_above_two() {
    // A program has standard input, output and error only (D26).
    let left = cash_given("", r#"cat /dev/fd/3; echo "rc=$?""#);

    assert_eq!(left.stdout, "rc=1\n");
    assert!(left.stderr.contains("/dev/fd/3"), "{}", left.stderr);
}

#[test]
fn a_bundled_tool_knows_the_names_where_a_redirection_does() {
    // At the root of a drive, as resolving `/dev/null` leaves it; deeper down, a file.
    let dir = tempfile::tempdir().expect("a scratch folder");
    std::fs::create_dir(dir.path().join("dev")).expect("a folder called dev");
    std::fs::write(dir.path().join("dev").join("null"), "a file\n").expect("a file");
    let left = cash_in(
        dir.path(),
        "",
        r#"cat "${PWD:0:2}/dev/null"; echo "rc=$?"; cat dev/null"#,
    );

    assert_eq!(left.stdout, "rc=0\na file\n");
    assert_eq!(left.stderr, "");
}
