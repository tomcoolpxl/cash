//! The `/dev` names in a bundled tool's own opens (**D7**).
//!
//! A redirection to `/dev/stdin` or `/dev/null` works because cash opens the file itself
//! and knows the names. A bundled tool (`cat /dev/stdin`, `tee /dev/stderr`,
//! `cp /dev/null f`) is handed the name as written (D4) and opens it itself, and Windows
//! has no `/dev`: "The system cannot find the path specified." The tools are 85 uutils
//! crates, each opening its files its own way, with no one function in Rust they all go
//! through. They all go through one Windows function, though: every open in `cash.exe`,
//! of the standard library, uucore and the tools alike, is a call to `CreateFileW` from
//! `kernel32.dll`, and every existence check that does not open is `GetFileAttributesW`.
//!
//! So in a bundled tool's process, and only there (never in the shell), [`install`]
//! points `cash.exe`'s imports of those two, and of the calls that ask about or copy an
//! open file, at the functions below. They answer the `/dev` names as a redirection does,
//! and pass every other path on unchanged:
//!
//! | Name | What the tool gets |
//! | --- | --- |
//! | `/dev/stdin`, `/dev/fd/0` | its own standard input |
//! | `/dev/stdout`, `/dev/fd/1` | its own standard output, shared, not opened again |
//! | `/dev/stderr`, `/dev/fd/2` | its own standard error |
//! | `/dev/fd/3` and up | nothing: a program has no descriptor above 2 (D26) |
//! | `/dev/null` | the null device |
//! | `/dev/tty` | the console: its input to read, its screen to write |
//!
//! A name is one where the shell's redirections take it (`names_under_dev` in cash-core):
//! at `\dev` at the root, with or without a drive, so as written or as resolving it
//! against a folder leaves it. The standard library hands `CreateFileW` `/dev/stdin` made
//! absolute, `\\?\C:\dev\stdin`. Deeper down (`C:/src/dev/null`) it is a file like any
//! other.

use std::ffi::c_void;
use std::fs::File;
use std::os::windows::io::{FromRawHandle, IntoRawHandle};
use std::sync::atomic::{AtomicUsize, Ordering};

