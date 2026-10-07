//! `zip`, `unzip` and `zipinfo`: the golden output of `tests/oracle/zip_cases.sh` and
//! `unzip_cases.sh`, and what the oracles cannot show: Windows' own readers and writers
//! of zip archives (`tar.exe` and `PowerShell`), names Windows cannot hold, symbolic links,
//! owners, absolute names with a drive, substitution files and the console's questions.
//!
//! The scripts ran under Info-ZIP's zip 3.0 and `UnZip` 6.00 to make the golden files;
//! here they run under cash. Where cash differs on purpose, the expected text is
//! replaced in the test with the reason beside it.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, golden, run, run_in, run_oracle_script, with_divergence};
use crate::read_console::Script;

#[test]
fn zip_matches_info_zip_3_0() {
    // zip joins a comment's lines with CRLF, which the golden file's reading turns to LF.
    assert_eq!(
        run_oracle_script("zip_cases").replace("\r\n", "\n"),
        golden("zip_cases")
    );
}

#[test]
fn unzip_and_zipinfo_match_unzip_6_00() {
    // cash's own version lines, in the usages.
    let expected = with_divergence(
        &golden("unzip_cases"),
        "UnZip 6.00 of 20 April 2009, by Info-ZIP.  Maintained by C. Spieler.  Send\n\
         bug reports using http://www.info-zip.org/zip-bug.html; see README for details.\n",
        "UnZip (cash): UnZip 6.00's options, in pure Rust.\n",
        1,
    );
    let expected = with_divergence(
        &expected,
        "ZipInfo 3.00 of 20 April 2009, by Greg Roelofs and the Info-ZIP group.\n",
        "ZipInfo (cash): ZipInfo 3.00's options, in pure Rust.\n",
        1,
    );
    assert_eq!(run_oracle_script("unzip_cases"), expected);
}

#[test]
fn the_three_are_builtins_with_pages() {
    for (name, word) in [
        ("zip", "zip 3.0"),
        ("unzip", "UnZip 6.00"),
        ("zipinfo", "ZipInfo 3.00"),
    ] {
        let out = run(&format!("type {name}"));
        assert!(
            out.stdout.contains("shell builtin"),
            "{name}: {}",
            out.stdout
        );
        let page = run(&format!("help {name}"));
        assert_eq!(page.code, 0, "{}", page.stderr);
        assert!(page.stdout.contains(word), "{name}: {}", page.stdout);
    }
}

/// Both ways with Windows' own: `tar.exe` (bsdtar, on libarchive) reads what cash's zip
/// writes and writes what cash's unzip reads; so does `PowerShell`'s .NET, whose
/// Compress-Archive writes `\` between a name's parts.
#[test]
fn windows_and_cash_read_each_others_archives() {
    let scratch = Scratch::new("zip-windows");
    let out = run_in(
        scratch.path(),
        "t=\"$SYSTEMROOT/System32/tar.exe\"; mkdir -p src/sub; printf 'hello\\n' > src/a.txt; \
         printf 'world\\n' > src/sub/b.txt; head -c 3000 /dev/zero | tr '\\0' a > src/big.txt; \
         zip -qr c.zip src; mkdir x; \"$t\" -xf c.zip -C x && cat x/src/a.txt x/src/sub/b.txt; \
         cmp x/src/big.txt src/big.txt && echo same; \
         \"$t\" -a -cf w.zip src; unzip -tq w.zip; unzip -p w.zip src/sub/b.txt; \
         powershell -NoProfile -Command 'Compress-Archive -Path src -DestinationPath p.zip; \
         Expand-Archive -Path c.zip -DestinationPath e' && cat e/src/a.txt; \
         unzip -tq p.zip; unzip -p p.zip src/sub/b.txt",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "hello\nworld\nsame\nNo errors detected in compressed data of w.zip.\nworld\nhello\n\
         No errors detected in compressed data of p.zip.\nworld"
    );
}

/// `ab:cd`, `CON` and `end.` from Python's zipfile, with `ok` beside them.
const BAD_NAMES: &str = "504b0304140000000000831822507073dba506000000060000000500000061623a636461623a63640a\
    504b030414000000000083182250a7690c5d040000000400000003000000434f4e434f4e0a\
    504b030414000000000083182250de0c05e8050000000500000004000000656e642e656e642e0a\
    504b0304140000000000831822507d0e16da0300000003000000020000006f6b6f6b0a\
    504b01021403140000000000831822507073dba50600000006000000050000000000000000000000a4810000000061623a6364\
    504b0102140314000000000083182250a7690c5d0400000004000000030000000000000000000000a48129000000434f4e\
    504b0102140314000000000083182250de0c05e80500000005000000040000000000000000000000a4814e000000656e642e\
    504b01021403140000000000831822507d0e16da0300000003000000020000000000000000000000a481750000006f6b\
    504b05060000000004000400c6000000980000000000";

