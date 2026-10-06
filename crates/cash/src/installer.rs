//! `cash --install-finish`, `cash --install-remove` and `cash --update`: the steps of the
//! per-user installer that are cash's own, and the upgrade from the releases page.
//!
//! The setup on the releases page copies a release into
//! `%LOCALAPPDATA%\Programs\cash\<version>\` and runs that folder's `cash.exe
//! --install-finish ROOT`, which does for this channel what Scoop's manifest does for
//! Scoop's, from one implementation: points the `current` junction at the version folder,
//! puts `current` on the user PATH, writes the Windows Terminal profile, makes or
//! refreshes the tool links, writes a starter `~/.bashrc` once, and sweeps the version
//! folders no process runs. A running `cash.exe` is never touched: a window that is open
//! keeps its version, new tabs get the new one. The uninstaller runs `--install-remove`
//! before it deletes the files; the user's `~/.bashrc`, history and config stay. `cash
//! --update` fetches the latest release, checks its `.sha256`, unpacks it beside the
//! version in use and runs the same finish step; `--check` only reports. A cash installed
//! by Scoop refuses all three and names `scoop update cash`.
//!
//! The web is reached through Windows' own `curl.exe`, and the zip opened with its
//! `tar.exe`, both in System32 since Windows 10 1803. For the tests: `CASH_UPDATE_API`
//! names the release to ask about in place of GitHub's API, and `CASH_UPDATE_DOWNLOAD`
//! the folder its files come from; `curl` reads a `file://` address for either.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cash_win32::install_layout::{Layout, installed_by_scoop, layout};
use cash_win32::userpath::Added;

const FINISH_USAGE: &str = "usage: cash --install-finish ROOT [--links] [--scoop] [--quiet]\n\
                            ROOT is the install folder holding the version folders.";

const REMOVE_USAGE: &str = "usage: cash --install-remove ROOT [--quiet]";

const UPDATE_USAGE: &str = "usage: cash --update [--check]";

/// Where the latest release is asked about.
const RELEASES_API: &str = "https://api.github.com/repos/tomcoolpxl/cash/releases/latest";

/// Where the releases are, for a cash that cannot update itself.
const RELEASES_PAGE: &str = "https://github.com/tomcoolpxl/cash/releases";

/// The target in the release zip's name.
const TARGET: &str = "x86_64-pc-windows-msvc";

/// Where a swept version folder is renamed to first, so that a delete that stops halfway
/// leaves nothing that looks like a version; the next run finishes it.
const SWEPT: &str = ".cash-old";

/// The error of opening a running program for writing.
const ERROR_SHARING_VIOLATION: i32 = 32;

/// `cash --install-finish …`, `--install-remove …` or `--update …`: the process exit
/// status, or `None` when the command line is none of them.
pub fn command(args: &[String]) -> Option<u8> {
    let rest = args.get(2..).unwrap_or_default();
    match args.get(1).map(String::as_str) {
        Some("--install-finish") => Some(finish_command(rest)),
        Some("--install-remove") => Some(remove_command(rest)),
        Some("--update") => Some(update_command(rest)),
        _ => None,
    }
}

/// One line a step, each `cash: …`, unless `--quiet`.
struct Say {
    quiet: bool,
}

impl Say {
    fn line(&self, text: &str) {
        if self.quiet {
            return;
        }
        // A reader that stops early is not an error worth a panic.
        let _ = writeln!(std::io::stdout().lock(), "cash: {text}");
    }
}

/// A path as Windows shows it: `C:\Users\…`.
fn shown(path: &Path) -> String {
    cash_win32::path::to_backslash(path)
}

/// A folder's own name.
fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// This `cash.exe`, as a real path: the version folder's, not `current\cash.exe`.
fn own_exe() -> Result<PathBuf, String> {
    std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|e| format!("cannot find cash.exe: {e}"))
}

// ---- --install-finish ---------------------------------------------------------------

struct FinishOptions {
    root: PathBuf,
    links: bool,
    scoop: bool,
    quiet: bool,
}

