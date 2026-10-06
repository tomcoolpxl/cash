//! `getopt`, `rev`, `clear` and `reset` — ROADMAP item 10, part 2.
//!
//! `getopt` and `rev` are checked against util-linux 2.42.3 case by case: the scripts in
//! `tests/oracle` ran under the real tools to make the `.out` files, and run here under
//! cash. Where cash differs on purpose, the expected text is replaced in the test, with
//! the reason beside it, so a difference cannot hide in the golden file.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction"
)]

use std::os::windows::process::CommandExt as _;
use std::path::PathBuf;
use std::process::Stdio;

use crate::common::cash_command;

/// A console of its own, without a window, for `reset` to restore.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(crate) fn oracle_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("oracle")
}

/// Runs an oracle script under cash; standard output and error together, as the golden
/// files were made.
pub(crate) fn run_oracle_script(name: &str) -> String {
    let out = cash_command()
        .arg(format!("{name}.sh"))
        .current_dir(oracle_dir())
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr)
}

pub(crate) fn golden(name: &str) -> String {
    std::fs::read_to_string(oracle_dir().join(format!("{name}.out")))
        .expect("read golden output")
        .replace("\r\n", "\n")
}

/// `golden` with `from` replaced by `to`, which must occur exactly once.
pub(crate) fn with_divergence(golden: &str, from: &str, to: &str) -> String {
    assert_eq!(golden.matches(from).count(), 1, "golden text moved: {from}");
    golden.replacen(from, to, 1)
}

#[test]
fn getopt_matches_util_linux() {
    let expected = with_divergence(
        &golden("getopt_cases"),
        // Refused on purpose: cash is not a C shell, and csh quoting means nothing to it.
        "[tcsh] [-o] [a] [--] [-a]\nout=[ -a --]\nerr=[]\nrc=0\n",
        "[tcsh] [-o] [a] [--] [-a]\nout=[]\nerr=[getopt: -s tcsh is not supported: cash is \
         not a C shell\nTry 'getopt --help' for more information.]\nrc=2\n",
    );
    assert_eq!(run_oracle_script("getopt_cases"), expected);
}

#[test]
fn rev_matches_util_linux() {
    let expected = with_divergence(
        &golden("rev_cases"),
        // D20: a CRLF line keeps its CR at the end; util-linux moves it to the front.
        " \\r c b a \\n \\r y x \\n\n",
        " c b a \\r \\n y x \\r \\n\n",
    );
    assert_eq!(run_oracle_script("rev_cases"), expected);
}

#[test]
fn getopt_drives_a_real_option_loop() {
    // The idiom getopt exists for: canonicalise, eval, then walk the words.
    let script = r#"
        parse() {
            eval set -- "$(getopt -o vo: -l verbose,output: -n demo -- "$@")" || return
            while true; do
                case $1 in
                    -v|--verbose) echo verbose; shift ;;
                    -o|--output) echo "output=[$2]"; shift 2 ;;
                    --) shift; break ;;
                esac
            done
            for word; do echo "word=[$word]"; done
        }
        parse file1 -v --output='C:\out dir\x.txt' "it's" -o- --verb -- -v
    "#;
    let out = cash_command()
        .args(["-c", script])
        .output()
        .expect("run cash");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "verbose\noutput=[C:\\out dir\\x.txt]\noutput=[-]\nverbose\n\
         word=[file1]\nword=[it's]\nword=[-v]\n"
    );
}

#[test]
fn getopt_reports_itself_as_enhanced() {
    // Scripts test `getopt -T` for 4 before relying on long options.
    let out = cash_command()
        .args(["-c", "getopt -T; echo $?"])
        .output()
        .expect("run cash");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "4\n");
}

#[test]
fn clear_writes_ncurses_bytes() {
    let out = cash_command()
        .args(["-c", "clear; printf '|'; clear -x"])
        .output()
        .expect("run cash");
    assert_eq!(out.stdout, b"\x1b[H\x1b[2J\x1b[3J|\x1b[H\x1b[2J");
}

#[test]
fn reset_restores_the_console_and_writes_the_reset_sequence() {
    let out = cash_command()
        .args(["-c", "reset; echo rc=$?; reset -q"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    assert_eq!(
        out.stderr,
        b"\x1bc\x1b]104\x07\x1b[!p\x1b[?3;4l\x1b[4l\x1b>\x1b[?69l\r",
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("rc=0\n"), "{stdout}");
}

#[test]
fn a_terminal_that_is_not_vt_is_refused() {
    let out = cash_command()
        .args(["-c", "clear -T dumb; echo $?; reset dumb; echo $?"])
        .output()
        .expect("run cash");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "1\n1\n");
}
