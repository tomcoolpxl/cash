//! `cash doctor` — **D35**.
//!
//! cash requires no particular userland and resolves whatever is on `PATH` (D8). The
//! trouble is that a broken userland on Windows does not announce itself. A real machine
//! surveyed while writing the spec had:
//!
//! - every Unix command shimmed to **BusyBox**, so `awk` was a POSIX subset with none of
//!   gawk's extensions
//! - GnuWin32 `coreutils` and GNU `grep` installed but **unreachable**, their shims
//!   silently overwritten by a later BusyBox install
//! - `find` and `sort` shadowed by DOS `C:\WINDOWS\system32\find.exe` and `sort.exe`,
//!   which came earlier on `PATH` — so `find . -name '*.tf'` quietly hit the DOS tool
//!
//! None of that produces an error message. It produces wrong answers.
//!
//! The report fits the window, up to 80 columns: a line for each check that passed, then
//! each warning and note with what to do about it. A path in what was found is written
//! as cash prints it, the home folder as `~`; a path in a fix is written out in full, to
//! be pasted.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use cash_win32::env::{Environment, split_path};
use cash_win32::resolve::{DEFAULT_PATHEXT, parse_pathext, resolve};
use unicode_width::UnicodeWidthStr;

/// The report is laid out for this many columns, and narrower when the window is.
const WIDTH: usize = 80;

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Ok,
    Note,
    Warn,
}

impl Level {
    const fn marker(self) -> &'static str {
        match self {
            Self::Ok => "ok  ",
            Self::Note => "note",
            Self::Warn => "WARN",
        }
    }
}

struct Finding {
    level: Level,
    /// What was found: one short line for an `ok`, a sentence for a note or a warning.
    text: String,
    /// Names under the text, each with what it means for that one.
    rows: Vec<(String, String)>,
    /// What to do about it.
    fix: Option<String>,
}

impl Finding {
    fn ok(text: impl Into<String>) -> Self {
        Self::new(Level::Ok, text, None)
    }

    fn note(text: impl Into<String>, fix: Option<String>) -> Self {
        Self::new(Level::Note, text, fix)
    }

    fn warn(text: impl Into<String>, fix: Option<String>) -> Self {
        Self::new(Level::Warn, text, fix)
    }

    fn new(level: Level, text: impl Into<String>, fix: Option<String>) -> Self {
        Self {
            level,
            text: text.into(),
            rows: Vec::new(),
            fix,
        }
    }
}

/// Commands a bash script is entitled to assume exist.
///
/// Only the ones cash does **not** carry are listed. A builtin needs no diagnosis: it is
/// present by construction, and an earlier version of this list asked about `cat`, `cut`,
/// `tr`, `sort`, `head`, `tail`, `wc` and `mktemp` — all builtins — then told the user to
/// `winget install` something they already had. Checking cash's own resolution rather
/// than `PATH` is what stops that class of advice.
const EXPECTED: &[(&str, &str)] = &[];

/// Commands cash answers for itself, checked to confirm it still does.
///
/// A regression here means a builtin was dropped or shadowed, which is worth knowing —
/// but it is the opposite question from EXPECTED, and gets the opposite advice.
const CARRIED: &[&str] = &[
    "ps", "top", "find", "xargs", "less", "more", "which", "kill", "cat", "mktemp", "hostname",
    "chmod", "id", "groups", "awk", "sed", "stat", "tty", "nohup", "who", "users", "pinky",
    "logname", "hostid", "pathchk", "install", "dos2unix", "unix2dos", "fuser", "lsof", "ss",
    "ping", "stty", "tput", "iconv", "column", "xxd", "hexdump", "uuidgen", "xdg-open", "pbcopy",
    "pbpaste", "flock", "watch", "free", "nice", "renice", "nc", "diff", "cmp", "grep", "suspend",
    "mkfifo", "stdbuf", "getconf", "locale", "gzip", "gunzip", "zcat",
];

/// Run the diagnostic. Returns a process exit code.
pub fn run() -> u8 {
    let env = Environment::from_process();
    let path_value = env.get("PATH").unwrap_or_default();
    let entries: Vec<PathBuf> = split_path(path_value).map(PathBuf::from).collect();
    let pathext = env.get("PATHEXT").map_or_else(
        || DEFAULT_PATHEXT.iter().map(|s| (*s).to_string()).collect(),
        parse_pathext,
    );
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let mut findings = Vec::new();

    let builtins = builtin_names();

    // In the order the report gives them: what passed first, then the notes.
    check_shells(&mut findings, &entries, &pathext, &cwd);
    check_own_path(&mut findings);
    check_userland(&mut findings, &builtins, &entries);
    check_commands(&mut findings, &builtins, &entries, &pathext, &cwd);
    check_busybox(&mut findings, &builtins, &entries, &pathext, &cwd);
    check_dos_shadowing(&mut findings, &builtins, &entries, &pathext, &cwd);
    check_deliberate_shadows(&mut findings, &builtins, &entries, &pathext, &cwd);
    check_platform(&mut findings);
    check_carapace(&mut findings, &entries, &pathext, &cwd);
    check_sudo(&mut findings, &entries, &pathext, &cwd);
    check_session(&mut findings);
    let header = install_header(&mut findings);

    print!("{}", render(&header, &findings, width()));
    u8::from(findings.iter().any(|finding| finding.level == Level::Warn))
}

