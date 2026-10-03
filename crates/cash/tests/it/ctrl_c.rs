//! Ctrl-C and a running script — **D13**.
//!
//! Windows sends the keyboard's Ctrl-C to every process on the console, and ends one that
//! has no handler for it where it stands. For a script that meant one of two things
//! (2026-09-30). While it waited for a program, the program died and the script went on
//! with its next command:
//!
//! ```text
//! ping.exe -n 30 127.0.0.1 > /dev/null
//! echo "after-ping rc=$?"          # printed, with 137
//! ```
//!
//! And while it ran commands of its own, a loop of builtins, Windows ended cash: no `EXIT`
//! trap, no trap on `INT`. Git Bash ends the script in both cases, its `EXIT` trap run,
//! or runs the trap on `INT` and goes on.
//!
//! `select` and `mapfile` waited for the keyboard as `read` had: they left the console to
//! collect a line, where Ctrl-C did nothing before Enter or ended cash. They take the
//! console's keys now, as `read` does, and their tests are at the end of this file.
//!
//! Each test runs a script file in cash on a pseudo console and types Ctrl-C at it. In the
//! first tests the console is collecting a line then, so the key is the console's control
//! event, as it is for a user; `read_console.rs` has the reads, where it is a key.

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

use super::read_console::Script;

use crate::common::CASH;

/// A program that runs until something ends it, and writes `127.0.0.1` to the console
/// once it is running, whatever the display language.
const PING: &str = "ping.exe -n 30 127.0.0.1";

/// Runs `body` as a script in a cash that Ctrl-C can reach.
///
/// A process ignores Ctrl-C if its parent did, and a test runner may: the scripts here
/// are started as a user's are, from a process that does not.
fn script(name: &str, body: &str) -> Script {
    cash_win32::console::enable_ctrl_c();
    Script::start(name, body)
}

/// The report's first half: the program dies of the Ctrl-C, and the script ends with it,
/// with the status of a command SIGINT ended and its `EXIT` trap run.
#[test]
fn ctrl_c_ends_a_script_whose_program_it_ended() {
    let (status, left) = script(
        "ctrl-c-program",
        &format!(
            r#"trap 'echo "exit trap" >> out.txt' EXIT
{PING}
echo "went on: rc=$?" >> out.txt"#
        ),
    )
    .when_shown("127.0.0.1")
    .type_keys("\x03")
    .ends();

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
}

/// The second half: nothing of cash was listening, and Windows ended it.
#[test]
fn ctrl_c_ends_a_script_running_commands_of_its_own() {
    let (status, left) = script(
        "ctrl-c-builtins",
        r#"trap 'echo "exit trap" >> out.txt' EXIT
echo looping
while :; do :; done
echo "went on" >> out.txt"#,
    )
    .when_shown("looping")
    .type_keys("\x03")
    .ends();

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
}

#[test]
fn a_trap_on_int_runs_between_commands_and_the_script_goes_on() {
    let left = script(
        "trap-builtins",
        r#"trap 'echo trapped >> out.txt; stop=1' INT
echo looping
while [ -z "$stop" ]; do :; done
echo "went on" >> out.txt"#,
    )
    .when_shown("looping")
    .type_keys("\x03")
    .finish();

    assert_eq!(left.out, "trapped\nwent on");
}

/// As in Bash, the trap runs once the program has ended, and `$?` is the program's.
#[test]
fn a_trap_on_int_runs_after_the_program_the_ctrl_c_ended() {
    let left = script(
        "trap-program",
        &format!(
            r#"trap 'echo trapped >> out.txt' INT
{PING}
echo "went on: rc=$?" >> out.txt"#
        ),
    )
    .when_shown("127.0.0.1")
    .type_keys("\x03")
    .finish();

    assert_eq!(left.out, "trapped\nwent on: rc=130");
}

