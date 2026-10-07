//! gzip members, as GNU gzip 1.14 reads and writes them, for the `gzip` command (D78).
//!
//! The header with its fields (the stored name and time, an extra field, a comment, a
//! header CRC), the deflate stream, the trailer's CRC and length, and what stands after
//! a member: another one, zeros, or garbage. [`read_start`] says which; [`inflate_member`]
//! and [`deflate_all`] move the data; [`header`] makes a header as Git for Windows' gzip
//! writes one. Problems are [`Trouble`]s and [`HeaderProblem`]s, which the command words.

use std::io::{self, Read, Write};

use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};

/// The header's OS byte: Unix, as Git for Windows' gzip writes it.
pub const OS_UNIX: u8 = 3;
/// The deflate method byte, the only one there is.
pub const DEFLATED: u8 = 8;
/// The header's flag bit for a header CRC.
pub const FHCRC: u8 = 0x02;
/// The flag bit for an extra field.
pub const FEXTRA: u8 = 0x04;
/// The flag bit for a stored name.
pub const FNAME: u8 = 0x08;
/// The flag bit for a comment.
pub const FCOMMENT: u8 = 0x10;
/// The flag bit of encryption, which gzip never had.
pub const FENCRYPTED: u8 = 0x20;
/// The reserved flag bits.
pub const FRESERVED: u8 = 0xC0;
/// How much is read and written at a time.
pub const CHUNK: usize = 64 * 1024;

/// What went wrong inside a compressed stream.
#[derive(Debug)]
pub enum Trouble {
    /// The input could not be read.
    Read(io::Error),
    /// The output could not be written.
    Write(io::Error),
    /// The input ended inside a member.
    Eof,
    /// The deflate stream is not one.
    Format,
    /// The trailer's CRC does not match the data.
    Crc,
    /// The trailer's length does not match the data.
    Length,
}

/// Buffered reading of one input, a byte or a chunk at a time.
pub struct Input {
    reader: Box<dyn Read>,
    buf: Vec<u8>,
    pos: usize,
    len: usize,
    eof: bool,
    /// Bytes read from the reader so far, what GNU gzip counts as `bytes_in`.
    pub read_total: u64,
}

impl Input {
    /// Buffered reading of `reader`.
    pub fn new(reader: Box<dyn Read>) -> Self {
        Self {
            reader,
            buf: vec![0; CHUNK],
            pos: 0,
            len: 0,
            eof: false,
            read_total: 0,
        }
    }

    /// Makes sure bytes are buffered; `false` at the end of the input.
    pub fn fill(&mut self) -> io::Result<bool> {
        if self.pos < self.len {
            return Ok(true);
        }
        if self.eof {
            return Ok(false);
        }
        loop {
            match self.reader.read(&mut self.buf) {
                Ok(0) => {
                    self.eof = true;
                    return Ok(false);
                }
                Ok(n) => {
                    self.pos = 0;
                    self.len = n;
                    self.read_total += n as u64;
                    return Ok(true);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    }

    /// The next byte, `None` at the end.
    pub fn byte(&mut self) -> io::Result<Option<u8>> {
        if !self.fill()? {
            return Ok(None);
        }
        let byte = self.buf.get(self.pos).copied();
        self.pos += 1;
        Ok(byte)
    }

    /// The bytes buffered and not yet taken.
    pub fn available(&self) -> &[u8] {
        self.buf.get(self.pos..self.len).unwrap_or_default()
    }

    /// Takes `n` of the buffered bytes.
    pub fn advance(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.len);
    }

    /// Whether the input has ended.
    pub fn at_end(&mut self) -> io::Result<bool> {
        Ok(!self.fill()?)
    }
}

/// A member's header, read.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Member {
    /// The stored time, 0 when none.
    pub mtime: u32,
    /// The stored name, when there is one and it is wanted.
    pub name: Option<Vec<u8>>,
    /// The header's length in bytes.
    pub header_len: u64,
    /// Bytes of an extra field, said in verbose mode.
    pub extra: Option<u16>,
}

/// A gzip header that cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderProblem {
    /// The message; GNU gzip puts a colon after the name for some, a space for others.
    pub text: String,
    /// Whether a colon, not a space, follows the name.
    pub colon: bool,
}

