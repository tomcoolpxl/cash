//! `z [-l|-i] [WORDS...]`: the folder jump (spec D80), zoxide's `z` on the record of the
//! folders an interactive cash has been in (`cash_core::kept::folders`).
//!
//! `z` alone, `z -` and `z FOLDER` are `cd`'s; any other words go to the best-ranked
//! folder whose path holds them in turn, the last in its last part. `-l` lists the
//! matches with their scores; `-i` opens a list to pick from on the terminal, as `croot`
//! draws on it.

use std::io::Write as _;
use std::path::Path;

use cash_core::{ExecutionExitCode, ExecutionResult, builtins, kept};
use cash_picker::colours::Colours;
use cash_picker::list::{Line, List, Scope, Setup};
use cash_picker::term;
use clap::Parser as _;

/// Jump to a folder you often go to.
#[derive(clap::Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct ZCommand {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

const USAGE: &str = "Usage: z [-l|-i] [WORDS...]";

/// What `z --help` prints.
const HELP: &str = "\
Usage: z [-l|-i] [WORDS...]
Go to the best-ranked recorded folder whose path holds WORDS in turn.

  -l, --list         list the matches with their scores, best first
  -i, --interactive  pick one of the matches from a list
  -h, --help         show this help

With no WORDS, z goes home; `z -` goes back, and `z FOLDER` is `cd FOLDER`.";

/// What the words ask for.
#[derive(PartialEq, Eq)]
enum Mode {
    Jump,
    List,
    Pick,
}

impl builtins::Command for ZCommand {
    type Error = cash_core::Error;

    fn new<I: IntoIterator<Item = String>>(args: I) -> Result<Self, clap::Error> {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut mode = Mode::Jump;
        let mut words: Vec<&str> = Vec::new();
        let mut options = true;
        for arg in &self.args {
            match arg.as_str() {
                "--" if options => options = false,
                "-l" | "--list" if options => mode = Mode::List,
                "-i" | "--interactive" if options => mode = Mode::Pick,
                "-h" | "--help" if options => {
                    writeln!(context.stdout(), "{HELP}")?;
                    return Ok(ExecutionResult::success());
                }
                option if options && option.len() > 1 && option.starts_with('-') => {
                    writeln!(context.stderr(), "z: {option}: unknown option\n{USAGE}")?;
                    return Ok(ExecutionExitCode::InvalidUsage.into());
                }
                word => words.push(word),
            }
        }

        // `z`, `z -` and `z FOLDER` are `cd`'s, as zoxide has them.
        if mode == Mode::Jump {
            let is_folder = |word: &str| {
                context
                    .shell
                    .absolute_path(cash_win32::path::accept_path(word))
                    .is_dir()
            };
            let cd_word = match words.as_slice() {
                [] => Some(None),
                [word] if *word == "-" || is_folder(word) => Some(Some(*word)),
                _ => None,
            };
            if let Some(word) = cd_word {
                let cd = crate::cd::CdCommand::try_parse_from(std::iter::once("cd").chain(word))
                    .map_err(|error| cash_core::Error::from(std::io::Error::other(error)))?;
                return builtins::Command::execute(&cd, context).await;
            }
        }

        let Some(record) = context.shell.folder_record() else {
            writeln!(
                context.stderr(),
                "z: no folders are recorded: CASH_NO_RECORDS is set, or LOCALAPPDATA is not"
            )?;
            return Ok(ExecutionResult::general_error());
        };
        let here = cash_win32::path::render(context.shell.working_dir());
        let found = kept::folders::find(&record, &words, kept::now(), Some(&here));
        if found.is_empty() {
            let what = if words.is_empty() {
                "no folder is recorded yet".to_owned()
            } else {
                format!("no folder matches '{}'", words.join(" "))
            };
            writeln!(context.stderr(), "z: {what}")?;
            return Ok(ExecutionResult::general_error());
        }

        match mode {
            Mode::List => {
                let mut stdout = context.stdout();
                for (folder, score) in &found {
                    writeln!(stdout, "{score:>6.1} {}", folder.path)?;
                }
                Ok(ExecutionResult::success())
            }
            Mode::Pick => match pick(&context, &found)? {
                Ok(Some(path)) => go(&mut context, &path),
                Ok(None) => Ok(ExecutionResult::general_error()),
                Err(result) => Ok(result),
            },
            Mode::Jump => {
                let best = found[0].0.path.clone();
                go(&mut context, &best)
            }
        }
    }
}

/// Changes to `dir` as `cd` would, reporting a failure as `z`'s.
fn go<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    dir: &str,
) -> Result<ExecutionResult, cash_core::Error> {
    match context.shell.set_working_dir(Path::new(dir)) {
        Ok(()) => Ok(ExecutionResult::success()),
        Err(error) => {
            let error = error.worded();
            writeln!(context.stderr(), "z: {dir}: {error}")?;
            Ok(ExecutionResult::general_error())
        }
    }
}

/// The folder picked from `found` in a list drawn on the terminal: `Ok(None)` when the
/// list closed without a pick, `Err` with the status to return when it could not be
/// drawn.
fn pick<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    found: &[(kept::folders::Folder, f64)],
) -> Result<Result<Option<String>, ExecutionResult>, cash_core::Error> {
    let Ok(mut console) = std::fs::OpenOptions::new().write(true).open("CONOUT$") else {
        writeln!(context.stderr(), "z: no terminal to draw on")?;
        return Ok(Err(ExecutionExitCode::InvalidUsage.into()));
    };
    let var = |name: &str| context.shell.env_str(name).map(|value| value.into_owned());
    let colour = var("NO_COLOR").is_none_or(|value| value.is_empty());
    let lines = found
        .iter()
        .map(|(folder, _)| Line {
            text: context.shell.tilde_shorten(folder.path.clone()),
            when: u64::try_from(folder.last).ok().and_then(|seconds| {
                std::time::SystemTime::UNIX_EPOCH
                    .checked_add(std::time::Duration::from_secs(seconds))
            }),
        })
        .collect();
    let mut list = List::new(Setup {
        scopes: vec![Scope {
            title: "Folders".to_owned(),
            lines,
        }],
        typed: String::new(),
        hints: "Enter go  Esc".to_owned(),
        colours: if colour {
            Colours::new(None, var("CASH_PICKER_COLORS").as_deref())
        } else {
            Colours::none()
        },
        paths: true,
    });
    let height = var("CASH_PICKER_HEIGHT");
    match term::run_list(&mut list, &mut console, height.as_deref(), &mut Vec::new()) {
        Ok(picked) => Ok(Ok(picked.and_then(|(_, index, _)| {
            found.get(index).map(|(folder, _)| folder.path.clone())
        }))),
        Err(error) => {
            writeln!(context.stderr(), "z: {error}")?;
            Ok(Err(ExecutionResult::general_error()))
        }
    }
}
