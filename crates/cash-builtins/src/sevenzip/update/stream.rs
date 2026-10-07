//! The gzip, bzip2 and xz handlers' update (`GzHandler.cpp`, `Bz2Handler.cpp`,
//! `XzHandler.cpp`): one item, compressed anew from the disk, or the old stream copied
//! as it is (a gzip stream renamed gets a new header over its old data). gzip's header
//! names the file and gives its time in Unix seconds, MS-DOS's host and the level's
//! extra flags; xz's blocks are checked with CRC32 unless `-mcrc` says otherwise.

use std::io::{self, Read, Seek, SeekFrom, Write};
use std::num::NonZeroU64;

use cash_archive::codec::{self, Codec};

use super::super::archive::{Kind, Opened};
use super::super::methods::{self, MethodError};
use super::{Item, Job, Out, Stop, Warnings, progress, win, win_error};

/// FILETIME ticks at the Unix epoch, and in a second.
const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
const TICKS_PER_SECOND: u64 = 10_000_000;

/// gzip's flag for a name, and the host 7-Zip writes on Windows (`kHostOS`, FAT).
const FNAME: u8 = 8;
const HOST_FAT: u8 = 0;

/// What `-m` sets for the stream formats.
pub(super) struct Settings {
    level: u32,
    /// gzip's `-mtm`: whether the time goes into the header.
    mtime: bool,
    /// bzip2's block size in 100 kB.
    block: u32,
    /// xz's settings, as 7z's are parsed.
    xz: Option<methods::Settings>,
}

fn boolean(value: Option<&str>) -> Result<bool, MethodError> {
    match value.map(str::to_ascii_lowercase).as_deref() {
        None | Some("" | "on" | "+") => Ok(true),
        Some("off" | "-") => Ok(false),
        _ => Err(MethodError::Invalid),
    }
}

/// `ParsePropToUInt32`: the number after the name, else the value's, else none.
fn number(rest: &str, value: Option<&str>) -> Result<Option<u32>, MethodError> {
    let text = match (rest, value) {
        ("", None) => return Ok(None),
        ("", Some(v)) => v,
        (r, None) => r,
        _ => return Err(MethodError::Invalid),
    };
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(MethodError::Invalid);
    }
    text.parse().map(Some).map_err(|_| MethodError::Invalid)
}

impl Settings {
    /// The handler's `SetProperties`: `CSingleMethodProps` for gzip (with `-mtm` and
    /// `-mtp`, never `-mtc` or `-mta`) and bzip2, `CMultiMethodProps` for xz.
    pub(super) fn parse(
        kind: Kind,
        properties: &[(String, Option<String>)],
    ) -> Result<Self, MethodError> {
        let mut settings = Self {
            level: 5,
            mtime: true,
            block: 9,
            xz: None,
        };
        if kind == Kind::Xz {
            settings.xz = Some(methods::Settings::parse(properties)?);
            return Ok(settings);
        }
        let mut params: Vec<(String, String)> = Vec::new();
        for (name, value) in properties {
            let name = name.to_ascii_lowercase();
            let value = value.as_deref();
            if name.is_empty() {
                return Err(MethodError::Invalid);
            }
            if kind == Kind::Gzip {
                match name.as_str() {
                    "tm" => {
                        settings.mtime = boolean(value)?;
                        continue;
                    }
                    "ta" | "tc" => {
                        if boolean(value)? {
                            return Err(MethodError::Invalid);
                        }
                        continue;
                    }
                    _ => {}
                }
                if let Some(rest) = name.strip_prefix("tp") {
                    match number(rest, value)?.unwrap_or(0) {
                        0 | 1 | 3 | 16 => continue,
                        _ => return Err(MethodError::Invalid),
                    }
                }
            }
            if let Some(rest) = name.strip_prefix('x') {
                settings.level = number(rest, value)?.unwrap_or(9).min(9);
                continue;
            }
            if name.starts_with("mt") || name.starts_with("memuse") {
                continue;
            }
            let known: &[&str] = if kind == Kind::Gzip {
                &["a", "fb", "pass", "mc"]
            } else {
                &["d", "pass"]
            };
            let key: String = name.chars().take_while(char::is_ascii_alphabetic).collect();
            if !known.contains(&key.as_str()) {
                return Err(MethodError::Invalid);
            }
            let val = value.map_or_else(
                || name.get(key.len()..).unwrap_or_default().to_owned(),
                str::to_owned,
            );
            params.push((key, val));
        }
        settings.block = methods::bzip2_block(settings.level, &params);
        Ok(settings)
    }
}