impl HeaderProblem {
    /// The message about `name`.
    pub fn shown(&self, name: &str) -> String {
        if self.colon {
            format!("{name}: {}", self.text)
        } else {
            format!("{name} {}", self.text)
        }
    }
}

/// What stands at the start of a member.
#[derive(Debug, PartialEq, Eq)]
pub enum Start {
    /// A member, its header read.
    Member(Member),
    /// `-f -c` on data that is not gzip's: these bytes, then the rest, pass unchanged.
    Passthrough(Vec<u8>),
    /// A format cash's gzip does not carry.
    Refused(&'static str),
    /// Not gzip's magic at the first member.
    NotGzip,
    /// A gzip header that cannot be used.
    Problem(HeaderProblem),
    /// After a member: nothing more.
    End,
    /// After a member: zero bytes to the end.
    Zeros,
    /// After a member: something that is not another member.
    Garbage,
}

/// Reads a member's start. `part` is 1 for the first member; `name_wanted` says whether
/// a stored name is kept; `passthrough` is `-f -c`, which copies what is not gzip's.
#[expect(clippy::too_many_lines, reason = "one header field after another")]
pub fn read_start(
    input: &mut Input,
    part: u32,
    name_wanted: bool,
    passthrough: bool,
) -> Result<Start, Trouble> {
    let need = |input: &mut Input| -> Result<u8, Trouble> {
        input.byte().map_err(Trouble::Read)?.ok_or(Trouble::Eof)
    };
    let Some(m0) = input.byte().map_err(Trouble::Read)? else {
        // Nothing at all: `-f -c` passes it on as it is, nothing.
        return if part > 1 {
            Ok(Start::End)
        } else if passthrough {
            Ok(Start::Passthrough(Vec::new()))
        } else {
            Err(Trouble::Eof)
        };
    };
    let m1 = input.byte().map_err(Trouble::Read)?;
    let magic = [m0, m1.unwrap_or(0)];
    let is_gzip = m1.is_some() && (magic == [0x1f, 0x8b] || magic == [0x1f, 0x9e]);
    if !is_gzip {
        if part > 1 {
            if m0 != 0 {
                return if m1.is_none() {
                    Err(Trouble::Eof)
                } else {
                    Ok(Start::Garbage)
                };
            }
            let mut next = m1;
            loop {
                match next {
                    None => return Ok(Start::Zeros),
                    Some(0) => next = input.byte().map_err(Trouble::Read)?,
                    Some(_) => return Ok(Start::Garbage),
                }
            }
        }
        let refused = match magic {
            [0x1f, 0x9d] => Some("compress (LZW) format"),
            [0x1f, 0x1e] => Some("pack format"),
            [0x1f, 0xa0] => Some("lzh format"),
            [b'P', b'K'] => Some("zip format"),
            _ => None,
        };
        if let Some(format) = refused.filter(|_| m1.is_some()) {
            return Ok(Start::Refused(format));
        }
        if passthrough {
            let mut taken = vec![m0];
            taken.extend(m1);
            return Ok(Start::Passthrough(taken));
        }
        return if m1.is_none() {
            Err(Trouble::Eof)
        } else {
            Ok(Start::NotGzip)
        };
    }

    let mut header = magic.to_vec();
    let method = need(input)?;
    header.push(method);
    if method != DEFLATED {
        return Ok(Start::Problem(HeaderProblem {
            text: format!("unknown method {method} -- not supported"),
            colon: true,
        }));
    }
    let flags = need(input)?;
    header.push(flags);
    if flags & FENCRYPTED != 0 {
        return Ok(Start::Problem(HeaderProblem {
            text: "is encrypted -- not supported".to_owned(),
            colon: false,
        }));
    }
    if flags & FRESERVED != 0 {
        return Ok(Start::Problem(HeaderProblem {
            text: format!("has flags 0x{flags:x} -- not supported"),
            colon: false,
        }));
    }
    let mut stamp = [0u8; 4];
    for byte in &mut stamp {
        *byte = need(input)?;
    }
    header.extend_from_slice(&stamp);
    let mtime = u32::from_le_bytes(stamp);
    header.push(need(input)?); // extra flags
    header.push(need(input)?); // OS
    let mut member = Member {
        mtime,
        ..Member::default()
    };
    if flags & FEXTRA != 0 {
        let low = need(input)?;
        let high = need(input)?;
        header.extend_from_slice(&[low, high]);
        let len = u16::from_le_bytes([low, high]);
        member.extra = Some(len);
        for _ in 0..len {
            header.push(need(input)?);
        }
    }
    if flags & FNAME != 0 {
        let mut name = Vec::new();
        loop {
            let byte = need(input)?;
            header.push(byte);
            if byte == 0 {
                break;
            }
            name.push(byte);
        }
        if name_wanted {
            member.name = Some(name);
        }
    }
    if flags & FCOMMENT != 0 {
        loop {
            let byte = need(input)?;
            header.push(byte);
            if byte == 0 {
                break;
            }
        }
    }
    if flags & FHCRC != 0 {
        let computed = crc32fast::hash(&header) & 0xffff;
        let low = need(input)?;
        let high = need(input)?;
        header.extend_from_slice(&[low, high]);
        let found = u16::from_le_bytes([low, high]);
        if u32::from(found) != computed {
            return Ok(Start::Problem(HeaderProblem {
                text: format!(
                    "header checksum 0x{found:04x} != computed checksum 0x{computed:04x}"
                ),
                colon: true,
            }));
        }
    }
    member.header_len = header.len() as u64;
    Ok(Start::Member(member))
}

/// A gzip header for what is compressed.
pub fn header(mtime: u32, level: u32, name: Option<&[u8]>) -> Vec<u8> {
    let mut header = vec![0x1f, 0x8b, DEFLATED, if name.is_some() { FNAME } else { 0 }];
    header.extend_from_slice(&mtime.to_le_bytes());
    header.push(match level {
        9 => 2,
        1 => 4,
        _ => 0,
    });
    header.push(OS_UNIX);
    if let Some(name) = name {
        header.extend_from_slice(name);
        header.push(0);
    }
    header
}

/// The low 32 bits of a size, as the trailer stores it.
pub fn low_32(size: u64) -> u32 {
    u32::try_from(size & 0xffff_ffff).unwrap_or(0)
}

/// The bytes and the CRC of one member's data.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// The data's CRC-32.
    pub crc: u32,
    /// The data's length.
    pub size: u64,
}

