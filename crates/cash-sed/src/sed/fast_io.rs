// Line-based I/O
//
// Abstractions that allow file lines to be read and output through
// buffered readers and writers.
// Search for "main" to see a usage example.
//
// Cash builds for Windows only and removed upstream's Unix-only zero-copy
// path (mmap(2) input, write(2) and copy_file_range(2) output).
//
// SPDX-License-Identifier: MIT
// Copyright (c) 2025 Diomidis Spinellis
//
// This file is part of the uutils sed package.
// It is licensed under the MIT License.
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Seek, Write};
use std::rc::Rc;

use std::str;

use std::path::{Path, PathBuf};
use uucore::error::{UError, UResult, USimpleError};

use crate::sed::error_handling::strerror;

/// Buffered line reader from any BufRead input.
pub struct ReadLineCursor {
    reader: Box<dyn BufRead>,
    buffer: Vec<u8>,
    /// Whether a CR before the LF is split off and remembered (the default), rather than
    /// kept as part of the line.
    strip_cr: bool,
    /// The byte that ends a line (`ProcessingContext::delimiter`).
    delimiter: u8,
}

impl ReadLineCursor {
    /// Construct from anything that implements `Read`.
    fn new<R: Read + 'static>(r: R) -> Self {
        let buf = BufReader::new(r);
        Self {
            reader: Box::new(buf),
            buffer: Vec::new(),
            strip_cr: true,
            delimiter: b'\n',
        }
    }

    /// If a line is available, return it and its \n termination.
    fn get_line(&mut self) -> io::Result<Option<(Vec<u8>, bool, bool)>> {
        self.buffer.clear();
        // read_until *includes* the delimiter if present
        let bytes_read = self.reader.read_until(self.delimiter, &mut self.buffer)?;
        if bytes_read == 0 {
            return Ok(None);
        }
        // O(1) check whether it ended in the delimiter
        let has_newline = self.buffer.last() == Some(&self.delimiter);
        // strip it if you don’t want to expose it to the caller
        if has_newline {
            self.buffer.pop();
        }
        // A CR belongs to a CRLF line ending only where lines end in LF.
        let has_crlf =
            self.strip_cr && self.delimiter == b'\n' && has_newline && self.buffer.ends_with(b"\r");
        if has_crlf {
            self.buffer.pop();
        }
        let line = std::mem::take(&mut self.buffer);
        Ok(Some((line, has_newline, has_crlf)))
    }

    /// Return true if the previously returned line was the last one.
    fn last_line(&mut self) -> io::Result<bool> {
        // FIXME(rust-lang#86423): Replace with BufRead::has_data_left()
        // when/if method becomes stable.
        Ok(self.reader.fill_buf()?.is_empty())
    }
}

/// A chunk of data that is input and can be output, often very efficiently
#[derive(Debug, PartialEq, Eq)]
pub struct IOChunk {
    utf8_verified: Cell<bool>, // True if the contents are valid UTF-8
    content: IOChunkContent,
}

impl IOChunk {
    /// Construct an IOChunk from the given content
    fn from_content(content: IOChunkContent) -> Self {
        Self {
            utf8_verified: Cell::new(false),
            content,
        }
    }

    /// Clear the object's contents, converting it into Owned if needed.
    pub fn clear(&mut self) {
        self.utf8_verified.set(true);
        match &mut self.content {
            IOChunkContent::Owned {
                content,
                has_newline,
                ..
            } => {
                content.clear();
                *has_newline = false;
            }
        }
    }

    /// Return true if the content is empty.
    pub fn is_empty(&self) -> bool {
        self.content.len() == 0
    }

    /// Return true if the content ends with a newline.
    pub fn is_newline_terminated(&self) -> bool {
        match &self.content {
            IOChunkContent::Owned { has_newline, .. } => *has_newline,
        }
    }

    #[cfg(test)]
    /// Create an Owned newline-terminated IOChunk from a string.
    pub fn new_from_str(s: &str) -> Self {
        IOChunk {
            content: IOChunkContent::new_owned(s.as_bytes().to_vec(), true),
            utf8_verified: Cell::new(false),
        }
    }

    /// Set the object's contents to the specified string.
    /// Convert it into Owned if needed.
    pub fn set_to_string(&mut self, new_content: String, add_newline: bool) {
        self.set_to_bytes(new_content.into_bytes(), add_newline);
        self.utf8_verified.set(true);
    }

