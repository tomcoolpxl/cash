//! Check commands for code quality validation.
//!
//! This module provides various code quality checks that can be run individually
//! or as part of a CI workflow. Each check wraps an external tool and provides
//! consistent error handling and verbose output.
//!
//! `check unused-deps` needs `cargo-udeps` (`cargo install cargo-udeps`) and a nightly
//! toolchain; the other checks need only the pinned toolchain.
//!
//! `build` and `lint` check the default features, the set that is shipped and tested, so
//! they reuse what `cargo test` already built. Each other feature set is a whole separate
//! build of the workspace; `--all-features` asks for one, and CI does.

use anyhow::{Context, Result};
use clap::Parser;
use xshell::{Shell, cmd};

/// Run code quality checks.
#[derive(Parser)]
pub enum CheckCommand {
    /// Check that the code compiles.
    Build(BuildArgs),
    /// Check code formatting.
    Fmt,
    /// Run clippy lints.
    Lint(LintArgs),
    /// Check for unused dependencies (requires nightly).
    UnusedDeps,
    /// Check that the shell's crates take the working directory and environment from the
    /// shell, not from the process.
    ProcessState,
    /// Check that no crate but `cash-core` names `DefaultShellExtensions` outside its tests,
    /// which compiles every builtin again for it.
    OneExtensions,
}

/// Options for the build check.
#[derive(Default, Parser)]
pub struct BuildArgs {
    /// Only check crates that build with the workspace-wide MSRV; for use when
    /// the check is being run with the oldest toolchain the workspace as a
    /// whole supports.
    #[clap(long = "workspace-msrv")]
    workspace_msrv: bool,

    /// Check with every feature enabled instead of the default ones.
    #[clap(long)]
    all_features: bool,
}

/// Options for the lint check.
#[derive(Default, Parser)]
pub struct LintArgs {
    /// Lint with every feature enabled instead of the default ones.
    #[clap(long)]
    all_features: bool,
}

/// Run a check command.
pub fn run(cmd: &CheckCommand, verbose: bool) -> Result<()> {
    let sh = Shell::new()?;

    match cmd {
        CheckCommand::Fmt => check_fmt(&sh, verbose),
        CheckCommand::Lint(args) => check_lint(&sh, args, verbose),
        CheckCommand::UnusedDeps => check_unused_deps(&sh, verbose),
        CheckCommand::ProcessState => check_process_state(verbose),
        CheckCommand::OneExtensions => check_one_extensions(verbose),
        CheckCommand::Build(args) => check_build(&sh, args, verbose),
    }
}

fn check_fmt(sh: &Shell, verbose: bool) -> Result<()> {
    eprintln!("Checking code formatting...");
    if verbose {
        eprintln!("Running: cargo fmt --check --all");
    }
    cmd!(sh, "cargo fmt --check --all")
        .run()
        .context("Format check failed")?;
    eprintln!("Format check passed.");
    Ok(())
}

fn check_lint(sh: &Shell, lint_args: &LintArgs, verbose: bool) -> Result<()> {
    eprintln!("Running clippy...");
    let mut args = vec!["clippy", "--workspace", "--all-targets"];
    if lint_args.all_features {
        args.push("--all-features");
    }
    if verbose {
        args.push("--verbose");
        eprintln!("Running: cargo {}", args.join(" "));
    }
    cmd!(sh, "cargo {args...}")
        .run()
        .context("Clippy check failed")?;
    eprintln!("Clippy check passed.");
    Ok(())
}

fn check_unused_deps(sh: &Shell, verbose: bool) -> Result<()> {
    eprintln!("Checking for unused dependencies (requires nightly)...");
    if verbose {
        eprintln!("Running: cargo +nightly udeps --workspace --all-targets --all-features");
    }
    cmd!(
        sh,
        "cargo +nightly udeps --workspace --all-targets --all-features"
    )
    .run()
    .context("Unused dependency check failed")?;
    eprintln!("Unused dependency check passed.");
    Ok(())
}

