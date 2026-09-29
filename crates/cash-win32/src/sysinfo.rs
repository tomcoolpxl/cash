//! What the machine is, asked directly.
//!
//! cash: every number here has a Win32 call behind it and nothing spawns a process. The
//! tools that print this sort of banner elsewhere — `neofetch` and its descendants — are
//! shell scripts that shell out dozens of times, which is why they take a visible moment
//! on Windows in particular, where each spawn is expensive. `coolfetch` is built on this
//! module instead, and the cost is a handful of syscalls.

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, MAX_PATH};

/// Cumulative processor counters returned by Windows, in 100-nanosecond units.
///
/// `kernel` includes `idle`, matching the contract of `GetSystemTimes`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuTimes {
    /// Time spent idle; also included in `kernel` as required by the Win32 API.
    pub idle: u64,
    /// Time spent in kernel mode, including idle time.
    pub kernel: u64,
    /// Time spent in user mode.
    pub user: u64,
}

/// A cheap, machine-wide physical-memory snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryStatus {
    /// Installed usable physical memory.
    pub physical_total: u64,
    /// Physical memory Windows can make available immediately.
    pub physical_available: u64,
    /// Bytes currently committed by the system.
    pub commit_total: u64,
    /// Current soft ceiling for committed memory.
    pub commit_limit: u64,
    /// Highest committed byte count since boot.
    pub commit_peak: u64,
}

/// One process in a [`SystemSnapshot`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotProcess {
    /// Its process id.
    pub pid: u32,
    /// The id of the process that started it. Windows reuses ids, so that process may
    /// have exited and the id now name another.
    pub parent_pid: u32,
    /// Its image name, as Task Manager shows it.
    pub name: String,
    /// The base priority its threads start at.
    pub base_priority: i32,
}

/// Every process on the machine, and how many threads wait for a processor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemSnapshot {
    /// The processes, ordered by pid.
    pub processes: Vec<SnapshotProcess>,
    /// Threads ready to run that no processor is running: the processor queue.
    pub ready_threads: u32,
}

/// Thread states of a thread that waits for a processor: ready, chosen to run next on
/// one (standby), and ready but not yet given one (deferred ready). The first two are
/// the states Windows' "Thread State" performance counter documents as 1 and 3.
const READY: u32 = 1;
const STANDBY: u32 = 3;
const DEFERRED_READY: u32 = 7;

/// `SystemProcessInformation` and `SystemProcessorPerformanceInformation`.
const PROCESS_INFORMATION: i32 = 5;
const PROCESSOR_PERFORMANCE_INFORMATION: i32 = 8;

/// `STATUS_INFO_LENGTH_MISMATCH`: the buffer was too small.
#[expect(
    clippy::cast_possible_wrap,
    reason = "NTSTATUS error codes are negative i32 values written as hex"
)]
const INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;

type NtQuerySystemInformationFn =
    unsafe extern "system" fn(i32, *mut core::ffi::c_void, u32, *mut u32) -> i32;

/// `NtQuerySystemInformation`, looked up at run time as Microsoft advises: it documents
/// the function, and the members read here, as possibly changing or going in a future
/// Windows, and a lookup that fails leaves the callers to do without.
fn nt_query_system_information() -> Option<NtQuerySystemInformationFn> {
    use std::sync::LazyLock;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

    static QUERY: LazyLock<Option<NtQuerySystemInformationFn>> = LazyLock::new(|| {
        // SAFETY: ntdll is loaded into every Windows process.
        let ntdll = unsafe { GetModuleHandleA(c"ntdll.dll".as_ptr().cast()) };
        if ntdll.is_null() {
            return None;
        }
        // SAFETY: the module handle is valid and the name is NUL terminated.
        let found = unsafe { GetProcAddress(ntdll, c"NtQuerySystemInformation".as_ptr().cast()) };
        found.map(|function| {
            // SAFETY: the documented signature of NtQuerySystemInformation.
            unsafe { std::mem::transmute::<_, NtQuerySystemInformationFn>(function) }
        })
    });
    *QUERY
}

/// Takes [`SystemSnapshot`]s, keeping one buffer between them.
///
/// One `NtQuerySystemInformation(SystemProcessInformation)` call, the one Toolhelp's
/// process snapshot is built on, returns every process followed by its threads, each
/// thread with its state. Counting the threads that wait for a processor gives the
/// processor queue that the `\System\Processor Queue Length` performance counter
/// reports, without starting performance counters: opening that counter cost `top`
/// about 350 ms of CPU at every start, and some Windows builds lack it.
#[derive(Default)]
pub struct SystemQuery {
    /// `u64`s so that the structures in it are aligned.
    buffer: Vec<u64>,
}

