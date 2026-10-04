//! `cash --link-tools [--add-to-path] [DIR]`: hard links to `cash.exe`, one per tool, so
//! that programs outside cash can run the tools it carries (ROADMAP items 16 and 18,
//! spec D65); `cash --unlink-tools [DIR]` takes them away again.
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
//!   re-running after an upgrade updates them; any other file is left alone and listed.
//!   A link Windows will not delete is renamed aside, and a later run deletes it;
//! - every tool is linked; those whose names a Windows program also has in System32 are
//!   named;
//! - PATH changes only when asked: `--add-to-path` puts the folder at the front of the
//!   user `Path` ([`cash_win32::userpath`]), which Windows puts after the machine's, so
//!   System32's `find` and `sort` still win for `.bat` files while cash's tools win over
//!   the user's other Unix tools. `--unlink-tools` removes the links, that entry, and the
//!   folder when nothing else is in it.

use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use cash_win32::userpath::Added;

/// The manifest in a links folder: the names of the links cash made there, one a line.
pub const MANIFEST: &str = ".cash-links";

/// How a link renamed aside is named: `ls.exe.cash-old-1`. With no `.exe` at the end, no
/// command lookup finds it.
const SET_ASIDE: &str = ".cash-old-";

/// The error Windows gives for a hard link to another volume.
const ERROR_NOT_SAME_DEVICE: i32 = 17;

/// The error NTFS gives when a file already has its 1023 names.
const ERROR_TOO_MANY_LINKS: i32 = 1142;

const USAGE: &str = "usage: cash --link-tools [--add-to-path] [DIR]\n       \
                     cash --unlink-tools [DIR]\n\
                     DIR is `bin` next to cash.exe when not given.";

/// `cash --link-tools …` or `cash --unlink-tools …`: the process exit status, or `None`
/// when the command line is neither.
pub fn command(args: &[String]) -> Option<u8> {
    let linking = match args.get(1).map(String::as_str) {
        Some("--link-tools") => true,
        Some("--unlink-tools") => false,
        _ => return None,
    };
    let mut add_to_path = false;
    let mut dir = None;
    for arg in args.iter().skip(2) {
        match arg.as_str() {
            "--add-to-path" if linking => add_to_path = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Some(0);
            }
            other if !other.starts_with("--") && dir.is_none() => dir = Some(other),
            other => {
                eprintln!("cash: unexpected argument `{other}`\n{USAGE}");
                return Some(2);
            }
        }
    }
    Some(if linking {
        run(dir, add_to_path)
    } else {
        unlink(dir)
    })
}

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

fn write_manifest(dir: &Path, names: &BTreeSet<String>) -> Result<(), String> {
    let manifest = names.iter().fold(String::new(), |mut text, name| {
        text.push_str(name);
        text.push('\n');
        text
    });
    std::fs::write(dir.join(MANIFEST), manifest)
        .map_err(|e| format!("{}: {e}", render(&dir.join(MANIFEST))))
}

/// If this process is a link `cash --link-tools` made, the tool it stands for: the file's
/// name, when the manifest in its folder lists it. `cash.exe` itself, and a copy under
/// another name nobody linked, are the shell.
///
/// cash re-enters its own exe for bundled tools (`--invoke-bundled`), for `sh`, `bash` and
/// `cash`, and for its virtual paths; in a tool's process that exe is the link. Such a
/// child says so (`CASH_ARGV0`, or the dispatch flag), and `main` asks before this. It was
/// told by a variable holding the link's path, which every descendant inherited, so a
/// tool that started the same link again (`find.exe -exec find.exe`) got the shell
/// (BIN-09).
pub fn linked_tool() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let stem = exe.file_stem()?.to_str()?;
    if stem.eq_ignore_ascii_case("cash") {
        return None;
    }
    let manifest = read_manifest(exe.parent()?);
    manifest
        .into_iter()
        .find(|name| name.eq_ignore_ascii_case(stem))
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
    /// Links Windows would not delete, renamed aside for a later run to delete.
    set_aside: Vec<String>,
}

/// `cash --link-tools [--add-to-path] [DIR]`. Returns the process exit status.
fn run(dir: Option<&str>, add_to_path: bool) -> u8 {
    let (dir, outcome) = match link_all(dir) {
        Ok(done) => done,
        Err(message) => {
            eprintln!("cash --link-tools: {message}");
            return 1;
        }
    };
    if !add_to_path {
        report(&dir, &outcome, None);
        return 0;
    }
    match cash_win32::userpath::add_first(&dir) {
        Ok(added) => {
            report(&dir, &outcome, Some(added));
            0
        }
        Err(e) => {
            report(&dir, &outcome, None);
            eprintln!("cash --link-tools: cannot change the user PATH: {e}");
            1
        }
    }
}

