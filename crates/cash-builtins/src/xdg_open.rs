//! `xdg-open`: `start` under the name cross-platform scripts try first.
//!
//! A script that wants to open a file or a URL for the user probes for `xdg-open`
//! (Linux), `open` (macOS) and `start` (Windows), in that order more often than not, and
//! on a Windows without `xdg-open` falls through to `start` by way of cmd. Here the name
//! answers directly, with the one argument xdg-open 1.2.1 takes and its exit codes: 0
//! for a program started, 1 for a mistake on the command line, 2 for a file that does not
//! exist, 4 when Windows could not open the target. A URL is anything with a scheme of
//! two or more letters, so a drive letter is a path.

use std::io::Write;
use std::path::Path;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// What `--help` prints, and what a call without an argument gets on standard error.
const HELP: &str = "\
   xdg-open -- opens a file or URL in its default program

Synopsis

   xdg-open { file | URL }

   xdg-open { --help | --manual | --version }

Use 'help xdg-open' or 'xdg-open --manual' for additional info.
";

/// What `--manual` adds to the help: the exit codes.
const MANUAL: &str = "
Exit Codes

   0      the program was started
   1      error in command line syntax
   2      the file passed on the command line does not exist
   4      the action failed: Windows has no program for the target, or it refused
";

/// Open a file, folder or URL with its default program; `start`, under the name
/// cross-platform scripts try first.
#[derive(Parser)]
#[command(
    disable_help_flag = true,
    disable_version_flag = true,
    override_usage = "xdg-open { FILE | URL }\n       xdg-open { --help | --manual | --version }"
)]
pub(crate) struct XdgOpenCommand {
    /// The file or URL, or one of `--help`, `--manual` and `--version`.
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "FILE-OR-URL"
    )]
    args: Vec<String>,
}

/// Whether `target` is a URL or another scheme (`mailto:`, `ms-settings:`), which goes to
/// Windows as written, rather than a path.
///
/// A scheme is two or more letters before the colon, so `C:/x` and `c:\x` are paths.
fn is_url(target: &str) -> bool {
    let Some((scheme, _)) = target.split_once(':') else {
        return false;
    };
    scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

impl builtins::Command for XdgOpenCommand {
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
        let mut target: Option<&str> = None;
        for arg in &self.args {
            match arg.as_str() {
                "--help" | "-h" => {
                    write!(context.stdout(), "{HELP}")?;
                    return Ok(ExecutionResult::success());
                }
                "--manual" => {
                    write!(context.stdout(), "{HELP}{MANUAL}")?;
                    return Ok(ExecutionResult::success());
                }
                "--version" => {
                    writeln!(
                        context.stdout(),
                        "xdg-open (cash): start, under the name cross-platform scripts try first"
                    )?;
                    return Ok(ExecutionResult::success());
                }
                option if option.starts_with('-') && option.len() > 1 => {
                    writeln!(
                        context.stderr(),
                        "xdg-open: unexpected option '{option}'\n\
                         Try 'xdg-open --help' for more information."
                    )?;
                    return Ok(ExecutionResult::new(1));
                }
                word => {
                    if target.is_some() {
                        writeln!(
                            context.stderr(),
                            "xdg-open: unexpected argument '{word}'\n\
                             Try 'xdg-open --help' for more information."
                        )?;
                        return Ok(ExecutionResult::new(1));
                    }
                    target = Some(word);
                }
            }
        }
        let Some(target) = target else {
            write!(context.stderr(), "{HELP}")?;
            return Ok(ExecutionResult::new(1));
        };

        if !is_url(target) && !context.shell.absolute_path(Path::new(target)).exists() {
            writeln!(context.stderr(), "xdg-open: file '{target}' does not exist")?;
            return Ok(ExecutionResult::new(2));
        }
        match crate::win::open_with_default_program(context.shell, target) {
            Ok(()) => Ok(ExecutionResult::success()),
            Err(error) => {
                writeln!(
                    context.stderr(),
                    "xdg-open: {target}: {}",
                    cash_core::error::os_error_text(&error)
                )?;
                Ok(ExecutionResult::new(4))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_url;

    #[test]
    fn a_scheme_is_two_letters_or_more_so_a_drive_is_a_path() {
        assert!(is_url("https://example.com/?a=1&b=2"));
        assert!(is_url("mailto:someone@example.com"));
        assert!(is_url("ms-settings:display"));
        assert!(is_url("file:///C:/x.txt"));
        assert!(!is_url("C:/Users/x/report.pdf"));
        assert!(!is_url(r"c:\Users\x"));
        assert!(!is_url("report.pdf"));
        assert!(!is_url("./notes:today.txt"));
        assert!(!is_url("1a:b"));
    }
}
