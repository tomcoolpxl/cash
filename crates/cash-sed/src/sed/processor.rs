// Process the files with the compiled scripts
//
// SPDX-License-Identifier: MIT
// Copyright (c) 2025 Diomidis Spinellis
//
// This file is part of the uutils sed package.
// It is licensed under the MIT License.
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use crate::sed::command::{
    Address, AppendElement, CharacterMode, Command, CommandData, InputAction, LineFile, LineInput,
    ProcessingContext, Transliteration,
};
use crate::sed::error_handling::{runtime_err, runtime_error, strerror};
use crate::sed::fast_io::{
    DEV_STDIN, IOChunk, LineReader, OutputBuffer, is_directory, is_pipe_path, read_dev_stdin,
};
use crate::sed::fast_regex::Regex;
use crate::sed::in_place::InPlace;
use crate::sed::named_writer::{self, NamedWriter};

use memchr::memchr;
use std::borrow::Cow;
use std::cell::RefCell;
use std::ffi::OsStr;
use std::io::{self, BufRead, IsTerminal, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;
use uucore::error::{UError, UResult, USimpleError, set_exit_code};

/// Return the specified command variant, or return an error from the calling function.
/// The compiler gives each command code its data, so the error is never reached; it
/// stands in for what was a panic. GNU sed words its own as "INTERNAL ERROR".
// Example: let path = extract_variant!(command, Path);
macro_rules! extract_variant {
    ($cmd:expr, $variant:ident) => {
        match &$cmd.data {
            CommandData::$variant(inner) => inner,
            _ => {
                return runtime_error(concat!(
                    "INTERNAL ERROR: expected ",
                    stringify!($variant),
                    " command data"
                ));
            }
        }
    };
}

/// Return true if the passed address matches the current I/O context.
fn match_address(
    addr: &Address,
    reader: &mut LineReader,
    pattern: &mut IOChunk,
    context: &mut ProcessingContext,
) -> UResult<bool> {
    match addr {
        Address::Re(re) => {
            let regex = re_or_saved_re(re.as_ref(), context)?;
            regex
                .is_match(pattern)
                .map_err(|e| runtime_err(e.to_string()))
        }

        Address::Line(lineno) => Ok(context.line_number == *lineno),

        // `first~step` matches line `first` and every `step`th line after it.
        Address::Step { first, step } => Ok(context
            .line_number
            .checked_sub(*first)
            .is_some_and(|after| after.is_multiple_of(*step))),

        // "$" is the last line of the input: of the current file with `-s` and `-i`, and
        // else of the last file with any, as GNU sed looks ahead past files that are
        // empty or cannot be read (`no_later_input`). Only empty regular files were
        // looked past.
        Address::Last => Ok(reader
            .last_line()
            .map_err(|e| read_error(&context.input_name, &e))?
            && (context.separate || no_later_input(context))),

        // The relative forms are only ever second addresses, which `applies` decides
        // itself.
        Address::RelLine(_) | Address::StepEnd(_) => {
            runtime_error("INTERNAL ERROR: invalid address type")
        }
    }
}

/// GNU sed's name for an input in an error: `stdin` for `-`, else its name.
fn input_display_name(path: &Path) -> String {
    if path == Path::new("-") {
        "stdin".to_string()
    } else {
        path.display().to_string()
    }
}

/// GNU sed's error for an input it could not read, with its status 4: `read error on
/// stdin: ...`.
fn read_error(path: &Path, error: &io::Error) -> Box<dyn UError> {
    runtime_err(format!(
        "read error on {}: {}",
        input_display_name(path),
        strerror(error)
    ))
}

/// Whether no input follows the current file's, for `$`. As GNU sed looks ahead, a file
/// that is empty or cannot be read has none, and standard input is looked into. Another
/// file that is not a regular one, a pipe, is opened and read into, and kept open for
/// its turn (`ReadAhead`); it was taken to have input, and a named pipe asked for its
/// kind was then busy when its turn came.
fn no_later_input(context: &mut ProcessingContext) -> bool {
    if context.last_file {
        return true;
    }
    if let Some(later) = context.later_input {
        return !later;
    }
    let reading_stdin = context.input_name == Path::new("-");
    let later = context
        .later_files
        .iter()
        .enumerate()
        .any(|(offset, path)| {
            let index = context.later_start + offset;
            if path == Path::new("-") {
                // Standard input read to its end already has nothing more.
                return !reading_stdin
                    && io::stdin()
                        .lock()
                        .fill_buf()
                        .is_ok_and(|buffer| !buffer.is_empty());
            }
            if !is_pipe_path(path) {
                match std::fs::metadata(path) {
                    Ok(metadata) if metadata.is_file() => return metadata.len() > 0,
                    Ok(metadata) if metadata.is_dir() => return true,
                    Ok(_) => {}
                    Err(_) => return false,
                }
            }
            if let Some(has_input) = context.read_ahead.has_input(index) {
                return has_input;
            }
            let Ok(reader) = LineReader::open_with(path, context.treats_cr_as_data()) else {
                return false;
            };
            context.read_ahead.keep(index, reader);
            context.read_ahead.has_input(index).unwrap_or(false)
        });
    context.later_input = Some(later);
    !later
}

#[allow(dead_code)]
/// Return true if the command applies to the given pattern.
fn applies(
    command: &mut Command,
    reader: &mut LineReader,
    pattern: &mut IOChunk,
    context: &mut ProcessingContext,
) -> UResult<bool> {
    let linenum = context.line_number;

    let result = if let Some(addr2) = &command.addr2 {
        // Two addresses. What a range already latched says about this line, or `None`
        // when the line is past a numbered end and the first address decides again.
        let mut latched = None;
        if let Some(start) = command.start_line {
            match addr2 {
                // `addr1,N` and `addr1,+N` end on a line number, inclusive, which is the
                // range's last line (`c` prints its text there). A line already past it,
                // reached when the command was skipped, closes the range and is the first
                // address's to start again, as GNU sed has it. The line after the end was
                // refused without that check, so `/x/,+1p` missed a second `x` right after
                // a range, and `2,3c T` never printed `T` (`REVIEW_REPORT.md` TXT-04,
                // TXT-07).
                Address::RelLine(n) | Address::Line(n) => {
                    let end = if matches!(addr2, Address::RelLine(_)) {
                        start + *n
                    } else {
                        *n
                    };
                    if linenum < end {
                        latched = Some(true);
                    } else {
                        command.start_line = None;
                        if linenum == end {
                            context.last_address = true;
                            latched = Some(true);
                        }
                    }
                }
                Address::StepEnd(step) => {
                    // Inclusive end on multiple of step
                    if linenum.is_multiple_of(*step) {
                        command.start_line = None;
                    }
                    latched = Some(true);
                }
                _ => {
                    if match_address(addr2, reader, pattern, context)? {
                        command.start_line = None;
                        context.last_address = true;
                    }
                    latched = Some(true);
                }
            }
        }

        if let Some(latched) = latched {
            Ok(latched)
        } else if let Some(addr1) = &command.addr1 {
            // See if latch must start.
            if match_address(addr1, reader, pattern, context)? {
                match addr2 {
                    Address::Line(n) if linenum >= *n => {
                        context.last_address = true;
                    }
                    // `addr1,+0` and `addr1,~0` are the first line alone, as in GNU sed;
                    // `~0` would otherwise wait for a multiple of 0, which never comes.
                    Address::RelLine(0) | Address::StepEnd(0) => {
                        context.last_address = true;
                    }
                    _ => {
                        command.start_line = Some(linenum);
                    }
                }
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    } else if let Some(addr1) = &command.addr1 {
        // Single address
        Ok(match_address(addr1, reader, pattern, context)?)
    } else {
        // No address
        Ok(true)
    };

    if command.non_select {
        result.map(|v| !v)
    } else {
        result
    }
}

/// Write the specified chunk to the output for a given processing context.
fn write_chunk(
    output: &mut OutputBuffer,
    context: &ProcessingContext,
    chunk: &IOChunk,
) -> UResult<()> {
    output.write_chunk(chunk)?;

    if context.unbuffered {
        output.flush()?;
    }

    Ok(())
}

/// Return a reference to the current or the saved RE if the RE is None.
/// Update the saved RE to RE.
///
/// An empty RE before any other is GNU sed's error, found when it is first matched but
/// worded and placed as one in the script, where its reading ended (`-e expression #1,
/// char 0`), with the script's exit status 1. It was a run-time error of cash's own form
/// with status 2.
fn re_or_saved_re<'a>(
    regex: Option<&Regex>,
    context: &'a mut ProcessingContext,
) -> UResult<&'a Regex> {
    if let Some(re) = regex {
        // First time we see this regex: clone it *once* into the context, and return a
        // reference into context.saved_regex.
        Ok(context.saved_regex.insert(re.clone()))
    } else if let Some(ref saved_re) = context.saved_regex {
        // We already have one: just borrow it.
        Ok(saved_re)
    } else {
        Err(USimpleError::new(
            1,
            format!(
                "{}: no previous regular expression",
                context.script_end_place
            ),
        ))
    }
}

/// The cash that runs commands for sed run as a program of its own (`set_shell`).
static SHELL: OnceLock<PathBuf> = OnceLock::new();

/// Makes the `e` command and the `e` flag of `s` run commands with the cash at `path`.
/// Only sed run as a program of its own, outside cash, calls it (`src/bin/sed.rs`, for
/// testing): inside cash, this process's own exe is cash, as a bundled tool
/// (`cash --invoke-bundled sed`) and as a link `cash --link-tools` made (`sed.exe`) alike.
pub fn set_shell(path: PathBuf) {
    // Set once, before sed runs; a second call would change nothing.
    let _ = SHELL.set(path);
}

/// The variable through which a cash learns the name it was started by
/// (`cash_core::commands::ARGV0_VARIABLE`). Set, it also tells a cash whose exe is a
/// link `cash --link-tools` made that it was started as the shell, not as the tool.
const ARGV0_VARIABLE: &str = "CASH_ARGV0";

/// The command that runs `cmd` in the shell: cash with `-c`. sed runs inside cash, so
/// that is this process's own exe, whatever its file is named. It was taken to be cash
/// only when named `cash` (or `cash-…`), so in a linked `sed.exe` the commands ran in
/// `cmd`; and a `CASH_BIN` variable, a test hook, chose any program in production.
fn shell_command(cmd: &OsStr) -> std::process::Command {
    // Should Windows not say where this process's exe is, the cash on PATH is the next
    // best.
    let cash = SHELL
        .get()
        .cloned()
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_else(|| PathBuf::from("cash"));
    let mut c = std::process::Command::new(cash);
    c.env(ARGV0_VARIABLE, "cash")
        .args(["--norc", "--noprofile", "-c"])
        .arg(cmd);
    c
}

/// Run the given command bytes in a shell, returning its raw standard
/// output. The child's standard error is left connected to this process's
/// own, matching GNU sed's behavior where shell errors surface directly
/// rather than being silently captured.
///
/// A command that cannot be started is GNU sed's "error in subprocess", status 4; cash's
/// sed said why, in its own form. Bytes of the command that are not UTF-8, which a
/// Windows command line cannot hold, are replaced, so that the shell runs it and says
/// what is wrong with it, as GNU sed's shell does; it was refused.
fn shell_stdout(cmd: &[u8]) -> UResult<Vec<u8>> {
    let os_cmd = std::ffi::OsString::from(String::from_utf8_lossy(cmd).into_owned());
    shell_command(&os_cmd)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            let mut stdout = child
                .stdout
                .take()
                .ok_or_else(|| io::Error::other("no pipe from its standard output"))?;
            let mut buf = Vec::new();
            stdout.read_to_end(&mut buf)?;
            child.wait()?;
            Ok(buf)
        })
        .map_err(|_| runtime_err("error in subprocess"))
}

