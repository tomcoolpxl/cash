//! What rar lists of an archive, read from cash-archive's RAR headers: the archive's
//! own facts and an item a header, each as rar words it.

use cash_archive::rar::{self, rar13, rar15_40, rar50};
use cash_core::timefmt::Zone;
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};

/// What a header holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    File,
    Directory,
    Service,
    UnixLink,
    WindowsLink,
    Junction,
    HardLink,
    FileCopy,
}

impl Kind {
    /// rar's word for it in a technical listing.
    pub(super) const fn word(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Directory => "Directory",
            Self::Service => "Service",
            Self::UnixLink => "Unix symbolic link",
            Self::WindowsLink => "Windows symbolic link",
            Self::Junction => "NTFS junction point",
            Self::HardLink => "Hard link",
            Self::FileCopy => "File reference",
        }
    }
}

/// A FILETIME as rar reads it: rar keeps times as nanoseconds in 64 bits, so one after
/// 2185, a damaged header's say, wraps around (seen: a damaged date shown as 1899).
pub(super) const fn rar_ticks(ticks: u64) -> u64 {
    ticks.wrapping_mul(100) / 100
}

/// A file's checksum, as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Hash {
    None,
    Crc32(u32),
    Blake2(Vec<u8>),
}

/// A time as rar shows it: the wall clock in the zone, with its nanoseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Stamp {
    pub(super) wall: NaiveDateTime,
    pub(super) nanos: u32,
}

impl Stamp {
    /// `YYYY-MM-DD HH:MM`, a column's.
    pub(super) fn short(self) -> String {
        self.wall.format("%Y-%m-%d %H:%M").to_string()
    }

    /// `YYYY-MM-DD HH:MM:SS,nnnnnnnnn`, a technical listing's.
    pub(super) fn long(self) -> String {
        format!(
            "{},{:09}",
            self.wall.format("%Y-%m-%d %H:%M:%S"),
            self.nanos
        )
    }

    /// A UTC time from Unix seconds and nanoseconds, on the zone's wall clock.
    fn from_unix(zone: &Zone, seconds: i64, nanos: u32) -> Option<Self> {
        let utc = DateTime::<Utc>::from_timestamp(seconds, 0)?;
        Some(Self {
            wall: zone.to_local(utc),
            nanos,
        })
    }

    /// A Windows FILETIME on the zone's wall clock, as rar reads it.
    fn from_filetime(zone: &Zone, ticks: u64) -> Option<Self> {
        let ticks = rar_ticks(ticks);
        let seconds = i64::try_from(ticks / 10_000_000).ok()? - 11_644_473_600;
        let nanos = u32::try_from(ticks % 10_000_000).ok()? * 100;
        Self::from_unix(zone, seconds, nanos)
    }

    /// An MS-DOS time, kept as a local wall clock already.
    fn from_dos(time: u32, nanos: u32, add_second: bool) -> Option<Self> {
        let date = NaiveDate::from_ymd_opt(
            i32::try_from(1980 + (time >> 25)).ok()?,
            (time >> 21) & 0xF,
            (time >> 16) & 0x1F,
        )?;
        let wall = date.and_hms_opt((time >> 11) & 0x1F, (time >> 5) & 0x3F, (time & 0x1F) * 2)?;
        let wall = if add_second {
            wall + chrono::Duration::seconds(1)
        } else {
            wall
        };
        Some(Self { wall, nanos })
    }
}

/// One header as rar lists it.
#[derive(Clone, Debug)]
pub(super) struct Item {
    pub(super) name: String,
    pub(super) kind: Kind,
    /// A folder, or a link standing for one: listed without sizes.
    pub(super) folder: bool,
    /// Its header failed its checksum and was read as it stands.
    pub(super) damaged: bool,
    pub(super) target: Option<String>,
    pub(super) size: u64,
    pub(super) packed: u64,
    pub(super) split_before: bool,
    pub(super) split_after: bool,
    pub(super) modified: Option<Stamp>,
    /// The creation and access times `-ts` keeps.
    pub(super) created: Option<Stamp>,
    pub(super) accessed: Option<Stamp>,
    pub(super) attributes: String,
    pub(super) hash: Hash,
    pub(super) mac: bool,
    pub(super) host: Option<&'static str>,
    pub(super) compression: String,
    pub(super) solid: bool,
    pub(super) encrypted: bool,
    /// A recovery record's share of the archive, its `RR%`.
    pub(super) recovery_percent: Option<u64>,
    /// An older version of a file (`-ver`), its name ending in `;N`.
    pub(super) version: Option<u64>,
}