/// The columns the report has: the window's, up to [`WIDTH`]; [`WIDTH`] when the output
/// is not a console.
fn width() -> usize {
    if !std::io::stdout().is_terminal() {
        return WIDTH;
    }
    cash_win32::termios::window_size()
        .map_or(WIDTH, |(_, columns)| usize::from(columns).clamp(40, WIDTH))
}

/// The report for `findings` under `header`, in lines that end before the last of
/// `width` columns: a line for each check that passed, then each warning and each note,
/// a blank line apart, and the verdict.
fn render(header: &str, findings: &[Finding], width: usize) -> String {
    let limit = width.saturating_sub(1);
    let mut lines = wrap("", "  ", &format!("cash doctor · {header}"), limit);
    lines.push(String::new());
    for finding in findings.iter().filter(|f| f.level == Level::Ok) {
        lines.extend(block(finding, limit));
    }
    let blank = |lines: &mut Vec<String>| {
        if lines.last().is_some_and(|line| !line.is_empty()) {
            lines.push(String::new());
        }
    };
    for level in [Level::Warn, Level::Note] {
        for finding in findings.iter().filter(|f| f.level == level) {
            blank(&mut lines);
            lines.extend(block(finding, limit));
        }
    }
    blank(&mut lines);
    let warnings = findings.iter().filter(|f| f.level == Level::Warn).count();
    if warnings == 0 {
        lines.push("No problems found.".to_owned());
    } else {
        let verdict = format!(
            "{warnings} warning{}: cash works regardless, but scripts may get quietly wrong \
             answers from the tools above.",
            if warnings == 1 { "" } else { "s" }
        );
        lines.extend(wrap("", "", &verdict, limit));
    }
    let mut out = lines.join("\n").replace(NO_BREAK, " ");
    out.push('\n');
    out
}

/// What stands for a space inside a path or a command until the report is printed, so
/// that it is one word to [`wrap`]: `C:/Program Files/Git` is never cut after `Program`,
/// nor `sudo -u USER` after `sudo`.
const NO_BREAK: char = '\u{a0}';

/// `text` with its spaces made [`NO_BREAK`]s.
fn unbroken(text: &str) -> String {
    text.replace(' ', &NO_BREAK.to_string())
}

/// `path` written out in full, for a fix to be pasted from.
fn pasted(path: &Path) -> String {
    unbroken(&cash_win32::path::render(path))
}

/// One finding in lines of at most `limit` columns: the marker and the text, the rows
/// under it with their names in a column, and the fix after an arrow.
fn block(finding: &Finding, limit: usize) -> Vec<String> {
    const TEXT: &str = "        ";
    const UNDER: &str = "          ";
    let marker = format!("  {}  ", finding.level.marker());
    let mut lines = wrap(&marker, TEXT, &finding.text, limit);
    let names = finding
        .rows
        .iter()
        .map(|(name, _)| name.width())
        .max()
        .unwrap_or(0);
    for (name, meaning) in &finding.rows {
        let first = format!("{UNDER}{name}{}  ", " ".repeat(names - name.width()));
        let rest = " ".repeat(UNDER.len() + names + 2);
        lines.extend(wrap(&first, &rest, meaning, limit));
    }
    if let Some(fix) = &finding.fix {
        lines.extend(wrap(&format!("{TEXT}→ "), UNDER, fix, limit));
    }
    lines
}

/// `text`'s words, split at spaces, in lines of at most `limit` columns, the first after
/// `first` and the others after `rest`. A word longer than a line has a line of its own.
fn wrap(first: &str, rest: &str, text: &str, limit: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = first.to_owned();
    let mut empty = true;
    for word in text.split(' ').filter(|word| !word.is_empty()) {
        if !empty && line.width() + 1 + word.width() > limit {
            lines.push(std::mem::replace(&mut line, rest.to_owned()));
            empty = true;
        }
        if !empty {
            line.push(' ');
        }
        line.push_str(word);
        empty = false;
    }
    lines.push(line);
    lines
}

/// `items` as prose: `a`, `a and b`, `a, b and c`.
fn join_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// `path` as cash prints it, with the home folder (`$HOME`, else `%USERPROFILE%`) as `~`,
/// and its spaces [`unbroken`], for a finding's text.
pub(crate) fn shown(path: &Path) -> String {
    unbroken(&shown_from(path, crate::init_rc::home().as_deref()))
}

