//! zip as 7-Zip's zip handler lists and reads it (`ZipHandler.cpp`, `ZipItem.cpp`): the
//! central directory's items with the facts 7-Zip shows (time by the NTFS or Unix extra
//! field, else MS-DOS's; attributes by the host; the method with its encryption and
//! level; the extra fields' names; the version, volume and offset), and each item's
//! data through cash-archive's zip reader.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use cash_archive::sevenz::Problem;
use cash_archive::zip::read::{self as zread, DataError};
use cash_archive::zip::{Entry, method};

use super::archive::{Data, Item, Prop};

/// FILETIME ticks at the Unix epoch.
const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;

/// What opening a zip found.
pub(super) struct Opening {
    pub(super) physical_size: u64,
    pub(super) props: Vec<(&'static str, String)>,
    pub(super) items: Vec<Item>,
    pub(super) zip: Zip,
}

/// A zip archive, open, for reading its items' data.
pub(super) struct Zip {
    path: PathBuf,
    archive: zread::Archive,
    password: Option<String>,
}

/// The properties a technical listing shows for a zip item, in 7-Zip's order.
pub(super) const ITEM_PROPS: [Prop; 17] = [
    Prop::Path,
    Prop::Folder,
    Prop::Size,
    Prop::PackedSize,
    Prop::Modified,
    Prop::Created,
    Prop::Accessed,
    Prop::Attributes,
    Prop::Encrypted,
    Prop::Comment,
    Prop::Crc,
    Prop::Method,
    Prop::Characteristics,
    Prop::HostOs,
    Prop::Version,
    Prop::VolumeIndex,
    Prop::Offset,
];

fn le16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn le64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// An extra field's blocks: id and data.
fn extra_blocks(extra: &[u8]) -> Vec<(u16, &[u8])> {
    let mut blocks = Vec::new();
    let mut pos = 0;
    while let (Some(id), Some(size)) = (le16(extra, pos), le16(extra, pos + 2)) {
        let start = pos + 4;
        let end = start + usize::from(size);
        let Some(data) = extra.get(start..end) else {
            break;
        };
        blocks.push((id, data));
        pos = end;
    }
    blocks
}

/// `CExtraSubBlock::PrintInfo`: the block's name, `UT` with its flags and times.
fn extra_name(id: u16, data: &[u8]) -> String {
    let name = match id {
        0x0001 => "Zip64",
        0x000A => "NTFS",
        0x000D => "UNIX",
        0x0017 => "StrongCrypto",
        0x5455 => "UT",
        0x5855 => "UX",
        0x7855 => "Ux",
        0x7875 => "ux",
        0x6375 => "uc",
        0x7075 => "up",
        0x4453 => "SD",
        0x9901 => "WzAES",
        0xD935 => "ApkAlign",
        _ => return format!("0x{id:X}"),
    };
    let mut s = name.to_owned();
    if id == 0x5455
        && let Some(&flags) = data.first()
    {
        s.push(':');
        for (bit, c) in [(1, 'M'), (2, 'A'), (4, 'C')] {
            if flags & bit != 0 {
                s.push(c);
            }
        }
        let size = data.len() - 1;
        if size.is_multiple_of(4) {
            s.push(':');
            s.push_str(&(size / 4).to_string());
        }
    }
    s
}

/// `ExtractNtfsTime`: one of the NTFS block's three times.
fn ntfs_time(data: &[u8], index: usize) -> Option<u64> {
    if data.len() < 32 {
        return None;
    }
    let mut p = 4;
    let mut size = data.len() - 4;
    while size > 4 {
        let tag = le16(data, p)?;
        let attr = usize::from(le16(data, p + 2)?).min(size - 4);
        p += 4;
        size -= 4;
        if tag == 1 && attr >= 24 {
            return le64(data, p + 8 * index);
        }
        p += attr;
        size -= attr;
    }
    None
}

/// `Extract_UnixTime` on the central directory's `UT` block: the modification time
/// only, when its flag says so.
fn unix_time_central(data: &[u8], index: usize) -> Option<u32> {
    if data.len() < 5 || index != 0 || data[0] & 1 == 0 {
        return None;
    }
    le32(data, 1)
}

/// MS-DOS's local time as FILETIME ticks, in the shell's zone.
fn dos_ticks(entry: &Entry, zone: &cash_core::timefmt::Zone) -> Option<u64> {
    let (date, time) = (entry.time.date, entry.time.time);
    if date == 0 && time == 0 {
        return None;
    }
    let year = 1980 + i32::from(date >> 9);
    let month = u32::from((date >> 5) & 15);
    let day = u32::from(date & 31);
    let hour = u32::from(time >> 11);
    let minute = u32::from((time >> 5) & 63);
    let second = u32::from(time & 31) * 2;
    let naive =
        chrono::NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, second)?;
    let unix = zone.local_to_unix(naive)?;
    let ticks = i128::from(unix) * 10_000_000 + i128::from(UNIX_EPOCH_TICKS);
    u64::try_from(ticks).ok()
}

