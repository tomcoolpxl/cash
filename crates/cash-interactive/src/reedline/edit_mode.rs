use cash_core::{
    interfaces::{self, InputFunction, Key, KeyAction, KeyBindings as _, KeySequence, KeyStroke},
    trace_categories,
};
use radix_trie::Trie;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

/// What a key bound to `yank-last-arg` carries. It is never run: `parse_event` turns it into
/// an edit of the line. The NUL keeps it from equalling any real `bind -x` command.
pub(crate) const YANK_LAST_ARG: &str = "\0cash:yank-last-arg";

/// What `edit-and-execute-command` runs, as Bash's own does (bashline.c): the line goes into
/// history, then `fc` opens it in `$VISUAL` or `$EDITOR` and runs what is saved. The `fc`
/// entry added after it stands for the `fc` invocation itself, which `fc` skips and removes.
/// The editor falls back to `vi`, as `fc`'s does.
pub(crate) const EDIT_AND_EXECUTE_COMMAND: &str = r#"history -s "$READLINE_LINE"; history -s fc; READLINE_LINE=; READLINE_POINT=0; fc -e "${VISUAL:-${EDITOR:-vi}}""#;

/// What Alt-← carries: fish's `prevd-or-backward-word`. The input backend, which can see the
/// line, runs `prevd` when it is empty and moves a word left otherwise (spec D62).
pub(crate) const PREVD_OR_BACKWARD_WORD: &str = "\0cash:prevd-or-backward-word";

/// What Alt-→ carries: fish's `nextd-or-forward-word`; see [`PREVD_OR_BACKWARD_WORD`].
pub(crate) const NEXTD_OR_FORWARD_WORD: &str = "\0cash:nextd-or-forward-word";

/// For a folder-history key's marker, the command it runs on an empty line and the edit it
/// makes on any other.
pub(crate) fn folder_history_key(marker: &str) -> Option<(&'static str, reedline::EditCommand)> {
    // Quiet at either end of the history, as fish is: the key simply does nothing there.
    match marker {
        PREVD_OR_BACKWARD_WORD => Some((
            "prevd 2>/dev/null",
            reedline::EditCommand::MoveWordLeft { select: false },
        )),
        NEXTD_OR_FORWARD_WORD => Some((
            "nextd 2>/dev/null",
            reedline::EditCommand::MoveWordRight { select: false },
        )),
        _ => None,
    }
}

/// Supplies a word of a history entry for `yank-last-arg`: the entry `back` places before
/// the newest, and its word `nth` (the last when `None`); see [`crate::history_words::pick`].
pub(crate) type HistoryWords = Box<dyn Fn(usize, Option<i64>) -> Option<String> + Send>;

#[derive(thiserror::Error, Debug)]
pub enum KeyError {
    /// Unsupported key sequence
    #[error("unsupported key sequence: {0}")]
    UnsupportedKeySequence(KeySequence),

    /// Unsupported key action
    #[error("unsupported key action: {0}")]
    UnsupportedKeyAction(KeyAction),
}

pub(crate) struct MutableEditMode {
    inner: Arc<Mutex<UpdatableBindings>>,
}

impl MutableEditMode {
    pub fn new(bindings: reedline::Keybindings, history_words: HistoryWords) -> Self {
        let mut inner = UpdatableBindings::new(bindings);
        inner.history_words = Some(history_words);
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub fn bindings(&self) -> Arc<Mutex<UpdatableBindings>> {
        self.inner.clone()
    }
}

impl reedline::EditMode for MutableEditMode {
    fn parse_event(&mut self, event: reedline::ReedlineRawEvent) -> reedline::ReedlineEvent {
        let mut inner = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.inner.lock())
        });

        inner.parse_event(event)
    }

    fn edit_mode(&self) -> reedline::PromptEditMode {
        let inner = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.inner.lock())
        });

        inner.edit_mode()
    }
}

pub(crate) struct UpdatableBindings {
    bindings: reedline::Keybindings,
    edit_mode: Box<dyn reedline::EditMode>,
    /// Trie for raw byte sequences. Supports both exact lookups and prefix matching
    /// during macro resolution.
    raw_mappings: Trie<Vec<u8>, interfaces::KeyAction>,
    /// Tracks defined macros.
    macros: HashMap<interfaces::KeySequence, interfaces::KeySequence>,
    /// Readline-style Meta-digit prefix currently being assembled.
    numeric_argument: Option<NumericArgument>,
    /// Completed argument for the `bind -x` command that just left the editor.
    completed_numeric_argument: Option<i64>,
    /// Where `yank-last-arg` finds its words; without one it does nothing.
    history_words: Option<HistoryWords>,
    /// The last `yank-last-arg` insertion, while presses of it follow one another.
    last_yank: Option<Yank>,
    /// Ctrl-X was pressed and the second key of its chord has not arrived yet.
    pending_ctrl_x: bool,
}

/// What a `yank-last-arg` press inserted, so the next press can take it back.
#[derive(Clone, Copy, Debug)]
struct Yank {
    /// How many entries before the newest the word came from.
    back: usize,
    /// The word asked for: `None` is the last.
    nth: Option<i64>,
    /// The inserted word's length in graphemes, the unit Backspace removes.
    graphemes: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct NumericArgument {
    magnitude: i64,
    negative: bool,
    has_digits: bool,
}

impl NumericArgument {
    fn push_digit(&mut self, digit: u32) {
        self.has_digits = true;
        self.magnitude = self
            .magnitude
            .saturating_mul(10)
            .saturating_add(i64::from(digit));
    }

