//! The zip handler's update (`ZipHandlerOut.cpp`, `ZipUpdate.cpp`, `ZipOut.cpp`,
//! `ZipAddCommon.cpp`): each new file compressed with the method `-m` names (Deflate,
//! or Store at `-mx0`), and stored when that does not make it smaller; its time in
//! MS-DOS's form in the local zone and, in the central directory, in an NTFS field; its
//! name in the OEM code page with a UTF-8 copy beside it, or in UTF-8 when the code
//! page cannot hold it; `ZipCrypto`, or AES with `-mem`. Items kept are copied whole;
//! renamed ones get a new local header over their old data.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::num::NonZeroU64;

use cash_archive::codec::{self, Codec};
use cash_archive::zip::encode::{lzma_writer, ppmd_writer, zip_crypto_writer};
use cash_archive::zip::read::{self as zread};
use cash_archive::zip::{Entry, aes};

use super::super::archive::Opened;
use super::super::extract::ask_password;
use super::super::methods::{self, MethodError, lzma_params, param, size};
use super::super::text;
use super::super::zip7::Zip;
use super::{Input, Item, Job, Out, Stop, Warnings, progress, win, win_error};

const LOCAL_SIG: u32 = 0x0403_4B50;
const CENTRAL_SIG: u32 = 0x0201_4B50;
const END_SIG: u32 = 0x0605_4B50;
const END64_SIG: u32 = 0x0606_4B50;
const END64_LOCATOR_SIG: u32 = 0x0706_4B50;
const DESCRIPTOR_SIG: u32 = 0x0807_4B50;

/// The version 7-Zip says it was made by (6.3), and the host: MS-DOS's (FAT).
const MADE_BY_VERSION: u8 = 63;
const HOST_FAT: u8 = 0;

/// The versions needed to extract (`kExtractVersion_*`).
mod version {
    pub(super) const DEFAULT: u8 = 10;
    pub(super) const DIR: u8 = 20;
    pub(super) const ZIP_CRYPTO: u8 = 20;
    pub(super) const DEFLATE: u8 = 20;
    pub(super) const DEFLATE64: u8 = 21;
    pub(super) const ZIP64: u8 = 45;
    pub(super) const BZIP2: u8 = 46;
    pub(super) const AES: u8 = 51;
    pub(super) const LZMA: u8 = 63;
    pub(super) const PPMD: u8 = 63;
    pub(super) const XZ: u8 = 20;
}

mod method {
    pub(super) const STORE: u16 = 0;
    pub(super) const DEFLATE: u16 = 8;
    pub(super) const DEFLATE64: u16 = 9;
    pub(super) const BZIP2: u16 = 12;
    pub(super) const LZMA: u16 = 14;
    pub(super) const XZ: u16 = 95;
    pub(super) const PPMD: u16 = 98;
    pub(super) const AES: u16 = 99;
}

mod flag {
    pub(super) const ENCRYPTED: u16 = 1;
    pub(super) const LZMA_EOS: u16 = 2;
    pub(super) const DESCRIPTOR: u16 = 8;
    pub(super) const STRONG: u16 = 0x40;
    pub(super) const UTF8: u16 = 0x800;
}

mod extra {
    pub(super) const ZIP64: u16 = 0x0001;
    pub(super) const NTFS: u16 = 0x000A;
    pub(super) const STRONG_ENCRYPT: u16 = 0x0017;
    pub(super) const UNIX_TIME: u16 = 0x5455;
    pub(super) const UNICODE_NAME: u16 = 0x7075;
    pub(super) const AES: u16 = 0x9901;
}

/// `k_PropVar_TimePrec_*` for `-mtp`.
const PREC_UNIX: u32 = 1;
const PREC_DOS: u32 = 2;
const PREC_BASE: u32 = 16;

/// The UTF-8 code page's number, for `-mcp`.
const CP_UTF8: u32 = 65_001;

/// Methods by name (`kMethodNames1`, `kMethodNames2` from 93), as `-m0=` names them.
const METHOD_NAMES: [(&str, u16); 16] = [
    ("store", 0),
    ("shrink", 1),
    ("reduce1", 2),
    ("reduce2", 3),
    ("reduce3", 4),
    ("reduce4", 5),
    ("implode", 6),
    ("deflate", 8),
    ("deflate64", 9),
    ("pkimploding", 10),
    ("bzip2", 12),
    ("lzma", 14),
    ("zstd", 93),
    ("mp3", 94),
    ("xz", 95),
    ("ppmd", 98),
];

/// The encryption `-mem` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CryptoMode {
    ZipCrypto,
    /// AES at a strength: 1 to 3, AES-128 to AES-256.
    Aes(u8),
}

/// The zip handler's `-m` (`CHandler::SetProperties`).
pub(super) struct Settings {
    multi: methods::Settings,
    /// `-mm=N`, or a method named by number or as Copy.
    main_method: Option<u16>,
    /// `-mem`: AES or `ZipCrypto`; none leaves it to the archive.
    crypto: Option<CryptoMode>,
    force_local: bool,
    force_utf8: bool,
    code_page: Option<u32>,
    remove_sfx: bool,
    seq_out: bool,
    mtime: bool,
    atime: bool,
    ctime: bool,
    prec: u32,
}

fn boolean(value: Option<&str>) -> Result<bool, MethodError> {
    match value.map(str::to_ascii_lowercase).as_deref() {
        None | Some("" | "on" | "+") => Ok(true),
        Some("off" | "-") => Ok(false),
        _ => Err(MethodError::Invalid),
    }
}

fn number(text: &str) -> Result<u32, MethodError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(MethodError::Invalid);
    }
    text.parse().map_err(|_| MethodError::Invalid)
}

