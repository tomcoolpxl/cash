//! Every open file of every process: the handle walk Sysinternals' handle.exe and System
//! Informer do, for `lsof` (TODO phase 17, 2026-10-04).
//!
//! `NtQuerySystemInformation(SystemExtendedHandleInformation)` lists each handle in the
//! system with its process, value, access and object type. A File handle is copied into
//! cash with `DuplicateHandle` and named with `GetFinalPathNameByHandleW`. A question to a
//! synchronous file object waits for its lock, which a process blocked in a read of a
//! pipe holds for as long as the read lasts; so the questions are asked on a worker
//! thread, and one that has not answered within half a second is left behind while
//! another worker carries on past it.
//!
//! Unelevated, only the processes the user may open are walked, as Linux `lsof` without
//! root. Elevated, the walking thread turns `SeDebugPrivilege` on for itself alone, as
//! handle.exe does for its process, to open other accounts' processes; protected
//! processes stay closed.

use std::collections::{BTreeSet, HashMap};
use std::io;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, FALSE, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_APPEND_DATA, FILE_NAME_NORMALIZED, FILE_READ_DATA, FILE_STANDARD_INFO, FILE_TYPE_DISK,
    FILE_WRITE_DATA, FileStandardInfo, GetFileInformationByHandleEx, GetFileType,
    GetFinalPathNameByHandleW, VOLUME_NAME_DOS,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, PROCESS_DUP_HANDLE};

use crate::handle::open_process;
use crate::sysinfo;

/// `SystemExtendedHandleInformation`.
const EXTENDED_HANDLE_INFORMATION: i32 = 64;

/// How long a worker may take over one handle before it is taken for stuck.
const QUERY_TIMEOUT: Duration = Duration::from_millis(500);

/// Workers started in all, the first and those after stuck ones; past this the rest of the
/// handles go unasked.
const MAX_WORKERS: usize = 8;

/// One `SYSTEM_HANDLE_TABLE_ENTRY_INFO_EX`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Entry {
    object: usize,
    process_id: usize,
    handle_value: usize,
    granted_access: u32,
    creator_back_trace_index: u16,
    object_type_index: u16,
    handle_attributes: u32,
    reserved: u32,
}

/// What a handle is open on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A file on a disk.
    File,
    /// A folder on a disk.
    Directory,
    /// A pipe, named or anonymous.
    Pipe,
    /// A console (`\Device\ConDrv`).
    Console,
    /// The null device, `NUL`.
    Null,
    /// Another device.
    Device,
}

/// A file or folder a process holds open, or a standard handle on whatever it is.
#[derive(Debug)]
pub struct OpenFile {
    /// The process holding it.
    pub pid: u32,
    /// The handle's value in its process, which `lsof` shows as the descriptor.
    pub handle: usize,
    /// Which standard handle it is, 0 to 2, as the process's parameters record them.
    pub std: Option<u8>,
    /// What it is open on.
    pub kind: Kind,
    /// The file or folder; empty for a pipe or a device, which are not named (asking
    /// their name can wait for as long as a read on them lasts).
    pub path: PathBuf,
    /// Opened to read (or, for a folder, to list).
    pub read: bool,
    /// Opened to write or append.
    pub write: bool,
    /// The file's size; `None` for a folder, a device or where Windows would not say.
    pub size: Option<u64>,
}

/// What a walk found, and what it could not look at.
#[derive(Debug, Default)]
pub struct Walk {
    /// The files, by process and handle.
    pub files: Vec<OpenFile>,
    /// Processes holding files that could not be opened: another account's, unelevated,
    /// or protected ones.
    pub unopened: BTreeSet<u32>,
    /// Handles left unanswered because a question to one of them never returned.
    pub abandoned: usize,
}

/// The files open in every process, or in `pids` only.
///
/// # Errors
///
/// When Windows will not list the handles: `NtQuerySystemInformation` is missing or
/// refuses, or no File object type is found in the list.
pub fn open_files(pids: Option<&BTreeSet<u32>>) -> io::Result<Walk> {
    // A thread of its own, so the privilege it turns on is on for nothing else.
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("handle walk".to_owned())
            .spawn_scoped(scope, || {
                let impersonating = debug_privilege_for_this_thread();
                let walk = walk(pids);
                if impersonating {
                    // SAFETY: ends the impersonation this thread started.
                    unsafe { windows_sys::Win32::Security::RevertToSelf() };
                }
                walk
            })?
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("the handle walk failed")))
    })
}