    const fn value(self) -> i64 {
        let magnitude = if self.has_digits { self.magnitude } else { 1 };
        if self.negative { -magnitude } else { magnitude }
    }
}

impl UpdatableBindings {
    pub fn new(bindings: reedline::Keybindings) -> Self {
        // Clone the bindings so we can keep a copy for later updates.
        let edit_mode = Self::rebuild_edit_mode(&bindings);

        Self {
            bindings,
            edit_mode,
            raw_mappings: Trie::new(),
            macros: HashMap::new(),
            numeric_argument: None,
            completed_numeric_argument: None,
            history_words: None,
            last_yank: None,
            pending_ctrl_x: false,
        }
    }

    pub fn update(&mut self, f: impl Fn(&mut reedline::Keybindings)) {
        f(&mut self.bindings);
        self.try_update_bindings_for_all_macros();
        self.edit_mode = Self::rebuild_edit_mode(&self.bindings);
    }

    fn rebuild_edit_mode(bindings: &reedline::Keybindings) -> Box<dyn reedline::EditMode> {
        Box::new(reedline::Emacs::new(bindings.clone()))
    }
}

impl reedline::EditMode for UpdatableBindings {
    fn parse_event(&mut self, event: reedline::ReedlineRawEvent) -> reedline::ReedlineEvent {
        let event: crossterm::event::Event = event.into();
        let numeric_piece = match &event {
            crossterm::event::Event::Key(crossterm::event::KeyEvent {
                code: crossterm::event::KeyCode::Char(ch),
                modifiers,
                ..
            }) if *modifiers == crossterm::event::KeyModifiers::ALT && ch.is_ascii_digit() => {
                ch.to_digit(10).map(Some)
            }
            crossterm::event::Event::Key(crossterm::event::KeyEvent {
                code: crossterm::event::KeyCode::Char('-'),
                modifiers,
                ..
            }) if *modifiers == crossterm::event::KeyModifiers::ALT => Some(None),
            _ => None,
        };

        let key = match &event {
            crossterm::event::Event::Key(key) => Some((key.code, key.modifiers)),
            _ => None,
        };

        // The event originated as a `ReedlineRawEvent`, so converting it back
        // always succeeds; if it somehow did not, ignore the event rather than
        // panicking.
        let Ok(raw_event) = reedline::ReedlineRawEvent::try_from(event) else {
            return reedline::ReedlineEvent::None;
        };

        // Readline's Ctrl-X keymap. Reedline binds single keys only, so the chord is
        // tracked here: an unbound Ctrl-X waits for the next key, which is consumed
        // whether or not the chord means anything, as in Readline.
        let chord_second_key = std::mem::take(&mut self.pending_ctrl_x);
        let parsed = if chord_second_key {
            ctrl_x_chord(key)
        } else {
            let parsed = self.edit_mode.parse_event(raw_event);
            if key == Some((crossterm::event::KeyCode::Char('x'), CONTROL))
                && matches!(parsed, reedline::ReedlineEvent::None)
            {
                self.pending_ctrl_x = true;
                return reedline::ReedlineEvent::None;
            }
            parsed
        };

        if matches!(&parsed, reedline::ReedlineEvent::ExecuteHostCommand(command) if command == YANK_LAST_ARG)
        {
            return self.yank_last_arg();
        }
        if !matches!(parsed, reedline::ReedlineEvent::None) {
            self.last_yank = None;
        }

        // An explicit binding wins. Otherwise Meta-digit and Meta-minus build
        // the argument passed to the next bind -x command.
        if !chord_second_key
            && numeric_piece.is_some()
            && matches!(parsed, reedline::ReedlineEvent::None)
        {
            let argument = self.numeric_argument.get_or_insert_default();
            if let Some(digit) = numeric_piece.flatten() {
                argument.push_digit(digit);
            } else {
                argument.negative = true;
            }
            return reedline::ReedlineEvent::None;
        }

        if matches!(parsed, reedline::ReedlineEvent::ExecuteHostCommand(_)) {
            self.completed_numeric_argument =
                self.numeric_argument.take().map(NumericArgument::value);
        } else if !matches!(parsed, reedline::ReedlineEvent::None) {
            self.numeric_argument = None;
        }
        parsed
    }

    fn edit_mode(&self) -> reedline::PromptEditMode {
        self.edit_mode.edit_mode()
    }
}

impl interfaces::KeyBindings for UpdatableBindings {
    fn get_current(&self) -> HashMap<interfaces::KeySequence, interfaces::KeyAction> {
        let mut results = HashMap::new();

        for (key_combo, event) in self.bindings.get_keybindings() {
            let action = translate_reedline_event_to_action(event);
            if let Some(action) = action {
                if let Some(key) = translate_reedline_keycode(key_combo.key_code) {
                    let mut stroke = KeyStroke::from(key);

                    if key_combo.modifier.contains(reedline::KeyModifiers::CONTROL) {
                        stroke.control = true;
                    }
                    if key_combo.modifier.contains(reedline::KeyModifiers::ALT) {
                        stroke.alt = true;
                    }
                    if key_combo.modifier.contains(reedline::KeyModifiers::SHIFT) {
                        stroke.shift = true;
                    }
                    if key_combo.modifier.contains(reedline::KeyModifiers::HYPER) {
                        // TODO
                    }
                    if key_combo.modifier.contains(reedline::KeyModifiers::META) {
                        // TODO
                    }
                    if key_combo.modifier.contains(reedline::KeyModifiers::SUPER) {
                        // TODO
                    }

                    let seq = KeySequence::from(stroke);
                    results.insert(seq, action);
                }
            }
        }

        results
    }

