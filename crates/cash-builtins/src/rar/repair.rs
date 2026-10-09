//! `r`: a damaged archive repaired, as rar does it. With a recovery record, the shards
//! whose checksums fail are mended into `fixed.NAME`; without one, the archive is
//! rebuilt into `rebuilt.NAME` from every block whose header still checks, damaged
//! data and all, the blocks after a broken header found by looking for the next good
//! one.

use std::io::Write as _;
use std::path::PathBuf;

use super::cmdline::{Name, Parsed};
use cash_archive::rar::{rar50, recovery};

use super::open::{self, Failure, Found};
use super::{Rar, Stop, code};

pub(super) fn run<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    parsed: &Parsed,
) -> Result<(), Stop> {
    let Some(archive) = parsed.archive.as_deref() else {
        return Ok(());
    };
    // `destpath\`: the folder the result goes in, as typed.
    let folder = match parsed.names.last() {
        Some(Name::Plain(last)) if last.ends_with(['/', '\\']) => last.replace('\\', "/"),
        _ => String::new(),
    };
    for found in open::find(rar, archive) {
        repair(rar, &found, &folder)?;
    }
    Ok(())
}

fn repair<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    found: &Found,
    folder: &str,
) -> Result<(), Stop> {
    let bytes = match std::fs::read(&found.path) {
        Ok(bytes) => bytes,
        Err(error) => {
            rar.console.msg("\n");
            open::report_missing(rar, found, &error);
            if !rar.switches.no_done {
                rar.console.msg("Done\n");
            }
            return Ok(());
        }
    };
    let name = found
        .display
        .rsplit('/')
        .next()
        .unwrap_or(&found.display)
        .to_owned();
    let area = area(rar);
    // rar asks for no password here: headers it cannot read are searched for a
    // recovery record by its chunks' marks.
    let given = rar.given_password()?;
    let opened = match open::try_open(rar, found, given.as_deref()) {
        Ok(opened) => opened,
        Err(failure @ (Failure::WrongPassword(_) | Failure::NotRar)) => {
            rar.console
                .msg(&format!("\nSearching for recovery record{area}"));
            match recovery::rar5::recovery_by_marks(&bytes) {
                Ok(Some(marked)) => {
                    let mended = marked
                        .mended
                        .map(|mended| marked.damaged.iter().cloned().zip(mended).collect());
                    if !mend(
                        rar,
                        folder,
                        &name,
                        bytes,
                        &marked.damaged,
                        mended,
                        marked.intact,
                    ) {
                        done(rar);
                    }
                }
                _ if matches!(failure, Failure::NotRar) => unreadable(rar, found, folder, &name),
                _ => {
                    rar.console.msg("\nData recovery record not found");
                    rar.console.err("\nNo files found");
                    rar.fail(code::NO_FILES);
                    done(rar);
                }
            }
            return Ok(());
        }
        // A main header that does not check: its flags unknown, the record is
        // looked for by its marks.
        Err(_) if main_header_damaged(&bytes) => {
            rar.console
                .msg(&format!("\nSearching for recovery record{area}"));
            if let Ok(Some(marked)) = recovery::rar5::recovery_by_marks(&bytes) {
                let mended = marked
                    .mended
                    .map(|mended| marked.damaged.iter().cloned().zip(mended).collect());
                if !mend(
                    rar,
                    folder,
                    &name,
                    bytes.clone(),
                    &marked.damaged,
                    mended,
                    marked.intact,
                ) {
                    reconstruct(rar, found, folder, &name, &bytes, false)?;
                }
            } else {
                reconstruct(rar, found, folder, &name, &bytes, true)?;
            }
            return Ok(());
        }
        Err(_) => {
            reconstruct(rar, found, folder, &name, &bytes, true)?;
            return Ok(());
        }
    };
    let rar5 = opened
        .archive
        .as_rar50()
        .filter(|archive| archive.main.has_recovery_record());
    if let Some(archive) = rar5 {
        let password = given.as_deref().map(str::as_bytes);
        rar.console
            .msg(&format!("\nSearching for recovery record{area}"));
        let (damaged, mended, intact, base) = recovered(archive, password, &bytes);
        // Damage the record cannot mend is said, and the archive rebuilt instead.
        if !mend(rar, folder, &name, base, &damaged, mended, intact) {
            reconstruct(rar, found, folder, &name, &bytes, false)?;
        }
        return Ok(());
    }
    // Encrypted headers are not rebuilt, read or not.
    if opened.facts.encrypted_headers {
        rar.console.msg("\nData recovery record not found");
        done(rar);
        return Ok(());
    }
    reconstruct(rar, found, folder, &name, &bytes, true)
}

