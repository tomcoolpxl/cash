//! Win32 filesystem metadata and security inspection.
//!
//! Provides native resolution of Windows file owners (SIDs -> Account Names)
//! with in-memory caching, accurate hard link counts, and directory link counts.

use std::collections::HashMap;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle as _;
use std::path::Path;
use std::sync::Mutex;

use windows_sys::Win32::Security::{
    GetFileSecurityW, GetLengthSid, GetSecurityDescriptorOwner, LookupAccountSidW,
    OWNER_SECURITY_INFORMATION, SID_NAME_USE,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
};

static SID_CACHE: Mutex<Option<HashMap<Vec<u8>, String>>> = Mutex::new(None);

/// Query the account name of the file's owner via its security descriptor.
///
/// Returns the username if resolved, or None if the filesystem or permissions
/// do not support querying security info.
#[must_use]
pub fn get_file_owner(path: &Path) -> Option<String> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();

    let mut needed: u32 = 0;
    // SAFETY: querying required buffer size with a null pointer and length 0.
    unsafe {
        GetFileSecurityW(
            wide.as_ptr(),
            OWNER_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            0,
            &raw mut needed,
        );
    }

    if needed == 0 {
        return None;
    }

    let mut buffer = vec![0u8; needed as usize];
    // SAFETY: `buffer` is allocated to the required `needed` size.
    let ok = unsafe {
        GetFileSecurityW(
            wide.as_ptr(),
            OWNER_SECURITY_INFORMATION,
            buffer.as_mut_ptr().cast(),
            needed,
            &raw mut needed,
        )
    };

    if ok == 0 {
        return None;
    }

    let mut p_sid = std::ptr::null_mut();
    let mut defaulted = 0;
    // SAFETY: `buffer` contains a valid SECURITY_DESCRIPTOR retrieved by `GetFileSecurityW`.
    let ok = unsafe {
        GetSecurityDescriptorOwner(
            buffer.as_mut_ptr().cast(),
            &raw mut p_sid,
            &raw mut defaulted,
        )
    };

    if ok == 0 || p_sid.is_null() {
        return None;
    }

    // SAFETY: `p_sid` is a valid PSID returned by `GetSecurityDescriptorOwner`.
    let sid_len = unsafe { GetLengthSid(p_sid) } as usize;
    // SAFETY: `p_sid` points to a valid SID buffer of length `sid_len`.
    let sid_bytes = unsafe { std::slice::from_raw_parts(p_sid.cast::<u8>(), sid_len) };

    if let Some(cached) = cached_lookup(sid_bytes) {
        return Some(cached);
    }

    let mut name = [0u16; 256];
    let mut name_len = u32::try_from(name.len()).unwrap_or(u32::MAX);
    let mut domain = [0u16; 256];
    let mut domain_len = u32::try_from(domain.len()).unwrap_or(u32::MAX);
    let mut kind: SID_NAME_USE = 0;

    // SAFETY: `p_sid` is valid, and the buffers are sized by their respective lengths.
    let ok = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            p_sid,
            name.as_mut_ptr(),
            &raw mut name_len,
            domain.as_mut_ptr(),
            &raw mut domain_len,
            &raw mut kind,
        )
    };

    if ok == 0 {
        return None;
    }

    let resolved = String::from_utf16_lossy(&name[..name_len as usize]);
    cache_insert(sid_bytes.to_vec(), resolved.clone());
    Some(resolved)
}

fn cached_lookup(sid_bytes: &[u8]) -> Option<String> {
    let mut guard = SID_CACHE.lock().unwrap_or_else(|p| p.into_inner());
    guard.as_mut().and_then(|c| c.get(sid_bytes).cloned())
}

fn cache_insert(sid_bytes: Vec<u8>, name: String) {
    SID_CACHE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(sid_bytes, name);
}

/// Returns the username of the current running process.
#[must_use]
pub fn current_user() -> String {
    crate::process::current_process_user()
        .or_else(|| std::env::var("USERNAME").ok())
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_else(|| String::from("user"))
}

/// Count the number of subdirectories directly under `path`.
#[must_use]
pub fn count_subdirectories(path: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        if let Ok(ft) = entry.file_type() {
            if ft.is_dir() {
                count += 1;
            }
        }
    }
    count
}

/// Compute the POSIX-equivalent link count for a file or directory.
///
/// For directories, returns `2 + count of subdirectories`.
/// For regular files, returns the NTFS hard link count via `GetFileInformationByHandle`.
#[must_use]
pub fn file_link_count(path: &Path, metadata: &std::fs::Metadata) -> u32 {
    if metadata.is_dir() {
        2 + u32::try_from(count_subdirectories(path)).unwrap_or(0)
    } else {
        if let Ok(file) = std::fs::File::open(path) {
            let handle = file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
            // SAFETY: zeroed struct is valid for BY_HANDLE_FILE_INFORMATION.
            let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            // SAFETY: `handle` is a valid open file handle, `info` is a valid out-pointer.
            let ok = unsafe { GetFileInformationByHandle(handle, &raw mut info) };
            if ok != 0 {
                return info.nNumberOfLinks;
            }
        }
        1
    }
}
