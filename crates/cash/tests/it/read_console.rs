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
//! Ctrl-C was the next thing a console collecting a line could not do (2026-09-30). In a
//! plain `read` it did nothing, or ended cash where it stood, the interactive shell
//! included, with no `EXIT` trap run: the console made a control event of the key, which
//! cash ignored or had no handler for. In the reads above, which take the console's keys,
//! `read` returned 130 and the script went on with its next command. In Bash the script
//! ends there, unless a trap on `INT` runs instead, after which the read goes on. So
//! every read at a console takes its keys now, and an interrupted read ends the script.
//!
//! Each test runs a script file in cash on a pseudo console, types at it, and looks at
//! what the script read and what the console shows. Git Bash reads the same, which
//! `pty_oracle.rs` holds the two to at the prompt; of Ctrl-C it shows nothing, where
//! cash shows `^C` as a Linux terminal does.

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
use std::time::{Duration, Instant};

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
        let dir =
            std::env::temp_dir().join(format!("cash-read-console-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
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

    /// Waits for a read to show `prompt`, which it does once the keys are its to take.
    /// Ctrl-C typed before that is the console's to act on, not the read's.
    fn at_prompt(mut self, prompt: &str) -> Self {
        self.session
            .expect(prompt, STUCK)
            .expect("the read shows its prompt");
        self
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
        self.left()
    }

    /// Waits for cash to end, and returns its exit status with what the script left. A
    /// read that Ctrl-C does not end fails the test here.
    fn ends(mut self) -> (u32, Left) {
        let started = Instant::now();
        let status = loop {
            let _ = self.session.read_available();
            if let Some(status) = self.session.try_wait().expect("ask after cash") {
                break status;
            }
            assert!(
                started.elapsed() < STUCK,
                "cash is still running. The console shows:\n{}",
                self.session.screen().text()
            );
            std::thread::sleep(Duration::from_millis(15));
        };
        (status, self.left())
    }

    fn left(&mut self) -> Left {
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
    // The third read is not silent: its line is shown.
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
/// done: for `head`, which reads what the console hands it.
#[test]
fn the_console_is_as_it_was_after_the_read() {
    let left = Script::start(
        "restored",
        r#"read -rs -n 1 key; line=$(head -n 1); printf '%q %q\n' "$key" "$line" >> out.txt"#,
    )
    .type_keys("ktyped\r")
    .finish();

    assert_eq!(left.out, "k typed");
    assert_eq!(left.screen, "typed");
}

/// What becomes of a script when Ctrl-C is typed, after `ab`, at the prompt of its
/// `read`: cash's exit status, and what the script left. One that goes on past the read
/// says so in `out.txt`.
fn interrupted(name: &str, read: &str) -> (u32, Left) {
    Script::start(
        name,
        &format!(
            r#"trap 'echo "exit trap" >> out.txt' EXIT
{read}
echo "went on: rc=$? reply=$reply" >> out.txt"#
        ),
    )
    .at_prompt("reply> ")
    .type_keys("ab\x03")
    .ends()
}

/// The report: Ctrl-C ends the script where it reads, as SIGINT ends a Bash script, with
/// the status a shell gives a command a signal ended, 128 + 2, and the `EXIT` trap run.
/// A plain read did nothing until Enter, and went on with what was typed after.
#[test]
fn ctrl_c_ends_a_script_waiting_in_a_plain_read() {
    let (status, left) = interrupted("ctrl-c-plain", "read -r -p 'reply> ' reply");

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
    // Shown as a Linux terminal shows it; Git Bash's console shows `reply> ab`.
    assert_eq!(left.screen, "reply> ab^C");
}

/// These returned 130, and the script went on.
#[test]
fn ctrl_c_ends_a_script_waiting_for_a_count_of_characters() {
    let (status, left) = interrupted("ctrl-c-count", "read -r -n 5 -p 'reply> ' reply");

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
    assert_eq!(left.screen, "reply> ab^C");
}

#[test]
fn ctrl_c_ends_a_script_waiting_in_a_read_with_a_timeout() {
    let (status, left) = interrupted("ctrl-c-timeout", "read -r -t 5 -p 'reply> ' reply");

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
    assert_eq!(left.screen, "reply> ab^C");
}

#[test]
fn ctrl_c_ends_a_script_waiting_in_a_silent_read() {
    let (status, left) = interrupted("ctrl-c-silent", "read -rs -p 'reply> ' reply");

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
    assert_eq!(left.screen, "reply>");
}

#[test]
fn ctrl_c_ends_a_script_waiting_in_the_line_editor() {
    let (status, left) = interrupted("ctrl-c-editor", "read -re -p 'reply> ' reply");

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
    assert_eq!(left.screen, "reply> ab^C");
}

/// SIGINT reaches every process of a Bash script, so it ends whatever the read is a part
/// of, and the script with it. Here all of that is one process, and the interrupt is
/// passed on, out of each part.
#[test]
fn ctrl_c_ends_the_script_from_a_read_in_a_function() {
    let (status, left) = interrupted(
        "ctrl-c-function",
        r#"ask() { read -r -p 'reply> ' reply; echo "went on in the function" >> out.txt; }
ask"#,
    );

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
}

#[test]
fn ctrl_c_ends_the_script_from_a_read_in_a_loop_or_a_list() {
    let (status, left) = interrupted(
        "ctrl-c-loop",
        r#"while read -r -p 'reply> ' reply || echo "went on in the list" >> out.txt; do
    echo "went on in the loop" >> out.txt
done"#,
    );

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
}

#[test]
fn ctrl_c_ends_the_script_from_a_read_in_a_subshell() {
    let (status, left) = interrupted(
        "ctrl-c-subshell",
        r#"( read -r -p 'reply> ' reply; echo "went on in the subshell" >> out.txt )"#,
    );

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
}

#[test]
fn ctrl_c_ends_the_script_from_a_read_in_a_command_substitution() {
    let (status, left) = interrupted(
        "ctrl-c-substitution",
        r#"reply=$(read -r -p 'reply> ' reply; echo "went on in the substitution" >> out.txt)"#,
    );

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
}

#[test]
fn ctrl_c_ends_the_script_from_a_read_in_a_pipeline() {
    let (status, left) = interrupted("ctrl-c-pipeline", "read -r -p 'reply> ' reply | cat");

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
}

#[test]
fn ctrl_c_ends_the_script_from_a_read_in_a_sourced_file() {
    let (status, left) = interrupted(
        "ctrl-c-sourced",
        r#"printf '%s\n' 'read -r -p "reply> " reply' 'echo "went on in the file" >> out.txt' > asks.sh
. ./asks.sh"#,
    );

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
}

#[test]
fn ctrl_c_ends_the_script_from_a_read_in_an_eval() {
    let (status, left) = interrupted(
        "ctrl-c-eval",
        r#"eval 'read -r -p "reply> " reply
echo "went on in the eval" >> out.txt'"#,
    );

    assert_eq!(status, 130);
    assert_eq!(left.out, "exit trap");
}

/// What a script with a trap on `INT` reads when Ctrl-C is typed after `ab`, and then
/// `more`.
fn trapped(name: &str, read: &str, more: &str) -> Left {
    Script::start(
        name,
        &format!(
            r#"trap 'echo "trapped rc=$?" >> out.txt' INT
{read}
printf 'rc=%s reply=%q\n' "$?" "$reply" >> out.txt"#
        ),
    )
    .at_prompt("reply> ")
    .type_keys("ab\x03")
    .type_keys(more)
    .finish()
}

/// With a trap on `INT`, the trap runs and the read goes on, as in Bash. The line the
/// terminal was collecting is gone, as a terminal drops it at Ctrl-C.
#[test]
fn a_trap_on_int_runs_and_a_plain_read_goes_on_with_a_new_line() {
    let left = trapped("trap-plain", "read -r -p 'reply> ' reply", "cd\r");

    // The trap sees the status of the command before the read.
    assert_eq!(left.out, "trapped rc=0\nrc=0 reply=cd");
    // The prompt is shown once.
    assert_eq!(left.screen, "reply> ab^C\ncd");
}

#[test]
fn a_trap_on_int_runs_and_a_read_with_a_timeout_goes_on() {
    let left = trapped("trap-timeout", "read -r -t 5 -p 'reply> ' reply", "cd\r");

    assert_eq!(left.out, "trapped rc=0\nrc=0 reply=cd");
}

/// Characters read as they were typed have been read: they stay.
#[test]
fn a_trap_on_int_runs_and_a_read_of_a_count_keeps_what_it_had() {
    let left = trapped("trap-count", "read -r -n 5 -p 'reply> ' reply", "cde");

    assert_eq!(left.out, "trapped rc=0\nrc=0 reply=abcde");
}

/// The editor shows its line again and goes on editing it, as Readline does.
#[test]
fn a_trap_on_int_runs_and_the_line_editor_keeps_its_line() {
    let left = trapped("trap-editor", "read -re -p 'reply> ' reply", "cd\r");

    assert_eq!(left.out, "trapped rc=0\nrc=0 reply=abcd");
    assert_eq!(left.screen, "reply> ab^C\nreply> abcd");
}

#[test]
fn a_trap_on_int_that_exits_ends_the_script_with_its_status() {
    let (status, left) = Script::start(
        "trap-exits",
        r#"trap 'echo trapped >> out.txt; exit 7' INT
read -r -p 'reply> ' reply
echo "went on: rc=$?" >> out.txt"#,
    )
    .at_prompt("reply> ")
    .type_keys("ab\x03")
    .ends();

    assert_eq!(status, 7);
    assert_eq!(left.out, "trapped");
}

/// `trap '' INT` ignores the signal; the terminal still drops the line.
#[test]
fn an_ignored_int_leaves_the_read_waiting() {
    let left = Script::start(
        "ignored",
        r#"trap '' INT
read -r -p 'reply> ' reply
printf 'rc=%s reply=%q\n' "$?" "$reply" >> out.txt"#,
    )
    .at_prompt("reply> ")
    .type_keys("ab\x03cd\r")
    .finish();

    assert_eq!(left.out, "rc=0 reply=cd");
}

/// A line is edited as a terminal driver lets it be, now that the console does not
/// collect it: Ctrl-U takes the line back, Ctrl-W its last word and the blanks after it.
#[test]
fn ctrl_u_takes_back_the_line_and_ctrl_w_its_last_word() {
    let left = Script::start(
        "kill-and-word-erase",
        r#"read -r first; read -r second; printf '%q %q\n' "$first" "$second" >> out.txt"#,
    )
    .type_keys("wrong\x15right\rone two  \x17three\r")
    .finish();

    assert_eq!(left.out, r"right one\ three");
    assert_eq!(left.screen, "right\none three");
}

/// Ctrl-Z where a line starts ends input, as it did when the console collected the line
/// and as Windows users know it. Further on it is a character.
#[test]
fn ctrl_z_where_a_line_starts_ends_input() {
    let left = Script::start(
        "ctrl-z",
        r#"read -r first; printf 'rc=%s %q\n' "$?" "$first" >> out.txt
read -r second; printf 'rc=%s %q\n' "$?" "$second" >> out.txt"#,
    )
    .type_keys("\x1aa\x1ab\r")
    .finish();

    assert_eq!(left.out, "rc=1 ''\nrc=0 $'a\\032b'");
}
