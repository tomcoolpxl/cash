use cash_core::{ExecutionExitCode, ExecutionResult, builtins, history};
use clap::Parser;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

/// Query or manipulate the shell's command history.
// TODO(history): Evaluate which of the options conflict with each other.
#[derive(Parser)]
#[expect(clippy::option_option)]
pub(crate) struct HistoryCommand {
    /// Clears all history.
    #[arg(short = 'c')]
    clear_history: bool,

    /// Deletes the history entry at the given offset, or the entries from one offset to
    /// another (`START-END`). Positive offsets are relative to the beginning of the
    /// history, while negative offsets are relative to the end of the history.
    #[arg(short = 'd', value_name = "OFFSET", allow_hyphen_values = true)]
    delete_offset: Option<String>,

    /// Appends the history from the current session to the history file.
    #[arg(short = 'a', group = "anrw", num_args = 0..=1, value_name = "HIST_FILE")]
    append_session_to_file: Option<Option<String>>,

    /// Appends any remaining history from the history file to the current session.
    #[arg(short = 'n', group = "anrw", num_args = 0..=1, value_name = "HIST_FILE")]
    append_rest_of_file_to_session: Option<Option<String>>,

    /// Appends the history from the history file to the current session.
    #[arg(short = 'r', group = "anrw", num_args = 0..=1, value_name = "HIST_FILE")]
    append_file_to_session: Option<Option<String>>,

    /// Replaces the history file with the current session history.
    #[arg(short = 'w', group = "anrw", num_args = 0..=1, value_name = "HIST_FILE")]
    write_session_to_file: Option<Option<String>>,

    /// History-expands positional arguments and displays them.
    #[arg(short = 'p', num_args = 0.., value_name = "ARG")]
    expand_args: Option<Vec<String>>,

    /// Appends positional arguments as an entry in the current session.
    #[arg(short = 's', num_args = 0.., value_name = "ARG")]
    append_args_to_session: Option<Vec<String>>,

    /// Arguments.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

struct HistoryConfig {
    /// The file each of `-a`, `-n`, `-r` and `-w` names, or `HISTFILE`, resolved as the
    /// shell resolves a path (`$HOME/x` in its Unix spelling, a relative one from the
    /// shell's directory); none for `/dev/null`. They were opened as written, which a
    /// script's `history -w "$HOME/.hist"` missed.
    append_file: Option<PathBuf>,
    read_new_file: Option<PathBuf>,
    read_file: Option<PathBuf>,
    write_file: Option<PathBuf>,
    time_format: Option<String>,
    /// The zone `TZ` names, which the times are shown in.
    zone: cash_core::timefmt::Zone,
}

impl builtins::Command for HistoryCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        // An option it does not know is said, as in Bash, history or not: `history -z`
        // did nothing, status 0.
        if let Some(first) = self.args.first()
            && first.len() > 1
            && first.starts_with('-')
            && first != "--"
        {
            let option = first.get(..2).unwrap_or(first);
            writeln!(
                context.error_stream(),
                "{}: {option}: invalid option",
                context.command_name
            )?;
            writeln!(
                context.error_stream(),
                "{}: usage: history [-c] [-d offset] [n] or history -anrw [filename] or \
                 history -ps arg [arg...]",
                context.command_name
            )?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        // Retrieve the shell's history config while we still can.
        let default_file = context.shell.history_file_path();
        let file = |option: &Option<Option<String>>| {
            option.as_ref().and_then(|given| {
                get_effective_history_file_path(default_file.as_deref(), given.as_deref())
                    .map(|path| context.shell.absolute_path(path))
            })
        };
        let config = HistoryConfig {
            append_file: file(&self.append_session_to_file),
            read_new_file: file(&self.append_rest_of_file_to_session),
            read_file: file(&self.append_file_to_session),
            write_file: file(&self.write_session_to_file),
            time_format: context.shell.history_time_format(),
            zone: cash_core::timefmt::Zone::of_shell(context.shell),
        };

        let stdout = context.stdout();
        let stderr = context.error_stream();

        // The list is there with history off too, as in Bash: `history -s foo` adds to it
        // in a script, and `history` lists it. It did nothing, status 0.
        let history = context.shell.history_list_mut();
        self.execute_with_history(history, &config, stdout, stderr)
    }
}

