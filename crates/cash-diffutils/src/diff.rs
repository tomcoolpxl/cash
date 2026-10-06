// This file is part of cash's copy of the uutils diffutils package (CASH-PATCHES.md).
//
// For the full copyright and license information, please view the LICENSE-*
// files that was distributed with this source code.

//! `diff` itself: the operands, directories and their recursion, binary files, the
//! messages, and the exit status (0 same, 1 different, 2 trouble), as GNU diff's
//! `diff.c` and `dir.c` have them.

use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use crate::engine;
use crate::params::{Format, Params, Refusal, Request, parse_params};
use crate::utils::{error_words, format_time, modification_time};
use crate::{context_diff, ed_diff, normal_diff, side_diff, unified_diff};

/// The exit status for trouble: a file that cannot be read, a bad option.
const TROUBLE: i32 = 2;

pub const VERSION: &str = "diff (cash): GNU diffutils 3.12's options, from uutils diffutils";

pub const HELP: &str = "\
Usage: diff [OPTION]... FILES
Compare FILES line by line.

Mandatory arguments to long options are mandatory for short options too.
      --normal                  output a normal diff (the default)
  -q, --brief                   report only when files differ
  -s, --report-identical-files  report when two files are the same
  -c, -C NUM, --context[=NUM]   output NUM (default 3) lines of copied context
  -u, -U NUM, --unified[=NUM]   output NUM (default 3) lines of unified context
  -e, --ed                      output an ed script
  -y, --side-by-side            output in two columns
  -W, --width=NUM               output at most NUM (default 130) print columns
      --left-column             output only the left column of common lines
      --suppress-common-lines   do not output common lines

  -L, --label LABEL             use LABEL instead of file name and timestamp
                                  (can be repeated)

  -t, --expand-tabs             expand tabs to spaces in output
  -T, --initial-tab             make tabs line up by prepending a tab
      --tabsize=NUM             tab stops every NUM (default 8) print columns

  -r, --recursive                 recursively compare any subdirectories found
  -N, --new-file                  treat absent files as empty
  -x, --exclude=PAT               exclude files that match PAT

  -i, --ignore-case               ignore case differences in file contents
  -E, --ignore-tab-expansion      ignore changes due to tab expansion
  -Z, --ignore-trailing-space     ignore white space at line end
  -b, --ignore-space-change       ignore changes in the amount of white space
  -w, --ignore-all-space          ignore all white space
  -B, --ignore-blank-lines        ignore changes where lines are all blank
  -I, --ignore-matching-lines=RE  ignore changes where all lines match RE

  -a, --text                      treat all files as text
      --strip-trailing-cr         strip trailing carriage return on input

      --color[=WHEN]       color output; WHEN is 'never', 'always', or 'auto';
                             plain --color means --color='auto'

      --help               display this help and exit
  -v, --version            output version information and exit

FILES are 'FILE1 FILE2' or 'DIR1 DIR2' or 'DIR FILE' or 'FILE DIR'.
If a FILE is '-', read standard input.
Exit status is 0 if inputs are the same, 1 if different, 2 if trouble.

Inside cash, `help diff` says how diff treats Windows line endings and paths.
";

/// Runs diff with `args`, the tool's name first; returns its exit status.
pub fn main(args: &[OsString]) -> i32 {
    let params = match parse_params(args) {
        Ok(Request::Compare(params)) => params,
        Ok(Request::Help) => {
            print!("{HELP}");
            return 0;
        }
        Ok(Request::Version) => {
            println!("{VERSION}");
            return 0;
        }
        Err(Refusal::Usage(message)) => {
            eprintln!("diff: {message}");
            eprintln!("diff: Try 'diff --help' for more information.");
            return TROUBLE;
        }
        Err(Refusal::Plain(message)) => {
            eprintln!("diff: {message}");
            return TROUBLE;
        }
    };
    let mut diff = Diff {
        p: &params,
        out: Vec::new(),
        status: 0,
    };
    diff.compare_top();
    let mut stdout = std::io::stdout().lock();
    if let Err(error) = stdout.write_all(&diff.out).and_then(|()| stdout.flush()) {
        if error.kind() == std::io::ErrorKind::BrokenPipe {
            return diff.status;
        }
        eprintln!("diff: write error: {}", error_words(&error));
        return TROUBLE;
    }
    diff.status
}

/// What an operand is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    /// Not there: a `-N` empty file, or an error already reported.
    Absent,
    Directory,
    File,
    Stdin,
}

struct Diff<'a> {
    p: &'a Params,
    out: Vec<u8>,
    status: i32,
}

