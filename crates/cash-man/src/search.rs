//! Searching the manual: every page whose name, summary or text holds a word.

use crate::page::{Page, Subject};
use crate::{KINDS, Manual, render};

/// One page a search found, and the lines of it that matched.
#[derive(Debug)]
pub struct Hit<'a> {
    /// The page.
    pub page: &'a Page,
    /// The name it is reported under: a command's first name, or a topic's file.
    pub name: &'a str,
    /// The headings, sentences and code lines of the page that matched, as plain text.
    pub lines: Vec<String>,
}

impl Manual {
    /// Every page whose names, summary, title or text contain `word`, in any case:
    /// the commands by kind, as `help` lists them, then the topics.
    #[must_use]
    pub fn search(&self, word: &str) -> Vec<Hit<'_>> {
        let needle = word.to_lowercase();
        let matches = |text: &str| text.to_lowercase().contains(&needle);
        let mut commands: Vec<&Page> = self.pages.iter().collect();
        commands.sort_by_key(|page| {
            let kind = KINDS
                .iter()
                .position(|kind| *kind == page.kind)
                .unwrap_or(KINDS.len());
            (kind, page.file.as_str())
        });
        commands
            .into_iter()
            .chain(&self.topics)
            .filter_map(|page| {
                let lines: Vec<String> = page_texts(page)
                    .into_iter()
                    .filter(|line| matches(line))
                    .collect();
                let found = !lines.is_empty()
                    || page.names.iter().any(|name| matches(name))
                    || matches(&page.title)
                    || matches(&page.summary.replace('`', ""))
                    || page
                        .name_summaries
                        .iter()
                        .any(|(_, summary)| matches(&summary.replace('`', "")));
                let name = match page.subject {
                    Subject::Command => page.names.first().map_or("", String::as_str),
                    Subject::Topic => page.file.as_str(),
                };
                found.then_some(Hit { page, name, lines })
            })
            .collect()
    }
}

/// The pieces of a page a search matches and shows: each heading, each sentence of its
/// prose, and each line of code.
fn page_texts(page: &Page) -> Vec<String> {
    let mut texts = render::search_texts(&page.intro);
    for section in &page.sections {
        texts.push(section.title.clone());
        texts.extend(render::search_texts(&section.body));
    }
    texts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_search_finds_names_summaries_and_text_and_shows_the_lines() {
        let page = |file: &str, text: &str| Page::parse(file, Subject::Command, text);
        let manual = Manual {
            pages: vec![
                page(
                    "grep",
                    "---\nnames: grep egrep\nsummary: Match lines.\nsummary egrep: `grep -E`.\n\
                     kind: Text and file tools\n---\n## How it works\n\nIt reads a pipe. Then more.\n",
                ),
                page(
                    "cd",
                    "---\nsummary: Change directory.\nkind: Bash builtins\n---\n",
                ),
            ],
            topics: vec![Page::parse(
                "crlf",
                Subject::Topic,
                "---\ntitle: Line endings\nsummary: s\n---\nA pipe keeps CRLF.\n",
            )],
        };
        let found = |word: &str| -> Vec<(String, Vec<String>)> {
            manual
                .search(word)
                .into_iter()
                .map(|hit| (hit.name.to_owned(), hit.lines))
                .collect()
        };
        // Commands by kind: cd's Bash builtins before grep's text tools.
        assert_eq!(
            found("pipe"),
            [
                ("grep".to_owned(), vec!["It reads a pipe.".to_owned()]),
                ("crlf".to_owned(), vec!["A pipe keeps CRLF.".to_owned()]),
            ]
        );
        assert_eq!(found("GREP -E").len(), 1);
        assert_eq!(found("directory")[0].0, "cd");
        assert_eq!(found("line endings")[0].0, "crlf");
        assert_eq!(found("nowhere"), []);
    }
}
