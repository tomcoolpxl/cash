//! The user's own `Path`, in `HKEY_CURRENT_USER\Environment`.
//!
//! Windows appends it to the machine's for every program the user starts, and `cash
//! --link-tools --add-to-path` puts its links folder at the front of it (ROADMAP item 18).
//! Written the way PowerShell's `[Environment]::SetEnvironmentVariable` does not: .NET
//! reads the value expanded and writes it back as a plain `REG_SZ`, baking every
//! `%USERPROFILE%` into a fixed path (dotnet/runtime#1442). Here the value keeps its type,
//! `REG_EXPAND_SZ` on a stock Windows, and every entry but cash's is left as it was.

use std::io;
use std::path::Path;

use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS, LPARAM, WIN32_ERROR,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_VALUE_TYPE,
    RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW,
    RegDeleteValueW, RegGetValueW, RegSetValueExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};

/// A key under `HKEY_CURRENT_USER` to use in place of `Environment`. Tests set it, so no
/// test run touches the user's real `Path`; nothing is announced to other programs then.
pub const KEY_VAR: &str = "CASH_USER_ENVIRONMENT_KEY";

const ENVIRONMENT: &str = "Environment";
const VALUE: &str = "Path";

/// Whether [`add_first`] changed the user `Path`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Added {
    /// The folder is now the first entry.
    Added,
    /// An entry already named the folder, and was left where it is.
    AlreadyThere,
}

/// Put `dir` first on the user `Path`, unless an entry already names it: a user who moved
/// it has chosen its place.
pub fn add_first(dir: &Path) -> io::Result<Added> {
    let entry = crate::path::to_backslash(dir);
    let (value, kind) = read()?;
    match with_entry_first(value.as_deref().unwrap_or_default(), &entry) {
        Some(changed) => {
            write(&changed, kind)?;
            announce();
            Ok(Added::Added)
        }
        None => Ok(Added::AlreadyThere),
    }
}

