use clap::Parser;
use std::io::Write;
use std::path::PathBuf;

use cash_core::{ExecutionExitCode, ExecutionResult, builtins};

/// Manage the current directory stack.
#[derive(Default, Parser)]
pub(crate) struct DirsCommand {
    /// Clear the directory stack.
    #[arg(short = 'c')]
    clear: bool,

    /// Don't tilde-shorten paths.
    #[arg(short = 'l')]
    tilde_long: bool,

    /// Print one directory per line instead of all on one line.
    #[arg(short = 'p')]
    print_one_per_line: bool,

    /// Print one directory per line with its index.
    #[arg(short = 'v')]
    print_one_per_line_with_index: bool,

    /// `+N` or `-N`: print only that entry of the stack.
    #[arg(allow_hyphen_values = true)]
    selectors: Vec<String>,
}

impl builtins::Command for DirsCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        if self.clear {
            context.shell.directory_stack_mut().clear();
            return Ok(ExecutionResult::success());
        }

        let listing = listing(context.shell);

        // cash: `+N` and `-N` were rejected by the parser as unexpected arguments, so
        // `dirs +1` — how a script reads one entry back out without counting words —
        // failed with a usage error. bash keeps the last selector when given several.
        let selected = match self.selectors.last() {
            None => None,
            Some(arg) => {
                let Some(selector) = Selector::parse(arg) else {
                    // bash calls a bare word an option it does not know, and a malformed
                    // `+N` a number it cannot read.
                    let complaint = if arg.starts_with(['+', '-']) {
                        "invalid number"
                    } else {
                        "invalid option"
                    };
                    writeln!(
                        context.stderr(),
                        "{}: {arg}: {complaint}",
                        context.command_name
                    )?;
                    writeln!(
                        context.stderr(),
                        "{}: usage: dirs [-clpv] [+N] [-N]",
                        context.command_name
                    )?;
                    return Ok(ExecutionExitCode::InvalidUsage.into());
                };

                let Some(index) = selector.index_in(listing.len()) else {
                    // bash prints the index without its sign here, though `pushd` and
                    // `popd` keep the sign the caller typed.
                    return report_bad_index(&context, listing.len(), &selector.digits());
                };

                Some(index)
            }
        };

        let one_per_line = self.print_one_per_line || self.print_one_per_line_with_index;
        let entries: Vec<(usize, &PathBuf)> = match selected {
            Some(index) => vec![(index, &listing[index])],
            None => listing.iter().enumerate().collect(),
        };

        for (printed, (index, dir)) in entries.iter().enumerate() {
            if !one_per_line && printed > 0 {
                write!(context.stdout(), " ")?;
            }

            if self.print_one_per_line_with_index {
                write!(context.stdout(), "{index:2}  ")?;
            }

            let mut dir_str = dir.to_string_lossy().to_string();

            if !self.tilde_long {
                dir_str = context.shell.tilde_shorten(dir_str);
            }

            write!(context.stdout(), "{dir_str}")?;

            if one_per_line || printed == entries.len() - 1 {
                writeln!(context.stdout())?;
            }
        }

        Ok(ExecutionResult::success())
    }
}

/// A `+N` or `-N` argument, naming one entry of the stack listing.
#[derive(Clone, Copy)]
pub(crate) enum Selector {
    /// `+N`: counting from the current directory.
    FromLeft(usize),
    /// `-N`: counting from the far end, where `-0` is the oldest entry.
    FromRight(usize),
}

impl Selector {
    /// Reads `+N` or `-N`, and nothing else — a bare word is a directory or a mistake,
    /// which is the caller's to report.
    pub(crate) fn parse(arg: &str) -> Option<Self> {
        let (digits, from_left) = match arg.strip_prefix('+') {
            Some(digits) => (digits, true),
            None => (arg.strip_prefix('-')?, false),
        };

        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }

        let n = digits.parse::<usize>().ok()?;
        Some(if from_left {
            Self::FromLeft(n)
        } else {
            Self::FromRight(n)
        })
    }

    /// The absolute index into a listing of `len` entries, if it names one.
    pub(crate) const fn index_in(self, len: usize) -> Option<usize> {
        match self {
            Self::FromLeft(n) if n < len => Some(n),
            Self::FromRight(n) if n < len => Some(len - 1 - n),
            _ => None,
        }
    }

    /// The number as typed, without its sign — the form `dirs` reports it in.
    pub(crate) fn digits(self) -> String {
        match self {
            Self::FromLeft(n) | Self::FromRight(n) => n.to_string(),
        }
    }
}

/// The stack as bash lists it: the current directory, then the saved entries, most
/// recently pushed first.
///
/// cash: the shell stores the saved entries oldest-first, which is the opposite of how
/// every `+N` in a script counts. Everything here works on the listing and writes it back
/// through `store`, so the reversal lives in one place instead of in each builtin.
pub(crate) fn listing<SE: cash_core::ShellExtensions>(
    shell: &cash_core::Shell<SE>,
) -> Vec<PathBuf> {
    std::iter::once(shell.working_dir().to_path_buf())
        .chain(shell.directory_stack().iter().rev().cloned())
        .collect()
}

/// Stores a rearranged listing, making its first entry the working directory.
///
/// With `change_dir` false — bash's `-n` — the first entry is dropped instead: the shell
/// stays where it is, and whatever rotated or popped into its place is gone. That is
/// exactly what bash does, oddly enough: `pushd -n +1` on `[c b a base]` leaves
/// `[c a base c]`, with `b` nowhere.
pub(crate) fn store<SE: cash_core::ShellExtensions>(
    shell: &mut cash_core::Shell<SE>,
    listing: Vec<PathBuf>,
    change_dir: bool,
) -> Result<(), cash_core::Error> {
    let mut rest = listing;
    if rest.is_empty() {
        return Ok(());
    }

    let top = rest.remove(0);
    if change_dir {
        // Before anything is written back: a directory that has been removed since it was
        // pushed leaves the stack exactly as it was, as it does in bash.
        shell.set_working_dir(&top)?;
    }

    rest.reverse();
    *shell.directory_stack_mut() = rest;

    Ok(())
}

/// bash's two ways of refusing a selector: nothing has been pushed at all, or the index
/// is past what has been.
pub(crate) fn report_bad_index<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    listing_len: usize,
    shown: &str,
) -> Result<ExecutionResult, cash_core::Error> {
    if listing_len <= 1 {
        writeln!(
            context.stderr(),
            "{}: directory stack empty",
            context.command_name
        )?;
    } else {
        writeln!(
            context.stderr(),
            "{}: {shown}: directory stack index out of range",
            context.command_name
        )?;
    }

    Ok(ExecutionResult::general_error())
}
