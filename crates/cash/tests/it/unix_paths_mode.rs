//! `shopt -s unixpaths` (D82): the paths the shell shows a script spelled `/c/…`, as Git
//! Bash spells them, so that a loop climbing `${dir%/*}` to the root ends; a Windows
//! program handed `C:/…` for them, as MSYS hands them; and the mode passed on to a cash
//! started under it. Beside it, what holds in either mode: cash's own tools read `/c/…`,
//! as an MSYS tool does, and a program named by a `/c/…` path runs. Every drive the same:
//! no test takes the scratch directory or Windows to be on `C:`.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::Path;
use std::process::Stdio;

use crate::common::{
    Scratch, cash_command, git_for_windows, unix_spelling as unix, windows_directory,
};

/// `C:/x`, cash's spelling.
fn windows(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// What `script` prints, run by cash in `dir` with `HOME` there.
fn run_in(dir: &Path, home: &Path, script: &str) -> (String, String) {
    let out = cash_command()
        .args(["--norc", "--noprofile", "-c", script])
        .current_dir(dir)
        .env("HOME", windows(home))
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    (
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
    )
}

#[test]
fn the_shell_shows_its_directories_as_git_bash_does() {
    let scratch = Scratch::new("unixpaths-shown");
    let home = scratch.join("home");
    let sub = scratch.join("sub");
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::create_dir_all(&sub).expect("create sub");
    let (out, err) = run_in(
        scratch.path(),
        &home,
        "shopt -s unixpaths; echo \"$PWD\"; pwd; pwd -W; echo \"$HOME\" ~ ~/x ~+; \
         cd sub; echo \"$OLDPWD\" ~-; cd - >/dev/null; cd ~; pushd \"$OLDPWD\" >/dev/null; \
         dirs -l; echo \"${DIRSTACK[@]}\"; PS1='\\w'; echo \"${PS1@P}\"",
    );
    let (top, home) = (unix(scratch.path()), unix(&home));
    let expected = [
        top.clone(),
        top.clone(),
        windows(scratch.path()),
        format!("{home} {home} {home}/x {top}"),
        format!("{top} {top}"),
        format!("{top} {home}"),
        format!("{top} {home}"),
        top,
    ]
    .join("\n");
    assert_eq!(out.trim_end(), expected, "standard error: {err}");
}

#[test]
fn turning_the_mode_off_spells_them_back() {
    let scratch = Scratch::new("unixpaths-toggle");
    let home = scratch.join("home");
    std::fs::create_dir_all(&home).expect("create home");
    let (out, err) = run_in(
        scratch.path(),
        &home,
        "shopt -s unixpaths; cd home; echo \"$PWD $OLDPWD $HOME\"; \
         shopt -u unixpaths; echo \"$PWD $OLDPWD $HOME\"; pwd",
    );
    let (top, home) = (scratch.path(), home.as_path());
    let expected = format!(
        "{} {} {}\n{} {} {}\n{}",
        unix(home),
        unix(top),
        unix(home),
        windows(home),
        windows(top),
        windows(home),
        windows(home),
    );
    assert_eq!(out.trim_end(), expected, "standard error: {err}");
}

#[test]
fn a_cash_started_under_the_mode_takes_it() {
    let scratch = Scratch::new("unixpaths-child");
    let (out, err) = run_in(
        scratch.path(),
        scratch.path(),
        "shopt -s unixpaths; bash -c 'echo \"$PWD\"; shopt -q unixpaths && echo on'; \
         shopt -u unixpaths; bash -c 'shopt -q unixpaths || echo off'",
    );
    assert_eq!(
        out.trim_end(),
        format!("{}\non\noff", unix(scratch.path())),
        "standard error: {err}"
    );

    let inherited = cash_command()
        .args(["--norc", "--noprofile", "-c", "echo \"$PWD $HOME\""])
        .current_dir(scratch.path())
        .env("HOME", windows(scratch.path()))
        .env("CASH_UNIXPATHS", "1")
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    let top = unix(scratch.path());
    assert_eq!(
        String::from_utf8_lossy(&inherited.stdout).trim_end(),
        format!("{top} {top}")
    );
}

#[test]
fn a_climb_to_the_root_ends() {
    let scratch = Scratch::new("unixpaths-climb");
    // The guard stops a loop that would never end; the count says it ended by itself.
    let (out, err) = run_in(
        scratch.path(),
        scratch.path(),
        "shopt -s unixpaths; d=$PWD; n=0; while [ -n \"$d\" ] && [ $n -lt 99 ]; do \
         d=${d%/*}; n=$((n+1)); done; echo $n; d=$PWD; n=0; \
         while [ \"$d\" != / ] && [ $n -lt 99 ]; do d=$(dirname \"$d\"); n=$((n+1)); done; \
         echo $n",
    );
    let depth = unix(scratch.path()).matches('/').count();
    assert_eq!(
        out.trim_end(),
        format!("{depth}\n{depth}"),
        "standard error: {err}"
    );
}

#[test]
fn a_windows_program_is_handed_windows_paths() {
    let scratch = Scratch::new("unixpaths-args");
    // A whole word and an `=` value become `X:/…`, on any drive, there or not; `/c` alone, a
    // root alone and `cmd`'s own switches go as written, and `//c/w` is MSYS's escape for a
    // literal `/c/w`.
    let (out, err) = run_in(
        scratch.path(),
        scratch.path(),
        "shopt -s unixpaths; cmd //c echo /c/x /d/y --opt=/e/z NAME=/q/v /c /d /a/ //c/w; \
         cmd //c 'echo %HOME%'",
    );
    assert_eq!(
        out.trim_end(),
        format!(
            "C:/x D:/y --opt=E:/z NAME=Q:/v /c /d /a/ /c/w\n{}",
            windows(scratch.path())
        ),
        "standard error: {err}"
    );
}

#[test]
fn an_msys_program_is_handed_unix_paths_as_written() {
    let scratch = Scratch::new("unixpaths-msys");
    let echo = format!("{}/usr/bin/echo.exe", git_for_windows());
    if !Path::new(&echo).exists() {
        return;
    }
    let (out, err) = run_in(
        scratch.path(),
        scratch.path(),
        &format!("shopt -s unixpaths; '{echo}' /c/x --opt=/c/y"),
    );
    assert_eq!(out.trim_end(), "/c/x --opt=/c/y", "standard error: {err}");
}

#[test]
fn globs_and_lookups_give_unix_paths() {
    let scratch = Scratch::new("unixpaths-globs");
    std::fs::write(scratch.join("f.txt"), "x\n").expect("write f.txt");
    let (out, err) = run_in(
        scratch.path(),
        scratch.path(),
        "shopt -s unixpaths; echo \"$PWD\"/f.t?t; echo f.t?t; which cmd; type -P cmd; \
         command -v cmd",
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some(format!("{}/f.txt", unix(scratch.path())).as_str()),
        "standard error: {err}"
    );
    assert_eq!(lines.get(1).copied(), Some("f.txt"));
    // cmd.exe in System32, on the drive Windows is on.
    let cmd =
        format!("{}/system32/cmd.exe", unix(Path::new(&windows_directory()))).to_ascii_lowercase();
    for line in lines.iter().skip(2) {
        assert_eq!(line.to_ascii_lowercase(), cmd, "{out}");
    }
    assert_eq!(lines.len(), 5, "{out}");
}

#[test]
fn cash_tools_read_unix_paths_in_either_mode() {
    let scratch = Scratch::new("unixpaths-tools");
    std::fs::write(scratch.join("f.txt"), "a b\n").expect("write f.txt");
    let f = format!("{}/f.txt", unix(scratch.path()));
    // Files given as `/c/…` open; a script or a pattern that looks like one is left alone,
    // and a tool that only handles text gives its words back as written.
    let script = format!(
        "cat {f}; head -n1 {f}; sed -n '/a/p' {f}; awk '/a/ {{ print $2 }}' {f}; \
         diff {f} {f} && echo same; cp {f} {f}.copy && cat f.txt.copy; dirname {f}; \
         basename {f}"
    );
    let expected = format!(
        "a b\na b\na b\nb\nsame\na b\n{}\nf.txt",
        unix(scratch.path())
    );
    for mode in ["", "shopt -s unixpaths; "] {
        let _ = std::fs::remove_file(scratch.join("f.txt.copy"));
        let (out, err) = run_in(scratch.path(), scratch.path(), &format!("{mode}{script}"));
        assert_eq!(out.trim_end(), expected, "{mode}standard error: {err}");
        assert!(err.is_empty(), "{mode}standard error: {err}");
    }
}

#[test]
fn realpath_and_mktemp_give_unix_paths_in_the_mode() {
    let scratch = Scratch::new("unixpaths-realpath");
    std::fs::write(scratch.join("f.txt"), "x\n").expect("write f.txt");
    let (out, err) = run_in(
        scratch.path(),
        scratch.path(),
        "shopt -s unixpaths; realpath f.txt; readlink -f f.txt; t=$(mktemp); \
         case $t in /c/*) echo unix;; *) echo \"$t\";; esac; rm \"$t\"",
    );
    // `realpath` answers with the long name, where CI's temporary directory is the short
    // `RUNNER~1`.
    let long = std::fs::canonicalize(scratch.path()).expect("canonicalize the scratch");
    let f = format!("{}/f.txt", unix(&long));
    assert_eq!(
        out.trim_end(),
        format!("{f}\n{f}\nunix"),
        "standard error: {err}"
    );
}

#[test]
fn a_program_named_by_a_unix_path_runs() {
    let scratch = Scratch::new("unixpaths-exec");
    std::fs::write(scratch.join("t.sh"), "#!/bin/sh\necho ran\n").expect("write t.sh");
    let t = format!("{}/t.sh", unix(scratch.path()));
    // By `exec` and by the bundled `env`, which had the name as written and Windows no `/c`.
    let script = format!("{t}; (exec {t}); env X=1 {t}");
    for mode in ["", "shopt -s unixpaths; "] {
        let (out, err) = run_in(scratch.path(), scratch.path(), &format!("{mode}{script}"));
        assert_eq!(
            out.trim_end(),
            "ran\nran\nran",
            "{mode}standard error: {err}"
        );
    }
}
