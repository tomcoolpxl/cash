//! `grep`, `egrep` and `fgrep`: the golden output of `tests/oracle/grep_cases.sh`, and
//! the behaviour the oracle cannot show (the CRLF rule in detail, `-P` refused, the
//! names, the help page).
//!
//! The script ran under GNU grep 3.12 to make `grep_cases.out`; here it runs under
//! cash. Where cash differs on purpose, the expected text is replaced in the test with
//! the reason beside it, so a difference cannot hide in the golden file.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, golden, run, run_in, run_oracle_script, with_divergence};

/// The bytes of `od -An -c` output, as words.
fn od_words(text: &str) -> Vec<&str> {
    text.split_whitespace().collect()
}

#[test]
fn grep_matches_gnu_grep() {
    // The CRLF rule: a line ending in CRLF is matched without its `\r`, so `o$`, `-x`,
    // `-F -x` and `o*$` match where GNU grep's do not, and `.` and `[^a-z]$` do not
    // match the `\r` where GNU grep's do; `-o` never prints the `\r`. The line itself
    // is printed as it was read, and `-U` keeps the `\r`, as GNU grep on Linux has it.
    let expected = with_divergence(
        &golden("grep_cases"),
        "== CRLF\n0\n f o o \\r \\n\n f o o \\n\n f o o \\r \\n\n1\nrc=1\n4:foo\n5:oo\n",
        "== CRLF\n f o o \\r \\n\n1\n f o o \\r \\n\n f o o \\n\n f o o \\r \\n\n f o o \\n\n\
         0\nfoo\r\nrc=0\n4:foo\r\n5:oo\n",
        1,
    );
    let expected = with_divergence(
        &expected,
        "1\n0\n f o o \\n\n f o o \\r \\n\n a \\r b \\n\n b a r \\n\n== end\n",
        "0\n0\n f o o \\n\n f o o \\r \\n\n a \\r b \\n\n o o \\n b a r \\n\n== end\n",
        1,
    );
    // cash's own version line, for `--version` and `-V`.
    let expected = with_divergence(
        &expected,
        "grep (GNU grep) 3.12-modified\ngrep (GNU grep) 3.12-modified\n",
        "grep (cash): GNU grep 3.12's options, on ripgrep's engine\n\
         grep (cash): GNU grep 3.12's options, on ripgrep's engine\n",
        1,
    );
    // A repetition of an interval, `a\{1\}*`: refused as the bundled sed refuses it,
    // since grep's patterns are sed's; GNU grep reads it as `\(a\{1\}\)*`.
    let expected = with_divergence(
        &expected,
        "rc=0\naa\naaa\na\naXb\n",
        "rc=0\naa\naaa\ngrep: Invalid preceding regular expression\naXb\n",
        1,
    );
    assert_eq!(run_oracle_script("grep_cases"), expected);
}

#[test]
fn a_crlf_line_matches_without_its_cr_and_is_printed_as_read() {
    let dir = Scratch::new("grep-crlf");
    std::fs::write(dir.join("win.txt"), "alpha\r\nbeta\r\ngamma\r\n").expect("write");
    let out = run_in(dir.path(), "grep 'a$' win.txt | od -An -c");
    assert_eq!(
        od_words(&out.stdout),
        [
            "a", "l", "p", "h", "a", "\\r", "\\n", "b", "e", "t", "a", "\\r", "\\n", "g", "a", "m",
            "m", "a", "\\r", "\\n"
        ]
    );
    let out = run_in(dir.path(), "grep -o 'a$' win.txt | od -An -c");
    assert_eq!(od_words(&out.stdout), ["a", "\\n", "a", "\\n", "a", "\\n"]);
    let out = run_in(dir.path(), "grep -x beta win.txt; echo rc=$?");
    assert_eq!(out.stdout.replace("\r\n", "\n"), "beta\nrc=0");
    // `-b` counts the file's own bytes, `\r` included.
    let out = run_in(dir.path(), "grep -b gamma win.txt");
    assert_eq!(out.stdout.replace("\r\n", "\n"), "13:gamma");
    // `-U` keeps the `\r` in the line, as GNU grep on Linux has it.
    let out = run_in(dir.path(), "grep -U -c 'a$' win.txt");
    assert_eq!(out.stdout, "0");
    // A pattern file may end its lines in CRLF too.
    std::fs::write(dir.join("pats.txt"), "beta\r\ngamma\r\n").expect("write");
    let out = run_in(dir.path(), "grep -c -f pats.txt win.txt");
    assert_eq!(out.stdout, "2");
}

