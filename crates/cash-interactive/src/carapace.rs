//! Completions from carapace, when it is installed (spec D63).
//!
//! carapace-bin completes over 700 commands on Windows — git, gh, winget, scoop, docker,
//! kubectl, terraform, cargo, npm, dotnet — with a description for each candidate. Cash
//! does not carry it: it is a Go program of 90 MB. When `carapace.exe` sits beside
//! `cash.exe` or on `PATH`, a Tab on an argument of a command that has no completion of its
//! own (no `complete` spec and no `complete -D` default) asks `carapace <command> export …`,
//! and the menu shows its descriptions. It runs only on Tab. Measured where this was
//! written, a Tab costs 85–100 ms for carapace's own lists and 220–450 ms where it runs the
//! tool (git, docker), no more than the tools' own bash completion scripts.

use std::collections::HashSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a Tab waits for carapace before falling back to cash's own candidates.
const TIMEOUT: Duration = Duration::from_secs(3);

/// A candidate carapace offered.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    /// What is inserted.
    pub value: String,
    /// What the menu shows, when it differs from the value.
    pub display: Option<String>,
    /// carapace's one-line description, if it gave one.
    pub description: Option<String>,
    /// Whether a space follows the value once inserted.
    pub space_after: bool,
}

/// carapace, found for one value of `PATH`.
pub(crate) struct Found {
    exe: PathBuf,
    /// The commands it completes, folded to lower case.
    commands: HashSet<String>,
}

/// How long after carapace failed to answer it is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// Where carapace was found, remembered while `PATH` stays the same.
pub(crate) struct Cache {
    /// The `PATH` the answer in `found` is for.
    path_value: Option<String>,
    found: Option<Found>,
    /// The `PATH` for which carapace was found but did not answer, and when.
    failed: Option<(String, Instant)>,
    retry_after: Duration,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            path_value: None,
            found: None,
            failed: None,
            retry_after: RETRY_AFTER,
        }
    }
}

impl Cache {
    /// A cache that tries a failed carapace again after `retry_after`.
    #[cfg(test)]
    pub fn retrying_after(retry_after: Duration) -> Self {
        Self {
            retry_after,
            ..Self::default()
        }
    }

    /// carapace for the shell's current `PATH`, if it is installed.
    ///
    /// Found or absent, the answer is kept until `PATH` changes. A carapace that is there
    /// but does not answer is not written off: a program just installed or updated can take
    /// seconds to start while Windows scans it, longer than a Tab waits. It is asked again
    /// once [`RETRY_AFTER`] has passed, so a broken one costs a Tab at most that often.
    pub fn get(
        &mut self,
        shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
    ) -> Option<&Found> {
        let path_value = shell.env_str("PATH").unwrap_or_default().into_owned();
        if self.path_value.as_deref() == Some(path_value.as_str()) {
            return self.found.as_ref();
        }
        if let Some((failed_path, when)) = &self.failed
            && *failed_path == path_value
            && when.elapsed() < self.retry_after
        {
            return None;
        }

        let exe = match locate(shell) {
            Location::At(exe) => Some(exe),
            Location::Absent => None,
            // The listing is not ready, as on a first Tab with highlighting off: look once
            // by probing, which costs what one "command not found" does.
            Location::Unknown => shell.find_first_executable_in_path("carapace"),
        };
        match exe.map(program_behind).map(Found::load) {
            Some(None) => {
                self.found = None;
                self.failed = Some((path_value, Instant::now()));
                return None;
            }
            found => self.found = found.flatten(),
        }
        self.failed = None;
        self.path_value = Some(path_value);
        self.found.as_ref()
    }

    /// carapace as last found by [`Self::get`], without looking again.
    pub const fn found(&self) -> Option<&Found> {
        self.found.as_ref()
    }
}

/// The program to start for the carapace found at `exe`: the real `carapace.exe` when `exe` is
/// a Scoop shim whose target exists, which saves starting the shim, a second process, on every
/// Tab (about 70 ms measured). Anything else — installed by winget or by hand, or a shim
/// whose target is gone — is started as found.
fn program_behind(exe: PathBuf) -> PathBuf {
    #[cfg(windows)]
    if let Some(target) = cash_win32::scoop::shim_target(&exe).filter(|target| target.is_file()) {
        return target;
    }
    exe
}