/// `-mem=`: `AES128`, `AES-192`, `AES256` or `AES` (256), or `ZipCrypto`.
fn crypto_mode(value: Option<&str>) -> Result<CryptoMode, MethodError> {
    let value = value.ok_or(MethodError::Invalid)?;
    if value.eq_ignore_ascii_case("zipcrypto") {
        return Ok(CryptoMode::ZipCrypto);
    }
    let rest = value
        .get(..3)
        .filter(|p| p.eq_ignore_ascii_case("aes"))
        .and_then(|_| value.get(3..))
        .ok_or(MethodError::Invalid)?;
    if rest.is_empty() {
        return Ok(CryptoMode::Aes(3));
    }
    let bits = number(rest.strip_prefix('-').unwrap_or(rest))?;
    if bits % 64 != 0 || !(128..=256).contains(&bits) {
        return Err(MethodError::Invalid);
    }
    Ok(CryptoMode::Aes(u8::try_from(bits / 64 - 1).unwrap_or(3)))
}

impl Settings {
    pub(super) fn parse(properties: &[(String, Option<String>)]) -> Result<Self, MethodError> {
        let mut settings = Self {
            multi: methods::Settings::parse(&[])?,
            main_method: None,
            crypto: None,
            force_local: false,
            force_utf8: false,
            code_page: None,
            remove_sfx: false,
            seq_out: false,
            mtime: true,
            atime: false,
            ctime: false,
            prec: 0,
        };
        let mut rest = Vec::new();
        for (name, value) in properties {
            let lower = name.to_ascii_lowercase();
            let value = value.as_deref();
            match lower.as_str() {
                "" => return Err(MethodError::Invalid),
                "em" => settings.crypto = Some(crypto_mode(value)?),
                "cl" => {
                    settings.force_local = boolean(value)?;
                    if settings.force_local {
                        settings.force_utf8 = false;
                    }
                }
                "cu" => {
                    settings.force_utf8 = boolean(value)?;
                    if settings.force_utf8 {
                        settings.force_local = false;
                    }
                }
                "cp" => settings.code_page = Some(number(value.unwrap_or_default())?),
                "rsfx" => settings.remove_sfx = boolean(value)?,
                "rws" => settings.seq_out = boolean(value)?,
                "ros" => {
                    boolean(value)?;
                }
                "tm" => settings.mtime = boolean(value)?,
                "ta" => settings.atime = boolean(value)?,
                "tc" => settings.ctime = boolean(value)?,
                "m" if value
                    .is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())) =>
                {
                    let id = number(value.unwrap_or_default())?;
                    settings.main_method = Some(
                        u16::try_from(id)
                            .ok()
                            .filter(|&id| id <= 0xFF)
                            .ok_or(MethodError::Invalid)?,
                    );
                }
                n if n.starts_with("tp") => {
                    let digits = n.get(2..).unwrap_or_default();
                    settings.prec = match (digits, value) {
                        ("", None) => 0,
                        ("", Some(v)) | (v, None) => number(v)?,
                        _ => return Err(MethodError::Invalid),
                    };
                }
                // The 7z handler's own.
                "hc" | "he" | "hcf" | "tr" | "qs" | "mtf" => return Err(MethodError::Invalid),
                n if n.starts_with('s') => return Err(MethodError::Invalid),
                _ => rest.push((name.clone(), value.map(str::to_owned))),
            }
        }
        settings.multi = methods::Settings::parse(&rest)?;
        let (name, _) = settings.multi.single_method()?;
        if let Some(name) = name {
            if name.bytes().all(|b| b.is_ascii_digit()) {
                if let Some(id) = name.parse::<u16>().ok().filter(|&id| id <= 0xFF) {
                    settings.main_method = Some(id);
                }
            } else if name == "copy" {
                settings.main_method = Some(method::STORE);
            }
        }
        Ok(settings)
    }

    /// The method new files are compressed with: as named, else Store at level 0 and
    /// Deflate above.
    fn main_method(&self) -> Result<u16, Stop> {
        if let Some(id) = self.main_method {
            return Ok(id);
        }
        let (name, _) = self.multi.single_method()?;
        match name {
            Some(name) => METHOD_NAMES
                .iter()
                .find(|(n, _)| *n == name)
                .map(|&(_, id)| id)
                .ok_or_else(|| Stop::System(win_error(win::E_NOTIMPL))),
            None if self.multi.level == 0 => Ok(method::STORE),
            None => Ok(method::DEFLATE),
        }
    }
}

/// The handler's verdict on `-m`.
pub(super) fn check(properties: &[(String, Option<String>)]) -> Result<(), MethodError> {
    Settings::parse(properties).map(drop)
}

/// A member as the central directory will have it (`CItemOut`).
#[derive(Clone, Debug, Default)]
struct Record {
    made_by: u16,
    version_needed: u8,
    extract_host: u8,
    flags: u16,
    method: u16,
    time: u32,
    crc: u32,
    pack_size: u64,
    size: u64,
    name: Vec<u8>,
    /// The name in UTF-8, for an `up` field, when the code page held it.
    name_utf: Vec<u8>,
    /// The NTFS field's modified, accessed and created times, when written.
    ntfs: Option<[u64; 3]>,
    /// The `UT` field's modified time, when written instead.
    unix_time: Option<u32>,
    local_extra: Vec<u8>,
    central_extra: Vec<u8>,
    comment: Vec<u8>,
    internal_attrib: u16,
    external_attrib: u32,
    local_pos: u64,
    /// Whether the local header has Zip64 sizes, which a descriptor follows.
    local_zip64: bool,
}

const fn needs_zip64(v: u64) -> bool {
    v >= 0xFFFF_FFFF
}

