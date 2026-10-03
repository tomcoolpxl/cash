use crate::tokenizer;

/// Represents an error that occurred while parsing tokens.
#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    /// A parsing error occurred at a token: where it starts, and the token as Bash names
    /// it (`newline` for a line's end).
    #[error("syntax error near unexpected token `{1}'")]
    ParsingNear(crate::SourcePosition, String),

    /// A parsing error occurred at the end of the input.
    #[error("syntax error: unexpected end of file")]
    ParsingAtEndOfInput,

    /// The input ended inside a compound command, as Bash 5.3 reports it: naming the
    /// command and the line it started on.
    #[error("syntax error: unexpected end of file from `{keyword}' command on line {line}")]
    UnterminatedCompound {
        /// The reserved word that opened the command (`if`, `while`, `{`, ...).
        keyword: String,
        /// The 1-based line of that word.
        line: usize,
    },

    /// The input ended inside an array assignment's `(`, as Bash reports it, on the line
    /// the `(` is on.
    #[error("unexpected EOF while looking for matching `)'")]
    UnterminatedArray {
        /// The 1-based line of the `(`.
        line: usize,
    },

    /// An error occurred while tokenizing the input stream.
    #[error("{} (detected near {})", .inner, .position.as_ref().map_or_else(|| String::from("<unknown position>"), |p| std::format!("line {} col {}", p.line, p.column)))]
    Tokenizing {
        /// The inner error.
        inner: tokenizer::TokenizerError,
        /// Optionally provides the position of the error.
        position: Option<crate::SourcePosition>,
    },
}

#[cfg(feature = "diagnostics")]
#[allow(clippy::cast_sign_loss)]
#[allow(unused)] // Workaround unused warnings in nightly versions of the compiler
pub mod miette {
    use super::ParseError;
    use miette::SourceOffset;

    impl ParseError {
        /// Convert the original error to one miette can pretty print
        pub fn to_pretty_error(self, input: impl Into<String>) -> PrettyError {
            let input = input.into();
            let location = match self {
                Self::ParsingNear(ref pos, _) => {
                    Some(SourceOffset::from_location(&input, pos.line, pos.column))
                }
                Self::Tokenizing { ref position, .. } => position
                    .as_ref()
                    .map(|p| SourceOffset::from_location(&input, p.line, p.column)),
                Self::ParsingAtEndOfInput
                | Self::UnterminatedCompound { .. }
                | Self::UnterminatedArray { .. } => {
                    Some(SourceOffset::from_location(&input, usize::MAX, usize::MAX))
                }
            };

            PrettyError {
                cause: self,
                input,
                location,
            }
        }
    }

    /// Represents an error that occurred while parsing tokens.
    #[derive(thiserror::Error, Debug, miette::Diagnostic)]
    #[error("Cannot parse the input script")]
    pub struct PrettyError {
        cause: ParseError,
        #[source_code]
        input: String,
        #[label("{cause}")]
        location: Option<SourceOffset>,
    }
}

/// Represents a parsing error with its location information
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct ParseErrorLocation {
    #[from]
    inner: peg::error::ParseError<peg::str::LineCol>,
}

impl ParseErrorLocation {
    /// The byte offset in the input where parsing failed.
    pub const fn offset(&self) -> usize {
        self.inner.location.offset
    }
}

impl WordParseError {
    /// For an arithmetic expression, the byte offset where parsing it failed.
    pub const fn arithmetic_offset(&self) -> Option<usize> {
        match self {
            Self::ArithmeticExpression(location) => Some(location.offset()),
            _ => None,
        }
    }
}

/// Represents an error that occurred while parsing a word.
#[derive(Debug, thiserror::Error)]
pub enum WordParseError {
    /// An error occurred while parsing an arithmetic expression.
    #[error("failed to parse arithmetic expression")]
    ArithmeticExpression(ParseErrorLocation),

    /// An error occurred while parsing a shell pattern.
    #[error("failed to parse pattern")]
    Pattern(ParseErrorLocation),

    /// An error occurred while parsing a prompt string.
    #[error("failed to parse prompt string")]
    Prompt(ParseErrorLocation),

    /// An error occurred while parsing a parameter.
    #[error("failed to parse parameter '{0}'")]
    Parameter(String, ParseErrorLocation),

    /// An error occurred while parsing for brace expansion.
    #[error("failed to parse for brace expansion: '{0}'")]
    BraceExpansion(String, ParseErrorLocation),

    /// An error occurred while parsing a word.
    #[error("failed to parse word '{0}'")]
    Word(String, ParseErrorLocation),
}

/// Represents an error that occurred while parsing a (non-extended) test command.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct TestCommandParseError(#[from] peg::error::ParseError<usize>);

