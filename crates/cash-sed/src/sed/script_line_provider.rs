//! Provide the script contents line by line
//
// SPDX-License-Identifier: MIT
// Copyright (c) 2025 Diomidis Spinellis
//
// This file is part of the uutils sed package.
// It is licensed under the MIT License.
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;

use crate::sed::error_handling::strerror;
use uucore::error::{UResult, USimpleError};

#[derive(Debug, PartialEq)]
/// The specification of a script: through a string or a file
pub enum ScriptValue {
    StringVal(String),
    PathVal(PathBuf),
}

#[derive(Debug)]
/// The provider of script lines across all specified scripts
/// Scripts can be specified to sed as files or as strings.
pub struct ScriptLineProvider {
    sources: Vec<ScriptValue>,
    state: State,
    /// Where the current line is, as GNU sed places a compile error
    place: ScriptPlace,
}

/// Where a script line is, as GNU sed places a compile error: the `-e` expression (a
/// script given as an argument is the first) and the line's offset in it, or the script
/// file and the line's number.
#[derive(Debug, Clone)]
struct ScriptPlace {
    /// The expression's number, 1 for the first `-e`; `None` for a file
    expression: Option<usize>,
    /// The file's name as given (`-` for standard input)
    file_name: String,
    line_number: usize,
    /// The bytes of the expression before the line
    line_offset: usize,
    /// Whether a newline ends the line
    has_newline: bool,
}

impl Default for ScriptPlace {
    /// The start of the first expression, before any line is read.
    fn default() -> Self {
        Self {
            expression: Some(1),
            file_name: String::new(),
            line_number: 0,
            line_offset: 0,
            has_newline: false,
        }
    }
}

/// Encapsulation of the script line provider's state
enum State {
    NotStarted, // Processing has not yet started
    Active {
        index: usize,
        reader: Box<dyn BufRead>, // Object on which read_line is called
        input_name: String,       // Input description (path or script string)
        line_number: usize,       // Current line number
        /// The expression's number, or `None` for a file (`ScriptPlace`)
        expression: Option<usize>,
        /// The file's name as GNU sed gives it
        file_name: String,
        /// The bytes read before the next line
        offset: usize,
    },
    /// All scripts have been processed. Where the last one ended stays, for an error
    /// found there: `sua\uxu` reported its unterminated `s` at `::0:8`, no script and
    /// line 0.
    Done {
        input_name: String,
        line_number: usize,
    },
}

impl ScriptLineProvider {
    /// Construct the script provider from the specified script sources
    pub fn new(sources: Vec<ScriptValue>) -> Self {
        Self {
            sources,
            state: State::NotStarted,
            place: ScriptPlace::default(),
        }
    }

    /// Where GNU sed places a compile error in the current line, having read `consumed`
    /// bytes of it, and its newline too when `newline` is set: `-e expression #2, char
    /// 7` (the bytes of the expression read), or `file x.sed line 3` (a newline read
    /// moves to the next line).
    pub fn gnu_place(&self, consumed: usize, newline: bool) -> String {
        let place = &self.place;
        let newline = usize::from(newline && place.has_newline);
        match place.expression {
            Some(number) => format!(
                "-e expression #{number}, char {}",
                place.line_offset + consumed + newline
            ),
            None => format!(
                "file {} line {}",
                place.file_name,
                place.line_number + newline
            ),
        }
    }

    /// Where GNU sed places an error about the current line as a whole: an unmatched
    /// `{` (`-e expression #1, char 0`, or the file's line).
    pub fn gnu_line_place(&self) -> String {
        match self.place.expression {
            Some(number) => format!("-e expression #{number}, char 0"),
            None => format!(
                "file {} line {}",
                self.place.file_name, self.place.line_number
            ),
        }
    }

    /// Whether a newline ends the current line, which GNU sed may read on an error at
    /// the line's end.
    pub fn line_has_newline(&self) -> bool {
        self.place.has_newline
    }

    /// Return the currently processed script line number.
    pub fn get_line_number(&self) -> usize {
        match &self.state {
            State::Active { line_number, .. } | State::Done { line_number, .. } => *line_number,
            State::NotStarted => 0,
        }
    }

