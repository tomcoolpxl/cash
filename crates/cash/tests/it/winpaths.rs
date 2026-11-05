//! `shopt winpaths` — **D53**: an unquoted `C:\...` means the path it spells.
//!
//! A backslash is bash's escape character, so `cd C:\Users\me` reaches `cd` as
//! `C:Usersme`, and a path pasted from Explorer or a PowerShell prompt fails. With
//! `winpaths`, a word that starts with a drive and a backslash keeps its backslashes
//! wherever bash's escape would only have eaten them. It is on by default at the
//! interactive prompt and off in scripts, which keep bash's lexing exactly.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::needless_raw_string_hashes,
    clippy::panic_in_result_fn,
    reason = "an integration test is outside a test module by construction, and an async 
              test returns a Result so that setup can use ?"
)]

use std::process::Stdio;

use crate::common::{Scratch, cash_command};

fn cash(args: &[&str]) -> String {
    let out = cash_command()
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    String::from_utf8_lossy(&out.stdout).trim_end().to_owned()
}

/// What `printf '[%s]\n'` prints for `words` with `winpaths` on.
fn words_with_winpaths(words: &str) -> String {
    cash(&[
        "-c",
        &format!(r#"shopt -s winpaths; printf '[%s]\n' {words}"#),
    ])
}

#[test]
fn scripts_keep_bash_lexing() {
    assert_eq!(cash(&["-c", "shopt -q winpaths || echo off"]), "off");
    assert_eq!(
        cash(&["-c", r#"printf '[%s]\n' C:\Users\me"#]),
        "[C:Usersme]"
    );
}

#[test]
fn the_interactive_prompt_has_it_on() {
    // `--norc`: an interactive shell reads the runner's own ~/.bashrc, whose banner or
    // prompt would land in the output.
    assert_eq!(
        cash(&[
            "--norc",
            "-i",
            "-c",
            r#"shopt -q winpaths && printf '[%s]\n' C:\Users\me"#
        ]),
        r"[C:\Users\me]"
    );
}

#[test]
fn a_drive_path_keeps_its_backslashes() {
    assert_eq!(
        words_with_winpaths(r"C:\Users\me\src D:\a-b\c_d\e.f\1 C:\Users\Émile"),
        "[C:\\Users\\me\\src]\n[D:\\a-b\\c_d\\e.f\\1]\n[C:\\Users\\Émile]"
    );
}

#[test]
fn names_bash_would_only_have_unescaped_are_kept() {
    assert_eq!(
        words_with_winpaths(r"C:\$Recycle.Bin C:\ProgramData\{A1-B2}\x C:\@w\%t%\[o]"),
        "[C:\\$Recycle.Bin]\n[C:\\ProgramData\\{A1-B2}\\x]\n[C:\\@w\\%t%\\[o]]"
    );
}

#[test]
fn a_unc_path_keeps_its_backslashes_the_leading_two_included() {
    assert_eq!(
        words_with_winpaths(r"\\server\share\dir \\wsl$\Ubuntu\home \\ x"),
        "[\\\\server\\share\\dir]\n[\\\\wsl$\\Ubuntu\\home]\n[\\]\n[x]"
    );
    // Without the option, bash's reading.
    assert_eq!(
        cash(&["-c", r"printf '[%s]\n' \\server\share"]),
        r"[\servershare]"
    );
}

#[test]
fn cd_follows_a_pasted_unc_path() {
    let here = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .replace('/', r"\");
    let (drive, rest) = here.split_at(2);
    let unc = format!(r"\\localhost\{}${rest}", drive.get(..1).unwrap());
    assert_eq!(
        cash(&["-c", &format!("shopt -s winpaths; cd {unc} && pwd")]),
        unc.replace('\\', "/")
    );
}

#[test]
fn an_escaped_space_still_joins_the_word() {
    assert_eq!(
        words_with_winpaths(r"C:\Program\ Files\Git"),
        r"[C:\Program Files\Git]"
    );
}

#[test]
fn a_wildcard_after_a_backslash_still_globs() {
    let dir = Scratch::new("winpaths-glob");
    std::fs::write(dir.join("a.txt"), "").unwrap();
    std::fs::write(dir.join("b.txt"), "").unwrap();
    std::fs::write(dir.join("c.log"), "").unwrap();

    let spelled = dir.path().to_string_lossy().replace('/', r"\");
    let listed = words_with_winpaths(&format!(r"{spelled}\*.txt"));

    let names: Vec<&str> = listed
        .lines()
        .map(|line| line.rsplit(['/', '\\']).next().unwrap_or(line))
        .collect();
    assert_eq!(names, ["a.txt]", "b.txt]"], "{listed}");
}

#[test]
fn quotes_and_expansions_keep_their_meaning() {
    // After the leading unquoted run nothing is reinterpreted, and quoted text was
    // already literal.
    assert_eq!(
        words_with_winpaths(r#"C:\a"\b" 'C:\q\r' x=C:\y C:/fwd"#),
        "[C:\\a\\b]\n[C:\\q\\r]\n[x=C:y]\n[C:/fwd]"
    );
}

#[test]
fn cd_follows_a_pasted_path() {
    let here = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .replace('/', r"\");
    assert_eq!(
        cash(&["-c", &format!("shopt -s winpaths; cd {here} && pwd")]),
        here.replace('\\', "/")
    );
}

#[test]
fn it_can_be_turned_off_at_the_prompt() {
    assert_eq!(
        cash(&[
            "--norc",
            "-i",
            "-c",
            r#"shopt -u winpaths; printf '[%s]\n' C:\Users\me"#
        ]),
        "[C:Usersme]"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_backslashed_drive_path_completes() -> anyhow::Result<()> {
    use cash_builtins::ShellBuilderExt as _;

    let mut shell = cash_core::Shell::builder()
        .profile(cash_core::ProfileLoadBehavior::Skip)
        .rc(cash_core::RcLoadBehavior::Skip)
        .default_builtins(cash_builtins::BuiltinSet::BashMode)
        .build()
        .await?;
    shell.options_mut().windows_drive_paths = true;

    let dir = Scratch::new("winpaths-complete");
    std::fs::create_dir_all(dir.join("alpha-dir"))?;

    let line = format!(
        r"ls {}\alp",
        dir.path().to_string_lossy().replace('/', r"\")
    );
    let completions = shell.complete(&line, line.len()).await?;

    assert!(
        completions
            .candidates
            .iter()
            .any(|candidate| candidate.trim_end_matches('/').ends_with("alpha-dir")),
        "{line} completed to {:?}",
        completions.candidates
    );
    Ok(())
}

/// A scratch directory with `dist/windows` in it and a file `ab`, and what `script` prints
/// there with `winpaths` on.
fn relative_paths_scratch() -> Scratch {
    let dir = Scratch::new("winpaths-relative");
    std::fs::create_dir_all(dir.join("dist/windows")).unwrap();
    std::fs::write(dir.join("ab"), "").unwrap();
    dir
}

fn in_scratch(dir: &Scratch, script: &str) -> String {
    let here = dir.path().to_string_lossy().replace('\\', "/");
    cash(&[
        "-c",
        &format!("cd '{here}' && shopt -s winpaths && {script}"),
    ])
}

#[test]
fn a_relative_path_spelled_the_windows_way_is_the_path() {
    // `dist\windows` names a directory, and Bash's `distwindows` nothing: the word is
    // the path, to `cd`, as an argument and as a glob (the user, 2026-11-05).
    let dir = relative_paths_scratch();
    let here = dir.path().to_string_lossy().replace('\\', "/");
    assert_eq!(
        in_scratch(&dir, r"cd dist\windows && pwd"),
        format!("{here}/dist/windows")
    );
    // A glob's matches are spelled with slashes, as a drive path's are.
    assert_eq!(
        in_scratch(&dir, r"printf '[%s]\n' dist\windows dist\win*"),
        "[dist\\windows]\n[dist/windows]"
    );
}

#[test]
fn bashs_reading_of_a_relative_word_wins_where_it_names_something() {
    // A file `ab` exists, so `a\b` is `ab`, as in Bash; `x\y` names nothing either way
    // and is Bash's `xy`; a space after a backslash is still an escape; a backslash right
    // after a slash is Bash's escape too, since a Windows path has no empty component.
    let dir = relative_paths_scratch();
    assert_eq!(
        in_scratch(&dir, r"printf '[%s]\n' a\b x\y dist\ windows dist/\windows"),
        "[ab]\n[xy]\n[dist windows]\n[dist/windows]"
    );
}

#[test]
fn a_relative_path_keeps_bashs_lexing_with_winpaths_off() {
    let dir = relative_paths_scratch();
    assert_eq!(
        in_scratch(&dir, r"shopt -u winpaths; printf '[%s]\n' dist\windows"),
        "[distwindows]"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_relative_path_spelled_the_windows_way_completes() -> anyhow::Result<()> {
    use cash_builtins::ShellBuilderExt as _;

    // Tab on `cd dist\win` found nothing: Bash's reading, `distwin`, names nothing. It
    // completes to `dist\windows\`, in the spelling it was typed in.
    let dir = relative_paths_scratch();
    let mut shell = cash_core::Shell::builder()
        .profile(cash_core::ProfileLoadBehavior::Skip)
        .rc(cash_core::RcLoadBehavior::Skip)
        .default_builtins(cash_builtins::BuiltinSet::BashMode)
        .build()
        .await?;
    shell.options_mut().windows_drive_paths = true;
    shell.set_working_dir(dir.path())?;

    let line = r"cd dist\win";
    let completions = shell.complete(line, line.len()).await?;
    assert!(
        completions
            .candidates
            .iter()
            .any(|candidate| candidate.starts_with(r"dist\windows")),
        "{line} completed to {:?}",
        completions.candidates
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_backslash_after_a_slash_or_at_the_end_completes_as_each_reading_has_it()
-> anyhow::Result<()> {
    use cash_builtins::ShellBuilderExt as _;

    // `dist/\w` is Bash's `dist/w` (a Windows path has no empty component), so Tab gives
    // `dist/windows`; it gave `dist/\indows`, the typed `dist/\` put back on the name
    // (the user, 2026-11-05, in 2.5.1). A stray `dist/\` completes as `dist/` does, the
    // whole word replaced. `dist\` typed the Windows way lists what is inside.
    let dir = relative_paths_scratch();
    let mut shell = cash_core::Shell::builder()
        .profile(cash_core::ProfileLoadBehavior::Skip)
        .rc(cash_core::RcLoadBehavior::Skip)
        .default_builtins(cash_builtins::BuiltinSet::BashMode)
        .build()
        .await?;
    shell.options_mut().windows_drive_paths = true;
    shell.set_working_dir(dir.path())?;

    for (line, candidate) in [
        (r"cd dist/\w", "dist/windows"),
        (r"cd dist/\", "dist/windows"),
        (r"cd dist\", r"dist\windows"),
    ] {
        let completions = shell.complete(line, line.len()).await?;
        assert_eq!(completions.candidates, [candidate], "{line}");
        assert_eq!(
            (completions.insertion_index, completions.delete_count),
            (3, line.len() - 3),
            "{line} replaces its whole word"
        );
    }
    Ok(())
}
