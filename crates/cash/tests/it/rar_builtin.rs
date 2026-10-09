//! `rar` and `unrar`: the golden output of `tests/oracle/rar_*.sh`, made by Scoop's `WinRAR`
//! 7.23 (`Rar.exe`, `UnRAR.exe`) on Windows, and what the oracle cannot show.
//!
//! The scripts ran under `WinRAR`'s tools to make the golden files, with CRLF and `\`
//! turned to LF and `/`; here they run under cash. Where cash differs on purpose, the
//! expected text is replaced in the test with the reason beside it.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{golden, run, run_oracle_script, with_divergence};

/// The golden file `name` with cash's banners in rar's and unrar's places, `rar` and
/// `unrar` times each.
fn with_cash_banners(name: &str, rar: usize, unrar: usize) -> String {
    let expected = with_divergence(
        &golden(name),
        "RAR 7.23 x64   Copyright (c) 1993-2026 Alexander Roshal   27 Jun 2026\n",
        "RAR (cash)   RAR 7.23's commands and switches, in pure Rust\n",
        rar,
    );
    let expected = with_divergence(
        &expected,
        "Trial version             Type 'rar -?' for help\n",
        "cash's own code, not RARLAB's   Type 'rar -?' for help\n",
        rar,
    );
    with_divergence(
        &expected,
        "UNRAR 7.23 x64 freeware      Copyright (c) 1993-2026 Alexander Roshal\n",
        "UNRAR (cash)   UnRAR 7.23's commands and switches, in pure Rust\n",
        unrar,
    )
}

#[test]
fn rar_lists_as_winrar_7_23_does() {
    assert_eq!(
        run_oracle_script("rar_list"),
        with_cash_banners("rar_list", 330, 14)
    );
}

#[test]
fn rar_tests_extracts_and_prints_as_winrar_7_23_does() {
    assert_eq!(
        run_oracle_script("rar_extract"),
        with_cash_banners("rar_extract", 181, 3)
    );
}

#[test]
fn rar_adds_updates_moves_and_deletes_as_winrar_7_23_does() {
    // A trial WinRAR says so after its banner; cash has no trial to speak of.
    let expected = with_divergence(
        &with_cash_banners("rar_write", 53, 0),
        "\nEvaluation copy. Please register.\n",
        "",
        50,
    );
    assert_eq!(run_oracle_script("rar_write"), expected);
}

#[test]
fn rar_comments_renames_locks_and_changes_as_winrar_7_23_does() {
    assert_eq!(
        run_oracle_script("rar_modify"),
        with_cash_banners("rar_modify", 30, 0)
    );
}

#[test]
fn rar_finds_strings_as_winrar_7_23_does() {
    assert_eq!(
        run_oracle_script("rar_find"),
        with_cash_banners("rar_find", 26, 0)
    );
}

#[test]
fn rar_repairs_as_winrar_7_23_does() {
    assert_eq!(
        run_oracle_script("rar_repair"),
        with_cash_banners("rar_repair", 11, 0)
    );
}

#[test]
fn rar_and_unrar_name_themselves_as_cash() {
    let out = run("rar -iver; unrar -iver; rar | sed -n 2,3p; unrar | sed -n 2p");
    assert_eq!(
        out.stdout,
        "7.23 x64 (cash)\n7.23 x64 (cash)\n\
         RAR (cash)   RAR 7.23's commands and switches, in pure Rust\n\
         cash's own code, not RARLAB's   Type 'rar -?' for help\n\
         UNRAR (cash)   UnRAR 7.23's commands and switches, in pure Rust"
    );
}
