//! Call stack management for the shell.

use crate::traps::TrapSignal;
use crate::{ExecutionParameters, callstack, env, error, functions, trace_categories};

impl<SE: crate::extensions::ShellExtensions> crate::Shell<SE> {
    /// Returns whether or not the shell is actively executing in a sourced script.
    pub fn in_sourced_script(&self) -> bool {
        self.call_stack.in_sourced_script()
    }

    /// Returns whether or not the shell is actively executing in a shell function.
    pub fn in_function(&self) -> bool {
        self.call_stack.in_function()
    }

    /// Updates the shell's internal tracking state to reflect that a new interactive
    /// session is being started.
    pub fn start_interactive_session(&mut self) -> Result<(), error::Error> {
        // What `BASH_SOURCE` reports for a function defined here: `main` at the prompt,
        // and in Bash 5.3 the shell's `$0` for commands read from standard input (5.2
        // said `main` for both).
        let name = if self.options().interactive {
            "main".to_owned()
        } else {
            self.source_name_for_input()
        };
        self.call_stack.push_interactive_session(&name);
        Ok(())
    }

    /// `$0`, which Bash 5.3 reports as the source of what a `-c` string or standard
    /// input defines.
    fn source_name_for_input(&self) -> String {
        self.current_shell_name()
            .map_or_else(|| "main".to_owned(), std::borrow::Cow::into_owned)
    }

    /// Updates the shell's internal tracking state to reflect that the current
    /// interactive session is ending.
    pub fn end_interactive_session(&mut self) -> Result<(), error::Error> {
        if self
            .call_stack
            .current_frame()
            .is_none_or(|frame| !frame.frame_type.is_interactive_session())
        {
            return Err(error::ErrorKind::NotInInteractiveSession.into());
        }

        self.call_stack.pop();

        Ok(())
    }

    /// Updates the shell's internal tracking state to reflect that command
    /// string mode is being started.
    pub fn start_command_string_mode(&mut self) {
        // Bash 5.3 reports `$0` as the source of a function a `-c` string defines; 5.2
        // said `environment`.
        let name = self.source_name_for_input();
        self.call_stack.push_command_string(&name);
    }

    /// Updates the shell's internal tracking state to reflect that command
    /// string mode is ending.
    pub fn end_command_string_mode(&mut self) -> Result<(), error::Error> {
        if self
            .call_stack
            .current_frame()
            .is_none_or(|frame| !frame.frame_type.is_command_string())
        {
            return Err(error::ErrorKind::NotExecutingCommandString.into());
        }

        self.call_stack.pop();

        Ok(())
    }

    pub(crate) fn enter_trap_handler(
        &mut self,
        signal: crate::traps::TrapSignal,
        handler: Option<&crate::traps::TrapHandler>,
    ) {
        self.call_stack.push_trap_handler(signal, handler);
    }

    pub(crate) fn leave_trap_handler(&mut self) {
        self.call_stack.pop();
    }

    /// Acquires a block on trap delivery, preventing traps from being delivered until
    /// the block is released. Multiple blocks may be acquired, and trap delivery will
    /// remain suppressed until all blocks have been released.
    pub(crate) const fn acquire_trap_delivery_block(&mut self) {
        self.call_stack.acquire_trap_delivery_block();
    }

    /// Releases a block on trap delivery; note that trap delivery will remain
    /// suppressed until all blocks have been released.
    pub(crate) const fn release_trap_delivery_block(&mut self) {
        self.call_stack.release_trap_delivery_block();
    }

    /// Updates the shell's internal tracking state to reflect that a new shell
    /// function is being entered.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function being entered.
    /// * `function` - The function being entered.
    /// * `args` - The arguments being passed to the function.
    /// * `_params` - Current execution parameters.
    pub(crate) fn enter_function(
        &mut self,
        name: &str,
        function: &functions::Registration,
        args: impl IntoIterator<Item = String>,
        _params: &ExecutionParameters,
    ) -> Result<(), error::Error> {
        let funcnest = self.env_str("FUNCNEST").and_then(|v| v.parse::<i64>().ok());

        let max_call_depth = match funcnest {
            Some(n) if n > 0 => usize::try_from(n).ok(),
            Some(_) => None,
            None => self.options.max_function_call_depth.or(Some(500)),
        };

        if let Some(max_call_depth) = max_call_depth
            && self.call_stack.function_call_depth() >= max_call_depth
        {
            return Err(error::ErrorKind::MaxFunctionCallDepthExceeded.into());
        }

        if tracing::enabled!(target: trace_categories::FUNCTIONS, tracing::Level::DEBUG) {
            let depth = self.call_stack.function_call_depth();
            let prefix = repeated_char_str(' ', depth);
            tracing::debug!(target: trace_categories::FUNCTIONS, "Entering func [depth={depth}]: {prefix}{name}");
        }

        self.call_stack.push_function(name, function, args);
        self.env.push_scope(env::EnvironmentScope::Local);
        self.local_option_snapshots.push(None);
        let set_aside = self.set_aside_uninherited_traps(function.is_traced());
        self.function_trap_stash.push(set_aside);

        Ok(())
    }

