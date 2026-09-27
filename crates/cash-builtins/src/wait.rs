use clap::Parser;
use std::io::Write;

use cash_core::jobs::WaitTarget;
use cash_core::{ExecutionExitCode, ExecutionResult, builtins, expansion};
use futures::{FutureExt, future::select_all};

/// Wait for jobs to terminate.
///
/// A background job that finishes leaves the job table, and its status is saved (see
/// `JobManager`): `wait PID` or `wait %N` reads it afterwards, and `wait -n` returns each
/// finished job once, oldest first, before waiting for a running one. In POSIX mode
/// `wait -n` also forgets the status it returns, so a later `wait PID` finds nothing
/// (Bash 5.3).
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

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.wait_for_first_or_next {
            return self.wait_for_next(context).await;
        }

        let mut result = ExecutionResult::success();

        if self.ids.is_empty() {
            // Wait for all jobs.
            let jobs = context.shell.jobs_mut().wait_all().await?;
            // A CHLD trap runs for the children reaped, inside `wait`, as in Bash.
            context.shell.run_pending_chld_traps(&context.params).await;

            if context.shell.options().enable_job_control {
                for job in jobs {
                    writeln!(context.stdout(), "{job}")?;
                }
            }
            return Ok(result);
        }

        for id in &self.ids {
            if id.starts_with('%') {
                // It's a job spec.
                let job_id = context
                    .shell
                    .jobs_mut()
                    .resolve_job_spec(id)
                    .map(|job| job.id);
                if let Some(job_id) = job_id {
                    result = self.wait_for_job(context.shell, job_id).await?;
                } else if let Some(status) = id
                    .strip_prefix('%')
                    .and_then(|n| n.parse().ok())
                    .and_then(|n| context.shell.jobs_mut().collect_saved_job(n))
                {
                    result = ExecutionResult::new(status);
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

                let job_id = context.shell.jobs_mut().resolve_pid(pid).map(|job| job.id);
                if let Some(job_id) = job_id {
                    result = self.wait_for_job(context.shell, job_id).await?;
                } else if let Some(status) = context.shell.jobs_mut().collect_saved_pid(pid) {
                    result = ExecutionResult::new(status);
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

        context.shell.run_pending_chld_traps(&context.params).await;
        Ok(result)
    }
}

impl WaitCommand {
    /// Waits for the job with this id; once it has finished, it leaves the table and its
    /// status is saved for another `wait PID`.
    async fn wait_for_job<SE: cash_core::ShellExtensions>(
        &self,
        shell: &mut cash_core::Shell<SE>,
        job_id: usize,
    ) -> Result<ExecutionResult, cash_core::Error> {
        let jobs = shell.jobs_mut();
        let Some(job) = jobs.jobs.iter_mut().find(|job| job.id == job_id) else {
            return Ok(ExecutionResult::success());
        };
        let result = if self.wait_for_terminate {
            job.wait_for_termination().await?
        } else {
            job.wait().await?
        };
        if !job.has_unwaited_tasks() {
            jobs.remove_waited_job(job_id, u8::from(result.exit_code), true, false);
        }
        Ok(result)
    }

    /// `wait -n`: the oldest finished job not yet returned, or else the first of the
    /// running ones to finish.
    async fn wait_for_next<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, cash_core::Error> {
        let forget = context.shell.options().posix_mode;
        let jobs = context.shell.jobs_mut();
        // Move every job that has finished into the saved statuses, in table order.
        let _ = jobs.poll()?;

        let mut pids = Vec::new();
        let mut ids = Vec::new();
        for spec in &self.ids {
            if spec.starts_with('%') {
                if let Some(job) = jobs.resolve_job_spec(spec) {
                    ids.push(job.id);
                } else if let Some(n) = spec.strip_prefix('%').and_then(|n| n.parse().ok()) {
                    ids.push(n);
                }
            } else if let Ok(pid) = cash_core::int_utils::parse(spec, 10) {
                if let Some(job) = jobs.resolve_pid(pid) {
                    ids.push(job.id);
                }
                pids.push(pid);
            }
        }
        let target = if self.ids.is_empty() {
            WaitTarget::Any
        } else {
            WaitTarget::Only {
                pids: &pids,
                ids: &ids,
            }
        };

        let (result, id, pid) =
            if let Some((status, pid, id)) = jobs.take_unreported(&target, forget) {
                (ExecutionResult::new(status), id, pid)
            } else {
                let futures: Vec<_> = jobs
                    .jobs
                    .iter_mut()
                    .filter(|job| {
                        (self.ids.is_empty() || ids.contains(&job.id)) && job.has_unwaited_tasks()
                    })
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
                    jobs.remove_waited_job(id, u8::from(result.exit_code), !forget, true);
                }
                (result, id, pid)
            };

        if let Some(name) = &self.variable_to_receive_id {
            let value = pid.map_or_else(|| std::format!("%{id}"), |pid| pid.to_string());
            expansion::assign_to_named_parameter_in_builtin(
                context.shell,
                &context.params,
                name,
                value,
            )
            .await?;
        }
        Ok(result)
    }
}
