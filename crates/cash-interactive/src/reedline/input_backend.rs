use cash_core::trace_categories;
use nu_ansi_term::Style;
use reedline::MenuBuilder;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::{completer, edit_mode, highlighter, history, validator};
use crate::{InputBackend, ReadResult, ShellError, input_backend::InteractivePrompt, refs};

/// Represents an interactive shell capable of taking commands from standard input
/// and reporting results to standard output and standard error streams.
pub struct ReedlineInputBackend {
    reedline: Option<reedline::Reedline>,
    bindings: Arc<Mutex<edit_mode::UpdatableBindings>>,
    /// Colour is on: the F1 help is drawn in the prompt's colours.
    colour: bool,
}

const COMPLETION_MENU_NAME: &str = "completion_menu";

/// How many times `reedline.read_line()` is attempted before its error is
/// propagated: the initial call plus this many minus one retries. See
/// `read_line` below.
const MAX_READ_LINE_ATTEMPTS: u32 = 3;

fn completion_menu_text_style() -> Style {
    Style::new()
}

fn completion_menu_selected_text_style() -> Style {
    Style::new().bold().reverse()
}

fn completion_menu_match_text_style() -> Style {
    Style::new().underline()
}

fn completion_menu_selected_match_text_style() -> Style {
    completion_menu_selected_text_style().underline()
}

fn history_hint_style() -> Style {
    Style::new().italic().dimmed()
}

impl ReedlineInputBackend {
    /// Returns a new interactive shell instance, created with the provided options.
    ///
    /// # Arguments
    ///
    /// * `options` - Options for creating the input backend.
    /// * `shell_ref` - Shell that the backend will be used with.
    pub fn new(
        options: &crate::UIOptions,
        shell_ref: &refs::ShellRef<impl cash_core::ShellExtensions>,
    ) -> Result<Self, ShellError> {
        // Set up key bindings.
        let key_bindings = compose_key_bindings(COMPLETION_MENU_NAME);

        // Set up mutable edit mode, with history as the source of `yank-last-arg`'s words.
        let history_shell = shell_ref.clone();
        let history_words: edit_mode::HistoryWords = Box::new(move |back, nth| {
            let shell = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(history_shell.lock())
            });
            let history = shell.history()?;
            let index = history.count().checked_sub(back + 1)?;
            let line = history.get(index)?.command_line.clone();
            drop(shell);
            crate::history_words::pick(&line, nth)
        });
        let mutable_edit_mode = edit_mode::MutableEditMode::new(key_bindings, history_words);
        let updatable_bindings = mutable_edit_mode.bindings();

        // Create helper objects that implement reedline traits; each will
        // hold a reference to the shell.
        let completer = completer::ReedlineCompleter {
            shell: shell_ref.clone(),
            carapace: crate::carapace::Cache::default(),
        };
        let validator = validator::ReedlineValidator {
            shell: shell_ref.clone(),
        };
        let highlighter = highlighter::ReedlineHighlighter {
            shell: shell_ref.clone(),
            syntax: !options.disable_color && !options.disable_highlighting,
        };
        let history = history::ReedlineHistory {
            shell: shell_ref.clone(),
        };

        // Set up completion menu. Set an empty marker to avoid the
        // line's text horizontally shifting around during/after completion.
        // We set a max column count of 10 to ensure it's larger than the
        // hard-coded default (4 last we checked); if there's not enough
        // horizontal space in the terminal to fit that many columns, given
        // the actual text to be displayed, it will get effectively dereased
        // anyhow.
        let completion_menu = Box::new(super::menu::QuoteAwareMenu(
            reedline::ColumnarMenu::default()
                .with_name(COMPLETION_MENU_NAME)
                // The whole line, so completing inside `'my dir/in|'` sees the closing quote
                // after the cursor and replaces it (D40).
                .with_input_mode(reedline::InputMode::FullBuffer)
                .with_marker("")
                .with_columns(10)
                .with_text_style(completion_menu_text_style())
                .with_match_text_style(completion_menu_match_text_style())
                .with_selected_text_style(completion_menu_selected_text_style())
                .with_selected_match_text_style(completion_menu_selected_match_text_style()),
        ));

        // Set up default history-based hinter.
        let mut hinter = reedline::DefaultHinter::default();
        if !options.disable_color {
            hinter = hinter.with_style(history_hint_style());
        }

        // Instantiate reedline with some defaults and hand it ownership of
        // the helpers.
        // Cash's highlighter replaces Reedline's example one, which hard-codes white as the
        // neutral input color. It is installed even without syntax colours, when it paints
        // the terminal's default color, because it also decides where abbreviations expand.
        let reedline = reedline::Reedline::create()
            .with_ansi_colors(!options.disable_color)
            .use_bracketed_paste(!options.disable_bracketed_paste)
            .with_completer(Box::new(completer))
            .with_quick_completions(true)
            // As Bash does, a Tab first inserts what all the candidates share; the next
            // shows them (D40). With nothing shared left to insert, the first shows them.
            .with_partial_completions(true)
            .with_shared_prefix_first(true)
            .with_validator(Box::new(validator))
            .with_hinter(Box::new(hinter))
            .with_highlighter(Box::new(highlighter))
            .with_menu(reedline::ReedlineMenu::EngineCompleter(completion_menu))
            .with_edit_mode(Box::new(mutable_edit_mode))
            .with_history(Box::new(history));

        let mut shell = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(shell_ref.lock())
        });

        shell.set_key_bindings(Some(updatable_bindings.clone()));
        drop(shell);

        Ok(Self {
            reedline: Some(reedline),
            bindings: updatable_bindings,
            colour: !options.disable_color,
        })
    }
}

