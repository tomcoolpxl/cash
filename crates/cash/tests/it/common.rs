//! What the tests of this executable share: the binary, a run of it and its output, and
//! a scratch folder.
//!
//! A test runs cash as a user would, but not with the user's settings: without the
//! developer's `%APPDATA%\cash\config.toml` (`--no-config`), and without the variables
//! that make a non-interactive shell run code or behave differently before the script
//! starts (`BASH_ENV`, `ENV`, `FUNCNEST`, `CDPATH`, `GLOBIGNORE`). Before this module,
//! 43 modules had a run helper of their own and most passed the developer's environment
//! straight through (`REVIEW_REPORT.md` BIN-18, BIN-20).
//!
//! A scratch folder's name carries the process id and a counter, so two test runs at
//! once, another session's included, never empty each other's folders (BIN-19).
//!
//! Modules move to these helpers as they are touched.

#![allow(
    dead_code,
    clippy::expect_used,
    reason = "a helper that no module calls yet is not an error while modules move here \
              one at a time, and a test that cannot even start cash should stop loudly"
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
    for name in ISOLATED_VARIABLES {
        command.env_remove(name);
    }
    command
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