/// Inflates one member's deflate stream from `input` into `out`, then checks its
/// trailer. `written` counts what `out` took.
pub fn inflate_member(
    input: &mut Input,
    out: &mut dyn Write,
    written: &mut u64,
) -> Result<Counts, Trouble> {
    let mut inflater = Decompress::new(false);
    let mut out_buf = vec![0u8; 2 * CHUNK];
    let mut hasher = crc32fast::Hasher::new();
    let mut size = 0u64;
    loop {
        let more = input.fill().map_err(Trouble::Read)?;
        let (before_in, before_out) = (inflater.total_in(), inflater.total_out());
        let status = inflater
            .decompress(input.available(), &mut out_buf, FlushDecompress::None)
            .map_err(|_| Trouble::Format)?;
        let used = to_usize(inflater.total_in() - before_in);
        let made = to_usize(inflater.total_out() - before_out);
        input.advance(used);
        let produced = out_buf.get(..made).unwrap_or_default();
        hasher.update(produced);
        out.write_all(produced).map_err(Trouble::Write)?;
        size += made as u64;
        *written += made as u64;
        match status {
            Status::StreamEnd => break,
            Status::Ok | Status::BufError => {
                if used == 0 && made == 0 {
                    return Err(if more { Trouble::Format } else { Trouble::Eof });
                }
            }
        }
    }
    let mut trailer = [0u8; 8];
    for byte in &mut trailer {
        *byte = input.byte().map_err(Trouble::Read)?.ok_or(Trouble::Eof)?;
    }
    let crc = hasher.finalize();
    if u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]) != crc {
        return Err(Trouble::Crc);
    }
    if u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]) != low_32(size) {
        return Err(Trouble::Length);
    }
    Ok(Counts { crc, size })
}