    fn get_untranslated(&self, bytes: &[u8]) -> Option<&KeyAction> {
        self.raw_mappings.get(bytes)
    }

    fn bind(&mut self, seq: KeySequence, action: KeyAction) -> Result<(), std::io::Error> {
        self.do_bind(seq, action, true)
    }

    fn try_unbind(&mut self, seq: KeySequence) -> bool {
        self.try_unbind_impl(&seq, true)
    }

    fn define_macro(
        &mut self,
        seq: KeySequence,
        target: KeySequence,
    ) -> Result<(), std::io::Error> {
        self.macros.insert(seq, target);
        self.update(|_| {});

        Ok(())
    }

    fn get_macros(&self) -> HashMap<KeySequence, KeySequence> {
        self.macros.clone()
    }
}

impl UpdatableBindings {
    pub const fn take_numeric_argument(&mut self) -> Option<i64> {
        self.completed_numeric_argument.take()
    }

    /// Readline's `yank-last-arg`: inserts the previous command's last word. Pressed again,
    /// it replaces that word with the last word of the command before, and so on back
    /// through history. A numeric argument picks word N instead (`yank-nth-arg`), and
    /// later presses keep asking for that word.
    fn yank_last_arg(&mut self) -> reedline::ReedlineEvent {
        use reedline::{EditCommand, ReedlineEvent};
        use unicode_segmentation::UnicodeSegmentation as _;

        let Some(history_words) = &self.history_words else {
            return ReedlineEvent::None;
        };

        let (back, nth, replaced) = match self.last_yank {
            Some(previous) => (previous.back + 1, previous.nth, previous.graphemes),
            None => (
                0,
                self.numeric_argument.take().map(NumericArgument::value),
                0,
            ),
        };

        // Readline rings the bell when history runs out; the line is left as it is, and
        // the next press asks for the same entry again.
        let Some(word) = history_words(back, nth) else {
            return ReedlineEvent::None;
        };

        self.last_yank = Some(Yank {
            back,
            nth,
            graphemes: word.graphemes(true).count(),
        });

        let mut edits = vec![EditCommand::Backspace; replaced];
        edits.push(EditCommand::InsertString(word));
        ReedlineEvent::Edit(edits)
    }

    /// Internal implementation that optionally removes from the macros map.
    /// When updating bindings for macros, we don't want to remove the macro definition itself.
    fn try_unbind_impl(&mut self, seq: &KeySequence, remove_from_macros: bool) -> bool {
        // Optionally remove from macros.
        let removed_macro = if remove_from_macros {
            self.macros.remove(seq).is_some()
        } else {
            false
        };

        match seq {
            interfaces::KeySequence::Strokes(_) => {
                if let Some((modifiers, key_code)) = translate_key_sequence_to_reedline(seq) {
                    let found = self.bindings.find_binding(modifiers, key_code).is_some();

                    if found {
                        self.update(|bindings| {
                            let _ = bindings.remove_binding(modifiers, key_code);
                        });
                    }

                    found || removed_macro
                } else {
                    removed_macro
                }
            }
            interfaces::KeySequence::Bytes(bytes) => {
                let flat_bytes: Vec<u8> = bytes.iter().flatten().copied().collect();
                let removed_raw = self.raw_mappings.remove(&flat_bytes).is_some();

                removed_raw || removed_macro
            }
        }
    }

    fn do_bind(
        &mut self,
        seq: KeySequence,
        action: KeyAction,
        rebuild_for_reedline: bool,
    ) -> Result<(), std::io::Error> {
        let Some(event) = translate_action_to_reedline_event(&action) else {
            return Err(std::io::Error::other(KeyError::UnsupportedKeyAction(
                action,
            )));
        };

        match seq {
            interfaces::KeySequence::Strokes(_) => {
                if let Some((modifiers, key_code)) = translate_key_sequence_to_reedline(&seq) {
                    if rebuild_for_reedline {
                        self.update(|bindings| {
                            bindings.add_binding(modifiers, key_code, event.clone());
                        });
                    } else {
                        self.bindings
                            .add_binding(modifiers, key_code, event.clone());
                    }

                    Ok(())
                } else {
                    Err(std::io::Error::other(KeyError::UnsupportedKeySequence(seq)))
                }
            }
            interfaces::KeySequence::Bytes(ref bytes) => {
                let flat_bytes: Vec<u8> = bytes.iter().flatten().copied().collect();
                self.raw_mappings.insert(flat_bytes, action);

                Ok(())
            }
        }
    }

    fn try_update_bindings_for_all_macros(&mut self) {
        let macros = self.macros.clone();
        for (seq, target) in macros {
            let _ = self.update_bindings_for_macro(seq, target);
        }
    }

    fn update_bindings_for_macro(
        &mut self,
        seq: KeySequence,
        target: KeySequence,
    ) -> Result<(), std::io::Error> {
        match target {
            // TODO(input): We acknowledge that this implementation eagerly resolves the macro
            // and what it will do. Subsequent changes to other key binding might invalidate
            // this. We also are *extremely* limited in what we support here.
            interfaces::KeySequence::Strokes(key_strokes) => {
                if key_strokes.is_empty() {
                    // Empty macro target - unbind any existing binding for this sequence.
                    self.try_unbind(seq);
                } else {
                    return Err(std::io::Error::other(
                        "binding key sequence to readline macro with strokes",
                    ));
                }
            }
            interfaces::KeySequence::Bytes(items) => {
                // Flatten all byte sequences into one contiguous buffer for prefix matching.
                let flat_bytes: Vec<u8> = items.iter().flatten().copied().collect();
                let actions = self.resolve_macro_body(&flat_bytes);

                if actions.is_empty() {
                    // No actions resolved - unbind any existing key binding for this sequence,
                    // but keep the macro definition so it shows up in `bind -s/-S`.
                    self.try_unbind_impl(&seq, false);
                    return Ok(());
                }

                // Create a single action: either the action itself (if just one), or a Sequence.
                let action = if let [single] = actions.as_slice() {
                    single.clone()
                } else {
                    KeyAction::Sequence(actions)
                };

                self.do_bind(seq, action, false)?;
            }
        }

        Ok(())
    }

