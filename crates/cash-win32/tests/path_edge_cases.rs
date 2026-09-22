//! Edge cases for the path model (D3, D7, D28, D29).
//!
//! `path_model.rs` covers the happy paths. These are the inputs that a shell actually
//! receives and that break naive implementations: empty strings, trailing separators,
//! mixed separators, relative paths that look absolute, case, unicode, and the
//! boundaries of the drive-letter rule.

#![cfg(windows)]

use std::path::{Path, PathBuf};

use cash_win32::path::{
    Target, accept, accept_path, is_absolute, is_reserved_name, lexically_normalize, render,
    to_backslash, to_extended, to_unix, unix_drive_spelling,
};

// ---------------------------------------------------------------------------
// Degenerate inputs
// ---------------------------------------------------------------------------

#[test]
fn empty_and_trivial_inputs_do_not_panic() {
    assert_eq!(render(Path::new("")), "");
    assert_eq!(accept_path(""), PathBuf::from(""));
    assert!(!is_absolute(Path::new("")));
    assert!(!is_reserved_name(Path::new("")));
    assert_eq!(unix_drive_spelling(""), None);
    assert_eq!(unix_drive_spelling("/"), None);
    assert_eq!(render(&lexically_normalize(Path::new(""))), "");
}

#[test]
fn a_lone_separator_is_not_a_drive() {
    assert_eq!(unix_drive_spelling("/"), None);
    assert_eq!(unix_drive_spelling("//"), None);
    assert_eq!(unix_drive_spelling("/1/x"), None, "digits are not drive letters");
    assert_eq!(unix_drive_spelling("/./x"), None);
}

#[test]
fn a_drive_letter_needs_a_following_slash() {
    // The regression that produced a spurious hint on `cmd.exe /d /s /c`.
    for flag in ["/c", "/d", "/s", "/q", "/Z"] {
        assert_eq!(unix_drive_spelling(flag), None, "{flag} should not look like a drive");
    }
    for path in ["/c/", "/c/x", "/d/data"] {
        assert!(unix_drive_spelling(path).is_some(), "{path} should look like a drive");
    }
}

// ---------------------------------------------------------------------------
// Separators
// ---------------------------------------------------------------------------

#[test]
fn mixed_separators_normalise() {
    assert_eq!(render(&accept_path(r"C:\Users/thraa\docs")), "C:/Users/thraa/docs");
    assert_eq!(render(&accept_path("C:/Users\\thraa")), "C:/Users/thraa");
}

