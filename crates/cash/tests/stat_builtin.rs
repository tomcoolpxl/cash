//! Native `stat` builtin tests.
//!
//! Verifies that `stat` names a file's real owner — read from its security descriptor —
//! rather than the current user with a placeholder uid. On Windows Server an elevated
//! administrator's new files are owned by BUILTIN\Administrators (GitHub's runners), so
//! the two differ there; the owner is checked against `Get-Acl`, an independent source.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cash-stat-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Self(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cash(script: &str) -> String {
    let out = Command::new(CASH)
        .args(["-c", script])
        .output()
        .expect("failed to run cash");
    assert!(
        out.status.success(),
        "cash -c {script:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

/// The file's owner name (without its domain) and its SID's RID, from `Get-Acl`.
fn acl_owner(path: &Path) -> (String, u32) {
    // `$args` does not reach a `-Command` script, so the path is embedded as a
    // single-quoted literal, with any `'` doubled.
    let literal = path.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$acl = Get-Acl -LiteralPath '{literal}'; $acl.Owner; \
         $acl.GetOwner([System.Security.Principal.SecurityIdentifier]).Value"
    );
    let out = Command::new("powershell")
        // Run from PowerShell 7, the inherited PSModulePath points Windows PowerShell at
        // modules it cannot load, and `Get-Acl` fails to autoload; without it, Windows
        // PowerShell uses its own default.
        .env_remove("PSModulePath")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .expect("failed to run powershell");
    assert!(
        out.status.success(),
        "Get-Acl failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let owner = lines.next().expect("no owner from Get-Acl");
    let sid = lines.next().expect("no owner SID from Get-Acl");
    let name = owner.rsplit('\\').next().unwrap().to_string();
    let rid = sid
        .rsplit('-')
        .next()
        .unwrap()
        .parse()
        .expect("SID has a RID");
    (name, rid)
}

fn sample_file(scratch: &Scratch) -> (PathBuf, String) {
    let file = scratch.path().join("alpha.txt");
    std::fs::write(&file, b"sample content").expect("write file");
    let posix = file.to_string_lossy().replace('\\', "/");
    (file, posix)
}

#[test]
fn stat_names_the_files_real_owner() {
    let scratch = Scratch::new("owner-name");
    let (file, posix) = sample_file(&scratch);
    let (owner, _) = acl_owner(&file);

    let out = cash(&format!("stat -c '%U|%G' '{posix}'"));
    assert_eq!(out, format!("{owner}|{owner}"));
}

#[test]
fn stat_uid_is_the_owner_sids_rid() {
    let scratch = Scratch::new("owner-rid");
    let (file, posix) = sample_file(&scratch);
    let (_, rid) = acl_owner(&file);

    let out = cash(&format!("stat -c '%u|%g' '{posix}'"));
    assert_eq!(out, format!("{rid}|{rid}"));
    assert_ne!(out, "1000|1000", "stat still prints the placeholder uid");
}

#[test]
fn stat_default_report_names_the_real_owner() {
    let scratch = Scratch::new("default-report");
    let (file, posix) = sample_file(&scratch);
    let (owner, rid) = acl_owner(&file);

    let out = cash(&format!("stat '{posix}'"));
    let row = out
        .lines()
        .find(|line| line.contains("Uid:"))
        .unwrap_or_else(|| panic!("no Uid row:\n{out}"));
    // The fields are padded, and an account name may itself contain spaces, so compare
    // with all whitespace removed.
    let squeeze = |s: &str| s.split_whitespace().collect::<String>();
    let row = squeeze(row);
    let owner = squeeze(&owner);
    assert!(
        row.contains(&format!("Uid:({rid}/{owner})")),
        "the Uid field should be {rid}/{owner}: {row}"
    );
    assert!(
        row.contains(&format!("Gid:({rid}/{owner})")),
        "the Gid field should be {rid}/{owner}: {row}"
    );
}
