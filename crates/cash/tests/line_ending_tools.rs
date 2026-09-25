//! `dos2unix` and `unix2dos` — **D20**, **D48**, ROADMAP §7.
//!
//! A clean Windows machine has neither tool, and Git for Windows only supplies them when
//! its `usr/bin` is on PATH. cash carries both, following the dos2unix 7.x manual: in
//! place by default, `-n` pairs, standard input to standard output, `-k`, binary files
//! skipped unless forced, and each tool's own BOM default. What is not carried (code
//! pages, UTF-16, Mac line breaks) is refused by name, and `enable -n` gives the name
//! back to PATH.

#![cfg(windows)]
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

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// A PATH with nothing but Windows itself on it: no Git for Windows `dos2unix.exe`.
const BARE_PATH: &str = r"C:\WINDOWS\system32;C:\WINDOWS";

struct Output {
    stdout: Vec<u8>,
    stderr: String,
    code: i32,
}

impl Output {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).trim_end().to_string()
    }
}

/// Runs `script` in `dir`, feeding `stdin`, and keeps stdout byte-exact.
fn cash_in(dir: &Path, script: &str, stdin: &[u8]) -> Output {
    let mut child = Command::new(CASH)
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

fn cash_with_path(script: &str, path: &str) -> Output {
    let out = Command::new(CASH)
        .args(["-c", script])
        .env("PATH", path)
        .output()
        .expect("failed to run cash");
    Output {
        stdout: out.stdout,
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// A fresh scratch directory, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cash-d2u-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Self(dir)
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.0.join(name), bytes).expect("write fixture");
    }

    fn read(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.0.join(name)).expect("read result")
    }

    fn run(&self, script: &str) -> Output {
        cash_in(&self.0, script, b"")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

#[test]
fn dos2unix_converts_in_place_byte_exact() {
    let s = Scratch::new("in-place-d2u");
    s.write("f.txt", b"one\r\ntwo\r\nlone\rcr\r\r\nlast");
    let out = s.run("dos2unix f.txt; echo rc=$?");
    assert_eq!(out.text(), "rc=0", "stderr: {}", out.stderr);
    assert_eq!(s.read("f.txt"), b"one\ntwo\nlone\rcr\r\nlast");
    assert_eq!(
        out.stderr, "dos2unix: converting file f.txt to Unix format...",
        "the real tool's message"
    );
    // No temporary file is left behind.
    let names: Vec<_> = std::fs::read_dir(&s.0).unwrap().collect();
    assert_eq!(names.len(), 1, "leftover files: {names:?}");
}

#[test]
fn unix2dos_converts_in_place_byte_exact() {
    let s = Scratch::new("in-place-u2d");
    s.write("f.txt", b"one\ntwo\nthree");
    let out = s.run("unix2dos f.txt; echo rc=$?");
    assert_eq!(out.text(), "rc=0", "stderr: {}", out.stderr);
    assert_eq!(s.read("f.txt"), b"one\r\ntwo\r\nthree");
    assert_eq!(
        out.stderr,
        "unix2dos: converting file f.txt to DOS format..."
    );
}

#[test]
fn unix2dos_does_not_double_an_existing_cr() {
    let s = Scratch::new("no-double-cr");
    s.write("f.txt", b"crlf\r\nlf\ncrcrlf\r\r\n");
    s.run("unix2dos f.txt");
    assert_eq!(s.read("f.txt"), b"crlf\r\nlf\r\ncrcrlf\r\r\n");
    // And converting twice is idempotent.
    s.run("unix2dos f.txt");
    assert_eq!(s.read("f.txt"), b"crlf\r\nlf\r\ncrcrlf\r\r\n");
}

#[test]
fn both_tools_filter_standard_input() {
    let s = Scratch::new("stdin");
    let out = cash_in(&s.0, "dos2unix", b"a\r\nb\r\n");
    assert_eq!(out.stdout, b"a\nb\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.is_empty(),
        "stdin mode is silent: {}",
        out.stderr
    );

    let out = cash_in(&s.0, "unix2dos", b"a\nb\n");
    assert_eq!(out.stdout, b"a\r\nb\r\n");

    let out = cash_in(&s.0, "printf 'x\\r\\n' | dos2unix - | od -An -c", b"");
    assert_eq!(
        out.text().split_whitespace().collect::<Vec<_>>(),
        ["x", "\\n"]
    );
}

#[test]
fn new_file_mode_takes_pairs_and_leaves_the_input_alone() {
    let s = Scratch::new("new-file");
    s.write("a.txt", b"a\r\n");
    s.write("b.txt", b"b\r\n");
    s.write("c.txt", b"c\r\n");
    let out = s.run("dos2unix -n a.txt a.out b.txt b.out -o c.txt; echo rc=$?");
    assert_eq!(out.text(), "rc=0", "stderr: {}", out.stderr);
    assert_eq!(s.read("a.txt"), b"a\r\n", "the input was modified");
    assert_eq!(s.read("a.out"), b"a\n");
    assert_eq!(s.read("b.out"), b"b\n");
    assert_eq!(s.read("c.txt"), b"c\n", "-o switches back to in-place");
    assert!(
        out.stderr
            .contains("converting file a.txt to file a.out in Unix format..."),
        "{}",
        out.stderr
    );

    let unpaired = s.run("dos2unix -n a.txt; echo rc=$?");
    assert_eq!(unpaired.text(), "rc=1");
    assert!(unpaired.stderr.contains("not specified in new-file mode"));
}

#[test]
fn keepdate_keeps_the_modification_time() {
    let s = Scratch::new("keepdate");
    s.write("k.txt", b"k\r\n");
    s.write("n.txt", b"n\r\n");
    let old = SystemTime::now() - Duration::from_secs(3 * 24 * 3600);
    for name in ["k.txt", "n.txt"] {
        std::fs::File::options()
            .write(true)
            .open(s.0.join(name))
            .unwrap()
            .set_modified(old)
            .unwrap();
    }
    let out = s.run("dos2unix -k k.txt && dos2unix n.txt");
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let mtime = |name: &str| {
        std::fs::metadata(s.0.join(name))
            .unwrap()
            .modified()
            .unwrap()
    };
    assert_eq!(s.read("k.txt"), b"k\n");
    assert_eq!(mtime("k.txt"), old, "-k did not keep the date");
    assert!(mtime("n.txt") > old, "without -k the date should move");
}

#[test]
fn binary_files_are_skipped_unless_forced() {
    let s = Scratch::new("binary");
    s.write("bin", b"a\r\n\0\r\n");
    let out = s.run("dos2unix bin; echo rc=$?");
    assert_eq!(out.text(), "rc=0", "the manual: skipping is not an error");
    assert_eq!(s.read("bin"), b"a\r\n\0\r\n", "a binary file was changed");
    assert_eq!(
        out.stderr,
        "dos2unix: Binary symbol 0x00 found at line 2\ndos2unix: Skipping binary file bin"
    );

    let strict = s.run("dos2unix --error-binary bin; echo rc=$?");
    assert_eq!(strict.text(), "rc=1");

    let forced = s.run("dos2unix -f bin; echo rc=$?");
    assert_eq!(forced.text(), "rc=0", "stderr: {}", forced.stderr);
    assert_eq!(s.read("bin"), b"a\n\0\n");
}

#[test]
fn bom_defaults_follow_each_tool() {
    let s = Scratch::new("bom");
    // dos2unix removes a BOM by default and keeps it with -b.
    s.write("d1", b"\xEF\xBB\xBFx\r\n");
    s.write("d2", b"\xEF\xBB\xBFx\r\n");
    s.run("dos2unix d1; dos2unix -b d2");
    assert_eq!(s.read("d1"), b"x\n");
    assert_eq!(s.read("d2"), b"\xEF\xBB\xBFx\n");

    // unix2dos keeps a BOM by default, adds one with -m and removes it with -r.
    s.write("u1", b"\xEF\xBB\xBFx\n");
    s.write("u2", b"x\n");
    s.write("u3", b"\xEF\xBB\xBFx\n");
    s.run("unix2dos u1; unix2dos -m u2; unix2dos -r u3");
    assert_eq!(s.read("u1"), b"\xEF\xBB\xBFx\r\n");
    assert_eq!(s.read("u2"), b"\xEF\xBB\xBFx\r\n");
    assert_eq!(s.read("u3"), b"x\r\n");
}

#[test]
fn a_missing_file_fails_like_the_real_tool() {
    let s = Scratch::new("missing");
    s.write("ok", b"o\r\n");
    let out = s.run("dos2unix nope ok; echo rc=$?");
    assert_eq!(out.text(), "rc=2", "ENOENT, as the real tool returns");
    assert!(
        out.stderr
            .contains("dos2unix: nope: No such file or directory")
    );
    assert_eq!(s.read("ok"), b"o\n", "the other file was still converted");

    let quiet = s.run("dos2unix -q nope; echo rc=$?");
    assert_eq!(quiet.text(), "rc=0", "quiet mode returns zero");
    assert!(quiet.stderr.is_empty(), "{}", quiet.stderr);
}

#[test]
fn info_reports_line_break_counts() {
    let s = Scratch::new("info");
    s.write("i1", b"\xEF\xBB\xBFa\r\nb\nc\rd");
    s.write("u", b"a\n");
    let out = s.run("dos2unix -i i1 u");
    assert_eq!(
        out.text(),
        "       1       1       1  UTF-8     text    i1\n       0       1       0  no_bom    text    u"
    );
    let convert = s.run("dos2unix -ic i1 u; unix2dos -ic i1 u");
    assert_eq!(convert.text(), "i1\ni1\nu");
    assert_eq!(
        s.read("i1"),
        b"\xEF\xBB\xBFa\r\nb\nc\rd",
        "-i changed a file"
    );
}

// ---------------------------------------------------------------------------
// Refused by name
// ---------------------------------------------------------------------------

#[test]
fn unsupported_options_are_refused_by_name() {
    let s = Scratch::new("refused");
    s.write("f", b"f\r\n");
    for option in ["-iso", "-437", "-ul", "-c mac", "--allow-chown"] {
        let out = s.run(&format!("dos2unix {option} f; echo rc=$?"));
        assert_eq!(out.text(), "rc=1", "{option} was accepted: {}", out.stderr);
        assert!(
            out.stderr.contains("not supported") && out.stderr.contains("enable -n dos2unix"),
            "{option}: {}",
            out.stderr
        );
        assert_eq!(s.read("f"), b"f\r\n", "{option} converted the file anyway");
    }
    let out = s.run("unix2dos -c mac f; echo rc=$?");
    assert_eq!(out.text(), "rc=1");

    let unknown = s.run("dos2unix --bogus f; echo rc=$?");
    assert_eq!(unknown.text(), "rc=1");
    assert!(
        unknown.stderr.contains("unrecognized option"),
        "{}",
        unknown.stderr
    );
}

#[test]
fn utf16_input_is_refused_not_mangled() {
    let s = Scratch::new("utf16");
    s.write("w", b"\xFF\xFEa\0\r\0\n\0");
    let out = s.run("dos2unix -f w; echo rc=$?");
    assert_eq!(out.text(), "rc=1");
    assert!(out.stderr.contains("UTF-16"), "{}", out.stderr);
    assert_eq!(s.read("w"), b"\xFF\xFEa\0\r\0\n\0");
}

// ---------------------------------------------------------------------------
// Resolution honesty (D8, D48)
// ---------------------------------------------------------------------------

#[test]
fn type_reports_the_builtins() {
    for name in ["dos2unix", "unix2dos"] {
        let out = cash_with_path(&format!("type {name}"), BARE_PATH);
        assert_eq!(
            out.text(),
            format!("{name} is a shell builtin"),
            "{}",
            out.stderr
        );
    }
}

#[test]
fn enable_n_gives_the_name_back_to_path() {
    // On a bare PATH nothing else answers, so the name is honestly not found.
    let out = cash_with_path(
        "enable -n dos2unix; type dos2unix; echo rc=$?; enable -n unix2dos; type -t unix2dos; echo rc=$?",
        BARE_PATH,
    );
    assert!(
        !out.text().contains("builtin"),
        "still a builtin: {}",
        out.text()
    );
    assert_eq!(
        out.text()
            .lines()
            .filter(|l| l.starts_with("rc="))
            .collect::<Vec<_>>(),
        ["rc=1", "rc=1"],
        "stdout: {} stderr: {}",
        out.text(),
        out.stderr
    );

    // With the inherited PATH it is whatever PATH has (Git for Windows' copy, or nothing),
    // and never the builtin.
    let path = std::env::var("PATH").unwrap_or_default();
    let out = cash_with_path("enable -n dos2unix; type -t dos2unix", &path);
    assert!(
        out.text().is_empty() || out.text() == "file",
        "unexpected resolution: {}",
        out.text()
    );
}
