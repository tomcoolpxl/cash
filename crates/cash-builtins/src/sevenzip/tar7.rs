//! tar as 7-Zip's tar handler reads it (`TarIn.cpp`, `TarHandler.cpp`): header by
//! header, GNU long names and links and pax records gathered into the item they belong
//! to, the facts it lists for each item and for the archive, and each item's data.
//!
//! 7-Zip's view differs from GNU tar's (cash's `tar`): what it calls damage, what it
//! counts as headers, and the properties it shows.

use std::fmt::Write as _;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use cash_archive::sevenz::Problem;

use super::archive::{Data, Item, Prop, TimePrec};
use super::volume::{Location, Source};

const RECORD: u64 = 512;

/// FILETIME ticks at the Unix epoch.
const UNIX_EPOCH_TICKS: i128 = 116_444_736_000_000_000;

/// A time from a pax record: seconds, nanoseconds, and how many digits it had.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PaxTime {
    pub(super) sec: i64,
    pub(super) ns: u32,
    pub(super) digits: Option<usize>,
}

/// One item as 7-Zip's `CItemEx` holds it.
#[derive(Clone, Debug, Default)]
#[expect(clippy::struct_excessive_bools, reason = "7-Zip's flags for an item")]
pub(super) struct TarItem {
    pub(super) pack_size: u64,
    pub(super) size: u64,
    pub(super) mtime: i64,
    mtime_is_bin: bool,
    pack_size_is_bin: bool,
    size_is_bin: bool,
    pub(super) link_flag: u8,
    pub(super) dev_major: Option<u32>,
    pub(super) dev_minor: Option<u32>,
    pub(super) mode: u32,
    pub(super) uid: u32,
    pub(super) gid: u32,
    pub(super) name: Vec<u8>,
    pub(super) link_name: Vec<u8>,
    pub(super) user: Vec<u8>,
    pub(super) group: Vec<u8>,
    pub(super) magic: [u8; 8],
    pub(super) mtime_pax: Option<PaxTime>,
    pub(super) atime_pax: Option<PaxTime>,
    pub(super) ctime_pax: Option<PaxTime>,
    pub(super) sparse: Vec<(u64, u64)>,
    header_error: bool,
    method_error: bool,
    signed_checksum: bool,
    prefix_used: bool,
    pax_error: bool,
    pax_overflow: bool,
    pax_path: bool,
    pax_link: bool,
    pax_size: bool,
    long_name: bool,
    long_name_2: bool,
    long_link: bool,
    long_link_2: bool,
    pub(super) header_pos: u64,
    pub(super) header_size: u64,
    pax_records: u64,
    pax_record_path: Vec<u8>,
    pax_raw_lines: Vec<u8>,
    schily_fflags: Vec<u8>,
}

impl TarItem {
    pub(super) const fn is_symlink(&self) -> bool {
        self.link_flag == b'2' && self.size == 0
    }

    const fn is_hardlink(&self) -> bool {
        self.link_flag == b'1'
    }

    pub(super) const fn is_sparse(&self) -> bool {
        self.link_flag == b'S'
    }

    const fn unpack_size(&self) -> u64 {
        if self.is_symlink() {
            self.link_name.len() as u64
        } else {
            self.size
        }
    }

    pub(super) const fn pack_size_aligned(&self) -> u64 {
        (self.pack_size + 0x1FF) & !0x1FF
    }

    pub(super) fn is_dir(&self) -> bool {
        match self.link_flag {
            b'5' | b'D' => true,
            0 | b'0' | b'2' => self.name.last() == Some(&b'/'),
            _ => false,
        }
    }

    pub(super) fn combined_mode(&self) -> u32 {
        let kind = match self.link_flag {
            b'2' => 0o120_000,
            b'4' => 0o060_000,
            b'3' => 0o020_000,
            b'6' => 0o010_000,
            _ if self.is_dir() => 0o040_000,
            _ => 0o100_000,
        };
        (self.mode & !0o170_000) | kind
    }

    fn is_magic_gnu(&self) -> bool {
        &self.magic == b"ustar  \0"
    }

    fn is_magic_posix(&self) -> bool {
        &self.magic == b"ustar\x0000"
    }

    fn is_magic_ustar5(&self) -> bool {
        &self.magic[..5] == b"ustar"
    }

    const fn is_warning(&self) -> bool {
        self.pack_size < self.size && self.link_flag == b'5'
    }

    pub(super) const fn data_pos(&self) -> u64 {
        self.header_pos + self.header_size
    }
}

/// What the whole archive showed (`CArchive`'s flags).
#[derive(Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "7-Zip's flags for an archive"
)]
struct Flags {
    gnu: bool,
    posix: bool,
    pax_items: bool,
    prefix: bool,
    long_name: bool,
    long_link: bool,
    pax: bool,
    pax_path: bool,
    pax_link: bool,
    mtime: bool,
    atime: bool,
    ctime: bool,
    schily: bool,
    pax_global_error: bool,
    warning: bool,
    ascii: bool,
}

/// How reading stopped short (`k_ErrorType_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fault {
    UnexpectedEnd,
    Corrupted,
}

/// A buffer of a long name, a long link or pax records (`CTempBuffer`).
#[derive(Default)]
struct TempBuffer {
    data: Vec<u8>,
    string_size: usize,
    confirmed: bool,
    nonzero_tail: bool,
}

