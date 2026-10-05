use std::{
    collections::HashMap,
    fmt::{self, Display, Formatter},
};

/// Represents an action that can be taken in response to a key sequence.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum KeyAction {
    /// Execute a shell command.
    ShellCommand(String),
    /// Execute an input "function".
    DoInputFunction(InputFunction),
    /// Execute a sequence of actions (in order).
    Sequence(Vec<Self>),
}

impl Display for KeyAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShellCommand(command) => write!(f, "shell command: {command}"),
            Self::DoInputFunction(function) => function.fmt(f),
            Self::Sequence(actions) => {
                write!(f, "sequence[")?;
                for (i, action) in actions.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    action.fmt(f)?;
                }
                write!(f, "]")
            }
        }
    }
}

/// Defines all input functions. Based on standard `readline` functions,
/// augmented with some cash-specific extensions (`cash-accept-hint`,
/// `cash-accept-hint-word`, which were brush's).
#[derive(
    Clone,
    Debug,
    Eq,
    Hash,
    PartialEq,
    strum_macros::EnumString,
    strum_macros::Display,
    strum_macros::EnumIter,
    strum_macros::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
#[expect(missing_docs)]
pub enum InputFunction {
    Abort,
    AcceptLine,
    AliasExpandLine,
    ArrowKeyPrefix,
    BackwardByte,
    BackwardChar,
    BackwardDeleteChar,
    BackwardKillLine,
    BackwardKillWord,
    BackwardWord,
    BashViComplete,
    BeginningOfHistory,
    BeginningOfLine,
    BracketedPasteBegin,
    CallLastKbdMacro,
    CapitalizeWord,
    CashAcceptHint,
    CashAcceptHintWord,
    /// cash: the file and folder picker, croot (spec D73).
    CashPicker,
    CharacterSearch,
    CharacterSearchBackward,
    ClearDisplay,
    ClearScreen,
    Complete,
    CompleteCommand,
    CompleteFilename,
    CompleteHostname,
    CompleteIntoBraces,
    CompleteUsername,
    CompleteVariable,
    CopyBackwardWord,
    CopyForwardWord,
    CopyRegionAsKill,
    DabbrevExpand,
    DeleteChar,
    DeleteCharOrList,
    DeleteHorizontalSpace,
    DigitArgument,
    DisplayShellVersion,
    DoLowercaseVersion,
    DowncaseWord,
    DumpFunctions,
    DumpMacros,
    DumpVariables,
    DynamicCompleteHistory,
    EditAndExecuteCommand,
    EmacsEditingMode,
    EndKbdMacro,
    EndOfHistory,
    EndOfLine,
    ExchangePointAndMark,
    ExecuteNamedCommand,
    ExportCompletions,
    FetchHistory,
    ForwardBackwardDeleteChar,
    ForwardByte,
    ForwardChar,
    ForwardSearchHistory,
    ForwardWord,
    GlobCompleteWord,
    GlobExpandWord,
    GlobListExpansions,
    HistoryAndAliasExpandLine,
    HistoryExpandLine,
    HistorySearchBackward,
    HistorySearchForward,
    HistorySubstringSearchBackward,
    HistorySubstringSearchForward,
    InsertComment,
    InsertCompletions,
    InsertLastArgument,
    KillLine,
    KillRegion,
    KillWholeLine,
    KillWord,
    MagicSpace,
    MenuComplete,
    MenuCompleteBackward,
    NextHistory,
    NextScreenLine,
    NonIncrementalForwardSearchHistory,
    NonIncrementalForwardSearchHistoryAgain,
    NonIncrementalReverseSearchHistory,
    NonIncrementalReverseSearchHistoryAgain,
    OldMenuComplete,
    OperateAndGetNext,
    OverwriteMode,
    PossibleCommandCompletions,
    PossibleCompletions,
    PossibleFilenameCompletions,
    PossibleHostnameCompletions,
    PossibleUsernameCompletions,
    PossibleVariableCompletions,
    PreviousHistory,
    PreviousScreenLine,
    PrintLastKbdMacro,
    QuotedInsert,
    ReReadInitFile,
    RedrawCurrentLine,
    ReverseSearchHistory,
    RevertLine,
    SelfInsert,
    SetMark,
    ShellBackwardKillWord,
    ShellBackwardWord,
    ShellExpandLine,
    ShellForwardWord,
    ShellKillWord,
    ShellTransposeWords,
    SkipCsiSequence,
    SpellCorrectWord,
    StartKbdMacro,
    TabInsert,
    TildeExpand,
    TransposeChars,
    TransposeWords,
    TtyStatus,
    Undo,
    UniversalArgument,
    UnixFilenameRubout,
    UnixLineDiscard,
    UnixWordRubout,
    UpcaseWord,
    ViAppendEol,
    ViAppendMode,
    ViArgDigit,
    #[strum(serialize = "vi-bWord")]
    ViBWord,
    ViBackToIndent,
    ViBackwardBigword,
    ViBackwardWord,
    ViBword,
    ViChangeCase,
    ViChangeChar,
    ViChangeTo,
    ViCharSearch,
    ViColumn,
    ViComplete,
    ViDelete,
    ViDeleteTo,
    #[strum(serialize = "vi-eWord")]
    ViEWord,
    ViEditAndExecuteCommand,
    ViEditingMode,
    ViEndBigword,
    ViEndWord,
    ViEofMaybe,
    ViEword,
    #[strum(serialize = "vi-fWord")]
    ViFWord,
    ViFetchHistory,
    ViFirstPrint,
    ViForwardBigword,
    ViForwardWord,
    ViFword,
    ViGotoMark,
    ViInsertBeg,
    ViInsertionMode,
    ViMatch,
    ViMovementMode,
    ViNextWord,
    ViOverstrike,
    ViOverstrikeDelete,
    ViPrevWord,
    ViPut,
    ViRedo,
    ViReplace,
    ViRubout,
    ViSearch,
    ViSearchAgain,
    ViSetMark,
    ViSubst,
    ViTildeExpand,
    ViUndo,
    ViUnixWordRubout,
    ViYankArg,
    ViYankPop,
    ViYankTo,
    Yank,
    YankLastArg,
    YankNthArg,
    YankPop,
}

