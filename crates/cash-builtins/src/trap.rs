use clap::Parser;
use std::io::Write;

use cash_core::traps::TrapSignal;
use cash_core::{ExecutionResult, builtins};

/// Manage signal traps.
#[derive(Parser)]
pub(crate) struct TrapCommand {
    /// List all signal names.
    #[arg(short = 'l')]
    list_signals: bool,

    /// Print registered trap commands.
    #[arg(short = 'p')]
    print_trap_commands: bool,

    /// Print registered trap actions.
    #[arg(short = 'P')]
    print_trap_actions: bool,

    args: Vec<String>,
}

impl builtins::Command for TrapCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.print_trap_actions {
            if self.print_trap_commands {
                writeln!(
                    context.error_stream(),
                    "trap: cannot specify both -p and -P"
                )?;
                return Ok(ExecutionResult::new(2));
            }
            if self.args.is_empty() {
                writeln!(context.error_stream(), "trap: -P: signal argument required")?;
                return Ok(ExecutionResult::new(2));
            }
            for signal_type in &self.args {
                let sig: TrapSignal = signal_type.parse()?;
                if let Some(handler) = context.shell.traps().get_handler(sig) {
                    writeln!(context.stdout(), "{}", handler.command)?;
                }
            }
            return Ok(ExecutionResult::success());
        }

        if self.list_signals {
            cash_core::traps::format_signals(context.stdout(), TrapSignal::iterator())
                .map(|()| ExecutionResult::success())
        } else if self.print_trap_commands || self.args.is_empty() {
            if !self.args.is_empty() {
                for signal_type in &self.args {
                    Self::display_handlers_for(&context, signal_type.parse()?)?;
                }
            } else {
                Self::display_all_handlers(&context)?;
            }
            Ok(ExecutionResult::success())
        } else if self.args.len() == 1 {
            // When only a single argument is given, it is assumed to be a signal name
            // and an indication to remove the handlers for that signal.
            let signal = self.args[0].as_str();
            Self::remove_all_handlers(&mut context, signal.parse()?);
            Ok(ExecutionResult::success())
        } else if self.args[0] == "-" {
            // "-" as the first argument indicates that the remaining
            // arguments are signal names and we need to remove the handlers for them.
            for signal in &self.args[1..] {
                Self::remove_all_handlers(&mut context, signal.parse()?);
            }
            Ok(ExecutionResult::success())
        } else {
            let handler = &self.args[0];

            let mut signal_types = Vec::with_capacity(self.args.len() - 1);
            for signal in &self.args[1..] {
                signal_types.push(signal.parse()?);
            }

            Self::register_handler(&mut context, signal_types, handler.as_str());
            Ok(ExecutionResult::success())
        }
    }
}

impl TrapCommand {
    fn display_all_handlers(
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    ) -> Result<(), cash_core::Error> {
        let mut signals: Vec<TrapSignal> = context
            .shell
            .traps()
            .iter_handlers()
            .map(|(signal, _)| signal)
            .collect();
        signals.sort_by_key(|&signal| listing_order(signal));
        for signal in signals {
            Self::display_handlers_for(context, signal)?;
        }
        Ok(())
    }

    /// Prints a trap as Bash does, as a command that sets it again: the command
    /// single-quoted, a signal with its `SIG` prefix.
    fn display_handlers_for(
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        signal_type: TrapSignal,
    ) -> Result<(), cash_core::Error> {
        if let Some(handler) = context.shell.traps().get_handler(signal_type) {
            let prefix = if matches!(signal_type, TrapSignal::Signal(_)) {
                "SIG"
            } else {
                ""
            };
            writeln!(
                context.stdout(),
                "trap -- {} {prefix}{signal_type}",
                cash_core::escape::single_quote(&handler.command)
            )?;
        }
        Ok(())
    }

    fn remove_all_handlers(
        context: &mut cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        signal: TrapSignal,
    ) {
        context.shell.traps_mut().remove_handlers(signal);
    }

    fn register_handler<I>(
        context: &mut cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        signals: I,
        handler: &str,
    ) where
        I: IntoIterator<Item = TrapSignal>,
    {
        // Our new source context is relative to the current position.
        // TODO(source-info): Provide the location of the specific token that makes up
        // `self.args[0]`.
        let source_info = context.shell.call_stack().current_pos_as_source_info();
        let depth = context.shell.depth();

        for signal in signals {
            context.shell.traps_mut().register_handler(
                signal,
                handler.to_owned(),
                source_info.clone(),
                depth,
            );
        }
    }
}

/// Where `trap -p` lists a trap, as Bash numbers them: EXIT as signal 0, the signals by
/// number, then DEBUG, ERR and RETURN after the last signal.
const fn listing_order(signal: TrapSignal) -> i32 {
    match signal {
        TrapSignal::Exit => 0,
        TrapSignal::Signal(signal) => signal.number(),
        TrapSignal::Debug => 1000,
        TrapSignal::Err => 1001,
        TrapSignal::Return => 1002,
    }
}
