//! RAR 1.5 to 4 as 7-Zip 26.03 opens and lists them (`RarHandler.cpp`): each volume's
//! blocks, decrypted when the headers are (RAR 3's AES, a salt before each header), the
//! file headers read into items and an item split across volumes joined into one, and
//! each property as 7-Zip names and words it.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use cash_archive::rar::crypto::rar30::Rar30Cipher;

use super::super::archive::{Item, Prop, TimePrec};
use super::volname::VolumeName;

const MARKER: [u8; 7] = *b"Rar!\x1a\x07\x00";

/// The archive header's size, comment aside.
const ARCHIVE_HEADER_SIZE: usize = 13;

mod block {
    pub(super) const ARCHIVE_HEADER: u8 = 0x73;
    pub(super) const FILE_HEADER: u8 = 0x74;
    pub(super) const END_OF_ARCHIVE: u8 = 0x7B;
}

mod arc_flags {
    pub(super) const VOLUME: u32 = 1;
    pub(super) const SOLID: u32 = 8;
    pub(super) const NEW_VOL_NAME: u32 = 0x10;
    pub(super) const RECOVERY: u32 = 0x40;
    pub(super) const BLOCK_ENCRYPTION: u32 = 0x80;
}

mod end_flags {
    pub(super) const NEXT_VOL: u32 = 1;
    pub(super) const DATA_CRC: u32 = 2;
    pub(super) const REV_SPACE: u32 = 4;
    pub(super) const VOL_NUMBER: u32 = 8;
}

mod file_flags {
    pub(super) const SPLIT_BEFORE: u32 = 1;
    pub(super) const SPLIT_AFTER: u32 = 1 << 1;
    pub(super) const ENCRYPTED: u32 = 1 << 2;
    pub(super) const COMMENT: u32 = 1 << 3;
    pub(super) const SOLID: u32 = 1 << 4;
    pub(super) const SIZE64: u32 = 1 << 8;
    pub(super) const UNICODE_NAME: u32 = 1 << 9;
    pub(super) const SALT: u32 = 1 << 10;
    pub(super) const EXT_TIME: u32 = 1 << 12;
    pub(super) const LONG_BLOCK: u32 = 1 << 15;
}

const HOST_MSDOS: u8 = 0;
const HOST_OS2: u8 = 1;
const HOST_WIN32: u8 = 2;
const HOST_UNIX: u8 = 3;
const HOST_BEOS: u8 = 5;

const HOST_NAMES: [&str; 6] = ["MS DOS", "OS/2", "Win32", "Unix", "Mac OS", "BeOS"];
const ARC_FLAG_NAMES: [&str; 10] = [
    "Volume",
    "Comment",
    "Lock",
    "Solid",
    "NewVolName",
    "Authenticity",
    "Recovery",
    "BlockEncryption",
    "FirstVolume",
    "EncryptVer",
];

/// FILETIME ticks at the Unix epoch.
const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;

fn le16(p: &[u8], at: usize) -> u16 {
    p.get(at..at + 2)
        .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
}