impl SystemQuery {
    /// Every process and the processor queue now; `None` where Windows will not say.
    pub fn take(&mut self) -> Option<SystemSnapshot> {
        let query = nt_query_system_information()?;
        // Processes and threads come and go between asking for the size and asking for
        // the data, so the buffer grows with room to spare, a few times at most.
        for _ in 0..4 {
            if self.buffer.is_empty() {
                self.buffer.resize(1 << 17, 0);
            }
            let capacity = u32::try_from(self.buffer.len() * 8).unwrap_or(u32::MAX);
            let mut needed = 0u32;
            // SAFETY: the buffer holds `capacity` writable bytes and `needed` is a valid
            // out-parameter.
            let status = unsafe {
                query(
                    PROCESS_INFORMATION,
                    self.buffer.as_mut_ptr().cast(),
                    capacity,
                    &raw mut needed,
                )
            };
            if status == INFO_LENGTH_MISMATCH {
                let words = (needed as usize).div_ceil(8) + (needed as usize).div_ceil(8) / 4;
                self.buffer.resize(words.max(self.buffer.len() * 2), 0);
                continue;
            }
            if status < 0 {
                return None;
            }
            let length = (needed as usize).min(self.buffer.len() * 8);
            // SAFETY: the call succeeded and wrote `length` bytes of process records.
            return unsafe { parse_processes(self.buffer.as_ptr().cast(), length) };
        }
        None
    }
}

/// Reads the process records `NtQuerySystemInformation` wrote at `base`, `length` bytes.
///
/// # Safety
///
/// `base` must point to `length` readable bytes holding the records the call wrote.
unsafe fn parse_processes(base: *const u8, length: usize) -> Option<SystemSnapshot> {
    use windows_sys::Win32::System::WindowsProgramming::{
        SYSTEM_PROCESS_INFORMATION, SYSTEM_THREAD_INFORMATION,
    };

    let process_size = size_of::<SYSTEM_PROCESS_INFORMATION>();
    let thread_size = size_of::<SYSTEM_THREAD_INFORMATION>();
    let mut processes = Vec::new();
    let mut ready_threads = 0u32;
    let mut offset = 0usize;

    loop {
        if offset.checked_add(process_size)? > length {
            return None;
        }
        let record = base.wrapping_add(offset);
        // SAFETY: the record lies within the buffer, checked just above.
        let process: SYSTEM_PROCESS_INFORMATION =
            unsafe { std::ptr::read_unaligned(record.cast()) };
        let pid = u32::try_from(process.UniqueProcessId as usize).unwrap_or(u32::MAX);
        // The documentation now names this member InheritedFromUniqueProcessId.
        let parent_pid = u32::try_from(process.Reserved2 as usize).unwrap_or(0);

        let threads = process.NumberOfThreads as usize;
        let threads_at = offset + process_size;
        if threads_at.checked_add(threads.checked_mul(thread_size)?)? > length {
            return None;
        }
        // The Idle process's threads stand for idle processors, never for work.
        if pid != 0 {
            for index in 0..threads {
                let record = base.wrapping_add(threads_at + index * thread_size);
                // SAFETY: every thread record lies within the buffer, checked above.
                let thread: SYSTEM_THREAD_INFORMATION =
                    unsafe { std::ptr::read_unaligned(record.cast()) };
                if matches!(thread.ThreadState, READY | STANDBY | DEFERRED_READY) {
                    ready_threads = ready_threads.saturating_add(1);
                }
            }
        }

        // SAFETY: the name is read only where it lies within the buffer.
        let name = unsafe { image_name(base, length, &process.ImageName) }
            .unwrap_or_else(|| String::from(if pid == 0 { "[System Process]" } else { "?" }));
        processes.push(SnapshotProcess {
            pid,
            parent_pid,
            name,
            base_priority: process.BasePriority,
        });

        if process.NextEntryOffset == 0 {
            break;
        }
        offset = offset.checked_add(process.NextEntryOffset as usize)?;
    }

    processes.sort_by_key(|process| process.pid);
    Some(SystemSnapshot {
        processes,
        ready_threads,
    })
}

