//! Test commands for running various test suites.
//!
//! This module provides commands for running different types of tests:
//!
//! - **Unit tests**: Fast tests that don't execute `cash.exe` (everything but the `cash`
//!   package's tests)
//! - **Integration tests**: All workspace tests, including the `cash` package's tests that
//!   drive `cash.exe`
//!
//! Both unit and integration tests support optional coverage collection via
//! `cargo-llvm-cov`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use xshell::{Shell, cmd};

use crate::common::{BuildProfile, find_workspace_root};

/// The nextest filter for unit tests. The tests that drive `cash.exe` all live in the `cash`
/// package, because `CARGO_BIN_EXE_cash` is only defined for the crate that declares the
/// binary; everything else is a unit test.
const UNIT_TEST_FILTER: &str = "not package(cash)";

/// Build profile arguments shared by the test commands.
#[derive(Args, Debug, Clone)]
pub struct BinaryArgs {
    /// Build profile to test.
    #[clap(long, short = 'p', value_enum, default_value_t = BuildProfile::Debug, global = true)]
    pub profile: BuildProfile,

    /// Use debug build profile (shorthand for --profile=debug).
    #[clap(long, conflicts_with_all = ["profile", "release"], global = true)]
    pub debug: bool,

    /// Use release build profile (shorthand for --profile=release).
    #[clap(long, conflicts_with_all = ["profile", "debug"], global = true)]
    pub release: bool,
}

impl BinaryArgs {
    /// Resolve the effective build profile, considering --debug/--release shorthands.
    #[must_use]
    pub const fn effective_profile(&self) -> BuildProfile {
        if self.debug {
            BuildProfile::Debug
        } else if self.release {
            BuildProfile::Release
        } else {
            self.profile
        }
    }
}

/// Run tests.
#[derive(Parser)]
pub struct TestCommand {
    /// Shared binary arguments.
    #[clap(flatten)]
    pub binary_args: BinaryArgs,

    /// Test subcommand.
    #[clap(subcommand)]
    pub subcommand: TestSubcommand,
}

/// Test subcommands.
#[derive(Subcommand, Clone)]
pub enum TestSubcommand {
    /// Run unit tests (fast tests that don't execute cash.exe).
    ///
    /// Excludes the `cash` package's tests, which drive the binary.
    Unit(UnitTestArgs),

    /// Run all workspace tests (unit + integration tests).
    ///
    /// This includes all tests: unit tests plus the `cash` package's tests that
    /// drive cash.exe.
    Integration(IntegrationTestArgs),
}

/// Arguments for unit tests.
#[derive(Args, Clone, Default)]
pub struct UnitTestArgs {
    /// Coverage options.
    #[clap(flatten)]
    pub coverage: CoverageArgs,
}

/// Arguments for integration tests.
#[derive(Args, Clone, Default)]
pub struct IntegrationTestArgs {
    /// Coverage options.
    #[clap(flatten)]
    pub coverage: CoverageArgs,

    /// Copy the nextest `JUnit` XML results to this path after the test run.
    /// The copy is performed even if tests fail, so CI can always upload results.
    #[clap(long)]
    pub results_output: Option<PathBuf>,
}

/// Arguments for coverage collection.
#[derive(Args, Clone, Default)]
pub struct CoverageArgs {
    /// Collect code coverage during test run.
    #[clap(long)]
    pub coverage: bool,

    /// Output file for coverage report (Cobertura XML format).
    /// Only used when --coverage is specified.
    #[clap(long, short = 'o', default_value = "codecov.xml")]
    pub coverage_output: PathBuf,
}

/// Run a test command.
pub fn run(cmd: &TestCommand, verbose: bool) -> Result<()> {
    let sh = Shell::new()?;

    match &cmd.subcommand {
        TestSubcommand::Unit(args) => run_unit_tests(&sh, &cmd.binary_args, args, verbose),
        TestSubcommand::Integration(args) => {
            run_integration_tests(&sh, &cmd.binary_args, args, verbose)
        }
    }
}

