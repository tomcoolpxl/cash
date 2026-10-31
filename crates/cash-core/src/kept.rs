//! What an interactive shell keeps across sessions in `%LOCALAPPDATA%\cash` (spec D79,
//! D80), local rather than roaming because the paths are this machine's:
//!
//! - `history-directories`: the directory each command ran in, beside Bash's history
//!   file, which holds commands and times only; Ctrl-R's picker shows a directory's
//!   commands from it ([`commands`]).
//! - `directories`: the directories arrived in, ranked as zoxide ranks them, for `z` and
//!   Alt-E's Alt-H ([`directories`]).
//!
//! Only an interactive shell keeps them (its startup sets [`Records`]), so scripts and
//! subshells moving about leave no trace; a visit is counted once a prompt, as zoxide's
//! hook counts it. A record that cannot be written is left as it is, said nowhere: it
//! is a convenience, never a reason for a command to fail.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Seconds since the Unix epoch, now.
#[must_use]
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

/// Whether two paths in cash's spelling name the same directory, case aside as Windows has
/// it.
#[must_use]
pub fn same_directory(a: &str, b: &str) -> bool {
    a.trim_end_matches('/').to_lowercase() == b.trim_end_matches('/').to_lowercase()
}

/// Where an interactive shell's records are, and the directory it last counted.
#[derive(Clone, Debug, Default)]
pub struct Records {
    history_directories: Option<PathBuf>,
    directories: Option<PathBuf>,
    noted: Option<String>,
}

impl Records {
    /// The records under `localappdata` (`%LOCALAPPDATA%`); none when it is unset or
    /// empty, in which case nothing is kept. Records under the names cash gave them
    /// before 1.27.0 take the present names here.
    #[must_use]
    pub fn at(localappdata: Option<&str>) -> Self {
        let Some(dir) = localappdata.filter(|dir| !dir.is_empty()) else {
            return Self::default();
        };
        let dir = PathBuf::from(dir).join("cash");
        rename_old_records(&dir);
        Self {
            history_directories: Some(dir.join("history-directories")),
            directories: Some(dir.join("directories")),
            noted: None,
        }
    }

    /// The file of the directories commands ran in, when one is kept.
    #[must_use]
    pub fn history_directories_file(&self) -> Option<&Path> {
        self.history_directories.as_deref()
    }

    /// The file of the directories visited, when one is kept.
    #[must_use]
    pub fn directories_file(&self) -> Option<&Path> {
        self.directories.as_deref()
    }

    /// Notes that `command` ran in `directory`.
    pub(crate) fn note_command(&self, directory: &str, command: &str) {
        if let Some(path) = &self.history_directories {
            let ran = commands::Ran {
                time: now(),
                directory: directory.to_owned(),
                command: command.to_owned(),
            };
            if let Err(error) = commands::append(path, &ran) {
                tracing::debug!("couldn't note the command's directory: {error}");
            }
        }
    }

    /// Counts a visit to `directory` when it is not the directory counted last, as the prompt
    /// comes back.
    pub(crate) fn note_directory(&mut self, directory: &str) {
        let Some(path) = &self.directories else {
            return;
        };
        if self
            .noted
            .as_deref()
            .is_some_and(|noted| same_directory(noted, directory))
        {
            return;
        }
        self.noted = Some(directory.to_owned());
        let mut record = directories::load(path).unwrap_or_default();
        directories::visit(&mut record, directory, now());
        if let Err(error) = directories::save(path, &record) {
            tracing::debug!("couldn't count the directory's visit: {error}");
        }
    }
}

/// Writes `text` to `path` whole, through a file beside it renamed over it, so that a
/// reader never sees half of it and a crash leaves the old one.
fn replace(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut spare = path.as_os_str().to_owned();
    spare.push(format!(".{}", std::process::id()));
    let spare = PathBuf::from(spare);
    fs::write(&spare, text)?;
    fs::rename(&spare, path).inspect_err(|_| {
        let _ = fs::remove_file(&spare);
    })
}

/// Gives the records in `dir` kept under their names before 1.27.0 the present names,
/// when no record has the present name yet. The old names are file names, kept here
/// for this alone.
fn rename_old_records(dir: &Path) {
    for (old, new) in [
        ("history-folders", "history-directories"),
        ("folders", "directories"),
    ] {
        let new = dir.join(new);
        if !new.exists() {
            let _ = fs::rename(dir.join(old), new);
        }
    }
}

/// The directory each command ran in: one line per command, `TIME<TAB>DIRECTORY<TAB>COMMAND`,
/// the command's backslashes, tabs and line ends escaped.
pub mod commands {
    use super::{Path, fs, io, replace};
    use std::io::Write as _;

