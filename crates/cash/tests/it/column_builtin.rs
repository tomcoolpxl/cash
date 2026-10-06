//! `column`, util-linux's: the golden output of `tests/oracle/column_cases.sh`, and the
//! behaviour the oracle cannot show (the console's width, Windows paths).
//!
//! The script ran under util-linux 2.42.3 to make `column_cases.out`; here it runs under
//! cash. Where cash differs on purpose, the expected text is replaced in the test with
//! the reason beside it, so a difference cannot hide in the golden file.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::PathBuf;
use std::process::Stdio;

use crate::common::{Scratch, cash_command, run};

fn oracle_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("oracle")
}

/// Runs the oracle script under cash; standard output and error together, as the golden
/// file was made.
fn run_oracle_script(name: &str) -> String {
    let out = cash_command()
        .arg(format!("{name}.sh"))
        .current_dir(oracle_dir())
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr)
}

fn golden(name: &str) -> String {
    std::fs::read_to_string(oracle_dir().join(format!("{name}.out")))
        .expect("read golden output")
        .replace("\r\n", "\n")
}

/// `golden` with `from` replaced by `to`, which must occur exactly once.
fn with_divergence(golden: &str, from: &str, to: &str) -> String {
    assert_eq!(golden.matches(from).count(), 1, "golden text moved: {from}");
    golden.replacen(from, to, 1)
}

#[test]
fn column_matches_util_linux() {
    let expected = with_divergence(
        &golden("column_cases"),
        // A CRLF line's CR is taken off the item and put back on the output line;
        // util-linux keeps it inside the item.
        " a \\r \\t b b \\r \\n\n",
        " a \\t b b \\r \\n\n",
    );
    let expected = with_divergence(
        &expected,
        // A byte that is not UTF-8 is kept, one cell wide; util-linux prints `\xff`.
        " a \\ x f f b _ _ c \\n d _ _ _ _ _\n _ _ e \\n\n",
        " a 377 b _ _ c \\n d _ _ _ _ e \\n\n",
    );
    let expected = with_divergence(
        &expected,
        // The CR of a CRLF line is a line ending, not part of the last cell.
        "         \"b\": \"b\\r\"\n",
        "         \"b\": \"b\"\n",
    );
    let expected = with_divergence(
        &expected,
        // A file that could not be read is a failure even when the table is printed;
        // util-linux's status is the table's.
        "a    b\nccc  d\nrc=0\n",
        "a    b\nccc  d\nrc=1\n",
    );
    let expected = with_divergence(
        &expected,
        "column from util-linux 2.42.3\n",
        "column (cash): util-linux 2.42.3's options\n",
    );
    assert_eq!(run_oracle_script("column_cases"), expected);
}

#[test]
fn column_is_a_builtin() {
    let out = run("type column");
    assert!(out.stdout.contains("shell builtin"), "{}", out.stdout);
}

#[test]
fn the_width_comes_from_columns_when_output_is_a_pipe() {
    let out = run("printf 'aaaa\\nbbbb\\ncccc\\ndddd\\n' | COLUMNS=20 column");
    assert_eq!(out.stdout, "aaaa\tcccc\nbbbb\tdddd");
    let out = run("printf 'aaaa\\nbbbb\\ncccc\\ndddd\\n' | COLUMNS=10 column");
    assert_eq!(out.stdout, "aaaa\nbbbb\ncccc\ndddd");
}

#[test]
fn files_take_windows_and_unix_paths() {
    let scratch = Scratch::new("column-paths");
    std::fs::write(scratch.join("sizes.txt"), "a 1\r\nbbb 22\r\n").expect("write input");
    let windows = scratch.join("sizes.txt").to_string_lossy().into_owned();
    let unix = format!("{}/sizes.txt", scratch.as_script_path());
    let out = run(&format!("column -t -R 2 '{windows}'"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "a     1\r\nbbb  22");
    let out = run(&format!("cd '{unix}'/.. && column -t sizes.txt"));
    assert_eq!(out.stdout, "a    1\r\nbbb  22");
}

#[test]
fn unsupported_options_are_refused_by_name() {
    for (option, shown) in [
        ("-T 1", "-T"),
        ("--table-wrap 1", "--table-wrap"),
        ("-E 1", "-E"),
        ("-C name=x", "-C"),
        ("-r 1 -i 1 -p 2", "-r"),
        ("--table-colorscheme x", "--table-colorscheme"),
    ] {
        let out = run(&format!("printf 'a b\\n' | column -t {option}"));
        assert_eq!(out.code, 1, "{option}: {}", out.stderr);
        assert_eq!(
            out.stderr,
            format!("column: {shown} is not supported"),
            "{option}"
        );
        assert_eq!(out.stdout, "", "{option}");
    }
}

#[test]
fn help_and_version_name_no_spec() {
    for flag in ["-h", "--help", "-V", "--version"] {
        let out = run(&format!("column {flag}"));
        assert_eq!(out.code, 0, "{flag}: {}", out.stderr);
        for word in ["D7", "ROADMAP", "spec.md", "§"] {
            assert!(!out.stdout.contains(word), "{flag} mentions {word}");
        }
    }
    let out = run("column -h");
    assert!(out.stdout.contains("--table-right"), "{}", out.stdout);
    assert!(out.stdout.contains("Not supported"), "{}", out.stdout);
}
