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

use std::path::{Path, PathBuf};

use cash_win32::env::{Environment, split_path};
use cash_win32::resolve::{DEFAULT_PATHEXT, parse_pathext, resolve};

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
    subject: String,
    detail: String,
    fix: Option<String>,
}

/// Commands a bash script is entitled to assume exist.
///
/// Only the ones cash does **not** carry are listed. A builtin needs no diagnosis: it is
/// present by construction, and an earlier version of this list asked about `cat`, `cut`,
/// `tr`, `sort`, `head`, `tail`, `wc` and `mktemp` — all builtins — then told the user to
/// `winget install` something they already had. Checking cash's own resolution rather
/// than `PATH` is what stops that class of advice.
const EXPECTED: &[(&str, &str)] = &[
    ("grep", "not bundled with cash; in the MS Coreutils bundle"),
    ("diff", "not bundled with cash; diffutils"),
];

/// Commands cash answers for itself, checked to confirm it still does.
///
/// A regression here means a builtin was dropped or shadowed, which is worth knowing —
/// but it is the opposite question from EXPECTED, and gets the opposite advice.
const CARRIED: &[&str] = &[
    "ps", "top", "find", "xargs", "less", "more", "which", "kill", "cat", "mktemp", "hostname",
    "chmod", "id", "groups", "awk", "sed", "stat", "tty", "nohup", "who", "users", "pinky",
    "logname", "hostid", "pathchk", "install", "dos2unix", "unix2dos", "fuser", "lsof", "ss",
    "ping",
];

/// Shells whose name must resolve to cash itself (D7).
const OWN_SHELLS: &[&str] = &["sh", "bash"];

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

    check_session(&mut findings);
    check_platform(&mut findings);
    check_carried(&mut findings, &builtins);
    check_shells(&mut findings, &entries, &pathext, &cwd);
    check_commands(&mut findings, &builtins, &entries, &pathext, &cwd);
    check_busybox(&mut findings, &builtins, &entries, &pathext, &cwd);
    check_dos_shadowing(&mut findings, &builtins, &entries, &pathext, &cwd);
    check_deliberate_shadows(&mut findings, &builtins, &entries, &pathext, &cwd);
    check_carapace(&mut findings, &entries, &pathext, &cwd);

    report(&findings)
}

fn check_session(findings: &mut Vec<Finding>) {
    #[cfg(windows)]
    {
        let nested = cash_win32::spawn::in_any_job();
        findings.push(Finding {
            level: Level::Ok,
            subject: "process containment".into(),
            detail: if nested {
                "running inside another job object (nested jobs work since Windows 8)".into()
            } else {
                "no enclosing job object".into()
            },
            fix: None,
        });
    }
}

fn check_platform(findings: &mut Vec<Finding>) {
    #[cfg(windows)]
    {
        findings.push(Finding {
            level: Level::Note,
            subject: "platform identity".into(),
            detail: "`uname -s` is `Windows_NT`, `$OSTYPE` is `windows` (§4 #25)".into(),
            fix: Some(
                "scripts matching only `MINGW*|MSYS*` should also match `Windows_NT*` for their Windows branch".into(),
            ),
        });
    }
}