impl Diff<'_> {
    fn trouble(&mut self, message: &str) {
        eprintln!("diff: {message}");
        self.status = TROUBLE;
    }

    fn differ(&mut self) {
        self.status = self.status.max(1);
    }

    fn kind_of(&mut self, name: &str, absent_is_empty: bool) -> Kind {
        if name == "-" {
            return Kind::Stdin;
        }
        match std::fs::metadata(name) {
            Ok(meta) if meta.is_dir() => Kind::Directory,
            Ok(_) => Kind::File,
            Err(error) => {
                if !(absent_is_empty && error.kind() == std::io::ErrorKind::NotFound) {
                    self.trouble(&format!("{name}: {}", error_words(&error)));
                }
                Kind::Absent
            }
        }
    }

    /// The two operands: `diff DIR FILE` compares `DIR/FILE` with `FILE`, two
    /// directories are walked, two files compared.
    fn compare_top(&mut self) {
        let mut name0 = self.p.from.to_string_lossy().into_owned();
        let mut name1 = self.p.to.to_string_lossy().into_owned();
        if name0 == "-" && name1 == "-" {
            self.report_identical(&name0, &name1);
            return;
        }
        // With -N a missing operand is an empty file, unless both are missing.
        let mut kind0 = self.kind_of(&name0, self.p.new_file);
        let mut kind1 = self.kind_of(&name1, self.p.new_file);
        if self.status == TROUBLE {
            return;
        }
        if kind0 == Kind::Absent && kind1 == Kind::Absent {
            self.trouble(&format!("{name0}: No such file or directory"));
            self.trouble(&format!("{name1}: No such file or directory"));
            return;
        }
        if kind0 == Kind::Directory && kind1 != Kind::Directory {
            if kind1 == Kind::Stdin {
                self.trouble("cannot compare '-' to a directory");
                return;
            }
            name0 = join(&name0, &base_name(&name1));
            kind0 = self.kind_of(&name0, self.p.new_file);
        } else if kind1 == Kind::Directory && kind0 != Kind::Directory {
            if kind0 == Kind::Stdin {
                self.trouble("cannot compare '-' to a directory");
                return;
            }
            name1 = join(&name1, &base_name(&name0));
            kind1 = self.kind_of(&name1, self.p.new_file);
        }
        if self.status == TROUBLE {
            return;
        }
        self.compare_pair(&name0, kind0, &name1, kind1, false);
    }

    /// Two entries of the same name in two directories, or the two operands.
    fn compare_pair(&mut self, name0: &str, kind0: Kind, name1: &str, kind1: Kind, in_dir: bool) {
        match (kind0, kind1) {
            (Kind::Directory, Kind::Directory) => {
                if self.p.recursive || !in_dir {
                    self.compare_dirs(name0, name1);
                } else {
                    let line = format!("Common subdirectories: {name0} and {name1}\n");
                    self.out.extend_from_slice(line.as_bytes());
                }
            }
            (Kind::Directory | Kind::Absent, Kind::Directory | Kind::Absent)
                if self.p.new_file && (kind0 == Kind::Directory || kind1 == Kind::Directory) =>
            {
                // -N: a directory on one side alone is walked against an empty one.
                let dir0 = (kind0 == Kind::Directory).then_some(name0);
                let dir1 = (kind1 == Kind::Directory).then_some(name1);
                self.walk_dirs(name0, dir0, name1, dir1);
            }
            (Kind::Directory, _) | (_, Kind::Directory) => {
                let what = |kind: Kind| {
                    if kind == Kind::Directory {
                        "directory"
                    } else {
                        "regular file"
                    }
                };
                let line = format!(
                    "File {name0} is a {} while file {name1} is a {}\n",
                    what(kind0),
                    what(kind1)
                );
                self.out.extend_from_slice(line.as_bytes());
                self.differ();
            }
            _ => {
                let data0 = self.read(name0, kind0);
                let data1 = self.read(name1, kind1);
                if let (Some(data0), Some(data1)) = (data0, data1) {
                    self.compare_files(
                        name0,
                        kind0 != Kind::Absent,
                        &data0,
                        name1,
                        kind1 != Kind::Absent,
                        &data1,
                        in_dir,
                    );
                }
            }
        }
    }

    /// The bytes of `name`: a file's, standard input's, or none for an absent file
    /// (`-N`); `None` after an error, reported.
    fn read(&mut self, name: &str, kind: Kind) -> Option<Vec<u8>> {
        let result = match kind {
            Kind::Absent => Ok(Vec::new()),
            Kind::Stdin => {
                let mut data = Vec::new();
                std::io::stdin().lock().read_to_end(&mut data).map(|_| data)
            }
            Kind::File | Kind::Directory => std::fs::read(name),
        };
        match result {
            Ok(data) => Some(data),
            Err(error) => {
                self.trouble(&format!("{name}: {}", error_words(&error)));
                None
            }
        }
    }

    fn compare_dirs(&mut self, dir0: &str, dir1: &str) {
        self.walk_dirs(dir0, Some(dir0), dir1, Some(dir1));
    }

    /// The entries of `dir0` and `dir1` side by side, in name order; `None` is a
    /// directory that is not there (`-N`).
    fn walk_dirs(&mut self, name0: &str, dir0: Option<&str>, name1: &str, dir1: Option<&str>) {
        let entries0 = self.entries(dir0);
        let entries1 = self.entries(dir1);
        let (mut i, mut j) = (0, 0);
        while i < entries0.len() || j < entries1.len() {
            let order = match (entries0.get(i), entries1.get(j)) {
                (Some(a), Some(b)) => a.as_bytes().cmp(b.as_bytes()),
                (Some(_), None) => std::cmp::Ordering::Less,
                _ => std::cmp::Ordering::Greater,
            };
            match order {
                std::cmp::Ordering::Equal => {
                    let entry = &entries0[i];
                    let path0 = join(name0, entry);
                    let path1 = join(name1, entry);
                    let kind0 = self.kind_of(&path0, false);
                    let kind1 = self.kind_of(&path1, false);
                    if kind0 != Kind::Absent && kind1 != Kind::Absent {
                        self.compare_pair(&path0, kind0, &path1, kind1, true);
                    }
                    i += 1;
                    j += 1;
                }
                std::cmp::Ordering::Less => {
                    let entry = entries0[i].clone();
                    self.only_in(name0, &entry, name1, true);
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    let entry = entries1[j].clone();
                    self.only_in(name1, &entry, name0, false);
                    j += 1;
                }
            }
        }
    }

    /// An entry one directory has and the other lacks: `Only in DIR: NAME`, or with
    /// `-N` a comparison against an empty file.
    fn only_in(&mut self, dir: &str, entry: &str, other_dir: &str, first: bool) {
        let path = join(dir, entry);
        let other = join(other_dir, entry);
        let kind = if self.p.new_file {
            self.kind_of(&path, false)
        } else {
            Kind::File
        };
        if !self.p.new_file {
            let line = format!("Only in {dir}: {entry}\n");
            self.out.extend_from_slice(line.as_bytes());
            self.differ();
            return;
        }
        // With -N a directory on one side alone is an empty one on the other, walked
        // only with -r; without, it is reported as a common subdirectory, as GNU does.
        if kind == Kind::Directory && !self.p.recursive {
            let (left, right) = if first {
                (&path, &other)
            } else {
                (&other, &path)
            };
            let line = format!("Common subdirectories: {left} and {right}\n");
            self.out.extend_from_slice(line.as_bytes());
            return;
        }
        if kind == Kind::Absent {
            return;
        }
        if first {
            self.compare_pair(&path, kind, &other, Kind::Absent, true);
        } else {
            self.compare_pair(&other, Kind::Absent, &path, kind, true);
        }
    }

    /// The names in `dir`, sorted by their bytes, without the excluded ones.
    fn entries(&mut self, dir: Option<&str>) -> Vec<String> {
        let Some(dir) = dir else {
            return Vec::new();
        };
        let listing = match std::fs::read_dir(dir) {
            Ok(listing) => listing,
            Err(error) => {
                self.trouble(&format!("{dir}: {}", error_words(&error)));
                return Vec::new();
            }
        };
        let mut names: Vec<String> = listing
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| !self.p.excludes.iter().any(|pattern| pattern.matches(name)))
            .collect();
        names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        names
    }

    /// The name a message shows for each side: its label when one was given.
    fn shown_names(&self, name0: &str, name1: &str) -> (String, String) {
        (
            self.p
                .labels
                .first()
                .cloned()
                .unwrap_or_else(|| name0.to_owned()),
            self.p
                .labels
                .get(1)
                .cloned()
                .unwrap_or_else(|| name1.to_owned()),
        )
    }

    fn report_identical(&mut self, name0: &str, name1: &str) {
        if self.p.report_identical_files {
            let (n0, n1) = self.shown_names(name0, name1);
            let line = format!("Files {n0} and {n1} are identical\n");
            self.out.extend_from_slice(line.as_bytes());
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the two files, each with its name, presence and bytes"
    )]
    fn compare_files(
        &mut self,
        name0: &str,
        present0: bool,
        data0: &[u8],
        name1: &str,
        present1: bool,
        data1: &[u8],
        in_dir: bool,
    ) {
        let p = self.p;
        let (shown0, shown1) = self.shown_names(name0, name1);
        let same_file = present0
            && present1
            && name0 != "-"
            && name1 != "-"
            && same_file::is_same_file(name0, name1).unwrap_or(false);
        if same_file || (data0 == data1 && !p.has_ignore_options()) {
            self.report_identical(name0, name1);
            return;
        }
        if p.brief && !p.has_ignore_options() {
            let line = format!("Files {shown0} and {shown1} differ\n");
            self.out.extend_from_slice(line.as_bytes());
            self.differ();
            return;
        }
        let binary = !p.text && (data0.contains(&0) || data1.contains(&0));
        if binary {
            if data0 == data1 {
                self.report_identical(name0, name1);
            } else {
                let line = if p.brief {
                    format!("Files {shown0} and {shown1} differ\n")
                } else {
                    format!("Binary files {shown0} and {shown1} differ\n")
                };
                self.out.extend_from_slice(line.as_bytes());
                self.differ();
            }
            return;
        }
        let (owned0, owned1);
        let (data0, data1): (&[u8], &[u8]) = if p.strip_trailing_cr {
            owned0 = engine::strip_trailing_cr(data0);
            owned1 = engine::strip_trailing_cr(data1);
            (&owned0, &owned1)
        } else {
            (data0, data1)
        };
        let lines0 = engine::split_lines(data0);
        let lines1 = engine::split_lines(data1);
        let changes = engine::script(&lines0, &lines1, p);
        let any_shown = changes.iter().any(|c| !c.ignore);
        if !any_shown {
            self.report_identical(name0, name1);
            return;
        }
        self.differ();
        if p.brief {
            let line = format!("Files {shown0} and {shown1} differ\n");
            self.out.extend_from_slice(line.as_bytes());
            return;
        }
        if in_dir {
            let line = format!("diff{} {name0} {name1}\n", self.option_string());
            self.out.extend_from_slice(line.as_bytes());
        }
        // A header is the name and the file's time, or its label: an absent file (-N)
        // has the epoch, standard input the present.
        let header = |label: Option<&String>, name: &str, present: bool| {
            label.cloned().unwrap_or_else(|| {
                let time = if !present {
                    modification_time(None)
                } else if name == "-" {
                    format_time(std::time::SystemTime::now())
                } else {
                    modification_time(Some(Path::new(name)))
                };
                format!("{name}\t{time}")
            })
        };
        let header0 = header(p.labels.first(), name0, present0);
        let header1 = header(p.labels.get(1), name1, present1);
        match p.format {
            Format::Normal => normal_diff::print(&mut self.out, &lines0, &lines1, &changes, p),
            Format::Unified => {
                unified_diff::print(
                    &mut self.out,
                    &lines0,
                    &lines1,
                    &changes,
                    &header0,
                    &header1,
                    p,
                );
            }
            Format::Context => {
                context_diff::print(
                    &mut self.out,
                    &lines0,
                    &lines1,
                    &changes,
                    &header0,
                    &header1,
                    p,
                );
            }
            Format::Ed => ed_diff::print(&mut self.out, &lines1, &changes, p),
            Format::SideBySide => side_diff::print(&mut self.out, &lines0, &lines1, &changes, p),
        }
    }

    /// The options as given, each quoted for a shell where it needs it, with a space
    /// before each: what follows `diff` on the line before each pair of files.
    fn option_string(&self) -> String {
        self.p
            .option_words
            .iter()
            .fold(String::new(), |mut text, word| {
                text.push(' ');
                text.push_str(&shell_quote(&word.to_string_lossy()));
                text
            })
    }
}

