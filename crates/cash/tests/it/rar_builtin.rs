//! `rar` and `unrar`: the golden output of `tests/oracle/rar_*.sh`, made by Scoop's `WinRAR`
//! 7.23 (`Rar.exe`, `UnRAR.exe`) on Windows, and what the oracle cannot show.
//!
//! The scripts ran under `WinRAR`'s tools to make the golden files, with CRLF and `\`
//! turned to LF and `/`; here they run under cash. Where cash differs on purpose, the
//! expected text is replaced in the test with the reason beside it.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
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
fn rar_repacks_solid_archives_as_winrar_7_23_does() {
    // A trial WinRAR says so after its banner when it adds; cash has no trial.
    let expected = with_divergence(
        &with_cash_banners("rar_solid", 23, 0),
        "\nEvaluation copy. Please register.\n",
        "",
        14,
    );
    assert_eq!(run_oracle_script("rar_solid"), expected);
}

#[test]
fn rar_asks_passwords_as_winrar_7_23_does() {
    // A trial WinRAR says so after its banner when it adds; cash has no trial.
    let expected = with_divergence(
        &with_cash_banners("rar_passwords", 48, 0),
        "\nEvaluation copy. Please register.\n",
        "",
        4,
    );
    assert_eq!(run_oracle_script("rar_passwords"), expected);
}

#[test]
fn rar_comments_renames_locks_and_changes_as_winrar_7_23_does() {
    // A trial WinRAR says so after its banner when it adds; cash has no trial.
    let expected = with_divergence(
        &with_cash_banners("rar_modify", 43, 0),
        "\nEvaluation copy. Please register.\n",
        "",
        5,
    );
    assert_eq!(run_oracle_script("rar_modify"), expected);
}

#[test]
fn rar_finds_strings_as_winrar_7_23_does() {
    assert_eq!(
        run_oracle_script("rar_find"),
        with_cash_banners("rar_find", 35, 0)
    );
}

