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

#[test]
fn unixpaths_renders_a_drive_as_git_bash_does() {
    // D82: `/c/dir` for `C:/dir`, the drive's letter in lower case, its root `/c`, as
    // Git Bash's `pwd` prints them; what has no drive is `render`'s.
    let unix = |text: &str| cash_win32::path::to_unix(Path::new(text));
    assert_eq!(unix(r"C:\Users\me"), "/c/Users/me");
    assert_eq!(unix("d:/data/x"), "/d/data/x");
    assert_eq!(unix("C:/"), "/c");
    assert_eq!(unix("C:"), "/c");
    assert_eq!(unix("//server/share/x"), "//server/share/x");
    assert_eq!(unix("/dev/null"), "/dev/null");
    assert_eq!(unix("relative/x"), "relative/x");
}

#[test]
fn unixpaths_gives_a_windows_program_its_paths_as_msys_does() {
    // D82: a whole word `/c/x` and the value of `--opt=/c/x` or `NAME=/c/x` become `C:/x`;
    // `//c/x` is a literal `/c/x`; nothing else is touched, so `cmd /c` and `/a/` survive.
    let arg = cash_win32::path::windows_argument;
    assert_eq!(arg("/c/Users/me").as_deref(), Some("C:/Users/me"));
    assert_eq!(arg("--file=/d/data/x").as_deref(), Some("--file=D:/data/x"));
    assert_eq!(arg("HOME=/c/Users/me").as_deref(), Some("HOME=C:/Users/me"));
    assert_eq!(arg("//c/literal").as_deref(), Some("/c/literal"));
    assert_eq!(arg("/c"), None);
    assert_eq!(arg("/q"), None);
    assert_eq!(arg("/a/"), None);
    assert_eq!(arg("x=/notadrive/y"), None);
    assert_eq!(arg("a b=/c/x"), None);
    assert_eq!(arg("C:/Users/me"), None);
    assert_eq!(arg("plain"), None);
    assert!(arg("/tmp/x").is_some_and(|x| x.ends_with("/x") && !x.starts_with("/tmp")));
}

#[test]
fn a_tool_opens_a_unix_spelling_on_any_drive_and_leaves_the_rest_as_written() {
    // What sed, awk, diff and cmp open: `/d/x` is drive D, as in Git Bash, whether or not
    // the drive is there; a Windows spelling keeps its backslashes, so that the tool says of
    // it what it was given; `/c` alone and a relative name are no drive.
    let open = cash_win32::path::tool_path;
    assert_eq!(open("/c/a/f.txt"), PathBuf::from("C:/a/f.txt"));
    assert_eq!(open("/d/data/x"), PathBuf::from("D:/data/x"));
    assert_eq!(open("/e/x"), PathBuf::from("E:/x"));
    assert_eq!(open("/z/net/share.txt"), PathBuf::from("Z:/net/share.txt"));
    assert_eq!(open(r"D:\sdk\x"), PathBuf::from(r"D:\sdk\x"));
    assert_eq!(open("/c"), PathBuf::from("/c"));
    assert_eq!(open("rel/x"), PathBuf::from("rel/x"));
    assert!(!open("/tmp/x").starts_with("/tmp"));
}

#[test]
fn the_windows_directory_is_asked_of_windows_on_whichever_drive_it_is() {
    // Never assumed to be `C:\Windows`: System32 is in it, and the system drive's root is
    // its drive's.
    let windows = cash_win32::fs::windows_directory();
    assert!(is_absolute(windows), "{}", windows.display());
    assert!(windows.join("System32").is_dir(), "{}", windows.display());
    let root = cash_win32::fs::system_drive_root();
    assert!(
        windows.starts_with(&root),
        "{} {}",
        windows.display(),
        root.display()
    );
    assert!(root.to_string_lossy().ends_with('\\'), "{}", root.display());
    assert_eq!(
        cash_win32::fs::system_program("cmd.exe"),
        windows.join("System32").join("cmd.exe")
    );
}