    /// Set the object's contents to the specified bytes.
    /// Convert it into Owned if needed.
    pub fn set_to_bytes(&mut self, new_content: Vec<u8>, add_newline: bool) {
        self.utf8_verified.set(false);
        match &mut self.content {
            IOChunkContent::Owned {
                content,
                has_newline,
                ..
            } => {
                *content = new_content;
                *has_newline = add_newline;
            }
        }
    }

    /// Return the content as a str.
    pub fn as_str(&self) -> Result<&str, Box<dyn UError>> {
        match &self.content {
            IOChunkContent::Owned { content, .. } => {
                str::from_utf8(content).map_err(|e| USimpleError::new(2, e.to_string()))
            }
        }
    }

    /// Return true if the content ends with a CRLF, which sed's output keeps (D49).
    pub fn is_crlf_terminated(&self) -> bool {
        match &self.content {
            IOChunkContent::Owned {
                has_newline,
                has_crlf,
                ..
            } => *has_newline && *has_crlf,
        }
    }

    /// Return the raw byte content (always safe).
    pub fn as_bytes(&self) -> &[u8] {
        match &self.content {
            IOChunkContent::Owned { content, .. } => content,
        }
    }

    /// Convert content to the Owned variant if it's not already.
    pub fn ensure_owned(&mut self) -> Result<(), Box<dyn UError>> {
        match &self.content {
            IOChunkContent::Owned { .. } => Ok(()), // already owned
        }
    }

    /// Return mutable access to the content and has_newline fields.
    pub fn fields_mut(&mut self) -> Result<(&mut Vec<u8>, &mut bool), Box<dyn UError>> {
        self.ensure_owned()?;

        match &mut self.content {
            IOChunkContent::Owned {
                content,
                has_newline,
                ..
            } => Ok((content, has_newline)),
        }
    }
}

/// Data read from input or to be written to a file.
#[derive(Debug, PartialEq, Eq)]
enum IOChunkContent {
    Owned {
        content: Vec<u8>,  // Line content without newline
        has_newline: bool, // True if \n-terminated
        has_crlf: bool,    // True if \r\n-terminated
    },
}

impl IOChunkContent {
    /// Construct a new Owned chunk.
    pub fn new_owned(content: Vec<u8>, has_newline: bool) -> Self {
        Self::new_owned_with_crlf(content, has_newline, false)
    }

    /// Construct a new Owned chunk with explicit CRLF status.
    pub fn new_owned_with_crlf(content: Vec<u8>, has_newline: bool, has_crlf: bool) -> Self {
        IOChunkContent::Owned {
            content,
            has_newline,
            has_crlf,
        }
    }

    /// Return the content's length (in bytes or characters).
    pub fn len(&self) -> usize {
        match self {
            IOChunkContent::Owned { content, .. } => content.len(),
        }
    }
}

/// Line reader over buffered input.
pub enum LineReader {
    ReadInput(ReadLineCursor),
}

/// Return a LineReader that uses the ReadInput method fot the specified file.
fn line_reader_read_input(file: File) -> io::Result<LineReader> {
    let boxed: Box<dyn Read> = Box::new(file);
    let reader = BufReader::new(boxed);
    Ok(LineReader::ReadInput(ReadLineCursor::new(reader)))
}

impl LineReader {
    /// Open the specified file for line input, keeping carriage returns as data when
    /// `cr_is_data` is set (see `ProcessingContext::treats_cr_as_data`).
    pub fn open_with(path: &PathBuf, cr_is_data: bool) -> io::Result<Self> {
        let mut reader = Self::open(path)?;
        let LineReader::ReadInput(cursor) = &mut reader;
        cursor.strip_cr = !cr_is_data;
        Ok(reader)
    }

    /// Open the specified file for line input.
    // Use "-" to read from the standard input.
    pub fn open(path: &PathBuf) -> io::Result<Self> {
        if path.as_os_str() == "-" {
            let stdin = io::stdin();
            let boxed: Box<dyn Read> = Box::new(stdin.lock());
            let reader = BufReader::new(boxed);
            return Ok(LineReader::ReadInput(ReadLineCursor::new(reader)));
        }

        let file = File::open(path)?;

        line_reader_read_input(file)
    }

    /// Open the specified file to read as a stream.
    #[cfg(test)]
    pub fn open_stream(path: &PathBuf) -> io::Result<Self> {
        let file = File::open(path)?;
        line_reader_read_input(file)
    }

