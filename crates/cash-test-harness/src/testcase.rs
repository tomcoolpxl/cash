//! Test case definitions and YAML schema.
//!
//! A field the schema does not know is an error, so that a case written for brush's
//! Unix harness (`pty`, `incompatible_os`, `incompatible_platforms`, `invocation`) is
//! refused rather than run without what it asked for.

use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

/// A file to create in the test's temporary directory.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TestFile {
    /// Relative path to test file within the temp directory.
    pub path: PathBuf,
    /// Contents to seed the file with.
    #[serde(default)]
    pub contents: String,
    /// Optionally provides relative path to the source file
    /// that should be used to populate this file.
    pub source_path: Option<PathBuf>,
    /// Whether the file should be executable.
    #[serde(default)]
    pub executable: bool,
}

/// A single test case.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TestCase {
    /// Name of the test case.
    pub name: Option<String>,

    /// Command-line arguments to the shell.
    #[serde(default)]
    pub args: Vec<String>,

    /// Default command-line shell arguments that should be *removed*.
    #[serde(default)]
    pub removed_default_args: HashSet<String>,

    /// Environment variables for the shell.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Home directory to set for the test.
    #[serde(default)]
    pub home_dir: Option<PathBuf>,

    /// Whether to skip this test.
    #[serde(default)]
    pub skip: bool,

    /// Input to provide via stdin.
    #[serde(default)]
    pub stdin: Option<String>,

    /// Whether to normalize whitespace when comparing output.
    #[serde(default)]
    pub ignore_whitespace: bool,

    /// Files to create in the test's temporary directory.
    #[serde(default)]
    pub test_files: Vec<TestFile>,

    /// Whether this test is a known failure.
    #[serde(default)]
    pub known_failure: bool,

    /// Timeout for this test in seconds.
    #[serde(default)]
    pub timeout_in_seconds: Option<u64>,

    // ==================== Expectation fields ====================
    /// Expected stdout content.
    #[serde(default)]
    pub expected_stdout: Option<String>,

    /// Expected stderr content.
    #[serde(default)]
    pub expected_stderr: Option<String>,

    /// Expected exit code.
    #[serde(default)]
    pub expected_exit_code: Option<i32>,

    /// Whether to use insta snapshot for this test's expectations.
    #[serde(default)]
    pub snapshot: bool,
}

impl TestCase {
    /// Whether the case says what to expect: one that does not passed whatever cash did.
    pub const fn has_expectation(&self) -> bool {
        self.expected_stdout.is_some()
            || self.expected_stderr.is_some()
            || self.expected_exit_code.is_some()
            || self.snapshot
    }
}

/// A set of test cases loaded from a single YAML file.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TestCaseSet {
    /// Name of the test case set.
    pub name: Option<String>,

    /// The test cases in this set.
    pub cases: Vec<TestCase>,

    /// Common test files applicable to all children test cases.
    #[serde(default)]
    pub common_test_files: Vec<TestFile>,

    /// Directory containing the YAML file (computed at runtime).
    #[serde(skip)]
    pub source_dir: PathBuf,

    /// Path to the YAML file (computed at runtime).
    #[serde(skip)]
    pub source_file: PathBuf,
}
