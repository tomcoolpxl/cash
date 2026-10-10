//! `hexdump`, util-linux's: the golden output of `tests/oracle/hexdump_cases.sh`, and
//! what the oracle cannot show (Windows paths).
//!
//! The script ran under util-linux 2.42.3 to make `hexdump_cases.out`; here it runs under
//! cash. Where cash differs on purpose, the expected text is replaced in the test with
//! the reason beside it, so a difference cannot hide in the golden file.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, golden, run, run_oracle_script, with_divergence};

#[test]
fn hexdump_matches_util_linux() {
    let expected = with_divergence(
        &golden("hexdump_cases"),
        // `-s` on a pipe skips the bytes; util-linux cannot seek a pipe.
        "== -s on stdin\nhexdump: stdin: Illegal seek\nrc=1\n",
        "== -s on stdin\n00000003  6c 6f 20 77 6f 72 6c 64  21 21                    |lo world!!|\n\
         0000000d\nrc=0\n",
        1,
    );
    let expected = with_divergence(
        &expected,
        "== -V\nhexdump from util-linux 2.42.3\nrc=0\n== --version\nhexdump from util-linux 2.42.3\n",
        "== -V\nhexdump (cash): util-linux 2.42.3's options\nrc=0\n\
         == --version\nhexdump (cash): util-linux 2.42.3's options\n",
        1,
    );
    assert_eq!(run_oracle_script("hexdump_cases"), expected);
}

#[test]
fn hexdump_is_a_builtin() {
    let out = run("type hexdump");
    assert!(out.stdout.contains("shell builtin"), "{}", out.stdout);
}

#[test]
fn hexdump_reads_windows_paths_and_format_files() {
    let scratch = Scratch::new("hexdump-paths");
    std::fs::write(scratch.join("in.bin"), b"hi\n").expect("write the input");
    std::fs::write(
        scratch.join("fmt.txt"),
        "\"%_ad: \" 4/1 \"%02x \" \"\\n\"\r\n",
    )
    .expect("write the format file");
    let windows = scratch.join("in.bin").display().to_string();
    let out = run(&format!("hexdump -C '{windows}'"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "00000000  68 69 0a                                          |hi.|\n00000003"
    );
    let unix = scratch.as_script_path();
    let out = run(&format!("cd '{unix}' && hexdump -f fmt.txt in.bin"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "0: 68 69 0a");
}

#[test]
fn hexdump_refuses_colour_and_says_so() {
    let out = run("printf x | hexdump -L=always -C");
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        "hexdump: colour output (-L=always) is not supported; formats are printed plain"
    );
    assert_eq!(out.stdout, "");
}

#[test]
fn hexdump_help_is_a_page_without_developer_notes() {
    let out = run("help hexdump");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let text = &out.stdout;
    assert!(text.starts_with("NAME\n    hexdump - "), "{text}");
    assert!(text.contains("%_p"), "{text}");
    for developer_note in ["D7", "(row", "§", "spec.md", "ROADMAP"] {
        assert!(!text.contains(developer_note), "{developer_note} in {text}");
    }
    // util-linux's help starts with an empty line, and so does this one.
    let usage = run("hexdump --help");
    assert_eq!(usage.code, 0);
    assert!(
        usage
            .stdout
            .starts_with("\nUsage:\n hexdump [options] <file>..."),
        "{}",
        usage.stdout
    );
}