impl Item {
    /// Whether a listing names it among the files: not a service.
    pub(super) fn is_file_like(&self) -> bool {
        self.kind != Kind::Service
    }
}

/// The archive's own facts, from its main header and its end.
#[derive(Clone, Debug, Default)]
pub(super) struct Facts {
    /// `RAR 5`, `RAR 1.5`, `RAR 1.4`.
    pub(super) format: &'static str,
    pub(super) solid: bool,
    pub(super) volume: bool,
    /// RAR 5's volume number, counted from 1.
    pub(super) volume_number: Option<u64>,
    pub(super) recovery: bool,
    pub(super) locked: bool,
    pub(super) encrypted_headers: bool,
    pub(super) first_volume: bool,
    pub(super) new_numbering: bool,
    pub(super) original_name: Option<String>,
    pub(super) original_time: Option<Stamp>,
    /// Whether the archive has its end header.
    pub(super) end: bool,
    /// The volume number a RAR 4 end header gives, from 0.
    pub(super) end_volume: Option<u64>,
    /// Whether another volume follows.
    pub(super) next_volume: bool,
}

impl Facts {
    /// The `Details:` line's words.
    pub(super) fn details(&self) -> String {
        let mut text = self.format.to_owned();
        if self.solid {
            text.push_str(", solid");
        }
        if self.volume {
            match self.volume_number {
                Some(number) => {
                    text.push_str(", volume ");
                    text.push_str(&number.to_string());
                }
                None => text.push_str(", volume"),
            }
        }
        if self.recovery {
            text.push_str(", recovery record");
        }
        if self.locked {
            text.push_str(", lock");
        }
        if self.encrypted_headers {
            text.push_str(", encrypted headers");
        }
        text
    }
}

/// Windows' attributes as rar spells them, `ICADSHR`, a dot for each not set.
pub(super) fn windows_attributes(attributes: u64) -> String {
    [
        (0x2000, 'I'),
        (0x800, 'C'),
        (0x20, 'A'),
        (0x10, 'D'),
        (0x4, 'S'),
        (0x2, 'H'),
        (0x1, 'R'),
    ]
    .iter()
    .map(|&(bit, c)| if attributes & bit != 0 { c } else { '.' })
    .collect()
}

/// A Unix mode: `-rw-r--r--`, `drwxr-xr-x`, `lrwxrwxrwx`.
pub(super) fn unix_attributes(mode: u64) -> String {
    let kind = match mode & 0o170_000 {
        0o140_000 => 's',
        0o120_000 => 'l',
        0o060_000 => 'b',
        0o040_000 => 'd',
        0o020_000 => 'c',
        0o010_000 => 'p',
        _ => '-',
    };
    let mut text = String::with_capacity(10);
    text.push(kind);
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 7;
        text.push(if bits & 4 != 0 { 'r' } else { '-' });
        text.push(if bits & 2 != 0 { 'w' } else { '-' });
        text.push(if bits & 1 != 0 { 'x' } else { '-' });
    }
    let mut bytes: Vec<char> = text.chars().collect();
    if mode & 0o4000 != 0 {
        bytes[3] = if mode & 0o100 != 0 { 's' } else { 'S' };
    }
    if mode & 0o2000 != 0 {
        bytes[6] = if mode & 0o010 != 0 { 's' } else { 'S' };
    }
    if mode & 0o1000 != 0 {
        bytes[9] = if mode & 0o001 != 0 { 't' } else { 'T' };
    }
    bytes.into_iter().collect()
}

/// A dictionary size as `-md=` shows it: whole gigabytes, megabytes or kilobytes.
pub(super) fn dictionary(size: u64) -> String {
    if size.is_multiple_of(1 << 30) {
        format!("{}g", size >> 30)
    } else if size.is_multiple_of(1 << 20) {
        format!("{}m", size >> 20)
    } else {
        format!("{}k", size >> 10)
    }
}

/// A RAR 5 header's items.
pub(super) fn rar5_items(archive: &rar50::Archive, zone: &Zone) -> Vec<Item> {
    archive
        .blocks
        .iter()
        .filter_map(|block| match block {
            rar50::Block::File(header) => Some(rar5_item(header, false, zone)),
            rar50::Block::Service(header) => Some(rar5_item(header, true, zone)),
            _ => None,
        })
        .collect()
}

