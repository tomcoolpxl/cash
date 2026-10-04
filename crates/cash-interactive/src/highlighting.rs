//! Generic syntax highlighting for shell commands.
//!
//! This module provides semantic tagging of shell command strings without
//! imposing any specific styling. Consumers can map the semantic categories
//! to their own color schemes or styles.

/// Semantic category for a highlighted span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightKind {
    /// Default text
    Default,
    /// Comment text
    Comment,
    /// Arithmetic expression
    Arithmetic,
    /// Parameter expansion (variables, etc.)
    Parameter,
    /// Command substitution
    CommandSubstitution,
    /// Quoted text
    Quoted,
    /// Operator (|, &&, etc.)
    Operator,
    /// Variable assignment
    Assignment,
    /// Hyphen-prefixed option
    HyphenOption,
    /// Function definition
    Function,
    /// Shell keyword
    Keyword,
    /// Builtin command
    Builtin,
    /// Alias
    Alias,
    /// External command (found in PATH)
    ExternalCommand,
    /// Command not found
    NotFoundCommand,
    /// Unknown command (cursor still in token)
    UnknownCommand,
}

impl HighlightKind {
    /// Whether a word of this kind stands where a command does.
    #[must_use]
    pub const fn is_command(self) -> bool {
        matches!(
            self,
            Self::Function
                | Self::Keyword
                | Self::Builtin
                | Self::Alias
                | Self::ExternalCommand
                | Self::NotFoundCommand
                | Self::UnknownCommand
        )
    }
}

/// Whether the word at byte `word_start` of `line` is an abbreviation that expands there.
///
/// It must be one `abbr` defines now (D60), standing as a command unless it was defined
/// with `--position anywhere`, and never inside quotes or a comment.
#[must_use]
pub fn abbreviation_applies(
    shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
    line: &str,
    word_start: usize,
) -> bool {
    let word = line
        .get(word_start..)
        .and_then(|rest| rest.split(char::is_whitespace).next())
        .unwrap_or_default();
    let Some(abbreviation) = shell.abbreviations().get(word) else {
        return false;
    };

    // With the cursor on the word, its command lookup is skipped: only its place matters.
    let highlighted = highlight_command(shell, line, word_start);
    let Some(span) = highlighted
        .spans()
        .iter()
        .find(|span| span.range.start == word_start)
    else {
        return false;
    };

    match abbreviation.position {
        cash_core::abbreviations::Position::Command => span.kind.is_command(),
        cash_core::abbreviations::Position::Anywhere => {
            !matches!(span.kind, HighlightKind::Quoted | HighlightKind::Comment)
        }
    }
}

/// A highlighted span of text with semantic meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    /// Byte range of this span within the input string.
    pub range: std::ops::Range<usize>,
    /// Semantic category of this span.
    pub kind: HighlightKind,
}

impl HighlightSpan {
    /// Creates a new highlight span over the given byte range.
    #[must_use]
    pub const fn new(range: std::ops::Range<usize>, kind: HighlightKind) -> Self {
        Self { range, kind }
    }
}

/// The result of highlighting a line: the spans plus the line they index into.
///
/// Pairing the two means span text is resolved against the original input
/// rather than a separately-supplied (and possibly mismatched) string.
pub struct Highlighted<'a> {
    line: &'a str,
    spans: Vec<HighlightSpan>,
}

impl<'a> Highlighted<'a> {
    /// The line that was highlighted.
    #[must_use]
    pub const fn line(&self) -> &'a str {
        self.line
    }

    /// The spans, in order, covering the entire line.
    #[must_use]
    pub fn spans(&self) -> &[HighlightSpan] {
        &self.spans
    }

    /// Returns the text of `span` within the highlighted line.
    ///
    /// `span` is expected to be one of [`Self::spans`]; a range that is out of
    /// bounds or off a UTF-8 char boundary (debug-asserted) yields `""` rather
    /// than panicking.
    #[must_use]
    pub fn text(&self, span: &HighlightSpan) -> &'a str {
        debug_assert!(
            self.line.is_char_boundary(span.range.start),
            "highlight span start {} is not a UTF-8 char boundary in {:?}",
            span.range.start,
            self.line,
        );
        debug_assert!(
            self.line.is_char_boundary(span.range.end),
            "highlight span end {} is not a UTF-8 char boundary in {:?}",
            span.range.end,
            self.line,
        );
        self.line.get(span.range.clone()).unwrap_or("")
    }

    /// Iterates `(kind, text)` for each span, with text borrowed from the line.
    pub fn iter(&self) -> impl Iterator<Item = (HighlightKind, &'a str)> + '_ {
        self.spans.iter().map(|span| (span.kind, self.text(span)))
    }
}

