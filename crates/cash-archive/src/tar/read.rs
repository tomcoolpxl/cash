//! Reading a tar archive as GNU tar 1.35 does: block by block, a long name or a pax
//! header gathered into the member it belongs to, damage reported in GNU's order.
//!
//! GNU tar works in whole blocks: a last block cut short is no block at all. A first
//! block that is not a header (or no block at all) is "not a tar archive"; a run of bad
//! blocks after a good one is "Skipping to next header" once; two zero blocks end the
//! archive, one alone ends it with a warning; the end of the input inside a member's
//! data is "Unexpected EOF in archive".
//!
//! A sparse member, in GNU's old `S` headers or in pax's 0.0, 0.1 and 1.0 forms, is read
//! back whole: its holes come out as zeros.

use std::collections::VecDeque;
use std::io::{self, Read, Write};

use super::header::{BLOCK, Check, Header, field, read_number, read_time};
use crate::member::{Kind, Member, Timestamp};

/// How the archive's main headers are laid out, by their magic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layout {
    /// No magic: Unix V7.
    V7,
    /// `ustar  `: GNU's.
    Gnu,
    /// `ustar` and `00`: POSIX's.
    Ustar,
}

/// One member as the archive has it.
#[derive(Clone, Debug)]
pub struct Entry {
    /// The member, from its headers.
    pub member: Member,
    /// Its own header, the last of its blocks before its data.
    pub header: Header,
    /// The header's block number in the archive.
    pub block: u64,
    /// Every header block the member took, long names and pax headers included, as
    /// they are in the archive.
    pub raw: Vec<u8>,
    /// Whose headers they are, by their magic.
    pub layout: Layout,
    /// The size of its data in the archive.
    pub data_size: u64,
}

/// What reading the archive came to next.
#[derive(Debug)]
pub enum Event {
    /// A member, its data next to be read.
    Member(Box<Entry>),
    /// GNU's "This does not look like a tar archive".
    NotTar,
    /// GNU's "Skipping to next header": a run of blocks that are not headers.
    Skipping,
    /// GNU's "A lone zero block at N": one zero block, then something else; the archive
    /// ends there.
    LoneZero(u64),
    /// The end of the archive.
    End,
}

/// Why reading stopped.
#[derive(Debug)]
pub enum ReadError {
    /// The input ends inside a member: GNU's "Unexpected EOF in archive".
    UnexpectedEof,
    /// Reading the input failed.
    Io(io::Error),
}

impl From<io::Error> for ReadError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// What a data copy came to: reading or writing failed.
#[derive(Debug)]
pub enum CopyError {
    /// Reading the archive failed.
    Read(ReadError),
    /// Writing the data failed.
    Write(io::Error),
}

/// A sparse member: where its stored regions go, and its size as a file.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Sparse {
    /// Each region's offset in the file and its length, in the order stored.
    map: Vec<(u64, u64)>,
    real_size: u64,
}

/// GNU's old sparse header: four regions of two 12-byte numbers, a flag saying an
/// extension block follows, and the size as a file.
mod old_sparse {
    pub const MAP: usize = 386;
    pub const ENTRIES: usize = 4;
    pub const IS_EXTENDED: usize = 482;
    pub const REAL_SIZE: (usize, usize) = (483, 12);
    /// An extension block: 21 regions, then its own flag.
    pub const EXTENSION_ENTRIES: usize = 21;
    pub const EXTENSION_IS_EXTENDED: usize = 504;
}

/// The regions of an old sparse header or extension block, from `at`, up to the first
/// whose length is empty, as GNU's `oldgnu_add_sparse` stops.
fn old_sparse_regions(block: &[u8], at: usize, entries: usize, map: &mut Vec<(u64, u64)>) {
    for i in 0..entries {
        let start = at + i * 24;
        let (Some(offset), Some(size)) = (
            block.get(start..start + 12),
            block.get(start + 12..start + 24),
        ) else {
            return;
        };
        if size.first() == Some(&0) {
            return;
        }
        map.push((
            read_number(offset).unwrap_or(0),
            read_number(size).unwrap_or(0),
        ));
    }
}

