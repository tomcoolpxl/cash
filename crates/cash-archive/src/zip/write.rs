//! Writing a zip archive as Info-ZIP's zip 3.0 does.
//!
//! Each local header is written first and its CRC and sizes filled in once the data is
//! through, or given after the data in a descriptor when the output cannot go back; a
//! member that does not shrink is stored instead; members of an old archive are copied
//! as they are; then comes the central directory.

use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::crypt::{Encrypt, Keys, make_header};
use super::{
    CENTRAL_SIGNATURE, DESCRIPTOR_SIGNATURE, DosTime, END_SIGNATURE, Entry, LOCAL_SIGNATURE,
    MADE_BY_ZIP_3_UNIX, TextCheck, ZIP64_END_SIGNATURE, ZIP64_LOCATOR_SIGNATURE, extra_id, field,
    fields, flag, le16, le32, method,
};
use crate::zip::read::{Archive, DataError, local};

/// Where an archive is written: a file that can go back to fill a header in, or a stream
/// that cannot.
pub trait Output: Write {
    /// Writes `bytes` at `at`, then goes back to the end; `false` when the output cannot
    /// go back.
    fn patch(&mut self, at: u64, bytes: &[u8]) -> io::Result<bool>;
    /// Cuts the output back to `at` to write there again; `false` when it cannot.
    fn rewind(&mut self, at: u64) -> io::Result<bool>;
    /// Whether it can go back.
    fn seekable(&self) -> bool;
    /// How many bytes the output starts with before the archive's first: a split
    /// archive's marker.
    fn start(&self) -> u64 {
        0
    }
    /// Where the byte at `at` goes: the part (disk) and the offset in it. A plain output
    /// is one part.
    fn place(&self, at: u64) -> (u32, u64) {
        (0, at)
    }
    /// Starts a new part when `len` more bytes would not fit in this one, so that a
    /// header is never split.
    fn reserve(&mut self, _len: u64) -> io::Result<()> {
        Ok(())
    }
}

/// A split archive being written: `NAME.z01`, `NAME.z02` and so on, each `size` bytes,
/// the first starting with the marker `PK\7\8`, and the last renamed `NAME.zip` when
/// it is done.
pub struct SplitOutput {
    stem: PathBuf,
    size: u64,
    /// The parts written and closed, with where each starts in the whole.
    parts: Vec<(PathBuf, u64)>,
    file: fs::File,
    path: PathBuf,
    /// Where the part being written starts in the whole.
    base: u64,
    /// What it holds so far.
    in_part: u64,
}

impl SplitOutput {
    /// A split archive for `target` (`NAME.zip`), in parts of `size` bytes.
    ///
    /// # Errors
    ///
    /// When the first part cannot be made.
    pub fn create(target: &Path, size: u64) -> io::Result<Self> {
        let stem = target.with_extension("");
        let path = part_path(&stem, 1);
        let mut file = fs::File::create(&path)?;
        file.write_all(&DESCRIPTOR_SIGNATURE.to_le_bytes())?;
        Ok(Self {
            stem,
            size,
            parts: Vec::new(),
            file,
            path,
            base: 0,
            in_part: 4,
        })
    }

    fn next_part(&mut self) -> io::Result<()> {
        self.file.flush()?;
        let path = part_path(&self.stem, self.parts.len() + 2);
        let file = fs::File::create(&path)?;
        let old = std::mem::replace(&mut self.path, path);
        self.file = file;
        self.parts.push((old, self.base));
        self.base += self.in_part;
        self.in_part = 0;
        Ok(())
    }

    /// Closes the last part as `target`: the parts closed before it, in order.
    ///
    /// # Errors
    ///
    /// When the last part cannot be renamed.
    pub fn finish(self, target: &Path) -> io::Result<Vec<PathBuf>> {
        let Self {
            file,
            path,
            parts,
            stem,
            ..
        } = self;
        drop(file);
        if fs::symlink_metadata(target).is_ok() {
            fs::remove_file(target)?;
        }
        fs::rename(&path, target)?;
        // Parts left from a longer split archive of the same name.
        let mut n = parts.len() + 1;
        loop {
            let stale = part_path(&stem, n);
            if stale == path || fs::remove_file(&stale).is_err() {
                break;
            }
            n += 1;
        }
        Ok(parts.into_iter().map(|(p, _)| p).collect())
    }
}

/// The name of part `n` of a split archive: `NAME.z01` and on.
pub fn part_path(stem: &Path, n: usize) -> PathBuf {
    stem.with_extension(format!("z{n:02}"))
}

