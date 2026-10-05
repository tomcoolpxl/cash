//! The help catalogue: one entry for every builtin, with its kind and a one-line summary,
//! pages for the builtins that need more than their `--help`, and topics.
//!
//! It is markdown, embedded at build time, so that it reads and reviews as text:
//!
//! - `builtins.md`: every builtin, under the heading of its kind, as `` - `NAME`: summary ``.
//! - `pages/NAME.md`: a builtin's page. Front matter may say `names:` (the builtins it
//!   covers, when not just the file's name) and `see:` (builtins and topics to read
//!   next).
//! - `topics/NAME.md`: a topic, with `title:` and `summary:` in its front matter, and
//!   `see:` as a page has it.
//!
//! The options of a builtin are not copied here: its page ends with its own `--help`.
//! `crates/cash/tests/it/help_catalogue.rs` fails when a builtin has no entry, or an
//! entry or page names no builtin.

pub mod render;

use std::sync::LazyLock;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/helpdocs.rs"));
}

/// The list of builtins by kind.
const INDEX: &str = include_str!("builtins.md");

/// The order `help topics` lists the topics in; any other topic follows, by name.
const TOPIC_ORDER: [&str; 8] = [
    "paths",
    "crlf",
    "elevation",
    "job-control",
    "keys",
    "config",
    "vars",
    "differences",
];

/// The hidden long option, without its dashes, that `cash help ...` passes the `help`
/// builtin from outside the shell, so its errors say `cash help:` rather than name the
/// `-c` script and line that run it.
pub const CASH_SUBCOMMAND_OPTION: &str = "cash-subcommand";

/// Words `help` takes as a request rather than a name.
pub const COMMANDS: [&str; 2] = ["topics", "search"];

/// A builtin's entry in the catalogue.
#[derive(Debug)]
pub struct Entry {
    /// The builtin's name.
    pub name: &'static str,
    /// The heading of its kind in `builtins.md`.
    pub kind: &'static str,
    /// One line, in markdown.
    pub summary: &'static str,
}

/// A builtin's page.
#[derive(Debug)]
pub struct Page {
    /// The file it came from, without `.md`.
    pub file: &'static str,
    /// The builtins it covers.
    pub names: Vec<&'static str>,
    /// Builtins and topics to read next.
    pub see: Vec<&'static str>,
    /// The markdown, front matter removed.
    pub body: &'static str,
}

/// A topic: something about cash that no one builtin owns.
#[derive(Debug)]
pub struct Topic {
    /// The name `help` takes for it.
    pub name: &'static str,
    /// Its title.
    pub title: &'static str,
    /// One line, in markdown.
    pub summary: &'static str,
    /// Builtins and topics to read next.
    pub see: Vec<&'static str>,
    /// The markdown, front matter removed.
    pub body: &'static str,
}

/// Everything `help` knows.
#[derive(Debug)]
pub struct Catalogue {
    /// The kinds of builtin, in the order `help` lists them.
    pub kinds: Vec<&'static str>,
    /// Every builtin's entry, in `builtins.md`'s order.
    pub entries: Vec<Entry>,
    /// The pages.
    pub pages: Vec<Page>,
    /// The topics, in the order `help topics` lists them.
    pub topics: Vec<Topic>,
}

impl Catalogue {
    /// The entry for the builtin `name`.
    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// The page that covers the builtin `name`.
    pub fn page(&self, name: &str) -> Option<&Page> {
        self.pages.iter().find(|page| page.names.contains(&name))
    }

    /// The topic `name`, in any case.
    pub fn topic(&self, name: &str) -> Option<&Topic> {
        self.topics
            .iter()
            .find(|topic| topic.name.eq_ignore_ascii_case(name))
    }

    /// Whether `name` is something `help` can show: a builtin, a topic, or a request.
    pub fn knows(&self, name: &str) -> bool {
        self.entry(name).is_some() || self.topic(name).is_some() || COMMANDS.contains(&name)
    }
}

/// The catalogue, read once.
pub fn catalogue() -> &'static Catalogue {
    static CATALOGUE: LazyLock<Catalogue> = LazyLock::new(read_catalogue);
    &CATALOGUE
}