impl Drop for ReedlineInputBackend {
    fn drop(&mut self) {
        // It's unpleasant to need to do so, but if we detect a panic in the process of being
        // unwound, then we arrange for our reedline::Reedline instance to *not* get dropped.
        // Without this, then there's a chance that our panic handler emitted important
        // diagnostics to stdout but dropping the Reedline object will end up erasing it
        // when the latter object's internal Painter gets dropped and, in turn, may flush
        // some not-yet-flushed terminal control sequences. This isn't theoretical; we've
        // actively seen this in various cases where a panic occurs with Reedline::read_line()
        // on the stack.
        if std::thread::panicking() {
            let reedline = std::mem::take(&mut self.reedline);
            std::mem::forget(reedline);
        }
    }
}

impl InputBackend for ReedlineInputBackend {
    /// Reads a line of input, using the given prompt.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The prompt to display to the user.
    fn read_line(
        &mut self,
        shell: &crate::ShellRef<impl cash_core::ShellExtensions>,
        prompt: InteractivePrompt,
    ) -> Result<ReadResult, ShellError> {
        // Hand Reedline the abbreviations as they stand (D60). Its table can only be added
        // to, and an entry overwritten by name; one `abbr -e` removed stays in it, and the
        // highlighter's `should_expand_abbr` refuses it.
        let (abbreviations, vi): (std::collections::HashMap<String, String>, bool) = {
            let shell = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(shell.lock())
            });
            let abbreviations = shell
                .abbreviations()
                .iter()
                .map(|a| (a.name.clone(), a.expansion.clone()))
                .collect();
            (abbreviations, shell.options().vi_mode)
        };
        // `set -o vi` and `set -o emacs` take effect at the next prompt, as in Bash.
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.bindings.lock())
        })
        .set_vi(vi);
        if !abbreviations.is_empty() {
            self.reedline = self
                .reedline
                .take()
                .map(|reedline| reedline.with_abbreviations(abbreviations));
        }

        // The prompt an entered line keeps (D61). Reedline has no way to clear a transient
        // prompt once set, so without `CASH_TRANSIENT_PS1` it gets the prompt itself, which
        // is what it would redraw anyway.
        let after_entry = prompt.after_entry();
        self.reedline = self
            .reedline
            .take()
            .map(|reedline| reedline.with_transient_prompt(Box::new(after_entry)));

        let Some(reedline) = &mut self.reedline else {
            return Ok(ReadResult::Eof);
        };

        let mut attempt: u32 = 1;
        loop {
            match reedline.read_line(&prompt) {
                Ok(reedline::Signal::Success(s)) => {
                    // A line croot set to run (D73) was accepted; the next is read again.
                    reedline.set_immediately_accept(false);
                    return Ok(ReadResult::Input(s));
                }
                Ok(reedline::Signal::CtrlC) => return Ok(ReadResult::Interrupted),
                Ok(reedline::Signal::CtrlD) => return Ok(ReadResult::Eof),
                Ok(reedline::Signal::ExternalBreak(_)) => {
                    return Err(ShellError::UnexpectedInputFailure);
                }
                Ok(reedline::Signal::HostCommand(command)) => {
                    // Alt-← and Alt-→ (D62): on an empty line they change folder, and the
                    // prompt is drawn afresh in the new one; on any other they move a word,
                    // and reading simply resumes, without recomposing the prompt.
                    // Alt-E (D73): croot below the line, which stays on screen; its pick
                    // goes back on the line, and a `cd` runs at once.
                    if command == edit_mode::PICKER {
                        super::picker::on_key(reedline, shell);
                        continue;
                    }
                    // F1 (D75): the help on the alternate screen; the main screen comes
                    // back as it was, and the next read redraws the line in place.
                    if command == edit_mode::HELP {
                        super::help::on_key(shell, self.colour);
                        continue;
                    }

                    let mut command = command;
                    if let Some((folder_command, word_move)) =
                        edit_mode::folder_history_key(&command)
                    {
                        if !reedline.current_buffer_contents().is_empty() {
                            reedline.run_edit_commands(&[word_move]);
                            continue;
                        }
                        folder_command.clone_into(&mut command);
                    }

                    // As Bash does for `bind -x`, clear the line before the command runs:
                    // its output starts where the prompt was, and the prompt is redrawn
                    // after it. The patched Reedline (vendor/reedline) redraws in place
                    // only when the cursor is back on the cell it left, which a cleared
                    // line no longer is, so the prompt goes below any output.
                    let _ = crossterm::execute!(
                        std::io::stdout(),
                        crossterm::cursor::MoveToColumn(0),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown)
                    );
                    let numeric_argument = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current()
                            .block_on(async { self.bindings.lock().await.take_numeric_argument() })
                    });
                    return Ok(ReadResult::BoundCommand {
                        command,
                        numeric_argument,
                    });
                }
                Ok(_) => return Err(ShellError::UnexpectedInputFailure),
                // An error here is almost always transient. The prevalent case:
                // reedline asks the terminal for the cursor position (DSR,
                // `ESC [ 6 n`) before painting a prompt, and again after an
                // external program (a `bind -x` command such as atuin's search
                // UI, fzf, ...) hands the terminal back. crossterm waits a fixed
                // 2s for the reply and then fails; a terminal busy repainting or
                // a multiplexer briefly holding the reply is enough to trip it,
                // and giving up would end the whole interactive session. That
                // failure happens before any input is read, so re-issuing the
                // read is safe; retry a bounded number of times before treating
                // the failure as real. A terminal that never answers therefore
                // fails after MAX_READ_LINE_ATTEMPTS x 2s rather than 2s.
                //
                // The one known exception: reedline restores the terminal mode
                // *after* computing its result, so if `disable_raw_mode` itself
                // fails, a line that was already submitted is lost and the retry
                // prompts afresh. That is a tcsetattr failure on a tty that just
                // worked; the alternative -- exiting the shell -- loses the same
                // line and everything else with it.
                Err(err) if attempt < MAX_READ_LINE_ATTEMPTS => {
                    attempt += 1;
                    tracing::debug!(
                        target: trace_categories::INPUT,
                        "reedline read_line failed; retrying (attempt {attempt}/{MAX_READ_LINE_ATTEMPTS}): {err}"
                    );
                }
                Err(err) => return Err(ShellError::InputError(err)),
            }
        }
    }

    fn get_read_buffer(&self) -> Option<(String, usize)> {
        self.reedline.as_ref().map(|r| {
            (
                r.current_buffer_contents().to_owned(),
                r.current_insertion_point(),
            )
        })
    }

    fn set_read_buffer(&mut self, buffer: String, cursor: usize) {
        if let Some(reedline) = &mut self.reedline {
            replace_buffer(reedline, buffer, cursor);
        }
    }
}