/// A process record's image name, where it points into the buffer at `base`.
///
/// # Safety
///
/// `base` must point to `length` readable bytes.
unsafe fn image_name(
    base: *const u8,
    length: usize,
    name: &windows_sys::Win32::Foundation::UNICODE_STRING,
) -> Option<String> {
    let start = name.Buffer as usize;
    let bytes = usize::from(name.Length);
    let buffer_start = base as usize;
    if name.Buffer.is_null()
        || bytes == 0
        || start < buffer_start
        || start.checked_add(bytes)? > buffer_start.checked_add(length)?
    {
        return None;
    }
    let units: Vec<u16> = (0..bytes / 2)
        .map(|index| {
            let unit = name.Buffer.wrapping_add(index);
            // SAFETY: the name lies within the buffer, checked above; read unaligned.
            unsafe { std::ptr::read_unaligned(unit) }
        })
        .collect();
    Some(String::from_utf16_lossy(&units))
}

/// Cumulative idle, kernel and user time of each logical processor (of the calling
/// thread's processor group), for `top`'s per-processor lines.
///
/// `GetSystemTimes` gives only the sum; this is the documented per-processor form of the
/// same counters, through `NtQuerySystemInformation`.
#[must_use]
pub fn processor_times() -> Option<Vec<CpuTimes>> {
    use windows_sys::Win32::System::WindowsProgramming::SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION;

    /// More processors than one processor group holds.
    const MOST: usize = 256;

    let query = nt_query_system_information()?;
    let mut buffer = vec![SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION::default(); MOST];
    let capacity =
        u32::try_from(MOST * size_of::<SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION>()).ok()?;
    let mut written = 0u32;
    // SAFETY: the buffer holds `capacity` writable bytes of the structure asked for, and
    // `written` is a valid out-parameter.
    let status = unsafe {
        query(
            PROCESSOR_PERFORMANCE_INFORMATION,
            buffer.as_mut_ptr().cast(),
            capacity,
            &raw mut written,
        )
    };
    if status < 0 {
        return None;
    }
    let count =
        (written as usize / size_of::<SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION>()).min(MOST);
    let as_ticks = |value: i64| u64::try_from(value).unwrap_or(0);
    Some(
        buffer[..count]
            .iter()
            .map(|processor| CpuTimes {
                idle: as_ticks(processor.IdleTime),
                kernel: as_ticks(processor.KernelTime),
                user: as_ticks(processor.UserTime),
            })
            .collect(),
    )
}

/// How long the machine has been up.
#[must_use]
pub fn uptime() -> std::time::Duration {
    use windows_sys::Win32::System::SystemInformation::GetTickCount64;

    // SAFETY: no arguments, no out-params; the count wraps after 584 million years.
    let millis = unsafe { GetTickCount64() };
    std::time::Duration::from_millis(millis)
}

/// Cumulative idle, kernel and user processor time for the whole machine.
///
/// Two snapshots are enough to build the familiar `top` CPU summary. This is one
/// kernel call and does not start performance counters or enumerate processors.
#[must_use]
pub fn cpu_times() -> Option<CpuTimes> {
    use windows_sys::Win32::{Foundation::FILETIME, System::Threading::GetSystemTimes};

    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut idle, mut kernel, mut user) = (zero, zero, zero);
    // SAFETY: all three arguments are valid FILETIME out-parameters.
    if unsafe { GetSystemTimes(&raw mut idle, &raw mut kernel, &raw mut user) } == 0 {
        return None;
    }

    Some(CpuTimes {
        idle: filetime_u64(idle),
        kernel: filetime_u64(kernel),
        user: filetime_u64(user),
    })
}

/// Physical memory accounting in one call.
#[must_use]
pub fn memory_status() -> Option<MemoryStatus> {
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetPerformanceInfo, PERFORMANCE_INFORMATION,
    };

    let mut info = PERFORMANCE_INFORMATION {
        cb: u32::try_from(std::mem::size_of::<PERFORMANCE_INFORMATION>()).unwrap_or(u32::MAX),
        ..Default::default()
    };
    // SAFETY: `info` is correctly sized and its size is passed alongside the pointer.
    if unsafe { K32GetPerformanceInfo(&raw mut info, info.cb) } == 0 {
        return None;
    }

    let page_size = u64::try_from(info.PageSize).unwrap_or(u64::MAX);
    let bytes = |pages: usize| {
        u64::try_from(pages)
            .unwrap_or(u64::MAX)
            .saturating_mul(page_size)
    };

    Some(MemoryStatus {
        physical_total: bytes(info.PhysicalTotal),
        physical_available: bytes(info.PhysicalAvailable),
        commit_total: bytes(info.CommitTotal),
        commit_limit: bytes(info.CommitLimit),
        commit_peak: bytes(info.CommitPeak),
    })
}

