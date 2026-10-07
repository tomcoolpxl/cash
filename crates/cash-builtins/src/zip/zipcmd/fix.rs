//! `-F` and `-FF`: a damaged archive's members copied to `--out`, as zip 3.0 finds them.
//!
//! `-F` trusts the central directory and corrects the offsets by what was put before the
//! archive. `-FF` trusts nothing: it scans the file for local headers, takes from the
//! central directory what it can still read, and writes a new one.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

use cash_archive::zip::read::{self, OpenError};
use cash_archive::zip::write::Writer;
use cash_archive::zip::{
    CENTRAL_SIGNATURE, DESCRIPTOR_SIGNATURE, DosTime, END_SIGNATURE, Entry, LOCAL_SIGNATURE,
    MADE_BY_ZIP_3_UNIX, extra_id, fields,
};
use cash_core::ExecutionResult;
use cash_win32::unix::Replacement;

use super::super::{Source as ArchiveSource, name_text, read_line};
use super::{Zip, code};
use crate::compress::strerror;

fn le16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([
        bytes.get(at).copied().unwrap_or(0),
        bytes.get(at + 1).copied().unwrap_or(0),
    ])
}

fn le32(bytes: &[u8], at: usize) -> u32 {
    u32::from(le16(bytes, at)) | u32::from(le16(bytes, at + 2)) << 16
}

fn le64(bytes: &[u8], at: usize) -> u64 {
    u64::from(le32(bytes, at)) | u64::from(le32(bytes, at + 4)) << 32
}

/// A member found by its local header: where it starts, its header's length, all it
/// takes (data and descriptor), and its record.
struct Found {
    start: u64,
    header_len: u64,
    total: u64,
    entry: Entry,
}

/// A local header at `at`, with its data and descriptor, when it holds together.
fn local_at(bytes: &[u8], at: usize) -> Option<Found> {
    let fixed = bytes.get(at..at + 30)?;
    let name_len = usize::from(le16(fixed, 26));
    let extra_len = usize::from(le16(fixed, 28));
    let name = bytes.get(at + 30..at + 30 + name_len)?.to_vec();
    let extra = bytes
        .get(at + 30 + name_len..at + 30 + name_len + extra_len)?
        .to_vec();
    let header_len = 30 + name_len + extra_len;
    let data = at + header_len;
    let flags = le16(fixed, 6);
    let mut crc = le32(fixed, 14);
    let mut compressed = u64::from(le32(fixed, 18));
    let mut size = u64::from(le32(fixed, 22));
    if let Some((_, zip64)) = fields(&extra)
        .into_iter()
        .find(|(id, _)| *id == extra_id::ZIP64)
    {
        if size == 0xffff_ffff && zip64.len() >= 8 {
            size = le64(zip64, 0);
        }
        if compressed == 0xffff_ffff && zip64.len() >= 16 {
            compressed = le64(zip64, 8);
        }
    }
    let mut total = header_len as u64 + compressed;
    if flags & 0x0008 != 0 && compressed == 0 {
        // The sizes are in the descriptor: the first whose size is the distance to it.
        let signature = DESCRIPTOR_SIGNATURE.to_le_bytes();
        let mut look = data;
        loop {
            let offset = bytes.get(look..)?.windows(4).position(|w| w == signature)?;
            let found = look + offset;
            if le32(bytes, found + 8) as usize == found - data {
                crc = le32(bytes, found + 4);
                compressed = u64::from(le32(bytes, found + 8));
                size = u64::from(le32(bytes, found + 12));
                total = (found + 16 - at) as u64;
                break;
            }
            look = found + 1;
        }
    } else if flags & 0x0008 != 0 {
        let after = data + usize::try_from(compressed).ok()?;
        let signed = le32(bytes, after) == DESCRIPTOR_SIGNATURE;
        total += if signed { 16 } else { 12 };
    }
    if at as u64 + total > bytes.len() as u64 {
        return None;
    }
    Some(Found {
        start: at as u64,
        header_len: header_len as u64,
        total,
        entry: Entry {
            version_made_by: MADE_BY_ZIP_3_UNIX,
            version_needed: le16(fixed, 4),
            flags,
            method: le16(fixed, 8),
            time: DosTime {
                time: le16(fixed, 10),
                date: le16(fixed, 12),
            },
            crc,
            compressed_size: compressed,
            size,
            name,
            extra,
            ..Entry::default()
        },
    })
}