impl Write for SplitOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.in_part >= self.size {
            self.next_part()?;
        }
        let room = usize::try_from(self.size - self.in_part).unwrap_or(usize::MAX);
        let n = self
            .file
            .write(buf.get(..buf.len().min(room)).unwrap_or_default())?;
        self.in_part += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Output for SplitOutput {
    fn patch(&mut self, at: u64, bytes: &[u8]) -> io::Result<bool> {
        if at >= self.base {
            let end = self.file.stream_position()?;
            self.file.seek(SeekFrom::Start(at - self.base))?;
            self.file.write_all(bytes)?;
            self.file.seek(SeekFrom::Start(end))?;
            return Ok(true);
        }
        let Some((path, base)) = self.parts.iter().rev().find(|(_, base)| *base <= at) else {
            return Ok(false);
        };
        let mut part = fs::OpenOptions::new().write(true).open(path)?;
        part.seek(SeekFrom::Start(at - base))?;
        part.write_all(bytes)?;
        Ok(true)
    }

    fn rewind(&mut self, at: u64) -> io::Result<bool> {
        if at < self.base {
            return Ok(false);
        }
        self.in_part = at - self.base;
        self.file.set_len(self.in_part)?;
        self.file.seek(SeekFrom::Start(self.in_part))?;
        Ok(true)
    }

    fn seekable(&self) -> bool {
        true
    }

    fn start(&self) -> u64 {
        4
    }

    fn place(&self, _at: u64) -> (u32, u64) {
        (
            u32::try_from(self.parts.len()).unwrap_or(u32::MAX),
            self.in_part,
        )
    }

    fn reserve(&mut self, len: u64) -> io::Result<()> {
        if self.in_part > 0 && self.in_part + len > self.size {
            self.next_part()?;
        }
        Ok(())
    }
}

impl Output for fs::File {
    fn patch(&mut self, at: u64, bytes: &[u8]) -> io::Result<bool> {
        let end = self.stream_position()?;
        self.seek(SeekFrom::Start(at))?;
        self.write_all(bytes)?;
        self.seek(SeekFrom::Start(end))?;
        Ok(true)
    }

    fn rewind(&mut self, at: u64) -> io::Result<bool> {
        self.set_len(at)?;
        self.seek(SeekFrom::Start(at))?;
        Ok(true)
    }

    fn seekable(&self) -> bool {
        true
    }
}

impl Output for &mut fs::File {
    fn patch(&mut self, at: u64, bytes: &[u8]) -> io::Result<bool> {
        Output::patch(&mut **self, at, bytes)
    }

    fn rewind(&mut self, at: u64) -> io::Result<bool> {
        Output::rewind(&mut **self, at)
    }

    fn seekable(&self) -> bool {
        true
    }
}

impl Output for io::Cursor<Vec<u8>> {
    fn patch(&mut self, at: u64, bytes: &[u8]) -> io::Result<bool> {
        let end = self.position();
        self.set_position(at);
        self.write_all(bytes)?;
        self.set_position(end);
        Ok(true)
    }

    fn rewind(&mut self, at: u64) -> io::Result<bool> {
        self.get_mut()
            .truncate(usize::try_from(at).unwrap_or(usize::MAX));
        self.set_position(at);
        Ok(true)
    }

    fn seekable(&self) -> bool {
        true
    }
}

/// An output that cannot go back: standard output.
pub struct Stream<W>(pub W);

impl<W: Write> Write for Stream<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<W: Write> Output for Stream<W> {
    fn patch(&mut self, _: u64, _: &[u8]) -> io::Result<bool> {
        Ok(false)
    }

    fn rewind(&mut self, _: u64) -> io::Result<bool> {
        Ok(false)
    }

    fn seekable(&self) -> bool {
        false
    }
}

/// A reader that can go back to its start, to store a member that would not shrink.
pub trait ReadSeek: Read + Seek {}

impl<T: Read + Seek> ReadSeek for T {}

