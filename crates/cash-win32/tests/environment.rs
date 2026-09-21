//! D5 (PATH is the one translated variable) and D31 (case-insensitive lookup).

#![cfg(windows)]

use cash_win32::env::{Environment, canonical_name, path_to_unix, path_to_windows, split_path};

#[test]
fn lookup_ignores_case() {
    // D31. §9 measured brush returning empty for $Path while $PATH worked.
    let mut env = Environment::new();
    env.set("Path", "C:/tools");

    assert_eq!(env.get("PATH"), Some("C:/tools"));
    assert_eq!(env.get("Path"), Some("C:/tools"));
    assert_eq!(env.get("path"), Some("C:/tools"));
    assert!(env.contains("pAtH"));
}

#[test]
fn posix_names_canonicalise_to_uppercase() {
    // So a script asking for $PATH sees it however Windows spelled it.
    assert_eq!(canonical_name("Path"), "PATH");
    assert_eq!(canonical_name("Temp"), "TEMP");
    assert_eq!(canonical_name("UserProfile"), "USERPROFILE");

    // Names cash does not know keep their spelling: inventing one would be guessing.
    assert_eq!(canonical_name("MyAppSetting"), "MyAppSetting");
    assert_eq!(canonical_name("GOPATH"), "GOPATH");
}

#[test]
fn setting_one_spelling_overwrites_the_other() {
    // On Windows these are genuinely the same variable, so cash must not hold two.
    let mut env = Environment::new();
    env.set("Path", "first");
    env.set("PATH", "second");

    assert_eq!(env.get("path"), Some("second"));
    assert_eq!(env.iter().filter(|(k, _)| k.eq_ignore_ascii_case("PATH")).count(), 1);
}

#[test]
fn scripts_see_path_in_unix_form() {
    // D5: so that `IFS=: read -ra dirs <<< "$PATH"` works.
    let mut env = Environment::new();
    env.set("PATH", r"C:\tools;C:\Windows\System32");

    let seen = env.get_for_script("PATH").expect("PATH is set");
    assert_eq!(seen, "/c/tools:/c/Windows/System32");
    assert!(!seen.contains(';'), "a script must never see semicolons in PATH");
}

#[test]
fn children_get_path_in_windows_form() {
    // D5: so that terraform.exe and git.exe understand it.
    let mut env = Environment::new();
    env.set("PATH", "/c/tools:/c/Windows");

    let block = env.to_child_block();
    let (_, path) = block.iter().find(|(k, _)| k == "PATH").expect("PATH present");
    assert_eq!(path, r"C:\tools;C:\Windows");
}

#[test]
fn no_other_variable_is_translated() {
    // D5 is explicit that a known-list would be a maintenance surface and a source of
    // silent surprise. GOPATH and friends pass through byte-for-byte.
    let mut env = Environment::new();
    env.set("GOPATH", r"C:\go;C:\go2");
    env.set("PYTHONPATH", "/c/py:/c/py2");
    env.set("CLASSPATH", r"C:\a;C:\b");

    assert_eq!(env.get_for_script("GOPATH").unwrap(), r"C:\go;C:\go2");
    assert_eq!(env.get_for_script("PYTHONPATH").unwrap(), "/c/py:/c/py2");

    let block = env.to_child_block();
    for name in ["GOPATH", "PYTHONPATH", "CLASSPATH"] {
        let (_, value) = block.iter().find(|(k, _)| k == name).unwrap();
        assert_eq!(value, env.get(name).unwrap(), "{name} must pass through verbatim");
    }
}

#[test]
fn path_round_trips_through_both_forms() {
    let windows = r"C:\tools;D:\sdk\bin;C:\Windows\System32";
    let unix = path_to_unix(windows);
    assert_eq!(unix, "/c/tools:/d/sdk/bin:/c/Windows/System32");
    assert_eq!(path_to_windows(&unix), windows);
}

#[test]
fn splitting_a_unix_path_does_not_break_on_drive_letters() {
    // The subtle case: a script is free to write PATH=C:/tools:$PATH, mixing forms.
    // Splitting naively on ':' would produce "C" and "/tools" as separate entries.
    let mixed = "C:/tools:/c/Windows:D:/sdk";
    let entries: Vec<&str> = split_path(mixed).collect();
    assert_eq!(entries, vec!["C:/tools", "/c/Windows", "D:/sdk"]);
}

#[test]
fn splitting_handles_both_separators() {
    assert_eq!(split_path("C:/a;C:/b").collect::<Vec<_>>(), vec!["C:/a", "C:/b"]);
    assert_eq!(split_path("/c/a:/c/b").collect::<Vec<_>>(), vec!["/c/a", "/c/b"]);
}

#[test]
fn empty_path_entries_are_dropped() {
    // Trailing separators are common in real PATHs and must not produce empty entries,
    // which would otherwise mean "search the current directory" — a security problem.
    assert_eq!(split_path("C:/a;;C:/b;").collect::<Vec<_>>(), vec!["C:/a", "C:/b"]);
    assert_eq!(split_path("/c/a::/c/b").collect::<Vec<_>>(), vec!["/c/a", "/c/b"]);
}

#[test]
fn the_real_process_environment_has_a_usable_path() {
    // Guards the actual D31 failure mode: on a Windows-supplied block spelled `Path`,
    // a case-sensitive shell finds nothing.
    let env = Environment::from_process();
    let path = env.get("PATH").expect("PATH must resolve regardless of Windows' spelling");
    assert!(!path.is_empty());

    let for_script = env.get_for_script("PATH").unwrap();
    assert!(for_script.starts_with('/'), "script form should be Unix-style, got {for_script}");
}
