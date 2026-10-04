//! Command resolution — **D8**, with **D46**'s ordering constraint.
//!
//! ```text
//! 1. builtin / function / alias        (the shell's business, not this module's)
//! 2. resolution cache
//! 3. PATH search, honouring PATHEXT
//! 4. dispatch by type, extension checked BEFORE any file read
//! ```
//!
//! **Extension before read is load-bearing, not stylistic.** App Execution Aliases —
//! `python.exe`, `bash.exe` and dozens of others in `%LOCALAPPDATA%\Microsoft\
//! WindowsApps` — are 0-byte reparse points carrying `IO_REPARSE_TAG_APPEXECLINK`. They
//! execute because `CreateProcessW` resolves the tag natively, but *reading* one yields
//! nothing. Checking PATHEXT first means `python.exe` dispatches as a native executable
//! and the empty read never happens.
//!
//! cash has no Scoop knowledge (D25): a shim is just another executable on `PATH`.

use std::path::{Path, PathBuf};

use crate::text;

/// How a resolved command should be executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dispatch {
    /// `.exe` / `.com` — straight to `CreateProcessW`.
    Native(PathBuf),
    /// `.cmd` / `.bat` — via `cmd.exe /d /s /c`, with arguments escaped per D32.
    Batch(PathBuf),
    /// `.ps1` — via `pwsh.exe`, falling back to `powershell.exe`.
    PowerShell(PathBuf),
    /// A script with a `#!` line; the interpreter is resolved separately.
    Shebang {
        /// The interpreter named after `#!`.
        interpreter: String,
        /// Arguments given on the shebang line, before the script path.
        args: Vec<String>,
        /// The text after the interpreter as it stands, which is what the kernel hands it
        /// as one argument: `#!/usr/bin/env -S bash -e` gives `env` the string
        /// `-S bash -e` to split (W32-08).
        line: String,
        /// The script itself.
        script: PathBuf,
    },
    /// An exit action (e.g. `/bin/false` -> 1, `/bin/true` -> 0).
    Exit(u8),
}

impl Dispatch {
    /// The file on disk this dispatch refers to.
    #[must_use]
    pub fn target(&self) -> &Path {
        match self {
            Self::Native(p) | Self::Batch(p) | Self::PowerShell(p) => p,
            Self::Shebang { script, .. } => script,
            Self::Exit(_) => Path::new(""),
        }
    }
}

/// The default `PATHEXT`, used when the environment does not supply one.
pub const DEFAULT_PATHEXT: &[&str] = &[".COM", ".EXE", ".BAT", ".CMD"];

/// The extensions a `PATHEXT` value names, or the default ones when it is unset or
/// empty: each with its leading dot, in upper case.
///
/// The shell's value is the shell's variable (`Shell::pathext`), which a script can change
/// without changing the process's; only code that runs as a process of its own (a bundled
/// tool, `cash doctor`) reads the process's, through [`process_pathext`].
#[must_use]
pub fn pathext_of(value: Option<&str>) -> Vec<String> {
    match value.map(str::trim) {
        Some(value) if !value.is_empty() => parse_pathext(value),
        _ => DEFAULT_PATHEXT
            .iter()
            .map(|ext| (*ext).to_owned())
            .collect(),
    }
}

/// The extensions of this process's own `PATHEXT`; see [`pathext_of`].
#[must_use]
pub fn process_pathext() -> Vec<String> {
    pathext_of(std::env::var("PATHEXT").ok().as_deref())
}

/// Parse a `PATHEXT` value into extensions, each including the leading dot.
#[must_use]
pub fn parse_pathext(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            let s = s.to_ascii_uppercase();
            if s.starts_with('.') {
                s
            } else {
                format!(".{s}")
            }
        })
        .collect()
}

/// Find a command on `PATH` and decide how to run it (D8).
///
/// A name containing a separator is treated as a path and not searched for, matching
/// POSIX. Otherwise each `PATH` entry is tried with the bare name first, then with each
/// `PATHEXT` extension appended.
#[must_use]
pub fn resolve(
    command: &str,
    path_entries: &[PathBuf],
    pathext: &[String],
    cwd: &Path,
) -> Option<Dispatch> {
    if command.contains('/') || command.contains('\\') {
        let candidate = if crate::path::is_absolute(Path::new(command)) {
            PathBuf::from(command)
        } else {
            cwd.join(command)
        };
        return classify_if_exists(&candidate, pathext);
    }

    for entry in path_entries {
        if let Some(found) = classify_if_exists(&entry.join(command), pathext) {
            return Some(found);
        }
    }

    None
}

