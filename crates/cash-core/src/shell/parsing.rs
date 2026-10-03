//! Parsing for shell instances.

use std::io::Read;

use crate::{Shell, extensions, trace_categories};

/// What the lines read so far are: a complete program, the start of one, or wrong.
pub(crate) enum Prefix {
    /// A complete program, to run.
    Complete(cash_parser::ast::Program),
    /// The start of one: more lines may complete it. What it parses as now is what to
    /// run, or to report, if no more lines come.
    NeedsMore(Result<cash_parser::ast::Program, cash_parser::ParseError>),
    /// Wrong, whatever follows.
    Wrong(cash_parser::ParseError),
}

impl<SE: extensions::ShellExtensions> Shell<SE> {
    /// Parses the given reader as a shell program, returning the resulting Abstract Syntax Tree
    /// for the program.
    pub fn parse<R: Read>(
        &self,
        reader: R,
    ) -> Result<cash_parser::ast::Program, cash_parser::ParseError> {
        // cash (D7): script source arriving with CRLF endings parses as if it had LF.
        // Git for Windows checks out `\r\n` by default, so this is the common case, not
        // the exotic one.
        let reader = cash_win32::text::NormalizeCrlf::new(reader);

        let mut parser = create_parser(reader, &self.parser_options());

        tracing::debug!(target: trace_categories::PARSE, "Parsing reader as program...");
        parser.parse_program()
    }

    /// Parses the given string as a shell program, returning the resulting Abstract Syntax Tree
    /// for the program.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to parse as a program.
    pub fn parse_string<S: Into<String>>(
        &self,
        s: S,
    ) -> Result<cash_parser::ast::Program, cash_parser::ParseError> {
        let s: String = s.into();
        parse_string_impl(&s, &self.parser_options())
    }

    /// Whether `input` is an incomplete program: more lines must be read before it can
    /// run. An interactive shell asks this before it runs what was typed.
    ///
    /// # Arguments
    ///
    /// * `input` - The input accumulated so far.
    pub fn needs_more_input(&self, input: &str) -> bool {
        matches!(self.parse_prefix(input.as_bytes()), Prefix::NeedsMore(_))
    }

    /// What a parse of the lines read so far says about them.
    pub(crate) fn parse_prefix(&self, input: &[u8]) -> Prefix {
        match self.parse(input) {
            // Mid-token: unclosed quotes, unterminated here documents, and the like.
            Err(cash_parser::ParseError::Tokenizing { inner, position })
                if inner.is_incomplete() =>
            {
                Prefix::NeedsMore(Err(cash_parser::ParseError::Tokenizing { inner, position }))
            }
            // Ran out of tokens partway through a construct; more input may complete it.
            Err(
                err @ (cash_parser::ParseError::ParsingAtEndOfInput
                | cash_parser::ParseError::UnterminatedCompound { .. }
                | cash_parser::ParseError::UnterminatedArray { .. }),
            ) => Prefix::NeedsMore(Err(err)),
            // A bad token at a specific position stays bad no matter what follows it.
            Err(err) => Prefix::Wrong(err),
            // Parsed cleanly. One catch: a trailing backslash-newline is a line
            // continuation, which the tokenizer drops silently at end of input.
            Ok(program) if self.ends_with_line_continuation(input) => {
                Prefix::NeedsMore(Ok(program))
            }
            Ok(program) => Prefix::Complete(program),
        }
    }

    /// Whether `input` ends with a backslash-newline acting as a line continuation: asked
    /// again with the newline removed, the tokenizer reports an unterminated escape only if
    /// that backslash was really escaping something.
    fn ends_with_line_continuation(&self, input: &[u8]) -> bool {
        let Some(truncated) = input.strip_suffix(b"\n") else {
            return false;
        };
        let truncated = truncated.strip_suffix(b"\r").unwrap_or(truncated);
        // Keeps the extra parse off the common path.
        if !truncated.ends_with(b"\\") {
            return false;
        }
        matches!(
            self.parse(truncated),
            Err(cash_parser::ParseError::Tokenizing {
                inner: cash_parser::TokenizerError::UnterminatedEscapeSequence,
                position: _,
            })
        )
    }

    /// Returns the options that should be used for parsing shell programs; reflects
    /// the current configuration state of the shell and may change over time.
    pub const fn parser_options(&self) -> cash_parser::ParserOptions {
        cash_parser::ParserOptions {
            enable_extended_globbing: self.options.extended_globbing,
            posix_mode: self.options.posix_mode,
            sh_mode: self.options.sh_mode,
            tilde_expansion_at_word_start: true,
            tilde_expansion_after_colon: false,
            tilde_expansion_in_assignment_words: false,
            windows_drive_paths: self.options.windows_drive_paths,
        }
    }
}

#[cached::macros::cached(
    max_size = 64,
    key = "(String, cash_parser::ParserOptions)",
    convert = r#"{ (s.to_owned(), parser_options.to_owned()) }"#
)]
fn parse_string_impl(
    s: &str,
    parser_options: &cash_parser::ParserOptions,
) -> Result<cash_parser::ast::Program, cash_parser::ParseError> {
    // cash (D7): `-c`, `eval` and `source`d function bodies may all carry CRLF, picked
    // up from a file or from a Windows tool's output.
    let s = &*cash_win32::text::normalize_crlf(s);

    let mut parser = create_parser(s.as_bytes(), parser_options);

    tracing::debug!(target: trace_categories::PARSE, "Parsing string as program...");
    parser.parse_program()
}

pub(super) fn create_parser<R: Read>(
    r: R,
    parser_options: &cash_parser::ParserOptions,
) -> cash_parser::Parser<std::io::BufReader<R>> {
    let reader = std::io::BufReader::new(r);
    cash_parser::Parser::new(reader, parser_options)
}
