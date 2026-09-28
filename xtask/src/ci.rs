//! CI workflow commands that aggregate multiple checks and tests.
//!
//! This module provides composite workflows that run multiple checks in sequence:
//!
//! ## Quick workflow (`cargo xtask ci quick`)
//!
//! Fast inner-loop checks for rapid iteration:
//! 1. **Format check** - Fast, catches formatting issues early
//! 2. **Lint check** - Clippy, which also proves the code compiles
//! 3. **Unit tests** - Every test outside the `cash` package
//!
//! ## Full workflow (`cargo xtask ci full`)
//!
//! Everything CI checks on a push, and what should pass before one:
//! 1. **Format check** and **Lint check**, as above
//! 2. **All tests** - The whole workspace, including the tests that drive cash.exe
//! 3. **Doc tests** - The examples in documentation comments, which nextest does not run
//!
//! The ordering is intentional: fast checks run first to provide quick feedback,
//! with slower comprehensive tests running last. CI runs `ci full` and then lints
//! with every feature enabled (.github/workflows/ci.yml).
//!
//! Both workflows run their tests through cargo-nextest (`cargo binstall cargo-nextest`),
//! which runs each test in a process of its own.

use anyhow::Result;
use clap::Parser;

use crate::check::{self, CheckCommand, LintArgs};
use crate::test::{
    self, BinaryArgs, IntegrationTestArgs, TestCommand, TestSubcommand, UnitTestArgs,
};

/// Type alias for a named step in a CI workflow.
type Step<'a> = (&'a str, Box<dyn Fn() -> Result<()> + 'a>);

/// Run CI workflows.
#[derive(Parser)]
pub enum CiCommand {
    /// Run quick inner-loop checks: fmt, lint, unit tests.
    ///
    /// Use this for rapid iteration during development.
    Quick(QuickArgs),

    /// Run the full workflow: fmt, lint, every test, doc tests.
    ///
    /// This is what CI runs on every push.
    Full(FullArgs),
}

/// Arguments for quick workflow.
#[derive(Parser)]
pub struct QuickArgs {
    /// Continue running checks even if one fails.
    #[clap(short = 'k', long)]
    continue_on_error: bool,
}

/// Arguments for the full workflow.
#[derive(Parser)]
pub struct FullArgs {
    /// Continue running checks even if one fails.
    #[clap(short = 'k', long)]
    continue_on_error: bool,
}

/// Run a CI workflow command.
pub fn run(cmd: &CiCommand, verbose: bool) -> Result<()> {
    match cmd {
        CiCommand::Quick(args) => run_quick(args, verbose),
        CiCommand::Full(args) => run_full(args, verbose),
    }
}

/// Create a `TestCommand` for unit tests.
fn make_unit_test_command() -> TestCommand {
    TestCommand {
        binary_args: BinaryArgs {
            profile: crate::common::BuildProfile::Debug,
            debug: false,
            release: false,
        },
        subcommand: TestSubcommand::Unit(UnitTestArgs::default()),
    }
}

/// Create a `TestCommand` for integration tests.
fn make_integration_test_command() -> TestCommand {
    TestCommand {
        binary_args: BinaryArgs {
            profile: crate::common::BuildProfile::Debug,
            debug: false,
            release: false,
        },
        subcommand: TestSubcommand::Integration(IntegrationTestArgs::default()),
    }
}

/// The checks every workflow starts with, fastest first. Clippy compiles everything
/// `cargo check` would, so a separate build check would only repeat it.
fn static_checks(verbose: bool) -> Vec<Step<'static>> {
    vec![
        (
            "Format check",
            Box::new(move || check::run(&CheckCommand::Fmt, verbose)),
        ),
        (
            "Lint check",
            Box::new(move || check::run(&CheckCommand::Lint(LintArgs::default()), verbose)),
        ),
    ]
}

/// Run quick inner-loop checks.
fn run_quick(args: &QuickArgs, verbose: bool) -> Result<()> {
    eprintln!("Running quick checks...\n");

    let mut steps = static_checks(verbose);
    steps.push((
        "Unit tests",
        Box::new(move || test::run(&make_unit_test_command(), verbose)),
    ));

    run_steps(&steps, args.continue_on_error, "Quick checks")
}

/// Run the full workflow, as CI does.
fn run_full(args: &FullArgs, verbose: bool) -> Result<()> {
    eprintln!("Running full checks...\n");

    let mut steps = static_checks(verbose);
    steps.push((
        "All tests",
        Box::new(move || test::run(&make_integration_test_command(), verbose)),
    ));
    steps.push(("Doc tests", Box::new(move || test::run_doc_tests(verbose))));

    run_steps(&steps, args.continue_on_error, "Full checks")
}

/// Run a series of steps, optionally continuing on error.
fn run_steps(steps: &[Step<'_>], continue_on_error: bool, workflow_name: &str) -> Result<()> {
    let mut failures: Vec<&str> = Vec::new();

    for (name, step) in steps {
        eprintln!("\n{}", "=".repeat(60));
        eprintln!("Running: {name}");
        eprintln!("{}\n", "=".repeat(60));

        if let Err(e) = step() {
            eprintln!("\n❌ {name} failed: {e}");
            if !continue_on_error {
                // `main` prints the full cause chain of a propagated error.
                return Err(e);
            }
            // Nothing downstream ever sees this error, so print its cause
            // chain here; that is where install hints and tool output live.
            for cause in e.chain().skip(1) {
                eprintln!("   caused by: {cause}");
            }
            failures.push(name);
        } else {
            eprintln!("\n✅ {name} passed");
        }
    }

    if !failures.is_empty() {
        eprintln!("\n{}", "=".repeat(60));
        eprintln!("{workflow_name} completed with failures:");
        for name in &failures {
            eprintln!("  ❌ {name}");
        }
        eprintln!("{}", "=".repeat(60));
        anyhow::bail!("{} check(s) failed", failures.len());
    }

    eprintln!("\n{}", "=".repeat(60));
    eprintln!("✅ All {workflow_name} passed!");
    eprintln!("{}", "=".repeat(60));

    Ok(())
}
