//! What `t`, `x`, `e` and `p` work through: an archive set's files in order, each with
//! its packed parts across the volumes, its method, encryption and checksums, read from
//! cash-archive's RAR headers.

use std::path::PathBuf;

use cash_archive::rar::{self, rar13, rar15_40, rar50};

use crate::rardata::{Part, PartCheck};

/// One volume of a set, open.
pub(super) struct Volume {
    pub(super) display: String,
    pub(super) path: PathBuf,
    pub(super) archive: rar::Archive,
}

/// A link's kind and target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Link {
    Unix(String),
    Windows(String),
    Junction(String),
    Hard(String),
    Copy(String),
}

/// How a file's data is packed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Method {
    Stored,
    /// RAR 5 and 7's, its algorithm version and dictionary.
    Lz5 {
        version: u8,
        dictionary: u64,
    },
    /// RAR 1.3 to 4's, its unpack version.
    Lz4 {
        version: u8,
    },
    /// A method this RAR does not have.
    Unknown,
}

/// How a file's data is encrypted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Crypto {
    Rar5 {
        salt: [u8; 16],
        iv: [u8; 16],
        count: u8,
        check: Option<[u8; 12]>,
        mac: bool,
    },
    Rar3 {
        salt: Option<[u8; 8]>,
    },
    Rar2,
    Rar15,
    Rar13,
}

/// A time a file keeps: Windows' FILETIME, or MS-DOS's local time with what it lacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Time {
    Utc(u64),
    Dos {
        time: u32,
        nanos: u32,
        add_second: bool,
    },
}

/// Which system's attributes a file keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Host {
    Windows,
    Unix,
    Other,
}

/// One file of the set: its first part's facts, its last part's checksums.
pub(super) struct Entry {
    pub(super) name: String,
    pub(super) directory: bool,
    pub(super) link: Option<Link>,
    /// Whether a symbolic link's target is a folder.
    pub(super) link_to_folder: bool,
    /// The unpacked size; `None` when the archive does not know it.
    pub(super) size: Option<u64>,
    pub(super) parts: Vec<Part>,
    pub(super) method: Method,
    pub(super) solid: bool,
    pub(super) crypto: Option<Crypto>,
    pub(super) crc: Option<u32>,
    pub(super) blake: Option<[u8; 32]>,
    /// Whether the checksums are MACs, made through the password's keys.
    pub(super) mac: bool,
    /// RAR 1.3's own checksum.
    pub(super) sum13: Option<u16>,
    /// A comment of its own, kept by RAR 1.5 to 2.9, which rar 7.23 does not read.
    pub(super) bad_comment: bool,
    pub(super) modified: Option<Time>,
    pub(super) created: Option<Time>,
    pub(super) accessed: Option<Time>,
    pub(super) attributes: u64,
    pub(super) host: Host,
    /// The volume its first part is in.
    pub(super) volume: usize,
    /// An older version of a file (`-ver`): its number, its name ending in `;N`.
    pub(super) version: Option<u64>,
    /// Its header failed its checksum and was read as it stands.
    pub(super) damaged: bool,
}

impl Entry {
    pub(super) const fn encrypted(&self) -> bool {
        self.crypto.is_some()
    }
}

/// The files of a set's volumes, in order, a split file's parts joined.
pub(super) fn entries(volumes: &[Volume]) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for (index, volume) in volumes.iter().enumerate() {
        if let Some(archive) = volume.archive.as_rar50() {
            rar5(archive, index, &mut entries);
        } else if let Some(archive) = volume.archive.as_rar15_40() {
            rar4(archive, index, &mut entries);
        } else if let Some(archive) = volume.archive.as_rar13() {
            rar1(archive, index, &mut entries);
        }
    }
    entries
}

/// The entry a split part continues, if the last one is it.
fn continued<'a>(entries: &'a mut [Entry], name: &str) -> Option<&'a mut Entry> {
    entries.last_mut().filter(|entry| entry.name == name)
}

