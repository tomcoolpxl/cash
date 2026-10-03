//! Reporting utilities for test results.

use crate::comparison::{ExpectationComparison, SingleExpectationComparison, TestComparison};
use crate::config::{OutputFormat, TestOptions};
use crate::util::write_diff;
use anyhow::Result;
use colored::Colorize;
use std::io::Write;

/// Result of running a single test case.
pub struct TestCaseResult {
    /// Name of the test case.
    pub name: Option<String>,
    /// Whether the test succeeded.
    pub success: bool,
    /// Whether the test was skipped.
    pub skip: bool,
    /// Whether this is a known failure.
    pub known_failure: bool,
    /// The comparison result.
    pub comparison: TestComparison,
}

impl TestCaseResult {
    /// Reports this result in pretty format.
    pub fn report_pretty(&self, options: &TestOptions) -> Result<()> {
        self.write_details(std::io::stderr(), options)
    }

    /// Writes the details of this result to a writer.
    pub fn write_details<W: Write>(&self, mut writer: W, options: &TestOptions) -> Result<()> {
        if self.skip {
            return Ok(());
        }

        if !options.verbose {
            if (!self.comparison.is_failure() && !self.known_failure)
                || (self.comparison.is_failure() && self.known_failure)
            {
                return Ok(());
            }
        }

        write!(
            writer,
            "* {}: [{}]... ",
            "Test case".bright_yellow(),
            self.name
                .as_ref()
                .map_or_else(|| "(unnamed)", |n| n.as_str())
                .italic()
        )?;

        if !self.comparison.is_failure() {
            if self.known_failure {
                writeln!(writer, "{}", "unexpected success.".bright_red())?;
            } else {
                writeln!(writer, "{}", "ok.".bright_green())?;
                return Ok(());
            }
        } else if self.known_failure {
            writeln!(writer, "{}", "known failure.".bright_magenta())?;
            if !options.display_known_failure_details {
                return Ok(());
            }
        }

        writeln!(writer)?;

        // Report expectation comparison
        if self.comparison.expectation.has_any_checks() {
            self.write_expectation_details(&mut writer, &self.comparison.expectation)?;
        }

        if !self.success {
            writeln!(writer, "    {}", "FAILED.".bright_red())?;
        }

        Ok(())
    }

    #[expect(clippy::unused_self)]
    fn write_expectation_details<W: Write>(
        &self,
        writer: &mut W,
        expectation: &ExpectationComparison,
    ) -> Result<()> {
        writeln!(writer, "    {} check:", "Expectation".cyan())?;

        // Exit code
        match &expectation.exit_code {
            SingleExpectationComparison::NotSpecified => {}
            SingleExpectationComparison::Matches => {
                writeln!(writer, "      exit code matches {}", "✔️".green())?;
            }
            SingleExpectationComparison::Differs { expected, actual } => {
                writeln!(
                    writer,
                    "      exit code {}: expected {}, got {}",
                    "DIFFERS".bright_red(),
                    expected.cyan(),
                    actual.bright_red()
                )?;
            }
        }

        // Stdout
        match &expectation.stdout {
            SingleExpectationComparison::NotSpecified => {}
            SingleExpectationComparison::Matches => {
                writeln!(writer, "      stdout matches {}", "✔️".green())?;
            }
            SingleExpectationComparison::Differs { expected, actual } => {
                writeln!(writer, "      stdout {}", "DIFFERS:".bright_red())?;
                write_diff(writer, 8, expected.as_str(), actual.as_str())?;
            }
        }

        // Stderr
        match &expectation.stderr {
            SingleExpectationComparison::NotSpecified => {}
            SingleExpectationComparison::Matches => {
                writeln!(writer, "      stderr matches {}", "✔️".green())?;
            }
            SingleExpectationComparison::Differs { expected, actual } => {
                writeln!(writer, "      stderr {}", "DIFFERS:".bright_red())?;
                write_diff(writer, 8, expected.as_str(), actual.as_str())?;
            }
        }

        // Snapshot
        if expectation.snapshot_used {
            if let Some(result) = &expectation.snapshot_result {
                match result {
                    crate::comparison::SnapshotResult::Matches => {
                        writeln!(writer, "      snapshot matches {}", "✔️".green())?;
                    }
                    crate::comparison::SnapshotResult::Differs { message } => {
                        writeln!(
                            writer,
                            "      snapshot {}: {}",
                            "DIFFERS".bright_red(),
                            message
                        )?;
                    }
                }
            }
        }

        Ok(())
    }
}

/// Results from running a set of test cases.
pub struct TestCaseSetResults {
    /// Name of the test case set.
    pub name: Option<String>,
    /// Number of successful tests.
    pub success_count: u32,
    /// Number of skipped tests.
    pub skip_count: u32,
    /// Number of known failures.
    pub known_failure_count: u32,
    /// Number of failed tests.
    pub fail_count: u32,
    /// Individual test case results.
    pub test_case_results: Vec<TestCaseResult>,
    /// Total duration of the successful tests.
    pub success_duration: std::time::Duration,
}

impl TestCaseSetResults {
    /// Reports these results in pretty format.
    pub fn report_pretty(&self, options: &TestOptions) -> Result<()> {
        self.write_details(std::io::stderr(), options)
    }

    fn write_details<W: Write>(&self, mut writer: W, options: &TestOptions) -> Result<()> {
        if options.verbose {
            writeln!(
                writer,
                "=================== {}: [{}] ===================",
                "Running test case set".blue(),
                self.name
                    .as_ref()
                    .map_or_else(|| "(unnamed)", |n| n.as_str())
                    .italic(),
            )?;
        }

        for test_case_result in &self.test_case_results {
            test_case_result.report_pretty(options)?;
        }

        if options.verbose {
            writeln!(
                writer,
                "    successful cases ran in {:?}",
                self.success_duration
            )?;
        }

        Ok(())
    }
}

/// Reports test results based on the configured output format.
pub fn report_results(results: Vec<TestCaseSetResults>, options: &TestOptions) -> Result<()> {
    match options.format {
        OutputFormat::Pretty => report_results_pretty(results, options),
        OutputFormat::Junit => report_results_junit(results, options),
        OutputFormat::Terse => Ok(()),
    }
}

fn report_results_pretty(results: Vec<TestCaseSetResults>, options: &TestOptions) -> Result<()> {
    for result in results {
        result.report_pretty(options)?;
    }
    Ok(())
}

fn report_results_junit(results: Vec<TestCaseSetResults>, options: &TestOptions) -> Result<()> {
    let mut report = junit_report::Report::new();

    for result in results {
        let mut suite = junit_report::TestSuite::new(result.name.unwrap_or(String::new()).as_str());
        for r in result.test_case_results {
            let test_case_name = r.name.as_deref().unwrap_or("");
            let mut test_case: junit_report::TestCase = if r.success {
                junit_report::TestCase::success(test_case_name, r.comparison.duration.try_into()?)
            } else if r.known_failure {
                junit_report::TestCase::skipped(test_case_name)
            } else {
                junit_report::TestCase::failure(
                    test_case_name,
                    r.comparison.duration.try_into()?,
                    "test failure",
                    "failed",
                )
            };

            let mut output_buf: Vec<u8> = vec![];
            r.write_details(&mut output_buf, options)?;

            let output_as_string = String::from_utf8(output_buf)?;
            test_case.set_system_out(strip_ansi_escapes::strip_str(output_as_string).as_str());

            suite.add_testcase(test_case);
        }

        report.add_testsuite(suite);
    }

    report.write_xml(std::io::stdout())?;
    writeln!(std::io::stdout())?;

    Ok(())
}
