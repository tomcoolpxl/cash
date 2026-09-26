//! Filesystem utilities for Windows.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::error;

// Selectively re-export items from stubs that we don't override.
pub(crate) use crate::sys::stubs::fs::MetadataExt;

/// Returns the current list of executable extensions from `PATHEXT`.
///
/// Each entry retains its leading dot (e.g. `".exe"`) and is stored
/// lowercased so case-insensitive comparisons can be done efficiently.
/// Changes to `PATHEXT` made inside the running shell or test environments
/// are dynamically reflected here.
pub fn pathext_extensions() -> Vec<String> {
    std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let lower = s.to_ascii_lowercase();
            if lower.starts_with('.') {
                lower
            } else {
                format!(".{lower}")
            }
        })
        .collect()
}

/// Returns the stem of a PATHEXT entry (with any leading `.` removed).
///
/// `PATHEXT` canonically stores entries like `.EXE`, but tolerant parsing
/// accepts entries without the leading dot too.
fn pathext_entry_stem(entry: &str) -> &str {
    entry.strip_prefix('.').unwrap_or(entry)
}

/// Returns true if the path's extension is in the PATHEXT list.
fn has_executable_extension_with(path: &Path, extensions: &[String]) -> bool {
    path.extension().is_some_and(|ext| {
        extensions
            .iter()
            .any(|e| ext.eq_ignore_ascii_case(pathext_entry_stem(e)))
    })
}

#[cfg(test)]
fn has_executable_extension(path: &Path) -> bool {
    let exts = pathext_extensions();
    has_executable_extension_with(path, &exts)
}

/// Returns true if `path` is, by itself, an existing executable file.
fn is_executable_file(path: &Path, extensions: &[String]) -> bool {
    has_executable_extension_with(path, extensions) && path.is_file()
}

/// Returns true if `path` is an existing file whose contents make it runnable: a `#!`
/// script or a PE image, whatever its name. Reads the file, so it runs only after the
/// `PATHEXT` checks (D46).
fn is_executable_by_content(path: &Path) -> bool {
    path.is_file() && cash_win32::resolve::has_executable_content(path)
}