#[expect(
    clippy::too_many_lines,
    reason = "a RAR 5 file header's facts, read in the header's order"
)]
fn rar5(archive: &rar50::Archive, volume: usize, entries: &mut Vec<Entry>) {
    for block in &archive.blocks {
        let rar50::Block::File(header) = block else {
            continue;
        };
        let mut name = String::from_utf8_lossy(&header.name).into_owned();
        if let Some(version) = header.version {
            name = format!("{name};{version}");
        }
        let crypto = header.encryption.as_ref().map(|e| Crypto::Rar5 {
            salt: e.salt,
            iv: e.iv,
            count: e.kdf_count,
            check: e.check_value,
            mac: e.flags & 0x02 != 0,
        });
        let blake = header
            .hash
            .as_ref()
            .filter(|hash| hash.hash_type == 0)
            .and_then(|hash| <[u8; 32]>::try_from(hash.data.as_slice()).ok());
        let range = &header.block.data_range;
        let part = Part {
            volume,
            pos: range.start as u64,
            size: header.packed_size(),
            check: header.is_split_after().then_some(PartCheck::Hashes {
                crc: header.data_crc32,
                blake,
            }),
        };
        if header.is_split_before()
            && let Some(entry) = continued(entries, &name)
        {
            entry.parts.push(part);
            if !header.is_split_after() {
                entry.crc = header.data_crc32;
                entry.blake = blake;
                entry.mac = header
                    .encryption
                    .as_ref()
                    .is_some_and(|e| e.flags & 0x02 != 0);
                entry.size = header.known_unpacked_size();
            }
            continue;
        }
        let info = header.compression_info;
        let version = (info & 0x3F) as u8;
        let power = (info >> 10) & 0x1F;
        let fraction = (info >> 15) & 0x1F;
        let method = match (version, (info >> 7) & 7) {
            (_, 0) => Method::Stored,
            (0, _) => Method::Lz5 {
                version,
                dictionary: (128 << 10) << power,
            },
            (1, _) => Method::Lz5 {
                version,
                dictionary: ((128 << 10) << power) / 32 * (32 + fraction),
            },
            _ => Method::Unknown,
        };
        let link = header.redirection.as_ref().map(|r| {
            let target = String::from_utf8_lossy(&r.target_name).into_owned();
            match r.redirection_type {
                1 => Link::Unix(target),
                2 => Link::Windows(target),
                3 => Link::Junction(target),
                4 => Link::Hard(target),
                _ => Link::Copy(target),
            }
        });
        let time = |stamp: Option<rar::FileTimestamp>| {
            stamp.map(|stamp| match stamp {
                rar::FileTimestamp::WindowsFiletime(ticks) => {
                    Time::Utc(super::item::rar_ticks(ticks))
                }
                rar::FileTimestamp::Unix {
                    seconds,
                    nanoseconds,
                } => Time::Utc(
                    (u64::from(seconds) + 11_644_473_600) * 10_000_000
                        + u64::from(nanoseconds / 100),
                ),
                rar::FileTimestamp::UnixSeconds(seconds) => {
                    Time::Utc((u64::from(seconds) + 11_644_473_600) * 10_000_000)
                }
            })
        };
        let times = header.file_times.unwrap_or_default();
        let modified = time(times.modified).or_else(|| {
            header
                .htime_mtime
                .or(header.mtime)
                .map(|seconds| Time::Utc((u64::from(seconds) + 11_644_473_600) * 10_000_000))
        });
        entries.push(Entry {
            name,
            directory: header.is_directory(),
            link,
            link_to_folder: header
                .redirection
                .as_ref()
                .is_some_and(|r| r.flags & 1 != 0),
            size: header.known_unpacked_size(),
            parts: vec![part],
            method,
            solid: info & 0x40 != 0,
            crypto,
            crc: header.data_crc32,
            blake,
            mac: header
                .encryption
                .as_ref()
                .is_some_and(|e| e.flags & 0x02 != 0),
            sum13: None,
            damaged: header.block.damaged,
            bad_comment: false,
            modified,
            created: time(times.created),
            accessed: time(times.accessed),
            attributes: header.attributes,
            host: match header.host_os {
                0 => Host::Windows,
                1 => Host::Unix,
                _ => Host::Other,
            },
            volume,
            version: header.version,
        });
    }
}

