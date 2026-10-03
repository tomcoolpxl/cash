//! Path model — **D3**, **D7**, **D29**.
//!
//! Two operations, deliberately separated because they are two different questions:
//!
//! - [`accept`] — take any spelling a script or a Windows tool might produce and work out
//!   what it means. `C:/foo`, `C:\foo`, `/c/foo`, `/tmp`, `/dev/null` all land here.
//! - [`render`] — produce cash's one canonical spelling, `C:/foo`. Forward slashes,
//!   always, because rendered paths usually become arguments to native `.exe` files.
//!
//! The asymmetry is the point: cash accepts everything and emits one thing. A path longer
//! than `MAX_PATH` reaches the filesystem in its `\\?\` form through the standard library,
//! which adds the prefix itself (D29); [`process_directory`] handles the one place that
//! form cannot go, a new program's working directory.

use std::path::{Path, PathBuf};

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

/// The `/dev/*` mappings that D7 keeps always on.
///
/// A device is named by the spelling alone, so the name has to be rooted at `/`:
/// `dev/null` is a file in a folder called `dev`, and so is `C:/dev/null`. What a
/// filesystem would read as the same name is accepted as that name — `/dev//null`,
/// `/dev/./null`, either separator (D3) — because `"$dir/null"` with `dir=/dev/` writes
/// the first. `//dev/null` is not: two leading slashes are a UNC path.
fn device_target(input: &str) -> Option<Target> {
    let unified = input.replace('\\', "/");
    if unified.starts_with("//") && !unified.starts_with("///") {
        return None;
    }

    let mut segments = unified
        .strip_prefix('/')?
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".");
    if segments.next()? != "dev" {
        return None;
    }

    match (segments.next()?, segments.next(), segments.next()) {
        ("null", None, _) => Some(Target::Null),
        ("stdin", None, _) => Some(Target::Stdin),
        ("stdout", None, _) => Some(Target::Stdout),
        ("stderr", None, _) => Some(Target::Stderr),
        // Digits only: `parse` on its own takes `+3` for a number.
        ("fd", Some(number), None) if number.bytes().all(|b| b.is_ascii_digit()) => {
            number.parse().ok().map(Target::Fd)
        }
        _ => None,
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
        let after = rest.get(1..).unwrap_or_default();
        if after.is_empty() || after.starts_with('/') {
            let letter = (drive as char).to_ascii_uppercase();
            return PathBuf::from(format!(
                "{letter}:{}",
                if after.is_empty() { "/" } else { after }
            ));
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
/// This is what `pwd` prints and what `$PWD` holds (D3). Never backslashes.
#[must_use]
pub fn render(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");

    // Strip the extended-length prefix if one survived this far; it is an internal
    // detail and must never be rendered to a script.
    let text = if let Some(rest) = text.strip_prefix("//?/UNC/") {
        format!("//{rest}")
    } else if let Some(rest) = text.strip_prefix("//?/") {
        rest.to_string()
    } else {
        text
    };

    // Uppercase the drive letter so `c:/foo` and `C:/foo` render identically — D3 says
    // one canonical form, and path comparison depends on it.
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let mut out = String::with_capacity(text.len());
        out.push(bytes[0].to_ascii_uppercase() as char);
        out.push_str(text.get(1..).unwrap_or_default());
        return out;
    }

    text
}

/// cash's own executable, rendered, with `/` appended: the prefix of every virtual path.
fn own_prefix() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("{}/", render(&exe)))
}

/// The path `which` prints for a command cash carries itself: cash's own executable
/// with the name appended, `C:/…/cash.exe/ls` (ROADMAP item 12).
///
/// No file can live there — `cash.exe` is a file, not a directory — so the path never
/// names anything else, and it says which cash runs the command. It is one word, so a
/// script's `LS=$(which ls); "$LS" -la` works: cash recognises the path when it is run
/// ([`virtual_tool`]). Programs outside cash cannot run it; nothing without a file on
/// disk could offer them that.
pub fn virtual_path(name: &str) -> Option<String> {
    own_prefix().map(|prefix| format!("{prefix}{name}"))
}

/// The command a [`virtual_path`] stands for, when `spelled` is one: cash's own
/// executable, in any accepted spelling, followed by a single name.
pub fn virtual_tool(spelled: &str) -> Option<String> {
    if !spelled.contains(['/', '\\']) {
        return None;
    }
    let prefix = own_prefix()?;
    let rendered = render(&accept_path(spelled));
    let head = rendered.get(..prefix.len())?;
    if !head.eq_ignore_ascii_case(&prefix) {
        return None;
    }
    let name = rendered.get(prefix.len()..)?;
    (!name.is_empty() && !name.contains('/')).then(|| name.to_owned())
}

