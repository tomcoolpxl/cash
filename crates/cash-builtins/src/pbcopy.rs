//! `pbcopy` and `pbpaste`, macOS's names for the clipboard, on Windows'.
//!
//! `pbcopy` reads standard input to its end and puts it on the clipboard as text;
//! `pbpaste` writes the clipboard's text to standard output. Between the two, line
//! endings change as they must: a Windows program expects CRLF on the clipboard and puts
//! CRLF there, a script works in LF. So `pbcopy` makes every lone LF a CRLF, and
//! `pbpaste` makes every CRLF an LF, so that `pbpaste | wc -l` and `cat file | pbcopy`
//! both do what they do on a Mac. Standard input is read as UTF-8, a leading byte order
//! mark dropped and an invalid byte replaced; the clipboard holds UTF-16, which every
//! Windows program reads. An empty input empties the clipboard, as on macOS.
//!
//! The pasteboard names (`-pboard general|ruler|find|font`) and `-Prefer` are macOS's
//! and accepted so scripts written there run; Windows has one clipboard, with text.

use std::io::{Read, Write};

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// The pasteboards macOS has; Windows has one, and the others are taken as it.
const PASTEBOARDS: [&str; 4] = ["general", "ruler", "find", "font"];
/// What `pbpaste -Prefer` may ask for; only text exists here.
const PREFERENCES: [&str; 3] = ["txt", "rtf", "ps"];

/// Copy standard input to the clipboard.
#[derive(Parser)]
#[command(
    disable_help_flag = true,
    disable_version_flag = true,
    override_usage = "pbcopy [-pboard {general | ruler | find | font}]"
)]
pub(crate) struct PbcopyCommand {
    /// The options, read here as macOS reads them (`-pboard NAME`).
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "OPTION"
    )]
    args: Vec<String>,
}

/// Write the clipboard's text to standard output.
#[derive(Parser)]
#[command(
    disable_help_flag = true,
    disable_version_flag = true,
    override_usage = "pbpaste [-pboard {general | ruler | find | font}] [-Prefer {txt | rtf | ps}]"
)]
pub(crate) struct PbpasteCommand {
    /// The options, read here as macOS reads them (`-pboard NAME`, `-Prefer TYPE`).
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "OPTION"
    )]
    args: Vec<String>,
}

/// What the options came to.
#[derive(Debug, PartialEq, Eq)]
enum Parsed {
    /// `-h`, `--help`: the usage, and nothing else.
    Help,
    /// `-V`, `--version`.
    Version,
    /// The clipboard, after any `-pboard` and `-Prefer`.
    Run,
}

/// The options read as macOS reads them: one-dash words with their value in the next
/// word (`-pboard general`), or after `=`.
///
/// `allow_prefer` is for `pbpaste`, the one with `-Prefer`.
fn parse(args: &[String], allow_prefer: bool) -> Result<Parsed, String> {
    let mut index = 0;
    while let Some(word) = args.get(index) {
        index += 1;
        let (name, inline) = match word.split_once('=') {
            Some((name, value)) => (name, Some(value.to_owned())),
            None => (word.as_str(), None),
        };
        let mut value = |what: &str| -> Result<String, String> {
            if let Some(value) = inline.clone() {
                return Ok(value);
            }
            let value = args
                .get(index)
                .cloned()
                .ok_or_else(|| format!("{name} needs {what}"))?;
            index += 1;
            Ok(value)
        };
        match name {
            "-h" | "--help" => return Ok(Parsed::Help),
            "-V" | "--version" => return Ok(Parsed::Version),
            "-pboard" | "--pboard" => {
                let board = value("a pasteboard name")?;
                if !PASTEBOARDS.contains(&board.as_str()) {
                    return Err(format!("unknown pasteboard '{board}'"));
                }
            }
            "-Prefer" | "--Prefer" if allow_prefer => {
                let kind = value("a type")?;
                if !PREFERENCES.contains(&kind.as_str()) {
                    return Err(format!("unknown type '{kind}'"));
                }
            }
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    Ok(Parsed::Run)
}

/// `input` as the text the clipboard gets: UTF-8 with its byte order mark dropped and
/// invalid bytes replaced, every lone LF made CRLF.
pub(crate) fn clipboard_text(input: &[u8]) -> String {
    let input = input.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(input);
    let text = String::from_utf8_lossy(input);
    let mut out = String::with_capacity(text.len() + text.len() / 16);
    let mut previous = '\0';
    for c in text.chars() {
        if c == '\n' && previous != '\r' {
            out.push('\r');
        }
        out.push(c);
        previous = c;
    }
    out
}

/// The clipboard's text as a script wants it: every CRLF an LF.
pub(crate) fn script_text(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// The usage, version or error of `who` written, with its status; `None` when the
/// clipboard is to be used.
fn preamble<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    who: &str,
    usage: &str,
    parsed: Result<Parsed, String>,
) -> Result<Option<ExecutionResult>, cash_core::Error> {
    match parsed {
        Ok(Parsed::Run) => Ok(None),
        Ok(Parsed::Help) => {
            writeln!(context.stdout(), "Usage: {usage}")?;
            Ok(Some(ExecutionResult::success()))
        }
        Ok(Parsed::Version) => {
            writeln!(context.stdout(), "{who} (cash)")?;
            Ok(Some(ExecutionResult::success()))
        }
        Err(message) => {
            writeln!(context.stderr(), "{who}: {message}\nUsage: {usage}")?;
            Ok(Some(ExecutionResult::general_error()))
        }
    }
}

/// A clipboard failure reported as `who`'s, with status 1.
fn clipboard_failed<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    who: &str,
    error: &std::io::Error,
) -> Result<ExecutionResult, cash_core::Error> {
    let reason = if error.kind() == std::io::ErrorKind::WouldBlock {
        error.to_string()
    } else {
        cash_core::error::os_error_text(error)
    };
    writeln!(context.stderr(), "{who}: {reason}")?;
    Ok(ExecutionResult::general_error())
}

impl builtins::Command for PbcopyCommand {
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
        let usage = "pbcopy [-pboard {general | ruler | find | font}]";
        if let Some(done) = preamble(&context, "pbcopy", usage, parse(&self.args, false))? {
            return Ok(done);
        }
        let mut input = Vec::new();
        context.stdin().read_to_end(&mut input)?;
        match cash_win32::clipboard::set_text(&clipboard_text(&input)) {
            Ok(()) => Ok(ExecutionResult::success()),
            Err(error) => clipboard_failed(&context, "pbcopy", &error),
        }
    }
}

