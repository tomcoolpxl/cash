//! Execution logic for running shell commands.

use crate::config::ShellConfig;
use crate::testcase::{ShellInvocation, TestCase, TestCaseSet, TestFile};
use anyhow::{Context, Result};
use assert_fs::fixture::{FileWriteStr, PathChild};
use std::{path::PathBuf, process::ExitStatus};

/// Default timeout for test commands in seconds.
pub const DEFAULT_TIMEOUT_IN_SECONDS: u64 = 15;

/// Result of running a shell command.
#[derive(Debug)]
pub struct RunResult {
    /// Exit status of the command.
    pub exit_status: ExitStatus,
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
}

impl TestCase {
    /// Runs this test case with the given shell configuration.
    pub fn run_shell(
        &self,
        shell_config: &ShellConfig,
        working_dir: &assert_fs::TempDir,
    ) -> Result<RunResult> {
        let test_cmd = self.create_command_for_shell(shell_config, working_dir);

        let result = if self.pty {
            self.run_command_with_pty(test_cmd)?
        } else {
            self.run_command_with_stdin(test_cmd)?
        };

        Ok(result)
    }

    /// Creates the test files in the given temporary directory.
    pub fn create_test_files_in(
        &self,
        temp_dir: &assert_fs::TempDir,
        test_case_set: &TestCaseSet,
    ) -> Result<()> {
        for test_file in test_case_set
            .common_test_files
            .iter()
            .chain(self.test_files.iter())
        {
            Self::create_test_file(temp_dir, test_file, &test_case_set.source_dir)?;
        }

        Ok(())
    }

    fn create_test_file(
        temp_dir: &assert_fs::TempDir,
        test_file: &TestFile,
        source_dir: &std::path::Path,
    ) -> Result<()> {
        let test_file_path = temp_dir.child(test_file.path.as_path());

        if let Some(source_path) = &test_file.source_path {
            if !test_file.contents.is_empty() {
                return Err(anyhow::anyhow!(
                    "test file {} has both contents and source_path",
                    test_file_path.to_string_lossy()
                ));
            }

            if source_path.is_absolute() {
                return Err(anyhow::anyhow!(
                    "source_path {} is not a relative path",
                    source_path.to_string_lossy()
                ));
            }

            let abs_source_path = source_dir.join(source_path);

            let source_contents = std::fs::read_to_string(&abs_source_path)
                .with_context(|| format!("reading {}", abs_source_path.to_string_lossy()))?;

            test_file_path.write_str(source_contents.as_str())?;
        } else {
            test_file_path.write_str(test_file.contents.as_str())?;
        }

        Ok(())
    }

    /// Constructs a `Command` to invoke the given shell binary, optionally
    /// prepending a launcher (e.g., `["wasmtime", "run", "--"]`). When a
    /// launcher is provided, the first element becomes the program to execute
    /// and the rest are passed as leading arguments before the shell binary path.
    fn new_shell_command(
        shell_path: &std::path::Path,
        launcher: Option<&[String]>,
    ) -> std::process::Command {
        if let Some([program, leading_args @ ..]) = launcher {
            let mut cmd = std::process::Command::new(program);
            cmd.args(leading_args);
            cmd.arg(shell_path);
            cmd
        } else {
            std::process::Command::new(shell_path)
        }
    }

    fn create_command_for_shell(
        &self,
        shell_config: &ShellConfig,
        working_dir: &assert_fs::TempDir,
    ) -> std::process::Command {
        let mut test_cmd = match self.invocation {
            ShellInvocation::ExecShellBinary => {
                Self::new_shell_command(&shell_config.path, shell_config.launcher.as_deref())
            }
            ShellInvocation::ExecScript(_) => unimplemented!("exec script test"),
        };

        for arg in &shell_config.default_args {
            if !self.removed_default_args.contains(arg) {
                test_cmd.arg(arg);
            }
        }

        // Clear all environment vars for consistency.
        test_cmd.args(&self.args).env_clear();

        if let Ok(sysroot) = std::env::var("SystemRoot") {
            test_cmd.env("SystemRoot", sysroot);
        }
        if let Ok(temp) = std::env::var("TEMP") {
            test_cmd.env("TEMP", &temp);
            test_cmd.env("TMP", temp);
        }

        // Set locale to C for consistent behavior across systems.
        test_cmd.env("LC_ALL", "C");
        // Hard-code a well known prompt for PS1.
        test_cmd.env("PS1", "test$ ");
        // Try to get decent backtraces when problems get hit.
        test_cmd.env("RUST_BACKTRACE", "1");
        // Compute a PATH that contains what we need.
        test_cmd.env("PATH", shell_config.compute_test_path_var());

        // Keep interactive cases from discovering the developer's real profile through
        // platform home-directory APIs and appending their input to ~/.cash_history.
        // Individual history/home tests override this below through `home_dir` or `env`.
        test_cmd.env("HOME", working_dir.to_string_lossy().to_string());

        // Set up any env vars needed for collecting coverage data.
        let cli_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let default_target_dir = || cli_dir.parent().unwrap().join("target");
        let coverage_target_dir = std::env::var("CARGO_TARGET_DIR")
            .ok()
            .map_or_else(default_target_dir, PathBuf::from);
        test_cmd.env("CARGO_LLVM_COV_TARGET_DIR", &coverage_target_dir);
        test_cmd.env(
            "LLVM_PROFILE_FILE",
            coverage_target_dir.join("cash-%p-%40m.profraw"),
        );

        for (k, v) in &self.env {
            test_cmd.env(k, v);
        }

        if let Some(home_dir) = &self.home_dir {
            let abs_home_dir = if home_dir.is_relative() {
                working_dir.join(home_dir)
            } else {
                home_dir.to_owned()
            };

            test_cmd.env("HOME", abs_home_dir.to_string_lossy().to_string());
        }

        test_cmd.current_dir(working_dir.to_string_lossy().to_string());

        test_cmd
    }

    // The pty runner drove a Unix pseudo-terminal through expectrl; cash is Windows-only,
    // so a case that asks for one is refused rather than silently run without it.
    #[expect(
        clippy::unused_self,
        reason = "a method alongside `run_command_with_stdin`"
    )]
    fn run_command_with_pty(&self, _cmd: std::process::Command) -> Result<RunResult> {
        Err(anyhow::anyhow!("pty tests are not supported on Windows"))
    }

    fn run_command_with_stdin(&self, cmd: std::process::Command) -> Result<RunResult> {
        let mut test_cmd = assert_cmd::Command::from_std(cmd);

        test_cmd.timeout(std::time::Duration::from_secs(
            self.timeout_in_seconds
                .unwrap_or(DEFAULT_TIMEOUT_IN_SECONDS),
        ));

        if let Some(stdin) = &self.stdin {
            test_cmd.write_stdin(stdin.as_bytes());
        }

        let cmd_result = test_cmd.output()?;

        Ok(RunResult {
            exit_status: cmd_result.status,
            stdout: String::from_utf8_lossy(cmd_result.stdout.as_slice()).to_string(),
            stderr: String::from_utf8_lossy(cmd_result.stderr.as_slice()).to_string(),
        })
    }
}