fn unhex(text: &str) -> Vec<u8> {
    let digits: Vec<u8> = text.bytes().filter(u8::is_ascii_hexdigit).collect();
    digits
        .chunks(2)
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).expect("hex"), 16).expect("hex digits")
        })
        .collect()
}

/// A member whose name Windows cannot hold is refused by name, `UnZip`'s way for a file it
/// cannot create; the rest is extracted.
#[test]
fn a_name_windows_cannot_hold_is_refused() {
    let scratch = Scratch::new("zip-names");
    std::fs::write(scratch.join("bad.zip"), unhex(BAD_NAMES)).expect("the archive");
    let out = run_in(scratch.path(), "unzip -q bad.zip; echo \"rc=$?\"; ls");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "rc=50\nbad.zip\nok");
    assert_eq!(
        out.stderr,
        "error:  cannot create ab:cd\n        Invalid argument\n\
         error:  cannot create CON\n        Invalid argument\n\
         error:  cannot create end.\n        Invalid argument"
    );
}

/// An absolute name loses its drive in the archive, as zip drops a leading `/`; unzip
/// strips a drive as it strips a `/`.
#[test]
fn a_drive_is_left_out_of_a_name() {
    let scratch = Scratch::new("zip-drive");
    let out = run_in(
        scratch.path(),
        "printf 'x\\n' > f; zip -q a.zip \"$PWD/f\"; unzip -Z1 a.zip | sed \"s|^${PWD#?:/}/||\"",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "f");
}

/// zip records the file's owner and group by their numbers, as `id` gives them, and
/// zipinfo shows them; `-X` leaves them out.
#[test]
fn the_owner_is_recorded_by_number() {
    let scratch = Scratch::new("zip-owner");
    let out = run_in(
        scratch.path(),
        "printf 'x\\n' > f; zip -q o.zip f; zip -qX x.zip f; \
         zipinfo -v o.zip | grep -c 'Unix UID/GID (any size)'; zipinfo -v x.zip | grep -c 'UID/GID'; \
         printf '%08x\\n' \"$(stat -c %u f)\"",
    );
    assert_eq!(out.stderr, "");
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.first(), Some(&"1"), "{}", out.stdout);
    assert_eq!(lines.get(1), Some(&"0"), "{}", out.stdout);
}

