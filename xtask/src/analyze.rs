//! Analysis commands: running performance benchmarks with optional output capture.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use xshell::{Shell, cmd};

/// Run analysis and comparison tools.
#[derive(Parser)]
pub enum AnalyzeCommand {
    /// Run benchmarks and output results.
    Bench(BenchArgs),
}

/// Arguments for benchmark analysis.
#[derive(Parser)]
pub struct BenchArgs {
    /// Output file for benchmark results (bencher format).
    #[clap(long, short = 'o')]
    output: Option<PathBuf>,
}

/// Run an analysis command.
pub fn run(cmd: &AnalyzeCommand, verbose: bool) -> Result<()> {
    let sh = Shell::new()?;

    match cmd {
        AnalyzeCommand::Bench(args) => run_bench(&sh, args, verbose),
    }
}

/// Run benchmarks using `cargo bench`.
///
/// When an output file is specified, benchmarks are run with `--output-format bencher`
/// to produce machine-readable output suitable for CI comparison tools.
fn run_bench(sh: &Shell, args: &BenchArgs, verbose: bool) -> Result<()> {
    eprintln!("Running benchmarks...");

    if let Some(output) = &args.output {
        // Run with output capture to file using tee
        if verbose {
            eprintln!("Running: cargo bench --workspace --benches -- --output-format bencher");
        }
        let bench_output = cmd!(
            sh,
            "cargo bench --workspace --benches -- --output-format bencher"
        )
        .read()
        .context("Benchmarks failed")?;

        // Write to file
        sh.write_file(output, &bench_output)?;
        // Also print to stdout
        println!("{bench_output}");
    } else {
        // Run without file output
        if verbose {
            eprintln!("Running: cargo bench --workspace --benches");
        }
        cmd!(sh, "cargo bench --workspace --benches")
            .run()
            .context("Benchmarks failed")?;
    }

    eprintln!("Benchmarks completed.");
    Ok(())
}