fn read_catalogue() -> Catalogue {
    let (kinds, entries) = read_index(INDEX);
    let pages = embedded::PAGES
        .iter()
        .map(|(file, text)| {
            let (front, body) = front_matter(text);
            let names = front.words("names");
            Page {
                file,
                names: if names.is_empty() { vec![*file] } else { names },
                see: front.words("see"),
                body,
            }
        })
        .collect();
    let mut topics: Vec<Topic> = embedded::TOPICS
        .iter()
        .map(|(name, text)| {
            let (front, body) = front_matter(text);
            Topic {
                name,
                title: front.value("title").unwrap_or(name),
                summary: front.value("summary").unwrap_or_default(),
                see: front.words("see"),
                body,
            }
        })
        .collect();
    topics.sort_by_key(|topic| {
        let rank = TOPIC_ORDER
            .iter()
            .position(|name| *name == topic.name)
            .unwrap_or(TOPIC_ORDER.len());
        (rank, topic.name)
    });
    Catalogue {
        kinds,
        entries,
        pages,
        topics,
    }
}

/// The kinds and entries of `builtins.md`: a `## ` line starts a kind, and an entry is
/// `` - `NAME`: summary ``.
fn read_index(text: &'static str) -> (Vec<&'static str>, Vec<Entry>) {
    let mut kinds = Vec::new();
    let mut entries = Vec::new();
    for line in text.lines() {
        if let Some(kind) = line.strip_prefix("## ") {
            kinds.push(kind.trim());
        } else if let Some(rest) = line.strip_prefix("- `")
            && let Some((name, summary)) = rest.split_once("`:")
            && let Some(kind) = kinds.last()
        {
            entries.push(Entry {
                name,
                kind,
                summary: summary.trim(),
            });
        }
    }
    (kinds, entries)
}

/// A file's front matter: `key: value` lines between two `---` lines at its start.
struct FrontMatter(Vec<(&'static str, &'static str)>);

impl FrontMatter {
    fn value(&self, key: &str) -> Option<&'static str> {
        self.0
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| *value)
    }

    fn words(&self, key: &str) -> Vec<&'static str> {
        self.value(key)
            .map(|value| value.split_whitespace().collect())
            .unwrap_or_default()
    }
}

/// `text` split into its front matter and the rest.
fn front_matter(text: &'static str) -> (FrontMatter, &'static str) {
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return (FrontMatter(Vec::new()), text);
    };
    let mut pairs = Vec::new();
    let mut body = "";
    let mut remaining = rest;
    while let Some((line, after)) = remaining.split_once('\n') {
        let line = line.trim_end_matches('\r');
        if line == "---" {
            body = after;
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            pairs.push((key.trim(), value.trim()));
        }
        remaining = after;
    }
    (FrontMatter(pairs), body)
}

/// One thing a search found: a builtin or a topic, and the lines of it that matched.
#[derive(Debug)]
pub struct Hit {
    /// The name `help` takes for it.
    pub name: &'static str,
    /// Whether it is a topic rather than a builtin.
    pub topic: bool,
    /// Its one-line summary, in markdown.
    pub summary: &'static str,
    /// The headings, sentences and code lines of its page that matched, as plain text.
    pub lines: Vec<String>,
}

/// Every builtin and topic whose name, summary, page or text contains `word`, in any case.
pub fn search(word: &str) -> Vec<Hit> {
    let catalogue = catalogue();
    let needle = word.to_lowercase();
    let matches = |text: &str| text.to_lowercase().contains(&needle);
    let matching_lines = |body: &str| -> Vec<String> {
        render::search_texts(body)
            .into_iter()
            .filter(|line| matches(line))
            .collect()
    };

    let mut hits = Vec::new();
    for entry in &catalogue.entries {
        // A page covering several builtins is reported once, under its first name.
        let page = catalogue.page(entry.name);
        let lines = page
            .filter(|page| page.names.first() == Some(&entry.name))
            .map(|page| matching_lines(page.body))
            .unwrap_or_default();
        if matches(entry.name) || matches(&render::plain(entry.summary)) || !lines.is_empty() {
            hits.push(Hit {
                name: entry.name,
                topic: false,
                summary: entry.summary,
                lines,
            });
        }
    }
    for topic in &catalogue.topics {
        let lines = matching_lines(topic.body);
        if matches(topic.name)
            || matches(topic.title)
            || matches(&render::plain(topic.summary))
            || !lines.is_empty()
        {
            hits.push(Hit {
                name: topic.name,
                topic: true,
                summary: topic.summary,
                lines,
            });
        }
    }
    hits
}

