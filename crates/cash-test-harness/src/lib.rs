//! Test harness library for cash's YAML compatibility cases.
//!
//! This crate runs YAML-based integration tests against cash and checks each case's
//! output against inline expectations specified in the YAML (`expected_stdout`,
//! `expected_stderr`, `expected_exit_code`) or against insta snapshots (`snapshot: true`).

#![expect(clippy::missing_panics_doc)]
#![expect(clippy::unwrap_used)]

mod comparison;
mod config;
mod execution;
mod reporting;
mod runner;
mod testcase;
pub mod util;

pub use comparison::{
    ExpectationComparison, SingleExpectationComparison, SnapshotResult, TestComparison,
};
pub use config::{OutputFormat, RunnerConfig, ShellConfig, TestOptions};
pub use execution::RunResult;
pub use runner::TestRunner;
pub use testcase::{TestCase, TestCaseSet, TestFile};