/// What a recovery record mends.
type Recovered = (
    Vec<std::ops::Range<usize>>,
    Option<Vec<(std::ops::Range<usize>, Vec<u8>)>>,
    bool,
    Vec<u8>,
);

/// What the recovery record of a RAR 5 archive read mends: the damaged shards, each
/// with its bytes when it mends them all, whether the record is whole, and the archive
/// the mended shards go into. A record whose header failed its checksum is left out of
/// that archive, which ends where it began, as rar writes it; one whose header does not
/// read at all is found by its chunks' marks.
fn recovered(archive: &rar50::Archive, password: Option<&[u8]>, bytes: &[u8]) -> Recovered {
    let Ok(damaged) = archive.recovery_damaged_shards(password) else {
        return match recovery::rar5::recovery_by_marks(bytes) {
            Ok(Some(marked)) => {
                let mended = marked
                    .mended
                    .map(|mended| marked.damaged.iter().cloned().zip(mended).collect());
                let prefix = bytes.get(..marked.protected).unwrap_or(bytes);
                let base = [prefix, &RAR5_END[..]].concat();
                (marked.damaged, mended, marked.intact, base)
            }
            _ => (Vec::new(), Some(Vec::new()), false, bytes.to_vec()),
        };
    };
    let mut repaired = Vec::new();
    let mended = if damaged.is_empty() {
        Some(Vec::new())
    } else {
        archive
            .repair_recovery_to_with_report(&mut repaired, password)
            .ok()
            .map(|_| {
                damaged
                    .iter()
                    .filter_map(|range| {
                        Some((range.clone(), repaired.get(range.clone())?.to_vec()))
                    })
                    .collect()
            })
    };
    let intact = archive.recovery_record_intact(password).unwrap_or(false);
    let damaged_header = archive.blocks.iter().find_map(|block| match block {
        rar50::Block::Service(header) if header.name == b"RR" && header.block.damaged => {
            Some(header.block.offset)
        }
        _ => None,
    });
    let base = match damaged_header.and_then(|at| bytes.get(..at)) {
        Some(prefix) => [prefix, &RAR5_END[..]].concat(),
        None => bytes.to_vec(),
    };
    (damaged, mended, intact, base)
}

/// The recovery record found: the damaged shards, said each, and when it mends them
/// (`mended`, each range with its bytes), written into `fixed.NAME` with the rest as
/// it was; and whether the record itself is whole. Whether it mended them: else
/// nothing is written and the run goes on.
fn mend<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    folder: &str,
    name: &str,
    bytes: Vec<u8>,
    damaged: &[std::ops::Range<usize>],
    mended: Option<Vec<(std::ops::Range<usize>, Vec<u8>)>>,
    intact: bool,
) -> bool {
    let area = area(rar);
    rar.console.msg("\nData recovery record found");
    rar.console.msg(&format!("\nAnalyzing file data{area}"));
    let shown = format!("{folder}fixed.{name}");
    rar.console.msg(&format!("\nBuilding {shown}{area}"));
    let outcome = if mended.is_some() {
        "data recovered"
    } else {
        "cannot recover data"
    };
    for range in damaged {
        let start = range.start as u64;
        rar.console.msg(&format!(
            "\nCorrupt {} bytes at {:08x} {:08x} - {outcome}     ",
            range.len(),
            start >> 32,
            start & 0xFFFF_FFFF
        ));
    }
    let Some(mended) = mended else {
        rar.console
            .msg("\n0 blocks are recovered, 0 blocks are relocated");
        return false;
    };
    if !mended.is_empty() {
        // Only the damaged shards change: the recovery record stays as it is, damaged
        // or not.
        let mut fixed = bytes;
        for (range, mended) in &mended {
            if let Some(to) = fixed.get_mut(range.clone())
                && to.len() == mended.len()
            {
                to.copy_from_slice(mended);
            }
        }
        write(rar, &shown, &fixed);
    }
    rar.console.msg(&format!(
        "\n{} blocks are recovered, 0 blocks are relocated",
        mended.len()
    ));
    if !intact {
        rar.console.msg("\nRecovery record is corrupt.");
        rar.fail(code::CRC);
    }
    done(rar);
    true
}

