//! `cash --link-tools [DIR]`: hard links to `cash.exe`, one per tool, so that programs
//! outside cash can run the tools it carries (ROADMAP item 16, spec D65).
//!
//! `which ls` prints `C:/…/cash.exe/ls` (D58), which only cash can run: Python's
//! `subprocess` or a `.bat` file can start only a real file. A hard link is one: the same
//! file as `cash.exe` under another name, costing no space. Started as `ls.exe`, cash
//! finds its name in the folder's manifest and runs as `ls` ([`linked_tool`]).
//!
//! Decided with the user:
//!
//! - the folder is DIR, or `bin` next to `cash.exe`, made when missing and used as it is
//!   when it exists;
//! - a link cash made before (named in the manifest) is refreshed to this `cash.exe`, so
//!   re-running after an upgrade updates them; any other file is left alone and listed;
//! - every tool is linked; those whose names a Windows program also has in System32 are
//!   named, since a `.bat` calling `find` gets cash's if the folder comes first on PATH;
//! - PATH is never changed: the command prints the folder and how to add it.

use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The manifest in a links folder: the names of the links cash made there, one a line.
pub const MANIFEST: &str = ".cash-links";

/// The error Windows gives for a hard link to another volume.
const ERROR_NOT_SAME_DEVICE: i32 = 17;

/// The tools to link: every command cash answers for itself that is not one of Bash's own
/// builtins, the same set `which` gives a path (D58), whose name can be a file's.
pub fn tool_names() -> Vec<String> {
    let mut names: Vec<String> = crate::doctor::builtin_names()
        .into_iter()
        .filter(|name| !cash_builtins::is_bash_builtin(name) && is_plain_name(name))
        .collect();
    names.sort();
    names
}

/// A name that makes a plain file name: letters, digits, `-`, `_`, `+`, and not one of the
/// device names Windows reserves (`con`, `nul`, `com1`…).
fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+'))
        && !is_reserved_device(name)
}

fn is_reserved_device(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(lower.as_str(), "con" | "prn" | "aux" | "nul")
        || ((lower.starts_with("com") || lower.starts_with("lpt"))
            && lower.len() == 4
            && lower.as_bytes()[3].is_ascii_digit())
}

/// The names a manifest lists.
fn read_manifest(dir: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(dir.join(MANIFEST))
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The variable a linked tool's process sets to its own exe's path. cash re-enters its own
/// exe for bundled tools (`--invoke-bundled`), for `sh`, `bash` and `cash`, and for its
/// virtual paths; in a tool's process that exe is the link, so a child started that way
/// would take itself for the tool again, without end. A child that finds its own path
/// here is such a re-entry and runs as cash; another link started from inside (`xargs.exe`
/// running `sort.exe`) has a path of its own, and runs as its tool.
const SELF_VAR: &str = "CASH_LINKED_TOOL_EXE";

/// If this process is a link `cash --link-tools` made, the tool it stands for: the file's
/// name, when the manifest in its folder lists it. `cash.exe` itself, a copy under another
/// name nobody linked, and cash re-entering a link it runs as ([`SELF_VAR`]) are the shell.
pub fn linked_tool() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let stem = exe.file_stem()?.to_str()?;
    if stem.eq_ignore_ascii_case("cash") {
        return None;
    }
    let reentered = std::env::var_os(SELF_VAR).is_some_and(|own| {
        own.to_string_lossy()
            .eq_ignore_ascii_case(&exe.to_string_lossy())
    });
    if reentered {
        return None;
    }
    let manifest = read_manifest(exe.parent()?);
    let tool = manifest
        .into_iter()
        .find(|name| name.eq_ignore_ascii_case(stem))?;
    // SAFETY: called from `main` before cash starts any thread, so nothing reads the
    // environment concurrently.
    unsafe { std::env::set_var(SELF_VAR, &exe) };
    Some(tool)
}