/// Represents an error that occurred while parsing a key-binding specification.
#[derive(Debug, thiserror::Error)]
pub enum BindingParseError {
    /// An unknown error occurred while parsing a key-binding specification.
    #[error("unknown error while parsing key-binding: '{0}'")]
    Unknown(String),

    /// A key code was missing from the key-binding specification.
    #[error("missing key code in key-binding")]
    MissingKeyCode,
}

pub(crate) fn convert_peg_parse_error(
    err: &peg::error::ParseError<usize>,
    tokens: &[crate::Token],
) -> ParseError {
    let approx_token_index = err.location;

    if approx_token_index < tokens.len() {
        let mut token = &tokens[approx_token_index];
        // The furthest token any rule reached is often the end of the line after the word
        // Bash stops at: `if then`, `fi` alone.
        if matches!(token.to_str(), "\n" | ";")
            && let Some(word) = misplaced_reserved_word(&tokens[..approx_token_index])
        {
            token = word;
        }
        let name = match token.to_str() {
            "\n" => "newline",
            text => text,
        };
        ParseError::ParsingNear((*token.location().start).clone(), name.to_owned())
    } else if let Some((keyword, line)) = innermost_unclosed_compound(tokens) {
        if keyword == "=(" {
            ParseError::UnterminatedArray { line }
        } else {
            ParseError::UnterminatedCompound { keyword, line }
        }
    } else {
        ParseError::ParsingAtEndOfInput
    }
}

/// The first reserved word in `tokens` that cannot stand where it is, the token Bash's
/// parser stops at: `then` with no condition before it, `fi` with no `if` open, `done`
/// after an empty body. The peg parser's error is the furthest token any rule reached,
/// which for these is the end of the line.
///
/// A token-level approximation, as [`innermost_unclosed_compound`] is: a `case` is
/// skipped to its `esac`, and the head of a `for` or `select` to its `;` or newline.
fn misplaced_reserved_word(tokens: &[crate::Token]) -> Option<&crate::Token> {
    let mut scan = Scan {
        command_position: true,
        ..Scan::default()
    };
    for token in tokens {
        let word = match token {
            crate::Token::Operator(op, _) => {
                let op = op.as_str();
                if scan.in_head && matches!(op, "\n" | ";") {
                    scan.in_head = false;
                }
                scan.command_position = matches!(
                    op,
                    "\n" | ";" | ";;" | "&" | "&&" | "||" | "|" | "|&" | "(" | ")"
                );
                continue;
            }
            crate::Token::Word(word, _) => word.as_str(),
        };
        if scan.cases > 0 {
            match word {
                "case" => scan.cases += 1,
                "esac" => scan.cases -= 1,
                _ => {}
            }
            continue;
        }
        if scan.in_head || !scan.command_position {
            continue;
        }
        if !scan.takes(word) {
            return Some(token);
        }
        scan.command_position = matches!(
            word,
            "if" | "then" | "else" | "elif" | "while" | "until" | "do" | "{" | "!" | "time"
        );
    }
    None
}

/// What [`misplaced_reserved_word`] has seen so far.
#[derive(Default)]
struct Scan<'a> {
    /// The compound commands open, innermost last.
    open: Vec<Open<'a>>,
    /// Whether the next word starts a command.
    command_position: bool,
    /// How many `case`s deep the scan is, skipping them.
    cases: usize,
    /// Whether the scan is in a `for` or `select` head, which it skips.
    in_head: bool,
}

/// A compound command open at a point of the scan, in the part `phase` names: 0 its
/// condition or head, 1 its body (`then` or `do`), 2 an `else`.
struct Open<'a> {
    keyword: &'a str,
    phase: u8,
    commands: bool,
}

impl<'a> Scan<'a> {
    /// Takes `word`, in command position: false if it is a reserved word that cannot
    /// stand there.
    fn takes(&mut self, word: &'a str) -> bool {
        let top = self.open.last_mut();
        match (word, top) {
            ("if" | "while" | "until" | "{" | "for" | "select", _) => {
                self.open.push(Open {
                    keyword: word,
                    phase: u8::from(word == "{"),
                    commands: false,
                });
                self.in_head = matches!(word, "for" | "select");
                true
            }
            ("case", _) => {
                self.cases += 1;
                true
            }
            ("then", Some(top)) if top.keyword == "if" && top.phase == 0 && top.commands => {
                top.phase = 1;
                top.commands = false;
                true
            }
            ("elif" | "else", Some(top))
                if top.keyword == "if" && top.phase == 1 && top.commands =>
            {
                top.phase = if word == "elif" { 0 } else { 2 };
                top.commands = false;
                true
            }
            ("do", Some(top))
                if top.phase == 0
                    && (matches!(top.keyword, "for" | "select")
                        || (matches!(top.keyword, "while" | "until") && top.commands)) =>
            {
                top.phase = 1;
                top.commands = false;
                true
            }
            ("fi", Some(top)) if top.keyword == "if" && top.phase >= 1 && top.commands => {
                self.close()
            }
            ("done", Some(top))
                if matches!(top.keyword, "while" | "until" | "for" | "select")
                    && top.phase == 1
                    && top.commands =>
            {
                self.close()
            }
            ("}", Some(top)) if top.keyword == "{" && top.commands => self.close(),
            ("then" | "elif" | "else" | "do" | "fi" | "done" | "}", _) => false,
            ("!" | "time", _) | (_, None) => true,
            (_, Some(top)) => {
                top.commands = true;
                true
            }
        }
    }