/// No recovery record, or one that cannot mend (said before, `not_found` false):
/// the archive rebuilt into `rebuilt.NAME` from the blocks whose headers check.
fn reconstruct<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    found: &Found,
    folder: &str,
    name: &str,
    bytes: &[u8],
    not_found: bool,
) -> Result<(), Stop> {
    if not_found {
        rar.console.msg("\nData recovery record not found");
    }
    rar.console
        .msg(&format!("\nReconstructing {}", found.display));
    let shown = format!("{folder}rebuilt.{name}");
    rar.console.msg(&format!("\nBuilding {shown}{}", area(rar)));
    // A main header that does not check is said corrupt, twice, and rar asks whether
    // the archive it rebuilds is solid.
    let solid = if main_header_damaged(bytes) {
        for _ in 0..2 {
            rar.console
                .err("\nCorrupt header is found\nMain archive header is corrupt");
        }
        rar.fail(code::CRC);
        ask_solid(rar)?
    } else {
        false
    };
    let (rebuilt, names) = rebuild(bytes, solid);
    for found_name in &names {
        rar.console.msg(&format!("\nFound  {found_name}     "));
    }
    if let Some(rebuilt) = rebuilt {
        write(rar, &shown, &rebuilt);
    }
    done(rar);
    Ok(())
}

/// Whether a RAR 5 archive's main header fails its checksum or is not one.
fn main_header_damaged(bytes: &[u8]) -> bool {
    bytes.starts_with(RAR5_SIGNATURE)
        && !matches!(block5(bytes, RAR5_SIGNATURE.len()), Some((1, _, _)))
}

/// rar's question when the main header is corrupt: whether to mark the archive solid,
/// Yes alone saying so, whatever `-y` says.
fn ask_solid<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>) -> Result<bool, Stop> {
    rar.console
        .err("\nThe archive header is corrupt. Mark archive as solid? [Y]es, [N]o ");
    let Some(answer) = rar.read_answer(true)? else {
        rar.console.err("Read error in the file stdin");
        if let Some(extra) = rar.stdin_end_words(false) {
            rar.console.err(&format!("\n{extra}"));
        }
        return Err(Stop::Aborted(code::READ));
    };
    Ok(answer.trim_start().starts_with(['y', 'Y']))
}

/// A file that is no RAR archive, with no recovery record in it either: rar searches
/// again, tries to rebuild it, finds a header to be corrupt and no file, and writes
/// nothing.
fn unreadable<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    found: &Found,
    folder: &str,
    name: &str,
) {
    rar.console.msg("\nData recovery record not found");
    rar.console.msg("\nSearching for recovery record");
    rar.console.msg("\nData recovery record not found");
    rar.console
        .msg(&format!("\nReconstructing {}", found.display));
    rar.console
        .msg(&format!("\nBuilding {folder}rebuilt.{name}{}", area(rar)));
    rar.console.err(
        "\nUnexpected end of archive\n - the file header is corrupt\nUnexpected end of archive\nNo files found",
    );
    rar.fail(code::NO_FILES);
    done(rar);
}