fn le32(p: &[u8], at: usize) -> u32 {
    p.get(at..at + 4)
        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// `CheckHeaderCrc`: the low half of the CRC-32 of the header after its CRC.
fn header_crc_ok(header: &[u8]) -> bool {
    header.len() >= 2 && u32::from(le16(header, 0)) == crc32fast::hash(&header[2..]) & 0xFFFF
}

/// A time as RAR keeps it: MS-DOS's local time, an odd second, and the 100 ns ticks past.
#[derive(Clone, Copy, Debug, Default)]
struct RarTime {
    dos_time: u32,
    low_second: u8,
    sub_time: [u8; 3],
}

impl RarTime {
    /// `RarTimeToFileTime` then `LocalFileTimeToFileTime`: FILETIME ticks, in UTC.
    fn ticks(self, zone: &cash_core::timefmt::Zone) -> Option<u64> {
        let date = self.dos_time >> 16;
        let time = self.dos_time & 0xFFFF;
        let year = 1980 + i32::try_from(date >> 9).ok()?;
        let naive = chrono::NaiveDate::from_ymd_opt(year, (date >> 5) & 15, date & 31)?
            .and_hms_opt(time >> 11, (time >> 5) & 63, (time & 31) * 2)?;
        let unix = zone.local_to_unix(naive)?;
        let ticks = i128::from(unix) * 10_000_000 + i128::from(UNIX_EPOCH_TICKS);
        let extra = u64::from(self.low_second) * 10_000_000
            + (u64::from(self.sub_time[2]) << 16)
            + (u64::from(self.sub_time[1]) << 8)
            + u64::from(self.sub_time[0]);
        u64::try_from(ticks).ok()?.checked_add(extra)
    }
}

/// A file header (`CItem`).
#[derive(Clone, Debug, Default)]
pub(super) struct RarItem {
    pub(super) size: u64,
    pub(super) pack_size: u64,
    ctime: Option<RarTime>,
    atime: Option<RarTime>,
    mtime: RarTime,
    pub(super) file_crc: u32,
    pub(super) attrib: u32,
    pub(super) flags: u32,
    pub(super) host_os: u8,
    pub(super) unpack_version: u8,
    pub(super) method: u8,
    name: Vec<u8>,
    unicode_name: Option<String>,
    pub(super) salt: [u8; 8],
    /// Where the header begins, and its parts' sizes.
    pub(super) position: u64,
    pub(super) main_part_size: u64,
    pub(super) comment_size: u64,
    pub(super) align_size: u64,
    pub(super) vol_index: usize,
}

impl RarItem {
    pub(super) const fn is_encrypted(&self) -> bool {
        self.flags & file_flags::ENCRYPTED != 0
    }

    pub(super) const fn is_solid_flag(&self) -> bool {
        self.flags & file_flags::SOLID != 0
    }

    const fn is_commented(&self) -> bool {
        self.flags & file_flags::COMMENT != 0
    }

    pub(super) const fn is_split_before(&self) -> bool {
        self.flags & file_flags::SPLIT_BEFORE != 0
    }

    pub(super) const fn is_split_after(&self) -> bool {
        self.flags & file_flags::SPLIT_AFTER != 0
    }

    pub(super) const fn has_salt(&self) -> bool {
        self.flags & file_flags::SALT != 0
    }

    const fn dict_size(&self) -> u32 {
        (self.flags >> 5) & 7
    }

    pub(super) const fn is_size_defined(&self) -> bool {
        self.size != u64::MAX
    }

    /// `IsDir`: the folder's dictionary value, else the host's folder attribute.
    pub(super) const fn is_dir(&self) -> bool {
        if self.dict_size() == 7 {
            return true;
        }
        match self.host_os {
            HOST_MSDOS | HOST_OS2 | HOST_WIN32 => self.attrib & 0x10 != 0,
            HOST_UNIX | HOST_BEOS => self.attrib & 0o170_000 == 0o040_000,
            _ => false,
        }
    }

    /// `IgnoreItem`: a volume label of MS-DOS, OS/2 or Windows.
    const fn ignored(&self) -> bool {
        matches!(self.host_os, HOST_MSDOS | HOST_OS2 | HOST_WIN32) && self.attrib & 8 != 0
    }

    /// `GetWinAttrib`.
    const fn win_attrib(&self) -> u32 {
        let mut a = match self.host_os {
            HOST_MSDOS | HOST_OS2 | HOST_WIN32 => self.attrib,
            HOST_UNIX | HOST_BEOS => (self.attrib << 16) | 0x8000,
            _ => 0,
        };
        if self.is_dir() {
            a |= 0x10;
        }
        a
    }

    /// Where the data begins.
    pub(super) const fn data_pos(&self) -> u64 {
        self.position + self.main_part_size + self.comment_size + self.align_size
    }

    /// `GetName`: the Unicode name when there is one, else the name in the OEM code page.
    fn name(&self) -> String {
        if self.flags & file_flags::UNICODE_NAME != 0
            && let Some(name) = self.unicode_name.as_ref().filter(|n| !n.is_empty())
        {
            return name.clone();
        }
        self.name
            .iter()
            .map(|&b| super::super::zip7::cp437(b))
            .collect()
    }
}

/// `DecodeUnicodeFileName`: RAR's compressed UTF-16 name, after the OEM one.
fn decode_unicode_name(name: &[u8], enc: &[u8], max: usize) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();
    let mut pos = 0;
    let mut flag_bits = 0u32;
    let mut flags = 0u8;
    let Some(&high) = enc.first() else {
        return out;
    };
    let high = u16::from(high) << 8;
    pos += 1;
    while pos < enc.len() && out.len() < max {
        if flag_bits == 0 {
            flags = enc[pos];
            pos += 1;
            flag_bits = 8;
        }
        if pos >= enc.len() {
            break;
        }
        let mut len = u16::from(enc[pos]);
        pos += 1;
        flag_bits -= 2;
        let mode = (flags >> flag_bits) & 3;
        if mode == 3 {
            if len & 0x80 != 0 {
                if pos >= enc.len() {
                    break;
                }
                let correction = enc[pos];
                pos += 1;
                let mut n = (len & 0x7F) + 2;
                while n > 0 && out.len() < max {
                    let c = name.get(out.len()).copied().unwrap_or(0);
                    out.push(u16::from(c.wrapping_add(correction)) + high);
                    n -= 1;
                }
            } else {
                let mut n = len + 2;
                while n > 0 && out.len() < max {
                    out.push(u16::from(name.get(out.len()).copied().unwrap_or(0)));
                    n -= 1;
                }
            }
        } else {
            if mode == 1 {
                len += high;
            } else if mode == 2 {
                if pos >= enc.len() {
                    break;
                }
                len += u16::from(enc[pos]) << 8;
                pos += 1;
            }
            out.push(len);
        }
    }
    if out.len() >= max {
        out.truncate(max.saturating_sub(1));
    }
    out
}

