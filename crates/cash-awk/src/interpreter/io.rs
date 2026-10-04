//
// Copyright (c) 2024-2026 Hemi Labs, Inc.
//
// This file is part of the posixutils-rs project covered under
// the MIT License.  For the full license text, please see the LICENSE
// file in the root directory of this project.
// SPDX-License-Identifier: MIT
//
use std::{
    collections::{HashMap, hash_map::Entry},
    fs::File,
    io::{BufReader, Bytes, Read, Write},
    path::PathBuf,
    rc::Rc,
    sync::OnceLock,
};

use super::string::AwkString;
use crate::regex::Regex;

/// awk's standard output, buffered as C's stdio buffers it: by line on a terminal, so
/// what is printed shows at once, and in blocks otherwise.
///
/// `print` went through `print!`, Rust's own standard output, which writes every line
/// with a call of its own (28 times slower than gawk over half a million records) and
/// panics when the reader has gone (`awk '{print}' | head -1`; `REVIEW_REPORT.md`
/// TXT-08). awk is one thread, so the writer is the thread's.
enum StdoutWriter {
    Line(std::io::LineWriter<std::io::Stdout>),
    Block(std::io::BufWriter<std::io::Stdout>),
}

impl StdoutWriter {
    fn new() -> Self {
        use std::io::IsTerminal as _;
        let stdout = std::io::stdout();
        if stdout.is_terminal() {
            Self::Line(std::io::LineWriter::new(stdout))
        } else {
            Self::Block(std::io::BufWriter::with_capacity(64 * 1024, stdout))
        }
    }

    fn writer(&mut self) -> &mut dyn Write {
        match self {
            Self::Line(writer) => writer,
            Self::Block(writer) => writer,
        }
    }
}

thread_local! {
    static STDOUT: std::cell::RefCell<StdoutWriter> = std::cell::RefCell::new(StdoutWriter::new());
}

/// Writes `text` to standard output.
pub(crate) fn write_stdout(text: &str) -> Result<(), String> {
    STDOUT
        .with_borrow_mut(|stdout| stdout.writer().write_all(text.as_bytes()))
        .map_err(stdout_failed)
}

/// Writes out what standard output holds: before another program runs (`system()`, a
/// pipe), which writes to the same place and must not overtake it, and when awk ends.
pub(crate) fn flush_stdout() -> Result<(), String> {
    STDOUT
        .with_borrow_mut(|stdout| stdout.writer().flush())
        .map_err(stdout_failed)
}

/// What a failed write to standard output means. When the reader has gone, awk stops as
/// the other bundled tools do on Windows, which has no `SIGPIPE` (spec §4 row 22), and as
/// gawk does with the signal ignored: with a message and status 2.
fn stdout_failed(error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::BrokenPipe {
        eprintln!("awk: write error: Broken pipe");
        std::process::exit(2);
    }
    format!("write error: {error}")
}

pub enum RecordSeparator {
    Char(u8),
    Null,
    Ere(Regex),
}

impl TryFrom<AwkString> for RecordSeparator {
    type Error = String;

    fn try_from(value: AwkString) -> Result<Self, Self::Error> {
        if value.is_empty() {
            Ok(RecordSeparator::Null)
        } else if value.len() == 1 {
            Ok(RecordSeparator::Char(value.as_bytes()[0]))
        } else {
            let ere = Regex::new(value.as_str())?;
            Ok(RecordSeparator::Ere(ere))
        }
    }
}

type ReadResult = Result<u8, String>;

macro_rules! read_iter_next {
    ($iter:expr, $ret:expr) => {
        match $iter.next() {
            Some(byte_result) => byte_result?,
            None => return $ret,
        }
    };
    ($iter:expr) => {
        read_iter_next!($iter, Ok(None))
    };
}

/// Convert bytes to String, trying UTF-8 first, falling back to Latin-1.
/// Latin-1 maps each byte 0x00-0xFF to the corresponding Unicode code point,
/// so it preserves byte values faithfully for single-byte encodings.
fn bytes_to_string(buf: Vec<u8>) -> String {
    match String::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => e.into_bytes().iter().map(|&b| b as char).collect(),
    }
}