#[test]
fn rar_repairs_as_winrar_7_23_does() {
    assert_eq!(
        run_oracle_script("rar_repair"),
        with_cash_banners("rar_repair", 23, 0)
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

#[test]
fn rar_adds_links_as_winrar_7_23_does() {
    // A trial WinRAR says so after its banner when it adds; cash has no trial.
    let expected = with_divergence(
        &with_cash_banners("rar_add_links", 10, 0),
        "\nEvaluation copy. Please register.\n",
        "",
        31,
    );
    assert_eq!(run_oracle_script("rar_add_links"), expected);
}

#[test]
fn rar_says_it_makes_no_recovery_volumes_or_sfx() {
    let out = run("rar rv2 -idq a.rar; echo \"rc=$?\"; rar s -idq a.rar; echo \"rc=$?\"");
    assert_eq!(out.stdout, "rc=2\nrc=2");
    // -idq ends standard error with one more line's end, as rar does.
    assert_eq!(
        out.stderr,
        "\nrar: cash's rar does not make recovery volumes\n\n\n\
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

/// A folder named alone is walked through its junctions, as Rar.exe 7.23 walks it, and a
/// junction back to a folder above is passed by, where Rar.exe goes round it until the
/// path is too long (512 names of a four-file tree): each file is stored once.
#[test]
fn a_folder_is_walked_through_junctions_storing_each_file_once() {
    let scratch = Scratch::new("rar-walk");
    let d = scratch.path().join("d");
    std::fs::create_dir_all(d.join("sub")).expect("folders");
    std::fs::create_dir_all(scratch.path().join("other")).expect("a folder");
    std::fs::write(d.join("a.txt"), "a").expect("a file");
    std::fs::write(d.join("sub").join("b.txt"), "b").expect("a file");
    std::fs::write(scratch.path().join("other").join("o.txt"), "o").expect("a file");
    cash_win32::junction::point(&d.join("out"), &scratch.path().join("other")).expect("a junction");
    cash_win32::junction::point(&d.join("sub").join("up"), &d).expect("a junction");
    let out = run_in(
        scratch.path(),
        "rar a -idq x.rar d; echo \"rc=$?\"; rar lb x.rar | tr '\\\\' / | sort; \
         rar a -idq -r m.rar 'd/*.txt'; echo \"rc=$?\"; rar lb m.rar | tr '\\\\' / | sort",
    );
    assert_eq!(
        out.stdout,
        "rc=0\nd\nd/a.txt\nd/out\nd/out/o.txt\nd/sub\nd/sub/b.txt\n\
         rc=0\nd/a.txt\nd/out/o.txt\nd/sub/b.txt",
        "{}",
        out.stderr
    );
}

/// A folder that cannot be listed is stored, then said by its whole path in Windows'
/// words, status 6, as Rar.exe 7.23 says it.
#[test]
fn a_folder_that_cannot_be_listed_is_said_as_rar_says_it() {
    let scratch = Scratch::new("rar-shut");
    std::fs::create_dir_all(scratch.join("d").join("shut")).expect("folders");
    std::fs::write(scratch.join("d").join("a.txt"), "a").expect("a file");
    std::fs::write(scratch.join("d").join("shut").join("s.txt"), "s").expect("a file");
    let shut = scratch.join("d").join("shut");
    let out = while_unreadable(&shut, || {
        run_in(
            scratch.path(),
            "rar a -idq s.rar d; echo \"rc=$?\"; rar lb s.rar | tr '\\\\' / | sort",
        )
    });
    assert_eq!(out.stdout, "rc=6\nd\nd/a.txt\nd/shut");
    let whole = std::fs::canonicalize(&shut).expect("the folder");
    let whole = whole.display().to_string();
    let whole = whole.strip_prefix(r"\\?\").unwrap_or(&whole);
    assert_eq!(
        out.stderr,
        format!("\nCannot read contents of {whole}\nAccess is denied.")
    );
}

/// Denies the user reading `folder` while `body` runs, as a folder of another account's is.
fn while_unreadable<T>(folder: &std::path::Path, body: impl FnOnce() -> T) -> T {
    let user = std::env::var("USERNAME").expect("a user name");
    let icacls = |args: &[&str]| {
        let status = std::process::Command::new("icacls")
            .arg(folder)
            .args(args)
            .stdout(std::process::Stdio::null())
            .status()
            .expect("icacls runs");
        assert!(status.success(), "icacls {args:?}");
    };
    icacls(&["/deny", &format!("{user}:(RX)")]);
    let result = body();
    icacls(&["/remove:d", &user]);
    result
}

/// The archives a mask names with `-r` are taken in the order their folders list them,
/// each folder gone into where it is met and through a junction, as Rar.exe 7.23 takes
/// them; cash took a folder's own first, by lowercase name, and no junction.
#[test]
fn a_masks_archives_are_taken_in_listing_order_through_junctions() {
    let scratch = Scratch::new("rar-mask-order");
    let w = scratch.path().join("w");
    for folder in ["b_sub", "A", "_u"] {
        std::fs::create_dir_all(w.join(folder)).expect("a folder");
    }
    std::fs::create_dir_all(scratch.path().join("other")).expect("a folder");
    cash_win32::junction::point(&w.join("j"), &scratch.path().join("other")).expect("a junction");
    let out = run_in(
        scratch.path(),
        "printf x > f.txt; rar a -idq one.rar f.txt; \
         for n in w/_x w/a w/B w/z w/b_sub/m w/A/k w/_u/u other/o; do cp one.rar $n.rar; done; \
         cd w && rar lt -r '*.rar' | tr '\\\\' / | sed -n 's|^Archive: ||p'",
    );
    assert_eq!(
        out.stdout, "A/k.rar\na.rar\nB.rar\nb_sub/m.rar\nj/o.rar\nz.rar\n_u/u.rar\n_x.rar",
        "{}",
        out.stderr
    );
}
