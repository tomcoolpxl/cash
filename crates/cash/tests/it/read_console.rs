//! `read` at a console: `-t`, `-d`, `-n` and `-s`.
//!
//! A script asked the terminal for its cell size and read the answer:
//!
//! ```text
//! printf '\033[16t'
//! IFS= read -rs -t 2 -d t reply
//! ```
//!
//! The answer, `ESC [ 6 ; 20 ; 10 t`, was shown on the screen as `^[[6;20;10t`, and cash
//! was still waiting for it 25 minutes later (2026-09-30). None of the three options did
//! anything at a console. It collected a line as a console does, showing the keys, until
//! an Enter that a terminal's answer does not have; and the timeout was looked at only
//! before the first key. A line typed in full and in time fared no better: `read -t 5`
//! kept its first character and reported a timeout.
//!
//! A script whose input or output is a pipe or a file talks to the terminal by name
//! instead, the usual form of such a query:
//!
//! ```text
//! printf '\033[16t' > /dev/tty
//! IFS= read -rs -t 2 -d t reply < /dev/tty
//! ```
//!
//! That could not be written at all: `/dev/tty` was looked for as the file `C:/dev/tty`
//! (2026-09-30). It is the console cash is attached to, its keys when opened to be read
//! and its screen when opened to be written, and the second half of this file reads and
//! writes it with the standard streams redirected.
//!
//! Each test runs a script file in cash on a pseudo console, types at it, and looks at
//! what the script read and what the console shows. Git Bash reads and shows the same,
//! which `pty_oracle.rs` holds the two to at the prompt.

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

use std::path::{Path, PathBuf};
use std::time::Duration;

use cash_win32::conpty::ConPtySession;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// What a script prints before it reads, and once it is done.
const READING: &str = "now-reading";
const FINISHED: &str = "read-finished";

/// How long a script may take to finish. Every read here has what it waits for within a
/// second, or a timeout of two at most: one still running after this is stuck.
const STUCK: Duration = Duration::from_secs(10);

/// A script running in cash on a pseudo console, in a folder of its own.
struct Script {
    session: ConPtySession,
    dir: PathBuf,
}

/// What a script left: the lines it wrote to `out.txt`, and what the console shows.
struct Left {
    out: String,
    screen: String,
}

impl Script {
    /// Runs `body` as a script file, as `cash script.sh`, and waits until it is about to
    /// read. `body` writes what it read to `out.txt`.
    fn start(name: &str, body: &str) -> Self {
        Self::start_beside(name, body, &[])
    }