/// What a volume's headers say (`CInArcInfo`).
#[derive(Clone, Debug, Default)]
pub(super) struct ArcInfo {
    flags: u32,
    start_pos: u64,
    pub(super) end_pos: u64,
    file_size: u64,
    end_flags: u32,
    vol_number: u32,
    end_read: bool,
}

impl ArcInfo {
    pub(super) const fn phy_size(&self) -> u64 {
        self.end_pos.saturating_sub(self.start_pos)
    }

    pub(super) const fn is_volume(&self) -> bool {
        self.flags & arc_flags::VOLUME != 0
    }

    pub(super) const fn is_solid(&self) -> bool {
        self.flags & arc_flags::SOLID != 0
    }

    const fn new_vol_name(&self) -> bool {
        self.flags & arc_flags::NEW_VOL_NAME != 0
    }

    const fn is_recovery(&self) -> bool {
        self.flags & arc_flags::RECOVERY != 0
    }

    const fn more_volumes(&self) -> bool {
        self.end_flags & end_flags::NEXT_VOL != 0
    }

    const fn vol_number_defined(&self) -> bool {
        self.end_flags & end_flags::VOL_NUMBER != 0
    }

    const fn data_crc_defined(&self) -> bool {
        self.end_flags & end_flags::DATA_CRC != 0
    }
}

/// What reading a header went wrong with (`EErrorType`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum HeaderError {
    Ok,
    Corrupted,
    UnexpectedEnd,
    Decryption,
}

/// One volume's headers, read as `CInArchive` reads them.
struct HeaderReader {
    file: File,
    position: u64,
    info: ArcInfo,
    /// The headers' decrypted bytes, read from after a header's salt.
    decrypted: Vec<u8>,
    crypto_pos: usize,
    crypto: bool,
    header_error_warning: bool,
}

impl HeaderReader {
    /// Reads up to `len` bytes: from the decrypted bytes in crypto mode, else the file.
    fn read(&mut self, len: usize) -> io::Result<Vec<u8>> {
        if self.crypto {
            let end = (self.crypto_pos + len).min(self.decrypted.len());
            let data = self.decrypted[self.crypto_pos.min(end)..end].to_vec();
            self.crypto_pos = end.max(self.crypto_pos);
            return Ok(data);
        }
        let mut data = Vec::with_capacity(len);
        (&mut self.file).take(len as u64).read_to_end(&mut data)?;
        Ok(data)
    }

    /// `FinishCryptoBlock`: past the padding to the next 16 bytes.
    const fn finish_crypto_block(&mut self) {
        if self.crypto {
            while self.crypto_pos & 15 != 0 {
                self.crypto_pos += 1;
                self.position += 1;
            }
        }
    }

