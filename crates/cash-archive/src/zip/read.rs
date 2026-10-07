//! Reading a zip archive as unzip 6.00 does.
//!
//! The end record is found from the end of the file, the central directory at the
//! offset it gives (moved by whatever was put before the archive), each member's local
//! header, and its data decrypted and decompressed.

use std::io::{self, BufReader, Read, Seek, SeekFrom};

use super::crypt::{Decrypt, Keys, check_header};
use super::{
    CENTRAL_SIGNATURE, DosTime, END_SIGNATURE, Entry, LOCAL_SIGNATURE, ZIP64_END_SIGNATURE,
    ZIP64_LOCATOR_SIGNATURE, extra_id, fields, le16, le32, le64, method,
};

/// The end of central directory record, with what the Zip64 record changes in it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct End {
    /// Where the record starts in the file.
    pub offset: u64,
    /// This disk's number.
    pub disk: u32,
    /// The disk the central directory starts on.
    pub central_disk: u32,
    /// Entries on this disk.
    pub entries_on_disk: u64,
    /// Entries in all.
    pub entries: u64,
    /// The central directory's size.
    pub central_size: u64,
    /// Its offset, from the start of the archive proper.
    pub central_offset: u64,
    /// The archive's comment.
    pub comment: Vec<u8>,
    /// Where the Zip64 end record starts, when there is one.
    pub zip64_offset: Option<u64>,
}

/// An archive's directory: its end record and its entries.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Archive {
    /// The file's size.
    pub file_size: u64,
    /// The end record.
    pub end: End,
    /// Bytes before the archive proper (a self-extractor's program, or junk): where the
    /// central directory ends less where the end record says it ends. Negative when
    /// bytes are missing.
    pub extra_bytes: i64,
    /// The central directory's records, in order.
    pub entries: Vec<Entry>,
}

impl Archive {
    /// Where an offset the archive gives is in the file.
    pub fn position(&self, offset: u64) -> u64 {
        offset.saturating_add_signed(self.extra_bytes.max(0))
    }
}

/// Why an archive could not be read.
#[derive(Debug)]
pub enum OpenError {
    /// No end record in the last 64 KiB: not a zip file, or one part of several.
    NoEnd,
    /// The central directory does not start or go on where the end record says, at the
    /// entry numbered (from 1).
    BadCentral(u64),
    /// Reading failed.
    Io(io::Error),
}

impl From<io::Error> for OpenError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// A member's local header.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Local {
    /// The version needed.
    pub version_needed: u16,
    /// Its flags.
    pub flags: u16,
    /// Its method.
    pub method: u16,
    /// Its time.
    pub time: DosTime,
    /// Its CRC, zero when a descriptor follows.
    pub crc: u32,
    /// Its compressed size.
    pub compressed_size: u64,
    /// Its size.
    pub size: u64,
    /// Its name.
    pub name: Vec<u8>,
    /// The local extra field, which has the access time the central one leaves out.
    pub extra: Vec<u8>,
    /// Where the data starts in the file.
    pub data_offset: u64,
}

/// Why a member's data cannot be read.
#[derive(Debug)]
pub enum DataError {
    /// No local header where the directory says: the offset in the file.
    BadLocal(u64),
    /// A method cash cannot decompress.
    Unsupported(u16),
    /// The data is encrypted and no password was given.
    NeedPassword,
    /// The password does not open it.
    BadPassword,
    /// Reading failed.
    Io(io::Error),
}

impl From<io::Error> for DataError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// How far from the end the end record may start: its 22 bytes and a comment of up to
/// 65,535.
const END_SEARCH: u64 = 22 + 65_535;

/// Finds the end record as unzip's `find_ecrec` does: the last signature in the file's
/// last 64 KiB.
fn find_end<R: Read + Seek>(input: &mut R, file_size: u64) -> Result<(u64, Vec<u8>), OpenError> {
    if file_size < 22 {
        return Err(OpenError::NoEnd);
    }
    let start = file_size.saturating_sub(END_SEARCH);
    input.seek(SeekFrom::Start(start))?;
    let mut tail = Vec::new();
    input.take(END_SEARCH).read_to_end(&mut tail)?;
    let signature = END_SIGNATURE.to_le_bytes();
    let at = tail
        .windows(4)
        .enumerate()
        .rev()
        .filter(|(i, w)| *w == signature && tail.len() - i >= 22)
        .map(|(i, _)| i)
        .next()
        .ok_or(OpenError::NoEnd)?;
    Ok((
        start + at as u64,
        tail.get(at..).unwrap_or_default().to_vec(),
    ))
}

