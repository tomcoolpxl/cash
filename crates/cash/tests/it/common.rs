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

/// The folder of the oracle scripts and their golden files, `tests/oracle`.
pub fn oracle_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("oracle")
}

/// Runs `tests/oracle/NAME.sh` under cash with no standard input: its standard output
/// and error together, as the golden file was made.
pub fn run_oracle_script(name: &str) -> String {
    let out = cash_command()
        .arg(format!("{name}.sh"))
        .current_dir(oracle_dir())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run cash");
    String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr)
}

/// `tests/oracle/NAME.out`, the original tool's output, with Unix line ends.
pub fn golden(name: &str) -> String {
    std::fs::read_to_string(oracle_dir().join(format!("{name}.out")))
        .expect("read golden output")
        .replace("\r\n", "\n")
}

/// `golden` with `from` replaced by `to`, where cash differs on purpose; `from` must
/// occur exactly `times` times, so a difference cannot hide in the golden file.
pub fn with_divergence(golden: &str, from: &str, to: &str, times: usize) -> String {
    assert_eq!(
        golden.matches(from).count(),
        times,
        "golden text moved: {from}"
    );
    golden.replace(from, to)
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

/// One finding of a `cash doctor` report, its wrapped lines joined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorFinding {
    /// `ok`, `note` or `WARN`.
    pub level: String,
    /// What was found, with the rows under it.
    pub text: String,
    /// What to do about it: the text after the arrow.
    pub fix: Option<String>,
}

/// The findings of a `cash doctor` report. A finding starts with its marker two columns
/// in; its text, its rows and its fix go on in the lines indented under it.
pub fn doctor_findings(report: &str) -> Vec<DoctorFinding> {
    let mut findings: Vec<DoctorFinding> = Vec::new();
    let mut in_fix = false;
    for line in report.lines() {
        let marker = ["ok", "note", "WARN"].into_iter().find_map(|level| {
            let rest = line.strip_prefix("  ")?.strip_prefix(level)?;
            rest.starts_with("  ").then(|| (level, rest.trim()))
        });
        if let Some((level, text)) = marker {
            findings.push(DoctorFinding {
                level: level.to_owned(),
                text: text.to_owned(),
                fix: None,
            });
            in_fix = false;
            continue;
        }
        // A blank line, the header or the verdict: no finding goes on.
        if !line.starts_with("        ") {
            in_fix = false;
            continue;
        }
        let Some(finding) = findings.last_mut() else {
            continue;
        };
        if let Some(fix) = line.trim_start().strip_prefix("→ ") {
            finding.fix = Some(fix.to_owned());
            in_fix = true;
        } else {
            let part = if in_fix {
                finding.fix.get_or_insert_with(String::new)
            } else {
                &mut finding.text
            };
            part.push(' ');
            part.push_str(line.trim());
        }
    }
    findings
}

/// The first line of a `cash doctor` report, `cash doctor · cash VERSION in FOLDER`, with
/// a wrapped folder joined back on.
pub fn doctor_header(report: &str) -> String {
    report
        .split("\n\n")
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The first finding of `report` whose text contains `text`.
pub fn doctor_finding(report: &str, text: &str) -> Option<DoctorFinding> {
    doctor_findings(report)
        .into_iter()
        .find(|finding| finding.text.contains(text))
}
