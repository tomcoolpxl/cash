//! WinRAR 7's header framing, as measured from the archives `Rar.exe` 7.23 writes.
//!
//! WinRAR writes a header before it knows the sizes it will hold and patches them in
//! place, so it reserves room: a file's sizes get the width of a generous bound on its
//! packed size, and the locator's offsets the width of a bound on the whole archive.
//! The bounds below reproduce every width observed; they are not WinRAR's own code.

use crate::rar::rar50::framing::vint_len;

use super::ArchiveEntry;

/// The "skip if unknown" header flag WinRAR sets on the main and end headers and on
/// the quick-open and recovery services.
pub(super) const HFL_SKIP_IF_UNKNOWN: u64 = 0x0004;

/// Compression information is written at least this wide.
pub(super) const COMPRESSION_WIDTH: usize = 2;

/// The width of a file's unpacked and packed size fields: room for twice its size
/// and a kilobyte, and five bytes at the least from a mebibyte on.
pub(super) fn size_width(size: u64) -> usize {
    let width = vint_len(size.saturating_mul(2).saturating_add(1024));
    if size >= 1 << 20 { width.max(5) } else { width }
}

/// The width of the locator's offsets: room for the archive's members, each its size,
/// 32 bytes and three for every character of its name, counted in 4,096ths.
pub(super) fn offset_width(entries: &[ArchiveEntry]) -> usize {
    let estimate = entries.iter().fold(1u64, |total, entry| {
        let size = entry.source.len().unwrap_or(0);
        let name = std::str::from_utf8(&entry.name)
            .map_or(entry.name.len(), |name| name.encode_utf16().count());
        total
            .saturating_add(size)
            .saturating_add(32)
            .saturating_add(3 * name as u64)
    });
    vint_len(estimate.saturating_mul(4096))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_widths_match_what_rar_exe_wrote() {
        for (size, width) in [
            (0, 2),
            (127, 2),
            (128, 2),
            (7679, 2),
            (7680, 3),
            (70_000, 3),
            (1_048_063, 3),
            (1_048_064, 4),
            (1_048_575, 4),
            (1_048_576, 5),
            (2_097_152, 5),
        ] {
            assert_eq!(size_width(size), width, "size {size}");
        }
    }

    #[test]
    fn offset_widths_match_what_rar_exe_wrote() {
        let entry = |name: &str, size: usize| {
            ArchiveEntry::new(name, crate::rar::EntrySource::from_bytes(vec![0; size]))
        };
        assert_eq!(offset_width(&[entry("f", 475)]), 3);
        assert_eq!(offset_width(&[entry("f", 476)]), 4);
        assert_eq!(offset_width(&[entry("f", 65_499)]), 4);
        assert_eq!(offset_width(&[entry("f", 65_500)]), 5);
        assert_eq!(offset_width(&[entry("a", 1), entry("b", 439)]), 3);
        assert_eq!(offset_width(&[entry("a", 1), entry("b", 440)]), 4);
        let long = "abcdefghijklmnopqrstuvwxyz0123456789";
        assert_eq!(offset_width(&[entry(long, 370)]), 3);
        assert_eq!(offset_width(&[entry(long, 371)]), 4);
    }
}