/// A number of pax's sparse records.
fn pax_number(value: &[u8]) -> Option<u64> {
    std::str::from_utf8(value).ok()?.trim().parse().ok()
}

/// Zeros for a sparse member's holes.
fn write_zeros(out: &mut dyn Write, mut count: u64) -> Result<(), CopyError> {
    const ZEROS: [u8; 8192] = [0; 8192];
    while count > 0 {
        let take = usize::try_from(count.min(ZEROS.len() as u64)).unwrap_or(ZEROS.len());
        out.write_all(ZEROS.get(..take).unwrap_or_default())
            .map_err(CopyError::Write)?;
        count -= take as u64;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Status {
    StillUnread,
    Success,
    Zero,
    Failure,
    EndOfFile,
}

/// A tar archive being read.
pub struct Reader<R: Read> {
    input: R,
    /// Blocks read so far: the number of the next block.
    block: u64,
    /// Data bytes of the current member not yet read.
    remaining: u64,
    /// Padding after them.
    padding: u64,
    prev: Status,
    ignore_zeros: bool,
    pending: VecDeque<Event>,
    finished: bool,
    /// Where the end of the archive begins: the first of its zero blocks, or the end.
    end_block: Option<u64>,
    global: Vec<(String, Vec<u8>)>,
    /// The current member's regions, when it is sparse.
    sparse: Option<Sparse>,
}

/// Pax records: `LENGTH KEY=VALUE\n`.
fn pax_records(data: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut records = Vec::new();
    let mut rest = data;
    while !rest.is_empty() {
        let Some(space) = rest.iter().position(|b| *b == b' ') else {
            break;
        };
        let Some(length) = std::str::from_utf8(rest.get(..space).unwrap_or_default())
            .ok()
            .and_then(|l| l.parse::<usize>().ok())
        else {
            break;
        };
        let Some(record) = rest.get(..length) else {
            break;
        };
        let body = record
            .get(space + 1..length.saturating_sub(1))
            .unwrap_or_default();
        if let Some(equals) = body.iter().position(|b| *b == b'=') {
            let key = String::from_utf8_lossy(body.get(..equals).unwrap_or_default()).into_owned();
            records.push((key, body.get(equals + 1..).unwrap_or_default().to_vec()));
        }
        rest = rest.get(length..).unwrap_or_default();
        if length == 0 {
            break;
        }
    }
    records
}

/// A pax time: seconds, with a fraction.
fn pax_time(value: &[u8]) -> Option<Timestamp> {
    let text = std::str::from_utf8(value).ok()?;
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let seconds: i64 = whole.parse().ok()?;
    let digits: String = fraction.chars().take(9).collect();
    let nanos = if digits.is_empty() {
        0
    } else {
        format!("{digits:0<9}").parse().ok()?
    };
    if seconds < 0 && nanos > 0 {
        Some(Timestamp {
            seconds: seconds - 1,
            nanos: 1_000_000_000 - nanos,
        })
    } else {
        Some(Timestamp { seconds, nanos })
    }
}

fn kind_of(flag: u8, name: &[u8]) -> Kind {
    match flag {
        b'0' | b'7' | b'S' => Kind::File,
        0 if name.ends_with(b"/") => Kind::Dir,
        0 => Kind::File,
        b'1' => Kind::HardLink,
        b'2' => Kind::Symlink,
        b'3' => Kind::Char,
        b'4' => Kind::Block,
        b'5' => Kind::Dir,
        b'6' => Kind::Fifo,
        other => Kind::Other(other),
    }
}

impl<R: Read> Reader<R> {
    /// An archive read from `input`; `ignore_zeros` is `-i`.
    pub const fn new(input: R, ignore_zeros: bool) -> Self {
        Self {
            input,
            block: 0,
            remaining: 0,
            padding: 0,
            prev: Status::StillUnread,
            ignore_zeros,
            pending: VecDeque::new(),
            finished: false,
            end_block: None,
            global: Vec::new(),
            sparse: None,
        }
    }

    /// The number of the next block.
    pub const fn block(&self) -> u64 {
        self.block
    }

    /// Where the archive's end begins, once [`Event::End`] has come: the first zero
    /// block, or the end of the input. Members are appended there.
    pub const fn end_block(&self) -> Option<u64> {
        self.end_block
    }

    /// The input, for what follows the archive.
    pub fn into_inner(self) -> R {
        self.input
    }

    /// One whole block, or `None` at the end of the input; a last block cut short is no
    /// block.
    fn read_block(&mut self) -> io::Result<Option<[u8; BLOCK]>> {
        let mut block = [0_u8; BLOCK];
        let mut got = 0;
        while got < BLOCK {
            match self.input.read(block.get_mut(got..).unwrap_or_default()) {
                Ok(0) => return Ok(None),
                Ok(n) => got += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        self.block += 1;
        Ok(Some(block))
    }

    /// Reads `size` bytes of a header member's data (a long name, a pax header) with its
    /// padding.
    fn read_header_data(&mut self, size: u64, raw: &mut Vec<u8>) -> Result<Vec<u8>, ReadError> {
        let mut data = Vec::new();
        let blocks = size.div_ceil(BLOCK as u64);
        for _ in 0..blocks {
            let Some(block) = self.read_block()? else {
                return Err(ReadError::UnexpectedEof);
            };
            raw.extend_from_slice(&block);
            data.extend_from_slice(&block);
        }
        data.truncate(usize::try_from(size).unwrap_or(usize::MAX));
        Ok(data)
    }

    /// Passes over what is left of the current member's data.
    pub fn skip_data(&mut self) -> Result<(), ReadError> {
        self.sparse = None;
        let mut left = self.remaining + self.padding;
        while left > 0 {
            if self.read_block()?.is_none() {
                self.remaining = 0;
                self.padding = 0;
                return Err(ReadError::UnexpectedEof);
            }
            left = left.saturating_sub(BLOCK as u64);
        }
        self.remaining = 0;
        self.padding = 0;
        Ok(())
    }

    /// Copies the current member's data to `out`, as it is decoded block by block.
    ///
    /// # Errors
    ///
    /// When the input ends inside it, reading fails, or writing does.
    pub fn copy_data(&mut self, out: &mut dyn Write) -> Result<(), CopyError> {
        if let Some(sparse) = self.sparse.take() {
            return self.copy_sparse(&sparse, out);
        }
        while self.remaining > 0 {
            let Some(block) = self.read_block().map_err(|e| CopyError::Read(e.into()))? else {
                self.remaining = 0;
                self.padding = 0;
                return Err(CopyError::Read(ReadError::UnexpectedEof));
            };
            let take = usize::try_from(self.remaining.min(BLOCK as u64)).unwrap_or(BLOCK);
            out.write_all(block.get(..take).unwrap_or_default())
                .map_err(CopyError::Write)?;
            self.remaining -= take as u64;
            self.padding = self.padding.saturating_sub((BLOCK - take) as u64);
        }
        self.padding = 0;
        Ok(())
    }

    /// The next block of the current member's data, for a sparse member's regions: each
    /// region begins a block.
    fn data_block(&mut self) -> Result<[u8; BLOCK], CopyError> {
        let left = self.remaining + self.padding;
        let block = if left == 0 {
            None
        } else {
            self.read_block().map_err(|e| CopyError::Read(e.into()))?
        };
        let Some(block) = block else {
            self.remaining = 0;
            self.padding = 0;
            return Err(CopyError::Read(ReadError::UnexpectedEof));
        };
        let left = left - BLOCK as u64;
        self.remaining = self.remaining.saturating_sub(BLOCK as u64).min(left);
        self.padding = left - self.remaining;
        Ok(block)
    }

    /// A sparse member's data, its holes as zeros, up to its size as a file.
    fn copy_sparse(&mut self, sparse: &Sparse, out: &mut dyn Write) -> Result<(), CopyError> {
        let mut at = 0_u64;
        for &(offset, size) in &sparse.map {
            write_zeros(out, offset.saturating_sub(at))?;
            at = at.max(offset);
            let mut left = size;
            while left > 0 {
                let block = self.data_block()?;
                let take = usize::try_from(left.min(BLOCK as u64)).unwrap_or(BLOCK);
                out.write_all(block.get(..take).unwrap_or_default())
                    .map_err(CopyError::Write)?;
                left -= take as u64;
                at += take as u64;
            }
        }
        write_zeros(out, sparse.real_size.saturating_sub(at))?;
        self.skip_data().map_err(CopyError::Read)
    }

    /// The map of a pax 1.0 sparse member, at the start of its data: a count, then an
    /// offset and a length for each region, a number to a line, padded to a block.
    fn read_data_map(&mut self, raw: &mut Vec<u8>) -> Result<(Vec<(u64, u64)>, u64), ReadError> {
        let mut text: Vec<u8> = Vec::new();
        let mut numbers: Vec<u64> = Vec::new();
        let mut blocks = 0;
        loop {
            let Some(block) = self.read_block()? else {
                return Err(ReadError::UnexpectedEof);
            };
            blocks += 1;
            raw.extend_from_slice(&block);
            text.extend_from_slice(&block);
            while let Some(newline) = text.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = text.drain(..=newline).collect();
                numbers.push(pax_number(line.get(..newline).unwrap_or_default()).unwrap_or(0));
                let count = numbers.first().copied().unwrap_or(0);
                if numbers.len() as u64 == 1 + 2 * count {
                    let map = numbers
                        .get(1..)
                        .unwrap_or_default()
                        .chunks(2)
                        .map(|pair| {
                            (
                                pair.first().copied().unwrap_or(0),
                                pair.get(1).copied().unwrap_or(0),
                            )
                        })
                        .collect();
                    return Ok((map, blocks));
                }
            }
        }
    }

    /// The regions of a sparse member, from its headers or the start of its data, with
    /// its name and size as a file put in its member; `None` for any other member.
    fn sparse_of(
        &mut self,
        entry: &mut Entry,
        pax: &[(String, Vec<u8>)],
    ) -> Result<Option<Sparse>, ReadError> {
        let record = |key: &str| pax.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v);
        let mut sparse = Sparse::default();
        if entry.header.typeflag() == b'S' {
            let block = entry.header.0;
            old_sparse_regions(
                &block,
                old_sparse::MAP,
                old_sparse::ENTRIES,
                &mut sparse.map,
            );
            let mut extended = block.get(old_sparse::IS_EXTENDED).is_some_and(|b| *b != 0);
            while extended {
                let Some(next) = self.read_block()? else {
                    return Err(ReadError::UnexpectedEof);
                };
                entry.raw.extend_from_slice(&next);
                old_sparse_regions(&next, 0, old_sparse::EXTENSION_ENTRIES, &mut sparse.map);
                extended = next
                    .get(old_sparse::EXTENSION_IS_EXTENDED)
                    .is_some_and(|b| *b != 0);
            }
            let (at, len) = old_sparse::REAL_SIZE;
            sparse.real_size = block
                .get(at..at + len)
                .and_then(read_number)
                .unwrap_or(entry.member.size);
        } else if record("GNU.sparse.major").and_then(|v| pax_number(v)) == Some(1) {
            let (map, blocks) = self.read_data_map(&mut entry.raw)?;
            sparse.map = map;
            entry.data_size = entry.data_size.saturating_sub(blocks * BLOCK as u64);
            sparse.real_size = record("GNU.sparse.realsize")
                .and_then(|v| pax_number(v))
                .unwrap_or(entry.data_size);
        } else if let Some(map) = record("GNU.sparse.map") {
            let numbers: Vec<u64> = map
                .split(|b| *b == b',')
                .map(|n| pax_number(n).unwrap_or(0))
                .collect();
            sparse.map = numbers
                .chunks(2)
                .map(|pair| {
                    (
                        pair.first().copied().unwrap_or(0),
                        pair.get(1).copied().unwrap_or(0),
                    )
                })
                .collect();
        } else if pax.iter().any(|(k, _)| k == "GNU.sparse.offset") {
            for (key, value) in pax {
                match key.as_str() {
                    "GNU.sparse.offset" => sparse.map.push((pax_number(value).unwrap_or(0), 0)),
                    "GNU.sparse.numbytes" => {
                        if let Some(last) = sparse.map.last_mut() {
                            last.1 = pax_number(value).unwrap_or(0);
                        }
                    }
                    _ => {}
                }
            }
        } else {
            return Ok(None);
        }
        if entry.header.typeflag() != b'S' && sparse.real_size == 0 {
            sparse.real_size = record("GNU.sparse.realsize")
                .or_else(|| record("GNU.sparse.size"))
                .and_then(|v| pax_number(v))
                .unwrap_or(entry.data_size);
        }
        if let Some(name) = record("GNU.sparse.name") {
            entry.member.name.clone_from(name);
        }
        entry.member.size = sparse.real_size;
        Ok(Some(sparse))
    }

    /// Copies the current member's data blocks, padding and all, as they are.
    ///
    /// # Errors
    ///
    /// As [`Reader::copy_data`].
    pub fn copy_raw_data(&mut self, out: &mut dyn Write) -> Result<(), CopyError> {
        let mut left = self.remaining + self.padding;
        while left > 0 {
            let Some(block) = self.read_block().map_err(|e| CopyError::Read(e.into()))? else {
                self.remaining = 0;
                self.padding = 0;
                return Err(CopyError::Read(ReadError::UnexpectedEof));
            };
            out.write_all(&block).map_err(CopyError::Write)?;
            left = left.saturating_sub(BLOCK as u64);
        }
        self.remaining = 0;
        self.padding = 0;
        Ok(())
    }

    /// The next thing the archive holds.
    ///
    /// # Errors
    ///
    /// When reading fails, or the input ends inside a member's headers or data.
    pub fn next_event(&mut self) -> Result<Event, ReadError> {
        if let Some(event) = self.pending.pop_front() {
            return Ok(event);
        }
        if self.finished {
            return Ok(Event::End);
        }
        self.skip_data()?;
        let mut raw = Vec::new();
        let mut long_name: Option<Vec<u8>> = None;
        let mut long_link: Option<Vec<u8>> = None;
        let mut pax: Vec<(String, Vec<u8>)> = Vec::new();
        loop {
            let at = self.block;
            let Some(block) = self.read_block()? else {
                self.finished = true;
                self.end_block = Some(at);
                if self.prev == Status::StillUnread {
                    self.prev = Status::EndOfFile;
                    return Ok(Event::NotTar);
                }
                return Ok(Event::End);
            };
            let header = Header(block);
            match header.check() {
                Check::Zero => {
                    if self.ignore_zeros {
                        self.prev = Status::Zero;
                        continue;
                    }
                    let next = self.read_block()?;
                    self.finished = true;
                    self.end_block = Some(at);
                    match next {
                        Some(next) if Header(next).check() == Check::Zero => return Ok(Event::End),
                        _ => return Ok(Event::LoneZero(at + 1)),
                    }
                }
                Check::Bad => {
                    match self.prev {
                        Status::StillUnread => {
                            self.pending.push_back(Event::Skipping);
                            self.prev = Status::Failure;
                            return Ok(Event::NotTar);
                        }
                        Status::Zero | Status::Success => {
                            self.prev = Status::Failure;
                            return Ok(Event::Skipping);
                        }
                        Status::Failure | Status::EndOfFile => {}
                    }
                    self.prev = Status::Failure;
                }
                Check::Good => {
                    raw.extend_from_slice(&block);
                    let flag = header.typeflag();
                    let size = read_number(header.field(field::SIZE)).unwrap_or(0);
                    match flag {
                        b'L' => {
                            let mut data = self.read_header_data(size, &mut raw)?;
                            while data.last() == Some(&0) {
                                data.pop();
                            }
                            long_name = Some(data);
                            continue;
                        }
                        b'K' => {
                            let mut data = self.read_header_data(size, &mut raw)?;
                            while data.last() == Some(&0) {
                                data.pop();
                            }
                            long_link = Some(data);
                            continue;
                        }
                        b'x' | b'X' => {
                            let data = self.read_header_data(size, &mut raw)?;
                            pax.extend(pax_records(&data));
                            continue;
                        }
                        b'g' => {
                            let data = self.read_header_data(size, &mut raw)?;
                            self.global.extend(pax_records(&data));
                            continue;
                        }
                        _ => {}
                    }
                    self.prev = Status::Success;
                    let mut entry = self.entry(&header, at, raw, long_name, long_link, &pax, size);
                    let sparse = self.sparse_of(&mut entry, &pax)?;
                    self.sparse = sparse;
                    self.remaining = entry.data_size;
                    self.padding =
                        entry.data_size.div_ceil(BLOCK as u64) * BLOCK as u64 - entry.data_size;
                    return Ok(Event::Member(Box::new(entry)));
                }
            }
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the parts a member is gathered from"
    )]
    fn entry(
        &self,
        header: &Header,
        block: u64,
        raw: Vec<u8>,
        long_name: Option<Vec<u8>>,
        long_link: Option<Vec<u8>>,
        pax: &[(String, Vec<u8>)],
        size: u64,
    ) -> Entry {
        let magic = header.field(field::MAGIC);
        let version = header.field(field::VERSION);
        let layout = if magic == b"ustar " && version == b" \0" {
            Layout::Gnu
        } else if magic == b"ustar\0" {
            Layout::Ustar
        } else {
            Layout::V7
        };
        let records: Vec<&(String, Vec<u8>)> = self.global.iter().chain(pax).collect();
        let record = |key: &str| {
            records
                .iter()
                .rev()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        };
        let name = long_name.or_else(|| record("path")).unwrap_or_else(|| {
            let name = header.text(field::NAME).to_vec();
            let prefix = header.text(field::PREFIX);
            if layout == Layout::Ustar && !prefix.is_empty() {
                let mut whole = prefix.to_vec();
                whole.push(b'/');
                whole.extend_from_slice(&name);
                whole
            } else {
                name
            }
        });
        let link = long_link
            .or_else(|| record("linkpath"))
            .unwrap_or_else(|| header.text(field::LINKNAME).to_vec());
        let number =
            |key: &str| record(key).and_then(|v| std::str::from_utf8(&v).ok()?.parse::<u64>().ok());
        let size = number("size").unwrap_or(size);
        let flag = header.typeflag();
        let kind = kind_of(flag, &name);
        let mtime = record("mtime")
            .and_then(|v| pax_time(&v))
            .unwrap_or_else(|| {
                Timestamp::seconds(read_time(header.field(field::MTIME)).unwrap_or(0))
            });
        let member = Member {
            name,
            kind: Some(kind),
            size,
            mode: u32::try_from(read_number(header.field(field::MODE)).unwrap_or(0) & 0o7777)
                .unwrap_or(0),
            uid: number("uid")
                .unwrap_or_else(|| read_number(header.field(field::UID)).unwrap_or(0)),
            gid: number("gid")
                .unwrap_or_else(|| read_number(header.field(field::GID)).unwrap_or(0)),
            uname: record("uname").unwrap_or_else(|| header.text(field::UNAME).to_vec()),
            gname: record("gname").unwrap_or_else(|| header.text(field::GNAME).to_vec()),
            mtime,
            atime: record("atime").and_then(|v| pax_time(&v)),
            ctime: record("ctime").and_then(|v| pax_time(&v)),
            link,
            device: (
                u32::try_from(read_number(header.field(field::DEVMAJOR)).unwrap_or(0)).unwrap_or(0),
                u32::try_from(read_number(header.field(field::DEVMINOR)).unwrap_or(0)).unwrap_or(0),
            ),
        };
        // GNU tar passes over a member's size worth of data for every kind but a folder.
        let data_size = if kind == Kind::Dir { 0 } else { size };
        Entry {
            member,
            header: *header,
            block,
            raw,
            layout,
            data_size,
        }
    }
}
