//! Process queries that the job-object layer and D42's elevated-child tracking need.

use std::os::windows::ffi::OsStringExt as _;

use windows_sys::Win32::Foundation::{
    CloseHandle, FALSE, FILETIME, GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessTimes, INFINITE, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

/// `GetExitCodeProcess` reports this while a process is still running.
const STILL_ACTIVE: u32 = 259;

/// A process held open, so that its pid stays its own.
///
/// Windows hands a pid out again soon after its process ends: within a second on a busy
/// machine (measured 2026-09-30, see `crates/cash/tests/it/process_identity.rs`). It does
/// not while a handle to the process is open. So for as long as this is held, its pid
/// names this process and no other, running or ended, and whatever asks by pid (`kill -0`,
/// a signal, [`is_pid_alive`]) gets this process's answer. It is what a zombie is on
/// Linux, where a child's pid stays reserved until its parent has reaped it.
///
/// An ended process that is held is not listed (by [`list`], `ps` or Task Manager), and
/// its executable is not kept open: it can be replaced or deleted (checked 2026-09-30).
#[derive(Debug)]
pub struct Held {
    handle: HANDLE,
    pid: u32,
}

// SAFETY: the handle is an index into a process-wide table with no thread affinity, the
// calls made through it (`WaitForSingleObject`, `GetProcessTimes`, `CloseHandle`) are
// thread-safe, and `Drop` closes it exactly once because `Held` is not `Clone`.
unsafe impl Send for Held {}
// SAFETY: as above; shared references only ever reach thread-safe Win32 calls.
unsafe impl Sync for Held {}

impl Held {
    /// Holds the process that has `pid` now, or `None` if there is none or it may not be
    /// opened.
    ///
    /// Which process that is, is the caller's to know: one it has just started, or one it
    /// checks with [`Self::started_by`].
    #[must_use]
    pub fn open(pid: u32) -> Option<Self> {
        let access = PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION;
        // SAFETY: OpenProcess returns null rather than a bad handle on failure.
        let handle = unsafe { OpenProcess(access, FALSE, pid) };
        (!handle.is_null()).then_some(Self { handle, pid })
    }

    /// The process's id.
    #[must_use]
    pub const fn pid(&self) -> u32 {
        self.pid
    }

    /// Whether the process is still running. Asked of the process, not of its exit
    /// status: a process may exit with 259, the status that means "still running".
    #[must_use]
    pub fn is_running(&self) -> bool {
        // SAFETY: the handle is valid and was opened with SYNCHRONIZE.
        unsafe { WaitForSingleObject(self.handle, 0) == WAIT_TIMEOUT }
    }

    /// Waits until the process has ended.
    pub fn wait(&self) {
        // SAFETY: the handle is valid and was opened with SYNCHRONIZE.
        unsafe { WaitForSingleObject(self.handle, INFINITE) };
    }

    /// When the process started, as a `FILETIME` count.
    #[must_use]
    pub fn started(&self) -> Option<u64> {
        creation_time(self.handle)
    }

    /// Whether the process had started by `seen`, a [`now_filetime`] count: whether it
    /// can be the process that had this pid then.
    ///
    /// A pid is handed out again only after its process has ended, so a process that
    /// started after the pid was seen is another one. One whose start time cannot be
    /// read is taken at its word.
    #[must_use]
    pub fn started_by(&self, seen: u64) -> bool {
        self.started().is_none_or(|at| at <= seen)
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        // SAFETY: the handle is valid and closed exactly once.
        unsafe { CloseHandle(self.handle) };
    }
}

/// When the process behind `handle` started, as a `FILETIME` count.
fn creation_time(handle: HANDLE) -> Option<u64> {
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut creation, mut exit, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: the handle is valid and all four out-params are valid FILETIMEs.
    let ok = unsafe {
        GetProcessTimes(
            handle,
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    };
    (ok != 0).then(|| as_u64(creation))
}

/// Whether a process ID currently refers to a running process.
///
/// Used to verify job teardown, and by D42 to check whether a tracked elevated child —
/// which cannot be assigned to cash's job object — is still alive at exit.
///
/// PIDs are reused by Windows, so a `true` result means "some process with this id is
/// running", not necessarily the one you started. It is that one only while a handle to
/// it is open: hold one ([`Held`], or [`crate::children`] for a background job's).
#[must_use]
pub fn is_pid_alive(pid: u32) -> bool {
    // Asked of the process where Windows lets this user wait on it. Its exit status is
    // no answer: a process may exit with 259, which would read as running for as long
    // as anything held it open.
    if let Some(process) = Held::open(pid) {
        return process.is_running();
    }

    // A process that may be queried but not waited on: its exit status is all there is.
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

/// When a process started, as a `FILETIME` count, or `None` if it cannot be opened.
#[must_use]
pub fn started(pid: u32) -> Option<u64> {
    let process = ProcessHandle::open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    creation_time(process.0)
}

/// Whether `child` can really be a child of the process `parent_pid`, which started at
/// `parent_started`.
///
/// Windows keeps an orphan's parent pid after the parent exits, and reuses pids, so a
/// process can name as its parent a pid that now belongs to a newer, unrelated process.
/// A child cannot predate its parent; one that does is such an orphan. When either start
/// time is unknown there is nothing to go on, and the parent link is taken at its word.
#[must_use]
pub fn is_child_of(child: &ProcessInfo, parent_pid: u32, parent_started: Option<u64>) -> bool {
    if child.parent_pid != parent_pid {
        return false;
    }
    match (parent_started, started(child.pid)) {
        (Some(parent), Some(child)) => child >= parent,
        _ => true,
    }
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
    if snapshot == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
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

    // Each member's start time, so an orphan whose parent pid was reused by a member is
    // not mistaken for its child (see `is_child_of`).
    let mut wanted: std::collections::HashMap<u32, Option<u64>> = std::collections::HashMap::new();
    wanted.insert(root, started(root));

    // Parents always have a lower pid than their children often enough that one pass is
    // not sufficient; iterate until the set stops growing. Bounded by the process count,
    // so it terminates even if the parent links contain a cycle from pid reuse.
    for _ in 0..all.len() {
        let before = wanted.len();
        for process in &all {
            if wanted.contains_key(&process.pid) {
                continue;
            }
            if let Some(&parent_started) = wanted.get(&process.parent_pid)
                && is_child_of(process, process.parent_pid, parent_started)
            {
                wanted.insert(process.pid, started(process.pid));
            }
        }
        if wanted.len() == before {
            break;
        }
    }

    all.into_iter()
        .filter(|p| wanted.contains_key(&p.pid))
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
    details_of(pid, true)
}

/// [`details`] without the account, which a refresh of `top` has already: finding it
/// is a call into Windows' security service for every process, and a process's
/// account never changes. [`owner`] finds it.
#[must_use]
pub fn usage(pid: u32) -> ProcessDetails {
    details_of(pid, false)
}

/// The account a process runs as, without its domain: `ps`'s `USER`.
#[must_use]
pub fn owner(pid: u32) -> Option<String> {
    let process = ProcessHandle::open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    token_user(process.0)
}

fn details_of(pid: u32, with_user: bool) -> ProcessDetails {
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

    if with_user {
        details.user = token_user(handle);
    }

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

/// The account running the current process, with its SID's RID.
#[must_use]
pub fn current_process_account() -> Option<(String, u32)> {
    // SAFETY: GetCurrentProcess returns a pseudo-handle for the current process.
    let current = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() };
    token_account(current)
}

/// The account a process's token names, without its domain.
fn token_user(process: windows_sys::Win32::Foundation::HANDLE) -> Option<String> {
    token_account(process).map(|(name, _)| name)
}

/// The current user's SID as text (`S-1-5-21-…`), as a security descriptor names it.
pub(crate) fn current_user_sid_string() -> Option<String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;

    // SAFETY: GetCurrentProcess returns a pseudo-handle for the current process.
    let current = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() };
    with_token_user(current, |user| {
        let mut text: *mut u16 = std::ptr::null_mut();
        // SAFETY: the SID comes from the token, and `text` is a valid out-param.
        if unsafe { ConvertSidToStringSidW(user.User.Sid, &raw mut text) } == 0 {
            return None;
        }
        // SAFETY: on success `text` is a null-terminated string the call allocated.
        let sid = unsafe { crate::net::widestring_at(text) };
        // SAFETY: the string was allocated by the call above, with LocalAlloc.
        unsafe { LocalFree(text.cast()) };
        Some(sid)
    })
}

/// The account a process's token names, without its domain, and its SID's RID.
fn token_account(process: windows_sys::Win32::Foundation::HANDLE) -> Option<(String, u32)> {
    use windows_sys::Win32::Security::{LookupAccountSidW, SID_NAME_USE};

    with_token_user(process, |user| {
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

        let rid = crate::fs::sid_rid(user.User.Sid)?;
        Some((String::from_utf16_lossy(&name[..name_len as usize]), rid))
    })
}

/// Runs `with` on the user a process's token names.
fn with_token_user<T>(
    process: windows_sys::Win32::Foundation::HANDLE,
    with: impl FnOnce(&windows_sys::Win32::Security::TOKEN_USER) -> Option<T>,
) -> Option<T> {
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
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
    with(user)
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
/// already failed. The process exits with `status`: POSIX's 128 + the signal's
/// number (137 for `KILL`), so `wait` reports what Bash reports.
pub fn terminate(pid: u32, status: u32) -> std::io::Result<()> {
    use windows_sys::Win32::System::Threading::{PROCESS_TERMINATE, TerminateProcess};

    // SAFETY: OpenProcess returns null rather than a bad handle on failure.
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, FALSE, pid) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error());
    }

    // SAFETY: handle is valid and carries PROCESS_TERMINATE.
    let ok = unsafe { TerminateProcess(handle, status) };
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