use windows_sys::Win32::Foundation::{
    DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_FILE_NOT_FOUND, ERROR_INVALID_HANDLE, FILETIME,
    GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, SetLastError,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, COPY_FILE_FAIL_IF_EXISTS, CREATE_ALWAYS, CREATE_NEW,
    FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_TAG_INFO, FILE_BASIC_INFO, FILE_END,
    FILE_END_OF_FILE_INFO, FILE_INFO_BY_HANDLE_CLASS, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FILE_STANDARD_INFO, FILE_TYPE_CHAR, FILE_TYPE_PIPE, FILE_WRITE_DATA,
    FileAttributeTagInfo, FileBasicInfo, FileEndOfFileInfo, FileStandardInfo, GetFileType,
    INVALID_FILE_ATTRIBUTES, LPPROGRESS_ROUTINE, OPEN_EXISTING, SET_FILE_POINTER_MOVE_METHOD,
};
use windows_sys::Win32::System::Console::{
    GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::core::{BOOL, PCWSTR};

use crate::endless::Endless;
use crate::wide::to_wide_nul;

type CreateFileW = unsafe extern "system" fn(
    PCWSTR,
    u32,
    u32,
    *const SECURITY_ATTRIBUTES,
    u32,
    u32,
    HANDLE,
) -> HANDLE;

type GetFileAttributesW = unsafe extern "system" fn(PCWSTR) -> u32;

type GetFileInformationByHandle =
    unsafe extern "system" fn(HANDLE, *mut BY_HANDLE_FILE_INFORMATION) -> BOOL;

type GetFileInformationByHandleEx =
    unsafe extern "system" fn(HANDLE, FILE_INFO_BY_HANDLE_CLASS, *mut c_void, u32) -> BOOL;

type CopyFileExW = unsafe extern "system" fn(
    PCWSTR,
    PCWSTR,
    LPPROGRESS_ROUTINE,
    *const c_void,
    *mut BOOL,
    u32,
) -> BOOL;

type SetFilePointerEx =
    unsafe extern "system" fn(HANDLE, i64, *mut i64, SET_FILE_POINTER_MOVE_METHOD) -> BOOL;

type SetFileInformationByHandle =
    unsafe extern "system" fn(HANDLE, FILE_INFO_BY_HANDLE_CLASS, *const c_void, u32) -> BOOL;

/// The functions the imports pointed at before [`install`]; 0 until then.
static CREATE_FILE: AtomicUsize = AtomicUsize::new(0);
static GET_FILE_ATTRIBUTES: AtomicUsize = AtomicUsize::new(0);
static GET_INFORMATION: AtomicUsize = AtomicUsize::new(0);
static GET_INFORMATION_EX: AtomicUsize = AtomicUsize::new(0);
static COPY_FILE: AtomicUsize = AtomicUsize::new(0);
static SET_POINTER: AtomicUsize = AtomicUsize::new(0);
static SET_INFORMATION: AtomicUsize = AtomicUsize::new(0);

/// Makes the `/dev` names open in this process as they do in a redirection. For a
/// bundled tool's process, before the tool runs.
///
/// Returns whether every import was found and pointed here.
pub fn install() -> bool {
    use crate::imports::redirect;

    let opens = redirect(
        "CreateFileW",
        create_file as CreateFileW as usize,
        &CREATE_FILE,
    );
    let attributes = redirect(
        "GetFileAttributesW",
        get_file_attributes as GetFileAttributesW as usize,
        &GET_FILE_ATTRIBUTES,
    );
    let information = redirect(
        "GetFileInformationByHandle",
        get_information as GetFileInformationByHandle as usize,
        &GET_INFORMATION,
    );
    let information_ex = redirect(
        "GetFileInformationByHandleEx",
        get_information_ex as GetFileInformationByHandleEx as usize,
        &GET_INFORMATION_EX,
    );
    let copies = redirect("CopyFileExW", copy_file as CopyFileExW as usize, &COPY_FILE);
    let seeks = redirect(
        "SetFilePointerEx",
        set_pointer as SetFilePointerEx as usize,
        &SET_POINTER,
    );
    let sizes = redirect(
        "SetFileInformationByHandle",
        set_information as SetFileInformationByHandle as usize,
        &SET_INFORMATION,
    );
    opens && attributes && information && information_ex && copies && seeks && sizes
}

/// Loads the function an import pointed at before [`install`].
///
/// # Safety
///
/// `F` must be the type of the function kept in `original`, and [`install`] must have
/// run, as it has whenever one of the functions here is called.
unsafe fn original<F: Copy>(original: &AtomicUsize) -> F {
    let address = original.load(Ordering::Acquire);
    // SAFETY: as the caller guarantees; a function pointer is the size of a usize.
    unsafe { std::mem::transmute_copy::<usize, F>(&address) }
}

/// A name under `/dev`, as the shell's redirections, its file tests, `ls`, `stat` and the
/// bundled tools all take it (D7): see [`dev_name`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevName {
    /// `/dev/null`.
    Null,
    /// `/dev/tty`, the console.
    Tty,
    /// `/dev/zero`.
    Zero,
    /// `/dev/random`.
    Random,
    /// `/dev/urandom`.
    Urandom,
    /// `/dev/stdin` (0), `/dev/stdout` (1), `/dev/stderr` (2) or `/dev/fd/N`: a
    /// descriptor's name.
    Descriptor(u32),
}

impl DevName {
    /// The device numbers Linux gives the device, major and minor, which `ls -l` and
    /// `stat` show; none for a descriptor's name, which is a link.
    #[must_use]
    pub const fn numbers(self) -> Option<(u32, u32)> {
        match self {
            Self::Null => Some((1, 3)),
            Self::Tty => Some((5, 0)),
            Self::Zero => Some((1, 5)),
            Self::Random => Some((1, 8)),
            Self::Urandom => Some((1, 9)),
            Self::Descriptor(_) => None,
        }
    }

    /// The endless input the device is, if it is one.
    #[must_use]
    pub const fn endless(self) -> Option<Endless> {
        match self {
            Self::Zero => Some(Endless::Zeros),
            Self::Random | Self::Urandom => Some(Endless::Random),
            _ => None,
        }
    }
}

