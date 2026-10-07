//! RAR 5 and 7 as 7-Zip 26.03 opens and lists them (`Rar5Handler.cpp`): each volume's
//! block headers, decrypted when the headers are, read into items; an item split across
//! volumes joined into one; an archive comment, ACLs and alternate streams taken apart
//! from the files; and each property as 7-Zip names and words it.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use cash_archive::rar::crypto::rar50::{Rar50Cipher, Rar50Keys};

use super::super::archive::{Item, Prop, TimePrec};
use super::volname::VolumeName;

const MARKER: [u8; 8] = *b"Rar!\x1a\x07\x01\x00";

/// The longest comment 7-Zip reads, as RAR writes none longer.
const COMMENT_MAX: u64 = 1 << 18;

/// FILETIME ticks at the Unix epoch.
const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;

mod header {
    pub(super) const ARC: u64 = 1;
    pub(super) const FILE: u64 = 2;
    pub(super) const SERVICE: u64 = 3;
    pub(super) const ARC_ENCRYPT: u64 = 4;
    pub(super) const END_OF_ARC: u64 = 5;

    pub(super) const EXTRA: u64 = 1;
    pub(super) const DATA: u64 = 1 << 1;
    pub(super) const PREV_VOL: u64 = 1 << 3;
    pub(super) const NEXT_VOL: u64 = 1 << 4;
}

mod arc_flags {
    pub(super) const VOL: u64 = 1;
    pub(super) const VOL_NUMBER: u64 = 1 << 1;
    pub(super) const SOLID: u64 = 1 << 2;
}

mod file_flags {
    pub(super) const DIR: u64 = 1;
    pub(super) const UNIX_TIME: u64 = 1 << 1;
    pub(super) const CRC32: u64 = 1 << 2;
    pub(super) const UNKNOWN_SIZE: u64 = 1 << 3;
}

mod extra_id {
    pub(super) const CRYPTO: u64 = 1;
    pub(super) const HASH: u64 = 2;
    pub(super) const TIME: u64 = 3;
    pub(super) const VERSION: u64 = 4;
    pub(super) const LINK: u64 = 5;
    pub(super) const SUBDATA: u64 = 7;
}

mod link_type {
    pub(super) const UNIX_SYMLINK: u64 = 1;
    pub(super) const WIN_SYMLINK: u64 = 2;
    pub(super) const WIN_JUNCTION: u64 = 3;
    pub(super) const HARD_LINK: u64 = 4;
    pub(super) const FILE_COPY: u64 = 5;
}

const HOST_WINDOWS: u64 = 0;
const HOST_UNIX: u64 = 1;

const ARC_FLAG_NAMES: [&str; 5] = ["Volume", "VolumeField", "Solid", "Recovery", "Lock"];
const FILE_FLAG_NAMES: [&str; 4] = ["Dir", "UnixTime", "CRC", "UnknownSize"];
const EXTRA_TYPES: [&str; 8] = [
    "0",
    "Crypto",
    "Hash",
    "Time",
    "Version",
    "Link",
    "UnixOwner",
    "Subdata",
];
const LINK_TYPES: [&str; 6] = [
    "0",
    "UnixSymLink",
    "WinSymLink",
    "WinJunction",
    "HardLink",
    "FileCopy",
];
const EXTRA_TIME_FLAGS: [char; 5] = ['u', 'M', 'C', 'A', 'n'];

/// `ReadVarInt`: a number seven bits a byte, and its length; none past ten bytes or the
/// end of `p`.
fn read_var(p: &[u8]) -> Option<(u64, usize)> {
    let mut value = 0u64;
    for (i, &b) in p.iter().take(10).enumerate() {
        let shift = u32::try_from(7 * i).unwrap_or(u32::MAX);
        value |= u64::from(b & 0x7F).checked_shl(shift).unwrap_or(0);
        if b & 0x80 == 0 {
            return Some((value, i + 1));
        }
    }
    None
}

/// Numbers read off the front of a header.
struct Cursor<'a> {
    data: &'a [u8],
}

impl<'a> Cursor<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    fn var(&mut self) -> Option<u64> {
        let (value, len) = read_var(self.data)?;
        self.data = &self.data[len..];
        Some(value)
    }

    const fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        if len > self.data.len() {
            return None;
        }
        let (head, rest) = self.data.split_at(len);
        self.data = rest;
        Some(head)
    }

    fn u32(&mut self) -> Option<u32> {
        let bytes = self.take(4)?;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    const fn rest(&self) -> &'a [u8] {
        self.data
    }
}

