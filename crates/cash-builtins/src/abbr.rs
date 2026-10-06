//! `abbr`, fish's: define words that expand in place at the prompt (spec D60).
//!
//! `abbr -a gco git checkout` makes `gco` turn into `git checkout` on the line when Space
//! or Enter follows it as the command word, so the full command is what runs and what
//! history keeps. The options are fish's, so a line copied from `config.fish` works:
//! `-a`/`--add` (the default when names are given), `-e`/`--erase`, `-s`/`--show` (the
//! default with no arguments), `-l`/`--list`, `-q`/`--query`, `-r`/`--rename`, and
//! `--position command|anywhere`. `-g` and `-U`, fish's old scope flags, are accepted and
//! ignored, as current fish does. `--regex`, `--function` and `--set-cursor` are refused.
//!
//! Abbreviations are kept across sessions, as fish keeps its: `-a`, `-e` and `-r` also
//! write `%APPDATA%\cash\abbreviations` (`cash_core::abbreviations::store`), which an
//! interactive shell reads after its rc files. A change that cannot be written is said,
//! and the abbreviation still stands for the session.

use std::io::Write;

use cash_core::{
    ExecutionResult,
    abbreviations::{self, Abbreviation, Abbreviations, Position, RenameError, store},
    builtins,
};

/// Manage fish-style abbreviations.
#[derive(clap::Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct AbbrCommand {
    /// Options and operands, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Add,
    Erase,
    Show,
    List,
    Query,
    Rename,
}

const USAGE: &str = "\
Usage: abbr [-a] [--position command|anywhere] NAME EXPANSION...
       abbr -e NAME...
       abbr [-s]
       abbr -l
       abbr -q NAME...
       abbr -r OLD NEW

Abbreviations expand in place at the prompt when Space or Enter follows them.";

impl builtins::Command for AbbrCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let (mode, position, operands) = match parse(&self.args) {
            Ok(parsed) => parsed,
            Err(Stop::Help) => {
                writeln!(context.stdout(), "{USAGE}")?;
                return Ok(ExecutionResult::success());
            }
            Err(Stop::Unsupported(option)) => {
                writeln!(context.stderr(), "abbr: {option} is not supported by cash")?;
                return Ok(ExecutionResult::new(2));
            }
            Err(Stop::Usage(message)) => return usage_error(&context, &message),
        };

        let status = match mode {
            Mode::Add => add(&mut context, position, &operands)?,
            Mode::Erase => erase(&mut context, &operands)?,
            Mode::Query => {
                let abbreviations = context.shell.abbreviations();
                if operands
                    .iter()
                    .any(|name| abbreviations.get(name).is_some())
                {
                    ExecutionResult::success()
                } else {
                    ExecutionResult::general_error()
                }
            }
            Mode::Rename => rename(&mut context, &operands)?,
            Mode::List | Mode::Show => {
                if !operands.is_empty() {
                    return usage_error(&context, "abbr -l and -s take no names");
                }
                let mut stdout = context.stdout();
                for abbreviation in context.shell.abbreviations().iter() {
                    if mode == Mode::List {
                        writeln!(stdout, "{}", abbreviation.name)?;
                    } else {
                        writeln!(stdout, "{}", show(abbreviation))?;
                    }
                }
                ExecutionResult::success()
            }
        };
        Ok(status)
    }
}

/// Why the options did not describe something to do.
enum Stop {
    Help,
    Unsupported(String),
    Usage(String),
}

/// The mode, the position for `-a`, and the operands.
fn parse(args: &[String]) -> Result<(Mode, Position, Vec<&str>), Stop> {
    let mut mode = None;
    let mut position = Position::Command;
    let mut operands: Vec<&str> = Vec::new();

    let position_value = |value: Option<&str>| {
        value
            .and_then(parse_position)
            .ok_or_else(|| Stop::Usage("--position is command or anywhere".into()))
    };

    let mut args = args.iter().map(String::as_str);
    let mut only_operands = false;
    while let Some(arg) = args.next() {
        if only_operands || !arg.starts_with('-') || arg == "-" {
            operands.push(arg);
            continue;
        }
        if let Some(value) = arg.strip_prefix("--position=") {
            position = position_value(Some(value))?;
            continue;
        }
        let chosen = match arg {
            "--" => {
                only_operands = true;
                continue;
            }
            "-a" | "--add" => Mode::Add,
            "-e" | "--erase" => Mode::Erase,
            "-s" | "--show" => Mode::Show,
            "-l" | "--list" => Mode::List,
            "-q" | "--query" => Mode::Query,
            "-r" | "--rename" => Mode::Rename,
            "-g" | "--global" | "-U" | "--universal" => continue,
            "-h" | "--help" => return Err(Stop::Help),
            "-p" | "--position" => {
                position = position_value(args.next())?;
                continue;
            }
            "--regex" | "-f" | "--function" | "--set-cursor" | "-c" | "--command" => {
                return Err(Stop::Unsupported(arg.to_owned()));
            }
            other => return Err(Stop::Usage(format!("unknown option '{other}'"))),
        };
        if mode.is_some_and(|m| m != chosen) {
            return Err(Stop::Usage(
                "only one of -a, -e, -s, -l, -q, -r may be given".into(),
            ));
        }
        mode = Some(chosen);
    }

    let mode = mode.unwrap_or(if operands.is_empty() {
        Mode::Show
    } else {
        Mode::Add
    });
    Ok((mode, position, operands))
}

