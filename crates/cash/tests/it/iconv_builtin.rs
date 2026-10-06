//! `iconv`: glibc's options on Windows' code pages.
//!
//! The oracle script in `tests/oracle` ran under glibc 2.44's `iconv` to make the `.out`
//! file, and runs here under cash. Where cash differs on purpose, the expected text is
//! replaced in the test with the reason beside it, so a difference cannot hide in the
//! golden file. The rest drives the builtin with files, standard input, `-o` and a
//! process substitution.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly"
)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::common::{Scratch, cash_command};

fn oracle_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("oracle")
}

/// Runs an oracle script under cash; standard output and error together, as the golden
/// files were made.
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

struct Output {
    stdout: Vec<u8>,
    stderr: String,
    code: i32,
}

/// Runs `script` in `dir`, feeding `stdin`, and keeps stdout byte-exact.
fn cash_in(dir: &Path, script: &str, stdin: &[u8]) -> Output {
    let mut child = cash_command()
        .args(["-c", script])
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run cash");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin)
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait for cash");
    Output {
        stdout: out.stdout,
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

#[test]
fn iconv_matches_glibc() {
    let expected = golden("iconv_cases");
    // `//IGNORE` without `-c` is silent, as `-c` is. glibc also reports an illegal
    // sequence at the end of the input, a quirk of its own iconv() under //IGNORE.
    let expected = with_divergence(
        &expected,
        "== //IGNORE\niconv: illegal input sequence at position 7\nrc=1\n",
        "== //IGNORE\nrc=1\n",
    );
    // `//TRANSLIT` is Windows' best fit and `?`: `€` to ASCII is `?`, not glibc's `EUR`.
    // `é` to `e` and `“` to `"` are the same in both.
    let expected = with_divergence(
        &expected,
        " 61 62 45 55 52 63 65 64 22\n",
        " 61 62 3f 63 65 64 22\n",
    );
    let expected = with_divergence(
        &expected,
        "rc=1\n 61 62 45 55 52 63 64\n",
        "rc=1\n 61 62 3f 63 64\n",
    );
    // `-c` exits 1 whenever it dropped something. glibc exits 0 for a CP932 lead byte
    // with a bad trail byte, and 1 for a bad UTF-8 byte: its SJIS module does not count
    // the skip, which is an inconsistency, not a rule.
    let expected = with_divergence(
        &expected,
        "rc=0\n 61 62 20 63 64\n",
        "rc=1\n 61 62 20 63 64\n",
    );
    assert_eq!(run_oracle_script("iconv_cases"), expected);
}

#[test]
fn converts_a_file_into_another_with_o() {
    let s = Scratch::new("iconv-o");
    std::fs::write(s.join("in.txt"), b"caf\xE9 \x80\r\n").unwrap();
    let out = cash_in(
        s.path(),
        "iconv -f CP1252 -t UTF-8 -o out.txt in.txt; echo rc=$?",
        b"",
    );
    assert_eq!(out.stdout, b"rc=0\n", "stderr: {}", out.stderr);
    assert_eq!(
        std::fs::read(s.join("out.txt")).unwrap(),
        "café €\r\n".as_bytes(),
        "CP1252 decoded, CRLF untouched"
    );
}

#[test]
fn reads_standard_input_and_writes_bytes_exactly() {
    let s = Scratch::new("iconv-stdin");
    let out = cash_in(s.path(), "iconv -f UTF-8 -t UTF-16", "a\u{e9}\n".as_bytes());
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, b"\xFF\xFEa\0\xE9\0\n\0");

    // A UTF-16 file with a mark: the mark picks the order and does not come out.
    std::fs::write(s.join("wide.txt"), b"\xFF\xFEh\0i\0\n\0").unwrap();
    let out = cash_in(s.path(), "iconv -f UTF-16 -t UTF-8 wide.txt", b"");
    assert_eq!(out.stdout, b"hi\n");
}

#[test]
fn a_missing_file_is_reported_and_the_rest_converted() {
    let s = Scratch::new("iconv-missing");
    std::fs::write(s.join("a.txt"), b"abc").unwrap();
    let out = cash_in(
        s.path(),
        "iconv -f UTF-8 -t LATIN1 nosuch.txt a.txt; echo \" rc=$?\"",
        b"",
    );
    assert_eq!(out.stdout, b"abc rc=1\n");
    assert_eq!(
        out.stderr,
        "iconv: cannot open input file `nosuch.txt': No such file or directory"
    );
}

#[test]
fn an_illegal_byte_names_its_position_and_keeps_the_output_so_far() {
    let s = Scratch::new("iconv-illegal");
    std::fs::write(s.join("bad.txt"), b"ok\n\xFFrest\n").unwrap();
    let out = cash_in(
        s.path(),
        "iconv -f UTF-8 -t CP1252 bad.txt; echo \"rc=$?\"",
        b"",
    );
    assert_eq!(out.stdout, b"ok\nrc=1\n");
    assert_eq!(out.stderr, "iconv: illegal input sequence at position 3");

    let out = cash_in(
        s.path(),
        "iconv -c -f UTF-8 -t CP1252 bad.txt; echo \"rc=$?\"",
        b"",
    );
    assert_eq!(
        out.stdout, b"ok\nrest\nrc=1\n",
        "-c drops it and still exits 1"
    );
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
}

#[test]
fn a_process_substitution_is_a_file() {
    let s = Scratch::new("iconv-procsub");
    let out = cash_in(
        s.path(),
        r"iconv -f LATIN1 -t UTF-8 <(printf 'caf\351')",
        b"",
    );
    assert_eq!(out.stdout, "café".as_bytes(), "stderr: {}", out.stderr);
}

#[test]
fn help_and_version_are_the_builtins_own() {
    let s = Scratch::new("iconv-help");
    let out = cash_in(s.path(), "iconv --version", b"");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "iconv (cash): glibc 2.44's options, on the Windows code pages"
    );
    let out = cash_in(s.path(), "iconv --help; help iconv", b"");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("--from-code=NAME"), "{text}");
    assert!(text.contains("Windows' own code pages"), "{text}");
    for developer_note in ["D7", "ROADMAP", "spec.md", "§"] {
        assert!(
            !text.contains(developer_note),
            "{developer_note} in: {text}"
        );
    }
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
}
