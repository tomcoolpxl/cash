//! Process queries that the job-object layer and D42's elevated-child tracking need.

use windows_sys::Win32::Foundation::{CloseHandle, FALSE, FILETIME};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// `GetExitCodeProcess` reports this while a process is still running.
const STILL_ACTIVE: u32 = 259;

/// Whether a process ID currently refers to a running process.
///
/// Used to verify job teardown, and by D42 to check whether a tracked elevated child —
/// which cannot be assigned to cash's job object — is still alive at exit.
///
/// PIDs are reused by Windows, so a `true` result means "some process with this id is
/// running", not necessarily the one you started. Callers that need certainty should
/// hold a handle instead.
#[must_use]
pub fn is_pid_alive(pid: u32) -> bool {
    // SAFETY: OpenProcess returns null rather than a bad handle on failure.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) };
    if handle.is_null() {
        return false;
    }

    let mut code: u32 = 0;
    // SAFETY: handle is valid here, and `code` is a valid out-param.
    let ok = unsafe { GetExitCodeProcess(handle, &raw mut code) };
    // SAFETY: closing a handle we just opened, exactly once.
    unsafe {
        CloseHandle(handle);
    }

    ok != 0 && code == STILL_ACTIVE
}

/// Total CPU time a process has consumed, in 100-nanosecond units.
///
/// The honest way to verify D19's suspend actually works: a suspended process stops
/// accruing CPU time immediately and deterministically, whereas observing side effects
/// like file writes is slow and racy.
///
/// Returns `None` if the process cannot be opened, which usually means it has exited.
#[must_use]
pub fn cpu_time(pid: u32) -> Option<u64> {
    // SAFETY: OpenProcess returns null rather than a bad handle on failure.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) };
    if handle.is_null() {
        return None;
    }

    let mut creation = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exit = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut kernel = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut user = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };

    // SAFETY: handle is valid and all four out-params are valid FILETIMEs.
    let ok = unsafe {
        GetProcessTimes(
            handle,
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    };
    // SAFETY: closing a handle we just opened, exactly once.
    unsafe {
        CloseHandle(handle);
    }

    if ok == 0 {
        return None;
    }

    Some(as_u64(kernel) + as_u64(user))
}

/// Combine a `FILETIME`'s halves into a single count of 100ns intervals.
const fn as_u64(time: FILETIME) -> u64 {
    ((time.dwHighDateTime as u64) << 32) | (time.dwLowDateTime as u64)
}

/// One row of a process listing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessInfo {
    /// The Windows process id — the one `kill` takes.
    pub pid: u32,
    /// The parent's process id. Reused by Windows, so it may name a process that has
    /// since exited and been replaced; a listing cannot tell the difference.
    pub parent_pid: u32,
    /// Windows base scheduling priority for threads created by this process.
    pub base_priority: i32,
    /// The executable's file name, without a directory.
    pub name: String,
}

/// Every process on the machine that this user can see.
///
/// The basis for a `ps` that reports Windows process ids. The `ps` on `PATH` here is
/// almost always the MSYS one from Git for Windows, which reports *MSYS* pids for *MSYS*
/// processes only — so it omits every native program, and the numbers it does print
/// cannot be handed to `kill`. Under cash that is worse than having no `ps` at all,
/// because it looks like it worked.
///
/// Ordered by pid, so successive runs are comparable and `ps | head` is meaningful.
#[must_use]
pub fn list() -> Vec<ProcessInfo> {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    // SAFETY: TH32CS_SNAPPROCESS ignores the pid argument and snapshots every process.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot.is_null() {
        return Vec::new();
    }

    // SAFETY: `PROCESSENTRY32W` is plain old data — integers and a fixed-size UTF-16
    // buffer — for which an all-zero bit pattern is valid. `dwSize` is set immediately
    // below, which is the only field the API requires before the first call.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    // The structure is a fixed ~556 bytes and `dwSize` is a `u32` by ABI, so the
    // saturating fallback is unreachable.
    entry.dwSize = u32::try_from(size_of::<PROCESSENTRY32W>()).unwrap_or(u32::MAX);

    let mut processes = Vec::new();

    // SAFETY: `entry` is correctly sized and the snapshot handle is valid.
    let mut ok = unsafe { Process32FirstW(snapshot, &raw mut entry) };
    while ok != 0 {
        processes.push(ProcessInfo {
            pid: entry.th32ProcessID,
            parent_pid: entry.th32ParentProcessID,
            base_priority: entry.pcPriClassBase,
            name: exe_name(&entry.szExeFile),
        });
        // SAFETY: as above.
        ok = unsafe { Process32NextW(snapshot, &raw mut entry) };
    }

    // SAFETY: closing the snapshot handle, exactly once.
    unsafe { CloseHandle(snapshot) };

    processes.sort_by_key(|p| p.pid);
    processes
}