fn le32(p: &[u8], at: usize) -> Option<u32> {
    let bytes = p.get(at..at + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn le64(p: &[u8], at: usize) -> Option<u64> {
    Some(u64::from(le32(p, at)?) | (u64::from(le32(p, at + 4)?) << 32))
}

/// The low 32 bits, as 7-Zip keeps a header's attributes and method.
const fn low32(value: u64) -> u32 {
    (value & 0xFFFF_FFFF) as u32
}

/// `FlagsToString`: each flag's name, or its value in hex where it has none.
fn flags_to_string(names: &[&str], flags: u64) -> String {
    let mut words = Vec::new();
    for bit in 0..64 {
        let flag = 1u64 << bit;
        if flags & flag == 0 {
            continue;
        }
        match names.get(bit) {
            Some(name) => words.push((*name).to_owned()),
            None => words.push(format!("0x{flag:X}")),
        }
    }
    words.join(" ")
}

/// `PrintType`: a table's name for `value`, or the number.
fn type_name(names: &[&str], value: u64) -> String {
    usize::try_from(value)
        .ok()
        .and_then(|i| names.get(i))
        .map_or_else(|| value.to_string(), |name| (*name).to_owned())
}

/// `PrintDictSize`: kilobytes, or megabytes or gigabytes where they come out whole.
fn dict_size(size: u64) -> String {
    let mut value = size >> 10;
    let mut unit = 'K';
    if value.trailing_zeros() >= 10 {
        unit = 'M';
        value >>= 10;
        if value.trailing_zeros() >= 10 {
            unit = 'G';
            value >>= 10;
        }
    }
    format!("{value}{unit}")
}

/// An item's link record (`CLinkInfo`): its kind, flags and the target's place in the
/// extra area.
struct LinkInfo {
    kind: u64,
    flags: u64,
    name_offset: usize,
    name_len: usize,
}

impl LinkInfo {
    fn parse(p: &[u8]) -> Option<Self> {
        let mut c = Cursor::new(p);
        let kind = c.var()?;
        let flags = c.var()?;
        let len = c.var()?;
        let len = usize::try_from(len).ok()?;
        if c.rest().len() != len {
            return None;
        }
        Some(Self {
            kind,
            flags,
            name_offset: p.len() - len,
            name_len: len,
        })
    }
}

/// A file or service header (`CItem`).
#[derive(Clone, Debug, Default)]
pub(super) struct RarItem {
    pub(super) common_flags: u64,
    pub(super) flags: u64,
    pub(super) record_type: u64,
    pub(super) version: Option<u64>,
    /// The ACL this item's security is, in `acls`.
    pub(super) acl: Option<usize>,
    pub(super) name: Vec<u8>,
    pub(super) vol_index: usize,
    pub(super) next_item: Option<usize>,
    pub(super) unix_mtime: u32,
    pub(super) crc: u32,
    pub(super) attrib: u32,
    pub(super) method: u32,
    pub(super) extra: Vec<u8>,
    pub(super) size: u64,
    pub(super) pack_size: u64,
    pub(super) host_os: u64,
    pub(super) data_pos: u64,
}

impl RarItem {
    pub(super) const fn is_split_before(&self) -> bool {
        self.common_flags & header::PREV_VOL != 0
    }

    pub(super) const fn is_split_after(&self) -> bool {
        self.common_flags & header::NEXT_VOL != 0
    }

    pub(super) const fn is_split(&self) -> bool {
        self.is_split_before() || self.is_split_after()
    }

    pub(super) const fn is_dir(&self) -> bool {
        self.flags & file_flags::DIR != 0
    }

    const fn has_unix_mtime(&self) -> bool {
        self.flags & file_flags::UNIX_TIME != 0
    }

    pub(super) const fn has_crc(&self) -> bool {
        self.flags & file_flags::CRC32 != 0
    }

    pub(super) const fn is_unknown_size(&self) -> bool {
        self.flags & file_flags::UNKNOWN_SIZE != 0
    }

    fn is_next_for(&self, prev: &Self) -> bool {
        !self.is_dir()
            && !prev.is_dir()
            && self.is_split_before()
            && prev.is_split_after()
            && self.name == prev.name
    }

    pub(super) const fn is_solid(&self) -> bool {
        self.method & (1 << 6) != 0
    }

    pub(super) const fn is_rar5_compat(&self) -> bool {
        self.method & (1 << 20) != 0
    }

    const fn compat_bit(&self) -> u32 {
        (self.method >> 20) & 1
    }

    pub(super) const fn algo_raw(&self) -> u32 {
        self.method & 0x3F
    }

    pub(super) const fn method_number(&self) -> u32 {
        (self.method >> 7) & 7
    }

    pub(super) const fn dict_main(&self) -> u32 {
        (self.method >> 10) & if self.algo_raw() == 0 { 0xF } else { 0x1F }
    }

    pub(super) const fn dict_frac(&self) -> u32 {
        if self.algo_raw() == 0 {
            0
        } else {
            (self.method >> 15) & 0x1F
        }
    }

    /// The dictionary the method's bits give: `(32 + frac) << (12 + main)`.
    fn dict(&self) -> u64 {
        (32 + u64::from(self.dict_frac()))
            .checked_shl(12 + self.dict_main())
            .unwrap_or(0)
    }

    pub(super) const fn is_service(&self) -> bool {
        self.record_type == header::SERVICE
    }

    fn is_named(&self, name: &str) -> bool {
        self.is_service() && self.name == name.as_bytes()
    }

    pub(super) fn is_stm(&self) -> bool {
        self.is_named("STM")
    }

    fn is_cmt(&self) -> bool {
        self.is_named("CMT")
    }

    fn is_acl(&self) -> bool {
        self.is_named("ACL")
    }

    /// `FindExtra`: the record of `id`, as its offset in the extra area and its size.
    pub(super) fn find_extra(&self, id: u64) -> Option<(usize, usize)> {
        let extra = &self.extra;
        let mut offset = 0;
        loop {
            let rem = extra.len() - offset;
            if rem == 0 {
                return None;
            }
            let (size, num) = read_var(&extra[offset..])?;
            offset += num;
            let mut rem = rem - num;
            let size = usize::try_from(size).ok()?;
            if size > rem {
                return None;
            }
            rem = size;
            let (record_id, num) = read_var(&extra[offset..offset + rem])?;
            offset += num;
            rem -= num;
            // RAR 5.21 and before stored a service header's Subdata record as one byte
            // short; it was always the last.
            if record_id == extra_id::SUBDATA
                && self.record_type == header::SERVICE
                && rem + 1 == extra.len() - offset
            {
                rem += 1;
            }
            if record_id == id {
                return Some((offset, rem));
            }
            offset += rem;
        }
    }

    fn extra_record(&self, id: u64) -> Option<&[u8]> {
        let (offset, size) = self.find_extra(id)?;
        self.extra.get(offset..offset + size)
    }

    pub(super) fn is_encrypted(&self) -> bool {
        self.find_extra(extra_id::CRYPTO).is_some()
    }

    /// `FindExtra_Blake`: where a `BLAKE2sp` digest is.
    pub(super) fn blake_offset(&self) -> Option<usize> {
        let (offset, size) = self.find_extra(extra_id::HASH)?;
        (size == 33 && self.extra.get(offset) == Some(&0)).then_some(offset + 1)
    }

    fn find_version(&self) -> Option<u64> {
        let mut c = Cursor::new(self.extra_record(extra_id::VERSION)?);
        let _flags = c.var()?;
        let version = c.var()?;
        c.rest().is_empty().then_some(version)
    }

    fn find_link(&self) -> Option<LinkInfo> {
        let (offset, size) = self.find_extra(extra_id::LINK)?;
        let mut link = LinkInfo::parse(self.extra.get(offset..offset + size)?)?;
        link.name_offset += offset;
        Some(link)
    }

    /// `Link_to_Prop`: the target of a link of `kind`; a symbolic link also of the
    /// Windows kinds. Windows' `\` turned into `/`.
    fn link_target(&self, kind: u64) -> String {
        let Some(link) = self.find_link() else {
            return String::new();
        };
        let mut windows = self.host_os == HOST_WINDOWS;
        if link.kind != kind {
            if kind != link_type::UNIX_SYMLINK {
                return String::new();
            }
            match link.kind {
                link_type::UNIX_SYMLINK => windows = false,
                link_type::WIN_SYMLINK | link_type::WIN_JUNCTION => windows = true,
                _ => return String::new(),
            }
        }
        let bytes = self
            .extra
            .get(link.name_offset..link.name_offset + link.name_len)
            .unwrap_or_default();
        let bytes = bytes.split(|&b| b == 0).next().unwrap_or_default();
        let text = String::from_utf8_lossy(bytes).into_owned();
        if windows {
            text.replace('\\', "/")
        } else {
            text
        }
    }

    /// `GetAltStreamName`.
    fn alt_stream_name(&self) -> Option<String> {
        let bytes = self.extra_record(extra_id::SUBDATA)?;
        let bytes = bytes.split(|&b| b == 0).next().unwrap_or_default();
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    /// `PrintInfo`: the extra records' names, the time record's flags and the link's kind.
    fn print_info(&self, s: &mut Vec<String>) {
        let extra = &self.extra;
        let mut offset = 0;
        loop {
            let rem = extra.len() - offset;
            if rem == 0 {
                return;
            }
            let Some((size, num)) = read_var(&extra[offset..]) else {
                return;
            };
            offset += num;
            let rem = rem - num;
            let Some(size) = usize::try_from(size).ok().filter(|&size| size <= rem) else {
                break;
            };
            let mut rem = size;
            let Some((id, num)) = read_var(&extra[offset..offset + rem]) else {
                break;
            };
            offset += num;
            rem -= num;
            if id == extra_id::SUBDATA
                && self.record_type == header::SERVICE
                && rem + 1 == extra.len() - offset
            {
                rem += 1;
            }
            let mut word = type_name(&EXTRA_TYPES, id);
            let record = extra.get(offset..offset + rem).unwrap_or_default();
            if id == extra_id::TIME {
                if let Some((flags, _)) = read_var(record) {
                    word.push(':');
                    for (i, c) in EXTRA_TIME_FLAGS.iter().enumerate() {
                        if flags & (1 << i) != 0 {
                            word.push(*c);
                        }
                    }
                    let rest = flags & !((1u64 << EXTRA_TIME_FLAGS.len()) - 1);
                    if rest != 0 {
                        let _ = write!(word, "_0x{rest:X}");
                    }
                }
            } else if id == extra_id::LINK
                && let Some(link) = LinkInfo::parse(record)
            {
                word.push(':');
                word.push_str(&type_name(&LINK_TYPES, link.kind));
                let mut flags = link.flags;
                if flags != 0 {
                    word.push(':');
                    if flags & 1 != 0 {
                        word.push('D');
                        flags &= !1;
                    }
                    if flags != 0 {
                        let _ = write!(word, "_0x{flags:X}");
                    }
                }
            }
            s.push(word);
            offset += rem;
        }
        s.push("ERROR".to_owned());
    }

    /// `GetWinAttrib`: Windows' attributes, a Unix mode above the posix marker.
    const fn win_attrib(&self) -> u32 {
        let mut a = match self.host_os {
            HOST_WINDOWS => self.attrib,
            HOST_UNIX => (self.attrib << 16) | 0x8000,
            _ => 0,
        };
        if self.is_dir() {
            a |= 0x10;
        }
        a
    }
}

/// A time as an item gives it: FILETIME ticks, the nanoseconds past them, and the
/// digits of a second it was kept to.
#[derive(Clone, Copy)]
struct Stamp {
    ticks: u64,
    extra: u8,
    digits: usize,
    prec: TimePrec,
}

impl Stamp {
    const fn unix(seconds: u32) -> Self {
        Self {
            ticks: seconds as u64 * 10_000_000 + UNIX_EPOCH_TICKS,
            extra: 0,
            digits: 0,
            prec: TimePrec::Unix,
        }
    }
}

/// `TimeRecordToProp`: the time record's `index`th time (modified, created, accessed).
fn time_record(item: &RarItem, index: u32) -> Option<Stamp> {
    let record = item.extra_record(extra_id::TIME)?;
    let (flags, num) = read_var(record)?;
    let p = &record[num..];
    if flags & (2 << index) == 0 {
        return None;
    }
    let mut stamps = 0;
    let mut current = 0;
    for i in 0..3 {
        if flags & (2 << i) != 0 {
            if i == index {
                current = stamps;
            }
            stamps += 1;
        }
    }
    if flags & 1 != 0 {
        let at = current * 4;
        let seconds = le32(p, at)?;
        let mut stamp = Stamp::unix(seconds);
        let all = stamps * 4;
        if flags & (1 << 4) != 0
            && all * 2 <= p.len()
            && let Some(ns) = le32(p, at + all).map(|ns| ns & 0x3FFF_FFFF)
            && ns < 1_000_000_000
        {
            stamp.ticks += u64::from(ns / 100);
            stamp.extra = u8::try_from(ns % 100).unwrap_or(0);
            stamp.digits = 9;
            stamp.prec = TimePrec::Digits(9);
        }
        Some(stamp)
    } else {
        let ticks = le64(p, current * 8)?;
        Some(Stamp {
            ticks,
            extra: 0,
            digits: 7,
            prec: TimePrec::Exact,
        })
    }
}

/// The archive's locator record.
#[derive(Clone, Copy, Debug, Default)]
struct Locator {
    flags: u64,
    quick_open: u64,
    recovery: u64,
}

/// The archive's metadata record: its name and when it was made.
#[derive(Clone, Debug, Default)]
struct Metadata {
    flags: u64,
    ctime: u64,
    name: Vec<u8>,
}

/// What a volume's archive header says (`CInArcInfo`).
#[derive(Clone, Debug, Default)]
pub(super) struct ArcInfo {
    flags: u64,
    vol_number: u64,
    pub(super) start_pos: u64,
    pub(super) end_pos: u64,
    end_flags: u64,
    end_read: bool,
    is_encrypted: bool,
    locator: Option<Result<Locator, ()>>,
    metadata: Option<Result<Metadata, ()>>,
    unknown_extra: bool,
    extra_error: bool,
    unsupported_feature: bool,
}

impl ArcInfo {
    pub(super) const fn is_volume(&self) -> bool {
        self.flags & arc_flags::VOL != 0
    }

    const fn is_solid(&self) -> bool {
        self.flags & arc_flags::SOLID != 0
    }

    const fn vol_index(&self) -> u64 {
        if self.flags & arc_flags::VOL_NUMBER != 0 {
            self.vol_number
        } else {
            0
        }
    }

    const fn more_volumes(&self) -> bool {
        self.end_flags & 1 != 0
    }

    pub(super) const fn phy_size(&self) -> u64 {
        self.end_pos.saturating_sub(self.start_pos)
    }

    /// `ParseExtra`: the locator and metadata records.
    fn parse_extra(&mut self, mut p: &[u8]) -> bool {
        loop {
            if p.is_empty() {
                return true;
            }
            let Some((size, num)) = read_var(p) else {
                return false;
            };
            p = &p[num..];
            let Some(size) = usize::try_from(size).ok().filter(|&s| s <= p.len()) else {
                return false;
            };
            let (record, rest) = p.split_at(size);
            p = rest;
            let Some((id, num)) = read_var(record) else {
                return false;
            };
            let body = &record[num..];
            match id {
                1 => self.locator = Some(parse_locator(body).ok_or(())),
                2 => self.metadata = Some(parse_metadata(body).ok_or(())),
                _ => self.unknown_extra = true,
            }
        }
    }
}

fn parse_locator(p: &[u8]) -> Option<Locator> {
    let mut c = Cursor::new(p);
    let mut locator = Locator {
        flags: c.var()?,
        ..Locator::default()
    };
    if locator.flags & 1 != 0 {
        locator.quick_open = c.var()?;
    }
    if locator.flags & 2 != 0 {
        locator.recovery = c.var()?;
    }
    Some(locator)
}

fn parse_metadata(p: &[u8]) -> Option<Metadata> {
    let mut c = Cursor::new(p);
    let mut metadata = Metadata {
        flags: c.var()?,
        ..Metadata::default()
    };
    if metadata.flags & 1 != 0 {
        let len = usize::try_from(c.var()?).ok()?;
        metadata.name = c.take(len)?.to_vec();
    }
    if metadata.flags & 2 != 0 {
        if metadata.flags & 4 != 0 && metadata.flags & 8 == 0 {
            metadata.ctime = u64::from(c.u32()?);
        } else {
            let low = c.u32()?;
            let high = c.u32()?;
            metadata.ctime = u64::from(low) | (u64::from(high) << 32);
        }
    }
    Some(metadata)
}

/// A block's header (`CHeader`).
struct Header {
    kind: u64,
    flags: u64,
    extra_size: usize,
    data_size: u64,
}

/// What reading a header came to, short of an I/O error.
enum Read5 {
    Header(Header),
    /// `S_FALSE`: not a header, or cut short (`unexpected_end` says which).
    Bad,
}

/// The keys of an archive whose headers are encrypted.
struct HeaderKeys {
    key: [u8; 32],
}

/// The encryption record's fields (`CCryptoInfo` and the decoder's properties).
struct CryptoProps {
    flags: u64,
    count: u8,
    salt: [u8; 16],
    iv: Option<[u8; 16]>,
    check: Option<[u8; 12]>,
}

impl CryptoProps {
    /// `SetDecoderProps`: version 0's fields; the IV only in a file's record.
    fn parse(p: &[u8], with_iv: bool) -> Option<Self> {
        let mut c = Cursor::new(p);
        let algo = c.var()?;
        let flags = c.var()?;
        if algo != 0 {
            return None;
        }
        let count = *c.take(1)?.first()?;
        let salt: [u8; 16] = c.take(16)?.try_into().ok()?;
        let iv = if with_iv {
            Some(c.take(16)?.try_into().ok()?)
        } else {
            None
        };
        let check = if flags & 1 != 0 {
            Some(c.take(12)?.try_into().ok()?)
        } else {
            None
        };
        Some(Self {
            flags,
            count,
            salt,
            iv,
            check,
        })
    }
}

/// The password as RAR 5 takes it: UTF-8, its first 127 characters.
pub(super) fn password_bytes(password: &str) -> Vec<u8> {
    let mut units: Vec<u16> = password.encode_utf16().collect();
    units.truncate(127);
    String::from_utf16_lossy(&units).into_bytes()
}

/// `CalcKey_and_CheckPassword`: the keys, and whether the record's check (when its own
/// checksum is right) says the password is the one.
pub(super) fn derive_keys(
    props_check: Option<[u8; 12]>,
    salt: [u8; 16],
    count: u8,
    password: &[u8],
) -> Option<(Rar50Keys, bool)> {
    use sha2::Digest as _;
    let keys = Rar50Keys::derive(password, salt, count).ok()?;
    let ok = match props_check {
        Some(check) => {
            let sum = sha2::Sha256::digest(&check[..8]);
            if sum[..4] == check[8..] {
                keys.password_check == check[..8]
            } else {
                true
            }
        }
        None => true,
    };
    Some((keys, ok))
}

/// One volume's headers, read as `CInArchive` reads them.
struct HeaderReader {
    file: File,
    position: u64,
    end: u64,
    keys: Option<HeaderKeys>,
    buf: Vec<u8>,
    /// Where the header's fields begin and end in `buf`.
    at: usize,
    size: usize,
    unexpected_end: bool,
    is_arc: bool,
    wrong_password: bool,
    need_password: bool,
}

impl HeaderReader {
    fn new(mut file: File) -> io::Result<Self> {
        let end = file.seek(SeekFrom::End(0))?;
        file.seek(SeekFrom::Start(0))?;
        Ok(Self {
            file,
            position: 0,
            end,
            keys: None,
            buf: Vec::new(),
            at: 0,
            size: 0,
            unexpected_end: false,
            is_arc: false,
            wrong_password: false,
            need_password: false,
        })
    }

    /// Reads `len` bytes, or says they were not all there.
    fn read_full(&mut self, len: usize) -> io::Result<Option<Vec<u8>>> {
        let mut data = Vec::with_capacity(len);
        (&mut self.file).take(len as u64).read_to_end(&mut data)?;
        if data.len() == len {
            Ok(Some(data))
        } else {
            self.unexpected_end = true;
            Ok(None)
        }
    }

    fn cursor(&self) -> Cursor<'_> {
        Cursor::new(&self.buf[self.at..self.size])
    }

    const fn advance(&mut self, len: usize) {
        self.at += len;
    }

    fn var(&mut self) -> Option<u64> {
        let (value, len) = read_var(&self.buf[self.at..self.size])?;
        self.at += len;
        Some(value)
    }

    const fn remaining(&self) -> usize {
        self.size - self.at
    }

    /// `ReadBlockHeader`.
    fn read_block_header(&mut self) -> io::Result<Read5> {
        self.file.seek(SeekFrom::Start(self.position))?;
        if let Some(keys) = &self.keys {
            let key = keys.key;
            let Some(first) = self.read_full(32)? else {
                return Ok(Read5::Bad);
            };
            let iv: [u8; 16] = first[..16].try_into().unwrap_or_default();
            let mut block = first[16..].to_vec();
            if Rar50Cipher::new(key, iv)
                .decrypt_in_place(&mut block)
                .is_err()
            {
                return Ok(Read5::Bad);
            }
            let Some((value, num)) = read_var(&block[4..7]) else {
                return Ok(Read5::Bad);
            };
            let Ok(size) = usize::try_from(value) else {
                return Ok(Read5::Bad);
            };
            if size < 2 {
                return Ok(Read5::Bad);
            }
            let total = size + 4 + num;
            let rounded = (total + 15) & !15;
            self.position += rounded as u64 + 16;
            let Some(rest) = self.read_full(rounded - 16)? else {
                return Ok(Read5::Bad);
            };
            let mut data = first[16..].to_vec();
            data.extend_from_slice(&rest);
            if Rar50Cipher::new(key, iv)
                .decrypt_in_place(&mut data)
                .is_err()
            {
                return Ok(Read5::Bad);
            }
            if data[total..].iter().any(|&b| b != 0) {
                return Ok(Read5::Bad);
            }
            data.truncate(total);
            self.buf = data;
            self.at = 4 + num;
            self.size = total;
        } else {
            let Some(start) = self.read_full(7)? else {
                return Ok(Read5::Bad);
            };
            let Some((value, num)) = read_var(&start[4..7]) else {
                return Ok(Read5::Bad);
            };
            let Ok(size) = usize::try_from(value) else {
                return Ok(Read5::Bad);
            };
            if size < 2 {
                return Ok(Read5::Bad);
            }
            let total = size + 4 + num;
            self.position += total as u64;
            let Some(rest) = self.read_full(total - 7)? else {
                return Ok(Read5::Bad);
            };
            let mut data = start;
            data.extend_from_slice(&rest);
            self.buf = data;
            self.at = 4 + num;
            self.size = total;
        }
        let crc = crc32fast::hash(&self.buf[4..self.size]);
        if le32(&self.buf, 0) != Some(crc) {
            return Ok(Read5::Bad);
        }
        let (Some(kind), Some(flags)) = (self.var(), self.var()) else {
            return Ok(Read5::Bad);
        };
        let mut h = Header {
            kind,
            flags,
            extra_size: 0,
            data_size: 0,
        };
        if flags & header::EXTRA != 0 {
            let Some(size) = self.var().filter(|&s| s < 1 << 21) else {
                return Ok(Read5::Bad);
            };
            h.extra_size = usize::try_from(size).unwrap_or(usize::MAX);
        }
        if flags & header::DATA != 0 {
            let Some(size) = self.var() else {
                return Ok(Read5::Bad);
            };
            h.data_size = size;
        }
        if h.extra_size > self.remaining() {
            return Ok(Read5::Bad);
        }
        Ok(Read5::Header(h))
    }

    /// `CInArchive::Open`: the marker, the encryption header if there is one, and the
    /// archive header.
    fn open(&mut self, password: Option<&str>, info: &mut ArcInfo) -> io::Result<bool> {
        let Some(marker) = self.read_full(MARKER.len())? else {
            self.unexpected_end = false;
            return Ok(false);
        };
        if marker != MARKER {
            return Ok(false);
        }
        self.position = MARKER.len() as u64;
        info.start_pos = 0;
        let Read5::Header(mut h) = self.read_block_header()? else {
            return Ok(false);
        };
        if h.kind == header::ARC_ENCRYPT {
            info.is_encrypted = true;
            self.is_arc = true;
            let Some(password) = password else {
                self.need_password = true;
                return Ok(false);
            };
            let Some(props) = CryptoProps::parse(&self.buf[self.at..self.size], false) else {
                return Ok(false);
            };
            let Some((keys, ok)) = derive_keys(
                props.check,
                props.salt,
                props.count,
                &password_bytes(password),
            ) else {
                return Ok(false);
            };
            if !ok {
                self.wrong_password = true;
                return Ok(false);
            }
            self.keys = Some(HeaderKeys { key: keys.key });
            match self.read_block_header()? {
                Read5::Header(next) => h = next,
                Read5::Bad => return Ok(false),
            }
        }
        if h.kind != header::ARC {
            return Ok(false);
        }
        self.is_arc = true;
        info.vol_number = 0;
        let Some(flags) = self.var() else {
            return Ok(false);
        };
        info.flags = flags;
        if flags & arc_flags::VOL_NUMBER != 0 {
            let Some(number) = self.var() else {
                return Ok(false);
            };
            info.vol_number = number;
        }
        if h.extra_size != self.remaining() {
            return Ok(false);
        }
        if h.extra_size != 0 {
            let extra = self.buf[self.at..self.size].to_vec();
            if !info.parse_extra(&extra) {
                info.extra_error = true;
            }
        }
        Ok(true)
    }

    /// `ReadFileHeader`.
    fn read_file_header(&mut self, h: &Header, item: &mut RarItem) -> bool {
        item.common_flags = h.flags;
        item.pack_size = h.data_size;
        item.unix_mtime = 0;
        item.crc = 0;
        let parsed = (|| {
            item.flags = self.var()?;
            item.size = self.var()?;
            item.attrib = low32(self.var()?);
            if item.has_unix_mtime() {
                item.unix_mtime = self.cursor().u32()?;
                self.advance(4);
            }
            if item.has_crc() {
                item.crc = self.cursor().u32()?;
                self.advance(4);
            }
            item.method = low32(self.var()?);
            item.host_os = self.var()?;
            let len = usize::try_from(self.var()?).ok()?;
            if len > self.remaining() {
                return None;
            }
            item.name = self.buf[self.at..self.at + len].to_vec();
            self.advance(len);
            if h.extra_size != 0 {
                if self.remaining() < h.extra_size {
                    return None;
                }
                item.extra = self.buf[self.at..self.at + h.extra_size].to_vec();
                self.advance(h.extra_size);
            } else {
                item.extra.clear();
            }
            Some(())
        })();
        parsed.is_some() && self.remaining() == 0
    }
}

/// An item as listed: its first and last headers, the file an alternate stream
/// belongs to, and the copy it is a link to (`CRefItem`).
#[derive(Clone, Copy, Debug)]
pub(super) struct Ref {
    pub(super) item: usize,
    pub(super) last: usize,
    pub(super) parent: Option<usize>,
}

/// A RAR 5 archive, open.
pub(in crate::sevenzip) struct Rar5 {
    pub(super) volumes: Vec<PathBuf>,
    pub(super) infos: Vec<ArcInfo>,
    pub(super) items: Vec<RarItem>,
    pub(super) refs: Vec<Ref>,
    pub(super) acls: Vec<Vec<u8>>,
    comment: Vec<u8>,
    comment_used: bool,
    error_in_acl: bool,
    split_error: bool,
    is_arc: bool,
    pub(super) unexpected_end: bool,
    pub(super) headers_error: bool,
    pub(super) unsupported_feature: bool,
    pub(super) missing_volume: Option<String>,
    num_blocks: u32,
    compat_mask: u32,
    method_masks: [u32; 2],
    algo_mask: u64,
    dict_max: [u64; 2],
}

/// Why a RAR 5 archive did not open.
pub(super) enum Failure {
    NotArchive,
    UnexpectedEnd,
    PasswordNeeded,
    WrongPassword,
}

impl Rar5 {
    /// `Open2`: the volumes from the one named on, their items read.
    #[expect(
        clippy::too_many_lines,
        reason = "7-Zip's Open2, volume by volume and item by item"
    )]
    pub(super) fn open(path: &Path, password: Option<&str>) -> io::Result<Result<Self, Failure>> {
        let mut rar = Self {
            volumes: Vec::new(),
            infos: Vec::new(),
            items: Vec::new(),
            refs: Vec::new(),
            acls: Vec::new(),
            comment: Vec::new(),
            comment_used: false,
            error_in_acl: false,
            split_error: false,
            is_arc: false,
            unexpected_end: false,
            headers_error: false,
            unsupported_feature: false,
            missing_volume: None,
            num_blocks: 0,
            compat_mask: 0,
            method_masks: [0; 2],
            algo_mask: 0,
            dict_max: [0; 2],
        };
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut names: Option<VolumeName> = None;
        let mut prev_split: Option<usize> = None;
        let mut prev_main: Option<usize> = None;
        let mut next_required = false;
        loop {
            let volume = if rar.infos.is_empty() {
                path.to_path_buf()
            } else {
                let names = names.get_or_insert_with(|| {
                    VolumeName::new(
                        &path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    )
                });
                let name = names.next_name();
                let volume = dir.join(&name);
                if !volume.is_file() {
                    if next_required {
                        rar.missing_volume = Some(name);
                    }
                    break;
                }
                volume
            };
            let file = File::open(&volume)?;
            let mut reader = HeaderReader::new(file)?;
            let end = reader.end;
            let mut info = ArcInfo::default();
            let opened = reader.open(password, &mut info)?;
            if reader.is_arc && reader.unexpected_end {
                rar.unexpected_end = true;
            }
            if rar.infos.is_empty() {
                rar.is_arc = reader.is_arc;
            }
            if !opened {
                if rar.infos.is_empty() {
                    return Ok(Err(if reader.need_password {
                        Failure::PasswordNeeded
                    } else if reader.wrong_password {
                        Failure::WrongPassword
                    } else if reader.is_arc && reader.unexpected_end {
                        Failure::UnexpectedEnd
                    } else {
                        Failure::NotArchive
                    }));
                }
                break;
            }
            rar.volumes.push(volume);
            let vol_index = rar.infos.len();
            loop {
                let mut item = RarItem::default();
                info.end_pos = reader.position;
                if reader.position > end {
                    rar.unexpected_end = true;
                    break;
                }
                let h = match reader.read_block_header()? {
                    Read5::Header(h) => h,
                    Read5::Bad => {
                        if reader.unexpected_end {
                            rar.unexpected_end = true;
                            info.end_pos = info.end_pos.max(reader.position).max(end);
                        } else {
                            rar.headers_error = true;
                        }
                        break;
                    }
                };
                if h.kind == header::END_OF_ARC {
                    info.end_pos = reader.position;
                    info.end_read = true;
                    match reader.var() {
                        Some(flags) => info.end_flags = flags,
                        None => rar.headers_error = true,
                    }
                    if reader.remaining() != 0 || h.extra_size != 0 || h.data_size != 0 {
                        info.unsupported_feature = true;
                    }
                    if info.is_volume() {
                        // RAR may pad a volume with zeros after its end.
                        info.end_pos += zero_tail(&mut reader.file, info.end_pos, end)?;
                    }
                    break;
                }
                if h.kind != header::FILE && h.kind != header::SERVICE {
                    rar.unsupported_feature = true;
                    break;
                }
                item.record_type = h.kind;
                if !reader.read_file_header(&h, &mut item) {
                    rar.headers_error = true;
                    break;
                }
                item.data_pos = reader.position;
                info.end_pos = reader.position;
                let pack_ok = if let Some(next) = reader.position.checked_add(item.pack_size) {
                    reader.position = next;
                    info.end_pos = next;
                    true
                } else {
                    rar.headers_error = true;
                    info.end_pos = info.end_pos.max(end);
                    false
                };
                let mut need_add = true;
                if !rar.comment_used && rar.comment.is_empty() && item.is_cmt() {
                    rar.comment_used = true;
                    if item.pack_size <= COMMENT_MAX
                        && item.pack_size == item.size
                        && item.pack_size != 0
                        && item.method_number() == 0
                        && !item.is_split()
                    {
                        if let Some(data) = stored_data(&mut reader.file, &item, password)? {
                            rar.comment = data;
                        }
                        need_add = false;
                    }
                }
                let mut r = Ref {
                    item: rar.items.len(),
                    last: rar.items.len(),
                    parent: None,
                };
                if need_add {
                    if item.is_service() {
                        if item.is_stm() {
                            r.parent = prev_main;
                        } else {
                            need_add = false;
                            if item.is_acl() {
                                rar.note_acl(&item, reader.keys.is_some(), prev_main);
                            }
                        }
                    }
                    if need_add && item.is_split_before() {
                        if let Some(prev) = prev_split {
                            let last = rar.refs[prev].last;
                            if item.is_next_for(&rar.items[last]) {
                                rar.refs[prev].last = rar.items.len();
                                rar.items[last].next_item = Some(rar.items.len());
                                need_add = false;
                            }
                        } else {
                            rar.split_error = true;
                        }
                    }
                    if need_add {
                        if item.is_split_after() {
                            prev_split = Some(rar.refs.len());
                        }
                        if !item.is_service() {
                            prev_main = Some(rar.refs.len());
                        }
                    }
                }
                item.version = item.find_version();
                item.vol_index = vol_index;
                rar.items.push(item);
                if need_add {
                    rar.refs.push(r);
                }
                if !pack_ok {
                    break;
                }
            }
            next_required = false;
            let is_volume = info.is_volume();
            let more = !info.end_read || info.more_volumes();
            if info.end_read && info.more_volumes() {
                next_required = true;
            }
            rar.infos.push(info);
            if !is_volume || !more {
                break;
            }
        }
        rar.fill_stats();
        Ok(Ok(rar))
    }

    /// An ACL service header: an error where 7-Zip finds one, else its data kept for the
    /// file before it (decoded when the file is extracted).
    fn note_acl(&mut self, item: &RarItem, crypto_mode: bool, prev_main: Option<usize>) {
        if (item.is_encrypted() && !crypto_mode)
            || item.is_solid()
            || prev_main.is_none()
            || item.size >= 1 << 24
            || item.size == 0
        {
            self.error_in_acl = true;
        }
    }

    /// `FillLinks`, the counting half: blocks, and each algorithm's methods and largest
    /// dictionary.
    fn fill_stats(&mut self) {
        for r in &self.refs {
            let item = &self.items[r.item];
            if !item.is_solid() {
                self.num_blocks += 1;
            }
            let algo = item.algo_raw();
            self.algo_mask |= 1u64 << algo;
            self.compat_mask |= 1 << item.compat_bit();
            if !item.is_dir() && (algo as usize) < self.method_masks.len() {
                let a = algo as usize;
                self.method_masks[a] |= 1 << item.method_number();
                self.dict_max[a] = self.dict_max[a].max(item.dict());
            }
        }
    }

    /// The pack sizes of an item's headers, summed (`GetPackSize`).
    pub(super) fn pack_size(&self, r: &Ref) -> u64 {
        let mut size = 0u64;
        let mut index = r.item;
        loop {
            let item = &self.items[index];
            size = size.wrapping_add(item.pack_size);
            match item.next_item {
                Some(next) => index = next,
                None => return size,
            }
        }
    }

    /// `kpidErrorFlags`.
    pub(super) fn error_flags(&self) -> Vec<&'static str> {
        let mut flags = Vec::new();
        if !self.is_arc {
            flags.push("Is not archive");
        }
        if self.headers_error || self.error_in_acl || self.split_error {
            flags.push("Headers Error");
        }
        if self.unexpected_end {
            flags.push("Unexpected end of archive");
        }
        if self.unsupported_feature {
            flags.push("Unsupported feature");
        }
        flags
    }

    /// `GetArchiveProperty`, in `kArcProps`' order; the empty ones left out.
    #[expect(
        clippy::too_many_lines,
        reason = "7-Zip's GetArchiveProperty, property by property"
    )]
    pub(super) fn archive_props(
        &self,
        zone: &cash_core::timefmt::Zone,
    ) -> Vec<(&'static str, String)> {
        let mut props = Vec::new();
        let info = self.infos.first();
        let plus = |b: bool| if b { "+" } else { "-" }.to_owned();
        if self.infos.len() > 1 {
            let sum: u64 = self.infos.iter().map(ArcInfo::phy_size).sum();
            props.push(("Total Physical Size", sum.to_string()));
        }
        let mut words = Vec::new();
        if let Some(info) = info {
            let flags = flags_to_string(&ARC_FLAG_NAMES, info.flags);
            if !flags.is_empty() {
                words.push(flags);
            }
            if info.extra_error {
                words.push("Extra-ERROR".to_owned());
            }
            if info.unsupported_feature {
                words.push("unsupported-feature".to_owned());
            }
            if let Some(metadata) = &info.metadata {
                match metadata {
                    Err(()) => words.push("Metadata-ERROR".to_owned()),
                    Ok(m) => {
                        words.push("Metadata".to_owned());
                        if m.flags & 1 != 0 {
                            words.push("arc-name".to_owned());
                        }
                        if m.flags & 2 != 0 {
                            let kind = if m.flags & 4 == 0 {
                                "win"
                            } else if m.flags & 8 != 0 {
                                "1ns"
                            } else {
                                "1s"
                            };
                            words.push(format!("ctime-{kind}"));
                        }
                    }
                }
            }
            if let Some(locator) = &info.locator {
                match locator {
                    Err(()) => words.push("Locator-ERROR".to_owned()),
                    Ok(l) => {
                        words.push("Locator".to_owned());
                        if l.flags & 1 != 0 {
                            words.push(format!("QuickOpen:{}", l.quick_open));
                        }
                        if l.flags & 2 != 0 {
                            words.push(format!("Recovery:{}", l.recovery));
                        }
                    }
                }
            }
            if info.unknown_extra {
                words.push("Unknown-Extra-Record".to_owned());
            }
        }
        if self.comment_used {
            words.push("Comment".to_owned());
        }
        if !self.acls.is_empty() {
            words.push("ACL".to_owned());
        }
        if !words.is_empty() {
            props.push(("Characteristics", words.join(" ")));
        }
        if let Some(info) = info {
            props.push(("Encrypted", plus(info.is_encrypted)));
            props.push(("Solid", plus(info.is_solid())));
        }
        props.push(("Blocks", self.num_blocks.to_string()));
        props.push(("Method", self.method()));
        if let Some(info) = info {
            props.push(("Multivolume", plus(info.is_volume())));
            if info.is_volume() {
                props.push(("Volume Index", info.vol_index().to_string()));
            }
        }
        props.push(("Volumes", self.infos.len().to_string()));
        if let Some(Ok(metadata)) = info.and_then(|i| i.metadata.as_ref()) {
            if !metadata.name.is_empty() {
                props.push(("Name", String::from_utf8_lossy(&metadata.name).into_owned()));
            }
            if metadata.flags & 2 != 0 {
                let ctime = metadata.ctime;
                let text = if metadata.flags & 4 != 0 {
                    if metadata.flags & 8 != 0 {
                        let seconds = ctime / 1_000_000_000;
                        let ns = ctime % 1_000_000_000;
                        let ticks = seconds * 10_000_000 + UNIX_EPOCH_TICKS + ns / 100;
                        super::super::text::time_ns(
                            zone,
                            ticks,
                            u8::try_from(ns % 100).unwrap_or(0),
                            9,
                        )
                    } else {
                        super::super::text::time_ns(
                            zone,
                            ctime * 10_000_000 + UNIX_EPOCH_TICKS,
                            0,
                            0,
                        )
                    }
                } else {
                    super::super::text::time_ns(zone, ctime, 0, 7)
                };
                if !text.is_empty() {
                    props.push(("Created", text));
                }
            }
        }
        if !self.comment.is_empty() {
            let text = String::from_utf8_lossy(&self.comment);
            let text = text.split('\0').next().unwrap_or_default();
            props.push(("Comment", multi_line(text)));
        }
        props
    }

    /// The archive's `Method =`: each algorithm, its largest dictionary and its methods.
    fn method(&self) -> String {
        let mut words: Vec<String> = Vec::new();
        let mut algo = self.algo_mask;
        let mut v = 0usize;
        while algo != 0 {
            if algo & 1 != 0 {
                let mut s = format!("v{}", v + 6);
                if v < self.method_masks.len() {
                    if self.dict_max[v] != 0 {
                        s.push(':');
                        s.push_str(&dict_size(self.dict_max[v]));
                    }
                    let mut method = self.method_masks[v];
                    let mut m = 0;
                    while method != 0 {
                        if method & 1 != 0 {
                            let _ = write!(s, ":m{m}");
                        }
                        m += 1;
                        method >>= 1;
                    }
                }
                words.push(s);
            }
            v += 1;
            algo >>= 1;
        }
        let mut s = words.join(" ");
        if self.compat_mask & 2 != 0 {
            s.push_str(":c");
            if self.compat_mask & 1 != 0 {
                s.push('n');
            }
        }
        s
    }

    /// Each listed item, with its properties as 7-Zip gives them.
    pub(super) fn listed(&self) -> Vec<Item> {
        self.refs.iter().map(|r| self.listed_item(r)).collect()
    }

    #[expect(
        clippy::too_many_lines,
        reason = "7-Zip's GetProperty, property by property"
    )]
    fn listed_item(&self, r: &Ref) -> Item {
        let item = &self.items[r.item];
        let last = &self.items[r.last];
        let plus = |b: bool| if b { "+" } else { "-" }.to_owned();
        let mut path = if item.is_stm() {
            let mut s = r
                .parent
                .map(|p| String::from_utf8_lossy(&self.items[self.refs[p].item].name).into_owned())
                .unwrap_or_default();
            let name = item.alt_stream_name().unwrap_or_default();
            if !name.starts_with(':') {
                s.push(':');
            }
            s.push_str(&name);
            s
        } else {
            let mut s = String::from_utf8_lossy(&item.name).into_owned();
            if let Some(version) = item.version {
                s = format!("[VER]/{version}/{s}");
            }
            s
        };
        path = path.replace('\\', "/");
        while path.ends_with('/') {
            path.pop();
        }
        let mut modified = time_record(item, 0);
        if modified.is_none() && item.has_unix_mtime() {
            modified = Some(Stamp::unix(item.unix_mtime));
        }
        if modified.is_none()
            && let Some(parent) = r.parent
        {
            let base = &self.items[self.refs[parent].item];
            modified = time_record(base, 0);
            if modified.is_none() && base.has_unix_mtime() {
                modified = Some(Stamp::unix(base.unix_mtime));
            }
        }
        let created = time_record(item, 1);
        let accessed = time_record(item, 2);
        let crc_item = if last.is_split_after() { item } else { last };
        let crc = (crc_item.has_crc() && !crc_item.is_encrypted()).then_some(crc_item.crc);
        let vol_index = self
            .infos
            .get(item.vol_index)
            .filter(|i| i.is_volume())
            .map(|i| i.vol_index().to_string())
            .unwrap_or_default();
        let mut characteristics = Vec::new();
        if item.acl.is_some() {
            characteristics.push("ACL".to_owned());
        }
        if item.flags != 0 {
            let flags = flags_to_string(&FILE_FLAG_NAMES, item.flags);
            if !flags.is_empty() {
                characteristics.push(flags);
            }
        }
        item.print_info(&mut characteristics);
        // `PrintInfo` ends with "ERROR" only when the records did not parse; drop it
        // when they did.
        let checksum = item
            .blake_offset()
            .and_then(|at| item.extra.get(at..at + 32))
            .map(|digest| {
                digest.iter().fold(String::new(), |mut hex, b| {
                    let _ = write!(hex, "{b:02x}");
                    hex
                })
            })
            .unwrap_or_default();
        let mut stamps = [modified, created, accessed];
        let mut listed = Item {
            path,
            is_dir: item.is_dir(),
            size: (!last.is_unknown_size()).then_some(last.size),
            packed: Some(self.pack_size(r)),
            attrib: Some(item.win_attrib()),
            crc,
            encrypted: item.is_encrypted(),
            method: Some(item_method(item)),
            host_os: Some(type_name(&["Windows", "Unix"], item.host_os)),
            ..Item::default()
        };
        for (slot, stamp) in stamps.iter_mut().enumerate() {
            if let Some(stamp) = stamp.take() {
                match slot {
                    0 => {
                        listed.modified = Some(stamp.ticks);
                        listed.mtime_prec = stamp.prec;
                    }
                    1 => listed.created = Some(stamp.ticks),
                    _ => listed.accessed = Some(stamp.ticks),
                }
                listed.time_digits[slot] = stamp.digits;
                listed.time_extra[slot] = stamp.extra;
            }
        }
        listed.extra = vec![
            (Prop::Folder, plus(item.is_dir())),
            (Prop::AltStream, plus(item.is_stm())),
            (Prop::Solid, plus(item.is_solid())),
            (Prop::SplitBefore, plus(item.is_split_before())),
            (Prop::SplitAfter, plus(last.is_split_after())),
            (Prop::Characteristics, characteristics.join(" ")),
            (Prop::SymLink, item.link_target(link_type::UNIX_SYMLINK)),
            (Prop::HardLink, item.link_target(link_type::HARD_LINK)),
            (Prop::CopyLink, item.link_target(link_type::FILE_COPY)),
            (Prop::VolumeIndex, vol_index),
            (Prop::Checksum, checksum),
            (Prop::NtSecurity, String::new()),
        ];
        listed
    }
}