/// Highlights a shell command string.
///
/// # Arguments
/// * `shell` - Reference to the shell for context (aliases, functions, builtins, etc.)
/// * `line` - The command string to highlight
/// * `cursor` - Current cursor position (byte offset)
///
/// # Returns
/// The highlighted line: spans covering the entire input, paired with the input.
#[must_use]
pub fn highlight_command<'a>(
    shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
    line: &'a str,
    cursor: usize,
) -> Highlighted<'a> {
    let mut highlighter = Highlighter::new(shell, line, cursor);
    highlighter.highlight_program(line, 0);
    Highlighted {
        line,
        spans: highlighter.spans,
    }
}

/// Operators after which the next word is a command: the list and pipeline separators, and
/// the opening of a subshell.
pub(crate) const STARTS_A_COMMAND: &[&str] = &[";", "&", "&&", "||", "|", "|&", "(", "\n"];

/// Reserved words that are followed by a command rather than by a name or a word list.
pub(crate) const KEYWORDS_BEFORE_A_COMMAND: &[&str] = &[
    "if", "then", "elif", "else", "while", "until", "do", "!", "{", "time",
];

enum CommandType {
    Function,
    Keyword,
    Builtin,
    Alias,
    External,
    NotFound,
    Unknown,
}

struct Highlighter<'a, SE: cash_core::ShellExtensions> {
    shell: &'a cash_core::Shell<SE>,
    /// The topmost input line; span offsets stored in `spans` index into this string.
    input_line: &'a str,
    cursor: usize,
    spans: Vec<HighlightSpan>,
    current_byte_index: usize,
    next_missing_kind: Option<HighlightKind>,
}

impl<'a, SE: cash_core::ShellExtensions> Highlighter<'a, SE> {
    const fn new(shell: &'a cash_core::Shell<SE>, input_line: &'a str, cursor: usize) -> Self {
        Self {
            shell,
            input_line,
            cursor,
            spans: Vec::new(),
            current_byte_index: 0,
            next_missing_kind: None,
        }
    }

    fn highlight_program(&mut self, line: &str, global_offset: usize) {
        if let Ok(tokens) = cash_parser::tokenize_str_with_options(
            line,
            &(self.shell.parser_options().tokenizer_options()),
        ) {
            let mut saw_command_token = false;

            // Tokenizer offsets are *character* indices into `line`; slicing needs bytes.
            let offsets = cash_parser::CharByteOffsets::new(line);
            let byte_offset = |char_offset: usize| offsets.byte(char_offset);

            for token in tokens {
                match token {
                    cash_parser::Token::Operator(op, token_location) => {
                        let start = global_offset + byte_offset(token_location.start.index);
                        let end = global_offset + byte_offset(token_location.end.index);
                        self.append_span(HighlightKind::Operator, start..end);
                        // A control operator ends one command, so the next word starts
                        // another: `ls | nosuch` marks `nosuch`, as fish does. Redirections
                        // are operators too, and leave the command as it was.
                        if STARTS_A_COMMAND.contains(&op.as_str()) {
                            saw_command_token = false;
                        }
                    }
                    cash_parser::Token::Word(w, token_location) => {
                        let start_byte = byte_offset(token_location.start.index);
                        let end_byte = byte_offset(token_location.end.index);

                        // Parse the raw slice from `line`, not `w.as_str()`: the tokenizer may
                        // drop chars from `w` (e.g. `\<newline>` continuations), so offsets into
                        // `w` no longer map onto `line`; offsets into the raw slice do.
                        let raw_word_text = line.get(start_byte..end_byte).unwrap_or("");
                        if let Ok(word_pieces) =
                            cash_parser::word::parse(raw_word_text, &self.shell.parser_options())
                        {
                            let token_range =
                                (global_offset + start_byte)..(global_offset + end_byte);

                            // Classify against the tokenized form `w` (the logical word) so
                            // command lookups ignore mid-word line continuations.
                            let default_text_kind = self.get_kind_for_word(
                                w.as_str(),
                                &word_pieces,
                                &token_range,
                                &mut saw_command_token,
                            );

                            for word_piece in word_pieces {
                                self.highlight_word_piece(
                                    word_piece,
                                    default_text_kind,
                                    token_range.start,
                                );
                            }
                        }
                    }
                }
            }

            self.skip_ahead(global_offset + line.len());
        } else {
            self.append_span(
                HighlightKind::Default,
                global_offset..global_offset + line.len(),
            );
        }
    }

