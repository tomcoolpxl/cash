//! Programs started from a folder whose path is too long for Windows to start a process
//! in (more than 258 characters, `cash_win32::path::MAX_PROCESS_DIRECTORY`).
//!
//! cash's builtins worked there, but every program failed, its bundled tools included, and
//! said `C:\…\cash.exe: Not a directory`. The program is now started in the folder's 8.3
//! short name, and where there is none, the error says why (the user, 2026-10-04).

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, run_in};

/// A script that makes a folder 300 characters below `$PWD`, `$p`, and goes into it.
const INTO_A_LONG_FOLDER: &str = r#"seg=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
p=$PWD; for i in 1 2 3 4 5 6; do p=$p/$seg$i; done
mkdir -p "$p" && cd "$p" || exit 99
"#;

#[test]
fn programs_start_in_a_folder_too_long_for_windows() {
    let scratch = Scratch::new("long-folder");
    let out = run_in(
        scratch.path(),
        &format!(
            "{INTO_A_LONG_FOLDER}echo hi > f.txt
            sort f.txt | wc -l
            cmd /c type f.txt
            echo \"${{#p}}\""
        ),
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    let lines: Vec<&str> = out.stdout.lines().map(str::trim).collect();
    assert_eq!(lines[..2], ["1", "hi"], "{}", out.stderr);
    assert!(lines[2].parse::<usize>().unwrap() > 300, "{}", out.stdout);
}

#[test]
fn a_folder_too_long_without_a_short_name_says_why() {
    // A folder deleted under the shell has no short name.
    let scratch = Scratch::new("long-folder-gone");
    let out = run_in(
        scratch.path(),
        &format!(
            "{INTO_A_LONG_FOLDER}top=$(cd \"$p\"/../../../../../..; pwd)
            (cd /; rm -rf \"$top\")
            cmd /c echo hi; echo \"rc=$?\""
        ),
    );
    assert!(
        out.stderr
            .contains("cmd: the working directory is too long for Windows to start a program in"),
        "{}",
        out.stderr
    );
    assert_eq!(out.stdout, "rc=126");
}