/// Turns a `rust-version` value into something orderable.
fn msrv_key(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// Finds the workspace crates that declare a `rust-version` higher than the
/// lowest one in the workspace, and so can't be built with the oldest toolchain
/// the workspace as a whole supports. Derived from cargo metadata so that the
/// manifests remain the only place this is recorded.
fn crates_above_workspace_msrv(sh: &Shell) -> Result<Vec<String>> {
    #[derive(serde::Deserialize)]
    struct Metadata {
        packages: Vec<Package>,
    }

    #[derive(serde::Deserialize)]
    struct Package {
        name: String,
        rust_version: Option<String>,
    }

    // `--no-deps` narrows the output to workspace members.
    let json = cmd!(sh, "cargo metadata --no-deps --format-version 1")
        .quiet()
        .read()
        .context("Failed to read cargo metadata")?;
    let metadata: Metadata =
        serde_json::from_str(&json).context("Failed to parse cargo metadata")?;

    let Some(workspace_msrv) = metadata
        .packages
        .iter()
        .filter_map(|p| p.rust_version.as_deref())
        .map(msrv_key)
        .min()
    else {
        return Ok(Vec::new());
    };

    Ok(metadata
        .packages
        .iter()
        .filter(|p| {
            p.rust_version
                .as_deref()
                .is_some_and(|v| msrv_key(v) > workspace_msrv)
        })
        .map(|p| p.name.clone())
        .collect())
}

fn check_build(sh: &Shell, build_args: &BuildArgs, verbose: bool) -> Result<()> {
    eprintln!("Checking that code compiles...");

    let excluded = if build_args.workspace_msrv {
        crates_above_workspace_msrv(sh)?
    } else {
        Vec::new()
    };
    if !excluded.is_empty() {
        eprintln!(
            "Skipping crates with a higher MSRV than the workspace: {}",
            excluded.join(", ")
        );
    }

    let mut args = vec!["check", "--all-targets", "--workspace"];
    if build_args.all_features {
        args.push("--all-features");
    }
    for name in &excluded {
        args.push("--exclude");
        args.push(name);
    }
    if verbose {
        args.push("--verbose");
        eprintln!("Running: cargo {}", args.join(" "));
    }
    cmd!(sh, "cargo {args...}")
        .run()
        .context("Build check failed")?;
    eprintln!("Build check passed.");
    Ok(())
}

/// Calls that take the cash *process*'s state: its working directory, its environment,
/// or a program started with both.
///
/// cash keeps its working directory and exported variables in the shell (D10) and never
/// moves the process's, so in the shell's crates each of these is a bug unless the line,
/// or the one before it, says why it is not (`// process state: <reason>`): `cd sub &&
/// ./tool.exe` ran the start directory's `tool.exe`, `xargs` lost `export`, `PATHEXT` and
/// `umask` were the process's (`REVIEW_REPORT.md` §4.1).
const PROCESS_STATE_CALLS: [&str; 5] = [
    "Command::new(",
    "env::var(",
    "env::var_os(",
    "env::current_dir(",
    "set_current_dir(",
];

/// The crates that run shell code, whose state is the shell's.
const PROCESS_STATE_CRATES: [&str; 2] = ["crates/cash-core/src", "crates/cash-builtins/src"];

/// The marker that says why a call may take the process's state.
const PROCESS_STATE_MARKER: &str = "process state:";

/// Fails on a call in [`PROCESS_STATE_CRATES`] that takes the process's state with no
/// reason given.
fn check_process_state(verbose: bool) -> Result<()> {
    let root = crate::common::find_workspace_root()?;
    let mut files = Vec::new();
    for dir in PROCESS_STATE_CRATES {
        collect_rust_files(&root.join(dir), &mut files)?;
    }
    files.sort();

    let mut unmarked = Vec::new();
    for file in &files {
        let text =
            std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        let lines: Vec<&str> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            if line.trim_start().starts_with("//") || !takes_process_state(line) {
                continue;
            }
            let reason_before = index
                .checked_sub(1)
                .and_then(|previous| lines.get(previous))
                .is_some_and(|previous| previous.contains(PROCESS_STATE_MARKER));
            if !line.contains(PROCESS_STATE_MARKER) && !reason_before {
                let shown = file.strip_prefix(&root).unwrap_or(file);
                unmarked.push(format!(
                    "{}:{}: {}",
                    shown.display(),
                    index + 1,
                    line.trim()
                ));
            }
        }
    }

    if verbose {
        eprintln!("Checked {} files for the process's state.", files.len());
    }
    if unmarked.is_empty() {
        eprintln!("Process state check passed.");
        return Ok(());
    }
    for found in &unmarked {
        eprintln!("{found}");
    }
    anyhow::bail!(
        "{} call(s) take the process's working directory or environment, which in the \
         shell's crates are the shell's (D10). Use the shell's (`Shell::working_dir`, \
         `env_str`, `pathext`, `commands::run_for_builtin`), or say why on the line \
         before: `// {PROCESS_STATE_MARKER} <reason>`.",
        unmarked.len()
    )
}

