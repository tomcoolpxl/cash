//! Win32 filesystem metadata and security inspection.
//!
//! Native resolution of Windows file owners (SIDs to account names, which `facts` keeps),
//! hard link counts asked by name where Windows can, and file identity.

use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _};
use std::path::Path;

use windows_sys::Win32::Security::{
    AccessCheck, DACL_SECURITY_INFORMATION, DuplicateToken, GENERIC_MAPPING,
    GROUP_SECURITY_INFORMATION, GetFileSecurityW, GetSecurityDescriptorGroup,
    GetSecurityDescriptorOwner, GetSidSubAuthority, GetSidSubAuthorityCount, IsValidSid,
    OBJECT_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PRIVILEGE_SET, PSID,
    SecurityImpersonation, TOKEN_DUPLICATE, TOKEN_QUERY,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
};

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

/// What `ls -l` shows of a file's security: its owner and group, and whether this
/// process may write to it (for a folder, create files in it) by its access list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileSecurity {
    /// The owner's account name.
    pub owner: Option<String>,
    /// The owner, with its RID.
    pub owner_account: Option<FileOwner>,
    /// The file's primary group, with its RID: `None` on a local machine, `Domain Users`
    /// in a domain, as Windows sets it on a new file.
    pub group: Option<FileOwner>,
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
        let owner_account = get_file_owner_info(path);
        return FileSecurity {
            owner: owner_account.as_ref().map(|owner| owner.name.clone()),
            owner_account,
            group: None,
            writable: None,
        };
    };
    security_of(&mut buffer)
}

/// What a security descriptor with its owner, group and access list says.
fn security_of(buffer: &mut [u8]) -> FileSecurity {
    let writable = may_write(buffer);
    let owner_account = owner_of(buffer);
    FileSecurity {
        owner: owner_account.as_ref().map(|owner| owner.name.clone()),
        owner_account,
        group: group_of(buffer),
        writable,
    }
}

/// What a long listing shows of a file beyond its directory entry
/// ([`listing_facts`]).
#[derive(Clone, Debug)]
pub struct ListingFacts {
    /// Its owner, group and whether this process may write it.
    pub security: FileSecurity,
    /// Its hard links.
    pub links: u32,
    /// Whether it starts with `#!`, when that was asked.
    pub starts_with_shebang: Option<bool>,
}

/// A file's security, hard links and, when `read_start`, whether it starts with `#!`.
///
/// From one open of it: `ls -l` opened each file three times for them (its security,
/// asked twice for the size first, its links, and its first bytes), 180 µs a file.
/// Data access is asked only for `read_start`, so a program is not opened for reading,
/// which an antivirus scans. Follows a link; `None` when the file cannot be opened.
#[must_use]
pub fn listing_facts(path: &Path, read_start: bool) -> Option<ListingFacts> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, GetLastError};
    use windows_sys::Win32::Security::GetKernelObjectSecurity;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL,
    };

    let open = |access: u32| {
        std::fs::OpenOptions::new()
            .access_mode(access)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
    };
    let attributes = READ_CONTROL | FILE_READ_ATTRIBUTES;
    let (mut file, readable) = match read_start.then(|| open(attributes | FILE_READ_DATA)) {
        Some(Ok(file)) => (file, true),
        _ => (open(attributes).ok()?, false),
    };
    let handle = file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;

    let info = OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let mut buffer = vec![0u8; 1024];
    let mut needed = 0u32;
    loop {
        // SAFETY: `handle` was opened with READ_CONTROL; `buffer` holds its length.
        let ok = unsafe {
            GetKernelObjectSecurity(
                handle,
                info,
                buffer.as_mut_ptr().cast(),
                u32::try_from(buffer.len()).unwrap_or(u32::MAX),
                &raw mut needed,
            )
        };
        if ok != 0 {
            break;
        }
        // SAFETY: reads the calling thread's last error.
        if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER || needed as usize <= buffer.len()
        {
            return None;
        }
        buffer.resize(needed as usize, 0);
    }
    let security = security_of(&mut buffer);

    // SAFETY: zeroed struct is valid for BY_HANDLE_FILE_INFORMATION.
    let mut by_handle: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `handle` is open for FILE_READ_ATTRIBUTES; `by_handle` is an out-pointer.
    let links = if unsafe { GetFileInformationByHandle(handle, &raw mut by_handle) } != 0 {
        by_handle.nNumberOfLinks
    } else {
        1
    };

    let starts_with_shebang = read_start.then(|| {
        let mut magic = [0u8; 2];
        readable && std::io::Read::read_exact(&mut file, &mut magic).is_ok() && &magic == b"#!"
    });
    Some(ListingFacts {
        security,
        links,
        starts_with_shebang,
    })
}

