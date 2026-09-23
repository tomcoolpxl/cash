//! What the machine is, asked directly.
//!
//! cash: every number here has a Win32 call behind it and nothing spawns a process. The
//! tools that print this sort of banner elsewhere — `neofetch` and its descendants — are
//! shell scripts that shell out dozens of times, which is why they take a visible moment
//! on Windows in particular, where each spawn is expensive. `coolfetch` is built on this
//! module instead, and the cost is a handful of syscalls.

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, MAX_PATH};

/// How long the machine has been up.
#[must_use]
pub fn uptime() -> std::time::Duration {
    use windows_sys::Win32::System::SystemInformation::GetTickCount64;

    // SAFETY: no arguments, no out-params; the count wraps after 584 million years.
    let millis = unsafe { GetTickCount64() };
    std::time::Duration::from_millis(millis)
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

/// Every fixed drive on the machine, as `C:`, `D:` and so on.
///
/// cash: Windows has drive letters rather than one tree, so reporting only the volume the
/// shell happens to be sitting on says nothing about the machine — a second disk holding
/// everything that matters would never appear. Removable and network drives are left out:
/// a fetch that spins up a USB stick or waits on a disconnected share is a fetch nobody
/// runs twice.
#[must_use]
pub fn fixed_drives() -> Vec<String> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};

    const DRIVE_FIXED: u32 = 3;

    // SAFETY: no arguments; returns a bitmask with one bit per drive letter.
    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Vec::new();
    }

    let mut drives = Vec::new();
    for bit in 0..26u32 {
        if mask & (1 << bit) == 0 {
            continue;
        }

        let letter = char::from(b'A' + u8::try_from(bit).unwrap_or(0));
        let root = std::format!("{letter}:\\");
        let wide = wide(root.as_str());

        // SAFETY: the root path is NUL-terminated.
        if unsafe { GetDriveTypeW(wide.as_ptr()) } == DRIVE_FIXED {
            drives.push(std::format!("{letter}:"));
        }
    }

    drives
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

/// A NUL-terminated UTF-16 copy, as every `W` entry point wants.
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
