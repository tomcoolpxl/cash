use cash_core::{ExecutionResult, sys};
use clap::{Parser, Subcommand};
use std::io::Write;

use crate::events;

/// Extension trait for adding cash-specific built-in commands to a shell builder.
pub(crate) trait ShellBuilderCashBuiltinExt {
    /// Add cash-specific builtins to a shell being built.
    #[must_use]
    fn cash_builtins(self) -> Self;
}

impl<SE: cash_core::extensions::ShellExtensions, S: cash_core::ShellBuilderState>
    ShellBuilderCashBuiltinExt for cash_core::ShellBuilder<SE, S>
{
    fn cash_builtins(self) -> Self {
        // cash (D9): registered under both names, as upstream did. The old `brushctl`
        // and `brushinfo` spellings are gone — D9 absorbed brush as a hard fork, and
        // these were the last two places the old name reached a user.
        self.builtin(
            "cashctl",
            cash_core::builtins::builtin::<CashCtlCommand, SE>(),
        )
        .builtin(
            "cashinfo",
            cash_core::builtins::builtin::<CashCtlCommand, SE>(),
        )
    }
}

/// Configure the running cash shell.
#[derive(Parser)]
pub(crate) struct CashCtlCommand {
    #[clap(subcommand)]
    command_group: CommandGroup,
}

#[derive(Subcommand)]
enum CommandGroup {
    #[clap(subcommand)]
    Complete(CompleteCommand),
    #[clap(subcommand)]
    Call(CallCommand),
    #[clap(subcommand)]
    Events(EventsCommand),
    #[clap(subcommand)]
    Process(ProcessCommand),
    /// Whether GUI applications started from cash (`code .`) keep running after cash
    /// exits. Prints the current setting when given no argument.
    #[clap(name = "gui-apps")]
    GuiApps {
        /// `outlive` (the default) or `close`.
        mode: Option<GuiAppsMode>,
    },
}

/// What happens to GUI applications cash started when it exits.
#[derive(Clone, Copy, clap::ValueEnum)]
enum GuiAppsMode {
    /// They keep running, as when started from PowerShell or Explorer.
    Outlive,
    /// They are closed with cash, like every console program it started.
    Close,
}

/// Commands for inspecting call state.
#[derive(Subcommand)]
enum CallCommand {
    /// Display the current call stack.
    #[clap(name = "stack")]
    ShowCallStack {
        /// Whether to show more details.
        #[clap(short = 'd', long = "detailed")]
        detailed: bool,
    },
}

/// Commands for generating completions.
#[derive(Subcommand)]
enum CompleteCommand {
    /// Generate completions for an input line.
    #[clap(name = "line")]
    Line {
        /// The 0-indexed cursor position for generation.
        #[arg(long = "cursor", short = 'c')]
        cursor_index: Option<usize>,

        /// The input line to generate completions for.
        line: String,
    },
}

/// Commands for configuring tracing events.
#[derive(Subcommand)]
enum EventsCommand {
    /// Display status of enabled events.
    Status,

    /// Enable event.
    Enable {
        /// Event to enable.
        event: events::TraceEvent,
    },

    /// Disable event.
    Disable {
        /// Event to disable.
        event: events::TraceEvent,
    },
}

/// Commands for inspecting process state.
#[expect(clippy::enum_variant_names)]
#[derive(Subcommand)]
enum ProcessCommand {
    /// Display process ID.
    #[clap(name = "pid")]
    ShowProcessId,
    /// Display process group ID.
    #[clap(name = "pgid")]
    ShowProcessGroupId,
    /// Display foreground process ID.
    #[clap(name = "fgpid")]
    ShowForegroundProcessId,
    /// Display parent process ID.
    #[clap(name = "ppid")]
    ShowParentProcessId,
}

impl cash_core::builtins::Command for CashCtlCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        match &self.command_group {
            CommandGroup::Call(call) => call.execute(&context),
            CommandGroup::Complete(complete) => complete.execute(&mut context).await,
            CommandGroup::Events(events) => events.execute(&context),
            CommandGroup::Process(process) => process.execute(&context),
            CommandGroup::GuiApps { mode } => gui_apps(&context, *mode),
        }
    }
}

