//! zip archives as Info-ZIP's zip 3.0 writes them and unzip 6.00 reads them: the
//! records, the extra fields both use, MS-DOS times, and the traditional encryption.
//!
//! The library reads and writes records and data; what a tool says about them, and how
//! it names a file on disk, is the front end's.

pub mod crypt;
pub mod read;
pub mod write;

/// A local file header's signature, `PK\3\4`.
pub const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
/// A central directory record's signature, `PK\1\2`.
pub const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
/// The end of central directory record's signature, `PK\5\6`.
pub const END_SIGNATURE: u32 = 0x0605_4b50;
/// The Zip64 end of central directory record's signature, `PK\6\6`.
pub const ZIP64_END_SIGNATURE: u32 = 0x0606_4b50;
/// The Zip64 end of central directory locator's signature, `PK\6\7`.
pub const ZIP64_LOCATOR_SIGNATURE: u32 = 0x0706_4b50;
/// A data descriptor's optional signature, `PK\7\8`.
pub const DESCRIPTOR_SIGNATURE: u32 = 0x0807_4b50;

/// Compression methods, by number.
pub mod method {
    /// Stored as it is.
    pub const STORED: u16 = 0;
    /// PKZIP 1's shrinking.
    pub const SHRUNK: u16 = 1;
    /// PKZIP 1's imploding.
    pub const IMPLODED: u16 = 6;
    /// Deflate.
    pub const DEFLATED: u16 = 8;
    /// Deflate64, which Windows' Explorer writes for large files.
    pub const DEFLATE64: u16 = 9;
    /// bzip2.
    pub const BZIP2: u16 = 12;
    /// LZMA, with zip's own small header.
    pub const LZMA: u16 = 14;
    /// zstd.
    pub const ZSTD: u16 = 93;
    /// xz.
    pub const XZ: u16 = 95;
    /// `PPMd`.
    pub const PPMD: u16 = 98;
    /// `WinZip`'s AES encryption, the real method in its extra field.
    pub const AES: u16 = 99;
}

/// General purpose flags.
pub mod flag {
    /// The data is encrypted.
    pub const ENCRYPTED: u16 = 0x0001;
    /// Deflate's maximum compression (with [`FAST`], super fast).
    pub const SLOW: u16 = 0x0002;
    /// Deflate's fast compression.
    pub const FAST: u16 = 0x0004;
    /// The CRC and sizes follow the data in a descriptor.
    pub const DESCRIPTOR: u16 = 0x0008;
    /// The name and comment are UTF-8.
    pub const UTF8: u16 = 0x0800;
}

/// The hosts a record says it was made on: the high byte of "version made by".
pub mod host {
    /// MS-DOS, OS/2 and Windows' FAT.
    pub const MSDOS: u8 = 0;
    /// Unix.
    pub const UNIX: u8 = 3;
    /// Windows' NTFS.
    pub const NTFS: u8 = 11;
}

/// Extra field ids.
pub mod extra_id {
    /// Zip64's sizes and offset.
    pub const ZIP64: u16 = 0x0001;
    /// Windows' NTFS times.
    pub const NTFS: u16 = 0x000a;
    /// Info-ZIP's universal times (`UT`).
    pub const TIMES: u16 = 0x5455;
    /// Info-ZIP's old Unix owner and times (`UX`).
    pub const UNIX_OLD: u16 = 0x5855;
    /// Info-ZIP's Unix owner of any size (`ux`).
    pub const UNIX_OWNER: u16 = 0x7875;
    /// Info-ZIP's UTF-8 path (`up`).
    pub const UNICODE_PATH: u16 = 0x7075;
    /// `WinZip`'s AES.
    pub const AES: u16 = 0x9901;
}

/// "Version made by" for Info-ZIP's zip 3.0 on Unix.
pub const MADE_BY_ZIP_3_UNIX: u16 = (host::UNIX as u16) << 8 | 0x1e;

/// The Unix type bits of a mode.
pub const S_IFMT: u32 = 0o170_000;
/// A folder's type bits.
pub const S_IFDIR: u32 = 0o040_000;
/// A file's type bits.
pub const S_IFREG: u32 = 0o100_000;
/// A symbolic link's type bits.
pub const S_IFLNK: u32 = 0o120_000;
/// A pipe's type bits.
pub const S_IFIFO: u32 = 0o010_000;

