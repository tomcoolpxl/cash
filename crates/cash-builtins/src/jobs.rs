use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionResult, builtins, jobs};

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

    #[expect(clippy::too_many_lines)]
    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        context.shell.jobs_mut().refresh_statuses()?;

        if self.list_changed_only {
            let mut missing = false;
            let requested_ids = if self.job_specs.is_empty() {
                None
            } else {
                let mut ids = Vec::new();
                for spec in &self.job_specs {
                    if let Some(job) = context.shell.jobs_mut().resolve_job_spec(spec) {
                        ids.push(job.id);
                    } else {
                        writeln!(
                            context.stderr(),
                            "{}: {spec}: no such job",
                            context.command_name
                        )?;
                        missing = true;
                    }
                }
                Some(ids)
            };

            let state_filtered_ids = context
                .shell
                .jobs()
                .snapshot()
                .into_iter()
                .filter(|snapshot| self.matches_state_filter(&snapshot.state))
                .filter(|snapshot| {
                    requested_ids
                        .as_ref()
                        .is_none_or(|ids| ids.contains(&snapshot.id))
                })
                .map(|snapshot| snapshot.id)
                .collect::<Vec<_>>();
            let notification_filter =
                if self.running_jobs_only || self.stopped_jobs_only || requested_ids.is_some() {
                    Some(state_filtered_ids.as_slice())
                } else {
                    None
                };
            let notifications = context
                .shell
                .jobs_mut()
                .take_status_notifications(notification_filter);
            for snapshot in notifications {
                self.display_rendered(
                    &context,
                    &snapshot.to_string(),
                    &snapshot.state,
                    snapshot.pid,
                )?;
            }
            return Ok(if missing {
                ExecutionResult::general_error()
            } else {
                ExecutionResult::success()
            });
        }

        let mut displayed_ids = Vec::new();
        if self.job_specs.is_empty() {
            // cash: a subshell owns no jobs but can see the parent's (read-only).
            for snapshot in context.shell.jobs().inherited() {
                if self.matches_state_filter(&snapshot.state) {
                    displayed_ids.push(snapshot.id);
                }
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
                if self.matches_state_filter(&job.state) {
                    displayed_ids.push(job.id);
                }
                self.display_job(&context, job)?;
            }
        } else {
            // cash: `jobs %1` used to refuse. The resolver already existed for `kill %1`
            // and `wait %1` (D22); only this caller was missing.
            let mut missing = false;
            for spec in &self.job_specs {
                let Some(job) = context.shell.jobs_mut().resolve_job_spec(spec) else {
                    writeln!(
                        context.stderr(),
                        "{}: {spec}: no such job",
                        context.command_name
                    )?;
                    missing = true;
                    continue;
                };

                // Take what printing needs while the mutable borrow is held; a `Job`
                // owns task handles and cannot be cloned.
                let rendered = job.to_string();
                let state = job.state.clone();
                let pid = job.representative_pid();
                if self.matches_state_filter(&state) {
                    displayed_ids.push(job.id);
                }
                self.display_rendered(&context, &rendered, &state, pid)?;
            }

            if missing {
                return Ok(ExecutionResult::general_error());
            }
        }

        context
            .shell
            .jobs_mut()
            .mark_notifications(Some(&displayed_ids));
        Ok(ExecutionResult::success())
    }
}

impl JobsCommand {
    const fn matches_state_filter(&self, state: &jobs::JobState) -> bool {
        (!self.running_jobs_only || matches!(state, jobs::JobState::Running))
            && (!self.stopped_jobs_only || matches!(state, jobs::JobState::Stopped))
    }

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

        self.display_rendered(
            context,
            &job.to_string(),
            &job.state,
            job.representative_pid(),
        )
    }

    /// Print one job, given everything printing needs.
    ///
    /// Split out so that a job reached through a job spec — which needs a mutable borrow
    /// to resolve — can be printed after that borrow ends.
    fn display_rendered(
        &self,
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        rendered: &str,
        state: &jobs::JobState,
        pid: Option<i32>,
    ) -> Result<(), cash_core::Error> {
        if self.running_jobs_only && !matches!(state, jobs::JobState::Running) {
            return Ok(());
        }
        if self.stopped_jobs_only && !matches!(state, jobs::JobState::Stopped) {
            return Ok(());
        }

        if self.show_pids_only {
            if let Some(pid) = pid {
                writeln!(context.stdout(), "{pid}")?;
            }
            return Ok(());
        }

        // cash: `jobs -l` used to refuse outright. It is how a person finds the pid of a
        // background job to hand to something else, and the pid is already known — the
        // job tracks it for `$!` and `kill %1` (D22).
        if self.also_show_pids {
            match pid {
                // bash puts the pid between the job marker and the status:
                //     [1]+ 12345 Running   sleep 30 &
                //
                // The marker ends after `]` and its optional `+`/`-`; splitting on the
                // first space instead would land inside the command, because the job's
                // own rendering separates status from command with a tab.
                Some(pid) => match insertion_point(rendered) {
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

        writeln!(context.stdout(), "{rendered}")?;

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
