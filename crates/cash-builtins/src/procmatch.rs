//! What `pgrep`, `pkill`, `pidof` and `killall` share: which processes a name means,
//! which of them may be signalled, and how a signal reaches one.
//!
//! The rules were decided once for the family (research/busybox-gap-analysis.md, Q2):
//!
//! * names match **case-insensitively**, because Windows image names are, so `pidof
//!   NOTEPAD` and `pidof notepad` agree as the file system does;
//! * **`.exe` is optional** on both sides: `notepad` and `notepad.exe` name the same
//!   image, so the suffix is set aside before comparing;
//! * the kill family never signals the shell itself, the kernel's pseudo-processes, the
//!   images Windows cannot survive losing, or anything running as a service account;
//!   and a process the user may not open is skipped rather than reported as a failure.

use cash_core::traps::TrapSignal;
use cash_core::{error, sys};
use cash_win32::process::{Held, ProcessInfo};
use fancy_regex::Regex;

/// `name` without a trailing `.exe`, compared case-insensitively.
fn without_exe(name: &str) -> &str {
    strip_suffix_ignoring_case(name, ".exe")
        .filter(|stem| !stem.is_empty())
        .unwrap_or(name)
}

/// A regular expression with a trailing `.exe` (or `\.exe`) removed, keeping a final
/// `$`, so that it can be matched against an image name that has had its own removed.
fn pattern_without_exe(pattern: &str) -> String {
    let (body, anchor) = pattern
        .strip_suffix('$')
        .map_or((pattern, ""), |body| (body, "$"));
    for suffix in [r"\.exe", ".exe"] {
        if let Some(stem) = strip_suffix_ignoring_case(body, suffix) {
            return std::format!("{stem}{anchor}");
        }
    }
    pattern.to_owned()
}

/// `text` without `suffix`, compared case-insensitively.
fn strip_suffix_ignoring_case<'a>(text: &'a str, suffix: &str) -> Option<&'a str> {
    let kept = text.len().checked_sub(suffix.len())?;
    if !text.is_char_boundary(kept) {
        return None;
    }
    let (stem, tail) = text.split_at(kept);
    tail.eq_ignore_ascii_case(suffix).then_some(stem)
}

/// Which image names a command means.
pub(crate) enum NameMatcher {
    /// A regular expression, searched for in the name (`pgrep`, `pkill`, `killall -r`).
    Pattern(Regex),
    /// One whole name (`pidof`, `killall`).
    Exact(String),
}

impl NameMatcher {
    /// A case-insensitive regular expression; with `exact`, it must match the whole name.
    pub(crate) fn pattern(pattern: &str, exact: bool) -> Result<Self, Box<fancy_regex::Error>> {
        let pattern = pattern_without_exe(pattern);
        let expression = if exact {
            std::format!("(?i)^(?:{pattern})$")
        } else {
            std::format!("(?i){pattern}")
        };
        Regex::new(&expression).map(Self::Pattern).map_err(Box::new)
    }

    /// One name, compared whole and case-insensitively.
    pub(crate) fn exact(name: &str) -> Self {
        Self::Exact(cash_win32::fold::name_key(without_exe(name)))
    }

    /// Whether this names the image `name`.
    pub(crate) fn matches(&self, name: &str) -> bool {
        let stem = without_exe(name);
        match self {
            Self::Pattern(regex) => regex.is_match(stem).unwrap_or(false),
            Self::Exact(wanted) => cash_win32::fold::name_key(stem) == *wanted,
        }
    }
}

/// Images whose loss takes Windows down or logs the user out. Killing `csrss.exe` as an
/// administrator is a blue screen, so no pattern reaches these.
const CRITICAL: &[&str] = &[
    "system",
    "secure system",
    "registry",
    "memory compression",
    "smss",
    "csrss",
    "wininit",
    "winlogon",
    "services",
    "lsass",
    "lsaiso",
    "fontdrvhost",
    "dwm",
];

/// Accounts that only services run as.
const SERVICE_ACCOUNTS: &[&str] = &["SYSTEM", "LOCAL SERVICE", "NETWORK SERVICE"];

/// Why the kill family leaves a matching process alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Protected {
    /// The shell running the command: `kill $$` says so if that is meant.
    Shell,
    /// The idle process, `System`, a critical image, or a service account's process.
    System,
}

impl Protected {
    /// How `killall -v` names the reason.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::Shell => "the shell itself",
            Self::System => "a system process",
        }
    }
}

/// Why the kill family must not signal `process`, if it must not.
pub(crate) fn protected(process: &ProcessInfo) -> Option<Protected> {
    if process.pid == std::process::id() {
        return Some(Protected::Shell);
    }
    let stem = without_exe(&process.name).to_lowercase();
    if process.pid <= 4 || CRITICAL.contains(&stem.as_str()) {
        return Some(Protected::System);
    }
    let user = cash_win32::process::details(process.pid).user;
    if user.is_some_and(|user| {
        SERVICE_ACCOUNTS
            .iter()
            .any(|a| user.eq_ignore_ascii_case(a))
    }) {
        return Some(Protected::System);
    }
    None
}

