use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionExitCode, ExecutionResult, builtins, expansion};
use futures::{FutureExt, future::select_all};

/// Wait for jobs to terminate.
#[derive(Parser)]
pub(crate) struct WaitCommand {
    /// Wait for specified job to terminate (instead of change status).
    #[arg(short = 'f')]
    wait_for_terminate: bool,

    /// Wait for a single job to change status; if jobs are specified, waits for
    /// the first to change status, and otherwise waits for the next change.
    #[arg(short = 'n')]
    wait_for_first_or_next: bool,

    /// Name of variable to receive the job ID of the job whose status is indicated.
    #[arg(short = 'p', value_name = "VAR_NAME")]
    variable_to_receive_id: Option<String>,

    /// Process IDs or job specs to wait for.
    ids: Vec<String>,
}

impl builtins::Command for WaitCommand {
    type Error = cash_core::Error;

    #[allow(
        clippy::too_many_lines,
        reason = "wait keeps option dispatch and job resolution together"
    )]
    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.wait_for_first_or_next {
            let jobs = context.shell.jobs_mut();
            let wanted: Vec<usize> = if self.ids.is_empty() {
                jobs.jobs.iter().map(|job| job.id).collect()
            } else {
                self.ids
                    .iter()
                    .filter_map(|id| {
                        if id.starts_with('%') {
                            jobs.resolve_job_spec(id).map(|job| job.id)
                        } else {
                            cash_core::int_utils::parse(id, 10)
                                .ok()
                                .and_then(|pid| jobs.resolve_pid(pid).map(|job| job.id))
                        }
                    })
                    .collect()
            };
            let futures: Vec<_> = jobs
                .jobs
                .iter_mut()
                .filter(|job| wanted.contains(&job.id) && job.has_unwaited_tasks())
                .map(|job| {
                    let id = job.id;
                    let pid = job.representative_pid();
                    let force_termination = self.wait_for_terminate;
                    async move {
                        let result = if force_termination {
                            job.wait_for_termination().await
                        } else {
                            job.wait().await
                        };
                        (result, id, pid)
                    }
                    .boxed()
                })
                .collect();
            if futures.is_empty() {
                return Ok(ExecutionResult::new(127));
            }
            let ((result, id, pid), _, _) = select_all(futures).await;
            let result = result?;
            if jobs
                .jobs
                .iter()
                .any(|job| job.id == id && !job.has_unwaited_tasks())
            {
                jobs.remove_waited_job(id);
            }
            if let Some(name) = &self.variable_to_receive_id {
                let value = pid.map_or_else(|| std::format!("%{id}"), |pid| pid.to_string());
                expansion::assign_to_named_parameter(context.shell, &context.params, name, value)
                    .await?;
            }
            return Ok(result);
        }

        let mut result = ExecutionResult::success();

        if !self.ids.is_empty() {
            for id in &self.ids {
                if id.starts_with('%') {
                    // It's a job spec.
                    if let Some(job) = context.shell.jobs_mut().resolve_job_spec(id) {
                        result = if self.wait_for_terminate {
                            job.wait_for_termination().await?
                        } else {
                            job.wait().await?
                        };
                    } else {
                        writeln!(
                            context.stderr(),
                            "{}: no such job: {}",
                            context.command_name,
                            id
                        )?;

                        result = ExecutionExitCode::GeneralError.into();
                    }
                } else {
                    // It's a process ID. cash: `pid=$!; wait "$pid"` is the companion to
                    // `$!`, so a pid has to resolve back to the job that owns it.
                    let Ok(pid) = cash_core::int_utils::parse(id.as_str(), 10) else {
                        writeln!(
                            context.stderr(),
                            "{}: `{}': not a pid or valid job spec",
                            context.command_name,
                            id
                        )?;
                        result = ExecutionExitCode::GeneralError.into();
                        continue;
                    };

                    if let Some(job) = context.shell.jobs_mut().resolve_pid(pid) {
                        result = if self.wait_for_terminate {
                            job.wait_for_termination().await?
                        } else {
                            job.wait().await?
                        };
                    } else {
                        // bash's wording and its exit code: 127 specifically, which
                        // scripts test for to tell "already finished" from "never mine".
                        writeln!(
                            context.stderr(),
                            "{}: pid {} is not a child of this shell",
                            context.command_name,
                            pid
                        )?;
                        result = ExecutionResult::from(ExecutionExitCode::from(127u8));
                    }
                }
            }
        } else {
            // Wait for all jobs.
            let jobs = context.shell.jobs_mut().wait_all().await?;

            if context.shell.options().enable_job_control {
                for job in jobs {
                    writeln!(context.stdout(), "{job}")?;
                }
            }
        }

        Ok(result)
    }
}
