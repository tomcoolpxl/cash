//! History management for shells.

use std::path::PathBuf;

use crate::{error, openfiles};

impl<SE: crate::extensions::ShellExtensions> crate::Shell<SE> {
    pub(super) fn load_history(&self) -> Result<Option<crate::history::History>, error::Error> {
        const MAX_FILE_SIZE_FOR_HISTORY_IMPORT: u64 = 1024 * 1024 * 1024; // 1 GiB

        let Some(history_path) = self.history_file_path() else {
            return Ok(None);
        };

        let mut options = std::fs::File::options();
        options.read(true);

        let mut history_file = self.open_file(
            &options,
            crate::sys::fs::Access::Read,
            &history_path,
            &self.default_exec_params(),
        )?;

        // Check on the file's size and remember how much startup consumed so
        // `history -n` can begin at the first subsequently appended byte.
        let mut history_file_len = None;
        if let openfiles::OpenFile::File(file) = &mut history_file {
            let file_metadata = file.metadata()?;
            let file_size = file_metadata.len();
            history_file_len = Some(file_size);

            // If the file is empty, no reason to try reading it. Note that this will also
            // end up excluding non-regular files that report a 0 file size but appear
            // to have contents when read.
            if file_size == 0 {
                return Ok(None);
            }

            // Bail if the file is unrealistically large. For now we just refuse to import it.
            if file_size > MAX_FILE_SIZE_FOR_HISTORY_IMPORT {
                return Err(error::ErrorKind::HistoryFileTooLargeToImport.into());
            }
        }

        let mut history = crate::history::History::import(history_file)?;
        if let Some(file_size) = history_file_len {
            history.mark_file_read_to(&history_path, file_size);
        }

        // As bash does, stamp entries that carry no timestamp in the file with the load time.
        let load_time = chrono::Utc::now();
        let unstamped: Vec<_> = history
            .iter()
            .filter(|item| item.timestamp.is_none())
            .map(|item| (item.id, item.clone()))
            .collect();
        for (id, mut item) in unstamped {
            item.timestamp = Some(load_time);
            history.update_by_id(id, item)?;
        }

        Ok(Some(history))
    }

    /// Returns the path to the history file used by the shell, if one is set.
    pub fn history_file_path(&self) -> Option<PathBuf> {
        self.env_str("HISTFILE")
            .filter(|s| !s.is_empty())
            .map(|s| PathBuf::from(s.into_owned()))
    }

    /// Returns the path to the history file used by the shell, if one is set.
    pub fn history_time_format(&self) -> Option<String> {
        self.env_str("HISTTIMEFORMAT").map(|s| s.into_owned())
    }

    /// Saves history back to any backing storage.
    pub fn save_history(&mut self) -> Result<(), error::Error> {
        if let Some(history_file_path) = self.history_file_path()
            && let Some(history) = &mut self.history
        {
            // See if there's *any* time format configured. That triggers writing out
            // timestamps.
            let write_timestamps = self.env.is_set("HISTTIMEFORMAT");

            // TODO(history): Observe options.append_to_history_file
            history.flush(
                history_file_path,
                true, /* append? */
                true, /* unsaved items only? */
                write_timestamps,
            )?;
        }

        Ok(())
    }

    /// How many command lines the shell has read at the prompt.
    pub(crate) const fn commands_read(&self) -> usize {
        self.commands_read
    }