/// `word` as a shell would need it typed: itself when it is plain, else in single
/// quotes.
fn shell_quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./:=+@%,".contains(&b));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

/// `dir/name`, or `dir` and `name` run together when `dir` already ends in a
/// separator, as GNU prints `Only in d1/: x` for `diff d1/ d2/`.
fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') || dir.ends_with('\\') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// The last component of `path`, as a `diff DIR FILE` looks for `FILE` in `DIR`.
fn base_name(path: &str) -> String {
    PathBuf::from(path).file_name().map_or_else(
        || path.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::field_reassign_with_default,
    clippy::assert_is_empty,
    clippy::panic_in_result_fn,
    clippy::format_collect,
    reason = "a test stops loudly, and sets the options it is about"
)]
mod tests {
    use super::*;

    #[test]
    fn joins_as_gnu_prints() {
        assert_eq!(join("d1", "x"), "d1/x");
        assert_eq!(join("d1/", "x"), "d1/x");
        assert_eq!(join("d1\\", "x"), "d1\\x");
    }

    #[test]
    fn quotes_option_words_a_shell_would_need_quoted() {
        assert_eq!(shell_quote("-r"), "-r");
        assert_eq!(shell_quote("*.o"), "'*.o'");
        assert_eq!(shell_quote("--exclude=*.o"), "'--exclude=*.o'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }
}