/// A member's data.
pub enum Input<'a> {
    /// None: a folder.
    None,
    /// A file, which can be read again.
    File(&'a mut dyn ReadSeek),
    /// A stream, read once.
    Stream(&'a mut dyn Read),
}

/// A member to add.
#[derive(Clone, Debug, Default)]
pub struct NewMember {
    /// Its name in the archive.
    pub name: Vec<u8>,
    /// [`method::STORED`], [`method::DEFLATED`] or [`method::BZIP2`].
    pub method: u16,
    /// The compression level, 1 to 9.
    pub level: u32,
    /// Its time.
    pub time: DosTime,
    /// The local header's extra field.
    pub local_extra: Vec<u8>,
    /// The central directory's extra field.
    pub central_extra: Vec<u8>,
    /// Its attributes: the Unix mode in the high half, MS-DOS's in the low byte.
    pub external_attributes: u32,
    /// The password to encrypt it with.
    pub password: Option<Vec<u8>>,
    /// Its comment.
    pub comment: Vec<u8>,
    /// Whether to tell text from binary, as zip does while it deflates.
    pub detect_text: bool,
    /// Its size, when known before: a member of 4 GiB or more needs Zip64 fields in its
    /// local header.
    pub size_hint: Option<u64>,
}

/// What adding a member came to.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Added {
    /// Bytes read.
    pub size: u64,
    /// Bytes stored, the encryption header included.
    pub compressed_size: u64,
    /// The method it was stored with.
    pub method: u16,
}

/// Counts what goes through to the output.
struct Counting<'a, O: Write> {
    out: &'a mut O,
    count: u64,
}

impl<O: Write> Write for Counting<'_, O> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.out.write(buf)?;
        self.count += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// A zip archive being written.
pub struct Writer<O: Output> {
    out: O,
    at: u64,
    entries: Vec<Entry>,
    seed: u64,
}

/// Reads all of `input` into `sink`: its CRC, its size, and what text it is.
fn pump(
    input: &mut dyn Read,
    sink: &mut dyn Write,
    text: &mut TextCheck,
) -> io::Result<(u32, u64)> {
    let mut crc = crc32fast::Hasher::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let n = match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        let chunk = buffer.get(..n).unwrap_or_default();
        crc.update(chunk);
        text.update(chunk);
        sink.write_all(chunk)?;
        size += n as u64;
    }
    Ok((crc.finalize(), size))
}

fn u32_or_mark(value: u64) -> u32 {
    u32::try_from(value)
        .ok()
        .filter(|v| *v != u32::MAX)
        .unwrap_or(u32::MAX)
}