    /// Adds a command line read at the prompt to history, as Bash does: not with
    /// `set +o history`, and not when `HISTCONTROL` or `HISTIGNORE` keeps it out. The list
    /// then keeps at most `HISTSIZE` entries.
    ///
    /// `HISTCONTROL=ignorespace` is how a user keeps a secret out of a history file that
    /// cash appends to at once (D44): ` export TOKEN=…`, typed with a leading space.
    pub fn add_to_history(&mut self, command: &str) -> Result<(), error::Error> {
        self.commands_read += 1;

        if !self.options.enable_command_history || self.history.is_none() {
            return Ok(());
        }

        let control = HistoryControl::parse(&self.env_str("HISTCONTROL").unwrap_or_default());
        if control.ignore_space && command.starts_with(' ') {
            return Ok(());
        }

        let command = command.trim();
        if command.is_empty() {
            return Ok(());
        }

        let previous = self.history.as_ref().and_then(|history| {
            history
                .count()
                .checked_sub(1)
                .and_then(|last| history.get(last))
                .map(|item| item.command_line.clone())
        });
        if control.ignore_dups && previous.as_deref() == Some(command) {
            return Ok(());
        }
        if self.history_ignores(command, previous.as_deref())? {
            return Ok(());
        }

        let max_items = self.history_size_limit("HISTSIZE");
        if let Some(history) = &mut self.history {
            if control.erase_dups {
                history.remove_matching(command);
            }
            history.add(crate::history::Item {
                id: 0,
                command_line: command.to_owned(),
                timestamp: Some(chrono::Utc::now()),
                dirty: true,
            })?;
            if let Some(max_items) = max_items {
                history.keep_newest(max_items);
            }
        }
        // cash (D79): where it ran, for Ctrl-R's "this folder".
        let folder = cash_win32::path::render(self.working_dir());
        self.records.note_command(&folder, command);

        Ok(())
    }

    /// Counts a visit to the current folder, once a prompt, when it is not the one counted
    /// last (D80).
    pub fn note_folder_visit(&mut self) {
        let folder = cash_win32::path::render(self.working_dir());
        self.records.note_folder(&folder);
    }

    /// The records an interactive shell keeps across sessions (D79, D80).
    pub const fn records(&self) -> &crate::kept::Records {
        &self.records
    }

    /// The record of the folders visited, for `z` and Alt-E's Alt-H (D80): the one an
    /// interactive shell keeps, or, in a script, the one it would keep, read only; `None`
    /// when `CASH_NO_RECORDS` is set or `%LOCALAPPDATA%` is not.
    pub fn folder_record(&self) -> Option<PathBuf> {
        if let Some(file) = self.records.folders_file() {
            return Some(file.to_path_buf());
        }
        if self
            .env_str("CASH_NO_RECORDS")
            .is_some_and(|value| !value.is_empty())
        {
            return None;
        }
        let localappdata = self.env_str("LOCALAPPDATA").map(|value| value.into_owned());
        crate::kept::Records::at(localappdata.as_deref())
            .folders_file()
            .map(std::path::Path::to_path_buf)
    }

