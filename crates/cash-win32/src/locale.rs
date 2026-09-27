//! Dates and times in the user's own Windows format.

use std::time::{SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows_sys::Win32::Globalization::{DATE_SHORTDATE, GetDateFormatEx, GetTimeFormatEx};
use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

/// Seconds from 1601-01-01, where `FILETIME` counts from, to 1970-01-01.
const EPOCH_DIFFERENCE_SECS: u64 = 11_644_473_600;

/// `time` in local time as the user's short date (`28/08/2026` or `8/28/2026`, as set in
/// Windows' region settings) and a 24-hour `HH:mm:ss` time: how `where.exe /t` shows a
/// file's modification time.
#[must_use]
pub fn short_date_and_time(time: SystemTime) -> Option<(String, String)> {
    let since = time.duration_since(UNIX_EPOCH).ok()?;
    let ticks = (since.as_secs() + EPOCH_DIFFERENCE_SECS) * 10_000_000
        + u64::from(since.subsec_nanos() / 100);
    let file_time = FILETIME {
        dwLowDateTime: (ticks & 0xFFFF_FFFF) as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    // SAFETY: a zeroed SYSTEMTIME is a valid out-parameter.
    let mut utc: SYSTEMTIME = unsafe { std::mem::zeroed() };
    // SAFETY: as above.
    let mut local: SYSTEMTIME = unsafe { std::mem::zeroed() };
    // SAFETY: valid pointers to initialised structures.
    if unsafe { FileTimeToSystemTime(&raw const file_time, &raw mut utc) } == 0 {
        return None;
    }
    // SAFETY: a null time zone means the current one; the pointers are valid.
    if unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &raw const utc, &raw mut local) }
        == 0
    {
        return None;
    }

    let mut buffer = [0u16; 128];
    let capacity = i32::try_from(buffer.len()).ok()?;
    // SAFETY: a null locale name is the user's default locale; the buffer is writable
    // for `capacity` characters.
    let written = unsafe {
        GetDateFormatEx(
            std::ptr::null(),
            DATE_SHORTDATE,
            &raw const local,
            std::ptr::null(),
            buffer.as_mut_ptr(),
            capacity,
            std::ptr::null(),
        )
    };
    let date = wide_to_string(&buffer, written)?;

    let format: Vec<u16> = "HH':'mm':'ss\0".encode_utf16().collect();
    // SAFETY: as above, with a NUL-terminated format picture.
    let written = unsafe {
        GetTimeFormatEx(
            std::ptr::null(),
            0,
            &raw const local,
            format.as_ptr(),
            buffer.as_mut_ptr(),
            capacity,
        )
    };
    let clock = wide_to_string(&buffer, written)?;
    Some((date, clock))
}

/// The text a Win32 formatting call wrote, `written` counting its terminating NUL.
fn wide_to_string(buffer: &[u16], written: i32) -> Option<String> {
    let length = usize::try_from(written).ok()?.checked_sub(1)?;
    Some(String::from_utf16_lossy(buffer.get(..length)?))
}