/// MS-DOS's attribute for a folder.
pub const DOS_DIRECTORY: u8 = 0x10;
/// MS-DOS's read-only attribute.
pub const DOS_READ_ONLY: u8 = 0x01;

/// A date and time on the wall, as MS-DOS keeps one.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct Civil {
    /// The year, four digits.
    pub year: i32,
    /// 1 to 12.
    pub month: u32,
    /// 1 to 31.
    pub day: u32,
    /// 0 to 23.
    pub hour: u32,
    /// 0 to 59.
    pub minute: u32,
    /// 0 to 59.
    pub second: u32,
}

/// An MS-DOS date and time: local, to two seconds, 1980 to 2107.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DosTime {
    /// Year since 1980 (7 bits), month (4), day (5).
    pub date: u16,
    /// Hour (5 bits), minute (6), second halved (5).
    pub time: u16,
}

impl DosTime {
    /// A wall time as MS-DOS keeps it: the seconds halved, the years held to 1980 and
    /// 2107 as Info-ZIP holds them.
    pub fn from_civil(civil: Civil) -> Self {
        if civil.year < 1980 {
            return Self {
                date: (1 << 5) | 1,
                time: 0,
            };
        }
        let year = u16::try_from((civil.year - 1980).min(127)).unwrap_or(127);
        let small = |value: u32, max: u32| u16::try_from(value.min(max)).unwrap_or(0);
        Self {
            date: year << 9 | small(civil.month, 15) << 5 | small(civil.day, 31),
            time: small(civil.hour, 31) << 11
                | small(civil.minute, 63) << 5
                | small(civil.second / 2, 31),
        }
    }

    /// The wall time it names.
    pub fn civil(self) -> Civil {
        Civil {
            year: 1980 + i32::from(self.date >> 9),
            month: u32::from(self.date >> 5 & 0x0f),
            day: u32::from(self.date & 0x1f),
            hour: u32::from(self.time >> 11),
            minute: u32::from(self.time >> 5 & 0x3f),
            second: u32::from(self.time & 0x1f) * 2,
        }
    }
}

/// One record of the central directory, with everything it holds.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Entry {
    /// The host (high byte) and the version of the software (low byte, times ten).
    pub version_made_by: u16,
    /// The host and version needed to extract it.
    pub version_needed: u16,
    /// General purpose flags: [`flag`].
    pub flags: u16,
    /// The compression method: [`method`].
    pub method: u16,
    /// The last change, local, to two seconds.
    pub time: DosTime,
    /// The CRC-32 of the data uncompressed.
    pub crc: u32,
    /// The size of the data as stored, encryption's header included.
    pub compressed_size: u64,
    /// The size of the data uncompressed.
    pub size: u64,
    /// The name, `/` between its parts, a folder's ending in `/`.
    pub name: Vec<u8>,
    /// The central directory's extra field.
    pub extra: Vec<u8>,
    /// The entry's comment.
    pub comment: Vec<u8>,
    /// The disk the entry starts on.
    pub disk_start: u32,
    /// Bit 0: the data is text.
    pub internal_attributes: u16,
    /// The host's attributes: on Unix the mode in the high half, MS-DOS's in the low byte.
    pub external_attributes: u32,
    /// Where the local header starts, from the start of the archive proper.
    pub local_offset: u64,
}

impl Entry {
    /// The host it says it was made on.
    pub const fn host(&self) -> u8 {
        self.version_made_by.to_be_bytes()[0]
    }

    /// Whether it names a folder: its name ends in `/`.
    pub fn is_dir(&self) -> bool {
        self.name.last() == Some(&b'/')
    }

    /// Whether its data is encrypted.
    pub const fn is_encrypted(&self) -> bool {
        self.flags & flag::ENCRYPTED != 0
    }

    /// Whether a descriptor follows its data.
    pub const fn has_descriptor(&self) -> bool {
        self.flags & flag::DESCRIPTOR != 0
    }

