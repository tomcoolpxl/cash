use cash_parser::ast::{self, CommandPrefixOrSuffixItem};
use itertools::Itertools;
use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::arithmetic::{self, ExpandAndEvaluate};
use crate::commands::{self, CommandArg};
use crate::env::{EnvironmentLookup, EnvironmentScope, valid_variable_name};
use crate::openfiles::{OpenFile, OpenFiles};
use crate::results::{
    ExecutionExitCode, ExecutionResult, ExecutionSpawnResult, ExecutionWaitResult, Interrupt,
};
use crate::shell::Shell;
use crate::variables::{
    ArrayLiteral, ShellValue, ShellValueLiteral, ShellValueUnsetType, ShellVariable,
};
use crate::{
    ShellFd, error, expansion, extendedtests, extensions, ioutils, jobs, openfiles, sys, timing,
};

/// Encapsulates the context of execution in a command pipeline.
struct PipelineExecutionContext<'a, SE: extensions::ShellExtensions> {
    /// The shell in which the command should be executed.
    shell: commands::ShellForCommand<'a, SE>,
    /// Process group ID for spawned processes.
    process_group_id: Option<i32>,
}

/// Parameters for execution.
#[derive(Clone, Default)]
pub struct ExecutionParameters {
    /// The open files tracked by the current context.
    open_files: openfiles::OpenFiles,
    /// Policy for how to manage spawned external processes.
    pub process_group_policy: ProcessGroupPolicy,
    /// Whether `errexit` (exit on error) behavior should be
    /// suppressed in this execution context. Defaults to `false`.
    pub suppress_errexit: bool,

    /// Where to report the process IDs of externals spawned under these parameters.
    ///
    /// cash (D11/D22): a background job runs as a tokio task executing a whole and-or
    /// list, so the child it spawns is invisible to the job that owns it — which left
    /// `$!` empty and `kill %1` unable to find anything to signal. The job installs a
    /// sink here before spawning the task, and the spawn site fills it in.
    ///
    /// `None` outside a background job, where the caller already holds the child.
    pub(crate) spawned_pid_sink: Option<std::sync::Arc<std::sync::Mutex<Vec<i32>>>>,

    /// Signalled once the background task knows whether it has a pid to report.
    ///
    /// cash: `cmd & pid=$!` is the standard idiom, and it was a race. The task spawns
    /// asynchronously, so `&` returned before the child existed and `$!` expanded to the
    /// empty string — reliably, not occasionally. `&` now waits for this.
    ///
    /// Fired at two points, so that waiting can never hang: when a pid is published, and
    /// when a builtin runs — a builtin is the shell's own code and will never produce a
    /// pid, so there is nothing further to wait for.
    pub(crate) spawned_pid_ready: Option<std::sync::Arc<tokio::sync::Notify>>,

    /// Whether this runs in a background job of a shell with job control.
    ///
    /// cash (D13): on Windows, the externals such a job spawns start in a process group
    /// of their own. The keyboard's Ctrl-C then no longer reaches them, as a background
    /// job's never does on Unix, and `kill -TERM %1` can send them a Ctrl-Break.
    pub(crate) background: bool,

    /// Descriptors above 2 that the current simple command's own redirections set.
    ///
    /// cash (D26): a native exe cannot see fd 3 and up, so only these make its spawn
    /// fail. Descriptors the shell merely holds — from `exec 3>log`, or from a
    /// redirection on an enclosing compound command or function call — are not
    /// passed and do not stop the command. Each simple command starts with this
    /// cleared, so a compound command's entries never reach the commands inside it.
    pub(crate) command_redirected_fds: Vec<ShellFd>,

    /// Whether these are the parameters of commands a program is running: a script, a
    /// `-c` string, a line typed at the prompt.
    ///
    /// A program started under them (a sourced file, `eval`, a trap, a command
    /// substitution) is a part of that one, and an interrupt is not its to act on: it
    /// passes the interrupt on, and the outermost program acts on it.
    pub(crate) within_program: bool,

    /// Whether this runs in a command started with `&`.
    ///
    /// cash (D13): such a command is not the keyboard's to interrupt. Bash has it ignore
    /// SIGINT when job control is off, and keeps it out of the terminal's foreground
    /// group when it is on. Here it is a task of the shell's own process, so it leaves a
    /// pending Ctrl-C for the foreground to act on.
    pub(crate) asynchronous: bool,
}

impl ExecutionParameters {
    /// Returns the standard input file; usable with `write!` et al.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    pub fn stdin(
        &self,
        shell: &Shell<impl extensions::ShellExtensions>,
    ) -> impl std::io::Read + 'static {
        self.try_stdin(shell).unwrap_or_else(|| {
            ioutils::FailingReaderWriter::new("standard input not available").into()
        })
    }

    /// Tries to retrieve the standard input file. Returns `None` if not set.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    pub fn try_stdin(&self, shell: &Shell<impl extensions::ShellExtensions>) -> Option<OpenFile> {
        self.try_fd(shell, openfiles::OpenFiles::STDIN_FD)
    }

    /// Returns the standard output file; usable with `write!` et al. In the event that
    /// no such file is available, returns a valid implementation of `std::io::Write`
    /// that fails all I/O requests.
    ///
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    pub fn stdout(
        &self,
        shell: &Shell<impl extensions::ShellExtensions>,
    ) -> impl std::io::Write + 'static {
        self.try_stdout(shell).unwrap_or_else(|| {
            ioutils::FailingReaderWriter::new("standard output not available").into()
        })
    }

    /// Tries to retrieve the standard output file. Returns `None` if not set.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    pub fn try_stdout(&self, shell: &Shell<impl extensions::ShellExtensions>) -> Option<OpenFile> {
        self.try_fd(shell, openfiles::OpenFiles::STDOUT_FD)
    }

    /// Returns the standard error file; usable with `write!` et al. In the event that
    /// no such file is available, returns a valid implementation of `std::io::Write`
    /// that fails all I/O requests.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    pub fn stderr(
        &self,
        shell: &Shell<impl extensions::ShellExtensions>,
    ) -> impl std::io::Write + 'static {
        self.try_stderr(shell).unwrap_or_else(|| {
            ioutils::FailingReaderWriter::new("standard error not available").into()
        })
    }

    /// Tries to retrieve the standard error file. Returns `None` if not set.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    pub fn try_stderr(&self, shell: &Shell<impl extensions::ShellExtensions>) -> Option<OpenFile> {
        self.try_fd(shell, openfiles::OpenFiles::STDERR_FD)
    }

    /// Returns the file descriptor with the given number. Returns `None`
    /// if the file descriptor is not open.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    /// * `fd` - The file descriptor number to retrieve.
    pub fn try_fd(
        &self,
        shell: &Shell<impl extensions::ShellExtensions>,
        fd: ShellFd,
    ) -> Option<openfiles::OpenFile> {
        match self.open_files.fd_entry(fd) {
            openfiles::OpenFileEntry::Open(f) => Some(f.clone()),
            openfiles::OpenFileEntry::NotPresent => None,
            openfiles::OpenFileEntry::NotSpecified => {
                // We didn't have this fd specified one way or the other; we fallback
                // to what's represented in the shell's open files.
                shell.persistent_open_files().try_fd(fd).cloned()
            }
        }
    }

    /// Sets the given file descriptor to the provided open file.
    ///
    /// # Arguments
    ///
    /// * `fd` - The file descriptor number to set.
    /// * `file` - The open file to set.
    pub fn set_fd(&mut self, fd: ShellFd, file: openfiles::OpenFile) {
        self.open_files.set_fd(fd, file);
    }

    /// Sets a descriptor on behalf of a redirection, noting it when it is above 2.
    fn set_redirected_fd(&mut self, fd: ShellFd, file: openfiles::OpenFile) {
        self.open_files.set_fd(fd, file);
        if fd > OpenFiles::STDERR_FD && !self.command_redirected_fds.contains(&fd) {
            self.command_redirected_fds.push(fd);
        }
    }

    /// Iterates over all open file descriptors in this context.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    pub fn iter_fds(
        &self,
        shell: &Shell<impl extensions::ShellExtensions>,
    ) -> impl Iterator<Item = (ShellFd, openfiles::OpenFile)> {
        let our_fds = self.open_files.iter_fds();
        let shell_fds = shell
            .persistent_open_files()
            .iter_fds()
            .filter(|(fd, _)| !self.open_files.contains_fd(*fd));

        #[allow(clippy::needless_collect)]
        let all_fds: Vec<_> = our_fds
            .chain(shell_fds)
            .map(|(fd, file)| (fd, file.clone()))
            .collect();

        all_fds.into_iter()
    }
}

#[derive(Clone, Debug, Default)]
/// Policy for how to manage spawned external processes.
pub enum ProcessGroupPolicy {
    /// Place the process in a new process group.
    #[default]
    NewProcessGroup,
    /// Place the process in the same process group as its parent.
    SameProcessGroup,
}

#[async_trait::async_trait]
pub trait Execute {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error>;
}

#[async_trait::async_trait]
trait ExecuteInPipeline<SE: extensions::ShellExtensions> {
    async fn execute_in_pipeline(
        &self,
        context: PipelineExecutionContext<'_, SE>,
        params: ExecutionParameters,
    ) -> Result<ExecutionSpawnResult, error::Error>;
}

#[async_trait::async_trait]
impl Execute for ast::Program {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut result = ExecutionResult::success();

        // Whether another program is running this one, which then has the last word on an
        // interrupt.
        let part_of_another = params.within_program;
        let mut params = params.clone();
        params.within_program = true;
        let params = &params;

        // A Ctrl-C that arrived when the prompt's last command line had nothing left to
        // interrupt is not for the next one.
        if !part_of_another && shell.options().interactive {
            let _ = cash_win32::console::take_interrupt();
        }

        for command in &self.complete_commands {
            // Execute the command and handle any errors without immediately propagating them.
            // This allows interactive shells to continue executing subsequent commands even after
            // errors.
            match command.execute(shell, params).await {
                Ok(exec_result) => result = exec_result,
                Err(err) => {
                    // cash: an interrupt (Ctrl-C while `read` waits for the keyboard) ends
                    // everything that is running, as SIGINT reaches every process of a
                    // Unix shell's foreground job: the sourced file, the `eval`, the
                    // command substitution and whatever runs them. Each passes it on, and
                    // the outermost program ends there: a script or a `-c` string ends the
                    // shell with status 130, and at the prompt the line is abandoned.
                    let interrupted = err.is_silent_interrupt();
                    if interrupted && part_of_another {
                        return Err(err);
                    }

                    // Display the error and convert to an execution result.
                    if !interrupted {
                        let _ = shell.display_error(&mut params.stderr(shell), &err);
                    }
                    let discards_line = err.discards_line();
                    result = err.into_result(shell);
                    // An error that abandons its line abandons the rest of a `-c` string,
                    // which Bash runs as one unit; a script goes on at its next line. An
                    // interrupt leaves nothing of the program to go on with.
                    let in_command_string = shell
                        .call_stack()
                        .current_frame()
                        .is_some_and(|frame| frame.frame_type.is_command_string());
                    if interrupted || (discards_line && in_command_string) {
                        shell.set_last_exit_status(result.exit_code.into());
                        break;
                    }
                }
            }

            // Update status
            shell.set_last_exit_status(result.exit_code.into());

            // Check if we should stop executing subsequent commands
            if !result.is_normal_flow() {
                break;
            }
        }

        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::CompoundList {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut result = ExecutionResult::success();

        for ast::CompoundListItem(ao_list, sep) in &self.0 {
            let run_async = matches!(sep, ast::SeparatorOperator::Async);

            if run_async {
                if let Some(job) = spawn_async_ao_list_in_task(ao_list, shell, params).await {
                    let job_formatted = job.to_pid_style_string();

                    if shell.options().interactive && !shell.is_subshell() {
                        writeln!(params.stderr(shell), "{job_formatted}")?;
                    }

                    result = ExecutionResult::success();
                } else {
                    let _ = writeln!(
                        params.stderr(shell),
                        "cash: fork: retry: Resource temporarily unavailable"
                    );
                    result = ExecutionResult::general_error();
                }
            } else {
                result = ao_list.execute(shell, params).await?;

                // Update status
                shell.set_last_exit_status(result.exit_code.into());
            }

            if !result.is_normal_flow() {
                break;
            }
        }

        Ok(result)
    }
}

