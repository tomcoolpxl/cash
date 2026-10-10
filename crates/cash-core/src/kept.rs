//! What an interactive shell keeps across sessions in `%LOCALAPPDATA%\cash` (spec D79,
//! D80), local rather than roaming because the paths are this machine's:
//!
//! - `history-folders`: the folder each command ran in, beside Bash's history file,
//!   which holds commands and times only; Ctrl-R's picker shows a folder's commands
//!   from it ([`commands`]).
//! - `folders`: the folders arrived in, ranked as zoxide ranks them, for `z` and Alt-E's
//!   Alt-H ([`folders`]).
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

/// Whether two paths in cash's spelling name the same folder, case aside as Windows has
/// it.
#[must_use]
pub fn same_folder(a: &str, b: &str) -> bool {
    a.trim_end_matches('/').to_lowercase() == b.trim_end_matches('/').to_lowercase()
}

/// Where an interactive shell's records are, and the folder it last counted.
#[derive(Clone, Debug, Default)]
pub struct Records {
    history_folders: Option<PathBuf>,
    folders: Option<PathBuf>,
    noted: Option<String>,
}

impl Records {
    /// The records under `localappdata` (`%LOCALAPPDATA%`); none when it is unset or
    /// empty, in which case nothing is kept.
    #[must_use]
    pub fn at(localappdata: Option<&str>) -> Self {
        let Some(dir) = localappdata.filter(|dir| !dir.is_empty()) else {
            return Self::default();
        };
        let dir = PathBuf::from(dir).join("cash");
        Self {
            history_folders: Some(dir.join("history-folders")),
            folders: Some(dir.join("folders")),
            noted: None,
        }
    }

    /// The file of the folders commands ran in, when one is kept.
    #[must_use]
    pub fn history_folders_file(&self) -> Option<&Path> {
        self.history_folders.as_deref()
    }

    /// The file of the folders visited, when one is kept.
    #[must_use]
    pub fn folders_file(&self) -> Option<&Path> {
        self.folders.as_deref()
    }

    /// Notes that `command` ran in `folder`.
    pub(crate) fn note_command(&self, folder: &str, command: &str) {
        if let Some(path) = &self.history_folders {
            let ran = commands::Ran {
                time: now(),
                folder: folder.to_owned(),
                command: command.to_owned(),
            };
            if let Err(error) = commands::append(path, &ran) {
                tracing::debug!("couldn't note the command's folder: {error}");
            }
        }
    }