/// What opening a tar found.
pub(super) struct Opening {
    pub(super) physical_size: u64,
    pub(super) props: Vec<(&'static str, String)>,
    pub(super) items: Vec<Item>,
    pub(super) error_flags: Vec<&'static str>,
    pub(super) warning_flags: Vec<&'static str>,
    pub(super) tar: Tar,
}

/// A tar archive's items, for reading their data.
pub(super) struct Tar {
    pub(super) location: Location,
    pub(super) items: Vec<TarItem>,
    /// Whether pax headers stood as items of their own (`_are_Pax_Items`).
    pub(super) pax_items: bool,
    /// Whether reading met an error or a warning, which updating refuses.
    pub(super) faulty: bool,
}

/// `IsArc_Tar`: a first header whose mode, size, time and checksum read as numbers.
pub(super) fn looks_like_tar(head: &[u8]) -> bool {
    if head.len() < 512 {
        return false;
    }
    let mut packed_bin = false;
    let mut time_bin = false;
    octal32(&head[100..108], true).is_some()
        && parse_size(&head[124..136], &mut packed_bin).is_some()
        && parse_mtime(&head[136..148], &mut time_bin).is_some()
        && octal32(&head[148..156], false).is_some()
}

/// Whether the first header's checksum is right, as opening checks it (summed as
/// unsigned bytes or, as old tars did, signed ones).
pub(super) fn header_sum_ok(head: &[u8]) -> bool {
    let (Some(record), Some(checksum)) = (head.get(..512), head.get(148..156)) else {
        return false;
    };
    let Some(checksum) = octal32(checksum, false) else {
        return false;
    };
    let mut buf = record.to_vec();
    buf[148..156].fill(b' ');
    let sum: u32 = buf.iter().map(|&b| u32::from(b)).sum();
    #[expect(clippy::cast_possible_wrap, reason = "old tars sum signed bytes")]
    let signed: i32 = buf.iter().map(|&b| i32::from(b as i8)).sum();
    #[expect(clippy::cast_sign_loss, reason = "compared as 7-Zip compares")]
    let signed = signed as u32;
    sum == checksum || signed == checksum
}

/// `OctalToNumber`: spaces, octal digits, then a space or NUL.
fn octal(field: &[u8], allow_empty: bool) -> Option<u64> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    let text = &field[..end];
    let start = text.iter().position(|&b| b != b' ').unwrap_or(text.len());
    let text = &text[start..];
    if text.is_empty() {
        return allow_empty.then_some(0);
    }
    let digits = text
        .iter()
        .take_while(|b| (b'0'..=b'7').contains(b))
        .count();
    let mut value = 0u64;
    for &b in &text[..digits] {
        value = value.checked_mul(8)?.checked_add(u64::from(b - b'0'))?;
    }
    match text.get(digits) {
        None | Some(b' ' | 0) => Some(value),
        _ => None,
    }
}

fn octal32(field: &[u8], allow_empty: bool) -> Option<u32> {
    octal(field, allow_empty).and_then(|v| u32::try_from(v).ok())
}

fn be64(b: &[u8]) -> u64 {
    b.iter()
        .take(8)
        .fold(0, |acc, &x| (acc << 8) | u64::from(x))
}

fn be32(b: &[u8]) -> u32 {
    b.iter()
        .take(4)
        .fold(0, |acc, &x| (acc << 8) | u32::from(x))
}

/// `ParseSize`: GNU's binary form, or octal (empty allowed).
fn parse_size(field: &[u8], is_bin: &mut bool) -> Option<u64> {
    if be32(field) == 1 << 31 {
        *is_bin = true;
        let v = be64(&field[4..]);
        return (v >> 63 == 0).then_some(v);
    }
    *is_bin = false;
    octal(field, true)
}

/// `ParseInt64_MTime`: zeros or spaces are 0, else binary or octal.
#[expect(
    clippy::cast_possible_wrap,
    reason = "7-Zip's signed reading of the field"
)]
fn parse_mtime(field: &[u8], is_bin: &mut bool) -> Option<i64> {
    *is_bin = false;
    if field[0] == 0 || field.iter().all(|&b| b == b' ') {
        return Some(0);
    }
    let h = be32(field);
    let v = be64(&field[4..]) as i64;
    if h == 1 << 31 {
        *is_bin = true;
        return (v >= 0).then_some(v);
    }
    if h == u32::MAX {
        *is_bin = true;
        return (v < 0).then_some(v);
    }
    octal(field, false).map(|u| u as i64)
}

/// A field as a string: up to its first NUL.
fn string(field: &[u8]) -> Vec<u8> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    field[..end].to_vec()
}

/// The archive's file, read record by record (`CArchive`).
struct Reader {
    file: Source,
    phy_size: u64,
    headers_size: u64,
    fault: Option<Fault>,
    flags: Flags,
    pax_global: Option<(Vec<u8>, Vec<u8>)>,
}

impl Reader {
    fn read_record(&mut self, buf: &mut [u8; 512]) -> io::Result<usize> {
        let mut done = 0;
        while done < buf.len() {
            match self.file.read(&mut buf[done..])? {
                0 => break,
                n => done += n,
            }
        }
        Ok(done)
    }

