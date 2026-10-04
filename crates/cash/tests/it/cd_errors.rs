//! `cd`'s failures, worded as bash words them.
//!
//! Every failed `cd` printed "cd: i/o error: The system cannot find the file specified.
//! (os error 2)", which names neither the directory nor the reason. That mattered most
//! for the commonest failure on Windows: an unquoted `cd C:\Users\me\src`, which bash's
//! lexer turns into `C:Usersmesrc` before `cd` sees it, so the message has to show that
//! word for the user to understand what happened.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::cash_command;

/// Standard error and the exit status of `script`, run as `bash -c` would be: with `$0`
/// `bash`, so its errors read as Bash's do, `bash: line 1: cd: …`.
fn stderr_and_status(script: &str) -> (String, i32) {
    let out = cash_command()
        .args(["-c", script, "bash"])
        .output()
        .expect("run cash");
    (
        String::from_utf8_lossy(&out.stderr).trim_end().to_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn a_missing_directory_is_named_with_bash_wording() {
    let (stderr, status) = stderr_and_status("cd no-such-directory-here");
    assert_eq!(
        stderr,
        "bash: line 1: cd: no-such-directory-here: No such file or directory"
    );
    assert_eq!(status, 1);
}

#[test]
fn a_file_is_not_a_directory() {
    let (stderr, status) = stderr_and_status("cd Cargo.toml");
    assert_eq!(stderr, "bash: line 1: cd: Cargo.toml: Not a directory");
    assert_eq!(status, 1);
}

#[test]
fn physical_mode_reports_the_same_way() {
    let (stderr, _) = stderr_and_status("cd -P no-such-directory-here");
    assert_eq!(
        stderr,
        "bash: line 1: cd: no-such-directory-here: No such file or directory"
    );
}

#[test]
fn a_path_that_lost_its_backslashes_gets_a_hint() {
    let (stderr, status) = stderr_and_status(r"cd C:\no\such\place");
    let mut lines = stderr.lines();
    assert_eq!(
        lines.next(),
        Some("bash: line 1: cd: C:nosuchplace: No such file or directory")
    );
    let hint = lines.next().unwrap_or_default();
    assert!(
        hint.starts_with("bash: line 1: cd: hint:") && hint.contains("C:/dir/sub"),
        "{stderr}"
    );
    assert_eq!(status, 1);
}

/// `CDPATH` is searched as Bash searches it: a non-empty entry's folder is printed, an
/// empty entry is the current folder, and `./name` is not looked up; it was ignored.
#[test]
fn cdpath_is_searched_as_bash_searches_it() {
    let scratch = crate::common::Scratch::new("cdpath");
    std::fs::create_dir_all(scratch.join("a").join("sub")).unwrap();
    std::fs::create_dir_all(scratch.join("here")).unwrap();
    let out = cash_command()
        .current_dir(scratch.path())
        .args([
            "-c",
            r#"CDPATH="$PWD/a"; cd sub; echo "[$PWD]"
               cd "$OLDPWD"; CDPATH=":$PWD/a"; cd here; echo "[${PWD##*/}]"
               cd ..; cd ./sub 2>/dev/null; echo "rc $?""#,
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 4, "{stdout}");
    assert!(lines[0].ends_with("/a/sub"), "{stdout}");
    assert_eq!(lines[1], format!("[{}]", lines[0]), "{stdout}");
    assert_eq!(lines[2..], ["[here]", "rc 1"], "{stdout}");
}

#[test]
fn an_ordinary_failure_gets_no_hint() {
    for script in [
        "cd C:/no/such/place",
        "cd nowhere",
        r"cd 'C:\no\such\place'",
    ] {
        let (stderr, _) = stderr_and_status(script);
        assert!(!stderr.contains("hint"), "{script}: {stderr}");
    }
}
