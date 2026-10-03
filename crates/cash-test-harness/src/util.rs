//! Utility functions for the test harness.

use anyhow::Result;

/// Get the OS ID from /etc/os-release file.
/// Returns the value of the ID field, which is the canonical OS identifier.
/// For example: "ubuntu", "opensuse-tumbleweed", "fedora", etc.
pub fn get_host_os_id() -> Option<String> {
    os_release::OsRelease::new().ok().and_then(|info| {
        if info.id.is_empty() {
            None
        } else {
            Some(info.id)
        }
    })
}

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

/// Resolves the first element of the given launcher token list to an absolute path.
///
/// This is needed because the test harness clears env vars (including `PATH`)
/// before spawning child processes, so a launcher binary referenced by name
/// (e.g., `wasmtime`) would fail to resolve at exec time.
pub fn resolve_launcher_path(tokens: &mut [String]) -> Result<()> {
    let Some(first) = tokens.first() else {
        return Ok(());
    };
    if std::path::Path::new(first.as_str()).is_absolute() {
        return Ok(());
    }
    let resolved = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join(first.as_str()))
        .find(|p| p.is_file())
        .ok_or_else(|| anyhow::anyhow!("could not resolve launcher binary '{first}' in PATH"))?;
    tokens[0] = resolved.to_string_lossy().into_owned();
    Ok(())
}
