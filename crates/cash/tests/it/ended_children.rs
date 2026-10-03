//! A pid names the process cash started, or none — **D22**.
//!
//! A script keeps `$!` and signals it later, often after the job has ended: `kill "$pid"`
//! in a cleanup trap, `while kill -0 "$pid"`. Bash on Linux answers "No such process",
//! because Linux comes back to a pid only after millions of others. Windows hands one
//! out again within a second on a busy machine (see `process_identity.rs`), and cash
//! asked only whether some process had the number: `kill -0` said the job was running,
//! and `kill` ended whichever program had been given the pid.
//!
//! cash now holds each process of a job open, so its pid stays its own after it ends,
//! and signals a job's processes only while they run. None of these tests waits for
//! Windows to reuse a pid, which is luck. They ask what decides it: whether the ended
//! child's pid is still that child's, and whether cash asks the process or its number.
//! A process that exits with 259 tells the two apart for certain: that status reads as
//! "still running" to anything that asks the number.

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

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use cash_win32::process::{is_pid_alive, now_filetime, started};

use crate::common::{Output, cash_command, output_of};

/// How long a step may take before the test gives up on it; each takes well under a
/// second when it works.
const PATIENCE: Duration = Duration::from_secs(10);

fn cash_in(dir: &Path, script: &str) -> Output {
    output_of(
        cash_command()
            .args(["-c", script])
            .current_dir(dir)
            .stdin(Stdio::null()),
    )
}

fn cash(script: &str) -> Output {
    cash_in(&std::env::temp_dir(), script)
}

/// The lines a running program prints, as they arrive.
fn lines_of(stream: impl Read + Send + 'static) -> Receiver<String> {
    let (send, receive) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            if send.send(line).is_err() {
                break;
            }
        }
    });
    receive
}

/// Waits for a line that starts with `prefix`, and returns the rest of it and the lines
/// before it.
fn line_after(lines: &Receiver<String>, prefix: &str) -> (String, Vec<String>) {
    let mut before = Vec::new();
    loop {
        let line = lines
            .recv_timeout(PATIENCE)
            .unwrap_or_else(|_| panic!("no line starting with {prefix:?}; got {before:?}"));
        if let Some(rest) = line.strip_prefix(prefix) {
            return (rest.to_owned(), before);
        }
        before.push(line);
    }
}

/// Waits for `child` to exit, or kills it and panics.
fn exit_of(mut child: Child, what: &str) -> i32 {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            return status.code().unwrap_or(-1);
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{what} did not finish");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A `cmd.exe` that waits for its input to close and then exits with `status`, so a test
/// decides when it ends.
fn waits_then_exits(program: &Path, status: u32) -> Child {
    Command::new(program)
        .args(["/d", "/c", &format!("set /p line= & exit {status}")])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| panic!("running {}: {error}", program.display()))
}

fn system32(program: &str) -> PathBuf {
    let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(system_root).join("System32").join(program)
}

// ---------------------------------------------------------------------------
// `kill PID`
// ---------------------------------------------------------------------------

