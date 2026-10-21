use std::borrow::Cow;
use std::sync::Arc;
use utf8_chars::BufReadCharsExt;

use crate::{SourcePosition, SourceSpan};

#[derive(Clone, Debug)]
pub(crate) enum TokenEndReason {
    /// End of input was reached.
    EndOfInput,
    /// An unescaped newline char was reached.
    UnescapedNewLine,
    /// Specified terminating char.
    SpecifiedTerminatingChar,
    /// A non-newline blank char was reached; it is carried so that the inside of `${ }`,
    /// reconstructed from tokens, keeps a tab a tab.
    NonNewLineBlank(char),
    /// A here-document's body is starting.
    HereDocumentBodyStart,
    /// A here-document's body was terminated.
    HereDocumentBodyEnd,
    /// A here-document's end tag was reached.
    HereDocumentEndTag,
    /// An operator was started.
    OperatorStart,
    /// An operator was terminated.
    OperatorEnd,
    /// Some other condition was reached.
    Other,
}

/// Compatibility alias for `SourceSpan`.
pub type TokenLocation = SourceSpan;

/// Represents a token extracted from a shell script.
#[derive(Clone, Debug)]
#[cfg_attr(
    any(test, feature = "serde"),
    derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)
)]
pub enum Token {
    /// An operator token.
    Operator(String, SourceSpan),
    /// A word token.
    Word(String, SourceSpan),
}

impl Token {
    /// Returns the string value of the token.
    pub fn to_str(&self) -> &str {
        match self {
            Self::Operator(s, _) => s,
            Self::Word(s, _) => s,
        }
    }

    /// Returns the location of the token in the source script.
    pub const fn location(&self) -> &SourceSpan {
        match self {
            Self::Operator(_, l) => l,
            Self::Word(_, l) => l,
        }
    }
}

#[cfg(feature = "diagnostics")]
impl From<&Token> for miette::SourceSpan {
    fn from(token: &Token) -> Self {
        let start = token.location().start.as_ref();
        Self::new(start.into(), token.location().length())
    }
}

/// Encapsulates the result of tokenizing a shell script.
#[derive(Clone, Debug)]
pub(crate) struct TokenizeResult {
    /// Reason for tokenization ending.
    pub reason: TokenEndReason,
    /// The token that was extracted, if any.
    pub token: Option<Token>,
}

/// Represents an error that occurred during tokenization.
#[derive(thiserror::Error, Debug)]
pub enum TokenizerError {
    /// An unterminated escape sequence was encountered at the end of the input stream.
    #[error("unterminated escape sequence")]
    UnterminatedEscapeSequence,

    /// An unterminated single-quoted substring was encountered at the end of the input stream.
    #[error("unterminated single quote at {0}")]
    UnterminatedSingleQuote(SourcePosition),

    /// An unterminated ANSI C-quoted substring was encountered at the end of the input stream.
    #[error("unterminated ANSI C quote at {0}")]
    UnterminatedAnsiCQuote(SourcePosition),

    /// An unterminated double-quoted substring was encountered at the end of the input stream.
    #[error("unterminated double quote at {0}")]
    UnterminatedDoubleQuote(SourcePosition),

    /// An unterminated back-quoted substring was encountered at the end of the input stream.
    #[error("unterminated backquote near {0}")]
    UnterminatedBackquote(SourcePosition),

    /// An unterminated extended glob (extglob) pattern was encountered at the end of the input
    /// stream.
    #[error("unterminated extglob near {0}")]
    UnterminatedExtendedGlob(SourcePosition),

    /// An unterminated variable expression was encountered at the end of the input stream.
    #[error("unterminated variable expression")]
    UnterminatedVariable,

    /// An unterminated command substitiion was encountered at the end of the input stream.
    #[error("unterminated command substitution")]
    UnterminatedCommandSubstitution,

    /// An unterminated arithmetic or other expansion was encountered at the end of the input
    /// stream.
    #[error("unterminated expansion")]
    UnterminatedExpansion,

    /// An error occurred decoding UTF-8 characters in the input stream.
    #[error("failed to decode UTF-8 characters")]
    FailedDecoding,

    /// An I/O here tag was missing.
    #[error("missing here tag for here document body")]
    MissingHereTagForDocumentBody,

    /// The indicated I/O here tag was missing.
    #[error("missing here tag '{0}'")]
    MissingHereTag(String),

    /// An unterminated here document sequence was encountered at the end of the input stream.
    #[error("unterminated here document sequence; tag(s) [{0}] found at: [{1}]")]
    UnterminatedHereDocuments(String, String),

    /// An I/O error occurred while reading from the input stream.
    #[error("failed to read input")]
    ReadError(#[from] std::io::Error),
}

impl TokenizerError {
    /// This error from text that began `lines` lines into the whole, with the position it
    /// names counted from the start of the whole.
    #[must_use]
    pub fn later_by(self, lines: usize) -> Self {
        let shift = |mut position: SourcePosition| {
            position.line += lines;
            position
        };
        match self {
            Self::UnterminatedSingleQuote(position) => {
                Self::UnterminatedSingleQuote(shift(position))
            }
            Self::UnterminatedAnsiCQuote(position) => Self::UnterminatedAnsiCQuote(shift(position)),
            Self::UnterminatedDoubleQuote(position) => {
                Self::UnterminatedDoubleQuote(shift(position))
            }
            Self::UnterminatedBackquote(position) => Self::UnterminatedBackquote(shift(position)),
            Self::UnterminatedExtendedGlob(position) => {
                Self::UnterminatedExtendedGlob(shift(position))
            }
            other => other,
        }
    }

    /// What Bash says of input that ended inside this construct, with the line in the
    /// input it names, if it is one Bash reads to its end: ``unexpected EOF while looking
    /// for matching `"'`` on the line the quote opened on, `` `)' `` for a command
    /// substitution on the line after the last.
    pub const fn bash_eof(&self) -> Option<(char, Option<usize>)> {
        match self {
            Self::UnterminatedSingleQuote(position) | Self::UnterminatedAnsiCQuote(position) => {
                Some(('\'', Some(position.line)))
            }
            Self::UnterminatedDoubleQuote(position) => Some(('"', Some(position.line))),
            Self::UnterminatedBackquote(position) => Some(('`', Some(position.line))),
            Self::UnterminatedExtendedGlob(position) => Some((')', Some(position.line))),
            Self::UnterminatedCommandSubstitution | Self::UnterminatedExpansion => {
                Some((')', None))
            }
            Self::UnterminatedVariable => Some(('}', None)),
            _ => None,
        }
    }

    /// Returns true if the error represents an error that could possibly be due
    /// to an incomplete input stream.
    pub const fn is_incomplete(&self) -> bool {
        matches!(
            self,
            Self::UnterminatedEscapeSequence
                | Self::UnterminatedAnsiCQuote(..)
                | Self::UnterminatedSingleQuote(..)
                | Self::UnterminatedDoubleQuote(..)
                | Self::UnterminatedBackquote(..)
                | Self::UnterminatedCommandSubstitution
                | Self::UnterminatedExpansion
                | Self::UnterminatedVariable
                | Self::UnterminatedExtendedGlob(..)
                | Self::UnterminatedHereDocuments(..)
        )
    }
}

/// Encapsulates a sequence of tokens.
#[derive(Debug)]
pub(crate) struct Tokens<'a> {
    /// Sequence of tokens.
    pub tokens: &'a [Token],
}

#[derive(Clone, Debug)]
enum QuoteMode {
    None,
    AnsiC(SourcePosition),
    Single(SourcePosition),
    Double(SourcePosition),
}

#[derive(Clone, Debug, Default)]
enum HereState {
    /// In this state, we are not currently tracking any here-documents.
    #[default]
    None,
    /// In this state, we expect that the next token will be a here tag.
    NextTokenIsHereTag { remove_tabs: bool },
    /// In this state, the *current* token is a here tag.
    CurrentTokenIsHereTag {
        remove_tabs: bool,
        operator_token_result: TokenizeResult,
    },
    /// In this state, we expect that the *next line* will be the body of
    /// a here-document.
    NextLineIsHereDoc,
    /// In this state, we are in the set of lines that comprise 1 or more
    /// consecutive here-document bodies.
    InHereDocs,
}

#[derive(Clone, Debug)]
struct HereTag {
    tag: String,
    tag_was_escaped_or_quoted: bool,
    remove_tabs: bool,
    position: SourcePosition,
    tokens: Vec<TokenizeResult>,
    pending_tokens_after: Vec<TokenizeResult>,
}

#[derive(Clone, Debug)]
struct CrossTokenParseState {
    /// Cursor within the overall token stream; used for error reporting.
    cursor: SourcePosition,
    /// The here-documents a `)` closed (`$(cat <<EOF` ... `EOF)`): each tag, the line it
    /// is on and the line of the `)`, for Bash's warning.
    closed_by_parenthesis: Vec<(String, usize, usize)>,
    /// Current state of parsing here-documents.
    here_state: HereState,
    /// Ordered queue of here tags for which we're still looking for matching here-document bodies.
    current_here_tags: Vec<HereTag>,
    /// Tokens already tokenized that should be used first to serve requests for tokens.
    queued_tokens: Vec<TokenizeResult>,
    /// Are we in an arithmetic expansion?
    arithmetic_expansion: bool,
    /// Whether the words since a `[[` have not yet reached its `]]`: there an extglob
    /// pattern is read whether extglob is on or not, as in Bash.
    in_double_brackets: bool,
    /// How many `${...}` expansions are being read, inside one another: their words are
    /// not held for after a here-document's body, even one begun inside them.
    braced_expansions: u32,
    /// Whether a word read now may be an assignment, as Bash's `assignment_acceptable` has
    /// it: at a command's start, after a keyword that begins one, and after another
    /// assignment. There `name[` reads its subscript to its `]`, blanks and all:
    /// `map[set auto]=on` is one word, which was two (bashdb).
    assignment_position: bool,
    /// Whether the next word is a redirection's target, which leaves the position as it is.
    redirection_target: bool,
    /// Whether the last word was an assignment with nothing after its `=`, which a `(`
    /// makes a compound one.
    bare_assignment: bool,
    /// How many compound assignments' `(`s are open, `a=(...)`: there a word that begins
    /// with `[` reads its subscript to its `]`, `([k one]=1)`.
    compound_assignments: u32,
}

