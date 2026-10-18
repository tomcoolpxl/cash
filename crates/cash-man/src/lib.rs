//! `man`: cash's own manual. Not man(1): no roff, no sections, no `MANPATH`; a page for
//! each command cash has and for cash's topics, read in a browser by `man` and in
//! short by `help`.
//!
//! The pages are markdown, one file each, in `pages/` (a command's) and `topics/`,
//! embedded deflated at build time. A command's page has fixed sections in a fixed
//! order ([`page::Heading`]), so every page reads the same way; the design is
//! `research/man-browser-design.md`.

pub mod page;
pub mod render;

use std::io::Read as _;
use std::sync::LazyLock;

use page::{Page, Subject};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/pages.rs"));
}

/// Every page of the manual.
#[derive(Debug)]
pub struct Manual {
    /// The commands' pages, by file name.
    pub pages: Vec<Page>,
    /// The topics, by file name.
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
    static MANUAL: LazyLock<Manual> = LazyLock::new(|| Manual {
        pages: read(embedded::PAGES, Subject::Command),
        topics: read(embedded::TOPICS, Subject::Topic),
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
        for page in manual.pages.iter().chain(&manual.topics) {
            assert!(
                page.problems().is_empty(),
                "{}.md: {:#?}",
                page.file,
                page.problems()
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