fn put16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn put32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn put64(b: &mut Vec<u8>, v: u64) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn low32(v: u64) -> u32 {
    u32::try_from(v & 0xFFFF_FFFF).unwrap_or(0)
}

fn len16(len: usize) -> Result<u16, Stop> {
    u16::try_from(len).map_err(|_| Stop::System(win_error(win::E_FAIL)))
}

impl Record {
    const fn has_descriptor(&self) -> bool {
        self.flags & flag::DESCRIPTOR != 0
    }

    /// `WriteTimeExtra` without the NTFS field, then the `up` field: what both
    /// headers add before their own extra fields.
    fn time_and_name_extra(&self, b: &mut Vec<u8>, ntfs: bool) {
        if let (true, Some([m, a, c])) = (ntfs, self.ntfs) {
            put16(b, extra::NTFS);
            put16(b, 32);
            put32(b, 0);
            put16(b, 1);
            put16(b, 24);
            put64(b, m);
            put64(b, a);
            put64(b, c);
        }
        if let Some(u) = self.unix_time {
            put16(b, extra::UNIX_TIME);
            put16(b, 5);
            b.push(1);
            put32(b, u);
        }
        if !self.name_utf.is_empty() {
            put16(b, extra::UNICODE_NAME);
            put16(b, u16::try_from(5 + self.name_utf.len()).unwrap_or(0));
            b.push(1);
            put32(b, crc32fast::hash(&self.name));
            b.extend_from_slice(&self.name_utf);
        }
    }

    /// `WriteCommonItemInfo`.
    fn common(&self, b: &mut Vec<u8>, zip64: bool) {
        b.push(if zip64 {
            self.version_needed.max(version::ZIP64)
        } else {
            self.version_needed
        });
        b.push(self.extract_host);
        put16(b, self.flags);
        put16(b, self.method);
        put32(b, self.time);
    }

    /// `WriteLocalHeader`: the header with Zip64 sizes when `zip64`.
    fn local_header(&self, zip64: bool) -> Result<Vec<u8>, Stop> {
        let mut extra = Vec::new();
        let (pack, size) = if self.has_descriptor() {
            (0, 0)
        } else {
            (self.pack_size, self.size)
        };
        if zip64 {
            put16(&mut extra, extra::ZIP64);
            put16(&mut extra, 16);
            put64(&mut extra, size);
            put64(&mut extra, pack);
        }
        self.time_and_name_extra(&mut extra, false);
        extra.extend_from_slice(&self.local_extra);
        let mut b = Vec::new();
        put32(&mut b, LOCAL_SIG);
        self.common(&mut b, zip64);
        put32(&mut b, if self.has_descriptor() { 0 } else { self.crc });
        put32(&mut b, if zip64 { 0xFFFF_FFFF } else { low32(pack) });
        put32(&mut b, if zip64 { 0xFFFF_FFFF } else { low32(size) });
        put16(&mut b, len16(self.name.len())?);
        put16(&mut b, len16(extra.len())?);
        b.extend_from_slice(&self.name);
        b.extend_from_slice(&extra);
        Ok(b)
    }

    /// `WriteDescriptor`.
    fn descriptor(&self) -> Vec<u8> {
        let mut b = Vec::new();
        put32(&mut b, DESCRIPTOR_SIG);
        put32(&mut b, self.crc);
        if self.local_zip64 {
            put64(&mut b, self.pack_size);
            put64(&mut b, self.size);
        } else {
            put32(&mut b, low32(self.pack_size));
            put32(&mut b, low32(self.size));
        }
        b
    }

    /// `WriteCentralHeader`.
    fn central_header(&self) -> Result<Vec<u8>, Stop> {
        let unpack64 = needs_zip64(self.size);
        let pack64 = needs_zip64(self.pack_size);
        let pos64 = needs_zip64(self.local_pos);
        let zip64 = unpack64 || pack64 || pos64;
        let mut extra = Vec::new();
        if zip64 {
            put16(&mut extra, extra::ZIP64);
            put16(
                &mut extra,
                8 * u16::from(unpack64) + 8 * u16::from(pack64) + 8 * u16::from(pos64),
            );
            if unpack64 {
                put64(&mut extra, self.size);
            }
            if pack64 {
                put64(&mut extra, self.pack_size);
            }
            if pos64 {
                put64(&mut extra, self.local_pos);
            }
        }
        self.time_and_name_extra(&mut extra, true);
        extra.extend_from_slice(&self.central_extra);
        let mut b = Vec::new();
        put32(&mut b, CENTRAL_SIG);
        put16(&mut b, self.made_by);
        self.common(&mut b, zip64);
        put32(&mut b, self.crc);
        put32(
            &mut b,
            if pack64 {
                0xFFFF_FFFF
            } else {
                low32(self.pack_size)
            },
        );
        put32(
            &mut b,
            if unpack64 {
                0xFFFF_FFFF
            } else {
                low32(self.size)
            },
        );
        put16(&mut b, len16(self.name.len())?);
        put16(&mut b, len16(extra.len())?);
        put16(&mut b, len16(self.comment.len())?);
        put16(&mut b, 0);
        put16(&mut b, self.internal_attrib);
        put32(&mut b, self.external_attrib);
        put32(
            &mut b,
            if pos64 {
                0xFFFF_FFFF
            } else {
                low32(self.local_pos)
            },
        );
        b.extend_from_slice(&self.name);
        b.extend_from_slice(&extra);
        b.extend_from_slice(&self.comment);
        Ok(b)
    }
}

/// Code page 437's high half, the OEM code page names are written in.
const CP437_HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";

/// A name in code page 437, or `None` when a character is not in it.
fn oem_name(name: &str) -> Option<Vec<u8>> {
    name.chars()
        .map(|c| {
            if c.is_ascii() {
                u8::try_from(u32::from(c)).ok()
            } else {
                CP437_HIGH
                    .chars()
                    .position(|h| h == c)
                    .and_then(|at| u8::try_from(0x80 + at).ok())
            }
        })
        .collect()
}