/// `path` as cash prints it, `home` and what is under it written from `~`.
fn shown_from(path: &Path, home: Option<&Path>) -> String {
    let text = cash_win32::path::render(path);
    let Some(home) = home.map(cash_win32::path::render) else {
        return text;
    };
    let home = home.trim_end_matches('/');
    if home.is_empty() {
        return text;
    }
    let (Some(head), Some(tail)) = (text.get(..home.len()), text.get(home.len()..)) else {
        return text;
    };
    if head.eq_ignore_ascii_case(home) && (tail.is_empty() || tail.starts_with('/')) {
        format!("~{tail}")
    } else {
        text
    }
}

/// The report's first line: this cash's version and where it is. An installed cash is
/// in `current`, the junction beside its version folder; a window open since before an
/// upgrade runs the version it started with, which a note says, with the one `current`
/// names now.
fn install_header(findings: &mut Vec<Finding>) -> String {
    let version = format!("cash {}", env!("CARGO_PKG_VERSION"));
    let Ok(exe) = std::env::current_exe().and_then(std::fs::canonicalize) else {
        return version;
    };
    let folder = exe.parent().map(shown).unwrap_or_default();
    let Some(layout) = cash_win32::install_layout::layout(&exe, cash_win32::junction::is_junction)
    else {
        return format!("{version} in {folder}");
    };
    let current = shown(&layout.current);
    if cash_win32::junction::points_at(&layout.current, &layout.version_dir) {
        return format!("{version} in {current}");
    }
    let named = cash_win32::junction::target(&layout.current)
        .and_then(|target| Some(target.file_name()?.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let own = layout
        .version_dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    findings.push(if named.is_empty() {
        Finding::note(
            format!("this window's cash is {own}, and {current} names another"),
            Some("windows opened from now on start the one current names".into()),
        )
    } else {
        Finding::note(
            format!("this window's cash is {own}, and {current} is {named}"),
            Some(format!("windows opened from now on start {named}")),
        )
    });
    format!("{version} in {folder}")
}

fn check_session(findings: &mut Vec<Finding>) {
    // As the session found it before making cash's own job, which every process is in
    // once it exists.
    let nested =
        cash_win32::session::started_nested().unwrap_or_else(cash_win32::spawn::in_any_job);
    findings.push(Finding::ok(if nested {
        "runs inside another job object, which works since Windows 8"
    } else {
        "not inside another job object"
    }));
}

fn check_platform(findings: &mut Vec<Finding>) {
    findings.push(Finding::note(
        format!(
            "{} says Windows_NT, $OSTYPE says windows",
            unbroken("uname -s")
        ),
        Some("a script that tests only MINGW*|MSYS* needs Windows_NT* too".into()),
    ));
}

/// Every name cash answers for itself, without building a shell.
pub(crate) fn builtin_names() -> std::collections::HashSet<String> {
    let mut names: std::collections::HashSet<String> = cash_builtins::default_builtins::<
        cash_core::extensions::DefaultShellExtensions,
    >(cash_builtins::BuiltinSet::BashMode)
    .into_keys()
    .collect();

    // The bundled utilities (D48) register as builtins too, but only once their registry
    // is installed. Installing is idempotent, so doing it here makes the diagnostic
    // independent of whether `main` reached that step before dispatching.
    cash_shell::bundled::install_default_providers();
    if let Some(bundled) = cash_shell::bundled::registry() {
        names.extend(bundled.keys().cloned());
    }

    names
}

/// The commands cash answers itself, confirmed to still be its own, and the folders of
/// `cash --link-tools` links on PATH (D65) with whether each link is still this
/// `cash.exe`. An upgrade replaces `cash.exe` with a new file, and the links keep the old
/// one: they go on running the old cash until `cash --link-tools` refreshes them.
fn check_userland(
    findings: &mut Vec<Finding>,
    builtins: &std::collections::HashSet<String>,
    entries: &[PathBuf],
) {
    let missing: Vec<&str> = CARRIED
        .iter()
        .copied()
        .filter(|name| !builtins.contains(*name))
        .collect();
    if !missing.is_empty() {
        findings.push(Finding::warn(
            format!("built-in commands are missing: {}", missing.join(", ")),
            Some("this is a build problem, not a machine problem".into()),
        ));
    }

    let mut current = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entry in entries {
        let Ok(dir) = std::fs::canonicalize(entry) else {
            continue;
        };
        if !seen.insert(dir.clone()) {
            continue;
        }
        let Some((total, stale)) = crate::link_tools::stale_links(&dir) else {
            continue;
        };
        if stale.is_empty() {
            current.push(format!("{total} tool links in {}", shown(&dir)));
            continue;
        }
        let mut names = stale.iter().take(8).cloned().collect::<Vec<_>>().join(", ");
        if stale.len() > 8 {
            names.push_str(", ...");
        }
        findings.push(Finding::warn(
            format!(
                "{} of {total} tool links in {} are not this cash.exe (an older one, or \
                 replaced): {names}",
                stale.len(),
                shown(&dir)
            ),
            Some(unbroken(&format!(
                "cash --link-tools '{}'",
                cash_win32::path::render(&dir)
            ))),
        ));
    }

    let mut parts = Vec::new();
    if missing.is_empty() {
        parts.push(format!("{} commands built in", builtins.len()));
    }
    parts.extend(current);
    if !parts.is_empty() {
        findings.push(Finding::ok(parts.join("; ")));
    }
}

/// `sh`, `bash` and `cash` are this cash, whatever PATH holds (D7). The rule is
/// unconditional and lives in command resolution, so the line names what it takes
/// precedence *over* — which on a bare Windows PATH is `C:\WINDOWS\system32\bash.exe`,
/// the WSL launcher. A script that reached that would silently continue under Linux. A
/// `cash.exe` on PATH is not named: it is cash too.
fn check_shells(findings: &mut Vec<Finding>, entries: &[PathBuf], pathext: &[String], cwd: &Path) {
    let ahead: Vec<String> = ["sh", "bash"]
        .into_iter()
        .filter_map(|shell| {
            let found = resolve(shell, entries, pathext, cwd)?;
            let target = found.target();
            Some(if shell == "bash" && is_system32(target) {
                "WSL's bash.exe".to_owned()
            } else {
                shown(target)
            })
        })
        .collect();
    let mut text = "sh, bash and cash start this cash".to_owned();
    if !ahead.is_empty() {
        text.push_str(", ahead of ");
        text.push_str(&join_and(&ahead));
    }
    findings.push(Finding::ok(text));
}

/// Whether a new window finds `cash`: the shells' line reports in-shell resolution,
/// which is always cash, so a portable cash whose folder is on no PATH still passes
/// there. This line asks whether the folder that stands for this `cash.exe` (`current`
/// for an installed one, Scoop's shims for Scoop's, its own for a portable one) is on the
/// user or the machine PATH.
fn check_own_path(findings: &mut Vec<Finding>) {
    let Some(report) = crate::own_path::doctor_report() else {
        return;
    };
    findings.push(if report.ok {
        Finding::ok(report.detail)
    } else {
        Finding::note(report.detail, report.fix)
    });
}

/// What cash's `sudo` elevates through: gsudo where it is installed, else Windows' own
/// `sudo`, whose new-window mode keeps a command's output in another window.
fn check_sudo(findings: &mut Vec<Finding>, entries: &[PathBuf], pathext: &[String], cwd: &Path) {
    use cash_win32::sysinfo::WindowsSudo;

    let gsudo = resolve("gsudo", entries, pathext, cwd);
    if let Some(gsudo) = &gsudo {
        check_gsudo(findings, gsudo.target());
    } else {
        findings.push(match cash_win32::sysinfo::windows_sudo() {
            WindowsSudo::Inline => Finding::ok("sudo through Windows' sudo, in this terminal"),
            WindowsSudo::InputClosed => Finding::note(
                "sudo goes through Windows' sudo, in this terminal but with its input closed",
                Some(format!(
                    "{}, in an elevated shell, gives it its input",
                    unbroken("sudo config --enable normal")
                )),
            ),
            WindowsSudo::NewWindow => Finding::note(
                "sudo goes through Windows' sudo, which opens a new window: the output stays \
                 there",
                Some(format!(
                    "{}, or {} in an elevated shell",
                    unbroken("scoop install gsudo"),
                    unbroken("sudo config --enable normal")
                )),
            ),
            WindowsSudo::Off => Finding::note(
                "no gsudo and no Windows sudo: sudo asks UAC, which opens a new window and \
                 does not wait",
                Some(format!(
                    "{}, or turn on sudo in Settings > System > For developers",
                    unbroken("scoop install gsudo")
                )),
            ),
        });
    }
    check_sudo_accounts(findings);
}

/// gsudo, with its credentials cache: while a session is open, what it elevates runs
/// without asking. The cache belongs to the shell that opened it, so `cash doctor`, a
/// process of its own, counts the sessions rather than asking whether it may use one.
fn check_gsudo(findings: &mut Vec<Finding>, gsudo: &Path) {
    let sessions = cash_win32::gsudo::status(gsudo, "CacheSessionsCount")
        .and_then(|count| count.parse::<u32>().ok());
    match sessions {
        Some(0) => findings.push(Finding::ok(
            "sudo through gsudo, in this terminal; each sudo asks",
        )),
        Some(count) => {
            findings.push(Finding::ok("sudo through gsudo, in this terminal"));
            findings.push(Finding::note(
                format!(
                    "gsudo's credentials cache is open ({count} session{}): sudo runs without \
                     asking in the shell that opened it",
                    if count == 1 { "" } else { "s" }
                ),
                Some(format!("{} closes it", unbroken("sudo -k"))),
            ));
        }
        None => findings.push(Finding::ok("sudo through gsudo, in this terminal")),
    }
}

/// What `sudo` and `su` depend on beyond the tool: whether this account is an
/// administrator, and whether other accounts can start this cash.
fn check_sudo_accounts(findings: &mut Vec<Finding>) {
    // A standard account elevates as the administrator who approves.
    if cash_win32::account::is_administrator() == Some(false) {
        findings.push(Finding::note(
            "this account is not an administrator: UAC asks an administrator to approve, and \
             the command runs as that administrator, with their files and ~",
            None,
        ));
    }

    // `su USER` and `sudo -u USER` start cash as that account, which needs to read it.
    if let Ok(exe) = std::env::current_exe()
        && cash_win32::account::may_execute(&exe, None) == Some(false)
    {
        let how = if cash_win32::install_layout::installed_by_scoop(&exe) {
            unbroken("scoop install -g cash")
        } else {
            "an install under Program Files".to_owned()
        };
        findings.push(Finding::note(
            format!(
                "only you and administrators can read this cash.exe, so {} and {} cannot \
                 start it as another account",
                unbroken("su USER"),
                unbroken("sudo -u USER")
            ),
            Some(format!("{how} makes it readable to every account")),
        ));
    }
}

/// carapace gives Tab completion, with descriptions, for the commands that bring none of
/// their own (D63). Optional, so its absence is a note, never a warning.
fn check_carapace(
    findings: &mut Vec<Finding>,
    entries: &[PathBuf],
    pathext: &[String],
    cwd: &Path,
) {
    let beside_cash = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("carapace.exe")))
        .filter(|path| path.is_file());
    let found = beside_cash.is_some() || resolve("carapace", entries, pathext, cwd).is_some();

    findings.push(if found {
        Finding::ok("Tab completion from carapace: git, winget, docker and 700 more")
    } else {
        Finding::note(
            "no carapace: Tab completes files, commands and variables, and what completion \
             scripts you source",
            Some(format!(
                "{}, for 700 more commands with descriptions",
                unbroken("scoop install extras/carapace-bin")
            )),
        )
    });
}