/// A central directory record at `at`: the record and its length.
fn central_at(bytes: &[u8], at: usize) -> Option<(Entry, usize)> {
    let fixed = bytes.get(at..at + 46)?;
    let name_len = usize::from(le16(fixed, 28));
    let extra_len = usize::from(le16(fixed, 30));
    let comment_len = usize::from(le16(fixed, 32));
    let rest = bytes.get(at + 46..at + 46 + name_len + extra_len + comment_len)?;
    Some((
        Entry {
            version_made_by: le16(fixed, 4),
            internal_attributes: le16(fixed, 36),
            external_attributes: le32(fixed, 38),
            name: rest.get(..name_len)?.to_vec(),
            extra: rest.get(name_len..name_len + extra_len)?.to_vec(),
            comment: rest.get(name_len + extra_len..)?.to_vec(),
            ..Entry::default()
        },
        46 + name_len + extra_len + comment_len,
    ))
}

impl<SE: cash_core::ShellExtensions> Zip<'_, SE> {
    /// `-F` or `-FF` on `path`, written to `--out`.
    pub(super) fn fix(
        &self,
        zipfile: &str,
        path: &Path,
    ) -> Result<ExecutionResult, cash_core::Error> {
        let Some(output) = self.options.output.clone() else {
            self.say.err(
                "\tzip warning: fix options -F and -FF require --out:\n                     zip -F indamagedarchive --out outfixedarchive\n",
            )?;
            return self.fail(
                "Invalid command arguments (fix options require --out)",
                code::PARAM,
            );
        };
        let target = self.context.shell.absolute_path(&output);
        let mut file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e) => {
                self.say.err(&format!("zip I/O error: {}", strerror(&e)))?;
                return self.fail(
                    &format!("Could not open input archive ({zipfile})"),
                    code::OPEN,
                );
            }
        };
        let mut replacement = match Replacement::create(target) {
            Ok(replacement) => replacement,
            Err(e) => {
                self.say.err(&format!("zip I/O error: {}", strerror(&e)))?;
                return self.fail(
                    &format!("Could not create output file ({output})"),
                    code::CREATE,
                );
            }
        };
        let mut writer = Writer::new(replacement.file());
        let comment = if self.options.fix == 1 {
            self.note("Fix archive (-F) - assume mostly intact archive\n")?;
            let archive = match read::open(&mut file) {
                Ok(archive) => archive,
                Err(OpenError::NoEnd) => {
                    replacement.abandon();
                    self.say.err("\tzip warning: bad archive - missing end signature\n\tzip warning: (If downloaded, was binary mode used?  If not, the\n\tzip warning:  archive may be scrambled and not recoverable)\n\tzip warning: Can't use -F to fix (try -FF)\n")?;
                    return self.fail(
                        &format!("Zip file structure invalid ({zipfile})"),
                        code::FORMAT,
                    );
                }
                Err(OpenError::BadCentral(number, total)) => {
                    replacement.abandon();
                    self.say.err(&format!(
                        "\tzip warning: expected {total} entries but found {}\n",
                        number - 1
                    ))?;
                    return self.fail(
                        &format!("Zip file structure invalid ({zipfile})"),
                        code::FORMAT,
                    );
                }
                Err(OpenError::Io(e)) => return Err(e.into()),
            };
            let (mut source, archive) = super::super::parts_source(path, &archive)
                .unwrap_or((ArchiveSource::File(file), archive));
            if archive.extra_bytes == 0 {
                self.note("Zip entry offsets do not need adjusting\n")?;
            } else {
                self.note(&format!(
                    "Zip entry offsets appear off by {} bytes - correcting...\n",
                    archive.extra_bytes
                ))?;
            }
            for entry in &archive.entries {
                self.note(&format!(" copying: {}\n", name_text(entry)))?;
                if writer.copy(&mut source, &archive, entry).is_err() {
                    self.say.err(&format!(
                        "\tzip warning: could not copy {}\n",
                        name_text(entry)
                    ))?;
                }
            }
            archive.end.comment.clone()
        } else {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            self.salvage(&bytes, &mut writer)?
        };
        writer.finish(&comment)?;
        if let Err(e) = replacement.finish(fs::FileTimes::new(), false, false) {
            self.say.err(&format!("zip I/O error: {}", strerror(&e)))?;
            return self.fail(
                &format!("Could not create output file ({output})"),
                code::CREATE,
            );
        }
        Ok(ExecutionResult::new(self.status))
    }

    /// `-FF`: the members found by scanning `bytes`, with what the central directory
    /// still says of them; the archive's comment.
    fn salvage<O: cash_archive::zip::write::Output>(
        &self,
        bytes: &[u8],
        writer: &mut Writer<O>,
    ) -> io::Result<Vec<u8>> {
        self.note("Fix archive (-FF) - salvage what can\n")?;
        let end_signature = END_SIGNATURE.to_le_bytes();
        let end = bytes
            .windows(4)
            .enumerate()
            .rev()
            .find(|(i, w)| *w == end_signature && bytes.len() - i >= 22)
            .map(|(i, _)| i);
        if let Some(at) = end {
            let disks = le16(bytes, at + 4);
            if disks == 0 {
                self.note(" Found end record (EOCDR) - says expect single disk archive\n")?;
            } else {
                self.note(&format!(
                    " Found end record (EOCDR) - says expect {} disks\n",
                    disks + 1
                ))?;
            }
        } else {
            self.say.err("\tzip warning: Missing end (EOCDR) signature - either this archive\n                     is not readable or the end is damaged\n")?;
            self.note("Is this a single-disk archive?  (y/n): ")?;
            let _ = read_line(self.context, true)?;
            self.note("  Assuming single-disk archive\n")?;
        }
        self.note("Scanning for entries...\n")?;
        let mut found: Vec<Found> = Vec::new();
        let mut central: HashMap<Vec<u8>, Entry> = HashMap::new();
        let mut comment = Vec::new();
        let mut central_seen = false;
        let mut at = 0;
        while at + 4 <= bytes.len() {
            if bytes.get(at..at + 2) != Some(b"PK") {
                at += 1;
                continue;
            }
            match le32(bytes, at) {
                LOCAL_SIGNATURE => {
                    if let Some(member) = local_at(bytes, at) {
                        self.note(&format!(
                            " copying: {}  ({} bytes)\n",
                            String::from_utf8_lossy(&member.entry.name),
                            member.entry.compressed_size
                        ))?;
                        at += usize::try_from(member.total).unwrap_or(1).max(1);
                        found.push(member);
                        continue;
                    }
                }
                CENTRAL_SIGNATURE => {
                    if !central_seen {
                        central_seen = true;
                        self.note("Central Directory found...\n")?;
                    }
                    if let Some((record, len)) = central_at(bytes, at) {
                        central.insert(record.name.clone(), record);
                        at += len;
                        continue;
                    }
                    self.say.err("\tzip warning: error reading entry:  Invalid argument\n\tzip warning: skipping this entry...\n")?;
                }
                END_SIGNATURE if bytes.len() - at >= 22 => {
                    self.note(&format!(
                        "EOCDR found ({:2} {:6})...\n",
                        le16(bytes, at + 4) + 1,
                        at
                    ))?;
                    let len = usize::from(le16(bytes, at + 20));
                    comment = bytes
                        .get(at + 22..at + 22 + len)
                        .unwrap_or_default()
                        .to_vec();
                    break;
                }
                _ => {}
            }
            at += 1;
        }
        let mut source = io::Cursor::new(bytes);
        for mut member in found {
            if let Some(record) = central.remove(&member.entry.name) {
                member.entry.version_made_by = record.version_made_by;
                member.entry.internal_attributes = record.internal_attributes;
                member.entry.external_attributes = record.external_attributes;
                member.entry.extra = record.extra;
                member.entry.comment = record.comment;
            }
            if writer
                .copy_span(
                    &mut source,
                    member.start,
                    member.header_len,
                    member.total,
                    &mut member.entry,
                )
                .is_ok()
            {
                writer.push_entry(member.entry);
            }
        }
        Ok(comment)
    }
}