/// The name's bytes, whether they are UTF-8 (the flag), and the UTF-8 copy for an `up`
/// field when they are the OEM code page's.
fn encode_name(name: &str, settings: &Settings) -> (Vec<u8>, bool, Vec<u8>) {
    let utf8_page = settings.code_page == Some(CP_UTF8);
    let mut try_utf8 = true;
    let mut local = Vec::new();
    if !utf8_page && (settings.force_local || !settings.force_utf8) {
        if let Some(bytes) = oem_name(name) {
            local = bytes;
            try_utf8 = false;
        } else {
            local = name
                .chars()
                .map(|c| {
                    u8::try_from(u32::from(c))
                        .ok()
                        .filter(u8::is_ascii)
                        .unwrap_or(b'_')
                })
                .collect();
            try_utf8 = !settings.force_local;
        }
    }
    let non_latin = !name.is_ascii();
    if try_utf8 {
        return (name.as_bytes().to_vec(), non_latin, Vec::new());
    }
    let utf = if non_latin {
        name.as_bytes().to_vec()
    } else {
        Vec::new()
    };
    (local, false, utf)
}

/// FILETIME ticks as Unix seconds, saturated to 32 bits (`FileTime_To_UnixTime`).
fn unix_time(ticks: u64) -> u32 {
    let seconds = ticks / 10_000_000;
    seconds
        .checked_sub(11_644_473_600)
        .map_or(0, |s| u32::try_from(s).unwrap_or(u32::MAX))
}

/// The new archive's members' common facts: name, times, flags; `CUpdateItem`'s part.
struct NewProps {
    name: Vec<u8>,
    is_utf8: bool,
    name_utf: Vec<u8>,
    time: u32,
    ntfs: Option<[u64; 3]>,
    unix_time: Option<u32>,
}

/// The extra field's blocks, each with its id, raw.
fn blocks(extra: &[u8]) -> Vec<(u16, &[u8])> {
    let mut list = Vec::new();
    let mut at = 0;
    while at + 4 <= extra.len() {
        let id = u16::from_le_bytes([extra[at], extra[at + 1]]);
        let len = usize::from(u16::from_le_bytes([extra[at + 2], extra[at + 3]]));
        let Some(data) = extra.get(at + 4..at + 4 + len) else {
            break;
        };
        list.push((id, data));
        at += 4 + len;
    }
    list
}

fn join_blocks(list: &[(u16, &[u8])]) -> Vec<u8> {
    let mut b = Vec::new();
    for (id, data) in list {
        put16(&mut b, *id);
        put16(&mut b, u16::try_from(data.len()).unwrap_or(0));
        b.extend_from_slice(data);
    }
    b
}

/// `RemoveUnknownSubBlocks`: AES's and strong encryption's fields only.
fn known_blocks(extra: &[u8]) -> Vec<u8> {
    let kept: Vec<(u16, &[u8])> = blocks(extra)
        .into_iter()
        .filter(|(id, _)| matches!(*id, extra::AES | extra::STRONG_ENCRYPT))
        .collect();
    join_blocks(&kept)
}

/// The central directory's extra field as 7-Zip keeps it: its Zip64 field read out.
fn without_zip64(extra: &[u8]) -> Vec<u8> {
    let kept: Vec<(u16, &[u8])> = blocks(extra)
        .into_iter()
        .filter(|(id, _)| *id != extra::ZIP64)
        .collect();
    join_blocks(&kept)
}

const fn is_aes(entry: &Entry) -> bool {
    entry.flags & flag::ENCRYPTED != 0
        && (entry.flags & flag::STRONG != 0 || entry.method == method::AES)
}

/// The version a method needs (`Set_Pre_CompressionResult`).
const fn method_version(id: u16) -> u8 {
    match id {
        method::DEFLATE => version::DEFLATE,
        method::DEFLATE64 => version::DEFLATE64,
        method::XZ => version::XZ,
        method::PPMD => version::PPMD,
        method::BZIP2 => version::BZIP2,
        method::LZMA => version::LZMA,
        _ => 0,
    }
}

/// The password and how it is used.
struct Crypto {
    password: Vec<u8>,
    /// AES's strength, else `ZipCrypto`.
    aes: Option<u8>,
}

impl Crypto {
    /// What the encryption adds to the data.
    const fn overhead(&self) -> u64 {
        match self.aes {
            Some(strength) => {
                let salt = match strength {
                    1 => 8,
                    2 => 12,
                    _ => 16,
                };
                salt + 2 + 10
            }
            None => 12,
        }
    }
}

/// A reader that counts and checks what passes through it.
struct Counted<R> {
    inner: R,
    crc: crc32fast::Hasher,
    size: u64,
}

impl<R: Read> Read for Counted<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.crc.update(&buf[..n]);
        self.size += n as u64;
        Ok(n)
    }
}

/// How each method compresses, from the settings and the largest new file.
struct Coder<'s> {
    settings: &'s Settings,
    reduce: u64,
}

