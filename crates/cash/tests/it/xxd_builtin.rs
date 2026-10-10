//! `xxd`, vim's: the golden output of `tests/oracle/xxd_cases.sh`, and what the oracle
//! cannot show (Windows paths, a terminal's colour).
//!
//! The script ran under vim's xxd of 2026-06-16 to make `xxd_cases.out`; here it runs
//! under cash. Where cash differs on purpose, the expected text is replaced in the test
//! with the reason beside it, so a difference cannot hide in the golden file.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, golden, run, run_oracle_script};

#[test]
fn xxd_matches_vims() {
    // No deliberate difference shows in the script: the version lines differ, and the
    // script keeps only their first word; `-E` is refused, which the script does not run.
    assert_eq!(run_oracle_script("xxd_cases"), golden("xxd_cases"));
}

#[test]
fn xxd_is_a_builtin() {
    let out = run("type xxd");
    assert!(out.stdout.contains("shell builtin"), "{}", out.stdout);
}

#[test]
fn xxd_reads_and_writes_windows_paths() {
    let scratch = Scratch::new("xxd-paths");
    std::fs::write(scratch.join("in.bin"), b"hi\n").expect("write the input");
    let windows = scratch.join("in.bin").display().to_string();
    let out = run(&format!("xxd '{windows}'"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "00000000: 6869 0a                                  hi."
    );
    // The output file is resolved the same way, and `-r` patches it in place.
    let unix = scratch.as_script_path();
    let out = run(&format!(
        "cd '{unix}' && printf '00000001: 6f' | xxd -r - in.bin && xxd -p in.bin"
    ));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "686f0a");
    // The C array is named after the path as given, made an identifier.
    let out = run(&format!("cd '{unix}' && xxd -i in.bin | head -1"));
    assert_eq!(out.stdout, "unsigned char in_bin[] = {");
}

#[test]
fn xxd_refuses_ebcdic_and_says_so() {
    let out = run("printf x | xxd -E");
    assert_eq!(out.code, 1);
    assert_eq!(out.stderr, "xxd: -E (EBCDIC) is not supported");
    assert_eq!(out.stdout, "");
}

#[test]
fn xxd_version_names_what_it_follows() {
    let out = run("xxd -v");
    assert_eq!(out.code, 0);
    assert_eq!(out.stderr, "xxd (cash): vim's xxd, 2026-06-16 options");
}

#[test]
fn xxd_help_is_a_page_without_developer_notes() {
    let out = run("help xxd");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let text = &out.stdout;
    assert!(text.starts_with("NAME\n    xxd - "), "{text}");
    assert!(text.contains("-r"), "{text}");
    for developer_note in ["D7", "(row", "§", "spec.md", "ROADMAP"] {
        assert!(!text.contains(developer_note), "{developer_note} in {text}");
    }
    let usage = run("xxd -h");
    assert_eq!(usage.code, 1);
    assert!(
        usage
            .stderr
            .starts_with("Usage:\n       xxd [options] [infile [outfile]]"),
        "{}",
        usage.stderr
    );
}