/// RAR 5's archive facts.
pub(super) fn rar5_facts(archive: &rar50::Archive, zone: &Zone) -> Facts {
    let main = &archive.main;
    let end = archive.blocks.iter().find_map(|block| match block {
        rar50::Block::End(end) => Some(end),
        _ => None,
    });
    let metadata = main.archive_metadata();
    Facts {
        format: "RAR 5",
        solid: main.is_solid(),
        volume: main.is_volume(),
        volume_number: main
            .is_volume()
            .then(|| main.volume_number.map_or(1, |n| n + 1)),
        recovery: main.has_recovery_record(),
        locked: main.is_locked(),
        encrypted_headers: main.encrypted_headers,
        first_volume: main.volume_number.unwrap_or(0) == 0,
        new_numbering: true,
        original_name: metadata
            .and_then(|m| m.name.as_ref())
            .filter(|name| name.first().is_some_and(|&b| b != 0))
            .map(|name| {
                let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
                String::from_utf8_lossy(name.get(..end).unwrap_or_default()).into_owned()
            }),
        original_time: metadata
            .and_then(|m| m.creation_filetime())
            .and_then(|ticks| Stamp::from_filetime(zone, ticks)),
        end: end.is_some(),
        end_volume: None,
        next_volume: end.is_some_and(rar50::EndHeader::has_next_volume),
    }
}

/// A RAR 5 time as a listing shows it, in the zone.
fn stamp_of(zone: &Zone, stamp: rar::FileTimestamp) -> Option<Stamp> {
    match stamp {
        rar::FileTimestamp::Unix {
            seconds,
            nanoseconds,
        } => Stamp::from_unix(zone, i64::from(seconds), nanoseconds),
        rar::FileTimestamp::UnixSeconds(seconds) => Stamp::from_unix(zone, i64::from(seconds), 0),
        rar::FileTimestamp::WindowsFiletime(ticks) => Stamp::from_filetime(zone, ticks),
    }
}

/// A RAR 5 member's compression as lt shows it: `RAR 5.0(v50) -m3 -md=4m`.
fn rar5_compression(info: u64, directory: bool) -> String {
    let version = info & 0x3F;
    let method = (info >> 7) & 7;
    let power = (info >> 10) & 0x1F;
    let fraction = (info >> 15) & 0x1F;
    let (label, dict) = match version {
        0 => ("v50", Some((128u64 << 10) << power)),
        1 => (
            "v70",
            Some(((128u64 << 10) << power) / 32 * (32 + fraction)),
        ),
        _ => ("v0", Some(0)),
    };
    let mut compression = format!("RAR 5.0({label}) -m{method}");
    if !directory && let Some(dict) = dict {
        compression.push_str(" -md=");
        compression.push_str(&dictionary(dict));
    }
    compression
}

fn rar5_item(header: &rar50::FileHeader, service: bool, zone: &Zone) -> Item {
    let solid = header.compression_info & 0x40 != 0;
    let directory = header.is_directory();
    let kind = if service {
        Kind::Service
    } else {
        match header.redirection.as_ref().map(|r| r.redirection_type) {
            Some(1) => Kind::UnixLink,
            Some(2) => Kind::WindowsLink,
            Some(3) => Kind::Junction,
            Some(4) => Kind::HardLink,
            Some(5) => Kind::FileCopy,
            _ if directory => Kind::Directory,
            _ => Kind::File,
        }
    };
    let compression = rar5_compression(header.compression_info, directory);
    let host = match header.host_os {
        0 => Some("Windows"),
        1 => Some("Unix"),
        _ => None,
    };
    let attributes = if service {
        ".B".to_owned()
    } else if header.host_os == 1 {
        unix_attributes(header.attributes)
    } else {
        windows_attributes(header.attributes)
    };
    let times = header.file_times.filter(|_| !service).unwrap_or_default();
    let modified = if service {
        None
    } else if let Some(stamp) = times.modified {
        stamp_of(zone, stamp)
    } else {
        header
            .htime_mtime
            .or(header.mtime)
            .and_then(|seconds| Stamp::from_unix(zone, i64::from(seconds), 0))
    };
    let hash = if let Some(hash) = &header.hash
        && hash.hash_type == 0
    {
        Hash::Blake2(hash.data.clone())
    } else if let Some(crc) = header.data_crc32 {
        Hash::Crc32(crc)
    } else {
        Hash::None
    };
    let mac = header
        .encryption
        .as_ref()
        .is_some_and(|e| e.flags & 0x02 != 0);
    let recovery_percent = (service && header.name == b"RR")
        .then(|| header.recovery_record().ok().flatten().map(|r| r.percent))
        .flatten();
    let mut name = String::from_utf8_lossy(&header.name).into_owned();
    if let Some(version) = header.version {
        name = format!("{name};{version}");
    }
    Item {
        name,
        kind,
        folder: directory,
        damaged: header.block.damaged,
        target: header
            .redirection
            .as_ref()
            .map(|r| String::from_utf8_lossy(&r.target_name).into_owned()),
        size: header.unpacked_size,
        packed: header.packed_size(),
        split_before: header.is_split_before(),
        split_after: header.is_split_after(),
        modified,
        created: times.created.and_then(|stamp| stamp_of(zone, stamp)),
        accessed: times.accessed.and_then(|stamp| stamp_of(zone, stamp)),
        attributes,
        hash,
        mac,
        host,
        compression,
        solid,
        encrypted: header.encrypted,
        recovery_percent,
        version: header.version,
    }
}