/// The host OS's name, as 7-Zip names it.
fn host_name(host: u8) -> String {
    const NAMES: [&str; 20] = [
        "FAT",
        "AMIGA",
        "VMS",
        "Unix",
        "VM/CMS",
        "Atari",
        "HPFS",
        "Macintosh",
        "Z-System",
        "CP/M",
        "TOPS-20",
        "NTFS",
        "SMS/QDOS",
        "Acorn",
        "VFAT",
        "MVS",
        "BeOS",
        "Tandem",
        "OS/400",
        "OS/X",
    ];
    NAMES
        .get(usize::from(host))
        .map_or_else(|| host.to_string(), |n| (*n).to_owned())
}

/// `CItem::IsDir`: a name ending in `/`, a FAT name ending in `\` with nothing in it,
/// or the host's folder attribute.
fn is_dir(entry: &Entry) -> bool {
    if entry.name.last() == Some(&b'/') {
        return true;
    }
    let host = entry.host();
    let fat = matches!(host, 0 | 6 | 11 | 14);
    if entry.size == 0 && entry.compressed_size == 0 && entry.name.last() == Some(&b'\\') && fat {
        return true;
    }
    let high = entry.external_attributes >> 16;
    match host {
        1 => high & 0o170_000 == 0o040_000,
        0 | 6 | 11 | 14 => entry.external_attributes & 0x10 != 0,
        3 => high & 0o170_000 == 0o040_000,
        _ => false,
    }
}

/// `CItem::GetWinAttrib`: FAT's and NTFS's attributes, Unix's mode with 7-Zip's
/// marker, and the folder bit.
fn win_attrib(entry: &Entry) -> u32 {
    let mut attrib = match entry.host() {
        0 | 11 => entry.external_attributes,
        3 => (entry.external_attributes & 0xFFFF_0000) | 0x8000,
        _ => 0,
    };
    if is_dir(entry) {
        attrib |= 0x10;
    }
    attrib
}

/// A name as 7-Zip reads it: UTF-8 when flagged or made on Unix, else the OEM code
/// page (437); the trailing `/` taken off.
fn name_of(entry: &Entry) -> String {
    let utf8 = entry.flags & 0x0800 != 0 || entry.host() == 3;
    let text: String = if utf8 || entry.name.is_ascii() {
        String::from_utf8_lossy(&entry.name).into_owned()
    } else {
        entry.name.iter().map(|&b| cp437(b)).collect()
    };
    text.trim_end_matches(['/', '\\']).to_owned()
}

/// A byte of code page 437, as Windows' OEM code page in Western Europe and the US.
fn cp437(b: u8) -> char {
    const HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";
    if b < 0x80 {
        char::from(b)
    } else {
        HIGH.chars().nth(usize::from(b - 0x80)).unwrap_or('?')
    }
}

/// `kpidMethod`: AES's strength or `ZipCrypto`, then the method, and Deflate's level or
/// LZMA's end marker.
fn method_text(entry: &Entry) -> String {
    let mut words = Vec::new();
    let aes = extra_blocks(&entry.extra)
        .into_iter()
        .find(|(i, _)| *i == 0x9901)
        .map(|(_, data)| data)
        .filter(|data| entry.method == 99 && data.len() >= 7);
    let wz_aes = aes.is_some();
    let id = if let Some(data) = aes {
        words.push(format!("AES-{}", (u32::from(data[4]) + 1) * 64));
        le16(data, 5).unwrap_or(0)
    } else {
        entry.method
    };
    if entry.flags & 1 != 0 && !wz_aes {
        words.push(if entry.flags & 0x40 != 0 {
            "StrongCrypto".to_owned()
        } else {
            "ZipCrypto".to_owned()
        });
    }
    let name = match id {
        0 => Some("Store"),
        1 => Some("Shrink"),
        2 => Some("Reduce1"),
        3 => Some("Reduce2"),
        4 => Some("Reduce3"),
        5 => Some("Reduce4"),
        6 => Some("Implode"),
        8 => Some("Deflate"),
        9 => Some("Deflate64"),
        10 => Some("PKImploding"),
        12 => Some("BZip2"),
        14 => Some("LZMA"),
        93 => Some("zstd"),
        94 => Some("MP3"),
        95 => Some("xz"),
        96 => Some("Jpeg"),
        97 => Some("WavPack"),
        98 => Some("PPMd"),
        99 => Some("LZFSE"),
        _ => None,
    };
    let mut m = name.map_or_else(|| id.to_string(), str::to_owned);
    let mut level = (entry.flags >> 1) & 3;
    if level != 0 {
        if id == method::LZMA {
            if level & 1 != 0 {
                m.push_str(":eos");
            }
            level &= !1;
        } else if id == method::DEFLATED {
            m.push(':');
            m.push_str(["Normal", "Maximum", "Fast", "Fastest"][usize::from(level)]);
            level = 0;
        }
        if level != 0 {
            let _ = write!(m, ":v{level}");
        }
    }
    words.push(m);
    words.join(" ")
}

