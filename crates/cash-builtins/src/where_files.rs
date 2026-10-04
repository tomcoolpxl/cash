//! `where` — Windows' `where.exe`, with options spelled the way cash spells them (D66).
//!
//! `where.exe` finds files by pattern along the current folder and `PATH`, or in the
//! folders named by `DIR:PATTERN` or `$VAR:PATTERN`, or everywhere below a folder with
//! `/r`. This port keeps its search, its order, its messages and its exit statuses (0
//! found, 1 not found, 2 an error), measured against `where.exe` on Windows 11. What
//! changes is what cash changes everywhere:
//!
//! - options take dashes, `-r DIR` or `--recursive DIR`, `-q`/`--quiet`, `-f`/`--quote`,
//!   `-t`/`--times`, the letters in either case as `where.exe` takes them; help is `-?` or
//!   `--help`. A `/q` is not an option, so a path can never be taken for one;
//! - paths print the way `pwd` prints them, `C:/Windows/notepad.exe` (D3);
//! - `PATH` and `$VAR` values may be in either form, `/c/tools:/c/bin` or
//!   `C:\tools;C:\bin`, and a `DIR` may be spelled `/c/Windows`.
//!
//! Unlike `which`, it knows nothing of builtins, functions or aliases: it answers where the
//! files are, which is the question `where.exe` answers.

use std::io::Write;
use std::path::{Path, PathBuf};

use cash_core::{ExecutionExitCode, ExecutionResult, builtins};
use clap::Parser;

const HELP: &str = "\
Usage: where [-r DIR] [-q] [-f] [-t] PATTERN...

Displays the location of files that match the search pattern. By default, the
search is done in the current folder and in the folders on PATH.

Options:
  -r, --recursive DIR  Search DIR and every folder below it, recursively.
  -q, --quiet          Print nothing; report only through the exit status.
  -f, --quote          Print each matched file name in double quotes.
  -t, --times          Print the size and the last modified date and time of
                       each matched file.
  -?, --help           Print this help.

PATTERN is a file name, and may use the wildcards * and ?. A PATTERN without an
extension also matches the name with each extension in PATHEXT. Two other forms
search other folders:
  $VAR:PATTERN   the folders listed in the environment variable VAR
  DIR:PATTERN    DIR, or a list of folders separated by ;
Neither can be used with -r.

Exit status: 0 if a file was found, 1 if none was, and 2 for an error.

Examples:
  where git
  where myfilename1 myfile????.*
  where '$windir:*.*'
  where -r C:/Windows *.exe *.dll *.bat
  where -q ??.???
  where 'C:/Windows;C:/Windows/System32:*.dll'
  where -f -t *.dll
";

/// Find files by pattern, as Windows' `where.exe` does.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct WhereCommand {
    /// Options and patterns, parsed here: `-?` is not a name clap takes.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// What the command line asked for.
#[derive(Default)]
struct Request {
    help: bool,
    recursive: Option<String>,
    quiet: bool,
    quote: bool,
    times: bool,
    patterns: Vec<String>,
}

/// A command line `where` cannot run, with the lines that say why.
struct Usage(Vec<String>);

fn parse(args: &[String]) -> Result<Request, Usage> {
    let invalid = |option: &str| {
        Usage(vec![
            format!("ERROR: Invalid argument or option - '{option}'."),
            "Type \"where --help\" for usage help.".to_owned(),
        ])
    };
    let value_expected = |option: &str| {
        Usage(vec![
            format!("ERROR: Invalid syntax. Value expected for '{option}'."),
            "Type \"where --help\" for usage help.".to_owned(),
        ])
    };

    let mut request = Request::default();
    let mut args = args.iter();
    let mut options_done = false;
    while let Some(arg) = args.next() {
        if options_done || arg == "-" || !arg.starts_with('-') {
            request.patterns.push(arg.clone());
            continue;
        }
        if arg == "--" {
            options_done = true;
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            let (name, value) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (long, None),
            };
            match name {
                "help" if value.is_none() => request.help = true,
                "quiet" if value.is_none() => request.quiet = true,
                "quote" if value.is_none() => request.quote = true,
                "times" if value.is_none() => request.times = true,
                "recursive" => {
                    let dir = match value {
                        Some(value) => value.to_owned(),
                        None => args.next().ok_or_else(|| value_expected(arg))?.clone(),
                    };
                    request.recursive = Some(dir);
                }
                _ => return Err(invalid(arg)),
            }
            continue;
        }
        // Short options, which may be bundled (`-qf`); `r` takes the rest of the word
        // or the next one as its folder.
        let letters: Vec<char> = arg.chars().skip(1).collect();
        for (at, letter) in letters.iter().enumerate() {
            match letter.to_ascii_lowercase() {
                '?' => request.help = true,
                'q' => request.quiet = true,
                'f' => request.quote = true,
                't' => request.times = true,
                'r' => {
                    let rest: String = letters[at + 1..].iter().collect();
                    let dir = if rest.is_empty() {
                        args.next()
                            .ok_or_else(|| value_expected(&format!("-{letter}")))?
                            .clone()
                    } else {
                        rest
                    };
                    request.recursive = Some(dir);
                    break;
                }
                _ => return Err(invalid(&format!("-{letter}"))),
            }
        }
    }
    Ok(request)
}

