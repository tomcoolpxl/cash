//! Test commands for running various test suites.
//!
//! The tests run in two lanes, each a nextest profile whose default filter picks its
//! tests (`.config/nextest.toml`):
//!
//! - **Quick**: every crate's own tests, which CI runs on every push beside the lints
//! - **Slow**: the tests that drive `cash.exe` and sed's ported suite, which CI runs on
//!   every push split over parallel runners, and before a release
//!
//! `all` runs both in one nextest run. Each supports optional coverage collection via
//! `cargo-llvm-cov`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use xshell::{Shell, cmd};

use crate::common::{BuildProfile, find_workspace_root};

/// A lane of the test suite.
#[derive(Clone, Copy, Debug)]
pub enum Lane {
    /// The default profile: every crate's own tests.
    Quick,
    /// The tests the quick lane leaves out.
    Slow,
    /// Both lanes, in one run.
    All,
}

impl Lane {
    /// The nextest profile whose default filter picks the lane's tests.
    const fn nextest_profile(self) -> &'static str {
        match self {
            Self::Quick => "default",
            Self::Slow => "slow",
            Self::All => "full",
        }
    }

    /// The lane, in a sentence: "the tests of {}".
    const fn words(self) -> &'static str {
        match self {
            Self::Quick => "the quick lane",
            Self::Slow => "the slow lane",
            Self::All => "both lanes",
        }
    }
}

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