    /// Whether a pattern of `HISTIGNORE` matches the whole of `command`. The patterns are
    /// separated by colons (`\:` is a colon in one), and `&` stands for the previous entry.
    fn history_ignores(&self, command: &str, previous: Option<&str>) -> Result<bool, error::Error> {
        let Some(patterns) = self.env_str("HISTIGNORE").filter(|p| !p.is_empty()) else {
            return Ok(false);
        };
        let extglob = self.options.extended_globbing;
        for pattern in split_unescaped_colons(&patterns) {
            if pattern == "&" {
                if previous == Some(command) {
                    return Ok(true);
                }
                continue;
            }
            let pattern =
                crate::patterns::Pattern::from(pattern.as_str()).set_extended_globbing(extglob);
            if pattern.exactly_matches(command)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The limit a history size variable sets: a number of 0 or more. Unset, empty, not a
    /// number or negative means no limit, as in Bash.
    pub(crate) fn history_size_limit(&self, name: &str) -> Option<usize> {
        self.env_str(name)
            .and_then(|value| value.trim().parse::<i64>().ok())
            .and_then(|n| usize::try_from(n).ok())
    }

    /// Performs history expansion on the given command line string using the shell's current history.
    pub fn expand_history(
        &self,
        line: &str,
    ) -> Result<crate::history::HistoryExpansionResult, crate::history::HistoryExpansionError> {
        crate::history::expand_history(line, self.history.as_ref())
    }
}

/// What `HISTCONTROL`'s colon-separated words ask for.
#[derive(Default)]
struct HistoryControl {
    ignore_space: bool,
    ignore_dups: bool,
    erase_dups: bool,
}

impl HistoryControl {
    fn parse(value: &str) -> Self {
        let mut control = Self::default();
        for word in value.split(':') {
            match word {
                "ignorespace" => control.ignore_space = true,
                "ignoredups" => control.ignore_dups = true,
                "ignoreboth" => {
                    control.ignore_space = true;
                    control.ignore_dups = true;
                }
                "erasedups" => control.erase_dups = true,
                _ => (),
            }
        }
        control
    }
}

/// `value` split at each colon that has no backslash before it, with `\:` made a colon.
fn split_unescaped_colons(value: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&':') => {
                if let Some(colon) = chars.next()
                    && let Some(part) = parts.last_mut()
                {
                    part.push(colon);
                }
            }
            ':' => parts.push(String::new()),
            _ => {
                if let Some(part) = parts.last_mut() {
                    part.push(c);
                }
            }
        }
    }
    parts.retain(|part| !part.is_empty());
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histcontrol_words() {
        let control = HistoryControl::parse("ignoreboth:erasedups");
        assert!(control.ignore_space && control.ignore_dups && control.erase_dups);
        let control = HistoryControl::parse("ignorespace");
        assert!(control.ignore_space && !control.ignore_dups && !control.erase_dups);
        let control = HistoryControl::parse("");
        assert!(!control.ignore_space && !control.ignore_dups && !control.erase_dups);
    }

    /// A shell that records history, with `vars` set.
    async fn shell_with(vars: &[(&str, &str)]) -> crate::Shell {
        let mut shell = crate::Shell::builder().build().await.unwrap();
        shell.options_mut().enable_command_history = true;
        let _ = shell.history_mut();
        for (name, value) in vars {
            shell
                .env_mut()
                .set_global(*name, crate::ShellVariable::new(*value))
                .unwrap();
        }
        shell
    }

    fn lines(shell: &crate::Shell) -> Vec<String> {
        shell
            .history()
            .map(|h| h.iter().map(|item| item.command_line.clone()).collect())
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn a_line_with_a_leading_space_is_kept_out_with_ignorespace() {
        let mut shell = shell_with(&[("HISTCONTROL", "ignoreboth")]).await;
        shell.add_to_history(" export TOKEN=secret").unwrap();
        shell.add_to_history("ls").unwrap();
        shell.add_to_history("ls").unwrap();
        assert_eq!(lines(&shell), ["ls"]);
    }

    #[tokio::test]
    async fn without_histcontrol_every_line_is_kept() {
        let mut shell = shell_with(&[]).await;
        shell.add_to_history(" ls").unwrap();
        shell.add_to_history("ls").unwrap();
        assert_eq!(lines(&shell), ["ls", "ls"]);
    }

    #[tokio::test]
    async fn erasedups_keeps_only_the_newest_copy() {
        let mut shell = shell_with(&[("HISTCONTROL", "erasedups")]).await;
        for line in ["a", "b", "a"] {
            shell.add_to_history(line).unwrap();
        }
        assert_eq!(lines(&shell), ["b", "a"]);
    }

    #[tokio::test]
    async fn histignore_patterns_keep_lines_out() {
        let mut shell = shell_with(&[("HISTIGNORE", "ls:[bf]g:exit *:&")]).await;
        for line in ["ls", "ls -l", "bg", "exit 1", "pwd", "pwd"] {
            shell.add_to_history(line).unwrap();
        }
        assert_eq!(lines(&shell), ["ls -l", "pwd"]);
    }

    #[tokio::test]
    async fn histsize_bounds_the_list() {
        let mut shell = shell_with(&[("HISTSIZE", "2")]).await;
        for line in ["a", "b", "c"] {
            shell.add_to_history(line).unwrap();
        }
        assert_eq!(lines(&shell), ["b", "c"]);
    }

    #[tokio::test]
    async fn set_plus_o_history_stops_recording() {
        let mut shell = shell_with(&[]).await;
        shell.options_mut().enable_command_history = false;
        shell.add_to_history("ls").unwrap();
        assert!(lines(&shell).is_empty(), "{:?}", lines(&shell));
    }

    #[test]
    fn histignore_splits_at_unescaped_colons() {
        assert_eq!(split_unescaped_colons("ls:&:[bf]g"), ["ls", "&", "[bf]g"]);
        assert_eq!(
            split_unescaped_colons(r"echo a\:b:pwd"),
            ["echo a:b", "pwd"]
        );
        assert_eq!(split_unescaped_colons("::x"), ["x"]);
    }
}