fn finish_command(args: &[String]) -> u8 {
    let mut root = None;
    let mut options = FinishOptions {
        root: PathBuf::new(),
        links: false,
        scoop: false,
        quiet: false,
    };
    for arg in args {
        match arg.as_str() {
            "--links" => options.links = true,
            "--scoop" => options.scoop = true,
            "--quiet" => options.quiet = true,
            "-h" | "--help" => {
                println!("{FINISH_USAGE}");
                return 0;
            }
            other if !other.starts_with("--") && root.is_none() => root = Some(other),
            other => {
                eprintln!("cash: unexpected argument `{other}`\n{FINISH_USAGE}");
                return 2;
            }
        }
    }
    let Some(root) = root else {
        eprintln!("{FINISH_USAGE}");
        return 2;
    };
    options.root = PathBuf::from(root);
    let exe = match own_exe() {
        Ok(exe) => exe,
        Err(message) => {
            eprintln!("cash: {message}");
            return 1;
        }
    };
    if installed_by_scoop(&exe) {
        eprintln!("cash: this cash was installed by Scoop; use scoop update cash");
        return 2;
    }
    match finish(&exe, &options) {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("cash: {message}");
            1
        }
    }
}

/// The steps after the files are in place, in order; the first that fails ends the run.
fn finish(exe: &Path, options: &FinishOptions) -> Result<(), String> {
    let say = Say {
        quiet: options.quiet,
    };
    let version_dir = exe.parent().ok_or("cash.exe has no folder")?;
    let root = std::fs::canonicalize(&options.root)
        .map_err(|e| format!("{}: {e}", shown(&options.root)))?;
    if version_dir.parent() != Some(root.as_path()) {
        return Err(format!(
            "{} is not in a version folder under {}",
            shown(exe),
            shown(&root)
        ));
    }
    let current = root.join("current");

    // The junction, set in place: never missing, never half-moved.
    cash_win32::junction::point(&current, version_dir)
        .map_err(|e| format!("{}: {e}", shown(&current)))?;
    say.line(&format!("{} -> {}", shown(&current), name_of(version_dir)));

    // `cash` as a command everywhere.
    match cash_win32::userpath::add_first(&current)
        .map_err(|e| format!("cannot change the user PATH: {e}"))?
    {
        Added::Added => say.line(&format!("{} added to the user PATH", shown(&current))),
        Added::AlreadyThere => say.line(&format!("{} is on the user PATH", shown(&current))),
    }

    // The Terminal profile runs `current\cash.exe`, which follows the upgrades.
    let written = crate::terminal_profile::write_for(&current.join("cash.exe"))?;
    say.line(&format!(
        "Windows Terminal profile: {}",
        shown(&written.fragment)
    ));

    // The links are hard links to this exe, so an upgrade refreshes them whenever an
    // earlier install made them.
    let bin = root.join("bin");
    if options.links || crate::link_tools::has_manifest(&bin) {
        let (total, added) = crate::link_tools::link_for_installer(&bin)?;
        say.line(&format!(
            "{total} tool links in {}, {}",
            shown(&bin),
            match added {
                Added::Added => "first on the user PATH",
                Added::AlreadyThere => "on the user PATH",
            }
        ));
    }

    if let Some(path) = crate::init_rc::run_once()? {
        say.line(&format!("wrote {}, a starter ~/.bashrc", shown(&path)));
    }

    for folder in old_versions(&root, version_dir) {
        match sweep(&folder) {
            Ok(()) => say.line(&format!("removed {}", name_of(&folder))),
            Err(why) => say.line(&format!("kept {}: {why}", name_of(&folder))),
        }
    }

    if options.scoop {
        install_scoop(&say)?;
    }
    Ok(())
}

/// The version folders under `root` that are not `keep`, this run's: those with a
/// `cash.exe`, and those an earlier sweep renamed aside but could not delete. Never
/// `bin`, never a junction.
fn old_versions(root: &Path, keep: &Path) -> Vec<PathBuf> {
    let keep = std::fs::canonicalize(keep).unwrap_or_else(|_| keep.to_path_buf());
    let current = root.join("current");
    let mut folders: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
        })
        .map(|entry| entry.path())
        .filter(|path| {
            let name = name_of(path);
            !name.eq_ignore_ascii_case("bin")
                && std::fs::canonicalize(path).is_ok_and(|real| real != keep)
                && !cash_win32::junction::points_at(&current, path)
                && (path.join("cash.exe").is_file() || name.ends_with(SWEPT))
        })
        .collect();
    folders.sort();
    folders
}

