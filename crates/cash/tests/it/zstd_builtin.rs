//! `zstd`, `unzstd` and `zstdcat`: the golden output of `tests/oracle/zstd_cases.sh`,
//! and what the oracle cannot show: the console (the refusal of compressed data, the
//! overwrite question), a stream of several frames, another reader of what cash writes
//! (Windows' own `tar.exe`, on libzstd), substitution files, the help page, and that
//! nothing is left beside a file when a run fails.
//!
//! The script ran under zstd 1.5.7 to make `zstd_cases.out`; here it runs under cash.
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
fn zstd_matches_zstd_1_5_7() {
    // cash's own version line.
    let expected = with_divergence(
        &golden("zstd_cases"),
        "*** Zstandard CLI (64-bit) v1.5.7, by Yann Collet ***\n",
        "*** zstd (cash): zstd v1.5.7's options, on ruzstd ***\n",
        2,
    );
    assert_eq!(run_oracle_script("zstd_cases"), expected);
}

#[test]
fn the_three_names_are_builtins_with_one_page() {
    for name in ["zstd", "unzstd", "zstdcat"] {
        let out = run(&format!("type {name}"));
        assert!(
            out.stdout.contains("shell builtin"),
            "{name}: {}",
            out.stdout
        );
        let page = run(&format!("help {name}"));
        assert_eq!(page.code, 0, "{}", page.stderr);
        assert!(page.stdout.contains("ruzstd"), "{name}: {}", page.stdout);
    }
}

/// A stream of several frames: cash writes one for each 4 MiB, and `-l` counts them;
/// through pipes and files, and concatenated.
#[test]
fn a_stream_of_several_frames_round_trips_and_lists() {
    let scratch = Scratch::new("zstd-large");
    let out = run_in(
        scratch.path(),
        "seq 1 1500000 > big; zstd -q -c big | zstd -dc | cmp - big && echo pipe-same; \
         zstd -q big && zstd -q -t big.zst && echo tested; \
         zstd -l big.zst | awk 'NR == 2 {print $1}'; \
         unzstd -c big.zst | cmp - big && echo file-same; \
         zstd -q -c big big | zstdcat | wc -c",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    let size = std::fs::metadata(scratch.join("big"))
        .expect("the file")
        .len();
    let frames = size.div_ceil(4 << 20);
    assert_eq!(
        out.stdout,
        format!("pipe-same\ntested\n{frames}\nfile-same\n{}", size * 2)
    );
}

/// What cash writes, another program reads: Windows' own `tar.exe`, on libarchive and
/// libzstd, unpacks a .tar.zst made here, with its content size and checksum.
#[test]
fn windows_tar_reads_what_cash_writes() {
    let scratch = Scratch::new("zstd-tar-exe");
    let out = run_in(
        scratch.path(),
        "tar=\"$SYSTEMROOT/System32/tar.exe\"; printf 'hello\\n' > f; \"$tar\" -cf a.tar f; \
         zstd -q a.tar; zstd -q --no-check -o b.tar.zst a.tar; mkdir x y; \
         \"$tar\" -xf a.tar.zst -C x && cat x/f; \"$tar\" -xf b.tar.zst -C y && cat y/f",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "hello\nhello");
}

#[test]
fn substitution_files_are_read() {
    let out = run("zstd -q -c <(printf 'hi\\n') | zstd -dc; zstdcat <(printf 'hi\\n' | zstd -q)");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "hi\nhi");
}

/// A run that fails leaves nothing beside the file: the temporary name is removed.
#[test]
fn a_failed_run_leaves_no_temporary_file() {
    let scratch = Scratch::new("zstd-tmp");
    let out = run_in(
        scratch.path(),
        "printf 'hello\\n' | zstd -q > good.zst; head -c 12 good.zst > bad.zst; \
         zstd -q -d bad.zst; echo \"rc=$?\"; zstd -q -t good.zst; echo \"rc=$?\"; ls -a",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stderr
            .contains("bad.zst : Read error (39) : premature end"),
        "{}",
        out.stderr
    );
    assert_eq!(out.stdout, "rc=1\nrc=0\n.\n..\nbad.zst\ngood.zst");
}

/// At a console, compressed data is not read from it or written to it without `-f` or
/// `-c`; decompressed data is.
#[test]
fn a_console_is_refused_compressed_data() {
    let left = Script::start(
        "zstd-terminal",
        "printf 'hello\\n' > h; zstd; echo \"rc=$?\" > out.txt; zstd -d; echo \"rc=$?\" >> out.txt; \
         zstd -q h; zstdcat h.zst; echo \"rc=$?\" >> out.txt",
    )
    .finish();
    assert_eq!(left.out, "rc=1\nrc=1\nrc=0");
    assert!(
        left.screen.contains("stdin is a console, aborting"),
        "{}",
        left.screen
    );
    assert!(left.screen.contains("hello"), "{}", left.screen);
}

/// At a console, a file that exists is asked about, and `y` overwrites it.
#[test]
fn the_overwrite_question_is_asked_at_a_console() {
    let left = Script::start(
        "zstd-overwrite-yes",
        "printf 'hello\\n' > x; printf 'old\\n' > x.zst; zstd x; echo \"rc=$?\" > out.txt; \
         zstd -dc x.zst >> out.txt",
    )
    .at_prompt("overwrite (y/n) ? ")
    .type_keys("y\r")
    .finish();
    assert_eq!(left.out, "rc=0\nhello");
    assert!(
        left.screen
            .contains("zstd: x.zst already exists; overwrite (y/n) ? y"),
        "{}",
        left.screen
    );
}
