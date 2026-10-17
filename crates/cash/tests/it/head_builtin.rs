//! `head`, taken from uutils into `crates/cash-uutils`: where it leaves a file under its
//! standard input, as GNU head leaves it.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::run_in;

#[test]
fn head_leaves_a_file_under_its_input_just_after_what_it_printed() {
    // `{ head -n1; cat; } < file` printed the first line and then what was left after
    // the 8 KiB head had read ahead. A head that comes to the file under way, as the
    // second one here, reads what is left of it, also where it reads the end first.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("small"), "one\ntwo\nthree\nfour\n").unwrap();
    // 108894 bytes: more than head reads in one go, so `-n -1` and `-c -3` seek.
    let big: String = (1..=20_000).map(|n| format!("{n}\n")).collect();
    std::fs::write(dir.path().join("big"), big).unwrap();
    for (script, want) in [
        ("{ head -n1; cat; } < small", "one\ntwo\nthree\nfour"),
        (
            "{ head -c2 >/dev/null; cat; } < small",
            "e\ntwo\nthree\nfour",
        ),
        ("{ head -n0; cat; } < small", "one\ntwo\nthree\nfour"),
        ("{ head -n-1 >/dev/null; cat; } < small", "four"),
        ("{ head -c-3 >/dev/null; cat; } < small", "ur"),
        ("{ head -n1 >/dev/null; cat | wc -c; } < big", "108892"),
        (
            "{ head -n1 >/dev/null; head -n-1 | wc -c; cat; } < big",
            "108886\n20000",
        ),
        (
            "{ head -c2 >/dev/null; head -c-3 | wc -c; cat; } < big",
            "108889\n00",
        ),
    ] {
        let out = run_in(dir.path(), script);
        assert_eq!(
            (out.stdout.as_str(), out.code),
            (want, 0),
            "{script}: {}",
            out.stderr
        );
    }
}
