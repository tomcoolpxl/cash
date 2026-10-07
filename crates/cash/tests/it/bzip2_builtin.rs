//! `bzip2`, `bunzip2` and `bzcat`: the golden output of `tests/oracle/bzip2_cases.sh`,
//! and what the oracle cannot show: the console (the refusal to write compressed data to
//! it or read it from it), a stream of many blocks, substitution files, the help page,
//! and that nothing is left beside a file when a run fails.
//!
//! The script ran under bzip2 1.0.8 to make `bzip2_cases.out`; here it runs under cash.
//! Where cash differs on purpose, the expected text is replaced in the test with the
//! reason beside it.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, golden, run, run_in, run_oracle_script, with_divergence};
use crate::read_console::Script;

#[test]
fn bzip2_matches_bzip2_1_0_8() {
    // cash's own version line, at the top of every usage and for -V and -L.
    let expected = with_divergence(
        &golden("bzip2_cases"),
        "bzip2, a block-sorting file compressor.  Version 1.0.8, 13-Jul-2019.\n",
        "bzip2 (cash): bzip2 1.0.8's options, on libbz2-rs-sys\n",
        10,
    );
    // -vv: libbzip2 prints its block-by-block trace from inside the library, which cash
    // does not carry; cash says what -v says.
    let expected = with_divergence(
        &expected,
        "  h:       \n    block 1: crc = 0xc1c080e2, combined CRC = 0xc1c080e2, size = 6\n    \
         final combined CRC = 0xc1c080e2\n    0.143:1, 56.000 bits/byte, -600.00% saved, \
         6 in, 42 out.\n",
        "  h:        0.143:1, 56.000 bits/byte, -600.00% saved, 6 in, 42 out.\n",
        1,
    );
    // -1 on 300 kB: cash writes a stream of each 100 kB, compressed on every core
    // (pbzip2's layout, the user's choice), where bzip2 writes one stream of three blocks.
    let expected = with_divergence(
        &expected,
        "== environment\n134\n",
        "== environment\n216\n",
        1,
    );
    assert_eq!(run_oracle_script("bzip2_cases"), expected);
}

#[test]
fn the_three_names_are_builtins_with_one_page() {
    for name in ["bzip2", "bunzip2", "bzcat"] {
        let out = run(&format!("type {name}"));
        assert!(
            out.stdout.contains("shell builtin"),
            "{name}: {}",
            out.stdout
        );
        let page = run(&format!("help {name}"));
        assert_eq!(page.code, 0, "{}", page.stderr);
        assert!(
            page.stdout.contains("libbz2-rs-sys"),
            "{name}: {}",
            page.stdout
        );
    }
}

/// A stream of many blocks, compressed and decompressed through pipes and files: at
/// `-1` a block is 100 kB, so a file of a megabyte and more crosses many of them.
#[test]
fn a_stream_of_many_blocks_round_trips_through_pipes_and_files() {
    let scratch = Scratch::new("bzip2-large");
    let out = run_in(
        scratch.path(),
        "seq 1 150000 > big; yes 'the quick brown fox' | head -c 400000 >> big; \
         bzip2 -1 -c big | bzip2 -dc | cmp - big && echo pipe-same; \
         bzip2 -k big && bzip2 -t big.bz2 && echo tested; \
         bunzip2 -c big.bz2 | cmp - big && echo file-same; \
         bzip2 -c big big | bzcat | wc -c",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    let size = std::fs::metadata(scratch.join("big"))
        .expect("the file")
        .len();
    assert_eq!(
        out.stdout,
        format!("pipe-same\ntested\nfile-same\n{}", size * 2)
    );
}

#[test]
fn substitution_files_are_read() {
    let out = run("bzip2 -c <(printf 'hi\\n') | bzip2 -dc; bzcat <(printf 'hi\\n' | bzip2)");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "hi\nhi");
}

/// A run that fails leaves nothing beside the file: the temporary name is removed.
#[test]
fn a_failed_run_leaves_no_temporary_file() {
    let scratch = Scratch::new("bzip2-tmp");
    let out = run_in(
        scratch.path(),
        "printf 'hello\\n' | bzip2 > good.bz2; head -c 20 good.bz2 > bad.bz2; \
         bzip2 -d bad.bz2; echo \"rc=$?\"; bzip2 -t good.bz2; echo \"rc=$?\"; ls -a",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stderr
            .contains("bzip2: Deleting output file bad, if it exists."),
        "{}",
        out.stderr
    );
    assert_eq!(out.stdout, "rc=2\nrc=0\n.\n..\nbad.bz2\ngood.bz2");
}

/// At a console, compressed data is not written to it or read from it, `-f` or not.
#[test]
fn a_console_is_refused_compressed_data() {
    let left = Script::start(
        "bzip2-terminal",
        "bzip2; echo \"rc=$?\" > out.txt; bzip2 -df; echo \"rc=$?\" >> out.txt; \
         printf 'hello\\n' | bzip2 | bzip2 -dc >> out.txt; echo \"rc=$?\" >> out.txt",
    )
    .finish();
    assert_eq!(left.out, "rc=1\nrc=1\nhello\nrc=0");
    assert!(
        left.screen.contains(
            "bzip2: I won't write compressed data to a terminal.\n\
             bzip2: For help, type: `bzip2 --help'."
        ),
        "{}",
        left.screen
    );
    assert!(
        left.screen
            .contains("bzip2: I won't read compressed data from a terminal."),
        "{}",
        left.screen
    );
}