/// The builtins and topics whose names are closest to `word`, nearest first: for a name
/// `help` does not know.
pub fn suggestions(word: &str, builtins: &[&str]) -> Vec<String> {
    let catalogue = catalogue();
    let word = word.to_lowercase();
    let mut candidates: Vec<(usize, String)> = builtins
        .iter()
        .map(|name| (*name).to_owned())
        .chain(catalogue.topics.iter().map(|topic| topic.name.to_owned()))
        .filter_map(|name| {
            let distance = edit_distance(&word, &name.to_lowercase());
            let close = distance <= (word.chars().count() / 3).max(1)
                || (word.chars().count() >= 3 && name.starts_with(word.as_str()));
            close.then_some((distance, name))
        })
        .collect();
    candidates.sort();
    candidates.dedup_by(|a, b| a.1 == b.1);
    candidates
        .into_iter()
        .take(5)
        .map(|(_, name)| name)
        .collect()
}

/// Levenshtein's distance between `a` and `b`, by characters.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let substitute = previous.get(j).copied().unwrap_or_default() + usize::from(ca != *cb);
            let delete = previous.get(j + 1).copied().unwrap_or_default() + 1;
            let insert = current.get(j).copied().unwrap_or_default() + 1;
            current.push(substitute.min(delete).min(insert));
        }
        previous = current;
    }
    previous.last().copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_covers_a_builtin_in_the_list() {
        let catalogue = catalogue();
        for page in &catalogue.pages {
            for name in &page.names {
                assert!(
                    catalogue.entry(name).is_some(),
                    "pages/{}.md names `{name}`, which builtins.md does not list",
                    page.file
                );
            }
        }
    }

    #[test]
    fn every_see_also_names_a_builtin_or_a_topic() {
        let catalogue = catalogue();
        let sees = catalogue
            .pages
            .iter()
            .map(|page| (page.file, &page.see))
            .chain(
                catalogue
                    .topics
                    .iter()
                    .map(|topic| (topic.name, &topic.see)),
            );
        for (file, see) in sees {
            for name in see {
                assert!(
                    catalogue.knows(name),
                    "{file}.md sees `{name}`, which is neither a builtin nor a topic"
                );
            }
        }
    }

    #[test]
    fn every_entry_has_a_kind_and_a_summary_and_appears_once() {
        let catalogue = catalogue();
        let mut names: Vec<&str> = catalogue.entries.iter().map(|entry| entry.name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "builtins.md lists a builtin twice");
        for entry in &catalogue.entries {
            assert!(!entry.summary.is_empty(), "`{}` has no summary", entry.name);
        }
    }

    #[test]
    fn every_topic_has_a_title_and_a_summary_and_no_builtin_takes_its_name() {
        let catalogue = catalogue();
        assert!(!catalogue.topics.is_empty());
        for topic in &catalogue.topics {
            assert!(
                !topic.summary.is_empty(),
                "topic {} has no summary",
                topic.name
            );
            assert!(
                catalogue.entry(topic.name).is_none() && !COMMANDS.contains(&topic.name),
                "topic {} is also a builtin's name, so `help {0}` could not reach it",
                topic.name
            );
        }
    }

    #[test]
    fn a_page_without_names_covers_its_file() {
        let (front, body) = front_matter("---\nsee: cd paths\n---\n## Text\n");
        assert_eq!(front.words("see"), ["cd", "paths"]);
        assert_eq!(body, "## Text\n");
    }

    #[test]
    fn near_misses_are_suggested() {
        assert_eq!(edit_distance("sde", "sed"), 2);
        let found = suggestions("sedd", &["sed", "set", "seq", "ls"]);
        assert_eq!(found.first().map(String::as_str), Some("sed"));
    }
}