async fn spawn_async_ao_list_in_task<'a, SE: extensions::ShellExtensions>(
    ao_list: &ast::AndOrList,
    shell: &'a mut Shell<SE>,
    params: &ExecutionParameters,
) -> Option<&'a jobs::Job> {
    // Reap finished background jobs before numbering this one. At the prompt they are
    // reported first, as Bash reports them, so `[1]+  Done` is printed and not lost;
    // while a file is sourced they wait until it is done. A script, which never reaches
    // a prompt to report them, reaps them silently, or a loop of background jobs would
    // pile them up.
    if shell.may_report_jobs_now() {
        let _ = shell.check_for_completed_jobs(params).await;
    } else if !shell.options().interactive {
        let _ = shell.jobs_mut().poll();
        shell.run_pending_chld_traps(params).await;
    }

    // Guard against runaway background jobs / fork bombs (matching ulimit -u).
    let slot_guard = jobs::SubshellSlotGuard::try_acquire().ok()?;

    // Clone the inputs.
    let mut cloned_shell = shell.clone();
    let mut cloned_params = params.clone();
    let cloned_ao_list = ao_list.clone();

    // cash (D13): at the prompt, a background job is out of the keyboard's reach. Scripts
    // keep sharing the console's group, as they always have.
    cloned_params.background = shell.options().enable_job_control;
    cloned_params.asynchronous = true;

    // Mark the child shell as not interactive; we don't want it messing with the terminal too much.
    cloned_shell.options_mut().interactive = false;

    // Redirect stdin to null, per spec.
    if let Ok(null) = openfiles::null() {
        cloned_params.set_fd(openfiles::OpenFiles::STDIN_FD, null);
    }

    // cash (D11/D22): give the task somewhere to report the pid it spawns, so the job
    // can answer `$!` and `kill %1`.
    let pid_sink = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    cloned_params.spawned_pid_sink = Some(std::sync::Arc::clone(&pid_sink));

    // cash: `cmd & pid=$!` must not be a race. The task publishes its pid — or reports
    // that it will never have one — through this, and `&` does not return until then.
    let pid_ready = std::sync::Arc::new(tokio::sync::Notify::new());
    cloned_params.spawned_pid_ready = Some(std::sync::Arc::clone(&pid_ready));

    let mut join_handle = tokio::spawn(async move {
        let _guard = slot_guard;
        cloned_ao_list
            .execute(&mut cloned_shell, &cloned_params)
            .await
    });

    // Either the task reached a point where `$!` is as accurate as it will ever be, or
    // it finished outright. Both arms resolve promptly: a task that runs forever either
    // spawned a process (first arm) or ran a builtin on the way (also first arm).
    let (completed_result, join_handle_opt) = tokio::select! {
        () = pid_ready.notified() => (None, Some(join_handle)),
        res = &mut join_handle => (Some(res), None),
    };

    let task = match completed_result {
        Some(Ok(res)) => jobs::JobTask::Completed(Some(res)),
        Some(Err(_join_err)) => {
            jobs::JobTask::Completed(Some(Ok(ExecutionResult::general_error())))
        }
        None => {
            if let Some(h) = join_handle_opt {
                jobs::JobTask::Internal(h)
            } else {
                jobs::JobTask::Completed(Some(Ok(ExecutionResult::general_error())))
            }
        }
    };

    Some(
        shell.jobs_mut().add_as_current(
            jobs::Job::new([task], ao_list.to_string(), jobs::JobState::Running)
                .with_spawned_pids(pid_sink),
        ),
    )
}

#[async_trait::async_trait]
impl Execute for ast::AndOrList {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let has_operators = !self.additional.is_empty();

        // For the first command, suppress errexit if there are more commands after it
        let mut first_params = params.clone();
        if has_operators {
            first_params.suppress_errexit = true;
        }

        let mut result = self.first.execute(shell, &first_params).await?;

        for (index, next_ao) in self.additional.iter().enumerate() {
            // Check for non-normal control flow.
            if !result.is_normal_flow() {
                break;
            }

            let (is_and, pipeline) = match next_ao {
                ast::AndOr::And(p) => (true, p),
                ast::AndOr::Or(p) => (false, p),
            };

            // If we short-circuit, then we don't break out of the whole loop
            // but we skip evaluating the current pipeline. We'll then continue
            // on and possibly evaluate a subsequent one (depending on the
            // operator before it).
            if is_and {
                if !result.is_success() {
                    continue;
                }
            } else if result.is_success() {
                continue;
            }

            // For the last command in the chain, use original params (errexit not suppressed)
            // For earlier commands, suppress errexit
            let mut params = params.clone();

            let is_last = index == self.additional.len() - 1;
            if !is_last {
                params.suppress_errexit = true;
            }

            result = pipeline.execute(shell, &params).await?;
        }

        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::Pipeline {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // Capture current timing if so requested.
        let stopwatch = self
            .timed
            .is_some()
            .then(timing::start_timing)
            .transpose()?;

        let mut params = params.clone();

        // cash (D13): a Ctrl-C that arrived while the shell ran commands of its own, a
        // loop of builtins say, is acted on here, before the next command: the trap on
        // INT runs and the shell goes on, or the script ends and the prompt's command
        // line is abandoned. Windows used to end the shell where it stood.
        if !params.asynchronous && cash_win32::console::take_interrupt() {
            let trap_result = act_on_console_interrupt(shell, &params).await?;
            if !trap_result.is_normal_flow() {
                return Ok(trap_result);
            }
        }

        // If this pipeline is negated, suppress errexit for commands within it
        if self.bang {
            params.suppress_errexit = true;
        }

        // Like Bash (`was_error_trap` in execute_cmd.c), whether an ERR trap applies is
        // decided before the pipeline runs: a trap it installs itself, for instance inside
        // a function it calls, does not fire for its own status.
        let had_err_trap = shell.traps().handles(crate::traps::TrapSignal::Err);

        // Spawn all the processes required for the pipeline, connecting outputs/inputs with pipes
        // as needed.
        let spawn_results = spawn_pipeline_processes(self, shell, &params).await?;

        // Wait for the processes. This also has a side effect of updating pipeline status.
        let mut result =
            wait_for_pipeline_processes_and_update_status(self, spawn_results, shell, &params)
                .await?;

        // Invert the exit code if requested.
        if self.bang {
            result.exit_code = ExecutionExitCode::from(if result.is_success() { 1 } else { 0 });
        }

        // Update exit status.
        shell.set_last_exit_status(result.exit_code.into());

        // Fire the ERR trap if the pipeline failed in a non-conditional context.
        // We reuse `suppress_errexit` here because bash suppresses the ERR trap in
        // exactly the same contexts it suppresses errexit (conditionals, `!`-prefixed
        // pipelines, etc.).
        // A `return` leaves before Bash reaches its ERR check, so it never fires ERR.
        if !result.is_success()
            && !params.suppress_errexit
            && !self.bang
            && had_err_trap
            && !matches!(
                result.next_control_flow,
                crate::ExecutionControlFlow::ReturnFromFunctionOrScript
            )
        {
            if shell.traps().handles(crate::traps::TrapSignal::Err) {
                let trap_result = shell
                    .invoke_trap_handler(crate::traps::TrapSignal::Err, &params)
                    .await?;
                // `exit` or `return` in the handler ends the script or function, as in Bash.
                if matches!(
                    trap_result.next_control_flow,
                    crate::ExecutionControlFlow::ExitShell
                        | crate::ExecutionControlFlow::ReturnFromFunctionOrScript
                ) {
                    result = trap_result;
                }
            }
        }

        // Apply errexit if not suppressed (and not negated)
        if !params.suppress_errexit && !self.bang {
            shell.apply_errexit_if_enabled(&mut result);
        }

        // If requested, report timing.
        if let (Some(timed), Some(stopwatch)) = (&self.timed, &stopwatch)
            && let Some(mut stderr) = params.try_fd(shell, openfiles::OpenFiles::STDERR_FD)
        {
            let timing = stopwatch.stop()?;
            // `time -p` has a fixed format; otherwise $TIMEFORMAT applies, and a set but
            // empty value suppresses the report.
            let format = if timed.is_posix_output() {
                Some(timing::POSIX_TIMEFORMAT.to_owned())
            } else {
                match shell.env_str("TIMEFORMAT") {
                    Some(format) => Some(format.into_owned()),
                    None => Some(timing::DEFAULT_TIMEFORMAT.to_owned()),
                }
            };
            if let Some(format) = format.filter(|f| !f.is_empty()) {
                match timing::format_timing(&format, &timing) {
                    Ok(report) => std::write!(stderr, "{report}")?,
                    Err(bad) => {
                        std::writeln!(stderr, "TIMEFORMAT: `{bad}': invalid format character")?;
                    }
                }
            }
        }

        Ok(result)
    }
}

async fn spawn_pipeline_processes(
    pipeline: &ast::Pipeline,
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
) -> Result<VecDeque<ExecutionSpawnResult>, error::Error> {
    let pipeline_len = pipeline.seq.len();
    let mut pipe_readers = vec![];
    let mut pipe_writers = vec![];
    let mut spawn_results = VecDeque::new();
    let mut process_group_id: Option<i32> = None;

    // Create pipes to use between commands, but only bother doing so if there's more than one
    // command.
    if pipeline_len > 1 {
        pipe_readers.reserve_exact(pipeline_len - 1);
        pipe_writers.reserve_exact(pipeline_len - 1);

        for _ in 0..(pipeline_len - 1) {
            let (reader, writer) = std::io::pipe()?;
            pipe_readers.push(Some(reader.into()));
            pipe_writers.push(Some(writer.into()));
        }
        // Push `None` to the readers; it will be popped off by the *first* command, which will
        // mean that command gets its stdin from the execution parameters' current stdin.
        pipe_readers.push(None);
    }

    for (current_pipeline_index, command) in pipeline.seq.iter().enumerate() {
        //
        // We run a command directly in the current shell if either of the following is true:
        //     * There's only one command in the pipeline.
        //     * This is the *last* command in the pipeline, the lastpipe option is enabled, and job
        //       monitoring is disabled.
        // Otherwise, we spawn a separate subshell for each command in the pipeline.
        //

        let run_in_current_shell = pipeline_len == 1
            || (current_pipeline_index == pipeline_len - 1
                && shell.options().run_last_pipeline_cmd_in_current_shell
                && !shell.options().enable_job_control);

        // Set up parameters appropriate for this command.
        let mut cmd_params = params.clone();

        // Install pipes.
        if let Some(Some(reader)) = pipe_readers.pop() {
            cmd_params.open_files.set_fd(OpenFiles::STDIN_FD, reader);
        }
        if let Some(Some(writer)) = pipe_writers.pop() {
            cmd_params.open_files.set_fd(OpenFiles::STDOUT_FD, writer);
        }

        let pipeline_context = if !run_in_current_shell {
            // Make sure that all commands in the pipeline are in the same process group.
            if current_pipeline_index > 0 {
                cmd_params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;
            }

            PipelineExecutionContext {
                shell: commands::ShellForCommand::OwnedShell {
                    target: Box::new(shell.clone()),
                    parent: shell,
                },
                process_group_id,
            }
        } else {
            PipelineExecutionContext {
                shell: commands::ShellForCommand::ParentShell(shell),
                process_group_id,
            }
        };

        let spawn_result = command
            .execute_in_pipeline(pipeline_context, cmd_params)
            .await?;

        // Update the process group ID if something was spawned.
        if let ExecutionSpawnResult::StartedProcess(child) = &spawn_result {
            if process_group_id.is_none() {
                process_group_id = child.pgid();
            }
        }

        spawn_results.push_back(spawn_result);
    }

    Ok(spawn_results)
}

