//! The offer a portable cash makes once, at its first interactive prompt.
//!
//! A portable cash is one unpacked from the release zip: neither Scoop's (under
//! `scoop\apps\`) nor the installer's (a version folder with a `current` junction beside
//! it), which each set the machine up as they are installed. The zip sets up nothing, so
//! the first interactive shell asks, once, whether to put cash on the user PATH, add the
//! Windows Terminal profile, and put the tools on PATH for other programs. Enter means no,
//! and nothing on the machine changes without a yes.
//!
//! The answers are kept in `%LOCALAPPDATA%\cash\portable-offer`, and the offer is never
//! made again while that file exists; `CASH_NO_OFFER` set in the environment, or by a
//! startup file, skips the offer and writes nothing. The offer is made only by an
//! interactive shell whose standard input and output are a console: never for `-c`, a
//! script, or a shell on a pipe.
//!
//! The questions are read with the console reader the `read` builtin uses, so Ctrl-C at a
//! question means no to the rest and the prompt follows. A yes runs the command the help
//! names for doing it by hand, `cash --add-to-path`, `cash --terminal-profile` or `cash
//! --link-tools --add-to-path`, as this `cash.exe` run again, so what it prints is the
//! command's own, and there is one implementation of each.

use std::io::{IsTerminal as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cash_win32::conin::{Line, Terminal};

/// The variable whose presence skips the offer.
pub const SKIP_VAR: &str = "CASH_NO_OFFER";

/// The file the answers are kept in, under `%LOCALAPPDATA%\cash`.
const MARKER: &str = "portable-offer";

const INTRO: &str = "This cash runs from the zip you unpacked. Three things it can set up for you \
                     (Enter = no):";

/// The questions, in the order asked: the key the answer is kept under, and the question.
const QUESTIONS: [(&str, &str); 3] = [
    (
        "path",
        "Put cash on your PATH, so `cash` works from any window? [y/N] ",
    ),
    (
        "profile",
        "Add a \"cash\" profile to Windows Terminal? [y/N] ",
    ),
    (
        "links",
        "Put its tools (ls.exe, sed.exe, awk.exe, ...) on your PATH for other programs? [y/N] ",
    ),
];

/// What decides whether the offer is made; every one of them has to hold.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Conditions {
    /// The shell is interactive (`shell.options().interactive`).
    pub interactive: bool,
    /// Standard input and standard output are a console, not a pipe or a file.
    pub console: bool,
    /// This `cash.exe` is neither Scoop's nor the installer's.
    pub portable: bool,
    /// The marker file exists: the offer was made before.
    pub asked_before: bool,
    /// `CASH_NO_OFFER` is set.
    pub skipped: bool,
}

/// Whether the offer is made under `conditions`.
pub(crate) const fn offers(conditions: Conditions) -> bool {
    conditions.interactive
        && conditions.console
        && conditions.portable
        && !conditions.asked_before
        && !conditions.skipped
}

/// The answers, one a question of [`QUESTIONS`]; a question not reached is no.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Answers {
    pub path: bool,
    pub profile: bool,
    pub links: bool,
}

impl Answers {
    fn set(&mut self, key: &str, yes: bool) {
        match key {
            "path" => self.path = yes,
            "profile" => self.profile = yes,
            "links" => self.links = yes,
            _ => {}
        }
    }
}

/// Whether a line typed at a question is a yes: `y` or `yes`, in any case, with nothing
/// else on the line. Enter, and anything else, is no.
pub(crate) fn is_yes(line: &str) -> bool {
    let answer = line.trim();
    answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes")
}

/// The marker's text: the day the offer was made, then one line a question, `yes` or
/// `no`.
pub(crate) fn marker_text(answers: Answers, date: &str) -> String {
    let word = |yes: bool| if yes { "yes" } else { "no" };
    format!(
        "asked={date}\npath={}\nprofile={}\nlinks={}\n",
        word(answers.path),
        word(answers.profile),
        word(answers.links)
    )
}

/// The calendar day of a moment, as seconds since the Unix epoch: `2026-10-06`.
pub(crate) fn civil_date(seconds_since_epoch: u64) -> String {
    // Howard Hinnant's `civil_from_days`, with the era arithmetic on signed numbers.
    let days = i64::try_from(seconds_since_epoch / 86_400).unwrap_or(0);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    civil_date(seconds)
}

/// Where the marker goes: `%LOCALAPPDATA%\cash\portable-offer`, or `None` when
/// `LOCALAPPDATA` is not set, in which case no offer is made, as no answer could be kept.
fn marker_path() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(local).join("cash").join(MARKER))
}

/// Writes the marker, making its folder.
fn write_marker(marker: &Path, answers: Answers) -> std::io::Result<()> {
    if let Some(folder) = marker.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(marker, marker_text(answers, &today()))
}

/// Makes the offer when every condition holds: once, before the first prompt, after
/// the startup files ran. `shell` says whether it is interactive and whether a startup
/// file set [`SKIP_VAR`].
pub(crate) fn run(shell: &cash_core::Shell<impl cash_core::ShellExtensions>) {
    let skipped = shell.env().get(SKIP_VAR).is_some() || std::env::var_os(SKIP_VAR).is_some();
    let Some(marker) = marker_path() else {
        return;
    };
    let Ok(exe) = std::env::current_exe().and_then(std::fs::canonicalize) else {
        return;
    };
    let cheap = Conditions {
        interactive: shell.options().interactive,
        // Decided below, once the rest holds: opening the console reader changes its
        // input mode, which is not done for a shell that makes no offer.
        console: true,
        portable: cash_win32::install_layout::is_portable(&exe),
        asked_before: marker.exists(),
        skipped,
    };
    if !offers(cheap) {
        return;
    }
    let stdin = std::io::stdin();
    let terminal = if std::io::stdout().is_terminal() {
        Terminal::open(&stdin, true, true)
    } else {
        None
    };
    let Some(mut terminal) = terminal else {
        return;
    };

    let answers = ask(&mut terminal, &exe);
    drop(terminal);
    if let Err(e) = write_marker(&marker, answers) {
        eprintln!(
            "cash: cannot write {}: {e}",
            cash_win32::path::to_backslash(&marker)
        );
    }
}