#[test]
fn perl_syntax_is_refused_by_name() {
    let out = run("echo foo | grep -P 'fo+'; echo rc=$?");
    assert_eq!(out.stdout, "rc=2");
    assert_eq!(out.stderr, "grep: -P is not supported; use -E");
    let out = run("echo foo | grep --perl-regexp foo; echo rc=$?");
    assert_eq!(out.stdout, "rc=2");
}

#[test]
fn egrep_and_fgrep_are_grep_with_a_syntax_preset() {
    let out = run("printf 'fo+\\nfoo\\n' | egrep 'fo+'");
    assert_eq!(out.stdout, "fo+\nfoo");
    let out = run("printf 'fo+\\nfoo\\n' | fgrep 'fo+'");
    assert_eq!(out.stdout, "fo+");
    // GNU's obsolescence warning is not printed.
    assert_eq!(out.stderr, "");
    let out = run("echo a | egrep -F a; echo rc=$?");
    assert_eq!(out.stderr, "grep: conflicting matchers specified");
    assert_eq!(out.stdout, "rc=2");
    let out = run("echo a | egrep -E a && echo a | fgrep -F a");
    assert_eq!(out.stdout, "a\na");
    let out = run("egrep --version; fgrep -V");
    assert_eq!(
        out.stdout,
        "grep (cash): GNU grep 3.12's options, on ripgrep's engine\n\
         grep (cash): GNU grep 3.12's options, on ripgrep's engine"
    );
    let out = run("type -t grep egrep fgrep");
    assert_eq!(out.stdout, "builtin\nbuiltin\nbuiltin");
}

#[test]
fn color_auto_colours_only_a_terminal_and_help_speaks_to_the_user() {
    let out = run("echo foo | grep --color=auto foo | od -An -c");
    assert_eq!(od_words(&out.stdout), ["f", "o", "o", "\\n"]);
    let out = run("help grep");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let text = &out.stdout;
    assert!(text.starts_with("NAME\n    grep - "), "{text}");
    assert!(text.contains("\nWINDOWS NOTES\n"), "{text}");
    assert!(text.contains("CRLF"), "{text}");
    // The options are GNU grep's own text, which the page ends with.
    assert!(text.contains("--include=GLOB"), "{text}");
    for developer_note in ["D20", "D76", "(row", "§", "spec.md", "ROADMAP"] {
        assert!(!text.contains(developer_note), "{developer_note} in {text}");
    }
    let out = run("grep --help | grep -c -- -P; grep --help | grep -c 'Report bugs'");
    assert_eq!(out.stdout, "0\n0");
}

#[test]
fn a_substitution_file_and_a_windows_path_are_searched() {
    let dir = Scratch::new("grep-paths");
    std::fs::create_dir_all(dir.join("src").join("deep")).expect("mkdir");
    std::fs::write(dir.join("src").join("a.rs"), "fn main() {}\n").expect("write");
    std::fs::write(dir.join("src").join("deep").join("b.rs"), "fn other() {}\n").expect("write");
    let out = run_in(dir.path(), "grep -c fn <(printf 'fn\\nfn\\n')");
    assert_eq!(out.stdout, "2");
    // A start spelled with backslashes is printed with slashes, as every path cash
    // prints; a start of `.` keeps its `./`, as GNU grep has it.
    let out = run_in(dir.path(), r"grep -r 'fn ' 'src\deep'; grep -rl fn .");
    assert_eq!(
        out.stdout,
        "src/deep/b.rs:fn other() {}\n./src/a.rs\n./src/deep/b.rs"
    );
    let out = run_in(dir.path(), "cd src && grep -rn other");
    assert_eq!(out.stdout, "deep/b.rs:1:fn other() {}");
}
