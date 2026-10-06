//! `mkfifo`, coreutils': make a named pipe with a file name. A Windows named pipe lives
//! under `\\.\pipe\` and is no file a script can `mkfifo` and then open by a path of its
//! own, so each name is refused with one line that points at what does the job in cash,
//! a process substitution, with coreutils' status 1 for a FIFO not made. Before this it
//! was `command not found`, status 127, which reads as the tool missing.
//!
//! The options are coreutils 9.x's: `-m MODE` and `--mode=MODE`, `-Z` and `--context`
//! are accepted and change nothing; `--help` and `--version` as theirs; no operand is
//! their `missing operand`.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Make named pipes; not files on Windows.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct MkfifoCommand {
    /// Options and names, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// The one line each name gets.
pub(crate) const REFUSAL: &str =
    "named pipes are not files on Windows; use a process substitution, <(…) or >(…)";

const HELP: &str = "\
Usage: mkfifo [OPTION]... NAME...
Create named pipes (FIFOs) with the given NAMEs.

Mandatory arguments to long options are mandatory for short options too.
  -m, --mode=MODE    set file permission bits to MODE, not a=rw - umask
  -Z                   set the SELinux security context to default type
      --context[=CTX]  like -Z, or if CTX is specified then set the SELinux
                         or SMACK security context to CTX
      --help     display this help and exit
      --version  output version information and exit

On Windows a named pipe is not a file, so no NAME can be made: use a
process substitution, <(command) or >(command), where a script would
read or write the FIFO.";

impl builtins::Command for MkfifoCommand {
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
        let mut names = Vec::new();
        let mut only_names = false;
        let mut args = self.args.iter().map(String::as_str);
        while let Some(arg) = args.next() {
            if only_names || arg == "-" || !arg.starts_with('-') {
                names.push(arg);
                continue;
            }
            match arg {
                "--" => only_names = true,
                "--help" => {
                    writeln!(context.stdout(), "{HELP}")?;
                    return Ok(ExecutionResult::success());
                }
                "--version" => {
                    writeln!(
                        context.stdout(),
                        "mkfifo (cash): coreutils' options; a FIFO cannot be made on Windows"
                    )?;
                    return Ok(ExecutionResult::success());
                }
                "-m" => {
                    if args.next().is_none() {
                        writeln!(
                            context.stderr(),
                            "mkfifo: option requires an argument -- 'm'\n\
                             Try 'mkfifo --help' for more information."
                        )?;
                        return Ok(ExecutionResult::general_error());
                    }
                }
                "-Z" | "--context" => {}
                _ if arg.starts_with("-m")
                    || arg.starts_with("--mode=")
                    || arg.starts_with("--context=") => {}
                _ => {
                    let shown = arg.strip_prefix("--").map_or_else(
                        || format!("invalid option -- '{}'", arg.chars().nth(1).unwrap_or('-')),
                        |long| format!("unrecognized option '--{long}'"),
                    );
                    writeln!(
                        context.stderr(),
                        "mkfifo: {shown}\nTry 'mkfifo --help' for more information."
                    )?;
                    return Ok(ExecutionResult::general_error());
                }
            }
        }
        if names.is_empty() {
            writeln!(
                context.stderr(),
                "mkfifo: missing operand\nTry 'mkfifo --help' for more information."
            )?;
            return Ok(ExecutionResult::general_error());
        }
        let mut stderr = context.stderr();
        for name in names {
            writeln!(stderr, "cash: mkfifo: {name}: {REFUSAL}")?;
        }
        Ok(ExecutionResult::general_error())
    }
}