/// A file's security descriptor, with the parts `info` names.
///
/// Asked once into a buffer most descriptors fit, and again only for one that does not:
/// each call opens the file, and asking for the size first opened it twice.
fn security_descriptor(path: &Path, info: OBJECT_SECURITY_INFORMATION) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, GetLastError};

    let wide = crate::wide::to_wide_nul(path);
    let mut buffer = vec![0u8; 1024];
    let mut needed: u32 = 0;
    loop {
        // SAFETY: `buffer` holds the length given.
        let ok = unsafe {
            GetFileSecurityW(
                wide.as_ptr(),
                info,
                buffer.as_mut_ptr().cast(),
                u32::try_from(buffer.len()).unwrap_or(u32::MAX),
                &raw mut needed,
            )
        };
        if ok != 0 {
            return Some(buffer);
        }
        // SAFETY: reads the calling thread's last error.
        if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER || needed as usize <= buffer.len()
        {
            return None;
        }
        buffer.resize(needed as usize, 0);
    }
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
        // SAFETY: the call succeeded, so `primary` is an open handle that is ours.
        let primary = unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(primary) };
        let mut duplicate = std::ptr::null_mut();
        // SAFETY: `primary` was opened with TOKEN_DUPLICATE above.
        let ok = unsafe {
            DuplicateToken(
                primary.as_raw_handle(),
                SecurityImpersonation,
                &raw mut duplicate,
            )
        };
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
    account_of(p_sid)
}

/// The primary group a security descriptor names, with its account name.
fn group_of(buffer: &mut [u8]) -> Option<FileOwner> {
    let mut p_sid = std::ptr::null_mut();
    let mut defaulted = 0;
    // SAFETY: `buffer` contains a valid SECURITY_DESCRIPTOR retrieved by `GetFileSecurityW`
    // with the group asked for.
    let ok = unsafe {
        GetSecurityDescriptorGroup(
            buffer.as_mut_ptr().cast(),
            &raw mut p_sid,
            &raw mut defaulted,
        )
    };
    if ok == 0 || p_sid.is_null() {
        return None;
    }
    account_of(p_sid)
}

