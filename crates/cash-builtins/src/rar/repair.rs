//! `r`: a damaged archive repaired, as rar does it. With a recovery record, the shards
//! whose checksums fail are mended into `fixed.NAME`; without one, the archive is
//! rebuilt into `rebuilt.NAME` from every block whose header still checks, damaged
//! data and all, the blocks after a broken header found by looking for the next good
//! one.

use std::io::Write as _;
use std::path::PathBuf;

use super::cmdline::{Name, Parsed};
use super::open::{self, Found};
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
    let area = if rar.switches.no_percent {
        ""
    } else {
        "      "
    };
    let opened = open::open(rar, found)?.ok();
    let rar5 = opened
        .as_ref()
        .and_then(|opened| opened.archive.as_rar50())
        .filter(|archive| archive.main.has_recovery_record());
    if let Some(archive) = rar5 {
        let password = rar.password.borrow().clone();
        let password = password.as_deref().map(str::as_bytes);
        rar.console
            .msg(&format!("\nSearching for recovery record{area}"));
        rar.console.msg("\nData recovery record found");
        rar.console.msg(&format!("\nAnalyzing file data{area}"));
        let shown = format!("{folder}fixed.{name}");
        rar.console.msg(&format!("\nBuilding {shown}{area}"));
        let damaged = archive
            .recovery_damaged_ranges(password)
            .unwrap_or_default();
        for range in &damaged {
            let start = range.start as u64;
            rar.console.msg(&format!(
                "\nCorrupt {} bytes at {:08x} {:08x} - data recovered     ",
                range.len(),
                start >> 32,
                start & 0xFFFF_FFFF
            ));
        }
        if !damaged.is_empty() {
            let mut mended = Vec::new();
            if let Err(error) = archive.repair_recovery_to_with_report(&mut mended, password) {
                rar.console.err(&format!("\n{error}"));
                rar.fail(code::CRC);
            } else {
                // Only the damaged shards change: the recovery record stays as it is,
                // damaged or not.
                let mut fixed = bytes;
                for range in &damaged {
                    if let (Some(to), Some(from)) =
                        (fixed.get_mut(range.clone()), mended.get(range.clone()))
                    {
                        to.copy_from_slice(from);
                    }
                }
                write(rar, &shown, &fixed);
            }
        }
        rar.console.msg(&format!(
            "\n{} blocks are recovered, 0 blocks are relocated",
            damaged.len()
        ));
        if !archive.recovery_record_intact(password).unwrap_or(false) {
            rar.console.msg("\nRecovery record is corrupt.");
            rar.fail(code::CRC);
        }
        done(rar);
        return Ok(());
    }
    rar.console.msg("\nData recovery record not found");
    rar.console
        .msg(&format!("\nReconstructing {}", found.display));
    let shown = format!("{folder}rebuilt.{name}");
    rar.console.msg(&format!("\nBuilding {shown}{area}"));
    let (rebuilt, names) = rebuild(&bytes);
    for found_name in &names {
        rar.console.msg(&format!("\nFound  {found_name}     "));
    }
    if let Some(rebuilt) = rebuilt {
        write(rar, &shown, &rebuilt);
    }
    done(rar);
    Ok(())
}

fn done<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>) {
    if !rar.switches.no_done {
        rar.console.msg("\nDone\n");
    }
}

fn write<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>, shown: &str, bytes: &[u8]) {
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
/// found; `None` when it is not a RAR 1.5 to 7 archive.
fn rebuild(bytes: &[u8]) -> (Option<Vec<u8>>, Vec<String>) {
    if bytes.starts_with(RAR5_SIGNATURE) {
        rebuild5(bytes)
    } else if bytes.starts_with(RAR4_SIGNATURE) {
        rebuild4(bytes)
    } else {
        (None, Vec::new())
    }
}

const RAR5_SIGNATURE: &[u8] = b"Rar!\x1a\x07\x01\x00";
const RAR4_SIGNATURE: &[u8] = b"Rar!\x1a\x07\x00";

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
fn rebuild5(bytes: &[u8]) -> (Option<Vec<u8>>, Vec<String>) {
    let mut out = RAR5_SIGNATURE.to_vec();
    let mut names = Vec::new();
    let mut at = RAR5_SIGNATURE.len();
    if let Some((1, len, _)) = block5(bytes, at) {
        out.extend_from_slice(&bytes[at..at + len]);
        at += len;
    } else {
        // Size 3: the main type, no header flags, no archive flags.
        let header = [0x03, 0x01, 0x00, 0x00];
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