/// Execute the pattern space as a shell command, replacing its contents
/// with the command's standard output, minus one trailing newline.
fn execute_pattern_as_shell_command(pattern: &mut IOChunk) -> UResult<()> {
    let mut shell_out = shell_stdout(pattern.as_bytes())?;
    if shell_out.ends_with(b"\r\n") {
        // On Windows a trailing \r\n is the line terminator. Strip both.
        shell_out.truncate(shell_out.len() - 2);
    }
    // Cash and some Windows tools end with a single \n. Strip it, as GNU sed does.
    if shell_out.ends_with(b"\n") {
        shell_out.pop();
    }
    pattern.set_to_bytes(shell_out, pattern.is_newline_terminated());
    Ok(())
}

/// Perform the specified RE replacement in the provided pattern space.
fn substitute(
    pattern: &mut IOChunk,
    command: &Command,
    context: &mut ProcessingContext,
    output: &mut OutputBuffer,
) -> UResult<()> {
    let sub = extract_variant!(command, Substitution);

    let mut count = 0;
    let mut last_end = 0;
    let mut result = Vec::new();
    let mut replaced = false;
    let text = pattern.as_bytes();

    let regex = re_or_saved_re(sub.regex.as_ref(), context)?;

    // The following let block allows a common input_runtime_error to be
    // called once in all cases, and most importantly, to finish the regex
    // mutable borrowing of context, so as to reuse context in the error call.
    let subst_result = match (sub.occurrence, sub.global, sub.replacement.max_group_number) {
        (1, false, 0) => {
            // Example: s/foo/bar/: find() is enough.
            match regex.find(pattern) {
                Err(e) => Err(e),
                Ok(Some(m)) => {
                    result.extend_from_slice(&text[last_end..m.start()]);

                    let replacement = sub.replacement.apply_match(&m);
                    result.extend_from_slice(&replacement);
                    replaced = true;
                    last_end = m.end();
                    Ok(())
                }
                Ok(None) => Ok(()), // No match
            }
        }

        (1, false, _) => {
            // Example: s/\(.\)\(.\)/\2\1/: captures() is enough.
            match regex.captures(pattern) {
                Err(e) => Err(e),
                Ok(Some(caps)) => {
                    #[expect(
                        clippy::expect_used,
                        reason = "every match has group 0, the whole match"
                    )]
                    let m = caps.get(0)?.expect("a match has group 0");
                    result.extend_from_slice(&text[last_end..m.start()]);

                    let replacement = sub.replacement.apply_captures(&caps)?;
                    result.extend_from_slice(&replacement);
                    replaced = true;
                    last_end = m.end();
                    Ok(())
                }
                Ok(None) => Ok(()), // No match
            }
        }

        (_, _, _) => {
            // Example: s/(.)(.)/\2\1/3: captures_iter() is needed.
            // Iterate over multiple captures of the RE in the pattern.
            'captures: {
                for caps_result in regex.captures_iter(pattern)? {
                    let caps = match caps_result {
                        Ok(caps) => caps,
                        Err(e) => break 'captures Err(e),
                    };
                    count += 1;

                    #[expect(
                        clippy::expect_used,
                        reason = "every match has group 0, the whole match"
                    )]
                    let m = caps.get(0)?.expect("a match has group 0");

                    // Always write the unmatched text before this match.
                    result.extend_from_slice(&text[last_end..m.start()]);

                    if (sub.global && count >= sub.occurrence) || count == sub.occurrence {
                        let replacement = sub.replacement.apply_captures(&caps)?;
                        result.extend_from_slice(&replacement);
                        replaced = true;
                    } else {
                        // Not the target match — leave the match unchanged.
                        result.extend_from_slice(m.as_bytes());
                    }

                    last_end = m.end();

                    // Early exit if only a specific occurrence,
                    // (not global) needed replacing.
                    if !sub.global && count == sub.occurrence {
                        break 'captures Ok(());
                    }
                }
                break 'captures Ok(());
            }
        }
    };

    // Handle errors.
    if let Err(e) = subst_result {
        return runtime_error(e.to_string());
    }

    // Handle substitution success.
    if replaced {
        result.extend_from_slice(&text[last_end..]);

        pattern.set_to_bytes(result, pattern.is_newline_terminated());

        // Apply the 'p' and 'e' flags in the order they were given: 'pe'
        // prints the pre-execution text then executes, while 'ep' executes
        // then prints the result.
        if sub.print_flag && sub.p_before_e {
            write_chunk(output, context, pattern)?;
        }
        if sub.execute {
            execute_pattern_as_shell_command(pattern)?;
        }
        if sub.print_flag && !sub.p_before_e {
            write_chunk(output, context, pattern)?;
        }

        // Write to file if needed.
        if let Some(ref writer) = sub.write_file {
            write_to_file(
                writer,
                pattern.as_bytes(),
                pattern.is_newline_terminated(),
                output,
                context,
            )?;
        }
        context.substitution_made = true;
    }

    Ok(())
}

