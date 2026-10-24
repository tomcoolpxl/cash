//! Shell verbs and file associations of the user's own, in `HKEY_CURRENT_USER`.
//!
//! Under `Software\Classes`: the entries Explorer adds to a menu, such as "Open cash
//! here" on a directory (D85), and what opens a `.sh` file.
//!
//! A verb is `<class>\shell\<verb>`: its default value the menu's text, `Icon` the icon,
//! and its `command` subkey the command line, where Explorer puts the item's path for
//! `%V`. Under `HKEY_CURRENT_USER` it needs no administrator and reaches only this user.

use std::io;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE,
    REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW,
    RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW, RegSetValueExW,
};
use windows_sys::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};

use crate::wide::to_wide_nul;

/// A key under `HKEY_CURRENT_USER` to use in place of `Software\Classes`. Tests set it, so
/// no test run touches the user's real Explorer menus.
pub const KEY_VAR: &str = "CASH_CLASSES_KEY";

const CLASSES: &str = r"Software\Classes";

/// What a verb shows and runs.
pub struct Verb<'a> {
    /// The menu's text.
    pub label: &'a str,
    /// The icon: a file, optionally `,INDEX`.
    pub icon: &'a str,
    /// The command line.
    pub command: &'a str,
}

/// Write `verb` as `name` on `class`, replacing what was there.
///
/// # Errors
///
/// A key or value that could not be written.
pub fn set(class: &str, name: &str, verb: &Verb<'_>) -> io::Result<()> {
    let path = verb_key(class, name);
    let key = create(&path)?;
    set_string(&key, None, verb.label)?;
    set_string(&key, Some("Icon"), verb.icon)?;
    let command = create(&format!(r"{path}\command"))?;
    set_string(&command, None, verb.command)
}

/// Remove the verb `name` from `class`, with everything under it; whether there was one.
///
/// # Errors
///
/// A key that is there but could not be removed.
pub fn remove(class: &str, name: &str) -> io::Result<bool> {
    let path = to_wide_nul(verb_key(class, name));
    // SAFETY: the subkey is NUL-terminated and names a key under the user's hive.
    match unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, path.as_ptr()) } {
        ERROR_SUCCESS => Ok(true),
        ERROR_FILE_NOT_FOUND => Ok(false),
        other => Err(os_error(other)),
    }
}

/// The command line of the verb `name` on `class`, when there is one.
pub fn command(class: &str, name: &str) -> Option<String> {
    let path = to_wide_nul(format!(r"{}\command", verb_key(class, name)));
    let mut buffer = vec![0u16; 2048];
    let mut size = u32::try_from(buffer.len() * 2).unwrap_or(u32::MAX);
    // SAFETY: the subkey is NUL-terminated, a null value name is the default value, and
    // `size` is the buffer's length in bytes.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            std::ptr::null(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &raw mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let chars = (size as usize / 2).min(buffer.len());
    let text = buffer.get(..chars).unwrap_or_default();
    let text = text.strip_suffix(&[0]).unwrap_or(text);
    Some(String::from_utf16_lossy(text))
}

fn verb_key(class: &str, name: &str) -> String {
    format!(r"{}\{class}\shell\{name}", root())
}

/// `Software\Classes`, or the tests' key in its place ([`KEY_VAR`]).
fn root() -> String {
    std::env::var(KEY_VAR).unwrap_or_else(|_| CLASSES.to_owned())
}

/// Whether the keys are the tests' own ([`KEY_VAR`]), not the user's.
fn redirected() -> bool {
    std::env::var_os(KEY_VAR).is_some()
}

/// Sets the value `name` (the key's default for `None`) of the key at `path` under the
/// classes, creating the key.
///
/// # Errors
///
/// A key or value that could not be written.
pub fn set_value(path: &str, name: Option<&str>, value: &str) -> io::Result<()> {
    let key = create(&format!(r"{}\{path}", root()))?;
    set_string(&key, name, value)
}

/// The string value `name` (the default for `None`) of the key at `path` under the
/// user's classes.
pub fn value(path: &str, name: Option<&str>) -> Option<String> {
    read_string(HKEY_CURRENT_USER, &format!(r"{}\{path}", root()), name)
}

/// The default value of the key at `path` in Windows' merged classes.
///
/// `HKEY_CLASSES_ROOT` holds the user's and the machine's: what a program installed for
/// every user, such as Git for Windows, set counts. The tests' key alone when redirected.
pub fn merged_default(path: &str) -> Option<String> {
    if redirected() {
        return value(path, None);
    }
    read_string(HKEY_CLASSES_ROOT, path, None)
}