/// Where carapace is.
enum Location {
    At(PathBuf),
    Absent,
    /// The `PATH` listing is not ready to say.
    Unknown,
}

/// carapace beside cash's own executable, else on the shell's `PATH`.
fn locate(shell: &cash_core::Shell<impl cash_core::ShellExtensions>) -> Location {
    if let Some(beside_cash) = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join(exe_name())))
        .filter(|path| path.is_file())
    {
        return Location::At(beside_cash);
    }
    // The PATH listing answers "is it there" without probing every directory; only a
    // carapace that is there is then looked up by path.
    match shell.executable_on_path_if_known("carapace") {
        Some(true) => shell
            .find_first_executable_in_path("carapace")
            .map_or(Location::Absent, Location::At),
        Some(false) => Location::Absent,
        None => Location::Unknown,
    }
}

const fn exe_name() -> &'static str {
    "carapace.exe"
}

impl Found {
    /// Asks carapace which commands it completes (`carapace --list`).
    fn load(exe: PathBuf) -> Option<Self> {
        let mut command = Command::new(&exe);
        command.arg("--list");
        let output = run(command)?;
        let listing: serde_json::Value = serde_json::from_slice(&output).ok()?;
        let commands = listing
            .as_object()?
            .keys()
            .map(|name| name.to_lowercase())
            .collect();
        Some(Self { exe, commands })
    }

    /// Whether carapace completes `command`, named as typed: a bare name or a path, with or
    /// without its extension.
    pub fn completes(&self, command: &str) -> Option<String> {
        let name = Path::new(command).file_stem()?.to_str()?.to_lowercase();
        self.commands.contains(&name).then_some(name)
    }

    /// carapace's candidates for `words` — the command's name as carapace knows it, its
    /// arguments, and last the word being completed (possibly empty) — run in `cwd` with
    /// `environment`. `None` when carapace failed or took too long.
    pub fn complete(
        &self,
        words: &[String],
        cwd: &Path,
        environment: Vec<(String, String)>,
    ) -> Option<Vec<Candidate>> {
        let mut command = Command::new(&self.exe);
        command
            .arg(words.first()?)
            .arg("export")
            .args(words)
            .current_dir(cwd)
            .env_clear()
            .envs(environment);
        parse_export(&run(command)?)
    }
}

/// Reads carapace's `export` JSON: `values`, each with `value`, `display` and
/// `description`, and `nospace`, the characters after which no space follows (`*`: none).
fn parse_export(output: &[u8]) -> Option<Vec<Candidate>> {
    let export: serde_json::Value = serde_json::from_slice(output).ok()?;
    let nospace = export
        .get("nospace")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let values = export.get("values")?.as_array()?;
    Some(
        values
            .iter()
            .filter_map(|entry| {
                let text = |key| {
                    entry
                        .get(key)
                        .and_then(serde_json::Value::as_str)
                        .filter(|s: &&str| !s.is_empty())
                };
                let value = text("value")?.to_owned();
                let space_after = nospace != "*"
                    && !value
                        .chars()
                        .last()
                        .is_some_and(|last| nospace.contains(last));
                Some(Candidate {
                    display: text("display")
                        .filter(|display| *display != value)
                        .map(str::to_owned),
                    description: text("description").map(str::to_owned),
                    space_after,
                    value,
                })
            })
            .collect(),
    )
}

/// Runs `command` and returns its standard output, or `None` if it failed to start, failed,
/// or ran past [`TIMEOUT`], in which case it is killed.
fn run(mut command: Command) -> Option<Vec<u8>> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Read on a thread of its own: a large answer would otherwise fill the pipe and stall
    // carapace before it exits.
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        let _ = stdout.read_to_end(&mut output);
        output
    });

    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return reader.join().ok(),
            Ok(Some(_)) => return None,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// The words of the command being completed at byte `pos` of `line`: the command's name,
