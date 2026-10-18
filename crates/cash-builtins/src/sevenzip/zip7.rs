//! zip as 7-Zip's zip handler lists and reads it (`ZipHandler.cpp`, `ZipItem.cpp`): the
//! central directory's items with the facts 7-Zip shows (time by the NTFS or Unix extra
//! field, else MS-DOS's; attributes by the host; the method with its encryption and
//! level; the extra fields' names; the version, volume and offset), and each item's
//! data through cash-archive's zip reader. A split zip (`NAME.z01` … `NAME.zip`) is read
//! as one from any of its parts; a zip with no end record, by its local headers.

use std::fmt::Write as _;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use cash_archive::sevenz::Problem;
use cash_archive::zip::read::{self as zread, DataError};
use cash_archive::zip::{Entry, method};

use super::archive::{Data, Item, Prop, TimePrec};
use super::volume::Location;

/// FILETIME ticks at the Unix epoch.
const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;

/// What opening a zip found.
pub(super) struct Opening {
    pub(super) physical_size: u64,
    pub(super) props: Vec<(&'static str, String)>,
    pub(super) items: Vec<Item>,
    /// What went wrong reading it, though it opened: a zip read by its local headers
    /// has no end, as 7-Zip says ("Unexpected end of archive").
    pub(super) error_flags: Vec<&'static str>,
    /// What went wrong, in words: a split zip's missing part.
    pub(super) error_message: Option<String>,
    /// A split zip's parts, the opened one first.
    pub(super) volumes: Option<super::archive::Split>,
    pub(super) zip: Zip,
}

/// A zip archive, open, for reading its items' data.
pub(super) struct Zip {
    pub(super) location: Location,
    pub(super) archive: zread::Archive,
    password: Option<String>,
    /// For each item, whether its data descriptor was lost (read by local headers).
    lost: Vec<bool>,
}

/// The properties a technical listing shows for a zip item, in 7-Zip's order.
pub(super) const ITEM_PROPS: [Prop; 17] = [
    Prop::Path,
    Prop::Directory,
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
/// or the host's directory attribute.
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
/// marker, and the directory bit.
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
pub(super) fn cp437(b: u8) -> char {
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

/// `kpidCharacts`: the extra field's blocks, after `Local` for a member read by its
/// local header, then the flags 7-Zip names.
fn characteristics(entry: &Entry, local: bool) -> String {
    let mut s: Vec<String> = local.then(|| "Local".to_owned()).into_iter().collect();
    s.extend(
        extra_blocks(&entry.extra)
            .into_iter()
            .map(|(id, data)| extra_name(id, data)),
    );
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

/// Whether the item has a CRC to show (`IsThereCrc`): AES's AE-2 has none; a directory's
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

/// `Extract_UnixTime` on a local header's `UT` block: each time its flags name, in
/// their order, the modification time first.
fn unix_time_local(data: &[u8], index: usize) -> Option<u32> {
    let flags = *data.first()?;
    if flags >> index & 1 == 0 {
        return None;
    }
    let before = (flags & ((1 << index) - 1)).count_ones() as usize;
    le32(data, 1 + 4 * before)
}

const LOCAL_SIGNATURE: &[u8] = b"PK\x03\x04";
const CENTRAL_SIGNATURE: &[u8] = b"PK\x01\x02";
const DESCRIPTOR_SIGNATURE: &[u8] = b"PK\x07\x08";

/// A member read by its local header, with no central directory to say more (7-Zip's
/// `Local`); `lost_descriptor` when its data descriptor was not found, and its data
/// taken to run to the file's end.
struct LocalMember {
    entry: Entry,
    lost_descriptor: bool,
}

/// The members of a zip with no central directory, one local header after another:
/// sizes from the header, or from the data descriptor after the data when the header
/// says one follows. Reading stops at the first place with no local header, or where a
/// member goes past the file's end.
fn scan_local(input: &mut (impl Read + Seek), size: u64) -> io::Result<Vec<LocalMember>> {
    let mut members = Vec::new();
    let mut pos = 0;
    while pos < size {
        input.seek(SeekFrom::Start(pos))?;
        let mut fixed = [0u8; 30];
        if super::archive::read_up_to(input, &mut fixed)? < fixed.len()
            || &fixed[..4] != LOCAL_SIGNATURE
        {
            break;
        }
        let name_len = usize::from(le16(&fixed, 26).unwrap_or(0));
        let mut variable = vec![0u8; name_len + usize::from(le16(&fixed, 28).unwrap_or(0))];
        if super::archive::read_up_to(input, &mut variable)? < variable.len() {
            break;
        }
        let extra = variable.split_off(name_len);
        let mut entry = Entry {
            version_needed: le16(&fixed, 4).unwrap_or(0),
            flags: le16(&fixed, 6).unwrap_or(0),
            method: le16(&fixed, 8).unwrap_or(0),
            time: cash_archive::zip::DosTime {
                time: le16(&fixed, 10).unwrap_or(0),
                date: le16(&fixed, 12).unwrap_or(0),
            },
            crc: le32(&fixed, 14).unwrap_or(0),
            compressed_size: u64::from(le32(&fixed, 18).unwrap_or(0)),
            size: u64::from(le32(&fixed, 22).unwrap_or(0)),
            name: variable,
            extra,
            local_offset: pos,
            ..Entry::default()
        };
        if let Some((_, data)) = extra_blocks(&entry.extra)
            .into_iter()
            .find(|(id, _)| *id == 0x0001)
        {
            if entry.size == 0xFFFF_FFFF {
                entry.size = le64(data, 0).unwrap_or(entry.size);
            }
            if entry.compressed_size == 0xFFFF_FFFF {
                entry.compressed_size = le64(data, 8).unwrap_or(entry.compressed_size);
            }
        }
        let data_start = pos + 30 + (name_len + entry.extra.len()) as u64;
        let mut lost_descriptor = false;
        let next = if entry.flags & 8 == 0 {
            data_start + entry.compressed_size
        } else if let Some((at, crc, packed, unpacked)) = find_descriptor(input, data_start, size)?
        {
            entry.crc = crc;
            entry.compressed_size = packed;
            entry.size = unpacked;
            at + 16
        } else {
            lost_descriptor = true;
            entry.crc = 0;
            entry.compressed_size = size - data_start.min(size);
            entry.size = 0;
            size
        };
        members.push(LocalMember {
            entry,
            lost_descriptor,
        });
        pos = next;
    }
    Ok(members)
}

/// A data descriptor after the data starting at `start`: its signature, a packed size
/// that is the distance from `start`, and another local header or the central
/// directory right after it, as 7-Zip finds one. Its place, CRC and sizes.
fn find_descriptor(
    input: &mut (impl Read + Seek),
    start: u64,
    size: u64,
) -> io::Result<Option<(u64, u32, u64, u64)>> {
    // A descriptor and the signature after it: twenty bytes, which a chunk's end may cut.
    const NEEDED: usize = 20;
    let mut buf = vec![0u8; (1 << 16) + NEEDED];
    let mut base = start;
    while base < size {
        input.seek(SeekFrom::Start(base))?;
        let n = super::archive::read_up_to(input, &mut buf)?;
        if n < NEEDED {
            break;
        }
        for i in 0..=n - NEEDED {
            if &buf[i..i + 4] != DESCRIPTOR_SIGNATURE {
                continue;
            }
            let at = base + i as u64;
            let packed = le32(&buf, i + 8).unwrap_or(0);
            let after = &buf[i + 16..i + 20];
            if u64::from(packed) == (at - start) & 0xFFFF_FFFF
                && (after == LOCAL_SIGNATURE || after == CENTRAL_SIGNATURE)
            {
                let crc = le32(&buf, i + 4).unwrap_or(0);
                let unpacked = le32(&buf, i + 12).unwrap_or(0);
                return Ok(Some((at, crc, u64::from(packed), u64::from(unpacked))));
            }
        }
        base += (n - (NEEDED - 1)) as u64;
    }
    Ok(None)
}

/// Opens a zip with no central directory by its local headers; `None` when it has
/// none at its start.
fn open_local(
    location: &Location,
    mut file: impl Read + Seek,
    zone: &cash_core::timefmt::Zone,
) -> io::Result<Option<Opening>> {
    let size = file.seek(SeekFrom::End(0))?;
    let members = scan_local(&mut file, size)?;
    if members.is_empty() {
        return Ok(None);
    }
    let items = members
        .iter()
        .map(|member| listed_item(&member.entry, zone, 0, Some(member.lost_descriptor)))
        .collect();
    let lost = members
        .iter()
        .map(|member| member.lost_descriptor)
        .collect();
    let archive = zread::Archive {
        file_size: size,
        entries: members.into_iter().map(|member| member.entry).collect(),
        ..zread::Archive::default()
    };
    Ok(Some(Opening {
        physical_size: size,
        props: vec![("Characteristics", "Local".to_owned())],
        items,
        error_flags: vec!["Unexpected end of archive"],
        error_message: None,
        volumes: None,
        zip: Zip {
            location: location.clone(),
            archive,
            password: None,
            lost,
        },
    }))
}

/// The number of the part a split zip's part is, by its name: `NAME.z01` is 1, and the
/// last part, `NAME.zip`, none.
fn part_number(path: &Path) -> Option<u32> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let digits = ext.strip_prefix('z')?;
    (digits.len() >= 2 && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

/// Whether `path` names a part of a split zip other than its last, `NAME.zip`, which is
/// there: 7-Zip opens the set from any of its parts.
pub(super) fn is_split_part(path: &Path) -> bool {
    part_number(path).is_some_and(|n| n > 0) && path.with_extension("zip").is_file()
}

/// The number of the disk a zip's end record is on: how many parts came before it.
fn end_disk(path: &Path) -> io::Result<u32> {
    let mut file = std::fs::File::open(path)?;
    let size = file.seek(SeekFrom::End(0))?;
    let tail_len = size.min(0xFFFF + 22 + 20);
    file.seek(SeekFrom::Start(size - tail_len))?;
    let mut tail = vec![0u8; usize::try_from(tail_len).unwrap_or(0)];
    file.read_exact(&mut tail)?;
    let Some(at) = tail.windows(4).rposition(|w| w == b"PK\x05\x06") else {
        return Ok(0);
    };
    let disk = le16(&tail, at + 4).unwrap_or(0);
    // Zip64's locator, before the record, has the number of disks.
    if disk == 0xFFFF
        && at >= 20
        && tail.get(at - 20..at - 16) == Some(b"PK\x06\x07")
        && let Some(disks) = le32(&tail, at - 4)
    {
        return Ok(disks.saturating_sub(1));
    }
    Ok(u32::from(disk))
}

/// A split zip's parts (Info-ZIP's and PKZIP's `-s`): `NAME.z01`, `NAME.z02`, … and
/// `NAME.zip`, whose end record says how many came before it; and which of them `path`
/// is. `None` when `path` is a zip of one part.
fn split_parts(path: &Path) -> io::Result<Option<(Vec<PathBuf>, usize)>> {
    let number = part_number(path);
    let last = if number.is_some() {
        path.with_extension("zip")
    } else if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        path.to_path_buf()
    } else {
        return Ok(None);
    };
    if !last.is_file() {
        return Ok(None);
    }
    let disks = end_disk(&last)?;
    if disks == 0 {
        return Ok(None);
    }
    let mut parts: Vec<PathBuf> = (1..=disks)
        .map(|n| path.with_extension(format!("z{n:02}")))
        .collect();
    parts.push(last);
    let opened = number.map_or(parts.len() - 1, |n| (n as usize).saturating_sub(1));
    Ok(Some((parts, opened)))
}

/// Opens a split zip, its parts read as one; a part missing is said, and its members'
/// data fails.
fn open_split(
    parts: &[PathBuf],
    opened: usize,
    zone: &cash_core::timefmt::Zone,
) -> io::Result<Option<Opening>> {
    let sizes: Vec<Option<u64>> = parts
        .iter()
        .map(|p| p.metadata().ok().filter(|m| m.is_file()).map(|m| m.len()))
        .collect();
    // Where each part starts in the ones that are there, read as one.
    let mut bases = Vec::with_capacity(parts.len());
    let mut sum = 0;
    for size in &sizes {
        bases.push(sum);
        sum += size.unwrap_or(0);
    }
    let present: Vec<PathBuf> = parts
        .iter()
        .zip(&sizes)
        .filter(|(_, size)| size.is_some())
        .map(|(p, _)| p.clone())
        .collect();
    let missing = parts.iter().zip(&sizes).find(|(_, size)| size.is_none());
    let location = Location::of_parts(present.clone());
    let mut file = location.open()?;
    let archive = match zread::open_parts(&mut file, &bases) {
        Ok(archive) => archive,
        Err(zread::OpenError::Io(error)) => return Err(error),
        Err(_) => return Ok(None),
    };
    let base_of = |disk: u32| {
        bases
            .get(disk as usize)
            .and_then(|&b| i64::try_from(b).ok())
            .unwrap_or(0)
    };
    let items = archive
        .entries
        .iter()
        .map(|entry| listed_item(entry, zone, -base_of(entry.disk_start), None))
        .collect();
    let mut props = Vec::new();
    if missing.is_none()
        && let Some(first) = present.first()
        && let Ok(mut f) = std::fs::File::open(first)
    {
        let mut head = [0u8; 4];
        if f.read_exact(&mut head).is_ok() && head == DESCRIPTOR_SIGNATURE {
            props.push(("Embedded Stub Size", "4".to_owned()));
        }
    }
    props.push(("Total Physical Size", sum.to_string()));
    props.push(("Multivolume", "+".to_owned()));
    props.push(("Volume Index", opened.to_string()));
    props.push(("Volumes", parts.len().to_string()));
    let opened_size = sizes.get(opened).copied().flatten().unwrap_or(0);
    // The opened part first: the others are read with it, not listed again.
    let mut listed_parts = vec![parts[opened].clone()];
    listed_parts.extend(present.iter().filter(|p| **p != parts[opened]).cloned());
    let lost = vec![false; archive.entries.len()];
    Ok(Some(Opening {
        physical_size: opened_size,
        props,
        items,
        error_flags: Vec::new(),
        error_message: missing.map(|(p, _)| {
            format!(
                "Missing volume : {}",
                p.file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            )
        }),
        volumes: Some(super::archive::Split {
            parts: listed_parts,
            first_size: opened_size,
            total: sum,
        }),
        zip: Zip {
            location,
            archive,
            password: None,
            lost,
        },
    }))
}

/// Opens `path` as a zip (`CHandler::Open`); `None` when it is not one.
pub(super) fn open(
    location: &Location,
    zone: &cash_core::timefmt::Zone,
) -> io::Result<Option<Opening>> {
    if let [path] = location.parts()
        && let Some((parts, opened)) = split_parts(path)?
    {
        return open_split(&parts, opened, zone);
    }
    let mut file = location.open()?;
    let archive = match zread::open(&mut file) {
        Ok(archive) => archive,
        Err(zread::OpenError::Io(error)) => return Err(error),
        // No end record: read by the local headers, as 7-Zip does.
        Err(zread::OpenError::NoEnd) => return open_local(location, file, zone),
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
        .map(|entry| listed_item(entry, zone, archive.extra_bytes, None))
        .collect();
    let physical_size = archive.file_size;
    let lost = vec![false; archive.entries.len()];
    Ok(Some(Opening {
        physical_size,
        props,
        items,
        error_flags: Vec::new(),
        error_message: None,
        volumes: None,
        zip: Zip {
            location: location.clone(),
            archive,
            password: None,
            lost,
        },
    }))
}

/// A zip item as the listing shows it. `local` for a member read by its local header:
/// whether its data descriptor was lost.
fn listed_item(
    entry: &Entry,
    zone: &cash_core::timefmt::Zone,
    base: i64,
    local: Option<bool>,
) -> Item {
    let blocks = extra_blocks(&entry.extra);
    let ntfs = blocks.iter().find(|(id, _)| *id == 0x000A).map(|(_, d)| *d);
    let ut = blocks.iter().find(|(id, _)| *id == 0x5455).map(|(_, d)| *d);
    let mut item = Item {
        path: name_of(entry),
        is_dir: is_dir(entry),
        // A lost descriptor took the size with it.
        size: (local != Some(true)).then_some(entry.size),
        packed: Some(entry.compressed_size),
        attrib: Some(win_attrib(entry)),
        crc: has_crc(entry).then_some(entry.crc),
        encrypted: entry.flags & 1 != 0,
        method: Some(method_text(entry)),
        // The host is the central directory's to say.
        host_os: Some(if local.is_some() {
            String::new()
        } else {
            host_name(entry.host())
        }),
        ..Item::default()
    };
    let unix_time: fn(&[u8], usize) -> Option<u32> = if local.is_some() {
        unix_time_local
    } else {
        unix_time_central
    };
    // (the item's slot: modified, created, accessed; the index of it in the NTFS and UT
    // blocks, which keep modified, accessed, created)
    let times = [(0usize, 0usize), (1, 2), (2, 1)];
    for (slot, index) in times {
        let value = ntfs
            .and_then(|d| ntfs_time(d, index))
            .map(|t| (t, 7, TimePrec::Exact))
            .or_else(|| {
                ut.and_then(|d| unix_time(d, index)).map(|u| {
                    (
                        u64::from(u) * 10_000_000 + UNIX_EPOCH_TICKS,
                        0,
                        TimePrec::Unix,
                    )
                })
            });
        let value = if slot == 0 {
            value.or_else(|| dos_ticks(entry, zone).map(|t| (t, 0, TimePrec::Dos)))
        } else {
            value
        };
        if let Some((ticks, digits, prec)) = value {
            match slot {
                0 => {
                    item.modified = Some(ticks);
                    item.mtime_prec = prec;
                }
                1 => item.created = Some(ticks),
                _ => item.accessed = Some(ticks),
            }
            item.time_digits[slot] = digits;
        }
    }
    let offset = i128::from(entry.local_offset) + i128::from(base);
    let mut characts = characteristics(entry, local.is_some());
    if local == Some(true) {
        characts.push_str(" Descriptor_ERROR");
    }
    item.extra = vec![
        (
            Prop::Directory,
            if item.is_dir { "+" } else { "-" }.to_owned(),
        ),
        (
            Prop::Comment,
            String::from_utf8_lossy(&entry.comment).replace(['\n', '\r'], "_"),
        ),
        (Prop::Characteristics, characts),
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
        let mut file = self.location.open()?;
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
            after_end: self.lost.get(index).copied().unwrap_or(false),
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
    /// The data was taken to run to the file's end, its descriptor lost: what decodes
    /// whole leaves the descriptor and more after it.
    after_end: bool,
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
        if self.after_end {
            return Err(Problem::DataAfterEnd);
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