/// Deflates all of `input` into `out` at `level`, with the trailer; the header is the
/// caller's. Returns the data's counts and the bytes written here.
pub fn deflate_all(
    input: &mut Input,
    out: &mut dyn Write,
    level: u32,
) -> Result<(Counts, u64), Trouble> {
    let mut deflater = Compress::new(Compression::new(level), false);
    let mut out_buf = vec![0u8; 2 * CHUNK];
    let mut hasher = crc32fast::Hasher::new();
    let mut size = 0u64;
    let mut written = 0u64;
    loop {
        let more = input.fill().map_err(Trouble::Read)?;
        let flush = if more {
            FlushCompress::None
        } else {
            FlushCompress::Finish
        };
        let (before_in, before_out) = (deflater.total_in(), deflater.total_out());
        let status = deflater
            .compress(input.available(), &mut out_buf, flush)
            .map_err(|_| Trouble::Format)?;
        let used = to_usize(deflater.total_in() - before_in);
        let made = to_usize(deflater.total_out() - before_out);
        hasher.update(input.available().get(..used).unwrap_or_default());
        input.advance(used);
        size += used as u64;
        let produced = out_buf.get(..made).unwrap_or_default();
        out.write_all(produced).map_err(Trouble::Write)?;
        written += made as u64;
        if status == Status::StreamEnd {
            break;
        }
    }
    let crc = hasher.finalize();
    let mut trailer = crc.to_le_bytes().to_vec();
    trailer.extend_from_slice(&low_32(size).to_le_bytes());
    out.write_all(&trailer).map_err(Trouble::Write)?;
    written += trailer.len() as u64;
    Ok((Counts { crc, size }, written))
}

/// What each thread deflates on its own: pigz's layout, without its shared window.
pub const PARALLEL_CHUNK: usize = 1 << 20;

/// The next `PARALLEL_CHUNK` bytes of `input`, fewer at its end.
fn next_chunk(input: &mut Input) -> io::Result<Vec<u8>> {
    let mut chunk = Vec::with_capacity(PARALLEL_CHUNK);
    while chunk.len() < PARALLEL_CHUNK && input.fill()? {
        let take = input.available().len().min(PARALLEL_CHUNK - chunk.len());
        chunk.extend_from_slice(input.available().get(..take).unwrap_or_default());
        input.advance(take);
    }
    Ok(chunk)
}

/// One chunk deflated apart: ended with a sync flush, so the next one's blocks follow
/// on a byte, or, the `last`, with the final block.
fn deflate_chunk(data: &[u8], level: u32, last: bool) -> io::Result<Vec<u8>> {
    let mut deflater = Compress::new(Compression::new(level), false);
    let mut out = Vec::with_capacity(data.len() / 2 + 1024);
    let flush = if last {
        FlushCompress::Finish
    } else {
        FlushCompress::Sync
    };
    loop {
        if out.capacity() - out.len() < 1024 {
            out.reserve(out.capacity().max(64 << 10));
        }
        let taken = to_usize(deflater.total_in());
        let status = deflater
            .compress_vec(data.get(taken..).unwrap_or_default(), &mut out, flush)
            .map_err(io::Error::other)?;
        let all_in = to_usize(deflater.total_in()) == data.len();
        if last && status == Status::StreamEnd {
            return Ok(out);
        }
        // A flush is done when it leaves room in the output.
        if !last && all_in && out.len() < out.capacity() {
            return Ok(out);
        }
    }
}

/// Writes one gzip member as tar's `gzip` does (no name, no time), its chunks deflated
/// `threads` at once as [`deflate_parallel`] lays them out.
pub struct ParallelWriter<W: Write> {
    inner: W,
    pending: Vec<u8>,
    level: u32,
    threads: usize,
    hasher: crc32fast::Hasher,
    size: u64,
    /// Whether the header is written.
    started: bool,
}

impl<W: Write> ParallelWriter<W> {
    /// A writer into `inner` at `level`.
    pub fn new(inner: W, level: u32, threads: usize) -> Self {
        Self {
            inner,
            pending: Vec::new(),
            level,
            threads: threads.max(1),
            hasher: crc32fast::Hasher::new(),
            size: 0,
            started: false,
        }
    }