/// its arguments, and last the word under the cursor (empty after a space), with the byte
/// where that last word starts. `None` when the cursor is on the command's name, inside
/// quotes, or after a redirection, all of which cash completes itself.
pub(crate) fn command_words(line: &str, pos: usize) -> Option<(Vec<String>, usize)> {
    let before = line.get(..pos)?;
    let tokens = cash_parser::tokenize_str(before).ok()?;

    // Token locations count characters; the insertion point is a byte offset.
    let byte_offsets: Vec<usize> = before
        .char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(before.len()))
        .collect();
    let byte = |index: usize| byte_offsets.get(index).copied().unwrap_or(before.len());

    let mut words: Vec<(String, usize)> = Vec::new();
    for token in tokens {
        match token {
            cash_parser::Token::Operator(op, _) => {
                if crate::highlighting::STARTS_A_COMMAND.contains(&op.as_str()) {
                    words.clear();
                } else {
                    return None;
                }
            }
            cash_parser::Token::Word(word, location) => {
                if words.is_empty()
                    && (is_assignment(&word)
                        || crate::highlighting::KEYWORDS_BEFORE_A_COMMAND.contains(&word.as_str()))
                {
                    continue;
                }
                words.push((word, byte(location.start.index)));
            }
        }
    }

    let (current, start) = if before.ends_with(char::is_whitespace) || words.is_empty() {
        (String::new(), pos)
    } else {
        let (word, start) = words.pop()?;
        if word.contains(['\'', '"', '\\', '$', '`']) {
            return None;
        }
        (word, start)
    };
    if words.is_empty() {
        return None;
    }

    let mut all: Vec<String> = words
        .into_iter()
        .map(|(word, _)| cash_parser::unquote_str(&word))
        .collect();
    all.push(current);
    Some((all, start))
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !name.starts_with(|c: char| c.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Option<(Vec<String>, usize)> {
        command_words(line, line.len())
    }

    #[test]
    fn the_words_of_the_command_under_the_cursor() {
        assert_eq!(words("git ch"), Some((vec!["git".into(), "ch".into()], 4)));
        assert_eq!(
            words("git checkout "),
            Some((vec!["git".into(), "checkout".into(), String::new()], 13))
        );
        assert_eq!(
            words("ls | FOO=1 git 'log' -"),
            Some((vec!["git".into(), "log".into(), "-".into()], 21))
        );
        assert_eq!(
            words("if git sta"),
            Some((vec!["git".into(), "sta".into()], 7))
        );
    }

    #[test]
    fn what_cash_completes_itself_is_left_to_it() {
        // The command's name.
        assert_eq!(words("gi"), None);
        assert_eq!(words("ls; gi"), None);
        // Inside quotes, after a redirection, and on an empty line.
        assert_eq!(words("git add 'my fi"), None);
        assert_eq!(words("git log > out"), None);
        assert_eq!(words(""), None);
    }

    #[test]
    fn export_json_becomes_candidates() {
        let json = br#"{"version":"v1","nospace":"/","values":[
            {"value":"checkout","display":"checkout","description":"Switch branches"},
            {"value":"src/","display":"src/","description":""},
            {"value":"--force","display":"-f, --force","description":"Force it"}
        ]}"#;
        assert_eq!(
            parse_export(json),
            Some(vec![
                Candidate {
                    value: "checkout".into(),
                    display: None,
                    description: Some("Switch branches".into()),
                    space_after: true,
                },
                Candidate {
                    value: "src/".into(),
                    display: None,
                    description: None,
                    space_after: false,
                },
                Candidate {
                    value: "--force".into(),
                    display: Some("-f, --force".into()),
                    description: Some("Force it".into()),
                    space_after: true,
                },
            ])
        );
        assert_eq!(parse_export(b"not json"), None);
    }

    #[test]
    fn a_command_is_known_by_its_name_however_it_is_typed() {
        let found = Found {
            exe: PathBuf::new(),
            commands: std::iter::once("git".to_owned()).collect(),
        };
        assert_eq!(found.completes("git").as_deref(), Some("git"));
        assert_eq!(found.completes("GIT.exe").as_deref(), Some("git"));
        assert_eq!(
            found.completes("C:/Git/cmd/git.exe").as_deref(),
            Some("git")
        );
        assert_eq!(found.completes("gitk"), None);
    }
}
