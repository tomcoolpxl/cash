//! `man`: cash's own manual. Not man(1): no roff, no sections, no `MANPATH`; a page for
//! each command cash has and for cash's topics, read in a browser by `man` and in
//! short by `help`.
//!
//! The pages are markdown, one file each, in `pages/` (a command's) and `topics/`,
//! embedded deflated at build time. A command's page has fixed sections in a fixed
//! order ([`page::Heading`]), so every page reads the same way; the design is
//! `research/man-browser-design.md`.

pub mod browser;
pub mod page;
pub mod render;
pub mod search;
pub mod tools;

use std::io::Read as _;
use std::sync::LazyLock;

use page::{Page, Subject};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/pages.rs"));
}

/// The kinds of command a page's `kind:` names, in the order `help` lists them.
pub const KINDS: [&str; 8] = [
    "Bash builtins",
    "cash's shell commands",
    "Terminal and clipboard",
    "Windows commands",
    "Processes and network",
    "Text and file tools",
    "Coreutils written for Windows",
    "Bundled coreutils, from uutils",
];

/// The order the topics are listed in; any other topic follows, by name.
pub const TOPIC_ORDER: [&str; 11] = [
    "getting-started",
    "paths",
    "crlf",
    "elevation",
    "job-control",
    "keys",
    "config",
    "installing",
    "tools",
    "vars",
    "differences",
];

/// Every page of the manual.
#[derive(Debug)]
pub struct Manual {
    /// The commands' pages, by file name.
    pub pages: Vec<Page>,
    /// The topics, in [`TOPIC_ORDER`].
    pub topics: Vec<Page>,
}

impl Manual {
    /// The page that covers the command `name`: as written, else in any case, as
    /// Windows finds `CD.exe` for `cd`.
    #[must_use]
    pub fn page(&self, name: &str) -> Option<&Page> {
        let covers = |page: &&Page| page.names.iter().any(|covered| covered == name);
        let covers_folded = |page: &&Page| {
            page.names
                .iter()
                .any(|covered| covered.eq_ignore_ascii_case(name))
        };
        self.pages
            .iter()
            .find(covers)
            .or_else(|| self.pages.iter().find(covers_folded))
    }

    /// The topic `name`, in any case.
    #[must_use]
    pub fn topic(&self, name: &str) -> Option<&Page> {
        self.topics
            .iter()
            .find(|topic| topic.file.eq_ignore_ascii_case(name))
    }

    /// The command's page or the topic `name` names, the command's first.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&Page> {
        self.page(name).or_else(|| self.topic(name))
    }

    /// Each `see:` name that is no page and no topic, with the page that names it.
    #[must_use]
    pub fn broken_links(&self) -> Vec<(String, String)> {
        self.pages
            .iter()
            .chain(&self.topics)
            .flat_map(|page| {
                page.see
                    .iter()
                    .filter(|name| self.find(name).is_none())
                    .map(|name| (page.file.clone(), name.clone()))
            })
            .collect()
    }
}

/// The manual, read once from what the build embedded.
pub fn manual() -> &'static Manual {
    static MANUAL: LazyLock<Manual> = LazyLock::new(|| {
        let mut topics = read(embedded::TOPICS, Subject::Topic);
        topics.sort_by_key(|topic| {
            let rank = TOPIC_ORDER
                .iter()
                .position(|name| *name == topic.file)
                .unwrap_or(TOPIC_ORDER.len());
            (rank, topic.file.clone())
        });
        Manual {
            pages: read(embedded::PAGES, Subject::Command),
            topics,
        }
    });
    &MANUAL
}

/// The pages of one directory, inflated and parsed.
fn read(stored: &[(&str, &[u8])], subject: Subject) -> Vec<Page> {
    stored
        .iter()
        .map(|(file, deflated)| Page::parse(file, subject, &inflate(deflated)))
        .collect()
}

/// A page's text from its deflated bytes. The build wrote them, so they inflate; if
/// they ever did not, the page is empty, and its checks fail.
fn inflate(deflated: &[u8]) -> String {
    let mut bytes = Vec::new();
    match flate2::read::DeflateDecoder::new(deflated).read_to_end(&mut bytes) {
        Ok(_) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_has_the_shape_of_a_page() {
        let manual = manual();
        assert!(manual.page("sed").is_some(), "{:?}", manual.pages);
        let problems: Vec<String> = manual
            .pages
            .iter()
            .chain(&manual.topics)
            .flat_map(|page| {
                page.problems()
                    .into_iter()
                    .map(|problem| format!("{}.md: {problem}", page.file))
            })
            .collect();
        assert_eq!(problems, Vec::<String>::new(), "{problems:#?}");
    }

    #[test]
    fn every_link_leads_to_a_page_and_every_kind_is_known() {
        let manual = manual();
        assert_eq!(manual.broken_links(), Vec::<(String, String)>::new());
        let unknown: Vec<(&str, &str)> = manual
            .pages
            .iter()
            .filter(|page| !KINDS.contains(&page.kind.as_str()))
            .map(|page| (page.file.as_str(), page.kind.as_str()))
            .collect();
        assert_eq!(unknown, Vec::<(&str, &str)>::new());
    }

    #[test]
    fn every_topic_has_a_title_and_a_summary_and_no_command_takes_its_name() {
        let manual = manual();
        assert_eq!(manual.topics.len(), TOPIC_ORDER.len());
        for (topic, name) in manual.topics.iter().zip(TOPIC_ORDER) {
            assert_eq!(topic.file, name, "the topics in their order");
            assert!(
                !topic.title.is_empty() && !topic.summary.is_empty(),
                "{name}"
            );
            assert!(
                manual.page(name).is_none() && !["topics", "search"].contains(&name),
                "topic {name} is also a command's name, so `help {name}` could not reach it"
            );
        }
    }

    #[test]
    fn a_page_is_found_by_any_name_it_covers_in_any_case() {
        let manual = manual();
        let cd = manual.page("cd").expect("cd's page");
        assert!(cd.section(page::Heading::Syntax).is_some());
        assert_eq!(manual.page("CD").map(|page| &page.file), Some(&cd.file));
        assert!(manual.page("no-such-command").is_none());
    }

    #[test]
    fn a_link_to_no_page_is_broken() {
        let page = |file: &str, see: &str| {
            Page::parse(
                file,
                Subject::Command,
                &format!("---\nsummary: s\nkind: k\nsee: {see}\n---\n"),
            )
        };
        let manual = Manual {
            pages: vec![page("cd", "pwd paths"), page("pwd", "cd nowhere")],
            topics: vec![Page::parse("paths", Subject::Topic, "---\ntitle: t\n---\n")],
        };
        assert_eq!(
            manual.broken_links(),
            [("pwd".to_owned(), "nowhere".to_owned())]
        );
    }
}