async fn wait_for_pipeline_processes_and_update_status(
    pipeline: &ast::Pipeline,
    mut process_spawn_results: VecDeque<ExecutionSpawnResult>,
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
) -> Result<ExecutionResult, error::Error> {
    let mut result = ExecutionResult::success();
    let mut stopped_children = vec![];
    let mut last_failure_exit_code: Option<ExecutionExitCode> = None;

    // Clear our the pipeline status so we can start filling it out.
    shell.last_pipeline_statuses_mut().clear();

    // The processes of its own the shell reaps here: after them, an interactive shell
    // reports the background jobs that finished meanwhile, and a CHLD trap runs once for
    // each, as in Bash.
    let mut reaped_processes = 0;

    // cash (D19): the keyboard's Ctrl-Z stops the interactive shell's foreground job, which
    // is then filed as a job for `fg` and `bg`. Not while a stage runs inside the shell: a
    // stopped pipeline still waits for such a stage, and one reading from a suspended
    // program would wait for ever.
    let ctrl_z = shell.ctrl_z_stops_foreground_jobs()
        && !process_spawn_results
            .iter()
            .any(|result| matches!(result, ExecutionSpawnResult::StartedTask(_)));

    // What the keyboard's Ctrl-C did to the pipeline's programs while the shell waited.
    let mut interrupt = Interrupt::None;

    while let Some(child) = process_spawn_results.pop_front() {
        let is_process = matches!(child, ExecutionSpawnResult::StartedProcess(_));
        let wait_result = if !stopped_children.is_empty() {
            child.poll().await?
        } else {
            let (wait_result, its_interrupt) = child.wait_or_stop(ctrl_z).await?;
            interrupt = interrupt.or(its_interrupt);
            wait_result
        };

        match wait_result {
            ExecutionWaitResult::Completed(current_result) => {
                if is_process {
                    reaped_processes += 1;
                }
                result = current_result;
                shell.set_last_exit_status(result.exit_code.into());
                shell
                    .last_pipeline_statuses_mut()
                    .push(result.exit_code.into());

                // Track the last failure for pipefail option
                if !result.is_success() {
                    last_failure_exit_code = Some(result.exit_code);
                }
            }
            ExecutionWaitResult::Stopped(child) => {
                result = ExecutionResult::stopped();
                shell.set_last_exit_status(result.exit_code.into());
                shell
                    .last_pipeline_statuses_mut()
                    .push(result.exit_code.into());

                // It becomes a job's process, whose pid `jobs -l` shows: held open, like
                // a background job's, so the pid stays its own after it ends.
                if let Some(pid) = child.pid().and_then(|pid| u32::try_from(pid).ok()) {
                    cash_win32::children::hold(pid);
                }
                stopped_children.push(jobs::JobTask::External(child));
            }
        }
    }

    // Apply pipefail semantics if enabled
    if shell.options().return_last_failure_from_pipeline {
        if let Some(failure_exit_code) = last_failure_exit_code {
            result.exit_code = failure_exit_code;
        }
    }

    if shell.options().interactive {
        sys::terminal::move_self_to_foreground()?;
    }

    // Bash reports a finished background job when a foreground job completes, not only
    // at the next prompt: `sleep 1 & sleep 2; echo after` shows `[1]+  Done` before
    // `after`. Not while a file is sourced (Bash 5.3): that waits until it returns.
    if reaped_processes > 0 {
        shell.jobs_mut().note_children_reaped(reaped_processes);
        shell.after_foreground_children(params).await?;
    }

    // If there were stopped jobs, then encapsulate the pipeline as a managed job and hand it
    // off to the job manager.
    if !stopped_children.is_empty() {
        let job = shell.jobs_mut().add_as_current(jobs::Job::new(
            stopped_children,
            pipeline.to_string(),
            jobs::JobState::Stopped,
        ));

        let formatted = job.to_string();

        // N.B. We use the '\r' to overwrite any ^Z output.
        writeln!(params.stderr(shell), "\r{formatted}")?;
    }

    // cash (D13): what that Ctrl-C means for the shell, now that the pipeline has ended.
    // As in Bash, a trap on INT runs once the foreground command is done, whatever the
    // command made of the interrupt. Without a trap the shell is interrupted only if a
    // program of the pipeline died of it: a script then ends, and the command line is
    // abandoned at the prompt. A program that took the interrupt in its stride leaves the
    // shell running. A command started with `&` is not the keyboard's to interrupt.
    let int = crate::traps::TrapSignal::Signal(sys::signal::Signal::Int);
    let acts = match interrupt {
        Interrupt::Fatal => true,
        Interrupt::Survived => shell.traps().handles(int),
        Interrupt::None => false,
    };
    if acts && !params.asynchronous {
        let trap_result = act_on_console_interrupt(shell, params).await?;
        if !trap_result.is_normal_flow() {
            return Ok(trap_result);
        }
    }

    Ok(result)
}

/// Acts on a Ctrl-C that reached the shell as the console's event: see
/// [`Shell::interrupt`].
///
/// Nothing has shown that one, where a `read` shows the key it takes. At the prompt Bash
/// starts a new line before it abandons the command line, and so does this.
async fn act_on_console_interrupt(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
) -> Result<ExecutionResult, error::Error> {
    let int = crate::traps::TrapSignal::Signal(sys::signal::Signal::Int);
    if shell.options().interactive && !shell.traps().handles(int) {
        let _ = writeln!(params.stderr(shell));
    }
    shell.interrupt(params).await
}

#[async_trait::async_trait]
impl<SE: extensions::ShellExtensions> ExecuteInPipeline<SE> for ast::Command {
    async fn execute_in_pipeline(
        &self,
        mut pipeline_context: PipelineExecutionContext<'_, SE>,
        mut params: ExecutionParameters,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        if pipeline_context.shell.options().do_not_execute_commands {
            return Ok(ExecutionSpawnResult::Completed(ExecutionResult::success()));
        }

        // Updates the shell with information about the currently executing command.
        pipeline_context.shell.set_current_cmd(self);

        match self {
            Self::Simple(simple) => simple.execute_in_pipeline(pipeline_context, params).await,
            Self::Compound(compound, redirects) => {
                // Set up any additional redirects.
                if let Some(redirects) = redirects {
                    for redirect in &redirects.0 {
                        setup_redirect(&mut pipeline_context.shell, &mut params, redirect).await?;
                    }
                }

                spawn_or_run_in_pipeline(pipeline_context, params, compound.clone()).await
            }
            Self::Function(func) => Ok(func
                .execute(&mut pipeline_context.shell, &params)
                .await?
                .into()),
        }
    }
}

/// Run a compound command as a pipeline member.
///
/// cash: this used to `await` the command inline, which meant a pipeline member ran to
/// *completion* before the next member was even created. Nothing was draining the pipe,
/// so a compound command producing more than the pipe buffer — 4096 bytes on Windows —
/// silently lost everything:
///
/// ```text
/// { seq 1 5000; } | wc -l     ->  0      (bash: 5000)
/// for f in *; do echo "$f"; done | tee log
/// ```
///
/// Total, silent data loss in an everyday construct, which is the failure class this
/// project rejects everywhere else. Below 4 KiB it happened to work, so it looked fine.
///
/// A member with its own shell is now spawned as a task, exactly as a builtin pipeline
/// member already was, so the reader runs while the writer writes. The last member of a
/// pipeline under `lastpipe` keeps running inline: it owns the parent shell, its output
/// is not going into a pipe anyone has to drain, and running it elsewhere would lose the
/// variable assignments `lastpipe` exists to keep.
async fn spawn_or_run_in_pipeline<SE: extensions::ShellExtensions>(
    pipeline_context: PipelineExecutionContext<'_, SE>,
    params: ExecutionParameters,
    compound: ast::CompoundCommand,
) -> Result<ExecutionSpawnResult, error::Error> {
    match pipeline_context.shell {
        commands::ShellForCommand::OwnedShell { target, .. } => {
            let Ok(slot_guard) = jobs::SubshellSlotGuard::try_acquire() else {
                use std::io::Write as _;
                let _ = writeln!(
                    params.stderr(&target),
                    "cash: fork: retry: Resource temporarily unavailable"
                );
                return Ok(ExecutionSpawnResult::Completed(
                    ExecutionResult::general_error(),
                ));
            };
            let mut shell = *target;
            let join_handle = tokio::task::spawn_blocking(move || {
                let _guard = slot_guard;
                let rt = tokio::runtime::Handle::current();
                rt.block_on(compound.execute(&mut shell, &params))
            });
            Ok(ExecutionSpawnResult::StartedTask(join_handle))
        }
        commands::ShellForCommand::ParentShell(shell) => {
            Ok(compound.execute(shell, &params).await?.into())
        }
    }
}

enum WhileOrUntil {
    While,
    Until,
}

#[async_trait::async_trait]
impl Execute for ast::CompoundCommand {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        match self {
            Self::BraceGroup(ast::BraceGroupCommand { list, .. }) => {
                list.execute(shell, params).await
            }
            Self::Subshell(ast::SubshellCommand { list, .. }) => {
                // Clone off a new subshell, and run the body of the subshell there.
                // TODO(source-info): Do we need to reset the line number?
                let mut subshell = shell.clone();

                // Handle errors within the subshell context to prevent fatal errors
                // from propagating to the parent shell.
                let subshell_result = match list.execute(&mut subshell, params).await {
                    Ok(result) => result,
                    // An interrupt ends the shell the subshell is a part of, too.
                    Err(error) if error.is_silent_interrupt() => return Err(error),
                    Err(error) => {
                        // Display the error to stderr, but prevent fatal error propagation
                        let mut stderr = params.stderr(shell);
                        let _ = shell.display_error(&mut stderr, &error);

                        // Convert error to result in subshell context
                        error.into_result(&subshell)
                    }
                };

                // Preserve the subshell's exit code, but don't honor any of its requests to exit
                // the shell, break out of loops, etc.
                Ok(ExecutionResult::from(subshell_result.exit_code))
            }
            Self::ForClause(f) => f.execute(shell, params).await,
            Self::SelectClause(sel) => sel.execute(shell, params).await,
            Self::CaseClause(c) => c.execute(shell, params).await,
            Self::IfClause(i) => i.execute(shell, params).await,
            Self::WhileClause(w) => (WhileOrUntil::While, w).execute(shell, params).await,
            Self::UntilClause(u) => (WhileOrUntil::Until, u).execute(shell, params).await,
            Self::Arithmetic(a) => a.execute(shell, params).await,
            Self::ArithmeticForClause(a) => a.execute(shell, params).await,
            Self::Coprocess(c) => c.execute(shell, params).await,
            Self::ExtendedTest(e) => {
                let result =
                    match extendedtests::eval_extended_test_expr(&e.expr, shell, params).await {
                        Ok(true) => 0,
                        Ok(false) => 1,
                        // An `=~` pattern that does not compile is reported, and the test
                        // yields 2 without ending the script.
                        Err(err) => match err.kind() {
                            error::ErrorKind::InvalidRegexError(regex_error, _) => {
                                let _ = writeln!(
                                    params.stderr(shell),
                                    "[[: invalid regular expression: {regex_error}"
                                );
                                2
                            }
                            _ => return Err(err),
                        },
                    };
                Ok(ExecutionResult::new(result))
            }
        }
    }
}