/// Asks the questions, doing what each yes asks for as it is given. A read that ends
/// otherwise than at Enter (Ctrl-C, Ctrl-D, a console gone) is no to it and to the rest.
fn ask(terminal: &mut Terminal, exe: &Path) -> Answers {
    let mut answers = Answers::default();
    say(INTRO);
    for (key, question) in QUESTIONS {
        let mut out = std::io::stdout().lock();
        let _ = write!(out, "  {question}");
        let _ = out.flush();
        drop(out);
        let yes = match terminal.line() {
            Ok(Line::Typed(line)) => is_yes(&line),
            Ok(Line::Interrupted | Line::EndOfInput) | Err(_) => break,
        };
        answers.set(key, yes);
        if yes {
            act(key, exe);
        }
    }
    answers
}

/// Does what a yes to the question `key` asked for.
fn act(key: &str, exe: &Path) {
    match key {
        "path" => run_own(exe, &["--add-to-path"]),
        "profile" => run_own(exe, &["--terminal-profile"]),
        "links" => run_own(exe, &["--link-tools", "--add-to-path"]),
        _ => {}
    }
}

/// Runs this `cash.exe` with `args`, its output going to the screen: the command the
/// help names for the same thing, so what it says is the same.
fn run_own(exe: &Path, args: &[&str]) {
    let status = Command::new(exe).args(args).stdin(Stdio::null()).status();
    if let Err(e) = status {
        eprintln!(
            "cash: cannot run {} {}: {e}",
            cash_win32::path::to_backslash(exe),
            args.join(" ")
        );
    }
}

fn say(line: &str) {
    // A screen that cannot be written to is no reason to stop.
    let _ = writeln!(std::io::stdout().lock(), "{line}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The answers a marker records: a line that is not `key=yes` or `key=no` is ignored,
    /// and a question missing from the file is no.
    fn parse_marker(text: &str) -> Answers {
        let mut answers = Answers::default();
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=') {
                answers.set(key.trim(), value.trim() == "yes");
            }
        }
        answers
    }

    const ALL: Conditions = Conditions {
        interactive: true,
        console: true,
        portable: true,
        asked_before: false,
        skipped: false,
    };

    #[test]
    fn the_offer_needs_every_condition() {
        assert!(offers(ALL));
        assert!(!offers(Conditions {
            interactive: false,
            ..ALL
        }));
        assert!(!offers(Conditions {
            console: false,
            ..ALL
        }));
        assert!(!offers(Conditions {
            portable: false,
            ..ALL
        }));
        assert!(!offers(Conditions {
            asked_before: true,
            ..ALL
        }));
        assert!(!offers(Conditions {
            skipped: true,
            ..ALL
        }));
    }

    #[test]
    fn y_and_yes_in_any_case_are_yes_and_everything_else_is_no() {
        assert!(is_yes("y\n"));
        assert!(is_yes("Y\n"));
        assert!(is_yes(" yes \n"));
        assert!(is_yes("YES"));
        assert!(!is_yes("\n"));
        assert!(!is_yes(""));
        assert!(!is_yes("n\n"));
        assert!(!is_yes("yep\n"));
        assert!(!is_yes("y n\n"));
    }

    #[test]
    fn the_marker_has_the_day_and_one_line_a_question() {
        let answers = Answers {
            path: true,
            profile: false,
            links: true,
        };
        let text = marker_text(answers, "2026-10-06");
        assert_eq!(text, "asked=2026-10-06\npath=yes\nprofile=no\nlinks=yes\n");
        assert_eq!(parse_marker(&text), answers);
        assert_eq!(
            marker_text(Answers::default(), "2026-10-06"),
            "asked=2026-10-06\npath=no\nprofile=no\nlinks=no\n"
        );
        // Lines out of order, blank or unknown are taken in their stride.
        assert_eq!(
            parse_marker("links = yes\n\nprofile=maybe\nasked=x\n"),
            Answers {
                path: false,
                profile: false,
                links: true,
            }
        );
        assert_eq!(parse_marker(""), Answers::default());
    }

    #[test]
    fn the_questions_are_three_in_the_order_path_profile_links() {
        let keys: Vec<&str> = QUESTIONS.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys, ["path", "profile", "links"]);
        for (_, question) in QUESTIONS {
            assert!(question.ends_with("[y/N] "), "{question}");
        }
        assert!(INTRO.contains("Three things"));
    }

    #[test]
    fn the_civil_date_counts_from_the_epoch() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(86_399), "1970-01-01");
        assert_eq!(civil_date(86_400), "1970-01-02");
        assert_eq!(civil_date(946_598_400), "1999-12-31");
        assert_eq!(civil_date(946_684_800), "2000-01-01");
        // The day after a leap day.
        assert_eq!(civil_date(951_868_800), "2000-03-01");
    }
}
