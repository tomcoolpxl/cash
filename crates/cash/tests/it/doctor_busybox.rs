//! `cash doctor` flags BusyBox applets known to break scripts — ROADMAP item 10, part 5.
//!
//! Scoop's BusyBox puts shims such as `wget.exe` on `PATH` whose `.shim` file names
//! `busybox.exe`. Doctor flags only the curated tools whose BusyBox applets break real
//! scripts (Q10), says what breaks, and points at a full copy further down `PATH` when
//! there is one. The shims here are fakes: a `.exe` name and a `.shim` file are all
//! doctor reads.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::Path;
use std::process::Command;

use crate::common::{CASH, doctor_finding};

/// A fake Scoop shim for `name`, pointing at `target`.
fn shim(dir: &Path, name: &str, target: &str) {
    std::fs::write(dir.join(format!("{name}.exe")), b"MZ").unwrap();
    std::fs::write(
        dir.join(format!("{name}.shim")),
        format!("path = \"{target}\"\n"),
    )
    .unwrap();
}

fn doctor(path: &str) -> String {
    // Not `cash_command()`: `doctor` is only the subcommand as cash's one argument, and
    // it reads no config and runs no script.
    let out = Command::new(CASH)
        .arg("doctor")
        .env("PATH", path)
        .output()
        .expect("run cash doctor");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_breaking_busybox_applet_is_flagged_with_the_reason() {
    let root = tempfile::tempdir().unwrap();
    let shims = root.path().join("shims");
    std::fs::create_dir(&shims).unwrap();
    shim(&shims, "wget", r"C:\scoop\apps\busybox\current\busybox.exe");
    // Not on the list: BusyBox cat is reduced too, but nothing notices.
    shim(&shims, "cal", r"C:\scoop\apps\busybox\current\busybox.exe");

    let report = doctor(&format!("{};C:\\Windows\\System32", shims.display()));
    let wget = doctor_finding(&report, "wget is ");
    assert!(wget.is_some(), "{report}");
    let wget = wget.unwrap();
    assert_eq!(wget.level, "WARN", "{report}");
    assert!(
        wget.text
            .ends_with("wget.exe, a BusyBox applet: -T (timeout) crashes it"),
        "{report}"
    );
    assert_eq!(
        wget.fix.as_deref(),
        Some("install the full tool: scoop install wget")
    );
    assert!(doctor_finding(&report, "cal is ").is_none(), "{report}");
}

#[test]
fn a_full_copy_later_on_path_is_pointed_at() {
    let root = tempfile::tempdir().unwrap();
    let shims = root.path().join("shims");
    let full = root.path().join("gnu");
    std::fs::create_dir(&shims).unwrap();
    std::fs::create_dir(&full).unwrap();
    // `nc`, then `tar`, were the example here until cash carried its own, which
    // doctor then has no reason to look for on PATH.
    shim(&shims, "make", r"C:\scoop\apps\busybox\current\busybox.exe");
    std::fs::write(full.join("make.exe"), b"MZ").unwrap();

    let report = doctor(&format!(
        "{};{};C:\\Windows\\System32",
        shims.display(),
        full.display()
    ));
    let shown_full = full.to_string_lossy().replace('\\', "/");
    let make = doctor_finding(&report, "$(shell ...) expands to nothing");
    assert!(make.is_some(), "{report}");
    // A fix's paths are written out in full, to be pasted.
    assert!(
        make.unwrap().fix.is_some_and(|fix| fix.contains(&format!(
            "is the full tool, later on PATH: put {shown_full} before"
        ))),
        "{report}"
    );
}

#[test]
fn a_shim_for_something_else_is_not_busybox() {
    let root = tempfile::tempdir().unwrap();
    let shims = root.path().join("shims");
    std::fs::create_dir(&shims).unwrap();
    shim(&shims, "wget", r"C:\scoop\apps\wget\current\wget.exe");

    let report = doctor(&format!("{};C:\\Windows\\System32", shims.display()));
    assert!(!report.contains("BusyBox"), "{report}");
}

/// `doctor` says what `sudo` elevates through.
#[test]
fn doctor_says_how_sudo_elevates() {
    let report = doctor(r"C:\Windows\System32");
    assert!(report.contains("sudo"), "{report}");
}