/// A signal as the kill family takes it: `0` only asks whether the process exists.
#[derive(Clone, Copy)]
pub(crate) enum Signal {
    Probe,
    Send(TrapSignal),
}

impl Signal {
    /// The default, `TERM`, which asks before `kill`'s D21 escalation terminates.
    pub(crate) fn term() -> Result<Self, cash_core::Error> {
        Ok(Self::Send(TrapSignal::try_from("TERM")?))
    }

    /// Its number, as `killall -v` prints it.
    pub(crate) fn number(self) -> i32 {
        match self {
            Self::Probe => 0,
            Self::Send(signal) => i32::try_from(signal).unwrap_or(0),
        }
    }
}

/// A signal given as a name (`TERM`, `SIGTERM`, `term`) or a number (`15`, `0`).
pub(crate) fn parse_signal(text: &str) -> Option<Signal> {
    if let Ok(number) = text.parse::<i32>() {
        return if number == 0 {
            Some(Signal::Probe)
        } else {
            TrapSignal::try_from(number).ok().map(Signal::Send)
        };
    }
    let upper = text.to_ascii_uppercase();
    if upper.is_empty() || !upper.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    TrapSignal::try_from(upper.as_str()).ok().map(Signal::Send)
}

/// What became of one signal.
pub(crate) enum Delivery {
    Sent,
    /// The process refused to be opened: another user's, or elevated.
    Denied,
    /// It exited between being listed and being signalled.
    Gone,
    Failed(cash_core::Error),
}

/// Sends `signal` to the one process `pid`, through `kill`'s own path (D21, D22), so
/// `TERM` asks first and escalates, and `STOP`/`CONT` suspend and resume.
fn deliver(pid: u32, signal: Signal) -> Delivery {
    let Ok(target) = i32::try_from(pid) else {
        return Delivery::Gone;
    };
    let result = match signal {
        Signal::Probe => sys::signal::check_signalable(target),
        Signal::Send(signal) => sys::signal::kill_process(target, signal),
    };
    match result {
        Ok(()) => Delivery::Sent,
        Err(error) => classify(error),
    }
}

/// [`deliver`], for a pid that came from a process listing finished by `listed` (a
/// `now_filetime` count): the signal goes to the process the listing named, or to none.
///
/// The process is held open before it is signalled, so the pid cannot change hands in
/// between, and one that started after the listing is not the process that was listed:
/// Windows gave it the pid since, which takes under a second on a busy machine. The
/// held process is returned for a caller that waits for it to end (`killall -w`).
pub(crate) fn deliver_listed(pid: u32, listed: u64, signal: Signal) -> (Delivery, Option<Held>) {
    let held = Held::open(pid);
    if held
        .as_ref()
        .is_some_and(|process| !process.started_by(listed))
    {
        return (Delivery::Gone, None);
    }
    (deliver(pid, signal), held)
}

fn classify(error: error::Error) -> Delivery {
    match error.as_io_error().map(std::io::Error::kind) {
        Some(std::io::ErrorKind::NotFound) => Delivery::Gone,
        Some(std::io::ErrorKind::PermissionDenied) => Delivery::Denied,
        _ => Delivery::Failed(error),
    }
}

/// Lists signal names for `killall -l`, two lines as psmisc prints them.
pub(crate) fn signal_names() -> Vec<String> {
    TrapSignal::iterator()
        .filter_map(|signal| match signal {
            TrapSignal::Signal(_) => Some(
                signal
                    .as_str()
                    .strip_prefix("SIG")
                    .unwrap_or(signal.as_str())
                    .to_owned(),
            ),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_is_optional_on_both_sides() {
        let bare = NameMatcher::exact("notepad");
        let suffixed = NameMatcher::exact("Notepad.EXE");
        for name in ["notepad.exe", "NOTEPAD.EXE", "notepad"] {
            assert!(bare.matches(name), "{name}");
            assert!(suffixed.matches(name), "{name}");
        }
        assert!(!bare.matches("notepad++.exe"));
    }

    #[test]
    fn patterns_ignore_case_and_the_suffix() {
        let pattern = NameMatcher::pattern("^note", false).unwrap();
        assert!(pattern.matches("Notepad.exe"));
        let anchored = NameMatcher::pattern(r"pad\.exe$", false).unwrap();
        assert!(anchored.matches("notepad.exe"));
        let exact = NameMatcher::pattern("notepad", true).unwrap();
        assert!(exact.matches("NOTEPAD.exe"));
        assert!(!exact.matches("notepad2.exe"));
        // The suffix is not matched, so `exe` does not mean every process.
        assert!(
            !NameMatcher::pattern("exe", false)
                .unwrap()
                .matches("notepad.exe")
        );
    }

    #[test]
    fn signals_by_name_or_number() {
        assert!(matches!(parse_signal("0"), Some(Signal::Probe)));
        for text in ["TERM", "term", "SIGTERM", "15"] {
            assert_eq!(parse_signal(text).map(Signal::number), Some(15), "{text}");
        }
        assert!(parse_signal("ZZZ").is_none());
        assert!(parse_signal("").is_none());
    }
}