    /// Like [`Self::start`], with `files` in the script's folder: a second script, for a
    /// cash the first one starts with a standard input and output of its own.
    fn start_beside(name: &str, body: &str, files: &[(&str, &str)]) -> Self {
        let dir =
            std::env::temp_dir().join(format!("cash-read-console-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
        for (file, contents) in files {
            std::fs::write(dir.join(file), contents).expect("write a file beside the script");
        }
        std::fs::write(
            dir.join("script.sh"),
            format!("echo {READING}\n{body}\necho {FINISHED}\n"),
        )
        .expect("write the script");

        let mut session =
            ConPtySession::start_in(Path::new(CASH), &["script.sh"], None, Some(&dir))
                .expect("start cash in a pseudo terminal");
        session
            .expect(READING, STUCK)
            .expect("the script reaches its read");
        Self { session, dir }
    }

    /// Waits a moment for the read to have begun, as a terminal's answer to a query
    /// arrives after the query. What is typed ahead of a read waits for it, but the
    /// console makes a key's or an answer's escape sequence of what arrives while the
    /// read is on.
    fn once_reading(mut self) -> Self {
        self.session
            .settle(Duration::from_millis(300), Duration::from_secs(2))
            .expect("read the console");
        self
    }

    /// Waits until the console shows `text`, which the script writes just before it
    /// reads, and then as [`Self::once_reading`] does: for a read that a second cash has
    /// to start for.
    fn once_shown(mut self, text: &str) -> Self {
        self.session
            .expect(text, STUCK)
            .expect("the script writes to the console before it reads");
        self.once_reading()
    }

    fn type_keys(mut self, keys: &str) -> Self {
        self.session.send(keys).expect("type at the console");
        self
    }

    /// Waits for the script to finish; a read that never returns fails the test here.
    fn finish(mut self) -> Left {
        self.session
            .expect(FINISHED, STUCK)
            .expect("the script finishes: its read returned");
        self.session
            .settle(Duration::from_millis(200), Duration::from_secs(5))
            .expect("the screen settles");
        let out = std::fs::read_to_string(self.dir.join("out.txt")).unwrap_or_default();
        let screen = self.session.screen().text();
        Left {
            out: out.trim_end().to_owned(),
            screen: screen
                .replace(READING, "")
                .replace(FINISHED, "")
                .trim()
                .to_owned(),
        }
    }
}

impl Drop for Script {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The report: the answer to a query ends at its last character, is not shown, and
/// arrives whole, its escape included.
#[test]
fn a_terminals_answer_is_read_to_its_last_character_and_not_shown() {
    let left = Script::start(
        "answer",
        r#"ask() {
    local reply=
    printf '\033[%s' "$1"
    IFS= read -rs -t 2 -d "$2" reply
    printf '%s -> %q rc=%s\n' "$1" "$reply" "$?" >> out.txt
}
ask 16t t"#,
    )
    .once_reading()
    .type_keys("\x1b[6;20;10t")
    .finish();

    // A read the timeout ended would report 142.
    assert_eq!(left.out, r#"16t -> $'\E[6;20;10' rc=0"#);
    assert_eq!(left.screen, "", "the answer was shown");
}

/// Each option on its own, with keys that arrive as the characters they are whatever
/// the console's mode, so typed ahead of the read: nothing here depends on timing.
#[test]
fn a_delimiter_ends_the_read_without_enter() {
    let left = Script::start(
        "delimiter",
        r#"IFS= read -r -d t reply; printf '%q rc=%s\n' "$reply" "$?" >> out.txt"#,
    )
    .type_keys("6;20;10t")
    .finish();

    assert_eq!(left.out, r#"6\;20\;10 rc=0"#);
    // Not silent, so shown, the delimiter too: as a terminal shows what is typed.
    assert_eq!(left.screen, "6;20;10t");
}

#[test]
fn a_count_of_characters_ends_the_read_without_enter() {
    let left = Script::start(
        "count",
        r#"read -r -n 3 reply; printf '%q rc=%s\n' "$reply" "$?" >> out.txt"#,
    )
    .type_keys("abc")
    .finish();

    assert_eq!(left.out, "abc rc=0");
    assert_eq!(left.screen, "abc");
}

#[test]
fn keys_past_the_count_wait_for_the_next_read() {
    let left = Script::start(
        "typed-ahead",
        r#"read -rs -n 1 first; read -rs -N 2 rest; read -r line
printf '%q %q %q\n' "$first" "$rest" "$line" >> out.txt"#,
    )
    .type_keys("abcdef\r")
    .finish();

    assert_eq!(left.out, "a bc def");
    // The third read asks nothing of the console but a line, which the console shows.
    assert_eq!(left.screen, "def");
}

#[test]
fn a_silent_read_shows_nothing() {
    let left = Script::start(
        "silent",
        r#"read -rs reply; printf '%q rc=%s\n' "$reply" "$?" >> out.txt"#,
    )
    .type_keys("secret\r")
    .finish();

    assert_eq!(left.out, "secret rc=0");
    assert_eq!(left.screen, "", "a silent read showed what was typed");
}

#[test]
fn the_timeout_ends_a_read_that_has_keys_but_no_enter() {
    let left = Script::start(
        "timeout",
        r#"read -r -t 1 reply; printf '%q rc=%s\n' "$reply" "$?" >> out.txt"#,
    )
    .type_keys("abc")
    .finish();

    // As in Bash, where the terminal keeps a line until its Enter: nothing was read.
    assert_eq!(left.out, "'' rc=142");
    assert_eq!(left.screen, "abc");
}

#[test]
fn the_timeout_leaves_what_was_read_short_of_the_delimiter() {
    let left = Script::start(
        "partial",
        r#"IFS= read -rs -t 1 -d t reply; printf '%q rc=%s\n' "$reply" "$?" >> out.txt"#,
    )
    .type_keys("6;20")
    .finish();

    assert_eq!(left.out, r#"6\;20 rc=142"#);
    assert_eq!(left.screen, "");
}

/// `read -t 5` kept `h` and reported a timeout five seconds later: the console handed
/// over the whole line at once, and the wait for the second character looked for a key
/// that was no longer in the console's queue.
#[test]
fn a_line_typed_within_the_timeout_is_read_whole() {
    let left = Script::start(
        "line-in-time",
        r#"read -r -t 5 reply; printf '%q rc=%s\n' "$reply" "$?" >> out.txt"#,
    )
    .type_keys("hello\r")
    .finish();

    assert_eq!(left.out, "hello rc=0");
    assert_eq!(left.screen, "hello");
}

#[test]
fn backspace_takes_back_a_character_of_a_line_read_with_a_timeout() {
    let left = Script::start(
        "backspace",
        r#"read -r -t 5 reply; printf '%q rc=%s\n' "$reply" "$?" >> out.txt"#,
    )
    .type_keys("helx\x7flo\r")
    .finish();

    assert_eq!(left.out, "hello rc=0");
    assert_eq!(left.screen, "hello");
}

/// A menu in a script reads a key, and the rest of its sequence if it was Escape.
#[test]
fn an_arrow_key_is_read_as_the_sequence_a_terminal_sends() {
    let left = Script::start(
        "arrow",
        r#"IFS= read -rs -n 1 key; IFS= read -rs -n 2 -t 0.05 rest
printf '%q %q\n' "$key" "$rest" >> out.txt"#,
    )
    .once_reading()
    .type_keys("\x1b[A")
    .finish();

    assert_eq!(left.out, r#"$'\E' \[A"#);
}

/// The console collects lines again, and shows them, once a read that took its keys is
/// done.
#[test]
fn the_console_is_as_it_was_after_the_read() {
    let left = Script::start(
        "restored",
        r#"read -rs -n 1 key; read -r line; printf '%q %q\n' "$key" "$line" >> out.txt"#,
    )
    .type_keys("ktyped\r")
    .finish();

    assert_eq!(left.out, "k typed");
    assert_eq!(left.screen, "typed");
}

/// The report's script, as a program runs it: a cash whose standard input is a pipe and
/// whose standard output is a file asks the terminal and reads its answer. What it asks
/// reaches the screen, the answer is not shown, and the pipe still holds its line.
#[test]
fn a_script_with_both_streams_redirected_asks_the_terminal_by_name() {
    let left = Script::start_beside(
        "tty-query",
        r#"echo piped | cash asks.sh > out.txt"#,
        &[(
            "asks.sh",
            r#"printf 'cell size? ' > /dev/tty
IFS= read -rs -t 5 -d t reply < /dev/tty
printf '%q rc=%s\n' "$reply" "$?"
IFS= read -r rest
printf 'stdin: %s\n' "$rest"
"#,
        )],
    )
    .once_shown("cell size?")
    .type_keys("\x1b[6;20;10t")
    .finish();

    assert_eq!(left.out, "$'\\E[6;20;10' rc=0\nstdin: piped");
    assert_eq!(left.screen, "cell size?", "the answer was shown");
}

/// A line from the terminal while standard input is a pipe: it is typed, shown and
/// edited as at a terminal, and the timeout, which a file's read ignores, is kept.
#[test]
fn a_line_is_read_from_the_terminal_while_stdin_is_a_pipe() {
    let left = Script::start(
        "tty-line",
        r#"echo piped | {
    read -r -t 5 typed < /dev/tty; rc=$?
    read -r rest
    printf '%q rc=%s %q\n' "$typed" "$rc" "$rest" >> out.txt
}"#,
    )
    .type_keys("helx\x7flo\r")
    .finish();

    assert_eq!(left.out, "hello rc=0 piped");
    assert_eq!(left.screen, "hello");
}

#[test]
fn the_timeout_ends_a_read_from_the_terminal_that_nothing_is_typed_at() {
    let left = Script::start(
        "tty-timeout",
        r#"echo piped | { read -r -t 1 reply < /dev/tty; printf '%q rc=%s\n' "$reply" "$?" >> out.txt; }"#,
    )
    .finish();

    assert_eq!(left.out, "'' rc=142");
    assert_eq!(left.screen, "");
}

#[test]
fn a_count_of_characters_is_read_from_the_terminal_without_enter() {
    let left = Script::start(
        "tty-count",
        r#"echo piped | { read -r -n 3 reply < /dev/tty; printf '%q rc=%s\n' "$reply" "$?" >> out.txt; }"#,
    )
    .type_keys("abc")
    .finish();

    assert_eq!(left.out, "abc rc=0");
    assert_eq!(left.screen, "abc");
}

#[test]
fn a_silent_read_from_the_terminal_shows_nothing() {
    let left = Script::start(
        "tty-silent",
        r#"echo piped | { read -rs reply < /dev/tty; printf '%q rc=%s\n' "$reply" "$?" >> out.txt; }"#,
    )
    .type_keys("secret\r")
    .finish();

    assert_eq!(left.out, "secret rc=0");
    assert_eq!(left.screen, "", "a silent read showed what was typed");
}

/// No option at all, and one descriptor read twice: a loop over the terminal's lines.
/// Letters outside ASCII arrive as they were typed, whatever the console's code page.
#[test]
fn lines_are_read_from_the_terminal_one_after_another() {
    let left = Script::start(
        "tty-lines",
        r#"echo piped | {
    { IFS= read -r first; IFS= read -r second; } < /dev/tty
    printf '%s|%s\n' "$first" "$second" >> out.txt
}"#,
    )
    .type_keys("één\rtwee\r")
    .finish();

    assert_eq!(left.out, "één|twee");
    assert_eq!(left.screen, "één\ntwee");
}

/// `[ -t 0 ]` is how a script decides whether to ask at all.
#[test]
fn the_terminal_opened_by_name_is_a_terminal() {
    let left = Script::start(
        "tty-is-a-terminal",
        r#"echo piped | {
    [ -t 0 ] && echo stdin-is-a-terminal >> out.txt
    [ -t 0 ] < /dev/tty && echo keys-are-a-terminal >> out.txt
}
[ -t 1 ] > captured.txt && echo a-file-is-a-terminal >> out.txt
[ -t 1 ] > /dev/tty && echo screen-is-a-terminal >> out.txt"#,
    )
    .finish();

    assert_eq!(left.out, "keys-are-a-terminal\nscreen-is-a-terminal");
}

/// A program given the terminal as its standard input reads what is typed: the handle
/// is one a child can inherit and read as a console.
#[test]
fn a_program_started_with_the_terminal_as_its_input_reads_what_is_typed() {
    let left = Script::start(
        "tty-child",
        r#"echo piped | cash -c 'IFS= read -r line; printf "%s\n" "$line"' < /dev/tty >> out.txt"#,
    )
    .type_keys("for the child\r")
    .finish();

    assert_eq!(left.out, "for the child");
    assert_eq!(left.screen, "for the child");
}

/// Writing, with standard output redirected: by the shell, appending, both streams at
/// once, and by a program given the screen as its standard output.
#[test]
fn what_is_written_to_the_terminal_reaches_the_screen_not_the_redirected_output() {
    let left = Script::start(
        "tty-write",
        r#"{
    echo shown > /dev/tty
    echo appended >> /dev/tty
    echo both-streams &> /dev/tty
    cmd /c echo from-a-program > /dev/tty
    echo kept
} > out.txt"#,
    )
    .finish();

    assert_eq!(left.out, "kept");
    assert_eq!(left.screen, "shown\nappended\nboth-streams\nfrom-a-program");
}

/// A cash with no console, as a service or a detached process has none: Bash says the
/// same of a process without a controlling terminal, and the command does not run.
#[test]
fn without_a_console_there_is_no_terminal_to_open() {
    use std::os::windows::process::CommandExt as _;

    /// The process is given no console, where it would inherit or be given one.
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let output = std::process::Command::new(CASH)
        .args([
            "-c",
            r#"echo written > /dev/tty; echo "write rc=$?"
read -t 1 reply < /dev/tty; echo "read rc=$?"
echo both &> /dev/tty; echo "both rc=$?""#,
        ])
        .stdin(std::process::Stdio::null())
        .creation_flags(DETACHED_PROCESS)
        .output()
        .expect("run cash without a console");

    assert_eq!(
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        "write rc=1\nread rc=1\nboth rc=1\n"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr
            .matches("failed to redirect to /dev/tty: No such device or address")
            .count(),
        3,
        "{stderr}"
    );
}
