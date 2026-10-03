//! YAML case runner for cash.
//!
//! This test harness runs YAML-based test cases with inline expectations
//! or insta snapshots.

use anyhow::Result;
use cash_test_harness::{RunnerConfig, TestOptions, TestRunner};
use clap::Parser;
use std::path::{Path, PathBuf};

async fn run_cash_tests(mut options: TestOptions) -> Result<bool> {
    // Resolve path to the shell-under-test.
    if options.cash_path.is_empty() {
        options.cash_path = assert_cmd::cargo::cargo_bin!("cash")
            .to_string_lossy()
            .to_string();
    }
    if !Path::new(&options.cash_path).exists() {
        return Err(anyhow::anyhow!(
            "cash binary not found: {}",
            options.cash_path
        ));
    }

    // Resolve test cases directory (in cases/brush/).
    let test_cases_dir = options.test_cases_path.as_deref().map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases/brush"),
        |p| p.to_owned(),
    );

    let test_shell = options.create_test_shell_config()?;

    let config =
        RunnerConfig::new(test_shell, test_cases_dir).with_platform_tags(options.platform_tags());

    let runner = TestRunner::new(config, options);
    runner.run().await
}

fn main() -> Result<()> {
    let unparsed_args: Vec<_> = std::env::args().collect();
    let options = TestOptions::parse_from(unparsed_args);

    let success = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(32)
        .build()?
        .block_on(run_cash_tests(options))?;

    if !success {
        std::process::exit(1);
    }

    Ok(())
}