/// Delete a version folder, renaming it aside first: Windows will not rename a folder
/// holding a running program, so one that is in use stays, with the reason.
fn sweep(folder: &Path) -> Result<(), String> {
    let exe = folder.join("cash.exe");
    if exe.exists() {
        in_use(&exe)?;
    }
    let aside = if name_of(folder).ends_with(SWEPT) {
        folder.to_path_buf()
    } else {
        let free = (1..1000)
            .map(|n| {
                let name = name_of(folder);
                let suffix = if n == 1 {
                    String::new()
                } else {
                    format!("-{n}")
                };
                folder.with_file_name(format!("{name}{SWEPT}{suffix}"))
            })
            .find(|aside| !aside.exists())
            .ok_or("no free name to rename it to")?;
        std::fs::rename(folder, &free).map_err(|e| format!("could not be renamed: {e}"))?;
        free
    };
    std::fs::remove_dir_all(&aside).map_err(|e| format!("could not be deleted: {e}"))
}

/// Whether a program is running: Windows maps a running exe so that nothing may open it
/// for writing, and a folder holding one can be renamed but not emptied. Opening it for
/// writing changes nothing; `Err` says why it could not be.
fn in_use(exe: &Path) -> Result<(), String> {
    match std::fs::OpenOptions::new().write(true).open(exe) {
        Ok(_) => Ok(()),
        Err(e) if e.raw_os_error() == Some(ERROR_SHARING_VIOLATION) => {
            Err("its cash.exe is running".to_owned())
        }
        Err(e) => Err(format!("its cash.exe is in use: {e}")),
    }
}

/// Where Scoop is, if it is installed: its shims folder under `%SCOOP%` or
/// `%USERPROFILE%\scoop`, or a `scoop` command on PATH.
fn scoop_installed() -> Option<PathBuf> {
    let roots = [
        std::env::var_os("SCOOP").map(PathBuf::from),
        std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("scoop")),
    ];
    for root in roots.into_iter().flatten() {
        let shim = root.join("shims").join("scoop.ps1");
        if shim.is_file() {
            return Some(shim);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        ["scoop.cmd", "scoop.ps1"]
            .into_iter()
            .map(|name| dir.join(name))
            .find(|shim| shim.is_file())
    })
}

/// Run Scoop's own installer, unless Scoop is installed already.
fn install_scoop(say: &Say) -> Result<(), String> {
    if let Some(found) = scoop_installed() {
        say.line(&format!("Scoop is installed already ({})", shown(&found)));
        return Ok(());
    }
    say.line("installing Scoop with its own installer: irm get.scoop.sh | iex");
    let powershell = cash_win32::fs::system_program(r"WindowsPowerShell\v1.0\powershell.exe");
    let status = Command::new(&powershell)
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "irm get.scoop.sh | iex",
        ])
        .status()
        .map_err(|e| format!("{}: {e}", shown(&powershell)))?;
    if status.success() {
        say.line("Scoop is installed: `scoop help` in a new window");
        Ok(())
    } else {
        Err(format!(
            "Scoop's installer ended with status {}",
            status.code().unwrap_or(-1)
        ))
    }
}

// ---- --install-remove ---------------------------------------------------------------

fn remove_command(args: &[String]) -> u8 {
    let mut root = None;
    let mut quiet = false;
    for arg in args {
        match arg.as_str() {
            "--quiet" => quiet = true,
            "-h" | "--help" => {
                println!("{REMOVE_USAGE}");
                return 0;
            }
            other if !other.starts_with("--") && root.is_none() => root = Some(other),
            other => {
                eprintln!("cash: unexpected argument `{other}`\n{REMOVE_USAGE}");
                return 2;
            }
        }
    }
    let Some(root) = root else {
        eprintln!("{REMOVE_USAGE}");
        return 2;
    };
    remove_install(Path::new(root), quiet)
}