#[async_trait::async_trait]
impl Execute for ast::CoprocessCommand {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        if shell.options().do_not_execute_commands {
            return Ok(ExecutionResult::success());
        }

        // Resolve the name of the variable that will receive the coprocess's file descriptors.
        let name = self
            .name
            .as_ref()
            .map_or(Cow::Borrowed("COPROC"), |w| Cow::Owned(w.to_string()));

        if !valid_variable_name(&name) {
            writeln!(
                params.stderr(shell),
                "coproc {name}: not a valid identifier"
            )?;
            return Ok(ExecutionExitCode::GeneralError.into());
        }

        // Set up the pipes that we'll use to communicate with the coprocess.
        let (stdin_reader, stdin_writer) = std::io::pipe()?;
        let (stdout_reader, stdout_writer) = std::io::pipe()?;

        // Allocate new fds in the (parent) shell for the read end of the coprocess's stdout
        // and the write end of the coprocess's stdin.
        let stdout_fd = shell.open_files_mut().add(stdout_reader.into())?;
        let stdin_fd = shell.open_files_mut().add(stdin_writer.into())?;

        // Crete a subshell that the coprocess will own and run in.
        let mut child_shell = shell.clone();
        child_shell.options_mut().interactive = false;

        // Setup redirection for the coprocess's shell's stdin/stdout.
        let mut child_params = params.clone();
        child_params
            .open_files
            .set_fd(OpenFiles::STDIN_FD, stdin_reader.into());
        child_params
            .open_files
            .set_fd(OpenFiles::STDOUT_FD, stdout_writer.into());

        let body = self.body.clone();
        let join_handle = tokio::spawn(async move {
            let pipeline_context = PipelineExecutionContext {
                shell: commands::ShellForCommand::ParentShell(&mut child_shell),
                process_group_id: None,
            };
            let spawn_result = body
                .execute_in_pipeline(pipeline_context, child_params)
                .await?;
            match spawn_result.wait().await? {
                ExecutionWaitResult::Completed(result) => Ok(result),
                ExecutionWaitResult::Stopped(_) => Ok(ExecutionResult::stopped()),
            }
        });

        let job = shell.jobs_mut().add_as_current(jobs::Job::new(
            [jobs::JobTask::Internal(join_handle)],
            format!("coproc {name}"),
            jobs::JobState::Running,
        ));
        let job_id = job.id;

        // Fill out the fd variable.
        let arr_value = ShellValue::from(vec![stdout_fd.to_string(), stdin_fd.to_string()]);
        shell
            .env_mut()
            .set_global(name.clone(), ShellVariable::new(arr_value))?;

        // Set the job ID for the coprocess in a separate variable with the _PID suffix.
        let pid_name = format!("{name}_PID");
        shell
            .env_mut()
            .set_global(pid_name, ShellVariable::new(job_id.to_string()))?;

        Ok(ExecutionResult::success())
    }
}

#[async_trait::async_trait]
impl Execute for ast::SelectClauseCommand {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        use std::io::{BufRead, Write};

        let mut result = ExecutionResult::success();

        // Same word source as a for clause: the given list, or the positional parameters.
        let choices = if let Some(unexpanded_values) = &self.values {
            expand_words(shell, params, unexpanded_values).await?
        } else {
            shell.current_shell_args().to_vec()
        };

        // bash exits immediately on an empty list rather than prompting forever.
        if choices.is_empty() {
            shell.set_last_exit_status(result.exit_code.into());
            return Ok(result);
        }

        // A byte at a time: an answer is one line, and the lines after it are the input
        // of the body's commands and of whatever follows the loop.
        let mut input = std::io::BufReader::with_capacity(1, params.stdin(shell));
        let mut show_menu = true;

        loop {
            if show_menu {
                let mut stderr = params.stderr(shell);
                for (index, choice) in choices.iter().enumerate() {
                    writeln!(stderr, "{}) {choice}", index + 1)?;
                }
                let _ = stderr.flush();
                show_menu = false;
            }

            // PS3 is the select prompt, and it goes to standard error alongside the menu.
            let prompt = shell
                .env()
                .get_str("PS3", shell)
                .map_or_else(|| String::from("#? "), |value| value.to_string());

            // cash (D13): at a console the answer is read a key at a time, as `read` reads
            // there, so that Ctrl-C is a key: left to the console's own collection of a
            // line, it did nothing before Enter or ended the shell. The keys are taken
            // before the prompt is shown, so that what is typed at the sight of it is the
            // answer's, and handed back before anything else runs.
            let mut prompted = false;
            let line = loop {
                let console = params
                    .try_stdin(shell)
                    .and_then(|stdin| stdin.console(true, true));
                if !std::mem::replace(&mut prompted, true) {
                    let mut stderr = params.stderr(shell);
                    write!(stderr, "{prompt}")?;
                    let _ = stderr.flush();
                }

                let Some(mut console) = console else {
                    let mut line = String::new();
                    break (input.read_line(&mut line)? != 0).then_some(line);
                };
                match console.line()? {
                    cash_win32::conin::Line::Typed(line) => break Some(line),
                    cash_win32::conin::Line::EndOfInput => break None,
                    cash_win32::conin::Line::Interrupted => {
                        // A trap on INT runs, and the wait for the answer goes on, as in
                        // Bash, without a second prompt.
                        drop(console);
                        let trap_result = shell.interrupt(params).await?;
                        if !trap_result.is_normal_flow() {
                            return Ok(trap_result);
                        }
                    }
                }
            };
            // End of input ends the loop, as bash's does: on a new line, and with status 1.
            let Some(line) = line else {
                let mut stderr = params.stderr(shell);
                writeln!(stderr)?;
                let _ = stderr.flush();
                result = ExecutionResult::general_error();
                break;
            };

            let answer = line.trim_end_matches(['\r', '\n']).to_string();

            // REPLY holds the raw line whatever it was, which is how a select body tells
            // "3" from "quit".
            shell.env_mut().update_or_add(
                "REPLY",
                ShellValueLiteral::Scalar(answer.clone()),
                |_| Ok(()),
                EnvironmentLookup::Anywhere,
                EnvironmentScope::Global,
            )?;

            // A blank line reprints the menu and asks again, without running the body.
            if answer.trim().is_empty() {
                show_menu = true;
                continue;
            }

            // A number in range selects; anything else sets the variable empty, which is
            // what a body testing `[ -z "$var" ]` relies on.
            let chosen = answer
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|n| *n >= 1 && *n <= choices.len())
                .map_or_else(String::new, |n| choices[n - 1].clone());

            shell.env_mut().update_or_add(
                &self.variable_name,
                ShellValueLiteral::Scalar(chosen),
                |_| Ok(()),
                EnvironmentLookup::Anywhere,
                EnvironmentScope::Global,
            )?;

            result = self.body.list.execute(shell, params).await?;
            if result.is_return_or_exit() {
                break;
            }

            let is_break = result.is_break();
            result.next_control_flow = result.next_control_flow.try_decrement_loop_levels();