/// Reads an archive's end record and central directory.
///
/// # Errors
///
/// When there is no end record, the central directory is not where it says, or reading
/// fails.
pub fn open<R: Read + Seek>(input: &mut R) -> Result<Archive, OpenError> {
    let file_size = input.seek(SeekFrom::End(0))?;
    let (offset, record) = find_end(input, file_size)?;
    let comment_len = usize::from(le16(&record, 20));
    let mut end = End {
        offset,
        disk: u32::from(le16(&record, 4)),
        central_disk: u32::from(le16(&record, 6)),
        entries_on_disk: u64::from(le16(&record, 8)),
        entries: u64::from(le16(&record, 10)),
        central_size: u64::from(le32(&record, 12)),
        central_offset: u64::from(le32(&record, 16)),
        comment: record
            .get(22..22 + comment_len)
            .or_else(|| record.get(22..))
            .unwrap_or_default()
            .to_vec(),
        zip64_offset: None,
    };
    let mut central_end = offset;
    if offset >= 20 {
        let mut locator = [0_u8; 20];
        input.seek(SeekFrom::Start(offset - 20))?;
        if input.read_exact(&mut locator).is_ok() && le32(&locator, 0) == ZIP64_LOCATOR_SIGNATURE {
            let zip64_at = le64(&locator, 8);
            // The locator's offset is from the start of the archive proper: find the
            // record by it, or just before the locator when bytes were put in front.
            let mut record = [0_u8; 56];
            let candidates = [zip64_at, (offset - 20).saturating_sub(56)];
            for at in candidates {
                input.seek(SeekFrom::Start(at))?;
                if input.read_exact(&mut record).is_ok() && le32(&record, 0) == ZIP64_END_SIGNATURE
                {
                    end.zip64_offset = Some(at);
                    end.disk = le32(&record, 16);
                    end.central_disk = le32(&record, 20);
                    end.entries_on_disk = le64(&record, 24);
                    end.entries = le64(&record, 32);
                    end.central_size = le64(&record, 40);
                    end.central_offset = le64(&record, 48);
                    central_end = at;
                    break;
                }
            }
        }
    }
    let expected = end.central_offset.saturating_add(end.central_size);
    let extra_bytes = i64::try_from(central_end).unwrap_or(i64::MAX)
        - i64::try_from(expected).unwrap_or(i64::MAX);
    let mut archive = Archive {
        file_size,
        end,
        extra_bytes,
        entries: Vec::new(),
    };
    let start = archive.position(archive.end.central_offset);
    input.seek(SeekFrom::Start(start))?;
    let mut reader = BufReader::new(input);
    for number in 1..=archive.end.entries {
        let mut fixed = [0_u8; 46];
        if reader.read_exact(&mut fixed).is_err() || le32(&fixed, 0) != CENTRAL_SIGNATURE {
            return Err(OpenError::BadCentral(number));
        }
        let name_len = usize::from(le16(&fixed, 28));
        let extra_len = usize::from(le16(&fixed, 30));
        let comment_len = usize::from(le16(&fixed, 32));
        let mut variable = vec![0_u8; name_len + extra_len + comment_len];
        if reader.read_exact(&mut variable).is_err() {
            return Err(OpenError::BadCentral(number));
        }
        let mut entry = Entry {
            version_made_by: le16(&fixed, 4),
            version_needed: le16(&fixed, 6),
            flags: le16(&fixed, 8),
            method: le16(&fixed, 10),
            time: DosTime {
                time: le16(&fixed, 12),
                date: le16(&fixed, 14),
            },
            crc: le32(&fixed, 16),
            compressed_size: u64::from(le32(&fixed, 20)),
            size: u64::from(le32(&fixed, 24)),
            name: variable.get(..name_len).unwrap_or_default().to_vec(),
            extra: variable
                .get(name_len..name_len + extra_len)
                .unwrap_or_default()
                .to_vec(),
            comment: variable
                .get(name_len + extra_len..)
                .unwrap_or_default()
                .to_vec(),
            disk_start: u32::from(le16(&fixed, 34)),
            internal_attributes: le16(&fixed, 36),
            external_attributes: le32(&fixed, 38),
            local_offset: u64::from(le32(&fixed, 42)),
        };
        apply_zip64(&mut entry);
        archive.entries.push(entry);
    }
    Ok(archive)
}

