//! `cash --add-to-path`, `cash --remove-from-path` and `cash doctor`'s line about PATH: the
//! folder that stands for this `cash.exe` on the user PATH, per layout. The test's
//! `cash.exe` is copied into a scratch folder laid out as a portable copy, as Scoop's
//! install or as the installer's. Each test has a home, a `LOCALAPPDATA` and a user `Path`
//! key of its own, so nothing of the developer's is read or written; the machine PATH is
//! the real one, only read, and no scratch folder is on it.
#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a failed \
              assumption in a test should abort it loudly"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::common::{CASH, DoctorFinding, doctor_findings};

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

/// A path without the `\\?\` prefix, lowercased, for comparing.
fn plain(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\");
    let text = text
        .strip_prefix(r"\\?\")
        .or_else(|| text.strip_prefix(r"\??\"))
        .unwrap_or(&text);
    text.trim_end_matches('\\').to_lowercase()
}

/// `path` as cash spells it on the user PATH: its folder real, backslashes, no prefix.
/// The name itself is kept, so a `current` junction is not followed.
fn entry(path: &Path) -> String {
    let parent = std::fs::canonicalize(path.parent().unwrap()).unwrap();
    plain(&parent.join(path.file_name().unwrap()))
}

/// A copy of the test's `cash.exe` in a layout of its own, with a home, a `LOCALAPPDATA`
/// and a registry key standing in for the user's `Environment`; all deleted when dropped.
struct Copy {
    dir: PathBuf,
    exe: PathBuf,
    home: PathBuf,
    local: PathBuf,
    key_root: String,
    key: String,
}

impl Copy {
    /// The exe in `<dir>\<folders...>\cash.exe`.
    fn under(name: &str, folders: &[&str]) -> Self {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("own-path")
            .join(unique(name));
        let _ = std::fs::remove_dir_all(&dir);
        let mut exe_dir = dir.clone();
        for folder in folders {
            exe_dir.push(folder);
        }
        let home = dir.join("home");
        let local = dir.join("local");
        for folder in [&exe_dir, &home, &local] {
            std::fs::create_dir_all(folder).unwrap();
        }
        let exe = exe_dir.join("cash.exe");
        std::fs::copy(CASH, &exe).unwrap();
        let key_root = format!(r"Software\cash-test-own-path-{}", unique(name));
        let key = format!(r"{key_root}\Environment");
        Self {
            dir,
            exe,
            home,
            local,
            key_root,
            key,
        }
    }

    /// `cash ARGS`, with this copy's home, `LOCALAPPDATA` and user `Path`. Not
    /// `cash_command()`: each command is cash's first argument, reading no config and
    /// running no script.
    fn cash(&self, args: &[&str]) -> Output {
        Command::new(&self.exe)
            .args(args)
            .current_dir(&self.dir)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("LOCALAPPDATA", &self.local)
            .env("CASH_USER_ENVIRONMENT_KEY", &self.key)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    /// What `cash doctor` says about PATH.
    fn doctor_path_line(&self) -> DoctorFinding {
        let out = self.cash(&["doctor"]);
        let report = text(&out.stdout);
        doctor_findings(&report)
            .into_iter()
            .find(|finding| {
                [" on your user PATH", " on the system PATH", " on no PATH"]
                    .iter()
                    .any(|said| finding.text.contains(said))
            })
            .unwrap_or_else(|| panic!("no line about PATH in:\n{report}"))
    }

    /// Stores `value` as this copy's user `Path`, as a user's own entries would be.
    fn seed_user_path(&self, value: &str) {
        let status = Command::new("reg")
            .args([
                "add",
                &format!(r"HKCU\{}", self.key),
                "/v",
                "Path",
                "/t",
                "REG_EXPAND_SZ",
                "/d",
                value,
                "/f",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "reg add failed");
    }

    /// The stored user `Path`, as `reg query` prints it; `None` when there is none.
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

    /// The entries of the stored user `Path`, lowercased.
    fn path_entries(&self) -> Vec<String> {
        self.user_path()
            .unwrap_or_default()
            .split(';')
            .filter(|each| !each.is_empty())
            .map(|each| each.trim_end_matches('\\').to_lowercase())
            .collect()
    }
}

impl Drop for Copy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        let _ = Command::new("reg")
            .args(["delete", &format!(r"HKCU\{}", self.key_root), "/f"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// A folder as cash shows it: real, backslashes, no prefix, the disk's case. A junction
/// is followed, so for `current` the caller shows its parent and adds the name.
fn shown(folder: &Path) -> String {
    let folder = std::fs::canonicalize(folder).unwrap();
    let text = folder.to_string_lossy().replace('/', "\\");
    text.strip_prefix(r"\\?\")
        .unwrap_or(&text)
        .trim_end_matches('\\')
        .to_owned()
}

/// A folder [`shown`] as `cash doctor` writes it: with forward slashes.
fn in_doctor(shown: &str) -> String {
    shown.replace('\\', "/")
}

/// The report: a portable cash is noted by doctor with the fix; `--add-to-path` puts its
/// folder first on the user PATH and doctor says `ok`; a second add changes nothing;
/// `--remove-from-path` takes it off, and doctor notes it again.
#[test]
fn a_portable_cash_is_noted_added_first_and_removed() {
    let copy = Copy::under("portable", &["unpacked"]);
    let folder = shown(copy.exe.parent().unwrap());
    // A user with entries of their own: cash's goes before them, and they stay.
    copy.seed_user_path(r"C:\users-own\bin;%USERPROFILE%\go\bin");

    let line = copy.doctor_path_line();
    assert_eq!(line.level, "note", "{line:?}");
    assert_eq!(
        line.text,
        format!(
            "{} is on no PATH, so new windows will not find cash",
            in_doctor(&folder)
        )
    );
    assert_eq!(line.fix.as_deref(), Some("cash --add-to-path"));

    let out = copy.cash(&["--add-to-path"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        format!("cash: {folder} added to your user PATH: windows opened from now on have cash\n")
    );
    assert_eq!(text(&out.stderr), "");
    assert_eq!(
        copy.path_entries(),
        [
            entry(copy.exe.parent().unwrap()),
            r"c:\users-own\bin".to_owned(),
            r"%userprofile%\go\bin".to_owned()
        ]
    );

    let line = copy.doctor_path_line();
    assert_eq!(line.level, "ok", "{line:?}");
    assert_eq!(
        line.text,
        format!("{} is on your user PATH", in_doctor(&folder))
    );
    assert_eq!(line.fix, None);

    // Again: there already, left where it is.
    let out = copy.cash(&["--add-to-path"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        format!("cash: {folder} is on your user PATH already\n")
    );
    assert_eq!(copy.path_entries().len(), 3);

    let out = copy.cash(&["--remove-from-path"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        format!("cash: {folder} removed from your user PATH\n")
    );
    assert_eq!(
        copy.path_entries(),
        [
            r"c:\users-own\bin".to_owned(),
            r"%userprofile%\go\bin".to_owned()
        ]
    );
    let line = copy.doctor_path_line();
    assert_eq!(line.level, "note", "{line:?}");
    assert_eq!(line.fix.as_deref(), Some("cash --add-to-path"));

    // Again: nothing to take off is no failure.
    let out = copy.cash(&["--remove-from-path"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        format!("cash: {folder} was not on your user PATH\n")
    );
}

/// The report: a cash under `scoop\apps\` is Scoop's, whose shims folder is what is on
/// PATH; cash reports on it and never adds or removes it.
#[test]
fn scoops_copy_names_its_shims_folder_and_leaves_it_to_scoop() {
    let copy = Copy::under("scoop", &["scoop", "apps", "cash", "current"]);
    let shims = copy.dir.join("scoop").join("shims");
    let shown = format!("{}\\shims", shown(&copy.dir.join("scoop")));

    let line = copy.doctor_path_line();
    assert_eq!(line.level, "note", "{line:?}");
    assert_eq!(
        line.text,
        format!(
            "{} is on no PATH, so new windows will not find cash",
            in_doctor(&shown)
        )
    );
    assert_eq!(line.fix.as_deref(), Some("scoop reset cash"));

    let out = copy.cash(&["--add-to-path"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        text(&out.stderr),
        format!("cash: installed by Scoop, but its shims folder {shown} is on no PATH\n")
    );
    assert_eq!(copy.user_path(), None);

    // As Scoop's installer leaves it: the shims folder on the user PATH.
    copy.seed_user_path(&shims.to_string_lossy());
    let out = copy.cash(&["--add-to-path"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "cash: installed by Scoop: its shims folder is already on PATH\n"
    );
    let line = copy.doctor_path_line();
    assert_eq!(line.level, "ok", "{line:?}");
    assert_eq!(
        line.text,
        format!("{} is on your user PATH", in_doctor(&shown))
    );
    assert_eq!(line.fix, None);

    let out = copy.cash(&["--remove-from-path"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("installed by Scoop") && text(&out.stderr).contains(&shown),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(copy.path_entries(), [plain(&shims)]);
}

/// The report: a cash in the installer's layout adds `current`, the junction beside its
/// version folder, never the version folder itself.
#[test]
fn an_installed_cash_adds_current_not_its_version_folder() {
    let copy = Copy::under("installed", &["Programs", "cash", "9.9.9"]);
    let root = copy.dir.join("Programs").join("cash");
    let current = root.join("current");
    cash_win32::junction::point(&current, &root.join("9.9.9")).unwrap();
    let shown_current = format!("{}\\current", shown(&root));

    let line = copy.doctor_path_line();
    assert_eq!(line.level, "note", "{line:?}");
    assert_eq!(
        line.text,
        format!(
            "{} is on no PATH, so new windows will not find cash",
            in_doctor(&shown_current)
        )
    );
    assert_eq!(line.fix.as_deref(), Some("cash --add-to-path"));

    let out = copy.cash(&["--add-to-path"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        format!(
            "cash: {shown_current} added to your user PATH: windows opened from now on have cash\n"
        )
    );
    assert_eq!(copy.path_entries(), [entry(&current)]);
    assert!(!copy.user_path().unwrap().contains("9.9.9"));

    let line = copy.doctor_path_line();
    assert_eq!(line.level, "ok", "{line:?}");
    assert_eq!(
        line.text,
        format!("{} is on your user PATH", in_doctor(&shown_current))
    );

    let out = copy.cash(&["--remove-from-path"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        format!("cash: {shown_current} removed from your user PATH\n")
    );
    assert_eq!(copy.user_path(), None);
}

/// The report: an argument is refused with the usage, and `--help` prints it.
#[test]
fn an_extra_argument_is_refused() {
    let copy = Copy::under("usage", &["unpacked"]);
    let out = copy.cash(&["--add-to-path", "C:\\x"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("usage: cash --add-to-path"),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(copy.user_path(), None);

    let out = copy.cash(&["--remove-from-path", "--help"]);
    assert!(out.status.success());
    assert!(
        text(&out.stdout).contains("cash --remove-from-path"),
        "{}",
        text(&out.stdout)
    );
}
