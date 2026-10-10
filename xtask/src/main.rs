//! xtask-style command-line tool for building this project.

mod analyze;
mod check;
mod ci;
mod common;
mod generate;
mod perf;
mod test;

use anyhow::Result;
use clap::Parser;

/// Global options shared across all commands.
#[derive(Parser, Debug, Clone, Copy)]
pub struct GlobalArgs {
    /// Enable verbose output.
    #[clap(long, short = 'v', global = true)]
    pub verbose: bool,
}

#[derive(Parser)]
#[clap(name = "xtask", about = "Build automation tasks for cash")]
struct CommandLineArgs {
    #[clap(flatten)]
    global: GlobalArgs,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Parser)]
enum Command {
    /// Run analysis tasks (benchmarks).
    #[clap(subcommand)]
    Analyze(analyze::AnalyzeCommand),
    /// Run code quality checks.
    #[clap(subcommand)]
    Check(check::CheckCommand),
    /// Run CI workflows.
    #[clap(subcommand)]
    Ci(ci::CiCommand),
    /// Generate documentation, completions, and schemas.
    #[clap(subcommand)]
    Gen(generate::GenCommand),
    /// Measure a build's size and start time against perf-budget.toml.
    Perf(perf::PerfArgs),
    /// Run tests.
    Test(Box<test::TestCommand>),
}

fn main() -> Result<()> {
    let args = CommandLineArgs::parse();
    let verbose = args.global.verbose;

    match &args.command {
        Command::Analyze(cmd) => analyze::run(cmd, verbose),
        Command::Gen(cmd) => generate::run(cmd, verbose),
        Command::Check(cmd) => check::run(cmd, verbose),
        Command::Test(cmd) => test::run(cmd, verbose),
        Command::Ci(cmd) => ci::run(cmd, verbose),
        Command::Perf(args) => perf::run(args, verbose),
    }
}
