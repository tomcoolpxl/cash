//! Trap handling for the shell.

use std::io::Write as _;

use crate::{ExecutionParameters, ExecutionResult, ProcessGroupPolicy, error, traps::TrapSignal};

/// The status of a process a signal ended: 128 + its number.
#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const fn signal_status(signal: crate::sys::signal::Signal) -> u8 {
    (128 + signal as i32) as u8
}

impl<SE: crate::extensions::ShellExtensions> crate::Shell<SE> {
    /// Runs any exit steps for the shell.
    ///
    /// This currently includes invoking the `EXIT` trap handler, if any.
    pub async fn on_exit(&mut self) -> Result<(), error::Error> {
        if self.traps.handles(TrapSignal::Exit) {
            self.invoke_trap_handler(TrapSignal::Exit, &self.default_exec_params())
                .await?;
        }

        Ok(())
    }

    /// `exit` in the shell itself: runs its `EXIT` trap there and then, as Bash does, so
    /// that the trap sees where `exit` was called (in a function, `FUNCNAME` is `f main`);
    /// the trap is then spent, and the shell's end runs it no more. A subshell's `exit`
    /// leaves its own trap to `end_subshell`. Returns the status to exit with,
    /// which an `exit` in the trap changes.
    pub async fn run_exit_trap_for_exit(&mut self, status: u8, params: &ExecutionParameters) -> u8 {
        if self.is_subshell() || !self.traps.handles(TrapSignal::Exit) {
            return status;
        }
        self.last_exit_status = status;
        let outcome = self.invoke_trap_handler(TrapSignal::Exit, params).await;
        self.traps.remove_handlers(TrapSignal::Exit);
        match outcome {
            Ok(result) if result.is_exit() => result.exit_code.into(),
            _ => status,
        }
    }

    /// Ends a subshell (`( … )`, a command substitution, a compound pipeline stage, a
    /// background job) that ended with `status`: runs its `EXIT` trap, under `params`, if
    /// it set one itself, as Bash does as the subshell exits. The parent's, which the copy
    /// carries, is not its to run. Returns the subshell's status, which an `exit` in the
    /// trap changes.
    pub(crate) async fn end_subshell(&mut self, status: u8, params: &ExecutionParameters) -> u8 {
        let set_here = self
            .traps
            .get_handler(TrapSignal::Exit)
            .is_some_and(|handler| handler.depth == self.depth());
        if !set_here {
            return status;
        }
        self.last_exit_status = status;
        match self.invoke_trap_handler(TrapSignal::Exit, params).await {
            Ok(result) if result.is_exit() => result.exit_code.into(),
            _ => status,
        }
    }

    /// Runs the trap for a signal the shell sent itself (`kill -TERM $$`), if one is set:
    /// `None` when there is none, so the caller applies the signal's default action.
    pub async fn raise_signal_trap(
        &mut self,
        signal: TrapSignal,
        params: &ExecutionParameters,
    ) -> Option<Result<ExecutionResult, error::Error>> {
        if !self.traps.handles(signal) {
            return None;
        }
        Some(self.invoke_trap_handler(signal, params).await)
    }

    /// A signal the shell sends itself, `kill -TERM $$`: handled inside the shell, as Bash
    /// handles it, rather than delivered through the OS. Delivered, `INT` and `TERM` ended
    /// cash at once, trap or no trap.
    ///
    /// A trapped signal runs its trap and the shell goes on: so does one Windows cannot
    /// send a program, `USR1` and the rest. Otherwise the signal's default
    /// (`default_signal_action`).
    ///
    /// # Errors
    ///
    /// The trap's error, or the interrupt an `INT` with no trap is at the prompt.
    pub async fn take_signal(
        &mut self,
        signal: TrapSignal,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let TrapSignal::Signal(raw) = signal else {
            return Ok(ExecutionResult::success());
        };
        if signal.as_str() != "KILL"
            && let Some(result) = self.raise_signal_trap(signal, params).await
        {
            // What the handler does to the commands around it is done there, as in Bash:
            // `break` leaves the loop the shell is in, `exit` ends it, `return` returns
            // from the function. It was dropped, and the loop went on.
            let trap = result?;
            return Ok(if trap.is_normal_flow() {
                ExecutionResult::success()
            } else {
                trap
            });
        }
        Ok(self
            .default_signal_action(raw, params)?
            .unwrap_or_else(ExecutionResult::success))
    }

