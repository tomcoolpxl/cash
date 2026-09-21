//! Path model — **D3**, **D7**, **D29**.
//!
//! Three operations, deliberately separated because they are three different questions:
//!
//! - [`accept`] — take any spelling a script or a Windows tool might produce and work out
//!   what it means. `C:/foo`, `C:\foo`, `/c/foo`, `/tmp`, `/dev/null` all land here.
//! - [`render`] — produce cash's one canonical spelling, `C:/foo`. Forward slashes,
//!   always, because rendered paths usually become arguments to native `.exe` files.
//! - [`to_extended`] — produce the `\\?\C:\foo` form the filesystem layer actually uses
//!   (D29). This is the only place backslashes are mandatory.
//!
//! The asymmetry is the point: cash accepts everything and emits one thing.

use std::path::{Component, Path, PathBuf};

/// What a path-shaped string actually refers to.
///
/// `/dev/null` and friends are not files, so [`accept`] cannot honestly return a
/// `PathBuf` for them. D7 requires them to work; D28/D29 mean they cannot be reached by
/// ordinary path resolution, because the `\\?\` prefix would turn `NUL` into a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// An ordinary filesystem path.
    Path(PathBuf),
    /// `/dev/null` — the null device, reached as `\\.\NUL` (D7's carve-out from D29).
    Null,
    /// `/dev/stdin`.
    Stdin,
    /// `/dev/stdout`.
    Stdout,
    /// `/dev/stderr`.
    Stderr,
    /// `/dev/fd/N`.
    Fd(u32),
}

/// Interpret any accepted path spelling.
///
/// Accepts, per D3:
/// - `C:/foo` — the canonical form
/// - `C:\foo` — as pasted from Explorer, or emitted by a native tool
/// - `/c/foo` — the Unix compat spelling
/// - `/tmp`, `/dev/null`, `/dev/stdout`, `/dev/fd/3`
/// - UNC, in either slash direction
/// - relative paths, left relative
#[must_use]
pub fn accept(input: &str) -> Target {
    match device_target(input) {
        Some(target) => target,
        None => Target::Path(accept_path(input)),
    }
}

/// The `/dev/*` and `/tmp` mappings that D7 keeps always on.
fn device_target(input: &str) -> Option<Target> {
    match input {
        "/dev/null" => Some(Target::Null),
        "/dev/stdin" => Some(Target::Stdin),
        "/dev/stdout" => Some(Target::Stdout),
        "/dev/stderr" => Some(Target::Stderr),
        _ => input
            .strip_prefix("/dev/fd/")
            .and_then(|n| n.parse().ok())
            .map(Target::Fd),
    }
}

/// Interpret a path-shaped string that is not a device.
#[must_use]
pub fn accept_path(input: &str) -> PathBuf {
    // Normalise separators first: every accepted spelling uses one or the other, and
    // Windows treats them interchangeably outside the `\\?\` namespace.
    let unified: String = input.replace('\\', "/");

    // UNC: //server/share
    if unified.starts_with("//") && !unified.starts_with("///") {
        return PathBuf::from(unified.replace('/', "\\"));
    }

    // Unix drive spelling: /c/Users -> C:/Users
    if let Some(rest) = unified.strip_prefix('/')
        && let Some(drive) = rest.as_bytes().first().copied()
        && drive.is_ascii_alphabetic()
    {
        let after = &rest[1..];
        if after.is_empty() || after.starts_with('/') {
            let letter = (drive as char).to_ascii_uppercase();
            return PathBuf::from(format!("{letter}:{}", if after.is_empty() { "/" } else { after }));
        }
    }

    // /tmp -> the user's temp directory. A compat mapping, not a real mount.
    if unified == "/tmp" || unified.starts_with("/tmp/") {
        let base = std::env::var("TEMP")
            .or_else(|_| std::env::var("TMP"))
            .unwrap_or_else(|_| "C:/Windows/Temp".to_string());
        let base = base.replace('\\', "/");
        let rest = unified.strip_prefix("/tmp").unwrap_or("");
        return PathBuf::from(format!("{}{rest}", base.trim_end_matches('/')));
    }

    PathBuf::from(unified)
}