    /// `GetNextItemReal`: one header record, after any zero records. `Ok(None)` is no
    /// item (the end, or damage set in `fault`).
    #[expect(clippy::too_many_lines, reason = "7-Zip's header, field by field")]
    fn next_real(&mut self, item: &mut TarItem) -> io::Result<Option<()>> {
        let mut buf = [0u8; 512];
        let mut empty_records = false;
        self.fault = None;
        loop {
            let n = self.read_record(&mut buf)?;
            if n == 0 {
                if !empty_records {
                    self.fault = Some(Fault::UnexpectedEnd);
                }
                return Ok(None);
            }
            if n != 512 {
                if !empty_records {
                    self.fault = Some(Fault::UnexpectedEnd);
                }
                return Ok(None);
            }
            if buf.iter().any(|&b| b != 0) {
                break;
            }
            item.header_size += RECORD;
            empty_records = true;
        }
        if empty_records {
            return Ok(None);
        }
        self.fault = Some(Fault::Corrupted);
        let Some(mode) = octal32(&buf[100..108], true) else {
            return Ok(None);
        };
        item.mode = mode;
        item.uid = octal32(&buf[108..116], false).unwrap_or(0);
        item.gid = octal32(&buf[116..124], false).unwrap_or(0);
        let Some(pack) = parse_size(&buf[124..136], &mut item.pack_size_is_bin) else {
            return Ok(None);
        };
        item.pack_size = pack;
        item.size = pack;
        item.size_is_bin = item.pack_size_is_bin;
        let Some(mtime) = parse_mtime(&buf[136..148], &mut item.mtime_is_bin) else {
            return Ok(None);
        };
        item.mtime = mtime;
        let Some(checksum) = octal32(&buf[148..156], false) else {
            return Ok(None);
        };
        buf[148..156].fill(b' ');
        item.link_flag = buf[156];
        item.link_name = string(&buf[157..257]);
        item.magic.copy_from_slice(&buf[257..265]);
        item.user = string(&buf[265..297]);
        item.group = string(&buf[297..329]);
        item.dev_major = None;
        if buf[329] != 0 {
            let Some(v) = octal32(&buf[329..337], false) else {
                return Ok(None);
            };
            item.dev_major = Some(v);
        }
        item.dev_minor = None;
        if buf[337] != 0 {
            let Some(v) = octal32(&buf[337..345], false) else {
                return Ok(None);
            };
            item.dev_minor = Some(v);
        }
        if buf[345] != 0 && item.is_magic_ustar5() && item.link_flag != b'L' {
            item.prefix_used = true;
            let mut name = string(&buf[345..500]);
            name.push(b'/');
            name.extend(string(&buf[..100]));
            item.name = name;
        } else {
            item.name = string(&buf[..100]);
        }
        if item.link_flag == b'1' {
            item.pack_size = 0;
            item.size = 0;
        }
        if item.link_flag == b'5' {
            item.pack_size = 0;
        }
        let sum: u32 = buf.iter().map(|&b| u32::from(b)).sum();
        if sum != checksum {
            #[expect(clippy::cast_possible_wrap, reason = "old tars sum signed bytes")]
            let signed: i32 = buf.iter().map(|&b| i32::from(b as i8)).sum();
            #[expect(clippy::cast_sign_loss, reason = "compared as 7-Zip compares")]
            if signed as u32 != checksum {
                return Ok(None);
            }
            item.signed_checksum = true;
        }
        item.header_size += RECORD;
        if item.link_flag == b'S' {
            let mut is_bin = false;
            let Some(real) = parse_size(&buf[483..495], &mut is_bin) else {
                return Ok(None);
            };
            item.size = real;
            item.size_is_bin = is_bin;
            if item.size < item.pack_size {
                return Ok(None);
            }
            let mut record = buf;
            let mut at = 386usize;
            let mut count = 4;
            let mut extended = record[386 + 4 * 24];
            let (mut end, mut packed) = (0u64, 0u64);
            loop {
                if extended > 1 {
                    return Ok(None);
                }
                for _ in 0..count {
                    let entry = &record[at..at + 24];
                    if be32(entry) == 0 {
                        if extended != 0 {
                            return Ok(None);
                        }
                        break;
                    }
                    let mut b = false;
                    let (Some(offset), Some(size)) = (
                        parse_size(&entry[..12], &mut b),
                        parse_size(&entry[12..], &mut b),
                    ) else {
                        return Ok(None);
                    };
                    at += 24;
                    if offset < end || item.size < offset || item.size - offset < size {
                        return Ok(None);
                    }
                    if size != 0 && (end & 0x1FF != 0 || offset & 0x1FF != 0) {
                        item.method_error = true;
                    }
                    end = offset + size;
                    packed += size;
                    item.sparse.push((offset, size));
                }
                if extended == 0 {
                    break;
                }
                if self.read_record(&mut record)? != 512 {
                    self.fault = Some(Fault::UnexpectedEnd);
                    return Ok(None);
                }
                item.header_size += RECORD;
                at = 0;
                count = 21;
                extended = record[21 * 24];
            }
            if end != item.size || packed != item.pack_size {
                item.method_error = true;
            }
        }
        if item.pack_size >= 1 << 63 {
            return Ok(None);
        }
        self.fault = None;
        Ok(Some(()))
    }

    /// `ReadDataToBuffer`: a long name's, long link's or pax record's data, up to
    /// `limit` bytes kept, the rest passed over.
    fn read_to_buffer(&mut self, item: &TarItem, limit: usize) -> io::Result<Option<TempBuffer>> {
        let mut tb = TempBuffer::default();
        let mut pack = item.pack_size_aligned();
        if pack == 0 {
            return Ok(Some(tb));
        }
        let size = usize::try_from(pack).map_or(limit, |p| p.min(limit));
        tb.data = vec![0u8; size];
        let mut done = 0;
        while done < size {
            match self.file.read(&mut tb.data[done..])? {
                0 => break,
                n => done += n,
            }
        }
        if done != size {
            self.fault = Some(Fault::UnexpectedEnd);
            return Ok(None);
        }
        pack -= size as u64;
        let zero = tb.data.iter().position(|&b| b == 0).unwrap_or(size);
        if zero as u64 >= item.pack_size {
            tb.confirmed = true;
        }
        if zero as u64 > item.pack_size {
            tb.string_size = usize::try_from(item.pack_size).unwrap_or(0);
            tb.nonzero_tail = true;
        } else {
            tb.string_size = zero;
            if zero != size {
                tb.confirmed = true;
                if tb.data[zero..].iter().any(|&b| b != 0) {
                    tb.nonzero_tail = true;
                }
            }
        }
        if pack != 0 {
            let here = self.file.stream_position()?;
            self.file.seek(SeekFrom::Start(here + pack))?;
        }
        Ok(Some(tb))
    }