/// Decode the fixed-size, NUL-terminated UTF-16 name `PROCESSENTRY32W` carries.
fn exe_name(raw: &[u16]) -> String {
    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
    String::from_utf16_lossy(&raw[..end])
}

/// The descendants of `root`, including `root` itself.
///
/// Built from the parent links in [`list`], so it reflects the process tree as Windows
/// currently reports it. A `ps` limited to the shell's own descendants is the closest
/// honest answer to what `ps` with no arguments means on Linux — "the processes attached
/// to my terminal" — since Windows has no controlling terminal to filter by.
#[must_use]
pub fn descendants(root: u32) -> Vec<ProcessInfo> {
    let all = list();

    let mut wanted: std::collections::HashSet<u32> = std::collections::HashSet::new();
    wanted.insert(root);

    // Parents always have a lower pid than their children often enough that one pass is
    // not sufficient; iterate until the set stops growing. Bounded by the process count,
    // so it terminates even if the parent links contain a cycle from pid reuse.
    for _ in 0..all.len() {
        let before = wanted.len();
        for process in &all {
            if wanted.contains(&process.parent_pid) {
                wanted.insert(process.pid);
            }
        }
        if wanted.len() == before {
            break;
        }
    }

    all.into_iter()
        .filter(|p| wanted.contains(&p.pid))
        .collect()
}

/// What a `ps`-style listing wants beyond a pid, a parent and a name.
///
/// Every field is optional because a process may refuse to open: anything running as
/// another user, and most of what the system itself runs, cannot be queried by an
/// unelevated shell. A listing that dropped those rows would be lying about what is on
/// the machine, so they are listed with the fields left empty instead.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProcessDetails {
    /// The account the process runs as, without its domain — `ps`'s `USER`.
    pub user: Option<String>,
    /// When it started, in 100ns units since 1601 — a `FILETIME`, for `START`/`STIME`.
    pub started: Option<u64>,
    /// Kernel plus user CPU time, in the same units — `TIME`.
    pub cpu: Option<u64>,
    /// Working set in bytes: the closest thing Windows has to `RSS`.
    pub resident: Option<u64>,
    /// Resident bytes not private to the process, when the host supports the extended
    /// working-set counter.
    pub shared_resident: Option<u64>,
    /// Commit charge in bytes: the closest thing Windows has to `VSZ`.
    pub committed: Option<u64>,
}

/// Looks up the per-process detail a `ps aux` or `ps -ef` row needs.
///
/// One `OpenProcess` for the lot, because opening a process is the expensive part and a
/// listing does this for every row.
#[must_use]
pub fn details(pid: u32) -> ProcessDetails {
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
    };

    let mut details = ProcessDetails::default();

    // SAFETY: OpenProcess returns null rather than a bad handle on failure.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) };
    if handle.is_null() {
        return details;
    }

    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut creation, mut exit, mut kernel, mut user) = (zero, zero, zero, zero);

    // SAFETY: handle is valid and all four out-params are valid FILETIMEs.
    let ok = unsafe {
        GetProcessTimes(
            handle,
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    };
    if ok != 0 {
        details.started = Some(as_u64(creation));
        details.cpu = Some(as_u64(kernel) + as_u64(user));
    }

    // Windows 10 22H2 and newer can give private working-set bytes in the extended
    // structure. Ask for that first; older supported Windows releases reject its size,
    // in which case the established base query still supplies RES and VIRT.
    let mut extended = PROCESS_MEMORY_COUNTERS_EX2 {
        cb: u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS_EX2>()).unwrap_or(u32::MAX),
        ..Default::default()
    };
    // SAFETY: EX2 begins with the complete base structure and `cb` advertises its size.
    let extended_ok = unsafe {
        K32GetProcessMemoryInfo(
            handle,
            (&raw mut extended).cast::<PROCESS_MEMORY_COUNTERS>(),
            extended.cb,
        )
    };
    if extended_ok != 0 {
        details.resident = Some(extended.WorkingSetSize as u64);
        details.committed = Some(extended.PrivateUsage as u64);
        if extended.PrivateWorkingSetSize != 0 {
            details.shared_resident = Some(
                (extended.WorkingSetSize as u64)
                    .saturating_sub(extended.PrivateWorkingSetSize as u64),
            );
        }
    } else {
        let mut counters = PROCESS_MEMORY_COUNTERS {
            cb: u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS>()).unwrap_or(u32::MAX),
            ..Default::default()
        };
        // SAFETY: handle is valid, and `counters` is correctly sized.
        if unsafe { K32GetProcessMemoryInfo(handle, &raw mut counters, counters.cb) } != 0 {
            details.resident = Some(counters.WorkingSetSize as u64);
            details.committed = Some(counters.PagefileUsage as u64);
        }
    }

    details.user = token_user(handle);

    // SAFETY: closing a handle we just opened, exactly once.
    unsafe {
        CloseHandle(handle);
    }

    details
}