/// Where one pattern is looked for.
enum Scope {
    /// The current folder, then `PATH`.
    Default,
    /// Folders named on the command line, or by an environment variable.
    Folders(Vec<String>),
    /// An environment variable that is not set.
    MissingVariable(String),
}

struct Search {
    /// The pattern as given, for messages.
    given: String,
    scope: Scope,
    /// The file-name pattern.
    name: String,
}

fn plan<SE: cash_core::ShellExtensions>(
    shell: &cash_core::Shell<SE>,
    pattern: &str,
    recursive: bool,
) -> Result<Search, Usage> {
    let error = |line: &str| Usage(vec![line.to_owned()]);
    if pattern.is_empty() {
        return Err(Usage(vec![
            "ERROR: Value for default option cannot be empty.".to_owned(),
            "Type \"where --help\" for usage help.".to_owned(),
        ]));
    }
    if let Some(rest) = pattern.strip_prefix('$')
        && let Some((variable, name)) = rest.split_once(':')
    {
        if recursive {
            return Err(error("ERROR: \"$env:pattern\" cannot be used with -r."));
        }
        let scope = match variable_value(shell, variable) {
            Some(value) => Scope::Folders(split_folders(&value)),
            None => Scope::MissingVariable(variable.to_owned()),
        };
        return Ok(Search {
            given: pattern.to_owned(),
            scope,
            name: name.to_owned(),
        });
    }
    if let Some((folders, name)) = pattern.rsplit_once(':') {
        if recursive {
            return Err(error(
                "ERROR: \"path:pattern\" format cannot be used with -r.",
            ));
        }
        if name.is_empty() {
            return Err(error("ERROR: Missing pattern in \"path:pattern\"."));
        }
        if name.contains(['/', '\\']) {
            return Err(error(
                "ERROR: Invalid pattern is specified in \"path:pattern\".",
            ));
        }
        return Ok(Search {
            given: pattern.to_owned(),
            scope: Scope::Folders(split_folders(folders)),
            name: name.to_owned(),
        });
    }
    Ok(Search {
        given: pattern.to_owned(),
        scope: Scope::Default,
        name: pattern.to_owned(),
    })
}

/// An environment variable's value, its name matched regardless of case as Windows does.
fn variable_value<SE: cash_core::ShellExtensions>(
    shell: &cash_core::Shell<SE>,
    name: &str,
) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    if let Some(value) = shell.env_str(name) {
        return Some(value.into_owned());
    }
    let actual = shell
        .env()
        .iter()
        .map(|(key, _)| key)
        .find(|key| key.eq_ignore_ascii_case(name))?
        .clone();
    shell.env_str(&actual).map(std::borrow::Cow::into_owned)
}

/// A folder list, in either `;` or `:` form.
fn split_folders(value: &str) -> Vec<String> {
    cash_win32::env::split_path(value)
        .map(str::to_owned)
        .collect()
}

/// The folders to search, as absolute paths, each once.
fn folders<SE: cash_core::ShellExtensions>(
    shell: &cash_core::Shell<SE>,
    scope: &Scope,
) -> Vec<PathBuf> {
    let listed: Vec<String> = match scope {
        Scope::Default => std::iter::once(".".to_owned())
            .chain(split_folders(&shell.env_str("PATH").unwrap_or_default()))
            .collect(),
        Scope::Folders(folders) => folders.clone(),
        Scope::MissingVariable(_) => Vec::new(),
    };
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for folder in listed {
        let path = absolute(shell, &folder);
        let key = cash_win32::fold::name_key(cash_win32::path::render(&path).trim_end_matches('/'));
        if !seen.contains(&key) {
            seen.push(key);
            out.push(path);
        }
    }
    out
}

/// `folder` as an absolute path, `..` resolved, against the shell's current folder.
fn absolute<SE: cash_core::ShellExtensions>(shell: &cash_core::Shell<SE>, folder: &str) -> PathBuf {
    let path = shell.absolute_path(Path::new(folder));
    std::path::absolute(&path).unwrap_or(path)
}

/// The names a pattern matches: itself, and without an extension, itself with each
/// `PATHEXT` extension.
fn name_patterns(name: &str, pathext: &[String]) -> Vec<String> {
    let mut patterns = vec![name.to_owned()];
    if !name.contains('.') {
        patterns.extend(pathext.iter().map(|ext| format!("{name}{ext}")));
    }
    patterns
}

