//! `cash --link-tools [DIR]` (ROADMAP item 16, D65): hard links to `cash.exe`, one per
//! tool, that programs outside cash can run.
#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction, and a failed \
              assumption in a test should abort it loudly"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// A fresh folder on the same drive as the test's `cash.exe`, which a hard link needs.
fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("link_tools")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn link_tools(dir: &Path) -> Output {
    Command::new(CASH)
        .arg("--link-tools")
        .arg(dir)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

/// Run a program with `input` on its standard input, from outside any shell.
fn run(program: &Path, args: &[&str], input: &str) -> Output {
    use std::io::Write as _;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn links_every_tool_and_writes_the_manifest() {
    let dir = folder("every");
    let out = link_tools(&dir);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(stdout.contains(" linked, 0 refreshed"), "{stdout}");
    assert!(stdout.contains("SetEnvironmentVariable('Path'"), "{stdout}");

    let manifest = std::fs::read_to_string(dir.join(".cash-links")).unwrap();
    let names: Vec<&str> = manifest.lines().collect();
    for tool in ["ls", "sort", "wc", "xargs", "awk", "sed", "uname"] {
        assert!(names.contains(&tool), "{tool} missing from {manifest}");
        assert!(dir.join(format!("{tool}.exe")).is_file(), "{tool}.exe");
    }
    // Bash's own builtins have no file to run, and `[` no file name.
    for builtin in ["cd", "echo", "test", "["] {
        assert!(!names.contains(&builtin), "{builtin} in {manifest}");
    }
}

#[test]
fn a_link_runs_as_its_tool_outside_cash() {
    let dir = folder("runs");
    assert!(link_tools(&dir).status.success());

    // A bundled tool, which cash runs by re-entering its own exe: here, the link.
    let sort = run(&dir.join("sort.exe"), &[], "b\na\n");
    assert_eq!(text(&sort.stdout), "a\nb\n", "{}", text(&sort.stderr));
    assert!(sort.status.success());

    let wc = run(&dir.join("wc.exe"), &["-l"], "1\n2\n3\n");
    assert_eq!(text(&wc.stdout).trim(), "3");

    let uname = run(&dir.join("uname.exe"), &["-s"], "");
    assert_eq!(text(&uname.stdout), "Windows_NT\n");

    let basename = run(&dir.join("basename.exe"), &["a/b/c.txt", ".txt"], "");
    assert_eq!(text(&basename.stdout), "c\n");

    // A tool's own status is the process's.
    let ls = run(&dir.join("ls.exe"), &["no-such-file-here"], "");
    assert_eq!(ls.status.code(), Some(2), "{}", text(&ls.stderr));

    // `whoami` is also a link here: cash, coming up as any tool, must not start the
    // `whoami.exe` beside it for the user's SID, or each start starts another.
    let whoami = run(&dir.join("whoami.exe"), &[], "");
    assert!(whoami.status.success(), "{}", text(&whoami.stderr));
    assert!(!text(&whoami.stdout).trim().is_empty());
}

#[test]
fn a_link_that_runs_commands_runs_them() {
    let dir = folder("commands");
    assert!(link_tools(&dir).status.success());

    let xargs = run(&dir.join("xargs.exe"), &["-n1", "basename"], "a/b c/d\n");
    assert_eq!(text(&xargs.stdout), "b\nd\n", "{}", text(&xargs.stderr));

    let timeout = run(&dir.join("timeout.exe"), &["10", "sort"], "y\nx\n");
    assert_eq!(text(&timeout.stdout), "x\ny\n", "{}", text(&timeout.stderr));
}

#[test]
fn rerunning_keeps_current_links_and_leaves_other_files_alone() {
    let dir = folder("rerun");
    std::fs::write(dir.join("ls.exe"), "not cash").unwrap();
    let first = text(&link_tools(&dir).stdout);
    assert!(
        first.contains("left alone, not cash's links: ls.exe"),
        "{first}"
    );
    assert_eq!(std::fs::read(dir.join("ls.exe")).unwrap(), b"not cash");

    let again = link_tools(&dir);
    assert!(again.status.success());
    let again = text(&again.stdout);
    assert!(again.contains("all already linked"), "{again}");
    assert!(
        again.contains("left alone, not cash's links: ls.exe"),
        "{again}"
    );
}

#[test]
fn a_replaced_link_is_refreshed_and_doctor_reports_it_first() {
    let dir = folder("stale");
    assert!(link_tools(&dir).status.success());

    // What an upgrade leaves: a link to a cash.exe that is no longer this one.
    std::fs::remove_file(dir.join("wc.exe")).unwrap();
    std::fs::copy(CASH, dir.join("wc.exe")).unwrap();

    let path = format!(
        "{};{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let doctor = Command::new(CASH)
        .arg("doctor")
        .env("PATH", &path)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let report = text(&doctor.stdout);
    assert!(
        report.contains("1 of")
            && report.contains("are not this cash.exe")
            && report.contains("wc"),
        "{report}"
    );
    assert!(report.contains("cash --link-tools"), "{report}");

    let refresh = text(&link_tools(&dir).stdout);
    assert!(refresh.contains("1 refreshed"), "{refresh}");

    let doctor = Command::new(CASH)
        .arg("doctor")
        .env("PATH", &path)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let report = text(&doctor.stdout);
    assert!(report.contains("links to this cash.exe"), "{report}");
}

#[test]
fn which_prints_the_link_when_it_is_on_path() {
    let dir = folder("which");
    assert!(link_tools(&dir).status.success());
    let path = format!(
        "{};{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(CASH)
        .args(["-c", "which ls; which cd"])
        .env("PATH", &path)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let stdout = text(&out.stdout);
    let mut lines = stdout.lines();
    let ls = lines.next().unwrap_or_default().to_lowercase();
    assert!(ls.ends_with("/link_tools/which/ls.exe"), "{stdout}");
    assert_eq!(lines.next(), Some("cd: shell builtin"), "{stdout}");
}

#[test]
fn a_folder_on_another_drive_is_an_error() {
    // Only where this machine has a second drive to try.
    let here = CASH.chars().next().unwrap_or('C').to_ascii_uppercase();
    let Some(other) = ('C'..='Z')
        .filter(|letter| *letter != here)
        .find(|letter| Path::new(&format!("{letter}:\\")).is_dir())
    else {
        return;
    };
    let dir = PathBuf::from(format!(
        "{other}:\\cash-link-tools-test-{}",
        std::process::id()
    ));
    let out = link_tools(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("a hard link cannot cross drives"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn a_bare_cash_copy_is_still_the_shell() {
    // A file named like a tool but not in a manifest is cash, not the tool.
    let dir = folder("unlisted");
    let copy = dir.join("ls.exe");
    std::fs::hard_link(CASH, &copy).unwrap();
    let out = Command::new(&copy)
        .args(["-c", "echo shell"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(text(&out.stdout), "shell\n");
}