    /// `CInArchive::Open`: the marker and the archive header.
    fn open(file: File, info: ArcInfo) -> io::Result<Option<Self>> {
        let mut reader = Self {
            file,
            position: 0,
            info,
            decrypted: Vec::new(),
            crypto_pos: 0,
            crypto: false,
            header_error_warning: false,
        };
        reader.info.file_size = reader.file.seek(SeekFrom::End(0))?;
        reader.file.seek(SeekFrom::Start(0))?;
        let marker = reader.read(MARKER.len())?;
        if marker != MARKER {
            return Ok(None);
        }
        reader.position = MARKER.len() as u64;
        let header = reader.read(ARCHIVE_HEADER_SIZE)?;
        if header.len() != ARCHIVE_HEADER_SIZE {
            return Ok(None);
        }
        reader.position += ARCHIVE_HEADER_SIZE as u64;
        let block_size = usize::from(le16(&header, 5));
        reader.info.flags = u32::from(le16(&header, 3));
        if block_size < ARCHIVE_HEADER_SIZE
            || header[2] != block::ARCHIVE_HEADER
            || !header_crc_ok(&header)
        {
            return Ok(None);
        }
        let comment = block_size - ARCHIVE_HEADER_SIZE;
        if reader.read(comment)?.len() != comment {
            return Ok(None);
        }
        reader.position += comment as u64;
        reader.info.start_pos = 0;
        Ok(Some(reader))
    }

