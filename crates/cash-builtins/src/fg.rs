use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionResult, builtins, jobs, sys};

/// Move a specified job to the foreground.
#[derive(Parser)]
pub(crate) struct FgCommand {
    /// Job spec for the job to move to the foreground; if not specified, the current job is moved.
    job_spec: Option<String>,
}

impl builtins::Command for FgCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        // The job's command line goes to standard output, as in Bash.
        let mut stdout = context.stdout();

        // Read interactive option before taking mutable borrow on jobs
        let is_interactive = context.shell.options().interactive;
        let ctrl_z = context.shell.ctrl_z_stops_foreground_jobs();

        if let Some(job_spec) = &self.job_spec {
            if let Some(job) = context.shell.jobs_mut().resolve_job_spec(job_spec) {
                job.move_to_foreground()?;
                writeln!(stdout, "{}", job.command_line)?;

                let result = job.wait_in_foreground(ctrl_z).await?;
                if is_interactive {
                    sys::terminal::move_self_to_foreground()?;
                }

                if matches!(job.state, jobs::JobState::Stopped) {
                    // N.B. We use the '\r' to overwrite any ^Z output.
                    let formatted = job.to_string();
                    writeln!(context.error_stream(), "\r{formatted}")?;
                }

                Ok(result)
            } else {
                // On standard error, the builtin's name first, as in Bash: it went to
                // standard output as `%3: fg: no such job`.
                writeln!(
                    context.error_stream(),
                    "{}: {}: no such job",
                    context.command_name,
                    job_spec
                )?;
                Ok(ExecutionResult::general_error())
            }
        } else {
            if let Some(job) = context.shell.jobs_mut().current_job_mut() {
                job.move_to_foreground()?;
                writeln!(stdout, "{}", job.command_line)?;

                let result = job.wait_in_foreground(ctrl_z).await?;
                if is_interactive {
                    sys::terminal::move_self_to_foreground()?;
                }

                if matches!(job.state, jobs::JobState::Stopped) {
                    // N.B. We use the '\r' to overwrite any ^Z output.
                    let formatted = job.to_string();
                    writeln!(context.error_stream(), "\r{formatted}")?;
                }

                Ok(result)
            } else {
                writeln!(
                    context.error_stream(),
                    "{}: no current job",
                    context.command_name
                )?;
                Ok(ExecutionResult::general_error())
            }
        }
    }
}
