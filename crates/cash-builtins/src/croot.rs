//! `croot [-d|-f] [DIR]`: the file and folder picker Alt-E opens, for scripts (spec D73).
//!
//! It draws on the console itself (`CONOUT$`), not on standard output, so that
//! `cd "$(croot)"` shows the picker while the pick is captured.

use std::io::Write as _;
use std::path::PathBuf;

use cash_core::{ExecutionExitCode, ExecutionResult, builtins};
use cash_picker::colours::Colours;
use cash_picker::context::{Shows, Then};
use cash_picker::{term, ui};
use clap::Parser;

/// Pick a file or folder in a tree, and print it.
#[derive(Parser)]
pub(crate) struct CrootCommand {
    /// Folders only (the default).
    #[arg(short = 'd', conflicts_with = "files")]
    folders: bool,

    /// Files and folders.
    #[arg(short = 'f')]
    files: bool,

    /// The folder to start in; the current folder without one.
    dir: Option<String>,
}

impl builtins::Command for CrootCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let Ok(mut console) = std::fs::OpenOptions::new().write(true).open("CONOUT$") else {
            writeln!(context.stderr(), "croot: no terminal to draw on")?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        };
        let root = match &self.dir {
            Some(dir) => {
                let path = context.shell.absolute_path(dir);
                if !path.is_dir() {
                    writeln!(context.stderr(), "croot: {dir}: not a folder")?;
                    return Ok(ExecutionResult::general_error());
                }
                path
            }
            None => context.shell.working_dir().to_path_buf(),
        };
        let var = |name: &str| context.shell.env_str(name).map(|value| value.into_owned());
        let home = var("HOME")
            .filter(|home| !home.is_empty())
            .map(|home| cash_win32::path::accept_path(&home));
        let colour = var("NO_COLOR").is_none_or(|value| value.is_empty());
        let mut picker = ui::Picker::new(ui::Setup {
            root,
            typed: String::new(),
            shows: if self.files {
                Shows::Everything
            } else {
                Shows::Folders
            },
            then: Then::Close,
            history: context
                .shell
                .directory_history()
                .back()
                .iter()
                .rev()
                .cloned()
                .collect(),
            home,
            colours: if colour {
                Colours::new(
                    var("LS_COLORS").as_deref(),
                    var("CASH_PICKER_COLORS").as_deref(),
                )
            } else {
                Colours::none()
            },
        });

        let mut picks: Vec<PathBuf> = Vec::new();
        let height = var("CASH_PICKER_HEIGHT");
        if let Err(error) = term::run(
            &mut picker,
            &mut console,
            height.as_deref(),
            |path, _, _| {
                picks.push(path.to_path_buf());
            },
        ) {
            writeln!(context.stderr(), "croot: {error}")?;
            return Ok(ExecutionResult::general_error());
        }
        if picks.is_empty() {
            return Ok(ExecutionResult::general_error());
        }
        let mut stdout = context.stdout();
        for path in picks {
            writeln!(stdout, "{}", cash_win32::path::render(&path))?;
        }
        Ok(ExecutionResult::success())
    }
}
