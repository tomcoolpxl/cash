//! What the builtins that read GNU's options share beside cash-getopt (decisions.md,
//! "Builtins share their option parsing, input opening and folder walks"): when the
//! options end, and the line a usage error is said with.

use std::fmt::Display;
use std::io::Write;

use cash_core::ExecutionResult;
use cash_getopt::Order;

/// When a GNU tool's options end: at the first operand when `POSIXLY_CORRECT` is set, as
/// glibc's `getopt_long` has it, else anywhere until `--`.
pub(crate) fn gnu_order<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Order {
    if context.shell.env_str("POSIXLY_CORRECT").is_some() {
        Order::StopAtOperand
    } else {
        Order::Permute
    }
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