/// Puts `buffer` in place of the line being edited, with the cursor at `cursor`.
///
/// The whole buffer: it cleared from the start to the end of the first line, so the
/// lines after it stayed when a `bind -x` command or Ctrl-X Ctrl-E set `READLINE_LINE`
/// for a buffer of several lines (PI-08).
fn replace_buffer(reedline: &mut reedline::Reedline, buffer: String, cursor: usize) {
    reedline.run_edit_commands(&[
        reedline::EditCommand::Clear,
        reedline::EditCommand::InsertString(buffer),
        reedline::EditCommand::MoveToPosition {
            position: cursor,
            select: false,
        },
    ]);
}

fn compose_key_bindings(completion_menu_name: &str) -> reedline::Keybindings {
    let mut key_bindings = reedline::default_emacs_keybindings();

    // Wire up tab to completion.
    key_bindings.add_binding(
        reedline::KeyModifiers::NONE,
        reedline::KeyCode::Tab,
        reedline::ReedlineEvent::UntilFound(vec![
            reedline::ReedlineEvent::Menu(completion_menu_name.to_string()),
            reedline::ReedlineEvent::MenuNext,
            reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::Complete]),
        ]),
    );
    // Alt-E: croot, the file and folder picker (D73), as `cash-picker`.
    key_bindings.add_binding(
        reedline::KeyModifiers::ALT,
        reedline::KeyCode::Char('e'),
        reedline::ReedlineEvent::ExecuteHostCommand(edit_mode::PICKER.to_owned()),
    );
    // F1: the one-screen help (D75), as `cash-help`.
    key_bindings.add_binding(
        reedline::KeyModifiers::NONE,
        reedline::KeyCode::F(1),
        reedline::ReedlineEvent::ExecuteHostCommand(edit_mode::HELP.to_owned()),
    );
    // Wire up shift-tab for completion.
    key_bindings.add_binding(
        reedline::KeyModifiers::SHIFT,
        reedline::KeyCode::BackTab,
        reedline::ReedlineEvent::MenuPrevious,
    );

    // Add undo.
    // NOTE: To match readline, we bind Ctrl+_ to undo; in practice, the only way
    // to get that to work out is to specify Ctrl+7 for the binding. It's not clear
    // that this is terribly portable across terminals/environments.
    key_bindings.add_binding(
        reedline::KeyModifiers::CONTROL,
        reedline::KeyCode::Char('7'),
        reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::Undo]),
    );

    // fish's prevd-or-backward-word and nextd-or-forward-word (D62); the input backend
    // decides between the two by whether the line is empty.
    for (key, marker) in [
        (reedline::KeyCode::Left, edit_mode::PREVD_OR_BACKWARD_WORD),
        (reedline::KeyCode::Right, edit_mode::NEXTD_OR_FORWARD_WORD),
    ] {
        key_bindings.add_binding(
            reedline::KeyModifiers::ALT,
            key,
            reedline::ReedlineEvent::ExecuteHostCommand(marker.to_owned()),
        );
    }

    // Readline's yank-last-arg, on both of its default keys. `edit_mode` turns the marker
    // into an edit; see `edit_mode::YANK_LAST_ARG`. Alt-_ arrives with Shift on some
    // keyboards and without it on others.
    for (modifiers, key) in [
        (reedline::KeyModifiers::ALT, '.'),
        (reedline::KeyModifiers::ALT, '_'),
        (
            reedline::KeyModifiers::ALT | reedline::KeyModifiers::SHIFT,
            '_',
        ),
    ] {
        key_bindings.add_binding(
            modifiers,
            reedline::KeyCode::Char(key),
            reedline::ReedlineEvent::ExecuteHostCommand(edit_mode::YANK_LAST_ARG.to_owned()),
        );
    }

    // Capitalize.
    key_bindings.add_binding(
        reedline::KeyModifiers::ALT,
        reedline::KeyCode::Char('c'),
        reedline::ReedlineEvent::Edit(vec![
            reedline::EditCommand::CapitalizeChar,
            reedline::EditCommand::MoveWordRight { select: false },
        ]),
    );

    // Add comment.
    key_bindings.add_binding(
        reedline::KeyModifiers::ALT,
        reedline::KeyCode::Char('#'),
        reedline::ReedlineEvent::Multiple(vec![
            reedline::ReedlineEvent::Edit(vec![
                reedline::EditCommand::MoveToStart { select: false },
                reedline::EditCommand::InsertChar('#'),
            ]),
            reedline::ReedlineEvent::Enter,
        ]),
    );

    key_bindings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_buffer_replaces_every_line_of_the_old() {
        let mut reedline = reedline::Reedline::create();
        replace_buffer(&mut reedline, "one\ntwo\nthree".to_owned(), 13);
        replace_buffer(&mut reedline, "new".to_owned(), 3);
        assert_eq!(reedline.current_buffer_contents(), "new");
        assert_eq!(reedline.current_insertion_point(), 3);
    }

    #[test]
    fn history_hint_style_is_theme_adaptive() {
        let style = history_hint_style();

        assert_eq!(style.foreground, None);
        assert_eq!(style.background, None);
        assert!(style.is_italic);
        assert!(style.is_dimmed);
    }

    #[test]
    fn completion_menu_styles_are_theme_adaptive() {
        let text_style = completion_menu_text_style();
        let match_style = completion_menu_match_text_style();
        let selected_text_style = completion_menu_selected_text_style();
        let selected_match_text_style = completion_menu_selected_match_text_style();

        assert_eq!(text_style.foreground, None);
        assert_eq!(text_style.background, None);

        assert_eq!(match_style.foreground, None);
        assert_eq!(match_style.background, None);
        assert!(match_style.is_underline);

        assert_eq!(selected_text_style.foreground, None);
        assert_eq!(selected_text_style.background, None);
        assert!(selected_text_style.is_bold);
        assert!(selected_text_style.is_reverse);

        assert_eq!(selected_match_text_style.foreground, None);
        assert_eq!(selected_match_text_style.background, None);
        assert!(selected_match_text_style.is_bold);
        assert!(selected_match_text_style.is_reverse);
        assert!(selected_match_text_style.is_underline);
    }
}