    /// Return the next line, if available.
    pub fn get_line(&mut self) -> io::Result<Option<IOChunk>> {
        match self {
            LineReader::ReadInput(cursor) => {
                if let Some((line, has_newline, has_crlf)) = cursor.get_line()? {
                    let chunk = IOChunk::from_content(IOChunkContent::new_owned_with_crlf(
                        line,
                        has_newline,
                        has_crlf,
                    ));
                    Ok(Some(chunk))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Return true if the previously returned line was the last one.
    pub fn last_line(&mut self) -> io::Result<bool> {
        match self {
            LineReader::ReadInput(cursor) => cursor.last_line(),
        }
    }

    /// Set the byte that ends a line (`ProcessingContext::delimiter`).
    pub fn set_delimiter(&mut self, delimiter: u8) {
        match self {
            LineReader::ReadInput(cursor) => cursor.delimiter = delimiter,
        }
    }
}

/// Whether `path` names a Windows named pipe (`\\.\pipe\name`), such as cash's process
/// substitution gives. Asking for such a file's kind opens it, as a reader of the pipe,
/// and the one reader its writer waits for is then gone: so it is opened only to be read.
pub fn is_pipe_path(path: &Path) -> bool {
    let name = path.as_os_str().to_string_lossy();
    let prefix = name.get(..9).unwrap_or_default();
    prefix.eq_ignore_ascii_case(r"\\.\pipe\") || prefix.eq_ignore_ascii_case("//./pipe/")
}

/// Whether `path` is a directory, which a named pipe is not (`is_pipe_path`).
pub fn is_directory(path: &Path) -> bool {
    !is_pipe_path(path) && path.is_dir()
}

/// Input files `$` has opened and read ahead into before their turn, by their place in
/// the list of input files, kept for it: a pipe read again would have lost what was
/// read (`processor::no_later_input`).
#[derive(Clone, Default)]
pub struct ReadAhead(Rc<RefCell<HashMap<usize, LineReader>>>);

impl ReadAhead {
    /// Keep `reader`, the input file at `index`, for its turn.
    pub fn keep(&self, index: usize, reader: LineReader) {
        self.0.borrow_mut().insert(index, reader);
    }

    /// The input file at `index`, if it has been opened ahead of its turn.
    pub fn take(&self, index: usize) -> Option<LineReader> {
        self.0.borrow_mut().remove(&index)
    }

    /// Whether the input file at `index` has been opened ahead, and if so whether it has
    /// input.
    pub fn has_input(&self, index: usize) -> Option<bool> {
        self.0
            .borrow_mut()
            .get_mut(&index)
            .map(|reader| reader.last_line().map_or(true, |at_end| !at_end))
    }
}

impl std::fmt::Debug for ReadAhead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.0.borrow().keys()).finish()
    }
}

/// GNU sed's special file for standard input, which `r` and `R` read. Windows has no
/// such file, so they read nothing from it.
pub const DEV_STDIN: &str = "/dev/stdin";

/// Standard input through a handle of its own, as reading `/dev/stdin` gives it in GNU
/// sed: apart from any buffering of standard input as sed's input. `None` when there is
/// no standard input.
pub fn stdin_file() -> Option<File> {
    use std::os::windows::io::AsHandle;
    io::stdin()
        .as_handle()
        .try_clone_to_owned()
        .ok()
        .map(File::from)
}

/// What `r /dev/stdin` reads, as opening `/dev/stdin` anew reads it on GNU sed's systems:
/// the rest of a pipe, and the whole of a file, whatever has been read of it.
pub fn read_dev_stdin() -> io::Result<Vec<u8>> {
    let mut contents = Vec::new();
    let Some(mut file) = stdin_file() else {
        return Ok(contents);
    };
    if file.metadata().is_ok_and(|metadata| metadata.is_file()) {
        // The handle shares its place with standard input's, which is left where it was.
        let place = file.stream_position()?;
        file.seek(io::SeekFrom::Start(0))?;
        file.read_to_end(&mut contents)?;
        file.seek(io::SeekFrom::Start(place))?;
    } else {
        file.read_to_end(&mut contents)?;
    }
    Ok(contents)
}

pub trait OutputWrite: Write {}
impl<T: Write> OutputWrite for T {}

/// Abstraction for outputting data.
/// All output is buffered and written via BufWriter.
///
/// A failed write is GNU sed's error, with its status 4: `couldn't write 5 items to
/// stdout: No space left on device`, or `couldn't flush stdout: ...`. Standard output's
/// reader having gone ends sed in silence with 141, as SIGPIPE ends GNU sed and every
/// tool of cash's own (spec D71). They were the system's words, with status 1.
pub struct OutputBuffer {
    out: BufWriter<Box<dyn OutputWrite + 'static>>, // Where to write
    // True when the last write didn't end with \n; the \n is deferred so
    // that commands like `p` don't emit a spurious newline under -n.
    pending_newline: bool,
    pending_crlf: bool,
    /// The byte that ends a line (`ProcessingContext::delimiter`).
    delimiter: u8,
    /// The output's name in an error: `stdout`, or the file an in-place edit writes.
    name: String,
}

impl OutputBuffer {
    pub fn new(w: Box<dyn OutputWrite + 'static>) -> Self {
        Self {
            out: BufWriter::new(w),
            pending_newline: false,
            pending_crlf: false,
            delimiter: b'\n',
            name: "stdout".to_string(),
        }
    }

    /// The buffer, named `name` in its errors.
    #[must_use]
    pub fn with_name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    /// Set the byte that ends a line (`ProcessingContext::delimiter`).
    pub fn set_delimiter(&mut self, delimiter: u8) {
        self.delimiter = delimiter;
    }

    /// GNU sed's error for `items` bytes it could not write.
    fn write_failed(&self, error: &io::Error, items: usize) -> Box<dyn UError> {
        if error.kind() == io::ErrorKind::BrokenPipe && self.name == "stdout" {
            std::process::exit(141);
        }
        let plural = if items == 1 { "item" } else { "items" };
        USimpleError::new(
            4,
            format!(
                "couldn't write {items} {plural} to {}: {}",
                self.name,
                strerror(error)
            ),
        )
    }

    /// Write `bytes` as they are.
    fn put(&mut self, bytes: &[u8]) -> UResult<()> {
        self.out
            .write_all(bytes)
            .map_err(|e| self.write_failed(&e, bytes.len()))
    }

    /// Write the end of a line: CRLF for a CRLF line (D49), else the delimiter.
    fn write_line_end(&mut self, crlf: bool) -> UResult<()> {
        if crlf && self.delimiter == b'\n' {
            self.put(b"\r\n")
        } else {
            let delimiter = [self.delimiter];
            self.put(&delimiter)
        }
    }

    /// Schedule the specified String or &str for eventual output
    pub fn write_str<S: Into<String>>(&mut self, s: S) -> UResult<()> {
        let mut s = s.into();
        let has_newline = s.ends_with('\n');
        if has_newline {
            s.truncate(s.len() - 1);
        }
        self.write_chunk(&IOChunk::from_content(IOChunkContent::new_owned(
            s.into_bytes(),
            has_newline,
        )))
    }

    /// Schedule the specified bytes for eventual output.
    pub fn write_bytes(&mut self, bytes: &[u8]) -> UResult<()> {
        let (content, has_newline) = if bytes.ends_with(b"\n") {
            (&bytes[..bytes.len() - 1], true)
        } else {
            (bytes, false)
        };
        self.write_chunk(&IOChunk::from_content(IOChunkContent::new_owned(
            content.to_vec(),
            has_newline,
        )))
    }

    /// Write `bytes` as they are, leaving a line end the last output left out to come
    /// before the next output: what GNU sed's `/dev/stdout` writes into sed's output.
    pub fn write_apart(&mut self, bytes: &[u8]) -> UResult<()> {
        self.put(bytes)
    }

    /// Write `bytes` as they are, after the end of a line the last output left out: a
    /// line `R` read, which GNU sed writes so, without an end when the file's last line
    /// has none.
    pub fn write_raw(&mut self, bytes: &[u8]) -> UResult<()> {
        self.flush_pending_newline()?;
        self.put(bytes)
    }

    /// Copy the specified file to the output.
    ///
    /// As in GNU sed, a file that cannot be opened is no error, and one that cannot be
    /// read, a directory, is "read error on F: Is a directory" with status 4; nothing was
    /// written for a directory.
    pub fn copy_file(&mut self, path: &PathBuf) -> UResult<()> {
        if is_directory(path) {
            return Err(USimpleError::new(
                4,
                format!("read error on {}: Is a directory", path.display()),
            ));
        }
        let Ok(file) = File::open(path) else {
            // Per POSIX, if the file can't be read treat it as empty.
            return Ok(());
        };

        // A line the last output left without its end gets it first, as in GNU sed: the
        // file's text was joined to it (`printf a | sed 'r f'`).
        self.flush_pending_newline()?;
        let mut reader = BufReader::new(file);
        loop {
            let buffer = reader.fill_buf().map_err(|e| {
                USimpleError::new(
                    4,
                    format!("read error on {}: {}", path.display(), strerror(&e)),
                )
            })?;
            if buffer.is_empty() {
                return Ok(());
            }
            let len = buffer.len();
            self.put(buffer)?;
            reader.consume(len);
        }
    }
}

/// Implementation of the std::io::Write trait
impl Write for OutputBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_bytes(buf)
            .map_err(|e| io::Error::other(e.to_string()))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush().map_err(|e| io::Error::other(e.to_string()))
    }
}