/// Render a path in cash's canonical spelling: `C:/foo/bar`.
///
/// This is what `pwd` prints and what `$PWD` holds (D3). Never backslashes — those only
/// appear inside [`to_extended`].
#[must_use]
pub fn render(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");

    // Strip the extended-length prefix if one survived this far; it is an internal
    // detail and must never be rendered to a script.
    let text = text
        .strip_prefix("//?/UNC/")
        .map(|rest| format!("//{rest}"))
        .unwrap_or_else(|| text.strip_prefix("//?/").unwrap_or(&text).to_string());

    // Uppercase the drive letter so `c:/foo` and `C:/foo` render identically — D3 says
    // one canonical form, and path comparison depends on it.
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let mut out = String::with_capacity(text.len());
        out.push(bytes[0].to_ascii_uppercase() as char);
        out.push_str(&text[1..]);
        return out;
    }

    text
}

/// Produce the `\\?\` form used at the filesystem boundary (D29).
///
/// Requires an absolute, backslash-separated path with `.` and `..` already resolved,
/// because `\\?\` is passed verbatim to the object manager — it does no normalisation
/// for us. `base` supplies the working directory for relative inputs.
///
/// Returns `None` for a relative path when `base` is itself relative, since no absolute
/// form can be produced.
#[must_use]
pub fn to_extended(path: &Path, base: &Path) -> Option<std::ffi::OsString> {
    let absolute = if is_absolute(path) {
        path.to_path_buf()
    } else if is_absolute(base) {
        base.join(path)
    } else {
        return None;
    };

    let cleaned = lexically_normalize(&absolute);
    let text = cleaned.to_string_lossy().replace('/', "\\");

    // Already extended: leave it alone.
    if text.starts_with("\\\\?\\") {
        return Some(text.into());
    }

    // UNC \\server\share -> \\?\UNC\server\share
    if let Some(rest) = text.strip_prefix("\\\\") {
        return Some(format!("\\\\?\\UNC\\{rest}").into());
    }

    Some(format!("\\\\?\\{text}").into())
}

/// Whether a path is absolute in the Windows sense cash cares about.
///
/// `std::path::Path::is_absolute` agrees for drive paths and UNC, but we also accept the
/// forward-slash spellings that D3 makes canonical.
#[must_use]
pub fn is_absolute(path: &Path) -> bool {
    let text = path.to_string_lossy();
    let bytes = text.as_bytes();

    if text.starts_with("\\\\") || text.starts_with("//") {
        return true;
    }
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
}

/// Resolve `.` and `..` without touching the filesystem.
///
/// D29 requires this: the `\\?\` prefix disables the OS normalisation that would
/// otherwise do it for us. Purely lexical, so it does not follow symlinks — which is the
/// correct behaviour for building a path to hand to `CreateFileW`.
#[must_use]
pub fn lexically_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    let mut depth = 0usize;

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if depth > 0 {
                    out.pop();
                    depth -= 1;
                } else {
                    // Above the root: Windows clamps rather than erroring.
                    out.push("..");
                }
            }
            other => {
                out.push(other.as_os_str());
                if matches!(other, Component::Normal(_)) {
                    depth += 1;
                }
            }
        }
    }

    out
}

/// Convert a path to the Unix compat spelling, `/c/foo`.
///
/// The inverse of [`accept_path`]'s drive handling, and what the `winpath` builtin (D45)
/// emits when asked for Unix form. Not used for rendering — D3 renders Windows-style.
#[must_use]
pub fn to_unix(path: &Path) -> String {
    let text = render(path);
    let bytes = text.as_bytes();

    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let letter = (bytes[0] as char).to_ascii_lowercase();
        let rest = &text[2..];
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        return if rest.is_empty() {
            format!("/{letter}")
        } else {
            format!("/{letter}/{rest}")
        };
    }

    text
}

/// Convert a path to the backslash spelling a DOS-lineage tool may require.
///
/// D4 forbids cash rewriting arguments on its own, so this is the deliberate escape
/// hatch the `winpath` builtin (D45) exposes.
#[must_use]
pub fn to_backslash(path: &Path) -> String {
    render(path).replace('/', "\\")
}

/// Win32 device names that are reserved in the ordinary path namespace (D28).
///
/// Reserved with *any* extension, so `nul.txt` is the device too.
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Whether a path's final component is a reserved Win32 device name (D28).
///
/// Such a path can only be opened as an ordinary file through the `\?\` namespace,
/// which bypasses reserved-name parsing. D28 wants `touch nul` to create a file called
/// `nul`, exactly as on Linux, because a POSIX script never means the device — it writes
/// `/dev/null`, which D7 maps explicitly.
#[must_use]
pub fn is_reserved_name(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };
    let name = name.to_string_lossy();

    // The name before the first dot is what Win32 matches against.
    let stem = name.split('.').next().unwrap_or(&name);
    RESERVED_NAMES
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
}