/// Options controlling how the tokenizer operates.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct TokenizerOptions {
    /// Whether or not to enable extended globbing patterns (extglob).
    pub enable_extended_globbing: bool,
    /// Whether or not to operate in POSIX compliance mode.
    pub posix_mode: bool,
    /// Whether or not we're running in SH emulation mode.
    pub sh_mode: bool,
}

impl Default for TokenizerOptions {
    fn default() -> Self {
        Self {
            enable_extended_globbing: true,
            posix_mode: false,
            sh_mode: false,
        }
    }
}

/// A tokenizer for shell scripts.
pub(crate) struct Tokenizer<'a, R: ?Sized + std::io::BufRead> {
    char_reader: itertools::PeekNth<utf8_chars::Chars<'a, R>>,
    cross_state: CrossTokenParseState,
    options: TokenizerOptions,
    /// The texts of aliases being read, read before the input, the last pushed first
    /// ([`Tokenizer::push_alias`]).
    pending: Vec<PendingAlias>,
    /// Whether an alias's text that ended in a blank was read to its end since
    /// [`Tokenizer::take_blank_end`] was last asked.
    blank_end: bool,
}

/// An alias's text read back into the input, as Bash reads it: a quote it opens is
/// closed by what follows it on the line.
struct PendingAlias {
    name: String,
    chars: std::collections::VecDeque<char>,
    ends_in_blank: bool,
}

/// Encapsulates the current token parsing state.
#[derive(Clone, Debug)]
struct TokenParseState {
    pub start_position: SourcePosition,
    pub token_so_far: String,
    pub token_is_operator: bool,
    pub in_escape: bool,
    pub quote_mode: QuoteMode,
}

impl TokenParseState {
    pub fn new(start_position: &SourcePosition) -> Self {
        Self {
            start_position: start_position.to_owned(),
            token_so_far: String::new(),
            token_is_operator: false,
            in_escape: false,
            quote_mode: QuoteMode::None,
        }
    }

    pub fn pop(&mut self, end_position: &SourcePosition) -> Token {
        let end = Arc::new(end_position.to_owned());
        let token_location = SourceSpan {
            start: Arc::new(std::mem::take(&mut self.start_position)),
            end,
        };

        let token = if std::mem::take(&mut self.token_is_operator) {
            Token::Operator(std::mem::take(&mut self.token_so_far), token_location)
        } else {
            Token::Word(std::mem::take(&mut self.token_so_far), token_location)
        };

        end_position.clone_into(&mut self.start_position);
        self.in_escape = false;
        self.quote_mode = QuoteMode::None;

        token
    }

    pub const fn started_token(&self) -> bool {
        !self.token_so_far.is_empty()
    }

    /// Returns true if the token so far consists only of blanks.
    ///
    /// This can only happen when tokenizing with `include_space`, where blanks are accumulated
    /// into the token so that the original text of a nested construct can be reproduced. Such
    /// blanks are not a word, so a `#` following them still begins a comment.
    pub fn only_blanks_so_far(&self) -> bool {
        !self.token_so_far.is_empty() && self.token_so_far.chars().all(is_blank)
    }

    pub fn append_char(&mut self, c: char) {
        self.token_so_far.push(c);
    }

    pub fn append_str(&mut self, s: &str) {
        self.token_so_far.push_str(s);
    }

    pub const fn unquoted(&self) -> bool {
        !self.in_escape && matches!(self.quote_mode, QuoteMode::None)
    }

    pub fn current_token(&self) -> &str {
        &self.token_so_far
    }

    pub fn is_specific_operator(&self, operator: &str) -> bool {
        self.token_is_operator && self.current_token() == operator
    }

    pub const fn in_operator(&self) -> bool {
        self.token_is_operator
    }

    fn is_newline(&self) -> bool {
        self.token_so_far == "\n"
    }

    fn replace_with_here_doc(&mut self, s: String) {
        self.token_so_far = s;
    }

    #[expect(clippy::too_many_lines)]
    pub fn delimit_current_token(
        &mut self,
        reason: TokenEndReason,
        cross_token_state: &mut CrossTokenParseState,
    ) -> Result<Option<TokenizeResult>, TokenizerError> {
        // If we don't have anything in the token, then don't yield an empty string token
        // *unless* it's the body of a here document.
        if !self.started_token() && !matches!(reason, TokenEndReason::HereDocumentBodyEnd) {
            return Ok(Some(TokenizeResult {
                reason,
                token: None,
            }));
        }

        // TODO(tokenizer): Make sure the here-tag meets criteria (and isn't a newline).
        let current_here_state = std::mem::take(&mut cross_token_state.here_state);
        match current_here_state {
            HereState::NextTokenIsHereTag { remove_tabs } => {
                // Don't yield the operator as a token yet. We need to make sure we collect
                // up everything we need for all the here-documents with tags on this line.
                let operator_token_result = TokenizeResult {
                    reason,
                    token: Some(self.pop(&cross_token_state.cursor)),
                };

                cross_token_state.here_state = HereState::CurrentTokenIsHereTag {
                    remove_tabs,
                    operator_token_result,
                };

                return Ok(None);
            }
            HereState::CurrentTokenIsHereTag {
                remove_tabs,
                operator_token_result,
            } => {
                if self.is_newline() {
                    return Err(TokenizerError::MissingHereTag(
                        self.current_token().to_owned(),
                    ));
                }

                cross_token_state.here_state = HereState::NextLineIsHereDoc;

                // Include the trailing \n in the here tag so it's easier to check against.
                let tag = std::format!("{}\n", self.current_token().trim_ascii_start());
                let tag_was_escaped_or_quoted = tag.contains(is_quoting_char);

                let tag_token_result = TokenizeResult {
                    reason,
                    token: Some(self.pop(&cross_token_state.cursor)),
                };

                cross_token_state.current_here_tags.push(HereTag {
                    tag,
                    tag_was_escaped_or_quoted,
                    remove_tabs,
                    position: cross_token_state.cursor.clone(),
                    tokens: vec![operator_token_result, tag_token_result],
                    pending_tokens_after: vec![],
                });

                return Ok(None);
            }
            HereState::NextLineIsHereDoc => {
                if self.is_newline() {
                    cross_token_state.here_state = HereState::InHereDocs;
                } else {
                    cross_token_state.here_state = HereState::NextLineIsHereDoc;
                }

                if let Some(last_here_tag) = cross_token_state.current_here_tags.last_mut() {
                    let token = self.pop(&cross_token_state.cursor);
                    let result = TokenizeResult {
                        reason,
                        token: Some(token),
                    };

                    last_here_tag.pending_tokens_after.push(result);
                } else {
                    return Err(TokenizerError::MissingHereTagForDocumentBody);
                }

                return Ok(None);
            }
            HereState::InHereDocs => {
                // We hit the end of the current here-document.
                let completed_here_tag = cross_token_state.current_here_tags.remove(0);

                // First queue the redirection operator and (start) here-tag.
                cross_token_state
                    .queued_tokens
                    .extend(completed_here_tag.tokens);

                // Leave a hint that we are about to start a here-document.
                cross_token_state.queued_tokens.push(TokenizeResult {
                    reason: TokenEndReason::HereDocumentBodyStart,
                    token: None,
                });

                // Then queue the body document we just finished.
                cross_token_state.queued_tokens.push(TokenizeResult {
                    reason,
                    token: Some(self.pop(&cross_token_state.cursor)),
                });

                // Then queue up the (end) here-tag.
                let end_tag = if completed_here_tag.tag_was_escaped_or_quoted {
                    unquote_str(&completed_here_tag.tag)
                } else {
                    completed_here_tag.tag
                };
                self.append_str(end_tag.trim_end_matches('\n'));
                cross_token_state.queued_tokens.push(TokenizeResult {
                    reason: TokenEndReason::HereDocumentEndTag,
                    token: Some(self.pop(&cross_token_state.cursor)),
                });

                // Now we're ready to queue up any tokens that came between the completed
                // here tag and the next here tag (or newline after it if it was the last).
                cross_token_state
                    .queued_tokens
                    .extend(completed_here_tag.pending_tokens_after);

                if cross_token_state.current_here_tags.is_empty() {
                    cross_token_state.here_state = HereState::None;
                } else {
                    cross_token_state.here_state = HereState::InHereDocs;
                }

                return Ok(None);
            }
            HereState::None => (),
        }

        let token = self.pop(&cross_token_state.cursor);
        let result = TokenizeResult {
            reason,
            token: Some(token),
        };

        Ok(Some(result))
    }
}

/// Break the given input shell script string into tokens, returning the tokens.
///
/// # Arguments
///
/// * `input` - The shell script to tokenize.
pub fn tokenize_str(input: &str) -> Result<Vec<Token>, TokenizerError> {
    tokenize_str_with_options(input, &TokenizerOptions::default())
}

