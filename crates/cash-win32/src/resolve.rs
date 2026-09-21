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
        /// The script itself.
        script: PathBuf,
    },
}

impl Dispatch {
    /// The file on disk this dispatch refers to.
    #[must_use]
    pub fn target(&self) -> &Path {
        match self {
            Self::Native(p) | Self::Batch(p) | Self::PowerShell(p) => p,
            Self::Shebang { script, .. } => script,
        }
    }
}

/// The default `PATHEXT`, used when the environment does not supply one.
pub const DEFAULT_PATHEXT: &[&str] = &[".COM", ".EXE", ".BAT", ".CMD"];

/// Parse a `PATHEXT` value into extensions, each including the leading dot.
#[must_use]
pub fn parse_pathext(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            let s = s.to_ascii_uppercase();
            if s.starts_with('.') { s } else { format!(".{s}") }
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
    // Bare name first: an explicitly-spelled `foo.exe` or an extensionless script.
    if candidate.is_file() {
        return Some(classify(candidate));
    }

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
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return path.to_path_buf();
    };

    let Ok(entries) = std::fs::read_dir(parent) else {
        return path.to_path_buf();
    };

    let wanted = name.to_string_lossy();
    for entry in entries.flatten() {
        let actual = entry.file_name();
        if actual.to_string_lossy().eq_ignore_ascii_case(&wanted) {
            return parent.join(actual);
        }
    }

    path.to_path_buf()
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
        _ => match read_shebang(path) {
            Some((interpreter, args)) => Dispatch::Shebang {
                interpreter,
                args,
                script: path.to_path_buf(),
            },
            // No extension cash knows and no shebang: hand it to CreateProcessW and let
            // Windows decide. It may still be executable.
            None => Dispatch::Native(path.to_path_buf()),
        },
    }
}

/// Read a `#!` line, if there is one.
///
/// Only the first line is read, and a BOM is tolerated (D41) because a shebang preceded
/// by one is otherwise invisible — which is exactly how a Notepad-saved script fails.
#[must_use]
pub fn read_shebang(path: &Path) -> Option<(String, Vec<String>)> {
    let bytes = std::fs::read(path).ok()?;
    let bytes = text::strip_bom(&bytes);

    let line_end = bytes.iter().position(|&b| b == b'\n').unwrap_or(bytes.len());
    let line = std::str::from_utf8(&bytes[..line_end]).ok()?;
    let line = text::trim_line_terminator(line);

    let rest = line.strip_prefix("#!")?.trim();
    if rest.is_empty() {
        return None;
    }

    let mut parts = rest.split_whitespace();
    let interpreter = parts.next()?.to_string();
    let args: Vec<String> = parts.map(str::to_string).collect();

    Some((interpreter, args))
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
        let dispatch = resolve(name, path_entries, pathext, cwd)?;
        return Some((dispatch, rest.to_vec()));
    }

    // Any other absolute Unix path — /bin/sh, /usr/bin/python3 — cannot exist on
    // Windows, so fall back to searching PATH for its basename.
    let name = interpreter.rsplit('/').next().unwrap_or(interpreter);
    let dispatch = resolve(name, path_entries, pathext, cwd)?;
    Some((dispatch, args.to_vec()))
}
