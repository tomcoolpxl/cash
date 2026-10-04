//! Configuration types for the test harness.

use clap::Parser;
use std::{ffi::OsString, path::PathBuf};

/// Configuration for the shell under test (cash).
#[derive(Clone, Debug)]
pub struct ShellConfig {
    /// Path to the shell binary.
    pub path: PathBuf,
    /// Default arguments to pass to this shell.
    pub default_args: Vec<String>,
    /// Default PATH variable for this shell.
    pub default_path_var: Option<String>,
}

impl ShellConfig {
    /// The PATH the cases run with: the one given, else none. The cases use what cash
    /// carries; brush's Unix folders (`/usr/bin`, …) and the folders of the host's PATH
    /// that hold a file named `sh` added nothing on Windows, where there are none.
    pub fn compute_test_path_var(&self) -> OsString {
        self.default_path_var.clone().unwrap_or_default().into()
    }
}

/// Configuration for the test runner.
#[derive(Clone, Debug)]
pub struct RunnerConfig {
    /// Configuration for the shell under test (cash).
    pub test_shell: ShellConfig,
    /// Directory containing test case YAML files.
    pub test_cases_dir: PathBuf,
    /// Directory for storing snapshots (relative to test case YAML files).
    pub snapshot_dir_name: String,
}

impl RunnerConfig {
    /// Creates a new runner config for the given shell under test.
    ///
    /// Callers typically build `test_shell` with
    /// `TestOptions::create_test_shell_config()`, which adds the standard
    /// flags like `--input-backend=basic`.
    pub fn new(test_shell: ShellConfig, test_cases_dir: PathBuf) -> Self {
        Self {
            test_shell,
            test_cases_dir,
            snapshot_dir_name: String::from("snaps"),
        }
    }

    /// Sets the snapshot directory name.
    #[must_use]
    pub fn with_snapshot_dir_name(mut self, name: impl Into<String>) -> Self {
        self.snapshot_dir_name = name.into();
        self
    }

    /// Sets the default PATH variable for the test shell.
    #[must_use]
    pub fn with_test_path_var(mut self, path_var: Option<String>) -> Self {
        self.test_shell.default_path_var = path_var;
        self
    }
}

/// Output format for test results.
#[derive(Clone, Copy, Default, clap::ValueEnum, Debug)]
pub enum OutputFormat {
    /// Human-readable colored output.
    #[default]
    Pretty,
    /// `JUnit` XML format.
    Junit,
    /// Minimal output.
    Terse,
}

/// Command-line options for the test harness.
#[derive(Clone, Parser, Debug)]
#[clap(version, about, disable_help_flag = true, disable_version_flag = true)]
pub struct TestOptions {
    /// Display usage information.
    #[clap(long = "help", action = clap::ArgAction::HelpLong)]
    pub help: Option<bool>,

    /// Output format for test results.
    #[clap(long = "format", default_value = "pretty")]
    pub format: OutputFormat,

    /// Display full details on known failures.
    #[clap(long = "known-failure-details")]
    pub display_known_failure_details: bool,

    /// Display details regarding successful test cases.
    #[clap(short = 'v', long = "verbose", env = "CASH_TEST_VERBOSE")]
    pub verbose: bool,

    /// List available tests without running them.
    #[clap(long = "list")]
    pub list_tests_only: bool,

    /// Exactly match filters (not just substring match).
    #[clap(long = "exact")]
    pub exact_match: bool,

    /// Optionally specify a non-default path for cash.
    #[clap(long = "cash-path", default_value = "", env = "CASH_TEST_SHELL_PATH")]
    pub cash_path: String,

    /// Optionally specify additional arguments for cash.
    #[clap(long = "cash-args", default_value = "", env = "CASH_TEST_SHELL_ARGS")]
    pub cash_args: String,

    /// Optionally specify path to test cases.
    #[clap(long = "test-cases-path", env = "CASH_TEST_CASES")]
    pub test_cases_path: Option<PathBuf>,

    /// Optionally specify PATH variable to use in shells.
    #[clap(long = "test-path-var", env = "CASH_TEST_PATH_VAR")]
    pub test_path_var: Option<String>,

    // The four below are taken and ignored: the binary is a test target without
    // libtest's harness, and `cargo test` and nextest pass it libtest's options.
    /// Show output from test cases (libtest's option; no effect).
    #[clap(long = "show-output")]
    pub show_output: bool,

    /// Capture output? (libtest's option; no effect).
    #[clap(long = "nocapture")]
    pub no_capture: bool,

    /// Colorize output? (libtest's option; no effect).
    #[clap(long = "color", default_value_t = clap::ColorChoice::Auto)]
    pub color: clap::ColorChoice,

    /// Run skipped tests only.
    #[clap(long = "ignored")]
    pub skipped_tests_only: bool,

    /// Unstable flags (libtest's option; no effect).
    #[clap(short = 'Z')]
    pub unstable_flag: Vec<String>,

    /// Patterns for tests to be excluded.
    #[clap(long = "skip")]
    pub exclude_filters: Vec<String>,

    /// Patterns for tests to be included.
    pub include_filters: Vec<String>,
}

impl TestOptions {
    /// Builds the `ShellConfig` for the shell under test based on
    /// the common options (path, extra args).
    pub fn create_test_shell_config(&self) -> ShellConfig {
        let mut default_args: Vec<String> = vec![
            "--norc".into(),
            "--noprofile".into(),
            "--no-config".into(),
            "--disable-bracketed-paste".into(),
            "--disable-color".into(),
            "--input-backend=basic".into(),
        ];

        // Append any additional shell args specified by the caller.
        self.cash_args.split_whitespace().for_each(|arg| {
            default_args.push(arg.into());
        });

        ShellConfig {
            path: PathBuf::from(&self.cash_path),
            default_args,
            default_path_var: self.test_path_var.clone(),
        }
    }

    /// Returns whether a test should run based on include/exclude filters.
    pub fn should_run_test(&self, qualified_name: &str) -> bool {
        if self.include_filters.is_empty() && self.exclude_filters.is_empty() {
            return true;
        }

        // If any include filters were given, then we are in opt-in mode.
        if !self.include_filters.is_empty()
            && !self.test_matches_filters(qualified_name, &self.include_filters)
        {
            return false;
        }

        // In all cases, exclude filters may be used to exclude tests.
        if !self.exclude_filters.is_empty()
            && self.test_matches_filters(qualified_name, &self.exclude_filters)
        {
            return false;
        }

        true
    }

    fn test_matches_filters(&self, qualified_test_name: &str, filters: &[String]) -> bool {
        if self.exact_match {
            filters.iter().any(|f| f == qualified_test_name)
        } else {
            filters
                .iter()
                .any(|filter| qualified_test_name.contains(filter))
        }
    }
}