/// Break the given input shell script string into tokens, returning the tokens.
///
/// # Arguments
///
/// * `input` - The shell script to tokenize.
/// * `options` - Options controlling how the tokenizer operates.
pub fn tokenize_str_with_options(
    input: &str,
    options: &TokenizerOptions,
) -> Result<Vec<Token>, TokenizerError> {
    uncached_tokenize_string(input, options)
}

/// The tokens of `input`, from this thread's memo when it was tokenized before
/// ([`crate::memo`]).
fn uncached_tokenize_string(
    input: &str,
    options: &TokenizerOptions,
) -> Result<Vec<Token>, TokenizerError> {
    type Tokenized = Vec<Token>;
    thread_local! {
        static TOKENIZED: std::cell::RefCell<crate::memo::Memo<TokenizerOptions, Tokenized>> =
            std::cell::RefCell::default();
    }
    crate::memo::remembered(&TOKENIZED, input, options, || {
        uncached_tokenize_str(input, options)
    })
}

/// The here-documents of `input` that a `)` closes, as in `$(cat <<EOF` ... `EOF)`.
///
/// For Bash's warning that each was "delimited by end-of-file": the tag, the line the
/// here-document begins on and the line of the `)`. What tokenizes before an error counts.
///
/// # Arguments
///
/// * `input` - The shell script to look in.
/// * `options` - Options controlling how the tokenizer operates.
pub fn here_documents_closed_by_parenthesis(
    input: &str,
    options: &TokenizerOptions,
) -> Vec<(String, usize, usize)> {
    let mut reader = std::io::BufReader::new(input.as_bytes());
    let mut tokenizer = crate::tokenizer::Tokenizer::new(&mut reader, options);
    while let Ok(result) = tokenizer.next_token() {
        if matches!(result.reason, TokenEndReason::EndOfInput) {
            break;
        }
    }
    std::mem::take(&mut tokenizer.cross_state.closed_by_parenthesis)
}

/// Break the given input shell script string into tokens, returning the tokens.
/// No caching is performed.
///
/// # Arguments
///
/// * `input` - The shell script to tokenize.
pub fn uncached_tokenize_str(
    input: &str,
    options: &TokenizerOptions,
) -> Result<Vec<Token>, TokenizerError> {
    let mut reader = std::io::BufReader::new(input.as_bytes());
    let mut tokenizer = crate::tokenizer::Tokenizer::new(&mut reader, options);

    let mut tokens = vec![];
    loop {
        match tokenizer.next_token()? {
            TokenizeResult {
                token: Some(token), ..
            } => tokens.push(token),
            TokenizeResult {
                reason: TokenEndReason::EndOfInput,
                ..
            } => break,
            _ => (),
        }
    }

    Ok(tokens)
}

/// cash (D2): where a `case` inside `$(...)` is, so that a pattern's unbalanced `)` does
/// not end the construct.
///
/// `$(case a in a*) echo m;; esac)` is valid bash, but a plain paren count sees the `)` of
/// `a*)` — which has no opener — and stops there, truncating the substitution to `case a
/// in a*`. The rest then parses as stray tokens and the error surfaces at the `;;`, far
/// from the real cause.
///
/// A case pattern's `)` appears in exactly two places: after `in`, and after a clause
/// terminator (`;;`, `;&`, `;;&`). Tracking that is enough to tell a pattern's paren from a
/// real one, and `(a*)` still balances normally because its opener is counted.
#[derive(Default)]
struct CaseScan {
    /// How many cases deep the scan is.
    depth: u32,
    /// Whether the next words are a pattern list.
    expecting_pattern: bool,
    /// Whether the pattern list began with an `(`, which balances its `)`.
    pattern_had_open_paren: bool,
    /// Words until a case's `in`: 2 right after `case`, 1 after its subject. Only that `in`
    /// starts the patterns, not a `for`'s inside the case.
    awaiting_in: u8,
    /// Whether a word of the pattern list was read: in it every word is a pattern but an
    /// `esac` that begins it, which ends the case. `case x in in|esac) ...` was taken to
    /// end at the `esac` of its pattern.
    pattern_started: bool,
}

impl CaseScan {
    /// Follows `token`. Its text carries any leading blank that preceded it, so " esac"
    /// arrives rather than "esac": compared trimmed.
    fn token(&mut self, token: &Token) {
        match token {
            Token::Word(word, _) => match word.trim() {
                // A run of blanks can come as a word of its own: it is none.
                "" => (),
                _ if self.awaiting_in == 2 => self.awaiting_in = 1,
                "in" if self.awaiting_in == 1 => {
                    self.awaiting_in = 0;
                    self.expecting_pattern = true;
                    self.pattern_started = false;
                }
                "esac" if !self.expecting_pattern || !self.pattern_started => {
                    self.depth = self.depth.saturating_sub(1);
                    self.expecting_pattern = false;
                }
                _ if self.expecting_pattern => self.pattern_started = true,
                "case" => {
                    self.depth += 1;
                    self.expecting_pattern = false;
                    self.awaiting_in = 2;
                }
                _ => (),
            },
            Token::Operator(op, _) => {
                if matches!(op.trim(), ";;" | ";&" | ";;&") {
                    self.expecting_pattern = self.depth > 0;
                    self.pattern_started = false;
                }
            }
        }
    }
}

impl<'a, R: ?Sized + std::io::BufRead> Tokenizer<'a, R> {
    pub fn new(reader: &'a mut R, options: &TokenizerOptions) -> Self {
        Tokenizer {
            options: options.clone(),
            char_reader: itertools::peek_nth(reader.chars()),
            cross_state: CrossTokenParseState {
                cursor: SourcePosition {
                    index: 0,
                    line: 1,
                    column: 1,
                },
                closed_by_parenthesis: vec![],
                here_state: HereState::None,
                current_here_tags: vec![],
                queued_tokens: vec![],
                arithmetic_expansion: false,
                in_double_brackets: false,
                braced_expansions: 0,
                assignment_position: true,
                redirection_target: false,
                bare_assignment: false,
                compound_assignments: 0,
            },
            pending: vec![],
            blank_end: false,
        }
    }

    /// Reads `text`, the alias `name` stands for, before the rest of the input, as Bash
    /// reads an alias: its characters join what comes after, so an alias that opens a
    /// quote has it closed by the line (`alias q="echo it's"; q quoted'`).
    ///
    /// `blank_after` is the blank that ended the alias's word, which the word took with it
    /// and which comes after the text again. A word that came out of the tokens queued
    /// behind a here-document has its text read alone and queued in its place, as aliases
    /// were expanded on the tokens; the alias is still taken for one being read.
    pub fn push_alias(&mut self, name: &str, text: &str, blank_after: Option<char>) {
        let ends_in_blank = text.ends_with([' ', '\t']);
        let mut chars: std::collections::VecDeque<char> = text.chars().collect();
        if !self.cross_state.queued_tokens.is_empty() {
            let tokens = tokenize_str_with_options(text, &self.options).unwrap_or_default();
            let queued = tokens.into_iter().map(|token| TokenizeResult {
                reason: TokenEndReason::Other,
                token: Some(token),
            });
            self.cross_state.queued_tokens.splice(0..0, queued);
            chars.clear();
        } else if let Some(blank) = blank_after {
            chars.push_back(blank);
        }
        self.pending.push(PendingAlias {
            name: name.to_owned(),
            chars,
            ends_in_blank,
        });
    }

    /// The aliases whose text is still being read, which are not looked up again.
    pub fn aliases_being_read(&self) -> impl Iterator<Item = &str> {
        self.pending.iter().map(|pending| pending.name.as_str())
    }

    /// Whether an alias's text that ended in a blank was read to its end since this was
    /// last asked: the word after it is looked up as an alias too.
    pub const fn take_blank_end(&mut self) -> bool {
        std::mem::replace(&mut self.blank_end, false)
    }

    /// The `n`th character still to read, an alias's or the input's, without reading it.
    fn peek_nth_char(&mut self, mut n: usize) -> Result<Option<char>, TokenizerError> {
        for pending in self.pending.iter().rev() {
            if let Some(c) = pending.chars.get(n) {
                return Ok(Some(*c));
            }
            n -= pending.chars.len();
        }
        match self.char_reader.peek_nth(n) {
            Some(Ok(c)) => Ok(Some(*c)),
            Some(Err(_)) => Err(TokenizerError::FailedDecoding),
            None => Ok(None),
        }
    }

    #[expect(clippy::unnecessary_wraps)]
    pub fn current_location(&self) -> Option<SourcePosition> {
        Some(self.cross_state.cursor.clone())
    }

    fn next_char(&mut self) -> Result<Option<char>, TokenizerError> {
        // An alias's text is read first; it has no place of its own in the input, so the
        // cursor stays where the alias's word ended.
        while let Some(pending) = self.pending.last_mut() {
            if let Some(c) = pending.chars.pop_front() {
                return Ok(Some(c));
            }
            if let Some(done) = self.pending.pop() {
                self.blank_end |= done.ends_in_blank;
            }
        }
        let c = self
            .char_reader
            .next()
            .transpose()
            .map_err(TokenizerError::ReadError)?;

        if let Some(ch) = c {
            if ch == '\n' {
                self.cross_state.cursor.line += 1;
                self.cross_state.cursor.column = 1;
            } else {
                self.cross_state.cursor.column += 1;
            }
            self.cross_state.cursor.index += 1;
        }

        Ok(c)
    }

    /// Consumes a character the caller has already peeked, or was stopped in front of.
    fn next_peeked_char(&mut self) -> Result<char, TokenizerError> {
        self.next_char()?
            .ok_or(TokenizerError::UnterminatedExpansion)
    }