/// Try a candidate path, with and without `PATHEXT` extensions.
fn classify_if_exists(candidate: &Path, pathext: &[String]) -> Option<Dispatch> {
    // If the candidate already has an extension, try the exact file first.
    if candidate.extension().is_some() && candidate.is_file() {
        return Some(classify(candidate));
    }

    // When the name has no extension, Windows command resolution prioritizes PATHEXT
    // (.COM, .EXE, .BAT, .CMD) so native binaries (like `docker.exe`) are chosen over
    // extensionless files/wrapper scripts in the same directory.
    for extension in pathext {
        let mut with_extension = candidate.as_os_str().to_os_string();
        with_extension.push(extension);
        let path = PathBuf::from(with_extension);
        if path.is_file() {
            // PATHEXT is conventionally uppercase (".EXE"), but the file on disk is
            // almost always lowercase. Windows opens either, so the mismatch is
            // invisible until the constructed spelling leaks into `command -v` output,
            // `$0`, or an error message — at which point it contradicts D3's promise of
            // one canonical form. Recover the real on-disk name instead.
            return Some(classify(&real_case(&path)));
        }
    }

    // If no PATHEXT extension matches, fall back to the extensionless file if it exists.
    if candidate.is_file() {
        return Some(classify(candidate));
    }

    None
}

/// Recover a file's true on-disk name, which may differ in case from how we spelled it.
///
/// Done by listing the parent directory rather than `fs::canonicalize`, deliberately:
/// canonicalisation resolves reparse points, and App Execution Aliases (D46) are reparse
/// points. Resolving one would report the packaged app's real location rather than the
/// alias the user actually invoked.
///
/// Falls back to the input if the directory cannot be read, since a wrong case is far
/// better than a failed lookup.
#[must_use]
pub fn real_case(path: &Path) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{FindClose, FindFirstFileW, WIN32_FIND_DATAW};

    let (Some(parent), Some(_)) = (path.parent(), path.file_name()) else {
        return path.to_path_buf();
    };

    // The file's own entry, which holds its name as the disk spells it: it read the
    // whole directory to find it (W32-16). A name cannot hold `*` or `?`, so the path is
    // no pattern.
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: an all-zero WIN32_FIND_DATAW is a valid value for the out-parameter.
    let mut found: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    // SAFETY: the path is NUL-terminated and `found` outlives the call.
    let handle = unsafe { FindFirstFileW(wide.as_ptr(), &raw mut found) };
    if handle == INVALID_HANDLE_VALUE {
        return path.to_path_buf();
    }
    // SAFETY: a search handle FindFirstFileW opened, closed once.
    unsafe { FindClose(handle) };

    let name = &found.cFileName;
    let length = name
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(name.len());
    name.get(..length)
        .filter(|name| !name.is_empty())
        .map_or_else(
            || path.to_path_buf(),
            |name| parent.join(std::ffi::OsString::from_wide(name)),
        )
}

fn is_pe_file(path: &Path) -> bool {
    use std::io::Read as _;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 2];
    if file.read_exact(&mut magic).is_ok() {
        magic == *b"MZ"
    } else {
        false
    }
}

/// Whether a file's own bytes make it runnable: a `#!` line, or a PE image (`MZ`).
///
/// This is how an extensionless file earns the execute bit that Windows never records.
/// Git for Windows' `/usr/bin/egrep` is a two-line `#!/bin/sh` script with no extension;
/// execution runs it (D8 step 4), so lookups (`type -a`, `command -v`, `test -x`) have to
/// count it as executable too, or they deny a command that plainly runs.
///
/// Reads the file, so callers must check the extension first (D46).
#[must_use]
pub fn has_executable_content(path: &Path) -> bool {
    read_shebang(path).is_some() || is_pe_file(path)
}

/// Decide how to execute a file that is known to exist (D8 step 4).
///
/// Extension is consulted first, per D46. Only a file whose extension says nothing is
/// read, and then only its first line.
#[must_use]
pub fn classify(path: &Path) -> Dispatch {
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_uppercase())
        .unwrap_or_default();

    match extension.as_str() {
        "EXE" | "COM" => Dispatch::Native(path.to_path_buf()),
        "CMD" | "BAT" => Dispatch::Batch(path.to_path_buf()),
        "PS1" => Dispatch::PowerShell(path.to_path_buf()),
        _ => match read_shebang_line(path) {
            Some((interpreter, line)) => Dispatch::Shebang {
                interpreter,
                args: line.split_whitespace().map(str::to_string).collect(),
                line,
                script: path.to_path_buf(),
            },
            None => {
                if is_pe_file(path) {
                    Dispatch::Native(path.to_path_buf())
                } else {
                    // POSIX fallback: a script with no shebang is executed with `sh`.
                    Dispatch::Shebang {
                        interpreter: "sh".to_string(),
                        args: Vec::new(),
                        line: String::new(),
                        script: path.to_path_buf(),
                    }
                }
            }
        },
    }
}

/// Read a `#!` line, if there is one.
///
/// Only the first line is read, and a BOM is tolerated (D41) because a shebang preceded
/// by one is otherwise invisible — which is exactly how a Notepad-saved script fails.
#[must_use]
pub fn read_shebang(path: &Path) -> Option<(String, Vec<String>)> {
    let (interpreter, line) = read_shebang_line(path)?;
    Some((
        interpreter,
        line.split_whitespace().map(str::to_string).collect(),
    ))
}

