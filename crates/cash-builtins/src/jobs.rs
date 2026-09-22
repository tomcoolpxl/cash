use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionResult, builtins, error, jobs};

/// Manage jobs.
#[derive(Parser)]
pub(crate) struct JobsCommand {
    /// Also show process IDs.
    #[arg(short = 'l')]
    also_show_pids: bool,

    /// List only jobs that have changed status since the last notification.
    #[arg(short = 'n')]
    list_changed_only: bool,

    /// Show only process IDs.
    #[arg(short = 'p')]
    show_pids_only: bool,

    /// Show only running jobs.
    #[arg(short = 'r')]
    running_jobs_only: bool,

    /// Show only stopped jobs.
    #[arg(short = 's')]
    stopped_jobs_only: bool,

    /// Job specs to list.
    // TODO(jobs): Add -x option
    job_specs: Vec<String>,
}

impl builtins::Command for JobsCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        if self.list_changed_only {
            return error::unimp("jobs -n");
        }

        if self.job_specs.is_empty() {
            // cash: a subshell owns no jobs but can see the parent's (read-only).
            for snapshot in context.shell.jobs().inherited() {
                if self.show_pids_only {
                    if let Some(pid) = snapshot.pid {
                        writeln!(context.stdout(), "{pid}")?;
                    }
                } else if self.also_show_pids
                    && let Some(pid) = snapshot.pid
                {
                    // A subshell sees the parent's jobs as snapshots, and a snapshot does
                    // carry its pid — `jobs -l` inside `$( )` would otherwise differ from
                    // the same command outside it.
                    let rendered = snapshot.to_string();
                    match insertion_point(&rendered) {
                        Some(at) => {
                            let (marker, rest) = rendered.split_at(at);
                            writeln!(context.stdout(), "{marker} {pid} {rest}")?;
                        }
                        None => writeln!(context.stdout(), "{rendered} {pid}")?,
                    }
                } else {
                    writeln!(context.stdout(), "{snapshot}")?;
                }
            }

            for job in &context.shell.jobs().jobs {
                self.display_job(&context, job)?;
            }
        } else {
            return error::unimp("jobs with job specs");
        }

        Ok(ExecutionResult::success())
    }
}

impl JobsCommand {
    fn display_job(
        &self,
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        job: &jobs::Job,
    ) -> Result<(), cash_core::Error> {
        if self.running_jobs_only && !matches!(job.state, jobs::JobState::Running) {
            return Ok(());
        }
        if self.stopped_jobs_only && !matches!(job.state, jobs::JobState::Stopped) {
            return Ok(());
        }

        if self.show_pids_only {
            if let Some(pid) = job.representative_pid() {
                writeln!(context.stdout(), "{pid}")?;
            }
            return Ok(());
        }

        // cash: `jobs -l` used to refuse outright. It is how a person finds the pid of a
        // background job to hand to something else, and the pid is already known — the
        // job tracks it for `$!` and `kill %1` (D22).
        if self.also_show_pids {
            let rendered = job.to_string();
            match job.representative_pid() {
                // bash puts the pid between the job marker and the status:
                //     [1]+ 12345 Running   sleep 30 &
                //
                // The marker ends after `]` and its optional `+`/`-`; splitting on the
                // first space instead would land inside the command, because the job's
                // own rendering separates status from command with a tab.
                Some(pid) => match insertion_point(&rendered) {
                    Some(at) => {
                        let (marker, rest) = rendered.split_at(at);
                        writeln!(context.stdout(), "{marker} {pid} {rest}")?;
                    }
                    None => writeln!(context.stdout(), "{rendered} {pid}")?,
                },
                None => writeln!(context.stdout(), "{rendered}")?,
            }
            return Ok(());
        }

        writeln!(context.stdout(), "{job}")?;

        Ok(())
    }
}

/// Where the pid goes in a `jobs -l` line: just past the `[N]` marker and any `+`/`-`.
fn insertion_point(rendered: &str) -> Option<usize> {
    let close = rendered.find(']')? + 1;
    let after = rendered
        .get(close..)
        .and_then(|rest| rest.chars().next())
        .filter(|c| matches!(c, '+' | '-'))
        .map_or(close, |c| close + c.len_utf8());
    Some(after)
}
