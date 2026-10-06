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

    let format = crate::wide::to_wide_nul("HH':'mm':'ss");
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

/// The user's regional format locale (`GetUserDefaultLocaleName`), as Windows names
/// it: `en-US`, `nl-BE`, `sr-Latn-RS`.
#[must_use]
pub fn user_locale_name() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buffer = [0u16; LOCALE_NAME_MAX_LENGTH];
    // SAFETY: `buffer` holds `buffer.len()` units, which is the capacity passed.
    let written = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), name_capacity()) };
    wide_to_string(&buffer, written)
}

/// The system's locale (`GetSystemDefaultLocaleName`), the one programs that are not
/// Unicode-aware run under.
#[must_use]
pub fn system_locale_name() -> Option<String> {
    use windows_sys::Win32::Globalization::GetSystemDefaultLocaleName;
    let mut buffer = [0u16; LOCALE_NAME_MAX_LENGTH];
    // SAFETY: as in `user_locale_name`.
    let written = unsafe { GetSystemDefaultLocaleName(buffer.as_mut_ptr(), name_capacity()) };
    wide_to_string(&buffer, written)
}

/// The user's display language (`GetUserDefaultUILanguage`), as a locale name.
#[must_use]
pub fn user_ui_locale_name() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
    // SAFETY: reads a language id; no memory of ours is touched.
    locale_name_of(u32::from(unsafe { GetUserDefaultUILanguage() }))
}

/// The system's display language (`GetSystemDefaultUILanguage`), as a locale name.
#[must_use]
pub fn system_ui_locale_name() -> Option<String> {
    use windows_sys::Win32::Globalization::GetSystemDefaultUILanguage;
    // SAFETY: as above.
    locale_name_of(u32::from(unsafe { GetSystemDefaultUILanguage() }))
}

/// `LOCALE_NAME_MAX_LENGTH`: the longest locale name, its NUL counted.
const LOCALE_NAME_MAX_LENGTH: usize = 85;

/// The capacity of a locale-name buffer, as the calls take it.
fn name_capacity() -> i32 {
    i32::try_from(LOCALE_NAME_MAX_LENGTH).unwrap_or(i32::MAX)
}

/// The locale name of a language id (`LCIDToLocaleName`).
fn locale_name_of(language: u32) -> Option<String> {
    use windows_sys::Win32::Globalization::LCIDToLocaleName;
    let mut buffer = [0u16; LOCALE_NAME_MAX_LENGTH];
    // SAFETY: `buffer` holds the capacity passed; no flags.
    let written = unsafe { LCIDToLocaleName(language, buffer.as_mut_ptr(), name_capacity(), 0) };
    wide_to_string(&buffer, written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_locale_names_are_windows_tags() {
        for name in [
            user_locale_name(),
            system_locale_name(),
            user_ui_locale_name(),
            system_ui_locale_name(),
        ] {
            let name = name.unwrap_or_default();
            assert!(
                name.len() >= 2 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "{name}"
            );
        }
    }
}
