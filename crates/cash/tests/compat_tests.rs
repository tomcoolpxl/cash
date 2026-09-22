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
        reason = "the Windows path is an early return, so the rest of `main`                   is compiled out and its signature looks over-general"
    )
)]

//! Compatibility test harness for brush shell.
//!
//! This test harness runs YAML-based test cases comparing brush output against
//! bash (the oracle shell) to validate compatibility.

#![cfg(any(unix, windows))]

use anyhow::Result;
use cash_test_harness::{
    OracleConfig, RunnerConfig, ShellConfig, TestMode, TestOptions, TestRunner, WhichShell,
};
use clap::Parser;
use std::path::{Path, PathBuf};

const BASH_CONFIG_NAME: &str = "bash";
const SH_CONFIG_NAME: &str = "sh";

fn get_bash_version_str(bash_path: &Path) -> Result<String> {
    cash_test_harness::util::get_bash_version_str(bash_path)
}

fn create_bash_oracle(options: &TestOptions) -> Result<OracleConfig> {
    let bash_version_str = get_bash_version_str(&options.bash_path)?;
    if options.verbose {
        eprintln!("Detected bash version: {bash_version_str}");
    }

    Ok(OracleConfig {
        name: String::from(BASH_CONFIG_NAME),
        shell: ShellConfig {
            which: WhichShell::NamedShell(options.bash_path.clone()),
            default_args: vec![String::from("--norc"), String::from("--noprofile")],
            default_path_var: options.test_path_var.clone(),
            launcher: None,
        },
        version_str: Some(bash_version_str),
    })
}

fn create_sh_oracle(options: &TestOptions) -> OracleConfig {
    OracleConfig {
        name: String::from(SH_CONFIG_NAME),
        shell: ShellConfig {
            which: WhichShell::NamedShell(PathBuf::from("sh")),
            default_args: vec![],
            default_path_var: options.test_path_var.clone(),
            launcher: None,
        },
        version_str: None,
    }
}

fn create_test_shell_config(options: &TestOptions, oracle_name: &str) -> Result<ShellConfig> {
    let mut config = options.create_test_shell_config()?;

    // Add --sh flag when testing against sh oracle.
    if oracle_name == SH_CONFIG_NAME {
        config.default_args.insert(0, "--sh".into());
    }

    Ok(config)
}

async fn run_compat_tests(mut options: TestOptions) -> Result<bool> {
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

    // Resolve bash path to absolute path to avoid issues when env is cleared.
    if options.bash_path.is_relative() || options.bash_path.as_path() == Path::new("bash") {
        if let Ok(resolved) = which::which(&options.bash_path) {
            options.bash_path = resolved;
        }
    }

    // Resolve test cases directory (now under compat/).
    let test_cases_dir = options.test_cases_path.as_deref().map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases/compat"),
        |p| p.to_owned(),
    );

    let mut all_passed = true;

    // Run tests for each enabled config
    if options.should_enable_config(BASH_CONFIG_NAME, &[BASH_CONFIG_NAME]) {
        let oracle = create_bash_oracle(&options)?;
        let test_shell = create_test_shell_config(&options, &oracle.name)?;

        let config = RunnerConfig::new(PathBuf::from(&options.brush_path), test_cases_dir.clone())
            .with_oracle(oracle)
            .with_mode(TestMode::Oracle)
            .with_platform_tags(options.platform_tags());

        let config = RunnerConfig {
            test_shell,
            ..config
        };

        let runner = TestRunner::new(config, options.clone());
        if !runner.run().await? {
            all_passed = false;
        }
    }

    if options.should_enable_config(SH_CONFIG_NAME, &[BASH_CONFIG_NAME]) {
        let oracle = create_sh_oracle(&options);
        let test_shell = create_test_shell_config(&options, &oracle.name)?;

        let config = RunnerConfig::new(PathBuf::from(&options.brush_path), test_cases_dir.clone())
            .with_oracle(oracle)
            .with_mode(TestMode::Oracle)
            .with_platform_tags(options.platform_tags());

        let config = RunnerConfig {
            test_shell,
            ..config
        };

        let runner = TestRunner::new(config, options.clone());
        if !runner.run().await? {
            all_passed = false;
        }
    }

    Ok(all_passed)
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
            .block_on(run_compat_tests(options))?;

        if !success {
            std::process::exit(1);
        }

        Ok(())
    }
}