/// Whether a file name matches a pattern as Windows matches it: `*` and `?`, any case,
/// a trailing `.` meaning no extension, and `NAME.*` matching `NAME` too.
fn matches(pattern: &str, name: &str) -> bool {
    let pattern = cash_win32::fold::name_key(pattern);
    let name = cash_win32::fold::name_key(name);
    let trimmed = pattern.trim_end_matches('.');
    let trimmed = if trimmed.is_empty() {
        pattern.as_str()
    } else {
        trimmed
    };
    if wildcard(trimmed, &name) {
        return true;
    }
    trimmed
        .strip_suffix(".*")
        .is_some_and(|stem| !name.contains('.') && wildcard(stem, &name))
}

fn wildcard(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let (mut p, mut n) = (0, 0);
    let mut backtrack: Option<(usize, usize)> = None;
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            backtrack = Some((p, n));
            p += 1;
        } else if let Some((star, from)) = backtrack {
            p = star + 1;
            n = from + 1;
            backtrack = Some((star, from + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

/// A folder's entries, files and folders apart, each in the order Windows lists them.
fn list(folder: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return (Vec::new(), Vec::new());
    };
    let mut files = Vec::new();
    let mut folders = Vec::new();
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            folders.push(path);
        } else if kind.is_symlink() && std::fs::metadata(&path).is_ok_and(|m| m.is_dir()) {
            // A link to a folder is not followed below, which could loop, nor a file.
        } else {
            // Files, links to them, and app execution aliases (`WindowsApps`), which
            // are reparse points Windows runs as programs.
            files.push(path);
        }
    }
    let order = |path: &PathBuf| {
        path.file_name()
            .map(|name| cash_win32::fold::name_key(&name.to_string_lossy()))
            .unwrap_or_default()
    };
    files.sort_by_key(order);
    folders.sort_by_key(order);
    (files, folders)
}

/// The files in `folder` that match any of `patterns`.
fn matching(folder: &Path, patterns: &[String]) -> Vec<PathBuf> {
    list(folder)
        .0
        .into_iter()
        .filter(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                patterns.iter().any(|pattern| matches(pattern, &name))
            })
        })
        .collect()
}

/// The files at and below `folder` that match, a folder's own files before those of the
/// folders in it.
fn matching_below(folder: &Path, patterns: &[String], found: &mut Vec<PathBuf>) {
    found.extend(matching(folder, patterns));
    for below in list(folder).1 {
        matching_below(&below, patterns, found);
    }
}

/// The dash spelling of a `where.exe` option written with a slash (`/q` is `-q`).
fn slash_option(pattern: &str) -> Option<String> {
    let letter = pattern.strip_prefix('/')?;
    ["r", "q", "f", "t", "?"]
        .iter()
        .find(|option| letter.eq_ignore_ascii_case(option))
        .map(|option| format!("-{option}"))
}

fn line(path: &Path, quote: bool, times: bool) -> String {
    let rendered = cash_win32::path::render(path);
    let name = if quote {
        format!("\"{rendered}\"")
    } else {
        rendered
    };
    if !times {
        return name;
    }
    let metadata = std::fs::metadata(path).or_else(|_| std::fs::symlink_metadata(path));
    let size = metadata.as_ref().map_or(0, std::fs::Metadata::len);
    let (date, time) = metadata
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(cash_win32::locale::short_date_and_time)
        .unwrap_or_default();
    format!("{size:>10}   {date}      {time}  {name}")
}

/// Every search the command line asks for, and the folder `-r` starts from; or why the
/// command line cannot run, found before anything is printed, as `where.exe` does.
fn prepare<SE: cash_core::ShellExtensions>(
    shell: &cash_core::Shell<SE>,
    request: &Request,
) -> Result<(Vec<Search>, Option<PathBuf>), Usage> {
    // Each pattern once, as `where.exe` takes a repeated one.
    let mut patterns: Vec<&String> = Vec::new();
    for pattern in &request.patterns {
        if !patterns
            .iter()
            .any(|seen| seen.eq_ignore_ascii_case(pattern))
        {
            patterns.push(pattern);
        }
    }
    let searches = patterns
        .into_iter()
        .map(|pattern| plan(shell, pattern, request.recursive.is_some()))
        .collect::<Result<Vec<_>, _>>()?;

    let root = match &request.recursive {
        Some(dir) => {
            let root = absolute(shell, dir);
            if !root.is_dir() {
                return Err(Usage(vec![
                    "ERROR: The system cannot find the file specified.".to_owned(),
                ]));
            }
            Some(root)
        }
        None => None,
    };
    Ok((searches, root))
}