fn check_commands(
    findings: &mut Vec<Finding>,
    builtins: &std::collections::HashSet<String>,
    entries: &[PathBuf],
    pathext: &[String],
    cwd: &Path,
) {
    for (command, note) in EXPECTED {
        // cash's own answer wins, and needs no advice about installing anything.
        if builtins.contains(*command) {
            findings.push(Finding::ok(format!("{command} is built in")));
            continue;
        }

        let Some(dispatch) = resolve(command, entries, pathext, cwd) else {
            findings.push(Finding::warn(
                format!("{command} not found: {note}"),
                Some(suggest_install(command)),
            ));
            continue;
        };

        let target = dispatch.target().to_path_buf();
        let (level, text, fix) = describe(command, &target, entries, pathext, cwd);
        findings.push(Finding::new(level, text, fix));
    }
}

/// Describe where a command actually came from, and whether that is a problem.
fn describe(
    command: &str,
    target: &Path,
    entries: &[PathBuf],
    pathext: &[String],
    cwd: &Path,
) -> (Level, String, Option<String>) {
    let found = shown(target);

    // A Store alias whose app is not installed opens the Microsoft Store instead of
    // running — the notorious `python` behaviour (D46). They are 0-byte reparse points.
    if is_store_alias(target) {
        return (
            Level::Warn,
            format!("{command} is {found}, a Microsoft Store alias (0 bytes)"),
            Some(
                "if the app is not installed this opens the Store instead of running; \
                 disable it under Settings > Apps > Advanced app settings > App execution aliases"
                    .into(),
            ),
        );
    }

    // DOS-lineage tools in System32 shadow their Unix namesakes whenever System32 comes
    // earlier on PATH, which it almost always does. Checked here so each command gets
    // exactly one verdict rather than a contradictory pair.
    if is_system32(target) && DOS_SHADOWED.contains(&command) {
        return (
            Level::Warn,
            format!("{command} is {found}, the DOS tool, not the Unix one"),
            Some(dos_fix(command)),
        );
    }

    // Scoop shims carry a sibling .shim naming their real target, which is how a
    // BusyBox applet can masquerade as `sed`.
    if let Some(real) = shim_target(target) {
        let real_name = real.file_name().map(|n| n.to_string_lossy().to_lowercase());
        if real_name.as_deref() == Some("busybox.exe") {
            let reason =
                busybox_breakage(command).map_or_else(String::new, |(why, _)| format!(": {why}"));
            return (
                Level::Warn,
                format!("{command} is {found}, a BusyBox applet{reason}"),
                Some(busybox_fix(command, target, entries, pathext, cwd)),
            );
        }
        return (
            Level::Note,
            format!("{command} is {found}, a shim for {}", shown(&real)),
            None,
        );
    }

    (Level::Ok, format!("{command} is {found}"), None)
}