/// The properties a technical listing shows for a RAR 5 item, in 7-Zip's order.
pub(super) const ITEM_PROPS: [Prop; 23] = [
    Prop::Path,
    Prop::Folder,
    Prop::Size,
    Prop::PackedSize,
    Prop::Modified,
    Prop::Created,
    Prop::Accessed,
    Prop::Attributes,
    Prop::AltStream,
    Prop::Encrypted,
    Prop::Solid,
    Prop::SplitBefore,
    Prop::SplitAfter,
    Prop::Crc,
    Prop::HostOs,
    Prop::Method,
    Prop::Characteristics,
    Prop::SymLink,
    Prop::HardLink,
    Prop::CopyLink,
    Prop::VolumeIndex,
    Prop::Checksum,
    Prop::NtSecurity,
];

/// An item's `Method =`: the algorithm version, the method and the dictionary, and the
/// encryption's count and flags.
fn item_method(item: &RarItem) -> String {
    let mut s = format!("v{}", item.algo_raw() + 6);
    if item.is_rar5_compat() {
        s.push('c');
    }
    let _ = write!(s, ":m{}", item.method_number());
    if !item.is_dir() {
        s.push(':');
        s.push_str(&dict_size(item.dict()));
        if item.is_rar5_compat() {
            s.push_str(":c");
        }
    }
    if let Some((offset, size)) = item.find_extra(extra_id::CRYPTO) {
        let record = item.extra.get(offset..offset + size).unwrap_or_default();
        let mut c = Cursor::new(record);
        let algo = c.var().unwrap_or(0);
        s.push(' ');
        if algo == 0 {
            s.push_str("AES");
        } else {
            let _ = write!(s, "Crypto_{algo}");
        }
        // `CCryptoInfo::Parse`: the count and the flags, when the record is whole.
        let mut c = Cursor::new(record);
        if let (Some(_), Some(flags)) = (c.var(), c.var()) {
            let count = c.rest().first().copied().unwrap_or(0);
            let want = 1 + 16 + 16 + if flags & 1 != 0 { 12 } else { 0 };
            if c.rest().len() == want {
                let _ = write!(s, ":{count}:{flags}");
            }
        }
    }
    s
}

