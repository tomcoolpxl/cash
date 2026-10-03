//! Parser tests, with a snapshot of each parse.
//!
//! It also compared the PEG parser with a winnow one behind a feature, which was an
//! `unimplemented!()` stub (PI-13), so that comparison and the choice of parser are gone.

mod and_or_lists;
mod assignments;
mod complex;
mod compound_commands;
mod extended_test;
mod functions;
mod here_docs;
mod pipelines;
mod redirections;
mod simple_commands;

use crate::ast::Program;
use crate::error::ParseError;
use crate::parser::{Parser, ParserOptions};
use anyhow::Result;

/// Wrapper struct for serializing parse results with input context
#[derive(serde::Serialize)]
pub struct ParseResult<'a, T> {
    pub input: &'a str,
    pub result: &'a T,
}

/// Macro to assert snapshots with location information redacted.
/// This makes snapshots stable across parser changes that only affect source locations.
#[macro_export]
macro_rules! assert_snapshot_redacted {
    ($value:expr) => {{
        let mut settings = insta::Settings::clone_current();
        settings.add_redaction(".**.loc", "[location]");
        settings.bind(|| {
            insta::assert_ron_snapshot!($value);
        });
    }};
}

/// Parses `input` with the default options.
pub fn parse(input: &str) -> Result<Program, ParseError> {
    let options = ParserOptions::default();
    let mut parser = Parser::new(std::io::Cursor::new(input), &options);
    parser.parse_program()
}

/// Parses `input` for a snapshot test, saying what failed to parse if it did.
pub fn test_with_snapshot(input: &str) -> Result<Program> {
    parse(input).map_err(|e| anyhow::anyhow!("the parser failed: {e}\nInput: {input}"))
}

#[cfg(test)]
mod harness_tests {
    use super::*;

    #[test]
    fn a_simple_command_parses() {
        assert!(parse("echo hello").is_ok());
    }
}