    /// `ReadItem2`: an item, with the long names, long links and pax records before it.
    #[expect(
        clippy::too_many_lines,
        reason = "7-Zip's ReadItem2, record kind by kind"
    )]
    fn read_item(&mut self, item: &mut TarItem) -> io::Result<bool> {
        let mut name_buf = TempBuffer::default();
        let mut link_buf = TempBuffer::default();
        let mut pax_buf = TempBuffer::default();
        let mut extra_records = 0u64;
        loop {
            let real = self.next_real(item)?;
            if real.is_none() {
                if self.fault.is_none()
                    && (extra_records != 0
                        || item.long_name
                        || item.long_link
                        || item.pax_records != 0)
                {
                    self.fault = Some(Fault::Corrupted);
                }
                return Ok(false);
            }
            extra_records += 1;
            let lf = item.link_flag;
            if lf == b'L' || lf == b'K' {
                let Some(mut tb) = self.read_to_buffer(item, 1 << 14)? else {
                    return Ok(false);
                };
                if lf == b'L' {
                    item.long_name_2 = item.long_name;
                    item.long_name = true;
                } else {
                    item.long_link_2 = item.long_link;
                    item.long_link = true;
                }
                if !tb.confirmed {
                    tb.string_size = 0;
                }
                item.header_size += item.pack_size_aligned();
                if tb.string_size == 0
                    || tb.string_size as u64 + 1 != item.pack_size
                    || tb.nonzero_tail
                {
                    item.header_error = true;
                }
                if lf == b'L' {
                    name_buf = tb;
                } else {
                    link_buf = tb;
                }
                continue;
            }
            if lf == b'g' || lf == b'x' || lf == b'X' {
                let start = item.header_size == RECORD;
                let Some(tb) = self.read_to_buffer(item, 1 << 26)? else {
                    return Ok(false);
                };
                item.header_size += item.pack_size_aligned();
                if tb.string_size as u64 != item.pack_size || tb.string_size == 0 || tb.nonzero_tail
                {
                    item.pax_error = true;
                }
                item.pax_records += 1;
                if lf != b'g' {
                    item.pax_record_path.clone_from(&item.name);
                    pax_buf = tb;
                    continue;
                }
                if self.pax_global.is_some() {
                    self.flags.pax_global_error = true;
                }
                match parse_pax(&tb.data[..tb.string_size], false) {
                    Some(info) => self.pax_global = Some((item.name.clone(), info.unknown)),
                    None => self.flags.pax_global_error = true,
                }
                if start && item.pax_records == 1 && extra_records == 1 {
                    item.header_pos += item.header_size;
                    item.header_size = 0;
                    item.pax_records = 0;
                    extra_records = 0;
                } else {
                    self.flags.pax_global_error = true;
                }
                continue;
            }
            break;
        }
        if item.long_name && name_buf.string_size != 0 {
            item.name = name_buf.data[..name_buf.string_size].to_vec();
        }
        if item.long_link {
            item.link_name = link_buf
                .data
                .get(..link_buf.string_size)
                .unwrap_or_default()
                .to_vec();
        }
        self.fault = None;
        if pax_buf.string_size != 0 {
            match parse_pax(&pax_buf.data[..pax_buf.string_size], true) {
                None => item.pax_error = true,
                Some(info) => {
                    if let Some(path) = info.path {
                        item.name = path;
                        item.pax_path = true;
                    }
                    if let Some(link) = info.link {
                        item.link_name = link;
                        item.pax_link = true;
                    }
                    if let Some(user) = info.user {
                        item.user = user;
                    }
                    if let Some(group) = info.group {
                        item.group = group;
                    }
                    if let Some(fflags) = info.fflags {
                        item.schily_fflags = fflags;
                    }
                    if let Some(uid) = info.uid {
                        item.uid = uid;
                    }
                    if let Some(gid) = info.gid {
                        item.gid = gid;
                    }
                    if let Some(size) = info.size {
                        if item.size != 0 && item.size != size {
                            item.pax_error = true;
                        }
                        if size >= 1 << 63 {
                            item.pax_error = true;
                        } else {
                            item.size = size;
                            item.pack_size = size;
                            item.pax_size = true;
                        }
                    }
                    item.mtime_pax = info.mtime;
                    item.atime_pax = info.atime;
                    item.ctime_pax = info.ctime;
                    item.pax_raw_lines = info.unknown;
                    if info.overflow {
                        item.pax_overflow = true;
                    }
                    if info.tag_error || info.double_tag {
                        item.pax_error = true;
                    }
                }
            }
        }
        Ok(true)
    }
}