/// The account a SID names, with its RID; the name asked of LSA once per account
/// (`facts`).
fn account_of(p_sid: PSID) -> Option<FileOwner> {
    let rid = sid_rid(p_sid)?;
    let name = crate::facts::account_name(p_sid)?;
    Some(FileOwner { name, rid })
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

/// `FILE_STAT_BASIC_INFORMATION` (fileapi.h), what `GetFileInformationByName` fills;
/// windows-sys does not have it.
#[repr(C)]
struct FileStatBasicInformation {
    file_id: i64,
    creation_time: i64,
    last_access_time: i64,
    last_write_time: i64,
    change_time: i64,
    allocation_size: i64,
    end_of_file: i64,
    file_attributes: u32,
    reparse_tag: u32,
    number_of_links: u32,
    device_type: u32,
    device_characteristics: u32,
    reserved: u32,
    volume_serial_number: i64,
    file_id_128: [u8; 16],
}

type GetFileInformationByNameFn =
    unsafe extern "system" fn(*const u16, i32, *mut core::ffi::c_void, u32) -> i32;

/// `GetFileInformationByName`, looked up at run time: Windows 11 24H2 and later have it,
/// and linking it would keep cash from starting on any Windows before.
fn get_file_information_by_name() -> Option<GetFileInformationByNameFn> {
    use std::sync::LazyLock;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

    static FUNCTION: LazyLock<Option<GetFileInformationByNameFn>> = LazyLock::new(|| {
        [c"kernel32.dll", c"kernelbase.dll"]
            .iter()
            .find_map(|module| {
                // SAFETY: both modules are loaded into every Windows process; the name is NUL
                // terminated.
                let handle = unsafe { GetModuleHandleA(module.as_ptr().cast()) };
                if handle.is_null() {
                    return None;
                }
                // SAFETY: the module handle is valid and the name is NUL terminated.
                let found =
                    unsafe { GetProcAddress(handle, c"GetFileInformationByName".as_ptr().cast()) };
                found.map(|function| {
                    // SAFETY: the export has this signature (fileapi.h).
                    unsafe {
                        std::mem::transmute::<
                            unsafe extern "system" fn() -> isize,
                            GetFileInformationByNameFn,
                        >(function)
                    }
                })
            })
    });
    *FUNCTION
}

/// What a file's name alone tells, without opening it: `None` where Windows cannot say
/// (before 11 24H2, or a file system that refuses, as some shares do), and for a reparse
/// point (a link, a junction), which its callers want followed, as opening it follows it.
fn stat_by_name(path: &Path) -> Option<FileStatBasicInformation> {
    // FileStatBasicByNameInfo.
    const FILE_STAT_BASIC_BY_NAME_INFO: i32 = 0;
    let function = get_file_information_by_name()?;
    let wide = crate::wide::to_wide_nul(path);
    let mut info = std::mem::MaybeUninit::<FileStatBasicInformation>::zeroed();
    // SAFETY: `wide` is NUL terminated; `info` is valid for writes of its size, given.
    let ok = unsafe {
        function(
            wide.as_ptr(),
            FILE_STAT_BASIC_BY_NAME_INFO,
            info.as_mut_ptr().cast(),
            u32::try_from(size_of::<FileStatBasicInformation>()).ok()?,
        )
    };
    if ok == 0 {
        return None;
    }
    // SAFETY: the call succeeded and filled the structure.
    let info = unsafe { info.assume_init() };
    (info.reparse_tag == 0).then_some(info)
}

/// A file's hard links, asked by its name: see [`stat_by_name`].
fn links_by_name(path: &Path) -> Option<u32> {
    stat_by_name(path).map(|info| info.number_of_links)
}

/// [`FileInfo`] from what [`stat_by_name`] found: the volume's serial number is the 32
/// bits `GetFileInformationByHandle` gives of it. `None` while Windows leaves the serial
/// number 0, as 26300 does: an identity without its volume is no identity.
const fn file_info_of(info: &FileStatBasicInformation) -> Option<FileInfo> {
    if info.volume_serial_number == 0 {
        return None;
    }
    let serial = info.volume_serial_number.to_ne_bytes();
    Some(FileInfo {
        volume: u32::from_ne_bytes([serial[0], serial[1], serial[2], serial[3]]),
        index: u64::from_ne_bytes(info.file_id.to_ne_bytes()),
        links: info.number_of_links,
    })
}

/// A folder's link count counted, as Linux has it.
///
/// 2 (its name, and its own `.`) and one for each folder in it (its `..`). Reads the
/// whole folder, so it is for questions about one folder (`stat`, `ls --count-links`); a
/// listing shows [`file_link_count`]'s 1.
#[must_use]
pub fn folder_link_count(path: &Path) -> u32 {
    let subfolders = std::fs::read_dir(path).map_or(0, |entries| {
        entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .count()
    });
    2 + u32::try_from(subfolders).unwrap_or(u32::MAX - 2)
}

/// Compute the POSIX-equivalent link count for a file or directory.
///
/// A folder's is 1, as Git Bash shows it: Linux's 2 + its subfolders took reading the
/// whole folder, which for a temporary folder's `..` was 160,000 entries. A file's is its
/// NTFS hard links, asked by name where Windows can (11 24H2 and later), else through
/// `GetFileInformationByHandle`.
#[must_use]
pub fn file_link_count(path: &Path, metadata: &std::fs::Metadata) -> u32 {
    if metadata.is_dir() {
        return 1;
    }
    if let Some(links) = links_by_name(path) {
        return links.max(1);
    }
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

/// Whether two paths name the same file: the same volume and file index, as a hard link
/// and its original do. `false` when either cannot be opened.
#[must_use]
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (file_identity(a), file_identity(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// What an open handle is open on, as Windows tells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleKind {
    /// A file or folder on a disk.
    Disk,
    /// A pipe, anonymous or named.
    Pipe,
    /// A character device: the console, the null device.
    Char,
    /// Anything else, or a handle that is not open.
    Unknown,
}

/// What `handle` is open on.
#[must_use]
pub fn handle_kind(handle: std::os::windows::io::BorrowedHandle<'_>) -> HandleKind {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_TYPE_CHAR, FILE_TYPE_DISK, FILE_TYPE_PIPE, GetFileType,
    };

    // SAFETY: the handle is borrowed open; any handle may be asked.
    let kind = unsafe { GetFileType(handle.as_raw_handle()) };
    match kind {
        FILE_TYPE_DISK => HandleKind::Disk,
        FILE_TYPE_PIPE => HandleKind::Pipe,
        FILE_TYPE_CHAR => HandleKind::Char,
        _ => HandleKind::Unknown,
    }
}

/// Moves the position of the file `handle` is open on by `by` bytes from where it is:
/// what a reader read ahead and did not use, given back to whatever reads next.
///
/// # Errors
///
/// When Windows refuses: the handle is not open on a file on a disk, or the position
/// would go before the file's start.
pub fn move_position(
    handle: std::os::windows::io::BorrowedHandle<'_>,
    by: i64,
) -> std::io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{FILE_CURRENT, SetFilePointerEx};

    // SAFETY: the handle is borrowed open, and the new position need not be written back.
    let moved = unsafe {
        SetFilePointerEx(
            handle.as_raw_handle(),
            by,
            std::ptr::null_mut(),
            FILE_CURRENT,
        )
    };
    if moved == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// The entries of the folder `dir` whose names begin with `prefix`, each with whether it
/// is a folder.
///
/// One call, where a look for each name it might be is a call each; none for a folder
/// that is not there. The folder compares the names as it compares them, and Windows
/// matches a name's 8.3 alias too, giving back the long name.
///
/// # Errors
///
/// When the folder cannot be read for any other reason.
pub fn names_beginning(
    dir: &Path,
    prefix: &std::ffi::OsStr,
) -> std::io::Result<Vec<(std::ffi::OsString, bool)>> {
    use std::os::windows::ffi::OsStringExt as _;
    use windows_sys::Win32::Foundation::{
        ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES, ERROR_PATH_NOT_FOUND, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FIND_FIRST_EX_LARGE_FETCH, FindClose, FindExInfoBasic,
        FindExSearchNameMatch, FindFirstFileExW, FindNextFileW, WIN32_FIND_DATAW,
    };

    let mut pattern = dir.as_os_str().to_owned();
    pattern.push("\\");
    pattern.push(prefix);
    pattern.push("*");
    let pattern = crate::wide::to_wide_nul(&pattern);
    // SAFETY: zeroed is a valid WIN32_FIND_DATAW, filled in by the calls below.
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    // SAFETY: the pattern is null-terminated, `data` is a valid out-pointer, and the
    // filter pointer may be null for a name match.
    let find = unsafe {
        FindFirstFileExW(
            pattern.as_ptr(),
            FindExInfoBasic,
            (&raw mut data).cast(),
            FindExSearchNameMatch,
            std::ptr::null(),
            FIND_FIRST_EX_LARGE_FETCH,
        )
    };
    if find == INVALID_HANDLE_VALUE {
        let error = std::io::Error::last_os_error();
        return match error
            .raw_os_error()
            .and_then(|code| u32::try_from(code).ok())
        {
            Some(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) => Ok(Vec::new()),
            _ => Err(error),
        };
    }
    let mut found = Vec::new();
    loop {
        let length = data
            .cFileName
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(data.cFileName.len());
        let name = std::ffi::OsString::from_wide(data.cFileName.get(..length).unwrap_or_default());
        found.push((name, data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0));
        // SAFETY: `find` is the open search handle, `data` a valid out-pointer.
        if unsafe { FindNextFileW(find, &raw mut data) } == 0 {
            let error = std::io::Error::last_os_error();
            // SAFETY: `find` is open, and closed once.
            unsafe { FindClose(find) };
            return match error
                .raw_os_error()
                .and_then(|code| u32::try_from(code).ok())
            {
                Some(ERROR_NO_MORE_FILES) => Ok(found),
                _ => Err(error),
            };
        }
    }
}

/// A Windows program by its full path in System32 (`quser.exe`), for cash to start.
///
/// By a bare name, Windows looks first in the folder of the exe that asks, and in a
/// folder of `cash --link-tools` links a tool of that name is cash: started that way, cash
/// would start itself again as it came up, without end (D65).
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

/// Whether `path` is a program of Windows' own: one under `System32` or `SysWOW64` in
/// the Windows folder, in any spelling (`C:/WINDOWS/system32/net.exe`, as `type` prints it).
///
/// cmd never expands a wildcard, so every such program reads `*` and `?` itself when it
/// wants them (D81). `explorer.exe`, and the Python launcher's `py.exe`, which live in
/// the Windows folder itself, are not counted.
#[must_use]
pub fn is_system_program(path: &Path) -> bool {
    let root = std::env::var_os("SystemRoot").map_or_else(
        || std::path::PathBuf::from(r"C:\Windows"),
        std::path::PathBuf::from,
    );
    is_under_system_folders(&root, path)
}

/// [`is_system_program`] with the Windows folder given.
fn is_under_system_folders(root: &Path, path: &Path) -> bool {
    let spelled = |path: &Path| path.to_string_lossy().replace('/', "\\").to_lowercase();
    let path = spelled(path);
    ["System32", "SysWOW64"].into_iter().any(|folder| {
        let mut prefix = spelled(&root.join(folder));
        prefix.push('\\');
        path.starts_with(&prefix)
    })
}

/// What identifies a file across its names, and how many it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileInfo {
    /// The serial number of the volume it is on: `stat`'s device.
    pub volume: u32,
    /// Its file index on that volume: `stat`'s inode.
    pub index: u64,
    /// Its hard links.
    pub links: u32,
}

/// A file's or folder's [`FileInfo`].
///
/// Asked by name where Windows can (`GetFileInformationByName`): the identity behind `stat`,
/// `same_file` and the archivers' hard links, without an open per file. Else opened with
/// backup semantics, without which a folder cannot be opened at all: `stat` gave a folder
/// inode 0 and `[ d -ef d ]` was "not supported" (XC-3, ARCH-06). It asks only to read
/// attributes, and shares everything.
///
/// # Errors
///
/// The error of opening it or of asking.
pub fn file_info(path: &Path) -> std::io::Result<FileInfo> {
    if let Some(info) = stat_by_name(path).as_ref().and_then(file_info_of) {
        return Ok(info);
    }
    file_info_by_handle(path)
}

/// [`file_info`] through a handle: what it was before Windows could answer by name, and
/// still is for a link, which is followed.
fn file_info_by_handle(path: &Path) -> std::io::Result<FileInfo> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE,
    };

    let file = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let handle = file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
    // SAFETY: zeroed struct is valid for BY_HANDLE_FILE_INFORMATION.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `handle` is a valid open file handle, `info` is a valid out-pointer.
    let ok = unsafe { GetFileInformationByHandle(handle, &raw mut info) };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(FileInfo {
        volume: info.dwVolumeSerialNumber,
        index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        links: info.nNumberOfLinks,
    })
}