impl builtins::Command for PbpasteCommand {
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
        let usage = "pbpaste [-pboard {general | ruler | find | font}] [-Prefer {txt | rtf | ps}]";
        if let Some(done) = preamble(&context, "pbpaste", usage, parse(&self.args, true))? {
            return Ok(done);
        }
        match cash_win32::clipboard::get_text() {
            Ok(Some(text)) => {
                let mut stdout = context.stdout();
                stdout.write_all(script_text(&text).as_bytes())?;
                stdout.flush()?;
                Ok(ExecutionResult::success())
            }
            Ok(None) => Ok(ExecutionResult::success()),
            Err(error) => clipboard_failed(&context, "pbpaste", &error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn lone_line_feeds_become_crlf_and_the_bom_goes() {
        assert_eq!(clipboard_text(b"a\nb\r\nc\n"), "a\r\nb\r\nc\r\n");
        assert_eq!(clipboard_text(b"\xEF\xBB\xBFx\n"), "x\r\n");
        assert_eq!(clipboard_text(b"a\xffb"), "a\u{fffd}b");
        assert_eq!(clipboard_text(b""), "");
        assert_eq!(clipboard_text(b"\n\n"), "\r\n\r\n");
    }

    #[test]
    fn crlf_becomes_lf_on_the_way_out() {
        assert_eq!(script_text("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(script_text("a\rb\n"), "a\rb\n");
    }

    #[test]
    fn macos_s_options_are_accepted_and_others_refused() {
        assert_eq!(parse(&words(""), false), Ok(Parsed::Run));
        assert_eq!(parse(&words("-pboard general"), false), Ok(Parsed::Run));
        assert_eq!(parse(&words("-pboard=find"), false), Ok(Parsed::Run));
        assert_eq!(parse(&words("-Prefer txt"), true), Ok(Parsed::Run));
        assert_eq!(parse(&words("-h"), false), Ok(Parsed::Help));
        assert_eq!(parse(&words("--version"), true), Ok(Parsed::Version));
        assert_eq!(
            parse(&words("-Prefer txt"), false),
            Err("unknown option '-Prefer'".to_owned())
        );
        assert_eq!(
            parse(&words("-pboard"), false),
            Err("-pboard needs a pasteboard name".to_owned())
        );
        assert_eq!(
            parse(&words("-pboard other"), false),
            Err("unknown pasteboard 'other'".to_owned())
        );
        assert_eq!(
            parse(&words("-Prefer doc"), true),
            Err("unknown type 'doc'".to_owned())
        );
        assert_eq!(
            parse(&words("-x"), true),
            Err("unknown option '-x'".to_owned())
        );
    }
}