impl Coder<'_> {
    /// `method`'s data of `input` into `out`.
    fn compress(&self, id: u16, input: &mut dyn Read, out: &mut dyn Write) -> Result<(), Stop> {
        let (_, params) = self.settings.multi.single_method()?;
        let level = self.settings.multi.level_for(params);
        match id {
            method::STORE => {
                io::copy(input, out).map_err(Stop::System)?;
            }
            method::DEFLATE => {
                let level = match level {
                    0 => 0,
                    1..=2 => 1,
                    3..=4 => 3,
                    5..=6 => 6,
                    7..=8 => 8,
                    _ => 9,
                };
                let mut encoder =
                    flate2::write::DeflateEncoder::new(out, flate2::Compression::new(level));
                io::copy(input, &mut encoder).map_err(Stop::System)?;
                encoder.finish()?;
            }
            method::BZIP2 => {
                let block = methods::bzip2_block(level, params);
                let mut encoder = codec::writer(Codec::Bzip2, out, block)?;
                io::copy(input, &mut encoder).map_err(Stop::System)?;
                encoder.finish()?;
            }
            method::LZMA => {
                let options = lzma_params(level, params, self.reduce);
                let eos = param(params, "eos").is_none_or(|v| !matches!(v, "-" | "off"));
                let mut encoder = lzma_writer(out, &options, eos)?;
                io::copy(input, &mut encoder).map_err(Stop::System)?;
                encoder.finish()?;
            }
            method::XZ => {
                let options = lzma_params(level, params, self.reduce);
                let block = self.settings.multi.solid_bytes().and_then(NonZeroU64::new);
                let mut encoder =
                    codec::xz::xz_writer(options, codec::xz::Check::Crc32, block, out)?;
                io::copy(input, &mut encoder).map_err(Stop::System)?;
                encoder.finish()?;
            }
            method::PPMD => {
                // PpmdZip's CEncProps::Normalize.
                let level = level.clamp(1, 9);
                let mut memory_mb = param(params, "mem")
                    .and_then(|m| size(m, true))
                    .map_or_else(
                        || 1 << (level.min(8) - 1),
                        |m| u32::try_from(m >> 20).unwrap_or(256),
                    );
                let mut m = 1u32;
                while m < memory_mb {
                    if self.reduce <= u64::from(m << 20) / 16 {
                        memory_mb = m;
                        break;
                    }
                    m <<= 1;
                }
                let order = param(params, "o")
                    .and_then(|o| o.parse().ok())
                    .unwrap_or(3 + level);
                let mut encoder = ppmd_writer(out, order, memory_mb.max(1), level >= 7)?;
                io::copy(input, &mut encoder).map_err(Stop::System)?;
                encoder.finish()?;
            }
            _ => return Err(Stop::System(win_error(win::E_NOTIMPL))),
        }
        Ok(())
    }
}

