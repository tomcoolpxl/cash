//! Native `stat` builtin tests.
//!
//! Verifies that `stat` names a file's real owner — read from its security descriptor —
//! rather than the current user with a placeholder uid. On Windows Server an elevated
//! administrator's new files are owned by BUILTIN\Administrators (GitHub's runners), so
//! the two differ there; the owner and the group (`None` there) are checked against
//! `Get-Acl`, an independent source.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::common::{Scratch, cash_command};

fn cash(script: &str) -> String {
    let out = cash_command()
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

#[test]
fn stat_counts_a_folders_links_as_linux_does() {
    // 2 and one per folder in it, where `ls -l` shows 1 unless asked to count; a file's
    // are its hard links.
    let scratch = Scratch::new("stat-links");
    let dir = scratch.path().join("folder");
    std::fs::create_dir_all(dir.join("one")).unwrap();
    std::fs::create_dir_all(dir.join("two")).unwrap();
    std::fs::write(dir.join("file"), "x").unwrap();
    std::fs::hard_link(dir.join("file"), dir.join("again")).unwrap();
    let dir_str = dir.to_string_lossy().replace('\\', "/");
    assert_eq!(
        cash(&format!(
            "stat -c %h '{dir_str}' '{dir_str}/one' '{dir_str}/file'"
        )),
        "4\n2\n2"
    );
    let listed = cash(&format!(
        "cd '{dir_str}' && ls -l --count-links . && ls -l ."
    ));
    let link_of = |text: &str, name: &str| {
        text.lines()
            .find(|line| line.ends_with(&format!(" {name}")))
            .and_then(|line| line.split_whitespace().nth(1))
            .map(str::to_owned)
    };
    let (counted, plain) = listed.split_once("\ntotal").expect("two listings");
    assert_eq!(link_of(counted, "one").as_deref(), Some("2"), "{listed}");
    assert_eq!(link_of(plain, "one").as_deref(), Some("1"), "{listed}");
    assert_eq!(link_of(plain, "file").as_deref(), Some("2"), "{listed}");
}

/// An account's name (without its domain) and its SID's RID.
type Account = (String, u32);

/// The file's owner and group, from `Get-Acl`.
fn acl_accounts(path: &Path) -> (Account, Account) {
    // `$args` does not reach a `-Command` script, so the path is embedded as a
    // single-quoted literal, with any `'` doubled.
    let literal = path.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$acl = Get-Acl -LiteralPath '{literal}'; $sid = [System.Security.Principal.SecurityIdentifier]; \
         $acl.Owner; $acl.GetOwner($sid).Value; $acl.Group; $acl.GetGroup($sid).Value"
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
    let mut account = || {
        let name = lines.next().expect("no account from Get-Acl");
        let sid = lines.next().expect("no SID from Get-Acl");
        let rid = sid
            .rsplit('-')
            .next()
            .unwrap()
            .parse()
            .expect("SID has a RID");
        (name.rsplit('\\').next().unwrap().to_string(), rid)
    };
    let owner = account();
    let group = account();
    (owner, group)
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
    let ((owner, _), (group, _)) = acl_accounts(&file);

    let out = cash(&format!("stat -c '%U|%G' '{posix}'"));
    assert_eq!(out, format!("{owner}|{group}"));
}

#[test]
fn stat_uid_is_the_owner_sids_rid() {
    let scratch = Scratch::new("owner-rid");
    let (file, posix) = sample_file(&scratch);
    let ((_, rid), (_, gid)) = acl_accounts(&file);

    let out = cash(&format!("stat -c '%u|%g' '{posix}'"));
    assert_eq!(out, format!("{rid}|{gid}"));
    assert_ne!(out, "1000|1000", "stat still prints the placeholder uid");
}

#[test]
fn stat_default_report_names_the_real_owner() {
    let scratch = Scratch::new("default-report");
    let (file, posix) = sample_file(&scratch);
    let ((owner, rid), (group, gid)) = acl_accounts(&file);

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
    let group = squeeze(&group);
    assert!(
        row.contains(&format!("Uid:({rid}/{owner})")),
        "the Uid field should be {rid}/{owner}: {row}"
    );
    assert!(
        row.contains(&format!("Gid:({gid}/{group})")),
        "the Gid field should be {gid}/{group}: {row}"
    );
}

#[test]
fn a_folder_has_an_inode_and_ef_compares_files() {
    // A folder could not be opened without backup semantics: `stat` gave it inode 0 on
    // device 0 (ARCH-06), and `[ a -ef b ]` was "not supported" (XC-3).
    let scratch = Scratch::new("ef");
    let (file, posix) = sample_file(&scratch);
    std::fs::hard_link(&file, scratch.path().join("beta.txt")).expect("hard link");
    std::fs::create_dir(scratch.path().join("dir")).expect("create dir");
    let dir = scratch.path().to_string_lossy().replace('\\', "/");
    let out = cash(&format!(
        r"cd '{dir}'; [ $(stat -c %i dir) != 0 ] && echo inode; [ $(stat -c %d dir) = $(stat -c %d '{posix}') ] && echo device; [ alpha.txt -ef beta.txt ] && echo linked; [ dir -ef ./dir ] && echo same-dir; [ alpha.txt -ef dir ] || echo different"
    ));
    assert_eq!(out, "inode\ndevice\nlinked\nsame-dir\ndifferent");
}

/// Change is NTFS's change time, of the contents, the name or the attributes, as Git Bash's
/// stat shows it: a file modified in 2020 and made read-only now changed now. It was the
/// modification time.
#[test]
fn stat_change_is_the_files_change_time() {
    let scratch = Scratch::new("stat-change");
    let file = scratch.join("changed.txt");
    std::fs::write(&file, "x").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_mins(26_500_000))
        .unwrap();
    let mut permissions = std::fs::metadata(&file).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&file, permissions.clone()).unwrap();
    let posix = file.to_string_lossy().replace('\\', "/");
    let out = cash(&format!("stat -c '%Y %Z' '{posix}'"));
    #[expect(
        clippy::permissions_set_readonly_false,
        reason = "the scratch folder is removed after the test, its files with it"
    )]
    permissions.set_readonly(false);
    std::fs::set_permissions(&file, permissions).unwrap();
    let (modified, changed) = out.split_once(' ').unwrap();
    assert_eq!(modified, "1590000000", "{out}");
    assert!(changed.parse::<u64>().unwrap() > 1_700_000_000, "{out}");
}