/// Represents a sequence of keys.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum KeySequence {
    /// Strokes that make up the sequence.
    Strokes(Vec<KeyStroke>),
    /// Raw bytes that were used to generate this sequence.
    Bytes(Vec<Vec<u8>>),
}

impl Display for KeySequence {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Strokes(strokes) => {
                for stroke in strokes {
                    stroke.fmt(f)?;
                }
            }
            Self::Bytes(bytes) => {
                for byte in bytes.iter().flatten() {
                    if !byte.is_ascii_control() {
                        write!(f, "{}", *byte as char)?;
                    } else if *byte == b'\x1b' {
                        write!(f, r"\e")?;
                    } else if *byte >= 0x01 && *byte <= 0x1A {
                        // Control characters: display as \C-<letter>
                        let letter = (b'a' + (*byte - 1)) as char;
                        write!(f, r"\C-{letter}")?;
                    } else {
                        write!(f, r"\x{byte:02x}")?;
                    }
                }
            }
        }

        Ok(())
    }
}

impl From<KeyStroke> for KeySequence {
    /// Creates a new key sequence with a single stroke.
    fn from(value: KeyStroke) -> Self {
        Self::Strokes(vec![value])
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
/// Represents a single key press.
pub struct KeyStroke {
    /// Alt key was pressed.
    pub alt: bool,
    /// Control key was pressed.
    pub control: bool,
    /// Shift key was pressed.
    pub shift: bool,
    /// Primary key pressed.
    pub key: Key,
}

impl Display for KeyStroke {
    /// The stroke as Readline spells a key sequence, so that `bind -p` prints what
    /// `bind` reads back: `\C-a`, `\ex`, and for a key that sends an escape sequence the
    /// sequence xterm sends, `\e[H`, its modifiers encoded the way xterm encodes them,
    /// `\e[1;5H` for Ctrl-Home.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some((csi, last)) = self.key.csi_parts() {
            let modifier =
                1 + u8::from(self.shift) + 2 * u8::from(self.alt) + 4 * u8::from(self.control);
            return match (modifier, csi) {
                (1, "") => write!(f, "\\e[{last}"),
                (1, csi) => write!(f, "\\e[{csi}{last}"),
                (_, "") => write!(f, "\\e[1;{modifier}{last}"),
                (_, csi) => write!(f, "\\e[{csi};{modifier}{last}"),
            };
        }
        if self.alt {
            write!(f, "\\e")?;
        }
        if self.control {
            write!(f, "\\C-")?;
        }
        self.key.fmt(f)
    }
}