    /// Sends `signal` to the shell this is a copy of (a job, a subshell, a substitution),
    /// which runs in the same process: `kill -USR1 $$` from a copy. The shell takes it after
    /// the pipeline it runs, and in `wait` ([`Self::take_signals_sent`]); one that ends it,
    /// by the traps the copy was given, ends its wait for a foreground command at once.
    /// Whether it is at a prompt the copy knows from its own `interactive`, or from running
    /// as a job of a shell with job control, whose copies are not interactive.
    pub fn send_to_shell(&self, signal: crate::sys::signal::Signal, params: &ExecutionParameters) {
        let at_prompt = self.options().interactive || params.background;
        let ends_it = !self.traps.handles(TrapSignal::Signal(signal))
            && crate::traps::default_ends_shell(signal, at_prompt);
        crate::traps::send_to_shell(signal, ends_it);
    }

    /// Acts on the signals copies of the shell sent its process id (`( kill -USR1 $$ ) &`,
    /// [`Self::send_to_shell`]), oldest first, in the shell itself: after the
    /// pipeline it ran, and in `wait`, which they end. A copy takes none.
    ///
    /// `None` where none was for the shell to act on, as an ignored one is not; otherwise
    /// 128 + the signal's number once its trap has run, which `wait` returns, or what the
    /// trap or the signal's default did to the commands around, such as an `exit`.
    ///
    /// # Errors
    ///
    /// A trap's error, or the interrupt an `INT` with no trap is at the prompt.
    pub async fn take_signals_sent(
        &mut self,
        params: &ExecutionParameters,
    ) -> Result<Option<ExecutionResult>, error::Error> {
        if self.is_subshell() || !crate::traps::signals_sent_to_shell() {
            return Ok(None);
        }
        let mut outcome = None;
        for raw in crate::traps::take_signals_sent_to_shell() {
            let signal = TrapSignal::Signal(raw);
            let ignored = self
                .traps
                .get_handler(signal)
                .is_some_and(|handler| handler.command.is_empty());
            if ignored {
                continue;
            }
            if signal.as_str() != "KILL"
                && let Some(result) = self.raise_signal_trap(signal, params).await
            {
                let trap = result?;
                if !trap.is_normal_flow() {
                    return Ok(Some(trap));
                }
                outcome = Some(ExecutionResult::new(signal_status(raw)));
            } else if let Some(result) = self.default_signal_action(raw, params)? {
                return Ok(Some(result));
            }
        }
        Ok(outcome)
    }

    /// What a signal no trap takes does to the shell that was sent it: `None` where it
    /// leaves the shell running. `KILL` ends the shell. A script ends with 128 + the
    /// signal's number; an interactive shell ignores `TERM`, abandons the line on `INT` as
    /// Ctrl-C does, and exits on the others. `QUIT` (which Bash ignores in all cases),
    /// those that stop a process and those POSIX ignores (`CHLD`, `CONT`, `URG`, `WINCH`)
    /// leave it running.
    fn default_signal_action(
        &self,
        raw: crate::sys::signal::Signal,
        params: &ExecutionParameters,
    ) -> Result<Option<ExecutionResult>, error::Error> {
        let exit = ExecutionResult {
            exit_code: crate::ExecutionExitCode::from(signal_status(raw)),
            next_control_flow: crate::ExecutionControlFlow::ExitShell,
        };
        let name = TrapSignal::Signal(raw).as_str();
        if name == "KILL" {
            return Ok(Some(exit));
        }
        if raw.leaves_the_shell_running() || name == "QUIT" {
            return Ok(None);
        }
        match name {
            _ if !self.options().interactive => Ok(Some(exit)),
            "INT" => {
                writeln!(params.stderr(self))?;
                Err(error::ErrorKind::Interrupted.into())
            }
            "TERM" => Ok(None),
            _ => Ok(Some(exit)),
        }
    }

    /// What the keyboard's interrupt does to the shell, once the shell has decided that
    /// it is its to act on: a Ctrl-C typed at a `read`, one that arrived while the shell
    /// ran commands of its own, a foreground program that died of one.
    ///
    /// With a trap on `INT`, the trap runs and its result is returned: the shell goes on,
    /// unless the trap itself ends it or returns. Without one this is the error
    /// [`error::ErrorKind::Interrupted`], which ends a script with status 130, its `EXIT`
    /// trap run, and at the prompt abandons the command line.
    pub async fn interrupt(
        &mut self,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let signal = TrapSignal::Signal(crate::sys::signal::Signal::Int);
        match self.raise_signal_trap(signal, params).await {
            Some(trap_result) => trap_result,
            None => Err(error::ErrorKind::Interrupted.into()),
        }
    }