/// D13: a program that takes the interrupt in its stride does not take the script down
/// with it. Here it is a cash with a trap of its own, which ends with status 0.
#[test]
fn a_program_that_handles_ctrl_c_itself_leaves_the_script_running() {
    let cash = CASH.replace('\\', "/");
    let left = script(
        "survivor",
        &format!(
            r#"'{cash}' --no-config -c 'trap "echo handled >> out.txt" INT; {PING}; exit 0'
echo "went on: rc=$?" >> out.txt"#
        ),
    )
    .when_shown("127.0.0.1")
    .type_keys("\x03")
    .finish();

    assert_eq!(left.out, "handled\nwent on: rc=0");
}

/// A script that runs a script: the inner one ends as a shell an interrupt ended does,
/// with 130, and the outer one ends for it, as both do in Bash.
#[test]
fn ctrl_c_ends_the_script_that_started_the_interrupted_script() {
    let cash = CASH.replace('\\', "/");
    cash_win32::console::enable_ctrl_c();
    let (status, left) = Script::start_beside(
        "ctrl-c-nested",
        &format!(
            r#"trap 'echo "outer exit trap" >> out.txt' EXIT
'{cash}' --no-config inner.sh
echo "outer went on: rc=$?" >> out.txt"#
        ),
        &[(
            "inner.sh",
            &format!("{PING}\necho \"inner went on\" >> out.txt\n"),
        )],
    )
    .when_shown("127.0.0.1")
    .type_keys("\x03")
    .ends();

    assert_eq!(status, 130);
    assert_eq!(left.out, "outer exit trap");
}

/// `select` asks with its prompt, which it shows once the keys are its own.
#[test]
fn ctrl_c_ends_a_script_waiting_in_select() {
    let (status, left) = script(
        "ctrl-c-select",
        r#"trap 'echo "exit trap" >> out.txt' EXIT
select x in one two; do echo "chose $x" >> out.txt; break; done
echo "went on: rc=$?" >> out.txt"#,
    )
    .at_prompt("#? ")
    .type_keys("ab\x03")
    .ends();

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
    assert_eq!(left.screen, "1) one\n2) two\n#? ab^C");
}

/// As in Bash: the trap runs, and `select` goes on waiting for its answer, without
/// asking again. What was typed of the line is dropped, as a terminal drops it.
#[test]
fn a_trap_on_int_runs_and_select_goes_on_waiting() {
    let left = script(
        "trap-select",
        r#"trap 'echo trapped >> out.txt' INT
select x in one two; do echo "chose $x" >> out.txt; break; done"#,
    )
    .at_prompt("#? ")
    .type_keys("ab\x03")
    .type_keys("1\r")
    .finish();

    assert_eq!(left.out, "trapped\nchose one");
    assert_eq!(left.screen, "1) one\n2) two\n#? ab^C\n1");
}

/// `mapfile` has no prompt. A line it has shown was read by it, so the first line typed
/// says when it has the keys.
#[test]
fn ctrl_c_ends_a_script_waiting_in_mapfile() {
    let (status, left) = script(
        "ctrl-c-mapfile",
        r#"trap 'echo "exit trap" >> out.txt' EXIT
mapfile -t lines
echo "went on: ${#lines[@]}" >> out.txt"#,
    )
    .type_keys("one\r")
    .when_shown("one")
    .type_keys("tw\x03")
    .ends();

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
    assert_eq!(left.screen, "one\ntw^C");
}

/// The trap runs and the reading goes on: the line Ctrl-C was typed in is dropped, and
/// Ctrl-D where a line starts ends the input.
#[test]
fn a_trap_on_int_runs_and_mapfile_goes_on_reading() {
    let left = script(
        "trap-mapfile",
        r#"trap 'echo trapped >> out.txt' INT
mapfile -t lines
echo "${#lines[@]}: ${lines[*]}" >> out.txt"#,
    )
    .type_keys("one\r")
    .when_shown("one")
    .type_keys("tw\x03")
    .type_keys("three\r\x04")
    .finish();

    assert_eq!(left.out, "trapped\n2: one three");
    assert_eq!(left.screen, "one\ntw^C\nthree");
}