/// `FileTime_To_UnixTime`: seconds since 1970, 0 before it, saturated past 2106.
fn unix_time(ticks: u64) -> u32 {
    let seconds = ticks / TICKS_PER_SECOND;
    let epoch = UNIX_EPOCH_TICKS / TICKS_PER_SECOND;
    if seconds < epoch {
        return 0;
    }
    u32::try_from(seconds - epoch).unwrap_or(u32::MAX)
}

/// gzip's header as 7-Zip writes it (`CItem::WriteHeader`): the name flag alone.
fn gzip_header(name: &[u8], time: u32, xfl: u8) -> Vec<u8> {
    let mut header = vec![0x1F, 0x8B, 8, if name.is_empty() { 0 } else { FNAME }];
    header.extend_from_slice(&time.to_le_bytes());
    header.push(xfl);
    header.push(HOST_FAT);
    if !name.is_empty() {
        header.extend_from_slice(name);
        header.push(0);
    }
    header
}

/// The name gzip's header keeps: the path's last part, in the system's code page (here
/// Latin-1, as the reader takes it), `?` for what it cannot hold.
fn gzip_name(path: &str) -> Vec<u8> {
    let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
    base.chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
        .collect()
}

/// The deflate level for 7-Zip's: its levels in flate2's.
const fn deflate_level(level: u32) -> u32 {
    match level {
        0 => 0,
        1..=2 => 1,
        3..=4 => 3,
        5..=6 => 6,
        7..=8 => 8,
        _ => 9,
    }
}

/// A reader that keeps the CRC and the count of what passes through it.
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