/// The `/dev` name `path` is, if it is one.
///
/// `\dev` at the root, after any prefix (a drive, `\\?\C:`, a share), either separator,
/// and one name under it, or `fd` and a number: as a script writes it, `/dev/null`, or as
/// resolving it against a folder leaves it, `C:/dev/null`, or as the standard library
/// hands it to `CreateFileW`, `\\?\C:\dev\null`. Anywhere else (`C:/src/dev/null`), or
/// deeper, it is a file like any other. With a separator after it, `/dev/null/`, it names
/// a folder, which none of them is, as in Bash. The names are as Bash's own redirections
/// take them, case and all.
#[must_use]
pub fn dev_name(path: &std::path::Path) -> Option<DevName> {
    use std::path::Component;

    if path.as_os_str().to_string_lossy().ends_with(['/', '\\']) {
        return None;
    }
    let mut components = path.components().peekable();
    components.next_if(|component| matches!(component, Component::Prefix(_)));
    match (components.next(), components.next()) {
        (Some(Component::RootDir), Some(Component::Normal(dev))) if dev == "dev" => {}
        _ => return None,
    }
    let first = normal(components.next())?;
    let second = components.next();
    if second.is_none() {
        return match first {
            "null" => Some(DevName::Null),
            "tty" => Some(DevName::Tty),
            "stdin" => Some(DevName::Descriptor(0)),
            "stdout" => Some(DevName::Descriptor(1)),
            "stderr" => Some(DevName::Descriptor(2)),
            "zero" => Some(DevName::Zero),
            "random" => Some(DevName::Random),
            "urandom" => Some(DevName::Urandom),
            _ => None,
        };
    }
    let number = normal(second)?;
    if first != "fd" || components.next().is_some() {
        return None;
    }
    descriptor_number(number).map(DevName::Descriptor)
}

/// The name `component` is, if it is a plain one.
fn normal(component: Option<std::path::Component<'_>>) -> Option<&str> {
    match component {
        Some(std::path::Component::Normal(name)) => name.to_str(),
        _ => None,
    }
}

/// The descriptor `name` is the number of, as a listing of `/dev/fd` would write it:
/// digits and nothing else, and no zero in front. `+3`, `-1` and `03` are numbers to
/// `parse` and no such names, in Bash either.
fn descriptor_number(name: &str) -> Option<u32> {
    let digits = !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_digit());
    let zero_in_front = name.len() > 1 && name.starts_with('0');
    if digits && !zero_in_front {
        name.parse().ok()
    } else {
        None
    }
}

/// What a `/dev` name is in a tool's process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Device {
    /// The tool's standard input, output or error.
    Std(STD_HANDLE),
    /// A descriptor the process does not have.
    Missing,
    /// The null device.
    Null,
    /// The console.
    Tty,
    /// `/dev/zero`, `/dev/random` or `/dev/urandom`.
    Endless(Endless),
}

/// The device `name`, a path as `CreateFileW` is given it, is, if it is one.
fn device(name: &str) -> Option<Device> {
    Some(match dev_name(std::path::Path::new(name))? {
        DevName::Null => Device::Null,
        DevName::Tty => Device::Tty,
        name @ (DevName::Zero | DevName::Random | DevName::Urandom) => {
            Device::Endless(name.endless()?)
        }
        DevName::Descriptor(0) => Device::Std(STD_INPUT_HANDLE),
        DevName::Descriptor(1) => Device::Std(STD_OUTPUT_HANDLE),
        DevName::Descriptor(2) => Device::Std(STD_ERROR_HANDLE),
        DevName::Descriptor(_) => Device::Missing,
    })
}

/// `name`, a null-terminated wide string, as text.
///
/// # Safety
///
/// `name` must be null or point at a null-terminated wide string.
unsafe fn text(name: PCWSTR) -> Option<String> {
    if name.is_null() {
        return None;
    }
    let mut len = 0;
    // SAFETY: the string is null-terminated, as the caller guarantees, so every unit up
    // to the null may be read.
    while unsafe { name.wrapping_add(len).read() } != 0 {
        len += 1;
    }
    // SAFETY: `len` code units were just read.
    let wide = unsafe { std::slice::from_raw_parts(name, len) };
    Some(String::from_utf16_lossy(wide))
}