/// `cash -c '"$0" "$@"' TOOL`, to which the caller adds the arguments.
///
/// A process that runs the command `tool` names as cash would if it were typed, for a
/// [`virtual_path`] run where a process is needed (`exec`, `xargs`, `find -exec`).
pub fn reentry_command(tool: &str) -> std::process::Command {
    let own = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("cash.exe"));
    let mut command = std::process::Command::new(own);
    command.args(["-c", "\"$0\" \"$@\"", tool]);
    command
}

/// The longest working directory a process can be started in.
///
/// `MAX_PATH` less the backslash Windows appends and the terminating NUL
/// (`SetCurrentDirectoryW`). Long-path support does not lift it for a new process.
pub const MAX_PROCESS_DIRECTORY: usize = 258;

/// The folder to start a program in for the shell's working directory `dir`.
///
/// Windows starts no process in a folder whose path is longer than
/// [`MAX_PROCESS_DIRECTORY`], long-path support or not, and says only "The directory name is
/// invalid", which reached the user as `C:\…\cash.exe: Not a directory` for every
/// program, cash's bundled tools included. Such a folder is given by its 8.3 short name
/// (`C:\…\CASH-L~2\AAAAAA~1`), which Windows keeps on most volumes and which usually
/// fits; the program sees that spelling as its working directory, and relative paths
/// mean what they meant. Where there is none short enough, the error says why (the user,
/// 2026-10-04).
///
/// # Errors
///
/// The folder is too long and has no short name that fits.
pub fn process_directory(dir: &Path) -> std::io::Result<PathBuf> {
    let text = dir.to_string_lossy();
    let plain = text
        .strip_prefix(r"\\?\")
        .unwrap_or(&text)
        .replace('/', "\\");
    let length = plain.encode_utf16().count();
    if length <= MAX_PROCESS_DIRECTORY {
        return Ok(dir.to_path_buf());
    }
    if let Some(short) = short_name(&plain)
        && short.encode_utf16().count() <= MAX_PROCESS_DIRECTORY
    {
        return Ok(PathBuf::from(short));
    }
    Err(std::io::Error::other(format!(
        "the working directory is too long for Windows to start a program in \
         ({length} characters, the limit is {MAX_PROCESS_DIRECTORY}), and it has no short \
         name that fits"
    )))
}

/// The 8.3 short form of the folder `plain` (backslashes, no `\\?\`), without `\\?\`.
fn short_name(plain: &str) -> Option<String> {
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

    // A path this long reaches the API only in its extended form.
    let long = if let Some(unc) = plain.strip_prefix(r"\\") {
        format!(r"\\?\UNC\{unc}")
    } else {
        format!(r"\\?\{plain}")
    };
    let wide: Vec<u16> = long.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: a null buffer of length 0 asks for the length needed, NUL included.
    let needed = unsafe { GetShortPathNameW(wide.as_ptr(), std::ptr::null_mut(), 0) };
    if needed == 0 {
        return None;
    }
    let mut buffer = vec![0u16; needed as usize];
    // SAFETY: `buffer` holds `needed` units, the length the call above asked for.
    let written = unsafe { GetShortPathNameW(wide.as_ptr(), buffer.as_mut_ptr(), needed) };
    if written == 0 || written >= needed {
        return None;
    }
    let short = String::from_utf16_lossy(&buffer[..written as usize]);
    Some(if let Some(unc) = short.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        short.strip_prefix(r"\\?\").unwrap_or(&short).to_owned()
    })
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
        let rest = text.get(2..).unwrap_or_default();
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

/// If an argument uses a Unix path spelling cash accepts, return the Windows spelling
/// it would have.
///
/// D4 forbids cash rewriting arguments, so this never *changes* anything — it exists so
/// the shell can explain a failure. `/c/src/main.tf` resolves fine for operations cash
/// performs itself (D3), but is passed verbatim to a command, and any tool without its
/// own MSYS-style translation has no idea what it means. That includes MS Coreutils and
/// the bundled builtins, so the failure is common and the error message —
/// "The system cannot find the path specified" — explains nothing.
///
/// Matches any drive letter, not just `C`.
#[must_use]
pub fn unix_drive_spelling(arg: &str) -> Option<PathBuf> {
    // `/tmp/...` is accepted by cash the same way `/c/...` is (D3/D7), and fails the
    // same way when handed to a command — including a bundled builtin, which opens paths
    // directly rather than through cash's file layer. Same cliff, same explanation.
    if arg == "/tmp" || arg.starts_with("/tmp/") {
        return Some(accept_path(arg));
    }

    let rest = arg.strip_prefix('/')?;
    let mut chars = rest.chars();

    let letter = chars.next()?;
    if !letter.is_ascii_alphabetic() {
        return None;
    }

    // The letter must be the whole first segment, and be *followed* by a path.
    //
    // A bare `/d` is excluded deliberately: it is far more often a command flag —
    // `cmd.exe /d /s /c` — than a reference to drive D, and warning about it produced a
    // false positive on one of the most common invocations there is. The diagnostic is
    // only worth having if it is quiet when it is wrong.
    if chars.next() != Some('/') {
        return None;
    }

    Some(accept_path(arg))
}