fn add<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    position: Position,
    operands: &[&str],
) -> Result<ExecutionResult, cash_core::Error> {
    let [name, expansion @ ..] = operands else {
        return usage_error(context, "abbr -a needs a name and an expansion");
    };
    if expansion.is_empty() {
        return usage_error(context, "abbr -a needs a name and an expansion");
    }
    if !abbreviations::is_valid_name(name) {
        writeln!(
            context.stderr(),
            "abbr: '{name}' cannot be a name: it must be one word"
        )?;
        return Ok(ExecutionResult::new(2));
    }
    let abbreviation = Abbreviation {
        name: (*name).to_owned(),
        expansion: expansion.join(" "),
        position,
    };
    context.shell.abbreviations_mut().set(abbreviation.clone());
    save(context, |kept| kept.set(abbreviation))?;
    Ok(ExecutionResult::success())
}

/// Applies `change` to the file of kept abbreviations as well; a file that cannot be
/// written is said, and the session's change stands.
fn save<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    change: impl FnOnce(&mut Abbreviations),
) -> Result<(), cash_core::Error> {
    match store::save_change(change) {
        Ok(()) | Err(store::StoreError::NoPlace) => Ok(()),
        Err(error) => {
            writeln!(
                context.stderr(),
                "abbr: not kept for later sessions: {error}"
            )?;
            Ok(())
        }
    }
}

fn erase<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    operands: &[&str],
) -> Result<ExecutionResult, cash_core::Error> {
    if operands.is_empty() {
        return usage_error(context, "abbr -e needs a name");
    }
    let mut status = ExecutionResult::success();
    for name in operands {
        if !context.shell.abbreviations_mut().remove(name) {
            writeln!(context.stderr(), "abbr: no such abbreviation '{name}'")?;
            status = ExecutionResult::general_error();
        }
    }
    save(context, |kept| {
        for name in operands {
            kept.remove(name);
        }
    })?;
    Ok(status)
}

fn rename<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    operands: &[&str],
) -> Result<ExecutionResult, cash_core::Error> {
    let [old, new] = operands else {
        return usage_error(context, "abbr -r needs an old and a new name");
    };
    if !abbreviations::is_valid_name(new) {
        writeln!(
            context.stderr(),
            "abbr: '{new}' cannot be a name: it must be one word"
        )?;
        return Ok(ExecutionResult::new(2));
    }
    match context.shell.abbreviations_mut().rename(old, new) {
        Ok(()) => {
            save(context, |kept| {
                let _ = kept.rename(old, new);
            })?;
            Ok(ExecutionResult::success())
        }
        Err(RenameError::OldMissing) => {
            writeln!(context.stderr(), "abbr: no such abbreviation '{old}'")?;
            Ok(ExecutionResult::general_error())
        }
        Err(RenameError::NewExists) => {
            writeln!(context.stderr(), "abbr: '{new}' already exists")?;
            Ok(ExecutionResult::general_error())
        }
    }
}

fn parse_position(value: &str) -> Option<Position> {
    match value {
        "command" => Some(Position::Command),
        "anywhere" => Some(Position::Anywhere),
        _ => None,
    }
}

/// The `abbr` line that defines `abbreviation`, as `abbr --show` prints it.
fn show(abbreviation: &Abbreviation) -> String {
    let position = match abbreviation.position {
        Position::Command => "",
        Position::Anywhere => "--position anywhere ",
    };
    let plain = |s: &str| {
        s.chars()
            .all(|c| c.is_alphanumeric() || "_.,:/+%@=-".contains(c))
    };
    let name = if plain(&abbreviation.name) {
        abbreviation.name.clone()
    } else {
        cash_core::escape::single_quote(&abbreviation.name).into_owned()
    };
    format!(
        "abbr -a {position}-- {name} {}",
        cash_core::escape::single_quote(&abbreviation.expansion)
    )
}

fn usage_error<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    message: &str,
) -> Result<ExecutionResult, cash_core::Error> {
    writeln!(context.stderr(), "abbr: {message}\n{USAGE}")?;
    Ok(ExecutionResult::new(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_prints_a_line_that_defines_it_again() {
        let abbreviation = Abbreviation {
            name: "gco".into(),
            expansion: "git checkout".into(),
            position: Position::Command,
        };
        assert_eq!(show(&abbreviation), "abbr -a -- gco 'git checkout'");

        let anywhere = Abbreviation {
            name: "L".into(),
            expansion: "| less".into(),
            position: Position::Anywhere,
        };
        assert_eq!(show(&anywhere), "abbr -a --position anywhere -- L '| less'");

        let quoted = Abbreviation {
            name: "it's".into(),
            expansion: "echo it's".into(),
            position: Position::Command,
        };
        assert_eq!(show(&quoted), r"abbr -a -- 'it'\''s' 'echo it'\''s'");
    }
}