    /// Whether the archive calls it text.
    pub const fn is_text(&self) -> bool {
        self.internal_attributes & 1 != 0
    }

    /// The Unix mode, when the host keeps one and the record has it.
    pub fn unix_mode(&self) -> Option<u32> {
        let mode = self.external_attributes >> 16;
        unix_like(self.host()).then_some(mode).filter(|m| *m != 0)
    }

    /// MS-DOS's attributes.
    pub const fn dos_attributes(&self) -> u8 {
        self.external_attributes.to_le_bytes()[0]
    }

    /// Whether it is a symbolic link: its Unix mode says so.
    pub fn is_symlink(&self) -> bool {
        self.unix_mode().is_some_and(|m| m & S_IFMT == S_IFLNK)
    }

    /// The modification time of its `UT` field, in seconds since 1970.
    pub fn ut_mtime(&self) -> Option<i64> {
        times(&self.extra).and_then(|t| t.modified)
    }
}

/// The hosts whose records carry a Unix mode, as unzip reads them.
pub const fn unix_like(host: u8) -> bool {
    matches!(host, 3 | 5 | 7 | 13 | 16 | 17 | 18 | 19 | 30)
}

/// The fields of an extra field: each id with its data, up to the first that does not
/// fit.
pub fn fields(extra: &[u8]) -> Vec<(u16, &[u8])> {
    let mut out = Vec::new();
    let mut rest = extra;
    while rest.len() >= 4 {
        let id = u16::from_le_bytes([
            rest.first().copied().unwrap_or(0),
            rest.get(1).copied().unwrap_or(0),
        ]);
        let len = usize::from(u16::from_le_bytes([
            rest.get(2).copied().unwrap_or(0),
            rest.get(3).copied().unwrap_or(0),
        ]));
        let Some(data) = rest.get(4..4 + len) else {
            break;
        };
        out.push((id, data));
        rest = rest.get(4 + len..).unwrap_or_default();
    }
    out
}

/// One extra field: its id, its length and its data.
pub fn field(id: u16, data: &[u8]) -> Vec<u8> {
    let mut out = id.to_le_bytes().to_vec();
    out.extend_from_slice(&u16::try_from(data.len()).unwrap_or(u16::MAX).to_le_bytes());
    out.extend_from_slice(data);
    out
}

/// The times of a `UT` field.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Times {
    /// Its flags: bit 0 modification, 1 access, 2 creation.
    pub flags: u8,
    /// The modification time, in seconds since 1970.
    pub modified: Option<i64>,
    /// The access time.
    pub accessed: Option<i64>,
    /// The creation time.
    pub created: Option<i64>,
}

/// The `UT` field's times, as many as it holds.
pub fn times(extra: &[u8]) -> Option<Times> {
    let (_, data) = fields(extra)
        .into_iter()
        .find(|(id, _)| *id == extra_id::TIMES)?;
    let flags = *data.first()?;
    let mut out = Times {
        flags,
        ..Times::default()
    };
    let mut at = 1;
    let mut next = |present: bool| {
        if !present {
            return None;
        }
        let bytes = data.get(at..at + 4)?;
        at += 4;
        Some(i64::from(i32::from_le_bytes([
            bytes.first().copied().unwrap_or(0),
            bytes.get(1).copied().unwrap_or(0),
            bytes.get(2).copied().unwrap_or(0),
            bytes.get(3).copied().unwrap_or(0),
        ])))
    };
    out.modified = next(flags & 1 != 0);
    out.accessed = next(flags & 2 != 0);
    out.created = next(flags & 4 != 0);
    Some(out)
}

/// The owner and group of a `ux` field.
pub fn owner(extra: &[u8]) -> Option<(u64, u64)> {
    let (_, data) = fields(extra)
        .into_iter()
        .find(|(id, _)| *id == extra_id::UNIX_OWNER)?;
    if data.first() != Some(&1) {
        return None;
    }
    let mut at = 1;
    let mut number = || {
        let len = usize::from(*data.get(at)?);
        let bytes = data.get(at + 1..at + 1 + len)?;
        at += 1 + len;
        let mut value = 0_u64;
        for (i, b) in bytes.iter().enumerate().take(8) {
            value |= u64::from(*b) << (8 * i);
        }
        Some(value)
    };
    let uid = number()?;
    let gid = number()?;
    Some((uid, gid))
}

