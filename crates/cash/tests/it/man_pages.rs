//! The manual, `crates/cash-man`, covers every builtin the shell registers: a builtin
//! added without a page, or a page left behind by a builtin removed, fails here.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use std::collections::BTreeSet;

use crate::common::run;

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
