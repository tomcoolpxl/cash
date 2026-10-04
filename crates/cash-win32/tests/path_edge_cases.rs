//! Edge cases for the path model (D3, D7, D28, D29).
//!
//! `path_model.rs` covers the happy paths. These are the inputs that a shell actually
//! receives and that break naive implementations: empty strings, trailing separators,
//! mixed separators, relative paths that look absolute, case, unicode, and the
//! boundaries of the drive-letter rule.

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

use cash_win32::path::{
    MAX_PROCESS_DIRECTORY, Target, accept, accept_path, is_absolute, is_on_network,
    is_reserved_name, process_directory, render, to_backslash, to_unix, unix_drive_spelling,
};

// ---------------------------------------------------------------------------
// Degenerate inputs
// ---------------------------------------------------------------------------

#[test]
fn a_lone_separator_is_not_a_drive() {
    assert_eq!(unix_drive_spelling("/"), None);
    assert_eq!(unix_drive_spelling("//"), None);
    assert_eq!(
        unix_drive_spelling("/1/x"),
        None,
        "digits are not drive letters"
    );
    assert_eq!(unix_drive_spelling("/./x"), None);
}

#[test]
fn a_drive_letter_needs_a_following_slash() {
    // The regression that produced a spurious hint on `cmd.exe /d /s /c`.
    for flag in ["/c", "/d", "/s", "/q", "/Z"] {
        assert_eq!(
            unix_drive_spelling(flag),
            None,
            "{flag} should not look like a drive"
        );
    }
    for path in ["/c/", "/c/x", "/d/data"] {
        assert!(
            unix_drive_spelling(path).is_some(),
            "{path} should look like a drive"
        );
    }
}

// ---------------------------------------------------------------------------
// Separators
// ---------------------------------------------------------------------------

#[test]
fn mixed_separators_normalise() {
    assert_eq!(
        render(&accept_path(r"C:\Users/thraa\docs")),
        "C:/Users/thraa/docs"
    );
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
    assert_eq!(
        render(&accept_path("c:/Users/ThRaa/MyFile.TXT")),
        "C:/Users/ThRaa/MyFile.TXT"
    );
}

// ---------------------------------------------------------------------------
// UNC
// ---------------------------------------------------------------------------

#[test]
fn unc_survives_render_and_round_trip() {
    for input in ["//server/share", r"\\server\share"] {
        assert_eq!(
            render(&accept_path(input)),
            "//server/share",
            "failed for {input}"
        );
    }
    assert_eq!(
        to_backslash(&accept_path("//server/share/a/b")),
        r"\\server\share\a\b"
    );
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
    for near in [
        "/dev/nullx",
        "/dev/nul",
        "/dev/NULL",
        "dev/null",
        "/dev",
        "/dev/",
        "/dev/null/x",
        "/dev/stdin/0",
        "/dev/../dev/null",
    ] {
        assert!(
            matches!(accept(near), Target::Path(_)),
            "{near} was treated as a device"
        );
    }
}

#[test]
fn a_device_is_named_from_the_root_only() {
    // The name is all there is to go on, so a folder called `dev` must not be enough:
    // `cd ~/dev; echo x > null` writes a file, and so does naming that file in full.
    for file in [
        "dev/null",
        "./dev/null",
        "a/dev/null",
        "C:/dev/null",
        "/c/dev/null",
        "C:/Users/me/dev/null",
        "dev/stdout",
        "C:/dev/stdin",
        "C:/dev/fd/1",
        // Two leading slashes are a UNC path: the server `dev`.
        "//dev/null",
        "//dev/stdin",
    ] {
        assert!(
            matches!(accept(file), Target::Path(_)),
            "{file} was treated as a device"
        );
    }
}

#[test]
fn a_device_name_may_be_spelled_as_a_filesystem_would_read_it() {
    // `"$dir/null"` with `dir=/dev/` is `/dev//null`, and Linux opens the device.
    for null in [
        "/dev//null",
        "/dev/./null",
        "/./dev/null",
        "///dev/null",
        "/dev/null/",
        r"\dev\null",
    ] {
        assert_eq!(accept(null), Target::Null, "{null} is /dev/null");
    }
    assert_eq!(accept("/dev//stdin"), Target::Stdin);
    assert_eq!(accept("/dev/./stdout"), Target::Stdout);
    assert_eq!(accept(r"\dev\stderr"), Target::Stderr);
    assert_eq!(accept("/dev/fd//3"), Target::Fd(3));
}