impl<O: Output> Writer<O> {
    /// An archive written to `out`, from its start.
    pub fn new(out: O) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9e37_79b9, |d| {
                d.as_nanos()
                    .to_le_bytes()
                    .iter()
                    .fold(0_u64, |a, b| a.wrapping_mul(31).wrapping_add(u64::from(*b)))
            });
        let at = out.start();
        Self {
            out,
            at,
            entries: Vec::new(),
            seed: seed | 1,
        }
    }

    /// The members written so far, as the central directory will have them.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Gives the members written so far these comments, in order: zip's `-c`, asked
    /// for once the data is in.
    pub fn set_comments(&mut self, comments: &[Vec<u8>]) {
        for (entry, comment) in self.entries.iter_mut().zip(comments) {
            entry.comment.clone_from(comment);
        }
    }

    fn put(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.out.write_all(bytes)?;
        self.at += bytes.len() as u64;
        Ok(())
    }

    /// Adds a member from `input`.
    ///
    /// # Errors
    ///
    /// When reading the input or writing the archive fails.
    #[expect(
        clippy::too_many_lines,
        reason = "zip's zipup: the header, the data, the retry as stored, the sizes"
    )]
    pub fn add(&mut self, member: &NewMember, mut input: Input<'_>) -> io::Result<Added> {
        let start = self.at;
        let folder = matches!(input, Input::None);
        let mut method = if folder {
            method::STORED
        } else {
            member.method
        };
        let mut text = TextCheck::default();
        let large = member.size_hint.is_some_and(|s| s >= 0xffff_ffff);
        loop {
            let encrypted = member.password.is_some() && !folder;
            let descriptor = encrypted || !self.out.seekable();
            let mut flags = 0;
            if !member.name.is_ascii() {
                flags |= flag::UTF8;
            }
            if encrypted {
                flags |= flag::ENCRYPTED;
            }
            if descriptor {
                flags |= flag::DESCRIPTOR;
            }
            if method == method::DEFLATED {
                if member.level <= 2 {
                    flags |= flag::FAST;
                } else if member.level >= 8 {
                    flags |= flag::SLOW;
                }
            }
            let needed: u16 = if large {
                45
            } else if method == method::BZIP2 {
                46
            } else if method == method::DEFLATED || encrypted {
                20
            } else {
                10
            };
            let mut local_extra = member.local_extra.clone();
            if large {
                local_extra.extend(field(extra_id::ZIP64, &[0; 16]));
            }
            let mut header = Vec::with_capacity(30 + member.name.len() + local_extra.len());
            header.extend_from_slice(&LOCAL_SIGNATURE.to_le_bytes());
            header.extend_from_slice(&needed.to_le_bytes());
            header.extend_from_slice(&flags.to_le_bytes());
            header.extend_from_slice(&method.to_le_bytes());
            header.extend_from_slice(&member.time.time.to_le_bytes());
            header.extend_from_slice(&member.time.date.to_le_bytes());
            header.extend_from_slice(&[0; 12]);
            if large {
                header.splice(18..26, u32::MAX.to_le_bytes().repeat(2));
            }
            header.extend_from_slice(
                &u16::try_from(member.name.len())
                    .unwrap_or(u16::MAX)
                    .to_le_bytes(),
            );
            header.extend_from_slice(
                &u16::try_from(local_extra.len())
                    .unwrap_or(u16::MAX)
                    .to_le_bytes(),
            );
            header.extend_from_slice(&member.name);
            header.extend_from_slice(&local_extra);
            self.out.reserve(header.len() as u64)?;
            let (disk, local_offset) = self.out.place(start);
            self.put(&header)?;
            let data_start = self.at;
            let detect = member.detect_text && method == method::DEFLATED;
            let mut check = TextCheck::default();
            let (crc, size, compressed) = {
                let mut counting = Counting {
                    out: &mut self.out,
                    count: 0,
                };
                let mut sink: Box<dyn Write + '_> = if let Some(password) = &member.password
                    && !folder
                {
                    let mut keys = Keys::new(password);
                    let check_byte = member.time.time.to_be_bytes();
                    let mut random = [0_u8; 10];
                    for (i, b) in random.iter_mut().enumerate() {
                        self.seed = self
                            .seed
                            .wrapping_mul(6_364_136_223_846_793_005)
                            .wrapping_add(1);
                        *b = self.seed.to_le_bytes()[(i % 6) + 2];
                    }
                    let head = make_header(&mut keys, random, check_byte[0], check_byte[1]);
                    counting.write_all(&head)?;
                    Box::new(Encrypt::new(&mut counting, keys))
                } else {
                    Box::new(&mut counting)
                };
                let (crc, size) = match &mut input {
                    Input::None => (0, 0),
                    Input::File(file) => {
                        compress(*file, &mut sink, method, member.level, &mut check)?
                    }
                    Input::Stream(stream) => {
                        compress(*stream, &mut sink, method, member.level, &mut check)?
                    }
                };
                sink.flush()?;
                drop(sink);
                (crc, size, counting.count)
            };
            if detect {
                text = check;
            }
            let data_size = compressed.saturating_sub(if encrypted { 12 } else { 0 });
            if method != method::STORED && data_size >= size {
                let can_reread = match &mut input {
                    Input::File(file) => file.seek(SeekFrom::Start(0)).is_ok(),
                    _ => false,
                };
                if can_reread && self.out.rewind(start)? {
                    self.at = start;
                    method = method::STORED;
                    continue;
                }
            }
            self.at = data_start + compressed;
            if descriptor {
                let mut tail = DESCRIPTOR_SIGNATURE.to_le_bytes().to_vec();
                tail.extend_from_slice(&crc.to_le_bytes());
                if large {
                    tail.extend_from_slice(&compressed.to_le_bytes());
                    tail.extend_from_slice(&size.to_le_bytes());
                } else {
                    tail.extend_from_slice(&u32_or_mark(compressed).to_le_bytes());
                    tail.extend_from_slice(&u32_or_mark(size).to_le_bytes());
                }
                self.put(&tail)?;
            } else {
                let mut fill = crc.to_le_bytes().to_vec();
                if large {
                    fill.extend_from_slice(&[0xff; 8]);
                } else {
                    fill.extend_from_slice(&u32_or_mark(compressed).to_le_bytes());
                    fill.extend_from_slice(&u32_or_mark(size).to_le_bytes());
                }
                self.out.patch(start + 14, &fill)?;
                if large {
                    let mut sizes = size.to_le_bytes().to_vec();
                    sizes.extend_from_slice(&compressed.to_le_bytes());
                    let at =
                        start + 30 + member.name.len() as u64 + member.local_extra.len() as u64 + 4;
                    self.out.patch(at, &sizes)?;
                }
            }
            self.entries.push(Entry {
                version_made_by: MADE_BY_ZIP_3_UNIX,
                version_needed: needed,
                flags,
                method,
                time: member.time,
                crc,
                compressed_size: compressed,
                size,
                name: member.name.clone(),
                extra: member.central_extra.clone(),
                comment: member.comment.clone(),
                disk_start: disk,
                internal_attributes: u16::from(text.is_text()),
                external_attributes: member.external_attributes,
                local_offset,
            });
            return Ok(Added {
                size,
                compressed_size: compressed,
                method,
            });
        }
    }

    /// Adds the record of a member whose bytes were copied with [`Self::copy_span`].
    pub fn push_entry(&mut self, entry: Entry) {
        self.entries.push(entry);
    }

    /// Copies a member of another archive as it is: its local header, its data and its
    /// descriptor.
    ///
    /// # Errors
    ///
    /// When its local header is not where the directory says, or reading or writing
    /// fails.
    pub fn copy<R: Read + Seek>(
        &mut self,
        from: &mut R,
        archive: &Archive,
        entry: &Entry,
    ) -> Result<(), DataError> {
        let header = local(from, archive, entry)?;
        let start = archive.position(entry.local_offset);
        let mut length = header.data_offset - start + entry.compressed_size;
        if entry.has_descriptor() {
            from.seek(SeekFrom::Start(start + length))?;
            let mut signature = [0_u8; 4];
            let signed = from.read_exact(&mut signature).is_ok()
                && u32::from_le_bytes(signature) == DESCRIPTOR_SIGNATURE;
            let sizes = if entry.compressed_size >= 0xffff_ffff || entry.size >= 0xffff_ffff {
                16
            } else {
                8
            };
            length += 4 + sizes + if signed { 4 } else { 0 };
        }
        let mut entry = entry.clone();
        self.copy_span(from, start, header.data_offset - start, length, &mut entry)?;
        self.entries.push(entry);
        Ok(())
    }

    /// Copies `length` bytes from `start` in `from`, a member's local header (of
    /// `header_len` bytes) and what follows it, and puts the member's new place in
    /// `entry`.
    ///
    /// # Errors
    ///
    /// When reading or writing fails, or `from` ends early.
    pub fn copy_span<R: Read + Seek>(
        &mut self,
        from: &mut R,
        start: u64,
        header_len: u64,
        length: u64,
        entry: &mut Entry,
    ) -> Result<(), DataError> {
        from.seek(SeekFrom::Start(start))?;
        self.out.reserve(header_len)?;
        let (disk, offset) = self.out.place(self.at);
        let copied = io::copy(&mut from.take(length), &mut self.out)?;
        self.at += copied;
        if copied < length {
            return Err(DataError::Io(io::ErrorKind::UnexpectedEof.into()));
        }
        entry.disk_start = disk;
        entry.local_offset = offset;
        Ok(())
    }

    /// Writes the central directory and the end records, with the archive's comment.
    ///
    /// # Errors
    ///
    /// When writing fails.
    #[expect(
        clippy::too_many_lines,
        reason = "the central directory, the Zip64 records and the end record, part by part"
    )]
    pub fn finish(mut self, comment: &[u8]) -> io::Result<O> {
        let central_start = self.at;
        let entries = std::mem::take(&mut self.entries);
        let mut central_place: Option<(u32, u64)> = None;
        let mut on_last_disk = 0_u64;
        let mut last_disk = 0_u32;
        for entry in &entries {
            let mut zip64 = Vec::new();
            if entry.size >= 0xffff_ffff {
                zip64.extend_from_slice(&entry.size.to_le_bytes());
            }
            if entry.compressed_size >= 0xffff_ffff {
                zip64.extend_from_slice(&entry.compressed_size.to_le_bytes());
            }
            if entry.local_offset >= 0xffff_ffff {
                zip64.extend_from_slice(&entry.local_offset.to_le_bytes());
            }
            let mut extra: Vec<u8> = fields(&entry.extra)
                .into_iter()
                .filter(|(id, _)| *id != extra_id::ZIP64)
                .flat_map(|(id, data)| field(id, data))
                .collect();
            if !zip64.is_empty() {
                extra.splice(0..0, field(extra_id::ZIP64, &zip64));
            }
            let mut record = CENTRAL_SIGNATURE.to_le_bytes().to_vec();
            record.extend_from_slice(&entry.version_made_by.to_le_bytes());
            record.extend_from_slice(&entry.version_needed.to_le_bytes());
            record.extend_from_slice(&entry.flags.to_le_bytes());
            record.extend_from_slice(&entry.method.to_le_bytes());
            record.extend_from_slice(&entry.time.time.to_le_bytes());
            record.extend_from_slice(&entry.time.date.to_le_bytes());
            record.extend_from_slice(&entry.crc.to_le_bytes());
            record.extend_from_slice(&u32_or_mark(entry.compressed_size).to_le_bytes());
            record.extend_from_slice(&u32_or_mark(entry.size).to_le_bytes());
            record.extend_from_slice(
                &u16::try_from(entry.name.len())
                    .unwrap_or(u16::MAX)
                    .to_le_bytes(),
            );
            record.extend_from_slice(&u16::try_from(extra.len()).unwrap_or(u16::MAX).to_le_bytes());
            record.extend_from_slice(
                &u16::try_from(entry.comment.len())
                    .unwrap_or(u16::MAX)
                    .to_le_bytes(),
            );
            record.extend_from_slice(&u16::try_from(entry.disk_start).unwrap_or(0).to_le_bytes());
            record.extend_from_slice(&entry.internal_attributes.to_le_bytes());
            record.extend_from_slice(&entry.external_attributes.to_le_bytes());
            record.extend_from_slice(&u32_or_mark(entry.local_offset).to_le_bytes());
            record.extend_from_slice(&entry.name);
            record.extend_from_slice(&extra);
            record.extend_from_slice(&entry.comment);
            self.out.reserve(record.len() as u64)?;
            let place = self.out.place(self.at);
            if central_place.is_none() {
                central_place = Some(place);
            }
            if place.0 == last_disk {
                on_last_disk += 1;
            } else {
                last_disk = place.0;
                on_last_disk = 1;
            }
            self.put(&record)?;
        }
        let (central_disk, central_offset) =
            central_place.unwrap_or_else(|| self.out.place(central_start));
        let central_size = self.at - central_start;
        let count = entries.len() as u64;
        if count >= 0xffff || central_offset >= 0xffff_ffff || central_size >= 0xffff_ffff {
            self.out.reserve(56 + 20)?;
            let (zip64_disk, zip64_at) = self.out.place(self.at);
            if zip64_disk != last_disk {
                last_disk = zip64_disk;
                on_last_disk = 0;
            }
            let mut record = ZIP64_END_SIGNATURE.to_le_bytes().to_vec();
            record.extend_from_slice(&44_u64.to_le_bytes());
            record.extend_from_slice(&MADE_BY_ZIP_3_UNIX.to_le_bytes());
            record.extend_from_slice(&45_u16.to_le_bytes());
            record.extend_from_slice(&zip64_disk.to_le_bytes());
            record.extend_from_slice(&central_disk.to_le_bytes());
            record.extend_from_slice(&on_last_disk.to_le_bytes());
            record.extend_from_slice(&count.to_le_bytes());
            record.extend_from_slice(&central_size.to_le_bytes());
            record.extend_from_slice(&central_offset.to_le_bytes());
            record.extend_from_slice(&ZIP64_LOCATOR_SIGNATURE.to_le_bytes());
            record.extend_from_slice(&zip64_disk.to_le_bytes());
            record.extend_from_slice(&zip64_at.to_le_bytes());
            record.extend_from_slice(&(zip64_disk + 1).to_le_bytes());
            self.put(&record)?;
        }
        let end_len = 22 + comment.len() as u64;
        self.out.reserve(end_len)?;
        let (end_disk, _) = self.out.place(self.at);
        if end_disk != last_disk {
            on_last_disk = 0;
        }
        let small = |n: u64| {
            u16::try_from(n)
                .ok()
                .filter(|c| *c != u16::MAX)
                .unwrap_or(u16::MAX)
        };
        let mut end = END_SIGNATURE.to_le_bytes().to_vec();
        end.extend_from_slice(&small(u64::from(end_disk)).to_le_bytes());
        end.extend_from_slice(&small(u64::from(central_disk)).to_le_bytes());
        end.extend_from_slice(&small(on_last_disk).to_le_bytes());
        end.extend_from_slice(&small(count).to_le_bytes());
        end.extend_from_slice(&u32_or_mark(central_size).to_le_bytes());
        end.extend_from_slice(&u32_or_mark(central_offset).to_le_bytes());
        end.extend_from_slice(
            &u16::try_from(comment.len())
                .unwrap_or(u16::MAX)
                .to_le_bytes(),
        );
        end.extend_from_slice(comment);
        self.put(&end)?;
        self.out.flush()?;
        Ok(self.out)
    }
}