/// Take every entry naming `dir` off the user `Path`. Whether there was one.
pub fn remove(dir: &Path) -> io::Result<bool> {
    let entry = crate::path::to_backslash(dir);
    let (Some(value), kind) = read()? else {
        return Ok(false);
    };
    match without_entry(&value, &entry) {
        Some(changed) => {
            write(&changed, kind)?;
            announce();
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Whether an entry of the user `Path` names `dir`.
pub fn contains(dir: &Path) -> bool {
    let entry = crate::path::to_backslash(dir);
    read().is_ok_and(|(value, _)| {
        value.is_some_and(|value| value.split(';').any(|each| same_entry(each, &entry)))
    })
}

/// `value` with `entry` first, or `None` when an entry already names the same folder.
fn with_entry_first(value: &str, entry: &str) -> Option<String> {
    if value.split(';').any(|each| same_entry(each, entry)) {
        return None;
    }
    Some(if value.trim().is_empty() {
        entry.to_owned()
    } else {
        format!("{entry};{value}")
    })
}

/// `value` without the entries naming `entry`'s folder, or `None` when there are none.
/// Everything else, empty entries included, is kept as it was.
fn without_entry(value: &str, entry: &str) -> Option<String> {
    let kept: Vec<&str> = value
        .split(';')
        .filter(|each| !same_entry(each, entry))
        .collect();
    (kept.len() != value.split(';').count()).then(|| kept.join(";"))
}

/// Whether two `Path` entries name one folder: `%VAR%` expanded, quotes, case, slash
/// direction and a trailing separator aside.
fn same_entry(one: &str, other: &str) -> bool {
    let one = folder(one);
    !one.is_empty() && one == folder(other)
}

fn folder(entry: &str) -> String {
    expand(entry.trim().trim_matches('"'))
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// `%NAME%` replaced by the variable's value where it is set, as Windows expands a
/// `REG_EXPAND_SZ`; an unset name is left as it is.
fn expand(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((before, after)) = rest.split_once('%') {
        out.push_str(before);
        match after.split_once('%') {
            Some((name, tail)) if !name.is_empty() => {
                if let Ok(value) = std::env::var(name) {
                    out.push_str(&value);
                } else {
                    out.push('%');
                    out.push_str(name);
                    out.push('%');
                }
                rest = tail;
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn key_name() -> String {
    std::env::var(KEY_VAR).unwrap_or_else(|_| ENVIRONMENT.to_owned())
}

fn os_error(status: WIN32_ERROR) -> io::Error {
    io::Error::from_raw_os_error(i32::try_from(status).unwrap_or(i32::MAX))
}

/// The user `Path` as stored, `%VAR%` entries unexpanded, and its type. A user with no
/// `Path` of their own has `None`, and a new one is `REG_EXPAND_SZ`, as Windows makes it.
fn read() -> io::Result<(Option<String>, REG_VALUE_TYPE)> {
    let key = wide(&key_name());
    let name = wide(VALUE);
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
    let mut buffer: Vec<u16> = vec![0; 1024];
    loop {
        let mut kind: REG_VALUE_TYPE = 0;
        let mut size = u32::try_from(buffer.len() * 2).unwrap_or(u32::MAX);
        // SAFETY: both names are NUL-terminated, and the buffer is described by `size` in
        // bytes, as this call expects.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                flags,
                &raw mut kind,
                buffer.as_mut_ptr().cast(),
                &raw mut size,
            )
        };
        match status {
            ERROR_SUCCESS => {
                // `size` is in bytes and counts the terminator.
                let chars = (size as usize / 2).min(buffer.len());
                let text = buffer.get(..chars).unwrap_or_default();
                let text = text.strip_suffix(&[0]).unwrap_or(text);
                return Ok((Some(String::from_utf16_lossy(text)), kind));
            }
            ERROR_FILE_NOT_FOUND => return Ok((None, REG_EXPAND_SZ)),
            ERROR_MORE_DATA => buffer.resize(size as usize / 2 + 1, 0),
            other => return Err(os_error(other)),
        }
    }
}

/// An open registry key, closed when dropped.
struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the handle came from `RegCreateKeyExW` and is closed only here.
        unsafe { RegCloseKey(self.0) };
    }
}

/// Store `value` as the user `Path`, with the type it had; an empty one is removed, as a
/// user who never had a `Path` of their own had none.
fn write(value: &str, kind: REG_VALUE_TYPE) -> io::Result<()> {
    let subkey = wide(&key_name());
    let mut handle: HKEY = std::ptr::null_mut();
    // SAFETY: the subkey is NUL-terminated; the class and security attributes may be
    // null; `handle` receives the key, which `Key` closes.
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &raw mut handle,
            std::ptr::null_mut(),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(os_error(status));
    }
    let key = Key(handle);
    let name = wide(VALUE);

    let status = if value.is_empty() {
        // SAFETY: the key is open for setting values, and the name is NUL-terminated.
        unsafe { RegDeleteValueW(key.0, name.as_ptr()) }
    } else {
        let data = wide(value);
        let bytes = u32::try_from(data.len() * 2).map_err(|_| io::Error::other("Path too long"))?;
        // SAFETY: the key is open for setting values; `data` is NUL-terminated UTF-16 of
        // `bytes` bytes, as a string value is stored.
        unsafe { RegSetValueExW(key.0, name.as_ptr(), 0, kind, data.as_ptr().cast(), bytes) }
    };
    if status == ERROR_SUCCESS || (value.is_empty() && status == ERROR_FILE_NOT_FOUND) {
        Ok(())
    } else {
        Err(os_error(status))
    }
}

/// Tell running programs the environment changed, as the Environment Variables dialog
/// does: Explorer rereads it, so what it starts from now on sees the new `Path`.
fn announce() {
    if std::env::var_os(KEY_VAR).is_some() {
        return;
    }
    let what = wide(ENVIRONMENT);
    let mut result = 0usize;
    // SAFETY: `what` is a NUL-terminated string that outlives the call, which returns
    // within five seconds even when a window does not answer.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            what.as_ptr() as LPARAM,
            SMTO_ABORTIFHUNG,
            5000,
            &raw mut result,
        );
    }
}

/// A NUL-terminated UTF-16 copy, as every `W` entry point wants.
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_entry_goes_first_and_the_rest_is_kept_as_written() {
        assert_eq!(
            with_entry_first(r"%USERPROFILE%\go\bin;C:\x;;C:\y", r"C:\links"),
            Some(r"C:\links;%USERPROFILE%\go\bin;C:\x;;C:\y".to_owned())
        );
        assert_eq!(
            with_entry_first("", r"C:\links"),
            Some(r"C:\links".to_owned())
        );
    }

    #[test]
    fn a_folder_already_named_anywhere_is_left_where_it_is() {
        let value = r"C:\x;c:/LINKS/;C:\y";
        assert_eq!(with_entry_first(value, r"C:\links"), None);
        assert_eq!(with_entry_first(r#""C:\links""#, r"C:\links"), None);
    }

    #[test]
    fn an_entry_spelled_with_a_variable_names_the_same_folder() {
        let Ok(profile) = std::env::var("USERPROFILE") else {
            return;
        };
        let value = r"%USERPROFILE%\bin;C:\x";
        assert_eq!(with_entry_first(value, &format!(r"{profile}\bin")), None);
        assert_eq!(
            without_entry(value, &format!(r"{profile}\BIN\")),
            Some(r"C:\x".to_owned())
        );
    }

    #[test]
    fn removing_takes_every_copy_and_nothing_else() {
        assert_eq!(
            without_entry(r"C:\links;C:\x;;C:\LINKS\;%Z%\y", r"C:\links"),
            Some(r"C:\x;;%Z%\y".to_owned())
        );
        assert_eq!(without_entry(r"C:\x;C:\y", r"C:\links"), None);
        assert_eq!(without_entry(r"C:\links", r"C:\links"), Some(String::new()));
    }

    #[test]
    fn unset_variables_and_stray_percent_signs_stay_as_written() {
        assert_eq!(
            expand("%CASH_NO_SUCH_VARIABLE%\\x"),
            "%CASH_NO_SUCH_VARIABLE%\\x"
        );
        assert_eq!(expand("100%"), "100%");
        assert_eq!(expand("a%%b"), "a%%b");
        assert_eq!(expand("no percent"), "no percent");
    }
}