/// Apply the specified transliteration in the provided pattern space.
fn transliterate(pattern: &mut IOChunk, trans: &Transliteration, context: &ProcessingContext) {
    if context.character_mode == CharacterMode::Byte || trans.is_byte_identity {
        let text = pattern.as_bytes();
        let mut result = Vec::with_capacity(text.len());
        let mut replaced = false;

        for &byte in text {
            let mapped = trans.lookup_byte(byte);
            if mapped != byte {
                replaced = true;
            }
            result.push(mapped);
        }

        if replaced {
            pattern.set_to_bytes(result, pattern.is_newline_terminated());
        }

        return;
    }

    // Bytes that are not UTF-8 stay as they are, as in GNU sed; they were an error.
    let text = pattern.as_bytes();
    let mut result = Vec::with_capacity(text.len());
    let mut replaced = false;

    // Perform the transliteration.
    for chunk in text.utf8_chunks() {
        for ch in chunk.valid().chars() {
            let mapped = trans.lookup_char(ch);
            if mapped != ch {
                replaced = true;
            }
            let mut buffer = [0; 4];
            result.extend_from_slice(mapped.encode_utf8(&mut buffer).as_bytes());
        }
        result.extend_from_slice(chunk.invalid());
    }

    // Lazy replace.
    if replaced {
        pattern.set_to_bytes(result, pattern.is_newline_terminated());
    }
}