/// Resolves an owned path to the actual on-disk executable file, if any.
///
/// Follows execution's order (D8, `cash_win32::resolve`) so a lookup reports what running
/// the name would run: a file with a `PATHEXT` extension is returned unchanged (no
/// allocation); a file with some other extension is returned if its contents are
/// runnable; then each `PATHEXT` extension is appended in turn; and last, the path itself
/// is returned if its contents are runnable. That last step is what finds Git for
/// Windows' extensionless `#!/bin/sh` wrappers such as `/usr/bin/egrep`.
pub fn resolve_executable(path: PathBuf) -> Option<PathBuf> {
    let extensions = pathext_extensions();
    if is_executable_file(&path, &extensions) {
        return Some(path);
    }
    if path.extension().is_some() && is_executable_by_content(&path) {
        return Some(path);
    }
    // Try appending each PATHEXT extension.
    for ext in &extensions {
        let mut name = path.as_os_str().to_owned();
        name.push(ext);
        let candidate = PathBuf::from(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    is_executable_by_content(&path).then_some(path)
}

impl crate::sys::fs::PathExt for Path {
    fn readable(&self) -> bool {
        self.exists()
    }

    fn writable(&self) -> bool {
        self.metadata().is_ok_and(|m| !m.permissions().readonly())
    }

    fn executable(&self) -> bool {
        let extensions = pathext_extensions();
        if is_executable_file(self, &extensions) {
            return true;
        }
        // Try each PATHEXT extension without allocating a separate PathBuf
        // per candidate until one exists.
        extensions.iter().any(|ext| {
            let mut name = self.as_os_str().to_owned();
            name.push(ext);
            Self::new(&name).is_file()
        }) || is_executable_by_content(self)
    }

    fn exists_and_is_block_device(&self) -> bool {
        false
    }

    fn exists_and_is_char_device(&self) -> bool {
        false
    }

    fn exists_and_is_fifo(&self) -> bool {
        false
    }

    fn exists_and_is_socket(&self) -> bool {
        false
    }

    fn exists_and_is_setgid(&self) -> bool {
        false
    }

    fn exists_and_is_setuid(&self) -> bool {
        false
    }

    fn exists_and_is_sticky_bit(&self) -> bool {
        false
    }

    fn get_device_and_inode(&self) -> Result<(u64, u64), crate::error::Error> {
        // TODO(windows): implement using file index / volume serial number.
        Err(error::ErrorKind::NotSupportedOnThisPlatform("get_device_and_inode").into())
    }
}

/// Splits a PATH-like value into individual paths.
///
/// cash (D5): `$PATH` is rendered Unix-style to scripts — colon-separated, `/c/...` —
/// so this cannot be [`std::env::split_paths`], which splits on `;` alone and would
/// hand back the whole value as one entry. It splits on either separator, rejoins a
/// drive letter that a colon-split would have torn in half (`C:/tools`), and converts
/// each entry to the native spelling the filesystem needs.
///
/// Without this, `PATH=/c/foo:$PATH` — which D5 gives as a worked example — left the
/// shell unable to find any command at all.
pub fn split_paths<T: AsRef<OsStr> + ?Sized>(s: &T) -> std::vec::IntoIter<PathBuf> {
    let value = s.as_ref().to_string_lossy().into_owned();
    cash_win32::env::split_path(&value)
        .map(cash_win32::path::accept_path)
        .collect::<Vec<_>>()
        .into_iter()
}

/// Splits a Bash search path, retaining empty entries as the current directory.
pub fn split_paths_preserving_empty<T: AsRef<OsStr> + ?Sized>(
    s: &T,
) -> std::vec::IntoIter<PathBuf> {
    let value = s.as_ref().to_string_lossy().into_owned();
    cash_win32::env::split_path_preserving_empty(&value)
        .map(|entry| {
            if entry.is_empty() {
                PathBuf::from(".")
            } else {
                cash_win32::path::accept_path(entry)
            }
        })
        .collect::<Vec<_>>()
        .into_iter()
}

/// Opens a null file that will discard all I/O.
pub fn open_null_file() -> Result<std::fs::File, error::Error> {
    let f = std::fs::File::options()
        .read(true)
        .write(true)
        .open("NUL")?;
    Ok(f)
}

/// Gives the platform an opportunity to handle a special file path (e.g. `/dev/null`).
pub fn try_open_special_file(path: &Path) -> Option<Result<std::fs::File, std::io::Error>> {
    if path.ends_with("dev/null") && path.is_absolute() {
        Some(open_null_file().map_err(std::io::Error::other))
    } else {
        None
    }
}

/// Returns the default paths where executables are typically found on Windows.
pub(crate) fn get_default_executable_search_paths() -> Vec<PathBuf> {
    default_system_paths()
}

/// Returns the default paths where standard system utilities are found on Windows.
pub fn get_default_standard_utils_paths() -> Vec<PathBuf> {
    default_system_paths()
}

fn default_system_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(sysroot) = std::env::var("SystemRoot") {
        paths.push(PathBuf::from(&sysroot).join("system32"));
        paths.push(PathBuf::from(&sysroot));
        paths.push(PathBuf::from(&sysroot).join("System32").join("Wbem"));
        paths.push(
            PathBuf::from(&sysroot)
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0"),
        );
    }
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        paths.push(
            PathBuf::from(userprofile)
                .join("AppData")
                .join("Local")
                .join("Microsoft")
                .join("WindowsApps"),
        );
    }
    paths
}

/// Returns the path to the system-wide shell profile script.
///
/// On Windows, points to `%ProgramData%\cash\profile`. If the file exists, it is sourced
/// for login shells; if not, it is ignored without error.
pub fn get_system_profile_path() -> Option<&'static Path> {
    Some(Path::new(r"C:\ProgramData\cash\profile"))
}