/// A pax header's records (`CPaxInfo`).
#[derive(Default)]
struct PaxInfo {
    path: Option<Vec<u8>>,
    link: Option<Vec<u8>>,
    user: Option<Vec<u8>>,
    group: Option<Vec<u8>>,
    fflags: Option<Vec<u8>>,
    uid: Option<u32>,
    gid: Option<u32>,
    size: Option<u64>,
    mtime: Option<PaxTime>,
    atime: Option<PaxTime>,
    ctime: Option<PaxTime>,
    unknown: Vec<u8>,
    overflow: bool,
    tag_error: bool,
    double_tag: bool,
}

/// `ParsePaxTime`: seconds, a dot and up to nine digits kept.
fn parse_pax_time(value: &[u8], slot: &mut Option<PaxTime>, double: &mut bool) -> bool {
    if slot.is_some() {
        *double = true;
    }
    *slot = None;
    let (negative, digits) = match value.strip_prefix(b"-") {
        Some(rest) => (true, rest),
        None => (false, value),
    };
    let int_len = digits.iter().take_while(|b| b.is_ascii_digit()).count();
    if int_len == 0 {
        return false;
    }
    let Ok(sec) = std::str::from_utf8(&digits[..int_len])
        .unwrap_or_default()
        .parse::<i64>()
    else {
        return false;
    };
    let mut sec = if negative { -sec } else { sec };
    let rest = &digits[int_len..];
    if rest.is_empty() {
        *slot = Some(PaxTime {
            sec,
            ns: 0,
            digits: Some(0),
        });
        return true;
    }
    let Some(fraction) = rest.strip_prefix(b".") else {
        return false;
    };
    if !fraction.iter().all(u8::is_ascii_digit) {
        return false;
    }
    let mut ns = 0u32;
    for (i, &b) in fraction.iter().enumerate() {
        if i < 9 {
            ns = ns * 10 + u32::from(b - b'0');
        }
    }
    let kept = fraction.len().min(9);
    for _ in kept..9 {
        ns *= 10;
    }
    if negative && ns != 0 {
        sec -= 1;
        ns = 1_000_000_000 - ns;
    }
    *slot = Some(PaxTime {
        sec,
        ns,
        digits: Some(kept),
    });
    true
}

/// `CPaxInfo::ParsePax`: records of `length key=value\n`; for an item's own header, the
/// keys 7-Zip knows, the others kept as lines.
#[expect(clippy::too_many_lines, reason = "7-Zip's ParsePax, key by key")]
fn parse_pax(data: &[u8], is_file: bool) -> Option<PaxInfo> {
    let mut info = PaxInfo::default();
    let mut rest = data;
    while !rest.is_empty() {
        let space = rest.iter().take(25).position(|&b| b == b' ')?;
        if space == 0 {
            return None;
        }
        let size: usize = std::str::from_utf8(&rest[..space]).ok()?.parse().ok()?;
        let offset = space + 1;
        if size > rest.len() || size <= offset + 1 || rest[size - 1] != b'\n' {
            return None;
        }
        let record = &rest[offset..size];
        if record.contains(&0) {
            return None;
        }
        let eq = record[..record.len() - 1].iter().position(|&b| b == b'=')?;
        let key = &record[..eq];
        let value = &record[eq + 1..record.len() - 1];
        let mut parsed = false;
        if is_file {
            let known = true;
            let set_text = |slot: &mut Option<Vec<u8>>, double: &mut bool| {
                if slot.is_some() {
                    *double = true;
                }
                *slot = Some(value.to_vec());
                true
            };
            let id = |slot: &mut Option<u32>, double: &mut bool| -> bool {
                if slot.is_some() {
                    *double = true;
                }
                let text = std::str::from_utf8(value).unwrap_or_default();
                match text.parse::<u32>() {
                    Ok(v) if !text.is_empty() => {
                        *slot = Some(v);
                        true
                    }
                    _ => false,
                }
            };
            let detected = match key {
                b"path" => {
                    parsed = set_text(&mut info.path, &mut info.double_tag);
                    known
                }
                b"linkpath" => {
                    parsed = set_text(&mut info.link, &mut info.double_tag);
                    known
                }
                b"uname" => {
                    parsed = set_text(&mut info.user, &mut info.double_tag);
                    known
                }
                b"gname" => {
                    parsed = set_text(&mut info.group, &mut info.double_tag);
                    known
                }
                b"SCHILY.fflags" => {
                    parsed = set_text(&mut info.fflags, &mut info.double_tag);
                    known
                }
                b"uid" => {
                    parsed = id(&mut info.uid, &mut info.double_tag);
                    known
                }
                b"gid" => {
                    parsed = id(&mut info.gid, &mut info.double_tag);
                    known
                }
                b"size" => {
                    if info.size.is_some() {
                        info.double_tag = true;
                    }
                    info.size = std::str::from_utf8(value)
                        .ok()
                        .filter(|t| !t.is_empty())
                        .and_then(|t| t.parse().ok());
                    parsed = info.size.is_some();
                    known
                }
                b"mtime" => {
                    parsed = parse_pax_time(value, &mut info.mtime, &mut info.double_tag);
                    known
                }
                b"atime" => {
                    parsed = parse_pax_time(value, &mut info.atime, &mut info.double_tag);
                    known
                }
                b"ctime" => {
                    parsed = parse_pax_time(value, &mut info.ctime, &mut info.double_tag);
                    known
                }
                _ => false,
            };
            if detected && !parsed {
                info.tag_error = true;
            }
        }
        if !parsed && !info.overflow {
            let add = &rest[offset..size];
            if info.unknown.len() + add.len() < 1 << 16 {
                info.unknown.extend_from_slice(add);
            } else {
                info.overflow = true;
            }
        }
        rest = &rest[size..];
    }
    Some(info)
}

