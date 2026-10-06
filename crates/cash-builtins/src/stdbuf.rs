//! `stdbuf`, coreutils': run a command with its standard streams' buffering changed. GNU's
//! works by preloading a library into the program, which Windows has no equivalent of,
//! so the command runs as it is:
//!
//! * a builtin or bundled tool of cash's own runs unchanged. cash's builtins write each
//!   line as they make it, and the bundled coreutils (`cat` among them) line-buffer
//!   even into a pipe; `grep` is given `--line-buffered`, which is what `-oL` or `-o0`
//!   asks of it. `awk` and `sed` fill a buffer into a pipe, as GNU's do, and `stdbuf`
//!   cannot change that for them either;
//! * an external program runs as it is, after one line on standard error saying so, and
//!   `stdbuf` exits with the program's status.
//!
//! The options are coreutils 9.x's (`-i`, `-o`, `-e` with a MODE of `L`, `0` or a size),
//! its messages and its status 125 for a usage error, 127 for a command not found.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Run a command with changed buffering; on Windows, as it is.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct StdbufCommand {
    /// Options and the command, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// The one line an external program gets.
pub(crate) const NOTE: &str =
    "cannot change the buffering of an external program on Windows; running it as is";

/// coreutils' status for a usage error.
const USAGE_ERROR: u8 = 125;

const HELP: &str = "\
Usage: stdbuf OPTION... COMMAND
Run COMMAND, with modified buffering operations for its standard streams.

Mandatory arguments to long options are mandatory for short options too.
  -i, --input=MODE   adjust standard input stream buffering
  -o, --output=MODE  adjust standard output stream buffering
  -e, --error=MODE   adjust standard error stream buffering
      --help     display this help and exit
      --version  output version information and exit

If MODE is 'L' the corresponding stream will be line buffered.
This option is invalid with standard input.

If MODE is '0' the corresponding stream will be unbuffered.

Otherwise MODE is a number which may be followed by one of the following:
KB 1000, K 1024, MB 1000*1000, M 1024*1024, and so on for G, T, P, E, Z, Y.
Binary prefixes can be used, too: KiB=K, MiB=M, and so on.
In this case the corresponding stream will be fully buffered with the buffer
size set to MODE bytes.

On Windows there is no way into another program's buffering, so an external
COMMAND runs as it is, after a note on standard error. cash's own builtins
and bundled tools write line by line into a pipe already; 'grep' is given
--line-buffered.";

/// Which stream an option names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stream {
    Input,
    Output,
    Error,
}

/// What the options asked, and the command.
#[derive(Debug, PartialEq, Eq)]
struct Parsed {
    /// Whether any stream's buffering was named.
    any_mode: bool,
    /// Whether standard output was asked for line buffering or none.
    output_by_line: bool,
    /// The command and its arguments.
    command: Vec<String>,
}

/// Why the options did not describe a run: the message after `stdbuf: `, and whether
/// to add `Try 'stdbuf --help'`.
#[derive(Debug, PartialEq, Eq)]
enum Refusal {
    Help,
    Version,
    Message(String, bool),
}

/// The size suffixes coreutils takes after a number.
const SUFFIXES: [&str; 25] = [
    "", "KB", "K", "KiB", "MB", "M", "MiB", "GB", "G", "GiB", "TB", "T", "TiB", "PB", "P", "PiB",
    "EB", "E", "EiB", "ZB", "Z", "ZiB", "YB", "Y", "YiB",
];

/// Whether `mode` is one coreutils accepts: `L`, `0`, or a size with an optional suffix.
fn valid_mode(mode: &str) -> bool {
    if mode == "L" || mode == "0" {
        return true;
    }
    let digits = mode.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let suffix = mode.get(digits.len()..).unwrap_or_default();
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) && SUFFIXES.contains(&suffix)
}

/// The options as `getopt_long("+i:o:e:")` reads them: they end at the first word that
/// is not one, which starts the command.
fn parse(args: &[String]) -> Result<Parsed, Refusal> {
    let mut parsed = Parsed {
        any_mode: false,
        output_by_line: false,
        command: Vec::new(),
    };
    let mut index = 0;
    let mut mode_for = |stream: Stream, mode: &str| -> Result<(), Refusal> {
        if !valid_mode(mode) {
            return Err(Refusal::Message(format!("invalid mode '{mode}'"), false));
        }
        if stream == Stream::Input && mode == "L" {
            return Err(Refusal::Message(
                "line buffering stdin is meaningless".to_owned(),
                false,
            ));
        }
        parsed.any_mode = true;
        if stream == Stream::Output && (mode == "L" || mode == "0") {
            parsed.output_by_line = true;
        }
        Ok(())
    };
    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        let (stream, attached) = match arg {
            "--" => break,
            "--help" => return Err(Refusal::Help),
            "--version" => return Err(Refusal::Version),
            "-i" | "--input" => (Stream::Input, None),
            "-o" | "--output" => (Stream::Output, None),
            "-e" | "--error" => (Stream::Error, None),
            _ if arg.starts_with("--input=") => (Stream::Input, arg.get(8..)),
            _ if arg.starts_with("--output=") => (Stream::Output, arg.get(9..)),
            _ if arg.starts_with("--error=") => (Stream::Error, arg.get(8..)),
            _ if arg.starts_with("-i") => (Stream::Input, arg.get(2..)),
            _ if arg.starts_with("-o") => (Stream::Output, arg.get(2..)),
            _ if arg.starts_with("-e") => (Stream::Error, arg.get(2..)),
            _ if arg.starts_with("--") => {
                let long = arg.trim_start_matches('-');
                let long = long.split('=').next().unwrap_or(long);
                return Err(Refusal::Message(
                    format!("unrecognized option '--{long}'"),
                    true,
                ));
            }
            _ if arg.starts_with('-') && arg.len() > 1 => {
                return Err(Refusal::Message(
                    format!("invalid option -- '{}'", arg.chars().nth(1).unwrap_or('-')),
                    true,
                ));
            }
            _ => {
                index -= 1;
                break;
            }
        };
        let mode = if let Some(mode) = attached {
            mode.to_owned()
        } else {
            let Some(mode) = args.get(index) else {
                let shown = match stream {
                    Stream::Input => "i",
                    Stream::Output => "o",
                    Stream::Error => "e",
                };
                return Err(Refusal::Message(
                    format!("option requires an argument -- '{shown}'"),
                    true,
                ));
            };
            index += 1;
            mode.clone()
        };
        mode_for(stream, &mode)?;
    }
    parsed.command = args[index..].to_vec();
    if parsed.command.is_empty() {
        return Err(Refusal::Message("missing operand".to_owned(), true));
    }
    if !parsed.any_mode {
        return Err(Refusal::Message(
            "you must specify a buffering mode option".to_owned(),
            true,
        ));
    }
    Ok(parsed)
}