/// Whether a file is a Microsoft Store App Execution Alias (D46).
fn is_store_alias(path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    // 0 bytes plus a reparse point is the AppExecLink signature. Checking the size alone
    // would be too eager; checking the reparse attribute alone would catch symlinks.
    metadata.len() == 0 && metadata.file_type().is_symlink()
        || (metadata.len() == 0
            && path
                .to_string_lossy()
                .to_lowercase()
                .contains("windowsapps"))
}

/// If a path is a Scoop shim, the executable it really points at.
fn shim_target(path: &Path) -> Option<PathBuf> {
    cash_win32::scoop::shim_target(path)
}

/// How to install `command`: the table of `help tools`, which the prompt's hint for a
/// missing command reads too, so the two cannot disagree; a name it lacks falls back to
/// what the bundled tools' sources are.
fn suggest_install(command: &str) -> String {
    cash_builtins::helpdocs::install_hint(command).unwrap_or_else(|| match command {
        "awk" => "scoop install gawk".into(),
        "sed" => "scoop install sed".into(),
        // Everything else in EXPECTED is coreutils, findutils or grep, all of which
        // Microsoft bundles together.
        _ => "winget install Microsoft.Coreutils".into(),
    })
}

/// BusyBox applets that break scripts written for the GNU tools, each with what breaks
/// and where the full tool comes from. Curated rather than every applet (research/
/// busybox-gap-analysis.md, Q10): BusyBox `cat` is reduced too, but nothing notices.
/// Each reason was checked against BusyBox 1.38 (the Scoop build) when this was written.
const BUSYBOX_BREAKS: &[(&str, &str, &str)] = &[
    (
        "grep",
        "no -P, no --include",
        "winget install Microsoft.Coreutils",
    ),
    (
        "egrep",
        "no -P, no --include (it is BusyBox grep)",
        "winget install Microsoft.Coreutils",
    ),
    (
        "fgrep",
        "no --include (it is BusyBox grep)",
        "winget install Microsoft.Coreutils",
    ),
    (
        "make",
        "$(shell ...) expands to nothing, so GNU makefiles misbuild",
        "scoop install make",
    ),
    (
        "tar",
        "no --transform",
        "use Windows' own tar.exe, or scoop install tar",
    ),
    (
        "xz",
        "decompresses only; `xz FILE` cannot compress",
        "scoop install xz",
    ),
    // BusyBox's `nc` has no -z, so `nc -z host port` port checks failed; cash carries
    // its own `nc` now, so the shim is never reached.
    ("wget", "-T (timeout) crashes it", "scoop install wget"),
];

