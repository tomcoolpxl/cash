//! `rev`, util-linux's: each line's characters in reverse order.
//!
//! Checked against util-linux 2.42.3 (`crates/cash/tests/oracle`). Characters are code
//! points, as util-linux reverses them; bytes that are not UTF-8 are reversed one by one
//! rather than lost. `-0`/`--zero` separates records with NUL instead of newline, and a
//! last record without its separator is written without one. A missing file is reported
//! and the rest are still read, with status 1; `-` is a file name here, as in util-linux.
//!
//! One difference, deliberate (D20): a line ending in CRLF keeps its `\r` at the end.
//! util-linux reverses the `\r` to the front, which turns `rev | cut | rev` on a Windows
//! text file into lines that start with a carriage return.

use std::io::{Read, Write};

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Reverse the characters of each line.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct RevCommand {
    /// Options and files, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// `line` with its characters in reverse order; `line` excludes its separator.
fn reverse_record(line: &[u8], out: &mut Vec<u8>) {
    let (body, cr) = match line.split_last() {
        Some((b'\r', body)) => (body, true),
        _ => (line, false),
    };
    // The units to reverse: each valid character's bytes, or one invalid byte.
    let mut units: Vec<&[u8]> = Vec::with_capacity(body.len());
    for chunk in body.utf8_chunks() {
        let valid = chunk.valid();
        let mut start = 0;
        for c in valid.chars() {
            let len = c.len_utf8();
            units.push(&valid.as_bytes()[start..start + len]);
            start += len;
        }
        for byte in chunk.invalid().chunks(1) {
            units.push(byte);
        }
    }
    for unit in units.iter().rev() {
        out.extend_from_slice(unit);
    }
    if cr {
        out.push(b'\r');
    }
}

/// `input` with each record reversed, keeping separators where they were.
fn reverse_all(input: &[u8], separator: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut records = input.split(|&b| b == separator).peekable();
    while let Some(record) = records.next() {
        let last = records.peek().is_none();
        if last && record.is_empty() {
            break;
        }
        reverse_record(record, &mut out);
        if !last {
            out.push(separator);
        }
    }
    out
}

impl builtins::Command for RevCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut separator = b'\n';
        let mut files = Vec::new();
        let mut only_files = false;
        for arg in &self.args {
            if only_files || arg == "-" || !arg.starts_with('-') {
                files.push(arg.as_str());
                continue;
            }
            match arg.as_str() {
                "--" => only_files = true,
                "-0" | "--zero" => separator = 0,
                "-h" | "--help" => {
                    writeln!(
                        context.stdout(),
                        "Usage:\n rev [options] [<file> ...]\n\nReverse lines characterwise.\n\n\
                         Options:\n -0, --zero     zero line delimiter\n -h, --help     display this help\n"
                    )?;
                    return Ok(ExecutionResult::success());
                }
                other => {
                    let shown = other.strip_prefix("--").map_or_else(
                        || {
                            std::format!(
                                "invalid option -- '{}'",
                                other.chars().nth(1).unwrap_or('-')
                            )
                        },
                        |long| std::format!("unrecognized option '--{long}'"),
                    );
                    writeln!(
                        context.stderr(),
                        "rev: {shown}\nTry 'rev --help' for more information."
                    )?;
                    return Ok(ExecutionResult::general_error());
                }
            }
        }

        let mut status = ExecutionResult::success();
        let mut stdout = context.stdout();
        if files.is_empty() {
            let mut input = Vec::new();
            context.stdin().read_to_end(&mut input)?;
            stdout.write_all(&reverse_all(&input, separator))?;
            return Ok(status);
        }
        // Written once at the end, as util-linux's buffered output is: into a pipe, an
        // error about a missing file comes before any of the output.
        let mut reversed = Vec::new();
        for file in files {
            let path = context.shell.absolute_path(std::path::Path::new(file));
            match std::fs::read(&path) {
                Ok(input) => reversed.extend(reverse_all(&input, separator)),
                Err(error) => {
                    let reason = if error.kind() == std::io::ErrorKind::NotFound {
                        "No such file or directory".to_owned()
                    } else {
                        error.to_string()
                    };
                    writeln!(context.stderr(), "rev: cannot open {file}: {reason}")?;
                    status = ExecutionResult::general_error();
                }
            }
        }
        stdout.write_all(&reversed)?;
        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::reverse_all;

    #[test]
    fn records_keep_their_separators() {
        assert_eq!(reverse_all(b"abc\nxyz\n", b'\n'), b"cba\nzyx\n");
        assert_eq!(reverse_all(b"abc\nxy", b'\n'), b"cba\nyx");
        assert_eq!(reverse_all(b"\n\nab\n", b'\n'), b"\n\nba\n");
        assert_eq!(reverse_all(b"", b'\n'), b"");
        assert_eq!(reverse_all(b"ab\0cd", 0), b"ba\0dc");
    }

    #[test]
    fn a_crlf_line_keeps_its_cr_at_the_end() {
        assert_eq!(reverse_all(b"abc\r\nxy\r\n", b'\n'), b"cba\r\nyx\r\n");
    }

    #[test]
    fn characters_not_bytes_and_invalid_bytes_survive() {
        assert_eq!(
            reverse_all("héllo wörld\n".as_bytes(), b'\n'),
            "dlröw olléh\n".as_bytes()
        );
        assert_eq!(reverse_all(b"a\xffb\n", b'\n'), b"b\xffa\n");
    }
}
