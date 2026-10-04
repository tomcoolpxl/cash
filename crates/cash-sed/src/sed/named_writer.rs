// An abstraction for output files created on entry and flushed on exit
//
// SPDX-License-Identifier: MIT
// Copyright (c) 2025 Diomidis Spinellis
//
// This file is part of the uutils sed package.
// It is licensed under the MIT License.
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use crate::sed::error_handling::strerror;
use crate::sed::fast_io::with_crlf;

use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use uucore::error::{UError, UResult, USimpleError};

thread_local! {
    /// Global list of all writers that should be flushed at shutdown
    static FLUSH_LIST: RefCell<Vec<Rc<RefCell<NamedWriter>>>> = const { RefCell::new(Vec::new()) };
}

/// Where a writer writes.
#[derive(Debug)]
enum Target {
    File(BufWriter<File>),
    /// GNU sed's special file `/dev/stdout`: sed's own output, which the commands that
    /// write to it write into in order (`processor`), but in an in-place edit, whose
    /// output is the file.
    Stdout,
    /// GNU sed's special file `/dev/stderr`.
    Stderr,
}

#[derive(Debug)]
/// Writer that tracks its file name for better error messages
pub struct NamedWriter {
    pub path: PathBuf,
    target: Target,
    /// Whether the last line written lacked its end, which the next line is written
    /// after, as GNU sed does
    missing_newline: bool,
}

impl NamedWriter {
    /// Create a new writer, truncate the file, and register it for flushing.
    ///
    /// As in GNU sed, the commands that name one file share one writer, so that `w o`
    /// twice writes both commands' lines (the second truncated the file and the two
    /// overwrote each other); outside POSIX mode `/dev/stdout` and `/dev/stderr` are
    /// sed's standard output and error; and `/dev/null` is Windows' `NUL`. They were
    /// files no one could open.
    pub fn new(path: PathBuf, posix: bool) -> UResult<Rc<RefCell<Self>>> {
        if let Some(writer) = FLUSH_LIST.with(|list| {
            list.borrow()
                .iter()
                .find(|writer| writer.borrow().path == path)
                .cloned()
        }) {
            return Ok(writer);
        }

        let target = match path.to_str() {
            Some("/dev/stdout") if !posix => Target::Stdout,
            Some("/dev/stderr") if !posix => Target::Stderr,
            name => {
                let open_path = if name == Some("/dev/null") {
                    Path::new("NUL")
                } else {
                    path.as_path()
                };
                let file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(open_path)
                    // GNU sed's words and status 4, without a place; cash's sed placed
                    // it at the command and wrote "creating file".
                    .map_err(|e| {
                        USimpleError::new(
                            4,
                            format!("couldn't open file {}: {}", path.display(), strerror(&e)),
                        )
                    })?;
                Target::File(BufWriter::new(file))
            }
        };

        let writer = Rc::new(RefCell::new(Self {
            path,
            target,
            missing_newline: false,
        }));
        FLUSH_LIST.with(|list| list.borrow_mut().push(Rc::clone(&writer)));
        Ok(writer)
    }

    /// Whether the writer is GNU sed's `/dev/stdout`, sed's own output.
    pub fn is_stdout(&self) -> bool {
        matches!(self.target, Target::Stdout)
    }

    /// The writer's name in an error, as GNU sed gives it.
    fn name(&self) -> String {
        match self.target {
            Target::File(_) => self.path.display().to_string(),
            Target::Stdout => "stdout".to_string(),
            Target::Stderr => "stderr".to_string(),
        }
    }

    /// GNU sed's error for `items` bytes it could not write, with its status 4. Standard
    /// output's reader having gone ends sed in silence with 141, as SIGPIPE ends GNU sed
    /// (spec D71). It said `writing to file 'f': ...` with status 2.
    fn write_failed(&self, error: &io::Error, items: usize) -> Box<dyn UError> {
        if error.kind() == io::ErrorKind::BrokenPipe && self.is_stdout() {
            std::process::exit(141);
        }
        let plural = if items == 1 { "item" } else { "items" };
        USimpleError::new(
            4,
            format!(
                "couldn't write {items} {plural} to {}: {}",
                self.name(),
                strerror(error)
            ),
        )
    }

    /// Write `bytes`, failing as GNU sed does.
    fn put(&mut self, bytes: &[u8]) -> UResult<()> {
        let result = match &mut self.target {
            Target::File(writer) => writer.write_all(bytes),
            Target::Stdout => io::stdout().write_all(bytes),
            Target::Stderr => io::stderr().write_all(bytes),
        };
        result.map_err(|e| self.write_failed(&e, bytes.len()))
    }

