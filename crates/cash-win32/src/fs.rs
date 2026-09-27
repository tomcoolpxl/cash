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
    AccessCheck, DACL_SECURITY_INFORMATION, DuplicateToken, GENERIC_MAPPING,
    GROUP_SECURITY_INFORMATION, GetFileSecurityW, GetLengthSid, GetSecurityDescriptorOwner,
    GetSidSubAuthority, GetSidSubAuthorityCount, IsValidSid, LookupAccountSidW,
    OBJECT_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PRIVILEGE_SET, PSID, SID_NAME_USE,
    SecurityImpersonation, TOKEN_DUPLICATE, TOKEN_QUERY,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
};

static SID_CACHE: Mutex<Option<HashMap<Vec<u8>, String>>> = Mutex::new(None);

/// A file's owner: the account name and the RID (last sub-authority) of its SID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileOwner {
    /// The account name, without its domain.
    pub name: String,
    /// The owner SID's RID — the same number `id -u` reports for that account.
    pub rid: u32,
}

/// Query the account name of the file's owner via its security descriptor.
///
/// Returns the username if resolved, or None if the filesystem or permissions
/// do not support querying security info.
#[must_use]
pub fn get_file_owner(path: &Path) -> Option<String> {
    get_file_owner_info(path).map(|owner| owner.name)
}

/// The RID of a SID: its last sub-authority.
pub(crate) fn sid_rid(sid: PSID) -> Option<u32> {
    // SAFETY: `IsValidSid` accepts any pointer the caller obtained as a PSID.
    if sid.is_null() || unsafe { IsValidSid(sid) } == 0 {
        return None;
    }
    // SAFETY: `sid` is a valid SID, checked above.
    let count_ptr = unsafe { GetSidSubAuthorityCount(sid) };
    // SAFETY: for a valid SID the call returns a pointer into it.
    let count = unsafe { *count_ptr };
    if count == 0 {
        return None;
    }
    // SAFETY: `count - 1` is in range for this valid SID.
    let rid_ptr = unsafe { GetSidSubAuthority(sid, u32::from(count) - 1) };
    // SAFETY: for an in-range index the call returns a pointer into the SID.
    Some(unsafe { *rid_ptr })
}

/// Query the file's owner — account name and RID — via its security descriptor.
///
/// Returns None if the filesystem or permissions do not support querying security info.
#[must_use]
pub fn get_file_owner_info(path: &Path) -> Option<FileOwner> {
    let mut buffer = security_descriptor(path, OWNER_SECURITY_INFORMATION)?;
    owner_of(&mut buffer)
}

/// What `ls -l` shows of a file's security: its owner, and whether this process may
/// write to it (for a folder, create files in it) by its access list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileSecurity {
    /// The owner's account name.
    pub owner: Option<String>,
    /// Whether the access list grants this process write access; `None` when it could
    /// not be read.
    pub writable: Option<bool>,
}

/// The owner and the write access of a file, from one read of its security descriptor.
///
/// The check is `AccessCheck` with this process's own token, the question Windows asks
/// when the file is opened for writing: it counts the groups the token holds, deny
/// entries and inheritance, which reading the access list by eye does not. The
/// read-only attribute is not part of it; the caller combines the two.
#[must_use]
pub fn file_security(path: &Path) -> FileSecurity {
    let Some(mut buffer) = security_descriptor(
        path,
        OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
    ) else {
        return FileSecurity {
            owner: get_file_owner(path),
            writable: None,
        };
    };
    let writable = may_write(&mut buffer);
    FileSecurity {
        owner: owner_of(&mut buffer).map(|owner| owner.name),
        writable,
    }
}

/// A file's security descriptor, with the parts `info` names.
fn security_descriptor(path: &Path, info: OBJECT_SECURITY_INFORMATION) -> Option<Vec<u8>> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();

    let mut needed: u32 = 0;
    // SAFETY: querying required buffer size with a null pointer and length 0.
    unsafe {
        GetFileSecurityW(
            wide.as_ptr(),
            info,
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
            info,
            buffer.as_mut_ptr().cast(),
            needed,
            &raw mut needed,
        )
    };

    (ok != 0).then_some(buffer)
}