/// Returns the path to the system-wide shell rc script.
///
/// On Windows, points to `%ProgramData%\cash\cashrc`. If the file exists, it is sourced
/// for interactive non-login shells ahead of `~/.bashrc`; if not, it is ignored without error.
pub fn get_system_rc_path() -> Option<&'static Path> {
    Some(Path::new(r"C:\ProgramData\cash\cashrc"))
}

/// Returns the platform default for case-insensitive pathname expansion.
///
/// On Windows, filesystems are typically case-insensitive, so this returns `true`.
pub const fn default_case_insensitive_path_expansion() -> bool {
    true
}

/// Path separator characters on Windows.
const PATH_SEPARATORS: [char; 2] = ['/', '\\'];

/// Returns true if the string contains a path separator character.
///
/// On Windows, both `/` and `\` are considered path separators.
pub fn contains_path_separator(s: &str) -> bool {
    s.contains(PATH_SEPARATORS)
}

/// Returns true if the string ends with a path separator character.
///
/// On Windows, both `/` and `\` are considered path separators.
pub fn ends_with_path_separator(s: &str) -> bool {
    s.ends_with(PATH_SEPARATORS)
}

/// Returns the string with a trailing path separator removed, if present.
///
/// On Windows, both `/` and `\` are considered path separators.
pub fn strip_path_separator_suffix(s: &str) -> &str {
    s.strip_suffix(PATH_SEPARATORS).unwrap_or(s)
}

/// Finds the byte index of the last path separator in the string.
///
/// On Windows, both `/` and `\` are considered path separators.
pub fn rfind_path_separator(s: &str) -> Option<usize> {
    s.rfind(PATH_SEPARATORS)
}

/// Splits a string on path separator characters, returning an iterator of components.
///
/// On Windows, both `/` and `\` are used as separators.
pub fn split_path_for_pattern(s: &str) -> impl Iterator<Item = &str> {
    s.split(PATH_SEPARATORS)
}

/// Returns the root path for an absolute pattern, if the first component indicates one.
///
/// On Windows, recognizes both a leading separator (empty first component from splitting
/// a path like `/foo`) and a drive-letter prefix like `C:` as absolute.
///
/// TODO(windows): UNC paths like `\\server\share\foo` are not yet handled
/// specially; they split into `["", "", "server", "share", "foo"]`, and the
/// leading empty component causes them to be treated as if they were rooted
/// at `/`, which drops the server/share portion. Supporting UNC requires
/// peeking further into the component list.
pub fn pattern_path_root(first_component: &str) -> Option<PathBuf> {
    if first_component.is_empty() {
        // Leading separator, e.g. `/foo` split into ["", "foo"].
        Some(PathBuf::from("/"))
    } else if first_component.len() == 2
        && first_component.as_bytes()[0].is_ascii_alphabetic()
        && first_component.as_bytes()[1] == b':'
    {
        // Drive letter prefix, e.g. `c:/foo` split into ["c:", "foo"].
        let mut root = String::with_capacity(3);
        root.push_str(first_component);
        root.push('/');
        Some(PathBuf::from(root))
    } else {
        None
    }
}

/// Pushes a component onto a path for pattern expansion.
///
/// On Windows, `PathBuf::push` has special drive-letter and root-replacement
/// semantics that conflict with shell path construction (e.g. pushing `C:foo`
/// onto `D:\bar` replaces the whole path). This function always appends the
/// component as a child, operating on the underlying `OsString` so non-UTF-8
/// content in the path is preserved and no reallocation is needed.
pub fn push_path_for_pattern(path: &mut PathBuf, component: &str) {
    // Separator characters are ASCII, and WTF-8-encoded OsStr bytes are a
    // superset of UTF-8, so checking the last byte directly is safe.
    let bytes = path.as_os_str().as_encoded_bytes();
    let needs_sep = !bytes.is_empty() && !matches!(bytes.last(), Some(b'/' | b'\\'));

    let buf = path.as_mut_os_string();
    if needs_sep {
        buf.push("/");
    }
    buf.push(component);
}