    /// `GetNextItem`: the next file header, past the other blocks.
    #[expect(
        clippy::too_many_lines,
        reason = "7-Zip's GetNextItem, block kind by block kind"
    )]
    fn next_item(&mut self, password: Option<&str>) -> io::Result<(Option<RarItem>, HeaderError)> {
        loop {
            self.file.seek(SeekFrom::Start(self.position))?;
            self.info.end_pos = self.position;
            if !self.crypto && self.info.flags & arc_flags::BLOCK_ENCRYPTION != 0 {
                let Some(password) = password else {
                    return Ok((None, HeaderError::Decryption));
                };
                let salt = self.read(8)?;
                let Ok(salt) = <[u8; 8]>::try_from(salt.as_slice()) else {
                    return Ok((None, HeaderError::Ok));
                };
                self.position += 8;
                let Ok(mut cipher) =
                    Rar30Cipher::new(password_utf8(password).as_bytes(), Some(salt))
                else {
                    return Ok((None, HeaderError::Decryption));
                };
                let mut data = self.read(1 << 12)?;
                data.truncate(data.len() & !15);
                if cipher.decrypt_in_place(&mut data).is_err() {
                    return Ok((None, HeaderError::Decryption));
                }
                self.decrypted = data;
                self.crypto = true;
                self.crypto_pos = 0;
            }
            let head = self.read(7)?;
            if head.len() != 7 {
                let error = if head.is_empty() {
                    HeaderError::Ok
                } else {
                    HeaderError::UnexpectedEnd
                };
                self.info.end_pos = self.position + head.len() as u64;
                return Ok((None, error));
            }
            let kind = head[2];
            let flags = u32::from(le16(&head, 3));
            let head_size = usize::from(le16(&head, 5));
            if head_size < 7 {
                return Ok((None, HeaderError::Corrupted));
            }
            let bad_block = if self.crypto {
                HeaderError::Decryption
            } else {
                HeaderError::Corrupted
            };
            if !(block::FILE_HEADER..=block::END_OF_ARCHIVE).contains(&kind) {
                return Ok((None, bad_block));
            }
            if kind == block::END_OF_ARCHIVE {
                let mut header = head;
                let mut expect = 7;
                if flags & end_flags::DATA_CRC != 0 {
                    expect += 4;
                }
                if flags & end_flags::VOL_NUMBER != 0 {
                    expect += 2;
                }
                if flags & end_flags::REV_SPACE != 0 {
                    expect += 7;
                }
                if head_size < expect {
                    self.header_error_warning = true;
                }
                let mut footer_error = false;
                if head_size > 7 {
                    if head_size > 1 << 8 {
                        footer_error = true;
                    } else {
                        let after = self.read(head_size - 7)?;
                        if after.len() == head_size - 7 {
                            header.extend_from_slice(&after);
                        } else if self.crypto {
                            footer_error = true;
                        } else {
                            return Ok((None, HeaderError::UnexpectedEnd));
                        }
                    }
                }
                let crc_ok = header_crc_ok(&header[..head_size.min(header.len())]);
                let mut error = if footer_error || !crc_ok {
                    bad_block
                } else {
                    HeaderError::Ok
                };
                if !footer_error && crc_ok {
                    self.info.end_flags = flags;
                    let mut offset = 7;
                    if flags & end_flags::DATA_CRC != 0 {
                        if header.len() < offset + 4 {
                            error = HeaderError::Corrupted;
                        }
                        offset += 4;
                    }
                    if flags & end_flags::VOL_NUMBER != 0 {
                        if header.len() < offset + 2 {
                            error = HeaderError::Corrupted;
                        } else {
                            self.info.vol_number = u32::from(le16(&header, offset));
                        }
                    }
                    self.info.end_read = true;
                }
                self.position += header.len() as u64;
                self.finish_crypto_block();
                self.info.end_pos = self.position;
                return Ok((None, error));
            }
            if kind == block::FILE_HEADER {
                let rest = self.read(head_size - 7)?;
                if rest.len() != head_size - 7 {
                    return Ok((None, HeaderError::UnexpectedEnd));
                }
                let mut header = head;
                header.extend_from_slice(&rest);
                let result = self.read_file_header(&header, flags, head_size);
                if let Some(item) = &result {
                    let main = usize::try_from(item.main_part_size).unwrap_or(header.len());
                    if !header_crc_ok(&header[..main.min(header.len())]) {
                        return Ok((None, HeaderError::Corrupted));
                    }
                }
                self.finish_crypto_block();
                self.crypto = false;
                if let Some(item) = &result {
                    self.position = self.position.saturating_add(item.pack_size);
                }
                return Ok((result, HeaderError::Ok));
            }
            if self.crypto && head_size > 1 << 10 {
                return Ok((None, HeaderError::Decryption));
            }
            if flags & file_flags::LONG_BLOCK != 0 {
                let size = self.read(4)?;
                if size.len() != 4 {
                    return Ok((None, HeaderError::UnexpectedEnd));
                }
                let data_size = le32(&size, 0);
                self.position += u64::from(data_size);
                if self.crypto && data_size > 1 << 27 {
                    return Ok((None, HeaderError::Decryption));
                }
                self.crypto_pos = head_size;
            } else {
                self.crypto_pos = 0;
            }
            if self.position + head_size as u64 > self.info.file_size {
                return Ok((None, HeaderError::UnexpectedEnd));
            }
            self.position += head_size as u64;
            self.finish_crypto_block();
            self.crypto = false;
        }
    }

    /// `ReadHeaderReal`: the file header's fields after its first seven bytes.
    fn read_file_header(&mut self, header: &[u8], flags: u32, head_size: usize) -> Option<RarItem> {
        let mut p = header.get(7..)?;
        let mut item = RarItem {
            flags,
            ..RarItem::default()
        };
        if p.len() < 25 {
            return None;
        }
        item.pack_size = u64::from(le32(p, 0));
        item.size = u64::from(le32(p, 4));
        item.host_os = p[8];
        item.file_crc = le32(p, 9);
        item.mtime.dos_time = le32(p, 13);
        item.unpack_version = p[17];
        item.method = p[18];
        let name_size = usize::from(le16(p, 19));
        item.attrib = le32(p, 21);
        p = &p[25..];
        if flags & file_flags::SIZE64 != 0 {
            if p.len() < 8 {
                return None;
            }
            item.pack_size |= u64::from(le32(p, 0)) << 32;
            if item.pack_size >= 1 << 63 {
                return None;
            }
            item.size |= u64::from(le32(p, 4)) << 32;
            p = &p[8..];
        }
        if name_size > p.len() {
            return None;
        }
        let name = &p[..name_size];
        let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
        item.name = name[..end].to_vec();
        if flags & file_flags::UNICODE_NAME != 0 {
            if end < name_size {
                let max = name_size.min(0x400);
                let units = decode_unicode_name(name, &name[end + 1..], max);
                item.unicode_name = Some(String::from_utf16_lossy(&units));
            } else {
                item.unicode_name = String::from_utf8(item.name.clone()).ok();
            }
        }
        p = &p[name_size..];
        if item.has_salt() {
            if p.len() < 8 {
                return None;
            }
            item.salt.copy_from_slice(&p[..8]);
            p = &p[8..];
        }
        // Some archives have the flag without the field.
        if p.len() >= 2 && flags & file_flags::EXT_TIME != 0 {
            let a_mask = p[0] >> 4;
            let b = p[1];
            p = &p[2..];
            let m_mask = b >> 4;
            let c_mask = b & 15;
            if m_mask & 8 != 0 {
                p = read_time(p, m_mask, &mut item.mtime)?;
            }
            if c_mask & 8 != 0 {
                let (time, rest) = read_time_2(p, c_mask)?;
                item.ctime = Some(time);
                p = rest;
            }
            if a_mask & 8 != 0 {
                let (time, rest) = read_time_2(p, a_mask)?;
                item.atime = Some(time);
                p = rest;
            }
        }
        let main = 7 + (header.len() - 7 - p.len());
        item.position = self.position;
        item.main_part_size = main as u64;
        item.comment_size = head_size.saturating_sub(main) as u64;
        item.align_size = if self.crypto {
            ((16 - (head_size & 15)) & 15) as u64
        } else {
            0
        };
        self.position += head_size as u64;
        Some(item)
    }
}