/// Run unit tests (excludes integration test binaries).
///
/// Unit tests are fast tests that don't execute cash.exe.
pub fn run_unit_tests(
    sh: &Shell,
    binary_args: &BinaryArgs,
    args: &UnitTestArgs,
    verbose: bool,
) -> Result<()> {
    let profile = binary_args.effective_profile();
    eprintln!("Running unit tests ({profile:?} profile)...");

    if args.coverage.coverage {
        run_tests_with_coverage(
            sh,
            profile,
            Some(UNIT_TEST_FILTER),
            &args.coverage.coverage_output,
            verbose,
        )
    } else {
        run_nextest(sh, profile, Some(UNIT_TEST_FILTER), verbose)?;
        eprintln!("Unit tests passed.");
        Ok(())
    }
}

/// Run all workspace tests (unit + integration).
///
/// This runs all tests in the workspace, including the integration tests
/// that drive cash.exe.
pub fn run_integration_tests(
    sh: &Shell,
    binary_args: &BinaryArgs,
    args: &IntegrationTestArgs,
    verbose: bool,
) -> Result<()> {
    let profile = binary_args.effective_profile();

    eprintln!("Running integration tests ({profile:?} profile)...");

    let filter = None;

    let test_result = if args.coverage.coverage {
        run_tests_with_coverage(sh, profile, filter, &args.coverage.coverage_output, verbose)
    } else {
        run_nextest(sh, profile, filter, verbose).map(|()| {
            eprintln!("Integration tests passed.");
        })
    };

    // Copy nextest results if requested (even on test failure, so CI can upload them).
    if let Some(ref output) = args.results_output {
        copy_nextest_results(output)?;
    }

    test_result
}

/// Run the doc tests (the examples in documentation comments), which nextest does not run.
pub fn run_doc_tests(verbose: bool) -> Result<()> {
    let sh = Shell::new()?;
    eprintln!("Running doc tests...");
    if verbose {
        eprintln!("Running: cargo test --workspace --doc");
    }
    cmd!(sh, "cargo test --workspace --doc")
        .run()
        .context("Doc tests failed")?;
    eprintln!("Doc tests passed.");
    Ok(())
}

/// Builds the documentation of every crate of the workspace, which fails on a link to
/// something that is not there: the rustdoc lints are errors (`Cargo.toml`), but nothing
/// ran rustdoc, and ten such links had gathered in four crates by 2026-09-30.
pub fn run_docs(verbose: bool) -> Result<()> {
    let sh = Shell::new()?;
    eprintln!("Building the documentation...");
    if verbose {
        eprintln!("Running: cargo doc --workspace --no-deps");
    }
    cmd!(sh, "cargo doc --workspace --no-deps")
        .run()
        .context("The documentation does not build")?;
    eprintln!("The documentation builds.");
    Ok(())
}

/// Fails with the install command when cargo-nextest is missing, rather than letting
/// cargo's "no such command" read like a test failure.
fn require_nextest(sh: &Shell) -> Result<()> {
    cmd!(sh, "cargo nextest --version")
        .quiet()
        .ignore_stdout()
        .ignore_stderr()
        .run()
        .context(
            "cargo-nextest is not installed; install it with `cargo binstall cargo-nextest` \
             (or `cargo install cargo-nextest --locked`)",
        )
}

/// Run cargo nextest with optional filter expression.
fn run_nextest(
    sh: &Shell,
    profile: BuildProfile,
    filter_expr: Option<&str>,
    verbose: bool,
) -> Result<()> {
    require_nextest(sh)?;
    let mut args = vec!["nextest", "run", "--workspace", "--no-fail-fast"];

    if profile == BuildProfile::Release {
        args.push("--release");
    }

    // Add filter expression if provided
    let filter_value = filter_expr.map(str::to_string);
    if let Some(ref value) = filter_value {
        args.push("-E");
        args.push(value);
    }

    if verbose {
        eprintln!("Running: cargo {}", args.join(" "));
    }

    cmd!(sh, "cargo {args...}").run().context("Tests failed")?;
    Ok(())
}