/// The device `name` names, if it names one.
///
/// # Safety
///
/// `name` must be null or point at a null-terminated wide string.
unsafe fn device_at(name: PCWSTR) -> Option<Device> {
    // SAFETY: as the caller guarantees.
    device(&unsafe { text(name) }?)
}

fn original_create_file() -> CreateFileW {
    // SAFETY: the type kept there; set before the import was pointed here.
    unsafe { original(&CREATE_FILE) }
}

fn original_attributes() -> GetFileAttributesW {
    // SAFETY: as above.
    unsafe { original(&GET_FILE_ATTRIBUTES) }
}

/// `CreateFileW`, with the `/dev` names answered.
unsafe extern "system" fn create_file(
    name: PCWSTR,
    access: u32,
    share: u32,
    security: *const SECURITY_ATTRIBUTES,
    disposition: u32,
    flags: u32,
    template: HANDLE,
) -> HANDLE {
    let open = original_create_file();
    // SAFETY: the caller passes a valid name, as `CreateFileW` requires.
    let Some(device) = (unsafe { device_at(name) }) else {
        // SAFETY: the caller's arguments, passed on unchanged.
        return unsafe { open(name, access, share, security, disposition, flags, template) };
    };
    match device {
        Device::Std(which) => duplicate_std(which),
        Device::Missing => {
            // SAFETY: always safe.
            unsafe { SetLastError(ERROR_FILE_NOT_FOUND) };
            INVALID_HANDLE_VALUE
        }
        Device::Null => {
            let null = to_wide_nul(r"\\.\NUL");
            // SAFETY: `null` is null-terminated; the rest are the caller's arguments.
            unsafe {
                open(
                    null.as_ptr(),
                    access,
                    share,
                    security,
                    disposition,
                    flags,
                    template,
                )
            }
        }
        Device::Tty => {
            let writes = access & (GENERIC_WRITE | FILE_WRITE_DATA | FILE_APPEND_DATA) != 0;
            let console = to_wide_nul(if writes { "CONOUT$" } else { "CONIN$" });
            // SAFETY: `console` is null-terminated; a console is opened, never created.
            unsafe {
                open(
                    console.as_ptr(),
                    access,
                    share,
                    security,
                    OPEN_EXISTING,
                    flags,
                    template,
                )
            }
        }
        // Written to, it discards, as the null device does.
        Device::Endless(_)
            if access & (GENERIC_WRITE | FILE_WRITE_DATA | FILE_APPEND_DATA) != 0 =>
        {
            let null = to_wide_nul(r"\\.\NUL");
            // SAFETY: `null` is null-terminated; the rest are the caller's arguments.
            unsafe {
                open(
                    null.as_ptr(),
                    access,
                    share,
                    security,
                    disposition,
                    flags,
                    template,
                )
            }
        }
        Device::Endless(kind) => match crate::endless::open(kind) {
            Ok(file) => file.into_raw_handle(),
            Err(error) => {
                let code = error
                    .raw_os_error()
                    .and_then(|code| u32::try_from(code).ok());
                // SAFETY: always safe.
                unsafe { SetLastError(code.unwrap_or(ERROR_FILE_NOT_FOUND)) };
                INVALID_HANDLE_VALUE
            }
        },
    }
}