/// Queue the next line of a file `R` reads, its delimiter included when it has one, for
/// the end of the cycle; nothing once the file is read to its end, or when it could not
/// be opened. A last line without a newline is written without one, as in GNU sed.
///
/// A file that cannot be read is GNU sed's "read error on F: ...", status 4: a
/// directory, which it opens and cannot read.
fn queue_line_of(file: &LineFile, path: &Path, context: &mut ProcessingContext) -> UResult<()> {
    let mut file = file.borrow_mut();
    let reader = match &mut *file {
        LineInput::Missing => return Ok(()),
        LineInput::Directory => {
            return runtime_error(format!("read error on {}: Is a directory", path.display()));
        }
        LineInput::File(reader) | LineInput::Stdin(reader) => reader,
    };
    let mut text = Vec::new();
    match reader.read_until(context.delimiter(), &mut text) {
        Ok(0) => Ok(()),
        Ok(_) => {
            context.append_elements.push(AppendElement::Line(text));
            Ok(())
        }
        Err(e) => Err(read_error(path, &e)),
    }
}

/// Start the files `R` reads over, for the next input file of `-s` or `-i`, as GNU sed
/// does; but standard input, which GNU sed does not start over.
fn rewind_line_files(context: &ProcessingContext) -> UResult<()> {
    for (path, file) in &context.line_files {
        if let LineInput::File(reader) = &mut *file.borrow_mut() {
            reader
                .seek(SeekFrom::Start(0))
                .map_err(|e| read_error(path, &e))?;
        }
    }
    Ok(())
}

/// Write the file `r` reads to the output: GNU sed's special file `/dev/stdin` is what is
/// left of standard input, or all of it when it is a file, as opening `/dev/stdin` gives
/// it on GNU's systems; Windows has no such file, and nothing was written. Once sed has
/// read standard input as an input file (`stdin_done`), GNU sed has closed it, and the
/// file has nothing.
fn append_file(output: &mut OutputBuffer, path: &PathBuf, stdin_done: bool) -> UResult<()> {
    if path.as_os_str() == DEV_STDIN {
        if stdin_done {
            return Ok(());
        }
        let contents = read_dev_stdin().map_err(|e| read_error(Path::new("-"), &e))?;
        if contents.is_empty() {
            return Ok(());
        }
        return output.write_raw(&contents);
    }
    output.copy_file(path)
}

/// Write a line to the file of `w`, `W` or the `w` flag of `s`. GNU sed's `/dev/stdout`
/// is sed's own output, written in order with the rest of it, but in an in-place edit,
/// whose output is the file.
fn write_to_file(
    writer: &Rc<RefCell<NamedWriter>>,
    line: &[u8],
    newline: bool,
    output: &mut OutputBuffer,
    context: &ProcessingContext,
) -> UResult<()> {
    let mut writer = writer.borrow_mut();
    let bytes = writer.line_bytes(line, newline, context.delimiter());
    if writer.is_stdout() && !context.in_place {
        // GNU sed's `/dev/stdout` keeps its own count of a missing line end, apart
        // from sed's output, though both are written to standard output.
        output.write_apart(&bytes)?;
        if context.unbuffered {
            output.flush()?;
        }
        return Ok(());
    }
    writer.write_bytes(&bytes)
}

/// Output any data queued for output at the end of the cycle.
fn flush_appends(output: &mut OutputBuffer, context: &mut ProcessingContext) -> UResult<()> {
    for elem in &context.append_elements {
        match elem {
            AppendElement::Text(text) => {
                output.write_bytes(text.as_ref())?;
            }
            AppendElement::Path(path) => {
                append_file(output, path, context.stdin_done)?;
            }
            AppendElement::Line(line) => {
                output.write_raw(line)?;
            }
        }
    }
    context.append_elements.clear();
    Ok(())
}

/// Return the list command rendering for one ASCII byte.
fn readable_ascii_byte(byte: u8) -> Cow<'static, str> {
    match byte {
        b'\x07' => Cow::Borrowed(r"\a"),
        b'\x08' => Cow::Borrowed(r"\b"),
        b'\x0b' => Cow::Borrowed(r"\v"),
        b'\x0c' => Cow::Borrowed(r"\f"),
        b'\\' => Cow::Borrowed(r"\\"),
        b'\r' => Cow::Borrowed(r"\r"),
        b'\t' => Cow::Borrowed(r"\t"),
        b if b.is_ascii_control() => Cow::Owned(format!("\\{byte:03o}")),
        b if b == b' ' || b.is_ascii_graphic() => Cow::Owned(char::from(byte).to_string()),
        _ => Cow::Owned(format!("\\{byte:03o}")),
    }
}

/// Return the list command rendering for one UTF-8 character.
fn readable_char(ch: char) -> Cow<'static, str> {
    if ch.is_ascii() {
        return readable_ascii_byte(ch as u8);
    }
    if (ch as u32) <= 0xFFFF {
        Cow::Owned(format!("\\u{:04X}", ch as u32))
    } else {
        Cow::Owned(format!("\\U{:08X}", ch as u32))
    }
}