            if is_break {
                break;
            }
        }

        shell.set_last_exit_status(result.exit_code.into());
        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::ForClauseCommand {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut result = ExecutionResult::success();

        // If we were given explicit words to iterate over, then expand them all, with splitting
        // enabled.
        let expanded_values = if let Some(unexpanded_values) = &self.values {
            expand_words(shell, params, unexpanded_values).await?
        } else {
            // Otherwise, we use the current positional parameters.
            shell.current_shell_args().to_vec()
        };

        for value in expanded_values {
            if shell.options().print_commands_and_arguments {
                if let Some(unexpanded_values) = &self.values {
                    shell
                        .trace_command(
                            params,
                            std::format!(
                                "for {} in {}",
                                self.variable_name,
                                unexpanded_values.iter().join(" ")
                            ),
                        )
                        .await;
                } else {
                    shell
                        .trace_command(params, std::format!("for {}", self.variable_name))
                        .await;
                }
            }

            // Update the variable.
            shell.env_mut().update_or_add(
                &self.variable_name,
                ShellValueLiteral::Scalar(value),
                |_| Ok(()),
                EnvironmentLookup::Anywhere,
                EnvironmentScope::Global,
            )?;

            result = self.body.list.execute(shell, params).await?;
            if result.is_return_or_exit() {
                break;
            }

            let is_break = result.is_break();

            result.next_control_flow = result.next_control_flow.try_decrement_loop_levels();

            if is_break || result.is_continue() {
                break;
            }
        }

        shell.set_last_exit_status(result.exit_code.into());
        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::CaseClauseCommand {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // N.B. One would think it makes sense to trace the expanded value being switched
        // on, but that's not it.
        if shell.options().print_commands_and_arguments {
            shell
                .trace_command(params, std::format!("case {} in", self.value))
                .await;
        }

        let expanded_value = expansion::basic_expand_word(shell, params, &self.value).await?;
        let mut result: ExecutionResult = ExecutionResult::success();
        let mut force_execute_next_case = false;

        for case in &self.cases {
            if force_execute_next_case {
                force_execute_next_case = false;
            } else {
                let mut matches = false;
                for pattern in &case.patterns {
                    let expanded_pattern = expansion::basic_expand_pattern(shell, params, pattern)
                        .await?
                        .set_extended_globbing(shell.options().extended_globbing)
                        .set_case_insensitive(shell.options().case_insensitive_conditionals);

                    if expanded_pattern.exactly_matches(expanded_value.as_str())? {
                        matches = true;
                        break;
                    }
                }

                if !matches {
                    continue;
                }
            }

            result = if let Some(case_cmd) = &case.cmd {
                case_cmd.execute(shell, params).await?
            } else {
                ExecutionResult::success()
            };

            // Check for early return (return/exit) or loop control flow (break/continue)
            if !result.is_normal_flow() {
                break;
            }

            match case.post_action {
                ast::CaseItemPostAction::ExitCase => break,
                ast::CaseItemPostAction::UnconditionallyExecuteNextCaseItem => {
                    force_execute_next_case = true;
                }
                ast::CaseItemPostAction::ContinueEvaluatingCases => (),
            }
        }

        shell.set_last_exit_status(result.exit_code.into());

        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::IfClauseCommand {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // Execute condition with errexit suppressed
        let mut condition_params = params.clone();
        condition_params.suppress_errexit = true;
        let condition = self.condition.execute(shell, &condition_params).await?;

        // Check if the condition itself resulted in non-normal control flow.
        if !condition.is_normal_flow() {
            return Ok(condition);
        }

        if condition.is_success() {
            return self.then.execute(shell, params).await;
        }

        if let Some(elses) = &self.elses {
            for else_clause in elses {
                match &else_clause.condition {
                    Some(else_condition) => {
                        let else_condition_result =
                            else_condition.execute(shell, &condition_params).await?;

                        // Check if the elif condition caused non-normal control flow.
                        if !else_condition_result.is_normal_flow() {
                            return Ok(else_condition_result);
                        }

                        if else_condition_result.is_success() {
                            return else_clause.body.execute(shell, params).await;
                        }
                    }
                    None => {
                        return else_clause.body.execute(shell, params).await;
                    }
                }
            }
        }

        // If we got down here, then no branch was taken; we make sure to
        // reset the last exit status to success and then return success.
        let result = ExecutionResult::success();
        shell.set_last_exit_status(result.exit_code.into());

        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for (WhileOrUntil, &ast::WhileOrUntilClauseCommand) {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let is_while = match self.0 {
            WhileOrUntil::While => true,
            WhileOrUntil::Until => false,
        };
        let test_condition = &self.1.0;
        let body = &self.1.1;

        let mut result = ExecutionResult::success();

        // Execute loop condition with errexit suppressed
        let mut condition_params = params.clone();
        condition_params.suppress_errexit = true;

        loop {
            let condition_result = test_condition.execute(shell, &condition_params).await?;

            // Update status for condition
            shell.set_last_exit_status(condition_result.exit_code.into());

            if !condition_result.is_normal_flow() {
                result = condition_result;

                // If the condition has break/continue, the while/until loop itself
                // consumes one level. We need to decrement the level before returning.
                result.next_control_flow = result.next_control_flow.try_decrement_loop_levels();
                break;
            }

            if condition_result.is_success() != is_while {
                break;
            }

            result = body.list.execute(shell, params).await?;
            if result.is_return_or_exit() {
                break;
            }

            let is_break = result.is_break();

            result.next_control_flow = result.next_control_flow.try_decrement_loop_levels();

            if is_break || result.is_continue() {
                break;
            }
        }

        shell.set_last_exit_status(result.exit_code.into());
        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::ArithmeticCommand {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // An arithmetic error fails `(( ))` with status 1 and the line goes on, as `let`
        // does; in an expansion (`$((1+))`) the same error abandons the line.
        let value = match self.expr.eval(shell, params, true).await {
            Ok(value) => value,
            Err(
                err @ (arithmetic::EvalError::FailedToExpandExpression(_)
                | arithmetic::EvalError::ExpandingUnsetVariable(_)),
            ) => return Err(err.into()),
            Err(err) => {
                let _ = shell.display_error(&mut params.stderr(shell), &err.into());
                let result = ExecutionResult::general_error();
                shell.set_last_exit_status(result.exit_code.into());
                return Ok(result);
            }
        };
        let result = if value != 0 {
            ExecutionResult::success()
        } else {
            ExecutionResult::general_error()
        };

        shell.set_last_exit_status(result.exit_code.into());

        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::ArithmeticForClauseCommand {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut result = ExecutionResult::success();
        if let Some(initializer) = &self.initializer {
            initializer.eval(shell, params, true).await?;
        }

        loop {
            if let Some(condition) = &self.condition {
                // An empty condition (e.g., `for (( ; ; ))`) means "always true".
                if !condition.value.is_empty() && condition.eval(shell, params, true).await? == 0 {
                    break;
                }
            }

            result = self.body.list.execute(shell, params).await?;
            if result.is_return_or_exit() {
                break;
            }

            let is_break = result.is_break();

            result.next_control_flow = result.next_control_flow.try_decrement_loop_levels();

            if is_break || result.is_continue() {
                break;
            }

            if let Some(updater) = &self.updater {
                updater.eval(shell, params, true).await?;
            }
        }

        shell.set_last_exit_status(result.exit_code.into());
        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::FunctionDefinition {
    async fn execute(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        _params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let func_name = self.fname.value.clone();

        // In POSIX mode, function names can't shadow special builtins.
        if shell.options().posix_mode
            && shell
                .builtins()
                .get(&func_name)
                .is_some_and(|r| r.special_builtin)
        {
            return Err(
                error::Error::from(error::ErrorKind::FunctionNameShadowsSpecialBuiltin {
                    name: func_name,
                })
                .into_fatal(),
            );
        }

        // The function definition's source context should be the same as the current frame
        // so we directly pass that through.
        let source_info = shell
            .call_stack()
            .current_frame()
            .map_or_else(crate::SourceInfo::default, |frame| {
                frame.adjusted_source_info()
            });
        shell.define_func(func_name, self.clone(), &source_info);

        let result = ExecutionResult::success();
        shell.set_last_exit_status(result.exit_code.into());

        Ok(result)
    }
}

#[async_trait::async_trait]
#[allow(clippy::too_many_lines)]
impl<SE: extensions::ShellExtensions> ExecuteInPipeline<SE> for ast::SimpleCommand {
    async fn execute_in_pipeline(
        &self,
        mut context: PipelineExecutionContext<'_, SE>,
        mut params: ExecutionParameters,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        // Bash exposes the command currently being expanded, including its original
        // quoting, through BASH_COMMAND. This must happen before argument expansion so
        // the command itself and a DEBUG trap both observe the current command.
        context.shell.env_mut().update_or_add(
            "BASH_COMMAND",
            ShellValueLiteral::Scalar(self.to_string()),
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;

        let prefix_iter = self.prefix.as_ref().map(|s| s.0.iter()).unwrap_or_default();
        let suffix_iter = self.suffix.as_ref().map(|s| s.0.iter()).unwrap_or_default();
        let cmd_name_items = self
            .word_or_name
            .as_ref()
            .map(|won| CommandPrefixOrSuffixItem::Word(won.clone()));

        let mut assignments = vec![];
        let mut args: Vec<CommandArg> = vec![];
        let mut command_takes_assignments = false;

        // Capture the status change count before expansion, so we can detect
        // if expansion (e.g., command substitution) set an exit status.
        let status_change_count_before_expansion = context.shell.last_exit_status_change_count();

        let requires_seekable_file = self.word_or_name.as_ref().map_or(false, |won| {
            let s = won.flatten();
            let base = std::path::Path::new(&s)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(&s);
            base.eq_ignore_ascii_case("diff") || base.eq_ignore_ascii_case("cmp")
        });

        // Only this command's own redirections count here (D26); anything an enclosing
        // compound command noted is not "on the command".
        params.command_redirected_fds.clear();

        for item in prefix_iter.chain(cmd_name_items.iter()).chain(suffix_iter) {
            match item {
                CommandPrefixOrSuffixItem::IoRedirect(redirect) => {
                    if let Err(e) = setup_redirect(&mut context.shell, &mut params, redirect).await
                    {
                        writeln!(params.stderr(&context.shell), "error: {e}")?;
                        return Ok(ExecutionResult::general_error().into());
                    }
                }
                CommandPrefixOrSuffixItem::ProcessSubstitution(kind, subshell_command) => {
                    let (arg_path, installed_fd_num, substitution_file) =
                        setup_process_substitution(
                            &context.shell,
                            &params,
                            kind,
                            subshell_command,
                            false,
                            requires_seekable_file,
                        )
                        .await?;

                    if let Some(fd) = installed_fd_num {
                        params.open_files.set_fd(fd, substitution_file);
                    }

                    args.push(CommandArg::String(arg_path));
                }
                CommandPrefixOrSuffixItem::AssignmentWord(assignment, word) => {
                    if args.is_empty() {
                        // If we haven't yet seen any arguments, then this must be a proper
                        // scoped assignment. Add it to the list we're accumulating.
                        assignments.push(assignment);
                    } else {
                        if command_takes_assignments {
                            // This looks like an assignment, and the command being invoked is a
                            // well-known builtin that takes arguments that need to function like
                            // assignments (but which are processed by the builtin).
                            let expanded =
                                expand_assignment(&mut context.shell, &params, assignment).await?;
                            args.push(CommandArg::Assignment(expanded));
                        } else {
                            // This *looks* like an assignment, but it's really a string we should
                            // fully treat as a regular looking
                            // argument.
                            let mut next_args = expansion::full_expand_and_split_word(
                                &mut context.shell,
                                &params,
                                word,
                            )
                            .await?
                            .into_iter()
                            .map(CommandArg::String)
                            .collect();
                            args.append(&mut next_args);
                        }
                    }
                }
                CommandPrefixOrSuffixItem::Word(arg) => {
                    let mut next_args =
                        expansion::full_expand_and_split_word(&mut context.shell, &params, arg)
                            .await?;

                    if args.is_empty() {
                        if let Some(cmd_name) = next_args.first() {
                            // Aliases are only expanded when `expand_aliases` is enabled; it's
                            // enabled by default for interactive shells.
                            if context.shell.options().expand_aliases
                                && let Some(alias_value) =
                                    context.shell.aliases().get(cmd_name.as_str())
                            {
                                //
                                // TODO(#57): This is a total hack; aliases are supposed to be
                                // handled much earlier in the process.
                                //
                                // N.B. Tokenizing first releases our borrow of the shell's aliases,
                                // so we can take a mutable borrow of the shell to expand the words.
                                let alias_words = tokenize_alias_body(&context.shell, alias_value);
                                let mut alias_pieces =
                                    expand_words(&mut context.shell, &params, alias_words).await?;

                                next_args.remove(0);
                                alias_pieces.append(&mut next_args);

                                next_args = alias_pieces;
                            }

                            // Check if we're going to be invoking a special declaration builtin.
                            // That will change how we parse and process args. (An alias with an
                            // empty body leaves us with no words at all.)
                            if let Some(first_arg) = next_args.first()
                                && context
                                    .shell
                                    .builtins()
                                    .get(first_arg.as_str())
                                    .is_some_and(|r| !r.disabled && r.declaration_builtin)
                            {
                                command_takes_assignments = true;
                            }
                        }
                    } else if !command_takes_assignments
                        && context.shell.options().posix_mode
                        && args
                            .iter()
                            .all(|a| matches!(a, CommandArg::String(s) if s == "command"))
                        && let Some(first_arg) = next_args.first()
                        && context
                            .shell
                            .builtins()
                            .get(first_arg.as_str())
                            .is_some_and(|r| !r.disabled && r.declaration_builtin)
                    {
                        // Bash 5.3 POSIX mode: `command declare a=$x` keeps the declaration
                        // builtin's assignment parsing, so `$x` is not field-split. Bash
                        // does not do this outside POSIX mode.
                        command_takes_assignments = true;
                    }

                    let mut next_args = next_args.into_iter().map(CommandArg::String).collect();
                    args.append(&mut next_args);
                }
            }
        }

        // If we have a command, then execute it.
        if let Some(CommandArg::String(cmd_name)) = args.first() {
            let mut stderr = params.stderr(&context.shell);

            let (owned_shell, parent_shell) = match context.shell {
                commands::ShellForCommand::ParentShell(shell) => (None, shell),
                commands::ShellForCommand::OwnedShell { target, parent } => (Some(target), parent),
            };

            let shell = if let Some(owned_shell) = owned_shell {
                commands::ShellForCommand::OwnedShell {
                    target: owned_shell,
                    parent: parent_shell,
                }
            } else {
                commands::ShellForCommand::ParentShell(parent_shell)
            };

            let context = PipelineExecutionContext {
                shell,
                process_group_id: context.process_group_id,
            };

            match execute_command(context, params, cmd_name, &assignments, &args).await {
                Ok(result) => Ok(result),
                Err(err) if err.discards_line() => Err(err),
                Err(err) => {
                    let _ = parent_shell.display_error(&mut stderr, &err);

                    let result = err.into_result(parent_shell);
                    Ok(result.into())
                }
            }
        } else {
            // No command to run; assignments must be applied to this shell.
            for assignment in assignments {
                // Apply the assignment. Don't mark as fatal - let errors be handled
                // at the program level so multiple complete_commands can execute independently.
                apply_assignment(
                    assignment,
                    &mut context.shell,
                    &params,
                    false,
                    None,
                    EnvironmentScope::Global,
                )
                .await?;
            }

            // Assignment-only statements clear $_ (set to empty string).
            // This matches bash behavior where assignments don't have a "last
            // argument".
            context.shell.update_last_arg_variable(None);

            // We need to set the last exit status to indicate assignment success,
            // but only if there was no status set during expansion. We use the
            // status count captured before expansion to detect if command
            // substitution (or other expansion) set an exit status.
            if status_change_count_before_expansion == context.shell.last_exit_status_change_count()
            {
                context.shell.set_last_exit_status(0);
            }

            // Return the last exit status we have; in some cases, an expansion
            // might result in a non-zero exit status stored in the shell.
            Ok(ExecutionResult::new(context.shell.last_exit_status()).into())
        }
    }
}

async fn execute_command<T: Into<String>>(
    mut context: PipelineExecutionContext<'_, impl extensions::ShellExtensions>,
    params: ExecutionParameters,
    cmd_name: T,
    assignments: &[&ast::Assignment],
    args: &[CommandArg],
) -> Result<ExecutionSpawnResult, error::Error> {
    // Push a new ephemeral environment scope for the duration of the command. We'll
    // set command-scoped variable assignments after doing so, and revert them before
    // returning.
    let mut guard = crate::env::ScopeGuard::new(&mut context.shell, EnvironmentScope::Command);

    for assignment in assignments {
        // Ensure it's tagged as exported and created in the command scope.
        apply_assignment(
            assignment,
            guard.shell(),
            &params,
            true,
            Some(EnvironmentScope::Command),
            EnvironmentScope::Command,
        )
        .await?;
    }

    if guard.shell().options().print_commands_and_arguments {
        guard
            .shell()
            .trace_command(
                &params,
                args.iter().map(|arg| arg.quote_for_tracing()).join(" "),
            )
            .await;
    }

    guard.detach();
    drop(guard);

    // Construct the command struct.
    let mut cmd =
        commands::SimpleCommand::new(context.shell, params, cmd_name.into(), args.iter().cloned());
    cmd.process_group_id = context.process_group_id;

    // Arrange to pop off that ephemeral environment scope.
    cmd.post_execute = Some(|shell| shell.env_mut().pop_scope(EnvironmentScope::Command));

    // Run through any pre-execution hooks as best effort; a DEBUG trap that exits or
    // returns replaces the command.
    if let Ok(Some(result)) = commands::on_preexecute(&mut cmd).await {
        return Ok(result.into());
    }

    // Execute
    // TODO(jobs): do we need to move self back to foreground on error here?
    cmd.execute().await
}

/// Tokenizes the body of an alias into the unexpanded words that should replace the aliased command
/// name.
///
/// This only handles alias bodies that amount to a simple sequence of words; bodies containing
/// operators (pipes, redirections, `&&`, ...) and recursive expansion of chained aliases require
/// alias substitution to happen in the tokenizer instead (see issue #57).
fn tokenize_alias_body(
    shell: &Shell<impl extensions::ShellExtensions>,
    alias_value: &str,
) -> Vec<String> {
    // Tokenize the body the same way the shell would tokenize any other input it's given.
    let options = shell.parser_options().tokenizer_options();

    // If we can't tokenize the body, fall back to naively splitting it on whitespace.
    cash_parser::tokenize_str_with_options(alias_value, &options).map_or_else(
        |_| {
            alias_value
                .split_ascii_whitespace()
                .map(str::to_owned)
                .collect()
        },
        |tokens| tokens.iter().map(|t| t.to_str().to_owned()).collect(),
    )
}

/// Expands the given words, with splitting enabled, yielding the fields they expand to.
async fn expand_words(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    words: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<Vec<String>, error::Error> {
    // N.B. Expansion needs `&mut shell`, so the words have to be expanded in sequence.
    let mut fields = vec![];
    for word in words {
        fields.extend(expansion::full_expand_and_split_word(shell, params, word).await?);
    }
    Ok(fields)
}

async fn expand_assignment(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    assignment: &ast::Assignment,
) -> Result<ast::Assignment, error::Error> {
    let value = expand_assignment_value(shell, params, &assignment.value).await?;
    Ok(ast::Assignment {
        name: basic_expand_assignment_name(shell, params, &assignment.name).await?,
        value,
        append: assignment.append,
        loc: assignment.loc.clone(),
    })
}

async fn basic_expand_assignment_name(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    name: &ast::AssignmentName,
) -> Result<ast::AssignmentName, error::Error> {
    match name {
        ast::AssignmentName::VariableName(name) => {
            let expanded = expansion::basic_expand_word(shell, params, name).await?;
            Ok(ast::AssignmentName::VariableName(expanded))
        }
        ast::AssignmentName::ArrayElementName(name, index) => {
            let expanded_name = expansion::basic_expand_word(shell, params, name).await?;
            let expanded_index = expansion::basic_expand_word(shell, params, index).await?;
            Ok(ast::AssignmentName::ArrayElementName(
                expanded_name,
                expanded_index,
            ))
        }
    }
}

async fn expand_assignment_value(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    value: &ast::AssignmentValue,
) -> Result<ast::AssignmentValue, error::Error> {
    let expanded = match value {
        ast::AssignmentValue::Scalar(s) => {
            let expanded_word = expansion::basic_expand_assignment_word(shell, params, s).await?;
            ast::AssignmentValue::Scalar(ast::Word::from(expanded_word))
        }
        ast::AssignmentValue::Array(arr) => {
            let mut expanded_values = vec![];
            for (key, value) in arr {
                if let Some(k) = key {
                    let expanded_key = expansion::basic_expand_assignment_word(shell, params, k)
                        .await?
                        .into();
                    let expanded_value =
                        expansion::basic_expand_assignment_word(shell, params, value)
                            .await?
                            .into();
                    expanded_values.push((Some(expanded_key), expanded_value));
                } else {
                    // Array elements are treated as regular words, not assignments
                    let split_expanded_value =
                        expansion::full_expand_and_split_word(shell, params, value).await?;
                    for expanded_value in split_expanded_value {
                        expanded_values.push((None, expanded_value.into()));
                    }
                }
            }

            ast::AssignmentValue::Array(expanded_values)
        }
    };

    Ok(expanded)
}

#[expect(clippy::too_many_lines)]
async fn apply_assignment(
    assignment: &ast::Assignment,
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    mut export: bool,
    required_scope: Option<EnvironmentScope>,
    creation_scope: EnvironmentScope,
) -> Result<(), error::Error> {
    // Figure out if we are trying to assign to a variable or assign to an element of an existing
    // array.
    let mut array_index;
    let variable_name = match &assignment.name {
        ast::AssignmentName::VariableName(name) => {
            array_index = None;
            name
        }
        ast::AssignmentName::ArrayElementName(name, index) => {
            let expanded = expansion::basic_expand_word(shell, params, index).await?;
            array_index = Some(expanded);
            name
        }
    };

    // cash: an assignment through a name reference lands on the name it stands for, and
    // has to create that name if it is not there yet — `local -n out=$1; out=result` is
    // how a bash function writes back into a variable its caller owns, and the caller
    // need never have declared it. Resolving here rather than only in the lookup below is
    // what makes the creation path land in the right place.
    let Some(resolved_name) = shell.env().resolved_name(variable_name.as_str()) else {
        // A circular chain of references names nothing, so there is nowhere for the value
        // to go. bash warns and drops the assignment rather than writing to the reference
        // itself, which would quietly break the chain.
        return Ok(());
    };
    let resolved_name = resolved_name.into_owned();
    let variable_name = &resolved_name;

    // Expand the values.
    let mut new_value = match &assignment.value {
        ast::AssignmentValue::Scalar(unexpanded_value) => {
            let value =
                expansion::basic_expand_assignment_word(shell, params, unexpanded_value).await?;
            ShellValueLiteral::Scalar(value)
        }
        ast::AssignmentValue::Array(unexpanded_values) => {
            let mut elements = vec![];
            for (unexpanded_key, unexpanded_value) in unexpanded_values {
                let key = match unexpanded_key {
                    Some(unexpanded_key) => Some(
                        expansion::basic_expand_assignment_word(shell, params, unexpanded_key)
                            .await?,
                    ),
                    None => None,
                };

                if key.is_some() {
                    let value =
                        expansion::basic_expand_assignment_word(shell, params, unexpanded_value)
                            .await?;
                    elements.push((key, value));
                } else {
                    // Array elements are treated as regular words, not assignments
                    let values =
                        expansion::full_expand_and_split_word(shell, params, unexpanded_value)
                            .await?;
                    for value in values {
                        elements.push((None, value));
                    }
                }
            }
            ShellValueLiteral::Array(ArrayLiteral(elements))
        }
    };

    if shell.options().print_commands_and_arguments {
        let op = if assignment.append { "+=" } else { "=" };
        shell
            .trace_command(params, std::format!("{}{op}{new_value}", assignment.name))
            .await;
    }

    // See if we need to eval an array index.
    if let Some(idx) = &array_index {
        // An array subscript is arithmetically evaluated unless the target is an
        // associative array (in which case the subscript is used as a literal key).
        // A scalar or unset/untyped variable becomes an indexed array, so its
        // subscript still needs to be evaluated.
        let will_be_indexed_array =
            if let Some((_, existing_value)) = shell.env().get(variable_name) {
                !matches!(
                    existing_value.value(),
                    ShellValue::AssociativeArray(_)
                        | ShellValue::Unset(ShellValueUnsetType::AssociativeArray)
                )
            } else {
                true
            };

        if will_be_indexed_array {
            array_index = Some(
                arithmetic::expand_and_eval(shell, params, idx.as_str(), false)
                    .await?
                    .to_string(),
            );
        }
    }

    // Read option before taking mutable borrow on env.
    let export_variables_on_modification = shell.options().export_variables_on_modification;

    // If the target variable is marked as an integer, evaluate its scalar value arithmetically.
    if let Some((_, var)) = shell.env().get(variable_name) {
        if var.is_treated_as_integer() {
            if let ShellValueLiteral::Scalar(s) = &mut new_value {
                if let Ok(eval_val) = arithmetic::evaluate_str(shell, s.as_str()) {
                    *s = eval_val.to_string();
                }
            }
        }
    }

    // SECONDS is a live stopwatch rather than a stored scalar. Assignment resets its
    // baseline, including Bash's supported negative values; `+=` starts from the value
    // observed at the instant of assignment.
    if variable_name == "SECONDS"
        && array_index.is_none()
        && let ShellValueLiteral::Scalar(value) = &new_value
    {
        let assigned = value.parse::<i64>().unwrap_or(0);
        let assigned = if assignment.append {
            shell
                .env_str("SECONDS")
                .and_then(|current| current.parse::<i64>().ok())
                .unwrap_or(0)
                .saturating_add(assigned)
        } else {
            assigned
        };
        shell.set_stopwatch_seconds(assigned);
        if export && let Some((_, seconds)) = shell.env_mut().get_mut("SECONDS") {
            seconds.export();
        }
        return Ok(());
    }

    // See if we can find an existing value associated with the variable.
    if let Some((existing_value_scope, existing_value)) =
        shell.env_mut().get_mut(variable_name.as_str())
    {
        if required_scope.is_none() || Some(existing_value_scope) == required_scope {
            if let Some(array_index) = array_index {
                match new_value {
                    ShellValueLiteral::Scalar(s) => {
                        existing_value.assign_at_index(array_index, s, assignment.append)?;
                    }
                    ShellValueLiteral::Array(_) => {
                        return error::unimp("replacing an array item with an array");
                    }
                }
            } else {
                if !export
                    && export_variables_on_modification
                    && !matches!(new_value, ShellValueLiteral::Array(_))
                {
                    export = true;
                }

                existing_value.assign(new_value, assignment.append)?;
            }

            if export {
                existing_value.export();
            }

            // That's it!
            return Ok(());
        }
    }

    // If we fell down here, then we need to add it.
    let new_value = if let Some(array_index) = array_index {
        match new_value {
            ShellValueLiteral::Scalar(s) => {
                ShellValue::indexed_array_from_literals(ArrayLiteral(vec![(Some(array_index), s)]))
            }
            ShellValueLiteral::Array(_) => {
                return error::unimp("cannot assign list to array member");
            }
        }
    } else {
        match new_value {
            ShellValueLiteral::Scalar(s) => {
                export = export || shell.options().export_variables_on_modification;
                ShellValue::String(s)
            }
            ShellValueLiteral::Array(values) => ShellValue::indexed_array_from_literals(values),
        }
    };

    let mut new_var = ShellVariable::new(new_value);

    if export {
        new_var.export();
    }

    shell.env_mut().add(variable_name, new_var, creation_scope)
}

#[expect(clippy::too_many_lines)]
pub(crate) async fn setup_redirect(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &'_ mut ExecutionParameters,
    redirect: &ast::IoRedirect,
) -> Result<(), error::Error> {
    match redirect {
        ast::IoRedirect::OutputAndError(f, append) => {
            let mut expanded_fields =
                expansion::full_expand_and_split_word(shell, params, f).await?;
            if expanded_fields.len() != 1 {
                return Err(error::ErrorKind::InvalidRedirection.into());
            }

            let expanded_file_path = expanded_fields.remove(0);
            setup_redirect_output_and_error_to(shell, params, &expanded_file_path, *append)?;
        }

        ast::IoRedirect::File(specified_fd_num, kind, target) => {
            match target {
                ast::IoFileRedirectTarget::Filename(f) => {
                    let mut options = std::fs::File::options();

                    let mut expanded_fields =
                        expansion::full_expand_and_split_word(shell, params, f).await?;

                    if expanded_fields.len() != 1 {
                        return Err(error::ErrorKind::InvalidRedirection.into());
                    }

                    let written_file_path = expanded_fields.remove(0);
                    let expanded_file_path: PathBuf =
                        shell.absolute_path(Path::new(written_file_path.as_str()));

                    let default_fd_if_unspecified = get_default_fd_for_redirect_kind(kind);
                    let access = get_access_for_redirect_kind(kind);
                    match kind {
                        ast::IoFileRedirectKind::Read => {
                            options.read(true);
                        }
                        ast::IoFileRedirectKind::Write => {
                            if shell
                                .options()
                                .disallow_overwriting_regular_files_via_output_redirection
                            {
                                // First check to see if the path points to an existing regular
                                // file.
                                if !expanded_file_path.is_file() {
                                    options.create(true);
                                } else {
                                    options.create_new(true);
                                }
                                options.write(true);
                            } else {
                                options.create(true);
                                options.write(true);
                                options.truncate(true);
                            }
                        }
                        ast::IoFileRedirectKind::Append => {
                            options.create(true);
                            options.append(true);
                        }
                        ast::IoFileRedirectKind::ReadAndWrite => {
                            options.create(true);
                            options.read(true);
                            options.write(true);
                        }
                        ast::IoFileRedirectKind::Clobber => {
                            options.create(true);
                            options.write(true);
                            options.truncate(true);
                        }
                        ast::IoFileRedirectKind::DuplicateInput => {
                            options.read(true);
                        }
                        ast::IoFileRedirectKind::DuplicateOutput => {
                            options.create(true);
                            options.write(true);
                        }
                    }

                    let fd_num = specified_fd_num.unwrap_or(default_fd_if_unspecified);

                    let opened_file = shell
                        .open_file(&options, access, &expanded_file_path, params)
                        .map_err(|err| {
                            error::ErrorKind::RedirectionFailure(
                                redirect_target_name(&written_file_path, &expanded_file_path),
                                err.to_string(),
                            )
                        })?;

                    params.set_redirected_fd(fd_num, opened_file);
                }

                ast::IoFileRedirectTarget::Fd(fd) => {
                    let default_fd_if_unspecified = match kind {
                        ast::IoFileRedirectKind::DuplicateInput => 0,
                        ast::IoFileRedirectKind::DuplicateOutput => 1,
                        _ => {
                            return error::unimp("unexpected redirect kind");
                        }
                    };

                    let fd_num = specified_fd_num.unwrap_or(default_fd_if_unspecified);

                    if let Some(target_file) = params.try_fd(shell, *fd) {
                        params.set_redirected_fd(fd_num, target_file);
                    } else {
                        return Err(error::ErrorKind::BadFileDescriptor(*fd).into());
                    }
                }

                ast::IoFileRedirectTarget::Duplicate(word) => {
                    let mut expanded_fields =
                        expansion::full_expand_and_split_word(shell, params, word).await?;

                    if expanded_fields.len() != 1 {
                        return Err(error::ErrorKind::InvalidRedirection.into());
                    }
                    setup_duplicate_redirect(
                        shell,
                        params,
                        *specified_fd_num,
                        kind,
                        expanded_fields.remove(0),
                    )?;
                }

                ast::IoFileRedirectTarget::ProcessSubstitution(substitution_kind, subshell_cmd) => {
                    match kind {
                        ast::IoFileRedirectKind::Read
                        | ast::IoFileRedirectKind::Write
                        | ast::IoFileRedirectKind::Append
                        | ast::IoFileRedirectKind::ReadAndWrite
                        | ast::IoFileRedirectKind::Clobber => {
                            let (_arg_path, substitution_fd, substitution_file) =
                                setup_process_substitution(
                                    shell,
                                    params,
                                    substitution_kind,
                                    subshell_cmd,
                                    true,
                                    false,
                                )
                                .await?;

                            let target_file = substitution_file.clone();
                            if let Some(fd) = substitution_fd {
                                params.open_files.set_fd(fd, substitution_file);
                            }

                            let fd_num = specified_fd_num
                                .unwrap_or_else(|| get_default_fd_for_redirect_kind(kind));

                            params.set_redirected_fd(fd_num, target_file);
                        }
                        _ => return error::unimp("invalid process substitution"),
                    }
                }
            }
        }

        ast::IoRedirect::VariableFile(name, kind, target) => {
            // Expand descriptor-duplication words here so an indirect close (`x=-;
            // {fd}>&$x`) can select the descriptor already stored in the variable. The
            // shared helper then consumes the expansion without evaluating it twice.
            let expanded_duplicate = if let ast::IoFileRedirectTarget::Duplicate(word) = target {
                let mut fields = expansion::full_expand_and_split_word(shell, params, word).await?;
                if fields.len() != 1 {
                    return Err(error::ErrorKind::InvalidRedirection.into());
                }
                Some(fields.remove(0))
            } else {
                None
            };
            let closes_existing = expanded_duplicate.as_deref() == Some("-");
            let fd_num = if closes_existing {
                expansion::basic_expand_word(shell, params, format!("${{{name}}}"))
                    .await?
                    .parse::<ShellFd>()
                    .ok()
                    .filter(|fd| *fd >= 0)
                    .ok_or(error::ErrorKind::InvalidRedirection)?
            } else {
                next_variable_redirection_fd(shell, params)?
            };

            if let Some(expanded) = expanded_duplicate {
                setup_duplicate_redirect(shell, params, Some(fd_num), kind, expanded)?;
            } else {
                let numbered = ast::IoRedirect::File(Some(fd_num), kind.clone(), target.clone());
                Box::pin(setup_redirect(shell, params, &numbered)).await?;
            }

            if closes_existing {
                shell.open_files_mut().remove_fd(fd_num);
            } else {
                expansion::assign_to_named_parameter(shell, params, name, fd_num.to_string())
                    .await?;

                // Without varredir_close, Bash keeps the allocated descriptor beyond
                // this command. With it, the command-local entry dies with `params`.
                // A redirection-only `exec` copies `params` into the shell, preserving
                // Bash's exec exception automatically.
                if !shell.options().var_redir_close
                    && let Some(file) = params.try_fd(shell, fd_num)
                {
                    shell.open_files_mut().set_fd(fd_num, file);
                }
            }
        }

        ast::IoRedirect::HereDocument(fd_num, io_here) => {
            // If not specified, default to stdin (fd 0).
            let fd_num = fd_num.unwrap_or(0);

            // Expand if required.
            let io_here_doc = if io_here.requires_expansion {
                expansion::basic_expand_heredoc_word(shell, params, &io_here.doc).await?
            } else {
                io_here.doc.flatten()
            };

            let f = setup_open_file_with_contents(io_here_doc.as_str())?;

            params.set_redirected_fd(fd_num, f);
        }

        ast::IoRedirect::HereString(fd_num, word) => {
            // If not specified, default to stdin (fd 0).
            let fd_num = fd_num.unwrap_or(0);

            let mut expanded_word = expansion::basic_expand_word(shell, params, word).await?;
            expanded_word.push('\n');

            let f = setup_open_file_with_contents(expanded_word.as_str())?;

            params.set_redirected_fd(fd_num, f);
        }
    }

    Ok(())
}

fn next_variable_redirection_fd(
    shell: &Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
) -> Result<ShellFd, error::Error> {
    for fd in 10..=1024 {
        if params.try_fd(shell, fd).is_none() {
            return Ok(fd);
        }
    }
    Err(error::ErrorKind::TooManyOpenFiles.into())
}

fn setup_duplicate_redirect(
    shell: &Shell<impl extensions::ShellExtensions>,
    params: &mut ExecutionParameters,
    specified_fd_num: Option<ShellFd>,
    kind: &ast::IoFileRedirectKind,
    mut expanded: String,
) -> Result<(), error::Error> {
    let default_fd_if_unspecified = match kind {
        ast::IoFileRedirectKind::DuplicateInput => 0,
        ast::IoFileRedirectKind::DuplicateOutput => 1,
        _ => return error::unimp("unexpected redirect kind"),
    };
    let fd_num = specified_fd_num.unwrap_or(default_fd_if_unspecified);
    let dash = if expanded.ends_with('-') {
        expanded.pop();
        true
    } else {
        false
    };

    if expanded.is_empty() {
        // A bare '-' is handled by the close below.
    } else if expanded.chars().all(|c| c.is_ascii_digit()) {
        let source_fd_num = expanded
            .parse::<ShellFd>()
            .map_err(|_| error::ErrorKind::InvalidRedirection)?;
        let Some(target_file) = params.try_fd(shell, source_fd_num) else {
            return Err(error::ErrorKind::BadFileDescriptor(source_fd_num).into());
        };
        params.set_redirected_fd(fd_num, target_file);
    } else if fd_num == 1 && !dash {
        setup_redirect_output_and_error_to(shell, params, &expanded, false)?;
    } else {
        return Err(error::ErrorKind::InvalidRedirection.into());
    }

    if dash {
        let fd_to_close = if expanded.is_empty() {
            fd_num
        } else {
            expanded
                .parse::<ShellFd>()
                .map_err(|_| error::ErrorKind::InvalidRedirection)?
        };
        params.open_files.remove_fd(fd_to_close);
    }
    Ok(())
}

/// Sets up redirection of both stdout and stderr to the same file, given by `file_path`.
///
/// # Arguments
///
/// * `shell` - The shell instance.
/// * `params` - The execution parameters to modify.
/// * `file_path` - The path to the file to redirect output and error to.
/// * `append` - Whether to append. If `false`, the file will be truncated.
fn setup_redirect_output_and_error_to(
    shell: &Shell<impl extensions::ShellExtensions>,
    params: &mut ExecutionParameters,
    file_path: &str,
    append: bool,
) -> Result<(), error::Error> {
    let abs_file_path: PathBuf = shell.absolute_path(Path::new(file_path));

    let mut file_options = std::fs::File::options();
    file_options
        .create(true)
        .write(true)
        .truncate(!append)
        .append(append);

    let stdout_file = shell
        .open_file(
            &file_options,
            sys::fs::Access::Write,
            &abs_file_path,
            params,
        )
        .map_err(|err| {
            error::ErrorKind::RedirectionFailure(
                redirect_target_name(file_path, &abs_file_path),
                err.to_string(),
            )
        })?;

    let stderr_file = stdout_file.clone();

    params.open_files.set_fd(OpenFiles::STDOUT_FD, stdout_file);
    params.open_files.set_fd(OpenFiles::STDERR_FD, stderr_file);

    Ok(())
}

/// The name a redirection's target goes by when it cannot be opened: the path it was
/// resolved to, or for a device or a descriptor (`/dev/fd/9`) the name as it was written.
/// `/dev/tty` resolves to `C:/dev/tty` on its way to being opened, and no such file was
/// ever looked for.
fn redirect_target_name(written: &str, resolved: &Path) -> String {
    if sys::fs::is_special_file(resolved) || sys::fs::named_descriptor(resolved).is_some() {
        written.to_owned()
    } else {
        resolved.to_string_lossy().to_string()
    }
}

/// What a redirection of the given kind opens its file to do.
const fn get_access_for_redirect_kind(kind: &ast::IoFileRedirectKind) -> sys::fs::Access {
    match kind {
        ast::IoFileRedirectKind::Read | ast::IoFileRedirectKind::DuplicateInput => {
            sys::fs::Access::Read
        }
        ast::IoFileRedirectKind::Write
        | ast::IoFileRedirectKind::Append
        | ast::IoFileRedirectKind::Clobber
        | ast::IoFileRedirectKind::DuplicateOutput => sys::fs::Access::Write,
        ast::IoFileRedirectKind::ReadAndWrite => sys::fs::Access::ReadWrite,
    }
}

const fn get_default_fd_for_redirect_kind(kind: &ast::IoFileRedirectKind) -> ShellFd {
    match kind {
        ast::IoFileRedirectKind::Read => 0,
        ast::IoFileRedirectKind::Write => 1,
        ast::IoFileRedirectKind::Append => 1,
        ast::IoFileRedirectKind::ReadAndWrite => 0,
        ast::IoFileRedirectKind::Clobber => 1,
        ast::IoFileRedirectKind::DuplicateInput => 0,
        ast::IoFileRedirectKind::DuplicateOutput => 1,
    }
}

/// A `>(...)` that may still be running: the thread that runs it, and the named pipe it
/// reads from when it was handed to a command as a path.
struct OutputSubstitution {
    thread: std::thread::JoinHandle<()>,
    pipe: Option<String>,
}

/// The `>(...)` substitutions started, for the shell to wait for before it exits.
static OUTPUT_SUBSTITUTIONS: std::sync::Mutex<Vec<OutputSubstitution>> =
    std::sync::Mutex::new(Vec::new());

fn keep_output_substitution(thread: std::thread::JoinHandle<()>, pipe: Option<String>) {
    let mut running = OUTPUT_SUBSTITUTIONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Those that have finished are let go, so that a loop that starts many keeps few.
    running.retain(|substitution| !substitution.thread.is_finished());
    running.push(OutputSubstitution { thread, pipe });
}

/// Waits for the `>(...)` substitutions still running, as the shell exits (D17).
///
/// A substitution runs on a thread of the shell, where Bash's is a process of its own
/// that outlives it. So the shell used to end them unfinished when it exited, and what
/// they had not yet written was lost: `echo x > >(sleep 1; cat)` printed nothing, and
/// neither did `exec > >(tee log)` for what the script wrote last. Bash's substitution
/// writes it after Bash has gone; here the shell waits until it has.
///
/// Called once the shell itself is gone, when nothing of it holds the write end of a
/// substitution's input open any more: each has then been given the end of its input,
/// and ends when it has dealt with it. One handed a path no program opened is given the
/// end of its input here. One that never ends keeps the shell from exiting, as a
/// command that never ends does.
pub fn finish_output_substitutions() {
    let running = std::mem::take(
        &mut *OUTPUT_SUBSTITUTIONS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    for substitution in running {
        if let Some(pipe) = &substitution.pipe
            && !substitution.thread.is_finished()
        {
            cash_win32::pipe::release_unclaimed(pipe);
        }
        let _ = substitution.thread.join();
    }
}

/// Set up a process substitution, returning the argument the command should receive,
/// the fd the file is installed on, and the file itself.
///
/// cash (D17): the argument is a Win32 Named Pipe (`\\.\pipe\cash-procsub-...`), or a
/// direct pipe when the substitution is itself a redirection.
async fn setup_process_substitution(
    shell: &Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    kind: &ast::ProcessSubstitutionKind,
    subshell_cmd: &ast::SubshellCommand,
    for_redirect: bool,
    requires_seekable_file: bool,
) -> Result<(String, Option<ShellFd>, OpenFile), error::Error> {
    setup_process_substitution_win(
        shell,
        params,
        kind,
        subshell_cmd,
        for_redirect,
        requires_seekable_file,
    )
    .await
}

/// cash (D17): process substitution via Win32 Named Pipes and live streaming.
///
/// Windows has no `/dev/fd`, and a child cannot inherit an arbitrary descriptor (D26),
/// so the pipe-and-`/dev/fd/63` approach cannot work for native executables.
///
/// If `for_redirect` is true (e.g. `< <(cmd)` or `> >(cmd)`), the redirection is
/// handled entirely in-process: an anonymous pipe connects the subshell directly to the
/// outer command's standard input or output with zero disk usage and live streaming.
///
/// If `for_redirect` is false (e.g. `cat <(cmd)` or `cmd >(subshell)`), a Win32 Named
/// Pipe (`\\.\pipe\cash-procsub-...`) is created. Native executables open it via standard
/// Win32 file APIs, and data streams in real time via kernel memory pipes.
async fn setup_process_substitution_win(
    shell: &Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    kind: &ast::ProcessSubstitutionKind,
    subshell_cmd: &ast::SubshellCommand,
    for_redirect: bool,
    requires_seekable_file: bool,
) -> Result<(String, Option<ShellFd>, OpenFile), error::Error> {
    let mut subshell = shell.clone();
    let mut child_params = params.clone();
    child_params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

    if for_redirect {
        let (reader, writer) = std::io::pipe()?;
        let target_file = match kind {
            ast::ProcessSubstitutionKind::Read => {
                child_params
                    .open_files
                    .set_fd(OpenFiles::STDOUT_FD, writer.into());
                OpenFile::from(reader)
            }
            ast::ProcessSubstitutionKind::Write => {
                child_params
                    .open_files
                    .set_fd(OpenFiles::STDIN_FD, reader.into());
                OpenFile::from(writer)
            }
        };

        let subshell_cmd = subshell_cmd.to_owned();
        let thread = std::thread::Builder::new()
            .name("cash-procsub".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                if let Ok(rt) = rt {
                    let _ = rt.block_on(subshell_cmd.list.execute(&mut subshell, &child_params));
                }
            })?;
        if matches!(kind, ast::ProcessSubstitutionKind::Write) {
            keep_output_substitution(thread, None);
        }

        return Ok((String::new(), None, target_file));
    }

    // Windows CRT `_stat()` cannot stat Win32 Named Pipes (returning ERROR_INVALID_NAME / ENOENT).
    // Commands that require seeking or regular file stat (such as `diff` or `cmp`) fall back to
    // an awaited temp file so `diff <(a) <(b)` succeeds transparently.
    if requires_seekable_file && matches!(kind, ast::ProcessSubstitutionKind::Read) {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("cash-procsub-seek-{}-{n}", std::process::id()));

        child_params.open_files.set_fd(
            OpenFiles::STDOUT_FD,
            OpenFile::from(std::fs::File::create(&path)?),
        );

        let subshell_cmd = subshell_cmd.to_owned();
        let _ = subshell_cmd
            .list
            .execute(&mut subshell, &child_params)
            .await;

        let target_file = OpenFile::from(std::fs::File::open(&path)?);
        return Ok((cash_win32::path::render(&path), None, target_file));
    }

    let (path, target_file) = match kind {
        ast::ProcessSubstitutionKind::Read => {
            let sub = cash_win32::pipe::create_read_substitution()?;
            child_params
                .open_files
                .set_fd(OpenFiles::STDOUT_FD, sub.writer.into());
            (sub.path, openfiles::null()?)
        }
        ast::ProcessSubstitutionKind::Write => {
            let sub = cash_win32::pipe::create_write_substitution()?;
            child_params
                .open_files
                .set_fd(OpenFiles::STDIN_FD, sub.reader.into());
            (sub.path, openfiles::null()?)
        }
    };

    let subshell_cmd = subshell_cmd.to_owned();
    let thread = std::thread::Builder::new()
        .name("cash-procsub".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(rt) = rt {
                let _ = rt.block_on(subshell_cmd.list.execute(&mut subshell, &child_params));
            }
        })?;
    if matches!(kind, ast::ProcessSubstitutionKind::Write) {
        keep_output_substitution(thread, Some(path.clone()));
    }

    Ok((path, None, target_file))
}

fn setup_open_file_with_contents(contents: &str) -> Result<OpenFile, error::Error> {
    let bytes = contents.as_bytes();

    // cash: on Windows a pipe deadlocks above 4096 bytes, because nothing is draining it
    // until the command that reads the here-document starts. See
    // `sys::windows::fs::open_temp_with_contents`.
    Ok(crate::sys::fs::open_temp_with_contents(bytes)?.into())
}
