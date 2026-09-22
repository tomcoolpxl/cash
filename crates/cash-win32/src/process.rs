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