/// Undo what `--install-finish` set up outside the install folder, and the junction in
/// it. What is already gone is no failure; what cannot be undone is named, and the
/// status is 1.
fn remove_install(root: &Path, quiet: bool) -> u8 {
    let say = Say { quiet };
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let bin = root.join("bin");
    let current = root.join("current");
    let mut failures = Vec::new();

    if crate::link_tools::has_manifest(&bin) {
        match crate::link_tools::unlink_for_installer(&bin) {
            Ok(removed) => say.line(&format!(
                "{removed} tool links removed from {}",
                shown(&bin)
            )),
            Err(message) => failures.push(message),
        }
    } else {
        match cash_win32::userpath::remove(&bin) {
            Ok(true) => say.line(&format!("{} taken off the user PATH", shown(&bin))),
            Ok(false) => {}
            Err(e) => failures.push(format!("cannot change the user PATH: {e}")),
        }
    }
    match cash_win32::userpath::remove(&current) {
        Ok(true) => say.line(&format!("{} taken off the user PATH", shown(&current))),
        Ok(false) => {}
        Err(e) => failures.push(format!("cannot change the user PATH: {e}")),
    }
    match crate::terminal_profile::remove_quietly() {
        Ok(true) => say.line("Windows Terminal profile removed"),
        Ok(false) => {}
        Err(message) => failures.push(message),
    }
    if cash_win32::junction::is_junction(&current) {
        // The junction alone: its target is a version folder the uninstaller deletes.
        match std::fs::remove_dir(&current) {
            Ok(()) => say.line(&format!("{} removed", shown(&current))),
            Err(e) => failures.push(format!("{}: {e}", shown(&current))),
        }
    }
    for failure in &failures {
        eprintln!("cash: {failure}");
    }
    u8::from(!failures.is_empty())
}

// ---- --update -----------------------------------------------------------------------

fn update_command(args: &[String]) -> u8 {
    let mut check = false;
    for arg in args {
        match arg.as_str() {
            "--check" => check = true,
            "-h" | "--help" => {
                println!("{UPDATE_USAGE}");
                return 0;
            }
            other => {
                eprintln!("cash: unexpected argument `{other}`\n{UPDATE_USAGE}");
                return 2;
            }
        }
    }
    let exe = match own_exe() {
        Ok(exe) => exe,
        Err(message) => {
            eprintln!("cash: {message}");
            return 1;
        }
    };
    if installed_by_scoop(&exe) {
        eprintln!("cash: installed by Scoop: run scoop update cash");
        return 2;
    }
    let Some(layout) = layout(&exe, cash_win32::junction::is_junction) else {
        eprintln!(
            "cash: this cash was not installed by the installer; download the new version \
             from {RELEASES_PAGE}"
        );
        return 2;
    };
    match update(&layout, check) {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("cash: {message}");
            1
        }
    }
}

/// A Windows program in System32, which every supported Windows has.
fn system_tool(name: &str) -> Result<PathBuf, String> {
    let tool = cash_win32::fs::system_program(name);
    if tool.is_file() {
        Ok(tool)
    } else {
        Err(format!(
            "{} is missing; download the new version from {RELEASES_PAGE}",
            shown(&tool)
        ))
    }
}

/// Download `url` to `to` with curl; `token` goes in an `Authorization` header.
fn fetch(curl: &Path, url: &str, to: &Path, token: Option<&str>) -> Result<(), String> {
    let mut command = Command::new(curl);
    command
        .args(["-fsSL", "--retry", "2", "-o"])
        .arg(to)
        .arg(url);
    if let Some(token) = token {
        command
            .arg("-H")
            .arg(format!("Authorization: Bearer {token}"));
    }
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", shown(curl)))?;
    if output.status.success() {
        Ok(())
    } else {
        let why = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(format!("could not download {url}: {why}"))
    }
}

/// The `tag_name` of a release, as the API's JSON gives it.
fn tag_name(json: &str) -> Option<String> {
    let (_, rest) = json.split_once("\"tag_name\"")?;
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let (tag, _) = rest.strip_prefix('"')?.split_once('"')?;
    Some(tag.to_owned())
}