impl KeyStroke {
    /// Every spelling of the stroke a terminal sends: an unmodified arrow, Home or End
    /// also arrives as `\eO…` in application cursor mode, and Bash lists both.
    pub fn spellings(&self) -> Vec<String> {
        let mut spellings = vec![self.to_string()];
        if !(self.alt || self.control || self.shift)
            && let Some(("", last)) = self.key.csi_parts()
            && matches!(last, 'A' | 'B' | 'C' | 'D' | 'H' | 'F')
        {
            spellings.push(format!("\\eO{last}"));
        }
        spellings
    }
}

impl From<Key> for KeyStroke {
    /// Creates a new key stroke with a single key.
    fn from(value: Key) -> Self {
        Self {
            alt: false,
            control: false,
            shift: false,
            key: value,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
/// Represents a single key.
pub enum Key {
    /// A simple character key.
    Character(char),
    /// Backspace key.
    Backspace,
    /// Enter key.
    Enter,
    /// Left arrow key.
    Left,
    /// Right arrow key.
    Right,
    /// Up arrow key.
    Up,
    /// Down arrow key.
    Down,
    /// Home key.
    Home,
    /// End key.
    End,
    /// Page up key.
    PageUp,
    /// Page down key.
    PageDown,
    /// Tab key.
    Tab,
    /// Shift + Tab key.
    BackTab,
    /// Delete key.
    Delete,
    /// Insert key.
    Insert,
    /// F key.
    F(u8),
    /// Escape key.
    Escape,
}

impl Key {
    /// For a key that sends an xterm CSI sequence, its parameter (empty for none) and
    /// final character: Home is `\e[H` (`("", 'H')`), Delete `\e[3~` (`("3", '~')`).
    const fn csi_parts(&self) -> Option<(&'static str, char)> {
        Some(match self {
            Self::Up => ("", 'A'),
            Self::Down => ("", 'B'),
            Self::Right => ("", 'C'),
            Self::Left => ("", 'D'),
            Self::Home => ("", 'H'),
            Self::End => ("", 'F'),
            Self::BackTab => ("", 'Z'),
            Self::Insert => ("2", '~'),
            Self::Delete => ("3", '~'),
            Self::PageUp => ("5", '~'),
            Self::PageDown => ("6", '~'),
            Self::F(5) => ("15", '~'),
            Self::F(6) => ("17", '~'),
            Self::F(7) => ("18", '~'),
            Self::F(8) => ("19", '~'),
            Self::F(9) => ("20", '~'),
            Self::F(10) => ("21", '~'),
            Self::F(11) => ("23", '~'),
            Self::F(12) => ("24", '~'),
            _ => return None,
        })
    }
}

impl Display for Key {
    /// The key as Readline spells it in a key sequence: the control character a key
    /// sends (`\C-m` for Enter, `\C-?` for Backspace), or its xterm sequence.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some((csi, last)) = self.csi_parts() {
            return write!(f, "\\e[{csi}{last}");
        }
        match self {
            Self::Character(c @ ('\\' | '\"' | '\'')) => write!(f, "\\{c}")?,
            Self::Character(c) => write!(f, "{c}")?,
            Self::Backspace => write!(f, "\\C-?")?,
            Self::Enter => write!(f, "\\C-m")?,
            Self::Tab => write!(f, "\\C-i")?,
            Self::Escape => write!(f, "\\e")?,
            Self::F(n @ 1..=4) => write!(f, "\\eO{}", char::from(b'O' + n))?,
            Self::F(n) => write!(f, "F{n}")?,
            _ => {}
        }

        Ok(())
    }
}

/// Encapsulates the shell's interaction with key bindings for input.
pub trait KeyBindings: Send {
    /// Retrieves current bindings.
    fn get_current(&self) -> HashMap<KeySequence, KeyAction>;