/// `cash.exe`, as a real path.
fn own_exe() -> Result<PathBuf, String> {
    std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|e| format!("cannot find cash.exe: {e}"))
}

/// The links folder: DIR, or `bin` next to `cash.exe`.
fn links_folder(dir: Option<&str>, exe: &Path) -> Result<PathBuf, String> {
    Ok(match dir {
        Some(dir) => PathBuf::from(dir),
        None => exe.parent().ok_or("cash.exe has no folder")?.join("bin"),
    })
}

fn link_all(dir: Option<&str>) -> Result<(PathBuf, Outcome), String> {
    let exe = own_exe()?;
    let dir = links_folder(dir, &exe)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", render(&dir)))?;
    let dir = std::fs::canonicalize(&dir).map_err(|e| format!("{}: {e}", render(&dir)))?;
    sweep_set_aside(&dir);

    let owned = read_manifest(&dir);
    let tools = tool_names();
    let mut outcome = Outcome::default();
    let mut now_owned = BTreeSet::new();

    // A link that fails stops the run, but what was made before it is listed, with what
    // was cash's already: it was not, so a later run took them for someone else's and
    // --unlink-tools left them (BIN-12).
    if let Err(error) = link_each(&tools, &exe, &dir, &owned, &mut outcome, &mut now_owned) {
        let listed: BTreeSet<String> = now_owned.union(&owned).cloned().collect();
        let _ = write_manifest(&dir, &listed);
        return Err(error);
    }

    // Links cash made for tools it no longer carries.
    for name in owned.difference(&tools.iter().cloned().collect()) {
        let link = dir.join(format!("{name}.exe"));
        if !link.exists() {
            continue;
        }
        match remove_or_set_aside(&link) {
            Ok(aside) => {
                outcome.removed.push(name.clone());
                if aside {
                    outcome.set_aside.push(name.clone());
                }
            }
            // Still cash's, and still listed, for the next run to try again.
            Err(_) => {
                now_owned.insert(name.clone());
            }
        }
    }

    write_manifest(&dir, &now_owned)?;
    Ok((dir, outcome))
}

/// Links `tools` in `dir` to `exe`, adding each to `now_owned` as it is cash's, and to
/// `outcome` as what happened to it.
fn link_each(
    tools: &[String],
    exe: &Path,
    dir: &Path,
    owned: &BTreeSet<String>,
    outcome: &mut Outcome,
    now_owned: &mut BTreeSet<String>,
) -> Result<(), String> {
    for tool in tools {
        let link = dir.join(format!("{tool}.exe"));
        if link.exists() {
            if cash_win32::fs::same_file(&link, exe) {
                outcome.current.push(tool.clone());
                now_owned.insert(tool.clone());
                continue;
            }
            if !owned.contains(tool) {
                outcome.skipped.push(tool.clone());
                continue;
            }
            // A link cash made to an older cash.exe: replace it with one to this one.
            if remove_or_set_aside(&link).map_err(|e| format!("{}: {e}", render(&link)))? {
                outcome.set_aside.push(tool.clone());
            }
            make_link(exe, &link, dir)?;
            outcome.refreshed.push(tool.clone());
        } else {
            make_link(exe, &link, dir)?;
            outcome.linked.push(tool.clone());
        }
        now_owned.insert(tool.clone());
    }
    Ok(())
}

/// Delete a link or, when Windows refuses, rename it aside: `Ok(true)` then. Windows
/// deletes a name of a running program while the file has others, but not its last one;
/// that happens to links whose old `cash.exe` is gone while a program still runs one of
/// them, and renaming still works.
fn remove_or_set_aside(link: &Path) -> std::io::Result<bool> {
    let Err(error) = std::fs::remove_file(link) else {
        return Ok(false);
    };
    let name = link
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let free = (1..1000)
        .map(|n| link.with_file_name(format!("{name}{SET_ASIDE}{n}")))
        .find(|aside| !aside.exists());
    match free {
        Some(aside) if std::fs::rename(link, &aside).is_ok() => Ok(true),
        _ => Err(error),
    }
}