/// The place of a percentage after a line: none with `-idp`.
const fn area<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>) -> &'static str {
    if rar.switches.no_percent {
        ""
    } else {
        "      "
    }
}

pub(super) fn done<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>) {
    if !rar.switches.no_done {
        rar.console.msg("\nDone\n");
    }
}

pub(super) fn write<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>, shown: &str, bytes: &[u8]) {
    let path: PathBuf = rar.path(shown);
    let written = std::fs::File::create(&path).and_then(|mut file| file.write_all(bytes));
    if let Err(error) = written {
        rar.console.err(&format!(
            "\nCannot create {shown}\n{}",
            open::system_message(&error)
        ));
        rar.fail(code::CREATE);
    }
}

/// An archive rebuilt from the blocks whose headers check, and the names of the files
/// found; `None` when it is not a RAR 1.5 to 7 archive. A RAR 5 main header that
/// does not check is written anew, `solid` or not.
fn rebuild(bytes: &[u8], solid: bool) -> (Option<Vec<u8>>, Vec<String>) {
    if bytes.starts_with(RAR5_SIGNATURE) {
        rebuild5(bytes, solid)
    } else if bytes.starts_with(RAR4_SIGNATURE) {
        rebuild4(bytes)
    } else {
        (None, Vec::new())
    }
}

const RAR5_SIGNATURE: &[u8] = b"Rar!\x1a\x07\x01\x00";
const RAR4_SIGNATURE: &[u8] = b"Rar!\x1a\x07\x00";
/// RAR 5's end header as `WinRAR` writes it: no more volumes.
const RAR5_END: [u8; 8] = [0x1d, 0x77, 0x56, 0x51, 0x03, 0x05, 0x04, 0x00];

/// A RAR 5 vint at `at`: its value and the position after it.
fn vint(bytes: &[u8], mut at: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let byte = *bytes.get(at)?;
        at += 1;
        value |= u64::from(byte & 0x7f).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some((value, at));
        }
    }
    None
}

/// A RAR 5 block at `at` whose header CRC holds: its type, its whole length with its
/// data, and its file name when it is a file's.
fn block5(bytes: &[u8], at: usize) -> Option<(u64, usize, Option<String>)> {
    let crc = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?);
    let (size, body) = vint(bytes, at + 4)?;
    let size = usize::try_from(size)
        .ok()
        .filter(|&s| s > 0 && s < 1 << 21)?;
    let end = body.checked_add(size)?;
    if crc32fast::hash(bytes.get(at + 4..end)?) != crc {
        return None;
    }
    let (kind, next) = vint(bytes, body)?;
    let (flags, mut next) = vint(bytes, next)?;
    if flags & 1 != 0 {
        next = vint(bytes, next)?.1;
    }
    let data = if flags & 2 != 0 {
        let (data, after) = vint(bytes, next)?;
        next = after;
        usize::try_from(data).ok()?
    } else {
        0
    };
    let name = if kind == 2 {
        let (file_flags, after) = vint(bytes, next)?;
        let after = vint(bytes, after)?.1; // unpacked size
        let mut after = vint(bytes, after)?.1; // attributes
        if file_flags & 2 != 0 {
            after += 4;
        }
        if file_flags & 4 != 0 {
            after += 4;
        }
        let after = vint(bytes, after)?.1; // compression
        let after = vint(bytes, after)?.1; // host
        let (len, after) = vint(bytes, after)?;
        let name = bytes.get(after..after + usize::try_from(len).ok()?)?;
        Some(String::from_utf8_lossy(name).into_owned())
    } else {
        None
    };
    Some((kind, end.checked_add(data)?.checked_sub(at)?, name))
}

