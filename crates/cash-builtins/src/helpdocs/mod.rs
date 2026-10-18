//! What `help` needs beside the manual's pages, which are `cash-man`'s: the F1 screen,
//! the words `help` takes as requests, and the names to suggest for one it does not
//! know.
//!
//! `f1.md` is the one screen F1 shows at the prompt: not a topic, so `help topics` does
//! not list it, but markdown for the same reasons the pages are. The install hint and
//! `cash doctor` take their package names from the manual's `tools` topic, through
//! [`install_hint`] and [`tools`].

pub mod f1;
pub mod render;

pub use cash_man::tools::{Tool, install_hint, tools};

/// The hidden long option, without its dashes, that `cash help ...` passes the `help`
/// builtin from outside the shell, so its errors say `cash help:` rather than name the
/// `-c` script and line that run it.
pub const CASH_SUBCOMMAND_OPTION: &str = "cash-subcommand";

/// Words `help` takes as a request rather than a name.
pub const COMMANDS: [&str; 2] = ["topics", "search"];

/// The builtins and topics whose names are closest to `word`, nearest first: for a name
/// `help` does not know.
pub fn suggestions(word: &str, builtins: &[&str]) -> Vec<String> {
    let word = word.to_lowercase();
    let mut candidates: Vec<(usize, String)> = builtins
        .iter()
        .map(|name| (*name).to_owned())
        .chain(
            cash_man::manual()
                .topics
                .iter()
                .map(|topic| topic.file.clone()),
        )
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
    fn near_misses_are_suggested() {
        assert_eq!(edit_distance("sde", "sed"), 2);
        let found = suggestions("sedd", &["sed", "set", "seq", "ls"]);
        assert_eq!(found.first().map(String::as_str), Some("sed"));
        let topic = suggestions("path", &[]);
        assert_eq!(topic.first().map(String::as_str), Some("paths"));
    }
}