/// `kpidCharacts`: the central extra field's blocks, then the flags 7-Zip names.
fn characteristics(entry: &Entry) -> String {
    let mut s: Vec<String> = extra_blocks(&entry.extra)
        .into_iter()
        .map(|(id, data)| extra_name(id, data))
        .collect();
    let flags = entry.flags & !6;
    let named: Vec<&str> = [
        (0, "Encrypt"),
        (3, "Descriptor"),
        (6, "StrongCrypto"),
        (11, "UTF8"),
        (14, "Alt"),
    ]
    .into_iter()
    .filter(|(bit, _)| flags >> bit & 1 != 0)
    .map(|(_, name)| name)
    .collect();
    let mut rest = Vec::new();
    for bit in 0..16 {
        if flags >> bit & 1 != 0 && ![0, 3, 6, 11, 14].contains(&bit) {
            rest.push(format!("0x{:X}", 1u32 << bit));
        }
    }
    if !named.is_empty() || !rest.is_empty() {
        if !s.is_empty() {
            s.push(":".to_owned());
        }
        s.extend(named.into_iter().map(str::to_owned));
        s.extend(rest);
    }
    s.join(" ")
}

/// Whether the item has a CRC to show (`IsThereCrc`): AES's AE-2 has none; a folder's
/// zero is none.
fn has_crc(entry: &Entry) -> bool {
    if entry.method == 99
        && let Some((_, data)) = extra_blocks(&entry.extra)
            .into_iter()
            .find(|(i, _)| *i == 0x9901)
        && data.len() >= 2
    {
        return le16(data, 0) != Some(2);
    }
    entry.crc != 0 || !is_dir(entry)
}

/// Opens `path` as a zip (`CHandler::Open`); `None` when it is not one.
pub(super) fn open(path: &Path, zone: &cash_core::timefmt::Zone) -> io::Result<Option<Opening>> {
    let mut file = File::open(path)?;
    let archive = match zread::open(&mut file) {
        Ok(archive) => archive,
        Err(zread::OpenError::Io(error)) => return Err(error),
        Err(_) => return Ok(None),
    };
    let mut props = Vec::new();
    if !archive.end.comment.is_empty() {
        props.push((
            "Comment",
            String::from_utf8_lossy(&archive.end.comment).replace(['\n', '\r'], "_"),
        ));
    }
    let items = archive
        .entries
        .iter()
        .map(|entry| listed_item(entry, zone, archive.extra_bytes))
        .collect();
    let physical_size = archive.file_size;
    Ok(Some(Opening {
        physical_size,
        props,
        items,
        zip: Zip {
            path: path.to_path_buf(),
            archive,
            password: None,
        },
    }))
}

/// A zip item as the listing shows it.
fn listed_item(entry: &Entry, zone: &cash_core::timefmt::Zone, base: i64) -> Item {
    let blocks = extra_blocks(&entry.extra);
    let ntfs = blocks.iter().find(|(id, _)| *id == 0x000A).map(|(_, d)| *d);
    let ut = blocks.iter().find(|(id, _)| *id == 0x5455).map(|(_, d)| *d);
    let mut item = Item {
        path: name_of(entry),
        is_dir: is_dir(entry),
        size: Some(entry.size),
        packed: Some(entry.compressed_size),
        attrib: Some(win_attrib(entry)),
        crc: has_crc(entry).then_some(entry.crc),
        encrypted: entry.flags & 1 != 0,
        method: Some(method_text(entry)),
        host_os: Some(host_name(entry.host())),
        ..Item::default()
    };
    // (the item's slot: modified, created, accessed; the NTFS block's index of it)
    let times = [(0usize, 0usize), (1, 2), (2, 1)];
    for (slot, ntfs_index) in times {
        let value = ntfs
            .and_then(|d| ntfs_time(d, ntfs_index))
            .map(|t| (t, 7))
            .or_else(|| {
                ut.and_then(|d| unix_time_central(d, slot))
                    .map(|u| (u64::from(u) * 10_000_000 + UNIX_EPOCH_TICKS, 0))
            });
        let value = if slot == 0 {
            value.or_else(|| dos_ticks(entry, zone).map(|t| (t, 0)))
        } else {
            value
        };
        if let Some((ticks, digits)) = value {
            match slot {
                0 => item.modified = Some(ticks),
                1 => item.created = Some(ticks),
                _ => item.accessed = Some(ticks),
            }
            item.time_digits[slot] = digits;
        }
    }
    let offset = i128::from(entry.local_offset) + i128::from(base);
    item.extra = vec![
        (Prop::Folder, if item.is_dir { "+" } else { "-" }.to_owned()),
        (
            Prop::Comment,
            String::from_utf8_lossy(&entry.comment).replace(['\n', '\r'], "_"),
        ),
        (Prop::Characteristics, characteristics(entry)),
        (Prop::Version, (entry.version_needed & 0xFF).to_string()),
        (Prop::VolumeIndex, entry.disk_start.to_string()),
        (Prop::Offset, offset.to_string()),
    ];
    item
}