/// The record before the first RS match in `buf`, and the bytes after the match, or
/// `None` if no record ends in `buf` yet.
///
/// The match is found in the bytes, which may end inside a character or not be UTF-8:
/// they were decoded first, so a character split by the buffer's edge was fatal
/// (`REVIEW_REPORT.md` TXT-12). A match that reaches the end of the buffer before the
/// end of the input may go on in the bytes still to come (`RS = "\n+"` saw a separator
/// in each newline of a blank line, so a paragraph was followed by empty records), so
/// the record waits for more input.
fn ere_try_match(buf: &[u8], re: &Regex, at_end: bool) -> Option<(String, Vec<u8>)> {
    let m = re.find_separator(buf)?;
    if m.end == buf.len() && !at_end {
        return None;
    }
    Some((
        bytes_to_string(buf[..m.start].to_vec()),
        buf[m.end..].to_vec(),
    ))
}

pub trait RecordReader: Iterator<Item = ReadResult> {
    fn is_done(&self) -> bool;

    fn ere_byte_buffer(&mut self) -> &mut Vec<u8>;

    /// Read the next record. Returns the record text and whether a CRLF line
    /// ending was hidden from it.
    ///
    /// Line-ending policy (shared with cash's sed): when `strip_cr` is set and
    /// RS is a single newline (the default), a CR immediately before the
    /// terminating LF -- or a trailing CR on an unterminated last record -- is
    /// removed from the record and reported as `true`, so that `print` can
    /// restore the CRLF on output. With any other RS (another single
    /// character, an ERE, or paragraph mode `RS=""`) records are returned
    /// byte-for-byte as read, CRs included, and never reported as CRLF --
    /// exactly like awk on Linux. When `strip_cr` is false (the program
    /// mentions a CR, or `CASH_EOL=lf`), CR is always ordinary data.
    fn read_next_record(
        &mut self,
        separator: &RecordSeparator,
        strip_cr: bool,
    ) -> Result<Option<(String, bool)>, String> {
        if self.is_done() {
            return Ok(None);
        }
        match separator {
            RecordSeparator::Char(sep) => {
                let strip = strip_cr && *sep == b'\n';
                let finish = |mut buf: Vec<u8>| -> Result<Option<(String, bool)>, String> {
                    let crlf = strip && buf.last() == Some(&b'\r');
                    if crlf {
                        buf.pop();
                    }
                    Ok(Some((bytes_to_string(buf), crlf)))
                };
                let mut buf = Vec::new();
                let mut next = read_iter_next!(self);
                while next != *sep {
                    buf.push(next);
                    next = read_iter_next!(self, finish(buf));
                }
                finish(buf)
            }
            RecordSeparator::Ere(re) => {
                // Incremental matching: read bytes into a buffer and check
                // for RS matches periodically to support streaming/interactive
                // input without reading everything into memory at once.
                let mut byte_buf = std::mem::take(self.ere_byte_buffer());

                // Check existing buffer first (remainder from previous call)
                if let Some((record, remainder)) = ere_try_match(&byte_buf, re, false) {
                    *self.ere_byte_buffer() = remainder;
                    return Ok(Some((record, false)));
                }

                // After a failed match at length L, skip newline-triggered checks
                // until the buffer grows enough. This avoids O(n*m) worst-case
                // when every newline triggers a futile re-scan of the whole buffer.
                // Threshold starts at 0 so the first newline always triggers a check
                // (important for interactive/streaming input).
                let mut next_newline_check_len: usize = 0;
                let mut bytes_since_check = 0usize;
                loop {
                    match self.next() {
                        Some(byte_result) => {
                            let byte = byte_result?;
                            byte_buf.push(byte);
                            bytes_since_check += 1;
                            let check = if byte == b'\n' {
                                byte_buf.len() >= next_newline_check_len
                            } else {
                                bytes_since_check >= 8192
                            };
                            if check {
                                bytes_since_check = 0;
                                if let Some((record, remainder)) =
                                    ere_try_match(&byte_buf, re, false)
                                {
                                    *self.ere_byte_buffer() = remainder;
                                    return Ok(Some((record, false)));
                                }
                                let len = byte_buf.len();
                                next_newline_check_len = len + len.min(8192);
                            }
                        }
                        None => {
                            // EOF: check for final match, then return remainder
                            if byte_buf.is_empty() {
                                return Ok(None);
                            }
                            if let Some((record, remainder)) = ere_try_match(&byte_buf, re, true) {
                                if !remainder.is_empty() {
                                    *self.ere_byte_buffer() = remainder;
                                }
                                return Ok(Some((record, false)));
                            }
                            let input = bytes_to_string(byte_buf);
                            return Ok(Some((input, false)));
                        }
                    }
                }
            }
            RecordSeparator::Null => {
                // Skip leading blank lines
                let mut line_buf = Vec::new();
                loop {
                    let next = read_iter_next!(self);
                    if next == b'\n' {
                        if line_buf.is_empty() {
                            // blank line, keep skipping
                            continue;
                        }
                        // non-blank line found, we have the first line
                        break;
                    }
                    line_buf.push(next);
                }

                // line_buf has the first line (without newline)
                let mut record_buf = line_buf;

                // Accumulate subsequent lines until a blank line or EOF
                loop {
                    let mut line_buf = Vec::new();
                    loop {
                        match self.next() {
                            Some(byte_result) => {
                                let byte = byte_result?;
                                if byte == b'\n' {
                                    break;
                                }
                                line_buf.push(byte);
                            }
                            None => {
                                // EOF: if this line has content, add it
                                if !line_buf.is_empty() {
                                    record_buf.push(b'\n');
                                    record_buf.extend_from_slice(&line_buf);
                                }
                                return Ok(Some((bytes_to_string(record_buf), false)));
                            }
                        }
                    }
                    if line_buf.is_empty() {
                        // blank line: end of record
                        break;
                    }
                    record_buf.push(b'\n');
                    record_buf.extend_from_slice(&line_buf);
                }

                Ok(Some((bytes_to_string(record_buf), false)))
            }
        }
    }
}

