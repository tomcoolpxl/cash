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

/// A reusable local query for Windows' ready-to-run processor queue.
///
/// This is the inexpensive `\\System\\Processor Queue Length` performance counter.
/// It contains threads that are ready but waiting for a processor; it does not inspect
/// processes or contact another machine.
pub struct ProcessorQueue {
    query: windows_sys::Win32::System::Performance::PDH_HQUERY,
    counter: windows_sys::Win32::System::Performance::PDH_HCOUNTER,
}

// SAFETY: PDH real-time query handles are not thread-affine. `sample` requires mutable
// access, so a moved query is still collected serially, and `Drop` owns the only close.
unsafe impl Send for ProcessorQueue {}

impl ProcessorQueue {
    /// Open the local, language-neutral processor-queue counter.
    #[must_use]
    pub fn open() -> Option<Self> {
        use windows_sys::Win32::System::Performance::{
            PdhAddEnglishCounterW, PdhCloseQuery, PdhOpenQueryW,
        };

        let mut query = std::ptr::null_mut();
        // SAFETY: the data-source pointer is null for the local real-time source and
        // `query` is a valid out-parameter.
        if unsafe { PdhOpenQueryW(std::ptr::null(), 0, &raw mut query) } != ERROR_SUCCESS {
            return None;
        }

        let path: Vec<u16> = "\\System\\Processor Queue Length\0"
            .encode_utf16()
            .collect();
        let mut counter = std::ptr::null_mut();
        // SAFETY: `query` is open, `path` is NUL terminated, and `counter` is a valid
        // out-parameter. English counter names avoid locale-dependent registry names.
        if unsafe { PdhAddEnglishCounterW(query, path.as_ptr(), 0, &raw mut counter) }
            != ERROR_SUCCESS
        {
            // SAFETY: `query` was opened successfully above and has not been closed.
            let _ = unsafe { PdhCloseQuery(query) };
            return None;
        }

        Some(Self { query, counter })
    }

    /// Collect the current number of ready threads waiting for processor time.
    #[must_use]
    pub fn sample(&mut self) -> Option<f64> {
        use windows_sys::Win32::System::Performance::{
            PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE,
            PdhCollectQueryData, PdhGetFormattedCounterValue,
        };

        // SAFETY: the query remains open for `self`'s lifetime.
        if unsafe { PdhCollectQueryData(self.query) } != ERROR_SUCCESS {
            return None;
        }

        let mut value = PDH_FMT_COUNTERVALUE::default();
        // SAFETY: `counter` belongs to the open query and `value` is a valid out-parameter.
        if unsafe {
            PdhGetFormattedCounterValue(
                self.counter,
                PDH_FMT_DOUBLE,
                std::ptr::null_mut(),
                &raw mut value,
            )
        } != ERROR_SUCCESS
            || !matches!(value.CStatus, PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA)
        {
            return None;
        }

        // SAFETY: `PDH_FMT_DOUBLE` selects the union's `doubleValue` member.
        let queue = unsafe { value.Anonymous.doubleValue };
        queue.is_finite().then_some(queue.max(0.0))
    }
}

impl Drop for ProcessorQueue {
    fn drop(&mut self) {
        use windows_sys::Win32::System::Performance::PdhCloseQuery;

        // SAFETY: the query is owned by `self` and is closed exactly once here.
        let _ = unsafe { PdhCloseQuery(self.query) };
    }
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
