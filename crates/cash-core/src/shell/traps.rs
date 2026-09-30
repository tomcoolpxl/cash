//! Trap handling for the shell.

use crate::{ExecutionParameters, ExecutionResult, ProcessGroupPolicy, error, traps::TrapSignal};

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

        // A sourced file is also a nested DEBUG scope in Bash: the caller's DEBUG
        // trap is temporarily hidden unless functrace (`set -T`) is enabled.
        // Functions need no check here: `enter_function` sets aside the traps they
        // do not inherit, so a trap the function sets itself still fires.
        let inheritance_required =
            self.is_subshell() || (self.in_sourced_script() && matches!(signal, TrapSignal::Debug));
        if inheritance_required && !self.is_trap_inherited_in_current_scope(signal) {
            return Ok(ExecutionResult::success());
        }

        let Some(handler) = self.traps.get_handler(signal).cloned() else {
            return Ok(ExecutionResult::success());
        };

        let mut params = params.clone();
        params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

        // Preserve $? across trap handler execution so the handler doesn't
        // clobber the status that triggered it.
        let orig_last_exit_status = self.last_exit_status;

        // N.B. We use manual enter/leave rather than an RAII guard because a guard
        // would need to hold `&mut Shell`, preventing the mutable borrow required by
        // `run_string()`. This is safe because `result` is captured into a variable
        // (never early-returned with `?`), so `leave_trap_handler()` always runs.
        self.enter_trap_handler(signal, Some(&handler));

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
        self.last_exit_status = orig_last_exit_status;

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