#[test]
fn trailing_separators_are_preserved_not_doubled() {
    assert_eq!(render(&accept_path("C:/Users/")), "C:/Users/");
    assert_eq!(render(&accept_path(r"C:\Users\")), "C:/Users/");
    // A drive root keeps its slash; `C:` alone means "current dir on C:" and is not
    // the same path.
    assert_eq!(render(&accept_path("C:/")), "C:/");
}

#[test]
fn repeated_separators_do_not_become_unc() {
    // `C://Users` is a drive path with a redundant separator, not a UNC name.
    let got = render(&accept_path("C://Users"));
    assert!(got.starts_with("C:/"), "got {got}");
    assert!(!got.starts_with("//"), "was mistaken for UNC: {got}");
}

// ---------------------------------------------------------------------------
// Case
// ---------------------------------------------------------------------------

#[test]
fn drive_letters_render_uppercase_from_any_input_case() {
    for input in ["c:/x", "C:/x", "/c/x", "/C/x", r"c:\x"] {
        assert_eq!(render(&accept_path(input)), "C:/x", "failed for {input}");
    }
}

#[test]
fn the_unix_spelling_is_lowercase() {
    assert_eq!(to_unix(Path::new("C:/x")), "/c/x");
    assert_eq!(to_unix(Path::new("Z:/x")), "/z/x");
}

#[test]
fn the_rest_of_the_path_keeps_its_case() {
    // The filesystem is case-insensitive but case-*preserving*, and so is cash.
    assert_eq!(render(&accept_path("c:/Users/ThRaa/MyFile.TXT")), "C:/Users/ThRaa/MyFile.TXT");
}

// ---------------------------------------------------------------------------
// Dot segments
// ---------------------------------------------------------------------------

#[test]
fn dot_segments_resolve_in_awkward_positions() {
    assert_eq!(render(&lexically_normalize(Path::new("C:/a/./././b"))), "C:/a/b");
    assert_eq!(render(&lexically_normalize(Path::new("C:/a/b/c/../../d"))), "C:/a/d");
    assert_eq!(render(&lexically_normalize(Path::new("C:/a/../b/../c"))), "C:/c");
}

#[test]
fn a_filename_of_dots_is_not_a_dot_segment() {
    // `...` and `..foo` are ordinary names, not parent references.
    let got = render(&lexically_normalize(Path::new("C:/a/.../b")));
    assert!(got.contains("..."), "`...` was treated as a dot segment: {got}");
    let got = render(&lexically_normalize(Path::new("C:/a/..b")));
    assert!(got.contains("..b"), "`..b` was treated as a dot segment: {got}");
}

#[test]
fn climbing_past_the_root_clamps() {
    for input in ["C:/..", "C:/../..", "C:/a/../../.."] {
        let got = render(&lexically_normalize(Path::new(input)));
        assert!(got.starts_with("C:"), "{input} escaped the drive: {got}");
    }
}

// ---------------------------------------------------------------------------
// Extended-length form (D29)
// ---------------------------------------------------------------------------

#[test]
fn extended_form_never_double_prefixes() {
    let once = to_extended(Path::new("C:/x"), Path::new("C:/")).expect("absolute");
    let twice = to_extended(Path::new(&once), Path::new("C:/")).expect("already extended");
    assert_eq!(once, twice, "prefix was applied twice");
}

#[test]
fn extended_form_needs_an_absolute_base_for_relative_input() {
    assert!(to_extended(Path::new("rel/x"), Path::new("also/relative")).is_none());
    assert!(to_extended(Path::new("rel/x"), Path::new("C:/base")).is_some());
}

#[test]
fn extended_form_resolves_dots_because_the_os_will_not() {
    // The whole hazard of D29: `\\?\` is passed verbatim to the object manager.
    let got = to_extended(Path::new("C:/a/b/../../c"), Path::new("C:/")).expect("absolute");
    assert_eq!(got.to_string_lossy(), r"\\?\C:\c");
    assert!(!got.to_string_lossy().contains(".."), "a .. survived into the \\\\?\\ form");
}

#[test]
fn extended_form_of_a_long_path_is_not_truncated() {
    // Past MAX_PATH, which is the reason D29 exists.
    let deep = format!("C:/{}", vec!["segment"; 60].join("/"));
    let got = to_extended(Path::new(&deep), Path::new("C:/")).expect("absolute");
    assert!(got.to_string_lossy().len() > 260, "path was not long enough to test");
    assert!(got.to_string_lossy().starts_with(r"\\?\C:\"));
}

// ---------------------------------------------------------------------------
// UNC
// ---------------------------------------------------------------------------

#[test]
fn unc_survives_render_and_round_trip() {
    for input in ["//server/share", r"\\server\share"] {
        assert_eq!(render(&accept_path(input)), "//server/share", "failed for {input}");
    }
    assert_eq!(to_backslash(&accept_path("//server/share/a/b")), r"\\server\share\a\b");
}

#[test]
fn unc_is_absolute_and_has_no_drive_spelling() {
    assert!(is_absolute(Path::new("//server/share")));
    // There is no `/u/...` form for a UNC path, so it must not be mangled into one.
    assert_eq!(to_unix(Path::new("//server/share")), "//server/share");
}

// ---------------------------------------------------------------------------
// Devices and reserved names (D7, D28)
// ---------------------------------------------------------------------------

#[test]
fn dev_paths_are_matched_exactly() {
    assert_eq!(accept("/dev/null"), Target::Null);
    // Near-misses are ordinary paths, not devices.
    for near in ["/dev/nullx", "/dev/nul", "/dev/NULL", "dev/null", "/dev"] {
        assert!(
            matches!(accept(near), Target::Path(_)),
            "{near} was treated as a device"
        );
    }
}

#[test]
fn dev_fd_parses_only_real_numbers() {
    assert_eq!(accept("/dev/fd/0"), Target::Fd(0));
    assert_eq!(accept("/dev/fd/255"), Target::Fd(255));
    for bad in ["/dev/fd/", "/dev/fd/x", "/dev/fd/-1", "/dev/fd/1x"] {
        assert!(matches!(accept(bad), Target::Path(_)), "{bad} parsed as an fd");
    }
}

#[test]
fn reserved_names_are_detected_with_and_without_extensions() {
    for name in ["nul", "NUL", "Nul", "con", "aux", "prn", "com1", "LPT9"] {
        assert!(is_reserved_name(Path::new(name)), "{name} not detected");
        assert!(
            is_reserved_name(Path::new(&format!("{name}.txt"))),
            "{name}.txt not detected — reserved with any extension"
        );
        assert!(
            is_reserved_name(Path::new(&format!("C:/dir/{name}"))),
            "{name} not detected in a directory"
        );
    }
}

#[test]
fn names_that_merely_start_like_a_device_are_not_reserved() {
    for name in ["nulls", "console", "com", "com10", "lpt", "prnt", "auxiliary"] {
        assert!(!is_reserved_name(Path::new(name)), "{name} wrongly detected");
    }
}

// ---------------------------------------------------------------------------
// Unicode and spaces
// ---------------------------------------------------------------------------

#[test]
fn spaces_and_unicode_survive_every_conversion() {
    let path = "C:/Program Files/Ünïcodé Ordner/файл.txt";
    let accepted = accept_path(path);
    assert_eq!(render(&accepted), path);
    assert_eq!(to_backslash(&accepted), r"C:\Program Files\Ünïcodé Ordner\файл.txt");
    assert_eq!(to_unix(&accepted), "/c/Program Files/Ünïcodé Ordner/файл.txt");
    assert_eq!(render(&accept_path(&to_unix(&accepted))), path, "round trip lost data");
}

#[test]
fn tmp_mapping_handles_a_bare_prefix_correctly() {
    // `/tmpfoo` is not under `/tmp`.
    let Target::Path(bare) = accept("/tmpfoo") else {
        panic!("should be a path");
    };
    assert_eq!(render(&bare), "/tmpfoo", "/tmpfoo was rewritten as if under /tmp");

    let Target::Path(real) = accept("/tmp/x") else {
        panic!("should be a path");
    };
    assert!(is_absolute(&real), "/tmp/x did not resolve: {}", render(&real));
}

#[test]
fn the_tmp_spelling_is_also_diagnosable() {
    // `/tmp/x` resolves for operations cash performs but not for a command handed it —
    // including a bundled builtin, which opens paths directly. Same cliff as `/c/x`.
    assert!(unix_drive_spelling("/tmp/x").is_some());
    assert!(unix_drive_spelling("/tmp").is_some());

    // A directory that merely starts with the letters is not /tmp.
    assert_eq!(unix_drive_spelling("/tmpfoo"), None);
    assert_eq!(unix_drive_spelling("/tmpfoo/x"), None);
}