    fn consume_char(&mut self) -> Result<(), TokenizerError> {
        let _ = self.next_char()?;
        Ok(())
    }

    fn peek_char(&mut self) -> Result<Option<char>, TokenizerError> {
        self.peek_nth_char(0)
    }

    /// The character after the next one, without consuming either.
    fn peek_second_char(&mut self) -> Result<Option<char>, TokenizerError> {
        self.peek_nth_char(1)
    }

    pub fn next_token(&mut self) -> Result<TokenizeResult, TokenizerError> {
        let result = self.next_token_until(None, false /* include space? */)?;
        match &result.token {
            Some(Token::Word(word, _)) if word == "[[" => {
                self.cross_state.in_double_brackets = true;
            }
            Some(Token::Word(word, _)) if word == "]]" => {
                self.cross_state.in_double_brackets = false;
            }
            _ => {}
        }
        self.note_assignment_position(result.token.as_ref());
        Ok(result)
    }

    /// Follows where an assignment may be read ([`CrossTokenParseState::assignment_position`])
    /// past `token`, as Bash's `assignment_acceptable` does.
    fn note_assignment_position(&mut self, token: Option<&Token>) {
        let state = &mut self.cross_state;
        match token {
            Some(Token::Operator(op, _)) => {
                let bare_assignment = std::mem::take(&mut state.bare_assignment);
                match op.as_str() {
                    "(" if bare_assignment => state.compound_assignments += 1,
                    ")" if state.compound_assignments > 0 => {
                        state.compound_assignments -= 1;
                        state.assignment_position = true;
                    }
                    ";" | "&" | "&&" | "||" | "|" | "|&" | ";;" | ";&" | ";;&" | "\n" | "("
                    | ")" => {
                        state.assignment_position = true;
                        state.redirection_target = false;
                    }
                    // The word after a redirection is its target.
                    _ => state.redirection_target = true,
                }
            }
            Some(Token::Word(word, _)) => {
                state.bare_assignment = false;
                if std::mem::take(&mut state.redirection_target) || state.compound_assignments > 0 {
                    return;
                }
                // `name=(` is a compound assignment wherever it comes: `declare -A m=(` too.
                let assignment = is_assignment_word(word);
                state.bare_assignment = assignment && word.ends_with('=');
                // A `{` opens commands wherever it comes, `function f {` too.
                if word == "{" {
                    state.assignment_position = true;
                } else if state.assignment_position {
                    state.assignment_position = assignment
                        || matches!(
                            word.as_str(),
                            "then"
                                | "do"
                                | "else"
                                | "elif"
                                | "if"
                                | "while"
                                | "until"
                                | "!"
                                | "time"
                                | "coproc"
                        );
                }
            }
            None => {}
        }
    }

    /// Whether a `[` that comes now begins a subscript, read to its `]` with its blanks:
    /// after a name where an assignment may be (`map[set auto]=on`), or at the start of a
    /// word in a compound assignment (`([k one]=1)`), as Bash reads them.
    fn subscript_follows(&self, word_so_far: &str) -> bool {
        let state = &self.cross_state;
        if state.compound_assignments > 0 {
            return word_so_far.is_empty();
        }
        state.assignment_position && !state.redirection_target && is_name(word_so_far)
    }

    /// Consumes a nested construct (e.g., `$((...))` or `$[...]`), handling nested delimiters
    /// and here-documents.
    ///
    /// # Arguments
    ///
    /// * `state` - The current token parse state to append characters to.
    /// * `terminating_char` - The character that terminates the construct (e.g., `)` or `]`).
    /// * `nesting_open` - The character that increases nesting depth when encountered (e.g., `(` or
    ///   `[`).
    /// * `initial_nesting` - The initial nesting count (e.g., 2 for `$((`, 1 for `$[`).
    fn consume_nested_construct(
        &mut self,
        state: &mut TokenParseState,
        terminating_char: char,
        nesting_open: &str,
        nesting_count: u32,
    ) -> Result<(), TokenizerError> {
        // A here-document inside is the construct's own, its body read at the first
        // newline inside, as Bash reads the construct as a program of its own: with one
        // pending on the line, `cat <<A; echo "$(cat <<B ...)"` ran out of input.
        let outer = self.set_aside_here_documents();
        let result = self.consume_nested_text(state, terminating_char, nesting_open, nesting_count);
        self.take_back_here_documents(outer);
        result
    }

    /// Puts aside the here-documents pending on the line, for a construct inside it whose
    /// words are its own (`$(...)`, `${...}`): with one pending, each word inside was held
    /// for after its body.
    fn set_aside_here_documents(&mut self) -> (HereState, Vec<HereTag>) {
        (
            std::mem::take(&mut self.cross_state.here_state),
            std::mem::take(&mut self.cross_state.current_here_tags),
        )
    }

    /// Takes back what [`Self::set_aside_here_documents`] put aside. A here-document the
    /// construct left open (`$(cat <<EOF)`, its body on the lines after) waits after those,
    /// for the end of the line, which inside a `${...}` has not come yet.
    fn take_back_here_documents(&mut self, (state, mut tags): (HereState, Vec<HereTag>)) {
        let inner = std::mem::take(&mut self.cross_state.current_here_tags);
        self.cross_state.here_state = if inner.is_empty() || self.cross_state.braced_expansions > 0
        {
            state
        } else {
            HereState::NextLineIsHereDoc
        };
        tags.extend(inner);
        self.cross_state.current_here_tags = tags;
    }

    /// The loop of [`Self::consume_nested_construct`].
    fn consume_nested_text(
        &mut self,
        state: &mut TokenParseState,
        terminating_char: char,
        nesting_open: &str,
        mut nesting_count: u32,
    ) -> Result<(), TokenizerError> {
        let mut pending_here_doc_tokens = vec![];
        let mut drain_here_doc_tokens = false;
        let mut case = CaseScan::default();

        // The nesting outside a `((` command that began inside the construct. The `)`s that
        // close it end here as the terminating char, not as the `))` operator that ends its
        // arithmetic, so the arithmetic ends when the nesting is back to this: it went on,
        // and the `<<` of a later here-document was read as a shift (PI-02).
        let mut arithmetic_command_nesting: Option<u32> = None;

        loop {
            let cur_token = if drain_here_doc_tokens && !pending_here_doc_tokens.is_empty() {
                if pending_here_doc_tokens.len() == 1 {
                    drain_here_doc_tokens = false;
                }
                pending_here_doc_tokens.remove(0)
            } else {
                let was_arithmetic = self.cross_state.arithmetic_expansion;
                let cur_token = self.next_token_until(Some(terminating_char), true)?;
                if !was_arithmetic
                    && self.cross_state.arithmetic_expansion
                    && arithmetic_command_nesting.is_none()
                {
                    arithmetic_command_nesting = Some(nesting_count);
                }

                if matches!(
                    cur_token.reason,
                    TokenEndReason::HereDocumentBodyStart
                        | TokenEndReason::HereDocumentBodyEnd
                        | TokenEndReason::HereDocumentEndTag
                ) {
                    pending_here_doc_tokens.push(cur_token);
                    continue;
                }
                cur_token
            };

            if matches!(cur_token.reason, TokenEndReason::UnescapedNewLine)
                && !pending_here_doc_tokens.is_empty()
            {
                pending_here_doc_tokens.push(cur_token);
                drain_here_doc_tokens = true;
                continue;
            }

            if let Some(cur_token_value) = cur_token.token {
                state.append_str(cur_token_value.to_str());
                case.token(&cur_token_value);

                if matches!(&cur_token_value, Token::Operator(o, _) if o == nesting_open) {
                    nesting_count += 1;
                    // A parenthesised pattern — `(a*)` — brings its own opener, so its
                    // closer must balance normally rather than being skipped.
                    if case.expecting_pattern {
                        case.pattern_had_open_paren = true;
                    }
                }
            }

            match cur_token.reason {
                TokenEndReason::HereDocumentBodyStart => {
                    state.append_char('\n');
                }
                // The text is re-parsed as a program, where any blank will do; bash's
                // `declare -f` also prints a tab between these tokens as a space.
                TokenEndReason::NonNewLineBlank(_) => state.append_char(' '),
                TokenEndReason::SpecifiedTerminatingChar => {
                    // A case pattern's closing paren has no opener, so it must not be
                    // counted — otherwise the construct appears to end here.
                    if case.depth > 0 && case.expecting_pattern && !case.pattern_had_open_paren {
                        case.expecting_pattern = false;
                        state.append_char(self.next_peeked_char()?);
                        continue;
                    }

                    if case.pattern_had_open_paren {
                        case.pattern_had_open_paren = false;
                        case.expecting_pattern = false;
                    }

                    nesting_count -= 1;
                    if arithmetic_command_nesting == Some(nesting_count) {
                        arithmetic_command_nesting = None;
                        self.cross_state.arithmetic_expansion = false;
                    }
                    if nesting_count == 0 {
                        break;
                    }
                    state.append_char(self.next_peeked_char()?);
                }
                TokenEndReason::EndOfInput => {
                    return Err(TokenizerError::UnterminatedExpansion);
                }
                _ => (),
            }
        }

        state.append_char(self.next_peeked_char()?);
        Ok(())
    }