    /// Resolve a macro body (byte sequence) into a sequence of actions using prefix matching.
    /// Returns a flattened vector of actions (any nested Sequences are expanded).
    fn resolve_macro_body(&self, bytes: &[u8]) -> Vec<KeyAction> {
        let mut actions = Vec::new();
        let mut remaining = bytes;

        while !remaining.is_empty() {
            // Find the longest prefix match in the trie.
            if let Some((matched_key, action)) = self.find_longest_prefix_match(remaining) {
                // Flatten any nested Sequence actions.
                Self::flatten_action_into(&mut actions, action.clone());
                remaining = &remaining[matched_key.len()..];
            } else {
                // No match found - skip one byte and continue.
                // This handles unbound byte sequences gracefully.
                tracing::debug!(
                    target: trace_categories::INPUT,
                    "skipping unbound byte in macro resolution: 0x{:02x}",
                    remaining[0]
                );
                remaining = &remaining[1..];
            }
        }

        actions
    }

    /// Find the longest prefix match in the trie for the given bytes.
    /// Uses the trie's native `get_ancestor` method which efficiently finds
    /// the longest matching prefix.
    fn find_longest_prefix_match(&self, bytes: &[u8]) -> Option<(Vec<u8>, &KeyAction)> {
        use radix_trie::TrieCommon;

        self.raw_mappings.get_ancestor(bytes).and_then(|subtrie| {
            let key = subtrie.key()?.clone();
            let value = subtrie.value()?;
            Some((key, value))
        })
    }

    /// Flatten an action into the actions vector, expanding any Sequence variants.
    fn flatten_action_into(actions: &mut Vec<KeyAction>, action: KeyAction) {
        match action {
            KeyAction::Sequence(inner_actions) => {
                for inner in inner_actions {
                    Self::flatten_action_into(actions, inner);
                }
            }
            other => actions.push(other),
        }
    }
}

const CONTROL: crossterm::event::KeyModifiers = crossterm::event::KeyModifiers::CONTROL;

/// The second key of a Ctrl-X chord, with Readline's default meaning: Ctrl-E edits the
/// line in an editor and runs it, Ctrl-U undoes. Any other key is swallowed.
fn ctrl_x_chord(
    key: Option<(crossterm::event::KeyCode, crossterm::event::KeyModifiers)>,
) -> reedline::ReedlineEvent {
    use crossterm::event::KeyCode;
    match key {
        Some((KeyCode::Char('e'), CONTROL)) => {
            reedline::ReedlineEvent::ExecuteHostCommand(EDIT_AND_EXECUTE_COMMAND.to_owned())
        }
        Some((KeyCode::Char('u'), CONTROL)) => {
            reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::Undo])
        }
        _ => reedline::ReedlineEvent::None,
    }
}

fn translate_key_sequence_to_reedline(
    seq: &KeySequence,
) -> Option<(reedline::KeyModifiers, reedline::KeyCode)> {
    let KeySequence::Strokes(strokes) = seq else {
        // TODO(input): handle other kinds of key sequences
        return None;
    };

    let [stroke] = &strokes.as_slice() else {
        // TODO(input): handle multiple strokes
        return None;
    };

    let mut modifiers = reedline::KeyModifiers::empty();
    modifiers.set(reedline::KeyModifiers::ALT, stroke.alt);
    modifiers.set(reedline::KeyModifiers::CONTROL, stroke.control);
    modifiers.set(reedline::KeyModifiers::SHIFT, stroke.shift);

    let key_code = match stroke.key {
        Key::Character(c) => reedline::KeyCode::Char(c),
        Key::Backspace => reedline::KeyCode::Backspace,
        Key::Enter => reedline::KeyCode::Enter,
        Key::Left => reedline::KeyCode::Left,
        Key::Right => reedline::KeyCode::Right,
        Key::Up => reedline::KeyCode::Up,
        Key::Down => reedline::KeyCode::Down,
        Key::Home => reedline::KeyCode::Home,
        Key::End => reedline::KeyCode::End,
        Key::PageUp => reedline::KeyCode::PageUp,
        Key::PageDown => reedline::KeyCode::PageDown,
        Key::Tab => reedline::KeyCode::Tab,
        Key::BackTab => reedline::KeyCode::BackTab,
        Key::Delete => reedline::KeyCode::Delete,
        Key::Insert => reedline::KeyCode::Insert,
        Key::F(n) => reedline::KeyCode::F(n),
        Key::Escape => reedline::KeyCode::Esc,
    };

    Some((modifiers, key_code))
}