impl builtins::Command for WhereCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let usage_error = |context: &cash_core::ExecutionContext<'_, SE>, lines: &[String]| {
            let mut stderr = context.stderr();
            for line in lines {
                writeln!(stderr, "{line}")?;
            }
            Ok::<_, cash_core::Error>(ExecutionResult::from(ExecutionExitCode::Custom(2)))
        };

        let request = match parse(&self.args) {
            Ok(request) => request,
            Err(Usage(lines)) => return usage_error(&context, &lines),
        };
        if request.help {
            write!(context.stdout(), "{HELP}")?;
            return Ok(ExecutionResult::success());
        }
        if request.patterns.is_empty() {
            return usage_error(
                &context,
                &[
                    "Usage: where [-r DIR] [-q] [-f] [-t] PATTERN...".to_owned(),
                    "Type \"where --help\" for usage help.".to_owned(),
                ],
            );
        }

        let shell = &*context.shell;
        let (searches, root) = match prepare(shell, &request) {
            Ok(prepared) => prepared,
            Err(Usage(lines)) => return usage_error(&context, &lines),
        };

        let pathext = shell.pathext();

        let mut any_found = false;
        let mut missing = Vec::new();
        for search in &searches {
            if let Scope::MissingVariable(name) = &search.scope
                && !request.quiet
            {
                writeln!(
                    context.stderr(),
                    "ERROR: Environment variable \"{name}\" is not found."
                )?;
            }
            let names = name_patterns(&search.name, &pathext);
            let mut found = Vec::new();
            match &root {
                Some(root) => matching_below(root, &names, &mut found),
                None => {
                    for folder in folders(shell, &search.scope) {
                        found.extend(matching(&folder, &names));
                    }
                }
            }
            if found.is_empty() {
                missing.push(&search.given);
                continue;
            }
            any_found = true;
            if request.quiet {
                continue;
            }
            let mut stdout = context.stdout();
            for path in &found {
                writeln!(stdout, "{}", line(path, request.quote, request.times))?;
            }
        }

        if !request.quiet {
            if !any_found {
                writeln!(
                    context.stderr(),
                    "INFO: Could not find files for the given pattern(s)."
                )?;
            } else {
                for pattern in &missing {
                    writeln!(context.stderr(), "INFO: Could not find \"{pattern}\".")?;
                }
            }
            // `where /q x`, from a cmd habit or a ported script: say what it was taken for.
            for pattern in &missing {
                if let Some(dash) = slash_option(pattern) {
                    writeln!(
                        context.stderr(),
                        "INFO: \"{pattern}\" was taken for a pattern; options take a dash here: {dash}"
                    )?;
                }
            }
        }

        Ok(if any_found {
            ExecutionResult::success()
        } else {
            ExecutionResult::general_error()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_as_windows_matches_them() {
        assert!(matches("notepad.exe", "Notepad.EXE"));
        assert!(matches("note*.exe", "notepad.exe"));
        assert!(matches("?foo.exe", "afoo.exe"));
        assert!(!matches("?foo.exe", "foo.exe"));
        assert!(matches("foo.*", "foo"));
        assert!(matches("foo.*", "Foo.txt"));
        assert!(!matches("foo.*", "afoo.exe"));
        assert!(matches("*.*", "LICENSE"));
        assert!(matches("foo.", "foo"));
        assert!(!matches("foo.", "foo.exe"));
        assert!(matches("*", "anything.at.all"));
    }

    #[test]
    fn a_pattern_without_an_extension_also_takes_pathext() {
        let pathext = vec![".COM".to_owned(), ".EXE".to_owned()];
        assert_eq!(
            name_patterns("git", &pathext),
            ["git", "git.COM", "git.EXE"]
        );
        assert_eq!(name_patterns("git.exe", &pathext), ["git.exe"]);
    }

    #[test]
    fn options_take_dashes_in_either_case_and_bundle() {
        let args = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let request = parse(&args(&["-QF", "--times", "-r", "C:/x", "git"]))
            .ok()
            .unwrap();
        assert!(request.quiet && request.quote && request.times);
        assert_eq!(request.recursive.as_deref(), Some("C:/x"));
        assert_eq!(request.patterns, ["git"]);

        assert!(parse(&args(&["-?"])).ok().unwrap().help);
        assert!(parse(&args(&["--help"])).ok().unwrap().help);
        assert_eq!(
            parse(&args(&["--recursive=C:/y", "x"]))
                .ok()
                .unwrap()
                .recursive
                .as_deref(),
            Some("C:/y")
        );
        // A slash is a pattern, not an option.
        assert_eq!(parse(&args(&["/q"])).ok().unwrap().patterns, ["/q"]);
        assert!(parse(&args(&["-z", "x"])).is_err());
        assert!(parse(&args(&["-r"])).is_err());
        assert_eq!(parse(&args(&["--", "-q"])).ok().unwrap().patterns, ["-q"]);
    }
}