/// Whether `line` calls one of [`PROCESS_STATE_CALLS`], as a whole name: `SimpleCommand::new`
/// is not `Command::new`.
fn takes_process_state(line: &str) -> bool {
    PROCESS_STATE_CALLS.iter().any(|call| {
        line.match_indices(call).any(|(at, _)| {
            !line
                .get(..at)
                .unwrap_or_default()
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
    })
}

/// The shell's crates generic over the shell's extensions are compiled once for each type
/// they are used with, in each crate that uses it. The shell has its own (`cash-shell`'s
/// `BrushShellExtensions`); a call with `DefaultShellExtensions` outside a test compiled
/// every builtin again: 1.34.0's `cash.exe` was 4.7 MB larger for `env`'s question, and
/// `cash doctor`'s list of names cost as much again.
const ONE_EXTENSIONS_NAME: &str = "DefaultShellExtensions";

/// Fails on a crate but `cash-core`, which defines it, that names
/// [`ONE_EXTENSIONS_NAME`] in code that is not a test: before its file's
/// `#[cfg(test)]`, or out of `tests` and `benches`.
fn check_one_extensions(verbose: bool) -> Result<()> {
    let root = crate::common::find_workspace_root()?;
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root.join("crates")).context("reading crates")? {
        let crate_dir = entry?.path();
        if crate_dir
            .file_name()
            .is_some_and(|name| name == "cash-core")
        {
            continue;
        }
        let src = crate_dir.join("src");
        if src.is_dir() {
            collect_rust_files(&src, &mut files)?;
        }
    }
    files.sort();

    let mut found = Vec::new();
    for file in &files {
        let text =
            std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        for (index, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("#[cfg(test)]") {
                break;
            }
            if !code.starts_with("//") && names_other_extensions(code) {
                let shown = file.strip_prefix(&root).unwrap_or(file);
                found.push(format!("{}:{}: {}", shown.display(), index + 1, code));
            }
        }
    }

    if verbose {
        eprintln!("Checked {} files for {ONE_EXTENSIONS_NAME}.", files.len());
    }
    if found.is_empty() {
        eprintln!("One shell extensions check passed.");
        return Ok(());
    }
    for line in &found {
        eprintln!("{line}");
    }
    anyhow::bail!(
        "{} line(s) name {ONE_EXTENSIONS_NAME} outside cash-core and the tests, which compiles \
         every builtin again for it; use the shell's own extensions, as \
         `cash_shell::entry::default_builtin_names` does.",
        found.len()
    )
}

/// Whether `code` names [`ONE_EXTENSIONS_NAME`] other than as the default of a type
/// parameter (`SE: ShellExtensions = DefaultShellExtensions`), which compiles nothing
/// until code leaves the parameter out, and that code names the type.
fn names_other_extensions(code: &str) -> bool {
    code.match_indices(ONE_EXTENSIONS_NAME).any(|(at, _)| {
        let before = code
            .get(..at)
            .unwrap_or_default()
            .trim_end_matches(|c: char| c.is_alphanumeric() || c == '_' || c == ':')
            .trim_end();
        !(before.ends_with('=') && before.contains("ShellExtensions"))
    })
}

/// Every `.rs` file under `dir`.
fn collect_rust_files(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two calls that compiled the builtins again are caught; a default is not.
    #[test]
    fn a_call_with_the_default_extensions_is_caught() {
        assert!(names_other_extensions(
            "|| cash_builtins::default_builtins::<cash_core::extensions::DefaultShellExtensions>("
        ));
        assert!(names_other_extensions(
            "cash_core::extensions::DefaultShellExtensions,"
        ));
        assert!(!names_other_extensions(
            "pub type ShellRef<SE: cash_core::ShellExtensions = cash_core::extensions::DefaultShellExtensions> ="
        ));
        assert!(!names_other_extensions("let x = 1;"));
    }
}
