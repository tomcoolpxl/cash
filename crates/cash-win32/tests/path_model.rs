//! D3's path model: accept everything, render one thing.
//!
//! §4's divergence table says `pwd` prints `C:/src` rather than `/c/src`, and §9 measured
//! brush printing `C:\Users\thraa\...` today. These tests pin the target behaviour.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result. Shell snippets are spelled with hashes \
              throughout, including where they are not strictly needed, because \
              alternating the two forms by accident of content reads worse."
)]

use std::path::{Path, PathBuf};

use cash_win32::path::{Target, accept, accept_path, is_absolute, render, to_backslash, to_unix};

#[test]
fn every_accepted_spelling_reaches_the_same_place() {
    let expected = PathBuf::from("C:/Users/thraa");

    for spelling in [
        "C:/Users/thraa",
        "C:\\Users\\thraa",
        "/c/Users/thraa",
        "c:/Users/thraa",
    ] {
        let Target::Path(got) = accept(spelling) else {
            panic!("{spelling} should be an ordinary path");
        };
        assert_eq!(
            render(&got),
            render(&expected),
            "spelling {spelling:?} did not normalise as D3 requires"
        );
    }
}

#[test]
fn rendering_is_always_forward_slash_with_an_uppercase_drive() {
    // D3: "The canonical spelling is C:/test/dir — drive letter, forward slashes.
    // Never backslashes on output."
    assert_eq!(render(Path::new("C:\\src\\infra")), "C:/src/infra");
    assert_eq!(render(Path::new("c:/src/infra")), "C:/src/infra");
    assert_eq!(render(Path::new("C:/src/infra")), "C:/src/infra");
}

#[test]
fn rendering_never_leaks_the_extended_prefix() {
    // \\?\ is an internal detail of D29. A script must never see it.
    assert_eq!(render(Path::new(r"\\?\C:\src")), "C:/src");
    assert_eq!(render(Path::new(r"\\?\UNC\server\share")), "//server/share");
}

#[test]
fn dev_targets_are_not_paths() {
    // D7 carves these out of D29: with the \\?\ prefix, `NUL` would be a *file*, so
    // /dev/null has to resolve to the device explicitly.
    assert_eq!(accept("/dev/null"), Target::Null);
    assert_eq!(accept("/dev/stdin"), Target::Stdin);
    assert_eq!(accept("/dev/stdout"), Target::Stdout);
    assert_eq!(accept("/dev/stderr"), Target::Stderr);
    assert_eq!(accept("/dev/fd/3"), Target::Fd(3));
}

#[test]
fn bare_nul_is_an_ordinary_path_not_a_device() {
    // D28: `touch nul` creates a file called nul, exactly as on Linux. Only /dev/null
    // means the device. §9 measured brush sending it to the device today.
    assert!(matches!(accept("nul"), Target::Path(_)));
    assert!(matches!(accept("C:/tmp/nul"), Target::Path(_)));
}

#[test]
fn unc_paths_survive_both_spellings() {
    let Target::Path(from_slashes) = accept("//server/share/file") else {
        panic!("UNC should be a path");
    };
    let Target::Path(from_backslashes) = accept(r"\\server\share\file") else {
        panic!("UNC should be a path");
    };
    assert_eq!(render(&from_slashes), render(&from_backslashes));
    assert_eq!(render(&from_slashes), "//server/share/file");
}

#[test]
fn absoluteness_accepts_both_slash_directions() {
    assert!(is_absolute(Path::new("C:/x")));
    assert!(is_absolute(Path::new("C:\\x")));
    assert!(is_absolute(Path::new(r"\\server\share")));
    assert!(is_absolute(Path::new("//server/share")));
    assert!(!is_absolute(Path::new("sub/file")));
    assert!(!is_absolute(Path::new("C:")));
}

#[test]
fn winpath_conversions_round_trip() {
    // D45's escape hatch: D4 forbids cash rewriting arguments, so the user converts
    // deliberately when a DOS-lineage tool needs backslashes.
    assert_eq!(to_unix(Path::new("C:/Users/thraa")), "/c/Users/thraa");
    assert_eq!(to_backslash(Path::new("C:/Users/thraa")), r"C:\Users\thraa");

    let unix = to_unix(Path::new("C:/Users/thraa"));
    assert_eq!(render(&accept_path(&unix)), "C:/Users/thraa");

    let win = to_backslash(Path::new("C:/Users/thraa"));
    assert_eq!(render(&accept_path(&win)), "C:/Users/thraa");
}

#[test]
fn drive_root_survives_the_unix_spelling() {
    assert_eq!(render(&accept_path("/c")), "C:/");
    assert_eq!(render(&accept_path("/c/")), "C:/");
    assert_eq!(to_unix(Path::new("C:/")), "/c");
}

#[test]
fn tmp_maps_to_the_real_temp_directory() {
    // A compat mapping, not a mount. §9 measured /tmp already working in brush.
    let Target::Path(got) = accept("/tmp") else {
        panic!("/tmp should be a path");
    };
    assert!(
        is_absolute(&got),
        "/tmp resolved to {got:?}, which is not absolute"
    );

    let Target::Path(nested) = accept("/tmp/cash-test.txt") else {
        panic!("/tmp/... should be a path");
    };
    assert!(
        render(&nested).ends_with("/cash-test.txt"),
        "got {}",
        render(&nested)
    );
}

#[test]
fn a_path_that_merely_starts_with_a_letter_is_not_a_drive() {
    // /cash/foo must not become C:/ash/foo. The drive spelling requires the letter to be
    // the whole first segment.
    let Target::Path(got) = accept("/cash/foo") else {
        panic!("should be a path");
    };
    assert_eq!(render(&got), "/cash/foo");
}

#[test]
fn a_bare_drive_letter_flag_is_not_mistaken_for_a_path() {
    // Regression: `cmd.exe /d /s /c ...` is one of the most common invocations on
    // Windows, and `/d` was being read as drive D — producing a spurious hint on a
    // command that was perfectly correct.
    assert_eq!(cash_win32::path::unix_drive_spelling("/d"), None);
    assert_eq!(cash_win32::path::unix_drive_spelling("/s"), None);
    assert_eq!(cash_win32::path::unix_drive_spelling("/c"), None);

    // A real drive-spelled path still resolves.
    assert!(cash_win32::path::unix_drive_spelling("/c/Windows").is_some());
    assert!(cash_win32::path::unix_drive_spelling("/d/data/x").is_some());

    // And a directory that merely begins with a letter is not a drive.
    assert_eq!(cash_win32::path::unix_drive_spelling("/cash/foo"), None);
}