/// Whether the executable at `path` is built for the Windows GUI subsystem — a program
/// that opens windows rather than one that runs in a console. `None` if it is not a PE
/// image this can read.
#[must_use]
pub fn is_gui_image(path: &std::path::Path) -> Option<bool> {
    use std::io::{Read as _, Seek as _, SeekFrom};

    const IMAGE_SUBSYSTEM_WINDOWS_GUI: u16 = 2;

    let mut file = std::fs::File::open(path).ok()?;
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos).ok()?;
    if dos.get(..2)? != b"MZ" {
        return None;
    }
    let pe = u32::from_le_bytes(dos.get(0x3C..0x40)?.try_into().ok()?);
    // The signature, the 20-byte COFF header, then the optional header, whose
    // `Subsystem` field sits at offset 68 in both PE32 and PE32+.
    let mut header = [0u8; 24 + 70];
    file.seek(SeekFrom::Start(u64::from(pe))).ok()?;
    file.read_exact(&mut header).ok()?;
    if header.get(..4)? != b"PE\0\0" {
        return None;
    }
    let subsystem = u16::from_le_bytes(header.get(24 + 68..24 + 70)?.try_into().ok()?);
    Some(subsystem == IMAGE_SUBSYSTEM_WINDOWS_GUI)
}

/// The files mapped into a process as modules: its executable and every loaded DLL.
///
/// `None` when the process cannot be opened for reading (other users' processes and
/// protected ones, without elevation); the caller then leaves those rows out.
///
/// The list comes from a Toolhelp snapshot, which reads a process's modules in one call:
/// asking for each module's path in turn (`GetModuleFileNameExW`) took five times as long,
/// about a second for the 12,500 modules of 220 processes (2026-10-02), and `fuser` on a
/// system DLL asks every process. A snapshot keeps a path to 259 characters, so a path
/// that long is asked for in full, as before.
#[must_use]
pub fn modules(pid: u32) -> Option<Vec<std::path::PathBuf>> {
    use windows_sys::Win32::Foundation::ERROR_BAD_LENGTH;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, TH32CS_SNAPMODULE,
        TH32CS_SNAPMODULE32,
    };

    // To a snapshot, 0 is the calling process; here it is the System Idle Process, which
    // no one may open.
    if pid == 0 {
        return None;
    }
    // The snapshot fails with "bad length" while the process is loading a module.
    let mut snapshot = INVALID_HANDLE_VALUE;
    for _ in 0..4 {
        // SAFETY: always safe; the result is checked.
        snapshot =
            unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid) };
        // SAFETY: always safe.
        if snapshot != INVALID_HANDLE_VALUE || unsafe { GetLastError() } != ERROR_BAD_LENGTH {
            break;
        }
    }
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }
    let snapshot = ProcessHandle(snapshot);

    // SAFETY: an all-zero `MODULEENTRY32W` is valid; its size is set below.
    let mut entry: MODULEENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = u32::try_from(size_of::<MODULEENTRY32W>()).unwrap_or(u32::MAX);
    let mut paths = Vec::new();
    let mut full_paths = None;
    // SAFETY: the snapshot is open and `entry` is a valid out-param of the size it says.
    let mut more = unsafe { Module32FirstW(snapshot.0, &raw mut entry) } != 0;
    while more {
        let length = entry
            .szExePath
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(entry.szExePath.len());
        let path = if length + 1 >= entry.szExePath.len() {
            // Possibly cut short: asked of the process in full.
            let process =
                full_paths.get_or_insert_with(|| ProcessHandle::open(pid, MODULE_QUERY_ACCESS));
            process
                .as_ref()
                .and_then(|process| module_path(process, entry.hModule))
        } else {
            entry
                .szExePath
                .get(..length)
                .map(std::ffi::OsString::from_wide)
        };
        paths.extend(path.map(std::path::PathBuf::from));
        // SAFETY: as above.
        more = unsafe { Module32NextW(snapshot.0, &raw mut entry) } != 0;
    }
    Some(paths)
}