/// `PrintPropVal_MultiLine`: a value with line breaks on lines of its own, in braces.
pub(super) fn multi_line(text: &str) -> String {
    if !text.contains('\n') {
        return text.to_owned();
    }
    let text = text.replace("\r\n", "\n");
    let mut out = String::from("\n{\n");
    let mut rest = text.as_str();
    while !rest.is_empty() {
        if let Some((line, after)) = rest.split_once('\n') {
            out.push_str(line);
            out.push('\n');
            rest = after;
        } else {
            out.push_str(rest);
            out.push('\n');
            break;
        }
    }
    out.push('}');
    out
}

/// A stored item's data, decrypted when it is encrypted, as `DecodeToBuf` reads the
/// archive comment: none where its password or checksum is not right.
fn stored_data(
    file: &mut File,
    item: &RarItem,
    password: Option<&str>,
) -> io::Result<Option<Vec<u8>>> {
    file.seek(SeekFrom::Start(item.data_pos))?;
    let mut data = Vec::new();
    file.take(item.pack_size).read_to_end(&mut data)?;
    if data.len() as u64 != item.pack_size {
        return Ok(None);
    }
    let mut mac = None;
    if let Some(record) = item.extra_record(extra_id::CRYPTO) {
        let Some(password) = password else {
            return Ok(None);
        };
        let Some(props) = CryptoProps::parse(record, true) else {
            return Ok(None);
        };
        let Some((keys, true)) = derive_keys(
            props.check,
            props.salt,
            props.count,
            &password_bytes(password),
        ) else {
            return Ok(None);
        };
        let Some(iv) = props.iv else {
            return Ok(None);
        };
        if Rar50Cipher::new(keys.key, iv)
            .decrypt_in_place(&mut data)
            .is_err()
        {
            return Ok(None);
        }
        if props.flags & 2 != 0 {
            mac = Some(keys);
        }
    }
    let Ok(size) = usize::try_from(item.size) else {
        return Ok(None);
    };
    if data.len() < size {
        return Ok(None);
    }
    data.truncate(size);
    if item.has_crc() {
        let crc = crc32fast::hash(&data);
        let crc = mac.as_ref().map_or(crc, |keys| keys.mac_crc32(crc));
        if crc != item.crc {
            return Ok(None);
        }
    }
    if let Some(at) = item.blake_offset() {
        let digest = cash_archive::rar::rar50::blake2sp::hash(&data);
        let digest = mac.as_ref().map_or(digest, |keys| keys.mac_hash32(digest));
        if item.extra.get(at..at + 32) != Some(&digest[..]) {
            return Ok(None);
        }
    }
    Ok(Some(data))
}

/// `ReadZeroTail`: how many zeros follow a volume's end, up to 4 KiB, when nothing else
/// does.
fn zero_tail(file: &mut File, at: u64, end: u64) -> io::Result<u64> {
    const MAX: u64 = 1 << 12;
    if at >= end {
        return Ok(0);
    }
    let len = end - at;
    if len > MAX {
        return Ok(0);
    }
    file.seek(SeekFrom::Start(at))?;
    let mut data = Vec::new();
    file.take(len).read_to_end(&mut data)?;
    Ok(if data.iter().all(|&b| b == 0) { len } else { 0 })
}