/// Compresses `input` into `sink` by `method`: its CRC and size.
fn compress(
    input: &mut dyn Read,
    sink: &mut dyn Write,
    method: u16,
    level: u32,
    text: &mut TextCheck,
) -> io::Result<(u32, u64)> {
    match method {
        method::DEFLATED => {
            let mut encoder = flate2::write::DeflateEncoder::new(
                sink,
                flate2::Compression::new(level.clamp(1, 9)),
            );
            let result = pump(input, &mut encoder, text)?;
            encoder.finish()?;
            Ok(result)
        }
        method::BZIP2 => {
            let mut encoder =
                bzip2::write::BzEncoder::new(sink, bzip2::Compression::new(level.clamp(1, 9)));
            let result = pump(input, &mut encoder, text)?;
            encoder.finish()?;
            Ok(result)
        }
        _ => pump(input, sink, text),
    }
}

/// The two bytes of a little-endian field, for tests and front ends.
pub fn read_u16(bytes: &[u8], at: usize) -> u16 {
    le16(bytes, at)
}

/// The four bytes of a little-endian field.
pub fn read_u32(bytes: &[u8], at: usize) -> u32 {
    le32(bytes, at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zip::read;
    use crate::zip::{Civil, DOS_READ_ONLY, S_IFREG};

    fn unhex(text: &str) -> Vec<u8> {
        let digits: Vec<u8> = text.bytes().filter(u8::is_ascii_hexdigit).collect();
        digits
            .chunks(2)
            .map(|pair| {
                u8::from_str_radix(std::str::from_utf8(pair).unwrap_or("0"), 16).unwrap_or(0)
            })
            .collect()
    }

    fn hello(method: u16, detect_text: bool) -> NewMember {
        NewMember {
            name: b"a.txt".to_vec(),
            method,
            level: 6,
            time: DosTime::from_civil(Civil {
                year: 2020,
                month: 1,
                day: 2,
                hour: 3,
                minute: 4,
                second: 6,
            }),
            external_attributes: (S_IFREG | 0o664) << 16,
            detect_text,
            ..NewMember::default()
        }
    }

    /// `zip -X` of `a.txt`, with `hello` in it, by Info-ZIP's zip 3.0: deflate tried,
    /// stored, called text.
    const INFO_ZIP_X: &str = "504b03040a00000000008318225020303a36060000000600000005000000612e74787468656c6c6f0a\
        504b01021e030a00000000008318225020303a360600000006000000050000000000000001000000b48100000000612e747874\
        504b0506000000000100010033000000290000000000";

    #[test]
    fn a_member_is_written_as_info_zip_writes_it() {
        let mut writer = Writer::new(io::Cursor::new(Vec::new()));
        let mut data = io::Cursor::new(b"hello\n".to_vec());
        let added = writer
            .add(&hello(method::DEFLATED, true), Input::File(&mut data))
            .unwrap_or_default();
        assert_eq!(added.method, method::STORED);
        let bytes = writer
            .finish(b"")
            .map(io::Cursor::into_inner)
            .unwrap_or_default();
        assert_eq!(bytes, unhex(INFO_ZIP_X));
    }

    #[test]
    fn what_is_written_reads_back() {
        let mut writer = Writer::new(io::Cursor::new(Vec::new()));
        let big = "a".repeat(3000).into_bytes();
        let mut member = hello(method::DEFLATED, true);
        member.name = b"big.txt".to_vec();
        member.external_attributes |= u32::from(DOS_READ_ONLY);
        let added = writer
            .add(&member, Input::File(&mut io::Cursor::new(big.clone())))
            .unwrap_or_default();
        assert_eq!(added.method, method::DEFLATED);
        let mut secret = hello(method::DEFLATED, true);
        secret.password = Some(b"pw".to_vec());
        writer
            .add(
                &secret,
                Input::Stream(&mut io::Cursor::new(b"hello\n".to_vec())),
            )
            .unwrap_or_default();
        let bytes = writer
            .finish(b"note")
            .map(io::Cursor::into_inner)
            .unwrap_or_default();
        let mut input = io::Cursor::new(bytes.clone());
        let archive = read::open(&mut input).unwrap_or_default();
        assert_eq!(archive.end.comment, b"note");
        assert_eq!(archive.entries.len(), 2);
        let mut contents = Vec::new();
        for entry in &archive.entries {
            let local = read::local(&mut input, &archive, entry).unwrap_or_default();
            let start = usize::try_from(local.data_offset).unwrap_or(0);
            let end = start + usize::try_from(entry.compressed_size).unwrap_or(0);
            let raw = bytes.get(start..end).unwrap_or_default();
            let mut out = Vec::new();
            if let Ok(mut reader) = read::data(raw, entry, Some(b"pw")) {
                reader.read_to_end(&mut out).unwrap_or_default();
            }
            contents.push(out);
        }
        assert_eq!(contents, [big, b"hello\n".to_vec()]);
        let secret_entry = archive.entries.get(1).cloned().unwrap_or_default();
        assert!(secret_entry.is_encrypted() && secret_entry.has_descriptor());
        let local = read::local(&mut input, &archive, &secret_entry).unwrap_or_default();
        let start = usize::try_from(local.data_offset).unwrap_or(0);
        let raw = bytes.get(start..).unwrap_or_default();
        assert!(matches!(
            read::data(raw, &secret_entry, Some(b"no")),
            Err(DataError::BadPassword)
        ));
    }

    #[test]
    fn a_split_archive_reads_back_from_its_parts() {
        let dir = tempfile::tempdir().unwrap_or_else(|_| unreachable!("a temporary folder"));
        let target = dir.path().join("sp.zip");
        let mut writer = Writer::new(
            SplitOutput::create(&target, 300).unwrap_or_else(|_| unreachable!("a part")),
        );
        let contents: Vec<Vec<u8>> = (0..4_u8)
            .map(|i| {
                (0..250_u16)
                    .map(|n| u8::try_from(n % 7).unwrap_or(0) + b'a' + i)
                    .collect()
            })
            .collect();
        for (i, body) in contents.iter().enumerate() {
            let mut member = hello(method::STORED, false);
            member.name = format!("f{i}").into_bytes();
            writer
                .add(&member, Input::File(&mut io::Cursor::new(body.clone())))
                .unwrap_or_default();
        }
        let output = writer
            .finish(b"")
            .unwrap_or_else(|_| unreachable!("finished"));
        let parts = output.finish(&target).unwrap_or_default();
        assert!(parts.len() >= 3, "{parts:?}");
        let mut files: Vec<fs::File> = parts
            .iter()
            .chain(std::iter::once(&target))
            .filter_map(|p| fs::File::open(p).ok())
            .collect();
        let mut first = [0_u8; 4];
        files
            .first_mut()
            .map(|f| f.read_exact(&mut first))
            .transpose()
            .unwrap_or_default();
        assert_eq!(first, DESCRIPTOR_SIGNATURE.to_le_bytes());
        let mut whole = read::Concat::new(files).unwrap_or_else(|_| unreachable!("the parts"));
        let bases = whole.bases();
        let archive = read::open_parts(&mut whole, &bases).unwrap_or_default();
        assert_eq!(archive.end.disk as usize, parts.len());
        assert_eq!(archive.entries.len(), 4);
        for (entry, body) in archive.entries.iter().zip(&contents) {
            let local = read::local(&mut whole, &archive, entry).unwrap_or_default();
            whole
                .seek(SeekFrom::Start(local.data_offset))
                .unwrap_or_default();
            let mut out = Vec::new();
            if let Ok(mut reader) =
                read::data((&mut whole).take(entry.compressed_size), entry, None)
            {
                reader.read_to_end(&mut out).unwrap_or_default();
            }
            assert_eq!(&out, body);
        }
    }

    #[test]
    fn a_stream_gets_descriptors_and_copies_keep_them() {
        let mut writer = Writer::new(Stream(Vec::new()));
        writer
            .add(
                &hello(method::DEFLATED, true),
                Input::Stream(&mut io::Cursor::new(b"hello\n".to_vec())),
            )
            .unwrap_or_default();
        let Stream(bytes) = writer.finish(b"").unwrap_or_else(|_| Stream(Vec::new()));
        let mut input = io::Cursor::new(bytes);
        let archive = read::open(&mut input).unwrap_or_default();
        let entry = archive.entries.first().cloned().unwrap_or_default();
        assert!(entry.has_descriptor());
        assert_eq!(entry.method, method::DEFLATED);
        let mut copy = Writer::new(io::Cursor::new(Vec::new()));
        copy.copy(&mut input, &archive, &entry).unwrap_or(());
        let copied = copy
            .finish(b"")
            .map(io::Cursor::into_inner)
            .unwrap_or_default();
        let again = read::open(&mut io::Cursor::new(copied)).unwrap_or_default();
        assert_eq!(again.entries.first().map(|e| e.crc), Some(entry.crc));
    }
}