/// FILETIME ticks and the nanoseconds past them, of a Unix time.
fn ticks_of(sec: i64, ns: u32) -> Option<(u64, u8)> {
    let ticks = i128::from(sec) * 10_000_000 + UNIX_EPOCH_TICKS + i128::from(ns / 100);
    let ticks = u64::try_from(ticks).ok()?;
    Some((ticks, u8::try_from(ns % 100).unwrap_or(0)))
}

/// A name as 7-Zip shows it: UTF-8, without its trailing `/`.
fn shown_name(name: &[u8]) -> String {
    let text = String::from_utf8_lossy(name);
    text.trim_end_matches('/').to_owned()
}

/// `AddSpecCharToString`: printable, else `[hh]`.
fn spec_char(c: u8, s: &mut String) {
    if c <= 0x20 || c > 127 {
        let _ = write!(s, "[{c:02x}]");
    } else {
        s.push(char::from(c));
    }
}

/// The item's `Characteristics`.
#[expect(clippy::too_many_lines, reason = "7-Zip's words, one by one")]
fn characteristics(item: &TarItem) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut flag = String::new();
    spec_char(item.link_flag, &mut flag);
    words.push(flag);
    if item.is_magic_gnu() {
        words.push("GNU".to_owned());
    } else if item.is_magic_posix() {
        words.push("POSIX".to_owned());
    } else {
        let mut magic = String::new();
        for &b in &item.magic {
            spec_char(b, &mut magic);
        }
        words.push(magic);
    }
    if item.signed_checksum {
        words.push("SignedChecksum".to_owned());
    }
    if item.prefix_used {
        words.push("PREFIX".to_owned());
    }
    words.push(
        if item.name.is_ascii() {
            "ASCII"
        } else {
            "UTF8"
        }
        .to_owned(),
    );
    for (used, used_2, name) in [
        (item.long_name, item.long_name_2, "LongName"),
        (item.long_link, item.long_link_2, "LongLink"),
    ] {
        if used {
            words.push(if used_2 {
                format!("{name}*")
            } else {
                name.to_owned()
            });
        }
    }
    if item.mtime_is_bin {
        words.push("bin_mtime".to_owned());
    }
    if item.pack_size_is_bin {
        words.push("bin_psize".to_owned());
    }
    if item.size_is_bin {
        words.push("bin_size".to_owned());
    }
    match item.pax_records {
        0 => {}
        1 => words.push("PAX".to_owned()),
        n => words.push(format!("PAX:{n}")),
    }
    if item.mtime_pax.is_some() {
        words.push("mtime".to_owned());
    }
    if item.atime_pax.is_some() {
        words.push("atime".to_owned());
    }
    if item.ctime_pax.is_some() {
        words.push("ctime".to_owned());
    }
    if item.pax_path {
        words.push("pax_path".to_owned());
    }
    if item.pax_link {
        words.push("pax_linkpath".to_owned());
    }
    if item.pax_size {
        words.push("pax_size".to_owned());
    }
    if !item.schily_fflags.is_empty() {
        words.push(format!(
            "SCHILY.fflags={}",
            String::from_utf8_lossy(&item.schily_fflags)
        ));
    }
    if item.is_sparse() {
        words.push("SPARSE".to_owned());
    }
    if item.is_warning() {
        words.push("WARNING".to_owned());
    }
    if item.header_error {
        words.push("ERROR".to_owned());
    }
    if item.method_error {
        words.push("METHOD_ERROR".to_owned());
    }
    if item.pax_error {
        words.push("PAX_error".to_owned());
    }
    if !item.pax_raw_lines.is_empty() {
        words.push("PAX_unsupported_line".to_owned());
    }
    if item.pax_overflow {
        words.push("PAX_overflow".to_owned());
    }
    words.join(" ")
}

/// `CPaxExtra::Print_To_String`, as a listing shows it: a line break becomes `_`.
fn comment(path: &[u8], raw: &[u8]) -> String {
    let mut s = Vec::new();
    if !path.is_empty() {
        s.extend_from_slice(path);
        s.push(b'\n');
    }
    s.extend_from_slice(raw);
    String::from_utf8_lossy(&s).replace(['\n', '\r', '\t'], "_")
}

/// The properties a technical listing shows for a tar item, in 7-Zip's order.
pub(super) const ITEM_PROPS: [Prop; 18] = [
    Prop::Path,
    Prop::Folder,
    Prop::Size,
    Prop::PackedSize,
    Prop::Modified,
    Prop::Created,
    Prop::Accessed,
    Prop::Mode,
    Prop::User,
    Prop::Group,
    Prop::UserId,
    Prop::GroupId,
    Prop::SymLink,
    Prop::HardLink,
    Prop::Characteristics,
    Prop::Comment,
    Prop::DeviceMajor,
    Prop::DeviceMinor,
];