/// Buffered state for rendering one list command output line.
struct ListLine {
    buffer: String,
    width: usize,
    max_width: usize,
}

impl ListLine {
    /// Create an empty list output line with the specified maximum width.
    fn new(max_width: usize) -> Self {
        Self {
            buffer: String::new(),
            width: 0,
            max_width,
        }
    }

    /// Write a rendered list item, folding before the item if needed.
    fn write_item(&mut self, output: &mut OutputBuffer, out_str: &str) -> UResult<()> {
        let out_len = out_str.len();
        // A width of 0 (`l 0`, `-l 0`) never wraps, as in GNU sed.
        if self.max_width > 0 && self.width + out_len + 1 > self.max_width {
            self.buffer.push_str("\\\n");
            output.write_str(std::mem::take(&mut self.buffer))?;
            self.width = 0;
        }
        self.buffer.push_str(out_str);
        self.width += out_len;
        Ok(())
    }

    /// Write the current list line with an embedded newline marker.
    fn write_embedded_newline(&mut self, output: &mut OutputBuffer) -> UResult<()> {
        self.buffer.push_str("$\n");
        output.write_str(&self.buffer)?;
        self.buffer.clear();
        self.width = 0;
        Ok(())
    }

    /// Finish the list line if it has buffered output.
    fn finish(&mut self, output: &mut OutputBuffer) -> UResult<()> {
        if !self.buffer.is_empty() {
            self.buffer.push_str("$\n");
            output.write_str(&self.buffer)?;
            self.buffer.clear();
            self.width = 0;
        }
        Ok(())
    }
}

/// List the passed pattern space in unambiguous form.
fn list(
    output: &mut OutputBuffer,
    line: &IOChunk,
    max_width: usize,
    context: &ProcessingContext,
) -> UResult<()> {
    // Special case for an empty pattern space
    if line.is_empty() {
        if line.is_newline_terminated() {
            output.write_str("$\n")?;
        }
        return Ok(());
    }

    let mut list_line = ListLine::new(max_width);

    if !context.uutil_extensions || context.character_mode == CharacterMode::Byte {
        // List non-ASCII bytes in octal.
        for &byte in line.as_bytes() {
            if byte == b'\n' {
                list_line.write_embedded_newline(output)?;
                continue;
            }
            let out_str = readable_ascii_byte(byte);
            list_line.write_item(output, &out_str)?;
        }
    } else {
        // List non-ASCII 8-bit characters in octal; Unicode in hex \u or \U. Bytes
        // that are not UTF-8 are listed in octal; they were an error.
        for chunk in line.as_bytes().utf8_chunks() {
            for ch in chunk.valid().chars() {
                if ch == '\n' {
                    list_line.write_embedded_newline(output)?;
                    continue;
                }
                let out_str = readable_char(ch);
                list_line.write_item(output, &out_str)?;
            }
            for &byte in chunk.invalid() {
                list_line.write_item(output, &readable_ascii_byte(byte))?;
            }
        }
    }

    list_line.finish(output)
}

/// Handle address 0 read at the beginning of each file.
fn process_address_0(
    commands: Option<Rc<RefCell<Command>>>,
    output: &mut OutputBuffer,
    stdin_done: bool,
) -> UResult<()> {
    // Prescan for zero-address which must produce output
    // before any input line is read.
    {
        let mut current = commands;
        while let Some(cmd_rc) = current {
            let next = {
                let cmd = cmd_rc.borrow();

                if cmd.code == 'r'
                    && matches!(cmd.addr1, Some(Address::Line(0)))
                    && cmd.addr2.is_none()
                {
                    let path = extract_variant!(cmd, Path);
                    append_file(output, path, stdin_done)?;
                }

                cmd.next.clone()
            };
            current = next;
        }
    }
    Ok(())
}

