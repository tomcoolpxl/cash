//! Utility functions for the test harness.

use anyhow::Result;

/// Writes a diff between two strings to a writer.
pub fn write_diff(
    writer: &mut impl std::io::Write,
    indent: usize,
    left: &str,
    right: &str,
) -> Result<()> {
    use colored::Colorize;

    let indent_str = " ".repeat(indent);

    let diff = diff::lines(left, right);
    for d in diff {
        let formatted = match d {
            diff::Result::Left(l) => std::format!("{indent_str}- {l}").red(),
            diff::Result::Both(l, _) => std::format!("{indent_str}  {l}").bright_black(),
            diff::Result::Right(r) => std::format!("{indent_str}+ {r}").green(),
        };

        writeln!(writer, "{formatted}")?;
    }

    Ok(())
}