/// The zip handler's `UpdateItems` and `Update`; returns the files read.
#[expect(clippy::too_many_lines, reason = "7-Zip's Update2St, step by step")]
pub(super) fn write<SE: cash_core::ShellExtensions>(
    job: &Job<'_, '_, SE>,
    opened: Option<&Opened>,
    items: &[Item],
    out: &mut Out<'_>,
    warnings: &mut Warnings,
    processed: &mut [bool],
) -> Result<u64, Stop> {
    let settings = Settings::parse(&job.options.properties)?;
    if job.options.stdout {
        return Err(Stop::System(win_error(win::E_NOTIMPL)));
    }
    let zip: Option<&Zip> = opened.and_then(Opened::zip);
    let entries = zip.map_or(&[][..], |z| z.archive.entries.as_slice());
    let invalid = || Stop::System(win_error(win::E_INVALIDARG));

    // The password: -p's, or typed for -p without one.
    let mut password = job.options.password.clone().filter(|p| !p.is_empty());
    if password.is_none() && job.options.password.is_some() {
        let prompt = |t: &str| {
            job.console.so(t);
            job.console.flush_so();
        };
        password = Some(ask_password(job.env, &prompt)?);
    }
    let there_are_aes = items
        .iter()
        .filter_map(|ui| ui.up.arc.and_then(|a| entries.get(a)))
        .any(is_aes);
    let crypto = match password {
        Some(password) => {
            if !password.chars().all(|c| (' '..='\u{7F}').contains(&c)) {
                return Err(invalid());
            }
            let aes = match settings.crypto {
                Some(CryptoMode::Aes(strength)) => Some(strength),
                Some(CryptoMode::ZipCrypto) => None,
                None => there_are_aes.then_some(3),
            };
            if aes.is_some() && password.len() > 99 {
                return Err(invalid());
            }
            Some(Crypto {
                password: password.into_bytes(),
                aes,
            })
        }
        None => None,
    };

    let main = settings.main_method()?;
    let mut sequence = vec![main];
    if main != method::STORE {
        sequence.push(method::STORE);
    }
    if items.iter().any(|ui| ui.up.new_data && !ui.is_dir)
        && !matches!(
            main,
            method::STORE
                | method::DEFLATE
                | method::BZIP2
                | method::LZMA
                | method::XZ
                | method::PPMD
        )
    {
        return Err(Stop::System(win_error(win::E_NOTIMPL)));
    }
    let reduce = items
        .iter()
        .filter(|ui| ui.up.new_data && !ui.is_dir)
        .map(|ui| ui.size)
        .max()
        .unwrap_or(0);
    let coder = Coder {
        settings: &settings,
        reduce,
    };

    // A self-extractor's program before the archive stays, unless -mrsfx.
    let base = if let Some(zip) = zip
        && zip.archive.extra_bytes > 0
        && !settings.remove_sfx
    {
        let stub = u64::try_from(zip.archive.extra_bytes).unwrap_or(0);
        let mut file = File::open(&zip.path).map_err(Stop::System)?;
        let copied = io::copy(&mut (&mut file).take(stub), out).map_err(Stop::System)?;
        if copied != stub {
            return Err(Stop::System(win_error(win::E_FAIL)));
        }
        stub
    } else {
        0
    };

    let mut records: Vec<Record> = Vec::new();
    let mut files_read = 0u64;
    let mut source = match zip {
        Some(z) => Some(File::open(&z.path).map_err(Stop::System)?),
        None => None,
    };
    for ui in items {
        if ui.up.new_data {
            let props = new_props(ui, &settings, job);
            let mut rec = Record {
                made_by: u16::from(MADE_BY_VERSION) | u16::from(HOST_FAT) << 8,
                extract_host: 0,
                name: props.name.clone(),
                name_utf: props.name_utf.clone(),
                time: props.time,
                ntfs: props.ntfs,
                unix_time: props.unix_time,
                flags: if props.is_utf8 { flag::UTF8 } else { 0 },
                external_attrib: ui.attrib.unwrap_or(0),
                ..Record::default()
            };
            if ui.is_dir {
                rec.version_needed = version::DIR;
                rec.method = method::STORE;
                rec.local_pos = out.position() - base;
                out.write_all(&rec.local_header(false)?)?;
                records.push(rec);
                continue;
            }
            let Some(d) = ui.up.dir else {
                continue;
            };
            job.announce(ui);
            let Some(mut file) = job.open_new(d, warnings) else {
                continue;
            };
            // UpdatePropsFromStream: the open file's size and attributes.
            if let Some(meta) = file.file().and_then(|f| f.metadata().ok()) {
                use std::os::windows::fs::MetadataExt;
                rec.external_attrib = meta.file_attributes();
            }
            add_file(
                &mut rec,
                &mut file,
                ui,
                &sequence,
                crypto.as_ref(),
                &coder,
                &settings,
                out,
                base,
            )?;
            processed[d] = true;
            files_read += 1;
            records.push(rec);
        } else {
            let (Some(a), Some(zip), Some(file)) = (ui.up.arc, zip, source.as_mut()) else {
                continue;
            };
            let entry = &entries[a];
            // ReportOperation names the archive's item, by its old name.
            let mut shown = job.arc_items[a].name.clone();
            if job.arc_items[a].is_dir {
                shown.push('/');
            }
            progress(job.console, job.options, 3, "=", &shown);
            let props = ui.up.new_props.then(|| new_props(ui, &settings, job));
            let rec = copy_old(zip, file, entry, props, out, base)?;
            records.push(rec);
        }
    }

    // The central directory and its end.
    let cd_offset = out.position() - base;
    for rec in &records {
        out.write_all(&rec.central_header()?)?;
    }
    let cd_end = out.position() - base;
    let cd_size = cd_end - cd_offset;
    let count = records.len() as u64;
    let many = count >= 0xFFFF;
    let mut end = Vec::new();
    if needs_zip64(cd_offset) || needs_zip64(cd_size) || many {
        put32(&mut end, END64_SIG);
        put64(&mut end, 44);
        put16(&mut end, 45);
        put16(&mut end, 45);
        put32(&mut end, 0);
        put32(&mut end, 0);
        put64(&mut end, count);
        put64(&mut end, count);
        put64(&mut end, cd_size);
        put64(&mut end, cd_offset);
        put32(&mut end, END64_LOCATOR_SIG);
        put32(&mut end, 0);
        put64(&mut end, cd_end);
        put32(&mut end, 1);
    }
    let comment = zip.map_or(&[][..], |z| z.archive.end.comment.as_slice());
    let items16 = if many {
        0xFFFF
    } else {
        u16::try_from(count).unwrap_or(0xFFFF)
    };
    put32(&mut end, END_SIG);
    put32(&mut end, 0);
    put16(&mut end, items16);
    put16(&mut end, items16);
    put32(
        &mut end,
        if needs_zip64(cd_size) {
            0xFFFF_FFFF
        } else {
            low32(cd_size)
        },
    );
    put32(
        &mut end,
        if needs_zip64(cd_offset) {
            0xFFFF_FFFF
        } else {
            low32(cd_offset)
        },
    );
    put16(&mut end, len16(comment.len())?);
    end.extend_from_slice(comment);
    out.write_all(&end)?;
    Ok(files_read)
}

/// The facts an item with new properties gets: its name as written, its MS-DOS time,
/// and the times its NTFS or `UT` field keeps.
fn new_props<SE: cash_core::ShellExtensions>(
    ui: &Item,
    settings: &Settings,
    job: &Job<'_, '_, SE>,
) -> NewProps {
    let mut name = ui.name.replace('\\', "/");
    if ui.is_dir && !name.ends_with('/') {
        name.push('/');
    }
    let (bytes, is_utf8, name_utf) = encode_name(&name, settings);
    let pick = |on: bool, t: Option<u64>| if on { t.unwrap_or(0) } else { 0 };
    let mtime = pick(settings.mtime, ui.modified);
    let atime = pick(settings.atime, ui.accessed);
    let ctime = pick(settings.ctime, ui.created);
    let (ntfs, unix) = match settings.prec {
        PREC_DOS => (None, None),
        PREC_UNIX | PREC_BASE => (None, (mtime != 0).then(|| unix_time(mtime))),
        _ => (
            (mtime != 0 || atime != 0 || ctime != 0).then_some([mtime, atime, ctime]),
            None,
        ),
    };
    NewProps {
        name: bytes,
        is_utf8,
        name_utf,
        time: text::dos_time(&job.env.zone, mtime),
        ntfs,
        unix_time: unix,
    }
}

