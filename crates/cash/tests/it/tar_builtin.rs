//! `tar`: the golden output of `tests/oracle/tar_cases.sh`, and what the oracle cannot
//! show: Windows' own `tar.exe` reading what cash writes and the other way round,
//! absolute names with a drive, owner names, symbolic links, names Windows cannot hold,
//! the console, substitution files and `--one-top-level`.
//!
//! The script ran under GNU tar 1.35 to make `tar_cases.out`; here it runs under cash.
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
fn tar_matches_gnu_tar_1_35() {
    // cash's own version line.
    let expected = with_divergence(
        &golden("tar_cases"),
        "tar (GNU tar) 1.35\n",
        "tar (cash): GNU tar 1.35's options, in pure Rust\n",
        1,
    );
    assert_eq!(run_oracle_script("tar_cases"), expected);
}

#[test]
fn tar_is_a_builtin_with_a_page() {
    let out = run("type tar");
    assert!(out.stdout.contains("shell builtin"), "{}", out.stdout);
    let page = run("help tar");
    assert_eq!(page.code, 0, "{}", page.stderr);
    assert!(page.stdout.contains("GNU tar 1.35"), "{}", page.stdout);
}

/// Each compression, both ways: what cash writes, Windows' `tar.exe` (bsdtar, on
/// libarchive) unpacks, and what it writes, cash unpacks; a name over 100 bytes goes
/// through GNU's long-name header one way and a pax header the other.
#[test]
fn windows_tar_and_cash_read_each_other() {
    let scratch = Scratch::new("tar-tar-exe");
    let out = run_in(
        scratch.path(),
        "t=\"$SYSTEMROOT/System32/tar.exe\"; mkdir -p src/sub; printf 'hello\\n' > src/a.txt; \
         printf 'world\\n' > src/sub/b.txt; long=src/$(printf 'n%.0s' $(seq 1 120)).txt; \
         printf 'long\\n' > \"$long\"; \
         for z in '' -z -j -J --zstd; do \
           rm -rf x y; mkdir x y; \
           tar $z -cf c.tar src && \"$t\" -xf c.tar -C x && cat x/src/a.txt x/src/sub/b.txt \"x/$long\"; \
           \"$t\" $z -cf w.tar src && tar -xf w.tar -C y && cat y/src/a.txt y/src/sub/b.txt \"y/$long\"; \
         done | sort | uniq -c | awk '{print $1, $2}'",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "10 hello\n10 long\n10 world");
}

/// An absolute name loses its drive, as GNU tar's loses its `/`, and says so once.
#[test]
fn an_absolute_name_loses_its_drive() {
    let scratch = Scratch::new("tar-absolute");
    let out = run_in(
        scratch.path(),
        "printf 'hello\\n' > f; tar -cf a.tar \"$PWD/f\" \"$PWD/f\"; echo \"rc=$?\"; \
         tar -tf a.tar | sed \"s|^${PWD#?:/}/||\"; mkdir x; tar -xf a.tar -C x; \
         cat \"x/${PWD#?:/}/f\"",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "rc=0\nf\nf\nhello");
    let drive = scratch
        .as_script_path()
        .get(..3)
        .expect("a drive")
        .to_owned();
    assert_eq!(
        out.stderr,
        format!("tar: Removing leading `{drive}' from member names")
    );
}

/// Without `--numeric-owner`, a long listing names the file's owner and group, as
/// `stat` does; with it, their numbers.
#[test]
fn a_listing_names_the_owner() {
    let scratch = Scratch::new("tar-owner");
    let out = run_in(
        scratch.path(),
        "printf 'x\\n' > f; tar -cf o.tar f; tar -tvf o.tar | awk '{print $2}'; \
         stat -c %U/%G f; tar --numeric-owner -tvf o.tar | awk '{print $2}'; stat -c %u/%g f",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 4, "{}", out.stdout);
    assert_eq!(lines.first(), lines.get(1), "{}", out.stdout);
    assert_eq!(lines.get(2), lines.get(3), "{}", out.stdout);
}

fn put(header: &mut [u8; 512], at: usize, bytes: &[u8]) {
    header
        .get_mut(at..at + bytes.len())
        .expect("inside the header")
        .copy_from_slice(bytes);
}

/// An archive of one symbolic link, `l` to `a.txt`, as GNU tar writes one.
fn symlink_archive() -> Vec<u8> {
    let mut header = [0_u8; 512];
    put(&mut header, 0, b"l");
    put(&mut header, 100, b"0000777\0");
    put(&mut header, 108, b"0000000\0");
    put(&mut header, 116, b"0000000\0");
    put(&mut header, 124, b"00000000000\0");
    put(&mut header, 136, b"00000000000\0");
    put(&mut header, 148, b"        ");
    put(&mut header, 156, b"2");
    put(&mut header, 157, b"a.txt");
    put(&mut header, 257, b"ustar  \0");
    let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
    put(&mut header, 148, format!("{sum:06o}\0 ").as_bytes());
    let mut archive = header.to_vec();
    archive.resize(512 * 20, 0);
    archive
}

