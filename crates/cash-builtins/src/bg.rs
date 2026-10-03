use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionResult, builtins};

/// Moves a job to run in the background.
#[derive(Parser)]
pub(crate) struct BgCommand {
    /// List of job specs to move to background.
    job_specs: Vec<String>,
}

impl builtins::Command for BgCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        // Without job control, as in a script, there is no foreground to move a job
        // between, and Bash refuses: `set -m` turns it on (the user, 2026-10-03).
        if !context.shell.options().enable_job_control {
            writeln!(
                context.error_stream(),
                "{}: no job control",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        }

        let mut exit_code = ExecutionResult::success();

        if !self.job_specs.is_empty() {
            for job_spec in &self.job_specs {
                if let Some(job) = context.shell.jobs_mut().resolve_job_spec(job_spec) {
                    job.move_to_background()?;
                } else {
                    writeln!(
                        context.error_stream(),
                        "{}: {}: no such job",
                        context.command_name,
                        job_spec
                    )?;
                    exit_code = ExecutionResult::general_error();
                }
            }
        } else {
            if let Some(job) = context.shell.jobs_mut().current_job_mut() {
                job.move_to_background()?;
            } else {
                writeln!(
                    context.error_stream(),
                    "{}: no current job",
                    context.command_name
                )?;
                exit_code = ExecutionResult::general_error();
            }
        }

        Ok(exit_code)
    }
}
