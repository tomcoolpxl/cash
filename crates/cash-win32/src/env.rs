//! Environment variables — **D5**, **D31**.
//!
//! Two separate problems that both bite immediately on Windows.
//!
//! **D31 — case.** Windows environment blocks conventionally use `Path`, `ProgramFiles`,
//! `Temp`, `UserProfile`. Bash lookup is case-sensitive. §9 measured the consequence:
//! `$PATH` and `$TEMP` resolve, `$Path` and `$Temp` come back empty. Left alone on a
//! less forgiving environment block, `$PATH` itself would be empty — probably the
//! highest-frequency breakage available.
//!
//! **D5 — `PATH` is the one variable cash translates.** Scripts see the Unix form,
//! colon-separated with `/c/...`, so `IFS=: read -ra dirs <<< "$PATH"` works. Children
//! get the Windows form, semicolon-separated, so `terraform.exe` understands it. No
//! other variable is touched: `GOPATH`, `PYTHONPATH` and `CLASSPATH` pass through
//! verbatim, because a known-list would be a maintenance surface and a source of silent
//! surprise.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;

use crate::path;

/// POSIX names that normalise to uppercase on import (D31), so scripts see the spelling
/// they expect regardless of how Windows spelled it.
const POSIX_NAMES: &[&str] = &[
    "PATH", "HOME", "TMPDIR", "TMP", "TEMP", "USER", "USERNAME", "SHELL", "PWD", "OLDPWD",
    "LANG", "LC_ALL", "TERM", "EDITOR", "VISUAL", "HOSTNAME", "PATHEXT", "COMSPEC",
    "USERPROFILE", "APPDATA", "LOCALAPPDATA", "PROGRAMFILES", "SYSTEMROOT", "WINDIR",
];

/// Canonicalise a variable name for storage (D31).
///
/// Well-known POSIX names become uppercase; everything else keeps the spelling it
/// arrived with, since inventing a canonical form for arbitrary names would be guessing.
#[must_use]
pub fn canonical_name(name: &str) -> Cow<'_, str> {
    let upper = name.to_ascii_uppercase();
    if POSIX_NAMES.contains(&upper.as_str()) {
        Cow::Owned(upper)
    } else {
        Cow::Borrowed(name)
    }
}

/// A case-insensitive environment, matching how Windows actually behaves.
#[derive(Debug, Default, Clone)]
pub struct Environment {
    /// Keyed by uppercase name; the value carries the display spelling.
    vars: BTreeMap<String, (String, String)>,
}

impl Environment {
    /// An empty environment.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Capture the current process environment, canonicalising names per D31.
    #[must_use]
    pub fn from_process() -> Self {
        let mut env = Self::new();
        for (key, value) in std::env::vars() {
            env.set(&key, &value);
        }
        env
    }

    /// Set a variable. Lookup is case-insensitive, so this overwrites any spelling.
    pub fn set(&mut self, name: &str, value: &str) {
        let display = canonical_name(name).into_owned();
        let key = display.to_ascii_uppercase();
        self.vars.insert(key, (display, value.to_string()));
    }

    /// Look a variable up, ignoring case (D31).
    ///
    /// `$PATH`, `$Path` and `$path` all resolve to the same variable — because on
    /// Windows they *are* the same variable.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.vars.get(&name.to_ascii_uppercase()).map(|(_, v)| v.as_str())
    }

    /// Remove a variable, ignoring case.
    pub fn remove(&mut self, name: &str) -> Option<String> {
        self.vars.remove(&name.to_ascii_uppercase()).map(|(_, v)| v)
    }

    /// Whether a variable exists, ignoring case.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.vars.contains_key(&name.to_ascii_uppercase())
    }

    /// Iterate over `(display name, value)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.vars.values().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// The value a *script* should see for `name` (D5).
    ///
    /// Identical to [`get`](Self::get) for everything except `PATH`, which is rendered
    /// Unix-style so scripts can split it on `:`.
    #[must_use]
    pub fn get_for_script(&self, name: &str) -> Option<Cow<'_, str>> {
        let value = self.get(name)?;
        if name.eq_ignore_ascii_case("PATH") {
            Some(Cow::Owned(path_to_unix(value)))
        } else {
            Some(Cow::Borrowed(value))
        }
    }

    /// Build the environment block for a child process (D5).
    ///
    /// `PATH` is converted to the semicolon-separated Windows form so that native
    /// executables understand it. Everything else is passed through verbatim, per D5.
    #[must_use]
    pub fn to_child_block(&self) -> Vec<(String, String)> {
        self.iter()
            .map(|(name, value)| {
                if name.eq_ignore_ascii_case("PATH") {
                    (name.to_string(), path_to_windows(value))
                } else {
                    (name.to_string(), value.to_string())
                }
            })
            .collect()
    }
}

/// Render a `PATH` in the Unix form scripts expect: `/c/tools:/c/Windows` (D5).
#[must_use]
pub fn path_to_unix(value: &str) -> String {
    split_path(value)
        .map(|entry| path::to_unix(Path::new(entry)))
        .collect::<Vec<_>>()
        .join(":")
}

/// Render a `PATH` in the Windows form native executables expect (D5).
#[must_use]
pub fn path_to_windows(value: &str) -> String {
    split_path(value)
        .map(|entry| path::render(&path::accept_path(entry)).replace('/', "\\"))
        .collect::<Vec<_>>()
        .join(";")
}

/// Split a `PATH` value into entries, accepting either separator.
///
/// Splitting a Unix-form value on `:` is not naive: an entry may still be spelled
/// `C:/tools`, because a script is free to write `PATH=C:/tools:$PATH`. A single
/// alphabetic segment followed by one starting with a slash is a drive letter, not a
/// separator, so the two are rejoined.
pub fn split_path(value: &str) -> impl Iterator<Item = &str> {
    let entries: Vec<&str> = if value.contains(';') {
        // Semicolons are unambiguous: this is the Windows form.
        value.split(';').filter(|s| !s.is_empty()).collect()
    } else {
        let raw: Vec<&str> = value.split(':').collect();
        let mut merged: Vec<&str> = Vec::with_capacity(raw.len());
        let mut index = 0;
        while index < raw.len() {
            let segment = raw[index];
            let is_drive_letter = segment.len() == 1
                && segment.as_bytes()[0].is_ascii_alphabetic()
                && raw
                    .get(index + 1)
                    .is_some_and(|next| next.starts_with('/') || next.starts_with('\\'));

            if is_drive_letter {
                // Rejoin `C` + `/tools` into the original `C:/tools` slice.
                let start = segment.as_ptr() as usize - value.as_ptr() as usize;
                let next = raw[index + 1];
                let end = next.as_ptr() as usize - value.as_ptr() as usize + next.len();
                merged.push(&value[start..end]);
                index += 2;
            } else {
                merged.push(segment);
                index += 1;
            }
        }
        merged.into_iter().filter(|s| !s.is_empty()).collect()
    };

    entries.into_iter()
}