impl HistoryCommand {
    fn execute_with_history(
        &self,
        history: &mut history::History,
        config: &HistoryConfig,
        stdout: impl Write,
        mut stderr: impl Write,
    ) -> Result<ExecutionResult, cash_core::Error> {
        if self.clear_history {
            history.clear()?;
        }

        if let Some(spec) = &self.delete_offset {
            // An offset, or a range of them (`2-3`, `-2--1`), as Bash takes them; a
            // position outside the history and what is no number are said, status 1. A
            // range was refused, and an offset of 0 was a usage error.
            let count = history.count();
            let (start, end) = match delete_range(spec, count) {
                Ok(range) => range,
                Err(message) => {
                    writeln!(stderr, "history: {message}")?;
                    return Ok(ExecutionResult::general_error());
                }
            };
            for index in (start..=end).rev() {
                let _ = history.remove_nth_item(index);
            }

            return Ok(ExecutionResult::success());
        }

        if self.append_session_to_file.is_some() {
            if let Some(file_path) = &config.append_file {
                history.flush(
                    file_path,
                    true,                         /* append? */
                    true,                         /* unsaved items only */
                    config.time_format.is_some(), /* write timestamps? */
                )?;
            }

            return Ok(ExecutionResult::success());
        }

        if self.append_rest_of_file_to_session.is_some() {
            if let Some(file_path) = &config.read_new_file {
                history.read_file(file_path, true)?;
            }
            return Ok(ExecutionResult::success());
        }

        if self.append_file_to_session.is_some() {
            if let Some(file_path) = &config.read_file {
                history.read_file(file_path, false)?;
            }
            return Ok(ExecutionResult::success());
        }

        if self.write_session_to_file.is_some() {
            if let Some(file_path) = &config.write_file {
                history.flush(
                    file_path,
                    false,                        /* append? */
                    false,                        /* unsaved items only? */
                    config.time_format.is_some(), /* write timestamps? */
                )?;
            }

            return Ok(ExecutionResult::success());
        }

        if let Some(args) = &self.expand_args {
            let mut stdout = stdout;
            for arg in args {
                // A failure is said as Bash says it for `-p`.
                let Ok(expanded) = history::expand_history(arg, Some(history)) else {
                    writeln!(stderr, "history: {arg}: history expansion failed")?;
                    return Ok(ExecutionResult::general_error());
                };
                writeln!(stdout, "{}", expanded.line)?;
            }
            return Ok(ExecutionResult::success());
        }

        if let Some(args) = &self.append_args_to_session {
            history.add(history::Item::new(args.join(" ")))?;
            return Ok(ExecutionResult::success());
        }

        // One count at most, as Bash takes it.
        if self.args.len() > 1 {
            writeln!(stderr, "history: too many arguments")?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }
        let max_entries: Option<usize> = if let Some(arg) = self.args.first() {
            Some(cash_core::int_utils::parse(arg.as_str(), 10)?)
        } else {
            None
        };

        display_history(history, config, max_entries, stdout, stderr)?;

        Ok(ExecutionResult::success())
    }
}

fn display_history(
    history: &history::History,
    config: &HistoryConfig,
    max_entries: Option<usize>,
    mut stdout: impl Write,
    _stderr: impl Write,
) -> Result<(), cash_core::Error> {
    let item_count = history.count();
    // A count above the entries there are lists them all; it panicked (`history 10`).
    let skip_count = item_count.saturating_sub(max_entries.unwrap_or(item_count));

    for (i, item) in history.iter().skip(skip_count).enumerate() {
        // A specifier chrono does not know panicked here.
        let formatted_timestamp = match (item.timestamp, &config.time_format) {
            (Some(timestamp), Some(time_format)) => config
                .zone
                .format(timestamp.with_timezone(&chrono::Utc), time_format),
            _ => String::new(),
        };

        // Output format is something like:
        //     1  echo hello world
        std::writeln!(
            stdout,
            "{:>5}  {formatted_timestamp}{}",
            skip_count + i + 1,
            item.command_line
        )?;
    }

    Ok(())
}

/// The entries `history -d` deletes, 0-based and inclusive, of `count`: one offset or
/// `START-END`, a negative offset counting back from the end (`-1` is the last), or what
/// Bash says of the spec.
fn delete_range(spec: &str, count: usize) -> Result<(usize, usize), String> {
    let position = |text: &str| -> Result<usize, String> {
        let number: i64 = text
            .trim()
            .parse()
            .map_err(|_| format!("{text}: invalid number"))?;
        let count = i64::try_from(count).unwrap_or(i64::MAX);
        let one_based = if number < 0 {
            count + 1 + number
        } else {
            number
        };
        if (1..=count).contains(&one_based) {
            usize::try_from(one_based - 1).map_err(|_| format!("{text}: invalid number"))
        } else {
            Err(format!("{text}: history position out of range"))
        }
    };
    // The `-` between two offsets is one after the first character, which may be the
    // first offset's sign.
    let split = spec
        .char_indices()
        .skip(1)
        .find(|&(_, c)| c == '-')
        .map(|(at, _)| at);
    match split.map(|at| (spec.get(..at), spec.get(at + 1..))) {
        Some((Some(first), Some(last))) if !last.is_empty() => {
            let (start, end) = (position(first)?, position(last)?);
            if start > end {
                return Err(format!("{first}: history position out of range"));
            }
            Ok((start, end))
        }
        _ => position(spec).map(|index| (index, index)),
    }
}

/// The file `-a`, `-n`, `-r` or `-w` names, or `HISTFILE`; none for `/dev/null`, which
/// takes the lines and gives none, as in Bash: it was looked for as `C:/dev/null`.
fn get_effective_history_file_path<'a>(
    default_history_file_path: Option<&'a Path>,
    option: Option<&'a str>,
) -> Option<&'a Path> {
    option
        .map(Path::new)
        .or(default_history_file_path)
        .filter(|path| {
            !matches!(
                cash_win32::path::accept(&path.to_string_lossy()),
                cash_win32::path::Target::Null
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use pretty_assertions::{assert_eq, assert_matches};

    #[test]
    fn test_parse_dash_a() -> Result<()> {
        let cmd = HistoryCommand::try_parse_from(["history", "5"])?;
        assert_matches!(cmd.append_session_to_file, None);

        let cmd = HistoryCommand::try_parse_from(["history", "-a"])?;
        assert_matches!(cmd.append_session_to_file, Some(None));

        let cmd = HistoryCommand::try_parse_from(["history", "-a", "token"])?;
        assert_eq!(
            cmd.append_session_to_file,
            Some(Some(String::from("token")))
        );

        Ok(())
    }
}
