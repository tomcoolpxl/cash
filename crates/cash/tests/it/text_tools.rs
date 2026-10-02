//! The bundled awk, sed and bc, run as a script runs them (D48, D49, D56).
//!
//! Their own crates test what each computes; these tests drive them through cash, for
//! what only shows in a pipeline: how a tool ends when its reader goes away, and the
//! order its output takes beside another program's.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly"
)]

use crate::common::run;

#[test]
fn awk_stops_with_a_write_error_when_its_reader_goes() {
    // `print` went through Rust's `print!`, which panics when the pipe is closed:
    // "failed printing to stdout: The pipe is being closed" (REVIEW_REPORT.md TXT-08).
    let out = run(r#"seq 1 200000 | awk '{print}' | head -1; echo "status ${PIPESTATUS[1]}""#);
    assert_eq!(out.stdout, "1\nstatus 2", "{}", out.stderr);
    assert!(!out.stderr.contains("panicked"), "{}", out.stderr);
    assert!(
        out.stderr.contains("awk: write error: Broken pipe"),
        "{}",
        out.stderr
    );
}

#[test]
fn awk_output_comes_before_what_system_prints() {
    // gawk, mawk and BWK awk flush before `system()`; without it, `b` overtook `a`
    // (TXT-14).
    let out = run(r#"awk 'BEGIN { printf "a"; system("echo b"); print "c" }' | cat"#);
    assert_eq!(out.stdout, "ab\nc", "{}", out.stderr);
}

#[test]
fn plain_getline_reads_on_into_the_next_file_as_gawk_does() {
    // Plain `getline` read only from the file the main loop had open, and from nothing in
    // BEGIN (REVIEW_REPORT.md TXT-06). Each expected output is gawk 5.4's.
    let scratch = crate::common::Scratch::new("awk-getline");
    std::fs::write(scratch.path().join("f1.txt"), "a\nb\nc\n").unwrap();
    std::fs::write(scratch.path().join("f2.txt"), "d\ne\n").unwrap();
    std::fs::write(scratch.path().join("empty.txt"), "").unwrap();
    let cases = [
        (
            r#"awk 'NR==1{while((getline l)>0) print "got", l} END{print NR, FNR}' f1.txt f2.txt"#,
            "got b\ngot c\ngot d\ngot e\n5 2",
        ),
        (
            r#"awk 'BEGIN{getline; print $0, NR, FNR, FILENAME}' f2.txt"#,
            "d 1 1 f2.txt",
        ),
        (
            r#"awk 'BEGIN{while ((getline line) > 0) n++; print n, NR}' f1.txt f2.txt"#,
            "5 5",
        ),
        (
            r#"awk '{print FILENAME, FNR, NR, $0; nextfile}' f1.txt f2.txt"#,
            "f1.txt 1 1 a\nf2.txt 1 2 d",
        ),
        (
            r#"awk 'FNR==2{getline; getline} {print FILENAME, FNR, NR, $0}' f1.txt empty.txt f2.txt"#,
            "f1.txt 1 1 a\nf2.txt 1 4 d\nf2.txt 2 5 e",
        ),
        (
            r#"printf 'x\ny\n' | awk 'BEGIN{getline; print "begin", $0} {print "main", $0}'"#,
            "begin x\nmain y",
        ),
    ];
    for (script, expected) in cases {
        let out = crate::common::run_in(scratch.path(), script);
        assert_eq!(out.stdout, expected, "{script}: {}", out.stderr);
    }
}