/// A handle of the process's own to its standard input, output or error, for the caller
/// to close.
fn duplicate_std(which: STD_HANDLE) -> HANDLE {
    // SAFETY: always safe; the result is checked.
    let handle = unsafe { GetStdHandle(which) };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        // SAFETY: always safe.
        unsafe { SetLastError(ERROR_INVALID_HANDLE) };
        return INVALID_HANDLE_VALUE;
    }
    let mut copy: HANDLE = std::ptr::null_mut();
    // SAFETY: always safe.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: `process` is this process's pseudo-handle, `handle` is open, and `copy` is
    // a valid out-param.
    let copied = unsafe {
        DuplicateHandle(
            process,
            handle,
            process,
            &raw mut copy,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if copied == 0 {
        INVALID_HANDLE_VALUE
    } else {
        copy
    }
}

/// `GetFileAttributesW`, with the `/dev` names answered: each is there, and no folder.
unsafe extern "system" fn get_file_attributes(name: PCWSTR) -> u32 {
    // SAFETY: the caller passes a valid name, as `GetFileAttributesW` requires.
    match unsafe { device_at(name) } {
        None => {
            // SAFETY: the caller's argument, passed on unchanged.
            unsafe { original_attributes()(name) }
        }
        Some(Device::Missing) => {
            // SAFETY: always safe.
            unsafe { SetLastError(ERROR_FILE_NOT_FOUND) };
            INVALID_FILE_ATTRIBUTES
        }
        Some(_) => FILE_ATTRIBUTE_NORMAL,
    }
}

/// Whether Windows keeps no file information for `handle`: a pipe or a character device
/// (the console, `NUL`), of which it says "Incorrect function".
fn has_no_information(handle: HANDLE) -> bool {
    // SAFETY: any handle may be asked; a bad one is `FILE_TYPE_UNKNOWN`.
    let kind = unsafe { GetFileType(handle) };
    kind == FILE_TYPE_PIPE || kind == FILE_TYPE_CHAR
}

/// The time now, as file times are kept.
fn now() -> FILETIME {
    let mut time = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    // SAFETY: `time` is a valid out-param.
    unsafe { GetSystemTimeAsFileTime(&raw mut time) };
    time
}

/// `time` as one count, as the newer information classes keep it.
fn count(time: FILETIME) -> i64 {
    (i64::from(time.dwHighDateTime) << 32) | i64::from(time.dwLowDateTime)
}

/// `GetFileInformationByHandle`, answering for a pipe or a device as for an empty file.
///
/// A tool asks before it reads (`cat` whether its input is a folder, `cp` what kind of
/// file to copy), and failed on `/dev/stdin` and `/dev/null` as uutils does on `NUL`. The
/// file index is the handle, so that two of them are not taken for the same file.
unsafe extern "system" fn get_information(
    handle: HANDLE,
    info: *mut BY_HANDLE_FILE_INFORMATION,
) -> BOOL {
    // SAFETY: the type kept there; set before the import was pointed here.
    let get: GetFileInformationByHandle = unsafe { original(&GET_INFORMATION) };
    // SAFETY: the caller's arguments, passed on unchanged.
    let answered = unsafe { get(handle, info) };
    if answered != 0 || info.is_null() || !has_no_information(handle) {
        return answered;
    }
    let time = now();
    let index = handle as usize;
    // SAFETY: `info` is the caller's, valid for a write.
    unsafe {
        info.write(BY_HANDLE_FILE_INFORMATION {
            dwFileAttributes: FILE_ATTRIBUTE_NORMAL,
            ftCreationTime: time,
            ftLastAccessTime: time,
            ftLastWriteTime: time,
            dwVolumeSerialNumber: 0,
            nFileSizeHigh: 0,
            nFileSizeLow: 0,
            nNumberOfLinks: 1,
            nFileIndexHigh: u32::try_from(index >> 32).unwrap_or(0),
            nFileIndexLow: u32::try_from(index & 0xFFFF_FFFF).unwrap_or(0),
        });
    }
    1
}

/// `GetFileInformationByHandleEx`, answering for a pipe or a device as for an empty file,
/// for the classes the standard library asks for.
unsafe extern "system" fn get_information_ex(
    handle: HANDLE,
    class: FILE_INFO_BY_HANDLE_CLASS,
    info: *mut c_void,
    size: u32,
) -> BOOL {
    // SAFETY: the type kept there; set before the import was pointed here.
    let get: GetFileInformationByHandleEx = unsafe { original(&GET_INFORMATION_EX) };
    // SAFETY: the caller's arguments, passed on unchanged.
    let answered = unsafe { get(handle, class, info, size) };
    if answered != 0 || info.is_null() || !has_no_information(handle) {
        return answered;
    }
    let fits = |needed: usize| usize::try_from(size).is_ok_and(|size| size >= needed);
    match class {
        c if c == FileBasicInfo && fits(size_of::<FILE_BASIC_INFO>()) => {
            let now = count(now());
            let basic = FILE_BASIC_INFO {
                CreationTime: now,
                LastAccessTime: now,
                LastWriteTime: now,
                ChangeTime: now,
                FileAttributes: FILE_ATTRIBUTE_NORMAL,
            };
            // SAFETY: `info` is the caller's, and `size` says it holds the class.
            unsafe { info.cast::<FILE_BASIC_INFO>().write(basic) };
        }
        c if c == FileStandardInfo && fits(size_of::<FILE_STANDARD_INFO>()) => {
            let standard = FILE_STANDARD_INFO {
                AllocationSize: 0,
                EndOfFile: 0,
                NumberOfLinks: 1,
                DeletePending: false,
                Directory: false,
            };
            // SAFETY: as above.
            unsafe { info.cast::<FILE_STANDARD_INFO>().write(standard) };
        }
        c if c == FileAttributeTagInfo && fits(size_of::<FILE_ATTRIBUTE_TAG_INFO>()) => {
            let tag = FILE_ATTRIBUTE_TAG_INFO {
                FileAttributes: FILE_ATTRIBUTE_NORMAL,
                ReparseTag: 0,
            };
            // SAFETY: as above.
            unsafe { info.cast::<FILE_ATTRIBUTE_TAG_INFO>().write(tag) };
        }
        // Any other class: the real answer, a failure.
        _ => return answered,
    }
    1
}

/// `SetFilePointerEx`, answering a move by nothing on a pipe or a device as on an empty
/// file: the position is 0.
///
/// `dd` asks where its output is before it writes, and stopped with "Incorrect function"
/// on `of=/dev/null` and "Invalid input" on a pipe. A real move still fails.
unsafe extern "system" fn set_pointer(
    handle: HANDLE,
    distance: i64,
    position: *mut i64,
    method: SET_FILE_POINTER_MOVE_METHOD,
) -> BOOL {
    // SAFETY: the type kept there; set before the import was pointed here.
    let set: SetFilePointerEx = unsafe { original(&SET_POINTER) };
    // SAFETY: the caller's arguments, passed on unchanged.
    let answered = unsafe { set(handle, distance, position, method) };
    if answered != 0 || distance != 0 || method == FILE_END || !has_no_information(handle) {
        return answered;
    }
    if !position.is_null() {
        // SAFETY: the caller's out-param.
        unsafe { position.write(0) };
    }
    1
}

/// `SetFileInformationByHandle`, answering a truncation to nothing of a pipe or a device
/// as of an empty file: done.
///
/// `dd` truncates its output to where it starts writing, 0, and a pipe or `NUL` cannot
/// be: after the answers above it is an empty file, and that is no error.
unsafe extern "system" fn set_information(
    handle: HANDLE,
    class: FILE_INFO_BY_HANDLE_CLASS,
    info: *const c_void,
    size: u32,
) -> BOOL {
    // SAFETY: the type kept there; set before the import was pointed here.
    let set: SetFileInformationByHandle = unsafe { original(&SET_INFORMATION) };
    // SAFETY: the caller's arguments, passed on unchanged.
    let answered = unsafe { set(handle, class, info, size) };
    let to_nothing = class == FileEndOfFileInfo
        && !info.is_null()
        && usize::try_from(size).is_ok_and(|size| size >= size_of::<FILE_END_OF_FILE_INFO>())
        // SAFETY: the caller's input, of the size it says.
        && unsafe { info.cast::<FILE_END_OF_FILE_INFO>().read_unaligned() }.EndOfFile == 0;
    if answered != 0 || !to_nothing || !has_no_information(handle) {
        return answered;
    }
    1
}

/// `CopyFileExW`, copying from or to a `/dev` name by reading and writing.
///
/// `cp` copies a file with `CopyFileExW`, which opens both paths itself, past the
/// imports: `cp /dev/null f` found no `\dev\null`. Where either is a `/dev` name, the
/// two are opened as above and the one read into the other; the progress routine is
/// not called, and no attributes or times are copied, as a device has none.
unsafe extern "system" fn copy_file(
    from: PCWSTR,
    to: PCWSTR,
    progress: LPPROGRESS_ROUTINE,
    data: *const c_void,
    cancel: *mut BOOL,
    flags: u32,
) -> BOOL {
    // SAFETY: the caller passes valid names, as `CopyFileExW` requires.
    let from_device = unsafe { device_at(from) };
    // SAFETY: as above.
    let to_device = unsafe { device_at(to) };
    if from_device.is_none() && to_device.is_none() {
        // SAFETY: the type kept there; set before the import was pointed here.
        let copy: CopyFileExW = unsafe { original(&COPY_FILE) };
        // SAFETY: the caller's arguments, passed on unchanged.
        return unsafe { copy(from, to, progress, data, cancel, flags) };
    }

    let share = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
    let create = if flags & COPY_FILE_FAIL_IF_EXISTS == 0 {
        CREATE_ALWAYS
    } else {
        CREATE_NEW
    };
    // SAFETY: `from` is a valid name; the rest are constants.
    let source = unsafe {
        create_file(
            from,
            GENERIC_READ,
            share,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if source == INVALID_HANDLE_VALUE {
        return 0;
    }
    // SAFETY: an open handle of this function's own, closed by the file.
    let source = unsafe { File::from_raw_handle(source) };
    // SAFETY: `to` is a valid name; the rest are constants.
    let target = unsafe {
        create_file(
            to,
            GENERIC_WRITE,
            share,
            std::ptr::null(),
            create,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    if target == INVALID_HANDLE_VALUE {
        return 0;
    }
    // SAFETY: as above.
    let target = unsafe { File::from_raw_handle(target) };
    match std::io::copy(&mut &source, &mut &target) {
        Ok(_) => 1,
        Err(error) => {
            let code = error
                .raw_os_error()
                .and_then(|code| u32::try_from(code).ok());
            // SAFETY: always safe.
            unsafe { SetLastError(code.unwrap_or(ERROR_FILE_NOT_FOUND)) };
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_are_known_in_the_forms_a_tool_passes_them() {
        assert_eq!(
            device(r"\\?\C:\dev\stdin"),
            Some(Device::Std(STD_INPUT_HANDLE))
        );
        assert_eq!(device(r"\dev\stdout"), Some(Device::Std(STD_OUTPUT_HANDLE)));
        assert_eq!(device("/dev/fd/2"), Some(Device::Std(STD_ERROR_HANDLE)));
        assert_eq!(device("/dev/fd/0"), Some(Device::Std(STD_INPUT_HANDLE)));
        assert_eq!(device("/dev/fd/3"), Some(Device::Missing));
        assert_eq!(device("/dev/null"), Some(Device::Null));
        assert_eq!(device("/dev/tty"), Some(Device::Tty));
        assert_eq!(device("/dev//./null"), Some(Device::Null));
        assert_eq!(device("/dev/zero"), Some(Device::Endless(Endless::Zeros)));
        assert_eq!(
            device("/dev/urandom"),
            Some(Device::Endless(Endless::Random))
        );
    }

    #[test]
    fn the_names_are_known_on_any_drive_as_the_shell_knows_them() {
        assert_eq!(device("C:/dev/null"), Some(Device::Null));
        assert_eq!(
            device(r"d:\dev\stderr"),
            Some(Device::Std(STD_ERROR_HANDLE))
        );
        assert_eq!(
            device(r"\\?\Q:\dev\fd\1"),
            Some(Device::Std(STD_OUTPUT_HANDLE))
        );
        assert_eq!(device(r"\\server\share\dev\null"), Some(Device::Null));
    }

    #[test]
    fn a_path_elsewhere_is_a_path() {
        assert_eq!(device("C:/src/dev/null"), None);
        assert_eq!(device("dev/null"), None);
        assert_eq!(device("/dev/null/"), None);
        assert_eq!(device(r"\\?\C:\dev\stdin\"), None);
        assert_eq!(device("/dev/sda"), None);
        assert_eq!(device("/dev/null/x"), None);
        assert_eq!(device("/dev/fd/03"), None);
        assert_eq!(device("/dev/fd/+3"), None);
        assert_eq!(device("/DEV/NULL"), None);
        assert_eq!(device(r"\\.\NUL"), None);
        assert_eq!(device("CONIN$"), None);
    }
}