/// Takes the Zip64 field's values for the fields that say to look there.
fn apply_zip64(entry: &mut Entry) {
    let Some((_, data)) = fields(&entry.extra)
        .into_iter()
        .find(|(id, _)| *id == extra_id::ZIP64)
    else {
        return;
    };
    let data = data.to_vec();
    let mut at = 0;
    let mut next = |wanted: bool| {
        if !wanted || data.len() < at + 8 {
            return None;
        }
        let value = le64(&data, at);
        at += 8;
        Some(value)
    };
    if let Some(size) = next(entry.size == 0xffff_ffff) {
        entry.size = size;
    }
    if let Some(size) = next(entry.compressed_size == 0xffff_ffff) {
        entry.compressed_size = size;
    }
    if let Some(offset) = next(entry.local_offset == 0xffff_ffff) {
        entry.local_offset = offset;
    }
}

/// Reads a member's local header.
///
/// # Errors
///
/// When there is no local header where the directory says, or reading fails.
pub fn local<R: Read + Seek>(
    input: &mut R,
    archive: &Archive,
    entry: &Entry,
) -> Result<Local, DataError> {
    let at = archive.position(entry.local_offset);
    input.seek(SeekFrom::Start(at))?;
    let mut fixed = [0_u8; 30];
    if input.read_exact(&mut fixed).is_err() || le32(&fixed, 0) != LOCAL_SIGNATURE {
        return Err(DataError::BadLocal(at));
    }
    let name_len = usize::from(le16(&fixed, 26));
    let extra_len = usize::from(le16(&fixed, 28));
    let mut variable = vec![0_u8; name_len + extra_len];
    if input.read_exact(&mut variable).is_err() {
        return Err(DataError::BadLocal(at));
    }
    let mut local = Local {
        version_needed: le16(&fixed, 4),
        flags: le16(&fixed, 6),
        method: le16(&fixed, 8),
        time: DosTime {
            time: le16(&fixed, 10),
            date: le16(&fixed, 12),
        },
        crc: le32(&fixed, 14),
        compressed_size: u64::from(le32(&fixed, 18)),
        size: u64::from(le32(&fixed, 22)),
        name: variable.get(..name_len).unwrap_or_default().to_vec(),
        extra: variable.get(name_len..).unwrap_or_default().to_vec(),
        data_offset: at + 30 + (name_len + extra_len) as u64,
    };
    if let Some((_, data)) = fields(&local.extra)
        .into_iter()
        .find(|(id, _)| *id == extra_id::ZIP64)
    {
        if local.size == 0xffff_ffff && data.len() >= 8 {
            local.size = le64(data, 0);
        }
        if local.compressed_size == 0xffff_ffff && data.len() >= 16 {
            local.compressed_size = le64(data, 8);
        }
    }
    Ok(local)
}

/// Whether cash can decompress a method.
pub const fn supported(method: u16) -> bool {
    matches!(
        method,
        method::STORED
            | method::DEFLATED
            | method::DEFLATE64
            | method::BZIP2
            | method::LZMA
            | method::XZ
            | method::ZSTD
    )
}