/// The account running the current process.
#[must_use]
pub fn current_process_user() -> Option<String> {
    // SAFETY: GetCurrentProcess returns a pseudo-handle for the current process.
    let current = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() };
    token_user(current)
}

/// The account a process's token names, without its domain.
fn token_user(process: windows_sys::Win32::Foundation::HANDLE) -> Option<String> {
    use windows_sys::Win32::Security::{
        GetTokenInformation, LookupAccountSidW, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER, TokenUser,
    };
    use windows_sys::Win32::System::Threading::OpenProcessToken;

    let mut token = std::ptr::null_mut();
    // SAFETY: `process` is a live handle and `token` is a valid out-param.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) } == 0 {
        return None;
    }

    // Ask for the size first: a TOKEN_USER carries a variable-length SID after it.
    let mut needed: u32 = 0;
    // SAFETY: a null buffer with a zero length is the documented way to ask for the size.
    unsafe {
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &raw mut needed);
    }

    // A `TOKEN_USER` starts with a pointer, so the buffer it is read into has to be
    // pointer-aligned — which a `Vec<u8>` is not required to be. Asking for `u64`s gets
    // the alignment from the allocator rather than from luck.
    let words = (needed as usize).div_ceil(size_of::<u64>()).max(1);
    let mut buffer = vec![0u64; words];
    // SAFETY: the buffer is at least the size the call above asked for.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &raw mut needed,
        )
    };
    // SAFETY: closing a handle we just opened, exactly once.
    unsafe {
        CloseHandle(token);
    }

    if ok == 0 || words * size_of::<u64>() < size_of::<TOKEN_USER>() {
        return None;
    }

    // SAFETY: on success the buffer holds a TOKEN_USER followed by the SID it points at,
    // it is aligned for one by construction, and it outlives this borrow.
    let user: &TOKEN_USER = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };

    let mut name = [0u16; 256];
    let mut name_len = u32::try_from(name.len()).unwrap_or(u32::MAX);
    let mut domain = [0u16; 256];
    let mut domain_len = u32::try_from(domain.len()).unwrap_or(u32::MAX);
    let mut kind: SID_NAME_USE = 0;

    // SAFETY: the SID comes from the token, and both buffers are sized by their lengths.
    let ok = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            user.User.Sid,
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

    Some(String::from_utf16_lossy(&name[..name_len as usize]))
}

/// The machine's name, spelled the way Windows spells it.
///
/// cash: there are two names for one machine here. The DNS hostname API — which the
/// `hostname` crate, .NET's `Dns.GetHostName` and uutils all use — answers
/// `desktop-tomc`, while `%COMPUTERNAME%`, the domain half of `id -un` and every Windows
/// tool answer `DESKTOP-TOMC`. A shell that uses both has its own machine under two
/// names, so `[ "$(hostname)" = "$COMPUTERNAME" ]` is false. This is the spelling the
/// rest of Windows agrees on, and cash reports it everywhere.
#[must_use]
pub fn computer_name() -> Option<String> {
    use windows_sys::Win32::System::SystemInformation::{ComputerNameNetBIOS, GetComputerNameExW};

    // A NetBIOS name is at most 15 characters, but ask for room and let the API say.
    let mut buffer = [0u16; 256];
    let mut size = u32::try_from(buffer.len()).unwrap_or(u32::MAX);

    // SAFETY: the buffer and its length are handed over together, and the call writes at
    // most `size` UTF-16 units into it.
    let ok = unsafe { GetComputerNameExW(ComputerNameNetBIOS, buffer.as_mut_ptr(), &raw mut size) };
    if ok == 0 {
        return None;
    }

    let name = String::from_utf16_lossy(&buffer[..size as usize]);
    if name.is_empty() { None } else { Some(name) }
}