/// `ReadTime`: an odd second and up to three bytes of 100 ns ticks.
fn read_time<'a>(p: &'a [u8], mask: u8, time: &mut RarTime) -> Option<&'a [u8]> {
    time.low_second = u8::from(mask & 4 != 0);
    let digits = usize::from(mask & 3);
    time.sub_time = [0; 3];
    if digits > p.len() {
        return None;
    }
    time.sub_time[3 - digits..].copy_from_slice(&p[..digits]);
    Some(&p[digits..])
}

/// A time of its own: its MS-DOS time, then as `read_time`.
fn read_time_2(p: &[u8], mask: u8) -> Option<(RarTime, &[u8])> {
    if p.len() < 4 {
        return None;
    }
    let mut time = RarTime {
        dos_time: le32(p, 0),
        ..RarTime::default()
    };
    let rest = read_time(&p[4..], mask, &mut time)?;
    Some((time, rest))
}

/// The password as given, its first 127 UTF-16 units: what RAR 3's key is made from.
pub(super) fn password_utf8(password: &str) -> String {
    let mut units: Vec<u16> = password.encode_utf16().collect();
    units.truncate(127);
    String::from_utf16_lossy(&units)
}

/// An item as listed: its first header and how many follow it in later volumes.
#[derive(Clone, Copy, Debug)]
pub(super) struct Ref {
    pub(super) volume: usize,
    pub(super) item: usize,
    pub(super) count: usize,
}

/// A RAR 1.5 to 4 archive, open.
pub(in crate::sevenzip) struct Rar4 {
    pub(super) volumes: Vec<PathBuf>,
    pub(super) phy_sizes: Vec<u64>,
    pub(super) items: Vec<RarItem>,
    pub(super) refs: Vec<Ref>,
    pub(super) info: ArcInfo,
    unexpected_end: bool,
    headers_error: bool,
    encrypted_headers_error: bool,
    header_warning: bool,
    pub(super) missing_volume: Option<String>,
}

/// Why a RAR 1.5 to 4 archive did not open.
pub(super) enum Failure {
    NotArchive,
    PasswordNeeded,
    WrongPassword,
}

