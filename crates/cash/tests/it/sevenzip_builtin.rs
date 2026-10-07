//! `7z` and `7za`: the golden output of `tests/oracle/7z_cases.sh`, made by Scoop's 7-Zip
//! 26.03 on Windows, and what the oracle cannot show: the `7za` name, the page, and what
//! extraction leaves on disk (times, attributes).
//!
//! The script ran under 7-Zip to make the golden file, with its CRLF and `\` turned to
//! LF and `/` (the user's choice for cash's 7z); here it runs under cash. Where cash
//! differs on purpose, the expected text is replaced in the test with the reason beside
//! it.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, golden, run, run_in, run_oracle_script, with_divergence};

const STORED: &[u8] = include_bytes!("../../../cash-archive/tests/fixtures/7z/stored.7z");
const AES: &[u8] = include_bytes!("../../../cash-archive/tests/fixtures/7z/aes.7z");

#[test]
fn seven_z_matches_7_zip_26_03() {
    // cash's own banner, in 7-Zip's place.
    let expected = with_divergence(
        &golden("7z_cases"),
        "7-Zip 26.03 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-09-03\n",
        "7-Zip (cash) : 7-Zip 26.03's options, in pure Rust\n",
        137,
    );
    assert_eq!(run_oracle_script("7z_cases"), expected);
}

#[test]
fn seven_z_reads_and_writes_the_other_formats_as_7_zip_26_03_does() {
    // cash's own banner, in 7-Zip's place.
    let expected = with_divergence(
        &golden("7z_formats"),
        "7-Zip 26.03 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-09-03\n",
        "7-Zip (cash) : 7-Zip 26.03's options, in pure Rust\n",
        153,
    );
    assert_eq!(run_oracle_script("7z_formats"), expected);
}