    /// Return the currently processed script descriptive name.
    pub fn get_input_name(&self) -> &str {
        match &self.state {
            State::Active { input_name, .. } | State::Done { input_name, .. } => {
                input_name.as_str()
            }
            State::NotStarted => "",
        }
    }

    /// Return the next script line to process across all scripts.
    pub fn next_line(&mut self) -> UResult<Option<Vec<u8>>> {
        let mut line = Vec::new();

        loop {
            let advance = match &mut self.state {
                State::NotStarted => Some(0),
                State::Active {
                    index,
                    reader,
                    line_number,
                    expression,
                    file_name,
                    offset,
                    ..
                } => {
                    line.clear();
                    let bytes = reader.read_until(b'\n', &mut line)?;
                    if bytes == 0 {
                        Some(*index + 1) // finished reading this source
                    } else {
                        *line_number += 1;
                        let has_newline = line.ends_with(b"\n");
                        self.place = ScriptPlace {
                            expression: *expression,
                            file_name: file_name.clone(),
                            line_number: *line_number,
                            line_offset: *offset,
                            has_newline,
                        };
                        *offset += bytes;
                        // Remove trailing newline
                        if has_newline {
                            line.pop();
                        }
                        return Ok(Some(line));
                    }
                }
                State::Done { .. } => {
                    return Ok(None);
                }
            };

            if let Some(next_index) = advance {
                self.advance_source(next_index)?;
            }
        }
    }

    // Move to the next available script source.
    fn advance_source(&mut self, next_index: usize) -> UResult<()> {
        if next_index >= self.sources.len() {
            let (input_name, line_number) =
                match std::mem::replace(&mut self.state, State::NotStarted) {
                    State::Active {
                        input_name,
                        line_number,
                        ..
                    } => (input_name, line_number),
                    State::NotStarted | State::Done { .. } => (String::new(), 0),
                };
            self.state = State::Done {
                input_name,
                line_number,
            };
            return Ok(());
        }

        // GNU sed numbers the `-e` expressions alone, a script argument among them.
        let expression_number = self
            .sources
            .iter()
            .take(next_index + 1)
            .filter(|source| matches!(source, ScriptValue::StringVal(_)))
            .count();
        match &self.sources[next_index] {
            ScriptValue::StringVal(s) => {
                let cursor = std::io::Cursor::new(s.as_bytes().to_vec());
                self.state = State::Active {
                    index: next_index,
                    reader: Box::new(BufReader::new(cursor)),
                    input_name: format!("<script argument {}>", next_index + 1),
                    line_number: 0,
                    expression: Some(expression_number),
                    file_name: String::new(),
                    offset: 0,
                };
            }
            ScriptValue::PathVal(p) => {
                let file_name = p.to_string_lossy().to_string();
                if file_name == "-" {
                    self.state = State::Active {
                        index: next_index,
                        reader: Box::new(BufReader::new(io::stdin())),
                        input_name: "<stdin>".to_string(),
                        line_number: 0,
                        expression: None,
                        file_name,
                        offset: 0,
                    };
                } else {
                    // GNU sed's words and status 4; cash's sed said "error opening script
                    // file".
                    let file = File::open(p).map_err(|e| {
                        USimpleError::new(
                            4,
                            format!("couldn't open file {file_name}: {}", strerror(&e)),
                        )
                    })?;
                    self.state = State::Active {
                        index: next_index,
                        reader: Box::new(BufReader::new(file)),
                        input_name: file_name.clone(),
                        line_number: 0,
                        expression: None,
                        file_name,
                        offset: 0,
                    };
                }
            }
        }

        Ok(())
    }
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::NotStarted => f.debug_struct("NotStarted").finish(),
            State::Done {
                input_name,
                line_number,
            } => f
                .debug_struct("Done")
                .field("input_name", input_name)
                .field("line_number", line_number)
                .finish(),
            State::Active {
                index,
                input_name,
                line_number,
                ..
            } => f
                .debug_struct("Active")
                .field("index", index)
                .field("input_name", input_name)
                .field("line_number", line_number)
                .field("reader", &"<BufRead>")
                .finish(),
        }
    }
}