/// What a process is opened with to ask for a module's full path.
const MODULE_QUERY_ACCESS: u32 = windows_sys::Win32::System::Threading::PROCESS_QUERY_INFORMATION
    | windows_sys::Win32::System::Threading::PROCESS_VM_READ;

/// The full path of the module `module` in `process`.
fn module_path(
    process: &ProcessHandle,
    module: windows_sys::Win32::Foundation::HMODULE,
) -> Option<std::ffi::OsString> {
    use windows_sys::Win32::System::ProcessStatus::K32GetModuleFileNameExW;

    let mut name = vec![0u16; 32_768];
    let size = u32::try_from(name.len()).unwrap_or(u32::MAX);
    // SAFETY: the handle is open with the access the call needs, `module` is one of the
    // process's, and `name` has `size` writable units.
    let length = unsafe { K32GetModuleFileNameExW(process.0, module, name.as_mut_ptr(), size) };
    let length = usize::try_from(length).ok().filter(|&length| length > 0)?;
    name.get(..length).map(std::ffi::OsString::from_wide)
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
    fn another_process_has_its_modules_and_pid_0_none() {
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "set /p line="])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        // A process being started has no module list to read until its loader has run.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let listed = loop {
            if let Some(listed) = modules(child.id()) {
                break listed;
            }
            assert!(std::time::Instant::now() < deadline, "no module list");
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        drop(child.stdin.take());
        let _ = child.wait();
        let named = |name: &str| {
            listed
                .iter()
                .any(|m| m.to_string_lossy().to_ascii_lowercase().ends_with(name))
        };
        assert!(named("\\cmd.exe") && named("\\kernel32.dll"), "{listed:?}");
        // To a snapshot 0 is the caller; the System Idle Process lists nothing.
        assert_eq!(modules(0), None);
    }

    #[test]
    fn a_held_process_is_asked_and_not_its_exit_status() {
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "set /p line= & exit 259"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let held = Held::open(child.id()).unwrap();
        assert_eq!(held.pid(), child.id());
        assert!(held.is_running());

        drop(child.stdin.take());
        // 259 is what Windows reports for a process that has not exited.
        assert_eq!(child.wait().unwrap().code(), Some(259));
        assert!(!held.is_running());
        assert!(!is_pid_alive(child.id()));
    }

    #[test]
    fn a_process_that_started_after_its_pid_was_seen_is_another_one() {
        // This process has the pid: only its start time says it is not the one that had
        // the pid a moment before it started.
        let me = Held::open(std::process::id()).unwrap();
        let began = me.started().unwrap();
        assert_eq!(started(std::process::id()), Some(began));
        assert!(me.started_by(began));
        assert!(me.started_by(now_filetime()));
        assert!(!me.started_by(began - 1));
    }

    #[test]
    fn a_missing_process_has_neither() {
        // Process ids are multiples of four, so this one never exists.
        assert_eq!(image_path(3), None);
        assert_eq!(modules(3), None);
    }
}