/// The numbers of a version, `v` and any pre-release or build suffix dropped:
/// `v1.6.0` is `[1, 6, 0]`.
fn version_parts(text: &str) -> Option<Vec<u64>> {
    let text = text.strip_prefix('v').unwrap_or(text);
    let core = text.split(['-', '+']).next()?;
    core.split('.').map(|part| part.parse().ok()).collect()
}

/// Whether `latest` is a newer version than `current`.
fn is_newer(latest: &str, current: &str) -> Result<bool, String> {
    let read =
        |text: &str| version_parts(text).ok_or_else(|| format!("cannot read the version `{text}`"));
    Ok(read(latest)? > read(current)?)
}

/// The latest release's version, without its `v`.
fn latest_version(curl: &Path) -> Result<String, String> {
    let api = std::env::var("CASH_UPDATE_API").unwrap_or_else(|_| RELEASES_API.to_owned());
    let token = std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|token| !token.is_empty());
    let file = std::env::temp_dir().join(format!("cash-update-{}.json", std::process::id()));
    fetch(curl, &api, &file, token.as_deref())?;
    let json = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", shown(&file)));
    let _ = std::fs::remove_file(&file);
    let tag = tag_name(&json?).ok_or_else(|| format!("{api} names no release (no tag_name)"))?;
    Ok(tag.strip_prefix('v').unwrap_or(&tag).to_owned())
}

/// The hex SHA-256 of a file.
fn sha256_of(path: &Path) -> Result<String, String> {
    use sha2::Digest as _;
    use std::io::Read as _;

    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", shown(path)))?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| format!("{}: {e}", shown(path)))?;
        if read == 0 {
            break;
        }
        hasher.update(buffer.get(..read).unwrap_or_default());
    }
    Ok(hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
            hex
        }))
}

/// `cash --update [--check]` for a cash in the installer's layout.
fn update(layout: &Layout, check: bool) -> Result<(), String> {
    let curl = system_tool("curl.exe")?;
    let latest = latest_version(&curl)?;
    let current = env!("CARGO_PKG_VERSION");
    if !is_newer(&latest, current)? {
        println!("cash {current} is the latest");
        return Ok(());
    }
    if check {
        println!("cash {latest} is available (you have {current}): run cash --update");
        return Ok(());
    }

    let target = layout.root.join(&latest);
    if target.join("cash.exe").is_file() {
        println!("cash: {latest} is unpacked already in {}", shown(&target));
    } else {
        let tar = system_tool("tar.exe")?;
        let name = format!("cash-v{latest}-{TARGET}.zip");
        let base = std::env::var("CASH_UPDATE_DOWNLOAD").unwrap_or_else(|_| {
            format!("https://github.com/tomcoolpxl/cash/releases/download/v{latest}")
        });
        let zip = std::env::temp_dir().join(&name);
        let sum = std::env::temp_dir().join(format!("{name}.sha256"));
        println!("cash: downloading {name}");
        let fetched = fetch(&curl, &format!("{base}/{name}"), &zip, None)
            .and_then(|()| fetch(&curl, &format!("{base}/{name}.sha256"), &sum, None))
            .and_then(|()| verify(&zip, &sum));
        let _ = std::fs::remove_file(&sum);
        if let Err(message) = fetched {
            let _ = std::fs::remove_file(&zip);
            return Err(message);
        }
        println!("cash: the SHA-256 matches");
        let unpacked = unpack(&tar, &zip, &target);
        let _ = std::fs::remove_file(&zip);
        unpacked?;
        println!("cash: unpacked into {}", shown(&target));
    }

    // The new cash finishes its own install: the junction, the profile, the links.
    let new_exe = target.join("cash.exe");
    let status = Command::new(&new_exe)
        .arg("--install-finish")
        .arg(&layout.root)
        .status()
        .map_err(|e| format!("{}: {e}", shown(&new_exe)))?;
    if !status.success() {
        return Err(format!(
            "the finish step of {latest} failed with status {}",
            status.code().unwrap_or(-1)
        ));
    }
    println!("cash {latest} is installed: new windows get it; this one keeps {current}");
    Ok(())
}