    /// Returns the next token from the input stream, optionally stopping early when a specified
    /// terminating character is encountered.
    ///
    /// # Arguments
    ///
    /// * `terminating_char` - An optional character that, if encountered, will stop the
    ///   tokenization process and return the token up to that character.
    /// * `include_space` - If true, include spaces in the tokenization process. This is not
    ///   typically the case, but can be helpful when needing to preserve the original source text
    ///   embedded within a command substitution or similar construct.
    #[expect(clippy::cognitive_complexity)]
    #[expect(clippy::if_same_then_else)]
    #[expect(clippy::panic_in_result_fn)]
    #[expect(clippy::too_many_lines)]
    #[allow(clippy::unwrap_in_result)]
    fn next_token_until(
        &mut self,
        terminating_char: Option<char>,
        include_space: bool,
    ) -> Result<TokenizeResult, TokenizerError> {
        let mut state = TokenParseState::new(&self.cross_state.cursor);
        let mut result: Option<TokenizeResult> = None;

        while result.is_none() {
            // First satisfy token results from our queue. Once we exhaust the queue then
            // we'll look at the input stream.
            if !self.cross_state.queued_tokens.is_empty() {
                return Ok(self.cross_state.queued_tokens.remove(0));
            }

            let next = self.peek_char()?;
            let c = next.unwrap_or('\0');

            // When we hit the end of the input, then we're done with the current token (if there is
            // one).
            if next.is_none() {
                // TODO(tokenizer): Verify we're not waiting on some terminating character?
                // Verify we're out of all quotes.
                if state.in_escape {
                    return Err(TokenizerError::UnterminatedEscapeSequence);
                }
                match state.quote_mode {
                    QuoteMode::None => (),
                    QuoteMode::AnsiC(pos) => {
                        return Err(TokenizerError::UnterminatedAnsiCQuote(pos));
                    }
                    QuoteMode::Single(pos) => {
                        return Err(TokenizerError::UnterminatedSingleQuote(pos));
                    }
                    QuoteMode::Double(pos) => {
                        return Err(TokenizerError::UnterminatedDoubleQuote(pos));
                    }
                }

                // Verify we're not in a here document.
                if !matches!(self.cross_state.here_state, HereState::None) {
                    if self.remove_here_end_tag(&mut state, &mut result, false)? {
                        // If we hit end tag without a trailing newline, try to get next token.
                        continue;
                    }

                    let tag_names = self
                        .cross_state
                        .current_here_tags
                        .iter()
                        .map(|tag| tag.tag.trim())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let tag_positions = self
                        .cross_state
                        .current_here_tags
                        .iter()
                        .map(|tag| std::format!("{}", tag.position))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(TokenizerError::UnterminatedHereDocuments(
                        tag_names,
                        tag_positions,
                    ));
                }

                result = state
                    .delimit_current_token(TokenEndReason::EndOfInput, &mut self.cross_state)?;
            //
            // Handle being in a here document.
            //
            } else if matches!(self.cross_state.here_state, HereState::InHereDocs) {
                //
                // For now, just include the character in the current token. We also check
                // if there are leading tabs to be removed.
                //
                // In `$(...)`, a line of the tag and the `)` ends the here-document there,
                // and the `)` ends the substitution, as Bash reads `v=$(cat <<EOF` ... `EOF)`
                // (with a warning). It was read to the end of the input.
                let closing = (terminating_char == Some(c) && c == ')')
                    .then(|| self.cross_state.current_here_tags.first())
                    .flatten()
                    .map(|tag| {
                        let name = if tag.tag_was_escaped_or_quoted {
                            unquote_str(tag.tag.as_str())
                        } else {
                            tag.tag.clone()
                        };
                        let name = name.trim_start_matches('\t').trim_end_matches('\n');
                        (name.to_owned(), tag.position.line)
                    });
                if let Some((name, line)) = closing
                    && self.remove_here_end_tag(&mut state, &mut result, false)?
                {
                    // The `)` is read again, as the end of the substitution.
                    let closed_on = self.cross_state.cursor.line;
                    self.cross_state
                        .closed_by_parenthesis
                        .push((name, line, closed_on));
                } else if !self.cross_state.current_here_tags.is_empty()
                    && self.cross_state.current_here_tags[0].remove_tabs
                    && (!state.started_token() || state.current_token().ends_with('\n'))
                    && c == '\t'
                {
                    // Consume it but don't include it.
                    self.consume_char()?;
                } else {
                    self.consume_char()?;
                    state.append_char(c);

                    // See if this was a newline character following the terminating here tag.
                    if c == '\n' {
                        self.remove_here_end_tag(&mut state, &mut result, true)?;
                    }
                }
            //
            // Look for the specially specified terminating char.
            //
            } else if state.unquoted() && terminating_char == Some(c) {
                result = state.delimit_current_token(
                    TokenEndReason::SpecifiedTerminatingChar,
                    &mut self.cross_state,
                )?;
            } else if state.in_operator() {
                //
                // We're in an operator. See if this character continues an operator, or if it
                // must be a separate token (because it wouldn't make a prefix of an operator).
                //

                let mut hypothetical_token = state.current_token().to_owned();
                hypothetical_token.push(c);

                if state.unquoted() && self.is_operator(hypothetical_token.as_ref()) {
                    self.consume_char()?;
                    state.append_char(c);
                } else {
                    assert!(state.started_token());

                    //
                    // N.B. If the completed operator indicates a here-document, then keep
                    // track that the *next* token should be the here-tag.
                    //
                    if self.cross_state.arithmetic_expansion {
                        //
                        // We're in an arithmetic context; don't consider << and <<-
                        // special. They're not here-docs, they're either a left-shift
                        // operator or a left-shift operator followed by a unary
                        // minus operator.
                        //

                        if state.is_specific_operator(")") && c == ')' {
                            self.cross_state.arithmetic_expansion = false;
                        }
                    } else if state.is_specific_operator("<<") {
                        self.cross_state.here_state =
                            HereState::NextTokenIsHereTag { remove_tabs: false };
                    } else if state.is_specific_operator("<<-") {
                        self.cross_state.here_state =
                            HereState::NextTokenIsHereTag { remove_tabs: true };
                    } else if state.is_specific_operator("(") && c == '(' {
                        self.cross_state.arithmetic_expansion = true;
                    }

                    let reason = if state.current_token() == "\n" {
                        TokenEndReason::UnescapedNewLine
                    } else {
                        TokenEndReason::OperatorEnd
                    };

                    result = state.delimit_current_token(reason, &mut self.cross_state)?;
                }
            //
            // See if this is a character that changes the current escaping/quoting state.
            //
            } else if does_char_newly_affect_quoting(&state, c) {
                if c == '\\' {
                    // Consume the backslash ourselves so we can peek past it.
                    self.consume_char()?;

                    if matches!(self.peek_char()?, Some('\n')) {
                        // Make sure the newline char gets consumed too.
                        self.consume_char()?;

                        // Make sure to include neither the backslash nor the newline character.
                    } else {
                        state.in_escape = true;
                        state.append_char(c);
                    }
                } else if c == '\'' {
                    if state.token_so_far.ends_with('$') {
                        state.quote_mode = QuoteMode::AnsiC(self.cross_state.cursor.clone());
                    } else {
                        state.quote_mode = QuoteMode::Single(self.cross_state.cursor.clone());
                    }

                    self.consume_char()?;
                    state.append_char(c);
                } else if c == '\"' {
                    state.quote_mode = QuoteMode::Double(self.cross_state.cursor.clone());
                    self.consume_char()?;
                    state.append_char(c);
                }
            }
            //
            // Handle end of single-quote, double-quote, or ANSI-C quote.
            else if !state.in_escape
                && matches!(
                    state.quote_mode,
                    QuoteMode::Single(..) | QuoteMode::AnsiC(..)
                )
                && c == '\''
            {
                state.quote_mode = QuoteMode::None;
                self.consume_char()?;
                state.append_char(c);
            } else if !state.in_escape
                && matches!(state.quote_mode, QuoteMode::Double(..))
                && c == '\"'
            {
                state.quote_mode = QuoteMode::None;
                self.consume_char()?;
                state.append_char(c);
            }
            //
            // Handle end of escape sequence.
            // TODO(tokenizer): Handle double-quote specific escape sequences.
            else if state.in_escape {
                state.in_escape = false;
                self.consume_char()?;
                state.append_char(c);
            } else if (state.unquoted()
                || (matches!(state.quote_mode, QuoteMode::Double(_)) && !state.in_escape))
                && (c == '$' || c == '`')
            {
                // TODO(tokenizer): handle quoted $ or ` in a double quote
                if c == '$' {
                    // Consume the '$' so we can peek beyond.
                    self.consume_char()?;

                    // Now peek beyond to see what we have.
                    let char_after_dollar_sign = self.peek_char()?;
                    match char_after_dollar_sign {
                        Some('(') => {
                            // Add the '$' we already consumed to the token.
                            state.append_char('$');

                            // Consume the '(' and add it to the token.
                            state.append_char(self.next_peeked_char()?);

                            // Check to see if this is possibly an arithmetic expression
                            // (i.e., one that starts with `$((`).
                            let (initial_nesting, is_arithmetic) =
                                if matches!(self.peek_char()?, Some('(')) {
                                    // Consume the second '(' and add it to the token.
                                    state.append_char(self.next_peeked_char()?);
                                    (2, true)
                                } else {
                                    (1, false)
                                };

                            // Inside an arithmetic expansion `<<` is a shift. The outer one's
                            // state comes back at the end of an inner one: it was cleared, and
                            // `$(( $((1)) << 2 ))` read `<< 2` as a here-document (PI-05).
                            // A command substitution's text is commands, even inside one.
                            let outer_arithmetic = self.cross_state.arithmetic_expansion;
                            self.cross_state.arithmetic_expansion = is_arithmetic;

                            self.consume_nested_construct(&mut state, ')', "(", initial_nesting)?;

                            self.cross_state.arithmetic_expansion = outer_arithmetic;
                        }

                        Some('[') => {
                            // Add the '$' we already consumed to the token.
                            state.append_char('$');

                            // Consume the '[' and add it to the token.
                            state.append_char(self.next_peeked_char()?);

                            // Keep track that we're in an arithmetic expression, since
                            // some text will be interpreted differently as a result; the
                            // outer state comes back at its end, as for `$((`.
                            let outer_arithmetic = self.cross_state.arithmetic_expansion;
                            self.cross_state.arithmetic_expansion = true;

                            self.consume_nested_construct(&mut state, ']', "[", 1)?;

                            self.cross_state.arithmetic_expansion = outer_arithmetic;
                        }

                        Some('{') => {
                            // Add the '$' we already consumed to the token.
                            state.append_char('$');

                            // Consume the '{' and add it to the token.
                            state.append_char(self.next_peeked_char()?);

                            // `${ cmd; }` and `${| cmd; }` run commands: a brace group in them
                            // (a function's body) has a `}` of its own, which does not end
                            // them, as in Bash. The first `}` ended them.
                            let funsub = matches!(self.peek_char()?, Some(' ' | '\t' | '\n' | '|'));
                            let mut groups = 0_usize;

                            let mut pending_here_doc_tokens = vec![];
                            let mut drain_here_doc_tokens = false;

                            // The words inside are the expansion's, not the line's: with a
                            // here-document pending on the line, `cat <<EOF >"${f}"` lost
                            // `f` and read `${}`.
                            let outer = self.set_aside_here_documents();
                            self.cross_state.braced_expansions += 1;

                            loop {
                                let cur_token = if drain_here_doc_tokens
                                    && !pending_here_doc_tokens.is_empty()
                                {
                                    if pending_here_doc_tokens.len() == 1 {
                                        drain_here_doc_tokens = false;
                                    }

                                    pending_here_doc_tokens.remove(0)
                                } else {
                                    let cur_token = self.next_token_until(
                                        Some('}'),
                                        true, /* preserve whitespace in substitution */
                                    )?;

                                    // See if this is a here-document-related token we need to hold
                                    // onto until after we've seen all the tokens that need to show
                                    // up before we get to the body.
                                    if matches!(
                                        cur_token.reason,
                                        TokenEndReason::HereDocumentBodyStart
                                            | TokenEndReason::HereDocumentBodyEnd
                                            | TokenEndReason::HereDocumentEndTag
                                    ) {
                                        pending_here_doc_tokens.push(cur_token);
                                        continue;
                                    }

                                    cur_token
                                };

                                if matches!(cur_token.reason, TokenEndReason::UnescapedNewLine)
                                    && !pending_here_doc_tokens.is_empty()
                                {
                                    pending_here_doc_tokens.push(cur_token);
                                    drain_here_doc_tokens = true;
                                    continue;
                                }

                                if let Some(cur_token_value) = cur_token.token {
                                    // Whitespace before a token is kept with it here.
                                    if funsub && cur_token_value.to_str().trim() == "{" {
                                        groups += 1;
                                    }
                                    state.append_str(cur_token_value.to_str());
                                }

                                match cur_token.reason {
                                    TokenEndReason::HereDocumentBodyStart => {
                                        state.append_char('\n');
                                    }
                                    TokenEndReason::NonNewLineBlank(blank) => {
                                        state.append_char(blank);
                                    }
                                    // The end of a brace group inside, not of the expansion.
                                    TokenEndReason::SpecifiedTerminatingChar if groups > 0 => {
                                        state.append_char(self.next_peeked_char()?);
                                        groups -= 1;
                                    }
                                    TokenEndReason::SpecifiedTerminatingChar => {
                                        // We hit the end brace we were looking for but did not
                                        // yet consume it. Do so now.
                                        state.append_char(self.next_peeked_char()?);
                                        break;
                                    }
                                    TokenEndReason::EndOfInput => {
                                        return Err(TokenizerError::UnterminatedVariable);
                                    }
                                    _ => (),
                                }
                            }

                            self.cross_state.braced_expansions -= 1;
                            self.take_back_here_documents(outer);
                        }
                        _ => {
                            // This is either a different character, or else the end of the string.
                            // Either way, add the '$' we already consumed to the token.
                            state.append_char('$');
                        }
                    }
                } else {
                    // We look for the terminating backquote. First disable normal consumption and
                    // consume the starting backquote.
                    let backquote_pos = self.cross_state.cursor.clone();
                    self.consume_char()?;

                    // Add the opening backquote to the token.
                    state.append_char(c);

                    // Now continue until we see an unescaped backquote.
                    let mut escaping_enabled = false;
                    let mut done = false;
                    while !done {
                        // Read (and consume) the next char.
                        let next_char_in_backquote = self.next_char()?;
                        if let Some(cib) = next_char_in_backquote {
                            // Include it in the token no matter what.
                            state.append_char(cib);

                            // Watch out for escaping.
                            if !escaping_enabled && cib == '\\' {
                                escaping_enabled = true;
                            } else {
                                // Look for an unescaped backquote to terminate.
                                if !escaping_enabled && cib == '`' {
                                    done = true;
                                }
                                escaping_enabled = false;
                            }
                        } else {
                            return Err(TokenizerError::UnterminatedBackquote(backquote_pos));
                        }
                    }
                }
            }
            //
            // [Extension]
            // If extended globbing is enabled, the last consumed character is an
            // unquoted start of an extglob pattern, *and* if the current character
            // is an open parenthesis, then this begins an extglob pattern.
            else if c == '('
                && (self.options.enable_extended_globbing || self.cross_state.in_double_brackets)
                && state.unquoted()
                && !state.in_operator()
                && state
                    .current_token()
                    .ends_with(|x| Self::can_start_extglob(x))
            {
                // Consume the '(' and append it.
                self.consume_char()?;
                state.append_char(c);

                let mut paren_depth = 1;
                let mut in_escape = false;

                // Keep consuming until we see the matching end ')'.
                while paren_depth > 0 {
                    if let Some(extglob_char) = self.next_char()? {
                        // Include it in the token.
                        state.append_char(extglob_char);

                        match extglob_char {
                            _ if in_escape => in_escape = false,
                            '\\' => in_escape = true,
                            '(' => paren_depth += 1,
                            ')' => paren_depth -= 1,
                            _ => (),
                        }
                    } else {
                        return Err(TokenizerError::UnterminatedExtendedGlob(
                            self.cross_state.cursor.clone(),
                        ));
                    }
                }
            //
            // [bash] A subscript where an assignment may be is read to its `]`, its blanks
            // too, as Bash reads it: `map[set auto]=on`, `([k one]=1)`.
            } else if c == '['
                && state.unquoted()
                && !state.in_operator()
                && !include_space
                && self.subscript_follows(state.current_token())
            {
                self.consume_char()?;
                state.append_char(c);
                self.consume_nested_construct(&mut state, ']', "[", 1)?;
            //
            // [bash] A process substitution inside a word is part of the word:
            // `--file=<(cmd)`, `f=<(cmd)`. It ended the word there, and `--file=` and the
            // substitution's path were two arguments. One that begins a word is still
            // read as the operator it starts, as before.
            } else if (c == '<' || c == '>')
                && state.unquoted()
                && !state.in_operator()
                && state.started_token()
                && matches!(self.peek_second_char()?, Some('('))
            {
                self.consume_char()?;
                state.append_char(c);
                state.append_char(self.next_peeked_char()?);
                self.consume_nested_construct(&mut state, ')', "(", 1)?;
            //
            // If the character *can* start an operator, then it will.
            //
            } else if state.unquoted() && Self::can_start_operator(c) {
                if state.started_token() {
                    result = state.delimit_current_token(
                        TokenEndReason::OperatorStart,
                        &mut self.cross_state,
                    )?;
                } else {
                    state.token_is_operator = true;
                    self.consume_char()?;
                    state.append_char(c);
                }
            //
            // Whitespace gets discarded (and delimits tokens).
            //
            } else if state.unquoted() && is_blank(c) {
                if state.started_token() {
                    result = state.delimit_current_token(
                        TokenEndReason::NonNewLineBlank(c),
                        &mut self.cross_state,
                    )?;
                } else if include_space {
                    state.append_char(c);
                } else {
                    // Make sure we don't include this char in the token range.
                    state.start_position.column += 1;
                    state.start_position.index += 1;
                }

                self.consume_char()?;
            }
            //
            // N.B. We need to remember if we were recursively called in a variable
            // expansion expression; in that case we won't think a token was started but...
            // we'd be wrong.
            //
            // The `!only_blanks_so_far` clause keeps a comment recognizable inside a nested
            // construct. With `include_space`, a blank is appended to the token when none is
            // started and delimits the token when one is, so the blanks before a `#` alternate
            // between the two; after an odd number of them a token is "in progress" and the `#`
            // was appended to it rather than starting a comment. `$( #'<newline>)` then failed to
            // tokenize with an unterminated single quote, while `$(  #'<newline>)` — two blanks —
            // was fine.
            //
            else if !state.token_is_operator
                && (state.started_token() || matches!(terminating_char, Some('}')))
                && !(c == '#' && state.only_blanks_so_far())
            {
                self.consume_char()?;
                state.append_char(c);
            } else if c == '#' {
                // Consume the '#'.
                self.consume_char()?;

                let mut done = false;
                while !done {
                    done = match self.peek_char()? {
                        Some('\n') => true,
                        None => true,
                        _ => {
                            // Consume the peeked char; it's part of the comment.
                            self.consume_char()?;
                            false
                        }
                    };
                }
                // Re-start loop as if the comment never happened.
            } else if state.started_token() {
                // In all other cases where we have an in-progress token, we delimit here.
                result =
                    state.delimit_current_token(TokenEndReason::Other, &mut self.cross_state)?;
            } else {
                // If we got here, then we don't have a token in progress and we're not starting an
                // operator. Add the character to a new token.
                self.consume_char()?;
                state.append_char(c);
            }
        }

        // The loop above ends only once there is a result.
        result.ok_or(TokenizerError::UnterminatedExpansion)
    }