const fn filetime_u64(time: windows_sys::Win32::Foundation::FILETIME) -> u64 {
    ((time.dwHighDateTime as u64) << 32) | (time.dwLowDateTime as u64)
}

/// The edition Windows calls itself, e.g. `Windows 11 Pro`.
///
/// Read from the registry rather than from `GetVersionEx`, which has lied about the
/// version since Windows 8.1 unless the caller ships a compatibility manifest.
#[must_use]
pub fn os_name() -> Option<String> {
    let product = registry_string(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "ProductName",
    )?;

    // The registry still says "Windows 10" on Windows 11; the build number is what tells
    // them apart, and reporting the wrong one is exactly the sort of confident wrongness
    // this project avoids elsewhere.
    let build = os_build().unwrap_or_default();
    if build >= 22_000 && product.contains("Windows 10") {
        return Some(product.replace("Windows 10", "Windows 11"));
    }

    Some(product)
}

/// The build number, which is how Windows versions are actually told apart.
#[must_use]
pub fn os_build() -> Option<u32> {
    registry_string(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "CurrentBuildNumber",
    )
    .and_then(|value| value.parse().ok())
}

/// The NT version as `ver` prints it, `10.0.26200.9550`: major and minor version, the
/// build, and the update revision (UBR) that monthly updates raise.
#[must_use]
pub fn nt_version() -> Option<String> {
    const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let major = registry_dword(KEY, "CurrentMajorVersionNumber").unwrap_or(10);
    let minor = registry_dword(KEY, "CurrentMinorVersionNumber").unwrap_or(0);
    let build = os_build()?;
    Some(match registry_dword(KEY, "UBR") {
        Some(revision) => std::format!("{major}.{minor}.{build}.{revision}"),
        None => std::format!("{major}.{minor}.{build}"),
    })
}

/// The feature update's name, e.g. `24H2`.
#[must_use]
pub fn os_release() -> Option<String> {
    registry_string(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "DisplayVersion",
    )
}

/// The processor's marketing name, as the firmware reported it.
#[must_use]
pub fn cpu_name() -> Option<String> {
    registry_string(
        r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
        "ProcessorNameString",
    )
    .map(|name| name.trim().to_string())
}

