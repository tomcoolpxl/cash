use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionResult, builtins, escape};

/// Echo text to standard output.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct EchoCommand {
    /// Suppress the trailing newline from the output.
    #[arg(short = 'n')]
    no_trailing_newline: bool,

    /// Interpret backslash escapes in the provided text.
    #[arg(short = 'e')]
    interpret_backslash_escapes: bool,

    /// Do not interpret backslash escapes in the provided text.
    #[arg(short = 'E')]
    no_interpret_backslash_escapes: bool,

    /// Tokens to echo to standard output.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for EchoCommand {
    type Error = cash_core::Error;

    /// Bash's own reading of echo's options, by hand: clap built its parser for every
    /// `echo`, a third of a loop of them, and refused what Bash takes. A word that is `-`
    /// and only `n`, `e` and `E` holds options, applied in order, so the last of `-e` and
    /// `-E` wins and `-nn` is `-n`; the first other word, `-` and `--` among them, ends
    /// them and is echoed.
    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        let mut this = Self {
            no_trailing_newline: false,
            interpret_backslash_escapes: false,
            no_interpret_backslash_escapes: false,
            args: Vec::new(),
        };
        // The first is the name the command was run by.
        let mut words = args.into_iter().skip(1).peekable();
        while let Some(flags) = words.peek().and_then(|word| word.strip_prefix('-')) {
            if flags.is_empty() || !flags.chars().all(|c| matches!(c, 'n' | 'e' | 'E')) {
                break;
            }
            for flag in flags.chars() {
                match flag {
                    'n' => this.no_trailing_newline = true,
                    'e' => {
                        this.interpret_backslash_escapes = true;
                        this.no_interpret_backslash_escapes = false;
                    }
                    _ => {
                        this.interpret_backslash_escapes = false;
                        this.no_interpret_backslash_escapes = true;
                    }
                }
            }
            words.next();
        }
        this.args = words.collect();
        Ok(this)
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        let mut trailing_newline = !self.no_trailing_newline;
        let mut s;
        // Without -e or -E, as `shopt xpg_echo` says (off by default).
        let interpret = self.interpret_backslash_escapes
            || (!self.no_interpret_backslash_escapes
                && context
                    .shell
                    .options()
                    .echo_builtin_expands_escape_sequences);
        if interpret {
            s = String::new();
            for (i, arg) in self.args.iter().enumerate() {
                if i > 0 {
                    s.push(' ');
                }

                let (expanded_arg, keep_going) = escape::expand_backslash_escapes(
                    arg.as_str(),
                    escape::EscapeExpansionMode::EchoBuiltin,
                )?;
                s.push_str(&String::from_utf8_lossy(expanded_arg.as_slice()));

                if !keep_going {
                    trailing_newline = false;
                    break;
                }
            }
        } else {
            s = self.args.join(" ");
        }

        if trailing_newline {
            s.push('\n');
        }

        let mut stdout = context.stdout();
        stdout.write_all(s.as_bytes())?;
        stdout.flush()?;

        Ok(ExecutionResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cash_core::builtins::Command as _;

    fn parsed(words: &[&str]) -> (bool, bool, bool, Vec<String>) {
        let args = std::iter::once("echo")
            .chain(words.iter().copied())
            .map(str::to_owned);
        let echo = EchoCommand::new(args).unwrap();
        (
            echo.no_trailing_newline,
            echo.interpret_backslash_escapes,
            echo.no_interpret_backslash_escapes,
            echo.args,
        )
    }

    #[test]
    fn options_are_read_as_bash_reads_them() {
        let words = |w: &[&str]| w.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        // The last of -e and -E wins; a repeated option is taken.
        assert_eq!(
            parsed(&["-e", "-E", "x"]),
            (false, false, true, words(&["x"]))
        );
        assert_eq!(parsed(&["-neE", "x"]), (true, false, true, words(&["x"])));
        assert_eq!(
            parsed(&["-nn", "-ee", "x"]),
            (true, true, false, words(&["x"]))
        );
        // `-`, `--` and a word with another letter end the options and are echoed.
        assert_eq!(
            parsed(&["-", "-n"]),
            (false, false, false, words(&["-", "-n"]))
        );
        assert_eq!(
            parsed(&["-n", "--", "x"]),
            (true, false, false, words(&["--", "x"]))
        );
        assert_eq!(
            parsed(&["-nx", "y"]),
            (false, false, false, words(&["-nx", "y"]))
        );
        assert_eq!(
            parsed(&["x", "-n"]),
            (false, false, false, words(&["x", "-n"]))
        );
    }
}