/// The machine's DNS hostname, as returned by the DNS subsystem (often lowercased).
#[must_use]
pub fn dns_hostname() -> Option<String> {
    use windows_sys::Win32::System::SystemInformation::{
        ComputerNameDnsHostname, GetComputerNameExW,
    };

    let mut buffer = [0u16; 256];
    let mut size = u32::try_from(buffer.len()).unwrap_or(u32::MAX);

    // SAFETY: the buffer and its length are handed over together, and the call writes at
    // most `size` UTF-16 units into it.
    let ok =
        unsafe { GetComputerNameExW(ComputerNameDnsHostname, buffer.as_mut_ptr(), &raw mut size) };
    if ok == 0 {
        return None;
    }

    let name = String::from_utf16_lossy(&buffer[..size as usize]);
    if name.is_empty() { None } else { Some(name) }
}

/// The machine's DNS domain, if it is in one.
///
/// Empty on a workgroup machine, which is most of them; `hostname -f` then has nothing to
/// append and reports the bare name, as it does on a Linux box with no domain.
#[must_use]
pub fn dns_domain() -> Option<String> {
    use windows_sys::Win32::System::SystemInformation::{
        ComputerNameDnsDomain, GetComputerNameExW,
    };

    let mut buffer = [0u16; 256];
    let mut size = u32::try_from(buffer.len()).unwrap_or(u32::MAX);

    // SAFETY: the buffer and its length are handed over together, and the call writes at
    // most `size` UTF-16 units into it.
    let ok =
        unsafe { GetComputerNameExW(ComputerNameDnsDomain, buffer.as_mut_ptr(), &raw mut size) };
    if ok == 0 {
        return None;
    }

    let domain = String::from_utf16_lossy(&buffer[..size as usize]);
    if domain.is_empty() {
        None
    } else {
        Some(domain)
    }
}

/// Total physical memory in bytes, for the `%MEM` column and `free`.
#[must_use]
pub fn total_physical_memory() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    // SAFETY: `MEMORYSTATUSEX` is plain old data; `dwLength` is set as the API requires.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = u32::try_from(size_of::<MEMORYSTATUSEX>()).unwrap_or(u32::MAX);

    // SAFETY: `status` is correctly sized.
    if unsafe { GlobalMemoryStatusEx(&raw mut status) } == 0 {
        return None;
    }

    Some(status.ullTotalPhys)
}

/// Physical memory currently available, in bytes.
#[must_use]
pub fn available_physical_memory() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    // SAFETY: as above.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = u32::try_from(size_of::<MEMORYSTATUSEX>()).unwrap_or(u32::MAX);

    // SAFETY: as above.
    if unsafe { GlobalMemoryStatusEx(&raw mut status) } == 0 {
        return None;
    }

    Some(status.ullAvailPhys)
}

/// The current time as a `FILETIME` count, so elapsed time can be measured against
/// [`ProcessDetails::started`] without leaving these units.
#[must_use]
pub fn now_filetime() -> u64 {
    use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;

    let mut now = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    // SAFETY: `now` is a valid out-param.
    unsafe {
        GetSystemTimeAsFileTime(&raw mut now);
    }

    as_u64(now)
}

/// Terminate a process immediately (`kill -9`, D21).
///
/// Uncatchable, as `SIGKILL` is on POSIX. `TerminateProcess` runs no cleanup in the
/// target, which is the point: this is what you reach for when asking politely has
/// already failed.
pub fn terminate(pid: u32) -> std::io::Result<()> {
    use windows_sys::Win32::System::Threading::{PROCESS_TERMINATE, TerminateProcess};

    // SAFETY: OpenProcess returns null rather than a bad handle on failure.
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, FALSE, pid) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error());
    }

    // SAFETY: handle is valid and carries PROCESS_TERMINATE.
    let ok = unsafe { TerminateProcess(handle, 1) };
    // SAFETY: closing a handle we just opened, exactly once.
    unsafe {
        CloseHandle(handle);
    }

    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Resume a process created suspended, resuming all its threads.
///
/// Uses NT native API `NtResumeProcess` from `ntdll.dll`.
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "process is an opaque Win32 kernel handle, not a dereferenceable memory pointer"
)]
pub fn resume_process(process: windows_sys::Win32::Foundation::HANDLE) -> std::io::Result<()> {
    use std::sync::LazyLock;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

    type NtResumeProcessFn = unsafe extern "system" fn(process_handle: HANDLE) -> i32;

    static NT_RESUME: LazyLock<Option<NtResumeProcessFn>> = LazyLock::new(|| {
        // SAFETY: ntdll is guaranteed to be loaded into every Windows process.
        let ntdll = unsafe { GetModuleHandleA(c"ntdll.dll".as_ptr().cast()) };
        if ntdll.is_null() {
            return None;
        }
        // SAFETY: ntdll handle is valid.
        let proc = unsafe { GetProcAddress(ntdll, c"NtResumeProcess".as_ptr().cast()) };
        proc.map(|p| {
            // SAFETY: NtResumeProcess signature matches NtResumeProcessFn.
            unsafe { std::mem::transmute::<_, NtResumeProcessFn>(p) }
        })
    });

    if let Some(nt_resume) = *NT_RESUME {
        // SAFETY: handle is valid.
        let status = unsafe { nt_resume(process) };
        if status >= 0 {
            Ok(())
        } else {
            Err(std::io::Error::from_raw_os_error(status))
        }
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "NtResumeProcess not found in ntdll.dll",
        ))
    }
}