pub struct FileStream {
    bytes: Bytes<BufReader<File>>,
    is_done: bool,
    ere_byte_buffer: Vec<u8>,
}

/// What went wrong with a file, in the C library's words, as gawk reports it: "No such
/// file or directory" where Windows says "The system cannot find the path specified.
/// (os error 3)".
pub(crate) fn strerror(error: &std::io::Error) -> String {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::NotFound => "No such file or directory".to_string(),
        ErrorKind::PermissionDenied => "Permission denied".to_string(),
        ErrorKind::AlreadyExists => "File exists".to_string(),
        ErrorKind::IsADirectory => "Is a directory".to_string(),
        ErrorKind::NotADirectory => "Not a directory".to_string(),
        ErrorKind::InvalidFilename | ErrorKind::InvalidInput => "Invalid argument".to_string(),
        ErrorKind::BrokenPipe => "Broken pipe".to_string(),
        ErrorKind::StorageFull => "No space left on device".to_string(),
        _ => {
            let text = error.to_string();
            match text.find(" (os error ") {
                Some(end) => text.get(..end).unwrap_or_default().to_string(),
                None => text,
            }
        }
    }
}

impl FileStream {
    /// The file at `path`, to read; the error is gawk's for an input file.
    pub fn open(path: &str) -> Result<Self, String> {
        let file = File::open(path)
            .map_err(|e| format!("cannot open file `{path}' for reading: {}", strerror(&e)))?;
        let reader = BufReader::new(file);
        Ok(Self {
            bytes: reader.bytes(),
            is_done: false,
            ere_byte_buffer: Vec::new(),
        })
    }
}

impl Iterator for FileStream {
    type Item = ReadResult;

    fn next(&mut self) -> Option<Self::Item> {
        match self.bytes.next() {
            Some(Ok(byte)) => Some(Ok(byte)),
            Some(Err(e)) => Some(Err(e.to_string())),
            None => {
                self.is_done = true;
                None
            }
        }
    }
}

impl RecordReader for FileStream {
    fn is_done(&self) -> bool {
        self.is_done && self.ere_byte_buffer.is_empty()
    }

    fn ere_byte_buffer(&mut self) -> &mut Vec<u8> {
        &mut self.ere_byte_buffer
    }
}

#[cfg(test)]
pub struct StringRecordReader {
    string: String,
    index: usize,
    ere_byte_buffer: Vec<u8>,
}

#[cfg(test)]
impl<S: Into<String>> From<S> for StringRecordReader {
    fn from(value: S) -> Self {
        Self {
            string: value.into(),
            index: 0,
            ere_byte_buffer: Vec::new(),
        }
    }
}

#[cfg(test)]
impl Iterator for StringRecordReader {
    type Item = ReadResult;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.string.len() {
            None
        } else {
            let result = self.string.as_bytes()[self.index];
            self.index += 1;
            Some(Ok(result))
        }
    }
}

