//! `help` and `cash help`: the catalogue covers every builtin the shell registers, and
//! what `help` prints from it, by kind, page, topic and search.
//!
//! The catalogue is `crates/cash-builtins/src/helpdocs`. The first test is the one that
//! keeps it from drifting: a builtin added without an entry, or an entry left behind
//! by a builtin removed, fails it.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::collections::BTreeSet;
use std::process::Command;

use cash_builtins::helpdocs;

use crate::common::{CASH, isolate, output_of, run};

#[test]
fn every_builtin_has_an_entry_and_every_entry_is_a_builtin() {
    let out = run("compgen -b");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let registered: BTreeSet<&str> = out.stdout.lines().map(str::trim).collect();
    let catalogue = helpdocs::catalogue();
    let listed: BTreeSet<&str> = catalogue.entries.iter().map(|entry| entry.name).collect();

    let missing: Vec<_> = registered.difference(&listed).collect();
    assert!(
        missing.is_empty(),
        "builtins with no entry in crates/cash-builtins/src/helpdocs/builtins.md: {missing:?}"
    );
    let stale: Vec<_> = listed.difference(&registered).collect();
    assert!(
        stale.is_empty(),
        "entries in builtins.md that name no builtin: {stale:?}"
    );
    for page in &catalogue.pages {
        for name in &page.names {
            assert!(
                registered.contains(name),
                "pages/{}.md names `{name}`, which is not a builtin",
                page.file
            );
        }
    }
}

#[test]
fn help_lists_the_builtins_by_kind_each_with_its_summary() {
    let out = run("help");
    assert_eq!(out.code, 0, "{}", out.stderr);
    for kind in &helpdocs::catalogue().kinds {
        assert!(
            out.stdout.contains(&format!("\n{kind}\n")),
            "no heading `{kind}`:\n{}",
            out.stdout
        );
    }
    assert!(
        out.stdout
            .lines()
            .any(|line| line.trim_start().starts_with("cd ")
                && line.ends_with("Change the shell's working directory.")),
        "{}",
        out.stdout
    );
}

#[test]
fn help_marks_a_builtin_that_hides_a_windows_program() {
    let system32 = std::path::Path::new(
        &std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()),
    )
    .join("System32");
    if !system32.join("sort.exe").is_file() {
        return;
    }
    let out = run("help");
    assert!(
        out.stdout.lines().any(|line| line.starts_with(" +sort ")),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("sort.exe"), "{}", out.stdout);
    let page = run("help sort");
    assert!(
        page.stdout.contains("Inside cash, sort runs this builtin"),
        "{}",
        page.stdout
    );
}

#[test]
fn help_sed_is_a_page_with_its_windows_notes_and_its_own_options() {
    let out = run("help sed");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let text = &out.stdout;
    assert!(text.starts_with("NAME\n    sed - "), "{text}");
    assert!(text.contains("\nLINE ENDINGS\n"), "{text}");
    assert!(text.contains("CASH_EOL=lf"), "{text}");
    // The options are sed's own `--help`, not a copy.
    assert!(text.contains("\nUSAGE AND OPTIONS\n"), "{text}");
    assert!(text.contains("--in-place"), "{text}");
    assert!(text.contains("help crlf"), "{text}");
    assert!(text.contains("D49"), "{text}");
    // Not a terminal, so no colour.
    assert!(!text.contains('\x1b'), "{text}");
}

#[test]
fn help_topics_lists_every_topic_and_each_one_opens() {
    let out = run("help topics");
    assert_eq!(out.code, 0, "{}", out.stderr);
    for topic in &helpdocs::catalogue().topics {
        assert!(
            out.stdout
                .lines()
                .any(|line| line.trim_start().starts_with(&format!("{} ", topic.name))),
            "`{}` is not listed:\n{}",
            topic.name,
            out.stdout
        );
        let page = run(&format!("help {}", topic.name));
        assert_eq!(page.code, 0, "help {}: {}", topic.name, page.stderr);
        assert_ne!(page.stdout, "");
    }
    for name in [
        "paths",
        "crlf",
        "elevation",
        "job-control",
        "keys",
        "config",
        "vars",
        "differences",
    ] {
        assert!(
            helpdocs::catalogue().topic(name).is_some(),
            "no topic {name}"
        );
    }
}

#[test]
fn help_search_finds_builtins_and_topics_with_the_line_that_matched() {
    let out = run("help search CRLF");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.contains("sed (builtin) - "), "{}", out.stdout);
    assert!(out.stdout.contains("crlf (topic) - "), "{}", out.stdout);
    assert!(
        out.stdout
            .lines()
            .any(|line| line.starts_with("    ") && line.to_lowercase().contains("crlf")),
        "{}",
        out.stdout
    );

    let none = run("help search zzqqxx-nothing");
    assert_eq!(none.code, 1);
    assert_eq!(none.stdout, "");
}

#[test]
fn an_unknown_name_fails_and_suggests_the_closest_and_a_search() {
    let out = run("help sedd");
    assert_eq!(out.code, 1);
    assert!(
        out.stderr.contains("no help topics match `sedd'"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("Did you mean: sed"), "{}", out.stderr);
    assert!(out.stderr.contains("help search sedd"), "{}", out.stderr);
}

#[test]
fn bash_options_still_work_for_builtins_and_topics() {
    let out = run("help -d cd sed paths; help -s cd");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("cd - Change the shell's working directory.")
    );
    assert!(lines.iter().any(|line| line.starts_with("sed - ")));
    assert!(lines.iter().any(|line| line.starts_with("paths - ")));
    assert!(lines.iter().any(|line| line.starts_with("cd: cd")));
}

#[test]
fn cash_help_works_from_outside_the_shell() {
    // Not `cash_command()`: `help` is only the subcommand as cash's first argument.
    let out = output_of(isolate(Command::new(CASH).args(["help", "-d", "sed"])));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.starts_with("sed - "), "{}", out.stdout);

    let unknown = output_of(isolate(Command::new(CASH).args(["help", "sedd"])));
    assert_eq!(unknown.code, 1);
    assert!(
        unknown.stderr.contains("Did you mean: sed"),
        "{}",
        unknown.stderr
    );
}

#[test]
fn a_script_named_help_still_runs() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("help"), "echo the script ran\n").unwrap();
    let out = output_of(isolate(
        Command::new(CASH).arg("help").current_dir(dir.path()),
    ));
    assert_eq!(out.stdout, "the script ran");
}
