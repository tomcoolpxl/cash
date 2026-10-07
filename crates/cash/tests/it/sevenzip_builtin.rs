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
        63,
    );
    assert_eq!(run_oracle_script("7z_cases"), expected);
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