/// Opens `path` as a tar (`CHandler::Open2`); `None` when it is not one.
#[expect(
    clippy::too_many_lines,
    reason = "7-Zip's Open2 and the archive's facts"
)]
/// `name` is the archive's name, which a tar without items needs to open.
pub(super) fn open(location: &Location, name: &Path) -> io::Result<Option<Opening>> {
    let mut reader = Reader {
        file: location.open()?,
        phy_size: 0,
        headers_size: 0,
        fault: None,
        flags: Flags {
            ascii: true,
            ..Flags::default()
        },
        pax_global: None,
    };
    let end = reader.file.seek(SeekFrom::End(0))?;
    reader.file.seek(SeekFrom::Start(0))?;
    let mut items: Vec<TarItem> = Vec::new();
    let mut last_fault = None;
    loop {
        let mut item = TarItem {
            header_pos: reader.phy_size,
            ..TarItem::default()
        };
        let filled = reader.read_item(&mut item)?;
        if let Some(fault) = reader.fault {
            last_fault = Some(fault);
        }
        reader.headers_size += item.header_size;
        reader.phy_size = item.header_pos + item.header_size;
        if !filled {
            break;
        }
        let flags = &mut reader.flags;
        if item.is_magic_gnu() {
            flags.gnu = true;
        } else if item.is_magic_posix() {
            flags.posix = true;
        }
        flags.pax |= item.pax_records != 0;
        flags.mtime |= item.mtime_pax.is_some();
        flags.atime |= item.atime_pax.is_some();
        flags.ctime |= item.ctime_pax.is_some();
        flags.schily |= !item.schily_fflags.is_empty();
        flags.pax_path |= item.pax_path;
        flags.pax_link |= item.pax_link;
        flags.long_name |= item.long_name;
        flags.long_link |= item.long_link;
        flags.prefix |= item.prefix_used;
        flags.pax_items |= matches!(item.link_flag, b'x' | b'X' | b'g');
        flags.warning |= item.is_warning() || item.header_error || item.pax_error;
        flags.ascii &= item.name.is_ascii();
        reader.phy_size += item.pack_size_aligned();
        reader.file.seek(SeekFrom::Start(reader.phy_size))?;
        items.push(item);
        if reader.phy_size > end {
            last_fault = Some(Fault::UnexpectedEnd);
            break;
        }
    }
    if items.is_empty() {
        if last_fault.is_some() {
            return Ok(None);
        }
        if !name
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("tar"))
        {
            return Ok(None);
        }
    }
    let flags = &reader.flags;
    let mut characts: Vec<&str> = Vec::new();
    for (on, word) in [
        (flags.gnu, "GNU"),
        (flags.posix, "POSIX"),
        (flags.pax_items, "PAX_ITEM"),
        (flags.prefix, "PREFIX"),
        (flags.long_name, "LongName"),
        (flags.long_link, "LongLink"),
        (flags.pax, "PAX"),
        (flags.pax_path, "path"),
        (flags.pax_link, "linkpath"),
        (flags.mtime, "mtime"),
        (flags.atime, "atime"),
        (flags.ctime, "ctime"),
        (flags.schily, "SCHILY.fflags"),
        (flags.pax_global_error, "PAX_GLOBAL_ERROR"),
    ] {
        if on {
            characts.push(word);
        }
    }
    characts.push(if flags.ascii { "ASCII" } else { "UTF8" });
    let mut props = vec![
        ("Headers Size", reader.headers_size.to_string()),
        ("Code Page", "UTF-8".to_owned()),
        ("Characteristics", characts.join(" ")),
    ];
    if let Some((name, raw)) = &reader.pax_global {
        let text = comment(name, raw);
        if !text.is_empty() {
            props.push(("Comment", text));
        }
    }
    let error_flags = match last_fault {
        Some(Fault::UnexpectedEnd) => vec!["Unexpected end of archive"],
        Some(Fault::Corrupted) => vec!["Headers Error"],
        None => Vec::new(),
    };
    let warning_flags = if flags.warning {
        vec!["Headers Error"]
    } else {
        Vec::new()
    };
    let listed = items.iter().map(listed_item).collect();
    Ok(Some(Opening {
        physical_size: reader.phy_size,
        props,
        items: listed,
        error_flags,
        warning_flags,
        tar: Tar {
            location: location.clone(),
            items,
            pax_items: flags.pax_items,
            faulty: last_fault.is_some() || flags.warning,
        },
    }))
}

/// A tar item as the listing shows it.
fn listed_item(item: &TarItem) -> Item {
    let mut listed = Item {
        path: shown_name(&item.name),
        is_dir: item.is_dir(),
        size: Some(item.unpack_size()),
        packed: Some(item.pack_size_aligned()),
        ..Item::default()
    };
    if let Some(pt) = item.mtime_pax {
        if let Some((ticks, extra)) = ticks_of(pt.sec, pt.ns) {
            listed.modified = Some(ticks);
            listed.time_digits[0] = pt.digits.unwrap_or(0);
            listed.time_extra[0] = extra;
            listed.mtime_prec =
                TimePrec::Digits(u32::try_from(pt.digits.unwrap_or(0)).unwrap_or(0));
        }
    } else if let Some((ticks, _)) = ticks_of(item.mtime, 0) {
        listed.modified = Some(ticks);
        listed.mtime_prec = TimePrec::Unix;
    }
    for (slot, pax) in [(1usize, item.ctime_pax), (2, item.atime_pax)] {
        if let Some(pt) = pax
            && let Some((ticks, extra)) = ticks_of(pt.sec, pt.ns)
        {
            if slot == 1 {
                listed.created = Some(ticks);
            } else {
                listed.accessed = Some(ticks);
            }
            listed.time_digits[slot] = pt.digits.unwrap_or(0);
            listed.time_extra[slot] = extra;
        }
    }
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    listed.extra = vec![
        (
            Prop::Folder,
            if item.is_dir() { "+" } else { "-" }.to_owned(),
        ),
        (
            Prop::Mode,
            super::text::posix_mode_string(item.combined_mode()),
        ),
        (Prop::User, text(&item.user)),
        (Prop::Group, text(&item.group)),
        (Prop::UserId, item.uid.to_string()),
        (Prop::GroupId, item.gid.to_string()),
        (
            Prop::SymLink,
            if item.is_symlink() {
                text(&item.link_name)
            } else {
                String::new()
            },
        ),
        (
            Prop::HardLink,
            if item.is_hardlink() {
                text(&item.link_name)
            } else {
                String::new()
            },
        ),
        (Prop::Characteristics, characteristics(item)),
        (
            Prop::Comment,
            comment(&item.pax_record_path, &item.pax_raw_lines),
        ),
        (
            Prop::DeviceMajor,
            item.dev_major.map(|v| v.to_string()).unwrap_or_default(),
        ),
        (
            Prop::DeviceMinor,
            item.dev_minor.map(|v| v.to_string()).unwrap_or_default(),
        ),
    ];
    if (item.is_symlink() || item.is_hardlink()) && !item.link_name.is_empty() {
        listed.link = Some(super::archive::Link {
            hard: item.is_hardlink(),
            relative: !item.is_hardlink(),
            junction: false,
            path: text(&item.link_name),
        });
    }
    listed
}

