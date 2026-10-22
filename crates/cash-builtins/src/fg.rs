use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionResult, builtins, jobs, sys};

/// Move a specified job to the foreground.
#[derive(Parser)]
pub(crate) struct FgCommand {
    /// Job spec for the job to move to the foreground; if not specified, the current job is moved.
    job_spec: Option<String>,

    /// Words after the job spec, which Bash ignores (`%1 extra` is `fg %1 extra`).
    #[arg(hide = true, trailing_var_arg = true, allow_hyphen_values = true)]
    _ignored: Vec<String>,
}

impl builtins::Command for FgCommand {
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

        // The job's command line goes to standard output, as in Bash.
        let mut stdout = context.stdout();

        // Read interactive option before taking mutable borrow on jobs
        let is_interactive = context.shell.options().interactive;
        let ctrl_z = context.shell.ctrl_z_stops_foreground_jobs();

        let found = match &self.job_spec {
            Some(job_spec) => context.shell.jobs_mut().resolve_job_spec(job_spec),
            None => context.shell.jobs_mut().current_job_mut(),
        };
        let Some(job) = found else {
            // On standard error, the builtin's name first, as in Bash: it went to
            // standard output as `%3: fg: no such job`.
            match &self.job_spec {
                Some(job_spec) => writeln!(
                    context.error_stream(),
                    "{}: {}: no such job",
                    context.command_name,
                    job_spec
                )?,
                None => writeln!(
                    context.error_stream(),
                    "{}: no current job",
                    context.command_name
                )?,
            }
            return Ok(ExecutionResult::general_error());
        };

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
        } else {
            // A job that ended in the foreground leaves the table unreported, as in Bash:
            // it was no `Done` later, and its number is free for the next job.
            let id = job.id;
            context.shell.jobs_mut().remove_job(id);
        }

        Ok(result)
    }
}