/// A member's data, decrypted and decompressed, from `raw`: its stored bytes, the
/// encryption header first when it has one.
///
/// # Errors
///
/// When the method is one cash does not have, the password is missing or wrong, or
/// reading the encryption header or the LZMA properties fails.
pub fn data<'a, R: Read + 'a>(
    raw: R,
    entry: &Entry,
    password: Option<&[u8]>,
) -> Result<Box<dyn Read + 'a>, DataError> {
    if !supported(entry.method) {
        return Err(DataError::Unsupported(entry.method));
    }
    let mut raw: Box<dyn Read + 'a> = Box::new(raw);
    if entry.is_encrypted() {
        let Some(password) = password else {
            return Err(DataError::NeedPassword);
        };
        let mut header = [0_u8; 12];
        raw.read_exact(&mut header)?;
        let mut keys = Keys::new(password);
        let check = if entry.has_descriptor() {
            entry.time.time.to_be_bytes()[0]
        } else {
            entry.crc.to_be_bytes()[0]
        };
        if !check_header(&mut keys, &header, check) {
            return Err(DataError::BadPassword);
        }
        raw = Box::new(Decrypt::new(raw, keys));
    }
    Ok(match entry.method {
        method::DEFLATED => Box::new(flate2::read::DeflateDecoder::new(raw)),
        method::DEFLATE64 => Box::new(deflate64::Deflate64Decoder::new(raw)),
        method::BZIP2 => Box::new(bzip2::read::MultiBzDecoder::new(raw)),
        method::LZMA => {
            // zip's own header: two version bytes, the properties' size, then the
            // properties byte and the dictionary size.
            let mut head = [0_u8; 4];
            raw.read_exact(&mut head)?;
            let mut props = vec![0_u8; usize::from(le16(&head, 2))];
            raw.read_exact(&mut props)?;
            let dict = le32(&props, 1);
            let size = if entry.flags & 0x0002 != 0 {
                u64::MAX
            } else {
                entry.size
            };
            let reader = lzma_rust2::LzmaReader::new_with_props(
                raw,
                size,
                props.first().copied().unwrap_or(0),
                dict.max(4096),
                None,
            )
            .map_err(|e| DataError::Io(io::Error::other(e.to_string())))?;
            Box::new(reader)
        }
        method::XZ => Box::new(lzma_rust2::XzReader::new(raw, false)),
        method::ZSTD => Box::new(crate::codec::reader(crate::codec::Codec::Zstd, raw)),
        _ => raw,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// `zip -0 -X` of one file, `a.txt` with `hello` in it, by Info-ZIP's zip 3.0.
    const INFO_ZIP: &str = "504b03040a00000000008318225020303a36060000000600000005000000612e74787468656c6c6f0a\
        504b01021e030a00000000008318225020303a360600000006000000050000000000000000000000b48100000000612e747874\
        504b0506000000000100010033000000290000000000";

    fn unhex(text: &str) -> Vec<u8> {
        let digits: Vec<u8> = text.bytes().filter(u8::is_ascii_hexdigit).collect();
        digits
            .chunks(2)
            .map(|pair| {
                u8::from_str_radix(std::str::from_utf8(pair).unwrap_or("0"), 16).unwrap_or(0)
            })
            .collect()
    }

    #[test]
    fn an_info_zip_archive_is_read() {
        let bytes = unhex(INFO_ZIP);
        let mut input = Cursor::new(bytes.clone());
        let archive = open(&mut input).unwrap_or_default();
        assert_eq!(archive.entries.len(), 1);
        let entry = archive.entries.first().cloned().unwrap_or_default();
        assert_eq!(entry.name, b"a.txt");
        assert_eq!(entry.unix_mode(), Some(0o100_664));
        assert_eq!(archive.extra_bytes, 0);
        let local = local(&mut input, &archive, &entry).unwrap_or_default();
        let start = usize::try_from(local.data_offset).unwrap_or(0);
        let raw = bytes.get(start..start + 6).unwrap_or_default();
        let mut out = Vec::new();
        if let Ok(mut reader) = data(raw, &entry, None) {
            reader.read_to_end(&mut out).unwrap_or_default();
        }
        assert_eq!(out, b"hello\n");
        let mut prefixed = b"#!junk\n".to_vec();
        prefixed.extend(&bytes);
        let archive = open(&mut Cursor::new(prefixed)).unwrap_or_default();
        assert_eq!(archive.extra_bytes, 7);
    }

    #[test]
    fn a_file_without_an_end_record_is_not_an_archive() {
        assert!(matches!(
            open(&mut Cursor::new(b"junk".to_vec())),
            Err(OpenError::NoEnd)
        ));
        assert!(matches!(
            open(&mut Cursor::new(vec![0_u8; 100])),
            Err(OpenError::NoEnd)
        ));
    }
}