#[test]
fn dev_fd_parses_only_real_numbers() {
    assert_eq!(accept("/dev/fd/0"), Target::Fd(0));
    assert_eq!(accept("/dev/fd/255"), Target::Fd(255));
    for bad in [
        "/dev/fd",
        "/dev/fd/",
        "/dev/fd/x",
        "/dev/fd/-1",
        "/dev/fd/+1",
        "/dev/fd/1x",
        "/dev/fd/1/2",
        "/dev/fd/99999999999",
    ] {
        assert!(
            matches!(accept(bad), Target::Path(_)),
            "{bad} parsed as an fd"
        );
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
    for name in [
        "nulls",
        "console",
        "com",
        "com10",
        "lpt",
        "prnt",
        "auxiliary",
    ] {
        assert!(
            !is_reserved_name(Path::new(name)),
            "{name} wrongly detected"
        );
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
    assert_eq!(
        to_backslash(&accepted),
        r"C:\Program Files\Ünïcodé Ordner\файл.txt"
    );
    assert_eq!(
        to_unix(&accepted),
        "/c/Program Files/Ünïcodé Ordner/файл.txt"
    );
    assert_eq!(
        render(&accept_path(&to_unix(&accepted))),
        path,
        "round trip lost data"
    );
}

#[test]
fn tmp_mapping_handles_a_bare_prefix_correctly() {
    // `/tmpfoo` is not under `/tmp`.
    let Target::Path(bare) = accept("/tmpfoo") else {
        panic!("should be a path");
    };
    assert_eq!(
        render(&bare),
        "/tmpfoo",
        "/tmpfoo was rewritten as if under /tmp"
    );

    let Target::Path(real) = accept("/tmp/x") else {
        panic!("should be a path");
    };
    assert!(
        is_absolute(&real),
        "/tmp/x did not resolve: {}",
        render(&real)
    );
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

// ---------------------------------------------------------------------------
// A program's working directory (process_directory)
// ---------------------------------------------------------------------------

/// A folder under `%TEMP%` whose path is longer than Windows will start a program in.
fn long_folder(name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir().join(format!("cash-long-{name}-{}", std::process::id()));
    for i in 0..6 {
        dir.push(format!("{}{i}", "a".repeat(50)));
    }
    assert!(dir.to_string_lossy().len() > MAX_PROCESS_DIRECTORY);
    dir
}

#[test]
fn a_folder_that_fits_is_left_as_it_is() {
    let dir = Path::new("C:/Users/someone/project");
    assert_eq!(process_directory(dir).unwrap(), dir);
}

#[test]
fn a_folder_too_long_is_given_by_its_short_name() {
    let dir = long_folder("short");
    std::fs::create_dir_all(&dir).unwrap();
    let short = process_directory(&dir);
    let top = dir.ancestors().nth(6).unwrap().to_path_buf();
    let same = short
        .as_ref()
        .ok()
        .map(|short| std::fs::canonicalize(short).unwrap() == std::fs::canonicalize(&dir).unwrap());
    let _ = std::fs::remove_dir_all(&top);

    let short = short.expect("a short name (8.3 names are on for the system drive)");
    assert!(
        short.to_string_lossy().len() <= MAX_PROCESS_DIRECTORY,
        "{}",
        short.display()
    );
    assert_eq!(same, Some(true), "{} is another folder", short.display());
}

#[test]
fn a_folder_too_long_without_a_short_name_says_why() {
    // A folder that does not exist has no short name either.
    let error = process_directory(&long_folder("missing")).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("too long for Windows to start a program in"),
        "{error}"
    );
}

#[test]
fn network_paths_are_unc_and_mapped_drives_only() {
    for unc in [
        "//server/share/x",
        r"\\server\share\x",
        r"\\?\UNC\server\share\x",
    ] {
        assert!(is_on_network(Path::new(unc)), "{unc}");
    }
    for local in [r"\\?\C:\x", r"\\.\pipe\x", "C:/Windows", "/tmp", "rel/x"] {
        assert!(!is_on_network(Path::new(local)), "{local}");
    }
}
