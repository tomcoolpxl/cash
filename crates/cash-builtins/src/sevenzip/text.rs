//! 7-Zip's small formats: sizes, times, attributes, and Windows' words for an error.

use std::fmt::Write as _;
use std::io;

use cash_core::timefmt::Zone;
use chrono::{DateTime, Utc};

/// Windows' own words for an error, as 7-Zip's `MyFormatMessage` gives them.
pub(super) fn system_message(error: &io::Error) -> String {
    let text = error.to_string();
    match text.find(" (os error ") {
        Some(at) => text.get(..at).unwrap_or_default().to_owned(),
        None => text,
    }
}

/// `PrintSize_bytes_Smart`: "N bytes (M KiB)", rounded up, MiB from 10 MiB, GiB from 10
/// GiB.
pub(super) fn size_smart(value: u64) -> String {
    let mut text = format!("{value} bytes");
    if value == 0 {
        return text;
    }
    let (bits, unit) = if value >= 10 << 30 {
        (30, 'G')
    } else if value >= 10 << 20 {
        (20, 'M')
    } else {
        (10, 'K')
    };
    let rounded = value.div_ceil(1 << bits);
    let _ = write!(text, " ({rounded} {unit}iB)");
    text
}

/// "1 file" or "N files", as 7-Zip counts.
pub(super) fn count(value: u64, one: &str, many: &str) -> String {
    format!("{value} {}", if value == 1 { one } else { many })
}

/// FILETIME ticks (100 ns since 1601) as seconds and ticks since 1970.
fn unix_of(ticks: u64) -> (i64, u32) {
    let seconds = i64::try_from(ticks / 10_000_000).unwrap_or(i64::MAX) - 11_644_473_600;
    let rest = (ticks % 10_000_000) as u32;
    (seconds, rest)
}

/// `UtcFileTime_To_LocalDosTime`: FILETIME ticks as an MS-DOS time in `zone`, rounded up
/// to the two seconds it counts in, within its years, 1980 to 2107.
pub(super) fn dos_time(zone: &Zone, ticks: u64) -> u32 {
    use chrono::{Datelike, Timelike};
    const LOW: u32 = 0x0021_0000;
    const HIGH: u32 = 0xFF9F_BF7D;
    let (seconds, rest) = unix_of(ticks);
    let mut seconds = seconds + i64::from(rest != 0);
    seconds += seconds.rem_euclid(2);
    let Some(utc) = DateTime::<Utc>::from_timestamp(seconds, 0) else {
        return LOW;
    };
    let local = zone.to_local(utc);
    let year = local.year();
    if year < 1980 {
        return LOW;
    }
    if year > 2107 {
        return HIGH;
    }
    let year = u32::try_from(year - 1980).unwrap_or(0);
    (year << 25)
        | (local.month() << 21)
        | (local.day() << 16)
        | (local.hour() << 11)
        | (local.minute() << 5)
        | (local.second() / 2)
}

/// A time as 7-Zip lists it: `YYYY-MM-DD HH:MM:SS` in the shell's zone, with `digits`
/// of the fraction (7 in a technical listing); empty for the zero time.
pub(super) fn time(zone: &Zone, ticks: u64, digits: usize) -> String {
    time_ns(zone, ticks, 0, digits)
}

/// [`time`] with the nanoseconds past the ticks, for 8 and 9 digits.
pub(super) fn time_ns(zone: &Zone, ticks: u64, extra: u8, digits: usize) -> String {
    if ticks == 0 {
        return String::new();
    }
    let (seconds, rest) = unix_of(ticks);
    let Some(when) = DateTime::<Utc>::from_timestamp(seconds, 0) else {
        return String::new();
    };
    let mut text = zone.format(when, "%Y-%m-%d %H:%M:%S");
    if digits > 0 {
        let fraction = format!("{rest:07}{extra:02}");
        text.push('.');
        text.push_str(fraction.get(..digits.min(9)).unwrap_or_default());
    }
    text
}