/// Process a single input file
fn process_file(
    commands: Option<Rc<RefCell<Command>>>,
    reader: &mut LineReader,
    output: &mut OutputBuffer,
    context: &mut ProcessingContext,
) -> UResult<()> {
    process_address_0(commands.clone(), output, context.stdin_done)?;

    // Loop over the input lines as pattern space.
    'lines: while let Some(mut pattern) = reader
        .get_line()
        .map_err(|e| read_error(&context.input_name, &e))?
    {
        context.line_number += 1;
        context.substitution_made = false;
        // Set the script command from which to start.
        let mut current: Option<Rc<RefCell<Command>>> =
            if let Some(action) = context.input_action.take() {
                // Continue processing the `N` command. What the cycle queued (`a`, `r`,
                // `R`) goes out now that `N` has its line, as in GNU sed; it went out
                // before `N` looked, so at the end of the input it came before the
                // pattern space `N` prints there.
                flush_appends(output, context)?;
                let mut combined_lines = action.prepend;
                combined_lines.push(context.delimiter());
                combined_lines.extend_from_slice(pattern.as_bytes());

                pattern.set_to_bytes(combined_lines, pattern.is_newline_terminated());
                action.next_command
            } else {
                // Start from the script top.
                commands.clone()
            };

        // Loop over script commands.
        while let Some(command_rc) = current.take() {
            let mut command = command_rc.borrow_mut();

            if !applies(&mut command, reader, &mut pattern, context)? {
                // Advance to next command
                current.clone_from(&command.next);
                continue;
            }

            match command.code {
                '{' => {
                    // Block begin; start processing the enclosed ones.
                    let body = extract_variant!(command, BranchTarget);
                    current.clone_from(body);
                    continue;
                }
                '}' => {
                    // Block end: continue with the block's patched next.
                }
                'a' => {
                    // Write the text to standard output at a later point.
                    let text = extract_variant!(command, Text);
                    context
                        .append_elements
                        .push(AppendElement::Text(text.clone()));
                }
                'b' => {
                    // Branch to the specified label or end if none is given.
                    let target = extract_variant!(command, BranchTarget);
                    if target.is_some() {
                        // New command to execute
                        current.clone_from(target);
                        continue;
                    }
                    // Branch to the end of the script.
                    break;
                }
                'c' => {
                    // Replace the pattern space with the text unless the command's range
                    // is still open after this line, as GNU sed has it: once at a range's
                    // end, on every line of a single address or a negated range (`2,4!c X`
                    // missed line 1), and not at all for a range left open at the end of
                    // the input. Then start the next cycle.
                    pattern.clear();
                    if command.start_line.is_none() {
                        let text = extract_variant!(command, Text);
                        output.write_bytes(text.as_ref())?;
                    }
                    break;
                }
                'd' => {
                    // Delete the pattern space and start the next cycle.
                    pattern.clear();
                    break;
                }
                'D' => {
                    // Delete up to \n and start a new cycle without new input.
                    if let Some(pos) = memchr(context.delimiter(), pattern.as_bytes()) {
                        let (s, _) = pattern.fields_mut()?;
                        s.drain(..=pos);
                        current.clone_from(&commands);
                        continue;
                    }
                    // Same as d
                    pattern.clear();
                    break;
                }
                'e' => match &command.data {
                    CommandData::None => {
                        execute_pattern_as_shell_command(&mut pattern)?;
                    }
                    CommandData::Text(cmd_bytes) => {
                        let shell_out = shell_stdout(cmd_bytes)?;
                        output.write_bytes(&shell_out)?;
                    }
                    _ => {
                        return runtime_error("INTERNAL ERROR: invalid 'e' command data");
                    }
                },
                'F' => {
                    // Output current input file name.
                    let mut bytes = context.input_name.as_os_str().as_encoded_bytes().to_vec();
                    bytes.push(b'\n');
                    output.write_bytes(&bytes)?;
                }
                'g' => {
                    // Replace pattern with the contents of the hold space.
                    pattern.set_to_bytes(context.hold.content.clone(), context.hold.has_newline);
                }
                'G' => {
                    // Append to pattern \n followed by hold space contents.
                    let (pat_content, pat_has_newline) = pattern.fields_mut()?;
                    pat_content.push(context.delimiter());
                    pat_content.extend_from_slice(&context.hold.content);
                    *pat_has_newline = context.hold.has_newline;
                }
                'h' => {
                    // Replace hold with the contents of the pattern space.
                    context.hold.content = pattern.as_bytes().to_vec();
                    context.hold.has_newline = pattern.is_newline_terminated();
                }
                'H' => {
                    // Append to hold \n followed by pattern space contents.
                    let delimiter = context.delimiter();
                    context.hold.content.push(delimiter);
                    context.hold.content.extend_from_slice(pattern.as_bytes());
                    context.hold.has_newline = pattern.is_newline_terminated();
                }
                'i' => {
                    // Write text to standard output.
                    let text = extract_variant!(command, Text);
                    output.write_bytes(text.as_ref())?;
                }
                'l' => {
                    let width = *extract_variant!(command, Number);
                    list(output, &pattern, width, context)?;
                }
                'n' => {
                    // The pattern space goes out before what the cycle queued, as at the
                    // end of a cycle and in GNU sed; it went out after.
                    if !context.quiet {
                        write_chunk(output, context, &pattern)?;
                    }
                    flush_appends(output, context)?;
                    if let Some(next_line) = reader
                        .get_line()
                        .map_err(|e| read_error(&context.input_name, &e))?
                    {
                        pattern = next_line;
                        context.line_number += 1;
                    } else {
                        context.stop_processing = true;
                        pattern.clear();
                        break 'lines;
                    }
                }
                'N' => {
                    // Append to pattern `\n` and the next line
                    // Rather than reading input here, which would result
                    // in a double borrow on reader, modify the action
                    // to perform when the next line is read.
                    context.input_action = Some(InputAction {
                        next_command: command.next.clone(),
                        prepend: pattern.as_bytes().to_vec(),
                    });
                    continue 'lines;
                }
                'p' => {
                    write_chunk(output, context, &pattern)?;
                }
                'P' => {
                    let line = pattern.as_bytes();
                    if let Some(pos) = memchr(context.delimiter(), line) {
                        output.write_bytes(&line[..=pos])?;
                    } else {
                        write_chunk(output, context, &pattern)?;
                    }
                }
                'q' => {
                    // Quit after printing the pattern space.
                    set_exit_code(
                        i32::try_from(*extract_variant!(command, Number)).unwrap_or(i32::MAX),
                    );
                    context.stop_processing = true;
                    break;
                }
                'Q' => {
                    // Quit immediatelly.
                    set_exit_code(
                        i32::try_from(*extract_variant!(command, Number)).unwrap_or(i32::MAX),
                    );
                    context.stop_processing = true;
                    context.quiet = true;
                    break;
                }
                'r' => {
                    // Copy the file to standard output at a later point.
                    let path = extract_variant!(command, Path);
                    context
                        .append_elements
                        .push(AppendElement::Path(path.clone()));
                }
                'R' => {
                    // Queue the file's next line, if it has one, for the end of the cycle.
                    let CommandData::LineFile(file) = &command.data else {
                        return runtime_error("INTERNAL ERROR: expected LineFile command data");
                    };
                    let path = context
                        .line_files
                        .iter()
                        .find(|(_, shared)| Rc::ptr_eq(shared, file))
                        .map(|(path, _)| path.clone())
                        .unwrap_or_default();
                    queue_line_of(file, &path, context)?;
                }
                's' => {
                    substitute(&mut pattern, &command, context, output)?;
                }
                't' if !context.substitution_made => { /* Do nothing. */ }
                't' => {
                    // Branch to the specified label or end if none is given
                    // if a substitution was made since last cycle or t.
                    let target = extract_variant!(command, BranchTarget);
                    context.substitution_made = false;
                    if target.is_some() {
                        // New command to execute
                        current.clone_from(target);
                        continue;
                    }
                    // Branch to the end of the script.
                    break;
                }
                'T' if context.substitution_made => {
                    // A substitution was made since the last cycle or T,
                    // so reset the flag and fall through without branching.
                    context.substitution_made = false;
                }
                'T' => {
                    // Branch to the specified label or end if none is given
                    // if no substitution was made since last cycle or T.
                    let target = extract_variant!(command, BranchTarget);
                    if target.is_some() {
                        // New command to execute
                        current.clone_from(target);
                        continue;
                    }
                    // Branch to the end of the script.
                    break;
                }
                'w' => {
                    // Append the pattern space to the specified file.
                    let writer = extract_variant!(command, NamedWriter);
                    write_to_file(
                        writer,
                        pattern.as_bytes(),
                        pattern.is_newline_terminated(),
                        output,
                        context,
                    )?;
                }
                'W' => {
                    // Append only the first line of the pattern space.
                    let writer = extract_variant!(command, NamedWriter);
                    let pattern_bytes = pattern.as_bytes();
                    let (first_line, found_newline) =
                        match pattern_bytes.iter().position(|&b| b == context.delimiter()) {
                            // A slice including the newline
                            Some(pos) => (&pattern_bytes[..=pos], true),
                            None => (pattern_bytes, false),
                        };
                    write_to_file(
                        writer,
                        first_line,
                        !found_newline && pattern.is_newline_terminated(),
                        output,
                        context,
                    )?;
                }
                'x' => {
                    // Exchange the contents of the pattern and hold spaces.
                    let (pat_content, pat_has_newline) = pattern.fields_mut()?;

                    // Swap newline if hold space is logically non-empty.
                    if !context.hold.content.is_empty() || context.hold.has_newline {
                        std::mem::swap(pat_has_newline, &mut context.hold.has_newline);
                    }
                    std::mem::swap(pat_content, &mut context.hold.content);
                }
                'y' => {
                    let trans = extract_variant!(command, Transliteration);
                    transliterate(&mut pattern, trans, context);
                }
                'z' => {
                    // Clear the pattern contents, but preserve newline state
                    // so automatic printing still emits an empty record.
                    let (pat_content, _) = pattern.fields_mut()?;
                    pat_content.clear();
                }
                ':' => {
                    // Branch target; do nothing.
                }
                'v' => {
                    // The version was checked when the script was compiled; it stopped
                    // the run as an internal error.
                }
                '=' => {
                    // Output current line number.
                    output.write_str(format!("{}\n", context.line_number))?;
                }
                // The compilation should supply only valid codes.
                c => {
                    return runtime_error(format!("INTERNAL ERROR: bad command '{c}'"));
                }
            } // match
            // Advance to next command.
            current.clone_from(&command.next);
        }

        if !context.quiet {
            write_chunk(output, context, &pattern)?;
        }

        flush_appends(output, context)?;

        if context.stop_processing {
            output.flush_pending_newline()?;
            break;
        }
    }

    // Handle any N command remains.
    if context.separate
        && let Some(action) = context.input_action.take()
    {
        if !context.quiet {
            let mut pending = action.prepend;
            pending.push(b'\n');
            output.write_bytes(&pending)?;
        }
        flush_appends(output, context)?;
        if context.unbuffered {
            output.flush()?;
        }
    }

    Ok(())
}

