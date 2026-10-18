//! The manual, `crates/cash-man`, covers every builtin the shell registers: a builtin
//! added without a page, or a page left behind by a builtin removed, fails here.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use std::collections::BTreeSet;

use crate::common::run;

#[test]
fn man_prints_a_page_as_text_when_its_output_is_not_a_terminal() {
    let out = run("COLUMNS=80 man cd");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout
            .starts_with("cd \u{2014} Change the shell's working directory.\n"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("\nSYNTAX\n"), "{}", out.stdout);
    assert!(!out.stdout.contains('\x1b'), "plain text: {:?}", out.stdout);
    assert!(out.stdout.lines().all(|line| line.chars().count() <= 80));

    // Alone, Getting started; a topic by its name or its title.
    let out = run("man");
    assert!(
        out.stdout.starts_with("Getting started \u{2014} "),
        "{}",
        out.stdout
    );
    assert!(run("man paths").stdout.starts_with("Paths: "));
    assert!(
        run("man 'getting started'")
            .stdout
            .starts_with("Getting started")
    );

    // A draft ends with the command's own --help.
    let out = run("man sort");
    assert!(
        out.stdout.contains("\nUSAGE AND OPTIONS\n"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("--numeric-sort"), "{}", out.stdout);
}

#[test]
fn man_k_lists_what_a_word_finds_and_an_unknown_name_is_said() {
    let out = run("man -k archive");
    assert_eq!(out.code, 0, "{}", out.stderr);
    for name in ["7z ", "tar ", "zip "] {
        assert!(
            out.stdout.lines().any(|line| line.starts_with(name)),
            "{name}in:\n{}",
            out.stdout
        );
    }
    let out = run("man -k zzqqxx");
    assert_eq!(out.code, 1);
    assert_eq!(out.stderr, "man: nothing appropriate for zzqqxx");

    let out = run("man sedd");
    assert_eq!(out.code, 1);
    assert!(
        out.stderr.starts_with("man: no page for sedd\n"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("Did you mean: sed"), "{}", out.stderr);
}

#[test]
fn every_builtin_has_a_page_and_every_page_names_a_builtin() {
    let out = run("compgen -b");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let registered: BTreeSet<&str> = out.stdout.lines().map(str::trim).collect();
    let manual = cash_man::manual();
    let covered: BTreeSet<&str> = manual
        .pages
        .iter()
        .flat_map(|page| page.names.iter().map(String::as_str))
        .collect();
    let missing: Vec<_> = registered.difference(&covered).collect();
    assert_eq!(
        missing,
        Vec::<&&str>::new(),
        "builtins with no page in crates/cash-man/pages"
    );
    let stale: Vec<_> = covered.difference(&registered).collect();
    assert_eq!(
        stale,
        Vec::<&&str>::new(),
        "pages in crates/cash-man/pages that name no builtin"
    );
}