/// Normalizes path separators for shell output.
///
/// On Windows, replaces `\` with `/` since backslash is the shell escape character.
pub fn normalize_path_separators(s: &str) -> std::borrow::Cow<'_, str> {
    if s.contains('\\') {
        std::borrow::Cow::Owned(s.replace('\\', "/"))
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

/// A unique temp path for a process substitution (D17).
///
/// Lives in a per-session directory so that a sweep at startup can clear anything left
/// by a session that was terminated without unwinding — which D6 does routinely.
/// Back a here-document or here-string with a temp file rather than a pipe.
///
/// A pipe deadlocks above its buffer. The shell writes the whole document before the
/// command that reads it has started, so once the write fills the pipe — 4096 bytes on
/// Windows, exactly — it blocks forever waiting for a reader that cannot exist yet.
/// Measured: a here-string of 4090 bytes worked and one of 4096 hung the shell. `$PATH`
/// on this machine is 4425 bytes, so D5's own worked example,
/// `IFS=: read -ra dirs <<< "$PATH"`, was a hang.
///
/// Linux avoids it by growing the pipe with `F_SETPIPE_SZ`. Windows has no equivalent,
/// so this does what bash does for here-documents anyway: writes a temp file.
///
/// `FILE_FLAG_DELETE_ON_CLOSE` is right here even though D17 had to reject it for
/// process substitution. The difference is who opens the file: a process substitution
/// hands the *child* a path, and a delete-pending file cannot be opened afresh, whereas
/// a here-document hands over the inherited *handle* and no path is ever used. The
/// kernel reclaims the file when the last handle closes, including when cash is
/// terminated without unwinding — which D6 does routinely.
pub(crate) fn open_temp_with_contents(contents: &[u8]) -> std::io::Result<std::fs::File> {
    use std::io::{Seek, SeekFrom, Write};
    use std::os::windows::fs::OpenOptionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Delete the file as soon as the last handle to it closes.
    const FILE_FLAG_DELETE_ON_CLOSE: u32 = 0x0400_0000;
    /// Let the child inherit and read it, and let the delete proceed while open.
    const SHARE_ALL: u32 = 0x0000_0001 | 0x0000_0002 | 0x0000_0004;
    /// cash: the part that keeps this off the disk. It tells the cache manager the file
    /// is transient, so the data is held in the system cache and written out only under
    /// memory pressure — and with the delete above, a here-document written and read in
    /// the same breath never reaches the platter at all. It is also why a RAM disk would
    /// buy nothing here: Windows already has one, and this is how a program asks for it.
    const FILE_ATTRIBUTE_TEMPORARY: u32 = 0x0000_0100;

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(std::format!("cash-here-{}-{n}", std::process::id()));

    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .share_mode(SHARE_ALL)
        .attributes(FILE_ATTRIBUTE_TEMPORARY)
        .custom_flags(FILE_FLAG_DELETE_ON_CLOSE)
        .open(&path)?;

    file.write_all(contents)?;
    file.seek(SeekFrom::Start(0))?;
    Ok(file)
}

/// Creates a file whose contents Windows should keep in memory.
///
/// cash: the same `FILE_ATTRIBUTE_TEMPORARY` hint as a here-document's, for the file a
/// process substitution writes. This one cannot also be delete-on-close — the child opens
/// it by path, and a delete-pending file cannot be opened afresh (D17) — so it is swept
/// instead, but the contents still need not travel to the disk and back.
pub(crate) fn create_temporary_file(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;

    /// Transient: hold it in the cache, write it out only under memory pressure.
    const FILE_ATTRIBUTE_TEMPORARY: u32 = 0x0000_0100;

    std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .attributes(FILE_ATTRIBUTE_TEMPORARY)
        .open(path)
}

pub(crate) fn process_substitution_temp_path() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    if n == 0 {
        sweep_stale_process_substitution_dirs();
    }

    std::env::temp_dir()
        .join(std::format!("cash-psub-{}", std::process::id()))
        .join(std::format!("{n}"))
}