#[cfg(test)]
impl ScriptLineProvider {
    /// A provider in the middle of script file `input_name`, at line `line_number`.
    pub fn with_active_state(input_name: &str, line_number: usize) -> Self {
        Self {
            sources: vec![],
            state: State::Active {
                input_name: input_name.to_string(),
                line_number,
                index: 0,
                reader: Box::new(BufReader::new(io::stdin())),
                expression: None,
                file_name: input_name.to_string(),
                offset: 0,
            },
            place: ScriptPlace {
                expression: None,
                file_name: input_name.to_string(),
                line_number,
                ..ScriptPlace::default()
            },
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::panic,
    reason = "a failed assumption in a test should abort it loudly"
)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_string_source() {
        let input = vec![
            ScriptValue::StringVal("line one\nline two\n".to_string()),
            ScriptValue::StringVal("line three".to_string()),
        ];
        let mut provider = ScriptLineProvider::new(input);

        let mut lines = Vec::new();
        while let Some(line) = provider.next_line().unwrap() {
            lines.push(String::from_utf8(line).unwrap().trim_end().to_string());
        }

        assert_eq!(lines, vec!["line one", "line two", "line three"]);
    }

    #[test]
    fn test_file_source() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "file line 1").unwrap();
        writeln!(temp_file, "file line 2").unwrap();

        let input = vec![ScriptValue::PathVal(temp_file.path().to_path_buf())];
        let mut provider = ScriptLineProvider::new(input);

        let mut lines = Vec::new();
        while let Some(line) = provider.next_line().unwrap() {
            lines.push(String::from_utf8(line).unwrap().trim_end().to_string());
        }

        assert_eq!(lines, vec!["file line 1", "file line 2"]);
    }

    #[test]
    fn test_mixed_source() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "file line 1").unwrap();
        writeln!(temp_file, "file line 2").unwrap();
        let temp_file2 = NamedTempFile::new().unwrap();

        let input = vec![
            ScriptValue::PathVal(temp_file.path().to_path_buf()),
            ScriptValue::StringVal("script line 1".to_string()),
            ScriptValue::PathVal(temp_file.path().to_path_buf()),
            ScriptValue::StringVal(String::new()),
            ScriptValue::PathVal(temp_file2.path().to_path_buf()),
            ScriptValue::StringVal("other script line 1".to_string()),
        ];
        let mut provider = ScriptLineProvider::new(input);

        let mut lines = Vec::new();
        while let Some(line) = provider.next_line().unwrap() {
            lines.push(String::from_utf8(line).unwrap().trim_end().to_string());
        }

        assert_eq!(
            lines,
            vec![
                "file line 1",
                "file line 2",
                "script line 1",
                "file line 1",
                "file line 2",
                "other script line 1",
            ]
        );
    }

    #[test]
    fn test_getters() {
        let input = vec![
            ScriptValue::StringVal("l1\nl2\n".to_string()),
            ScriptValue::StringVal("l3".to_string()),
        ];
        let mut provider = ScriptLineProvider::new(input);

        if let Some(line) = provider.next_line().unwrap() {
            assert_eq!(String::from_utf8(line).unwrap().trim(), "l1");
            assert_eq!(provider.get_line_number(), 1);
            assert_eq!(provider.get_input_name(), "<script argument 1>");
        } else {
            panic!("Expected a line");
        }

        if let Some(line) = provider.next_line().unwrap() {
            assert_eq!(String::from_utf8(line).unwrap().trim(), "l2");
            assert_eq!(provider.get_line_number(), 2);
            assert_eq!(provider.get_input_name(), "<script argument 1>");
        } else {
            panic!("Expected a line");
        }

        if let Some(line) = provider.next_line().unwrap() {
            assert_eq!(String::from_utf8(line).unwrap().trim(), "l3");
            assert_eq!(provider.get_line_number(), 1);
            assert_eq!(provider.get_input_name(), "<script argument 2>");
        } else {
            panic!("Expected a line");
        }
    }

    #[test]
    fn test_file_source_preserves_invalid_utf8_bytes() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"s/\xC2\xE7/X/\n").unwrap();

        let input = vec![ScriptValue::PathVal(temp_file.path().to_path_buf())];
        let mut provider = ScriptLineProvider::new(input);

        assert_eq!(provider.next_line().unwrap().unwrap(), b"s/\xC2\xE7/X/");
    }
}
