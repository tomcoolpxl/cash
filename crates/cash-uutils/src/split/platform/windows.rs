// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use crate::messages::translate;
use crate::split::MESSAGES;
use crate::split::platform::Writer;
use std::ffi::OsStr;
use std::io::{Error, ErrorKind, Result, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use uucore::display::Quotable;
use uucore::error::USimpleError;
use uucore::fs;
use uucore::show;

/// A writer that writes to a shell's standard input, the shell running `--filter`'s
/// command with `$FILE` the piece's name, as GNU split runs it.
///
/// cash: the shell is cash, this process's own exe, as for awk's `system()`: split runs
/// inside cash, as a bundled tool (`cash --invoke-bundled split`) or a link `cash
/// --link-tools` made (`split.exe`), whom `CASH_ARGV0` tells it was started as the
/// shell. uutils refused `--filter` on Windows, where std cannot set a variable for a
/// child alone the way its Unix writer set `FILE` for the whole process; `Command::env`
/// sets it for the child.
pub struct FilterWriter {
    /// The running shell.
    shell_process: Child,
}

impl Write for FilterWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.stdin()?.write(buf)
    }
    fn flush(&mut self) -> Result<()> {
        self.stdin()?.flush()
    }
}

impl FilterWriter {
    /// Starts the shell running `command` with `$FILE` set to `filepath`.
    fn new(command: &str, filepath: &OsStr) -> Result<Self> {
        let cash = std::env::current_exe().unwrap_or_else(|_| "cash".into());
        let shell_process = cash_win32::spawn::spawn(
            Command::new(cash)
                .env("CASH_ARGV0", "cash")
                .env("FILE", filepath)
                .args(["--norc", "--noprofile", "-c", command])
                .stdin(Stdio::piped()),
        )?;
        Ok(Self { shell_process })
    }

    fn stdin(&mut self) -> Result<&mut std::process::ChildStdin> {
        self.shell_process
            .stdin
            .as_mut()
            .ok_or_else(|| Error::from(ErrorKind::BrokenPipe))
    }
}

impl Drop for FilterWriter {
    /// Closes the shell's standard input and waits for it, saying how it ended when that
    /// was not with status 0.
    fn drop(&mut self) {
        drop(self.shell_process.stdin.take());
        match self.shell_process.wait() {
            Ok(status) if status.success() => {}
            Ok(status) => {
                if let Some(code) = status.code() {
                    show!(USimpleError::new(
                        1,
                        translate!("split-error-shell-process-returned", "code" => code)
                    ));
                } else {
                    show!(USimpleError::new(
                        1,
                        translate!("split-error-shell-process-terminated")
                    ));
                }
            }
            Err(error) => show!(USimpleError::new(1, error.to_string())),
        }
    }
}

/// A writer for the piece `filename`: the file, or with `filter` a shell that runs it.
pub fn instantiate_current_writer(
    filter: Option<&str>,
    input: &OsStr,
    filename: &OsStr,
    is_new: bool,
) -> Result<Writer> {
    if let Some(command) = filter {
        return Ok(Writer::Filter(FilterWriter::new(command, filename)?));
    }
    let file = if is_new {
        create_or_truncate_output_file(input, filename)?
    } else {
        // re-open file that we previously created to append to it
        let file = std::fs::OpenOptions::new()
            .append(true)
            .open(Path::new(filename))
            .map_err(|_| {
                Error::other(
                    translate!("split-error-unable-to-reopen-file", "file" => filename.quote()),
                )
            })?;

        if input_and_output_refer_to_same_file(input, &file) {
            return Err(Error::other(
                translate!("split-error-would-overwrite-input", "file" => filename.quote()),
            ));
        }

        file
    };
    Ok(Writer::File(file))
}

fn create_or_truncate_output_file(input: &OsStr, filename: &OsStr) -> Result<std::fs::File> {
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(filename))
    {
        Ok(file) => Ok(file),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(Path::new(filename))
                .map_err(|e| open_file_error(filename, e))?;

            if input_and_output_refer_to_same_file(input, &file) {
                return Err(Error::other(
                    translate!("split-error-would-overwrite-input", "file" => filename.quote()),
                ));
            }

            file.set_len(0).map_err(|e| open_file_error(filename, e))?;
            Ok(file)
        }
        Err(e) => Err(open_file_error(filename, e)),
    }
}

fn open_file_error(filename: &OsStr, e: Error) -> Error {
    let e = uucore::error::strip_errno(&e);
    Error::other(format!("{}: {e}", filename.quote()))
}

fn input_and_output_refer_to_same_file(input: &OsStr, output: &std::fs::File) -> bool {
    let input_info = if input == "-" {
        fs::FileInformation::from_file(&std::io::stdin())
    } else {
        fs::FileInformation::from_path(Path::new(input), true)
    };

    fs::infos_refer_to_same_file(input_info, fs::FileInformation::from_file(output))
}
pub fn paths_refer_to_same_file(p1: &OsStr, p2: &OsStr) -> bool {
    // Windows doesn't support many of the unix ways of paths being equals
    fs::paths_refer_to_same_file(Path::new(p1), Path::new(p2), true)
}