    /// Tries to find a binding for an untranslated byte sequence.
    fn get_untranslated(&self, bytes: &[u8]) -> Option<&KeyAction>;

    /// Sets or updates a binding.
    ///
    /// # Arguments
    ///
    /// * `seq` - The key sequence to bind.
    /// * `action` - The action to bind to the sequence.
    fn bind(&mut self, seq: KeySequence, action: KeyAction) -> Result<(), std::io::Error>;

    /// Unbinds a key sequence. Returns true if a binding was removed.
    ///
    /// # Arguments
    ///
    /// * `seq` - The key sequence to unbind.
    fn try_unbind(&mut self, seq: KeySequence) -> bool;

    /// Defines a macro that remaps a key sequence to another key sequence.
    ///
    /// # Arguments
    ///
    /// * `seq` - The key sequence to bind the macro to.
    /// * `target` - The sequence that makes up the macro.
    fn define_macro(&mut self, seq: KeySequence, target: KeySequence)
    -> Result<(), std::io::Error>;

    /// Retrieves all defined macros.
    fn get_macros(&self) -> HashMap<KeySequence, KeySequence>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(key: Key, control: bool, alt: bool) -> KeyStroke {
        KeyStroke {
            alt,
            control,
            shift: false,
            key,
        }
    }

    #[test]
    fn keys_are_spelled_as_readline_reads_them() {
        assert_eq!(
            stroke(Key::Character('a'), true, false).to_string(),
            r"\C-a"
        );
        assert_eq!(stroke(Key::Character('x'), false, true).to_string(), r"\ex");
        assert_eq!(stroke(Key::Home, false, false).to_string(), r"\e[H");
        assert_eq!(stroke(Key::Home, true, false).to_string(), r"\e[1;5H");
        assert_eq!(stroke(Key::Delete, false, true).to_string(), r"\e[3;3~");
        assert_eq!(stroke(Key::Enter, false, false).to_string(), r"\C-m");
        assert_eq!(stroke(Key::Backspace, false, false).to_string(), r"\C-?");
        assert_eq!(stroke(Key::F(1), false, false).to_string(), r"\eOP");
        assert_eq!(stroke(Key::F(5), false, false).to_string(), r"\e[15~");
    }

    #[test]
    fn arrows_home_and_end_have_both_spellings() {
        assert_eq!(
            stroke(Key::Up, false, false).spellings(),
            vec![r"\e[A".to_owned(), r"\eOA".to_owned()]
        );
        assert_eq!(
            stroke(Key::Up, true, false).spellings(),
            vec![r"\e[1;5A".to_owned()]
        );
        assert_eq!(
            stroke(Key::Delete, false, false).spellings(),
            vec![r"\e[3~".to_owned()]
        );
    }

    /// `bind` takes the history hint's functions by cash's names, not brush's (ARCH-10).
    #[test]
    fn the_hint_functions_have_cash_names() {
        use std::str::FromStr as _;

        assert_eq!(
            InputFunction::from_str("cash-accept-hint"),
            Ok(InputFunction::CashAcceptHint)
        );
        assert_eq!(
            InputFunction::from_str("cash-accept-hint-word"),
            Ok(InputFunction::CashAcceptHintWord)
        );
        assert!(InputFunction::from_str("brush-accept-hint").is_err());
    }
}
