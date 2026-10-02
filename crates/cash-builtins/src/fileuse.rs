//! Shared queries for `fuser` and `lsof`: who holds a file, how, and what a process is.
//!
//! Everything here rests on documented APIs (ROADMAP item 8): the Restart Manager for
//! files, IP Helper's socket tables for ports, and per-process image and module queries.
//! Windows has no system-wide handle listing short of the undocumented
//! `NtQuerySystemInformation` walk, so what those sources cannot answer is reported as
//! unknown rather than guessed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cash_win32::{process, restart};

/// How a process uses a file, as far as Windows lets us tell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Access {
    /// The process's own executable (`fuser`'s `e`, `lsof`'s `txt`).
    Executable,
    /// A loaded module, such as a DLL (`m`, `mem`).
    Mapped,
    /// Open in some other way (`f`, and an unknown descriptor in `lsof`).
    Open,
}

/// One process holding one file.
#[derive(Clone, Debug)]
pub(crate) struct FileHolder {
    pub pid: u32,
    pub path: PathBuf,
    pub access: Access,
}

/// A comparable spelling of a path: Windows paths are case-insensitive and accept either
/// separator.
pub(crate) fn path_key(path: &Path) -> String {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy().replace('/', "\\");
    text.strip_prefix(r"\\?\")
        .unwrap_or(&text)
        .to_ascii_lowercase()
}

/// The file a holder is asked about: its [`path_key`], and the names it may go by in a
/// process's image or module list.
struct Target {
    key: String,
    /// Its file name as given and as resolved, lowercase: `kernel32.dll`.
    names: Vec<String>,
}

impl Target {
    fn new(file: &Path) -> Self {
        let key = path_key(file);
        let mut names: Vec<String> = [Some(file), Some(Path::new(&key))]
            .into_iter()
            .flatten()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .collect();
        names.dedup();
        Self { key, names }
    }

    /// Whether `path`, from an image or module list, is this file.
    ///
    /// Only a path of the same file name is resolved and compared: resolving opens the
    /// file, and for a system DLL there are some 15,000 modules in a few hundred
    /// processes to look through, which made `fuser kernel32.dll` take seconds alone and
    /// tens of seconds beside a build (2026-09-30).
    fn is(&self, path: &Path) -> bool {
        let named = path.file_name().is_some_and(|name| {
            let name = name.to_string_lossy().to_ascii_lowercase();
            self.names.contains(&name)
        });
        named && path_key(path) == self.key
    }
}

/// How `pid` uses `target`.
fn classify(pid: u32, target: &Target, modules: &mut HashMap<u32, Option<Vec<PathBuf>>>) -> Access {
    if process::image_path(pid).is_some_and(|image| target.is(&image)) {
        return Access::Executable;
    }
    let loaded = modules
        .entry(pid)
        .or_insert_with(|| process::modules(pid))
        .as_ref()
        .is_some_and(|list| list.iter().any(|module| target.is(module)));
    if loaded { Access::Mapped } else { Access::Open }
}

/// The processes holding `file` open, one entry per process.
///
/// The Restart Manager fails (invalid handle) when a holder is a process it cannot
/// query, which is always the case for system DLLs. The answer is then built from
/// each process's image and module list instead: that finds executables and loaded
/// modules in every process this user can open, which is what such files are held as.
pub(crate) fn file_holders(file: &Path) -> std::io::Result<Vec<FileHolder>> {
    let target = Target::new(file);
    let mut modules = HashMap::new();
    let holders = match restart::holders(&[file]) {
        Ok(holders) => holders.into_iter().map(|h| h.pid).collect::<Vec<_>>(),
        Err(error) if error.raw_os_error() == Some(6) => {
            return Ok(module_holders(file, &target, &mut modules));
        }
        Err(error) => return Err(error),
    };
    Ok(holders
        .into_iter()
        .map(|pid| FileHolder {
            pid,
            path: file.to_path_buf(),
            access: classify(pid, &target, &mut modules),
        })
        .collect())
}

/// The processes whose executable or a loaded module is `target`.
fn module_holders(
    file: &Path,
    target: &Target,
    modules: &mut HashMap<u32, Option<Vec<PathBuf>>>,
) -> Vec<FileHolder> {
    process::list()
        .into_iter()
        .filter_map(|p| {
            let access = classify(p.pid, target, modules);
            (access != Access::Open).then(|| FileHolder {
                pid: p.pid,
                path: file.to_path_buf(),
                access,
            })
        })
        .collect()
}

/// Every regular file below `dir`, not following directory links or junctions.
pub(crate) fn files_below(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    files
}

/// The holders of any file below `dir`, each attributed to the files it holds.
///
/// The Restart Manager reports the union over all registered files, so this asks once
/// for the whole tree and, only when someone holds something, once per file to
/// attribute it.
pub(crate) fn holders_below(dir: &Path) -> std::io::Result<Vec<FileHolder>> {
    let files = files_below(dir);
    let refs: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
    if restart::holders(&refs)?.is_empty() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for file in &files {
        found.extend(file_holders(file)?);
    }
    Ok(found)
}

/// Process names and users, looked up once per command.
pub(crate) struct ProcessNames {
    names: HashMap<u32, String>,
    users: HashMap<u32, Option<String>>,
}

impl ProcessNames {
    pub(crate) fn new() -> Self {
        Self {
            names: process::list()
                .into_iter()
                .map(|p| (p.pid, p.name))
                .collect(),
            users: HashMap::new(),
        }
    }

    /// The executable name, or `?` for a process that has exited since.
    pub(crate) fn name(&self, pid: u32) -> &str {
        match pid {
            0 => "System Idle Process",
            _ => self.names.get(&pid).map_or("?", String::as_str),
        }
    }

    /// The account name, or `?` when it cannot be read without elevation.
    pub(crate) fn user(&mut self, pid: u32) -> String {
        self.users
            .entry(pid)
            .or_insert_with(|| process::details(pid).user)
            .clone()
            .unwrap_or_else(|| "?".to_owned())
    }
}

/// Service names for ports, from Windows' own `services` file, the equivalent of
/// `/etc/services`.
pub(crate) struct Services {
    by_port: HashMap<(u16, &'static str), String>,
    by_name: HashMap<(String, &'static str), u16>,
}

impl Services {
    pub(crate) fn load() -> Self {
        let mut services = Self {
            by_port: HashMap::new(),
            by_name: HashMap::new(),
        };
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let file = Path::new(&root).join(r"System32\drivers\etc\services");
        let Ok(text) = std::fs::read_to_string(file) else {
            return services;
        };
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("");
            let mut fields = line.split_whitespace();
            let (Some(name), Some(spec)) = (fields.next(), fields.next()) else {
                continue;
            };
            let Some((port, proto)) = spec.split_once('/') else {
                continue;
            };
            let Ok(port) = port.parse::<u16>() else {
                continue;
            };
            let proto: &'static str = match proto.to_ascii_lowercase().as_str() {
                "tcp" => "tcp",
                "udp" => "udp",
                _ => continue,
            };
            services
                .by_port
                .entry((port, proto))
                .or_insert_with(|| name.to_owned());
            for alias in std::iter::once(name).chain(fields) {
                services
                    .by_name
                    .entry((alias.to_ascii_lowercase(), proto))
                    .or_insert(port);
            }
        }
        services
    }

    /// The service name for a port, if the file lists one.
    pub(crate) fn name(&self, port: u16, proto: &'static str) -> Option<&str> {
        self.by_port.get(&(port, proto)).map(String::as_str)
    }

    /// A port given as a number or a service name.
    pub(crate) fn port(&self, text: &str, proto: &'static str) -> Option<u16> {
        text.parse().ok().or_else(|| {
            self.by_name
                .get(&(text.to_ascii_lowercase(), proto))
                .copied()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_keys_ignore_case_and_separators() {
        assert_eq!(
            path_key(Path::new("C:/Windows/System32/../System32/KERNEL32.dll")),
            path_key(Path::new(r"c:\windows\system32\kernel32.DLL"))
        );
    }

    #[test]
    fn this_process_holds_its_own_executable() {
        let exe = std::env::current_exe().unwrap();
        let found = file_holders(&exe).unwrap();
        let me = found.iter().find(|h| h.pid == std::process::id()).unwrap();
        assert_eq!(me.access, Access::Executable);
    }

    #[test]
    fn a_loaded_dll_is_mapped() {
        let root = std::env::var_os("SystemRoot").unwrap();
        let kernel32 = Path::new(&root).join(r"System32\kernel32.dll");
        // Asked of this process alone. The Restart Manager cannot answer for a system
        // DLL, so `file_holders` reads the module list of every process on the machine:
        // 12,500 modules in 220 processes here. `fuser` on a system DLL (fuser_lsof.rs)
        // is the one test that makes that walk.
        let access = classify(
            std::process::id(),
            &Target::new(&kernel32),
            &mut HashMap::new(),
        );
        assert_eq!(access, Access::Mapped);
        // Spelled another way, in another case and the other separator.
        let spelled = kernel32
            .to_string_lossy()
            .to_ascii_uppercase()
            .replace('\\', "/");
        let access = classify(
            std::process::id(),
            &Target::new(Path::new(&spelled)),
            &mut HashMap::new(),
        );
        assert_eq!(access, Access::Mapped);
    }

    #[test]
    fn well_known_services_resolve_both_ways() {
        let services = Services::load();
        assert_eq!(services.port("http", "tcp"), Some(80));
        assert_eq!(services.port("8080", "tcp"), Some(8080));
        assert_eq!(services.name(80, "tcp"), Some("http"));
    }
}