impl OutputBuffer {
    /// Schedule the specified output chunk for eventual output
    pub fn write_chunk(&mut self, chunk: &IOChunk) -> UResult<()> {
        if chunk.is_empty() && !chunk.is_newline_terminated() {
            return Ok(());
        }

        if self.pending_newline {
            self.write_line_end(self.pending_crlf)?;
            self.pending_newline = false;
            self.pending_crlf = false;
        }

        match &chunk.content {
            IOChunkContent::Owned {
                content,
                has_newline,
                has_crlf,
                ..
            } => {
                self.put(content)?;
                if *has_newline {
                    self.write_line_end(*has_crlf)?;
                }
                self.pending_newline = !has_newline;
                self.pending_crlf = *has_crlf;
                Ok(())
            }
        }
    }

    /// Write a deferred newline if the last output didn't end with one.
    pub fn flush_pending_newline(&mut self) -> UResult<()> {
        if self.pending_newline {
            self.write_line_end(self.pending_crlf)?;
            self.pending_newline = false;
            self.pending_crlf = false;
        }
        Ok(())
    }

    /// Flush the buffered data.
    pub fn flush(&mut self) -> UResult<()> {
        let Err(error) = self.out.flush() else {
            return Ok(());
        };
        // A write the buffer put off failed: it is a failed write to GNU sed, which
        // flushes as it writes.
        if error.kind() == io::ErrorKind::BrokenPipe && self.name == "stdout" {
            std::process::exit(141);
        }
        Err(USimpleError::new(
            4,
            format!("couldn't flush {}: {}", self.name, strerror(&error)),
        ))
    }
}
// Usage example (never compiled)
#[cfg(any())]
pub fn main() -> io::Result<()> {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "-".into());
    let mut reader = LineReader::open(&path)?;
    let stdout = Box::new(io::stdout().lock());
    let mut output = OutputBuffer::new(stdout);

    while let Some(chunk) = reader.get_line()? {
        output.write_chunk(&chunk)?;
    }

    output.flush()
}