/// Delete the links earlier runs renamed aside, those no longer running.
fn sweep_set_aside(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().contains(SET_ASIDE) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn make_link(exe: &Path, link: &Path, dir: &Path) -> Result<(), String> {
    std::fs::hard_link(exe, link).map_err(|e| match e.raw_os_error() {
        Some(ERROR_NOT_SAME_DEVICE) => format!(
            "{} is on another drive than {}; a hard link cannot cross drives. \
             Name a folder on the same drive.",
            render(dir),
            render(exe)
        ),
        Some(ERROR_TOO_MANY_LINKS) => format!(
            "{} already has the 1023 names NTFS allows a file, each links folder giving it \
             one a tool; `cash --unlink-tools DIR` removes a folder's links",
            render(exe)
        ),
        _ => format!("{}: {e}", render(link)),
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

fn report(dir: &Path, outcome: &Outcome, added: Option<Added>) {
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
    if !outcome.set_aside.is_empty() {
        say!(
            "  still in use, renamed aside to be deleted next time: {}",
            outcome.set_aside.join(" ")
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
            "  note: System32 has its own {}. Windows puts the user PATH after the machine's, \
             so from there programs outside cash still get those.",
            windows.join(", ")
        );
    }

    match added {
        Some(Added::Added) => {
            say!(
                "  added to the front of your user PATH: programs started from now on can run these"
            );
            say!("  (terminals already open keep the PATH they started with)");
        }
        Some(Added::AlreadyThere) => {
            say!("  already on your user PATH, left where it is");
        }
        None if cash_win32::userpath::contains(dir) => {
            say!("  the folder is on your user PATH");
        }
        None if on_path(dir) => {
            say!("  the folder is on PATH: other programs can run these now");
        }
        None => {
            say!(
                "  to let other programs run these, put the folder at the front of your user PATH:"
            );
            say!("    cash --link-tools --add-to-path \"{shown}\"");
        }
    }
}

/// `cash --unlink-tools [DIR]`: the links cash made there, the folder's entry on the user
/// PATH, and the folder when nothing else is in it. Returns the process exit status.
fn unlink(dir: Option<&str>) -> u8 {
    let dir = match own_exe().and_then(|exe| links_folder(dir, &exe)) {
        Ok(dir) => dir,
        Err(message) => {
            eprintln!("cash --unlink-tools: {message}");
            return 1;
        }
    };
    // A folder already gone can still be on PATH.
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    sweep_set_aside(&dir);

    let mut removed = 0;
    let mut set_aside = Vec::new();
    let mut failed = BTreeSet::new();
    for name in read_manifest(&dir) {
        let link = dir.join(format!("{name}.exe"));
        if !link.exists() {
            continue;
        }
        match remove_or_set_aside(&link) {
            Ok(aside) => {
                removed += 1;
                if aside {
                    set_aside.push(name);
                }
            }
            Err(e) => {
                eprintln!("cash --unlink-tools: {}: {e}", render(&link));
                failed.insert(name);
            }
        }
    }
    // A link that could not be removed stays listed, for the next run.
    if failed.is_empty() {
        let _ = std::fs::remove_file(dir.join(MANIFEST));
    } else {
        let _ = write_manifest(&dir, &failed);
    }

    let off_path = cash_win32::userpath::remove(&dir);
    let folder_gone = std::fs::remove_dir(&dir).is_ok() || !dir.exists();

    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "cash --unlink-tools: {}", render(&dir));
    let _ = writeln!(out, "  {removed} links removed");
    if !set_aside.is_empty() {
        let _ = writeln!(
            out,
            "  still in use, renamed aside: {}; run this again once the programs using them \
             exit",
            set_aside.join(" ")
        );
    }
    match &off_path {
        Ok(true) => {
            let _ = writeln!(out, "  taken off your user PATH");
        }
        Ok(false) => {}
        Err(e) => {
            let _ = writeln!(out, "  cannot change the user PATH: {e}");
        }
    }
    let _ = writeln!(
        out,
        "  {}",
        if folder_gone {
            "the folder is removed"
        } else {
            "the folder is kept: other files are in it"
        }
    );

    u8::from(!failed.is_empty() || off_path.is_err())
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

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn only_its_own_options_are_taken() {
        assert_eq!(command(&args("cash -c ls")), None);
        assert_eq!(command(&args("cash --link-tools --help")), Some(0));
        assert_eq!(command(&args("cash --link-tools a b")), Some(2));
        assert_eq!(command(&args("cash --link-tools --bogus")), Some(2));
        assert_eq!(command(&args("cash --unlink-tools --add-to-path")), Some(2));
    }
}
