//! Comparison types for test results.

/// Comparison of a single expectation.
#[derive(Debug)]
pub enum SingleExpectationComparison {
    /// Expectation was not specified (ignored).
    NotSpecified,
    /// Actual matches expected.
    Matches,
    /// Actual differs from expected.
    Differs {
        /// The expected value.
        expected: String,
        /// The actual value.
        actual: String,
    },
}

impl SingleExpectationComparison {
    /// Returns whether this comparison indicates a failure.
    pub const fn is_failure(&self) -> bool {
        matches!(self, Self::Differs { .. })
    }
}

/// Comparison against inline expectations.
#[derive(Debug)]
pub struct ExpectationComparison {
    /// Comparison of exit code.
    pub exit_code: SingleExpectationComparison,
    /// Comparison of stdout.
    pub stdout: SingleExpectationComparison,
    /// Comparison of stderr.
    pub stderr: SingleExpectationComparison,
    /// Whether snapshot comparison was used.
    pub snapshot_used: bool,
    /// Snapshot comparison result (if used).
    pub snapshot_result: Option<SnapshotResult>,
}

impl ExpectationComparison {
    /// Creates an empty expectation comparison (all not specified).
    pub const fn not_specified() -> Self {
        Self {
            exit_code: SingleExpectationComparison::NotSpecified,
            stdout: SingleExpectationComparison::NotSpecified,
            stderr: SingleExpectationComparison::NotSpecified,
            snapshot_used: false,
            snapshot_result: None,
        }
    }

    /// Returns whether this comparison indicates a failure.
    pub fn is_failure(&self) -> bool {
        self.exit_code.is_failure()
            || self.stdout.is_failure()
            || self.stderr.is_failure()
            || self
                .snapshot_result
                .as_ref()
                .is_some_and(|r| r.is_failure())
    }

    /// Returns whether any expectations were checked.
    pub const fn has_any_checks(&self) -> bool {
        !matches!(self.exit_code, SingleExpectationComparison::NotSpecified)
            || !matches!(self.stdout, SingleExpectationComparison::NotSpecified)
            || !matches!(self.stderr, SingleExpectationComparison::NotSpecified)
            || self.snapshot_used
    }
}

/// Result of a snapshot comparison.
#[derive(Debug)]
pub enum SnapshotResult {
    /// Snapshot matches.
    Matches,
    /// Snapshot differs (new snapshot created or update needed).
    Differs {
        /// Description of the difference.
        message: String,
    },
}

impl SnapshotResult {
    /// Returns whether this result indicates a failure.
    pub const fn is_failure(&self) -> bool {
        matches!(self, Self::Differs { .. })
    }
}

/// Combined test comparison result.
pub struct TestComparison {
    /// Expectation comparison (if expectations were defined).
    pub expectation: ExpectationComparison,
    /// Duration of the test run.
    pub duration: std::time::Duration,
}

impl TestComparison {
    /// Returns whether this comparison indicates a failure.
    pub fn is_failure(&self) -> bool {
        self.expectation.is_failure()
    }

    /// Creates a skipped comparison.
    pub const fn skipped() -> Self {
        Self {
            expectation: ExpectationComparison::not_specified(),
            duration: std::time::Duration::ZERO,
        }
    }
}

/// Compares an expected string with the actual one, optionally ignoring whitespace.
pub fn output_matches(expected: &str, actual: &str, ignore_whitespace: bool) -> bool {
    if ignore_whitespace {
        let whitespace_re = regex::Regex::new(r"\s+").unwrap();

        let cleaned_expected = whitespace_re.replace_all(expected, " ").to_string();
        let cleaned_actual = whitespace_re.replace_all(actual, " ").to_string();

        cleaned_expected == cleaned_actual
    } else {
        expected == actual
    }
}