    fn remove_here_end_tag(
        &mut self,
        state: &mut TokenParseState,
        result: &mut Option<TokenizeResult>,
        ends_with_newline: bool,
    ) -> Result<bool, TokenizerError> {
        // Bail immediately if we don't even have a *starting* here tag.
        if self.cross_state.current_here_tags.is_empty() {
            return Ok(false);
        }

        let next_here_tag = &self.cross_state.current_here_tags[0];

        let tag_str: Cow<'_, str> = if next_here_tag.tag_was_escaped_or_quoted {
            unquote_str(next_here_tag.tag.as_str()).into()
        } else {
            next_here_tag.tag.as_str().into()
        };
        // `<<-` takes the tabs that begin the tag off it too, as off its lines, as Bash
        // does: `<<-'<TAB>END'` never ended.
        let tag_str: Cow<'_, str> = if next_here_tag.remove_tabs {
            Cow::Owned(tag_str.trim_start_matches('\t').to_owned())
        } else {
            tag_str
        };

        let tag_str = if !ends_with_newline {
            tag_str
                .strip_suffix('\n')
                .unwrap_or_else(|| tag_str.as_ref())
        } else {
            tag_str.as_ref()
        };

        let quoted = next_here_tag.tag_was_escaped_or_quoted;
        let mut body = None;
        if let Some(current_token_without_here_tag) = state.current_token().strip_suffix(tag_str) {
            // Make sure that was either the start of the here document, or there
            // was a newline between the preceding part
            // and the tag.
            if current_token_without_here_tag.is_empty()
                || current_token_without_here_tag.ends_with('\n')
            {
                body = Some(current_token_without_here_tag.to_owned());
            }
        }
        // In a here-document whose tag is not quoted, a backslash-newline joins two lines
        // before the tag is looked for, as in Bash: `EO\` and `F` end it. They were two
        // lines of the body.
        if body.is_none() && !quoted && ends_with_newline {
            body =
                Self::body_before_joined_tag(state.current_token(), tag_str.trim_end_matches('\n'));
        }
        if let Some(body) = body {
            state.replace_with_here_doc(body);

            // Delimit the end of the here-document body.
            *result = state.delimit_current_token(
                TokenEndReason::HereDocumentBodyEnd,
                &mut self.cross_state,
            )?;

            return Ok(true);
        }
        Ok(false)
    }

    /// The body before the last line of `token`, when that line, its backslash-newlines
    /// joined, is `tag`: `EO\` and `F` on two lines of the token are the tag `EOF`.
    fn body_before_joined_tag(token: &str, tag: &str) -> Option<String> {
        let text = token.strip_suffix('\n')?;
        if !text.contains("\\\n") {
            return None;
        }
        // The last line begins after the last newline that no backslash escapes.
        let bytes = text.as_bytes();
        let start = (0..bytes.len())
            .rev()
            .find(|&at| {
                bytes.get(at) == Some(&b'\n')
                    && bytes.get(..at).is_some_and(|before| {
                        before.iter().rev().take_while(|&&b| b == b'\\').count() % 2 == 0
                    })
            })
            .map_or(0, |at| at + 1);
        let line = text.get(start..)?.replace("\\\n", "");
        (line == tag).then(|| text.get(..start).unwrap_or_default().to_owned())
    }

    const fn can_start_extglob(c: char) -> bool {
        matches!(c, '@' | '!' | '?' | '+' | '*')
    }

    const fn can_start_operator(c: char) -> bool {
        matches!(c, '&' | '(' | ')' | ';' | '\n' | '|' | '<' | '>')
    }

    fn is_operator(&self, s: &str) -> bool {
        // Handle non-POSIX operators.
        if !self.options.sh_mode && matches!(s, "<<<" | "&>" | "&>>" | ";;&" | ";&" | "|&") {
            return true;
        }

        matches!(
            s,
            "&" | "&&"
                | "("
                | ")"
                | ";"
                | ";;"
                | "\n"
                | "|"
                | "||"
                | "<"
                | ">"
                | ">|"
                | "<<"
                | ">>"
                | "<&"
                | ">&"
                | "<<-"
                | "<>"
        )
    }
}

