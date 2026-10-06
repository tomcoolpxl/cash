//! `stty`: GNU coreutils' words on the Windows console modes.
//!
//! Without a terminal on standard input it fails as GNU's does on a pipe. With one, on a
//! pseudo console, the four settings the console holds are set and read back, the
//! remembered ones round-trip through `-g`, and `stty -echo; read -r pw; stty echo` hides
//! what is typed, as in Bash: the main reason a script calls `stty` at all.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::run;
use crate::read_console::Script;

/// GNU's error for a standard input that is not a terminal.
const NOT_A_TERMINAL: &str = "stty: 'standard input': Inappropriate ioctl for device";

#[test]
fn without_a_terminal_stty_fails_as_gnu_does() {
    // `cash -c` under `output()` has no standard input at all.
    let out = run("stty size; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert_eq!(out.stderr, NOT_A_TERMINAL);

    let out = run("stty -a </dev/null; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert_eq!(out.stderr, NOT_A_TERMINAL);

    let out = run("echo | stty -echo; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert_eq!(out.stderr, NOT_A_TERMINAL);
}

#[test]
fn help_and_version_need_no_terminal() {
    let out = run("stty --help");
    assert_eq!(out.code, 0);
    assert!(
        out.stdout.starts_with("Usage: stty [SETTING]..."),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("[-]echo       echo input characters"));
    assert!(
        out.stdout
            .contains("On Windows, the terminal is the console")
    );

    let out = run("stty --version");
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        "stty (cash): GNU coreutils 9.11's words, on the Windows console modes"
    );
}

/// The options are checked before the terminal is, as GNU checks them.
#[test]
fn an_invalid_argument_and_clashing_options_are_refused_in_gnus_words() {
    let out = run("stty -a -g; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert_eq!(
        out.stderr,
        "stty: the options for verbose and stty-readable output styles are\nmutually exclusive"
    );

    let out = run("stty -a -echo; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert_eq!(
        out.stderr,
        "stty: when specifying an output style, modes may not be set"
    );

    let out = run("stty -F COM1 -echo; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert!(
        out.stderr.starts_with("stty: --file is not supported"),
        "{}",
        out.stderr
    );
}

/// What a script sees of `stty` on a console: the settings it reads back and the text
/// `stty` printed, each written to `out.txt` by the script itself.
fn on_a_console(name: &str, body: &str) -> String {
    Script::start(name, body).finish().out
}

#[test]
fn an_invalid_argument_is_refused_on_a_console_too() {
    let out = on_a_console(
        "invalid",
        r#"stty foo 2>> out.txt; echo "rc=$?" >> out.txt
stty -cs8 2>> out.txt; echo "rc=$?" >> out.txt
stty erase 2>> out.txt; echo "rc=$?" >> out.txt"#,
    );
    assert_eq!(
        out,
        "stty: invalid argument 'foo'\nTry 'stty --help' for more information.\nrc=1\n\
         stty: invalid argument '-cs8'\nTry 'stty --help' for more information.\nrc=1\n\
         stty: missing argument to 'erase'\nTry 'stty --help' for more information.\nrc=1"
    );
}

/// `-echo` goes to the console and comes back from it; bare `stty` reports it as GNU
/// does, and `echo` undoes it.
#[test]
fn echo_is_set_on_the_console_and_reported() {
    let out = on_a_console(
        "echo",
        r#"stty >> out.txt
stty -echo; echo "rc=$?" >> out.txt
stty >> out.txt
stty echo
stty >> out.txt"#,
    );
    assert_eq!(
        out,
        "speed 38400 baud; line = 0;\nrc=0\nspeed 38400 baud; line = 0;\n-echo\nspeed 38400 baud; line = 0;"
    );
}

/// The report: a password read in a script is not shown.
#[test]
fn stty_echo_off_hides_what_read_reads() {
    let left = Script::start(
        "password",
        r#"stty -echo; IFS= read -r password; stty echo
printf 'got %s\n' "$password" >> out.txt"#,
    )
    .type_keys("secret\r")
    .finish();

    assert_eq!(left.out, "got secret");
    assert_eq!(left.screen, "", "the password was shown");
}

/// `stty echo` afterwards: the next read shows its line again.
#[test]
fn stty_echo_shows_the_next_read_again() {
    let left = Script::start(
        "echo-again",
        r#"stty -echo; IFS= read -r hidden; stty echo; IFS= read -r shown
printf '%s %s\n' "$hidden" "$shown" >> out.txt"#,
    )
    .type_keys("one\rtwo\r")
    .finish();

    assert_eq!(left.out, "one two");
    assert_eq!(left.screen, "two");
}

/// `-a` has GNU's layout and the console's size; `size` agrees with it; `-g` restores
/// what a script changed, mapped settings and remembered ones alike.
#[test]
fn stty_a_size_and_g_report_and_restore() {
    let out = on_a_console(
        "report",
        r#"stty -a >> out.txt
echo "size=$(stty size)" >> out.txt
saved=$(stty -g)
stty raw -echo erase ^H 9600 min 3
stty >> out.txt
stty "$saved"; echo "rc=$?" >> out.txt
stty >> out.txt
echo "same=$([ "$(stty -g)" = "$saved" ] && echo yes || echo no)" >> out.txt"#,
    );
    let mut lines = out.lines();
    let first = lines.next().expect("the speed line");
    assert!(first.starts_with("speed 38400 baud; rows "), "{first}");
    assert!(first.ends_with("; line = 0;"), "{first}");
    let rows_cols: Vec<&str> = first
        .split("; ")
        .filter_map(|part| {
            part.strip_prefix("rows ")
                .or_else(|| part.strip_prefix("columns "))
        })
        .collect();
    assert_eq!(rows_cols.len(), 2, "{first}");
    assert_eq!(
        lines.next(),
        Some("intr = ^C; quit = ^\\; erase = ^?; kill = ^U; eof = ^D; eol = <undef>;")
    );
    assert!(
        out.contains("\n-parenb -parodd -cmspar cs8 -hupcl -cstopb cread -clocal -crtscts\n"),
        "{out}"
    );
    assert!(
        out.contains(
            "\nisig icanon iexten echo echoe echok -echonl -noflsh -xcase -tostop -echoprt\n"
        ),
        "{out}"
    );
    assert!(
        out.contains(&format!("\nsize={} {}\n", rows_cols[0], rows_cols[1])),
        "{out}"
    );
    assert!(
        out.contains(
            "\nspeed 9600 baud; line = 0;\nerase = ^H; min = 3; time = 0;\n-brkint -icrnl -imaxbel\n-opost\n-isig -icanon -echo\nrc=0\nspeed 38400 baud; line = 0;\nsame=yes"
        ),
        "{out}"
    );
}

/// `sane` after `raw`: the console collects lines and shows keys again, for the next
/// read, and `cooked` the same.
#[test]
fn sane_puts_the_console_back() {
    let left = Script::start(
        "sane",
        r#"stty raw -echo; stty sane; IFS= read -r line
printf '%s\n' "$line" >> out.txt
stty >> out.txt"#,
    )
    .type_keys("typed\r")
    .finish();

    assert_eq!(left.out, "typed\nspeed 38400 baud; line = 0;");
    assert_eq!(left.screen, "typed");
}

/// `rows` and `cols` change nothing and say nothing, as the real window is what `size`
/// reports; `speed` prints the speed.
#[test]
fn rows_cols_and_speed() {
    let out = on_a_console(
        "rows",
        r#"before=$(stty size)
stty rows 50 cols 132; echo "rc=$?" >> out.txt
[ "$(stty size)" = "$before" ] && echo same-size >> out.txt
stty 115200 speed >> out.txt
stty 38400"#,
    );
    assert_eq!(out, "rc=0\nsame-size\n115200");
}
