//! A page's examples, each a script the examples' test runs.
//!
//! Each block of code under a page's Examples heading runs under cash, and is checked
//! against what Git Bash and the real tools gave for it
//! (`crates/cash/tests/it/man_examples.rs`).
//!
//! A block that cannot run in a test, as it names a share, a directory of the reader's
//! own or a terminal, follows a line `<!-- parse only: why -->`: it is parsed, not run.
//! The line is not drawn.

use crate::page::{Heading, Page};

/// The mark before a block that is parsed, not run.
const PARSE_ONLY: &str = "<!-- parse only:";

/// A block of code under a page's Examples heading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Example {
    /// Its place among the page's examples, from 1.
    pub number: usize,
    /// The script: the block's lines, its four spaces of markdown taken off.
    pub script: String,
    /// Why it is parsed and not run, when it is.
    pub parse_only: Option<String>,
}

impl Page {
    /// The blocks of code under the Examples heading, in order.
    #[must_use]
    pub fn examples(&self) -> Vec<Example> {
        let Some(section) = self.section(Heading::Examples) else {
            return Vec::new();
        };
        let mut examples = Vec::new();
        let mut block: Vec<&str> = Vec::new();
        let mut parse_only: Option<String> = None;
        let mut fenced = false;
        let mut finish = |block: &mut Vec<&str>, parse_only: &mut Option<String>| {
            while block.last().is_some_and(|line| line.trim().is_empty()) {
                block.pop();
            }
            if !block.is_empty() {
                let script = block.join("\n") + "\n";
                examples.push(Example {
                    number: examples.len() + 1,
                    script,
                    parse_only: parse_only.take(),
                });
            }
            block.clear();
        };
        for line in section.body.lines() {
            if line.trim_start().starts_with("```") {
                if fenced {
                    finish(&mut block, &mut parse_only);
                }
                fenced = !fenced;
                continue;
            }
            if fenced {
                block.push(line);
            } else if let Some(code) = line.strip_prefix("    ") {
                block.push(code);
            } else if line.trim().is_empty() {
                if !block.is_empty() {
                    block.push("");
                }
            } else {
                finish(&mut block, &mut parse_only);
                if let Some(why) = line.trim().strip_prefix(PARSE_ONLY) {
                    parse_only = Some(why.trim_end_matches("-->").trim().to_owned());
                }
            }
        }
        finish(&mut block, &mut parse_only);
        examples
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::Subject;

    #[test]
    fn each_block_under_examples_is_a_script_and_a_mark_keeps_one_from_running() {
        let page = Page::parse(
            "cd",
            Subject::Command,
            "---\nsummary: s\nkind: k\n---\n## Syntax\n\n    cd DIR\n\n## Examples\n\n\
             Up one:\n\n    cd ..\n    pwd\n\nA share:\n\n<!-- parse only: names a share -->\n\n\
             \x20   cd //server/share\n\nFenced:\n\n```\nif true; then\n    echo yes\nfi\n```\n\n\
             ## Options\n\n    -L  logical\n",
        );
        assert_eq!(
            page.examples(),
            [
                Example {
                    number: 1,
                    script: "cd ..\npwd\n".to_owned(),
                    parse_only: None,
                },
                Example {
                    number: 2,
                    script: "cd //server/share\n".to_owned(),
                    parse_only: Some("names a share".to_owned()),
                },
                Example {
                    number: 3,
                    script: "if true; then\n    echo yes\nfi\n".to_owned(),
                    parse_only: None,
                },
            ]
        );
    }
}