    /// Removes the traps a function does not inherit, returning them for
    /// `leave_function` to restore.
    ///
    /// This is Bash's model: `ERR` is set aside unless `errtrace` is on, and `DEBUG` and
    /// `RETURN` unless `functrace` is on or the function has the trace attribute
    /// (`RETURN` always while a `DEBUG` trap is running). A trap the function sets
    /// itself therefore fires inside it, and outlives it.
    fn set_aside_uninherited_traps(
        &mut self,
        traced: bool,
    ) -> Vec<(TrapSignal, crate::traps::TrapHandler)> {
        let trace = traced || self.options.shell_functions_inherit_debug_and_return_traps;
        let in_debug_trap = self.call_stack.is_trap_signal_active(TrapSignal::Debug);
        let mut set_aside = Vec::new();
        for (signal, keep) in [
            (TrapSignal::Debug, trace),
            (
                TrapSignal::Err,
                self.options.shell_functions_inherit_err_trap,
            ),
            (TrapSignal::Return, trace && !in_debug_trap),
        ] {
            if !keep && let Some(handler) = self.traps.get_handler(signal).cloned() {
                self.traps.remove_handlers(signal);
                set_aside.push((signal, handler));
            }
        }
        set_aside
    }

    /// Updates the shell's internal tracking state to reflect that the shell
    /// has exited the top-most function on its call stack.
    pub(crate) fn leave_function(&mut self) -> Result<(), error::Error> {
        // Restore the traps set aside on entry, unless the function set its own.
        for (signal, handler) in self.function_trap_stash.pop().unwrap_or_default() {
            if !self.traps.handles(signal) {
                self.traps
                    .register_handler(signal, handler.command, handler.source_info);
            }
        }

        self.env.pop_scope(env::EnvironmentScope::Local)?;
        if let Some(Some(saved)) = self.local_option_snapshots.pop() {
            for kind in [
                crate::namedoptions::ShellOptionKind::Set,
                crate::namedoptions::ShellOptionKind::SetO,
            ] {
                for option in crate::namedoptions::options(kind).iter() {
                    option
                        .definition
                        .set(&mut self.options, option.definition.get(&saved));
                }
            }
        }

        if let Some(exited_call) = self.call_stack.pop() {
            if let callstack::FrameType::Function(func_call) = exited_call.frame_type {
                if tracing::enabled!(target: trace_categories::FUNCTIONS, tracing::Level::DEBUG) {
                    let depth = self.call_stack.function_call_depth();
                    let prefix = repeated_char_str(' ', depth);
                    tracing::debug!(target: trace_categories::FUNCTIONS, "Exiting func  [depth={depth}]: {prefix}{}", func_call.function_name);
                }
            } else {
                let err: error::Error =
                    error::ErrorKind::InternalError("mismatched call stack state".to_owned())
                        .into();
                return Err(err.into_fatal());
            }
        }

        Ok(())
    }

    /// Save the current `set` options for restoration when this function returns.
    pub fn save_local_options(&mut self) {
        if let Some(slot) = self.local_option_snapshots.last_mut() {
            *slot = Some(self.options.clone());
        }
    }

    /// Whether `local -` has saved an option snapshot in the current function.
    pub fn has_saved_local_options(&self) -> bool {
        self.local_option_snapshots
            .last()
            .is_some_and(Option::is_some)
    }

    /// Returns the *current* positional arguments for the shell ($1 and beyond).
    /// Influenced by the current call stack.
    pub fn current_shell_args(&self) -> &[String] {
        for frame in self.call_stack.iter() {
            if frame.shadows_positional_args {
                return &frame.args;
            }
        }

        self.args.as_slice()
    }

    /// Returns a mutable reference to *current* positional parameters for the shell
    /// ($1 and beyond).
    pub fn current_shell_args_mut(&mut self) -> &mut Vec<String> {
        for frame in self.call_stack.iter_mut() {
            if frame.shadows_positional_args {
                return &mut frame.args;
            }
        }

        &mut self.args
    }

    /// Record a `set --` mutation in the active positional-parameter scope.
    ///
    /// Bash deliberately does not mark `shift` this way. The distinction controls
    /// whether arguments temporarily supplied to a sourced script persist afterward.
    pub fn mark_current_shell_args_set(&mut self) {
        if let Some(frame) = self
            .call_stack
            .iter_mut()
            .find(|frame| frame.shadows_positional_args)
        {
            frame.positional_args_changed = true;
        }
    }
}

fn repeated_char_str(c: char, count: usize) -> String {
    (0..count).map(|_| c).collect()
}