    /// The bytes that write `line`, as GNU sed writes one: after the end the last line
    /// lacked, and ended by `delimiter` (a newline, or NUL with `-z`) when `newline`, a
    /// CRLF for a line that had one (`crlf`), as sed's output keeps it (D49). The end
    /// was always a newline, a line that lacked it was joined to the next one (`-s` and
    /// two files whose last lines have none), and a CRLF line lost its CR.
    pub fn line_bytes(&mut self, line: &[u8], newline: bool, delimiter: u8, crlf: bool) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(line.len() + 2);
        if self.missing_newline {
            bytes.push(delimiter);
        }
        if crlf && delimiter == b'\n' {
            bytes.extend_from_slice(&with_crlf(line));
        } else {
            bytes.extend_from_slice(line);
        }
        if newline {
            if crlf && delimiter == b'\n' {
                bytes.push(b'\r');
            }
            bytes.push(delimiter);
        }
        self.missing_newline = !newline;
        bytes
    }

    /// Write `bytes` (`line_bytes`) to the file, returning errors.
    pub fn write_bytes(&mut self, bytes: &[u8]) -> UResult<()> {
        self.put(bytes)
    }

    /// Write `line` (`line_bytes`) to the file, returning errors.
    pub fn write_line_bytes(&mut self, line: &[u8], newline: bool, delimiter: u8) -> UResult<()> {
        let bytes = self.line_bytes(line, newline, delimiter, false);
        self.put(&bytes)
    }

    /// Flush the writer, failing as GNU sed does: `couldn't flush F: ...`, status 4.
    pub fn flush(&mut self) -> UResult<()> {
        let result = match &mut self.target {
            Target::File(writer) => writer.flush(),
            Target::Stdout => io::stdout().flush(),
            Target::Stderr => io::stderr().flush(),
        };
        result.map_err(|e| {
            if e.kind() == io::ErrorKind::BrokenPipe && self.is_stdout() {
                std::process::exit(141);
            }
            USimpleError::new(
                4,
                format!("couldn't flush {}: {}", self.name(), strerror(&e)),
            )
        })
    }
}

/// Flush buffered content to the file, returning descriptive errors.
pub fn flush_all() -> UResult<()> {
    FLUSH_LIST.with(|cell| {
        for handle in cell.borrow().iter() {
            handle.borrow_mut().flush()?;
        }

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn test_write_line_bytes_appends_newline() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        let writer = NamedWriter::new(path.clone(), false).unwrap();

        writer
            .borrow_mut()
            .write_line_bytes(b"a\xE9", true, b'\n')
            .unwrap();
        writer.borrow_mut().flush().unwrap();

        assert_eq!(fs::read(path).unwrap(), b"a\xE9\n");
    }

    #[test]
    fn test_write_line_bytes_appends_no_newline() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        let writer = NamedWriter::new(path.clone(), false).unwrap();

        writer
            .borrow_mut()
            .write_line_bytes(b"a\xE9", false, b'\n')
            .unwrap();
        writer.borrow_mut().flush().unwrap();

        assert_eq!(fs::read(path).unwrap(), b"a\xE9");
    }

    // A line that lacked its end has it written before the next line, as in GNU sed, and
    // `-z` ends lines with NUL.
    #[test]
    fn test_line_ends() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        let writer = NamedWriter::new(path.clone(), false).unwrap();
        let mut writer = writer.borrow_mut();
        writer.write_line_bytes(b"a", false, b'\n').unwrap();
        writer.write_line_bytes(b"b", true, b'\n').unwrap();
        writer.write_line_bytes(b"c", true, 0).unwrap();
        writer.flush().unwrap();

        assert_eq!(fs::read(path).unwrap(), b"a\nb\nc\0");
    }

    // The commands that name one file share its writer, as in GNU sed.
    #[test]
    fn test_one_writer_per_file_name() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        let first = NamedWriter::new(path.clone(), false).unwrap();
        let second = NamedWriter::new(path, false).unwrap();
        assert!(Rc::ptr_eq(&first, &second));
    }

    // Outside POSIX mode `/dev/stdout` is GNU sed's special file; in it, a file name.
    #[test]
    fn test_dev_stdout_is_special_outside_posix_mode() {
        let writer = NamedWriter::new(PathBuf::from("/dev/stdout"), false).unwrap();
        assert!(writer.borrow().is_stdout());
        assert!(NamedWriter::new(PathBuf::from("/dev/stdout/x"), true).is_err());
    }
}
