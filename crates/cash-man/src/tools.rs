//! The packages that install a command cash cannot find.
//!
//! The table of the topic `tools` is the one place the install hint at the prompt and
//! `cash doctor` take their package names from, so that the two cannot disagree.

use std::sync::LazyLock;

use crate::{manual, render};

/// The topic whose table says which package installs a command.
const TOOLS_TOPIC: &str = "tools";

/// A row of the table: a command, and the package of each manager that installs it.
#[derive(Debug)]
pub struct Tool {
    /// The command's name, as typed.
    pub command: &'static str,
    /// Its winget id, if winget has it.
    pub winget: Option<&'static str>,
    /// Its Scoop name, with the bucket before a slash when not in `main`, if Scoop has it.
    pub scoop: Option<&'static str>,
    /// What it is, in a few words.
    pub what: &'static str,
}

/// The table's rows, in its order.
pub fn tools() -> &'static [Tool] {
    static TOOLS: LazyLock<Vec<Tool>> = LazyLock::new(|| {
        let Some(topic) = manual().topic(TOOLS_TOPIC) else {
            return Vec::new();
        };
        topic
            .sections
            .iter()
            .map(|section| section.body.as_str())
            .chain([topic.intro.as_str()])
            .flat_map(str::lines)
            .filter_map(render::table_row)
            .skip(1)
            .filter_map(|cells| match cells.as_slice() {
                [command, winget, scoop, what, ..] => Some(Tool {
                    command,
                    winget: package(winget),
                    scoop: package(scoop),
                    what,
                }),
                _ => None,
            })
            .collect()
    });
    &TOOLS
}

/// A cell's package: none for `-`.
fn package(cell: &'static str) -> Option<&'static str> {
    (cell != "-").then_some(cell)
}

/// How to install `command`, a name typed at the prompt that was not found.
///
/// `winget install ID, or scoop install NAME`, or the one of the two that exists;
/// nothing for a command the table does not have. The name is matched in any case and
/// without `.exe`.
#[must_use]
pub fn install_hint(command: &str) -> Option<String> {
    let stem = command.len().checked_sub(4).and_then(|at| {
        command
            .get(at..)
            .filter(|suffix| suffix.eq_ignore_ascii_case(".exe"))
            .and_then(|_| command.get(..at))
    });
    let name = stem.unwrap_or(command);
    let tool = tools()
        .iter()
        .find(|tool| tool.command.eq_ignore_ascii_case(name))?;
    let ways: Vec<String> = tool
        .winget
        .map(|id| format!("winget install {id}"))
        .into_iter()
        .chain(tool.scoop.map(|name| format!("scoop install {name}")))
        .collect();
    (!ways.is_empty()).then(|| ways.join(", or "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tools_table_is_read_and_each_row_names_a_package() {
        let tools = tools();
        assert!(tools.len() >= 50, "{} rows", tools.len());
        let mut names: Vec<&str> = tools.iter().map(|tool| tool.command).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), tools.len(), "a command has two rows");
        for tool in tools {
            assert!(
                !tool.command.is_empty() && !tool.what.is_empty(),
                "{tool:?}"
            );
            assert!(
                tool.winget.is_some() || tool.scoop.is_some(),
                "{} has no package at all",
                tool.command
            );
            assert!(
                tool.command == tool.command.to_lowercase() && !tool.command.contains('.'),
                "{}: a command is listed in lower case, without .exe",
                tool.command
            );
        }
    }

    #[test]
    fn the_hint_names_both_managers_or_the_one_that_has_the_tool() {
        assert_eq!(
            install_hint("jq").as_deref(),
            Some("winget install jqlang.jq, or scoop install jq")
        );
        assert_eq!(
            install_hint("JQ.exe").as_deref(),
            Some("winget install jqlang.jq, or scoop install jq")
        );
        assert_eq!(
            install_hint("Rg").as_deref(),
            Some("winget install BurntSushi.ripgrep.MSVC, or scoop install ripgrep")
        );
        assert_eq!(install_hint("gzip"), None, "gzip is cash's own now");
        assert_eq!(install_hint("mvn").as_deref(), Some("scoop install maven"));
        assert_eq!(
            install_hint("code").as_deref(),
            Some("winget install Microsoft.VisualStudioCode, or scoop install extras/vscode")
        );
        assert_eq!(install_hint("nosuchtool"), None);
        assert_eq!(install_hint(""), None);
        assert_eq!(install_hint("jq.ex"), None);
        assert_eq!(package("-"), None);
        assert_eq!(package("Y.Y"), Some("Y.Y"));
    }
}