/// Display adapter names from local Windows configuration, without sampling the GPU.
#[must_use]
pub fn gpu_names() -> Vec<String> {
    use windows_sys::Win32::Graphics::Gdi::{
        DISPLAY_DEVICE_MIRRORING_DRIVER, DISPLAY_DEVICEW, EnumDisplayDevicesW,
    };

    let mut names = Vec::new();
    for index in 0..64 {
        let mut device = DISPLAY_DEVICEW {
            cb: u32::try_from(std::mem::size_of::<DISPLAY_DEVICEW>()).unwrap_or(0),
            ..Default::default()
        };
        // SAFETY: device is initialized, its size is set, and the output pointer is valid.
        if unsafe { EnumDisplayDevicesW(std::ptr::null(), index, &raw mut device, 0) } == 0 {
            break;
        }
        if device.StateFlags & DISPLAY_DEVICE_MIRRORING_DRIVER != 0 {
            continue;
        }
        let end = device
            .DeviceString
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(device.DeviceString.len());
        let name = String::from_utf16_lossy(&device.DeviceString[..end]);
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Local IPv4 addresses on active non-loopback adapters; never contacts the network.
#[must_use]
pub fn local_ipv4_addresses() -> Vec<String> {
    use windows_sys::Win32::{
        Foundation::ERROR_BUFFER_OVERFLOW,
        NetworkManagement::{
            IpHelper::{
                GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
                GetAdaptersAddresses, IF_TYPE_SOFTWARE_LOOPBACK, IP_ADAPTER_ADDRESSES_LH,
            },
            Ndis::IfOperStatusUp,
        },
        Networking::WinSock::{AF_INET, SOCKADDR_IN},
    };

    let mut size = 15_000u32;
    // Adapter changes can invalidate a size query; bound retries rather than waiting.
    for _ in 0..3 {
        let count = (size as usize).div_ceil(std::mem::size_of::<IP_ADAPTER_ADDRESSES_LH>());
        let mut buffer = vec![IP_ADAPTER_ADDRESSES_LH::default(); count];
        // SAFETY: buffer has the required alignment and at least size writable bytes.
        // All pointers returned by this call remain within this allocation's lifetime.
        let status = unsafe {
            GetAdaptersAddresses(
                u32::from(AF_INET),
                GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER,
                std::ptr::null(),
                buffer.as_mut_ptr(),
                &raw mut size,
            )
        };
        if status == ERROR_BUFFER_OVERFLOW {
            continue;
        }
        if status != ERROR_SUCCESS {
            return Vec::new();
        }
        let mut addresses = Vec::new();
        let mut next = buffer.as_ptr();
        while !next.is_null() {
            // SAFETY: successful GetAdaptersAddresses returns a linked list in buffer.
            let adapter = unsafe { &*next };
            next = adapter.Next;
            if adapter.OperStatus != IfOperStatusUp || adapter.IfType == IF_TYPE_SOFTWARE_LOOPBACK {
                continue;
            }
            let mut unicast = adapter.FirstUnicastAddress;
            while !unicast.is_null() {
                // SAFETY: unicast is a node returned by the same successful call.
                let entry = unsafe { &*unicast };
                unicast = entry.Next;
                if entry.Address.lpSockaddr.is_null()
                    || entry.Address.iSockaddrLength
                        < i32::try_from(std::mem::size_of::<SOCKADDR_IN>()).unwrap_or(i32::MAX)
                {
                    continue;
                }
                // SAFETY: AF_INET was requested and the address length was checked.
                // Copy without requiring stronger alignment than SOCKADDR promises.
                let socket = unsafe {
                    entry
                        .Address
                        .lpSockaddr
                        .cast::<SOCKADDR_IN>()
                        .read_unaligned()
                };
                if socket.sin_family != AF_INET {
                    continue;
                }
                // SAFETY: S_addr is the IPv4 address member populated by Windows.
                let bytes = unsafe { socket.sin_addr.S_un.S_addr }.to_ne_bytes();
                let ip = std::net::Ipv4Addr::from(bytes);
                if !ip.is_unspecified() && !ip.is_loopback() {
                    addresses.push(std::format!("{ip}/{}", entry.OnLinkPrefixLength));
                }
            }
        }
        addresses.sort();
        addresses.dedup();
        return addresses;
    }
    Vec::new()
}

/// Every logical drive visible to this process, as `C:`, `D:` and so on.
///
/// Includes removable and mapped drives. Presence does not guarantee that a drive
/// is ready or reachable; callers should handle a failed usage query.
#[must_use]
pub fn logical_drives() -> Vec<String> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;

    // SAFETY: no arguments; returns a bitmask with one bit per drive letter.
    let mask = unsafe { GetLogicalDrives() };
    (b'A'..=b'Z')
        .filter(|letter| mask & (1 << (letter - b'A')) != 0)
        .map(|letter| std::format!("{}:", char::from(letter)))
        .collect()
}

/// Every fixed drive on the machine, as `C:`, `D:` and so on.
///
/// cash: Windows has drive letters rather than one tree, so reporting only the volume the
/// shell happens to be sitting on says nothing about the machine — a second disk holding
/// everything that matters would never appear. Removable and network drives are left out:
/// a fetch that spins up a USB stick or waits on a disconnected share is a fetch nobody
/// runs twice.
#[must_use]
pub fn fixed_drives() -> Vec<String> {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;

    const DRIVE_FIXED: u32 = 3;

    let mut drives = Vec::new();
    for drive in logical_drives() {
        let root = std::format!("{drive}\\");
        let wide = wide(root.as_str());

        // SAFETY: the root path is NUL-terminated.
        if unsafe { GetDriveTypeW(wide.as_ptr()) } == DRIVE_FIXED {
            drives.push(drive);
        }
    }

    drives
}

/// Whether a drive root identifies a mapped network drive.
#[must_use]
pub fn is_network_drive(root: &std::path::Path) -> bool {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;

    const DRIVE_REMOTE: u32 = 4;
    let root = wide(root.to_string_lossy().as_ref());
    // SAFETY: the drive root is NUL-terminated and valid for the call.
    unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_REMOTE }
}