/// The listing's five attribute columns: `D`, `R`, `H`, `S`, `A`.
pub(super) fn attributes_short(attrib: u32, is_dir: bool) -> String {
    let a = if is_dir { attrib | 0x10 } else { attrib };
    [
        (0x10, 'D'),
        (0x01, 'R'),
        (0x02, 'H'),
        (0x04, 'S'),
        (0x20, 'A'),
    ]
    .iter()
    .map(|(bit, c)| if a & bit != 0 { *c } else { '.' })
    .collect()
}

const WIN_ATTRIB_CHARS: &[u8; 30] = b"RHS8DAdNTsLCOIEVvX.PU.M......B";

/// `ConvertWinAttribToString`: every Windows attribute's letter, what has none in hex,
/// and a Unix mode after them when the attributes carry one (the 0x8000 mark).
pub(super) fn attributes_long(attrib: u32, is_dir: bool) -> String {
    let mut wa = if is_dir { attrib | 0x10 } else { attrib };
    let posix = (wa & 0x8000 != 0).then_some(wa >> 16);
    if posix.is_some() && wa & 0xF000_0000 != 0 {
        wa &= 0x3FFF;
    }
    let mut text = String::new();
    for (i, c) in WIN_ATTRIB_CHARS.iter().enumerate() {
        let flag = 1u32 << i;
        if wa & flag != 0 && *c != b'.' {
            wa &= !flag;
            text.push(char::from(*c));
        }
    }
    if wa != 0 {
        let _ = write!(text, " {wa:08X}");
    }
    if let Some(mode) = posix {
        text.push(' ');
        text.push_str(&posix_mode(mode));
    }
    text
}

/// A Unix mode as 7-Zip shows a tar item's `Mode`: `drwxr-xr-x`.
pub(super) fn posix_mode_string(mode: u32) -> String {
    posix_mode(mode)
}

fn posix_mode(a: u32) -> String {
    const TYPES: &[u8; 16] = b"0pc3d5b7-9lBsDEF";
    let mut s: Vec<u8> = vec![TYPES[((a >> 12) & 0xF) as usize]];
    let mut mask = 1u32 << 8;
    for _ in 0..3 {
        for c in *b"rwx" {
            s.push(if a & mask != 0 { c } else { b'-' });
            mask >>= 1;
        }
    }
    if a & 0x800 != 0 {
        s[3] = if a & (1 << 6) != 0 { b's' } else { b'S' };
    }
    if a & 0x400 != 0 {
        s[6] = if a & (1 << 3) != 0 { b's' } else { b'S' };
    }
    if a & 0x200 != 0 {
        s[9] = if a & 1 != 0 { b't' } else { b'T' };
    }
    let mut text = String::from_utf8_lossy(&s).into_owned();
    let high = a & !0xFFFF;
    if high != 0 {
        let _ = write!(text, " {high:08X}");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_round_up_in_7_zips_units() {
        assert_eq!(size_smart(0), "0 bytes");
        assert_eq!(size_smart(4), "4 bytes (1 KiB)");
        assert_eq!(size_smart(1067), "1067 bytes (2 KiB)");
        assert_eq!(size_smart(10 << 20), "10485760 bytes (10 MiB)");
        assert_eq!(size_smart((10 << 20) - 1), "10485759 bytes (10240 KiB)");
    }

    #[test]
    fn attributes_as_7_zip_spells_them() {
        assert_eq!(attributes_short(0x20, false), "....A");
        assert_eq!(attributes_short(0, true), "D....");
        assert_eq!(attributes_long(0x20, false), "A");
        assert_eq!(attributes_long(0x10, true), "D");
        assert_eq!(
            attributes_long(0x8020 | (0o100_644 << 16), false),
            "A -rw-r--r--"
        );
    }

    #[test]
    fn times_in_utc() {
        let zone = Zone::from_tz(Some("UTC0"));
        // 2026-10-07 08:00:00 UTC.
        let ticks = (1_791_360_000 + 11_644_473_600) * 10_000_000;
        assert_eq!(time(&zone, ticks, 0), "2026-10-07 08:00:00");
        assert_eq!(time(&zone, ticks + 5, 7), "2026-10-07 08:00:00.0000005");
        assert_eq!(time(&zone, 0, 0), "");
    }
}
