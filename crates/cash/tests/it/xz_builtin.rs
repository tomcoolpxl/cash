//! `xz`, `unxz`, `xzcat`, `lzma`, `unlzma` and `lzcat`: the golden output of
//! `tests/oracle/xz_cases.sh`, and what the oracle cannot show: the console (the refusal
//! to write compressed data to it or read it from it), a stream of many blocks, another
//! reader of what cash writes (Windows' own `tar.exe`), substitution files, the help
//! page, and that nothing is left beside a file when a run fails.
//!
//! The script ran under XZ Utils 5.8.3 to make `xz_cases.out`; here it runs under cash.
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
fn xz_matches_xz_utils_5_8() {
    // cash's own version line, under xz and lzma.
    let expected = with_divergence(
        &golden("xz_cases"),
        "xz (XZ Utils) 5.8.3\n",
        "xz (cash): XZ Utils 5.8's options, on lzma-rust2\n",
        3,
    );
    assert_eq!(run_oracle_script("xz_cases"), expected);
}

#[test]
fn the_six_names_are_builtins_with_one_page() {
    for name in ["xz", "unxz", "xzcat", "lzma", "unlzma", "lzcat"] {
        let out = run(&format!("type {name}"));
        assert!(
            out.stdout.contains("shell builtin"),
            "{name}: {}",
            out.stdout
        );
        let page = run(&format!("help {name}"));
        assert_eq!(page.code, 0, "{}", page.stderr);
        assert!(
            page.stdout.contains("lzma-rust2"),
            "{name}: {}",
            page.stdout
        );
    }
}

/// A stream of many blocks: `--block-size` starts a new one every 100 KiB, and `-l`
/// counts them; through pipes and files, and concatenated.
#[test]
fn a_stream_of_many_blocks_round_trips_and_lists() {
    let scratch = Scratch::new("xz-large");
    let out = run_in(
        scratch.path(),
        "seq 1 150000 > big; yes 'the quick brown fox' | head -c 400000 >> big; \
         xz -1 -c big | xz -dc | cmp - big && echo pipe-same; \
         xz -k --block-size=100KiB big && xz -t big.xz && echo tested; \
         xz --robot -l big.xz | awk -F'\\t' '/^file/ {print $3}'; \
         unxz -c big.xz | cmp - big && echo file-same; \
         xz -c big big | xzcat | wc -c",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    let size = std::fs::metadata(scratch.join("big"))
        .expect("the file")
        .len();
    let blocks = size.div_ceil(100 << 10);
    assert_eq!(
        out.stdout,
        format!("pipe-same\ntested\n{blocks}\nfile-same\n{}", size * 2)
    );
}

/// What cash writes, another program reads: Windows' own `tar.exe`, on libarchive and
/// liblzma, unpacks a .tar.xz and a .tar.lzma made here.
#[test]
fn windows_tar_reads_what_cash_writes() {
    let scratch = Scratch::new("xz-tar-exe");
    let out = run_in(
        scratch.path(),
        "tar=\"$SYSTEMROOT/System32/tar.exe\"; printf 'hello\\n' > f; \"$tar\" -cf a.tar f; \
         xz -k a.tar; lzma -k a.tar; mkdir x l; \
         \"$tar\" -xf a.tar.xz -C x && cat x/f; \"$tar\" -xf a.tar.lzma -C l && cat l/f",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "hello\nhello");
}

#[test]
fn substitution_files_are_read() {
    let out = run("xz -c <(printf 'hi\\n') | xz -dc; xzcat <(printf 'hi\\n' | xz)");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "hi\nhi");
}

/// A run that fails leaves nothing beside the file: the temporary name is removed.
#[test]
fn a_failed_run_leaves_no_temporary_file() {
    let scratch = Scratch::new("xz-tmp");
    let out = run_in(
        scratch.path(),
        "printf 'hello\\n' | xz > good.xz; head -c 40 good.xz > bad.xz; \
         xz -d bad.xz; echo \"rc=$?\"; xz -t good.xz; echo \"rc=$?\"; ls -a",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stderr.contains("xz: bad.xz: Unexpected end of input"),
        "{}",
        out.stderr
    );
    assert_eq!(out.stdout, "rc=1\nrc=0\n.\n..\nbad.xz\ngood.xz");
}

/// At a console, compressed data is not written to it or read from it, `-f` or not;
/// decompressed data is.
#[test]
fn a_console_is_refused_compressed_data() {
    let left = Script::start(
        "xz-terminal",
        "printf 'hello\\n' > h; xz; echo \"rc=$?\" > out.txt; xz -cf h; echo \"rc=$?\" >> out.txt; \
         xz -d; echo \"rc=$?\" >> out.txt; xz -k h; xzcat h.xz; echo \"rc=$?\" >> out.txt",
    )
    .finish();
    assert_eq!(left.out, "rc=1\nrc=1\nrc=1\nrc=0");
    assert!(
        left.screen
            .contains("xz: Compressed data cannot be written to a terminal"),
        "{}",
        left.screen
    );
    assert!(
        left.screen
            .contains("xz: Compressed data cannot be read from a terminal"),
        "{}",
        left.screen
    );
    assert!(left.screen.contains("hello"), "{}", left.screen);
}