#[cfg(test)]
#[expect(
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::unwrap_in_result,
    reason = "a failed assumption in a test should abort it loudly"
)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{Seek, SeekFrom};
    use tempfile::NamedTempFile;
    use tempfile::tempfile;

    #[test]
    fn test_owned_line_output() -> io::Result<()> {
        let tmp = NamedTempFile::new()?;
        {
            let file = tmp.reopen()?;
            let mut out = OutputBuffer::new(Box::new(file));
            out.write_str("foo\n").unwrap();
            out.write_str("bar\n").unwrap();
            out.flush().unwrap();
        } // File closes here as it leaves the scope

        let contents = fs::read(tmp.path())?;
        assert_eq!(contents.as_slice(), b"foo\nbar\n");
        Ok(())
    }

    #[test]
    fn test_small_file_unterminated() -> io::Result<()> {
        // Create and fill the input temp file:
        let mut input = NamedTempFile::new()?;
        write!(input, "first line\nsecond line\nlast line (unterminated)")?;
        input.flush()?;
        let input_path = input.path().to_path_buf();

        // Open reader on input file:
        let mut reader = LineReader::open(&input_path)?;

        // Create the output temp file (empty):
        let output = NamedTempFile::new()?;
        let output_path = output.path().to_path_buf();
        let out_file = File::create(&output_path)?;

        // Wrap it in your OutputBuffer and run the loop:
        let mut out = OutputBuffer::new(Box::new(out_file));
        let mut nline = 0;
        while let Some(chunk) = reader.get_line()? {
            out.write_chunk(&chunk).unwrap();
            nline += 1;
        }
        assert_eq!(nline, 3);

        out.flush().unwrap();

        // Verify that files match:
        let expected = fs::read(&input_path)?;
        let actual = fs::read(&output_path)?;
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn test_small_file_unterminated_stream() -> io::Result<()> {
        // Create and fill the input temp file:
        let mut input = NamedTempFile::new()?;
        write!(input, "first line\nsecond line\nlast line (unterminated)")?;
        input.flush()?;
        let input_path = input.path().to_path_buf();

        // Open reader on input file:
        let mut reader = LineReader::open_stream(&input_path)?;

        // Create the output temp file (empty):
        let output = NamedTempFile::new()?;
        let output_path = output.path().to_path_buf();
        let out_file = File::create(&output_path)?;

        // Wrap it in your OutputBuffer and run the loop:
        let mut out = OutputBuffer::new(Box::new(out_file));
        let mut nline = 0;
        while let Some(chunk) = reader.get_line()? {
            out.write_chunk(&chunk).unwrap();
            nline += 1;
        }
        assert_eq!(nline, 3);

        out.flush().unwrap();

        // Verify that files match:
        let expected = fs::read(&input_path)?;
        let actual = fs::read(&output_path)?;
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn test_stream_read() -> std::io::Result<()> {
        // Create temporary file with known contents
        let mut tmp = NamedTempFile::new()?;
        write!(tmp, "first line\nsecond line\nlast line\n")?;
        tmp.flush()?;

        let path = tmp.path().to_path_buf();
        let mut reader = LineReader::open_stream(&path)?;

        // Verify the reader's operation
        if let Some(IOChunk {
            content:
                IOChunkContent::Owned {
                    content,
                    has_newline,
                    ..
                },
            utf8_verified,
            ..
        }) = reader.get_line()?
        {
            assert_eq!(content, b"first line");
            assert_eq!(content.len(), 10);
            assert!(has_newline);
            assert!(!utf8_verified.get());
            assert!(!reader.last_line().unwrap());
        } else {
            panic!("Expected IOChunkContent::Owned");
        }

        if let Some(IOChunk {
            content:
                IOChunkContent::Owned {
                    content,
                    has_newline,
                    ..
                },
            ..
        }) = reader.get_line()?
        {
            assert_eq!(content, b"second line");
            assert!(has_newline);
            assert!(!reader.last_line().unwrap());
        } else {
            panic!("Expected IOChunkContent::Owned");
        }

        if let Some(content) = reader.get_line()? {
            assert_eq!(content.as_str().unwrap(), "last line");
            assert!(reader.last_line().unwrap());
        } else {
            panic!("Expected IOChunk");
        }

        assert_eq!(reader.get_line()?, None);

        Ok(())
    }

    // is_newline_terminated, is_empty
    #[test]
    fn test_owned_newline_terminated_non_empty() {
        let chunk = IOChunk::from_content(IOChunkContent::new_owned(b"line".to_vec(), true));
        assert!(chunk.is_newline_terminated());
        assert!(!chunk.is_empty());
    }

    #[test]
    fn test_owned_newline_terminated_empty() {
        let chunk = IOChunk::from_content(IOChunkContent::new_owned(Vec::new(), true));
        assert!(chunk.is_newline_terminated());
        assert!(chunk.is_empty());
    }

    #[test]
    fn test_owned_not_newline_terminated() {
        let chunk = IOChunk::from_content(IOChunkContent::new_owned(b"line".to_vec(), false));
        assert!(!chunk.is_newline_terminated());
    }

    // ensure_owned()
    #[test]
    fn test_ensure_owned_on_owned() {
        let mut chunk =
            IOChunk::from_content(IOChunkContent::new_owned(b"already owned".to_vec(), true));

        let result = chunk.ensure_owned();
        assert!(result.is_ok());

        // Content must be unchanged
        match &chunk.content {
            IOChunkContent::Owned {
                content,
                has_newline,
                ..
            } => {
                assert_eq!(content, b"already owned");
                assert!(*has_newline);
            }
        }
    }

    // fields_mut
    #[test]
    fn test_fields_mut_on_owned() {
        let mut chunk = IOChunk::from_content(IOChunkContent::new_owned(b"hello".to_vec(), false));

        let (s, _) = chunk.fields_mut().unwrap();
        s.extend_from_slice(b" world");

        assert_eq!(chunk.as_str().unwrap(), "hello world");
    }

    ///////////////////////////////
    // Unit tests for write_chunk()
    ///////////////////////////////

    fn new_for_test() -> (OutputBuffer, std::fs::File) {
        let file = tempfile().unwrap();
        let buf = OutputBuffer {
            out: BufWriter::new(Box::new(file.try_clone().unwrap())),
            pending_newline: false,
            pending_crlf: false,
            delimiter: b'\n',
            name: "stdout".to_string(),
        };
        (buf, file)
    }

    fn make_owned_chunk(s: &str, has_nl: bool) -> IOChunk {
        IOChunk {
            utf8_verified: Cell::new(true),
            content: IOChunkContent::Owned {
                content: s.as_bytes().to_vec(),
                has_newline: has_nl,
                has_crlf: false,
            },
        }
    }

    #[test]
    fn owned_without_newline() {
        let (mut buf, mut file) = new_for_test();
        let chunk = make_owned_chunk("hello", false);
        buf.write_chunk(&chunk).unwrap();

        buf.out.flush().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut out = String::new();
        file.read_to_string(&mut out).unwrap();

        assert_eq!(out, "hello");
    }

    #[test]
    fn owned_with_newline() {
        let (mut buf, mut file) = new_for_test();
        let chunk = make_owned_chunk("world", true);
        buf.write_chunk(&chunk).unwrap();

        buf.out.flush().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut out = String::new();
        file.read_to_string(&mut out).unwrap();

        assert_eq!(out, "world\n");
    }

    // pending_newline is injected between two no-newline chunks
    #[test]
    fn pending_newline_injected_between_chunks() {
        let (mut buf, mut file) = new_for_test();
        buf.write_chunk(&make_owned_chunk("first", false)).unwrap();
        buf.write_chunk(&make_owned_chunk("second", true)).unwrap();
        buf.out.flush().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut out = String::new();
        file.read_to_string(&mut out).unwrap();
        assert_eq!(out, "first\nsecond\n");
    }

    // flush_pending_newline emits the deferred newline
    #[test]
    fn flush_pending_newline_emits_newline() {
        let (mut buf, mut file) = new_for_test();
        buf.write_chunk(&make_owned_chunk("foo", false)).unwrap();
        assert!(buf.pending_newline);
        buf.flush_pending_newline().unwrap();
        assert!(!buf.pending_newline);
        buf.out.flush().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut out = String::new();
        file.read_to_string(&mut out).unwrap();
        assert_eq!(out, "foo\n");
    }

    // write_str strips trailing newline and sets pending_newline correctly
    #[test]
    fn write_str_with_trailing_newline() {
        let (mut buf, mut file) = new_for_test();
        buf.write_str("bar\n").unwrap();
        assert!(!buf.pending_newline);
        buf.out.flush().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut out = String::new();
        file.read_to_string(&mut out).unwrap();
        assert_eq!(out, "bar\n");
    }

    /// A writer whose device is full, as a full disk fails a write.
    struct DiskFull;

    impl Write for DiskFull {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::StorageFull))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    // A failed write is GNU sed's error, with its status 4: one the buffer puts off fails
    // its flush, and one too big for the buffer fails as it is written.
    #[test]
    fn write_to_full_device() {
        let mut out = OutputBuffer::new(Box::new(DiskFull)).with_name("out".to_string());
        out.write_bytes(b"abc\n").unwrap();
        let error = out.flush().unwrap_err();
        assert_eq!(error.code(), 4);
        assert_eq!(
            error.to_string(),
            "couldn't flush out: No space left on device"
        );

        let mut out = OutputBuffer::new(Box::new(DiskFull)).with_name("out".to_string());
        let error = out.write_bytes(&vec![b'x'; 20_000]).unwrap_err();
        assert_eq!(error.code(), 4);
        assert_eq!(
            error.to_string(),
            "couldn't write 20000 items to out: No space left on device"
        );
    }

    #[test]
    fn write_str_without_trailing_newline() {
        let (mut buf, mut file) = new_for_test();
        buf.write_str("baz").unwrap();
        assert!(buf.pending_newline);
        buf.flush_pending_newline().unwrap();
        buf.out.flush().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut out = String::new();
        file.read_to_string(&mut out).unwrap();
        assert_eq!(out, "baz\n");
    }
}
