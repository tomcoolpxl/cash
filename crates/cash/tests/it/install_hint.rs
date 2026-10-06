//! The hint for a command that is not found: at an interactive prompt, after Bash's
//! `jq: command not found`, one line `install it: winget install jqlang.jq, or scoop
//! install jq`, from the table of `help tools`. A script, `cash -c` and a shell whose
//! user defined `command_not_found_handle` get Bash's line and status 127 only, so no
//! script's output changes.
//!
//! The table's ids were each checked once, with `winget search --exact --id` and the
//! Scoop buckets' manifests, and recorded in `tests/fixtures/tools-ids.txt`; the last
//! test holds the table to that record, so an id cannot change without being checked
//! again. The tests never call winget or Scoop.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a failed \
              assumption should stop it loudly"
)]

use std::path::Path;
use std::time::Duration;

use cash_builtins::helpdocs;
use cash_win32::conpty::ConPtySession;

use crate::common::{CASH, Scratch, cash_command, output_of, run, with_isolated_environment};

/// The hint for `jq`, as the table has it.
const JQ_HINT: &str = "install it: winget install jqlang.jq, or scoop install jq";

/// How long the interactive shell may take to answer a line.
const STUCK: Duration = Duration::from_secs(10);

/// Runs `script` with `cash -c` in `dir`, with `PATH` set to `dir` alone, so that the
/// developer's own jq, if any, is not found.
fn run_without_path(dir: &Scratch, script: &str) -> crate::common::Output {
    output_of(
        cash_command()
            .args(["-c", script])
            .env("PATH", dir.path())
            .current_dir(dir.path()),
    )
}

#[test]
fn a_script_and_c_get_bashs_line_and_127_only() {
    let dir = Scratch::new("install-hint-c");
    let out = run_without_path(&dir, "jq; echo rc=$?");
    assert_eq!(out.stdout, "rc=127");
    assert!(
        out.stderr.ends_with(": line 1: jq: command not found"),
        "{}",
        out.stderr
    );
    assert_eq!(out.stderr.lines().count(), 1, "{}", out.stderr);
    assert!(!out.stderr.contains("install it"), "{}", out.stderr);

    std::fs::write(dir.join("script.sh"), "rg --version\n").unwrap();
    let out = output_of(
        cash_command()
            .arg("script.sh")
            .env("PATH", dir.path())
            .current_dir(dir.path()),
    );
    assert_eq!(out.code, 127);
    assert_eq!(out.stderr, "script.sh: line 1: rg: command not found");
}

/// An interactive cash on a pseudo console, in `dir`, with `PATH` set to `dir` alone.
fn interactive_in(dir: &Scratch) -> ConPtySession {
    let path = dir.path().to_string_lossy().into_owned();
    let mut session = with_isolated_environment(|env| {
        let mut env: Vec<(&str, &str)> = env
            .iter()
            .copied()
            .filter(|(name, _)| !name.eq_ignore_ascii_case("PATH") && *name != "PS1")
            .collect();
        env.push(("PATH", path.as_str()));
        env.push(("PS1", "PROMPT$ "));
        env.push(("HISTFILE", ""));
        ConPtySession::start_in(
            Path::new(CASH),
            &[
                "--noprofile",
                "--norc",
                "--no-config",
                "--disable-color",
                "-i",
            ],
            Some(&env),
            Some(dir.path()),
        )
    })
    .expect("start cash in a pseudo terminal");
    session.expect("PROMPT$", STUCK).expect("the prompt");
    session
}

/// The report: typed at the prompt, `jq` is followed by the hint; `gradle`, which winget
/// has no package for, names Scoop alone; a name outside the table gets Bash's line only.
#[test]
fn at_the_prompt_the_hint_follows_bashs_line() {
    let dir = Scratch::new("install-hint-prompt");
    let mut session = interactive_in(&dir);

    session.send("jq\r").unwrap();
    session.expect(JQ_HINT, STUCK).expect("the hint for jq");
    let screen = session.screen().text();
    assert!(screen.contains("cash: jq: command not found\n"), "{screen}");
    assert!(
        screen.contains(&format!("jq: command not found\n{JQ_HINT}")),
        "the hint does not follow Bash's line:\n{screen}"
    );

    session.send("echo rc=$?\r").unwrap();
    session.expect("rc=127", STUCK).expect("the status");

    session.send("gradle\r").unwrap();
    session
        .expect("install it: scoop install gradle", STUCK)
        .expect("the hint for gradle");

    session.send("nosuchtool-zz\r").unwrap();
    session.send("echo MARK_$((1 + 1))\r").unwrap();
    session.expect("MARK_2", STUCK).expect("the marker");
    let screen = session.screen().text();
    assert!(
        screen.contains("nosuchtool-zz: command not found"),
        "{screen}"
    );
    assert_eq!(
        screen.matches("install it:").count(),
        2,
        "a name outside the table got a hint:\n{screen}"
    );

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("cash exits"), 0);
}