    fn highlight_word_piece(
        &mut self,
        word_piece: cash_parser::word::WordPieceWithSource,
        default_text_kind: HighlightKind,
        global_offset: usize,
    ) {
        let piece =
            (global_offset + word_piece.start_index)..(global_offset + word_piece.end_index);
        self.skip_ahead(piece.start);

        match word_piece.piece {
            // `winpaths` (D53) keeps the backslashes of `C:\Users\me` by making each `\U`
            // literal text: a path separator, part of the word rather than a quote.
            cash_parser::word::WordPiece::SingleQuotedText(_)
                if self
                    .input_line
                    .get(piece.clone())
                    .is_some_and(|source| source.starts_with('\\')) =>
            {
                self.append_span(default_text_kind, piece.clone());
            }
            cash_parser::word::WordPiece::SingleQuotedText(_)
            | cash_parser::word::WordPiece::AnsiCQuotedText(_)
            | cash_parser::word::WordPiece::EscapeSequence(_) => {
                self.append_span(HighlightKind::Quoted, piece.clone());
            }
            cash_parser::word::WordPiece::DoubleQuotedSequence(subpieces)
            | cash_parser::word::WordPiece::GettextDoubleQuotedSequence(subpieces) => {
                self.set_next_missing_kind(HighlightKind::Quoted);
                for subpiece in subpieces {
                    self.highlight_word_piece(subpiece, HighlightKind::Quoted, global_offset);
                }
                self.set_next_missing_kind(HighlightKind::Quoted);
            }
            cash_parser::word::WordPiece::ParameterExpansion(_)
            | cash_parser::word::WordPiece::TildeExpansion(_) => {
                self.append_span(HighlightKind::Parameter, piece.clone());
            }
            cash_parser::word::WordPiece::BackquotedCommandSubstitution(command) => {
                self.set_next_missing_kind(HighlightKind::CommandSubstitution);
                self.highlight_program(
                    command.as_str(),
                    piece.start + 1, /* opening backtick */
                );
                self.set_next_missing_kind(HighlightKind::CommandSubstitution);
            }
            cash_parser::word::WordPiece::CommandSubstitution(command)
            | cash_parser::word::WordPiece::ProcessSubstitution { command, .. } => {
                self.set_next_missing_kind(HighlightKind::CommandSubstitution);
                self.highlight_program(
                    command.as_str(),
                    piece.start + 2, /* opening $( or <( */
                );
                self.set_next_missing_kind(HighlightKind::CommandSubstitution);
            }
            cash_parser::word::WordPiece::CurrentShellCommandSubstitution { command, reply } => {
                self.set_next_missing_kind(HighlightKind::CommandSubstitution);
                self.highlight_program(command.as_str(), piece.start + if reply { 3 } else { 2 });
                self.set_next_missing_kind(HighlightKind::CommandSubstitution);
            }
            cash_parser::word::WordPiece::ArithmeticExpression(_) => {
                // TODO(highlighting): Consider individually highlighting pieces of the expression
                // itself.
                self.append_span(HighlightKind::Arithmetic, piece.clone());
            }
            cash_parser::word::WordPiece::Text(_)
            | cash_parser::word::WordPiece::BadSubstitution(_) => {
                self.append_span(default_text_kind, piece.clone());
            }
        }

        self.skip_ahead(piece.end);
    }