/// A symbolic link is listed with its target, and made where Windows allows one
/// (Developer Mode, or an elevated shell); elsewhere it is refused as GNU tar refuses
/// one, with the status of an error.
#[test]
fn a_symbolic_link_is_made_where_windows_allows_one() {
    let scratch = Scratch::new("tar-symlink");
    std::fs::write(scratch.join("l.tar"), symlink_archive()).expect("the archive");
    std::fs::write(scratch.join("probe-target"), "").expect("a file");
    let allowed =
        std::os::windows::fs::symlink_file("probe-target", scratch.join("probe-link")).is_ok();
    let out = run_in(
        scratch.path(),
        "printf 'hi\\n' > a.txt; tar -tvf l.tar | awk '{print $1, $6, $7, $8}'; \
         tar -xf l.tar; echo \"rc=$?\"; cat l 2>/dev/null || true",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    if allowed {
        assert_eq!(out.stdout, "lrwxrwxrwx l -> a.txt\nrc=0\nhi");
        assert_eq!(out.stderr, "");
    } else {
        assert_eq!(out.stdout, "lrwxrwxrwx l -> a.txt\nrc=2");
        assert!(
            out.stderr
                .starts_with("tar: l: Cannot create symlink to 'a.txt': "),
            "{}",
            out.stderr
        );
        assert!(
            out.stderr
                .ends_with("tar: Exiting with failure status due to previous errors"),
            "{}",
            out.stderr
        );
    }
}

/// A member whose name Windows cannot hold is refused by name, and the rest of the
/// archive is extracted.
#[test]
fn a_name_windows_cannot_hold_is_refused() {
    let scratch = Scratch::new("tar-names");
    let out = run_in(
        scratch.path(),
        "printf 'x\\n' > plain; printf 'y\\n' > ok; \
         tar -cf n.tar --transform='s/^plain$/ab:cd/' plain --transform='s/^ok$/ok/' ok; \
         tar -rf n.tar --transform='s/plain/CON/' plain; \
         tar -rf n.tar --transform='s/plain/end./' plain; \
         tar -tf n.tar; mkdir out; tar -xf n.tar -C out; echo \"rc=$?\"; ls -A out",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "ab:cd\nok\nCON\nend.\nrc=2\nok");
    assert_eq!(
        out.stderr,
        // GNU's quotearg_colon: a colon in a name is escaped in a message.
        "tar: ab\\:cd: Cannot open: Invalid argument\n\
         tar: CON: Cannot open: Invalid argument\n\
         tar: end.: Cannot open: Invalid argument\n\
         tar: Exiting with failure status due to previous errors"
    );
}

#[test]
fn substitution_files_are_read() {
    let scratch = Scratch::new("tar-substitution");
    let out = run_in(
        scratch.path(),
        "tar -tf <(printf 'hi\\n' > f && tar -cf - f); tar -cf >(tar -tf -) f",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "f\nf");
}

/// The compressors cash does not carry are refused by name, before anything is written.
#[test]
fn a_compressor_cash_does_not_carry_is_refused() {
    let scratch = Scratch::new("tar-compressor");
    let out = run_in(
        scratch.path(),
        "printf 'x\\n' > f; tar -Zcf z.tar f; echo \"rc=$?\"; tar --lzop -cf z.tar f; \
         echo \"rc=$?\"; tar -I zip -cf z.tar f; echo \"rc=$?\"; ls",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "rc=2\nrc=2\nrc=2\nf");
    assert!(
        out.stderr
            .contains("tar: compress: compressor not carried by cash's tar\n"),
        "{}",
        out.stderr
    );
}

/// `--one-top-level`: everything under a folder named for the archive, or given; a name
/// already under it is left as it is. Without an archive's name there is no folder.
#[test]
fn one_top_level_puts_everything_under_one_folder() {
    let scratch = Scratch::new("tar-top-level");
    let out = run_in(
        scratch.path(),
        "mkdir src o p; printf 'a\\n' > src/a.txt; printf 't\\n' > top.txt; \
         tar -czf pack.tar.gz src top.txt; \
         (cd o && tar -xf ../pack.tar.gz --one-top-level && find . | sort); \
         (cd p && tar -xf ../pack.tar.gz --one-top-level=src && find . | sort); \
         tar -xf - --one-top-level < pack.tar.gz; echo \"rc=$?\"; \
         tar -xf pack.tar.gz -P --one-top-level=q; echo \"rc=$?\"",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        ".\n./pack\n./pack/src\n./pack/src/a.txt\n./pack/top.txt\n\
         .\n./src\n./src/a.txt\n./src/top.txt\nrc=2\nrc=2"
    );
    assert_eq!(
        out.stderr,
        "tar: Cannot deduce top-level directory name; please set it explicitly with \
         --one-top-level=DIR\n\
         Try 'tar --help' or 'tar --usage' for more information.\n\
         tar: '--one-top-level' cannot be used with '--absolute-names'\n\
         Try 'tar --help' or 'tar --usage' for more information."
    );
}

/// At a console, an archive is not written to it or read from it: `-f` was forgotten.
#[test]
fn a_console_is_refused_the_archive() {
    let left = Script::start(
        "tar-terminal",
        "printf 'x\\n' > f; tar -c f; echo \"rc=$?\" > out.txt; tar -t; echo \"rc=$?\" >> out.txt",
    )
    .finish();
    assert_eq!(left.out, "rc=2\nrc=2");
    assert!(
        left.screen
            .contains("Refusing to write archive contents to terminal (missing -f option?)"),
        "{}",
        left.screen
    );
    assert!(
        left.screen
            .contains("Refusing to read archive contents from terminal (missing -f option?)"),
        "{}",
        left.screen
    );
}