/// What breaks in BusyBox's `command`, and how to get the full tool, if it is listed.
fn busybox_breakage(command: &str) -> Option<(&'static str, &'static str)> {
    BUSYBOX_BREAKS
        .iter()
        .find(|(name, _, _)| *name == command)
        .map(|(_, why, install)| (*why, *install))
}

/// Whether `target` is a Scoop shim for BusyBox.
fn is_busybox(target: &Path) -> bool {
    shim_target(target).is_some_and(|real| {
        real.file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("busybox.exe"))
    })
}

/// The first copy of `command` further down `PATH` that is not BusyBox, if any: often
/// Git for Windows has the GNU tool, only later on `PATH` than Scoop's shims.
fn later_full_copy(
    command: &str,
    shim: &Path,
    entries: &[PathBuf],
    pathext: &[String],
    cwd: &Path,
) -> Option<PathBuf> {
    entries.iter().find_map(|entry| {
        let found = resolve(command, std::slice::from_ref(entry), pathext, cwd)?;
        let target = found.target().to_path_buf();
        (target != shim && !is_busybox(&target)).then_some(target)
    })
}

/// How to get the full tool: move an existing copy ahead of the shim, or install one.
fn busybox_fix(
    command: &str,
    shim: &Path,
    entries: &[PathBuf],
    pathext: &[String],
    cwd: &Path,
) -> String {
    if let Some(full) = later_full_copy(command, shim, entries, pathext, cwd) {
        let dir = full.parent().map(pasted).unwrap_or_default();
        let shim_dir = shim.parent().map(pasted).unwrap_or_default();
        return format!(
            "{} is the full tool, later on PATH: put {dir} before {shim_dir}",
            pasted(&full)
        );
    }
    let install = busybox_breakage(command).map_or("install the GNU tool", |(_, how)| how);
    format!("install the full tool: {install}")
}

/// BusyBox applets on `PATH` for the listed tools that cash does not carry itself.
/// Names in `EXPECTED` were already judged, with the same reason, by `describe`.
fn check_busybox(
    findings: &mut Vec<Finding>,
    builtins: &std::collections::HashSet<String>,
    entries: &[PathBuf],
    pathext: &[String],
    cwd: &Path,
) {
    for (command, why, _) in BUSYBOX_BREAKS {
        if builtins.contains(*command) || EXPECTED.iter().any(|(name, _)| name == command) {
            continue;
        }
        let Some(dispatch) = resolve(command, entries, pathext, cwd) else {
            continue;
        };
        let target = dispatch.target().to_path_buf();
        if !is_busybox(&target) {
            continue;
        }
        findings.push(Finding::warn(
            format!("{command} is {}, a BusyBox applet: {why}", shown(&target)),
            Some(busybox_fix(command, &target, entries, pathext, cwd)),
        ));
    }
}