fn gui_apps(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    mode: Option<GuiAppsMode>,
) -> Result<cash_core::ExecutionResult, cash_core::Error> {
    #[cfg(windows)]
    {
        match mode {
            Some(GuiAppsMode::Outlive) => cash_win32::session::set_gui_apps_outlive(true),
            Some(GuiAppsMode::Close) => cash_win32::session::set_gui_apps_outlive(false),
            None => {
                let current = if cash_win32::session::gui_apps_outlive() {
                    "outlive"
                } else {
                    "close"
                };
                writeln!(context.stdout(), "{current}")?;
            }
        }
        Ok(ExecutionResult::success())
    }
    #[cfg(not(windows))]
    {
        let _ = mode;
        writeln!(
            context.stderr(),
            "cashctl gui-apps: only meaningful on Windows"
        )?;
        Ok(ExecutionResult::general_error())
    }
}

impl CallCommand {
    fn execute(
        &self,
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    ) -> Result<cash_core::ExecutionResult, cash_core::Error> {
        match self {
            Self::ShowCallStack { detailed } => {
                let stack = context.shell.call_stack();
                let format_options = cash_core::callstack::FormatOptions {
                    show_args: *detailed,
                    show_entry_points: *detailed,
                };

                write!(context.stdout(), "{}", stack.format(&format_options))?;

                Ok(ExecutionResult::success())
            }
        }
    }
}

impl CompleteCommand {
    async fn execute(
        &self,
        context: &mut cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    ) -> Result<cash_core::ExecutionResult, cash_core::Error> {
        match self {
            Self::Line { cursor_index, line } => {
                let completions = context
                    .shell
                    .complete(line, cursor_index.unwrap_or(line.len()))
                    .await?;
                for candidate in completions.candidates {
                    writeln!(context.stdout(), "{candidate}")?;
                }
                Ok(ExecutionResult::success())
            }
        }
    }
}

impl EventsCommand {
    fn execute(
        &self,
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    ) -> Result<cash_core::ExecutionResult, cash_core::Error> {
        let event_config = crate::entry::get_event_config();

        let mut event_config = event_config.try_lock().map_err(|_| {
            cash_core::Error::from(cash_core::ErrorKind::Unimplemented(
                "Failed to acquire lock on event configuration",
            ))
        })?;

        if let Some(event_config) = event_config.as_mut() {
            match self {
                Self::Status => {
                    let enabled_events = event_config.get_enabled_events();
                    for event in enabled_events {
                        writeln!(context.stdout(), "{event}")?;
                    }
                }
                Self::Enable { event } => event_config.enable(*event)?,
                Self::Disable { event } => event_config.disable(*event)?,
            }

            Ok(cash_core::ExecutionResult::success())
        } else {
            Err(cash_core::ErrorKind::Unimplemented("event configuration not initialized").into())
        }
    }
}

impl ProcessCommand {
    fn execute(
        &self,
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    ) -> Result<cash_core::ExecutionResult, cash_core::Error> {
        match self {
            Self::ShowProcessId => {
                writeln!(context.stdout(), "{}", std::process::id())?;
                Ok(ExecutionResult::success())
            }
            Self::ShowProcessGroupId => {
                if let Some(pgid) = sys::terminal::get_process_group_id() {
                    writeln!(context.stdout(), "{pgid}")?;
                    Ok(ExecutionResult::success())
                } else {
                    writeln!(context.stderr(), "failed to get process group ID")?;
                    Ok(ExecutionResult::general_error())
                }
            }
            Self::ShowForegroundProcessId => {
                if let Some(pid) = sys::terminal::get_foreground_pid() {
                    writeln!(context.stdout(), "{pid}")?;
                    Ok(ExecutionResult::success())
                } else {
                    writeln!(context.stderr(), "failed to get foreground process ID")?;
                    Ok(ExecutionResult::general_error())
                }
            }
            Self::ShowParentProcessId => {
                if let Some(pid) = sys::terminal::get_parent_process_id() {
                    writeln!(context.stdout(), "{pid}")?;
                    Ok(ExecutionResult::success())
                } else {
                    writeln!(context.stderr(), "failed to get parent process ID")?;
                    Ok(ExecutionResult::general_error())
                }
            }
        }
    }
}