/// RAR 1.5 to 4's archive facts; `read` gives the archive's bytes at an offset, for the
/// volume number the end header keeps.
pub(super) fn rar4_facts(
    archive: &rar15_40::Archive,
    read: impl Fn(u64, usize) -> Option<Vec<u8>>,
) -> Facts {
    let main = &archive.main;
    let end = archive.blocks.iter().find_map(|block| match block {
        rar15_40::Block::End(end) => Some(end),
        _ => None,
    });
    let sfx = archive.sfx_offset as u64;
    Facts {
        format: "RAR 1.5",
        solid: main.is_solid(),
        volume: main.is_volume(),
        volume_number: None,
        recovery: main.has_recovery_record(),
        locked: main.flags & 0x0004 != 0,
        encrypted_headers: main.has_encrypted_headers(),
        first_volume: main.is_first_volume(),
        new_numbering: main.uses_new_numbering(),
        original_name: None,
        original_time: None,
        end: end.is_some(),
        end_volume: end.and_then(|end| {
            // The volume's number, after the data's CRC when there is one.
            (end.flags & 0x0008 != 0)
                .then(|| {
                    let at =
                        sfx + end.offset as u64 + 7 + if end.flags & 0x0002 != 0 { 4 } else { 0 };
                    let bytes = read(at, 2)?;
                    Some(u64::from(u16::from_le_bytes([
                        *bytes.first()?,
                        *bytes.get(1)?,
                    ])))
                })
                .flatten()
        }),
        next_volume: end.is_some_and(|end| end.flags & 0x0001 != 0),
    }
}

/// A RAR 1.5 to 4 archive's items.
pub(super) fn rar4_items(archive: &rar15_40::Archive) -> Vec<Item> {
    archive
        .blocks
        .iter()
        .filter_map(|block| match block {
            rar15_40::Block::File(header) => Some(rar4_item(header, false)),
            rar15_40::Block::NewSub(sub) => Some(rar4_item(&sub.file, true)),
            _ => None,
        })
        .collect()
}

