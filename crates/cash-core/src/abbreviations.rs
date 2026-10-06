//! Fish-style abbreviations: words that expand in place at the prompt (spec D60).
//!
//! An alias is replaced when the command runs, so history keeps the alias. An abbreviation
//! is replaced on the line as it is typed, when Space or Enter follows it, so the command
//! that runs, and the one history keeps, is the full one. The shell only stores them; the
//! interactive line editor does the expanding.

/// Where on the line an abbreviation expands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Position {
    /// Only as the command word, as fish does by default.
    #[default]
    Command,
    /// As any word outside quotes.
    Anywhere,
}

/// One abbreviation.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Abbreviation {
    /// The word that is typed.
    pub name: String,
    /// The text it becomes.
    pub expansion: String,
    /// Where it expands.
    pub position: Position,
}

/// The shell's abbreviations, in the order they were first defined.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Abbreviations {
    entries: Vec<Abbreviation>,
}

impl Abbreviations {
    /// The abbreviation named `name`, if there is one.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Abbreviation> {
        self.entries.iter().find(|a| a.name == name)
    }

    /// Defines `abbreviation`, replacing one of the same name where it stood.
    pub fn set(&mut self, abbreviation: Abbreviation) {
        match self
            .entries
            .iter_mut()
            .find(|a| a.name == abbreviation.name)
        {
            Some(existing) => *existing = abbreviation,
            None => self.entries.push(abbreviation),
        }
    }

    /// Removes the abbreviation named `name`; whether there was one.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|a| a.name != name);
        self.entries.len() != before
    }

    /// Renames `old` to `new`, keeping its place. Fails when `old` does not exist or
    /// `new` already does.
    pub fn rename(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        if self.get(new).is_some() {
            return Err(RenameError::NewExists);
        }
        let entry = self
            .entries
            .iter_mut()
            .find(|a| a.name == old)
            .ok_or(RenameError::OldMissing)?;
        new.clone_into(&mut entry.name);
        Ok(())
    }

    /// All abbreviations, in the order they were first defined.
    pub fn iter(&self) -> impl Iterator<Item = &Abbreviation> {
        self.entries.iter()
    }

    /// Whether there are none.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Why [`Abbreviations::rename`] failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenameError {
    /// There is no abbreviation by the old name.
    OldMissing,
    /// An abbreviation by the new name exists already.
    NewExists,
}

/// Whether `name` can be an abbreviation: one word, as the line editor splits words.
#[must_use]
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty() && !name.chars().any(char::is_whitespace)
}

/// The file abbreviations are kept in across sessions, `%APPDATA%\cash\abbreviations`.
///
/// One `NAME=EXPANSION` per line, `anywhere NAME=EXPANSION` for one that expands
/// anywhere on the line, LF-separated, written whole on every change. `abbr -a` and
/// `abbr -e` write it; an interactive shell reads it after the rc files, so a
/// definition in `~/.bashrc` wins over the file's for the same name. It is state, not
/// configuration: `--no-config` leaves it alone, and the folder is made when needed.
pub mod store {
    use std::path::PathBuf;

    use super::{Abbreviation, Abbreviations, Position};

    /// The file's name under `%APPDATA%\cash`.
    const FILE_NAME: &str = "abbreviations";

    /// Where the file goes: `%APPDATA%\cash\abbreviations`, or `None` when `APPDATA` is
    /// not set, in which case nothing is kept.
    #[must_use]
    pub fn path(appdata: Option<&str>) -> Option<PathBuf> {
        let appdata = appdata?;
        if appdata.is_empty() {
            return None;
        }
        Some(PathBuf::from(appdata).join("cash").join(FILE_NAME))
    }