/// Whether a path lives in System32.
fn is_system32(path: &Path) -> bool {
    let text = path.to_string_lossy().to_lowercase().replace('\\', "/");
    text.contains("/windows/system32")
}

/// DOS-lineage tools in System32 that shadow their Unix namesakes when System32 comes
/// earlier on `PATH` — which it almost always does.
const DOS_SHADOWED: &[&str] = &["find", "sort", "more"];

/// What to do about a DOS tool that answers to a Unix name.
fn dos_fix(command: &str) -> String {
    format!("{command} with Unix syntax fails there; put your Unix tools ahead of System32 on PATH")
}

fn check_dos_shadowing(
    findings: &mut Vec<Finding>,
    builtins: &std::collections::HashSet<String>,
    entries: &[PathBuf],
    pathext: &[String],
    cwd: &Path,
) {
    for command in DOS_SHADOWED {
        // A builtin cannot be shadowed: PATH is never consulted for it. Warning that
        // System32's `more.com` shadows `more` was exactly wrong once cash carried its
        // own pager — the DOS tool is the one being shadowed.
        if builtins.contains(*command) {
            continue;
        }

        // Anything in EXPECTED already received a single merged verdict from
        // describe(). Reporting it again here produced two contradictory lines for
        // `find` and `sort` — one "ok", one "WARN" — which is worse than either alone.
        if EXPECTED.iter().any(|(name, _)| name == command) {
            continue;
        }

        let Some(dispatch) = resolve(command, entries, pathext, cwd) else {
            continue;
        };
        let target = dispatch.target();

        if is_system32(target) {
            findings.push(Finding::warn(
                format!(
                    "{command} is {}, the DOS tool, not the Unix one",
                    shown(target)
                ),
                Some(dos_fix(command)),
            ));
        }
    }
}

/// System32 tools cash shadows on purpose, and how its own one differs.
const DELIBERATE_SHADOWS: &[(&str, &str)] = &[
    ("ping", "Linux flags: -c is the count, -n is numeric output"),
    ("reset", "resets the terminal, not a Remote Desktop session"),
    ("where", "options with -, not /, and C:/ paths"),
];