/// Every name cash answers for itself, without building a shell.
fn builtin_names() -> std::collections::HashSet<String> {
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

/// Confirm the commands cash carries are still cash's.
fn check_carried(findings: &mut Vec<Finding>, builtins: &std::collections::HashSet<String>) {
    let missing: Vec<&str> = CARRIED
        .iter()
        .copied()
        .filter(|name| !builtins.contains(*name))
        .collect();

    if missing.is_empty() {
        findings.push(Finding {
            level: Level::Ok,
            subject: "bundled userland".into(),
            detail: format!("{} commands answered by cash itself", builtins.len()),
            fix: None,
        });
    } else {
        findings.push(Finding {
            level: Level::Warn,
            subject: "bundled userland".into(),
            detail: format!("expected builtins are missing: {}", missing.join(", ")),
            fix: Some("this is a build problem, not a machine problem".into()),
        });
    }
}

/// `sh` and `bash` must be cash (D7); on Windows the alternative is WSL.
fn check_shells(findings: &mut Vec<Finding>, entries: &[PathBuf], pathext: &[String], cwd: &Path) {
    for shell in OWN_SHELLS {
        // The rule is unconditional and lives in command resolution, so the only useful
        // thing to report is what it takes precedence *over* — which on a bare Windows
        // PATH is `C:\WINDOWS\system32\bash.exe`, the WSL launcher. A script that
        // reached that would silently continue under Linux.
        let shadowed = resolve(shell, entries, pathext, cwd).map(|d| d.target().to_path_buf());

        let detail = match &shadowed {
            Some(found) if is_system32(found) => format!(
                "resolves to cash, ahead of {} — the WSL launcher",
                cash_win32::path::render(found)
            ),
            Some(found) => format!(
                "resolves to cash, ahead of {}",
                cash_win32::path::render(found)
            ),
            None => "resolves to cash".to_string(),
        };

        findings.push(Finding {
            level: Level::Ok,
            subject: (*shell).to_string(),
            detail,
            fix: None,
        });
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
    let found = beside_cash
        .or_else(|| resolve("carapace", entries, pathext, cwd).map(|d| d.target().to_path_buf()));

    findings.push(match found {
        Some(path) => Finding {
            level: Level::Ok,
            subject: "carapace".into(),
            detail: format!(
                "{} — Tab completes git, winget, docker and 700 more, with descriptions",
                cash_win32::path::render(&path)
            ),
            fix: None,
        },
        None => Finding {
            level: Level::Note,
            subject: "carapace".into(),
            detail: "not installed — Tab completes files, commands and variables, and what \
                     completion scripts you source"
                .into(),
            fix: Some(
                "scoop install extras/carapace-bin, for 700 more commands with descriptions".into(),
            ),
        },
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
            findings.push(Finding {
                level: Level::Ok,
                subject: (*command).to_string(),
                detail: "shell builtin".into(),
                fix: None,
            });
            continue;
        }

        let Some(dispatch) = resolve(command, entries, pathext, cwd) else {
            findings.push(Finding {
                level: Level::Warn,
                subject: (*command).to_string(),
                detail: format!("not found — {note}"),
                fix: Some(suggest_install(command)),
            });
            continue;
        };

        let target = dispatch.target().to_path_buf();
        let (level, detail, fix) = describe(command, &target, entries, pathext, cwd);
        findings.push(Finding {
            level,
            subject: (*command).to_string(),
            detail,
            fix,
        });
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
    let shown = cash_win32::path::render(target);

    // A Store alias whose app is not installed opens the Microsoft Store instead of
    // running — the notorious `python` behaviour (D46). They are 0-byte reparse points.
    if is_store_alias(target) {
        return (
            Level::Warn,
            format!("{shown} is a Microsoft Store alias (0 bytes)"),
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
            format!("{shown} is the DOS tool, not the Unix one"),
            Some(format!(
                "`{command}` with Unix syntax will fail; put your Unix toolchain earlier \
                 on PATH than System32"
            )),
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
                format!("{shown} is a BusyBox applet{reason}"),
                Some(busybox_fix(command, target, entries, pathext, cwd)),
            );
        }
        return (
            Level::Note,
            format!("{shown} -> {}", cash_win32::path::render(&real)),
            None,
        );
    }

    (Level::Ok, shown, None)
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
    ("diff", "no -y, no --color", "scoop install diffutils"),
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
    (
        "nc",
        "no -z, so `nc -z host port` port checks fail",
        "scoop install nmap (for ncat)",
    ),
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
        let dir = full
            .parent()
            .map(cash_win32::path::render)
            .unwrap_or_default();
        let shim_dir = shim
            .parent()
            .map(cash_win32::path::render)
            .unwrap_or_default();
        return format!(
            "{} is the full tool, later on PATH: put {dir} before {shim_dir}",
            cash_win32::path::render(&full)
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
        findings.push(Finding {
            level: Level::Warn,
            subject: (*command).to_owned(),
            detail: format!(
                "{} is a BusyBox applet: {why}",
                cash_win32::path::render(&target)
            ),
            fix: Some(busybox_fix(command, &target, entries, pathext, cwd)),
        });
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
            findings.push(Finding {
                level: Level::Warn,
                subject: format!("{command} (shadowing)"),
                detail: format!(
                    "{} is the DOS tool, not the Unix one",
                    cash_win32::path::render(target)
                ),
                fix: Some(format!(
                    "`{command}` with Unix syntax will fail; put your Unix toolchain \
                     earlier on PATH than System32"
                )),
            });
        }
    }
}

/// System32 tools cash shadows on purpose, and how its own one differs.
const DELIBERATE_SHADOWS: &[(&str, &str)] = &[
    (
        "ping",
        "Linux flags: -c is the count, -n is numeric output (D57)",
    ),
    (
        "reset",
        "the terminal reset; System32's is the Remote Desktop `reset session` (D55)",
    ),
];

/// Names cash answers although System32 has a program of the same name with other
/// flags. Reported so that nobody is surprised, and so that a `ping -n 3` habit has
/// somewhere to learn where `ping.exe` went.
fn check_deliberate_shadows(
    findings: &mut Vec<Finding>,
    builtins: &std::collections::HashSet<String>,
    entries: &[PathBuf],
    pathext: &[String],
    cwd: &Path,
) {
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
        let shadowed = cash_win32::path::render(target);
        findings.push(Finding {
            level: Level::Note,
            subject: (*command).to_owned(),
            detail: format!("cash's own, ahead of {shadowed}: {how}"),
            fix: Some(format!(
                "for Windows' {command}, run {shadowed} by its path, or `enable -n {command}`"
            )),
        });
    }
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

fn suggest_install(command: &str) -> String {
    match command {
        "awk" => "scoop install gawk".into(),
        "sed" => "scoop install sed".into(),
        // Everything else in EXPECTED is coreutils, findutils or grep, all of which
        // Microsoft bundles together.
        _ => "winget install Microsoft.Coreutils".into(),
    }
}

fn report(findings: &[Finding]) -> u8 {
    let warnings = findings.iter().filter(|f| f.level == Level::Warn).count();

    println!("cash doctor");
    println!();

    for finding in findings {
        println!(
            "  {}  {:<18} {}",
            finding.level.marker(),
            finding.subject,
            finding.detail
        );
        if let Some(fix) = &finding.fix {
            println!("        {:<18} -> {fix}", "");
        }
    }

    println!();
    if warnings == 0 {
        println!("No problems found.");
        0
    } else {
        println!("{warnings} warning(s). cash works regardless — D35 keeps it userland-agnostic —");
        println!("but scripts may get quietly wrong answers from the tools above.");
        1
    }
}
