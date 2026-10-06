//! `cash --install-finish`, `--install-remove` and `--update`: the steps the per-user
//! installer leaves to cash, run against a fake install folder with the test's `cash.exe`
//! copied in as a version. Each test has a home, a `LOCALAPPDATA` and a user `Path` key of
//! its own, so nothing of the developer's is read or written.
#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction, and a failed \
              assumption in a test should abort it loudly"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::common::CASH;

/// Registry key naming the installed fonts for a test: one that does not exist, so no
/// font is installed, whatever this machine has.
const NO_FONTS: &str = r"Software\cash-test-no-fonts";

/// `name` with this process's id and a count after it, so that no two folders, in this
/// run or another one at the same time, share a name.
fn unique(name: &str) -> String {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let count = COUNT.fetch_add(1, Ordering::Relaxed);
    format!("{name}-{}-{count}", std::process::id())
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

/// A fake install: the root the setup copies into, a home, a `LOCALAPPDATA`, and a
/// registry key standing in for the user's `Environment`; all deleted when dropped. On
/// the build's drive, as the tool links are hard links.
struct Install {
    dir: PathBuf,
    root: PathBuf,
    home: PathBuf,
    local: PathBuf,
    key_root: String,
    key: String,
}

impl Install {
    fn new(name: &str) -> Self {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("installer")
            .join(unique(name));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("Programs").join("cash");
        let home = dir.join("home");
        let local = dir.join("local");
        for folder in [&root, &home, &local] {
            std::fs::create_dir_all(folder).unwrap();
        }
        let key_root = format!(r"Software\cash-test-installer-{}", unique(name));
        let key = format!(r"{key_root}\Environment");
        Self {
            dir,
            root,
            home,
            local,
            key_root,
            key,
        }
    }

    /// A version folder with the test's `cash.exe` copied in, as the setup lays it out;
    /// that exe.
    fn version(&self, version: &str) -> PathBuf {
        let folder = self.root.join(version);
        std::fs::create_dir_all(&folder).unwrap();
        let exe = folder.join("cash.exe");
        std::fs::copy(CASH, &exe).unwrap();
        exe
    }

    /// `exe`, with this install's home, `LOCALAPPDATA` and user `Path`. Not
    /// `cash_command()`: each command is cash's first argument, reading no config and
    /// running no script.
    fn command(&self, exe: &Path) -> Command {
        let mut command = Command::new(exe);
        command
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("LOCALAPPDATA", &self.local)
            .env("CASH_USER_ENVIRONMENT_KEY", &self.key)
            .env("CASH_FONTS_KEY", NO_FONTS)
            .env_remove("CASH_UPDATE_API")
            .env_remove("CASH_UPDATE_DOWNLOAD")
            .stdin(Stdio::null());
        command
    }

    /// `exe --install-finish ROOT ARGS`.
    fn finish(&self, exe: &Path, args: &[&str]) -> Output {
        self.command(exe)
            .arg("--install-finish")
            .arg(&self.root)
            .args(args)
            .output()
            .unwrap()
    }

    fn current(&self) -> PathBuf {
        self.root.join("current")
    }

    fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    fn fragment(&self) -> PathBuf {
        self.local
            .join("Microsoft")
            .join("Windows Terminal")
            .join("Fragments")
            .join("cash")
            .join("cash.json")
    }

    /// The stored user `Path`, as `reg query` prints it.
    fn user_path(&self) -> Option<String> {
        let out = Command::new("reg")
            .args(["query", &format!(r"HKCU\{}", self.key), "/v", "Path"])
            .stderr(Stdio::null())
            .output()
            .unwrap();
        let stdout = text(&out.stdout);
        let line = stdout
            .lines()
            .find(|line| line.trim_start().starts_with("Path"))?;
        let rest = line.trim_start().strip_prefix("Path")?.trim_start();
        let (_kind, value) = rest.split_once(char::is_whitespace)?;
        Some(value.trim().to_owned())
    }

    /// The folder the `current` junction names, as `cash --install-finish` spells it.
    fn current_target(&self) -> Option<String> {
        let target = std::fs::read_link(self.current()).ok()?;
        Some(plain(&target))
    }
}

impl Drop for Install {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        let _ = Command::new("reg")
            .args(["delete", &format!(r"HKCU\{}", self.key_root), "/f"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// A path without the `\\?\` prefix, lowercased, for comparing.
fn plain(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\");
    let text = text
        .strip_prefix(r"\\?\")
        .or_else(|| text.strip_prefix(r"\??\"))
        .unwrap_or(&text);
    text.trim_end_matches('\\').to_lowercase()
}

/// `path` as the installer spells it on the user PATH: its folder real, backslashes, no
/// prefix. The name itself is kept, so the `current` junction is not followed.
fn entry(path: &Path) -> String {
    let parent = std::fs::canonicalize(path.parent().unwrap()).unwrap();
    plain(&parent.join(path.file_name().unwrap()))
}

/// The first entries of the stored user `Path`, lowercased.
fn path_entries(install: &Install) -> Vec<String> {
    install
        .user_path()
        .unwrap_or_default()
        .split(';')
        .map(|each| each.trim_end_matches('\\').to_lowercase())
        .collect()
}

#[test]
fn finish_points_current_puts_it_on_path_and_writes_the_profile_and_the_starter() {
    let install = Install::new("finish");
    let exe = install.version("9.9.9");

    // Hidden, as the setup runs it: nothing on standard output.
    let out = install.finish(&exe, &["--quiet"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "");
    assert_eq!(text(&out.stderr), "");

    assert_eq!(
        install.current_target().as_deref(),
        Some(entry(&install.root.join("9.9.9")).as_str())
    );
    assert_eq!(path_entries(&install), [entry(&install.current())]);

    // The profile runs current\cash.exe, which follows the upgrades.
    let json = std::fs::read_to_string(install.fragment()).unwrap();
    assert!(
        json.to_lowercase().contains(r"\\current\\cash.exe"),
        "{json}"
    );
    assert!(!json.contains("9.9.9"), "{json}");

    // The starter, once; and no tool links were asked for.
    assert!(install.home.join(".bashrc").is_file());
    assert!(install.local.join("cash").join("init-rc").is_file());
    assert!(!install.bin().exists());

    // Run again, aloud: one line a step, each `cash: `, and PATH is left as it is.
    let again = install.finish(&exe, &[]);
    let stdout = text(&again.stdout);
    assert!(again.status.success(), "{}", text(&again.stderr));
    assert!(
        stdout.lines().all(|line| line.starts_with("cash: ")),
        "{stdout}"
    );
    assert!(stdout.contains("current -> 9.9.9\n"), "{stdout}");
    assert!(stdout.contains("current is on the user PATH\n"), "{stdout}");
    assert!(stdout.contains("Windows Terminal profile: "), "{stdout}");
    assert!(!stdout.contains("tool links"), "{stdout}");
    assert_eq!(path_entries(&install), [entry(&install.current())]);

    // `cash doctor` reports the install.
    let doctor = install
        .command(&exe)
        .arg("doctor")
        .current_dir(&install.root)
        .output()
        .unwrap();
    let report = text(&doctor.stdout);
    let line = report
        .lines()
        .find(|line| line.split_whitespace().nth(1) == Some("install"))
        .unwrap_or_default();
    assert!(line.starts_with("  ok  "), "{report}");
    assert!(line.contains("current -> 9.9.9"), "{report}");
}

#[test]
fn links_make_the_tools_first_on_path_and_an_upgrade_refreshes_and_sweeps() {
    let install = Install::new("links");
    let old = install.version("9.9.9");
    let out = install.finish(&old, &["--links"]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(install.bin().join("ls.exe").is_file());
    assert!(install.bin().join("sed.exe").is_file());
    let manifest = std::fs::read_to_string(install.bin().join(".cash-links")).unwrap();
    assert!(manifest.lines().any(|name| name == "ls"), "{manifest}");
    assert!(
        stdout.contains(" tool links in ") && stdout.contains(", first on the user PATH\n"),
        "{stdout}"
    );
    assert_eq!(
        path_entries(&install),
        [entry(&install.bin()), entry(&install.current())]
    );

    // The upgrade: a new version folder, finished without `--links`. The manifest says
    // the links are wanted, so they are refreshed to the new exe; the old folder, whose
    // exe nothing runs, is swept.
    let new = install.version("9.9.10");
    let out = install.finish(&new, &[]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(stdout.contains("current -> 9.9.10\n"), "{stdout}");
    assert!(stdout.contains("tool links in "), "{stdout}");
    assert!(stdout.contains("cash: removed 9.9.9\n"), "{stdout}");
    assert!(!install.root.join("9.9.9").exists());
    assert_eq!(
        install.current_target().as_deref(),
        Some(entry(&install.root.join("9.9.10")).as_str())
    );
    let same = install
        .command(&new)
        .args([
            "--norc",
            "--no-config",
            "-c",
            "[ \"$1\" -ef \"$2\" ]",
            "cash",
        ])
        .arg(install.bin().join("ls.exe"))
        .arg(&new)
        .status()
        .unwrap();
    assert!(same.success(), "ls.exe is not a link to the new cash.exe");
    assert_eq!(
        path_entries(&install),
        [entry(&install.bin()), entry(&install.current())]
    );

    // The old version's doctor, from a window open since before the upgrade.
    let old_window = install.version("9.9.9");
    let doctor = install
        .command(&old_window)
        .arg("doctor")
        .current_dir(&install.root)
        .output()
        .unwrap();
    let report = text(&doctor.stdout);
    let line = report
        .lines()
        .find(|line| line.split_whitespace().nth(1) == Some("install"))
        .unwrap_or_default();
    assert!(line.starts_with("  note"), "{report}");
    assert!(
        line.contains("current -> 9.9.10") && line.contains("9.9.9"),
        "{report}"
    );
}

/// Kill a process and everything it started: `sleep` is a bundled tool, so the shell
/// re-enters its own exe for it.
fn kill_tree(child: &mut std::process::Child) {
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &child.id().to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.wait();
}

#[test]
fn a_version_folder_whose_exe_is_running_is_kept_and_an_idle_one_is_swept() {
    let install = Install::new("sweep");
    install.version("1.0.0");
    let busy = install.version("2.0.0");
    let new = install.version("3.0.0");
    let mut sleeping = install
        .command(&busy)
        .args(["--norc", "--no-config", "-c", "sleep 30"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Give the shell time to be running from its folder.
    std::thread::sleep(std::time::Duration::from_millis(500));

    let out = install.finish(&new, &[]);
    kill_tree(&mut sleeping);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(stdout.contains("cash: removed 1.0.0\n"), "{stdout}");
    assert!(
        stdout.contains("cash: kept 2.0.0: its cash.exe is running\n"),
        "{stdout}"
    );
    assert!(!install.root.join("1.0.0").exists());
    assert!(install.root.join("2.0.0").join("cash.exe").is_file());
    assert!(install.root.join("3.0.0").join("cash.exe").is_file());
    assert_eq!(
        install.current_target().as_deref(),
        Some(entry(&install.root.join("3.0.0")).as_str())
    );
}

#[test]
fn install_remove_undoes_the_links_the_path_the_profile_and_the_junction() {
    let install = Install::new("remove");
    let exe = install.version("9.9.9");
    assert!(
        install
            .finish(&exe, &["--links", "--quiet"])
            .status
            .success()
    );
    assert!(install.fragment().is_file());

    let out = install
        .command(&exe)
        .arg("--install-remove")
        .arg(&install.root)
        .output()
        .unwrap();
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(stdout.contains(" tool links removed from "), "{stdout}");
    assert!(
        stdout.contains("current taken off the user PATH\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Windows Terminal profile removed\n"),
        "{stdout}"
    );
    assert!(stdout.contains("current removed\n"), "{stdout}");

    assert!(!install.current().exists(), "the junction is still there");
    assert!(!install.bin().exists(), "the links folder is still there");
    assert!(!install.fragment().exists());
    assert_eq!(install.user_path(), None);
    // The version folder is the uninstaller's to delete; the user's files stay.
    assert!(install.root.join("9.9.9").join("cash.exe").is_file());
    assert!(install.home.join(".bashrc").is_file());

    // Run twice, as an uninstaller may be: nothing to do is no failure.
    let again = install
        .command(&exe)
        .arg("--install-remove")
        .arg(&install.root)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(again.status.success(), "{}", text(&again.stderr));
    assert_eq!(text(&again.stdout), "");
}

/// A `file://` address curl reads, for a file under the test's folder.
fn file_url(path: &Path) -> String {
    format!(
        "file:///{}",
        std::fs::canonicalize(path)
            .unwrap()
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('\\', "/")
    )
}

#[test]
fn update_check_compares_with_the_release_the_api_names() {
    let install = Install::new("check");
    let exe = install.version("9.9.9");
    assert!(install.finish(&exe, &["--quiet"]).status.success());
    let own = env!("CARGO_PKG_VERSION");

    let api = install.dir.join("latest.json");
    std::fs::write(
        &api,
        "{\n  \"tag_name\": \"v9.9.10\",\n  \"name\": \"cash 9.9.10\"\n}\n",
    )
    .unwrap();
    let out = install
        .command(&exe)
        .args(["--update", "--check"])
        .env("CASH_UPDATE_API", file_url(&api))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        format!("cash 9.9.10 is available (you have {own}): run cash --update\n")
    );

    std::fs::write(&api, "{\"tag_name\":\"v0.0.1\"}").unwrap();
    let out = install
        .command(&exe)
        .args(["--update", "--check"])
        .env("CASH_UPDATE_API", file_url(&api))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), format!("cash {own} is the latest\n"));

    // No answer: status 1, and the reason.
    let out = install
        .command(&exe)
        .args(["--update", "--check"])
        .env("CASH_UPDATE_API", file_url(&install.dir) + "/no-such.json")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).starts_with("cash: could not download file:///"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn update_fetches_verifies_unpacks_and_finishes_a_release() {
    let install = Install::new("update");
    let exe = install.version("9.9.9");
    assert!(
        install
            .finish(&exe, &["--links", "--quiet"])
            .status
            .success()
    );

    // A release, as the workflow attaches it: the zip and its `.sha256`.
    let release = install.dir.join("release");
    let stage = release.join("stage");
    std::fs::create_dir_all(&stage).unwrap();
    std::fs::copy(CASH, stage.join("cash.exe")).unwrap();
    std::fs::write(stage.join("README.md"), "# cash\n").unwrap();
    let name = "cash-v9.9.10-x86_64-pc-windows-msvc.zip";
    let zip = release.join(name);
    let packed = Command::new(r"C:\Windows\System32\tar.exe")
        .arg("-a")
        .arg("-cf")
        .arg(&zip)
        .arg("-C")
        .arg(&stage)
        .args(["cash.exe", "README.md"])
        .output()
        .unwrap();
    assert!(packed.status.success(), "{}", text(&packed.stderr));
    let api = install.dir.join("latest.json");
    std::fs::write(&api, "{\"tag_name\":\"v9.9.10\"}").unwrap();
    let sum = release.join(format!("{name}.sha256"));

    // A checksum that does not match: nothing is unpacked, nothing installed.
    std::fs::write(&sum, format!("{}  {name}\n", "0".repeat(64))).unwrap();
    let out = install
        .command(&exe)
        .arg("--update")
        .env("CASH_UPDATE_API", file_url(&api))
        .env("CASH_UPDATE_DOWNLOAD", file_url(&release))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stdout));
    assert!(
        text(&out.stderr).contains("does not match its .sha256"),
        "{}",
        text(&out.stderr)
    );
    assert!(!install.root.join("9.9.10").exists());
    assert_eq!(
        install.current_target().as_deref(),
        Some(entry(&install.root.join("9.9.9")).as_str())
    );

    // The real checksum, by the cash under test's own sha256sum.
    let hashed = Command::new(CASH)
        .args(["--norc", "--no-config", "-c", "sha256sum \"$1\"", "cash"])
        .arg(&zip)
        .output()
        .unwrap();
    let hash = text(&hashed.stdout)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned();
    assert_eq!(hash.len(), 64, "{}", text(&hashed.stderr));
    std::fs::write(&sum, format!("{hash}  {name}\n")).unwrap();

    let out = install
        .command(&exe)
        .arg("--update")
        .env("CASH_UPDATE_API", file_url(&api))
        .env("CASH_UPDATE_DOWNLOAD", file_url(&release))
        .output()
        .unwrap();
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.contains("cash: downloading cash-v9.9.10-"),
        "{stdout}"
    );
    assert!(stdout.contains("cash: the SHA-256 matches\n"), "{stdout}");
    assert!(stdout.contains("cash: unpacked into "), "{stdout}");
    // The new exe's finish step: the junction moved, the links refreshed, and the
    // version running this update kept, since it is running.
    assert!(stdout.contains("current -> 9.9.10\n"), "{stdout}");
    assert!(stdout.contains("tool links in "), "{stdout}");
    assert!(
        stdout.contains("cash: kept 9.9.9: its cash.exe is running\n"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with(&format!(
            "cash 9.9.10 is installed: new windows get it; this one keeps {}\n",
            env!("CARGO_PKG_VERSION")
        )),
        "{stdout}"
    );
    assert!(install.root.join("9.9.10").join("cash.exe").is_file());
    assert!(install.root.join("9.9.10").join("README.md").is_file());
    assert_eq!(
        install.current_target().as_deref(),
        Some(entry(&install.root.join("9.9.10")).as_str())
    );
    // The downloads are cleaned up.
    assert!(!std::env::temp_dir().join(name).exists());
}

#[test]
fn update_refuses_a_cash_outside_the_installers_layout() {
    let install = Install::new("plain");
    // A zip unpacked anywhere: no `current` junction beside the folder.
    let plain = install.dir.join("unzipped");
    std::fs::create_dir_all(&plain).unwrap();
    let exe = plain.join("cash.exe");
    std::fs::copy(CASH, &exe).unwrap();
    let out = install
        .command(&exe)
        .args(["--update", "--check"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        text(&out.stderr),
        "cash: this cash was not installed by the installer; download the new version from \
         https://github.com/tomcoolpxl/cash/releases\n"
    );
}

#[test]
fn a_cash_scoop_installed_refuses_the_installers_commands() {
    let install = Install::new("scoop");
    let folder = install
        .dir
        .join("scoop")
        .join("apps")
        .join("cash")
        .join("9.9.9");
    std::fs::create_dir_all(&folder).unwrap();
    let exe = folder.join("cash.exe");
    std::fs::copy(CASH, &exe).unwrap();

    let out = install.finish(&exe, &[]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        text(&out.stderr),
        "cash: this cash was installed by Scoop; use scoop update cash\n"
    );
    assert!(!install.current().exists());

    let out = install
        .command(&exe)
        .args(["--update", "--check"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        text(&out.stderr),
        "cash: installed by Scoop: run scoop update cash\n"
    );
}

#[test]
fn the_scoop_task_leaves_an_installed_scoop_alone() {
    let install = Install::new("scoop-task");
    let exe = install.version("9.9.9");
    // Scoop as its installer leaves it, under the user's profile.
    let shims = install.home.join("scoop").join("shims");
    std::fs::create_dir_all(&shims).unwrap();
    std::fs::write(shims.join("scoop.ps1"), "# scoop\n").unwrap();

    let out = install.finish(&exe, &["--scoop"]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        stdout.contains("cash: Scoop is installed already ("),
        "{stdout}"
    );
    assert!(!stdout.contains("installing Scoop"), "{stdout}");
}

#[test]
fn an_extra_or_missing_argument_is_refused() {
    let install = Install::new("usage");
    let exe = install.version("9.9.9");
    let out = install
        .command(&exe)
        .arg("--install-finish")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("usage:"),
        "{}",
        text(&out.stderr)
    );
    let out = install.finish(&exe, &["--bogus"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!install.current().exists());
}