impl builtins::Command for StdbufCommand {
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
        let mut parsed = match parse(&self.args) {
            Ok(parsed) => parsed,
            Err(Refusal::Help) => {
                writeln!(context.stdout(), "{HELP}")?;
                return Ok(ExecutionResult::success());
            }
            Err(Refusal::Version) => {
                writeln!(
                    context.stdout(),
                    "stdbuf (cash): coreutils' options; a program's buffering cannot be changed on Windows"
                )?;
                return Ok(ExecutionResult::success());
            }
            Err(Refusal::Message(message, try_help)) => {
                let mut stderr = context.stderr();
                writeln!(stderr, "stdbuf: {message}")?;
                if try_help {
                    writeln!(stderr, "Try 'stdbuf --help' for more information.")?;
                }
                return Ok(ExecutionResult::new(USAGE_ERROR));
            }
        };

        let name = parsed.command[0].clone();
        let is_own = context
            .shell
            .builtins()
            .get(&name)
            .is_some_and(|builtin| !builtin.disabled);
        if is_own {
            if parsed.output_by_line && matches!(name.as_str(), "grep" | "egrep" | "fgrep") {
                parsed.command.insert(1, "--line-buffered".to_owned());
            }
        } else {
            writeln!(context.stderr(), "cash: stdbuf: {NOTE}")?;
        }

        let params = context.params.clone();
        match cash_core::commands::run_for_builtin(context.shell, params, &parsed.command).await {
            Ok(result) => Ok(result),
            Err(error) => {
                writeln!(
                    context.stderr(),
                    "stdbuf: failed to run command '{name}': {}",
                    crate::xargs::start_failure(&error)
                )?;
                Ok(ExecutionResult::new(127))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn modes_are_l_zero_or_a_size() {
        for mode in ["L", "0", "4096", "4K", "1KB", "2MiB", "1G"] {
            assert!(valid_mode(mode), "{mode}");
        }
        for mode in ["X", "", "K", "4X", "-1", "1.5K"] {
            assert!(!valid_mode(mode), "{mode}");
        }
    }

    #[test]
    fn options_end_at_the_command() {
        let parsed = parse(&args("-oL -e 0 --input=4K cmd -o x"))
            .unwrap_or_else(|_| unreachable!("a valid line parses"));
        assert!(parsed.any_mode && parsed.output_by_line);
        assert_eq!(parsed.command, args("cmd -o x"));

        let parsed =
            parse(&args("-e L -- -cmd")).unwrap_or_else(|_| unreachable!("a valid line parses"));
        assert!(!parsed.output_by_line);
        assert_eq!(parsed.command, args("-cmd"));
    }

    #[test]
    fn the_refusals_are_coreutils() {
        assert_eq!(
            parse(&args("-oL")),
            Err(Refusal::Message("missing operand".into(), true))
        );
        assert_eq!(
            parse(&args("cmd")),
            Err(Refusal::Message(
                "you must specify a buffering mode option".into(),
                true
            ))
        );
        assert_eq!(
            parse(&args("-oX cmd")),
            Err(Refusal::Message("invalid mode 'X'".into(), false))
        );
        assert_eq!(
            parse(&args("-iL cmd")),
            Err(Refusal::Message(
                "line buffering stdin is meaningless".into(),
                false
            ))
        );
        assert_eq!(
            parse(&args("-x cmd")),
            Err(Refusal::Message("invalid option -- 'x'".into(), true))
        );
        assert_eq!(
            parse(&args("--nope cmd")),
            Err(Refusal::Message(
                "unrecognized option '--nope'".into(),
                true
            ))
        );
        assert_eq!(
            parse(&args("-o")),
            Err(Refusal::Message(
                "option requires an argument -- 'o'".into(),
                true
            ))
        );
        assert_eq!(parse(&args("--help")), Err(Refusal::Help));
    }
}