    /// A line of the file that is not `NAME=EXPANSION`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ParseError {
        /// The line's number, from 1.
        pub line: usize,
    }

    impl std::fmt::Display for ParseError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "line {}: not NAME=EXPANSION", self.line)
        }
    }

    /// The abbreviations `text` holds, in the file's order.
    ///
    /// # Errors
    ///
    /// The first line that is not `NAME=EXPANSION` (blank lines are skipped).
    pub fn parse(text: &str) -> Result<Vec<Abbreviation>, ParseError> {
        let mut entries = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() {
                continue;
            }
            let error = || ParseError { line: index + 1 };
            let (name, expansion) = line.split_once('=').ok_or_else(error)?;
            let (position, name) = match name.split_once(' ') {
                None => (Position::Command, name),
                Some(("anywhere", name)) => (Position::Anywhere, name),
                Some(_) => return Err(error()),
            };
            if !super::is_valid_name(name) {
                return Err(error());
            }
            entries.push(Abbreviation {
                name: name.to_owned(),
                expansion: expansion.to_owned(),
                position,
            });
        }
        Ok(entries)
    }

    /// Whether `abbreviation` can be a line of the file: a name without `=` or a space,
    /// and no newline in either part. One that cannot is kept for the session alone.
    #[must_use]
    pub fn representable(abbreviation: &Abbreviation) -> bool {
        !abbreviation.name.contains(['=', ' ', '\n', '\r'])
            && !abbreviation.expansion.contains(['\n', '\r'])
    }

    /// The file's text for `entries`, the ones [`representable`] left out.
    #[must_use]
    pub fn render<'a>(entries: impl IntoIterator<Item = &'a Abbreviation>) -> String {
        let mut text = String::new();
        for entry in entries.into_iter().filter(|entry| representable(entry)) {
            if entry.position == Position::Anywhere {
                text.push_str("anywhere ");
            }
            text.push_str(&entry.name);
            text.push('=');
            text.push_str(&entry.expansion);
            text.push('\n');
        }
        text
    }

    /// Why the file could not be read or written.
    #[derive(Debug)]
    pub enum StoreError {
        /// `APPDATA` is not set, so there is nowhere to keep them.
        NoPlace,
        /// The file or its folder could not be read or written.
        Io(PathBuf, std::io::Error),
        /// The file does not parse.
        Parse(PathBuf, ParseError),
    }

    impl std::fmt::Display for StoreError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::NoPlace => write!(f, "APPDATA is not set, so they cannot be kept"),
                Self::Io(path, error) => write!(
                    f,
                    "{}: {}",
                    cash_win32::path::render(path),
                    crate::error::os_error_text(error)
                ),
                Self::Parse(path, error) => {
                    write!(f, "{}: {error}", cash_win32::path::render(path))
                }
            }
        }
    }

    /// The abbreviations the file holds; none when there is no file yet.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the file cannot be read or does not parse.
    pub fn load(appdata: Option<&str>) -> Result<Vec<Abbreviation>, StoreError> {
        let path = path(appdata).ok_or(StoreError::NoPlace)?;
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(StoreError::Io(path, error)),
        };
        parse(&text).map_err(|error| StoreError::Parse(path, error))
    }

    /// Applies `change` to what the file holds and writes it back whole.
    ///
    /// So a shell that has not read the file (one running its rc file) loses nothing of
    /// it. A file that does not parse is written over: its lines were already refused
    /// once.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the file cannot be read or written.
    pub fn save_change(
        appdata: Option<&str>,
        change: impl FnOnce(&mut Abbreviations),
    ) -> Result<(), StoreError> {
        let path = path(appdata).ok_or(StoreError::NoPlace)?;
        let mut kept = Abbreviations::default();
        match load(appdata) {
            Ok(entries) => kept.entries = entries,
            Err(StoreError::Parse(..)) => {}
            Err(error) => return Err(error),
        }
        change(&mut kept);
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(|error| StoreError::Io(path.clone(), error))?;
        }
        std::fs::write(&path, render(kept.iter())).map_err(|error| StoreError::Io(path, error))
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used, reason = "tests assert loudly on failure")]
    mod tests {
        use super::*;

        #[test]
        fn the_file_is_one_name_and_expansion_per_line() {
            let text = "gco=git checkout\n\nanywhere L=| less\neq=a=b\n";
            let entries = parse(text).unwrap();
            assert_eq!(
                entries,
                vec![
                    Abbreviation {
                        name: "gco".into(),
                        expansion: "git checkout".into(),
                        position: Position::Command,
                    },
                    Abbreviation {
                        name: "L".into(),
                        expansion: "| less".into(),
                        position: Position::Anywhere,
                    },
                    Abbreviation {
                        name: "eq".into(),
                        expansion: "a=b".into(),
                        position: Position::Command,
                    },
                ]
            );
            assert_eq!(render(&entries), text.replace("\n\n", "\n"));
            // A CRLF file reads the same.
            assert_eq!(parse(&text.replace('\n', "\r\n")).unwrap(), entries);
        }

        #[test]
        fn a_line_that_is_not_a_definition_names_its_number() {
            assert_eq!(
                parse("gco=git checkout\nnonsense\n"),
                Err(ParseError { line: 2 })
            );
            assert_eq!(parse("elsewhere x=y\n"), Err(ParseError { line: 1 }));
            assert_eq!(parse("=y\n"), Err(ParseError { line: 1 }));
        }

        #[test]
        fn what_cannot_be_a_line_is_left_out() {
            let odd = Abbreviation {
                name: "a=b".into(),
                expansion: "x".into(),
                position: Position::Command,
            };
            let multi = Abbreviation {
                name: "m".into(),
                expansion: "two\nlines".into(),
                position: Position::Command,
            };
            assert!(!representable(&odd));
            assert!(!representable(&multi));
            assert_eq!(render([&odd, &multi]), "");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abbr(name: &str, expansion: &str) -> Abbreviation {
        Abbreviation {
            name: name.into(),
            expansion: expansion.into(),
            position: Position::Command,
        }
    }

    #[test]
    fn redefining_keeps_the_original_place() {
        let mut all = Abbreviations::default();
        all.set(abbr("gco", "git checkout"));
        all.set(abbr("gst", "git status"));
        all.set(abbr("gco", "git switch"));

        let names: Vec<_> = all.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["gco", "gst"]);
        assert_eq!(all.get("gco").unwrap().expansion, "git switch");
    }

    #[test]
    fn remove_and_rename() {
        let mut all = Abbreviations::default();
        all.set(abbr("gco", "git checkout"));
        all.set(abbr("gst", "git status"));

        assert_eq!(all.rename("gco", "gst"), Err(RenameError::NewExists));
        assert_eq!(all.rename("nope", "x"), Err(RenameError::OldMissing));
        assert_eq!(all.rename("gco", "co"), Ok(()));
        assert_eq!(all.iter().next().unwrap().name, "co");

        assert!(all.remove("co"));
        assert!(!all.remove("co"));
        assert!(all.get("gst").is_some());
    }

    #[test]
    fn a_name_is_one_word() {
        assert!(is_valid_name("gco"));
        assert!(!is_valid_name(""));
        assert!(!is_valid_name("git co"));
    }
}