/// Mark all address ranges non-active (and 0-starting ones as active).
fn reset_latched_address_ranges(range_commands: &mut [Rc<RefCell<Command>>]) {
    for cmd_rc in range_commands.iter() {
        let mut cmd = cmd_rc.borrow_mut();

        cmd.start_line =
            // Check for address-spec line 0 pre-latch extension.
            if let Some(addr1) = &cmd.addr1 && matches!(addr1, Address::Line(0)) {
                Some(0)
            } else {
                None
            };
    }
}

/// Open the input file `path`, as GNU sed opens one.
///
/// One that cannot be opened is reported, `can't read F: No such file or directory`,
/// and passed over, and sed ends with status 2: `None`. A directory, which GNU sed opens
/// and then fails to read, is its "read error on F: Is a directory", and one an
/// in-place edit refuses, "couldn't edit F: not a regular file", status 4. cash's sed
/// ended at the first file it could not open, with its own words and status 1.
///
/// With `--follow-symlinks` a file that is not there is GNU sed's "couldn't readlink F:
/// ...", status 4, before it is opened.
fn open_input(
    path: &PathBuf,
    index: usize,
    context: &ProcessingContext,
) -> UResult<Option<LineReader>> {
    if let Some(reader) = context.read_ahead.take(index) {
        return Ok(Some(reader));
    }
    if context.follow_symlinks
        && path.as_os_str() != "-"
        && !is_pipe_path(path)
        && let Err(error) = std::fs::symlink_metadata(path)
    {
        return runtime_error(format!(
            "couldn't readlink {}: {}",
            path.display(),
            strerror(&error)
        ));
    }
    if path.as_os_str() != "-" && is_directory(path) {
        return runtime_error(if context.in_place {
            format!("couldn't edit {}: not a regular file", path.display())
        } else {
            format!("read error on {}: Is a directory", path.display())
        });
    }
    match LineReader::open_with(path, context.treats_cr_as_data()) {
        Ok(reader) => Ok(Some(reader)),
        Err(error) => {
            uucore::show_error!("can't read {}: {}", path.display(), strerror(&error));
            Ok(None)
        }
    }
}

