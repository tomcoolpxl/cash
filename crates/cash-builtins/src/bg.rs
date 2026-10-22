use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionResult, builtins, jobs};

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
        let mut stdout = context.stdout();
        let mut stderr = context.error_stream();
        let name = context.command_name.as_str();

        if !self.job_specs.is_empty() {
            for job_spec in &self.job_specs {
                if let Some(job) = context.shell.jobs_mut().resolve_job_spec(job_spec) {
                    resume(job, name, &mut stdout, &mut stderr)?;
                } else {
                    writeln!(stderr, "{name}: {job_spec}: no such job")?;
                    exit_code = ExecutionResult::general_error();
                }
            }
        } else {
            if let Some(job) = context.shell.jobs_mut().current_job_mut() {
                resume(job, name, &mut stdout, &mut stderr)?;
            } else {
                writeln!(stderr, "{name}: no current job")?;
                exit_code = ExecutionResult::general_error();
            }
        }

        Ok(exit_code)
    }
}

/// Resumes a stopped job in the background and names it, `[1]+ sleep 9 &`, as Bash does;
/// one already running is only said to be, and that is no failure.
fn resume(
    job: &mut jobs::Job,
    name: &str,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<(), cash_core::Error> {
    if matches!(job.state, jobs::JobState::Running) {
        writeln!(stderr, "{name}: job {} already in background", job.id)?;
        return Ok(());
    }
    job.move_to_background()?;
    writeln!(
        stdout,
        "[{}]{:1} {} &",
        job.id,
        job.annotation().to_string(),
        job.command_line
    )?;
    Ok(())
}