#[test]
fn seven_za_is_the_same_command_under_its_own_name() {
    let out = run("7za | sed -n 4p; 7z | sed -n 4p");
    assert_eq!(
        out.stdout,
        "Usage: 7za <command> [<switches>...] <archive_name> [<file_names>...] [@listfile]\n\
         Usage: 7z <command> [<switches>...] <archive_name> [<file_names>...] [@listfile]"
    );
    let out = run("type 7z 7za");
    assert!(
        out.stdout.contains("7z is a shell builtin"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("7za is a shell builtin"),
        "{}",
        out.stdout
    );
    let page = run("help 7z");
    assert!(page.stdout.contains("7-Zip 26.03"), "{}", page.stdout);
}

#[test]
fn extraction_keeps_times_and_folders() {
    let dir = Scratch::new("7z-times");
    std::fs::write(dir.join("stored.7z"), STORED).expect("write the archive");
    let out = run_in(
        dir.path(),
        "7z x stored.7z -oout >/dev/null; echo $?; stat -c '%Y %n' out/d/a.txt out/d/sub/b.txt out/d out/d/empty",
    );
    // The fixture's times are 2026-10-07 08:00:00 UTC.
    assert_eq!(
        out.stdout,
        "0\n1791360000 out/d/a.txt\n1791360000 out/d/sub/b.txt\n1791360000 out/d\n1791360000 out/d/empty"
    );
    assert_eq!(
        std::fs::read(dir.join("out/d/a.txt")).expect("read"),
        b"hello\n"
    );
}

#[test]
fn every_method_writes_what_reads_back() {
    let dir = Scratch::new("7z-methods");
    // big.txt is 1.3 MB, more than one LZMA2 chunk at -mx1, so -mmt=4 runs two threads.
    let out = run_in(
        dir.path(),
        "mkdir d; seq 1 30000 > d/n.txt; printf 'alpha\\n' > d/a.txt; : > d/empty; \
         sum=$(sha256sum < d/n.txt); \
         for m in LZMA LZMA2 PPMd BZip2 Deflate Copy Delta:4; do \
           7z a -m0=$m -mx1 x.7z d -bso0 && 7z t x.7z -bso0 \
             && [ \"$(7z e -so x.7z d/n.txt | sha256sum)\" = \"$sum\" ] && echo \"$m ok\"; \
           rm x.7z; \
         done; \
         seq 1 200000 > big.txt; big=$(sha256sum < big.txt); \
         7z a -mx1 -mmt=4 mt.7z big.txt -bso0 \
           && [ \"$(7z e -so mt.7z big.txt | sha256sum)\" = \"$big\" ] && echo mt ok; \
         7z a -ms=off -mf=BCJ ns.7z d -bso0 && 7z l -slt ns.7z | grep -c '^Block = [0-9]'",
    );
    assert_eq!(
        out.stdout,
        "LZMA ok\nLZMA2 ok\nPPMd ok\nBZip2 ok\nDeflate ok\nCopy ok\nDelta:4 ok\nmt ok\n2",
        "{}",
        out.stderr
    );
}

#[test]
fn every_format_and_zip_method_writes_what_reads_back() {
    let dir = Scratch::new("7z-formats-write");
    let out = run_in(
        dir.path(),
        "seq 1 30000 > n.txt; sum=$(sha256sum < n.txt); \
         for f in n.tar n.gz n.bz2 n.xz; do \
           7z a $f n.txt -bso0 && 7z t $f -bso0 \
             && [ \"$(7z e -so $f | sha256sum)\" = \"$sum\" ] && echo \"$f ok\"; \
         done; \
         for m in Store Deflate BZip2 LZMA xz PPMd; do \
           7z a -mm=$m -pab x.zip n.txt -bso0 && 7z t -pab x.zip -bso0 \
             && [ \"$(7z e -so -pab x.zip n.txt | sha256sum)\" = \"$sum\" ] && echo \"$m ok\"; \
           rm x.zip; \
         done; \
         7z a -mem=AES256 -pab a.zip n.txt -bso0 \
           && [ \"$(7z e -so -pab a.zip | sha256sum)\" = \"$sum\" ] && echo aes ok",
    );
    assert_eq!(
        out.stdout,
        "n.tar ok\nn.gz ok\nn.bz2 ok\nn.xz ok\n\
         Store ok\nDeflate ok\nBZip2 ok\nLZMA ok\nxz ok\nPPMd ok\naes ok",
        "{}",
        out.stderr
    );
}

#[test]
fn encrypted_archives_need_their_password() {
    let dir = Scratch::new("7z-encrypt");
    let out = run_in(
        dir.path(),
        "mkdir d; printf 'secret data\\n' > d/s.txt; printf 'more\\n' > d/m.txt; \
         7z a -psecret -mhe=on e.7z d -bso0; echo $?; \
         7z t -psecret e.7z -bso0; echo $?; \
         7z t -pwrong e.7z -bso0 -bse0; echo $?; \
         7z a -psecret e.7z d/s.txt -bso0; echo $?; \
         7z x -psecret -so e.7z d/s.txt",
    );
    assert_eq!(out.stdout, "0\n0\n2\n0\nsecret data", "{}", out.stderr);
}

#[test]
fn deleting_from_a_solid_block_keeps_the_rest_whole() {
    let dir = Scratch::new("7z-repack");
    let out = run_in(
        dir.path(),
        "mkdir d; seq 1 50000 > d/n.txt; printf 'a\\n' > d/a.txt; printf 'z\\n' > d/z.txt; \
         sum=$(sha256sum < d/n.txt); \
         7z a s.7z d -bso0; 7z d s.7z d/a.txt -bso0; 7z t s.7z -bso0; echo $?; \
         7z l -ba s.7z | cut -c54-; \
         [ \"$(7z e -so s.7z d/n.txt | sha256sum)\" = \"$sum\" ] && echo same; \
         printf 'zz\\n' > d/z.txt; touch -d '2030-01-01' d/z.txt; \
         7z u s.7z d -bso0; 7z e -so s.7z d/z.txt",
    );
    assert_eq!(
        out.stdout, "0\nd\nd/n.txt\nd/z.txt\nsame\nzz",
        "{}",
        out.stderr
    );
}

#[test]
fn a_password_is_asked_for_and_given_on_standard_input() {
    let dir = Scratch::new("7z-password");
    std::fs::write(dir.join("aes.7z"), AES).expect("write the archive");
    let out = run_in(
        dir.path(),
        "printf 'secret\\n' | 7z x aes.7z -oout -bso0; echo $?; cat out/d/a.txt; 7z x -pwrong aes.7z -ono -bso0 -bse1; echo $?",
    );
    assert_eq!(
        out.stdout,
        "0\nhello\nERROR: aes.7z\nCannot open encrypted archive. Wrong password?\n\n2"
    );
}
