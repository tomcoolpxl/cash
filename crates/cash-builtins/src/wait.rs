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
///
/// A finished job that `jobs` has shown leaves the table too, and counts as returned:
/// `wait PID` reads its status, `wait -n` passes it over unless it is asked for by
/// process id or the shell is in POSIX mode, and a plain `wait` forgets it (Bash 5.3).
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
        let waiting = cash_core::ExecutionContext {
            shell: &mut *context.shell,
            command_name: context.command_name.clone(),
            params: context.params.clone(),
        };
        let mut notices = Vec::new();
        let result = self.wait(waiting, &mut notices).await;
        // A script tells here of the jobs it waited for that a signal ended, as Bash's
        // `wait` does: `script: line 3: 145010 Hangup  sleep 5`.
        context.shell.print_signal_notices(notices, &context.params);
        result
    }
}

impl WaitCommand {
    async fn wait<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
        notices: &mut Vec<String>,
    ) -> Result<ExecutionResult, cash_core::Error> {
        if self.wait_for_first_or_next {
            return self.wait_for_next(context, notices).await;
        }

        let mut result = ExecutionResult::success();
        // In POSIX mode a status is forgotten once `wait` has returned it (Bash 5.3).
        let forget = context.shell.options().posix_mode;

        if self.ids.is_empty() {
            // Wait for all jobs.
            let jobs = context.shell.jobs_mut().wait_all().await?;
            context.shell.jobs_mut().collect_signal_notices();
            notices.extend(context.shell.jobs_mut().take_signal_notices());
            // A CHLD trap runs for the children reaped, inside `wait`, as in Bash.
            context.shell.run_pending_chld_traps(&context.params).await;

            if context.shell.options().enable_job_control {
                let posix = context.shell.options().posix_mode;
                for job in jobs {
                    writeln!(context.stdout(), "{}", job.line(posix))?;
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
                    result = self.wait_for_job(context.shell, job_id, notices).await?;
                } else {
                    // A job spec names only a job in the table: once `jobs`, `wait` or
                    // `wait -n` has reported a finished job it names nothing, though
                    // `wait PID` still finds its status (Bash 5.3). Bash's words, and 127.
                    writeln!(
                        context.stderr(),
                        "{}: {}: no such job",
                        context.command_name,
                        id
                    )?;
                    result = ExecutionResult::from(ExecutionExitCode::from(127u8));
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
                    result = self.wait_for_job(context.shell, job_id, notices).await?;
                } else if let Some(status) = context.shell.jobs_mut().collect_saved_pid(pid, forget)
                {
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

    /// Waits for the job with this id. Once it has finished it is reported and stays in
    /// the table, so that `%N` and its pid find it again, until the next job, `jobs` or a
    /// plain `wait` takes it out (Bash 5.3). In POSIX mode it leaves at once, and its
    /// status with it.
    async fn wait_for_job<SE: cash_core::ShellExtensions>(
        &self,
        shell: &mut cash_core::Shell<SE>,
        job_id: usize,
        notices: &mut Vec<String>,
    ) -> Result<ExecutionResult, cash_core::Error> {
        let posix = shell.options().posix_mode;
        let jobs = shell.jobs_mut();
        let Some(job) = jobs.jobs.iter_mut().find(|job| job.id == job_id) else {
            return Ok(ExecutionResult::success());
        };
        let result = if self.wait_for_terminate {
            job.wait_for_termination().await?
        } else {
            job.wait().await?
        };
        notices.extend(jobs.take_signal_notice_of(job_id));
        if !job_has_unwaited_tasks(jobs, job_id) {
            if posix {
                jobs.remove_waited_job(job_id, u8::from(result.exit_code), false, false);
            } else {
                jobs.mark_waited(job_id);
            }
        }
        Ok(result)
    }

    /// `wait -n`: the oldest finished job not yet returned, or else the first of the
    /// running ones to finish.
    async fn wait_for_next<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
        notices: &mut Vec<String>,
    ) -> Result<ExecutionResult, cash_core::Error> {
        let forget = context.shell.options().posix_mode;
        let jobs = context.shell.jobs_mut();
        // See which jobs have finished; the one returned leaves the table, and the others
        // keep their numbers.
        jobs.refresh_statuses()?;

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
                    drop(futures);
                    // Nothing to wait for: the variable `-p` names is unset, as in Bash.
                    if let Some(name) = &self.variable_to_receive_id {
                        context.shell.env_mut().unset(name)?;
                    }
                    return Ok(ExecutionResult::new(127));
                }
                let ((result, id, pid), _, _) = select_all(futures).await;
                let result = result?;
                notices.extend(jobs.take_signal_notice_of(id));
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

/// Whether job `id` still has tasks a wait has not consumed.
fn job_has_unwaited_tasks(jobs: &cash_core::jobs::JobManager, id: usize) -> bool {
    jobs.jobs
        .iter()
        .any(|job| job.id == id && job.has_unwaited_tasks())
}
