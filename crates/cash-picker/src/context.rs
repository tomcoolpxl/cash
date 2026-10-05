//! What the command line says about the pick: the command it is for, the word under the
//! cursor, and so what the picker shows and what a pick does (spec D73).
//!
//! The line is read as far as the picker needs, not parsed: the simple command the cursor
//! is in (after `|`, `;`, `&`, `(`, `{`, `&&` or `||`), its words with quotes kept whole,
//! and its command word after any `NAME=value`, `sudo` and `command`/`builtin`/`exec`
//! prefix.

/// What the picker lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shows {
    /// Folders only.
    Folders,
    /// Folders and files.
    Everything,
}

/// What happens after a pick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Then {
    /// The line runs at once (`cd`, `pushd`, an empty line).
    Run,
    /// The picker closes and the line waits for more typing.
    Close,
    /// The picker stays open for the next path.
    StayOpen,
}

/// The line as the picker reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The command the cursor's simple command runs, as typed; `None` on an empty one.
    pub command: Option<String>,
    /// The byte range of the word under the cursor; empty at the cursor when the cursor
    /// is not in a word.
    pub word: std::ops::Range<usize>,
    /// That word with its quotes taken off.
    pub text: String,
}

impl Line {
    /// Reads `line` around the byte offset `cursor`.
    #[must_use]
    pub fn read(line: &str, cursor: usize) -> Self {
        let cursor = cursor.min(line.len());
        let words = words(line);
        // The simple command the cursor is in: the words after the last separator before
        // the cursor, up to the next one.
        let start = words
            .iter()
            .rposition(|w| w.separator && w.range.end <= cursor)
            .map_or(0, |i| i + 1);
        let end = words
            .iter()
            .skip(start)
            .position(|w| w.separator)
            .map_or(words.len(), |i| start + i);
        let command_words = words.get(start..end).unwrap_or_default();

        let under = command_words
            .iter()
            .find(|w| w.range.start <= cursor && cursor <= w.range.end);
        let (word, text) = under.map_or_else(
            || (cursor..cursor, String::new()),
            |w| (w.range.clone(), w.text.clone()),
        );
        let command = command_word(command_words, under.map(|w| w.range.start));
        Self {
            command,
            word,
            text,
        }
    }

    /// What the picker lists for this line.
    #[must_use]
    pub fn shows(&self) -> Shows {
        match self.command.as_deref().map(command_name) {
            None | Some("cd" | "pushd" | "rmdir" | "mkdir") => Shows::Folders,
            Some(_) => Shows::Everything,
        }
    }

    /// What a pick does on this line.
    #[must_use]
    pub fn then(&self) -> Then {
        match self.command.as_deref().map(command_name) {
            None | Some("cd" | "pushd") => Then::Run,
            Some("source" | ".") => Then::Close,
            Some(_) => Then::StayOpen,
        }
    }
}

/// A command's name without its folder or `.exe`, folded to lower case, as Windows finds
/// `CD`, `C:/Windows/System32/cd.exe` or `cd` alike.
fn command_name(command: &str) -> &'static str {
    let base = command.rsplit(['/', '\\']).next().unwrap_or(command);
    let base = base
        .strip_suffix(".exe")
        .or_else(|| base.strip_suffix(".EXE"))
        .unwrap_or(base);
    ["cd", "pushd", "rmdir", "mkdir", "source", "."]
        .into_iter()
        .find(|known| known.eq_ignore_ascii_case(base))
        .unwrap_or("other")
}

/// One word of the line, or a separator between simple commands.
#[derive(Debug)]
struct Word {
    range: std::ops::Range<usize>,
    text: String,
    separator: bool,
}

