//! `diff` and `cmp`: uutils diffutils' tools behind GNU diffutils 3.12's interface.
//!
//! Both are checked against GNU diffutils case by case: the scripts in `tests/oracle`
//! ran under the real tools in WSL to make the `.out` files, and run here under cash.
//! Where cash differs on purpose, the expected text is replaced in the test, with the
//! reason beside it, so a difference cannot hide in the golden file. The rest is what
//! only shows under cash: the zone of the header times, the shell's view of the two
//! tools, their help page and doctor's.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::process::Command;
use std::time::{Duration, SystemTime};

use crate::common::{CASH, Scratch, cash_command, isolate, output_of, run, run_in};
use crate::small_tools::{golden, run_oracle_script, with_divergence};

#[test]
fn diff_matches_gnu_diffutils() {
    let expected = with_divergence(
        &golden("diff_cases"),
        // The version line names cash's own tool and what it is made of.
        "diff (GNU diffutils) 3.12\ndiff (GNU diffutils) 3.12\n",
        "diff (cash): GNU diffutils 3.12's options, from uutils diffutils\n\
         diff (cash): GNU diffutils 3.12's options, from uutils diffutils\n",
    );
    assert_eq!(run_oracle_script("diff_cases"), expected);
}

#[test]
fn cmp_matches_gnu_diffutils() {
    let expected = with_divergence(
        &golden("cmp_cases"),
        // The version line names cash's own tool and what it is made of.
        "cmp (GNU diffutils) 3.12\ncmp (GNU diffutils) 3.12\n",
        "cmp (cash): GNU diffutils 3.12's options, from uutils diffutils\n\
         cmp (cash): GNU diffutils 3.12's options, from uutils diffutils\n",
    );
    assert_eq!(run_oracle_script("cmp_cases"), expected);
}

/// The `---`/`+++` headers show the files' times in the zone `TZ` names, as GNU's do,
/// and an absent file (`-N`) the epoch.
#[test]
fn header_times_follow_tz() {
    let dir = Scratch::new("diff-tz");
    std::fs::write(dir.join("a"), "x\n").unwrap();
    std::fs::write(dir.join("b"), "y\n").unwrap();
    // 2024-01-02 03:04:05 UTC.
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(1_704_164_645);
    for name in ["a", "b"] {
        std::fs::File::options()
            .write(true)
            .open(dir.join(name))
            .unwrap()
            .set_modified(when)
            .unwrap();
    }
    let headers = |tz: &str, script: &str| {
        let out = output_of(
            cash_command()
                .args(["-c", script])
                .current_dir(dir.path())
                .env("TZ", tz),
        );
        out.stdout
            .lines()
            .take(2)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        headers("UTC", "diff -u a b"),
        [
            "--- a\t2024-01-02 03:04:05.000000000 +0000",
            "+++ b\t2024-01-02 03:04:05.000000000 +0000"
        ]
    );
    assert_eq!(
        headers("Europe/Brussels", "diff -u a b"),
        [
            "--- a\t2024-01-02 04:04:05.000000000 +0100",
            "+++ b\t2024-01-02 04:04:05.000000000 +0100"
        ]
    );
    assert_eq!(
        headers("JST-9", "diff -c a b"),
        [
            "*** a\t2024-01-02 12:04:05.000000000 +0900",
            "--- b\t2024-01-02 12:04:05.000000000 +0900"
        ]
    );
    assert_eq!(
        headers("UTC", "diff -N -u a nosuch"),
        [
            "--- a\t2024-01-02 03:04:05.000000000 +0000",
            "+++ nosuch\t1970-01-01 00:00:00.000000000 +0000"
        ]
    );
}

/// Both are builtins of the shell, with the bundled path a script can exec (D48).
#[test]
fn diff_and_cmp_are_bundled_builtins() {
    let out = run("type -t diff cmp; command -v diff cmp");
    assert_eq!(out.stdout, "builtin\nbuiltin\ndiff\ncmp", "{}", out.stderr);
    for tool in ["diff", "cmp"] {
        let out = output_of(isolate(Command::new(CASH).args([
            "--invoke-bundled",
            tool,
            "--version",
        ])));
        assert_eq!(
            out.stdout,
            format!("{tool} (cash): GNU diffutils 3.12's options, from uutils diffutils"),
            "{}",
            out.stderr
        );
    }
}

/// A script's `if diff -q`/`cmp -s` sees GNU's exit status, and a pipeline gets the
/// diff's bytes as they are, a CRLF file's carriage returns included (D20).
#[test]
fn exit_status_and_bytes_in_a_script() {
    let dir = Scratch::new("diff-script");
    std::fs::write(dir.join("crlf"), "a\r\nb\r\n").unwrap();
    std::fs::write(dir.join("lf"), "a\nb\n").unwrap();
    std::fs::write(dir.join("lf2"), "a\nb\n").unwrap();
    let out = run_in(
        dir.path(),
        "diff -q crlf lf >/dev/null; echo \"diff $?\"; \
         if cmp -s lf lf2; then echo same; fi; \
         diff --strip-trailing-cr crlf lf; echo \"stripped $?\"; \
         diff crlf lf | od -An -c | head -1 | sed 's/  */ /g'",
    );
    assert_eq!(
        out.stdout, "diff 1\nsame\nstripped 0\n 1 , 2 c 1 , 2 \\n < a \\r \\n < b",
        "{}",
        out.stderr
    );
}

/// `help diff` is a page with the Windows notes and diff's own options; `help cmp`
/// opens the same page.
#[test]
fn help_diff_is_a_page_with_windows_notes_and_its_own_options() {
    let out = run("help diff");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let text = &out.stdout;
    assert!(text.starts_with("NAME\n    diff - "), "{text}");
    assert!(text.contains("\nWINDOWS NOTES\n"), "{text}");
    assert!(text.contains("--strip-trailing-cr"), "{text}");
    assert!(text.contains("help crlf"), "{text}");
    // The options are diff's own `--help`, not a copy.
    assert!(text.contains("\nUSAGE AND OPTIONS\n"), "{text}");
    assert!(text.contains("--report-identical-files"), "{text}");
    // Help speaks to the user: no spec decision numbers, rows or research files
    // (the user, 2026-10-05).
    for developer_note in ["D76", "D48", "(row", "§", "spec.md", "ROADMAP", "research/"] {
        assert!(!text.contains(developer_note), "{developer_note} in {text}");
    }
    let cmp = run("help cmp");
    assert_eq!(cmp.code, 0, "{}", cmp.stderr);
    assert!(cmp.stdout.contains("byte by byte"), "{}", cmp.stdout);
    assert!(cmp.stdout.contains("--ignore-initial"), "{}", cmp.stdout);
}

/// Doctor counts both among the commands cash answers for itself, and no longer asks
/// after a `diff` on PATH.
#[test]
fn doctor_counts_diff_and_cmp_as_carried() {
    let out =
        output_of(isolate(Command::new(CASH).arg("doctor")).env("PATH", r"C:\Windows\System32"));
    assert!(!out.stdout.contains("WARN  diff"), "{}", out.stdout);
    assert!(!out.stdout.contains("diffutils"), "{}", out.stdout);
    assert!(
        out.stdout.contains("  ok    ") && out.stdout.contains(" commands built in"),
        "{}",
        out.stdout
    );
}