    /// The pending chunks deflated at once and written, the last one ended if `last`.
    fn chunks(&mut self, last: bool) -> io::Result<()> {
        if !self.started {
            self.inner.write_all(&header(0, self.level, None))?;
            self.started = true;
        }
        let level = self.level;
        let chunks: Vec<&[u8]> = if self.pending.is_empty() {
            vec![&[]]
        } else {
            self.pending.chunks(PARALLEL_CHUNK).collect()
        };
        let count = chunks.len();
        let jobs: Vec<(usize, &[u8])> = chunks.into_iter().enumerate().collect();
        let deflated = super::parallel::map_ordered(jobs, |(at, chunk)| {
            deflate_chunk(chunk, level, last && at + 1 == count)
        })?;
        self.hasher.update(&self.pending);
        self.size += self.pending.len() as u64;
        self.pending.clear();
        for packed in deflated {
            self.inner.write_all(&packed)?;
        }
        Ok(())
    }

    /// Writes the last chunks and the trailer, and hands the writer back.
    ///
    /// # Errors
    ///
    /// When it cannot be written.
    pub fn finish(mut self) -> io::Result<W> {
        self.chunks(true)?;
        let crc = self.hasher.clone().finalize();
        self.inner.write_all(&crc.to_le_bytes())?;
        self.inner.write_all(&low_32(self.size).to_le_bytes())?;
        self.inner.flush()?;
        Ok(self.inner)
    }
}

impl<W: Write> Write for ParallelWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // One chunk more than the threads take, kept back: the last one is ended.
        let batch = PARALLEL_CHUNK * (self.threads + 1);
        let taken = buf.len().min(batch - self.pending.len());
        self.pending
            .extend_from_slice(buf.get(..taken).unwrap_or_default());
        if self.pending.len() >= batch {
            let keep = self.pending.split_off(PARALLEL_CHUNK * self.threads);
            self.chunks(false)?;
            self.pending = keep;
        }
        Ok(taken)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// [`deflate_all`] laid out for threads: chunks deflated apart, `threads` at once.
///
/// Each chunk but the last ends with a sync flush, in one member whose CRC is the
/// whole's (pigz's layout without its shared window). The bytes are the same for any
/// number of threads; input of one chunk is [`deflate_all`]'s, byte for byte.
///
/// # Errors
///
/// As [`deflate_all`]'s.
pub fn deflate_parallel(
    input: &mut Input,
    out: &mut dyn Write,
    level: u32,
    threads: usize,
) -> Result<(Counts, u64), Trouble> {
    let threads = threads.max(1);
    let first = next_chunk(input).map_err(Trouble::Read)?;
    if input.at_end().map_err(Trouble::Read)? {
        let mut alone = Input::new(Box::new(io::Cursor::new(first)));
        return deflate_all(&mut alone, out, level);
    }
    let mut hasher = crc32fast::Hasher::new();
    let mut size = 0u64;
    let mut written = 0u64;
    let mut pending = vec![first];
    loop {
        while pending.len() < threads {
            let chunk = next_chunk(input).map_err(Trouble::Read)?;
            if chunk.is_empty() {
                break;
            }
            pending.push(chunk);
        }
        let ended = input.at_end().map_err(Trouble::Read)?;
        let count = pending.len();
        let jobs: Vec<(usize, &Vec<u8>)> = pending.iter().enumerate().collect();
        let deflated = super::parallel::map_ordered(jobs, |(at, chunk)| {
            deflate_chunk(chunk, level, ended && at + 1 == count)
        })
        .map_err(|_| Trouble::Format)?;
        for (chunk, packed) in pending.iter().zip(&deflated) {
            hasher.update(chunk);
            size += chunk.len() as u64;
            out.write_all(packed).map_err(Trouble::Write)?;
            written += packed.len() as u64;
        }
        pending.clear();
        if ended {
            break;
        }
    }
    let crc = hasher.finalize();
    let mut trailer = crc.to_le_bytes().to_vec();
    trailer.extend_from_slice(&low_32(size).to_le_bytes());
    out.write_all(&trailer).map_err(Trouble::Write)?;
    written += trailer.len() as u64;
    Ok((Counts { crc, size }, written))
}