/// Process all input files
pub fn process_all_files(
    commands: Option<Rc<RefCell<Command>>>,
    files: Vec<PathBuf>,
    context: &mut ProcessingContext,
) -> UResult<()> {
    context.unbuffered = context.unbuffered || io::stdout().is_terminal();

    let mut in_place = InPlace::new(context.clone());
    let last_file_index = files.len() - 1;
    // Whether a file has been read, so that one that could not be is passed over.
    let mut read_one = false;
    // Whether a file could not be read: sed then ends with status 2, as GNU sed does,
    // whatever status `q` gave.
    let mut unreadable = false;

    for (index, path) in files.iter().enumerate() {
        context.last_file = index == last_file_index;
        context.later_files = files.get(index + 1..).unwrap_or_default().to_vec();
        context.later_start = index + 1;
        context.later_input = None;
        let Some(mut reader) = open_input(path, index, context)? else {
            unreadable = true;
            continue;
        };
        reader.set_delimiter(context.delimiter());
        let output = in_place.begin(path)?;
        output.set_delimiter(context.delimiter());

        if context.separate && read_one {
            rewind_line_files(context)?;
        }
        if context.separate || !read_one {
            context.line_number = 0;
            reset_latched_address_ranges(&mut context.range_commands);

            // Reset hold space for separate file processing
            context.hold.content.clear();
            context.hold.has_newline = true;
        }
        read_one = true;

        context.input_name = path.clone();
        process_file(commands.clone(), &mut reader, output, context)?;
        if path.as_os_str() == "-" {
            context.stdin_done = true;
        }

        // The input is closed before an in-place edit replaces it: Windows does not move
        // one file over another that is still open.
        drop(reader);
        in_place.end()?;

        if context.stop_processing {
            break;
        }
    }

    // An `N` on the last line of the input has no line to append: its pattern space is
    // printed, as in GNU sed. It is found here, past the files that follow and have no
    // lines, not on the last file with lines alone.
    if !context.separate
        && let Some(action) = context.input_action.take()
    {
        let output = &mut in_place.output;
        if !context.quiet {
            let mut pending = action.prepend;
            pending.push(b'\n');
            output.write_bytes(&pending)?;
        }
        flush_appends(output, context)?;
        output.flush()?;
    }

    // Flush all output files
    named_writer::flush_all()?;

    if unreadable {
        set_exit_code(2);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom};
    use tempfile::tempfile;

    // Inside cash, `e` runs its command in cash, this process's own exe, whatever its
    // file is named: this test's exe is not named `cash`, as a linked `sed.exe` is not,
    // and the commands went to `cmd` (TODO.md 14.6). `CASH_ARGV0` tells a cash whose exe
    // is such a link that it runs as the shell.
    #[test]
    fn test_shell_commands_run_in_this_process_exe() {
        let command = shell_command(OsStr::new("echo hi"));
        assert_eq!(
            std::path::Path::new(command.get_program()),
            std::env::current_exe().unwrap()
        );
        assert!(
            command
                .get_args()
                .eq(["--norc", "--noprofile", "-c", "echo hi"])
        );
        assert!(
            command
                .get_envs()
                .any(|(name, value)| name == "CASH_ARGV0" && value.is_some_and(|v| v == "cash"))
        );
    }

    #[test]
    fn test_readable_ascii_byte_named_escapes() {
        assert_eq!(readable_ascii_byte(b'\n'), r"\012");
        assert_eq!(readable_ascii_byte(b'\t'), r"\t");
        assert_eq!(readable_ascii_byte(b'\\'), r"\\");
    }

    #[test]
    fn test_readable_ascii_byte_printable_and_non_ascii() {
        assert_eq!(readable_ascii_byte(b'A'), "A");
        assert_eq!(readable_ascii_byte(b' '), " ");
        assert_eq!(readable_ascii_byte(0xE9), r"\351");
    }

    #[test]
    fn test_readable_char_ascii_delegates_to_byte() {
        assert_eq!(readable_char('\t'), r"\t");
        assert_eq!(readable_char('A'), "A");
    }

    #[test]
    fn test_readable_char_unicode_escapes() {
        assert_eq!(readable_char('κ'), r"\u03BA");
        assert_eq!(readable_char('😀'), r"\U0001F600");
    }

    #[test]
    fn test_write_list_item_appends_without_fold() {
        let mut file = tempfile().unwrap();
        let mut output = OutputBuffer::new(Box::new(file.try_clone().unwrap()));
        let mut line = ListLine::new(10);
        line.write_item(&mut output, "ab").unwrap();

        line.write_item(&mut output, "cd").unwrap();
        output.flush().unwrap();

        assert_eq!(line.buffer, "abcd");
        assert_eq!(line.width, 4);
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut written = String::new();
        file.read_to_string(&mut written).unwrap();
        assert_eq!(written, "");
    }

    #[test]
    fn test_write_list_item_folds_before_append() {
        let mut file = tempfile().unwrap();
        let mut output = OutputBuffer::new(Box::new(file.try_clone().unwrap()));
        let mut line = ListLine::new(5);
        line.write_item(&mut output, "abcd").unwrap();

        line.write_item(&mut output, "e").unwrap();
        output.flush().unwrap();

        assert_eq!(line.buffer, "e");
        assert_eq!(line.width, 1);
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut written = String::new();
        file.read_to_string(&mut written).unwrap();
        assert_eq!(written, "abcd\\\n");
    }

    #[test]
    fn test_list_line_finish_writes_terminator() {
        let mut file = tempfile().unwrap();
        let mut output = OutputBuffer::new(Box::new(file.try_clone().unwrap()));
        let mut line = ListLine::new(10);
        line.write_item(&mut output, "abc").unwrap();

        line.finish(&mut output).unwrap();
        output.flush().unwrap();

        assert_eq!(line.buffer, "");
        assert_eq!(line.width, 0);
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut written = String::new();
        file.read_to_string(&mut written).unwrap();
        assert_eq!(written, "abc$\n");
    }
}