/// A symbolic link stored with `-y` comes back as one where Windows allows it, else as a
/// file holding its target, as `UnZip` writes one on a system without links.
#[test]
fn a_symbolic_link_comes_back_where_windows_allows_one() {
    let scratch = Scratch::new("zip-symlink");
    std::fs::write(scratch.join("probe-target"), "").expect("a file");
    let allowed =
        std::os::windows::fs::symlink_file("probe-target", scratch.join("probe-link")).is_ok();
    let out = run_in(
        scratch.path(),
        "printf 'hi\\n' > a.txt; ln -s a.txt l 2>/dev/null || exit 0; zip -qy l.zip l a.txt; \
         zipinfo l.zip | sed -n 3p | cut -c1-10; mkdir x; cd x; unzip -q ../l.zip; cat l",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    if allowed {
        assert_eq!(out.stdout, "lrwxrwxrwx\nhi");
    }
}

#[test]
fn substitution_files_are_read_and_written() {
    let scratch = Scratch::new("zip-substitution");
    let out = run_in(
        scratch.path(),
        "printf 'hi\\n' > f; zip -q a.zip f; unzip -p <(cat a.zip) f; zipinfo -1 <(cat a.zip); \
         zip -q >(cat > b.zip) f; sleep 1; unzip -p b.zip f",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "hi\nf\nhi");
}

/// At a console, an existing file is asked about, and `A` replaces it and all after it.
#[test]
fn the_replace_question_is_asked_at_a_console() {
    let left = Script::start(
        "unzip-replace",
        "printf 'new\\n' > f; printf 'two\\n' > g; zip -q a.zip f g; printf 'old\\n' > f; \
         unzip a.zip; echo \"rc=$?\" > out.txt; cat f >> out.txt",
    )
    .at_prompt("replace f? [y]es, [n]o, [A]ll, [N]one, [r]ename: ")
    .type_keys("A\r")
    .finish();
    assert_eq!(left.out, "rc=0\nnew");
}

/// At a console, the password of an encrypted member is asked for, and kept for the next.
#[test]
fn a_password_is_asked_for_at_a_console() {
    let left = Script::start(
        "unzip-password",
        "printf 'secret\\n' > f; printf 'more\\n' > g; zip -q -P pw e.zip f g; rm f g; \
         unzip -q e.zip; echo \"rc=$?\" > out.txt; cat f g >> out.txt",
    )
    .at_prompt("[e.zip] f password: ")
    .type_keys("pw\r")
    .finish();
    assert_eq!(left.out, "rc=0\nsecret\nmore");
    assert!(!left.screen.contains("pw\n"), "{}", left.screen);
}

/// `zip -e` asks for the password twice at the console, and does not show it.
#[test]
fn zip_asks_for_a_password_twice() {
    let left = Script::start(
        "zip-encrypt",
        "printf 'x\\n' > f; zip -q -e e.zip f; echo \"rc=$?\" > out.txt; \
         unzip -P pw -p e.zip f >> out.txt",
    )
    .at_prompt("Enter password: ")
    .type_keys("pw\r")
    .at_prompt("Verify password: ")
    .type_keys("pw\r")
    .finish();
    assert_eq!(left.out, "rc=0\nx");
}

/// What 7-Zip writes and Info-ZIP's unzip cannot read: `WinZip`'s AES (128 and 256, stored
/// and deflated), `PPMd` and Deflate64; and PKZIP 1's shrunk and reduced members.
#[test]
fn unzip_reads_aes_ppmd_deflate64_and_pkzip_1s_methods() {
    let scratch = Scratch::new("unzip-methods");
    for (name, bytes) in [
        (
            "aes256.zip",
            &include_bytes!("../../../cash-archive/tests/fixtures/aes256.zip")[..],
        ),
        (
            "aes128.zip",
            &include_bytes!("../../../cash-archive/tests/fixtures/aes128.zip")[..],
        ),
        (
            "aesdef.zip",
            &include_bytes!("../../../cash-archive/tests/fixtures/aesdef.zip")[..],
        ),
        (
            "ppmd.zip",
            &include_bytes!("../../../cash-archive/tests/fixtures/ppmd.zip")[..],
        ),
        (
            "d64.zip",
            &include_bytes!("../../../cash-archive/tests/fixtures/d64.zip")[..],
        ),
        (
            "shrunk.zip",
            &include_bytes!("../../../cash-archive/tests/fixtures/shrunk.zip")[..],
        ),
        (
            "reduced.zip",
            &include_bytes!("../../../cash-archive/tests/fixtures/reduced.zip")[..],
        ),
    ] {
        std::fs::write(scratch.join(name), bytes).expect("a fixture");
    }
    let out = run_in(
        scratch.path(),
        "for f in aes256 aes128 ppmd d64; do unzip -P pw -p $f.zip a.txt; done; \
         unzip -P pw -p aesdef.zip | wc -c; unzip -p shrunk.zip; echo; unzip -p reduced.zip; echo; \
         unzip -P wrong -t aes256.zip; echo \"rc=$?\"; unzip -P pw -tq aes256.zip; \
         unzip -v aes256.zip | sed -n 4p | cut -c1-30",
    );
    assert_eq!(
        out.stdout,
        "hello hello hello\nhello hello hello\nhello hello hello\nhello hello hello\n11893\nhello\nhello\n\
         Archive:  aes256.zip\nCaution:  zero files tested in aes256.zip.\n\
         1 file skipped because of incorrect password.\nrc=82\n\
         No errors detected in compressed data of aes256.zip.\n      18  Unk:099       18   0"
    );
}

/// A split archive cash writes is read back by cash from all its parts, and made one
/// archive again with `-s 0`; `-sv` names the parts it closes.
#[test]
fn a_split_archive_is_read_from_its_parts() {
    let scratch = Scratch::new("zip-split");
    let out = run_in(
        scratch.path(),
        "seq 1 30000 > n.txt; zip -q -0 -s 64k sp.zip n.txt; ls; unzip -tq sp.zip; \
         unzip -p sp.zip n.txt | tail -1; zipinfo -1 sp.zip; zip -q -s 0 sp.zip --out one.zip; \
         unzip -tq one.zip; zip -0 -s 64k -sv sv.zip n.txt | sed -n '1p;$p'",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "n.txt\nsp.z01\nsp.z02\nsp.zip\nNo errors detected in compressed data of sp.zip.\n30000\nn.txt\n\
         No errors detected in compressed data of one.zip.\nsplitsize = 65536\n\tClosing split sv.z02"
    );
}

/// A wildcard in the archive's name names several archives: each processed in the
/// folder's order, `UnZip`'s tally after them, and the last one that is no archive reported
/// in full.
#[test]
fn a_wildcard_names_several_archives() {
    let scratch = Scratch::new("unzip-wildcard");
    let out = run_in(
        scratch.path(),
        "printf 'x\\n' > f; zip -q a.zip f; zip -q b.zip f; printf junk > c.zip; \
         unzip -tq '*.zip'; echo \"rc=$?\"; unzip -l '[ab].zip' | grep -c Archive; zipinfo -1 '[ab].zip'",
    );
    assert_eq!(
        out.stdout,
        "No errors detected in compressed data of a.zip.\n\
         No errors detected in compressed data of b.zip.\nrc=9\n2\nf\n\nf"
    );
    assert!(
        out.stderr
            .starts_with("[c.zip]\n  End-of-central-directory signature not found."),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains(
            "unzip:  cannot find zipfile directory in one of *.zip or\n        *.zip.zip, and cannot find c.zip.ZIP, period.\n\n\
             2 archives were successfully processed."
        ),
        "{}",
        out.stderr
    );
}