/// A user's `command_not_found_handle` is theirs to speak: it runs in place of Bash's
/// line, no hint while it is defined, and the hint again once it is gone.
#[test]
fn a_defined_command_not_found_handle_suppresses_the_hint() {
    let dir = Scratch::new("install-hint-handle");
    let mut session = interactive_in(&dir);

    session
        .send("command_not_found_handle() { echo \"handled $1\"; }\r")
        .unwrap();
    session.send("jq\r").unwrap();
    session.send("echo MARK_$((2 + 2))\r").unwrap();
    session.expect("MARK_4", STUCK).expect("the marker");
    let screen = session.screen().text();
    assert!(screen.contains("handled jq"), "{screen}");
    assert!(
        !screen.contains("jq: command not found") && !screen.contains("install it"),
        "the shell spoke beside a command_not_found_handle:\n{screen}"
    );

    session
        .send("unset -f command_not_found_handle; jq\r")
        .unwrap();
    session.expect(JQ_HINT, STUCK).expect("the hint once more");

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("cash exits"), 0);
}

#[test]
fn help_tools_opens_and_lists_jq_with_both_names() {
    let out = run("help tools");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let text = &out.stdout;
    assert!(
        text.starts_with("TOOLS TO INSTALL: WINGET AND SCOOP NAMES FOR COMMON COMMANDS\n"),
        "{text}"
    );
    // The prose is wrapped where the width says, so the hint may span two lines.
    let unwrapped = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(unwrapped.contains(JQ_HINT), "{text}");
    let row = text
        .lines()
        .find(|line| line.trim_start().starts_with("jq "))
        .unwrap_or_else(|| panic!("no row for jq:\n{text}"));
    assert!(row.contains("jqlang.jq") && row.contains("JSON"), "{row}");
    // The table is columns, not markdown: no bar, and no `|---|` line under the header.
    assert!(!row.contains('|'), "{row}");
    assert!(!text.contains("---"), "{text}");
    assert!(
        text.lines()
            .any(|line| line.trim_start().starts_with("command  ") && line.contains("winget")),
        "no header row:\n{text}"
    );
    assert!(text.contains("\nSEE ALSO\n"), "{text}");
    for developer_note in ["D75", "spec.md", "ROADMAP", "research"] {
        assert!(!text.contains(developer_note), "{developer_note} in {text}");
    }

    let out = run("help topics");
    assert!(
        out.stdout
            .lines()
            .any(|line| line.trim_start().starts_with("tools ")),
        "{}",
        out.stdout
    );

    let out = run("help search ripgrep");
    assert!(out.stdout.contains("tools (topic) - "), "{}", out.stdout);
}

/// `grep` and `diff` are cash's own, so the table has no row for either, and `cash
/// doctor` has nothing to name for them; `jq` is the hint's.
#[test]
fn doctor_and_the_hint_agree() {
    assert_eq!(helpdocs::install_hint("grep"), None);
    assert_eq!(helpdocs::install_hint("diff"), None);
    assert_eq!(
        helpdocs::install_hint("jq").as_deref(),
        Some("winget install jqlang.jq, or scoop install jq")
    );
}

/// The table holds every id that was checked, and nothing else.
#[test]
fn the_table_matches_the_ids_that_were_verified() {
    let recorded = include_str!("../fixtures/tools-ids.txt");
    let table: Vec<String> = helpdocs::tools()
        .iter()
        .map(|tool| {
            format!(
                "{}\t{}\t{}",
                tool.command,
                tool.winget.unwrap_or("-"),
                tool.scoop.unwrap_or("-")
            )
        })
        .collect();
    let recorded: Vec<&str> = recorded.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(
        table, recorded,
        "help tools and tests/fixtures/tools-ids.txt differ: check the id with \
         `winget search --exact --id ID` or the Scoop bucket, then record it"
    );
}