impl<R: ?Sized + std::io::BufRead> Iterator for Tokenizer<'_, R> {
    type Item = Result<TokenizeResult, TokenizerError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_token() {
            #[expect(clippy::manual_map)]
            Ok(result) => match result.token {
                Some(_) => Some(Ok(result)),
                None => None,
            },
            Err(e) => Some(Err(e)),
        }
    }
}

const fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// Whether `word` is a name, a variable's: a letter or `_`, then letters, digits and `_`.
fn is_name(word: &str) -> bool {
    let mut chars = word.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Whether `word` is an assignment: `name=`, `name+=`, or either after a subscript,
/// `name[…]=`, whose `]` is the one that closes its `[`.
fn is_assignment_word(word: &str) -> bool {
    let name_end = word
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(word.len());
    let (name, mut rest) = word.split_at(name_end);
    if !is_name(name) {
        return false;
    }
    if rest.starts_with('[') {
        let mut depth = 0usize;
        let Some(close) = rest.char_indices().find_map(|(at, c)| {
            match c {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(at)
        }) else {
            return false;
        };
        rest = rest.get(close + 1..).unwrap_or_default();
    }
    rest.starts_with('=') || rest.starts_with("+=")
}

const fn does_char_newly_affect_quoting(state: &TokenParseState, c: char) -> bool {
    // If we're currently escaped, then nothing affects quoting.
    if state.in_escape {
        return false;
    }

    match state.quote_mode {
        // When we're in a double quote or ANSI-C quote, only a subset of escape
        // sequences are recognized.
        QuoteMode::Double(_) | QuoteMode::AnsiC(_) => {
            if c == '\\' {
                // TODO(tokenizer): handle backslash in double quote
                true
            } else {
                false
            }
        }
        // When we're in a single quote, nothing affects quoting.
        QuoteMode::Single(_) => false,
        // When we're not already in a quote, then we can straightforwardly look for a
        // quote mark or backslash.
        QuoteMode::None => is_quoting_char(c),
    }
}

const fn is_quoting_char(c: char) -> bool {
    matches!(c, '\\' | '\'' | '\"')
}

/// Return a string with all the quoting removed.
///
/// # Arguments
///
/// * `s` - The string to unquote.
pub fn unquote_str(s: &str) -> String {
    let mut result = String::new();

    let mut in_escape = false;
    for c in s.chars() {
        match c {
            c if in_escape => {
                result.push(c);
                in_escape = false;
            }
            '\\' => in_escape = true,
            c if is_quoting_char(c) => (),
            c => result.push(c),
        }
    }

    result
}

#[cfg(test)]
mod tests {

    use super::*;
    use anyhow::Result;
    use insta::assert_ron_snapshot;
    use pretty_assertions::{assert_eq, assert_matches};

    #[derive(serde::Serialize, serde::Deserialize)]
    struct TokenizerResult<'a> {
        input: &'a str,
        result: Vec<Token>,
    }

    fn test_tokenizer(input: &str) -> Result<TokenizerResult<'_>> {
        Ok(TokenizerResult {
            input,
            result: tokenize_str(input)?,
        })
    }

    #[test]
    fn tokenize_process_substitution_inside_a_word() {
        // Inside a word it is part of the word, as in Bash; at a word's start it is the
        // operator, as before.
        let words: Vec<String> = tokenize_str("echo --file=<(echo hi) <(x)")
            .unwrap()
            .iter()
            .map(|token| token.to_str().to_owned())
            .collect();
        assert_eq!(words, ["echo", "--file=<(echo hi)", "<", "(", "x", ")"]);
    }

    #[test]
    fn tokenize_empty() -> Result<()> {
        let tokens = tokenize_str("")?;
        assert_eq!(tokens.len(), 0);
        Ok(())
    }

    #[test]
    fn tokenize_line_continuation() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"a\
bc"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_operators() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("a>>b")?);
        Ok(())
    }

    #[test]
    fn tokenize_comment() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"a #comment