impl Tar {
    /// Item `index`'s data: its bytes, its link's name for a symbolic link, its blocks
    /// and holes for a sparse file.
    pub(super) fn data(&self, index: usize) -> io::Result<TarData> {
        let item = &self.items[index];
        let source: Box<dyn Read> = if item.is_symlink() {
            Box::new(io::Cursor::new(item.link_name.clone()))
        } else if item.is_dir() {
            Box::new(io::empty())
        } else {
            let mut file = self.location.open()?;
            file.seek(SeekFrom::Start(item.data_pos()))?;
            let stored = file.take(item.pack_size_aligned());
            if item.is_sparse() {
                Box::new(Sparse::new(stored, item.sparse.clone(), item.size))
            } else {
                Box::new(stored)
            }
        };
        Ok(TarData {
            source,
            remaining: if item.is_dir() { 0 } else { item.unpack_size() },
            short: false,
        })
    }
}

/// A sparse file's data: its stored blocks at their offsets, zeros between.
struct Sparse<R> {
    inner: R,
    blocks: Vec<(u64, u64)>,
    size: u64,
    pos: u64,
}

impl<R: Read> Sparse<R> {
    const fn new(inner: R, blocks: Vec<(u64, u64)>, size: u64) -> Self {
        Self {
            inner,
            blocks,
            size,
            pos: 0,
        }
    }
}

impl<R: Read> Read for Sparse<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.size || buf.is_empty() {
            return Ok(0);
        }
        let want = usize::try_from((self.size - self.pos).min(buf.len() as u64)).unwrap_or(0);
        let block = self
            .blocks
            .iter()
            .find(|&&(offset, size)| self.pos >= offset && self.pos < offset + size);
        let n = if let Some(&(offset, size)) = block {
            let room = usize::try_from(offset + size - self.pos)
                .unwrap_or(usize::MAX)
                .min(want);
            self.inner.read(&mut buf[..room])?
        } else {
            let next = self
                .blocks
                .iter()
                .map(|&(offset, _)| offset)
                .filter(|&offset| offset > self.pos)
                .min()
                .unwrap_or(self.size);
            let zeros = usize::try_from(next - self.pos)
                .unwrap_or(usize::MAX)
                .min(want);
            buf[..zeros].fill(0);
            zeros
        };
        self.pos += n as u64;
        Ok(n)
    }
}

/// A tar item's data for extraction: what is stored, no more than its size; less is a
/// data error (`CLimitedSequentialOutStream`'s remainder).
pub(super) struct TarData {
    source: Box<dyn Read>,
    remaining: u64,
    short: bool,
}

impl Read for TarData {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let room = usize::try_from(self.remaining.min(buf.len() as u64)).unwrap_or(0);
        let n = self.source.read(&mut buf[..room])?;
        if n == 0 {
            self.short = true;
        }
        self.remaining -= n as u64;
        Ok(n)
    }
}

impl Data for TarData {
    fn finish(&mut self) -> Result<(), Problem> {
        let _ = io::copy(self, &mut io::sink());
        if self.short || self.remaining != 0 {
            return Err(Problem::Data);
        }
        Ok(())
    }

    fn encrypted(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_as_7_zip_reads_them() {
        assert_eq!(octal(b"0000644\0", false), Some(0o644));
        assert_eq!(octal(b"   644 \0", false), Some(0o644));
        assert_eq!(octal(b"\0\0\0\0", true), Some(0));
        assert_eq!(octal(b"\0\0\0\0", false), None);
        assert_eq!(octal(b"64x", false), None);
    }

    #[test]
    fn pax_times_keep_their_digits() {
        let mut slot = None;
        let mut double = false;
        assert!(parse_pax_time(b"1791360000.534595", &mut slot, &mut double));
        let pt = slot.unwrap();
        assert_eq!(
            (pt.sec, pt.ns, pt.digits),
            (1_791_360_000, 534_595_000, Some(6))
        );
        assert!(!parse_pax_time(b"12x", &mut slot, &mut double));
        assert!(double);
    }

    #[test]
    fn pax_records_parse_and_keep_the_unknown() {
        let info = parse_pax(b"21 path=dir/name.txt\n14 foo=barbaz\n", true).unwrap();
        assert_eq!(info.path.as_deref(), Some(&b"dir/name.txt"[..]));
        assert_eq!(info.unknown, b"foo=barbaz\n");
        assert!(parse_pax(b"9 bad\n", true).is_none());
    }
}