#[test]
fn an_ended_background_childs_pid_stays_its_own() {
    // The shell reports on its ended child and then waits, so the test can look at the
    // pid while cash still remembers the job.
    let before = now_filetime();
    let mut shell = cash_command()
        .args([
            "-c",
            r#"cmd.exe /d /c exit 7 & pid=$!
               wait "$pid"; echo "status=$?"
               kill -0 "$pid"; echo "probe=$?"
               kill "$pid"; echo "kill=$?"
               echo "pid=$pid"
               read -r line"#,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run cash");
    let stdout = lines_of(shell.stdout.take().unwrap());
    let stderr = lines_of(shell.stderr.take().unwrap());

    let (pid, said) = line_after(&stdout, "pid=");
    let pid: u32 = pid.parse().expect("a pid");
    let began = started(pid);
    let alive = is_pid_alive(pid);
    let after = now_filetime();

    drop(shell.stdin.take());
    exit_of(shell, "cash");
    let complaints: Vec<String> = stderr.iter().collect();

    // The pid still names a process, the one cash started, and that process has ended.
    // No other can be given the number while cash holds it.
    assert!(
        began.is_some_and(|at| before <= at && at <= after),
        "pid {pid} is no longer the child's: started {began:?}, test ran {before}..{after}"
    );
    assert!(!alive, "pid {pid} names a running process");

    assert_eq!(said, ["status=7", "probe=1", "kill=1"]);
    let no_such = format!("({pid}) - No such process");
    assert_eq!(
        complaints.iter().filter(|l| l.contains(&no_such)).count(),
        2,
        "stderr: {complaints:?}"
    );
}

#[test]
fn a_pid_whose_process_exited_with_259_has_no_process() {
    // The test holds the ended process, as cash holds a job's: the pid is its own, and
    // its exit status is the one that reads as "still running".
    let mut ended = waits_then_exits(&system32("cmd.exe"), 259);
    drop(ended.stdin.take());
    assert_eq!(ended.wait().unwrap().code(), Some(259));
    let pid = ended.id();

    let out = cash(&format!(
        r#"kill -0 {pid}; echo "probe=$?"; kill {pid}; echo "kill=$?""#
    ));
    assert_eq!(out.stdout, "probe=1\nkill=1", "stderr: {}", out.stderr);
    let no_such = format!("({pid}) - No such process");
    assert_eq!(
        out.stderr.matches(&no_such).count(),
        2,
        "stderr: {}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// `kill %N`
// ---------------------------------------------------------------------------

#[test]
fn a_job_that_has_ended_is_signalled_without_effect_or_error() {
    // Bash 5.3 on Linux: a job still listed, whose processes have ended, takes a signal
    // in silence and with status 0, because Bash passes over the processes it knows have
    // ended. cash sent the signal to the first one's pid: "entity not found" when the
    // number was free, another program's end when it was not.
    let out = cash(
        r#"cmd.exe /d /c exit 0 & pid=$!
           while kill -0 "$pid" 2>/dev/null; do sleep 0.1; done
           kill %1; echo "TERM=$?"
           kill -0 %1; echo "probe=$?"
           kill -KILL %1; echo "KILL=$?"
           kill -STOP %1; echo "STOP=$?"
           jobs"#,
    );
    assert_eq!(
        out.stdout,
        "TERM=0\nprobe=0\nKILL=0\nSTOP=0\n\
         [1]+  Done                       cmd.exe /d /c exit 0",
        "stderr: {}",
        out.stderr
    );
    assert_eq!(out.stderr, "");
}

/// Waits, in a script, for the file `out` to have something in it: the program that
/// writes it has started.
const UNTIL_OUT_IS_WRITTEN: &str = "for try in $(seq 100); do [ -s out ] && break; sleep 0.1; done";

#[test]
fn a_signal_to_a_job_reaches_the_process_still_running() {
    // `{ a; b; } &` once `a` has ended: the job is `b` now. cash signalled `a`'s pid,
    // which is no process or another program, and `b` ran on.
    let dir = tempfile::tempdir().expect("scratch dir");
    let out = cash_in(
        dir.path(),
        &format!(
            r#"{{ cmd.exe /d /c exit 0; ping.exe -n 10 127.0.0.1 > out; }} &
               {UNTIL_OUT_IS_WRITTEN}
               kill -KILL %1; echo "kill=$?"
               wait %1; echo "status=$?""#
        ),
    );
    assert_eq!(out.stdout, "kill=0\nstatus=137", "stderr: {}", out.stderr);
}

#[test]
fn a_signal_to_a_job_reaches_both_ends_of_its_pipeline() {
    // Bash signals the job's process group. The second ping does not read its input, so
    // it does not end because the first did: only a signal of its own ends it early, and
    // the pipeline's status is its status.
    let dir = tempfile::tempdir().expect("scratch dir");
    let out = cash_in(
        dir.path(),
        &format!(
            r#"ping.exe -n 10 127.0.0.1 | ping.exe -n 10 127.0.0.1 > out &
               {UNTIL_OUT_IS_WRITTEN}
               kill -KILL %1; echo "kill=$?"
               wait %1; echo "status=$?""#
        ),
    );
    assert_eq!(out.stdout, "kill=0\nstatus=137", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// `killall -w`
// ---------------------------------------------------------------------------

#[test]
fn killall_w_waits_for_the_process_and_not_for_its_pid() {
    // A copy of cmd.exe under a name no other process has (see `OwnPing`), so `killall`
    // reaches this test's process alone.
    static COPIES: AtomicU32 = AtomicU32::new(0);
    let name = format!(
        "cash-test-cmd-{}-{}",
        std::process::id(),
        COPIES.fetch_add(1, Ordering::Relaxed)
    );
    let dir = tempfile::tempdir().expect("scratch dir");
    let program = dir.path().join(format!("{name}.exe"));
    std::fs::copy(system32("cmd.exe"), &program).expect("copying cmd.exe");
    let mut target = waits_then_exits(&program, 259);

    // `CONT` does nothing to a running process, so `-w` waits for the target to end by
    // itself; `-v` says when the signal has been sent.
    let mut shell = cash_command()
        .args([
            "-c",
            &format!(r#"killall -v -w -CONT {name}; echo "rc=$?""#),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run cash");
    let stdout = lines_of(shell.stdout.take().unwrap());
    let stderr = lines_of(shell.stderr.take().unwrap());
    line_after(&stderr, "Killed ");

    // The target ends with the status that reads as "still running", and the test goes
    // on holding it: its pid names a process for as long as `killall` might wait.
    drop(target.stdin.take());
    assert_eq!(target.wait().unwrap().code(), Some(259));

    let (rc, _) = line_after(&stdout, "rc=");
    assert_eq!(rc, "0");
    exit_of(shell, "cash");
}