impl Zip {
    /// The password encrypted items are read with.
    pub(super) fn set_password(&mut self, password: &str) {
        self.password = Some(password.to_owned());
    }

    /// Item `index`'s data, decrypted and decompressed, its CRC checked at the end.
    pub(super) fn data(&self, index: usize) -> io::Result<ZipData> {
        let entry = &self.archive.entries[index];
        let mut file = File::open(&self.path)?;
        let local = zread::local(&mut file, &self.archive, entry);
        let check = has_crc(entry);
        let mut data = ZipData {
            reader: None,
            problem: None,
            crc: crc32fast::Hasher::new(),
            expected: entry.crc,
            check_crc: check,
            encrypted: entry.flags & 1 != 0,
            unpacked: 0,
            size: entry.size,
        };
        let Ok(local) = local else {
            data.problem = Some(Problem::Data);
            return Ok(data);
        };
        file.seek(SeekFrom::Start(local.data_offset))?;
        let raw = BufReader::new(file).take(entry.compressed_size);
        match zread::data(raw, entry, self.password.as_deref().map(str::as_bytes)) {
            Ok(reader) => data.reader = Some(reader),
            Err(DataError::Unsupported(_)) => data.problem = Some(Problem::UnsupportedMethod),
            Err(DataError::NeedPassword) => data.problem = Some(Problem::PasswordNeeded),
            Err(DataError::BadPassword) => data.problem = Some(Problem::WrongPassword),
            Err(DataError::BadLocal(_)) => data.problem = Some(Problem::Data),
            Err(DataError::Io(error)) => return Err(error),
        }
        Ok(data)
    }
}

/// A zip item's data for extraction: decoded, its CRC kept for `finish`.
pub(super) struct ZipData {
    reader: Option<Box<dyn Read>>,
    problem: Option<Problem>,
    crc: crc32fast::Hasher,
    expected: u32,
    check_crc: bool,
    encrypted: bool,
    unpacked: u64,
    size: u64,
}

impl Read for ZipData {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.problem.is_some() {
            return Ok(0);
        }
        let Some(reader) = self.reader.as_mut() else {
            return Ok(0);
        };
        match reader.read(buf) {
            Ok(n) => {
                self.crc.update(&buf[..n]);
                self.unpacked += n as u64;
                Ok(n)
            }
            Err(error) => {
                self.problem = Some(if error.kind() == io::ErrorKind::UnexpectedEof {
                    Problem::UnexpectedEnd
                } else {
                    Problem::Data
                });
                Ok(0)
            }
        }
    }
}

impl Data for ZipData {
    fn finish(&mut self) -> Result<(), Problem> {
        let _ = io::copy(self, &mut io::sink());
        if let Some(problem) = self.problem {
            return Err(problem);
        }
        if self.unpacked < self.size {
            return Err(Problem::UnexpectedEnd);
        }
        if self.check_crc && self.crc.clone().finalize() != self.expected {
            return Err(Problem::Crc);
        }
        Ok(())
    }

    fn encrypted(&self) -> bool {
        self.encrypted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_fields_as_7_zip_names_them() {
        assert_eq!(extra_name(0x5455, &[7, 0, 0, 0, 0]), "UT:MAC:1");
        assert_eq!(extra_name(0x000A, &[0; 32]), "NTFS");
        assert_eq!(extra_name(0x1234, &[]), "0x1234");
    }

    #[test]
    fn code_page_437_reads_its_high_half() {
        assert_eq!(cp437(0x80), 'Ç');
        assert_eq!(cp437(0xE1), 'ß');
        assert_eq!(cp437(b'a'), 'a');
    }
}