/// The handlers' `UpdateItems`; returns the files read.
#[expect(
    clippy::too_many_lines,
    reason = "three handlers' UpdateItems, step by step"
)]
pub(super) fn write<SE: cash_core::ShellExtensions>(
    job: &Job<'_, '_, SE>,
    kind: Kind,
    opened: Option<&Opened>,
    items: &[Item],
    out: &mut Out<'_>,
    warnings: &mut Warnings,
    processed: &mut [bool],
) -> Result<u64, Stop> {
    let settings = Settings::parse(kind, &job.options.properties)?;
    let invalid = || Stop::System(win_error(win::E_INVALIDARG));
    if items.is_empty() && kind == Kind::Xz {
        // Xz_EncodeEmpty: a stream without blocks, and without a check.
        let encoder = codec::xz::xz_writer(
            methods::Settings::parse(&[])?.xz_lzma(0)?,
            codec::xz::Check::None,
            None,
            &mut *out,
        )?;
        encoder.finish()?;
        return Ok(0);
    }
    let [ui] = items else {
        return Err(invalid());
    };
    if ui.up.new_props && ui.is_dir {
        return Err(invalid());
    }
    if !ui.up.new_data {
        if ui.up.arc != Some(0) {
            return Err(invalid());
        }
        let old_name = job
            .arc_items
            .first()
            .map_or(ui.name.as_str(), |a| a.name.as_str());
        progress(job.console, job.options, 3, "=", old_name);
        let stream = opened
            .and_then(Opened::stream)
            .ok_or_else(|| Stop::System(win_error(win::E_NOTIMPL)))?;
        let mut file = stream.location.open().map_err(Stop::System)?;
        if kind == Kind::Gzip
            && ui.up.new_props
            && let Some((header_size, xfl)) = stream.gzip_header
        {
            let time = if settings.mtime {
                ui.modified.map_or(0, unix_time)
            } else {
                0
            };
            out.write_all(&gzip_header(&gzip_name(&ui.name), time, xfl))?;
            file.seek(SeekFrom::Start(header_size))
                .map_err(Stop::System)?;
        }
        io::copy(&mut file, out).map_err(Stop::System)?;
        return Ok(0);
    }
    let Some(d) = ui.up.dir else {
        return Err(Stop::System(win_error(win::E_FAIL)));
    };
    job.announce(ui);
    let Some(mut file) = job.open_new(d, warnings) else {
        return Err(Stop::System(win_error(win::INVALID_FUNCTION)));
    };
    let di = &job.dir_items[d];
    let size = file.size_or(di.size);
    match kind {
        Kind::Gzip => {
            let time = if settings.mtime {
                unix_time(di.modified)
            } else {
                0
            };
            let xfl = if settings.level >= 7 { 2 } else { 4 };
            out.write_all(&gzip_header(&gzip_name(&ui.name), time, xfl))?;
            let mut input = Counted {
                inner: file,
                crc: crc32fast::Hasher::new(),
                size: 0,
            };
            let mut encoder = flate2::write::DeflateEncoder::new(
                &mut *out,
                flate2::Compression::new(deflate_level(settings.level)),
            );
            io::copy(&mut input, &mut encoder).map_err(Stop::System)?;
            encoder.finish()?;
            let crc = input.crc.finalize();
            out.write_all(&crc.to_le_bytes())?;
            out.write_all(
                &u32::try_from(input.size & 0xFFFF_FFFF)
                    .unwrap_or(0)
                    .to_le_bytes(),
            )?;
        }
        Kind::Bzip2 => {
            let mut encoder = codec::writer(Codec::Bzip2, &mut *out, settings.block)?;
            io::copy(&mut file, &mut encoder).map_err(Stop::System)?;
            encoder.finish()?;
        }
        _ => {
            let xz = settings
                .xz
                .as_ref()
                .ok_or_else(|| Stop::System(win_error(win::E_FAIL)))?;
            let check = match xz.crc_size {
                0 => codec::xz::Check::None,
                4 => codec::xz::Check::Crc32,
                8 => codec::xz::Check::Crc64,
                32 => codec::xz::Check::Sha256,
                _ => return Err(invalid()),
            };
            let block = xz.solid_bytes().and_then(NonZeroU64::new);
            let mut encoder = codec::xz::xz_writer(xz.xz_lzma(size)?, check, block, &mut *out)?;
            io::copy(&mut file, &mut encoder).map_err(Stop::System)?;
            encoder.finish()?;
        }
    }
    processed[d] = true;
    job.item_done();
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_headers_are_7_zips() {
        assert_eq!(
            gzip_header(b"a.txt", 0x6AC5_FC00, 4),
            [
                0x1F, 0x8B, 8, 8, 0x00, 0xFC, 0xC5, 0x6A, 4, 0, b'a', b'.', b't', b'x', b't', 0
            ]
        );
        assert_eq!(gzip_header(b"", 0, 2)[3], 0);
        assert_eq!(gzip_name("d/sub/b.txt"), b"b.txt");
        assert_eq!(gzip_name("d/\u{e9}\u{263a}"), [0xE9, b'?']);
    }

    #[test]
    fn unix_times_saturate() {
        assert_eq!(unix_time(0), 0);
        assert_eq!(unix_time(UNIX_EPOCH_TICKS + 15 * TICKS_PER_SECOND + 9), 15);
    }

    #[test]
    fn settings_follow_each_handler() {
        let props = |list: &[(&str, Option<&str>)]| -> Vec<(String, Option<String>)> {
            list.iter()
                .map(|(n, v)| ((*n).to_owned(), v.map(str::to_owned)))
                .collect()
        };
        assert!(Settings::parse(Kind::Gzip, &props(&[("tc", None)])).is_err());
        assert!(Settings::parse(Kind::Gzip, &props(&[("tc", Some("off"))])).is_ok());
        assert!(
            !Settings::parse(Kind::Gzip, &props(&[("tm", Some("off"))]))
                .unwrap()
                .mtime
        );
        assert_eq!(
            Settings::parse(Kind::Bzip2, &props(&[("x1", None)]))
                .unwrap()
                .block,
            1
        );
        assert!(Settings::parse(Kind::Bzip2, &props(&[("tm", None)])).is_err());
        assert!(Settings::parse(Kind::Xz, &props(&[("crc", Some("8"))])).is_ok());
    }
}