/// Copy the nextest `JUnit` XML results to the given output path.
fn copy_nextest_results(output: &Path) -> Result<()> {
    let workspace_root = find_workspace_root()?;
    let source = workspace_root.join("target/nextest/default/test-results.xml");
    std::fs::copy(&source, output).with_context(|| {
        format!(
            "Failed to copy nextest results from {} to {}",
            source.display(),
            output.display()
        )
    })?;
    eprintln!("Nextest results copied to: {}", output.display());
    Ok(())
}

/// Run tests with code coverage collection using `cargo-llvm-cov`.
///
/// The coverage workflow:
/// 1. Source environment variables from `cargo llvm-cov show-env`
/// 2. Clean previous coverage data
/// 3. Run tests (continuing even if tests fail to still generate report)
/// 4. Generate Cobertura XML report for CI integration
///
/// Requires `cargo-llvm-cov` to be installed: `cargo install cargo-llvm-cov`
fn run_tests_with_coverage(
    sh: &Shell,
    profile: BuildProfile,
    filter_expr: Option<&str>,
    output: &Path,
    verbose: bool,
) -> Result<()> {
    let output_path = output.display().to_string();

    eprintln!("Running tests with coverage ({profile:?} profile)...");
    eprintln!("Coverage output: {output_path}");

    // Set up llvm-cov environment
    eprintln!("Setting up llvm-cov environment...");
    if verbose {
        eprintln!("Running: cargo llvm-cov show-env --export-prefix");
    }
    let env_output = cmd!(sh, "cargo llvm-cov show-env --export-prefix")
        .read()
        .context("Failed to get llvm-cov environment. Is cargo-llvm-cov installed?")?;

    // Parse and set environment variables from llvm-cov output
    env_output
        .lines()
        .filter_map(|line| line.strip_prefix("export "))
        .filter_map(|rest| rest.split_once('='))
        .for_each(|(k, v)| sh.set_var(k, v.trim_matches(['"', '\''])));

    // Clean previous coverage data
    if verbose {
        eprintln!("Running: cargo llvm-cov clean --workspace");
    }
    cmd!(sh, "cargo llvm-cov clean --workspace")
        .run()
        .context("Failed to clean coverage data")?;

    // Build cargo nextest args
    require_nextest(sh)?;
    let mut test_args = vec!["nextest", "run", "--workspace", "--no-fail-fast"];
    if profile == BuildProfile::Release {
        test_args.push("--release");
    }

    // Add filter expression if provided
    let filter_value = filter_expr.map(str::to_string);
    if let Some(ref value) = filter_value {
        test_args.push("-E");
        test_args.push(value);
    }

    if verbose {
        eprintln!("Running: cargo {}", test_args.join(" "));
    }

    // Run tests - let output pass through naturally, but continue on failure to generate coverage
    // report
    let test_result = cmd!(sh, "cargo {test_args...}").run();
    let test_failed = test_result.is_err();

    if test_failed {
        eprintln!("Tests failed, but continuing to generate coverage report...");
    }

    // Generate coverage report (always attempt this)
    eprintln!("Generating coverage report...");
    if verbose {
        eprintln!("Running: cargo llvm-cov report --cobertura --output-path {output_path}");
    }
    cmd!(
        sh,
        "cargo llvm-cov report --cobertura --output-path {output_path}"
    )
    .run()
    .context("Failed to generate coverage report")?;

    eprintln!("Coverage report written to: {output_path}");

    // Now propagate test failure if tests failed
    if test_failed {
        anyhow::bail!("Tests failed (coverage report was still generated)");
    }

    eprintln!("Tests with coverage completed successfully.");
    Ok(())
}