/// The interpreter a `#!` line names and the text after it, unsplit.
#[must_use]
pub fn read_shebang_line(path: &Path) -> Option<(String, String)> {
    use std::io::Read as _;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 1024];
    let n = file.read(&mut buf).ok()?;
    let bytes = buf.get(..n)?;
    let bytes = text::strip_bom(bytes);

    let line_end = bytes
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(bytes.len());
    let line = std::str::from_utf8(bytes.get(..line_end)?).ok()?;
    let line = text::trim_line_terminator(line);

    let rest = line.strip_prefix("#!")?.trim();
    if rest.is_empty() {
        return None;
    }

    let (interpreter, line) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    Some((interpreter.to_string(), line.trim().to_string()))
}

fn resolve_named_interpreter(
    name: &str,
    path_entries: &[PathBuf],
    pathext: &[String],
    cwd: &Path,
) -> Option<Dispatch> {
    if name.eq_ignore_ascii_case("false") {
        return Some(Dispatch::Exit(1));
    }
    if name.eq_ignore_ascii_case("true") {
        return Some(Dispatch::Exit(0));
    }
    if name.eq_ignore_ascii_case("sh") || name.eq_ignore_ascii_case("bash") {
        if let Ok(cash_exe) = std::env::var("CARGO_BIN_EXE_cash") {
            return Some(Dispatch::Native(PathBuf::from(cash_exe)));
        }
        if let Ok(cash_exe) = std::env::var("CASH_EXE") {
            return Some(Dispatch::Native(PathBuf::from(cash_exe)));
        }
        if let Ok(own) = std::env::current_exe() {
            if own
                .file_stem()
                .is_some_and(|s| s.eq_ignore_ascii_case("cash"))
            {
                return Some(Dispatch::Native(own));
            }
        }
        if let Some(d) = resolve("cash", path_entries, pathext, cwd) {
            return Some(Dispatch::Native(d.target().to_path_buf()));
        }
        if let Ok(own) = std::env::current_exe() {
            return Some(Dispatch::Native(own));
        }
    }
    if name.eq_ignore_ascii_case("pwsh") {
        if let Some(d) = resolve("pwsh", path_entries, pathext, cwd) {
            return Some(Dispatch::PowerShell(d.target().to_path_buf()));
        }
        if let Some(d) = resolve("powershell", path_entries, pathext, cwd) {
            return Some(Dispatch::PowerShell(d.target().to_path_buf()));
        }
        return None;
    }
    if name.eq_ignore_ascii_case("powershell") {
        if let Some(d) = resolve("powershell", path_entries, pathext, cwd) {
            return Some(Dispatch::PowerShell(d.target().to_path_buf()));
        }
        if let Some(d) = resolve("pwsh", path_entries, pathext, cwd) {
            return Some(Dispatch::PowerShell(d.target().to_path_buf()));
        }
        return None;
    }
    if name.eq_ignore_ascii_case("python3") {
        if let Some(d) = resolve("python3", path_entries, pathext, cwd) {
            return Some(d);
        }
        return resolve("python", path_entries, pathext, cwd);
    }
    if name.eq_ignore_ascii_case("python") {
        if let Some(d) = resolve("python", path_entries, pathext, cwd) {
            return Some(d);
        }
        return resolve("python3", path_entries, pathext, cwd);
    }

    resolve(name, path_entries, pathext, cwd)
}

/// Resolve the interpreter named by a shebang line.
///
/// D7 keeps a virtual `/usr/bin/env`: `#!/usr/bin/env bash` resolves without a fake
/// filesystem, by taking the first argument as the real command and searching `PATH`
/// for it. `/bin/sh` and friends are likewise reduced to their basename and searched,
/// since those directories do not exist on Windows.
#[must_use]
pub fn resolve_interpreter(
    interpreter: &str,
    args: &[String],
    path_entries: &[PathBuf],
    pathext: &[String],
    cwd: &Path,
) -> Option<(Dispatch, Vec<String>)> {
    // Virtual /usr/bin/env: the real command is the first argument.
    if interpreter == "/usr/bin/env" || interpreter.ends_with("/env") {
        let (name, rest) = args.split_first()?;
        let dispatch = resolve_named_interpreter(name, path_entries, pathext, cwd)?;
        return Some((dispatch, rest.to_vec()));
    }

    if (interpreter.contains('/') || interpreter.contains('\\')) && Path::new(interpreter).is_file()
    {
        let dispatch = classify(Path::new(interpreter));
        return Some((dispatch, args.to_vec()));
    }

    // Any other absolute Unix path — /bin/sh, /usr/bin/python3 — cannot exist on
    // Windows, so fall back to searching PATH for its basename.
    let name = interpreter
        .rsplit('/')
        .next()
        .and_then(|s| s.rsplit('\\').next())
        .unwrap_or(interpreter);
    let dispatch = resolve_named_interpreter(name, path_entries, pathext, cwd)?;
    Some((dispatch, args.to_vec()))
}