impl Default for BinaryArgs {
    fn default() -> Self {
        Self {
            profile: BuildProfile::Debug,
            debug: false,
            release: false,
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
    /// Run the quick lane: every crate's own tests, as CI does on every push.
    ///
    /// Leaves out the slow lane: the tests that drive cash.exe and sed's ported suite.
    #[clap(alias = "unit")]
    Quick(LaneArgs),

    /// Run the slow lane: the tests that drive cash.exe and sed's ported suite, or a
    /// part of them.
    Slow(LaneArgs),

    /// Run every test of the workspace, both lanes in one run.
    #[clap(alias = "integration")]
    All(LaneArgs),

    /// Run the tests of the patched crates in `vendor/`, which are outside the workspace.
    Vendored,
}

/// Arguments for a lane's tests.
#[derive(Args, Clone, Default)]
pub struct LaneArgs {
    /// Coverage options.
    #[clap(flatten)]
    pub coverage: CoverageArgs,

    /// Run one part of the lane's tests, as nextest's `--partition` does: `hash:1/4` is
    /// the first of four. CI runs the slow lane so, on four runners at once.
    #[clap(long)]
    pub partition: Option<String>,

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
        TestSubcommand::Quick(args) => run_lane(&sh, &cmd.binary_args, Lane::Quick, args, verbose),
        TestSubcommand::Slow(args) => run_lane(&sh, &cmd.binary_args, Lane::Slow, args, verbose),
        TestSubcommand::All(args) => run_lane(&sh, &cmd.binary_args, Lane::All, args, verbose),
        TestSubcommand::Vendored => run_vendored_tests(verbose),
    }
}

/// Runs a lane's tests, or the part of them `args.partition` names.
pub fn run_lane(
    sh: &Shell,
    binary_args: &BinaryArgs,
    lane: Lane,
    args: &LaneArgs,
    verbose: bool,
) -> Result<()> {
    let profile = binary_args.effective_profile();
    let part = args
        .partition
        .as_deref()
        .map_or_else(String::new, |part| format!(", part {part}"));
    eprintln!(
        "Running the tests of {} ({profile:?} profile{part})...",
        lane.words()
    );

    let nextest = Nextest {
        build_profile: profile,
        lane,
        partition: args.partition.as_deref(),
    };
    let test_result = if args.coverage.coverage {
        run_tests_with_coverage(sh, &nextest, &args.coverage.coverage_output, verbose)
    } else {
        run_nextest(sh, &nextest, verbose).map(|()| {
            eprintln!("The tests of {} passed.", lane.words());
        })
    };

    // Copy nextest results if requested (even on test failure, so CI can upload them).
    if let Some(ref output) = args.results_output {
        copy_nextest_results(lane, output)?;
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

/// The crates in `vendor/` that carry a patch of cash's with tests of their own.
const VENDORED_CRATES_WITH_TESTS: [&str; 2] = ["reedline", "crossterm"];

/// Runs the tests of the vendored crates, among them the tests of cash's patches
/// (`vendor/*/CASH-PATCHES.md`). They are outside the workspace, so nothing else runs
/// them.
///
/// Each is built in a target folder of its own under `target/`, and the `Cargo.lock` a
/// run writes into `vendor/` is removed again. `NO_COLOR` is cleared: crossterm honours
/// it, and a reedline test checks the colours it would have.
pub fn run_vendored_tests(verbose: bool) -> Result<()> {
    let sh = Shell::new()?;
    let root = find_workspace_root()?;
    sh.change_dir(&root);
    for name in VENDORED_CRATES_WITH_TESTS {
        let manifest = format!("vendor/{name}/Cargo.toml");
        let target_dir = format!("target/vendor-{name}");
        let lock = root.join("vendor").join(name).join("Cargo.lock");
        let lock_was_there = lock.exists();
        eprintln!("Running the tests of vendor/{name}...");
        if verbose {
            eprintln!(
                "Running: cargo test --manifest-path {manifest} --lib --target-dir {target_dir} \
                 -- --test-threads=1"
            );
        }
        // One thread: crossterm's `test_no_color` sets `NO_COLOR` for the whole process,
        // which its colour tests read through a memoised `Once`, and run beside them it
        // made `test_format_reset_bg_color` fail on CI (2026-10-04).
        let result = cmd!(
            sh,
            "cargo test --manifest-path {manifest} --lib --target-dir {target_dir} -- --test-threads=1"
        )
        .env_remove("NO_COLOR")
        .run()
        .with_context(|| format!("The tests of vendor/{name} failed"));
        if !lock_was_there {
            let _ = std::fs::remove_file(&lock);
        }
        result?;
    }
    eprintln!("The vendored crates' tests passed.");
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

/// A nextest run of a lane.
struct Nextest<'a> {
    /// The cargo profile the tests are built with.
    build_profile: BuildProfile,
    /// The lane, whose nextest profile picks the tests.
    lane: Lane,
    /// The part of the lane's tests to run, as nextest's `--partition` takes it.
    partition: Option<&'a str>,
}

impl Nextest<'_> {
    /// The arguments of `cargo nextest run`.
    fn args(&self) -> Vec<&str> {
        let mut args = vec![
            "nextest",
            "run",
            "--workspace",
            "--no-fail-fast",
            "--profile",
            self.lane.nextest_profile(),
        ];
        if self.build_profile == BuildProfile::Release {
            args.push("--release");
        }
        if let Some(partition) = self.partition {
            args.push("--partition");
            args.push(partition);
        }
        args
    }
}

/// Runs cargo nextest.
fn run_nextest(sh: &Shell, nextest: &Nextest<'_>, verbose: bool) -> Result<()> {
    require_nextest(sh)?;
    let args = nextest.args();
    if verbose {
        eprintln!("Running: cargo {}", args.join(" "));
    }
    cmd!(sh, "cargo {args...}").run().context("Tests failed")?;
    Ok(())
}

/// Copy the nextest `JUnit` XML results to the given output path.
fn copy_nextest_results(lane: Lane, output: &Path) -> Result<()> {
    let workspace_root = find_workspace_root()?;
    // nextest writes the results under the profile it ran.
    let source = workspace_root
        .join("target/nextest")
        .join(lane.nextest_profile())
        .join("test-results.xml");
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
    nextest: &Nextest<'_>,
    output: &Path,
    verbose: bool,
) -> Result<()> {
    let output_path = output.display().to_string();

    eprintln!(
        "Running tests with coverage ({:?} profile)...",
        nextest.build_profile
    );
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

    require_nextest(sh)?;
    let test_args = nextest.args();
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

#[cfg(test)]
mod tests {
    /// The `default-filter` of a profile of `.config/nextest.toml`.
    fn default_filter(config: &str, profile: &str) -> String {
        let section = format!("[profile.{profile}]");
        let after = config.split_once(&section).unwrap().1;
        let line = after
            .lines()
            .find_map(|line| line.strip_prefix("default-filter = "))
            .unwrap();
        line.trim_matches('\'').to_owned()
    }

    /// nextest has no named filters, so the slow lane's is written twice; the quick lane
    /// is to be every test the slow lane leaves out, and no other.
    #[test]
    fn the_quick_lane_is_every_test_the_slow_lane_leaves_out() {
        let config = include_str!("../../.config/nextest.toml");
        let slow = default_filter(config, "slow");
        assert_eq!(default_filter(config, "default"), format!("not ({slow})"));
        assert_eq!(default_filter(config, "full"), "all()");
    }
}