fn translate_action_to_reedline_event(action: &KeyAction) -> Option<reedline::ReedlineEvent> {
    match action {
        KeyAction::ShellCommand(cmd) => {
            Some(reedline::ReedlineEvent::ExecuteHostCommand(cmd.to_owned()))
        }
        KeyAction::DoInputFunction(func) => translate_input_function_to_reedline_event(func),
        KeyAction::Sequence(actions) => {
            // Convert each action in the sequence to a reedline event.
            let events: Vec<_> = actions
                .iter()
                .filter_map(translate_action_to_reedline_event)
                .collect();

            if events.is_empty() {
                None
            } else if events.len() == 1 {
                events.into_iter().next()
            } else {
                Some(reedline::ReedlineEvent::Multiple(events))
            }
        }
    }
}

fn translate_input_function_to_reedline_event(
    func: &InputFunction,
) -> Option<reedline::ReedlineEvent> {
    use reedline::{EditCommand, ReedlineEvent};

    match func {
        InputFunction::BackwardDeleteChar => {
            Some(ReedlineEvent::Edit(vec![EditCommand::Backspace]))
        }
        InputFunction::BackwardKillWord => {
            Some(ReedlineEvent::Edit(vec![EditCommand::CutWordLeft]))
        }
        InputFunction::KillLine => Some(ReedlineEvent::Edit(vec![EditCommand::KillLine])),
        InputFunction::KillWholeLine => Some(ReedlineEvent::Edit(vec![EditCommand::CutFromStart])),
        InputFunction::KillWord => Some(ReedlineEvent::Edit(vec![EditCommand::CutWordRight])),
        InputFunction::DeleteChar => Some(ReedlineEvent::Edit(vec![EditCommand::Delete])),
        InputFunction::DowncaseWord => Some(ReedlineEvent::Edit(vec![EditCommand::LowercaseWord])),
        InputFunction::BackwardChar => Some(ReedlineEvent::Edit(vec![EditCommand::MoveLeft {
            select: false,
        }])),
        InputFunction::ForwardChar => Some(ReedlineEvent::Edit(vec![EditCommand::MoveRight {
            select: false,
        }])),
        InputFunction::EndOfLine => Some(ReedlineEvent::Edit(vec![EditCommand::MoveToLineEnd {
            select: false,
        }])),
        InputFunction::BeginningOfLine => {
            Some(ReedlineEvent::Edit(vec![EditCommand::MoveToLineStart {
                select: false,
            }]))
        }
        InputFunction::BackwardWord => Some(ReedlineEvent::Edit(vec![EditCommand::MoveWordLeft {
            select: false,
        }])),
        InputFunction::ForwardWord => Some(ReedlineEvent::Edit(vec![EditCommand::MoveWordRight {
            select: false,
        }])),
        InputFunction::Yank => Some(ReedlineEvent::Edit(vec![EditCommand::PasteCutBufferAfter])),
        InputFunction::ViRedo => Some(ReedlineEvent::Edit(vec![EditCommand::Redo])),
        InputFunction::ViUndo => Some(ReedlineEvent::Edit(vec![EditCommand::Undo])),
        InputFunction::EditAndExecuteCommand | InputFunction::ViEditAndExecuteCommand => Some(
            ReedlineEvent::ExecuteHostCommand(EDIT_AND_EXECUTE_COMMAND.to_owned()),
        ),
        InputFunction::YankLastArg => {
            Some(ReedlineEvent::ExecuteHostCommand(YANK_LAST_ARG.to_owned()))
        }
        InputFunction::TransposeChars => {
            Some(ReedlineEvent::Edit(vec![EditCommand::SwapGraphemes]))
        }
        InputFunction::UpcaseWord => Some(ReedlineEvent::Edit(vec![EditCommand::UppercaseWord])),
        InputFunction::Undo => Some(ReedlineEvent::Edit(vec![EditCommand::Undo])),
        InputFunction::ClearScreen => Some(ReedlineEvent::ClearScreen),
        InputFunction::AcceptLine => Some(ReedlineEvent::Enter),
        InputFunction::HistorySearchBackward => Some(ReedlineEvent::SearchHistory),
        InputFunction::RedrawCurrentLine => Some(ReedlineEvent::Repaint),
        InputFunction::Complete => Some(ReedlineEvent::Edit(vec![EditCommand::Complete])),
        InputFunction::CashAcceptHint => Some(ReedlineEvent::HistoryHintComplete),
        InputFunction::CashAcceptHintWord => Some(ReedlineEvent::HistoryHintWordComplete),
        _ => None,
    }
}

const fn translate_reedline_keycode(keycode: reedline::KeyCode) -> Option<Key> {
    match keycode {
        reedline::KeyCode::Backspace => Some(Key::Backspace),
        reedline::KeyCode::Enter => Some(Key::Enter),
        reedline::KeyCode::Left => Some(Key::Left),
        reedline::KeyCode::Right => Some(Key::Right),
        reedline::KeyCode::Up => Some(Key::Up),
        reedline::KeyCode::Down => Some(Key::Down),
        reedline::KeyCode::Home => Some(Key::Home),
        reedline::KeyCode::End => Some(Key::End),
        reedline::KeyCode::PageUp => Some(Key::PageUp),
        reedline::KeyCode::PageDown => Some(Key::PageDown),
        reedline::KeyCode::Tab => Some(Key::Tab),
        reedline::KeyCode::BackTab => Some(Key::BackTab),
        reedline::KeyCode::Delete => Some(Key::Delete),
        reedline::KeyCode::Insert => Some(Key::Insert),
        reedline::KeyCode::F(n) => Some(Key::F(n)),
        reedline::KeyCode::Char(c) => Some(Key::Character(c)),
        reedline::KeyCode::Null => None,
        reedline::KeyCode::Esc => Some(Key::Escape),
        reedline::KeyCode::CapsLock => None,
        reedline::KeyCode::ScrollLock => None,
        reedline::KeyCode::NumLock => None,
        reedline::KeyCode::PrintScreen => None,
        reedline::KeyCode::Pause => None,
        reedline::KeyCode::Menu => None,
        reedline::KeyCode::KeypadBegin => None,
        reedline::KeyCode::Media(_media_key_code) => None,
        reedline::KeyCode::Modifier(_modifier_key_code) => None,
    }
}