fn rar4_item(header: &rar15_40::FileHeader, service: bool) -> Item {
    let directory = header.is_directory();
    let kind = if service {
        Kind::Service
    } else if directory {
        Kind::Directory
    } else if header.host_os == 3 && header.attr & 0o170_000 == 0o120_000 {
        Kind::UnixLink
    } else {
        Kind::File
    };
    let method = header.method.saturating_sub(0x30);
    let dict_bits = (header.block.flags >> 5) & 7;
    let mut compression = format!("RAR 1.5(v{}) -m{method}", header.unp_ver);
    if dict_bits != 7 {
        compression.push_str(" -md=");
        compression.push_str(&dictionary(64u64 << 10 << dict_bits));
    }
    let host = match header.host_os {
        0 => Some("DOS"),
        1 => Some("OS/2"),
        2 => Some("Windows"),
        3 => Some("Unix"),
        4 => Some("Mac OS"),
        5 => Some("BeOS"),
        _ => None,
    };
    let attributes = if service {
        ".B".to_owned()
    } else if matches!(header.host_os, 3 | 5) {
        unix_attributes(u64::from(header.attr))
    } else {
        windows_attributes(u64::from(header.attr))
    };
    let refinement = header.mtime_refinement();
    let modified = Stamp::from_dos(
        header.file_time,
        refinement.map_or(0, |r| r.nanoseconds),
        refinement.is_some_and(|r| r.add_second),
    );
    Item {
        name: String::from_utf8_lossy(&header.name).into_owned(),
        kind,
        folder: directory,
        damaged: false,
        target: None,
        size: header.unp_size,
        packed: header.pack_size,
        split_before: header.is_split_before(),
        split_after: header.is_split_after(),
        modified,
        created: header
            .ctime()
            .and_then(|(time, r)| Stamp::from_dos(time, r.nanoseconds, r.add_second)),
        accessed: header
            .atime()
            .and_then(|(time, r)| Stamp::from_dos(time, r.nanoseconds, r.add_second)),
        attributes,
        hash: Hash::Crc32(header.file_crc),
        mac: false,
        host,
        compression,
        solid: header.is_solid(),
        encrypted: header.is_encrypted(),
        recovery_percent: None,
        version: None,
    }
}

/// RAR 1.3's archive facts.
pub(super) fn rar13_facts(archive: &rar13::Archive) -> Facts {
    Facts {
        format: "RAR 1.4",
        solid: archive.main.flags & 0x0008 != 0,
        volume: archive.main.is_volume(),
        locked: archive.main.flags & 0x0004 != 0,
        first_volume: true,
        ..Facts::default()
    }
}

/// A RAR 1.3 archive's items.
pub(super) fn rar13_items(archive: &rar13::Archive) -> Vec<Item> {
    archive
        .entries
        .iter()
        .map(|entry| {
            let header = &entry.header;
            let directory = header.file_attr & 0x10 != 0;
            let mut compression = format!("RAR 5.0(v13) -m{}", header.method);
            if !directory {
                compression.push_str(" -md=64k");
            }
            Item {
                name: String::from_utf8_lossy(&entry.name).into_owned(),
                folder: directory,
                damaged: false,
                kind: if directory {
                    Kind::Directory
                } else {
                    Kind::File
                },
                target: None,
                size: u64::from(header.unp_size),
                packed: u64::from(header.pack_size),
                split_before: header.flags & 0x01 != 0,
                split_after: header.flags & 0x02 != 0,
                modified: Stamp::from_dos(header.file_time, 0, false),
                created: None,
                accessed: None,
                attributes: windows_attributes(u64::from(header.file_attr)),
                hash: Hash::None,
                mac: false,
                host: None,
                compression,
                // rar does not say which RAR 1.3 files are solid.
                solid: false,
                encrypted: header.flags & 0x04 != 0,
                recovery_percent: None,
                version: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_attributes_read_icadshr() {
        assert_eq!(windows_attributes(0x20), "..A....");
        assert_eq!(windows_attributes(0x10), "...D...");
        assert_eq!(windows_attributes(0x27), "..A.SHR");
        assert_eq!(windows_attributes(0x2000), "I......");
        assert_eq!(windows_attributes(0x820), ".CA....");
        assert_eq!(windows_attributes(0x1100), ".......");
    }

    #[test]
    fn unix_modes_read_as_ls_does() {
        assert_eq!(unix_attributes(0o100_644), "-rw-r--r--");
        assert_eq!(unix_attributes(0o120_755), "lrwxr-xr-x");
        assert_eq!(unix_attributes(0o040_755), "drwxr-xr-x");
        assert_eq!(unix_attributes(0o040), "----r-----");
    }

    #[test]
    fn dictionaries_in_their_largest_whole_unit() {
        assert_eq!(dictionary(128 << 10), "128k");
        assert_eq!(dictionary(4 << 20), "4m");
        assert_eq!(dictionary(1 << 30), "1g");
        assert_eq!(dictionary(0), "0g");
    }

    #[test]
    fn dos_times_are_wall_clocks() {
        // 2026-04-27 07:09:00.
        let time = (46 << 25) | (4 << 21) | (27 << 16) | (7 << 11) | (9 << 5);
        let stamp = Stamp::from_dos(time, 0, false).unwrap();
        assert_eq!(stamp.long(), "2026-04-27 07:09:00,000000000");
        assert_eq!(stamp.short(), "2026-04-27 07:09");
    }
}