/// The program class the user chose for `extension` in Windows' "Open with" or Settings,
/// which wins over every class's default; none for the tests' key.
pub fn user_choice(extension: &str) -> Option<String> {
    if redirected() {
        return None;
    }
    let path = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{extension}\UserChoice"
    );
    read_string(HKEY_CURRENT_USER, &path, Some("ProgId")).filter(|class| !class.is_empty())
}

/// Removes the value `name` (the default for `None`) of the key at `path` under the
/// classes; whether there was one.
///
/// # Errors
///
/// A value that is there but could not be removed.
pub fn remove_value(path: &str, name: Option<&str>) -> io::Result<bool> {
    let subkey = to_wide_nul(format!(r"{}\{path}", root()));
    let name = name.map(to_wide_nul);
    let name_ptr = name.as_ref().map_or(std::ptr::null(), |name| name.as_ptr());
    // SAFETY: the subkey is NUL-terminated; the name is null (the default) or
    // NUL-terminated.
    match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, subkey.as_ptr(), name_ptr) } {
        ERROR_SUCCESS => Ok(true),
        ERROR_FILE_NOT_FOUND => Ok(false),
        other => Err(os_error(other)),
    }
}

/// Removes the key at `path` under the classes, with everything under it; whether there
/// was one.
///
/// # Errors
///
/// A key that is there but could not be removed.
pub fn remove_key(path: &str) -> io::Result<bool> {
    let subkey = to_wide_nul(format!(r"{}\{path}", root()));
    // SAFETY: the subkey is NUL-terminated and names a key under the user's hive.
    match unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, subkey.as_ptr()) } {
        ERROR_SUCCESS => Ok(true),
        ERROR_FILE_NOT_FOUND => Ok(false),
        other => Err(os_error(other)),
    }
}

/// Tells Explorer that file associations changed, so it shows the new ones at once; not
/// for the tests' key.
pub fn announce_associations() {
    if redirected() {
        return;
    }
    // SAFETY: SHCNE_ASSOCCHANGED takes no items; both pointers may be null.
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED.cast_signed(),
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}

fn read_string(hive: HKEY, path: &str, name: Option<&str>) -> Option<String> {
    let subkey = to_wide_nul(path);
    let name = name.map(to_wide_nul);
    let name_ptr = name.as_ref().map_or(std::ptr::null(), |name| name.as_ptr());
    let mut buffer = vec![0u16; 2048];
    let mut size = u32::try_from(buffer.len() * 2).unwrap_or(u32::MAX);
    // SAFETY: the subkey is NUL-terminated, the name null or NUL-terminated, and `size`
    // is the buffer's length in bytes.
    let status = unsafe {
        RegGetValueW(
            hive,
            subkey.as_ptr(),
            name_ptr,
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &raw mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let chars = (size as usize / 2).min(buffer.len());
    let text = buffer.get(..chars).unwrap_or_default();
    let text = text.strip_suffix(&[0]).unwrap_or(text);
    Some(String::from_utf16_lossy(text))
}

/// An open registry key, closed when dropped.
struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the handle came from `RegCreateKeyExW` and is closed only here.
        unsafe { RegCloseKey(self.0) };
    }
}

fn create(path: &str) -> io::Result<Key> {
    let subkey = to_wide_nul(path);
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
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            std::ptr::null(),
            &raw mut handle,
            std::ptr::null_mut(),
        )
    };
    if status == ERROR_SUCCESS {
        Ok(Key(handle))
    } else {
        Err(os_error(status))
    }
}

fn set_string(key: &Key, name: Option<&str>, value: &str) -> io::Result<()> {
    let name = name.map(to_wide_nul);
    let name_ptr = name.as_ref().map_or(std::ptr::null(), |name| name.as_ptr());
    let data = to_wide_nul(value);
    let bytes = u32::try_from(data.len() * 2).map_err(|_| io::Error::other("value too long"))?;
    // SAFETY: the key is open for setting values; the name is null or NUL-terminated;
    // `data` is NUL-terminated UTF-16 of `bytes` bytes, as a string value is stored.
    let status = unsafe { RegSetValueExW(key.0, name_ptr, 0, REG_SZ, data.as_ptr().cast(), bytes) };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(os_error(status))
    }
}

fn os_error(status: WIN32_ERROR) -> io::Error {
    io::Error::from_raw_os_error(i32::try_from(status).unwrap_or(i32::MAX))
}