    /// Invokes the handler registered for `signal`, if any.
    ///
    /// Behavior varies by signal type:
    ///
    /// * **Per-signal recursion guard** — each trap guards against its own self-recursion, but
    ///   different traps *can* fire from within each other's handlers (matching bash semantics).
    ///
    /// * **Inheritance** — in functions and subshells, some traps are only inherited when the
    ///   corresponding shell option is enabled (e.g. `errtrace` / `set -E` for `ERR`, `functrace` /
    ///   `set -T` for `DEBUG`/`RETURN`).
    ///
    /// * **`$?` preservation** — `last_exit_status` is saved before and restored after the handler
    ///   runs so the trap does not clobber the status that triggered it.
    ///
    /// # Arguments
    ///
    /// * `signal`: Signal to run handler for.
    ///
    /// * `params`: Execution parameters to use for handler.
    pub(crate) async fn invoke_trap_handler(
        &mut self,
        signal: TrapSignal,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // Per-signal self-recursion guard: don't re-enter a trap that is
        // already being handled. Different traps *can* fire from each
        // other's handlers (e.g. ERR inside EXIT, EXIT inside ERR).
        if self.call_stack().is_trap_signal_active(signal) {
            return Ok(ExecutionResult::success());
        }

        // Don't fire traps that have been explicitly suppressed (e.g. DEBUG
        // during programmable completion).
        if self.call_stack().is_trap_delivery_suppressed() {
            return Ok(ExecutionResult::success());
        }

        let Some(handler) = self.traps.get_handler(signal).cloned() else {
            return Ok(ExecutionResult::success());
        };

        // A subshell runs the traps it set itself; of its parent's, which the copy
        // carries, only those passed on. Functions and sourced files need no check here:
        // `enter_function` and `source_file` set aside the traps they do not inherit, so a
        // trap one sets itself still fires.
        let inherited_from_a_parent = self.is_subshell() && handler.depth != self.depth();
        if inherited_from_a_parent && !self.is_trap_inherited_in_current_scope(signal) {
            return Ok(ExecutionResult::success());
        }

        let mut params = params.clone();
        params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

        // Preserve $? across trap handler execution so the handler doesn't
        // clobber the status that triggered it; `PIPESTATUS` too, and whether `set -e` let the
        // last failure pass, as Bash keeps them (`_run_trap_internal`): a DEBUG trap now runs
        // between a loop's body and its condition, and bats' trap, running pipelines of its
        // own, made the loop's ignored failure (`[[ … ]] && return 0`) end the test.
        let orig_last_exit_status = self.last_exit_status;
        let orig_pipeline_statuses = self.last_pipeline_statuses.clone();
        let orig_failure_ignored = self.failure_ignored_by_errexit;

        // N.B. We use manual enter/leave rather than an RAII guard because a guard
        // would need to hold `&mut Shell`, preventing the mutable borrow required by
        // `run_string()`. This is safe because `result` is captured into a variable
        // (never early-returned with `?`), so `leave_trap_handler()` always runs.
        self.enter_trap_handler(signal, Some(&handler));
        let previous_before_exit_trap = self.status_before_exit_trap;
        if signal == TrapSignal::Exit {
            self.status_before_exit_trap = Some(orig_last_exit_status);
        }

        // Bash 5.3: `$BASH_TRAPSIG` holds the running trap's number, and its previous
        // value (normally none) comes back afterwards.
        let previous_trapsig = self
            .env
            .get_str("BASH_TRAPSIG", self)
            .map(|value| value.into_owned());
        let _ = self.env.set_global(
            "BASH_TRAPSIG",
            crate::variables::ShellVariable::new(signal.trap_number().to_string()),
        );

        let result = self
            .run_string(&handler.command, &handler.source_info, &params)
            .await;

        let _ = match previous_trapsig {
            Some(value) => self
                .env
                .set_global("BASH_TRAPSIG", crate::variables::ShellVariable::new(value)),
            None => self.env.unset("BASH_TRAPSIG").map(|_| ()),
        };

        self.leave_trap_handler();
        self.status_before_exit_trap = previous_before_exit_trap;
        self.last_exit_status = orig_last_exit_status;
        self.last_pipeline_statuses = orig_pipeline_statuses;
        self.failure_ignored_by_errexit = orig_failure_ignored;

        result
    }

    /// Returns whether the given trap signal is inherited in the current
    /// function or subshell scope.
    fn is_trap_inherited_in_current_scope(&self, signal: TrapSignal) -> bool {
        match signal {
            TrapSignal::Err => self.options().shell_functions_inherit_err_trap,
            TrapSignal::Debug | TrapSignal::Return => {
                self.options()
                    .shell_functions_inherit_debug_and_return_traps
            }
            // EXIT and system signals are always inherited — i.e. their visibility is
            // not gated by errtrace/functrace options. (The actual trap *state* for
            // subshells is managed separately via `Shell::clone`.)
            TrapSignal::Exit | TrapSignal::Signal(_) => true,
        }
    }
}