/// Turns `SeDebugPrivilege` on for this thread, when the account holds it (an elevated
/// administrator), through an impersonation token of its own. Whether the thread now
/// impersonates, to be ended with `RevertToSelf`.
fn debug_privilege_for_this_thread() -> bool {
    use windows_sys::Win32::Foundation::LUID;
    use windows_sys::Win32::Security::{
        AdjustTokenPrivileges, ImpersonateSelf, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW,
        SE_PRIVILEGE_ENABLED, SecurityImpersonation, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES,
        TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentThread, OpenThreadToken};

    // SAFETY: gives this thread a copy of the process token; no pointers.
    if unsafe { ImpersonateSelf(SecurityImpersonation) } == 0 {
        return false;
    }
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: no arguments; the pseudo handle of the calling thread.
    let thread = unsafe { GetCurrentThread() };
    // SAFETY: the pseudo handle of this thread, and a valid out-parameter.
    let opened = unsafe {
        OpenThreadToken(
            thread,
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            1,
            &raw mut token,
        )
    };
    if opened == 0 {
        return true;
    }
    // SAFETY: just opened, and closed nowhere else.
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let name = crate::wide::to_wide_nul("SeDebugPrivilege");
    let mut luid = LUID::default();
    // SAFETY: a NUL-terminated name and a valid out-parameter.
    if unsafe { LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &raw mut luid) } == 0 {
        return true;
    }
    let privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    // SAFETY: the token was opened for adjusting; no previous state is asked for. An
    // account without the privilege is refused, which leaves the walk unprivileged.
    unsafe {
        AdjustTokenPrivileges(
            token.as_raw_handle(),
            FALSE,
            &raw const privileges,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
    }
    true
}

/// Every handle in the system.
fn snapshot() -> io::Result<Vec<Entry>> {
    const HEADER: usize = 2 * size_of::<usize>();

    let query = sysinfo::nt_query_system_information()
        .ok_or_else(|| io::Error::other("NtQuerySystemInformation is missing"))?;
    // Handles come and go between asking for the size and asking for the data, so the
    // buffer grows with room to spare. `u64`s, so the entries in it are aligned.
    let mut buffer: Vec<u64> = vec![0; 1 << 19];
    for _ in 0..8 {
        let capacity = u32::try_from(buffer.len() * 8).unwrap_or(u32::MAX);
        let mut needed = 0u32;
        // SAFETY: the buffer holds `capacity` writable bytes and `needed` is a valid
        // out-parameter.
        let status = unsafe {
            query(
                EXTENDED_HANDLE_INFORMATION,
                buffer.as_mut_ptr().cast(),
                capacity,
                &raw mut needed,
            )
        };
        if status == sysinfo::INFO_LENGTH_MISMATCH {
            let bytes = (needed as usize).max(buffer.len() * 8) + (1 << 20);
            buffer.resize(bytes.div_ceil(8), 0);
            continue;
        }
        if status < 0 {
            return Err(io::Error::other(format!(
                "NtQuerySystemInformation refused the handle list: {status:#x}"
            )));
        }
        let count = buffer
            .first()
            .and_then(|&count| usize::try_from(count).ok())
            .unwrap_or(0);
        let room = (buffer.len() * 8).saturating_sub(HEADER) / size_of::<Entry>();
        // The entries follow the two-word header, aligned as the buffer's words are.
        let first = buffer.as_ptr().wrapping_add(HEADER / 8).cast::<Entry>();
        // SAFETY: `count` is capped to the entries the buffer holds past the header.
        let entries = unsafe { std::slice::from_raw_parts(first, count.min(room)) };
        return Ok(entries.to_vec());
    }
    Err(io::Error::other("the handle list kept growing"))
}

/// One File handle to ask about.
struct Job {
    pid: u32,
    handle: usize,
    access: u32,
    /// Which standard handle of its process this is.
    std: Option<u8>,
}

/// Looks up an export of `ntdll.dll`, loaded in every process.
fn ntdll(name: &core::ffi::CStr) -> Option<unsafe extern "system" fn() -> isize> {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

    // SAFETY: ntdll is loaded into every Windows process.
    let module = unsafe { GetModuleHandleA(c"ntdll.dll".as_ptr().cast()) };
    if module.is_null() {
        return None;
    }
    // SAFETY: the module handle is valid and the name is NUL terminated.
    unsafe { GetProcAddress(module, name.as_ptr().cast()) }
}