#[cfg(test)]
impl RecordReader for StringRecordReader {
    fn is_done(&self) -> bool {
        self.index == self.string.len() && self.ere_byte_buffer.is_empty()
    }

    fn ere_byte_buffer(&mut self) -> &mut Vec<u8> {
        &mut self.ere_byte_buffer
    }
}

/// A no-op record reader that immediately signals EOF.
/// The `ere_byte_buffer` field exists solely to satisfy the `RecordReader` trait;
/// `Vec::new()` (via `Default`) does not heap-allocate.
#[derive(Default)]
pub struct EmptyRecordReader {
    ere_byte_buffer: Vec<u8>,
}

impl Iterator for EmptyRecordReader {
    type Item = ReadResult;

    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}

impl RecordReader for EmptyRecordReader {
    fn is_done(&self) -> bool {
        true
    }

    fn ere_byte_buffer(&mut self) -> &mut Vec<u8> {
        &mut self.ere_byte_buffer
    }
}

#[derive(Default)]
pub struct WriteFiles {
    files: HashMap<String, File>,
}

impl WriteFiles {
    pub fn write(&mut self, filename: &str, contents: &str, append: bool) -> Result<(), String> {
        // The same stream as `print`, so the two keep their order.
        if filename == "/dev/stdout" {
            return write_stdout(contents);
        }
        match self.files.entry(filename.to_string()) {
            Entry::Occupied(mut e) => {
                e.get_mut()
                    .write_all(contents.as_bytes())
                    .map_err(|e| e.to_string())?;
            }
            Entry::Vacant(e) => {
                let mut file = File::options()
                    .write(true)
                    .create(true)
                    .truncate(!append)
                    .append(append)
                    .open(filename)
                    // gawk's words.
                    .map_err(|e| format!("cannot redirect to `{filename}': {}", strerror(&e)))?;
                file.write_all(contents.as_bytes())
                    .map_err(|e| e.to_string())?;
                e.insert(file);
            }
        }
        Ok(())
    }

    pub fn flush_file(&mut self, filename: &str) -> bool {
        if let Some(file) = self.files.get_mut(filename) {
            file.flush().is_ok()
        } else {
            false
        }
    }

    pub fn flush_all(&mut self) -> bool {
        let mut success = true;
        for file in self.files.values_mut() {
            success = success && file.flush().is_ok();
        }
        success
    }

    /// Close a previously-opened output file. Returns `Some(0)` on a
    /// successful flush+close, `Some(-1)` if flushing failed, or `None` if no
    /// file was open under this name.
    pub fn close_file(&mut self, filename: &str) -> Option<i32> {
        self.files
            .remove(filename)
            .map(|mut file| if file.flush().is_ok() { 0 } else { -1 })
    }
}

#[derive(Default)]
pub struct ReadFiles {
    files: HashMap<Rc<str>, FileStream>,
}

impl ReadFiles {
    pub fn read_next_record(
        &mut self,
        filename: AwkString,
        separator: &RecordSeparator,
        strip_cr: bool,
    ) -> Result<Option<(String, bool)>, String> {
        let filename = Rc::<str>::from(filename);
        match self.files.entry(filename.clone()) {
            Entry::Occupied(mut e) => e.get_mut().read_next_record(separator, strip_cr),
            Entry::Vacant(e) => {
                let mut file = FileStream::open(&filename)?;
                let result = file.read_next_record(separator, strip_cr);
                e.insert(file);
                result
            }
        }
    }

    /// Close a previously-opened input file. Returns `Some(0)` if a file was
    /// open under this name, or `None` otherwise.
    pub fn close_file(&mut self, filename: &str) -> Option<i32> {
        self.files.remove(filename).map(|_| 0)
    }
}

/// The cash that runs commands for awk run as a program of its own (`set_shell`).
static SHELL: OnceLock<PathBuf> = OnceLock::new();

/// Makes `system()` and pipes run commands with the cash at `path`. Only awk run as a
/// program of its own, outside cash, calls it (`src/bin/awk.rs`, for testing): inside
/// cash, this process's own exe is cash, as a bundled tool (`cash --invoke-bundled awk`)
/// and as a link `cash --link-tools` made (`awk.exe`) alike.
pub fn set_shell(path: PathBuf) {
    // Set once, before awk runs; a second call would change nothing.
    let _ = SHELL.set(path);
}