/// RAR 5: the main header as it is, then each file and service block found, the
/// quick-open record among them, and no end header, as rar rebuilds it. A main header
/// that does not check is replaced by a plain one.
fn rebuild5(bytes: &[u8], solid: bool) -> (Option<Vec<u8>>, Vec<String>) {
    let mut out = RAR5_SIGNATURE.to_vec();
    let mut names = Vec::new();
    let mut at = RAR5_SIGNATURE.len();
    if let Some((1, len, _)) = block5(bytes, at) {
        out.extend_from_slice(&bytes[at..at + len]);
        at += len;
    } else {
        // Size 3: the main type, header flags 4 (skip it if unknown), and the
        // archive's flags: solid or none, as rar writes it.
        let header = [0x03, 0x01, 0x04, if solid { 0x04 } else { 0x00 }];
        out.extend_from_slice(&crc32fast::hash(&header).to_le_bytes());
        out.extend_from_slice(&header);
    }
    while at < bytes.len() {
        match block5(bytes, at) {
            Some((kind, len, name)) if at + len <= bytes.len() => {
                if kind == 5 {
                    break;
                }
                if kind == 2 || kind == 3 {
                    out.extend_from_slice(&bytes[at..at + len]);
                }
                if let Some(name) = name {
                    names.push(name.replace('\\', "/"));
                }
                at += len;
            }
            _ => at += 1,
        }
    }
    (Some(out), names)
}

/// RAR 1.5 to 4: the main header, then each file and sub-block whose header CRC holds.
fn rebuild4(bytes: &[u8]) -> (Option<Vec<u8>>, Vec<String>) {
    let mut out = RAR4_SIGNATURE.to_vec();
    let mut names = Vec::new();
    let mut at = RAR4_SIGNATURE.len();
    while at < bytes.len() {
        match block4(bytes, at) {
            Some((kind, len, name)) if at + len <= bytes.len() => {
                if matches!(kind, 0x73 | 0x74 | 0x7a) {
                    out.extend_from_slice(&bytes[at..at + len]);
                }
                if let Some(name) = name {
                    names.push(name.replace('\\', "/"));
                }
                if kind == 0x7b {
                    break;
                }
                at += len;
            }
            _ => at += 1,
        }
    }
    (Some(out), names)
}

/// A RAR 1.5 to 4 block at `at` whose header CRC holds: its type, its length with its
/// data, and a file's name.
fn block4(bytes: &[u8], at: usize) -> Option<(u8, usize, Option<String>)> {
    let crc = u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?);
    let kind = *bytes.get(at + 2)?;
    let flags = u16::from_le_bytes(bytes.get(at + 3..at + 5)?.try_into().ok()?);
    let size = usize::from(u16::from_le_bytes(
        bytes.get(at + 5..at + 7)?.try_into().ok()?,
    ));
    if size < 7 || !(0x72..=0x7b).contains(&kind) {
        return None;
    }
    let header = bytes.get(at + 2..at + size)?;
    if (crc32fast::hash(header) & 0xffff) as u16 != crc {
        return None;
    }
    let mut data = 0u64;
    if flags & 0x8000 != 0 || kind == 0x74 || kind == 0x7a {
        data = u64::from(u32::from_le_bytes(
            bytes.get(at + 7..at + 11)?.try_into().ok()?,
        ));
        if kind != 0x73 && flags & 0x0100 != 0 {
            let high = u32::from_le_bytes(bytes.get(at + 32..at + 36)?.try_into().ok()?);
            data |= u64::from(high) << 32;
        }
    }
    let name = (kind == 0x74).then(|| {
        let len = usize::from(u16::from_le_bytes(
            bytes
                .get(at + 26..at + 28)
                .and_then(|b| b.try_into().ok())
                .unwrap_or([0, 0]),
        ));
        let start = at + 32 + if flags & 0x0100 != 0 { 8 } else { 0 };
        let raw = bytes.get(start..start + len).unwrap_or_default();
        let raw = raw.split(|&b| b == 0).next().unwrap_or_default();
        raw.iter().map(|&b| char::from(b)).collect::<String>()
    });
    let len = size.checked_add(usize::try_from(data).ok()?)?;
    Some((kind, len, name))
}
