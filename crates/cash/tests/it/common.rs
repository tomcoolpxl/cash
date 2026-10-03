//! What the tests of this executable share: the binary, a run of it and its output, and
//! a scratch folder.
//!
//! A test runs cash as a user would, but not with the user's settings: without the
//! developer's `%APPDATA%\cash\config.toml` (`--no-config`, and an empty `APPDATA` for
//! the cash it starts in turn), and without the variables
//! that make a non-interactive shell run code or behave differently before the script
//! starts (`BASH_ENV`, `ENV`, `FUNCNEST`, `CDPATH`, `GLOBIGNORE`). Before this module,
//! 43 modules had a run helper of their own and most passed the developer's environment
//! straight through (`REVIEW_REPORT.md` BIN-18, BIN-20).
//!
//! A scratch folder's name carries the process id and a counter, so two test runs at
//! once, another session's included, never empty each other's folders (BIN-19).
//!
//! Every module starts cash from here. Where a test cannot take `cash_command()` whole
//! (a subcommand such as `cash doctor` that must be cash's first argument), it uses
//! [`CASH`] and says why.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a test that cannot even start cash, or lacks what it needs, should stop loudly"
)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

/// The cash under test.
pub const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// Variables a test must not inherit from the developer's environment.
pub const ISOLATED_VARIABLES: [&str; 5] = ["BASH_ENV", "ENV", "FUNCNEST", "CDPATH", "GLOBIGNORE"];

/// What a run of cash printed and how it ended.
pub struct Output {
    /// Standard output, its trailing whitespace removed.
    pub stdout: String,
    /// Standard error, its trailing whitespace removed.
    pub stderr: String,
    /// The exit status, or -1 if there was none.
    pub code: i32,
}

/// A command that runs cash, isolated from the user's settings; add the arguments.
pub fn cash_command() -> Command {
    let mut command = Command::new(CASH);
    command.arg("--no-config");
    isolate(&mut command);
    command
}

/// The environment `cash_command()` gives cash, on a command built some other way: no
/// [`ISOLATED_VARIABLES`], and the empty `APPDATA`.
pub fn isolate(command: &mut Command) -> &mut Command {
    for name in ISOLATED_VARIABLES {
        command.env_remove(name);
    }
    command.env("APPDATA", no_appdata())
}

/// Calls `start` with the environment `cash_command()` gives cash, for a cash started some
/// other way, as on a pseudo terminal, which takes a whole environment block: the test's
/// own, without [`ISOLATED_VARIABLES`] and with the empty `APPDATA`.
pub fn with_isolated_environment<R>(start: impl FnOnce(&[(&str, &str)]) -> R) -> R {
    let appdata = no_appdata().to_string_lossy().into_owned();
    let vars: Vec<(String, String)> = std::env::vars()
        .filter(|(name, _)| {
            !name.eq_ignore_ascii_case("APPDATA")
                && !ISOLATED_VARIABLES
                    .iter()
                    .any(|isolated| name.eq_ignore_ascii_case(isolated))
        })
        .collect();
    let mut env: Vec<(&str, &str)> = vars
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    env.push(("APPDATA", &appdata));
    start(&env)
}

/// An empty folder for `%APPDATA%`, where cash looks for `cash\config.toml`. `--no-config`
/// keeps the developer's config from the cash a test starts; this keeps it from every cash
/// that one starts in turn (a shebang script, `sh -c`, `bash -c`), which inherit the
/// variable and take no flag. Shared by every test and never written, so its fixed name
/// is safe.
fn no_appdata() -> PathBuf {
    let dir = std::env::temp_dir().join("cash-it-no-appdata");
    std::fs::create_dir_all(&dir).expect("create the empty APPDATA folder");
    dir
}

/// Runs `command` to its end.
pub fn output_of(command: &mut Command) -> Output {
    let out = command.output().expect("failed to run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// Runs `script` with `cash -c`.
pub fn run(script: &str) -> Output {
    output_of(cash_command().args(["-c", script]))
}

/// Runs `script` with `cash -c`, in the folder `dir`.
pub fn run_in(dir: &Path, script: &str) -> Output {
    output_of(cash_command().args(["-c", script]).current_dir(dir))
}

/// The folder Git for Windows is installed in, found through the `git` on `PATH`
/// (`git --exec-path` is `<root>/mingw64/libexec/git-core`), with forward slashes.
///
/// Git for Windows is a prerequisite of cash's workload (spec D35), and CI has it: a test
/// that needs it fails without it, saying so, rather than passing without a word
/// (BIN-20; the user, 2026-10-04).
pub fn git_for_windows() -> String {
    let output = Command::new("git")
        .arg("--exec-path")
        .output()
        .expect("git, from Git for Windows, which this test needs (spec D35)");
    let exec_path = String::from_utf8_lossy(&output.stdout)
        .trim()
        .replace('\\', "/");
    let root = exec_path
        .strip_suffix("/mingw64/libexec/git-core")
        .unwrap_or_else(|| {
            panic!("`git --exec-path` is {exec_path:?}, not Git for Windows' mingw64 layout")
        });
    assert!(
        Path::new(root).join("usr/bin").is_dir(),
        "Git for Windows at {root} has no usr/bin"
    );
    root.to_owned()
}

/// A folder of the test's own, empty when made and removed when dropped.
pub struct Scratch(PathBuf);

impl Scratch {
    /// A new, empty scratch folder; `name` says which test it is for.
    pub fn new(name: &str) -> Self {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let count = COUNT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("cash-it-{name}-{}-{count}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the scratch folder");
        Self(dir)
    }

    /// The folder.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// `path` inside the folder.
    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.0.join(path)
    }

    /// The folder in cash's canonical spelling, safe to paste into a script.
    pub fn as_script_path(&self) -> String {
        self.0.to_string_lossy().replace('\\', "/")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