/// Unsigned little-endian numbers from a slice, short reads as zero.
pub(crate) fn le16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([
        bytes.get(at).copied().unwrap_or(0),
        bytes.get(at + 1).copied().unwrap_or(0),
    ])
}

pub(crate) fn le32(bytes: &[u8], at: usize) -> u32 {
    u32::from(le16(bytes, at)) | u32::from(le16(bytes, at + 2)) << 16
}

pub(crate) fn le64(bytes: &[u8], at: usize) -> u64 {
    u64::from(le32(bytes, at)) | u64::from(le32(bytes, at + 4)) << 32
}

/// Info-ZIP's `percent`: the saving from `size` to `compressed`, rounded as zip prints
/// it, negative when the data grew.
pub fn percent(size: u64, compressed: u64) -> i64 {
    let (mut n, mut m) = (i128::from(size), i128::from(compressed));
    if n > 0x00ff_ffff {
        n = (n + 0x80) >> 8;
        m = (m + 0x80) >> 8;
    }
    if n == 0 {
        return 0;
    }
    i64::try_from((1 + 200 * (n - m) / n) / 2).unwrap_or(0)
}

/// What Info-ZIP's zip calls text: no byte it black-lists (0 to 6, 14 to 25, 28 to 31),
/// and at least one it white-lists (tab, line feed, carriage return, or 32 and up).
#[derive(Clone, Copy, Debug, Default)]
pub struct TextCheck {
    black: bool,
    white: bool,
}

impl TextCheck {
    /// Takes in more of the data.
    pub fn update(&mut self, bytes: &[u8]) {
        if self.black {
            return;
        }
        for b in bytes {
            match b {
                0..=6 | 14..=25 | 28..=31 => {
                    self.black = true;
                    return;
                }
                9 | 10 | 13 | 32.. => self.white = true,
                _ => {}
            }
        }
    }

    /// Whether all of it was text.
    pub const fn is_text(self) -> bool {
        self.white && !self.black
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dos_times_round_trip_to_two_seconds() {
        let civil = Civil {
            year: 2020,
            month: 1,
            day: 2,
            hour: 3,
            minute: 4,
            second: 6,
        };
        let dos = DosTime::from_civil(civil);
        assert_eq!((dos.date, dos.time), (0x5022, 0x1883));
        assert_eq!(dos.civil(), civil);
        let early = DosTime::from_civil(Civil {
            year: 1970,
            ..civil
        });
        assert_eq!(early.civil().year, 1980);
    }

    #[test]
    fn percentages_round_as_zip_rounds_them() {
        assert_eq!(percent(3000, 21), 99);
        assert_eq!(percent(70001, 86), 100);
        assert_eq!(percent(6, 8), -32);
        assert_eq!(percent(0, 2), 0);
        assert_eq!(percent(6, 6), 0);
    }

    #[test]
    fn extra_fields_are_read_as_info_zip_writes_them() {
        let mut extra = field(extra_id::TIMES, &[3, 0x45, 0x5e, 0x0d, 0x5e, 1, 0, 0, 0]);
        extra.extend(field(
            extra_id::UNIX_OWNER,
            &[1, 4, 0xe8, 3, 0, 0, 4, 0xe8, 3, 0, 0],
        ));
        let t = times(&extra).unwrap_or_default();
        assert_eq!(t.modified, Some(0x5e0d_5e45));
        assert_eq!(t.accessed, Some(1));
        assert_eq!(owner(&extra), Some((1000, 1000)));
        assert_eq!(fields(&extra).len(), 2);
    }

    #[test]
    fn text_is_what_zip_calls_text() {
        let check = |bytes: &[u8]| {
            let mut t = TextCheck::default();
            t.update(bytes);
            t.is_text()
        };
        assert!(check(b"hello\n"));
        assert!(check(b"a\x07b"));
        assert!(check(b"\x1b[0m"));
        assert!(!check(b"\x07\x08"));
        assert!(!check(b"x\x00"));
        assert!(!check(b""));
    }
}
