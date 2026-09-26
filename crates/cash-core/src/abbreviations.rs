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
