//! `shopt winpaths` — **D53**: an unquoted `C:\...` means the path it spells.
//!
//! A backslash is bash's escape character, so `cd C:\Users\me` reaches `cd` as
//! `C:Usersme`, and a path pasted from Explorer or a PowerShell prompt fails. With
//! `winpaths`, a word that starts with a drive and a backslash keeps its backslashes
//! wherever bash's escape would only have eaten them. It is on by default at the
//! interactive prompt and off in scripts, which keep bash's lexing exactly.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::needless_raw_string_hashes,
    clippy::panic_in_result_fn,
    reason = "an integration test is outside a test module by construction, and an async 
              test returns a Result so that setup can use ?"
)]

use std::process::{Command, Stdio};

const CASH: &str = env!("CARGO_BIN_EXE_cash");

fn cash(args: &[&str]) -> String {
    let out = Command::new(CASH)
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
    assert_eq!(
        cash(&[
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
fn an_escaped_space_still_joins_the_word() {
    assert_eq!(
        words_with_winpaths(r"C:\Program\ Files\Git"),
        r"[C:\Program Files\Git]"
    );
}

#[test]
fn a_wildcard_after_a_backslash_still_globs() {
    let dir = std::env::temp_dir().join("cash-winpaths-glob");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.txt"), "").unwrap();
    std::fs::write(dir.join("b.txt"), "").unwrap();
    std::fs::write(dir.join("c.log"), "").unwrap();

    let spelled = dir.to_string_lossy().replace('/', r"\");
    let listed = words_with_winpaths(&format!(r"{spelled}\*.txt"));
    let _ = std::fs::remove_dir_all(&dir);

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

    let dir = std::env::temp_dir().join("cash-winpaths-complete");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("alpha-dir"))?;

    let line = format!(r"ls {}\alp", dir.to_string_lossy().replace('/', r"\"));
    let completions = shell.complete(&line, line.len()).await?;
    let _ = std::fs::remove_dir_all(&dir);

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