/// The values of a process's standard input, output and error handles, from its process
/// parameters (`RTL_USER_PROCESS_PARAMETERS`, which `SetStdHandle` writes). The layout is
/// undocumented and has not changed since Windows XP: the parameters at offset 0x20 of the
/// PEB, the three handles at 0x20, 0x28 and 0x30 of the parameters, on 64-bit Windows.
#[cfg(target_pointer_width = "64")]
fn standard_handles(pid: u32) -> Option<[usize; 3]> {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::Toolhelp32ReadProcessMemory;
    use windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION;

    /// `PROCESS_BASIC_INFORMATION`, which windows-sys gates behind its kernel feature.
    #[repr(C)]
    #[derive(Default)]
    #[expect(non_snake_case, reason = "the Windows structure's own field names")]
    struct PROCESS_BASIC_INFORMATION {
        ExitStatus: i32,
        PebBaseAddress: usize,
        AffinityMask: usize,
        BasePriority: i32,
        UniqueProcessId: usize,
        InheritedFromUniqueProcessId: usize,
    }

    type QueryInformationProcess =
        unsafe extern "system" fn(HANDLE, i32, *mut core::ffi::c_void, u32, *mut u32) -> i32;
    let query = ntdll(c"NtQueryInformationProcess")?;
    // SAFETY: the documented signature of NtQueryInformationProcess.
    let query = unsafe {
        std::mem::transmute::<unsafe extern "system" fn() -> isize, QueryInformationProcess>(query)
    };

    let process = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION).ok()?;
    let mut basic = PROCESS_BASIC_INFORMATION::default();
    // SAFETY: `ProcessBasicInformation` (0) fills a PROCESS_BASIC_INFORMATION of the size
    // given; the process was opened for the query.
    let status = unsafe {
        query(
            process.as_raw_handle(),
            0,
            (&raw mut basic).cast(),
            u32::try_from(size_of::<PROCESS_BASIC_INFORMATION>()).ok()?,
            std::ptr::null_mut(),
        )
    };
    if status < 0 || basic.PebBaseAddress == 0 {
        return None;
    }
    let read = |address: usize| -> Option<usize> {
        let mut value = 0usize;
        let mut done = 0usize;
        // SAFETY: reads one pointer-sized value of the other process into `value`.
        let ok = unsafe {
            Toolhelp32ReadProcessMemory(
                pid,
                address as *const core::ffi::c_void,
                (&raw mut value).cast(),
                size_of::<usize>(),
                &raw mut done,
            )
        };
        (ok != 0 && done == size_of::<usize>()).then_some(value)
    };
    let parameters = read(basic.PebBaseAddress + 0x20)?;
    if parameters == 0 {
        return None;
    }
    Some([
        read(parameters + 0x20)?,
        read(parameters + 0x28)?,
        read(parameters + 0x30)?,
    ])
}

#[cfg(not(target_pointer_width = "64"))]
fn standard_handles(_pid: u32) -> Option<[usize; 3]> {
    None
}

/// What device a handle that is not on a disk is open on, by its device type:
/// `FileFsDeviceInformation`, which the I/O manager answers without taking the file
/// object's lock, so it does not wait on a read in progress as a name query would.
fn device_kind(handle: HANDLE) -> Kind {
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;
    use windows_sys::Win32::System::Ioctl::{
        FILE_DEVICE_CONSOLE, FILE_DEVICE_NAMED_PIPE, FILE_DEVICE_NULL,
    };

    /// `FILE_FS_DEVICE_INFORMATION`.
    #[repr(C)]
    #[derive(Default)]
    struct DeviceInformation {
        device_type: u32,
        characteristics: u32,
    }
    type QueryVolumeInformationFile = unsafe extern "system" fn(
        HANDLE,
        *mut IO_STATUS_BLOCK,
        *mut core::ffi::c_void,
        u32,
        i32,
    ) -> i32;

    let Some(query) = ntdll(c"NtQueryVolumeInformationFile") else {
        return Kind::Device;
    };
    // SAFETY: the documented signature of NtQueryVolumeInformationFile.
    let query = unsafe {
        std::mem::transmute::<unsafe extern "system" fn() -> isize, QueryVolumeInformationFile>(
            query,
        )
    };
    let mut status = IO_STATUS_BLOCK::default();
    let mut information = DeviceInformation::default();
    // SAFETY: a valid handle, an IO_STATUS_BLOCK and a buffer of the size given;
    // `FileFsDeviceInformation` is class 4.
    let code = unsafe {
        query(
            handle,
            &raw mut status,
            (&raw mut information).cast(),
            u32::try_from(size_of::<DeviceInformation>()).unwrap_or(0),
            4,
        )
    };
    if code < 0 {
        return Kind::Device;
    }
    match information.device_type {
        FILE_DEVICE_NAMED_PIPE => Kind::Pipe,
        FILE_DEVICE_CONSOLE => Kind::Console,
        FILE_DEVICE_NULL => Kind::Null,
        _ => Kind::Device,
    }
}