    /// A command, and where and when it ran.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Ran {
        /// Seconds since the Unix epoch.
        pub time: i64,
        /// The directory, in cash's spelling.
        pub directory: String,
        /// The command line.
        pub command: String,
    }

    fn escape(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '\t' => out.push_str("\\t"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                c => out.push(c),
            }
        }
        out
    }

    fn unescape(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        }
        out
    }

    fn line(ran: &Ran) -> String {
        format!(
            "{}\t{}\t{}\n",
            ran.time,
            escape(&ran.directory),
            escape(&ran.command)
        )
    }

    /// Adds `ran` at the end of the file at `path`, making it and its directory.
    ///
    /// # Errors
    ///
    /// When the file cannot be opened or written.
    pub fn append(path: &Path, ran: &Ran) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        // One write, so that shells appending at once never interleave a line.
        file.write_all(line(ran).as_bytes())
    }

    /// The commands the file at `path` holds, oldest first; none when there is no file.
    /// A line that is not one is skipped.
    ///
    /// # Errors
    ///
    /// When the file is there but cannot be read.
    pub fn read(path: &Path) -> io::Result<Vec<Ran>> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        Ok(text
            .lines()
            .filter_map(|line| {
                let mut fields = line.splitn(3, '\t');
                let time = fields.next()?.parse().ok()?;
                let directory = unescape(fields.next()?);
                let command = unescape(fields.next()?);
                Some(Ran {
                    time,
                    directory,
                    command,
                })
            })
            .collect())
    }

    /// Cuts the file at `path` to its newest `keep` lines, when it has more.
    ///
    /// # Errors
    ///
    /// When it cannot be read or written again.
    pub fn trim(path: &Path, keep: usize) -> io::Result<()> {
        let all = read(path)?;
        if all.len() <= keep {
            return Ok(());
        }
        let text: String = all[all.len() - keep..].iter().map(line).collect();
        replace(path, &text)
    }
}

/// The directories arrived in, ranked as zoxide 0.9 ranks them: one line per directory,
/// `RANK<TAB>LAST<TAB>DIRECTORY`, the rank its visits, aged, and the last its time.
pub mod directories {
    use super::{Path, fs, io, replace, same_directory};
    use std::fmt::Write as _;

    /// When the ranks add up past this, all are scaled down (zoxide's `_ZO_MAXAGE`).
    pub const MAX_AGE: f64 = 10_000.0;

    /// A directory and how often and lately it was visited.
    #[derive(Clone, Debug, PartialEq)]
    pub struct Directory {
        /// The directory, in cash's spelling.
        pub path: String,
        /// Its visits, aged.
        pub rank: f64,
        /// When it was last visited, in seconds since the Unix epoch.
        pub last: i64,
    }