#[expect(clippy::too_many_lines)]
fn translate_reedline_event_to_action(event: &reedline::ReedlineEvent) -> Option<KeyAction> {
    match event {
        reedline::ReedlineEvent::Edit(cmds) => {
            match cmds.as_slice() {
                [reedline::EditCommand::Backspace] => Some(KeyAction::DoInputFunction(
                    InputFunction::BackwardDeleteChar,
                )),
                [reedline::EditCommand::BackspaceWord] => {
                    // Not quite accurate, because it doesn't save the deleted text.
                    Some(KeyAction::DoInputFunction(InputFunction::BackwardKillWord))
                }
                [reedline::EditCommand::CapitalizeChar] => None,
                [reedline::EditCommand::ClearToLineEnd] => {
                    // Not quite accurate, because it doesn't save the deleted text.
                    Some(KeyAction::DoInputFunction(InputFunction::KillLine))
                }
                [reedline::EditCommand::Complete] => {
                    Some(KeyAction::DoInputFunction(InputFunction::Complete))
                }
                [reedline::EditCommand::CutFromStart] => {
                    Some(KeyAction::DoInputFunction(InputFunction::KillWholeLine))
                }
                [reedline::EditCommand::KillLine] => {
                    Some(KeyAction::DoInputFunction(InputFunction::KillLine))
                }
                [reedline::EditCommand::CutWordLeft] => {
                    Some(KeyAction::DoInputFunction(InputFunction::BackwardKillWord))
                }
                [reedline::EditCommand::CutWordRight] => {
                    Some(KeyAction::DoInputFunction(InputFunction::KillWord))
                }
                [reedline::EditCommand::Delete] => {
                    Some(KeyAction::DoInputFunction(InputFunction::DeleteChar))
                }
                [reedline::EditCommand::DeleteWord] => {
                    Some(KeyAction::DoInputFunction(InputFunction::KillWord))
                }
                [reedline::EditCommand::InsertNewline] => None,
                [reedline::EditCommand::LowercaseWord] => {
                    Some(KeyAction::DoInputFunction(InputFunction::DowncaseWord))
                }
                [reedline::EditCommand::MoveLeft { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::BackwardChar))
                }
                [reedline::EditCommand::MoveLeft { select: true }] => None,
                [reedline::EditCommand::MoveRight { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::ForwardChar))
                }
                [reedline::EditCommand::MoveRight { select: true }] => None,
                // The start and end of the whole input (Reedline's Ctrl-Home, Ctrl-End)
                // have no Readline name: `beginning-of-line` and `end-of-line` stay on the
                // current line of a multi-line command, so these are not listed as them.
                [
                    reedline::EditCommand::MoveToEnd { .. }
                    | reedline::EditCommand::MoveToStart { .. },
                ] => None,
                [reedline::EditCommand::MoveToLineEnd { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::EndOfLine))
                }
                [reedline::EditCommand::MoveToLineEnd { select: true }] => None,
                [reedline::EditCommand::MoveToLineStart { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::BeginningOfLine))
                }
                [reedline::EditCommand::MoveToLineStart { select: true }] => None,
                [reedline::EditCommand::MoveWordLeft { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::BackwardWord))
                }
                [reedline::EditCommand::MoveWordLeft { select: true }] => None,
                [reedline::EditCommand::MoveWordRight { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::ForwardWord))
                }
                [reedline::EditCommand::MoveWordRight { select: true }] => None,
                [reedline::EditCommand::PasteCutBufferAfter] => {
                    Some(KeyAction::DoInputFunction(InputFunction::Yank))
                }
                [reedline::EditCommand::PasteCutBufferBefore] => None,
                [reedline::EditCommand::Redo] => {
                    Some(KeyAction::DoInputFunction(InputFunction::ViRedo))
                }
                [reedline::EditCommand::SelectAll] => None,
                [reedline::EditCommand::SwapGraphemes] => {
                    Some(KeyAction::DoInputFunction(InputFunction::TransposeChars))
                }
                [reedline::EditCommand::UppercaseWord] => {
                    Some(KeyAction::DoInputFunction(InputFunction::UpcaseWord))
                }
                [reedline::EditCommand::Undo] => {
                    Some(KeyAction::DoInputFunction(InputFunction::Undo))
                }
                _ => {
                    // TODO(input): Handle more?
                    tracing::debug!(target: trace_categories::INPUT, "unhandled edit commands: {cmds:?}");
                    None
                }
            }
        }
        reedline::ReedlineEvent::ClearScreen => {
            Some(KeyAction::DoInputFunction(InputFunction::ClearScreen))
        }
        reedline::ReedlineEvent::CtrlC => None,
        reedline::ReedlineEvent::CtrlD => None,
        reedline::ReedlineEvent::Enter => {
            Some(KeyAction::DoInputFunction(InputFunction::AcceptLine))
        }
        reedline::ReedlineEvent::Esc => None,
        reedline::ReedlineEvent::MenuPrevious => None,
        reedline::ReedlineEvent::OpenEditor => Some(KeyAction::DoInputFunction(
            InputFunction::EditAndExecuteCommand,
        )),
        reedline::ReedlineEvent::Left => {
            Some(KeyAction::DoInputFunction(InputFunction::BackwardChar))
        }
        reedline::ReedlineEvent::Right => {
            Some(KeyAction::DoInputFunction(InputFunction::ForwardChar))
        }
        reedline::ReedlineEvent::Up => Some(KeyAction::DoInputFunction(
            InputFunction::PreviousScreenLine,
        )),
        reedline::ReedlineEvent::Down => {
            Some(KeyAction::DoInputFunction(InputFunction::NextScreenLine))
        }
        reedline::ReedlineEvent::SearchHistory => Some(KeyAction::DoInputFunction(
            InputFunction::HistorySearchBackward,
        )),
        reedline::ReedlineEvent::Repaint => {
            Some(KeyAction::DoInputFunction(InputFunction::RedrawCurrentLine))
        }
        reedline::ReedlineEvent::HistoryHintComplete => {
            Some(KeyAction::DoInputFunction(InputFunction::CashAcceptHint))
        }
        reedline::ReedlineEvent::HistoryHintWordComplete => Some(KeyAction::DoInputFunction(
            InputFunction::CashAcceptHintWord,
        )),
        reedline::ReedlineEvent::Multiple(evts) => {
            if let &[
                reedline::ReedlineEvent::Edit(ref edit_cmds),
                reedline::ReedlineEvent::Enter,
            ] = evts.as_slice()
            {
                if let &[
                    reedline::EditCommand::MoveToStart { select: false },
                    reedline::EditCommand::InsertChar('#'),
                ] = edit_cmds.as_slice()
                {
                    return Some(KeyAction::DoInputFunction(InputFunction::InsertComment));
                }
            }

            // TODO(input): Try to extract something from these?
            tracing::debug!(target: trace_categories::INPUT, "unhandled composite event: {evts:?}");
            None
        }
        reedline::ReedlineEvent::UntilFound(uf_events) => {
            let mut i = 0;

            if uf_events.is_empty() {
                return None;
            }

            while i < uf_events.len() {
                match &uf_events[i] {
                    reedline::ReedlineEvent::HistoryHintComplete
                    | reedline::ReedlineEvent::HistoryHintWordComplete
                    | reedline::ReedlineEvent::Menu(_)
                    | reedline::ReedlineEvent::MenuDown
                    | reedline::ReedlineEvent::MenuUp
                    | reedline::ReedlineEvent::MenuLeft
                    | reedline::ReedlineEvent::MenuRight
                    | reedline::ReedlineEvent::MenuNext
                    | reedline::ReedlineEvent::MenuPrevious
                    | reedline::ReedlineEvent::MenuPageNext
                    | reedline::ReedlineEvent::MenuPagePrevious => {
                        i += 1;
                    }
                    _ => {
                        break;
                    }
                }
            }

            if i == uf_events.len() - 1 {
                translate_reedline_event_to_action(&uf_events[i])
            } else {
                // TODO(input): Try to extract something from these?
                tracing::debug!(target: trace_categories::INPUT, "unhandled until-found event: {uf_events:?}");
                None
            }
        }
        // Readline has no name for these; like its other unnamed bindings, unlisted.
        reedline::ReedlineEvent::ExecuteHostCommand(cmd) if folder_history_key(cmd).is_some() => {
            None
        }
        reedline::ReedlineEvent::ExecuteHostCommand(cmd) if cmd == YANK_LAST_ARG => {
            Some(KeyAction::DoInputFunction(InputFunction::YankLastArg))
        }
        reedline::ReedlineEvent::ExecuteHostCommand(cmd) if cmd == EDIT_AND_EXECUTE_COMMAND => {
            Some(KeyAction::DoInputFunction(
                InputFunction::EditAndExecuteCommand,
            ))
        }
        reedline::ReedlineEvent::ExecuteHostCommand(cmd) => {
            Some(KeyAction::ShellCommand(cmd.to_owned()))
        }
        evt => {
            // TODO(input): Handle more?
            tracing::debug!(target: trace_categories::INPUT, "unhandled event: {evt:?}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use reedline::EditMode as _;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> reedline::ReedlineRawEvent {
        reedline::ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(code, modifiers))).unwrap()
    }

    #[test]
    fn meta_digits_become_the_next_host_commands_numeric_argument() {
        let mut bindings = UpdatableBindings::new(reedline::default_emacs_keybindings());
        bindings
            .bind(
                KeyStroke {
                    control: true,
                    alt: false,
                    shift: false,
                    key: Key::Character('x'),
                }
                .into(),
                KeyAction::ShellCommand("echo bound".into()),
            )
            .unwrap();

        assert!(matches!(
            bindings.parse_event(key(KeyCode::Char('2'), KeyModifiers::ALT)),
            reedline::ReedlineEvent::None
        ));
        assert!(matches!(
            bindings.parse_event(key(KeyCode::Char('3'), KeyModifiers::ALT)),
            reedline::ReedlineEvent::None
        ));
        assert!(matches!(
            bindings.parse_event(key(KeyCode::Char('x'), KeyModifiers::CONTROL)),
            reedline::ReedlineEvent::ExecuteHostCommand(command) if command == "echo bound"
        ));
        assert_eq!(bindings.take_numeric_argument(), Some(23));
        assert_eq!(bindings.take_numeric_argument(), None);
    }

    #[test]
    fn meta_minus_makes_a_negative_argument() {
        let mut bindings = UpdatableBindings::new(reedline::default_emacs_keybindings());
        bindings
            .bind(
                KeyStroke {
                    control: true,
                    alt: false,
                    shift: false,
                    key: Key::Character('x'),
                }
                .into(),
                KeyAction::ShellCommand("echo bound".into()),
            )
            .unwrap();

        let _ = bindings.parse_event(key(KeyCode::Char('-'), KeyModifiers::ALT));
        let _ = bindings.parse_event(key(KeyCode::Char('4'), KeyModifiers::ALT));
        let _ = bindings.parse_event(key(KeyCode::Char('x'), KeyModifiers::CONTROL));
        assert_eq!(bindings.take_numeric_argument(), Some(-4));
    }

    const ALT_DOT: KeyStroke = KeyStroke {
        control: false,
        alt: true,
        shift: false,
        key: Key::Character('.'),
    };

    /// Bindings with Alt-. bound to `yank-last-arg` and `entries` as history, oldest first.
    fn with_history(entries: &[&str]) -> UpdatableBindings {
        let entries: Vec<String> = entries.iter().map(|&e| e.to_owned()).collect();
        let mut bindings = UpdatableBindings::new(reedline::default_emacs_keybindings());
        bindings.history_words = Some(Box::new(move |back, nth| {
            let index = entries.len().checked_sub(back + 1)?;
            crate::history_words::pick(&entries[index], nth)
        }));
        bindings
            .bind(
                ALT_DOT.into(),
                KeyAction::DoInputFunction(InputFunction::YankLastArg),
            )
            .unwrap();
        bindings
    }

    fn insert(word: &str, replacing: usize) -> reedline::ReedlineEvent {
        let mut edits = vec![reedline::EditCommand::Backspace; replacing];
        edits.push(reedline::EditCommand::InsertString(word.to_owned()));
        reedline::ReedlineEvent::Edit(edits)
    }

    #[test]
    fn alt_dot_walks_back_through_the_last_words_of_history() {
        let mut bindings = with_history(&["cp naïve.txt backup", "ls -la"]);
        let alt_dot = || key(KeyCode::Char('.'), KeyModifiers::ALT);

        assert_eq!(bindings.parse_event(alt_dot()), insert("-la", 0));
        assert_eq!(bindings.parse_event(alt_dot()), insert("backup", 3));
        // History has run out: the line stays as it is.
        assert_eq!(
            bindings.parse_event(alt_dot()),
            reedline::ReedlineEvent::None
        );

        // Any other key ends the run; the next press starts again from the newest entry.
        let _ = bindings.parse_event(key(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(bindings.parse_event(alt_dot()), insert("-la", 0));
    }

    #[test]
    fn alt_dot_replaces_by_graphemes_not_chars() {
        // "i" followed by a combining diaeresis: five graphemes, six chars, seven bytes.
        let naive = "nai\u{308}ve";
        let mut bindings = with_history(&["echo x", &format!("echo {naive}")]);
        let alt_dot = || key(KeyCode::Char('.'), KeyModifiers::ALT);

        assert_eq!(bindings.parse_event(alt_dot()), insert(naive, 0));
        assert_eq!(bindings.parse_event(alt_dot()), insert("x", 5));
    }

    #[test]
    fn a_numeric_argument_picks_that_word() {
        let mut bindings = with_history(&["cp src dest"]);
        let _ = bindings.parse_event(key(KeyCode::Char('1'), KeyModifiers::ALT));
        assert_eq!(
            bindings.parse_event(key(KeyCode::Char('.'), KeyModifiers::ALT)),
            insert("src", 0)
        );
    }

    #[test]
    fn yank_last_arg_is_listed_under_its_readline_name() {
        let bindings = with_history(&[]);
        assert_eq!(
            bindings.get_current().get(&KeySequence::from(ALT_DOT)),
            Some(&KeyAction::DoInputFunction(InputFunction::YankLastArg))
        );
    }

    #[test]
    fn ctrl_x_ctrl_e_edits_and_executes_the_line() {
        let mut bindings = with_history(&[]);
        assert_eq!(
            bindings.parse_event(key(KeyCode::Char('x'), KeyModifiers::CONTROL)),
            reedline::ReedlineEvent::None
        );
        assert_eq!(
            bindings.parse_event(key(KeyCode::Char('e'), KeyModifiers::CONTROL)),
            reedline::ReedlineEvent::ExecuteHostCommand(EDIT_AND_EXECUTE_COMMAND.to_owned())
        );
    }

    #[test]
    fn an_unknown_ctrl_x_chord_swallows_its_second_key_only() {
        let mut bindings = with_history(&[]);
        let _ = bindings.parse_event(key(KeyCode::Char('x'), KeyModifiers::CONTROL));
        assert_eq!(
            bindings.parse_event(key(KeyCode::Char('q'), KeyModifiers::NONE)),
            reedline::ReedlineEvent::None
        );
        assert_eq!(
            bindings.parse_event(key(KeyCode::Char('q'), KeyModifiers::NONE)),
            reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::InsertChar('q')])
        );
    }

    #[test]
    fn a_ctrl_x_binding_of_the_users_wins_over_the_chord() {
        let mut bindings = with_history(&[]);
        bindings
            .bind(
                KeyStroke {
                    control: true,
                    alt: false,
                    shift: false,
                    key: Key::Character('x'),
                }
                .into(),
                KeyAction::ShellCommand("echo mine".into()),
            )
            .unwrap();
        assert_eq!(
            bindings.parse_event(key(KeyCode::Char('x'), KeyModifiers::CONTROL)),
            reedline::ReedlineEvent::ExecuteHostCommand("echo mine".into())
        );
    }
}
