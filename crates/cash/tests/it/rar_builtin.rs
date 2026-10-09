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

use crate::common::{Scratch, golden, run, run_in, run_oracle_script, with_divergence};

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
fn rar_reconstructs_volumes_as_winrar_7_23_does() {
    assert_eq!(
        run_oracle_script("rar_reconstruct"),
        with_cash_banners("rar_reconstruct", 15, 0)
    );
}

#[test]
fn rar_extracts_by_switches_as_winrar_7_23_does() {
    assert_eq!(
        run_oracle_script("rar_extract_switches"),
        with_cash_banners("rar_extract_switches", 18, 0)
    );
}

#[test]
fn rar_keeps_versions_as_winrar_7_23_does() {
    // A trial WinRAR says so after its banner when it writes; cash has no trial.
    let expected = with_divergence(
        &with_cash_banners("rar_versions", 18, 0),
        "\nEvaluation copy. Please register.\n",
        "",
        5,
    );
    assert_eq!(run_oracle_script("rar_versions"), expected);
}

#[test]
fn rar_extracts_links_as_winrar_7_23_does() {
    assert_eq!(
        run_oracle_script("rar_links"),
        with_cash_banners("rar_links", 12, 0)
    );
}

#[test]
fn rar_keeps_times_as_winrar_7_23_does() {
    assert_eq!(
        run_oracle_script("rar_times"),
        with_cash_banners("rar_times", 0, 0)
    );
}

#[test]
fn rar_adds_by_name_as_winrar_7_23_does() {
    // A trial WinRAR says so before it writes; cash has no trial.
    let expected = with_divergence(
        &with_cash_banners("rar_add_names", 0, 0),
        "\nEvaluation copy. Please register.\n",
        "",
        21,
    );
    assert_eq!(run_oracle_script("rar_add_names"), expected);
}

#[test]
fn rar_names_by_date_as_winrar_7_23_does() {
    // A trial WinRAR says so before it writes; cash has no trial.
    let expected = with_divergence(
        &with_cash_banners("rar_agname", 0, 0),
        "\nEvaluation copy. Please register.\n",
        "",
        8,
    );
    assert_eq!(run_oracle_script("rar_agname"), expected);
}

#[test]
fn rar_logs_names_as_winrar_7_23_does() {
    assert_eq!(
        run_oracle_script("rar_log"),
        with_cash_banners("rar_log", 0, 0)
    );
}

#[test]
fn rar_cuts_volumes_with_recovery_records_as_winrar_7_23_does() {
    assert_eq!(
        run_oracle_script("rar_volumes"),
        with_cash_banners("rar_volumes", 0, 0)
    );
}

/// Volumes whose headers are encrypted, a recovery record in each or not, are each the
/// size asked and read back: they used to come out larger, their headers on top of a
/// volume's worth of data. (Rar.exe's own cut them elsewhere, its quick-open record
/// under the encryption, which cash's lack.)
#[test]
fn rar_volumes_with_encrypted_headers_keep_their_size() {
    let scratch = Scratch::new("rar-hp-volumes");
    let out = run_in(
        scratch.path(),
        "awk 'BEGIN { srand(5); for (i = 0; i < 70000; i++) printf \"%c\", 33 + int(rand() * 90) }' > a.bin
         for sw in '' -rr3; do
           rm -rf o && mkdir o
           rar a -m0 -idq -hpsecret -v20k $sw o/v.rar a.bin
           for f in o/*; do stat -c %s \"$f\"; done
           rar t -idq -psecret o/v.part1.rar && echo tested
         done",
    );
    assert_eq!(
        out.stdout, "20480\n20480\n20480\n9448\ntested\n20480\n20480\n20480\n14550\ntested",
        "{}",
        out.stderr
    );
}

#[test]
fn rar_says_it_makes_no_recovery_volumes_or_sfx() {
    let out = run("rar rv2 -idq a.rar; echo \"rc=$?\"; rar s -idq a.rar; echo \"rc=$?\"");
    assert_eq!(out.stdout, "rc=2\nrc=2");
    assert_eq!(
        out.stderr,
        "\nrar: cash's rar does not make recovery volumes\n\n\
         rar: cash's rar does not make self-extracting archives"
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