/// `Set_Pre_CompressionResult`, `Compress` and `WriteLocalHeader_Replace`: the local
/// header written with what is known, the data compressed by each method in turn until
/// one makes it smaller (Store last), then the header written again with what it came
/// to, or a descriptor after the data.
#[expect(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "the item, its file and the archive's settings; 7-Zip's Compress"
)]
fn add_file(
    rec: &mut Record,
    file: &mut Input,
    ui: &Item,
    sequence: &[u16],
    crypto: Option<&Crypto>,
    coder: &Coder<'_>,
    settings: &Settings,
    out: &mut Out<'_>,
    base: u64,
) -> Result<(), Stop> {
    let size = file.size_or(ui.size);
    // Standard input is read once: no second method, and no ZipCrypto, whose header
    // needs the CRC first.
    let in_seq = file.file().is_none();
    if in_seq && crypto.is_some_and(|c| c.aes.is_none()) {
        return Err(Stop::System(win_error(win::E_NOTIMPL)));
    }
    let first = sequence.first().copied().unwrap_or(method::STORE);
    // The size guessed before compressing: Zip64 only for files near 4 GiB.
    let guess = if first == method::STORE && crypto.is_none() {
        size
    } else if size < 0xF800_0000 {
        u64::from(u32::MAX - 1).max(size)
    } else {
        1 << 60
    };
    let set_result = |rec: &mut Record, id: u16, crc: u32, unpacked: u64, packed: u64| {
        let needed = match crypto {
            None => version::DEFAULT,
            Some(c) if c.aes.is_some() => version::AES,
            Some(_) => version::ZIP_CRYPTO,
        };
        rec.version_needed = needed.max(method_version(id));
        rec.method = id;
        rec.flags &= !flag::LZMA_EOS;
        if id == method::LZMA {
            rec.flags |= flag::LZMA_EOS;
        }
        rec.crc = crc;
        rec.size = unpacked;
        rec.pack_size = packed;
        rec.local_extra.clear();
        rec.central_extra.clear();
        if let Some(Crypto {
            aes: Some(strength),
            ..
        }) = crypto
        {
            let mut field = Vec::new();
            put16(&mut field, extra::AES);
            put16(&mut field, 7);
            put16(&mut field, 2);
            field.extend_from_slice(b"AE");
            field.push(*strength);
            put16(&mut field, id);
            rec.local_extra.clone_from(&field);
            rec.central_extra = field;
            rec.method = method::AES;
            rec.crc = 0;
        }
    };
    if crypto.is_some() {
        rec.flags |= flag::ENCRYPTED;
    }
    if settings.seq_out {
        rec.flags |= flag::DESCRIPTOR;
    }
    set_result(rec, first, 0, size, guess);
    rec.local_pos = out.position() - base;
    let zip64 = needs_zip64(rec.pack_size) || needs_zip64(rec.size);
    rec.local_zip64 = zip64;
    let header = rec.local_header(zip64)?;
    out.write_all(&header)?;
    let data_start = out.position();

    // ZipCrypto checks its header with the CRC's high half, read before.
    let mut crc_first = None;
    if let Some(Crypto { aes: None, .. }) = crypto
        && !rec.has_descriptor()
        && let Some(f) = file.file()
    {
        let mut counted = Counted {
            inner: &mut *f,
            crc: crc32fast::Hasher::new(),
            size: 0,
        };
        io::copy(&mut counted, &mut io::sink()).map_err(Stop::System)?;
        crc_first = Some(counted.crc.finalize());
        f.seek(SeekFrom::Start(0)).map_err(Stop::System)?;
    }
    let mut result = (first, 0, 0, 0);
    for (i, &id) in sequence.iter().enumerate() {
        if i > 0 {
            let Some(f) = file.file() else {
                break;
            };
            if !out.truncate_to(data_start).map_err(Stop::System)? {
                break;
            }
            f.seek(SeekFrom::Start(0)).map_err(Stop::System)?;
        }
        let mut input = Counted {
            inner: &mut *file,
            crc: crc32fast::Hasher::new(),
            size: 0,
        };
        match crypto {
            None => coder.compress(id, &mut input, &mut *out)?,
            Some(Crypto {
                password,
                aes: Some(strength),
            }) => {
                let mut writer = aes::aes_writer(&mut *out, *strength, password)?;
                coder.compress(id, &mut input, &mut writer)?;
                writer.finish()?;
            }
            Some(Crypto {
                password,
                aes: None,
            }) => {
                let check = match crc_first {
                    Some(crc) => u16::try_from(crc >> 16).unwrap_or(0),
                    None => u16::try_from(rec.time & 0xFFFF).unwrap_or(0),
                };
                let mut writer = zip_crypto_writer(&mut *out, password, check)?;
                coder.compress(id, &mut input, &mut writer)?;
            }
        }
        let packed = out.position() - data_start;
        let (crc, unpacked) = (input.crc.finalize(), input.size);
        result = (id, crc, unpacked, packed);
        let overhead = crypto.map_or(0, Crypto::overhead);
        if packed < unpacked + overhead {
            break;
        }
    }
    let (id, crc, unpacked, packed) = result;
    set_result(rec, id, crc, unpacked, packed);
    if rec.has_descriptor() {
        out.write_all(&rec.descriptor())?;
        return Ok(());
    }
    let again = rec.local_header(zip64 || needs_zip64(packed) || needs_zip64(unpacked))?;
    if again.len() != header.len() || !out.rewrite_at(data_start - header.len() as u64, &again)? {
        return Err(Stop::System(win_error(win::E_FAIL)));
    }
    Ok(())
}