    /// Closes the innermost compound command, which counts as a command of the one
    /// around it.
    fn close(&mut self) -> bool {
        self.open.pop();
        if let Some(parent) = self.open.last_mut() {
            parent.commands = true;
        }
        true
    }
}

/// Finds the innermost compound command still open at the end of `tokens`, returning
/// its opening reserved word and line.
///
/// This is a token-level approximation of the parser's own nesting: a word counts as a
/// reserved word only in command position, after an operator or another reserved word
/// that starts a command list.
fn innermost_unclosed_compound(tokens: &[crate::Token]) -> Option<(String, usize)> {
    let mut open: Vec<(&str, usize)> = Vec::new();
    let mut command_position = true;
    let mut after_assignment = false;
    for token in tokens {
        let assignment = after_assignment;
        after_assignment = matches!(token, crate::Token::Word(word, _) if word.ends_with('='));
        match token {
            crate::Token::Operator(op, span) => {
                // A subshell opens in command position, as Bash names it: `from `(' command`;
                // after `a=` it is an array's, `=(`.
                match op.as_str() {
                    "(" if command_position => open.push(("(", span.start.line)),
                    "(" if assignment => open.push(("=(", span.start.line)),
                    ")" => {
                        if let Some(index) =
                            open.iter().rposition(|(o, _)| matches!(*o, "(" | "=("))
                        {
                            open.truncate(index);
                        }
                    }
                    _ => {}
                }
                command_position = matches!(
                    op.as_str(),
                    "\n" | ";" | ";;" | ";&" | ";;&" | "&" | "&&" | "||" | "|" | "|&" | "(" | ")"
                );
            }
            crate::Token::Word(word, span) => {
                if !command_position {
                    continue;
                }
                let word = word.as_str();
                match word {
                    "if" | "case" | "while" | "until" | "for" | "select" | "{" => {
                        open.push((word, span.start.line));
                    }
                    "fi" | "esac" | "done" | "}" => {
                        let opener_matches = |opener: &str| match word {
                            "fi" => opener == "if",
                            "esac" => opener == "case",
                            "}" => opener == "{",
                            _ => matches!(opener, "while" | "until" | "for" | "select"),
                        };
                        if let Some(index) = open.iter().rposition(|(o, _)| opener_matches(o)) {
                            open.truncate(index);
                        }
                    }
                    _ => {}
                }
                // After these, the next word starts a command; after `for`, `case`
                // and `select` it is a name or subject, and anything else is an
                // ordinary command whose arguments follow.
                command_position = matches!(
                    word,
                    "if" | "then" | "else" | "elif" | "while" | "until" | "do" | "{" | "!"
                );
            }
        }
    }
    open.last()
        .map(|(keyword, line)| ((*keyword).to_owned(), *line))
}

#[cfg(test)]
mod tests {
    use super::ParseError;

    /// The token a syntax error in `text` names, and its line.
    fn near(text: &str) -> Option<(String, usize)> {
        let options = crate::parser::ParserOptions::default();
        match crate::parser::Parser::new(text.as_bytes(), &options).parse_program() {
            Err(ParseError::ParsingNear(position, token)) => Some((token, position.line)),
            _ => None,
        }
    }

    #[test]
    fn a_misplaced_reserved_word_is_named_as_bash_names_it() {
        // The peg parser fails at the end of the line; Bash at the word.
        assert_eq!(near("if then\n"), Some(("then".into(), 1)));
        assert_eq!(near("echo a\nfi\n"), Some(("fi".into(), 2)));
        assert_eq!(near("while true; do done\n"), Some(("done".into(), 1)));
        assert_eq!(near("if true; then fi\n"), Some(("fi".into(), 1)));
        assert_eq!(near("{ }\n"), Some(("}".into(), 1)));
    }

    #[test]
    fn other_tokens_are_named_where_the_parser_stopped() {
        assert_eq!(near("echo )\n"), Some((")".into(), 1)));
        // Reserved words that are arguments, or closed properly, are not misplaced.
        assert_eq!(near("echo fi done\n"), None);
        assert_eq!(
            near("for x in a; do echo; done\nif true; then :; fi\n"),
            None
        );
    }
}
