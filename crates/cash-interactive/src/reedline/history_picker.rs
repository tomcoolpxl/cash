//! Ctrl-R: the history picker, below the line it was pressed on (spec D79). Each command
//! once, at its last use, newest first; what the line held is the filter it opens with.
//! Enter puts the command picked on the line, Tab runs it; Ctrl-R in the picker shows
//! what ran in the current folder, from the record cash keeps beside the history file.

use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use cash_core::kept;
use cash_picker::colours::Colours;
use cash_picker::list::{Line, List, Scope, Setup};
use cash_picker::term;

/// What the picker needs from the shell, read while holding it briefly.
struct FromShell {
    /// Each command once, newest first.
    commands: Vec<Line>,
    /// The current folder, in cash's spelling.
    here: String,
    /// The record of the folder each command ran in, when one is kept.
    folders_file: Option<std::path::PathBuf>,
    home: Option<String>,
    picker_colors: Option<String>,
    height: Option<String>,
    colour: bool,
}

/// Seconds since the Unix epoch as a time.
fn time_of(seconds: i64) -> Option<SystemTime> {
    let seconds = u64::try_from(seconds).ok()?;
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
}

/// Ctrl-R on `reedline`'s line: the picker, then the line it leaves, set to be accepted by
/// the next read when Tab picked it.
pub(crate) fn on_key(
    reedline: &mut reedline::Reedline,
    shell: &crate::ShellRef<impl cash_core::ShellExtensions>,
) {
    let line = reedline.current_buffer_contents().to_owned();
    // Keys typed straight after Ctrl-R are the filter's; the rest go back to the line.
    let mut pending = reedline.take_pending_input();
    let picked = open(&line, shell, &mut pending);
    reedline.give_back_input(pending);
    let Some((command, run)) = picked else {
        return;
    };
    let end = command.len();
    reedline.run_edit_commands(&[
        reedline::EditCommand::Clear,
        reedline::EditCommand::InsertString(command),
        reedline::EditCommand::MoveToPosition {
            position: end,
            select: false,
        },
    ]);
    reedline.set_immediately_accept(run);
}

/// Opens the picker with `line` as its filter, reading the keys in `pending` first; the
/// command picked and whether to run it, or `None` when it closed without a pick.
fn open(
    line: &str,
    shell: &crate::ShellRef<impl cash_core::ShellExtensions>,
    pending: &mut Vec<crossterm::event::Event>,
) -> Option<(String, bool)> {
    let from_shell = {
        let shell = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(shell.lock())
        });
        let var = |name: &str| shell.env_str(name).map(|value| value.into_owned());
        let mut seen = HashSet::new();
        let commands = shell
            .history()
            .map(|history| {
                history
                    .iter()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .filter(|item| seen.insert(item.command_line.clone()))
                    .map(|item| Line {
                        text: item.command_line.clone(),
                        when: item.timestamp.and_then(|stamp| time_of(stamp.timestamp())),
                    })
                    .collect()
            })
            .unwrap_or_default();
        FromShell {
            commands,
            here: cash_win32::path::render(shell.working_dir()),
            folders_file: shell
                .records()
                .history_folders_file()
                .map(std::path::Path::to_path_buf),
            home: var("HOME")
                .filter(|home| !home.is_empty())
                .map(|home| cash_win32::path::render(&cash_win32::path::accept_path(&home))),
            picker_colors: var("CASH_PICKER_COLORS"),
            height: var("CASH_PICKER_HEIGHT"),
            colour: var("NO_COLOR").is_none_or(|value| value.is_empty()),
        }
    };

    let here = from_shell.folders_file.as_ref().map(|file| {
        let in_history: HashSet<&str> = from_shell
            .commands
            .iter()
            .map(|line| line.text.as_str())
            .collect();
        Scope {
            title: format!(
                "History in {}",
                shown(&from_shell.here, from_shell.home.as_deref())
            ),
            lines: ran_here(file, &from_shell.here, &in_history),
        }
    });
    let mut scopes = vec![Scope {
        title: "History".to_owned(),
        lines: from_shell.commands,
    }];
    scopes.extend(here);
    let mut list = List::new(Setup {
        scopes,
        typed: line.trim().to_owned(),
        hints: "Enter edit  Tab run  Ctrl-R here/all  Esc".to_owned(),
        colours: if from_shell.colour {
            Colours::new(None, from_shell.picker_colors.as_deref())
        } else {
            Colours::none()
        },
        paths: false,
    });
    let picked = term::run_list(
        &mut list,
        &mut std::io::stdout().lock(),
        from_shell.height.as_deref(),
        pending,
    );
    let (scope, index, tab) = match picked {
        Ok(picked) => picked?,
        Err(error) => {
            tracing::warn!("history picker: {error}");
            return None;
        }
    };
    let command = list.line(scope, index)?.text.clone();
    Some((command, tab))
}

/// The commands that ran in `here`, from the record at `file`: each once, at its last
/// run, newest first, and only those still `in_history`, so a command `history -d` or
/// `history -c` took out is gone from this list too.
fn ran_here(file: &std::path::Path, here: &str, in_history: &HashSet<&str>) -> Vec<Line> {
    let mut seen = HashSet::new();
    kept::commands::read(file)
        .unwrap_or_default()
        .into_iter()
        .rev()
        .filter(|ran| kept::same_folder(&ran.folder, here))
        .filter(|ran| in_history.contains(ran.command.as_str()))
        .filter(|ran| seen.insert(ran.command.clone()))
        .map(|ran| Line {
            text: ran.command,
            when: time_of(ran.time),
        })
        .collect()
}

/// A folder as a header shows it: under the home folder as `~/…`.
fn shown(folder: &str, home: Option<&str>) -> String {
    if let Some(home) = home
        && let Some(rest) = folder
            .get(..home.len())
            .filter(|head| head.eq_ignore_ascii_case(home))
            .and_then(|_| folder.get(home.len()..))
        && (rest.is_empty() || rest.starts_with('/'))
    {
        return format!("~{rest}");
    }
    folder.to_owned()
}
