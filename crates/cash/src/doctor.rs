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
/// `sed` and `awk` are called out because they are the ones people are surprised by:
/// they are separate GNU projects, so Microsoft's Coreutils bundle does not include
/// them and no single install is a complete answer.
const EXPECTED: &[(&str, &str)] = &[
    (
        "sed",
        "not in Microsoft's Coreutils bundle — separate GNU project",
    ),
    (
        "awk",
        "not in Microsoft's Coreutils bundle — separate GNU project",
    ),
    ("grep", "in the MS bundle"),
    ("find", "findutils; in the MS bundle"),
    ("xargs", "findutils; in the MS bundle"),
    ("cat", "coreutils"),
    ("cut", "coreutils"),
    ("tr", "coreutils"),
    ("sort", "coreutils"),
    ("head", "coreutils"),
    ("tail", "coreutils"),
    ("wc", "coreutils"),
    ("mktemp", "coreutils"),
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

    check_session(&mut findings);
    check_commands(&mut findings, &entries, &pathext, &cwd);
    check_dos_shadowing(&mut findings, &entries, &pathext, &cwd);

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

fn check_commands(
    findings: &mut Vec<Finding>,
    entries: &[PathBuf],
    pathext: &[String],
    cwd: &Path,
) {
    for (command, note) in EXPECTED {
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
        let (level, detail, fix) = describe(command, &target);
        findings.push(Finding {
            level,
            subject: (*command).to_string(),
            detail,
            fix,
        });
    }
}

/// Describe where a command actually came from, and whether that is a problem.
fn describe(command: &str, target: &Path) -> (Level, String, Option<String>) {
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
            return (
                Level::Warn,
                format!("{shown} is a BusyBox applet"),
                Some(busybox_advice(command)),
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

/// Why a particular BusyBox applet is worth replacing.
///
/// Generic advice would be useless — "BusyBox cat is reduced" is true but does not
/// matter, whereas BusyBox `awk` genuinely breaks real scripts. Say which is which.
fn busybox_advice(command: &str) -> String {
    let why = match command {
        "awk" => {
            "BusyBox awk is a POSIX subset with none of gawk's extensions — no gensub, \
                  no length(array), limited regex"
        }
        "sed" => "BusyBox sed lacks GNU extensions that scripts commonly rely on",
        "grep" => "BusyBox grep lacks GNU options such as -P and some -o behaviour",
        "find" | "xargs" => "BusyBox findutils are reduced and miss common GNU options",
        _ => {
            "BusyBox provides a reduced implementation; the full version behaves more \
              predictably for scripts written on Linux"
        }
    };
    format!("{why}. Install with: {}", suggest_install(command))
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
    entries: &[PathBuf],
    pathext: &[String],
    cwd: &Path,
) {
    for command in DOS_SHADOWED {
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
    let shim = path.with_extension("shim");
    let contents = std::fs::read_to_string(shim).ok()?;

    for line in contents.lines() {
        if let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("path")
        {
            return Some(PathBuf::from(value.trim().trim_matches('"')));
        }
    }
    None
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