/// A count as a `usize`, as large as can be when it does not fit.
pub fn to_usize(n: u64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

#[cfg(test)]
#[allow(clippy::panic, reason = "a test that finds no member stops loudly")]
mod tests {
    use super::*;

    #[test]
    fn the_header_is_gnu_s() {
        assert_eq!(
            header(0x5e0d_5da5, 6, Some(b"h")),
            vec![
                0x1f, 0x8b, 0x08, 0x08, 0xa5, 0x5d, 0x0d, 0x5e, 0x00, 0x03, b'h', 0
            ]
        );
        assert_eq!(
            header(0, 9, None),
            vec![0x1f, 0x8b, 0x08, 0x00, 0, 0, 0, 0, 0x02, 0x03]
        );
        assert_eq!(header(0, 1, None)[8], 0x04);
    }

    fn start_of(bytes: &[u8], part: u32, passthrough: bool) -> Result<Start, Trouble> {
        let mut input = Input::new(Box::new(io::Cursor::new(bytes.to_vec())));
        read_start(&mut input, part, true, passthrough)
    }

    #[test]
    fn a_header_is_read_with_its_fields() {
        let bytes = b"\x1f\x8b\x08\x08\xa5\x5d\x0d\x5e\x00\x03h\x00";
        let Ok(Start::Member(member)) = start_of(bytes, 1, false) else {
            panic!("not a member");
        };
        assert_eq!(member.mtime, 0x5e0d_5da5);
        assert_eq!(member.name.as_deref(), Some(&b"h"[..]));
        assert_eq!(member.header_len, 12);
        assert_eq!(member.extra, None);

        // An extra field, a comment and a header CRC.
        let mut bytes = b"\x1f\x8b\x08\x16\0\0\0\0\0\x03\x02\0ABc\x00".to_vec();
        let crc16 = u16::try_from(crc32fast::hash(&bytes) & 0xffff).unwrap_or(0);
        bytes.extend_from_slice(&crc16.to_le_bytes());
        let Ok(Start::Member(member)) = start_of(&bytes, 1, false) else {
            panic!("not a member");
        };
        assert_eq!(member.extra, Some(2));
        assert_eq!(member.header_len, 18);
        let mut wrong = bytes.clone();
        wrong[16] ^= 1;
        assert!(matches!(
            start_of(&wrong, 1, false),
            Ok(Start::Problem(HeaderProblem { colon: true, .. }))
        ));

        assert!(matches!(start_of(b"hello\n", 1, false), Ok(Start::NotGzip)));
        assert!(matches!(
            start_of(b"hello\n", 1, true),
            Ok(Start::Passthrough(_))
        ));
        assert!(matches!(
            start_of(b"\x1f", 1, true),
            Ok(Start::Passthrough(_))
        ));
        assert!(matches!(start_of(b"x", 1, false), Err(Trouble::Eof)));
        assert!(matches!(start_of(b"", 1, false), Err(Trouble::Eof)));
        assert!(matches!(start_of(b"\x1f\x8b", 1, false), Err(Trouble::Eof)));
        assert!(matches!(
            start_of(b"\x1f\x9d", 1, false),
            Ok(Start::Refused(_))
        ));
        assert!(matches!(
            start_of(b"\x1f\x8b\x09", 1, false),
            Ok(Start::Problem(HeaderProblem { colon: true, .. }))
        ));
        assert!(matches!(
            start_of(b"\x1f\x8b\x08\x20", 1, false),
            Ok(Start::Problem(HeaderProblem { colon: false, .. }))
        ));
        // After a member: nothing, zeros, garbage, or a byte and no more.
        assert!(matches!(start_of(b"", 2, false), Ok(Start::End)));
        assert!(matches!(start_of(b"\0\0\0", 2, false), Ok(Start::Zeros)));
        assert!(matches!(start_of(b"\0\0x", 2, false), Ok(Start::Garbage)));
        assert!(matches!(start_of(b"junk", 2, false), Ok(Start::Garbage)));
        assert!(matches!(start_of(b"j", 2, false), Err(Trouble::Eof)));
    }

    /// `data` deflated on four threads, then inflated: the data, and its counts.
    fn parallel_round_trip(data: &[u8]) -> (Vec<u8>, Vec<u8>, Counts) {
        let mut packed = Vec::new();
        let mut input = Input::new(Box::new(io::Cursor::new(data.to_vec())));
        let (counts, written) = deflate_parallel(&mut input, &mut packed, 6, 4).unwrap();
        assert_eq!(written, packed.len() as u64);
        let mut back = Vec::new();
        let mut written_back = 0;
        let mut reading = Input::new(Box::new(io::Cursor::new(packed.clone())));
        let read = inflate_member(&mut reading, &mut back, &mut written_back).unwrap();
        assert_eq!(read, counts);
        (packed, back, counts)
    }

    #[test]
    fn chunks_deflated_apart_inflate_as_one_member() {
        let text: Vec<u8> = (0..900_000u32)
            .flat_map(|i| format!("{} ", i % 4099).into_bytes())
            .collect();
        for len in [text.len(), 2 * PARALLEL_CHUNK, 2 * PARALLEL_CHUNK + 1] {
            let data = text.get(..len).unwrap_or(&text);
            let (_, back, counts) = parallel_round_trip(data);
            assert_eq!(back, data);
            assert_eq!(counts.crc, crc32fast::hash(data));
        }
    }

    #[test]
    fn one_chunk_is_deflated_as_before() {
        // 800 KB: under one chunk.
        let data: Vec<u8> = (0..200_000u32)
            .flat_map(|i| (i % 300).to_le_bytes())
            .collect();
        let (packed, _, _) = parallel_round_trip(&data);
        let mut before = Vec::new();
        let mut input = Input::new(Box::new(io::Cursor::new(data)));
        deflate_all(&mut input, &mut before, 6).unwrap();
        assert_eq!(packed, before);
        let (empty, back, _) = parallel_round_trip(&[]);
        assert_eq!(back, b"");
        let mut before = Vec::new();
        deflate_all(&mut Input::new(Box::new(io::empty())), &mut before, 6).unwrap();
        assert_eq!(empty, before);
    }

    #[test]
    fn a_round_trip_keeps_the_bytes_and_the_counts() {
        let text = b"hello hello hello hello\n".repeat(1000);
        let mut input = Input::new(Box::new(io::Cursor::new(text.clone())));
        let mut compressed = header(0, 6, None);
        let (counts, written) = deflate_all(&mut input, &mut compressed, 6).unwrap_or_default();
        assert_eq!(counts.size, text.len() as u64);
        assert_eq!(compressed.len() as u64, 10 + written);
        assert!(compressed.len() < text.len() / 10);

        let mut input = Input::new(Box::new(io::Cursor::new(compressed.clone())));
        let Ok(Start::Member(member)) = read_start(&mut input, 1, false, false) else {
            panic!("not a member");
        };
        assert_eq!(member.header_len, 10);
        let mut out = Vec::new();
        let mut taken = 0;
        let back = inflate_member(&mut input, &mut out, &mut taken).unwrap_or_default();
        assert_eq!(out, text);
        assert_eq!(back, counts);
        assert_eq!(taken, text.len() as u64);
        assert!(input.at_end().unwrap_or(false));

        // The trailer is checked.
        let mut corrupt = compressed.clone();
        let at = corrupt.len() - 8;
        corrupt[at] ^= 1;
        let mut input = Input::new(Box::new(io::Cursor::new(corrupt)));
        let _ = read_start(&mut input, 1, false, false);
        assert!(matches!(
            inflate_member(&mut input, &mut io::sink(), &mut 0),
            Err(Trouble::Crc)
        ));
        let mut truncated = compressed.clone();
        truncated.truncate(15);
        let mut input = Input::new(Box::new(io::Cursor::new(truncated)));
        let _ = read_start(&mut input, 1, false, false);
        assert!(matches!(
            inflate_member(&mut input, &mut io::sink(), &mut 0),
            Err(Trouble::Eof)
        ));
        let mut bad = compressed;
        bad[10] = 0xff;
        let mut input = Input::new(Box::new(io::Cursor::new(bad)));
        let _ = read_start(&mut input, 1, false, false);
        assert!(matches!(
            inflate_member(&mut input, &mut io::sink(), &mut 0),
            Err(Trouble::Format)
        ));
    }
}