/// Total and free bytes on the volume holding `path`.
#[must_use]
pub fn disk_usage(path: &std::path::Path) -> Option<(u64, u64)> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide = wide(path.to_string_lossy().as_ref());

    let mut available: u64 = 0;
    let mut total: u64 = 0;
    let mut free: u64 = 0;

    // SAFETY: the path is NUL-terminated and the three out-params are valid.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &raw mut available,
            &raw mut total,
            &raw mut free,
        )
    };

    if ok == 0 {
        return None;
    }

    Some((total, free))
}

/// Reads one string value from `HKEY_LOCAL_MACHINE`.
///
/// `RegGetValueW` is the one call that opens, reads, and expands in one step, and it
/// NUL-terminates for us — which the older `RegQueryValueEx` notoriously does not.
fn registry_string(subkey: &str, value: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};

    let subkey = wide(subkey);
    let value = wide(value);

    let mut buffer = [0u16; MAX_PATH as usize];
    let mut size = u32::try_from(std::mem::size_of_val(&buffer)).unwrap_or(u32::MAX);

    // SAFETY: both names are NUL-terminated, and the buffer is described by `size` in
    // bytes, which is what this call expects.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &raw mut size,
        )
    };

    if status != ERROR_SUCCESS {
        return None;
    }

    // `size` is in bytes and counts the terminator.
    let chars = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buffer[..chars.min(buffer.len())]))
}

/// A `REG_DWORD` under `HKEY_LOCAL_MACHINE`.
fn registry_dword(subkey: &str, value: &str) -> Option<u32> {
    use windows_sys::Win32::System::Registry::{
        HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW,
    };

    let subkey = wide(subkey);
    let value = wide(value);
    let mut data = 0u32;
    let mut size = u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4);

    // SAFETY: both names are NUL-terminated, and `data` is a DWORD described by `size`.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut data).cast(),
            &raw mut size,
        )
    };
    (status == ERROR_SUCCESS).then_some(data)
}