    /// The record at `path`; empty when there is no file. A line that is not one is
    /// skipped.
    ///
    /// # Errors
    ///
    /// When the file is there but cannot be read.
    pub fn load(path: &Path) -> io::Result<Vec<Directory>> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        Ok(text
            .lines()
            .filter_map(|line| {
                let mut fields = line.splitn(3, '\t');
                let rank: f64 = fields.next()?.parse().ok()?;
                let last = fields.next()?.parse().ok()?;
                let path = fields.next()?.to_owned();
                (rank.is_finite() && !path.is_empty()).then_some(Directory { path, rank, last })
            })
            .collect())
    }

    /// Writes `record` to `path`, whole.
    ///
    /// # Errors
    ///
    /// When it cannot be written.
    pub fn save(path: &Path, record: &[Directory]) -> io::Result<()> {
        let mut text = String::new();
        for directory in record {
            let _ = writeln!(
                text,
                "{}\t{}\t{}",
                directory.rank, directory.last, directory.path
            );
        }
        replace(path, &text)
    }

    /// Counts a visit to `directory` at `now`: its rank up by one, its time now; and, when
    /// the ranks add up past [`MAX_AGE`], every rank scaled by 0.9 and those under 1
    /// forgotten.
    pub fn visit(record: &mut Vec<Directory>, directory: &str, now: i64) {
        if let Some(known) = record
            .iter_mut()
            .find(|known| same_directory(&known.path, directory))
        {
            known.rank += 1.0;
            known.last = now;
        } else {
            record.push(Directory {
                path: directory.to_owned(),
                rank: 1.0,
                last: now,
            });
        }
        let total: f64 = record.iter().map(|directory| directory.rank).sum();
        if total > MAX_AGE {
            for directory in record.iter_mut() {
                directory.rank *= 0.9;
            }
            record.retain(|directory| directory.rank >= 1.0);
        }
    }

    /// How highly `directory` ranks at `now`: its rank weighted by the last visit's age,
    /// ×4 within the hour, ×2 the day, ×½ the week, ×¼ older.
    #[must_use]
    pub fn score(directory: &Directory, now: i64) -> f64 {
        let (hour, day, week) = (3_600, 86_400, 604_800);
        let age = now.saturating_sub(directory.last);
        let weight = if age < hour {
            4.0
        } else if age < day {
            2.0
        } else if age < week {
            0.5
        } else {
            0.25
        };
        directory.rank * weight
    }

    /// Whether `path` holds each of `words` in turn, the last within its last part, case
    /// aside (zoxide's rule). No words match every path.
    #[must_use]
    pub fn matches(path: &str, words: &[&str]) -> bool {
        let lower = path.to_lowercase();
        let mut from = 0;
        for word in words {
            let word = word.to_lowercase();
            if word.is_empty() {
                continue;
            }
            match lower.get(from..).and_then(|rest| rest.find(&word)) {
                Some(at) => from += at + word.len(),
                None => return false,
            }
        }
        let Some(last) = words.iter().rev().find(|word| !word.is_empty()) else {
            return true;
        };
        let part = lower
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or_default();
        part.contains(&last.to_lowercase())
    }

    /// The directories of `record` that match `words`, best first, leaving out `here`.
    #[must_use]
    pub fn ranked<'a>(
        record: &'a [Directory],
        words: &[&str],
        now: i64,
        here: Option<&str>,
    ) -> Vec<&'a Directory> {
        let mut found: Vec<&Directory> = record
            .iter()
            .filter(|directory| here.is_none_or(|here| !same_directory(&directory.path, here)))
            .filter(|directory| matches(&directory.path, words))
            .collect();
        found.sort_by(|a, b| score(b, now).total_cmp(&score(a, now)));
        found
    }

    /// How long a directory that is gone stays in the record, in seconds since its last
    /// visit: 90 days, as zoxide keeps one, so a drive unplugged for a while is not lost.
    pub const GONE_FOR: i64 = 90 * 86_400;

    /// The directories of the record at `path` that match `words` and are there, best first.
    ///
    /// Each comes with its score at `now`; `here` is left out. One that is gone is
    /// skipped, and forgotten when it was last visited more than [`GONE_FOR`] ago.
    #[must_use]
    pub fn find(
        path: &Path,
        words: &[&str],
        now: i64,
        here: Option<&str>,
    ) -> Vec<(Directory, f64)> {
        let Ok(mut record) = load(path) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        let mut expired = Vec::new();
        for directory in ranked(&record, words, now, here) {
            if Path::new(&directory.path).is_dir() {
                found.push((directory.clone(), score(directory, now)));
            } else if now.saturating_sub(directory.last) > GONE_FOR {
                expired.push(directory.path.clone());
            }
        }
        if !expired.is_empty() {
            record.retain(|known| !expired.contains(&known.path));
            let _ = save(path, &record);
        }
        found
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            cash_win32::path::temp_dir().join(format!("cash-kept-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn commands_keep_their_directory_line_ends_and_tabs() {
        let dir = scratch("commands");
        let path = dir.join("cash").join("history-directories");
        let ran = commands::Ran {
            time: 1,
            directory: "C:/src/a b".to_owned(),
            command: "for x in 1\tdo\n  echo \\x\ndone".to_owned(),
        };
        commands::append(&path, &ran).unwrap();
        commands::append(
            &path,
            &commands::Ran {
                time: 2,
                directory: "C:/x".to_owned(),
                command: "ls".to_owned(),
            },
        )
        .unwrap();
        let read = commands::read(&path).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0], ran);
        commands::trim(&path, 1).unwrap();
        let read = commands::read(&path).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].command, "ls");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_record_reads_as_empty() {
        let dir = scratch("missing");
        assert_eq!(commands::read(&dir.join("none")).unwrap(), []);
        assert_eq!(directories::load(&dir.join("none")).unwrap(), []);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn visits_add_up_and_age() {
        let mut record = Vec::new();
        directories::visit(&mut record, "C:/src", 100);
        directories::visit(&mut record, "c:/SRC/", 200);
        assert_eq!(record.len(), 1);
        assert!((record[0].rank - 2.0).abs() < f64::EPSILON);
        assert_eq!(record[0].last, 200);
        // Past the maximum, every rank is scaled and the smallest forgotten.
        record.push(directories::Directory {
            path: "C:/big".to_owned(),
            rank: directories::MAX_AGE,
            last: 0,
        });
        directories::visit(&mut record, "C:/new", 300);
        assert!(record.iter().all(|f| f.path != "C:/new"), "{record:?}");
        assert!(record.iter().any(|f| f.path == "C:/src"));
    }

    #[test]
    fn scores_weigh_the_last_visit() {
        let directory = directories::Directory {
            path: "C:/x".to_owned(),
            rank: 4.0,
            last: 1_000_000,
        };
        assert!((directories::score(&directory, 1_000_010) - 16.0).abs() < f64::EPSILON);
        assert!((directories::score(&directory, 1_000_000 + 7_200) - 8.0).abs() < f64::EPSILON);
        assert!((directories::score(&directory, 1_000_000 + 172_800) - 2.0).abs() < f64::EPSILON);
        assert!((directories::score(&directory, 1_000_000 + 1_000_000) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn words_match_in_turn_the_last_in_the_last_part() {
        assert!(directories::matches("C:/src/cash/crates", &["crates"]));
        assert!(directories::matches("C:/src/cash/crates", &["src", "crat"]));
        assert!(directories::matches("C:/Src/Cash", &["CASH"]));
        assert!(!directories::matches("C:/src/cash/crates", &["cash"]));
        assert!(!directories::matches(
            "C:/src/cash/crates",
            &["crates", "src"]
        ));
        assert!(directories::matches("C:/anything", &[]));
    }

    #[test]
    fn the_best_comes_first_and_here_is_left_out() {
        let record = vec![
            directories::Directory {
                path: "C:/a/cash".to_owned(),
                rank: 1.0,
                last: 0,
            },
            directories::Directory {
                path: "C:/b/cash".to_owned(),
                rank: 5.0,
                last: 0,
            },
            directories::Directory {
                path: "C:/c/cash".to_owned(),
                rank: 9.0,
                last: 0,
            },
        ];
        let found = directories::ranked(&record, &["cash"], 10, Some("c:/C/cash"));
        let paths: Vec<&str> = found.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["C:/b/cash", "C:/a/cash"]);
    }

    #[test]
    fn finding_skips_a_gone_directory_and_forgets_it_after_ninety_days() {
        let dir = scratch("find");
        let there = dir.join("there");
        fs::create_dir(&there).unwrap();
        let there = there.to_string_lossy().replace('\\', "/");
        let path = dir.join("directories");
        let now = 1_000_000_000;
        let directory = |path: &str, last: i64| directories::Directory {
            path: path.to_owned(),
            rank: 1.0,
            last,
        };
        let record = vec![
            directory(&there, now),
            directory("C:/gone-lately/there", now - 86_400),
            directory("C:/gone-long-ago/there", now - directories::GONE_FOR - 1),
        ];
        directories::save(&path, &record).unwrap();
        let found = directories::find(&path, &["there"], now, None);
        let paths: Vec<&str> = found.iter().map(|(f, _)| f.path.as_str()).collect();
        assert_eq!(paths, [there.as_str()]);
        let kept: Vec<String> = directories::load(&path)
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .collect();
        assert_eq!(kept, [there, "C:/gone-lately/there".to_owned()]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn records_count_a_directory_once_until_another() {
        let dir = scratch("records");
        let mut records = Records::at(Some(dir.to_str().unwrap()));
        records.note_directory("C:/one");
        records.note_directory("C:/one");
        records.note_directory("C:/two");
        records.note_directory("C:/one");
        let record = directories::load(records.directories_file().unwrap()).unwrap();
        let one = record.iter().find(|f| f.path == "C:/one").unwrap();
        assert!((one.rank - 2.0).abs() < f64::EPSILON, "{record:?}");
        records.note_command("C:/one", "make");
        let ran = commands::read(records.history_directories_file().unwrap()).unwrap();
        assert_eq!(ran[0].directory, "C:/one");
        assert!(Records::at(None).directories_file().is_none());
        assert!(Records::at(Some("")).directories_file().is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn records_under_their_old_names_take_the_present_ones() {
        let dir = scratch("old-names");
        let kept = dir.join("cash");
        fs::create_dir_all(&kept).unwrap();
        fs::write(kept.join("history-folders"), "1\tC:/a\tls\n").unwrap();
        fs::write(kept.join("folders"), "old").unwrap();
        fs::write(kept.join("directories"), "present").unwrap();
        let records = Records::at(Some(dir.to_str().unwrap()));
        let ran = commands::read(records.history_directories_file().unwrap()).unwrap();
        assert_eq!(ran[0].command, "ls");
        assert!(!kept.join("history-folders").exists());
        // A record under the present name is never replaced by an old one.
        assert_eq!(
            fs::read_to_string(kept.join("directories")).unwrap(),
            "present"
        );
        assert!(kept.join("folders").exists());
        let _ = fs::remove_dir_all(dir);
    }
}