/// The links a folder's manifest lists that are no longer this `cash.exe`: made by an
/// older one and not refreshed since, replaced by another file, or deleted. With the
/// number listed, for `cash doctor`; `None` when the folder has no manifest.
pub fn stale_links(dir: &Path) -> Option<(usize, Vec<String>)> {
    let owned = read_manifest(dir);
    if owned.is_empty() {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let stale = owned
        .iter()
        .filter(|name| !cash_win32::fs::same_file(&dir.join(format!("{name}.exe")), &exe))
        .cloned()
        .collect();
    Some((owned.len(), stale))
}

/// What one run did.
#[derive(Default)]
struct Outcome {
    linked: Vec<String>,
    refreshed: Vec<String>,
    current: Vec<String>,
    removed: Vec<String>,
    skipped: Vec<String>,
}

/// `cash --link-tools [DIR]`. Returns the process exit status.
pub fn run(dir: Option<&str>) -> u8 {
    match link_all(dir) {
        Ok((dir, outcome)) => {
            report(&dir, &outcome);
            0
        }
        Err(message) => {
            eprintln!("cash --link-tools: {message}");
            1
        }
    }
}

fn link_all(dir: Option<&str>) -> Result<(PathBuf, Outcome), String> {
    let exe = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|e| format!("cannot find cash.exe: {e}"))?;
    let dir = match dir {
        Some(dir) => PathBuf::from(dir),
        None => exe.parent().ok_or("cash.exe has no folder")?.join("bin"),
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", render(&dir)))?;
    let dir = std::fs::canonicalize(&dir).map_err(|e| format!("{}: {e}", render(&dir)))?;

    let owned = read_manifest(&dir);
    let tools = tool_names();
    let mut outcome = Outcome::default();
    let mut now_owned = BTreeSet::new();

    for tool in &tools {
        let link = dir.join(format!("{tool}.exe"));
        if link.exists() {
            if cash_win32::fs::same_file(&link, &exe) {
                outcome.current.push(tool.clone());
                now_owned.insert(tool.clone());
                continue;
            }
            if !owned.contains(tool) {
                outcome.skipped.push(tool.clone());
                continue;
            }
            // A link cash made to an older cash.exe: replace it with one to this one.
            std::fs::remove_file(&link).map_err(|e| format!("{}: {e}", render(&link)))?;
            make_link(&exe, &link, &dir)?;
            outcome.refreshed.push(tool.clone());
        } else {
            make_link(&exe, &link, &dir)?;
            outcome.linked.push(tool.clone());
        }
        now_owned.insert(tool.clone());
    }

    // Links cash made for tools it no longer carries.
    for name in owned.difference(&tools.iter().cloned().collect()) {
        let link = dir.join(format!("{name}.exe"));
        if link.exists() && std::fs::remove_file(&link).is_ok() {
            outcome.removed.push(name.clone());
        }
    }

    let manifest = now_owned.iter().fold(String::new(), |mut text, name| {
        text.push_str(name);
        text.push('\n');
        text
    });
    std::fs::write(dir.join(MANIFEST), manifest)
        .map_err(|e| format!("{}: {e}", render(&dir.join(MANIFEST))))?;

    Ok((dir, outcome))
}

fn make_link(exe: &Path, link: &Path, dir: &Path) -> Result<(), String> {
    std::fs::hard_link(exe, link).map_err(|e| {
        if e.raw_os_error() == Some(ERROR_NOT_SAME_DEVICE) {
            format!(
                "{} is on another drive than {}; a hard link cannot cross drives. \
                 Name a folder on the same drive.",
                render(dir),
                render(exe)
            )
        } else {
            format!("{}: {e}", render(link))
        }
    })
}

fn render(path: &Path) -> String {
    cash_win32::path::render(path)
}

/// The linked tools whose names a program in System32 also has.
fn windows_names(names: &[&String]) -> Vec<String> {
    let system32 = std::env::var_os("SystemRoot")
        .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
        .join("System32");
    names
        .iter()
        .filter(|name| system32.join(format!("{name}.exe")).exists())
        .map(|name| (*name).clone())
        .collect()
}

/// Whether `dir` is on the process's PATH.
fn on_path(dir: &Path) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path)
            .any(|entry| std::fs::canonicalize(&entry).is_ok_and(|entry| entry == dir))
    })
}

fn report(dir: &Path, outcome: &Outcome) {
    // A reader that stops early (`| head`) is not an error worth a panic.
    let mut out = std::io::stdout().lock();
    macro_rules! say {
        ($($arg:tt)*) => {
            let _ = writeln!(out, $($arg)*);
        };
    }
    let shown = render(dir);
    let total = outcome.linked.len() + outcome.refreshed.len() + outcome.current.len();
    say!("cash --link-tools: {shown}");
    if outcome.linked.is_empty() && outcome.refreshed.is_empty() {
        say!("  {total} tools, all already linked to this cash.exe");
    } else {
        say!(
            "  {total} tools: {} linked, {} refreshed from an older cash.exe, {} already current",
            outcome.linked.len(),
            outcome.refreshed.len(),
            outcome.current.len()
        );
    }
    if !outcome.removed.is_empty() {
        say!(
            "  removed, no longer carried: {}",
            outcome.removed.join(" ")
        );
    }
    if !outcome.skipped.is_empty() {
        say!(
            "  left alone, not cash's links: {}",
            outcome
                .skipped
                .iter()
                .map(|name| format!("{name}.exe"))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    let linked: Vec<&String> = outcome
        .linked
        .iter()
        .chain(&outcome.refreshed)
        .chain(&outcome.current)
        .collect();
    let windows = windows_names(&linked);
    if !windows.is_empty() {
        say!(
            "  note: Windows has its own {} in System32. With this folder before System32 on \
             PATH, a .bat file calling one gets cash's; put it after System32 to keep Windows'.",
            windows.join(", ")
        );
    }

    if on_path(dir) {
        say!("  the folder is on PATH: other programs can run these now");
    } else {
        let windows_path = dir.display().to_string();
        let windows_path = windows_path.strip_prefix(r"\\?\").unwrap_or(&windows_path);
        say!(
            "  to let other programs find them, add the folder to your user PATH, e.g. in PowerShell:"
        );
        say!(
            "    [Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';{windows_path}', 'User')"
        );
        say!("  (cash does not change PATH itself; programs started after that see it)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_names_and_punctuation_are_not_tool_file_names() {
        assert!(is_plain_name("ls"));
        assert!(is_plain_name("b2sum"));
        assert!(is_plain_name("dos2unix"));
        assert!(!is_plain_name("["));
        assert!(!is_plain_name("con"));
        assert!(!is_plain_name("COM1"));
        assert!(is_plain_name("comm"));
        assert!(!is_plain_name(""));
    }
}