    fn append_span(&mut self, kind: HighlightKind, range: std::ops::Range<usize>) {
        debug_assert!(
            self.input_line.is_char_boundary(range.start),
            "span start {} is not a UTF-8 char boundary in {:?}",
            range.start,
            self.input_line,
        );
        debug_assert!(
            self.input_line.is_char_boundary(range.end),
            "span end {} is not a UTF-8 char boundary in {:?}",
            range.end,
            self.input_line,
        );

        // See if we need to cover a gap between this substring and the one that preceded it.
        if range.start > self.current_byte_index {
            let missing_kind = self.next_missing_kind.unwrap_or(HighlightKind::Comment);
            self.spans.push(HighlightSpan::new(
                self.current_byte_index..range.start,
                missing_kind,
            ));
            self.current_byte_index = range.start;
        }

        let end = range.end;
        if !range.is_empty() {
            self.spans.push(HighlightSpan::new(range, kind));
        }

        self.current_byte_index = end;
    }

    fn skip_ahead(&mut self, dest: usize) {
        // Append a no-op span to make sure we cover any trailing gaps in the input line not
        // otherwise styled.
        self.append_span(HighlightKind::Default, dest..dest);
    }

    const fn set_next_missing_kind(&mut self, kind: HighlightKind) {
        self.next_missing_kind = Some(kind);
    }

    fn get_kind_for_word(
        &self,
        w: &str,
        pieces: &[cash_parser::word::WordPieceWithSource],
        token_range: &std::ops::Range<usize>,
        saw_command_token: &mut bool,
    ) -> HighlightKind {
        if !*saw_command_token {
            if w.contains('=') {
                HighlightKind::Assignment
            } else {
                // After `if`, `then`, `do`, `!` and the like, the next word is a command too.
                *saw_command_token = !KEYWORDS_BEFORE_A_COMMAND.contains(&w);
                match self.classify_possible_command(w, pieces, token_range) {
                    CommandType::Function => HighlightKind::Function,
                    CommandType::Keyword => HighlightKind::Keyword,
                    CommandType::Builtin => HighlightKind::Builtin,
                    CommandType::Alias => HighlightKind::Alias,
                    CommandType::External => HighlightKind::ExternalCommand,
                    CommandType::NotFound => HighlightKind::NotFoundCommand,
                    CommandType::Unknown => HighlightKind::UnknownCommand,
                }
            }
        } else {
            if self.shell.is_keyword(w) {
                HighlightKind::Keyword
            } else if w.starts_with('-') {
                HighlightKind::HyphenOption
            } else {
                HighlightKind::Default
            }
        }
    }

    fn classify_possible_command(
        &self,
        name: &str,
        pieces: &[cash_parser::word::WordPieceWithSource],
        token_range: &std::ops::Range<usize>,
    ) -> CommandType {
        if self.shell.is_keyword(name) {
            return CommandType::Keyword;
        } else if self.shell.aliases().contains_key(name) {
            return CommandType::Alias;
        } else if self.shell.funcs().get(name).is_some() {
            return CommandType::Function;
        } else if self.shell.builtins().contains_key(name) {
            return CommandType::Builtin;
        }

        // Short-circuit if the cursor is still in this token (inclusive of its end, so a
        // command still being typed isn't prematurely flagged as not-found).
        if self.cursor >= token_range.start && self.cursor <= token_range.end {
            return CommandType::Unknown;
        }

        // Whether the command exists is asked of the word the shell will run, not of its
        // spelling: `"C:\Program Files\Git\bin\git.exe"` is a path once its quotes go, and
        // with `winpaths` off `C:\tools\x.exe` runs as `C:toolsx.exe`. A word whose meaning
        // waits on an expansion stays neutral.
        let Some(command) = literal_word(pieces) else {
            return CommandType::Unknown;
        };

        if cash_core::sys::fs::contains_path_separator(&command) {
            // One look at a local file is cheap; on the network it can wait on a server,
            // for seconds when it is offline, on every keystroke (PI-10, D59). Such a
            // word stays neutral.
            let absolute = self.shell.absolute_path(std::path::Path::new(&command));
            if cash_win32::path::is_on_network(&absolute) {
                CommandType::Unknown
            } else if self.shell.is_runnable_path(&command) {
                CommandType::External
            } else {
                CommandType::NotFound
            }
        } else {
            // From the background PATH listing: probing every PATH directory for every
            // PATHEXT extension here would cost a missing name over a hundred milliseconds
            // on each keystroke. Until the listing is ready, the word stays neutral.
            match self.shell.executable_on_path_if_known(&command) {
                Some(true) => CommandType::External,
                Some(false) => CommandType::NotFound,
                None => CommandType::Unknown,
            }
        }
    }
}

