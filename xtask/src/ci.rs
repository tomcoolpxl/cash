//! CI workflow commands that aggregate multiple checks and tests.
//!
//! This module provides composite workflows that run multiple checks in sequence:
//!
//! ## Quick workflow (`cargo xtask ci quick`)
//!
//! What CI runs on every push, but the slow lane:
//! 1. **Format check** - Fast, catches formatting issues early
//! 2. **Lint check** - Clippy, which also proves the code compiles
//! 3. **Documentation** - `cargo doc`, which fails on a link to something that is not there
//! 4. **Quick lane** - Every crate's own tests (`.config/nextest.toml`)
//! 5. **Doc tests** - The examples in documentation comments, which nextest does not run
//!
//! ## Full workflow (`cargo xtask ci full`)
//!
//! The same, with both lanes of tests in one run: what should pass before a release.
//!
//! The ordering is intentional: fast checks run first to provide quick feedback,
//! with slower comprehensive tests running last. On every push, CI runs `ci quick`,
//! lints with every feature enabled, and runs the slow lane in four parts on runners of
//! their own (`cargo xtask test slow --partition hash:N/4`, .github/workflows/ci.yml).
//!
//! Both workflows run their tests through cargo-nextest (`cargo binstall cargo-nextest`),
//! which runs each test in a process of its own.

use anyhow::Result;
use clap::Parser;

use crate::check::{self, CheckCommand, LintArgs};
use crate::test::{self, BinaryArgs, Lane, LaneArgs};

/// Type alias for a named step in a CI workflow.
type Step<'a> = (&'a str, Box<dyn Fn() -> Result<()> + 'a>);

/// Run CI workflows.
#[derive(Parser)]
pub enum CiCommand {
    /// Run what CI runs on every push but the slow lane: fmt, lint, the documentation,
    /// the quick lane's tests, doc tests.
    ///
    /// Use this for rapid iteration during development.
    Quick(QuickArgs),

    /// Run the full workflow: the quick one with every test, the slow lane's too.
    ///
    /// This is what should pass before a release.
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
        CiCommand::Quick(args) => {
            eprintln!("Running quick checks...\n");
            run_steps(
                &steps(Lane::Quick, verbose),
                args.continue_on_error,
                "Quick checks",
            )
        }
        CiCommand::Full(args) => {
            eprintln!("Running full checks...\n");
            run_steps(
                &steps(Lane::All, verbose),
                args.continue_on_error,
                "Full checks",
            )
        }
    }
}

/// A workflow's steps, fastest first, with the tests of `lane`. Clippy compiles
/// everything `cargo check` would, so a separate build check would only repeat it.
fn steps(lane: Lane, verbose: bool) -> Vec<Step<'static>> {
    vec![
        (
            "Format check",
            Box::new(move || check::run(&CheckCommand::Fmt, verbose)),
        ),
        (
            "Process state check",
            Box::new(move || check::run(&CheckCommand::ProcessState, verbose)),
        ),
        (
            "Lint check",
            Box::new(move || check::run(&CheckCommand::Lint(LintArgs::default()), verbose)),
        ),
        ("Documentation", Box::new(move || test::run_docs(verbose))),
        (
            match lane {
                Lane::Quick => "Quick lane's tests",
                Lane::Slow => "Slow lane's tests",
                Lane::All => "All tests",
            },
            Box::new(move || {
                let sh = xshell::Shell::new()?;
                test::run_lane(
                    &sh,
                    &BinaryArgs::default(),
                    lane,
                    &LaneArgs::default(),
                    verbose,
                )
            }),
        ),
        ("Doc tests", Box::new(move || test::run_doc_tests(verbose))),
    ]
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
