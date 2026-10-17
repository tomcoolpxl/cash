//! `tail`, taken from uutils into `crates/cash-uutils`: the end of a pipe, of a file
//! of whole 64 KiB blocks, and of a file followed by polling, each as GNU tail gives it.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::run_in;

/// 16-byte lines, `lines` of them.
fn lines_text(lines: u32) -> String {
    (0..lines)
        .map(|n| format!("line {n:010}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn tail_gives_the_end_of_a_pipe_and_of_a_file_of_whole_blocks() {
    // `head -n -1 big | tail -c 8` printed 2,000 to 10,000 bytes: the pipe's reads come
    // in mixed sizes, and tail kept chunks it no longer needed. A file of 64 KiB gave
    // nothing for `tail -n 2`, and one of 128 KiB lost its first 64 KiB.
    let dir = tempfile::tempdir().unwrap();
    for (name, lines) in [("big", 20_000), ("b64", 4096), ("b128", 8192)] {
        std::fs::write(dir.path().join(name), lines_text(lines)).unwrap();
    }
    for (script, want) in [
        ("head -n -1 big | tail -c 11", "0000019998"),
        ("cat big | tail -c 12 | wc -c", "12"),
        (
            "head -n -1 big | tail -n 2",
            "line 0000019997\nline 0000019998",
        ),
        ("tail -n 2 b64", "line 0000004094\nline 0000004095"),
        ("tail -c 5 b64", "4095"),
        ("tail -n 4097 b64 | wc -l", "4096"),
        ("tail -n 5000 b128 | wc -l", "5000"),
        ("tail -n 8192 b128 | head -n 1", "line 0000000000"),
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

#[test]
fn tail_follows_a_growing_file_by_polling_without_printing_it_again() {
    // With polling, every change read as the file replaced: "tail: 'f' has been
    // replaced;  following new file", and the file printed again from its start.
    // Each tail ends when the `sleep` it watches does.
    let dir = tempfile::tempdir().unwrap();
    for options in ["-f", "-F --use-polling", "-f --use-polling"] {
        let script = format!(
            "printf 'a\\n' > f; sleep 3 & p=$!; \
             tail {options} -s 0.2 --pid=$p f > out 2> err & t=$!; \
             sleep 1; echo b >> f; wait $t; cat out; cat err"
        );
        let out = run_in(dir.path(), &script);
        assert_eq!(
            (out.stdout.as_str(), out.code),
            ("a\nb", 0),
            "{options}: {}",
            out.stderr
        );
    }
}

#[test]
fn tail_follow_ends_once_the_reader_of_its_output_has_gone() {
    // GNU's tail asks between polls whether its output still has a reader, and ends as
    // SIGPIPE ends it; this one heard of it only at its next write, when the file grew,
    // and `tail -f f | head -1` ran until then, here for ever. A reader that stays keeps
    // it following: this tail ends when the `sleep` it watches does.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("f"), "a\nb\n").unwrap();
    for (script, want) in [
        (
            r#"tail -f -s 0.2 f | head -1; echo "${PIPESTATUS[*]}""#,
            "a\n141 0",
        ),
        (
            r#"sleep 2 & p=$!; tail -f -s 0.2 --pid=$p f | cat; echo "${PIPESTATUS[*]}""#,
            "a\nb\n0 0",
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
