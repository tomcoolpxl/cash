//! Alt-E: croot, the file and folder picker, opened below the line it was pressed on
//! (spec D73). The line says what to list and what a pick does; the pick goes back on
//! it, and for `cd`, `pushd` or an empty line the line runs at once.

use std::path::{Path, PathBuf};

use cash_picker::colours::Colours;
use cash_picker::context::{Line, Then};
use cash_picker::{pick, term, ui};

/// What the line editor does once the picker has closed.
enum After {
    /// Go on editing: the line is now `line`, the cursor at byte `cursor`.
    Edit { line: String, cursor: usize },
    /// Run `line` now.
    Run(String),
}

/// What the picker needs from the shell, read while holding it briefly.
struct FromShell {
    cwd: PathBuf,
    home: Option<PathBuf>,
    history: Vec<PathBuf>,
    ls_colors: Option<String>,
    picker_colors: Option<String>,
    height: Option<String>,
    colour: bool,
}

/// Alt-E on `reedline`'s line: the picker, then the line it leaves, set to be accepted
/// by the next read when it runs at once.
pub(crate) fn on_key(
    reedline: &mut reedline::Reedline,
    shell: &crate::ShellRef<impl cash_core::ShellExtensions>,
) {
    let line = reedline.current_buffer_contents().to_owned();
    let cursor = reedline.current_insertion_point();
    // Keys typed straight after Alt-E are the picker's; the rest go back to the line.
    let mut pending = reedline.take_pending_input();
    let picked = open(&line, cursor, shell, &mut pending);
    reedline.give_back_input(pending);
    let (new_line, new_cursor, run) = match picked {
        None => return,
        Some(After::Edit { line, cursor }) => (line, cursor, false),
        Some(After::Run(line)) => {
            let end = line.len();
            (line, end, true)
        }
    };
    reedline.run_edit_commands(&[
        reedline::EditCommand::Clear,
        reedline::EditCommand::InsertString(new_line),
        reedline::EditCommand::MoveToPosition {
            position: new_cursor,
            select: false,
        },
    ]);
    reedline.set_immediately_accept(run);
}

/// Opens the picker for `line` with the cursor at byte `cursor`, reading the keys in
/// `pending` first; `None` when it closed without a pick, which leaves the line as it was.
fn open(
    line: &str,
    cursor: usize,
    shell: &crate::ShellRef<impl cash_core::ShellExtensions>,
    pending: &mut Vec<crossterm::event::Event>,
) -> Option<After> {
    let from_shell = {
        let shell = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(shell.lock())
        });
        let var = |name: &str| shell.env_str(name).map(|value| value.into_owned());
        FromShell {
            cwd: shell.working_dir().to_path_buf(),
            home: var("HOME")
                .filter(|home| !home.is_empty())
                .map(|home| cash_win32::path::accept_path(&home)),
            history: shell
                .directory_history()
                .back()
                .iter()
                .rev()
                .cloned()
                .collect(),
            ls_colors: var("LS_COLORS"),
            picker_colors: var("CASH_PICKER_COLORS"),
            height: var("CASH_PICKER_HEIGHT"),
            colour: var("NO_COLOR").is_none_or(|value| value.is_empty()),
        }
    };

    let context = Line::read(line, cursor);
    let (root, typed) = pick::start(&context.text, &from_shell.cwd, from_shell.home.as_deref());
    let mut picker = ui::Picker::new(ui::Setup {
        root,
        typed,
        shows: context.shows(),
        then: context.then(),
        history: from_shell.history,
        home: from_shell.home.clone(),
        colours: if from_shell.colour {
            Colours::new(
                from_shell.ls_colors.as_deref(),
                from_shell.picker_colors.as_deref(),
            )
        } else {
            Colours::none()
        },
    });

    let mut picks: Vec<String> = Vec::new();
    let written = |path: &Path, folder: bool| {
        pick::written(path, folder, &from_shell.cwd, from_shell.home.as_deref())
    };
    // While the picker stays open, each pick shows on the line: drawn from the start of
    // the word it replaces, which is this many columns before the cursor.
    let echo_from: usize = line
        .get(context.word.start..cursor)
        .unwrap_or_default()
        .chars()
        .filter_map(unicode_width::UnicodeWidthChar::width)
        .sum();
    let rest = line.get(context.word.end..).unwrap_or_default();
    let ran = term::run(
        &mut picker,
        &mut std::io::stdout().lock(),
        from_shell.height.as_deref(),
        pending,
        Some(echo_from),
        |path, folder, _close| {
            picks.push(written(path, folder));
            Some(format!("{} {rest}", picks.join(" ")))
        },
    );
    if let Err(error) = ran {
        tracing::warn!("croot: {error}");
    }
    if picks.is_empty() {
        return None;
    }

    let then = context.then();
    let mut replacement = picks.join(" ");
    if context.command.is_none() {
        // An empty line, or a word alone: the pick is where to go.
        replacement = format!("cd {replacement}");
    } else if then != Then::Run {
        replacement.push(' ');
    }
    let word = context.word;
    let new_line = format!(
        "{}{replacement}{}",
        line.get(..word.start).unwrap_or_default(),
        line.get(word.end..).unwrap_or_default()
    );
    Some(if then == Then::Run {
        After::Run(new_line)
    } else {
        After::Edit {
            cursor: word.start + replacement.len(),
            line: new_line,
        }
    })
}