/// The variable through which a cash learns the name it was started by
/// (`cash_core::commands::ARGV0_VARIABLE`). Set, it also tells a cash whose exe is a
/// link `cash --link-tools` made that it was started as the shell, not as the tool.
const ARGV0_VARIABLE: &str = "CASH_ARGV0";

/// The command that runs `cmd_str` in the shell, for `system()` and pipes: cash with
/// `-c`. awk runs inside cash, so that is this process's own exe, whatever its file is
/// named. It was taken to be cash only when named `cash` (or `cash-…`), so in a linked
/// `awk.exe` the commands ran in `cmd`; and a `CASH_BIN` variable, a test hook, chose
/// any program in production.
pub(crate) fn create_shell_command(cmd_str: &str) -> std::process::Command {
    // Another program writes where awk does, so what awk has printed goes first, as gawk,
    // mawk and BWK awk (and POSIX, for system()) have it (TXT-14). A failure here is the
    // write's to report, when it is tried again.
    let _ = flush_stdout();

    // Should Windows not say where this process's exe is, the cash on PATH is the next
    // best.
    let cash = SHELL
        .get()
        .cloned()
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_else(|| PathBuf::from("cash"));
    let mut cmd = std::process::Command::new(cash);
    cmd.env(ARGV0_VARIABLE, "cash")
        .args(["--norc", "--noprofile", "-c", cmd_str]);
    cmd
}

#[derive(Default)]
pub struct WritePipes {
    pipes: HashMap<Rc<str>, std::process::Child>,
}

impl WritePipes {
    pub fn write(&mut self, command: AwkString, contents: AwkString) -> Result<(), String> {
        let key = Rc::from(command.clone());
        let child = match self.pipes.entry(key) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => {
                let command_str = command.as_str().to_string();
                let mut cmd = create_shell_command(&command_str);
                cmd.stdin(std::process::Stdio::piped());
                let child = cmd.spawn().map_err(|e| e.to_string())?;
                e.insert(child)
            }
        };
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write as _;
            stdin
                .write_all(contents.as_bytes())
                .map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err("failed to write to pipe: stdin unavailable".to_string())
        }
    }

    pub fn flush_file(&mut self, filename: &str) -> bool {
        if let Some(child) = self.pipes.get_mut(filename) {
            if let Some(stdin) = child.stdin.as_mut() {
                use std::io::Write as _;
                return stdin.flush().is_ok();
            }
        }
        false
    }

    pub fn flush_all(&mut self) -> bool {
        let mut success = true;
        for child in self.pipes.values_mut() {
            if let Some(stdin) = child.stdin.as_mut() {
                use std::io::Write as _;
                success = success && stdin.flush().is_ok();
            }
        }
        success
    }

    /// Close a previously-opened output pipe. Returns `Some(0)` on a
    /// successful `pclose`, `Some(-1)` if `pclose` failed, or `None` if no pipe
    /// was open under this name.
    pub fn close_pipe(&mut self, filename: &str) -> Option<i32> {
        self.pipes.remove(filename).map(|mut child| {
            drop(child.stdin.take());
            match child.wait() {
                Ok(status) => status.code().unwrap_or(0),
                Err(_) => -1,
            }
        })
    }
}

impl Drop for WritePipes {
    fn drop(&mut self) {
        for (_, mut child) in self.pipes.drain() {
            drop(child.stdin.take());
            let _ = child.wait();
        }
    }
}

pub struct PipeRecordReader {
    child: std::process::Child,
    /// Buffered: unbuffered, every byte of a command's output was a read of its own
    /// (`REVIEW_REPORT.md` TXT-16).
    stdout_reader: Option<std::io::Bytes<BufReader<std::process::ChildStdout>>>,
    is_done: bool,
    closed: bool,
    ere_byte_buffer: Vec<u8>,
}

impl PipeRecordReader {
    pub fn open(command: &str) -> Result<Self, String> {
        let mut cmd = create_shell_command(command);
        cmd.stdout(std::process::Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "missing stdout".to_string())?;
        use std::io::Read as _;
        Ok(Self {
            child,
            stdout_reader: Some(BufReader::new(stdout).bytes()),
            is_done: false,
            closed: false,
            ere_byte_buffer: Vec::new(),
        })
    }