"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_comment_in_command_substitution() {
        // A comment inside `$( )` is a comment however many blanks precede its `#`. An odd number
        // of them used to leave a token in progress, so the `#` was appended to that token instead
        // of starting a comment, and the apostrophe in the comment's text then opened a quote that
        // was never closed.
        //
        // The comment is dropped from the text reconstructed for the substitution, leaving just
        // the blanks that preceded it. A blank that delimits an in-progress token comes back as a
        // single space, so the blanks aren't always reproduced verbatim; that's insignificant
        // inside `$( )`, where the text gets re-parsed as a program.
        for (prefix, reconstructed_blanks) in [
            ("", ""),
            (" ", " "),
            ("  ", "  "),
            ("   ", "   "),
            ("\t", "\t"),
            ("\t\t", "\t "),
            (" \t", "  "),
            ("\t ", "\t "),
        ] {
            let input = format!("$({prefix}# it's a comment\n)\n");
            let tokens = tokenize_str(input.as_str()).unwrap();
            let token_strs: Vec<_> = tokens.iter().map(Token::to_str).collect();
            assert_eq!(
                token_strs,
                [format!("$({reconstructed_blanks}\n)").as_str(), "\n"],
                "tokenizing {input:?}"
            );
        }
    }

    #[test]
    fn tokenize_a_brace_group_inside_a_current_shell_substitution() {
        // A function's body in `${ ... }` has a `}` of its own; the first `}` ended the
        // substitution.
        for input in ["x=${ g() { echo in; }; g; }", "x=${ { echo a; }; }"] {
            let tokens = tokenize_str(input).unwrap();
            let token_strs: Vec<_> = tokens.iter().map(Token::to_str).collect();
            assert_eq!(token_strs, [input], "tokenizing {input:?}");
        }
    }

    #[test]
    fn tokenize_tab_in_parameter_expansion() {
        // The inside of `${ }` is tokenized and put back together; a tab that ended one of its
        // tokens came back as a space, so `${count#0<TAB>}` (git-prompt.sh's upstream count)
        // looked for `0<SPACE>` and never matched.
        for input in [
            "${count#0\t}",
            "${count%\t0}",
            "${count#*\t}",
            "${u:-a\tb}",
            "${u:-a\t\tb}",
            "${u:-a \tb}",
            "\"${count#0\t}\"",
        ] {
            let tokens = tokenize_str(input).unwrap();
            let token_strs: Vec<_> = tokens.iter().map(Token::to_str).collect();
            assert_eq!(token_strs, [input], "tokenizing {input:?}");
        }
    }

    #[test]
    fn tokenize_comment_at_eof() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r"a #comment")?);
        Ok(())
    }

    #[test]
    fn tokenize_empty_here_doc() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<HERE
HERE
"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_here_doc() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<HERE
SOMETHING
HERE
echo after
"
        )?);
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<HERE
SOMETHING
HERE
"
        )?);
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<HERE
SOMETHING
HERE

"
        )?);
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<HERE
SOMETHING
HERE"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_here_doc_with_tab_removal() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<-HERE
	SOMETHING
	HERE
"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_here_doc_with_other_tokens() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<EOF | wc -l
A B C
1 2 3
D E F
EOF
"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_multiple_here_docs() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<HERE1 <<HERE2
SOMETHING
HERE1
OTHER
HERE2
echo after
"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_unterminated_here_doc() {
        let result = tokenize_str(
            r"cat <<HERE
SOMETHING
",
        );
        assert!(result.is_err());
    }

    #[test]
    fn tokenize_missing_here_tag() {
        let result = tokenize_str(
            r"cat <<
",
        );
        assert!(result.is_err());
    }

    #[test]
    fn tokenize_here_doc_in_command_substitution() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"echo $(cat <<HERE
TEXT
HERE
)"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_here_doc_in_double_quoted_command_substitution() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r#"echo "$(cat <<HERE
TEXT
HERE
)""#
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_here_doc_in_double_quoted_command_substitution_with_space() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r#"echo "$(cat << HERE
TEXT
HERE
)""#
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_complex_here_docs_in_command_substitution() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"echo $(cat <<HERE1 <<HERE2 | wc -l
TEXT
HERE1
OTHER
HERE2
)"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_simple_backquote() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r"echo `echo hi`")?);
        Ok(())
    }

    #[test]
    fn tokenize_backquote_with_escape() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r"echo `echo\`hi`")?);
        Ok(())
    }

    #[test]
    fn tokenize_unterminated_backquote() {
        assert_matches!(
            tokenize_str("`"),
            Err(TokenizerError::UnterminatedBackquote(_))
        );
    }

    #[test]
    fn tokenize_unterminated_command_substitution() {
        // $( is consumed before the tokenizer knows whether it's $( or $((,
        // so it goes through consume_nested_construct and yields UnterminatedExpansion.
        assert_matches!(
            tokenize_str("$("),
            Err(TokenizerError::UnterminatedExpansion)
        );
    }

    #[test]
    fn tokenize_unterminated_arithmetic_expansion() {
        assert_matches!(
            tokenize_str("$(("),
            Err(TokenizerError::UnterminatedExpansion)
        );
    }

    #[test]
    fn tokenize_unterminated_legacy_arithmetic_expansion() {
        assert_matches!(
            tokenize_str("$["),
            Err(TokenizerError::UnterminatedExpansion)
        );
    }

    #[test]
    fn tokenize_command_substitution() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("a$(echo hi)b c")?);
        Ok(())
    }

    #[test]
    fn tokenize_command_substitution_with_subshell() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("$( (:) )")?);
        Ok(())
    }

    #[test]
    fn tokenize_command_substitution_containing_case() -> Result<()> {
        // cash (D2): a case pattern's `)` has no opener, so a plain paren count stops
        // the substitution at `a*)` and truncates it to `case a in a*`. The whole
        // construct must survive as one token.
        let tokens = tokenize_str("$(case a in a*) echo m;; esac)")?;
        assert_eq!(tokens.len(), 1, "expected one token, got {tokens:?}");
        assert_eq!(tokens[0].to_str(), "$(case a in a*) echo m;; esac)");

        // A parenthesised pattern brings its own opener and must still balance.
        let tokens = tokenize_str("$(case a in (a*) echo m;; esac)")?;
        assert_eq!(tokens[0].to_str(), "$(case a in (a*) echo m;; esac)");

        // Nested cases, and `esac` appearing inside a quoted string.
        let tokens = tokenize_str("$(case a in a) case b in b) echo n;; esac;; esac)")?;
        assert_eq!(
            tokens.len(),
            1,
            "nested case not consumed whole: {tokens:?}"
        );

        let tokens = tokenize_str("$(case a in a) echo \"an esac here\";; esac)")?;
        assert_eq!(
            tokens.len(),
            1,
            "quoted esac ended the construct: {tokens:?}"
        );

        // A word merely containing `esac` must not terminate anything.
        let tokens = tokenize_str("$(echo esacular)")?;
        assert_eq!(tokens[0].to_str(), "$(echo esacular)");

        // An `esac` in a pattern list is a pattern, as `in` is; only one that begins the
        // list ends the case. A `for`'s `in` starts no patterns, and a run of blanks
        // before the `esac` is no pattern word.
        let tokens = tokenize_str("$(case x in in|esac) echo no;; x) echo yes;; esac)")?;
        assert_eq!(
            tokens[0].to_str(),
            "$(case x in in|esac) echo no;; x) echo yes;; esac)"
        );
        let tokens = tokenize_str("$(case a in a) for i in 1; do :; done;; esac)")?;
        assert_eq!(
            tokens[0].to_str(),
            "$(case a in a) for i in 1; do :; done;; esac)"
        );
        let tokens = tokenize_str("$(\n\t\t\t\tcase \"$x\" in\n\t\t\t\tesac)")?;
        assert_eq!(tokens.len(), 1, "{tokens:?}");

        Ok(())
    }

    #[test]
    fn tokenize_command_substitution_containing_extglob() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("echo $(echo !(x))")?);
        Ok(())
    }

    #[test]
    fn tokenize_arithmetic_expression() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("a$((1+2))b c")?);
        Ok(())
    }

    #[test]
    fn tokenize_arithmetic_expression_with_space() -> Result<()> {
        // N.B. The spacing comes out a bit odd, but it gets processed okay
        // by later stages.
        assert_ron_snapshot!(test_tokenizer("$(( 1 ))")?);
        Ok(())
    }
    #[test]
    fn tokenize_arithmetic_expression_with_parens() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("$(( (0) ))")?);
        Ok(())
    }

    #[test]
    fn tokenize_special_parameters() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("$$")?);
        assert_ron_snapshot!(test_tokenizer("$@")?);
        assert_ron_snapshot!(test_tokenizer("$!")?);
        assert_ron_snapshot!(test_tokenizer("$?")?);
        assert_ron_snapshot!(test_tokenizer("$*")?);
        Ok(())
    }

    #[test]
    fn tokenize_unbraced_parameter_expansion() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("$x")?);
        assert_ron_snapshot!(test_tokenizer("a$x")?);
        Ok(())
    }

    #[test]
    fn tokenize_unterminated_parameter_expansion() {
        assert_matches!(
            tokenize_str("${x"),
            Err(TokenizerError::UnterminatedVariable)
        );
    }

    #[test]
    fn tokenize_braced_parameter_expansion() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("${x}")?);
        assert_ron_snapshot!(test_tokenizer("a${x}b")?);
        Ok(())
    }

    #[test]
    fn tokenize_braced_parameter_expansion_with_escaping() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r"a${x\}}b")?);
        Ok(())
    }

    #[test]
    fn tokenize_whitespace() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("1 2 3")?);
        Ok(())
    }

    #[test]
    fn tokenize_escaped_whitespace() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r"1\ 2 3")?);
        Ok(())
    }

    #[test]
    fn tokenize_single_quote() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r"x'a b'y")?);
        Ok(())
    }

    #[test]
    fn tokenize_double_quote() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r#"x"a b"y"#)?);
        Ok(())
    }

    #[test]
    fn tokenize_double_quoted_command_substitution() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r#"x"$(echo hi)"y"#)?);
        Ok(())
    }

    #[test]
    fn tokenize_double_quoted_arithmetic_expression() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r#"x"$((1+2))"y"#)?);
        Ok(())
    }

    #[test]
    fn test_quote_removal() {
        assert_eq!(unquote_str(r#""hello""#), "hello");
        assert_eq!(unquote_str(r"'hello'"), "hello");
        assert_eq!(unquote_str(r#""hel\"lo""#), r#"hel"lo"#);
        assert_eq!(unquote_str(r"'hel\'lo'"), r"hel'lo");
    }
}