/// This process's token as an impersonation token, which `AccessCheck` needs; opened
/// once, and kept for the process's life.
fn impersonation_token() -> Option<windows_sys::Win32::Foundation::HANDLE> {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    static TOKEN: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let token = TOKEN.get_or_init(|| {
        let mut primary = std::ptr::null_mut();
        // SAFETY: returns the current process's pseudo-handle; nothing to uphold.
        let process = unsafe { GetCurrentProcess() };
        // SAFETY: a process handle and a valid out-pointer.
        if unsafe { OpenProcessToken(process, TOKEN_QUERY | TOKEN_DUPLICATE, &raw mut primary) }
            == 0
        {
            return None;
        }
        let mut duplicate = std::ptr::null_mut();
        // SAFETY: `primary` was opened with TOKEN_DUPLICATE above.
        let ok = unsafe { DuplicateToken(primary, SecurityImpersonation, &raw mut duplicate) };
        // SAFETY: `primary` is a handle this function opened and no longer needs.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(primary) };
        (ok != 0).then_some(duplicate as usize)
    });
    token.map(|handle| handle as windows_sys::Win32::Foundation::HANDLE)
}

/// Whether the descriptor's access list lets this process write the file's data, which
/// for a folder is the right to create a file in it.
fn may_write(descriptor: &mut [u8]) -> Option<bool> {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ALL_ACCESS, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
        FILE_WRITE_DATA,
    };
    let token = impersonation_token()?;
    let mapping = GENERIC_MAPPING {
        GenericRead: FILE_GENERIC_READ,
        GenericWrite: FILE_GENERIC_WRITE,
        GenericExecute: FILE_GENERIC_EXECUTE,
        GenericAll: FILE_ALL_ACCESS,
    };
    // Room for the privileges the check may report using.
    // u32s, aligned for PRIVILEGE_SET.
    let mut privileges = [0u32; 64];
    let mut privileges_len = u32::try_from(std::mem::size_of_val(&privileges)).unwrap_or(0);
    let mut granted = 0u32;
    let mut status = 0;
    // SAFETY: `descriptor` holds a descriptor with owner, group and DACL; the token is an
    // impersonation token; every out-pointer is valid, the privilege buffer sized by
    // `privileges_len`.
    let ok = unsafe {
        AccessCheck(
            descriptor.as_mut_ptr().cast(),
            token,
            FILE_WRITE_DATA,
            &raw const mapping,
            privileges.as_mut_ptr().cast::<PRIVILEGE_SET>(),
            &raw mut privileges_len,
            &raw mut granted,
            &raw mut status,
        )
    };
    (ok != 0).then_some(status != 0)
}

/// The owner a security descriptor names, with its account name.
fn owner_of(buffer: &mut [u8]) -> Option<FileOwner> {
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
    let rid = sid_rid(p_sid)?;

    if let Some(name) = cached_lookup(sid_bytes) {
        return Some(FileOwner { name, rid });
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
    Some(FileOwner {
        name: resolved,
        rid,
    })
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

/// The current process's account as a [`FileOwner`]: what a file created now would name
/// as its owner, and the fallback when a file's own owner cannot be read.
#[must_use]
pub fn current_owner() -> Option<FileOwner> {
    crate::process::current_process_account().map(|(name, rid)| FileOwner { name, rid })
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

/// Whether two paths name the same file: the same volume and file index, as a hard link
/// and its original do. `false` when either cannot be opened.
#[must_use]
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (file_identity(a), file_identity(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// A Windows program by its full path in System32 (`whoami.exe`), for cash to start.
///
/// By a bare name, Windows looks first in the folder of the exe that asks, and
/// in a folder of `cash --link-tools` links `whoami.exe` is cash: started that way, cash
/// under the name `whoami` would start `whoami.exe` again as it came up, without end (D65).
#[must_use]
pub fn system_program(name: &str) -> std::path::PathBuf {
    std::env::var_os("SystemRoot")
        .map_or_else(
            || std::path::PathBuf::from(r"C:\Windows"),
            std::path::PathBuf::from,
        )
        .join("System32")
        .join(name)
}

/// A file's volume serial number and file index, which identify it across its names.
fn file_identity(path: &Path) -> Option<(u32, u32, u32)> {
    let file = std::fs::File::open(path).ok()?;
    let handle = file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
    // SAFETY: zeroed struct is valid for BY_HANDLE_FILE_INFORMATION.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `handle` is a valid open file handle, `info` is a valid out-pointer.
    let ok = unsafe { GetFileInformationByHandle(handle, &raw mut info) };
    (ok != 0).then_some((
        info.dwVolumeSerialNumber,
        info.nFileIndexHigh,
        info.nFileIndexLow,
    ))
}