/// A NUL-terminated UTF-16 copy, as every `W` entry point wants.
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert loudly on failure"
)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::WindowsProgramming::{
        SYSTEM_PROCESS_INFORMATION, SYSTEM_THREAD_INFORMATION,
    };

    /// Lays out process records as `NtQuerySystemInformation` does: each process, its
    /// threads right after it, and its name after them.
    fn records(processes: &[(usize, usize, &str, &[u32])]) -> Vec<u64> {
        let process_size = size_of::<SYSTEM_PROCESS_INFORMATION>();
        let thread_size = size_of::<SYSTEM_THREAD_INFORMATION>();
        let mut buffer = vec![0u64; 4096];
        let base = buffer.as_mut_ptr().cast::<u8>();
        let mut offset = 0;
        for (index, &(pid, parent, name, states)) in processes.iter().enumerate() {
            let name: Vec<u16> = name.encode_utf16().collect();
            let name_at = offset + process_size + states.len() * thread_size;
            let next = (name_at + name.len() * 2).next_multiple_of(8);
            let mut process = SYSTEM_PROCESS_INFORMATION {
                NextEntryOffset: if index + 1 == processes.len() {
                    0
                } else {
                    u32::try_from(next - offset).unwrap()
                },
                NumberOfThreads: u32::try_from(states.len()).unwrap(),
                UniqueProcessId: pid as _,
                Reserved2: parent as _,
                BasePriority: 8,
                ..Default::default()
            };
            process.ImageName.Length = u16::try_from(name.len() * 2).unwrap();
            process.ImageName.MaximumLength = process.ImageName.Length;
            process.ImageName.Buffer = base.wrapping_add(name_at).cast();
            // SAFETY: every write here lands inside the 32 KiB buffer.
            unsafe { std::ptr::write_unaligned(base.wrapping_add(offset).cast(), process) };
            for (thread, &state) in states.iter().enumerate() {
                let record = SYSTEM_THREAD_INFORMATION {
                    ThreadState: state,
                    ..Default::default()
                };
                let at = base.wrapping_add(offset + process_size + thread * thread_size);
                // SAFETY: as above.
                unsafe { std::ptr::write_unaligned(at.cast(), record) };
            }
            for (unit, &value) in name.iter().enumerate() {
                let at = base.wrapping_add(name_at + unit * 2);
                // SAFETY: as above.
                unsafe { std::ptr::write_unaligned(at.cast(), value) };
            }
            offset = next;
        }
        buffer
    }

    #[test]
    fn records_are_read_as_windows_lays_them_out() {
        const RUNNING: u32 = 2;
        const WAITING: u32 = 5;
        let buffer = records(&[
            (
                1234,
                4,
                "tool.exe",
                &[READY, WAITING, STANDBY, DEFERRED_READY],
            ),
            // The Idle process: its threads stand for idle processors, and do not count.
            (0, 0, "", &[RUNNING, READY]),
            (4, 0, "System", &[WAITING, RUNNING]),
        ]);
        // SAFETY: the buffer holds the records `records` wrote.
        let snapshot = unsafe { parse_processes(buffer.as_ptr().cast(), buffer.len() * 8) }
            .expect("records parse");
        assert_eq!(snapshot.ready_threads, 3);
        let names: Vec<_> = snapshot
            .processes
            .iter()
            .map(|p| (p.pid, p.parent_pid, p.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                (0, 0, "[System Process]"),
                (4, 0, "System"),
                (1234, 4, "tool.exe")
            ]
        );
    }

    #[test]
    fn a_record_running_past_the_buffer_is_refused() {
        let buffer = records(&[(1234, 4, "tool.exe", &[READY; 3])]);
        let cut = size_of::<SYSTEM_PROCESS_INFORMATION>() + 8;
        // SAFETY: fewer bytes than were written are claimed; they are all readable.
        let snapshot = unsafe { parse_processes(buffer.as_ptr().cast(), cut) };
        assert_eq!(snapshot, None);
    }

    #[test]
    fn a_snapshot_lists_this_process() {
        let snapshot = SystemQuery::default().take().expect("a process snapshot");
        let me = snapshot
            .processes
            .iter()
            .find(|process| process.pid == std::process::id())
            .expect("this process is listed");
        assert!(me.name.to_ascii_lowercase().ends_with(".exe"), "{me:?}");
        assert!(snapshot.processes.iter().any(|process| process.pid == 4));
    }

    #[test]
    fn more_busy_threads_than_processors_make_a_queue() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let processors = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        let stop = Arc::new(AtomicBool::new(false));
        let spinners: Vec<_> = (0..processors * 2)
            .map(|_| {
                let stop = Arc::clone(&stop);
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        std::hint::spin_loop();
                    }
                })
            })
            .collect();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let snapshot = SystemQuery::default().take();
        stop.store(true, Ordering::Relaxed);
        for spinner in spinners {
            spinner.join().unwrap();
        }
        let ready = snapshot.expect("a process snapshot").ready_threads;
        assert!(
            ready as usize >= processors / 2,
            "{} spinning threads on {processors} processors, but {ready} ready",
            processors * 2
        );
    }

    #[test]
    fn processor_times_add_up_to_the_machines() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let busy_time = |times: &[CpuTimes]| -> u64 {
            times
                .iter()
                .map(|cpu| (cpu.kernel + cpu.user).saturating_sub(cpu.idle))
                .sum()
        };
        let before = processor_times().expect("per-processor times");
        let machine_before = cpu_times().expect("machine times");
        let stop = Arc::new(AtomicBool::new(false));
        let spinner = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    std::hint::spin_loop();
                }
            })
        };
        std::thread::sleep(std::time::Duration::from_millis(300));
        let after = processor_times().expect("per-processor times");
        let machine_after = cpu_times().expect("machine times");
        stop.store(true, Ordering::Relaxed);
        spinner.join().unwrap();

        let busy = busy_time(&after).saturating_sub(busy_time(&before));
        let machine = (machine_after.kernel + machine_after.user)
            .saturating_sub(machine_after.idle)
            .saturating_sub(
                (machine_before.kernel + machine_before.user).saturating_sub(machine_before.idle),
            );
        // One thread spun for 0.3 s: at least 0.2 s of busy time, 2,000,000 ticks.
        assert!(
            busy >= 2_000_000,
            "per-processor busy {busy}, machine {machine}"
        );
        assert!(
            busy.abs_diff(machine) <= machine / 2 + 1_000_000,
            "per-processor busy {busy}, machine {machine}"
        );
    }

    #[test]
    fn processor_times_come_one_per_processor() {
        let times = processor_times().expect("per-processor times");
        let processors = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        assert!(
            times.len() >= processors.min(64),
            "{} for {processors}",
            times.len()
        );
        assert!(times.iter().all(|cpu| cpu.kernel >= cpu.idle));
    }
}
