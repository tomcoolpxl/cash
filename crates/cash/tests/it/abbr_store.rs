//! Abbreviations kept across sessions in `%APPDATA%\cash\abbreviations`: written by
//! `abbr -a`, `-e` and `-r`, read by an interactive shell after its rc files. Each test
//! gives cash an `APPDATA` of its own.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::process::Stdio;

use crate::common::{Output, Scratch, cash_command, output_of};

/// Runs `script` in a shell with `APPDATA` at `appdata`; `interactive` adds `-i`, so
/// the kept abbreviations are read, with `--norc` unless an rc file is given.
fn cash(appdata: &Scratch, interactive: bool, rc: Option<&str>, script: &str) -> Output {
    let mut command = cash_command();
    command.env("APPDATA", appdata.path()).stdin(Stdio::null());
    match rc {
        Some(rc) => command.args(["--rcfile", rc]),
        None => command.arg("--norc"),
    };
    command.arg("--noprofile");
    if interactive {
        command.args(["-ic", script]);
    } else {
        command.args(["-c", script]);
    }
    output_of(&mut command)
}

/// The file's text, LF-separated as it is written.
fn file(appdata: &Scratch) -> String {
    std::fs::read_to_string(appdata.join("cash").join("abbreviations")).unwrap_or_default()
}

#[test]
fn abbr_a_once_is_enough_for_every_interactive_shell_after() {
    let appdata = Scratch::new("abbr-store-add");
    let out = cash(
        &appdata,
        false,
        None,
        "abbr -a gco git checkout; abbr -a --position anywhere L '| less'",
    );
    assert_eq!((out.code, out.stderr.as_str()), (0, ""));
    assert_eq!(file(&appdata), "gco=git checkout\nanywhere L=| less\n");
    assert!(!file(&appdata).contains('\r'), "the file is LF-separated");

    let out = cash(&appdata, true, None, "abbr");
    assert_eq!(out.stderr, "");
    assert_eq!(
        out.stdout,
        "abbr -a -- gco 'git checkout'\nabbr -a --position anywhere -- L '| less'"
    );

    // A shell that runs a script does not read them: nothing expands there.
    let out = cash(&appdata, false, None, "abbr");
    assert_eq!((out.stdout.as_str(), out.stderr.as_str()), ("", ""));
}

#[test]
fn erase_and_rename_reach_the_file_too() {
    let appdata = Scratch::new("abbr-store-erase");
    cash(
        &appdata,
        false,
        None,
        "abbr -a gco git checkout; abbr -a gst git status",
    );
    let out = cash(&appdata, true, None, "abbr -e gco; abbr -r gst st");
    assert_eq!((out.code, out.stderr.as_str()), (0, ""));
    assert_eq!(file(&appdata), "st=git status\n");
    let out = cash(&appdata, true, None, "abbr -l");
    assert_eq!(out.stdout, "st");
}

#[test]
fn the_rc_file_wins_for_the_same_name_and_the_file_keeps_the_rest() {
    let appdata = Scratch::new("abbr-store-rc");
    cash(
        &appdata,
        false,
        None,
        "abbr -a gco git checkout; abbr -a gst git status",
    );
    let rc = appdata.join("rc.sh");
    std::fs::write(&rc, "abbr -a gco git switch\n").unwrap();
    let out = cash(&appdata, true, Some(&rc.to_string_lossy()), "abbr");
    assert_eq!(out.stderr, "");
    assert_eq!(
        out.stdout,
        "abbr -a -- gco 'git switch'\nabbr -a -- gst 'git status'"
    );
    // The rc file's definition was merged into the file, and `gst` was not lost to it.
    assert_eq!(file(&appdata), "gco=git switch\ngst=git status\n");
}

#[test]
fn a_file_that_does_not_parse_is_said_once_and_ignored() {
    let appdata = Scratch::new("abbr-store-bad");
    std::fs::create_dir_all(appdata.join("cash")).unwrap();
    std::fs::write(
        appdata.join("cash").join("abbreviations"),
        "gco=git checkout\nnonsense\n",
    )
    .unwrap();
    let out = cash(&appdata, true, None, "abbr; echo st=$?");
    assert_eq!(out.stdout, "st=0");
    assert_eq!(
        out.stderr,
        format!(
            "cash: the saved abbreviations are ignored: {}/cash/abbreviations: line 2: not NAME=EXPANSION",
            appdata.as_script_path()
        )
    );
    // Adding one writes the file over, since its lines were refused already.
    cash(&appdata, false, None, "abbr -a gst git status");
    assert_eq!(file(&appdata), "gst=git status\n");
}

#[test]
fn without_appdata_nothing_is_kept_and_nothing_is_said() {
    let out = output_of(
        cash_command()
            .env_remove("APPDATA")
            .args([
                "--norc",
                "--noprofile",
                "-c",
                "abbr -a gco git checkout; abbr -l",
            ])
            .stdin(Stdio::null()),
    );
    assert_eq!(
        (out.code, out.stdout.as_str(), out.stderr.as_str()),
        (0, "gco", "")
    );
}