fn rar4(archive: &rar15_40::Archive, volume: usize, entries: &mut Vec<Entry>) {
    let solid_archive = archive.main.is_solid();
    for block in &archive.blocks {
        let rar15_40::Block::File(header) = block else {
            continue;
        };
        let name = String::from_utf8_lossy(&header.name).replace('\\', "/");
        let part = Part {
            volume,
            pos: header.packed_range.start as u64,
            size: header.pack_size,
            check: (header.is_split_after() && header.unp_ver >= 20)
                .then_some(PartCheck::Crc(header.file_crc)),
        };
        if header.is_split_before()
            && let Some(entry) = continued(entries, &name)
        {
            entry.parts.push(part);
            if !header.is_split_after() {
                entry.crc = Some(header.file_crc);
                entry.size = Some(header.unp_size);
            }
            continue;
        }
        let crypto = header.is_encrypted().then_some(if header.unp_ver >= 29 {
            Crypto::Rar3 { salt: header.salt }
        } else if header.unp_ver >= 20 {
            Crypto::Rar2
        } else {
            Crypto::Rar15
        });
        let method = match header.method {
            0x30 => Method::Stored,
            0x31..=0x35 if header.unp_ver <= 29 || header.unp_ver == 36 => Method::Lz4 {
                version: header.unp_ver,
            },
            _ => Method::Unknown,
        };
        let refinement = header.mtime_refinement();
        let unix_link = matches!(header.host_os, 3 | 5) && header.attr & 0o170_000 == 0o120_000;
        entries.push(Entry {
            name,
            directory: header.is_directory(),
            link: unix_link.then(|| Link::Unix(String::new())),
            link_to_folder: false,
            size: Some(header.unp_size),
            parts: vec![part],
            method,
            solid: header.is_solid() || (header.unp_ver < 20 && solid_archive),
            crypto,
            crc: Some(header.file_crc),
            blake: None,
            mac: false,
            sum13: None,
            damaged: false,
            // rar 7.23 reads no RAR 1.5 to 2.9 file comment, stored or packed.
            bad_comment: header.block.flags & 0x0008 != 0,
            modified: Some(Time::Dos {
                time: header.file_time,
                nanos: refinement.map_or(0, |r| r.nanoseconds),
                add_second: refinement.is_some_and(|r| r.add_second),
            }),
            created: None,
            accessed: None,
            attributes: u64::from(header.attr),
            host: match header.host_os {
                0..=2 | 4 => Host::Windows,
                3 | 5 => Host::Unix,
                _ => Host::Other,
            },
            volume,
            version: None,
        });
    }
}

fn rar1(archive: &rar13::Archive, volume: usize, entries: &mut Vec<Entry>) {
    let solid_archive = archive.main.is_solid();
    for entry13 in &archive.entries {
        let header = &entry13.header;
        let name = String::from_utf8_lossy(&entry13.name).replace('\\', "/");
        let part = Part {
            volume,
            pos: entry13.packed_range.start as u64,
            size: u64::from(header.pack_size),
            check: None,
        };
        if header.flags & 0x01 != 0
            && let Some(entry) = continued(entries, &name)
        {
            entry.parts.push(part);
            if header.flags & 0x02 == 0 {
                entry.sum13 = Some(header.file_crc);
            }
            continue;
        }
        entries.push(Entry {
            name,
            directory: header.file_attr & 0x10 != 0,
            link: None,
            link_to_folder: false,
            size: Some(u64::from(header.unp_size)),
            parts: vec![part],
            method: if header.method == 0 {
                Method::Stored
            } else {
                Method::Lz4 { version: 15 }
            },
            solid: solid_archive,
            crypto: (header.flags & 0x04 != 0).then_some(Crypto::Rar13),
            crc: None,
            blake: None,
            mac: false,
            sum13: Some(header.file_crc),
            damaged: false,
            bad_comment: false,
            modified: Some(Time::Dos {
                time: header.file_time,
                nanos: 0,
                add_second: false,
            }),
            created: None,
            accessed: None,
            attributes: u64::from(header.file_attr),
            host: Host::Windows,
            volume,
            version: None,
        });
    }
}
