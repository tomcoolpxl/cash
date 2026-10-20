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
        // A path in any spelling cash reads (D3), `/c/x` too.
        let accepted = crate::path::accept_path(command);
        let candidate = if crate::path::is_absolute(&accepted) {
            accepted
        } else {
            cwd.join(accepted)
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
    use std::os::windows::ffi::OsStringExt as _;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{FindClose, FindFirstFileW, WIN32_FIND_DATAW};

    let (Some(parent), Some(_)) = (path.parent(), path.file_name()) else {
        return path.to_path_buf();
    };

    // The file's own entry, which holds its name as the disk spells it: it read the
    // whole directory to find it (W32-16). A name cannot hold `*` or `?`, so the path is
    // no pattern.
    let wide = crate::wide::to_wide_nul(path);
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

/// Whether the file is a PE image Windows can start: `MZ`, and not a DLL (`IMAGE_FILE_DLL`
/// in its COFF header), which `CreateProcess` refuses and which completion offered as a
/// command (`noise.dll` from System32). One read of the file's first 512 bytes, where the
/// header lies in practice; a header beyond them is read where it is.
fn is_pe_file(path: &Path) -> bool {
    use std::io::{Read as _, Seek as _, SeekFrom};
    const IMAGE_FILE_DLL: u16 = 0x2000;

    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 512];
    let mut length = 0;
    while length < head.len() {
        match file.read(head.get_mut(length..).unwrap_or_default()) {
            Ok(0) | Err(_) => break,
            Ok(read) => length += read,
        }
    }
    let head = head.get(..length).unwrap_or_default();
    if head.get(..2) != Some(b"MZ") {
        return false;
    }
    // Where the COFF header is: `e_lfanew`, then `PE\0\0`, then its characteristics at 22.
    let Some(pe) = head
        .get(0x3C..0x40)
        .and_then(|bytes| bytes.try_into().ok())
        .map(|bytes| u32::from_le_bytes(bytes) as usize)
    else {
        // Too short for a DOS header's pointer: taken as before, a PE image.
        return true;
    };
    let coff = if let Some(coff) = head.get(pe..pe + 24) {
        coff.to_vec()
    } else {
        let mut coff = vec![0u8; 24];
        if file.seek(SeekFrom::Start(pe as u64)).is_err() || file.read_exact(&mut coff).is_err() {
            return true;
        }
        coff
    };
    if coff.get(..4) != Some(b"PE\0\0") {
        // An old DOS program, which Windows still starts.
        return true;
    }
    let characteristics = coff
        .get(22..24)
        .map_or(0, |bytes| u16::from_le_bytes([bytes[0], bytes[1]]));
    characteristics & IMAGE_FILE_DLL == 0
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
    // `CARGO_BIN_EXE_cash` and `CASH_EXE`, test hooks nothing set, sent every `#!/bin/sh`
    // to whatever a user's variable of that name held (W32-18).
    if name.eq_ignore_ascii_case("sh") || name.eq_ignore_ascii_case("bash") {
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

/// The status a shebang's interpreter would end with at once, as [`resolve_interpreter`]
/// gives it as [`Dispatch::Exit`], told without searching `PATH`.
///
/// Only `true` and `false` end so, named or through `/usr/bin/env`; an interpreter given
/// as a file is classified as `resolve_interpreter` classifies it. The shell asks before
/// it starts a script, to
/// answer one of those at once; asking `resolve_interpreter` searched `PATH` for every
/// real interpreter, which the start then searched for again.
#[must_use]
pub fn interpreter_exit(interpreter: &str, args: &[String]) -> Option<u8> {
    let name = if interpreter == "/usr/bin/env" || interpreter.ends_with("/env") {
        args.first()?.as_str()
    } else if (interpreter.contains('/') || interpreter.contains('\\'))
        && Path::new(interpreter).is_file()
    {
        return match classify(Path::new(interpreter)) {
            Dispatch::Exit(code) => Some(code),
            _ => None,
        };
    } else {
        interpreter
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(interpreter)
    };
    if name.eq_ignore_ascii_case("false") {
        Some(1)
    } else if name.eq_ignore_ascii_case("true") {
        Some(0)
    } else {
        None
    }
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