/// A file's volume serial number and file index, which identify it across its names.
fn file_identity(path: &Path) -> Option<(u32, u64)> {
    file_info(path).ok().map(|info| (info.volume, info.index))
}

/// Moves a file or folder to the Recycle Bin, as Explorer's Delete does, asking nothing
/// and showing nothing. The path is made whole first: the bin keeps where it came from.
///
/// # Errors
///
/// When the shell does not move it, with its code.
pub fn recycle(path: &Path) -> std::io::Result<()> {
    use windows_sys::Win32::UI::Shell::{
        FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHFILEOPSTRUCTW,
        SHFileOperationW,
    };
    let whole = std::path::absolute(path)?;
    // The list of names ends in a second NUL.
    let mut from = crate::wide::to_wide_nul(&whole);
    from.push(0);
    let flags = FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT;
    let mut operation = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        fFlags: u16::try_from(flags).unwrap_or(u16::MAX),
        ..SHFILEOPSTRUCTW::default()
    };
    // SAFETY: `operation` is a valid SHFILEOPSTRUCTW whose `pFrom` points at a
    // double-NUL-terminated wide string that lives across the call.
    let result = unsafe { SHFileOperationW(&raw mut operation) };
    if result != 0 {
        return Err(std::io::Error::other(format!(
            "the shell could not move it to the Recycle Bin (code {result:#x})"
        )));
    }
    if operation.fAnyOperationsAborted != 0 {
        return Err(std::io::Error::other(
            "moving it to the Recycle Bin was stopped",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod by_name_tests {
    use super::{FileInfo, file_info, file_info_by_handle, file_link_count, stat_by_name};

    #[test]
    fn a_file_is_the_same_asked_by_name_as_through_a_handle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        std::fs::write(&path, "x").unwrap();
        std::fs::hard_link(&path, dir.path().join("other")).unwrap();
        let by_handle = file_info_by_handle(&path).unwrap();
        assert_eq!(by_handle.links, 2);
        // Before Windows 11 24H2 there is no asking by name; the handle answers then. Asked
        // by name, the file and its links are those the handle gives; the volume, when
        // Windows gives one at all.
        if let Some(info) = stat_by_name(&path) {
            assert_eq!(
                (
                    u64::from_ne_bytes(info.file_id.to_ne_bytes()),
                    info.number_of_links
                ),
                (by_handle.index, by_handle.links)
            );
            if let Some(found) = super::file_info_of(&info) {
                assert_eq!(found, by_handle);
            }
        }
        assert_eq!(file_info(&path).unwrap(), by_handle);
        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(file_link_count(&path, &meta), 2);
        // A folder's is 1, as Git Bash shows it, however many subfolders it has.
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let meta = std::fs::metadata(dir.path()).unwrap();
        assert_eq!(file_link_count(dir.path(), &meta), 1);
        let folder: FileInfo = file_info(dir.path()).unwrap();
        assert_eq!(folder, file_info_by_handle(dir.path()).unwrap());
    }
}

#[cfg(test)]
mod system_program_tests {
    use super::is_under_system_folders;
    use std::path::Path;

    #[test]
    fn system32_and_syswow64_in_any_spelling() {
        let root = Path::new(r"C:\WINDOWS");
        for program in [
            "C:/WINDOWS/system32/net.exe",
            r"C:\Windows\System32\OpenSSH\scp.exe",
            r"c:\windows\syswow64\cmd.exe",
            r"C:\WINDOWS/System32/cmd.exe",
        ] {
            assert!(
                is_under_system_folders(root, Path::new(program)),
                "{program}"
            );
        }
    }

    #[test]
    fn the_windows_folder_itself_and_other_folders_are_not() {
        let root = Path::new(r"C:\Windows");
        for program in [
            r"C:\Windows\explorer.exe",
            r"C:\Windows\py.exe",
            r"C:\Windows\System32x\x.exe",
            r"C:\Program Files\Git\usr\bin\scp.exe",
            r"D:\Windows\System32\net.exe",
            "net.exe",
        ] {
            assert!(
                !is_under_system_folders(root, Path::new(program)),
                "{program}"
            );
        }
    }
}
