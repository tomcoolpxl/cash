//! `gzip`, `gunzip` and `zcat`: the golden output of `tests/oracle/gzip_cases.sh`, and
//! what the oracle cannot show: the console (the refusal to write compressed data to
//! it, the overwrite question), a stream of many chunks, substitution files, the help
//! page, and that nothing is left beside a file when a run fails.
//!
//! The script ran under GNU gzip 1.14 to make `gzip_cases.out`; here it runs under
//! cash. Where cash differs on purpose, the expected text is replaced in the test with
//! the reason beside it, so a difference cannot hide in the golden file.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, golden, run, run_in, run_oracle_script, with_divergence};
use crate::read_console::Script;

#[test]
fn gzip_matches_gnu_gzip() {
    // GNU's gunzip and zcat are scripts that run gzip, and the usage line names what
    // ran them; cash's are the builtin under its own name.
    let expected = with_divergence(
        &golden("gzip_cases"),
        "Usage: /usr/sbin/gunzip [OPTION]... [FILE]...\nUsage: gzip [OPTION]... [FILE]...\n",
        "Usage: gunzip [OPTION]... [FILE]...\nUsage: zcat [OPTION]... [FILE]...\n",
        1,
    );
    // A zip file: GNU gzip reads one of a single member, and says so of a bad one;
    // cash carries deflate in gzip's framing only, and refuses zip by name.
    let expected = with_divergence(
        &expected,
        "\ngzip: pk.gz: not a valid zip file\n",
        "gzip: pk.gz: zip format is not supported by cash's gzip\n",
        1,
    );
    // cash's own version line, under each of the three names.
    let expected = with_divergence(
        &expected,
        "gzip 1.14-modified\ngzip 1.14-modified\ngunzip (gzip) 1.14-modified\n\
         zcat (gzip) 1.14-modified\n",
        "gzip (cash): GNU gzip 1.14's options, in pure Rust\n\
         gzip (cash): GNU gzip 1.14's options, in pure Rust\n\
         gunzip (cash): GNU gzip 1.14's options, in pure Rust\n\
         zcat (cash): GNU gzip 1.14's options, in pure Rust\n",
        1,
    );
    assert_eq!(run_oracle_script("gzip_cases"), expected);
}

#[test]
fn the_three_names_are_builtins_with_one_page() {
    for name in ["gzip", "gunzip", "zcat"] {
        let out = run(&format!("type {name}"));
        assert!(
            out.stdout.contains("shell builtin"),
            "{name}: {}",
            out.stdout
        );
        let page = run(&format!("help {name}"));
        assert_eq!(page.code, 0, "{}", page.stderr);
        assert!(
            page.stdout.contains("miniz_oxide"),
            "{name}: {}",
            page.stdout
        );
        assert!(page.stdout.contains("--best"), "{name}: {}", page.stdout);
    }
}

/// A stream of many chunks, compressed and decompressed through the shell: the
/// buffers are 64 KiB, so a member of several hundred kilobytes crosses them many
/// times, in a pipe and in a file.
#[test]
fn a_large_stream_round_trips_through_pipes_and_files() {
    let scratch = Scratch::new("gzip-large");
    let out = run_in(
        scratch.path(),
        "seq 1 150000 > big; yes 'the quick brown fox' | head -c 400000 >> big; \
         gzip -c big | gzip -dc | cmp - big && echo pipe-same; \
         gzip -k big && gzip -l big.gz | tail -1 | awk '{print $2}'; \
         gunzip -c big.gz | cmp - big && echo file-same; \
         gzip -9c big > best.gz; gzip -1c big > fast.gz; gzip -t best.gz fast.gz && echo levels-ok; \
         zcat best.gz fast.gz | wc -c",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    let size = std::fs::metadata(scratch.join("big"))
        .expect("the file")
        .len();
    assert_eq!(
        out.stdout,
        format!("pipe-same\n{size}\nfile-same\nlevels-ok\n{}", size * 2)
    );
}

#[test]
fn substitution_files_are_read_and_written() {
    let out = run(
        "gzip -c <(printf 'hi\\n') | gzip -dc; zcat <(printf 'hi\\n' | gzip -c); gzip -l <(printf 'x\\n' | gzip -nc) | tail -1 | awk '{print $2}'",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "hi\nhi\n2");
}

/// A run that fails leaves nothing beside the file: the temporary name is removed.
#[test]
fn a_failed_run_leaves_no_temporary_file() {
    let scratch = Scratch::new("gzip-tmp");
    let out = run_in(
        scratch.path(),
        "printf 'hello\\n' | gzip -nc > good.gz; head -c 15 good.gz > bad.gz; \
         gzip -d bad.gz; echo \"rc=$?\"; gzip -t good.gz; echo \"rc=$?\"; ls -a",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stderr.contains("bad.gz: unexpected end of file"),
        "{}",
        out.stderr
    );
    assert_eq!(out.stdout, "rc=1\nrc=0\n.\n..\nbad.gz\ngood.gz");
}

/// At a console, compressed data is not written to it or read from it without `-f`.
#[test]
fn a_console_is_refused_compressed_data_without_f() {
    let left = Script::start(
        "gzip-terminal",
        "gzip; echo \"rc=$?\" > out.txt; gzip -d; echo \"rc=$?\" >> out.txt; \
         printf 'hello\\n' | gzip -f | gzip -dc >> out.txt; echo \"rc=$?\" >> out.txt",
    )
    .finish();
    assert_eq!(left.out, "rc=1\nrc=1\nhello\nrc=0");
    assert!(
        left.screen.contains(
            "gzip: compressed data not written to a terminal. Use -f to force compression.\n\
             For help, type: gzip -h"
        ),
        "{}",
        left.screen
    );
    assert!(
        left.screen.contains(
            "gzip: compressed data not read from a terminal. Use -f to force decompression."
        ),
        "{}",
        left.screen
    );
}

/// At a console, a file that exists is asked about, and `y` overwrites it.
#[test]
fn the_overwrite_question_is_asked_at_a_console() {
    let left = Script::start(
        "gzip-overwrite-yes",
        "printf 'hello\\n' > x; printf 'old\\n' > x.gz; gzip x; echo \"rc=$?\" > out.txt; \
         ls x* >> out.txt; gzip -dc x.gz >> out.txt",
    )
    .at_prompt("do you wish to overwrite (y or n)? ")
    .type_keys("y\r")
    .finish();
    assert_eq!(left.out, "rc=0\nx.gz\nhello");
    assert!(
        left.screen
            .contains("gzip: x.gz already exists; do you wish to overwrite (y or n)? y"),
        "{}",
        left.screen
    );
}

#[test]
fn the_overwrite_question_answered_no_keeps_both_files() {
    let left = Script::start(
        "gzip-overwrite-no",
        "printf 'hello\\n' > x; printf 'old\\n' > x.gz; gzip x; echo \"rc=$?\" > out.txt; \
         ls x* >> out.txt; cat x.gz >> out.txt",
    )
    .at_prompt("do you wish to overwrite (y or n)? ")
    .type_keys("no\r")
    .finish();
    assert_eq!(left.out, "rc=2\nx\nx.gz\nold");
    assert!(
        left.screen.contains(
            "gzip: x.gz already exists; do you wish to overwrite (y or n)? no\n\tnot overwritten"
        ) || left
            .screen
            .contains("(y or n)? no\n        not overwritten"),
        "{}",
        left.screen
    );
}
