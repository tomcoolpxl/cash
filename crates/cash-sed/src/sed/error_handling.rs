// Parse delimited character sequences
//
// SPDX-License-Identifier: MIT
// Copyright (c) 2025 Diomidis Spinellis
//
// This file is part of the uutils sed package.
// It is licensed under the MIT License.
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use crate::sed::command::ProcessingContext;
use crate::sed::script_char_provider::ScriptCharProvider;
use crate::sed::script_line_provider::ScriptLineProvider;

use std::rc::Rc;

use uucore::display::Quotable;
use uucore::error::{UError, UResult, USimpleError};

#[derive(Clone, Debug)]
/// The location in a script where a command is defined
pub struct ScriptLocation {
    pub input_name: Rc<str>,  // Shared input name
    pub line_number: usize,   // 1-based line number
    pub column_number: usize, // 1-based column number
}

impl Default for ScriptLocation {
    fn default() -> Self {
        ScriptLocation {
            input_name: Rc::from("<unknown>"),
            line_number: 1,
            column_number: 1,
        }
    }
}

impl ScriptLocation {
    /// Construct with position information from the given providers.
    pub fn at_position(lines: &ScriptLineProvider, line: &ScriptCharProvider) -> Self {
        ScriptLocation {
            line_number: lines.get_line_number(),
            column_number: line.get_pos() + 1,
            input_name: Rc::from(lines.get_input_name()),
        }
    }
}

/// The compile error `msg` at the provider location, as a value for `map_err` and
/// `ok_or_else`. Its exit code is 1 (compilation phase).
///
/// The column is GNU sed's count of the characters it has read: an error found at a
/// character has read that character, and one found at the end of the line has read
/// the line, no more. The end of the line counted one more, so `s/a/b` was refused at
/// column 6 where GNU sed says char 5.
pub fn compilation_err(
    lines: &ScriptLineProvider,
    line: &ScriptCharProvider,
    msg: impl ToString,
) -> Box<dyn UError> {
    let column = if line.eol() {
        line.get_pos()
    } else {
        line.get_pos() + 1
    };
    compilation_err_at(lines, column, msg)
}

/// The compile error `msg` at `column` of the current script line, for an error GNU
/// sed reports after reading on: a regular expression's, once the command that holds
/// it is read. Its exit code is 1 (compilation phase).
pub fn compilation_err_at(
    lines: &ScriptLineProvider,
    column: usize,
    msg: impl ToString,
) -> Box<dyn UError> {
    USimpleError::new(
        1,
        format!(
            "{}:{}:{}: error: {}",
            lines.get_input_name(),
            lines.get_line_number(),
            column,
            msg.to_string()
        ),
    )
}

/// Fail with msg as a compile error at the provider location.
/// The error's exit code is 1 (compilation phase).
pub fn compilation_error<T>(
    lines: &ScriptLineProvider,
    line: &ScriptCharProvider,
    msg: impl ToString,
) -> UResult<T> {
    Err(compilation_err(lines, line, msg))
}

/// The error `msg` at the command's location, with the given exit code.
fn location_err(location: &ScriptLocation, msg: impl ToString, exit_code: i32) -> Box<dyn UError> {
    USimpleError::new(
        exit_code,
        format!(
            "{}:{}:{}: error: {}",
            location.input_name,
            location.line_number,
            location.column_number,
            msg.to_string()
        ),
    )
}

/// The compilation error `msg` at the command's location, as a value.
/// Its exit code is 1 (compilation phase).
pub fn semantic_err(location: &ScriptLocation, msg: impl ToString) -> Box<dyn UError> {
    location_err(location, msg, 1)
}

/// Fail with msg as a compilation error at the command's location.
/// The error's exit code is 1 (compilation phase).
pub fn semantic_error<T>(location: &ScriptLocation, msg: impl ToString) -> UResult<T> {
    Err(semantic_err(location, msg))
}

/// The runtime error `msg` at the command's location, as a value.
/// Its exit code is 2 (processing phase).
pub fn runtime_err(location: &ScriptLocation, msg: impl ToString) -> Box<dyn UError> {
    location_err(location, msg, 2)
}

/// Fail with msg as a runtime error at the command's location.
/// The error's exit code is 2 (processing phase).
pub fn runtime_error<T>(location: &ScriptLocation, msg: impl ToString) -> UResult<T> {
    Err(runtime_err(location, msg))
}

/// The runtime error `msg` at the command's and input's location, as a value.
/// Its exit code is 2 (processing phase).
pub fn input_runtime_err(
    location: &ScriptLocation,
    context: &ProcessingContext,
    msg: impl ToString,
) -> Box<dyn UError> {
    USimpleError::new(
        2,
        format!(
            "{}:{}:{}: {}:{} error: {}",
            location.input_name,
            location.line_number,
            location.column_number,
            context.input_name.quote(),
            context.line_number,
            msg.to_string()
        ),
    )
}

/// Fail with msg as a runtime error at the command's and input's location.
/// This is to be used in cases where the error depends on both, for example,
/// a fancy regular expression applied on invalid UTF-8 input.
/// (A fixed string match will not err in this case.)
/// The error's exit code is 2 (processing phase).
pub fn input_runtime_error<T>(
    location: &ScriptLocation,
    context: &ProcessingContext,
    msg: impl ToString,
) -> UResult<T> {
    Err(input_runtime_err(location, context, msg))
}
