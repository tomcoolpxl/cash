use std::io::{Read, Write};

use clap::Parser;

use cash_core::{ErrorKind, ExecutionExitCode, ExecutionResult, builtins, env, escape, variables};

/// Read lines from standard input into an indexed array variable.
#[derive(Parser)]
pub(crate) struct MapFileCommand {
    /// Delimiter to use (defaults to newline).
    #[arg(short = 'd')]
    delimiter: Option<String>,

    /// Maximum number of entries to read (0 means no limit).
    #[arg(short = 'n', default_value_t = 0)]
    max_count: i64,

    /// Index into array at which to start assignment.
    #[arg(short = 'O', allow_hyphen_values = true)]
    origin: Option<i64>,

    /// Number of initial entries to skip.
    #[arg(short = 's', default_value_t = 0, value_parser = clap::value_parser!(i64).range(0..))]
    skip_count: i64,

    /// Whether or not to remove the delimiter from each read line.
    #[arg(short = 't')]
    remove_delimiter: bool,

    /// File descriptor to read from (defaults to stdin).
    #[arg(short = 'u', default_value_t = 0)]
    fd: cash_core::ShellFd,

    /// Name of function to call for each group of lines.
    #[arg(short = 'C')]
    callback: Option<String>,

    /// Number of lines to pass the callback for each group.
    #[arg(short = 'c', default_value_t = 5000, value_parser = clap::value_parser!(i64).range(1..))]
    callback_group_size: i64,

    /// Name of array to read into.
    #[arg(default_value = "MAPFILE")]
    array_var_name: String,
}

impl builtins::Command for MapFileCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        if let Some(origin) = self.origin {
            if origin < 0 {
                writeln!(
                    context.stderr(),
                    "{}: {origin}: invalid array origin",
                    context.command_name
                )?;
                return Ok(ExecutionExitCode::GeneralError.into());
            }
        }

        if let Some((_, var)) = context.shell.env().get(&self.array_var_name) {
            if var.value().is_associative_array() {
                writeln!(
                    context.stderr(),
                    "{}: {}: not an indexed array",
                    context.command_name,
                    self.array_var_name
                )?;
                return Ok(ExecutionExitCode::GeneralError.into());
            }
        }

        let input_file = context
            .try_fd(self.fd)
            .ok_or_else(|| ErrorKind::BadFileDescriptor(self.fd))?;

        if self.origin.is_none() {
            context.shell.env_mut().update_or_add(
                &self.array_var_name,
                variables::ShellValueLiteral::Array(variables::ArrayLiteral(Vec::new())),
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;
        }

        self.read_entries(input_file, context).await?;

        Ok(ExecutionResult::success())
    }
}

impl MapFileCommand {
    async fn read_entries<SE: cash_core::ShellExtensions>(
        &self,
        mut input_file: cash_core::openfiles::OpenFile,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<(), cash_core::Error> {
        let _term_mode = setup_terminal_settings(&input_file)?;

        let mut stored = 0usize;
        let mut read_count = 0;
        let max_count = self.max_count.try_into()?;
        let delimiter = match &self.delimiter {
            Some(d) if d.is_empty() => b'\0',
            Some(d) => d.as_bytes().first().copied().unwrap_or(b'\n'),
            None => b'\n',
        };

        let mut buf = [0u8; 1];

        while max_count == 0 || stored < max_count {
            let mut line = vec![];
            let mut saw_delimiter = false;

            loop {
                match input_file.read(&mut buf) {
                    Ok(0) => break,                                         // End of input
                    Ok(1) if buf[0] == b'\x03' => break,                    // Ctrl+C
                    Ok(1) if buf[0] == b'\x04' && line.is_empty() => break, // Ctrl+D
                    Ok(1) => {
                        let byte = buf[0];
                        line.push(byte);
                        if byte == delimiter {
                            saw_delimiter = true;
                            break;
                        }
                    }
                    Ok(_) => unreachable!("input can only be 0, 1, or error"),
                    Err(e) => return Err(e.into()),
                }
            }

            if line.is_empty() && !saw_delimiter {
                break;
            }

            if read_count < self.skip_count {
                read_count += 1;
                continue;
            }

            if self.remove_delimiter && line.ends_with(&[delimiter]) {
                line.pop();
            }

            let line_str = String::from_utf8_lossy(&line).to_string();

            let index = i64::try_from(stored)?.saturating_add(self.origin.unwrap_or(0));
            if let Some(callback) = &self.callback
                && (stored + 1).is_multiple_of(usize::try_from(self.callback_group_size)?)
            {
                let script = std::format!("{callback} {index} {}", escape::single_quote(&line_str));
                let source = cash_core::SourceInfo::from("mapfile callback");
                context
                    .shell
                    .run_string(script, &source, &context.params)
                    .await?;
            }
            context.shell.env_mut().update_or_add_array_element(
                &self.array_var_name,
                index.to_string(),
                line_str,
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;
            stored += 1;
        }

        Ok(())
    }
}

fn setup_terminal_settings(
    file: &cash_core::openfiles::OpenFile,
) -> Result<Option<cash_core::terminal::AutoModeGuard>, cash_core::Error> {
    let mode = cash_core::terminal::AutoModeGuard::new(file.to_owned()).ok();
    if let Some(mode) = &mode {
        let config = cash_core::terminal::Settings::builder()
            .line_input(false)
            .interrupt_signals(false)
            .build();

        mode.apply_settings(&config)?;
    }

    Ok(mode)
}
