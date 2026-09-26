//! What cash changed in `bc` on the way in from posixutils-rs (spec D56); the upstream
//! suite itself runs in `bc.rs`.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction"
)]

use std::io::Write as _;
use std::process::{Command, Stdio};

const CASH: &str = env!("CARGO_BIN_EXE_cash");

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

/// `cash -c SCRIPT` with `input` on standard input.
fn cash(script: &str, input: &str) -> Output {
    let mut child = Command::new(CASH)
        .args(["-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run cash");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait for cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

#[test]
fn bc_is_a_builtin_and_does_arithmetic() {
    let out = cash(
        r#"type bc; echo 'scale=5; 1/3; 2^100' | bc; x=$(echo '3*7' | bc); echo "x=$x""#,
        "",
    );
    assert_eq!(
        out.stdout, "bc is a shell builtin\n.33333\n1267650600228229401496703205376\nx=21\n",
        "{}",
        out.stderr
    );
}

#[test]
fn crlf_input_is_read_as_lf() {
    // D20: a script saved with Windows line endings, piped or as a file.
    let piped = cash("bc", "1+1\r\nscale=2\r\n1/3\r\n");
    assert_eq!(piped.stdout, "2\n.33\n", "{}", piped.stderr);
    assert_eq!(piped.code, 0);

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("crlf.bc");
    std::fs::write(&file, "define f(x) {\r\n  return (x * 2)\r\n}\r\nf(21)\r\n").unwrap();
    let spelled = file.to_string_lossy().replace('\\', "/");
    let from_file = cash(&format!("bc '{spelled}' < /dev/null"), "");
    assert_eq!(from_file.stdout, "42\n", "{}", from_file.stderr);
}

#[test]
fn a_lone_cr_is_still_an_error() {
    let out = cash("bc", "1+\r1\n");
    assert!(out.stderr.contains("illegal character"), "{}", out.stderr);
    assert_eq!(out.code, 1);
}

#[test]
fn gnu_options_scripts_pass_are_accepted() {
    let out = cash(
        "echo 1+1 | bc -q; echo 1+1 | bc -s; echo 1+1 | bc -w; echo 's(0)' | bc --mathlib; \
         echo 'l(1)' | bc -ql",
        "",
    );
    assert_eq!(out.stdout, "2\n2\n2\n0\n0\n", "{}", out.stderr);
}

#[test]
fn a_gnu_extension_is_named_when_it_fails() {
    for (program, named) in [
        ("print \"x\"\n", "`print`"),
        ("x = read()\n", "`read`"),
        ("if (1) 1 else 2\n", "`else`"),
        ("1 && 1\n", "`&&`"),
        ("0 || 1\n", "`||`"),
        ("!0\n", "`!`"),
        ("1 # a comment\n", "`#` comments"),
        ("total = 5\n", "`total`"),
        ("5\nlast\n", "`last`"),
        ("halt\n", "`halt`"),
    ] {
        let out = cash("bc", program);
        assert!(
            out.stderr.contains(named) && out.stderr.contains("GNU bc extension"),
            "{program:?}: {}",
            out.stderr
        );
        assert_eq!(out.code, 1, "{program:?}");
    }
}

#[test]
fn an_ordinary_error_gets_no_extension_note() {
    let out = cash("bc", "1 +\n");
    assert!(!out.stderr.contains("GNU"), "{}", out.stderr);
    assert_eq!(out.code, 1);
}