/// Names cash answers although System32 has a program of the same name with other
/// flags, in one note. Reported so that nobody is surprised, and so that a `ping -n 3`
/// habit has somewhere to learn where `ping.exe` went.
fn check_deliberate_shadows(
    findings: &mut Vec<Finding>,
    builtins: &std::collections::HashSet<String>,
    entries: &[PathBuf],
    pathext: &[String],
    cwd: &Path,
) {
    let mut rows = Vec::new();
    let mut example = None;
    for (command, how) in DELIBERATE_SHADOWS {
        if !builtins.contains(*command) {
            continue;
        }
        let Some(dispatch) = resolve(command, entries, pathext, cwd) else {
            continue;
        };
        let target = dispatch.target();
        if !is_system32(target) {
            continue;
        }
        example.get_or_insert_with(|| (*command, pasted(target)));
        rows.push(((*command).to_owned(), (*how).to_owned()));
    }
    let Some((command, path)) = example else {
        return;
    };
    let names: Vec<String> = rows.iter().map(|(name, _)| name.clone()).collect();
    let (text, which) = if rows.len() == 1 {
        (
            format!("{command} is cash's own, ahead of System32's:"),
            command.to_owned(),
        )
    } else {
        (
            format!("{} are cash's own, ahead of System32's:", join_and(&names)),
            "one".to_owned(),
        )
    };
    findings.push(Finding {
        level: Level::Note,
        text,
        rows,
        fix: Some(format!(
            "run Windows' {which} by its path, {path}, or switch cash's off: {}",
            unbroken(&format!("enable -n {command}"))
        )),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note_with_rows() -> Finding {
        Finding {
            level: Level::Note,
            text: "ping, reset and where are cash's own, ahead of System32's:".into(),
            rows: vec![
                (
                    "ping".into(),
                    "Linux flags: -c is the count, -n is numeric output".into(),
                ),
                (
                    "reset".into(),
                    "resets the terminal, not a Remote Desktop session".into(),
                ),
            ],
            fix: Some(
                "run Windows' one by its path, C:/WINDOWS/system32/PING.EXE, or switch \
                 cash's off: enable -n ping"
                    .into(),
            ),
        }
    }

    #[test]
    fn what_passed_comes_first_then_each_note_with_its_rows_and_fix() {
        let findings = [
            Finding::ok("sh, bash and cash start this cash, ahead of WSL's bash.exe"),
            note_with_rows(),
            Finding::ok("~/scoop/shims is on your user PATH"),
        ];
        let report = render("cash 1.2.3 in ~/scoop/apps/cash/current", &findings, 80);
        assert_eq!(
            report,
            "cash doctor · cash 1.2.3 in ~/scoop/apps/cash/current\n\
             \n  ok    sh, bash and cash start this cash, ahead of WSL's bash.exe\
             \n  ok    ~/scoop/shims is on your user PATH\
             \n\
             \n  note  ping, reset and where are cash's own, ahead of System32's:\
             \n          ping   Linux flags: -c is the count, -n is numeric output\
             \n          reset  resets the terminal, not a Remote Desktop session\
             \n        → run Windows' one by its path, C:/WINDOWS/system32/PING.EXE, or switch\
             \n          cash's off: enable -n ping\
             \n\
             \nNo problems found.\n"
        );
    }

    #[test]
    fn warnings_come_before_notes_and_are_counted() {
        let findings = [
            Finding::note("a note", None),
            Finding::warn("first warning", Some("its fix".into())),
            Finding::warn("second warning", None),
        ];
        let report = render("cash 1.2.3", &findings, 80);
        assert_eq!(
            report,
            "cash doctor · cash 1.2.3\n\
             \n  WARN  first warning\
             \n        → its fix\
             \n\
             \n  WARN  second warning\
             \n\
             \n  note  a note\
             \n\
             \n2 warnings: cash works regardless, but scripts may get quietly wrong answers\
             \nfrom the tools above.\n"
        );
        let one = render("cash 1.2.3", &[Finding::warn("w", None)], 80);
        assert!(one.contains("\n1 warning: cash works"), "{one}");
    }

    #[test]
    fn every_line_ends_before_the_last_column_and_wraps_under_its_text() {
        let findings = [
            Finding::ok(
                "211 commands built in; 151 tool links in ~/scoop/persist/cash/bin and more \
                 words to wrap",
            ),
            note_with_rows(),
            Finding::note(
                "only you and administrators can read this cash.exe, so su USER and sudo -u \
                 USER cannot start it as another account",
                Some("scoop install -g cash makes it readable to every account".into()),
            ),
        ];
        for width in [40, 60, 80] {
            let report = render("cash 1.2.3 in ~/scoop/apps/cash/current", &findings, width);
            for line in report.lines() {
                assert!(line.width() < width, "{width}: {line:?}\n{report}");
            }
            // A wrapped ok line goes on under its text, a fix under the arrow's text,
            // and a row's meaning under the meanings.
            assert!(report.contains("\n        "), "{report}");
            assert!(report.contains("\n          "), "{report}");
        }
        let narrow = render("cash 1.2.3", &findings, 40);
        assert!(
            narrow.contains("\n          ping   Linux flags: -c is the\n                 count,"),
            "{narrow}"
        );
    }

    #[test]
    fn a_path_with_spaces_is_never_cut_at_them() {
        let path = Path::new(r"C:\Program Files\Git\usr\bin\bash.exe");
        let findings = [Finding::ok(format!(
            "sh, bash and cash start this cash, ahead of {}",
            shown_from(path, None).replace(' ', &NO_BREAK.to_string())
        ))];
        let report = render("cash 1.2.3", &findings, 60);
        assert!(
            report.contains("\n        C:/Program Files/Git/usr/bin/bash.exe\n"),
            "{report}"
        );
        assert!(!report.contains(NO_BREAK), "{report:?}");
        assert_eq!(pasted(path), "C:/Program\u{a0}Files/Git/usr/bin/bash.exe");
    }

    #[test]
    fn a_word_longer_than_the_line_has_one_of_its_own() {
        assert_eq!(
            wrap("  ", "    ", "a C:/a/very/long/path/name b", 12),
            ["  a", "    C:/a/very/long/path/name", "    b"]
        );
        assert_eq!(wrap("> ", "  ", "", 10), ["> "]);
    }

    #[test]
    fn lists_read_as_prose() {
        let list =
            |items: &[&str]| join_and(&items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        assert_eq!(list(&[]), "");
        assert_eq!(list(&["ping"]), "ping");
        assert_eq!(list(&["ping", "reset"]), "ping and reset");
        assert_eq!(list(&["ping", "reset", "where"]), "ping, reset and where");
    }

    #[test]
    fn a_path_under_home_starts_with_a_tilde() {
        let home = Path::new(r"C:\Users\me");
        assert_eq!(
            shown_from(Path::new(r"C:\Users\me\scoop\shims"), Some(home)),
            "~/scoop/shims"
        );
        assert_eq!(shown_from(Path::new(r"c:\users\ME"), Some(home)), "~");
        assert_eq!(
            shown_from(
                Path::new(r"\\?\C:\Users\me\x"),
                Some(Path::new("C:/Users/me/"))
            ),
            "~/x"
        );
        // Another folder that only starts with the same letters is not under home.
        assert_eq!(
            shown_from(Path::new(r"C:\Users\meg\x"), Some(home)),
            "C:/Users/meg/x"
        );
        assert_eq!(
            shown_from(Path::new(r"C:\WINDOWS\system32\PING.EXE"), Some(home)),
            "C:/WINDOWS/system32/PING.EXE"
        );
        assert_eq!(
            shown_from(Path::new(r"C:\Users\me\x"), None),
            "C:/Users/me/x"
        );
    }
}