/// Opens a process with `access`, closing the handle when dropped.
struct ProcessHandle(windows_sys::Win32::Foundation::HANDLE);

impl ProcessHandle {
    fn open(pid: u32, access: u32) -> Option<Self> {
        // SAFETY: OpenProcess returns null rather than a bad handle on failure.
        let handle = unsafe { OpenProcess(access, FALSE, pid) };
        (!handle.is_null()).then_some(Self(handle))
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: the handle is valid and closed exactly once.
        unsafe { CloseHandle(self.0) };
    }
}

/// The full path of a process's executable, if this user may query it.
///
/// Uses `QueryFullProcessImageNameW`, which needs only limited query rights, so it works
/// for most processes of other users too.
#[must_use]
pub fn image_path(pid: u32) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Threading::{PROCESS_NAME_WIN32, QueryFullProcessImageNameW};

    let process = ProcessHandle::open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let mut buffer = vec![0u16; 32_768];
    let mut size = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    // SAFETY: the handle is valid, and `buffer` has `size` writable UTF-16 units.
    let ok = unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &raw mut size,
        )
    };
    (ok != 0).then(|| {
        buffer.truncate(size as usize);
        std::ffi::OsString::from_wide(&buffer).into()
    })
}

/// The files mapped into a process as modules: its executable and every loaded DLL.
///
/// `None` when the process cannot be opened for reading (other users' processes and
/// protected ones, without elevation); the caller then leaves those rows out.
#[must_use]
pub fn modules(pid: u32) -> Option<Vec<std::path::PathBuf>> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::HMODULE;
    use windows_sys::Win32::System::ProcessStatus::{
        K32EnumProcessModulesEx, K32GetModuleFileNameExW, LIST_MODULES_ALL,
    };
    use windows_sys::Win32::System::Threading::{PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

    let process = ProcessHandle::open(pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ)?;
    let mut handles: Vec<HMODULE> = vec![std::ptr::null_mut(); 256];
    // Modules can load between the calls, so grow and retry a few times.
    for _ in 0..4 {
        let bytes = u32::try_from(handles.len() * size_of::<HMODULE>()).unwrap_or(u32::MAX);
        let mut needed = 0u32;
        // SAFETY: `handles` has `bytes` writable bytes; `needed` is writable.
        let ok = unsafe {
            K32EnumProcessModulesEx(
                process.0,
                handles.as_mut_ptr(),
                bytes,
                &raw mut needed,
                LIST_MODULES_ALL,
            )
        };
        if ok == 0 {
            return None;
        }
        let count = needed as usize / size_of::<HMODULE>();
        if count <= handles.len() {
            handles.truncate(count);
            break;
        }
        handles = vec![std::ptr::null_mut(); count + 32];
    }

    let mut name = vec![0u16; 32_768];
    let size = u32::try_from(name.len()).unwrap_or(u32::MAX);
    Some(
        handles
            .iter()
            .filter_map(|&module| {
                // SAFETY: the handle is valid, `module` came from the enumeration, and
                // `name` has `size` writable units.
                let length =
                    unsafe { K32GetModuleFileNameExW(process.0, module, name.as_mut_ptr(), size) };
                (length > 0).then(|| std::ffi::OsString::from_wide(&name[..length as usize]).into())
            })
            .collect(),
    )
}

#[cfg(test)]
mod module_tests {
    use super::*;

    #[test]
    fn this_process_has_an_image_and_modules() {
        let image = image_path(std::process::id()).unwrap();
        assert_eq!(image, std::env::current_exe().unwrap());
        let modules = modules(std::process::id()).unwrap();
        assert!(modules.iter().any(|m| m == &image), "{modules:?}");
        assert!(
            modules.iter().any(|m| m
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with("kernel32.dll")),
            "{modules:?}"
        );
    }

    #[test]
    fn a_missing_process_has_neither() {
        // Process ids are multiples of four, so this one never exists.
        assert_eq!(image_path(3), None);
        assert_eq!(modules(3), None);
    }
}