impl Rar4 {
    /// `Open2`: the volumes from the one named on, their items read.
    #[expect(
        clippy::too_many_lines,
        reason = "7-Zip's Open2, volume by volume and item by item"
    )]
    pub(super) fn open(path: &Path, password: Option<&str>) -> io::Result<Result<Self, Failure>> {
        let mut rar = Self {
            volumes: Vec::new(),
            phy_sizes: Vec::new(),
            items: Vec::new(),
            refs: Vec::new(),
            info: ArcInfo::default(),
            unexpected_end: false,
            headers_error: false,
            encrypted_headers_error: false,
            header_warning: false,
            missing_volume: None,
        };
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let base = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut names: Option<VolumeName> = None;
        let mut next_required = false;
        // One `CInArchive` reads every volume: what an end header said stays until the
        // next one says otherwise.
        let mut carried = ArcInfo::default();
        loop {
            let volume = if rar.volumes.is_empty() {
                path.to_path_buf()
            } else {
                if rar.volumes.len() == 1 && !rar.info.is_volume() {
                    break;
                }
                let new_style = rar.info.new_vol_name();
                let names = names.get_or_insert_with(|| VolumeName::new(&base, new_style));
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
            let Some(mut reader) = HeaderReader::open(file, carried.clone())? else {
                return Ok(Err(Failure::NotArchive));
            };
            let end = reader.info.file_size;
            let vol_index = rar.volumes.len();
            loop {
                if reader.position > end {
                    rar.unexpected_end = true;
                    break;
                }
                let (item, error) = reader.next_item(password)?;
                match error {
                    HeaderError::Ok => {}
                    HeaderError::UnexpectedEnd => rar.unexpected_end = true,
                    HeaderError::Corrupted => rar.headers_error = true,
                    HeaderError::Decryption => rar.encrypted_headers_error = true,
                }
                let Some(mut item) = item else {
                    if error == HeaderError::Decryption && rar.items.is_empty() {
                        return Ok(Err(if password.is_none() {
                            Failure::PasswordNeeded
                        } else {
                            Failure::WrongPassword
                        }));
                    }
                    if reader.info.is_volume() && reader.info.is_recovery() && reader.info.end_read
                    {
                        // RAR pads a multivolume archive with a recovery record with zeros.
                        reader.info.end_pos +=
                            zero_tail(&mut reader.file, reader.info.end_pos, end)?;
                    }
                    break;
                };
                if item.ignored() {
                    continue;
                }
                item.vol_index = vol_index;
                let joined = item.is_split_before()
                    && rar.refs.last_mut().is_some_and(|r| {
                        r.count += 1;
                        true
                    });
                if !joined {
                    rar.refs.push(Ref {
                        volume: vol_index,
                        item: rar.items.len(),
                        count: 1,
                    });
                }
                rar.items.push(item);
            }
            if reader.header_error_warning {
                rar.header_warning = true;
            }
            if rar.volumes.is_empty() {
                rar.info = reader.info.clone();
            }
            rar.phy_sizes.push(reader.info.phy_size());
            rar.volumes.push(volume);
            next_required = false;
            carried = reader.info.clone();
            if !reader.info.is_volume() {
                break;
            }
            if reader.info.end_read {
                if !reader.info.more_volumes() {
                    break;
                }
                next_required = true;
            }
        }
        Ok(Ok(rar))
    }

    /// `IsSolid`: an item of RAR 1.5 is solid when the archive is and it is not the first.
    pub(super) fn is_solid(&self, index: usize) -> bool {
        let item = &self.items[self.refs[index].item];
        if item.unpack_version < 20 {
            return self.info.is_solid() && index > 0;
        }
        item.is_solid_flag()
    }

    pub(super) fn pack_size(&self, r: &Ref) -> u64 {
        self.items[r.item..r.item + r.count]
            .iter()
            .fold(0u64, |sum, item| sum.wrapping_add(item.pack_size))
    }

    /// `kpidErrorFlags`.
    pub(super) fn error_flags(&self) -> Vec<&'static str> {
        let mut flags = Vec::new();
        if self.headers_error {
            flags.push("Headers Error");
        }
        if self.encrypted_headers_error {
            flags.push("Headers Error in encrypted archive. Wrong password?");
        }
        if self.unexpected_end {
            flags.push("Unexpected end of archive");
        }
        flags
    }

    /// `kpidWarningFlags`.
    pub(super) fn warning_flags(&self) -> Vec<&'static str> {
        if self.header_warning {
            vec!["Headers Error"]
        } else {
            Vec::new()
        }
    }

    /// `GetArchiveProperty`, in `kArcProps`' order; the empty ones left out.
    pub(super) fn archive_props(&self) -> Vec<(&'static str, String)> {
        let plus = |b: bool| if b { "+" } else { "-" }.to_owned();
        let mut props = Vec::new();
        if self.volumes.len() > 1 {
            props.push((
                "Total Physical Size",
                self.phy_sizes.iter().sum::<u64>().to_string(),
            ));
        }
        let mut words: Vec<String> = Vec::new();
        for (bit, name) in ARC_FLAG_NAMES.iter().enumerate() {
            if self.info.flags & (1 << bit) != 0 {
                words.push((*name).to_owned());
            }
        }
        for bit in ARC_FLAG_NAMES.len()..32 {
            let flag = 1u32 << bit;
            if self.info.flags & flag != 0 {
                words.push(format!("0x{flag:X}"));
            }
        }
        if self.info.data_crc_defined() {
            words.push("VolCRC".to_owned());
        }
        if !words.is_empty() {
            props.push(("Characteristics", words.join(" ")));
        }
        props.push(("Solid", plus(self.info.is_solid())));
        let blocks = (0..self.refs.len()).filter(|&i| !self.is_solid(i)).count();
        props.push(("Blocks", blocks.to_string()));
        props.push(("Multivolume", plus(self.info.is_volume())));
        if self.info.vol_number_defined() {
            props.push(("Volume Index", self.info.vol_number.to_string()));
        }
        props.push(("Volumes", self.volumes.len().to_string()));
        props
    }

    /// Each listed item, with its properties as 7-Zip gives them.
    pub(super) fn listed(&self, zone: &cash_core::timefmt::Zone) -> Vec<Item> {
        // Each solid stream is a block, for 7z's choice of what to decode and what to ask
        // a password for.
        let mut block = 0u64;
        self.refs
            .iter()
            .enumerate()
            .map(|(index, r)| {
                let mut item = self.listed_item(index, r, zone);
                if !self.is_solid(index) {
                    block += 1;
                }
                item.block = Some(block);
                item
            })
            .collect()
    }

    /// Whether extracting the item asks for a password, as `Extract` does: RAR 2.0's
    /// and 3's encryption, not RAR 1.5's.
    pub(super) fn needs_password(&self, index: usize) -> bool {
        let item = &self.items[self.refs[index].item];
        item.is_encrypted() && !item.is_dir() && item.unpack_version >= 20
    }

    fn listed_item(&self, index: usize, r: &Ref, zone: &cash_core::timefmt::Zone) -> Item {
        let item = &self.items[r.item];
        let last = &self.items[r.item + r.count - 1];
        let plus = |b: bool| if b { "+" } else { "-" }.to_owned();
        let crc = if last.is_split_after() {
            item.file_crc
        } else {
            last.file_crc
        };
        let method = if (b'0'..=b'5').contains(&item.method) {
            let mut s = format!("m{}", char::from(item.method));
            if !item.is_dir() {
                let _ = write!(s, ":{}", 16 + item.dict_size());
            }
            s
        } else {
            item.method.to_string()
        };
        let vol_index = if self.info.vol_number_defined() {
            (u64::from(self.info.vol_number) + r.volume as u64).to_string()
        } else {
            String::new()
        };
        let mut listed = Item {
            path: item.name().replace('\\', "/"),
            is_dir: item.is_dir(),
            size: last.is_size_defined().then_some(last.size),
            packed: Some(self.pack_size(r)),
            modified: item.mtime.ticks(zone),
            created: item.ctime.and_then(|t| t.ticks(zone)),
            accessed: item.atime.and_then(|t| t.ticks(zone)),
            attrib: Some(item.win_attrib()),
            crc: Some(crc),
            encrypted: item.is_encrypted(),
            method: Some(method),
            host_os: Some(
                HOST_NAMES
                    .get(usize::from(item.host_os))
                    .map_or_else(|| item.host_os.to_string(), |n| (*n).to_owned()),
            ),
            time_digits: [7; 3],
            mtime_prec: TimePrec::Exact,
            ..Item::default()
        };
        listed.extra = vec![
            (Prop::Folder, plus(item.is_dir())),
            (Prop::Solid, plus(self.is_solid(index))),
            (Prop::Commented, plus(item.is_commented())),
            (Prop::SplitBefore, plus(item.is_split_before())),
            (Prop::SplitAfter, plus(last.is_split_after())),
            (Prop::Version, item.unpack_version.to_string()),
            (Prop::VolumeIndex, vol_index),
        ];
        listed
    }
}

/// The properties a technical listing shows for a RAR 1.5 to 4 item, in 7-Zip's order.
pub(super) const ITEM_PROPS: [Prop; 18] = [
    Prop::Path,
    Prop::Folder,
    Prop::Size,
    Prop::PackedSize,
    Prop::Modified,
    Prop::Created,
    Prop::Accessed,
    Prop::Attributes,
    Prop::Encrypted,
    Prop::Solid,
    Prop::Commented,
    Prop::SplitBefore,
    Prop::SplitAfter,
    Prop::Crc,
    Prop::HostOs,
    Prop::Method,
    Prop::Version,
    Prop::VolumeIndex,
];

/// `ReadZeroTail`: how many zeros follow a volume's end, up to 4 KiB, when nothing else
/// does.
fn zero_tail(file: &mut File, at: u64, end: u64) -> io::Result<u64> {
    if at >= end || end - at > 1 << 12 {
        return Ok(0);
    }
    file.seek(SeekFrom::Start(at))?;
    let mut data = Vec::new();
    file.take(end - at).read_to_end(&mut data)?;
    Ok(if data.iter().all(|&b| b == 0) {
        end - at
    } else {
        0
    })
}