/// A time is shown as Git Bash's stat shows it: in the zone an exported `TZ` names, to
/// the 100 ns NTFS keeps, with the zone's offset. It was UTC to the second, `+0000`.
#[test]
fn stat_shows_a_time_in_the_zone_to_the_tick() {
    let scratch = Scratch::new("stat-zone");
    let file = scratch.join("dated.txt");
    std::fs::write(&file, "x").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::new(1_590_000_000, 123_456_700))
        .unwrap();
    let posix = file.to_string_lossy().replace('\\', "/");
    let out = cash(&format!(
        "export TZ=UTC; stat -c %y '{posix}'; TZ=JST-9 stat -c %y '{posix}'"
    ));
    assert_eq!(
        out,
        "2020-05-20 18:40:00.123456700 +0000\n2020-05-21 03:40:00.123456700 +0900"
    );
}

/// A link is shown as Git Bash's stat shows it: `%N` and the report's `File:` line with
/// ` -> ` and its target, in cash's spelling as `readlink` gives it, and its size the
/// target's length. `%N` was the name alone, and the size 0.
#[test]
fn stat_shows_a_links_target() {
    let scratch = Scratch::new("stat-link");
    std::fs::create_dir(scratch.join("sub")).unwrap();
    cash_win32::junction::point(&scratch.join("link"), &scratch.join("sub")).unwrap();
    let posix = scratch.path().to_string_lossy().replace('\\', "/");
    let out = cash(&format!(
        "cd '{posix}' && readlink link && stat -c '%N|%s' link && stat link | head -1"
    ));
    let lines: Vec<&str> = out.lines().collect();
    let target = lines.first().copied().unwrap_or_default();
    assert_eq!(
        lines.get(1..),
        Some(
            &[
                &*format!("'link' -> '{target}'|{}", target.len()),
                &*format!("  File: link -> {target}"),
            ][..]
        ),
        "{out}"
    );
}
