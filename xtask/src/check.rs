//! Check commands for code quality validation.
//!
//! This module provides various code quality checks that can be run individually
//! or as part of a CI workflow. Each check wraps an external tool and provides
//! consistent error handling and verbose output.
//!
//! `check unused-deps` needs `cargo-udeps` (`cargo install cargo-udeps`) and a nightly
//! toolchain; the other checks need only the pinned toolchain.

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
    Lint,
    /// Check for unused dependencies (requires nightly).
    UnusedDeps,
}

/// Options for the build check.
#[derive(Default, Parser)]
pub struct BuildArgs {
    /// Only check crates that build with the workspace-wide MSRV; for use when
    /// the check is being run with the oldest toolchain the workspace as a
    /// whole supports.
    #[clap(long = "workspace-msrv")]
    workspace_msrv: bool,
}

/// Run a check command.
pub fn run(cmd: &CheckCommand, verbose: bool) -> Result<()> {
    let sh = Shell::new()?;

    match cmd {
        CheckCommand::Fmt => check_fmt(&sh, verbose),
        CheckCommand::Lint => check_lint(&sh, verbose),
        CheckCommand::UnusedDeps => check_unused_deps(&sh, verbose),
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

fn check_lint(sh: &Shell, verbose: bool) -> Result<()> {
    eprintln!("Running clippy...");
    let mut args = vec!["clippy", "--workspace", "--all-features", "--all-targets"];
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

fn check_build(sh: &Shell, args: &BuildArgs, verbose: bool) -> Result<()> {
    eprintln!("Checking that code compiles...");

    let excluded = if args.workspace_msrv {
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

    let mut args = vec!["check", "--all-features", "--all-targets", "--workspace"];
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