    /// `pclose` the pipe and return the resulting status (0 on success, -1 on
    /// failure). Marks the reader closed so `Drop` will not close it again.
    fn pclose(&mut self) -> i32 {
        self.closed = true;
        self.stdout_reader = None;
        match self.child.wait() {
            Ok(status) => status.code().unwrap_or(0),
            Err(_) => -1,
        }
    }
}

impl Iterator for PipeRecordReader {
    type Item = ReadResult;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_done {
            return None;
        }
        if let Some(reader) = self.stdout_reader.as_mut() {
            match reader.next() {
                Some(Ok(b)) => Some(Ok(b)),
                Some(Err(e)) => {
                    self.is_done = true;
                    Some(Err(e.to_string()))
                }
                None => {
                    self.is_done = true;
                    None
                }
            }
        } else {
            None
        }
    }
}

impl RecordReader for PipeRecordReader {
    fn is_done(&self) -> bool {
        self.is_done && self.ere_byte_buffer.is_empty()
    }

    fn ere_byte_buffer(&mut self) -> &mut Vec<u8> {
        &mut self.ere_byte_buffer
    }
}

impl Drop for PipeRecordReader {
    fn drop(&mut self) {
        if !self.closed {
            self.stdout_reader = None;
            let _ = self.child.wait();
        }
    }
}

#[derive(Default)]
pub struct ReadPipes {
    pipes: HashMap<Rc<str>, PipeRecordReader>,
}

impl ReadPipes {
    pub fn read_next_record(
        &mut self,
        command: AwkString,
        separator: &RecordSeparator,
        strip_cr: bool,
    ) -> Result<Option<(String, bool)>, String> {
        let command = Rc::<str>::from(command);
        match self.pipes.entry(command.clone()) {
            Entry::Occupied(mut e) => e.get_mut().read_next_record(separator, strip_cr),
            Entry::Vacant(e) => {
                let mut reader = PipeRecordReader::open(&command)?;
                let result = reader.read_next_record(separator, strip_cr);
                e.insert(reader);
                result
            }
        }
    }

    /// Close a previously-opened input pipe. Returns `Some(0)` on a successful
    /// `pclose`, `Some(-1)` if `pclose` failed, or `None` if no pipe was open
    /// under this name.
    pub fn close_pipe(&mut self, command: &str) -> Option<i32> {
        self.pipes.remove(command).map(|mut reader| reader.pclose())
    }
}

pub struct StdinRecordReader {
    /// Standard input, locked once for the reader's life: it was locked and unlocked for
    /// every byte (`REVIEW_REPORT.md` TXT-16).
    bytes: Bytes<std::io::StdinLock<'static>>,
    is_done: bool,
    ere_byte_buffer: Vec<u8>,
}

impl Default for StdinRecordReader {
    fn default() -> Self {
        Self {
            bytes: std::io::stdin().lock().bytes(),
            is_done: false,
            ere_byte_buffer: Vec::new(),
        }
    }
}

impl Iterator for StdinRecordReader {
    type Item = ReadResult;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.bytes.next();
        match next {
            Some(Ok(byte)) => Some(Ok(byte)),
            Some(Err(e)) => Some(Err(e.to_string())),
            None => {
                self.is_done = true;
                None
            }
        }
    }
}

impl RecordReader for StdinRecordReader {
    fn is_done(&self) -> bool {
        self.is_done && self.ere_byte_buffer.is_empty()
    }

    fn ere_byte_buffer(&mut self) -> &mut Vec<u8> {
        &mut self.ere_byte_buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split_records(file_contents: &str, separator: RecordSeparator) -> Vec<String> {
        let mut reader = StringRecordReader::from(file_contents);
        let mut result = Vec::new();
        while let Some((record, _)) = reader.read_next_record(&separator, true).unwrap() {
            result.push(record);
        }
        result
    }

    #[test]
    fn split_empty_file() {
        assert!(split_records("", RecordSeparator::Null).is_empty());
    }

    #[test]
    fn split_records_with_paragraph_separator() {
        let records = split_records("record1\nrecord2\n\nrecord3\n", RecordSeparator::Null);
        assert_eq!(records, vec!["record1\nrecord2", "record3"]);
    }

    #[test]
    fn split_records_with_separator_chars() {
        let records = split_records("record1,record2,record3", RecordSeparator::Char(b','));
        assert_eq!(records, vec!["record1", "record2", "record3"]);
    }
}