/// Remove process-substitution temp directories left by sessions that are gone.
///
/// D17 preferred `FILE_FLAG_DELETE_ON_CLOSE`, whose appeal is that the kernel reclaims
/// the file even when a process is terminated without unwinding — which D6 does
/// routinely. That does not work here: the child opens the file by *path*, and a
/// delete-pending file cannot be opened afresh. So this is D17's named fallback, a
/// per-session directory swept at startup, keyed on whether the owning pid still exists.
fn sweep_stale_process_substitution_dirs() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(pid) = name.strip_prefix("cash-psub-") else {
            continue;
        };
        let Ok(pid) = pid.parse::<u32>() else {
            continue;
        };

        // Leave our own directory, and any belonging to a session still running.
        if pid == std::process::id() || cash_win32::process::is_pid_alive(pid) {
            continue;
        }

        let _ = std::fs::remove_dir_all(entry.path());
    }
}

// The unit tests live at the end of the file so that adding a function above them
// does not trip `clippy::items_after_test_module`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_separator_helpers_both_slashes() {
        assert!(contains_path_separator("foo/bar"));
        assert!(contains_path_separator(r"foo\bar"));
        assert!(contains_path_separator(r"mixed/and\back"));
        assert!(!contains_path_separator("foobar"));

        assert!(ends_with_path_separator("foo/"));
        assert!(ends_with_path_separator(r"foo\"));
        assert!(!ends_with_path_separator("foo"));

        assert_eq!(strip_path_separator_suffix("foo/"), "foo");
        assert_eq!(strip_path_separator_suffix(r"foo\"), "foo");
        assert_eq!(strip_path_separator_suffix("foo"), "foo");

        assert_eq!(rfind_path_separator("a/b/c"), Some(3));
        assert_eq!(rfind_path_separator(r"a\b\c"), Some(3));
        assert_eq!(rfind_path_separator(r"a/b\c"), Some(3));
        assert_eq!(rfind_path_separator("abc"), None);
    }

    #[test]
    fn split_path_for_pattern_both_slashes() {
        let parts: Vec<_> = split_path_for_pattern("a/b/c").collect();
        assert_eq!(parts, vec!["a", "b", "c"]);

        let parts: Vec<_> = split_path_for_pattern(r"a\b\c").collect();
        assert_eq!(parts, vec!["a", "b", "c"]);

        let parts: Vec<_> = split_path_for_pattern(r"a/b\c").collect();
        assert_eq!(parts, vec!["a", "b", "c"]);

        let parts: Vec<_> = split_path_for_pattern("/a/b").collect();
        assert_eq!(parts, vec!["", "a", "b"]);
    }

    #[test]
    fn pattern_path_root_leading_separator() {
        assert_eq!(pattern_path_root(""), Some(PathBuf::from("/")));
    }

    #[test]
    fn pattern_path_root_drive_letters() {
        assert_eq!(pattern_path_root("c:"), Some(PathBuf::from("c:/")));
        assert_eq!(pattern_path_root("C:"), Some(PathBuf::from("C:/")));
        assert_eq!(pattern_path_root("Z:"), Some(PathBuf::from("Z:/")));
    }

    #[test]
    fn pattern_path_root_rejects_non_drive_two_char_prefix() {
        // "1:" is not a valid drive letter — must be alphabetic.
        assert_eq!(pattern_path_root("1:"), None);
        // Longer drive-like strings are not treated as roots.
        assert_eq!(pattern_path_root("cd"), None);
        assert_eq!(pattern_path_root("c:\\"), None);
        assert_eq!(pattern_path_root("foo"), None);
    }

    #[test]
    fn push_path_for_pattern_appends_with_forward_slash() {
        let mut p = PathBuf::from(r"C:\Users\reuben");
        push_path_for_pattern(&mut p, "foo");
        // Forward slash is used as the appended separator, yielding mixed
        // separators — acceptable because `normalize_path_separators` is
        // applied downstream before display.
        assert_eq!(p, PathBuf::from(r"C:\Users\reuben/foo"));
    }

    #[test]
    fn push_path_for_pattern_no_double_separator() {
        let mut p = PathBuf::from("C:/Users/reuben/");
        push_path_for_pattern(&mut p, "foo");
        assert_eq!(p, PathBuf::from("C:/Users/reuben/foo"));

        let mut p = PathBuf::from(r"C:\Users\reuben\");
        push_path_for_pattern(&mut p, "foo");
        assert_eq!(p, PathBuf::from(r"C:\Users\reuben\foo"));
    }

    #[test]
    fn push_path_for_pattern_onto_drive_root() {
        let mut p = PathBuf::from("c:/");
        push_path_for_pattern(&mut p, "foo");
        assert_eq!(p, PathBuf::from("c:/foo"));
    }

    #[test]
    fn push_path_for_pattern_onto_empty() {
        let mut p = PathBuf::new();
        push_path_for_pattern(&mut p, "foo");
        // Empty path stays un-prefixed — we only add a separator between
        // existing content and the new component.
        assert_eq!(p, PathBuf::from("foo"));
    }

    #[test]
    fn normalize_path_separators_converts_backslashes() {
        use std::borrow::Cow;
        // Already-forward-slashed input is borrowed (no allocation).
        assert!(matches!(
            normalize_path_separators("c:/foo/bar"),
            Cow::Borrowed("c:/foo/bar")
        ));
        // Mixed or backslashed input becomes owned and fully forward-slashed.
        let normalized = normalize_path_separators(r"c:\foo\bar");
        assert_eq!(normalized.as_ref(), "c:/foo/bar");
        let normalized = normalize_path_separators(r"c:\foo/bar");
        assert_eq!(normalized.as_ref(), "c:/foo/bar");
    }

    #[test]
    fn default_case_insensitive_is_true() {
        assert!(default_case_insensitive_path_expansion());
    }

    #[test]
    fn has_executable_extension_is_case_insensitive() {
        // Force the PATHEXT cache for this test's defaults.
        assert!(has_executable_extension(Path::new("foo.exe")));
        assert!(has_executable_extension(Path::new("foo.EXE")));
        assert!(has_executable_extension(Path::new("foo.Cmd")));
        assert!(!has_executable_extension(Path::new("foo.txt")));
        assert!(!has_executable_extension(Path::new("foo")));
    }

    #[test]
    fn pathext_entry_stem_strips_dot() {
        assert_eq!(pathext_entry_stem(".exe"), "exe");
        assert_eq!(pathext_entry_stem(".cmd"), "cmd");
        // Tolerant: entries without a leading dot are returned as-is.
        assert_eq!(pathext_entry_stem("exe"), "exe");
        assert_eq!(pathext_entry_stem(""), "");
    }

    #[test]
    fn resolve_executable_for_nonexistent_returns_none() {
        // A path that cannot exist on any test host.
        let path = PathBuf::from(r"C:\__brush_test_definitely_missing__");
        assert!(resolve_executable(path).is_none());
    }

    #[test]
    fn test_dynamic_pathext() {
        let exts = pathext_extensions();
        assert!(exts.contains(&".exe".to_string()));
        assert!(exts.contains(&".bat".to_string()) || exts.contains(&".cmd".to_string()));

        // Test with custom extensions slice
        let custom = vec![".custom".to_string(), ".xyz".to_string()];
        assert!(has_executable_extension_with(Path::new("run.xyz"), &custom));
        assert!(has_executable_extension_with(
            Path::new("run.CUSTOM"),
            &custom
        ));
        assert!(!has_executable_extension_with(
            Path::new("run.exe"),
            &custom
        ));
    }
}
