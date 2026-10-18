//! What the builtins that read GNU's options share beside cash-getopt (decisions.md,
//! "Builtins share their option parsing, input opening and directory walks"): when the
//! options end, and the line a usage error is said with.

use std::fmt::Display;
use std::io::Write;

use cash_core::ExecutionResult;
use cash_getopt::Order;

/// Whether the shell has `POSIXLY_CORRECT` set, which makes glibc's `getopt_long` end the
/// options at the first operand.
pub(crate) fn posixly_correct<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> bool {
    context.shell.env_str("POSIXLY_CORRECT").is_some()
}

/// When a GNU tool's options end: at the first operand under `POSIXLY_CORRECT`, else
/// anywhere until `--`.
pub(crate) fn gnu_order<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Order {
    if posixly_correct(context) {
        Order::StopAtOperand
    } else {
        Order::Permute
    }
}

/// Runs a tool's reading of its command line with the shell's `POSIXLY_CORRECT` in force
/// for every parser it makes that sets no order of its own (`cash_getopt::posixly_correct`).
pub(crate) fn read_options<SE: cash_core::ShellExtensions, T>(
    context: &cash_core::ExecutionContext<'_, SE>,
    read: impl FnOnce() -> T,
) -> T {
    cash_getopt::posixly_correct(posixly_correct(context), read)
}

/// Says `problem` as GNU's tools and util-linux's say a usage error, `tool: problem` and
/// the line that points at `--help`, and ends with `status`.
pub(crate) fn gnu_usage_error<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    tool: &str,
    problem: impl Display,
    status: u8,
) -> Result<ExecutionResult, cash_core::Error> {
    writeln!(
        context.stderr(),
        "{tool}: {problem}\nTry '{tool} --help' for more information."
    )?;
    Ok(ExecutionResult::new(status))
}