enum Message {
    /// A job answered: the file, or `None` for a handle that is not one on a disk.
    Answer(Option<OpenFile>),
    /// A worker found no jobs left.
    Done,
}

fn walk(pids: Option<&BTreeSet<u32>>) -> io::Result<Walk> {
    // The type number of File objects differs between Windows builds: read it off a
    // handle cash opens itself, which the list then holds.
    let probe = std::fs::File::open(r"\\.\NUL")?;
    let entries = snapshot()?;
    let own = (std::process::id() as usize, probe.as_raw_handle() as usize);
    let file_type = entries
        .iter()
        .find(|e| (e.process_id, e.handle_value) == own)
        .map(|e| e.object_type_index)
        .ok_or_else(|| io::Error::other("no File object type in the handle list"))?;
    drop(probe);

    let mut opened: HashMap<u32, Option<OwnedHandle>> = HashMap::new();
    let mut standard: HashMap<u32, Option<[usize; 3]>> = HashMap::new();
    let mut jobs = Vec::new();
    for entry in entries.iter().filter(|e| e.object_type_index == file_type) {
        let Ok(pid) = u32::try_from(entry.process_id) else {
            continue;
        };
        if pid == 0 || pids.is_some_and(|wanted| !wanted.contains(&pid)) {
            continue;
        }
        let process = opened
            .entry(pid)
            .or_insert_with(|| open_process(pid, PROCESS_DUP_HANDLE).ok());
        if process.is_some() {
            let std = standard
                .entry(pid)
                .or_insert_with(|| standard_handles(pid))
                .and_then(|handles| handles.iter().position(|&h| h == entry.handle_value))
                .and_then(|index| u8::try_from(index).ok());
            jobs.push(Job {
                pid,
                handle: entry.handle_value,
                access: entry.granted_access,
                std,
            });
        }
    }
    let unopened: BTreeSet<u32> = opened
        .iter()
        .filter(|(_, process)| process.is_none())
        .map(|(&pid, _)| pid)
        .collect();
    let processes: Arc<HashMap<u32, OwnedHandle>> = Arc::new(
        opened
            .into_iter()
            .filter_map(|(pid, process)| process.map(|p| (pid, p)))
            .collect(),
    );

    let jobs = Arc::new(jobs);
    let next = Arc::new(AtomicUsize::new(0));
    let (sender, receiver) = mpsc::channel();
    let start = || {
        let (jobs, next, processes, sender) = (
            Arc::clone(&jobs),
            Arc::clone(&next),
            Arc::clone(&processes),
            sender.clone(),
        );
        std::thread::Builder::new()
            .name("handle names".to_owned())
            .spawn(move || work(&jobs, &next, &processes, &sender))
            .is_ok()
    };

    let mut walk = Walk {
        unopened,
        ..Walk::default()
    };
    let (mut workers, mut done) = (usize::from(start()), 0);
    while workers > done {
        match receiver.recv_timeout(QUERY_TIMEOUT) {
            Ok(Message::Answer(file)) => walk.files.extend(file),
            Ok(Message::Done) => done += 1,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // A worker is stuck on a handle. It keeps the handle and the processes,
                // and ends by itself if the question ever returns.
                walk.abandoned += 1;
                if next.load(Ordering::SeqCst) >= jobs.len() || workers >= MAX_WORKERS {
                    break;
                }
                if start() {
                    workers += 1;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    walk.abandoned += jobs.len().saturating_sub(next.load(Ordering::SeqCst));
    walk.files.sort_by_key(|f| (f.pid, f.handle));
    Ok(walk)
}

fn work(
    jobs: &[Job],
    next: &AtomicUsize,
    processes: &HashMap<u32, OwnedHandle>,
    sender: &mpsc::Sender<Message>,
) {
    while let Some(job) = jobs.get(next.fetch_add(1, Ordering::SeqCst)) {
        let file = processes.get(&job.pid).and_then(|p| ask(p, job));
        if sender.send(Message::Answer(file)).is_err() {
            return;
        }
    }
    let _ = sender.send(Message::Done);
}

/// Copies the job's handle into cash and asks what it is open on.
fn ask(process: &OwnedHandle, job: &Job) -> Option<OpenFile> {
    let mut copy: HANDLE = std::ptr::null_mut();
    // SAFETY: no arguments; the pseudo handle of this process.
    let current = unsafe { GetCurrentProcess() };
    // SAFETY: the process was opened for duplicating; the copy lands in this process
    // with the same access, and `copy` is a valid out-parameter.
    let ok = unsafe {
        DuplicateHandle(
            process.as_raw_handle(),
            job.handle as HANDLE,
            current,
            &raw mut copy,
            0,
            FALSE,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if ok == 0 {
        return None;
    }
    // SAFETY: just duplicated into this process, and closed nowhere else.
    let copy = unsafe { OwnedHandle::from_raw_handle(copy) };
    let raw = copy.as_raw_handle();
    let read = job.access & FILE_READ_DATA != 0;
    let write = job.access & (FILE_WRITE_DATA | FILE_APPEND_DATA) != 0;
    // Pipes, consoles and other devices have no path to give; a standard handle on one
    // is listed by its kind.
    // SAFETY: a valid handle.
    if unsafe { GetFileType(raw) } != FILE_TYPE_DISK {
        return job.std.map(|std| OpenFile {
            pid: job.pid,
            handle: job.handle,
            std: Some(std),
            kind: device_kind(raw),
            path: PathBuf::new(),
            read,
            write,
            size: None,
        });
    }
    let path = final_path(raw)?;
    let mut info = FILE_STANDARD_INFO::default();
    // SAFETY: a valid handle and a buffer of the size given.
    let standard = unsafe {
        GetFileInformationByHandleEx(
            raw,
            FileStandardInfo,
            (&raw mut info).cast(),
            u32::try_from(size_of::<FILE_STANDARD_INFO>()).unwrap_or(0),
        )
    } != 0;
    let directory = standard && info.Directory;
    Some(OpenFile {
        pid: job.pid,
        handle: job.handle,
        std: job.std,
        kind: if directory {
            Kind::Directory
        } else {
            Kind::File
        },
        path,
        read,
        write,
        size: (standard && !directory)
            .then(|| u64::try_from(info.EndOfFile).ok())
            .flatten(),
    })
}

/// The path a handle is open on, with a drive letter, or `\\server\share` for a network
/// file; `None` for a volume without a letter.
fn final_path(handle: HANDLE) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt as _;

    let mut buffer = vec![0u16; 32_768];
    let capacity = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    // SAFETY: a valid handle and a buffer of `capacity` UTF-16 units.
    let length = unsafe {
        GetFinalPathNameByHandleW(
            handle,
            buffer.as_mut_ptr(),
            capacity,
            FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
        )
    } as usize;
    if length == 0 || length >= buffer.len() {
        return None;
    }
    buffer.truncate(length);
    let text = std::ffi::OsString::from_wide(&buffer);
    let text = text.to_string_lossy();
    let plain = if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
    };
    Some(PathBuf::from(plain))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_walk_finds_a_file_this_process_holds_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("held.txt");
        std::fs::write(&path, b"12345").unwrap();
        let held = std::fs::File::open(&path).unwrap();
        let me = BTreeSet::from([std::process::id()]);
        let walk = open_files(Some(&me)).unwrap();
        let found = walk
            .files
            .iter()
            .find(|f| f.handle == held.as_raw_handle() as usize);
        assert!(found.is_some(), "not found among {:?}", walk.files);
        let found = found.unwrap();
        assert_eq!(
            (
                found.pid,
                found.read,
                found.write,
                found.kind,
                found.std,
                found.size
            ),
            (std::process::id(), true, false, Kind::File, None, Some(5))
        );
        assert!(
            crate::fold::same_name(
                &found.path.to_string_lossy(),
                std::fs::canonicalize(&path)
                    .unwrap()
                    .to_string_lossy()
                    .trim_start_matches(r"\\?\")
            ),
            "{:?} is not {path:?}",
            found.path
        );
        assert!(walk.unopened.is_empty());
    }

    #[test]
    fn the_standard_handles_are_marked_whatever_they_are_on() {
        use windows_sys::Win32::System::Console::{
            GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        };

        let me = BTreeSet::from([std::process::id()]);
        let walk = open_files(Some(&me)).unwrap();
        for (index, which) in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE]
            .into_iter()
            .enumerate()
        {
            // SAFETY: no pointers; returns this process's handle or null.
            let value = unsafe { GetStdHandle(which) } as usize;
            if value == 0 || value == usize::MAX {
                continue;
            }
            let marked = walk
                .files
                .iter()
                .find(|f| f.std == u8::try_from(index).ok());
            assert!(
                marked.is_some_and(|f| f.handle == value),
                "standard handle {index} ({value:#x}) not marked among {:?}",
                walk.files
            );
        }
    }
}
