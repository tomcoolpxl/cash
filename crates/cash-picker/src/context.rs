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
        // A word that starts `C:\` keeps its backslashes until its first quote, as the
        // prompt reads it (`shopt winpaths`): `cd C:\Users\me\sr` starts in `C:\Users\me`.
        let mut winpath = line.get(at..).is_some_and(starts_with_drive_backslash);
        // A UNC path's leading `\\` is two backslashes, not an escaped one.
        if winpath && line.get(at..).is_some_and(|rest| rest.starts_with(r"\\")) {
            text.push_str(r"\\");
            chars.next();
            chars.next();
        }
        while let Some(&(_, c)) = chars.peek() {
            match quote {
                Some(q) if c == q => quote = None,
                None if c == '\\' && winpath => {
                    chars.next();
                    // At the line's end too: `cd C:\Users\` is being typed.
                    let separates = chars.peek().is_none_or(|&(_, next)| is_winpath_char(next));
                    if separates {
                        text.push('\\');
                    } else if let Some(&(_, escaped)) = chars.peek() {
                        text.push(escaped);
                        chars.next();
                    }
                    continue;
                }
                Some('"') if c == '\\' => {
                    chars.next();
                    if let Some(&(_, escaped)) = chars.peek() {
                        text.push(escaped);
                    }
                }
                Some(_) => text.push(c),
                None if c == '\'' || c == '"' => {
                    quote = Some(c);
                    winpath = false;
                }
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

/// Whether `word` begins `X:\`, a drive letter, a colon and a backslash, or `\\name`, a
/// UNC path's server.
fn starts_with_drive_backslash(word: &str) -> bool {
    let bytes = word.as_bytes();
    let drive =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    drive
        || word
            .strip_prefix(r"\\")
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '$'))
}

/// Whether a backslash before `c`, in a word that starts `C:\`, is a separator rather
/// than an escape: before what can start a file name, or a wildcard (the shell's rule).
fn is_winpath_char(c: char) -> bool {
    c.is_alphanumeric()
        || matches!(
            c,
            '.' | '_'
                | '-'
                | '$'
                | '@'
                | '+'
                | '%'
                | ','
                | '='
                | '^'
                | '~'
                | '#'
                | '['
                | ']'
                | '{'
                | '}'
                | '*'
                | '?'
        )
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

    #[test]
    fn a_word_starting_with_a_drive_keeps_its_backslashes_as_the_prompt_does() {
        assert_eq!(at(r"cd C:\Users\me\sr^").text, r"C:\Users\me\sr");
        assert_eq!(at(r"cd C:\Users\^").text, r"C:\Users\");
        assert_eq!(at(r"ls C:\logs\*.txt^").text, r"C:\logs\*.txt");
        // Before a space it is the escape it always was, and after a quote too.
        assert_eq!(at(r"cd C:\Program\ Files\Gi^").text, r"C:\Program Files\Gi");
        assert_eq!(at(r"cd C:\x'y'\z^").text, "C:\\xyz");
        // A UNC path's too, its leading `\\` included.
        assert_eq!(at(r"cd \\server\share\di^").text, r"\\server\share\di");
        assert_eq!(at(r"cd \\wsl$\Ubuntu\ho^").text, r"\\wsl$\Ubuntu\ho");
        // A word that starts with neither is read as before.
        assert_eq!(at(r"cd a\b^").text, "ab");
        assert_eq!(at(r"cd \\ x^").text, "x");
    }
}