    /// Counts a visit to `folder` when it is not the folder counted last, as the prompt
    /// comes back.
    pub(crate) fn note_folder(&mut self, folder: &str) {
        let Some(path) = &self.folders else {
            return;
        };
        if self
            .noted
            .as_deref()
            .is_some_and(|noted| same_folder(noted, folder))
        {
            return;
        }
        self.noted = Some(folder.to_owned());
        let mut record = folders::load(path).unwrap_or_default();
        folders::visit(&mut record, folder, now());
        if let Err(error) = folders::save(path, &record) {
            tracing::debug!("couldn't count the folder's visit: {error}");
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

/// The folder each command ran in: one line per command, `TIME<TAB>FOLDER<TAB>COMMAND`,
/// the command's backslashes, tabs and line ends escaped.
pub mod commands {
    use super::{Path, fs, io, replace};
    use std::io::Write as _;

    /// A command, and where and when it ran.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Ran {
        /// Seconds since the Unix epoch.
        pub time: i64,
        /// The folder, in cash's spelling.
        pub folder: String,
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
            escape(&ran.folder),
            escape(&ran.command)
        )
    }

    /// Adds `ran` at the end of the file at `path`, making it and its folder.
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
                let folder = unescape(fields.next()?);
                let command = unescape(fields.next()?);
                Some(Ran {
                    time,
                    folder,
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

/// The folders arrived in, ranked as zoxide 0.9 ranks them: one line per folder,
/// `RANK<TAB>LAST<TAB>FOLDER`, the rank its visits, aged, and the last its time.
pub mod folders {
    use super::{Path, fs, io, replace, same_folder};
    use std::fmt::Write as _;

    /// When the ranks add up past this, all are scaled down (zoxide's `_ZO_MAXAGE`).
    pub const MAX_AGE: f64 = 10_000.0;

    /// A folder and how often and lately it was visited.
    #[derive(Clone, Debug, PartialEq)]
    pub struct Folder {
        /// The folder, in cash's spelling.
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
    pub fn load(path: &Path) -> io::Result<Vec<Folder>> {
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
                (rank.is_finite() && !path.is_empty()).then_some(Folder { path, rank, last })
            })
            .collect())
    }

    /// Writes `record` to `path`, whole.
    ///
    /// # Errors
    ///
    /// When it cannot be written.
    pub fn save(path: &Path, record: &[Folder]) -> io::Result<()> {
        let mut text = String::new();
        for folder in record {
            let _ = writeln!(text, "{}\t{}\t{}", folder.rank, folder.last, folder.path);
        }
        replace(path, &text)
    }

    /// Counts a visit to `folder` at `now`: its rank up by one, its time now; and, when
    /// the ranks add up past [`MAX_AGE`], every rank scaled by 0.9 and those under 1
    /// forgotten.
    pub fn visit(record: &mut Vec<Folder>, folder: &str, now: i64) {
        if let Some(known) = record
            .iter_mut()
            .find(|known| same_folder(&known.path, folder))
        {
            known.rank += 1.0;
            known.last = now;
        } else {
            record.push(Folder {
                path: folder.to_owned(),
                rank: 1.0,
                last: now,
            });
        }
        let total: f64 = record.iter().map(|folder| folder.rank).sum();
        if total > MAX_AGE {
            for folder in record.iter_mut() {
                folder.rank *= 0.9;
            }
            record.retain(|folder| folder.rank >= 1.0);
        }
    }

    /// How highly `folder` ranks at `now`: its rank weighted by the last visit's age,
    /// ×4 within the hour, ×2 the day, ×½ the week, ×¼ older.
    #[must_use]
    pub fn score(folder: &Folder, now: i64) -> f64 {
        let (hour, day, week) = (3_600, 86_400, 604_800);
        let age = now.saturating_sub(folder.last);
        let weight = if age < hour {
            4.0
        } else if age < day {
            2.0
        } else if age < week {
            0.5
        } else {
            0.25
        };
        folder.rank * weight
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

    /// The folders of `record` that match `words`, best first, leaving out `here`.
    #[must_use]
    pub fn ranked<'a>(
        record: &'a [Folder],
        words: &[&str],
        now: i64,
        here: Option<&str>,
    ) -> Vec<&'a Folder> {
        let mut found: Vec<&Folder> = record
            .iter()
            .filter(|folder| here.is_none_or(|here| !same_folder(&folder.path, here)))
            .filter(|folder| matches(&folder.path, words))
            .collect();
        found.sort_by(|a, b| score(b, now).total_cmp(&score(a, now)));
        found
    }

    /// Forgets `folder`, gone since it was visited, in the record at `path`.
    pub fn forget(path: &Path, folder: &str) {
        if let Ok(mut record) = load(path) {
            let before = record.len();
            record.retain(|known| !same_folder(&known.path, folder));
            if record.len() != before {
                let _ = save(path, &record);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cash-kept-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn commands_keep_their_folder_line_ends_and_tabs() {
        let dir = scratch("commands");
        let path = dir.join("cash").join("history-folders");
        let ran = commands::Ran {
            time: 1,
            folder: "C:/src/a b".to_owned(),
            command: "for x in 1\tdo\n  echo \\x\ndone".to_owned(),
        };
        commands::append(&path, &ran).unwrap();
        commands::append(
            &path,
            &commands::Ran {
                time: 2,
                folder: "C:/x".to_owned(),
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
        assert_eq!(folders::load(&dir.join("none")).unwrap(), []);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn visits_add_up_and_age() {
        let mut record = Vec::new();
        folders::visit(&mut record, "C:/src", 100);
        folders::visit(&mut record, "c:/SRC/", 200);
        assert_eq!(record.len(), 1);
        assert!((record[0].rank - 2.0).abs() < f64::EPSILON);
        assert_eq!(record[0].last, 200);
        // Past the maximum, every rank is scaled and the smallest forgotten.
        record.push(folders::Folder {
            path: "C:/big".to_owned(),
            rank: folders::MAX_AGE,
            last: 0,
        });
        folders::visit(&mut record, "C:/new", 300);
        assert!(record.iter().all(|f| f.path != "C:/new"), "{record:?}");
        assert!(record.iter().any(|f| f.path == "C:/src"));
    }

    #[test]
    fn scores_weigh_the_last_visit() {
        let folder = folders::Folder {
            path: "C:/x".to_owned(),
            rank: 4.0,
            last: 1_000_000,
        };
        assert!((folders::score(&folder, 1_000_010) - 16.0).abs() < f64::EPSILON);
        assert!((folders::score(&folder, 1_000_000 + 7_200) - 8.0).abs() < f64::EPSILON);
        assert!((folders::score(&folder, 1_000_000 + 172_800) - 2.0).abs() < f64::EPSILON);
        assert!((folders::score(&folder, 1_000_000 + 1_000_000) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn words_match_in_turn_the_last_in_the_last_part() {
        assert!(folders::matches("C:/src/cash/crates", &["crates"]));
        assert!(folders::matches("C:/src/cash/crates", &["src", "crat"]));
        assert!(folders::matches("C:/Src/Cash", &["CASH"]));
        assert!(!folders::matches("C:/src/cash/crates", &["cash"]));
        assert!(!folders::matches("C:/src/cash/crates", &["crates", "src"]));
        assert!(folders::matches("C:/anything", &[]));
    }

    #[test]
    fn the_best_comes_first_and_here_is_left_out() {
        let record = vec![
            folders::Folder {
                path: "C:/a/cash".to_owned(),
                rank: 1.0,
                last: 0,
            },
            folders::Folder {
                path: "C:/b/cash".to_owned(),
                rank: 5.0,
                last: 0,
            },
            folders::Folder {
                path: "C:/c/cash".to_owned(),
                rank: 9.0,
                last: 0,
            },
        ];
        let found = folders::ranked(&record, &["cash"], 10, Some("c:/C/cash"));
        let paths: Vec<&str> = found.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["C:/b/cash", "C:/a/cash"]);
    }

    #[test]
    fn records_count_a_folder_once_until_another() {
        let dir = scratch("records");
        let mut records = Records::at(Some(dir.to_str().unwrap()));
        records.note_folder("C:/one");
        records.note_folder("C:/one");
        records.note_folder("C:/two");
        records.note_folder("C:/one");
        let record = folders::load(records.folders_file().unwrap()).unwrap();
        let one = record.iter().find(|f| f.path == "C:/one").unwrap();
        assert!((one.rank - 2.0).abs() < f64::EPSILON, "{record:?}");
        records.note_command("C:/one", "make");
        let ran = commands::read(records.history_folders_file().unwrap()).unwrap();
        assert_eq!(ran[0].folder, "C:/one");
        assert!(Records::at(None).folders_file().is_none());
        assert!(Records::at(Some("")).folders_file().is_none());
        let _ = fs::remove_dir_all(dir);
    }
}