/// `UpdateItemOldData`: an item of the old archive copied, its local header, data and
/// descriptor as they were; or, with new properties, a new local header over its data.
fn copy_old(
    zip: &Zip,
    file: &mut File,
    entry: &Entry,
    props: Option<NewProps>,
    out: &mut Out<'_>,
    base: u64,
) -> Result<Record, Stop> {
    let not_impl = || Stop::System(win_error(win::E_NOTIMPL));
    let local = zread::local(file, &zip.archive, entry).map_err(|_| not_impl())?;
    let local_zip64 = blocks(&local.extra)
        .iter()
        .any(|(id, _)| *id == extra::ZIP64);
    let descriptor = if entry.has_descriptor() {
        if local_zip64 { 24 } else { 16 }
    } else {
        0
    };
    if descriptor != 0 {
        // CheckDescriptor: 7-Zip copies only what it can check.
        file.seek(SeekFrom::Start(local.data_offset + entry.compressed_size))
            .map_err(Stop::System)?;
        let mut d = vec![0u8; descriptor];
        file.read_exact(&mut d).map_err(|_| not_impl())?;
        let le = |at: usize, n: usize| -> u64 {
            d[at..at + n]
                .iter()
                .rev()
                .fold(0u64, |v, &b| v << 8 | u64::from(b))
        };
        let sizes_ok = if local_zip64 {
            le(8, 8) == entry.compressed_size && le(16, 8) == entry.size
        } else {
            le(8, 4) == entry.compressed_size & 0xFFFF_FFFF && le(12, 4) == entry.size & 0xFFFF_FFFF
        };
        if le(0, 4) != u64::from(DESCRIPTOR_SIG) || le(4, 4) != u64::from(entry.crc) || !sizes_ok {
            return Err(not_impl());
        }
    }
    let header_pos = zip.archive.position(entry.local_offset);
    let mut rec = Record {
        made_by: entry.version_made_by,
        version_needed: entry.version_needed.to_le_bytes()[0],
        extract_host: entry.version_needed.to_le_bytes()[1],
        flags: entry.flags,
        method: entry.method,
        time: u32::from(entry.time.date) << 16 | u32::from(entry.time.time),
        crc: entry.crc,
        pack_size: entry.compressed_size,
        size: entry.size,
        name: entry.name.clone(),
        central_extra: without_zip64(&entry.extra),
        comment: entry.comment.clone(),
        internal_attrib: entry.internal_attributes,
        external_attrib: entry.external_attributes,
        local_zip64,
        ..Record::default()
    };
    let (from, len) = if let Some(props) = props {
        rec.flags &= !flag::DESCRIPTOR;
        rec.name = props.name;
        rec.name_utf = props.name_utf;
        rec.flags = if props.is_utf8 {
            rec.flags | flag::UTF8
        } else {
            rec.flags & !flag::UTF8
        };
        rec.time = props.time;
        rec.ntfs = props.ntfs;
        rec.unix_time = props.unix_time;
        rec.central_extra = known_blocks(&entry.extra);
        rec.local_extra = known_blocks(&local.extra);
        rec.local_pos = out.position() - base;
        let zip64 = needs_zip64(rec.pack_size) || needs_zip64(rec.size);
        rec.local_zip64 = zip64;
        out.write_all(&rec.local_header(zip64)?)?;
        (local.data_offset, entry.compressed_size)
    } else {
        rec.local_pos = out.position() - base;
        (
            header_pos,
            local.data_offset - header_pos + entry.compressed_size + descriptor as u64,
        )
    };
    file.seek(SeekFrom::Start(from)).map_err(Stop::System)?;
    let copied = io::copy(&mut (&mut *file).take(len), out).map_err(Stop::System)?;
    if copied != len {
        return Err(Stop::System(win_error(win::E_FAIL)));
    }
    Ok(rec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(list: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
        list.iter()
            .map(|(n, v)| ((*n).to_owned(), v.map(str::to_owned)))
            .collect()
    }

    #[test]
    fn names_go_in_the_oem_code_page_or_utf_8() {
        let settings = Settings::parse(&[]).unwrap();
        assert_eq!(
            encode_name("d/a.txt", &settings),
            (b"d/a.txt".to_vec(), false, Vec::new())
        );
        assert_eq!(
            encode_name("d/caf\u{e9}", &settings),
            (
                b"d/caf\x82".to_vec(),
                false,
                "d/caf\u{e9}".as_bytes().to_vec()
            )
        );
        assert_eq!(
            encode_name("smile\u{263a}", &settings),
            ("smile\u{263a}".as_bytes().to_vec(), true, Vec::new())
        );
        let utf = Settings::parse(&props(&[("cu", None)])).unwrap();
        assert_eq!(
            encode_name("d/caf\u{e9}", &utf),
            ("d/caf\u{e9}".as_bytes().to_vec(), true, Vec::new())
        );
    }

    #[test]
    fn settings_take_the_zip_handlers_names() {
        let s = Settings::parse(&props(&[("em", Some("AES-192")), ("mm", None)]));
        assert!(s.is_err());
        let s = Settings::parse(&props(&[("em", Some("AES192"))])).unwrap();
        assert_eq!(s.crypto, Some(CryptoMode::Aes(2)));
        let s = Settings::parse(&props(&[("m", Some("14"))])).unwrap();
        assert_eq!(s.main_method().unwrap(), method::LZMA);
        let s = Settings::parse(&props(&[("m", Some("BZip2"))])).unwrap();
        assert_eq!(s.main_method().unwrap(), method::BZIP2);
        let s = Settings::parse(&props(&[("x", Some("0"))])).unwrap();
        assert_eq!(s.main_method().unwrap(), method::STORE);
        assert!(Settings::parse(&props(&[("hc", Some("off"))])).is_err());
        assert!(Settings::parse(&props(&[("em", Some("AES100"))])).is_err());
    }

    #[test]
    fn central_records_carry_the_ntfs_time() {
        let rec = Record {
            made_by: 63,
            version_needed: 10,
            name: b"a".to_vec(),
            ntfs: Some([1, 0, 0]),
            external_attrib: 0x20,
            ..Record::default()
        };
        let central = rec.central_header().unwrap();
        assert_eq!(&central[..6], &[0x50, 0x4B, 1, 2, 63, 0]);
        assert_eq!(central.len(), 46 + 1 + 36);
        let local = rec.local_header(false).unwrap();
        assert_eq!(local.len(), 30 + 1);
    }
}
