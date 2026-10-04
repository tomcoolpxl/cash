// Parse delimited character sequences
//
// SPDX-License-Identifier: MIT
// Copyright (c) 2025 Diomidis Spinellis
//
// This file is part of the uutils sed package.
// It is licensed under the MIT License.
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use crate::sed::script_char_provider::ScriptCharProvider;
use crate::sed::script_line_provider::ScriptLineProvider;

use std::rc::Rc;

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

/// A compile error, `msg` at `place`, as GNU sed words it: `sed: -e expression #1, char
/// 5: unterminated `s' command`, or `sed: file x.sed line 2: ...`. cash's sed wrote
/// `<script argument 1>:1:5: error:`. Its exit code is 1 (compilation phase).
fn placed_err(place: &str, msg: impl ToString) -> Box<dyn UError> {
    USimpleError::new(1, format!("{place}: {}", msg.to_string()))
}

/// The compile error `msg` at the provider location, as a value for `map_err` and
/// `ok_or_else`. Its exit code is 1 (compilation phase).
///
/// The place counts what GNU sed has read: an error found at a character has read that
/// character, and one found at the end of the line has read the line, no more. The end
/// of the line counted one more, so `s/a/b` was refused at column 6 where GNU sed says
/// char 5.
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
    compilation_err_at(lines, column, false, msg)
}

/// The compile error `msg` where GNU sed reports it after reading the newline that ends
/// the line, when the error is at the line's end: a command missing its delimiter
/// (`s`), its file name (`w`) or its second address (`1,`).
pub fn compilation_err_past_line(
    lines: &ScriptLineProvider,
    line: &ScriptCharProvider,
    msg: impl ToString,
) -> Box<dyn UError> {
    let (column, newline) = if line.eol() {
        (line.get_pos(), true)
    } else {
        (line.get_pos() + 1, false)
    };
    compilation_err_at(lines, column, newline, msg)
}

/// The compile error `msg` at `column` of the current script line, past the newline that
/// ends it with `newline`, for an error GNU sed reports after reading on: a regular
/// expression's, once the command that holds it is read. Its exit code is 1 (compilation
/// phase).
pub fn compilation_err_at(
    lines: &ScriptLineProvider,
    column: usize,
    newline: bool,
    msg: impl ToString,
) -> Box<dyn UError> {
    placed_err(&lines.gnu_place(column, newline), msg)
}

/// The compile error `msg` of a whole line, which GNU sed places at no character of it:
/// `-e expression #1, char 0`, or the file's line (an unmatched `{`).
pub fn compilation_err_of_line(place: &str, msg: impl ToString) -> Box<dyn UError> {
    placed_err(place, msg)
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

/// What went wrong with a file, in the C library's words, as GNU sed reports it: "No
/// such file or directory" where Windows says "The system cannot find the path
/// specified. (os error 3)".
pub fn strerror(error: &std::io::Error) -> String {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::NotFound => "No such file or directory".to_string(),
        ErrorKind::PermissionDenied => "Permission denied".to_string(),
        ErrorKind::AlreadyExists => "File exists".to_string(),
        ErrorKind::IsADirectory => "Is a directory".to_string(),
        ErrorKind::NotADirectory => "Not a directory".to_string(),
        ErrorKind::InvalidFilename | ErrorKind::InvalidInput => "Invalid argument".to_string(),
        ErrorKind::StorageFull => "No space left on device".to_string(),
        ErrorKind::BrokenPipe => "Broken pipe".to_string(),
        _ => {
            let text = error.to_string();
            match text.find(" (os error ") {
                Some(end) => text.get(..end).unwrap_or_default().to_string(),
                None => text,
            }
        }
    }
}

/// The run-time error `msg`, as GNU sed's `panic` words one: `sed: read error on x: Is a
/// directory`, with no place, and its status 4. cash's sed placed it at the command and
/// the input line (`<script argument 1>:1:8: 'file':1 error: ...`), with status 2.
pub fn runtime_err(msg: impl ToString) -> Box<dyn UError> {
    USimpleError::new(4, msg.to_string())
}

/// Fail with the run-time error `msg` (`runtime_err`).
pub fn runtime_error<T>(msg: impl ToString) -> UResult<T> {
    Err(runtime_err(msg))
}
