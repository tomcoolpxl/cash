// cash (D43): on Windows this file compiles to a stub that skips, so everything the
// Linux path needs is unused there. Denying warnings workspace-wide would otherwise
// make the stub impossible.
#![cfg_attr(
    windows,
    allow(
        unused,
        dead_code,
        clippy::unnecessary_wraps,
        clippy::needless_return,
        reason = "the Windows path is an early return, so the rest of `main` \
                  is compiled out and its signature looks over-general"
    )
)]

//! Brush-only test harness.
//!
//! This test harness runs YAML-based test cases with inline expectations
//! or insta snapshots, without comparing against an oracle shell.

#![cfg(any(unix, windows))]

use anyhow::Result;
use cash_test_harness::{RunnerConfig, TestMode, TestOptions, TestRunner};
use clap::Parser;
use std::path::{Path, PathBuf};

async fn run_brush_tests(mut options: TestOptions) -> Result<bool> {
    // Resolve path to the shell-under-test.
    if options.brush_path.is_empty() {
        options.brush_path = assert_cmd::cargo::cargo_bin!("cash")
            .to_string_lossy()
            .to_string();
    }
    if !Path::new(&options.brush_path).exists() {
        return Err(anyhow::anyhow!(
            "brush binary not found: {}",
            options.brush_path
        ));
    }

    // Resolve test cases directory (in cases/brush/).
    let test_cases_dir = options.test_cases_path.as_deref().map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases/brush"),
        |p| p.to_owned(),
    );

    let test_shell = options.create_test_shell_config()?;

    let config = RunnerConfig::new(PathBuf::from(&options.brush_path), test_cases_dir)
        .with_mode(TestMode::Expectation)
        .with_platform_tags(options.platform_tags());

    let config = RunnerConfig {
        test_shell,
        ..config
    };

    let runner = TestRunner::new(config, options);
    runner.run().await
}

fn main() -> Result<()> {
    // cash (D43): the differential suite diffs against a reference bash and needs a
    // PTY, neither of which exists on Windows. Conformance therefore runs on Linux CI;
    // Windows behaviour is covered by cash's own acceptance corpus, because §4's
    // divergences are deliberate and a bash reference would flag every one as a failure.
    //
    // Skipping rather than failing keeps `cargo test --workspace` meaningful on a
    // Windows dev machine.
    #[cfg(windows)]
    {
        eprintln!(
            "skipped: the differential suite runs on Linux (D43); Windows is \
             covered by `cargo test -p cash --test acceptance`."
        );
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        let unparsed_args: Vec<_> = std::env::args().collect();
        let options = TestOptions::parse_from(unparsed_args);

        let success = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(32)
            .build()?
            .block_on(run_brush_tests(options))?;

        if !success {
            std::process::exit(1);
        }

        Ok(())
    }
}