/// Unquoted characters that make a word mean something only expansion settles: a glob
/// or a brace expansion.
const EXPANDED_WHEN_UNQUOTED: [char; 4] = ['*', '?', '[', '{'];

/// The text a word stands for once its quotes are removed, when that is known without
/// expanding anything; `None` for a word with an expansion, a substitution, a glob or
/// braces in it.
///
/// The pieces come from the word parser, so `winpaths` (D53) has already settled which
/// backslashes of `C:\Users\me` are path separators and which are escapes.
fn literal_word(pieces: &[cash_parser::word::WordPieceWithSource]) -> Option<String> {
    let mut literal = String::new();
    for piece in pieces {
        push_literal(&piece.piece, false, &mut literal)?;
    }
    Some(literal)
}

fn push_literal(
    piece: &cash_parser::word::WordPiece,
    in_double_quotes: bool,
    literal: &mut String,
) -> Option<()> {
    use cash_parser::word::WordPiece;

    match piece {
        WordPiece::Text(text) if in_double_quotes || !text.contains(EXPANDED_WHEN_UNQUOTED) => {
            literal.push_str(text);
        }
        WordPiece::SingleQuotedText(text) => literal.push_str(text),
        // The parser only makes an escape of a character the backslash quotes, so the
        // backslash goes; `\<newline>` is a line continuation and stands for nothing.
        WordPiece::EscapeSequence(escape) => {
            literal.extend(escape.strip_prefix('\\')?.chars().filter(|&c| c != '\n'));
        }
        WordPiece::DoubleQuotedSequence(pieces) => {
            for piece in pieces {
                push_literal(&piece.piece, true, literal)?;
            }
        }
        _ => return None,
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_highlight_simple_command() {
        let shell = cash_core::Shell::builder().build().await.unwrap();
        let line = "somecommand hello";
        // Use cursor position at the end so we get final highlighting
        let highlighted = highlight_command(&shell, line, line.len());

        // Should have at least 2 spans
        assert!(!highlighted.spans().is_empty());

        // Verify highlighting produces spans that cover the input
        let total_covered: usize = highlighted.spans().iter().map(|s| s.range.len()).sum();
        assert_eq!(total_covered, line.len(), "Spans should cover entire input");

        // The command should be classified as something (NotFound, External, etc.)
        let cmd_span = highlighted
            .spans()
            .iter()
            .find(|s| highlighted.text(s) == "somecommand");
        assert!(cmd_span.is_some(), "Should have a span for the command");
    }

    /// The kind of the span that is exactly `word` in `line`.
    fn word_kind(
        shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
        line: &str,
        word: &str,
    ) -> HighlightKind {
        let highlighted = highlight_command(shell, line, line.len());
        let span = highlighted
            .spans()
            .iter()
            .find(|s| highlighted.text(s) == word);
        span.unwrap().kind
    }

    fn command_kind(
        shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
        line: &str,
    ) -> HighlightKind {
        word_kind(shell, line, line.split_whitespace().next().unwrap())
    }

    /// The command word's kind once the background PATH listing has answered.
    async fn settled_command_kind(
        shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
        line: &str,
    ) -> HighlightKind {
        settled_word_kind(shell, line, line.split_whitespace().next().unwrap()).await
    }

    /// `word`'s kind once the background PATH listing has answered.
    async fn settled_word_kind(
        shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
        line: &str,
        word: &str,
    ) -> HighlightKind {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let kind = word_kind(shell, line, word);
            if kind != HighlightKind::UnknownCommand {
                return kind;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the PATH listing never answered"
            );
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }

    #[tokio::test]
    async fn command_existence_comes_from_the_path_listing_without_waiting() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tool.exe"), b"MZ").unwrap();

        let mut shell = cash_core::Shell::builder().build().await.unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        shell
            .set_env_global("PATH", cash_core::ShellVariable::new(path))
            .unwrap();

        // Asked before the listing exists, the word stays neutral rather than waiting.
        assert_eq!(
            command_kind(&shell, "tool x"),
            HighlightKind::UnknownCommand
        );

        assert_eq!(
            settled_command_kind(&shell, "tool x").await,
            HighlightKind::ExternalCommand
        );
        assert_eq!(
            settled_command_kind(&shell, "no-such-tool x").await,
            HighlightKind::NotFoundCommand
        );
    }

    #[tokio::test]
    async fn an_abbreviation_applies_where_it_is_defined_to() {
        use cash_core::abbreviations::{Abbreviation, Position};

        let mut shell = cash_core::Shell::builder().build().await.unwrap();
        for (name, position) in [("gco", Position::Command), ("L", Position::Anywhere)] {
            shell.abbreviations_mut().set(Abbreviation {
                name: name.into(),
                expansion: "x".into(),
                position,
            });
        }

        // (line as the editor holds it, byte where the word starts, expands)
        for (line, start, expands) in [
            ("gco ", 0, true),
            ("gco", 0, true),
            ("  gco ", 2, true),
            ("echo a | gco ", 9, true),
            ("FOO=1 gco ", 6, true),
            ("echo gco ", 5, false),
            ("echo 'gco ", 5, false),
            ("gcox ", 0, false),
            ("ls L ", 3, true),
            ("L ", 0, true),
            ("echo 'a L ", 8, false),
            ("# L ", 2, false),
        ] {
            assert_eq!(
                abbreviation_applies(&shell, line, start),
                expands,
                "{line:?} at {start}"
            );
        }

        shell.abbreviations_mut().remove("gco");
        assert!(!abbreviation_applies(&shell, "gco ", 0));
    }

    #[tokio::test]
    async fn every_command_of_a_pipeline_or_list_is_a_command() {
        let dir = tempfile::tempdir().unwrap();
        let mut shell = cash_core::Shell::builder().build().await.unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        shell
            .set_env_global("PATH", cash_core::ShellVariable::new(path))
            .unwrap();

        // Each word is followed by more text: a word the cursor is still in stays neutral.
        for (line, word) in [
            ("echo a | missing-one b", "missing-one"),
            ("echo a; missing-two b", "missing-two"),
            ("echo a && missing-three b", "missing-three"),
            ("if missing-four; then missing-five; fi", "missing-four"),
            ("if missing-four; then missing-five; fi", "missing-five"),
            ("! missing-six b", "missing-six"),
        ] {
            assert_eq!(
                settled_word_kind(&shell, line, word).await,
                HighlightKind::NotFoundCommand,
                "{word} in {line:?}"
            );
        }

        // Arguments and redirection targets are not commands.
        assert_eq!(
            word_kind(&shell, "echo a | cat plain-arg", "plain-arg"),
            HighlightKind::Default
        );
        assert_eq!(
            word_kind(&shell, "echo a > out-file", "out-file"),
            HighlightKind::Default
        );
    }

    /// `(kind, text)` of each span inside the first occurrence of `word` in `line`.
    fn word_spans(
        shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
        line: &str,
        word: &str,
    ) -> Vec<(HighlightKind, String)> {
        let start = line.find(word).unwrap();
        let end = start + word.len();
        let highlighted = highlight_command(shell, line, line.len());
        highlighted
            .spans()
            .iter()
            .filter(|span| span.range.start >= start && span.range.end <= end)
            .map(|span| (span.kind, highlighted.text(span).to_owned()))
            .collect()
    }

    #[tokio::test]
    async fn a_pasted_drive_path_is_a_path_not_escapes() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("has space")).unwrap();
        std::fs::write(root.path().join("has space").join("tool.exe"), b"MZ").unwrap();
        std::fs::write(root.path().join("notes.txt"), b"notes").unwrap();
        let dir = root.path().to_string_lossy().replace('/', r"\");
        assert!(dir.starts_with(r"C:\"), "{dir}");

        let mut shell = cash_core::Shell::builder().build().await.unwrap();
        shell.options_mut().windows_drive_paths = true;

        // `winpaths` keeps every backslash but the one before the space, which is bash's
        // escape; the command is the path they spell, found without its `.exe` too.
        for tool in [
            format!(r"{dir}\has\ space\tool.exe"),
            format!(r"{dir}\has\ space\tool"),
        ] {
            let spans = word_spans(&shell, &format!("{tool} --help"), &tool);
            assert!(
                spans.iter().all(|(kind, text)| *kind
                    == if text == r"\ " {
                        HighlightKind::Quoted
                    } else {
                        HighlightKind::ExternalCommand
                    }),
                "{spans:?}"
            );
        }

        // A directory and a file that is not a program are not commands.
        for not_a_command in [dir.clone(), format!(r"{dir}\notes.txt")] {
            let spans = word_spans(&shell, &format!("{not_a_command} x"), &not_a_command);
            assert!(
                spans
                    .iter()
                    .all(|(kind, _)| *kind == HighlightKind::NotFoundCommand),
                "{spans:?}"
            );
        }

        // As an argument the path is plain text.
        let notes = format!(r"{dir}\notes.txt");
        let spans = word_spans(&shell, &format!("cat {notes}"), &notes);
        assert!(
            spans
                .iter()
                .all(|(kind, _)| *kind == HighlightKind::Default),
            "{spans:?}"
        );

        // With `winpaths` off each backslash is an escape again, and the command is
        // `C:Users...`, a name without a separator, which is not on PATH.
        shell.options_mut().windows_drive_paths = false;
        let tool = format!(r"{dir}\has\ space\tool.exe");
        let line = format!("{tool} --help");
        assert_eq!(
            settled_word_kind(&shell, &line, "C:").await,
            HighlightKind::NotFoundCommand
        );
        let spans = word_spans(&shell, &line, &tool);
        assert!(
            spans.iter().all(|(kind, text)| *kind
                == if text.starts_with('\\') {
                    HighlightKind::Quoted
                } else {
                    HighlightKind::NotFoundCommand
                }),
            "{spans:?}"
        );
    }

    /// A command path on the network is not looked at on each keystroke: an offline
    /// server stalled typing (PI-10). It stays neutral, as one waiting on an expansion.
    #[tokio::test]
    async fn a_command_path_on_the_network_stays_neutral() {
        let shell = cash_core::Shell::builder().build().await.unwrap();
        let tool = "//cash-no-such-server.invalid/share/tool.exe";
        let started = std::time::Instant::now();
        assert_eq!(
            word_kind(&shell, &format!("{tool} x"), tool),
            HighlightKind::UnknownCommand
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[tokio::test]
    async fn a_command_path_that_waits_on_an_expansion_stays_neutral() {
        let shell = cash_core::Shell::builder().build().await.unwrap();
        for line in ["~/no/such/tool x", "$HOME/no/such/tool x"] {
            assert_eq!(
                word_kind(&shell, line, "/no/such/tool"),
                HighlightKind::UnknownCommand,
                "{line}"
            );
        }
    }

    #[tokio::test]
    async fn test_highlight_quoted_string() {
        let shell = cash_core::Shell::builder().build().await.unwrap();
        let line = r#"echo "hello world""#;
        let highlighted = highlight_command(&shell, line, 0);

        // Should have spans for: echo, space, "hello world"
        assert!(!highlighted.spans().is_empty());

        // Check that quoted parts are marked as Quoted
        assert!(
            highlighted
                .spans()
                .iter()
                .any(|s| s.kind == HighlightKind::Quoted)
        );
    }

    #[tokio::test]
    async fn test_highlight_parameter_expansion() {
        let shell = cash_core::Shell::builder().build().await.unwrap();
        let line = "echo $HOME";
        let highlighted = highlight_command(&shell, line, 0);

        // Should have spans including a parameter expansion
        assert!(
            highlighted
                .spans()
                .iter()
                .any(|s| s.kind == HighlightKind::Parameter)
        );
    }

    #[tokio::test]
    async fn test_highlight_covers_entire_input() {
        let shell = cash_core::Shell::builder().build().await.unwrap();
        let line = "echo hello world";
        let highlighted = highlight_command(&shell, line, 0);

        // Verify that spans cover the entire input (no gaps)
        let mut covered = vec![false; line.len()];
        for span in highlighted.spans() {
            for item in covered
                .iter_mut()
                .take(span.range.end)
                .skip(span.range.start)
            {
                *item = true;
            }
        }

        assert!(covered.iter().all(|&c| c), "Not all characters are covered");
    }

    /// Asserts the invariants every highlighter output must satisfy (mirrors
    /// `fuzz/fuzz_targets/fuzz_highlight.rs`).
    fn assert_spans_are_valid(highlighted: &Highlighted<'_>) {
        let line = highlighted.line();

        // 1. Each span is in-range and lands on UTF-8 char boundaries.
        for span in highlighted.spans() {
            assert!(
                span.range.start <= span.range.end,
                "span has start > end: {span:?} (line={line:?})",
            );
            assert!(
                span.range.end <= line.len(),
                "span end exceeds line length: {span:?} (line.len()={})",
                line.len(),
            );
            assert!(
                line.is_char_boundary(span.range.start),
                "span start not on char boundary: {span:?} (line={line:?}, bytes={:?})",
                line.as_bytes(),
            );
            assert!(
                line.is_char_boundary(span.range.end),
                "span end not on char boundary: {span:?} (line={line:?}, bytes={:?})",
                line.as_bytes(),
            );
        }

        // 2. Spans are ordered and contiguous, covering the entire input.
        let mut next_expected_start = 0usize;
        for span in highlighted.spans() {
            assert_eq!(
                span.range.start, next_expected_start,
                "spans are not contiguous: {span:?} (expected start={next_expected_start}, line={line:?})",
            );
            next_expected_start = span.range.end;
        }
        assert_eq!(
            next_expected_start,
            line.len(),
            "spans do not cover entire input (covered {next_expected_start} of {}, line={line:?})",
            line.len(),
        );

        // 3. Resolving each span's text must not panic.
        for (_, _) in highlighted.iter() {}
    }

    #[tokio::test]
    async fn test_highlight_multibyte_chars_in_word_does_not_panic() {
        // Regression: a multibyte word followed by another token used to panic from
        // mixing char indices (tokenizer) with byte indices (word parser).
        let shell = cash_core::Shell::builder().build().await.unwrap();
        let line = ": 爸爸 /";
        let highlighted = highlight_command(&shell, line, line.len());

        assert_spans_are_valid(&highlighted);
    }

    #[tokio::test]
    async fn test_highlight_multibyte_chars_partial_input_does_not_panic() {
        // Simulate intermediate keystrokes while a user is typing the line.
        let shell = cash_core::Shell::builder().build().await.unwrap();
        let full = ": 爸爸 /";
        // Every char-boundary prefix must highlight without panicking.
        for (boundary, _) in full
            .char_indices()
            .chain(std::iter::once((full.len(), ' ')))
        {
            // `boundary` is sourced from char_indices(), so slicing is on a char boundary.
            #[expect(clippy::string_slice)]
            let line = &full[..boundary];
            let highlighted = highlight_command(&shell, line, line.len());
            assert_spans_are_valid(&highlighted);
        }
    }

    #[tokio::test]
    async fn test_highlight_multibyte_in_various_positions() {
        let shell = cash_core::Shell::builder().build().await.unwrap();
        let cases = [
            "爸",
            "爸 x",
            "x 爸",
            "echo 爸爸",
            "爸爸=value",
            "\"爸爸\" /",
            "$爸",
            "$(爸爸) /",
            "`爸爸` /",
            "# 爸爸 comment",
        ];
        for line in cases {
            let highlighted = highlight_command(&shell, line, line.len());
            assert_spans_are_valid(&highlighted);
        }
    }

    #[tokio::test]
    async fn test_highlight_issue_1128_multibyte_then_paren() {
        // Regression for #1128: a 2-byte char immediately followed by `(` panicked
        // because the operator's char index was sliced as a byte offset, landing
        // mid-character. Exercise the reported chars, including the keystroke-by-
        // keystroke sequence (the char alone, then the char + `(`).
        let shell = cash_core::Shell::builder().build().await.unwrap();
        for prefix in ["£", "€", "é", "½", "§", "²", "ï", "¤", "…"] {
            for line in [prefix.to_string(), format!("{prefix}(")] {
                let highlighted = highlight_command(&shell, &line, line.len());
                assert_spans_are_valid(&highlighted);
            }
        }
    }
}