/// Check `zip` against its `.sha256` file (`<hex>  <name>`).
fn verify(zip: &Path, sum: &Path) -> Result<(), String> {
    let listed = std::fs::read_to_string(sum).map_err(|e| format!("{}: {e}", shown(sum)))?;
    let expected = listed
        .split_whitespace()
        .next()
        .ok_or("the .sha256 file is empty")?
        .to_lowercase();
    let actual = sha256_of(zip)?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{} does not match its .sha256 ({actual} against {expected}); nothing installed",
            name_of(zip)
        ))
    }
}

/// Unpack `zip` into `target`, a new folder, with Windows' tar.
fn unpack(tar: &Path, zip: &Path, target: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(|e| format!("{}: {e}", shown(target)))?;
    let output = Command::new(tar)
        .arg("-xf")
        .arg(zip)
        .arg("-C")
        .arg(target)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", shown(tar)))?;
    if !output.status.success() {
        let why = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let _ = std::fs::remove_dir_all(target);
        return Err(format!("could not unpack {}: {why}", name_of(zip)));
    }
    if !target.join("cash.exe").is_file() {
        let _ = std::fs::remove_dir_all(target);
        return Err(format!("{} holds no cash.exe", name_of(zip)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number_not_by_text() {
        assert_eq!(is_newer("v1.6.0", "1.5.0"), Ok(true));
        assert_eq!(is_newer("v1.10.0", "1.9.3"), Ok(true));
        assert_eq!(is_newer("1.5.0", "1.5.0"), Ok(false));
        assert_eq!(is_newer("v1.4.3", "1.5.0"), Ok(false));
        assert_eq!(is_newer("v2.0.0-rc1", "1.5.0"), Ok(true));
        assert!(is_newer("latest", "1.5.0").is_err());
    }

    #[test]
    fn the_tag_name_is_read_from_the_api_json() {
        let json =
            "{\n  \"url\": \"x\",\n  \"tag_name\" : \"v1.6.0\",\n  \"name\": \"cash 1.6.0\"\n}";
        assert_eq!(tag_name(json).as_deref(), Some("v1.6.0"));
        assert_eq!(tag_name("{\"message\":\"Not Found\"}"), None);
    }

    #[test]
    fn the_sweep_takes_other_version_folders_with_a_cash_exe_and_nothing_else() {
        let root = tempfile::tempdir().unwrap();
        let make = |name: &str, with_exe: bool| {
            let dir = root.path().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            if with_exe {
                std::fs::write(dir.join("cash.exe"), "MZ").unwrap();
            }
            dir
        };
        let keep = make("1.5.0", true);
        make("1.4.3", true);
        make("1.4.2.cash-old", false);
        make("bin", true);
        make("notes", false);
        std::fs::write(root.path().join("unins000.exe"), "MZ").unwrap();
        // The junction's target is never swept, even when it is not this run's folder.
        let pointed = make("1.4.0", true);
        cash_win32::junction::point(&root.path().join("current"), &pointed).unwrap();

        let names: Vec<String> = old_versions(root.path(), &keep)
            .iter()
            .map(|path| name_of(path))
            .collect();
        assert_eq!(names, ["1.4.2.cash-old", "1.4.3"]);

        for folder in old_versions(root.path(), &keep) {
            sweep(&folder).unwrap();
        }
        assert!(!root.path().join("1.4.3").exists());
        assert!(!root.path().join("1.4.2.cash-old").exists());
        assert!(keep.join("cash.exe").is_file());
        assert!(pointed.join("cash.exe").is_file());
        assert!(root.path().join("bin").join("cash.exe").is_file());
    }

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn only_its_own_options_are_taken() {
        assert_eq!(command(&args("cash -c ls")), None);
        assert_eq!(command(&args("cash --install-finish --help")), Some(0));
        assert_eq!(command(&args("cash --install-finish")), Some(2));
        assert_eq!(command(&args("cash --install-finish a --bogus")), Some(2));
        assert_eq!(command(&args("cash --install-remove")), Some(2));
        assert_eq!(command(&args("cash --install-remove --help")), Some(0));
        assert_eq!(command(&args("cash --update --help")), Some(0));
        assert_eq!(command(&args("cash --update now")), Some(2));
    }
}