/// Splits the line into words and separators, keeping quoted text whole. An unclosed
/// quote runs to the end of the line, as it does while it is still being typed.
fn words(line: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some(&(at, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if matches!(c, ';' | '|' | '&' | '(' | ')' | '{' | '}') {
            chars.next();
            // `&&`, `||` and `;;` are one separator.
            if chars.peek().is_some_and(|&(_, next)| next == c) {
                chars.next();
            }
            let end = chars.peek().map_or(line.len(), |&(i, _)| i);
            words.push(Word {
                range: at..end,
                text: String::new(),
                separator: true,
            });
            continue;
        }
        let mut text = String::new();
        let mut quote: Option<char> = None;
        while let Some(&(_, c)) = chars.peek() {
            match quote {
                Some(q) if c == q => quote = None,
                Some('"') if c == '\\' => {
                    chars.next();
                    if let Some(&(_, escaped)) = chars.peek() {
                        text.push(escaped);
                    }
                }
                Some(_) => text.push(c),
                None if c == '\'' || c == '"' => quote = Some(c),
                None if c.is_whitespace() || matches!(c, ';' | '|' | '&' | '(' | ')') => {
                    break;
                }
                None if c == '\\' => {
                    chars.next();
                    if let Some(&(_, escaped)) = chars.peek() {
                        text.push(escaped);
                    }
                }
                None => text.push(c),
            }
            chars.next();
        }
        let end = chars.peek().map_or(line.len(), |&(i, _)| i);
        words.push(Word {
            range: at..end,
            text,
            separator: false,
        });
    }
    words
}

/// The command word of a simple command: the first word that is not an assignment, nor
/// a prefix that runs the next word (`sudo` with its options, `command`, `builtin`,
/// `exec`, `nohup`, `time`). `None` when there is none before the word being typed, so a
/// line whose only word is under the cursor has no command yet.
fn command_word(words: &[Word], typing: Option<usize>) -> Option<String> {
    let mut words = words
        .iter()
        .filter(|w| Some(w.range.start) != typing)
        .peekable();
    while let Some(word) = words.next() {
        let text = word.text.as_str();
        let is_assignment = text.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !name.starts_with(|c: char| c.is_ascii_digit())
        });
        if is_assignment || matches!(text, "command" | "builtin" | "exec" | "nohup" | "time") {
            continue;
        }
        if text != "sudo" {
            return Some(text.to_owned());
        }
        // sudo's options, and the user after `-u`.
        while let Some(option) = words.peek().filter(|w| w.text.starts_with('-')) {
            let takes_user = matches!(option.text.as_str(), "-u" | "--user");
            words.next();
            if takes_user {
                words.next();
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `^` marks the cursor.
    fn at(marked: &str) -> Line {
        let cursor = marked.find('^').unwrap();
        Line::read(&marked.replacen('^', "", 1), cursor)
    }

    #[test]
    fn an_empty_line_shows_folders_and_runs() {
        let line = at("^");
        assert_eq!(line.command, None);
        assert_eq!((line.shows(), line.then()), (Shows::Folders, Then::Run));
        assert_eq!(line.word, 0..0);
    }

    #[test]
    fn cd_with_a_partial_word_replaces_it() {
        let line = at("cd ~/src/fo^");
        assert_eq!(line.command.as_deref(), Some("cd"));
        assert_eq!((line.word.clone(), line.text.as_str()), (3..11, "~/src/fo"));
        assert_eq!((line.shows(), line.then()), (Shows::Folders, Then::Run));
    }

    #[test]
    fn a_word_alone_is_not_yet_a_command() {
        // `fo` + Alt-E: the word is what is being typed; there is no command before it.
        let line = at("fo^");
        assert_eq!(line.command, None);
        assert_eq!(line.text, "fo");
    }

    #[test]
    fn other_commands_show_everything_and_stay_open() {
        let line = at("cp a.txt ^");
        assert_eq!(line.command.as_deref(), Some("cp"));
        assert_eq!(line.word, 9..9);
        assert_eq!(
            (line.shows(), line.then()),
            (Shows::Everything, Then::StayOpen)
        );
        assert_eq!(at("source ^").then(), Then::Close);
        assert_eq!(at("rmdir ^").shows(), Shows::Folders);
    }

    #[test]
    fn the_command_is_the_one_the_cursor_is_in() {
        assert_eq!(at("ls | grep x; cd ^").command.as_deref(), Some("cd"));
        assert_eq!(at("make && vim ^").command.as_deref(), Some("vim"));
        assert_eq!(at("cd ^ ; vim x").command.as_deref(), Some("cd"));
    }

    #[test]
    fn prefixes_and_assignments_are_skipped() {
        assert_eq!(at("sudo -u admin cp ^").command.as_deref(), Some("cp"));
        assert_eq!(at("FOO=1 BAR=2 cd ^").command.as_deref(), Some("cd"));
        assert_eq!(at("command cd ^").command.as_deref(), Some("cd"));
        assert_eq!(at("CD ^").shows(), Shows::Folders);
    }

    #[test]
    fn quotes_keep_a_word_whole() {
        let line = at("cd 'My Fi^");
        assert_eq!((line.word.clone(), line.text.as_str()), (3..9, "My Fi"));
        let line = at(r#"vim "a b"/c^"#);
        assert_eq!(line.text, "a b/c");
        let line = at(r"cd My\ Docs^");
        assert_eq!(line.text, "My Docs");
    }
}
