//! The formats of XZ Utils' `xz`: .xz, .lzma, .lz and raw LZMA2.
//!
//! What `xz` needs beyond a reader and a writer: the format of its input told by xz's
//! own tests ([`detect`]), decoding that counts the bytes read and tells a truncated
//! stream from a corrupt one ([`decompress`]), the presets with xz's "extreme" variant
//! ([`Settings`]), and a .xz file's streams and blocks, read from its index backwards
//! as `xz --list` reads them ([`file_info`]).

use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};
use std::num::NonZeroU64;

use lzma_rust2::{
    Action, CheckType, EncodeMode, LzipStream, Lzma2Options, Lzma2Stream, Lzma2Writer, LzmaOptions,
    LzmaStream, LzmaWriter, MfType, Status, StreamResult, XzOptions, XzStream, XzWriter,
};

use super::Encoder;

/// What xz reads before it decides what its input is.
pub const SNIFF: usize = 8192;

/// The .xz magic.
const XZ_MAGIC: [u8; 6] = [0xfd, b'7', b'z', b'X', b'Z', 0];

/// The size of a .xz stream's header and of its footer.
const HEADER: u64 = 12;
const HEADER_LEN: usize = 12;

/// The formats xz reads and writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    /// .xz, a container of blocks with an index and a check.
    Xz,
    /// .lzma, `LZMA_Alone`: a 13-byte header and one LZMA stream.
    Lzma,
    /// .lz, lzip's members.
    Lzip,
    /// Raw LZMA2, with no header at all.
    Raw,
}

/// Whether `first` starts a .xz stream.
pub fn is_xz(first: &[u8]) -> bool {
    first.starts_with(&XZ_MAGIC)
}

/// Whether `first` starts an lzip member.
pub fn is_lzip(first: &[u8]) -> bool {
    first.starts_with(b"LZIP")
}

/// Whether `first` looks like a .lzma header, by xz's own test: valid properties, a
/// dictionary of 2^n or 2^n + 2^(n-1) bytes (or the largest), and a known size below
/// 256 GiB.
pub fn is_lzma(first: &[u8]) -> bool {
    let Some(header) = first.get(..13) else {
        return false;
    };
    let mut props = u32::from(header.first().copied().unwrap_or(u8::MAX));
    if props > (4 * 5 + 4) * 9 + 8 {
        return false;
    }
    let pb = props / 45;
    props -= pb * 45;
    let lp = props / 9;
    let lc = props - lp * 9;
    if lc + lp > 4 {
        return false;
    }
    let dict = u32::from_le_bytes([
        header.get(1).copied().unwrap_or(0),
        header.get(2).copied().unwrap_or(0),
        header.get(3).copied().unwrap_or(0),
        header.get(4).copied().unwrap_or(0),
    ]);
    if dict != u32::MAX {
        let mut d = dict.wrapping_sub(1);
        d |= d >> 2;
        d |= d >> 3;
        d |= d >> 4;
        d |= d >> 8;
        d |= d >> 16;
        d = d.wrapping_add(1);
        if d != dict || dict == 0 {
            return false;
        }
    }
    let size = header
        .get(5..13)
        .and_then(|bytes| bytes.try_into().ok())
        .map_or(u64::MAX, u64::from_le_bytes);
    size == u64::MAX || size <= 1 << 38
}

/// The format of input that starts with `first`, as xz decides it: .xz, then .lz, then
/// .lzma. Raw data has no mark and is never found.
pub fn detect(first: &[u8]) -> Option<Format> {
    if is_xz(first) {
        Some(Format::Xz)
    } else if is_lzip(first) {
        Some(Format::Lzip)
    } else if is_lzma(first) {
        Some(Format::Lzma)
    } else {
        None
    }
}

/// Whether input that starts with `first` is in `format`, as xz checks a format the
/// user named.
pub fn is_format(format: Format, first: &[u8]) -> bool {
    match format {
        Format::Xz => is_xz(first),
        Format::Lzma => is_lzma(first),
        Format::Lzip => is_lzip(first),
        Format::Raw => true,
    }
}

/// Reads up to [`SNIFF`] bytes, as many as there are: what xz looks at to tell the
/// format.
///
/// # Errors
///
/// When reading fails.
pub fn sniff(input: &mut dyn Read) -> io::Result<Vec<u8>> {
    let mut first = Vec::with_capacity(SNIFF);
    input.take(SNIFF as u64).read_to_end(&mut first)?;
    Ok(first)
}

/// What stopped decoding.
#[derive(Debug)]
pub enum Broken {
    /// The input ends inside a stream: xz's "Unexpected end of input".
    Truncated,
    /// The data is damaged, a check fails, or something follows the stream that may not:
    /// xz's "Compressed data is corrupt".
    Corrupt,
    /// The stream uses something this decoder does not have: xz's "Unsupported
    /// options".
    Unsupported,
    /// Reading the input failed.
    Read(io::Error),
    /// Writing the output failed.
    Write(io::Error),
}

/// A push decoder of lzma-rust2's.
trait Push {
    fn push(&mut self, input: &[u8], output: &mut [u8], action: Action)
    -> io::Result<StreamResult>;
}

macro_rules! push {
    ($type:ty) => {
        impl Push for $type {
            fn push(
                &mut self,
                input: &[u8],
                output: &mut [u8],
                action: Action,
            ) -> io::Result<StreamResult> {
                self.process(input, output, action)
            }
        }
    };
}

push!(XzStream);
push!(LzmaStream);
push!(LzipStream);
push!(Lzma2Stream);

fn broken(error: &io::Error) -> Broken {
    match error.kind() {
        io::ErrorKind::UnexpectedEof => Broken::Truncated,
        io::ErrorKind::Unsupported => Broken::Unsupported,
        _ => Broken::Corrupt,
    }
}

/// Feeds `input` to `decoder` until its stream ends: the bytes read.
fn drive(
    decoder: &mut dyn Push,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<u64, Broken> {
    let mut buffer = vec![0_u8; 64 << 10];
    let mut read = 0_u64;
    loop {
        let available = input.fill_buf().map_err(Broken::Read)?;
        let action = if available.is_empty() {
            Action::Finish
        } else {
            Action::Run
        };
        let result = decoder
            .push(available, &mut buffer, action)
            .map_err(|e| broken(&e))?;
        input.consume(result.bytes_consumed);
        read += result.bytes_consumed as u64;
        let produced = buffer.get(..result.bytes_produced).unwrap_or_default();
        output.write_all(produced).map_err(Broken::Write)?;
        if result.status == Status::StreamEnd {
            return Ok(read);
        }
        if action == Action::Finish && result.bytes_produced == 0 && result.bytes_consumed == 0 {
            return Err(Broken::Truncated);
        }
    }
}

/// Every .xz stream of `input` in turn, as liblzma reads concatenated ones: stream
/// padding in words of four zero bytes, then a whole stream header before the next
/// stream is begun, so that a few stray bytes at the end are an early end of input and
/// a dozen are corruption.
fn xz_streams(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    single_stream: bool,
) -> Result<u64, Broken> {
    let mut read = drive(&mut XzStream::new(false), input, output)?;
    if single_stream {
        return Ok(read);
    }
    loop {
        let mut padding = 0_u64;
        loop {
            let available = input.fill_buf().map_err(Broken::Read)?;
            if available.is_empty() {
                return if padding.is_multiple_of(4) {
                    Ok(read + padding)
                } else {
                    Err(Broken::Corrupt)
                };
            }
            let zeros = available.iter().take_while(|byte| **byte == 0).count();
            let more = zeros == available.len();
            input.consume(zeros);
            padding += zeros as u64;
            if !more {
                break;
            }
        }
        if !padding.is_multiple_of(4) {
            return Err(Broken::Corrupt);
        }
        let mut header = Vec::with_capacity(HEADER_LEN);
        input
            .take(HEADER)
            .read_to_end(&mut header)
            .map_err(Broken::Read)?;
        if header.len() < HEADER_LEN {
            return Err(Broken::Truncated);
        }
        if !is_xz(&header) {
            return Err(Broken::Corrupt);
        }
        read += padding;
        let mut next = io::Cursor::new(header).chain(&mut *input);
        read += drive(&mut XzStream::new(false), &mut next, output)?;
    }
}

/// Decodes `input` as `format` into `output`: the bytes read.
///
/// A .xz input is every stream in turn, with the padding between them, unless
/// `single_stream`; lzip's members follow one another the same way, and what follows
/// the last is let be. After a .lzma or raw stream nothing may follow, as in xz, unless
/// `single_stream`. A raw stream is read with a dictionary of `raw_dict` bytes.
///
/// # Errors
///
/// What stopped the decoding, as [`Broken`] tells it.
pub fn decompress(
    format: Format,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    single_stream: bool,
    raw_dict: u32,
) -> Result<u64, Broken> {
    let (read, alone) = match format {
        Format::Xz => (xz_streams(input, output, single_stream)?, false),
        Format::Lzip => (drive(&mut LzipStream::new(), input, output)?, false),
        Format::Lzma => {
            // The decoder takes all it is given; what it took past the stream's end
            // it gives back, and that follows the stream as much as the rest does.
            let mut stream = LzmaStream::new_mem_limit(u32::MAX, None);
            let read = drive(&mut stream, input, output)?;
            if !single_stream && !stream.unused_input().is_empty() {
                return Err(Broken::Corrupt);
            }
            (read, true)
        }
        Format::Raw => (drive(&mut Lzma2Stream::new(raw_dict), input, output)?, true),
    };
    if alone && !single_stream && !input.fill_buf().map_err(Broken::Read)?.is_empty() {
        return Err(Broken::Corrupt);
    }
    Ok(read)
}

/// An integrity check of a .xz stream, by its id.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Check {
    /// No check.
    None,
    /// CRC-32.
    Crc32,
    /// CRC-64, xz's default.
    Crc64,
    /// SHA-256.
    Sha256,
}

impl Check {
    /// The check xz's `--check` names, or `None` for a name it does not know.
    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "none" => Some(Self::None),
            "crc32" => Some(Self::Crc32),
            "crc64" => Some(Self::Crc64),
            "sha256" => Some(Self::Sha256),
            _ => None,
        }
    }

    const fn check_type(self) -> CheckType {
        match self {
            Self::None => CheckType::None,
            Self::Crc32 => CheckType::Crc32,
            Self::Crc64 => CheckType::Crc64,
            Self::Sha256 => CheckType::Sha256,
        }
    }
}

/// How to compress: xz's preset, `-e`, `--check` and `--block-size`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Settings {
    /// 0 to 9.
    pub preset: u32,
    /// `-e`, liblzma's extreme variant of the preset.
    pub extreme: bool,
    /// The check of each block.
    pub check: Check,
    /// Start a new .xz block after this many bytes of input.
    pub block_size: Option<NonZeroU64>,
}

impl Settings {
    /// LZMA's options for the preset, the extreme variant as liblzma makes it.
    pub fn lzma_options(&self) -> LzmaOptions {
        let level = self.preset.min(9);
        let mut options = LzmaOptions::with_preset(level);
        if self.extreme {
            options.mode = EncodeMode::Normal;
            options.mf = MfType::Bt4;
            if level == 3 || level == 5 {
                options.nice_len = 192;
                options.depth_limit = 0;
            } else {
                options.nice_len = 273;
                options.depth_limit = 512;
            }
        }
        options
    }

    /// The dictionary size of the preset: what a raw stream is read with.
    pub fn dict_size(&self) -> u32 {
        self.lzma_options().dict_size
    }
}

impl<W: Write> Encoder for Lzma2Writer<W> {
    fn finish(self: Box<Self>) -> io::Result<()> {
        let mut inner = (*self).finish()?;
        inner.flush()
    }
}

/// A writer that compresses into `inner` in `format`; lzip is never asked for, since
/// xz does not write it.
///
/// # Errors
///
/// When the format's header cannot be written.
pub fn compressor<'a>(
    format: Format,
    settings: &Settings,
    inner: impl Write + 'a,
) -> io::Result<Box<dyn Encoder + 'a>> {
    let lzma_options = settings.lzma_options();
    Ok(match format {
        Format::Xz | Format::Lzip => {
            // lzma-rust2 makes a block at least as large as the dictionary, where
            // liblzma keeps to the size asked for. A block never refers back past its
            // own start, so a dictionary the size of the block loses nothing.
            let mut lzma_options = lzma_options;
            if let Some(block) = settings.block_size {
                let block = u32::try_from(block.get()).unwrap_or(u32::MAX);
                lzma_options.dict_size = lzma_options.dict_size.min(block.max(4096));
            }
            let mut options = XzOptions {
                lzma_options,
                ..XzOptions::default()
            };
            options.set_check_sum_type(settings.check.check_type());
            options.set_block_size(settings.block_size);
            Box::new(XzWriter::new(inner, options)?)
        }
        Format::Lzma => Box::new(LzmaWriter::new_use_header(inner, &lzma_options, None)?),
        Format::Raw => Box::new(Lzma2Writer::new(
            inner,
            Lzma2Options {
                lzma_options,
                ..Lzma2Options::default()
            },
        )),
    })
}

/// What `xz --list` found wrong with a file.
#[derive(Debug)]
pub enum InfoProblem {
    /// The file is empty.
    Empty,
    /// Shorter than a stream's header and footer.
    TooSmall,
    /// The end of the file is not a .xz stream's footer.
    NotRecognized,
    /// The index or the headers do not agree, or do not add up.
    Corrupt,
    /// Flags this reader does not know.
    Unsupported,
    /// Reading the file failed.
    Read(io::Error),
}

impl From<io::Error> for InfoProblem {
    fn from(error: io::Error) -> Self {
        Self::Read(error)
    }
}

/// One block of a .xz stream, as its index has it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockInfo {
    /// Where the block starts in the file.
    pub comp_offset: u64,
    /// Where its data starts in the uncompressed whole.
    pub uncomp_offset: u64,
    /// Its header, data and check, without its padding.
    pub unpadded_size: u64,
    /// The size of its data.
    pub uncomp_size: u64,
}

impl BlockInfo {
    /// What the block takes in the file, padding included.
    pub const fn total_size(&self) -> u64 {
        self.unpadded_size.div_ceil(4) * 4
    }
}

/// One stream of a .xz file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamInfo {
    /// Where the stream starts in the file.
    pub comp_offset: u64,
    /// Where its data starts in the uncompressed whole.
    pub uncomp_offset: u64,
    /// Header, blocks, index and footer.
    pub comp_size: u64,
    /// The size of its data.
    pub uncomp_size: u64,
    /// The check's id, 0 to 15.
    pub check: u8,
    /// The stream padding after it.
    pub padding: u64,
    /// Its blocks, in order.
    pub blocks: Vec<BlockInfo>,
}

/// What a .xz file holds, as its indexes say.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileInfo {
    /// The streams, in order.
    pub streams: Vec<StreamInfo>,
}

impl FileInfo {
    /// The file's size: every stream and its padding.
    pub fn file_size(&self) -> u64 {
        self.streams.iter().map(|s| s.comp_size + s.padding).sum()
    }

    /// The size of the data of every stream.
    pub fn uncomp_size(&self) -> u64 {
        self.streams.iter().map(|s| s.uncomp_size).sum()
    }

    /// The blocks of every stream.
    pub fn block_count(&self) -> u64 {
        self.streams.iter().map(|s| s.blocks.len() as u64).sum()
    }

    /// The stream padding of the whole file.
    pub fn padding(&self) -> u64 {
        self.streams.iter().map(|s| s.padding).sum()
    }

    /// The checks the streams use, as a bit set of their ids.
    pub fn checks(&self) -> u32 {
        self.streams.iter().fold(0, |set, s| set | (1 << s.check))
    }
}

/// The name xz gives a check id.
pub fn check_name(id: u8) -> String {
    match id {
        0 => "None".to_owned(),
        1 => "CRC32".to_owned(),
        4 => "CRC64".to_owned(),
        10 => "SHA-256".to_owned(),
        other => format!("Unknown-{other}"),
    }
}

fn read_at<R: Read + Seek>(file: &mut R, at: u64, buffer: &mut [u8]) -> io::Result<()> {
    file.seek(SeekFrom::Start(at))?;
    file.read_exact(buffer)
}

fn le32(bytes: &[u8], at: usize) -> u32 {
    bytes
        .get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u32::from_le_bytes)
}

/// A multibyte integer of .xz's index, from `bytes` at `*at`.
fn varint(bytes: &[u8], at: &mut usize) -> Result<u64, InfoProblem> {
    let mut value = 0_u64;
    for shift in (0..63).step_by(7) {
        let byte = *bytes.get(*at).ok_or(InfoProblem::Corrupt)?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            if byte == 0 && shift > 0 {
                return Err(InfoProblem::Corrupt);
            }
            return Ok(value);
        }
    }
    Err(InfoProblem::Corrupt)
}

/// A stream's flags, from its header or footer: the check id.
const fn stream_flags(flags: &[u8]) -> Result<u8, InfoProblem> {
    match flags {
        [0, check] if *check & 0xf0 == 0 => Ok(*check),
        _ => Err(InfoProblem::Unsupported),
    }
}

/// The records of an index of `size` bytes: each block's unpadded and uncompressed
/// sizes.
fn index_records(index: &[u8]) -> Result<Vec<(u64, u64)>, InfoProblem> {
    if index.first() != Some(&0) || index.len() < 8 {
        return Err(InfoProblem::Corrupt);
    }
    let body = index.len() - 4;
    if crc32fast::hash(index.get(..body).unwrap_or_default()) != le32(index, body) {
        return Err(InfoProblem::Corrupt);
    }
    let mut at = 1;
    let count = varint(index, &mut at)?;
    let mut records = Vec::new();
    for _ in 0..count {
        let unpadded = varint(index, &mut at)?;
        let uncompressed = varint(index, &mut at)?;
        if unpadded < 5 {
            return Err(InfoProblem::Corrupt);
        }
        records.push((unpadded, uncompressed));
    }
    if at > body || at.div_ceil(4) * 4 != body {
        return Err(InfoProblem::Corrupt);
    }
    if index
        .get(at..body)
        .is_some_and(|pad| pad.iter().any(|b| *b != 0))
    {
        return Err(InfoProblem::Corrupt);
    }
    Ok(records)
}

/// The stream whose footer, or the stream padding after it, ends at `end`.
fn stream_ending_at<R: Read + Seek>(file: &mut R, mut end: u64) -> Result<StreamInfo, InfoProblem> {
    // The footer, with the stream padding after it skipped a word at a time.
    let mut padding = 0;
    let mut footer = [0_u8; HEADER_LEN];
    loop {
        if end < HEADER {
            return Err(InfoProblem::Corrupt);
        }
        read_at(file, end - HEADER, &mut footer)?;
        if le32(&footer, 8) != 0 {
            break;
        }
        let mut word = 2;
        while le32(&footer, word * 4) == 0 {
            padding += 4;
            end -= 4;
            if word == 0 {
                break;
            }
            word -= 1;
        }
    }
    end -= HEADER;
    if footer.get(10..) != Some(b"YZ".as_slice())
        || crc32fast::hash(footer.get(4..10).unwrap_or_default()) != le32(&footer, 0)
    {
        return Err(InfoProblem::Corrupt);
    }
    let check = stream_flags(footer.get(8..10).unwrap_or_default())?;
    let index_size = (u64::from(le32(&footer, 4)) + 1) * 4;
    if end < index_size + HEADER {
        return Err(InfoProblem::Corrupt);
    }
    end -= index_size;
    let mut index = vec![0_u8; usize::try_from(index_size).map_err(|_| InfoProblem::Corrupt)?];
    read_at(file, end, &mut index)?;
    let records = index_records(&index)?;
    let blocks_size: u64 = records.iter().map(|(u, _)| u.div_ceil(4) * 4).sum();
    if end < blocks_size + HEADER {
        return Err(InfoProblem::Corrupt);
    }
    let start = end - blocks_size - HEADER;
    let mut header = [0_u8; HEADER_LEN];
    read_at(file, start, &mut header)?;
    let flags = header.get(6..8).unwrap_or_default();
    if !is_xz(&header) || crc32fast::hash(flags) != le32(&header, 8) {
        return Err(InfoProblem::Corrupt);
    }
    if stream_flags(flags)? != check {
        return Err(InfoProblem::Corrupt);
    }
    let mut blocks = Vec::new();
    let mut comp_offset = start + HEADER;
    let mut uncomp_offset = 0;
    for (unpadded_size, uncomp_size) in records {
        let block = BlockInfo {
            comp_offset,
            uncomp_offset,
            unpadded_size,
            uncomp_size,
        };
        comp_offset += block.total_size();
        uncomp_offset += uncomp_size;
        blocks.push(block);
    }
    Ok(StreamInfo {
        comp_offset: start,
        uncomp_offset: 0,
        comp_size: HEADER + blocks_size + index_size + HEADER,
        uncomp_size: uncomp_offset,
        check,
        padding,
        blocks,
    })
}

/// Reads a .xz file's streams and blocks, as `xz --list` does.
///
/// liblzma's file info decoder, which xz uses, reads the first stream's header, which
/// says whether the file is .xz at all; then from the end, each stream's footer,
/// padding skipped, its index and its header, where anything amiss is corruption.
///
/// # Errors
///
/// What [`InfoProblem`] says, in the order xz finds it.
pub fn file_info<R: Read + Seek>(file: &mut R) -> Result<FileInfo, InfoProblem> {
    let size = file.seek(SeekFrom::End(0))?;
    if size == 0 {
        return Err(InfoProblem::Empty);
    }
    if size < 2 * HEADER {
        return Err(InfoProblem::TooSmall);
    }
    let mut first = [0_u8; HEADER_LEN];
    read_at(file, 0, &mut first)?;
    if !is_xz(&first) {
        return Err(InfoProblem::NotRecognized);
    }
    if crc32fast::hash(first.get(6..8).unwrap_or_default()) != le32(&first, 8) {
        return Err(InfoProblem::Corrupt);
    }
    stream_flags(first.get(6..8).unwrap_or_default())?;
    let mut streams = Vec::new();
    let mut end = size;
    while end > 0 {
        let stream = stream_ending_at(file, end)?;
        end = stream.comp_offset;
        streams.push(stream);
    }
    streams.reverse();
    let mut uncomp_offset = 0;
    for stream in &mut streams {
        stream.uncomp_offset = uncomp_offset;
        for block in &mut stream.blocks {
            block.uncomp_offset += uncomp_offset;
        }
        uncomp_offset += stream.uncomp_size;
    }
    Ok(FileInfo { streams })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `printf 'hello\n' | xz` under XZ Utils 5.8.3.
    const HELLO_XZ: &[u8] = &[
        0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00, 0x00, 0x04, 0xe6, 0xd6, 0xb4, 0x46, 0x04, 0xc0, 0x0a,
        0x06, 0x21, 0x01, 0x16, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xaa, 0x30,
        0x8e, 0xa6, 0x01, 0x00, 0x05, 0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x0a, 0x00, 0x00, 0x00, 0xa5,
        0x60, 0x97, 0xf1, 0x94, 0xf6, 0xfd, 0xe0, 0x00, 0x01, 0x26, 0x06, 0x3a, 0x93, 0x3b, 0x0a,
        0x1f, 0xb6, 0xf3, 0x7d, 0x01, 0x00, 0x00, 0x00, 0x00, 0x04, 0x59, 0x5a,
    ];

    fn packed(format: Format, data: &[u8]) -> Vec<u8> {
        let settings = Settings {
            preset: 6,
            extreme: false,
            check: Check::Crc64,
            block_size: None,
        };
        let mut out = Vec::new();
        let mut encoder = compressor(format, &settings, &mut out).unwrap();
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap();
        out
    }

    fn unpacked(format: Format, data: &[u8], single: bool) -> Result<Vec<u8>, Broken> {
        let mut out = Vec::new();
        decompress(
            format,
            &mut io::BufReader::new(data),
            &mut out,
            single,
            8 << 20,
        )?;
        Ok(out)
    }

    #[test]
    fn formats_are_told_as_xz_tells_them() {
        assert_eq!(detect(&packed(Format::Xz, b"x")), Some(Format::Xz));
        assert_eq!(detect(&packed(Format::Lzma, b"x")), Some(Format::Lzma));
        assert_eq!(detect(b"LZIP\x01"), Some(Format::Lzip));
        assert_eq!(detect(b"hello\n"), None);
        assert_eq!(detect(b"hello world, more than 13"), None);
    }

    #[test]
    fn every_format_round_trips() {
        for format in [Format::Xz, Format::Lzma, Format::Raw] {
            let data = b"hello hello hello\n".repeat(500);
            assert_eq!(
                unpacked(format, &packed(format, &data), false).unwrap(),
                data
            );
        }
        assert_eq!(unpacked(Format::Xz, HELLO_XZ, false).unwrap(), b"hello\n");
    }

    #[test]
    fn what_follows_a_stream_is_judged_as_xz_judges_it() {
        let mut padded = HELLO_XZ.to_vec();
        padded.extend_from_slice(&[0; 4]);
        padded.extend_from_slice(HELLO_XZ);
        assert_eq!(
            unpacked(Format::Xz, &padded, false).unwrap(),
            b"hello\nhello\n"
        );
        assert_eq!(unpacked(Format::Xz, &padded, true).unwrap(), b"hello\n");
        let mut junk = HELLO_XZ.to_vec();
        junk.extend_from_slice(b"junk");
        let got = unpacked(Format::Xz, &junk, false);
        assert!(matches!(got, Err(Broken::Truncated)), "{got:?}");
        let cut = HELLO_XZ.get(..30).unwrap();
        assert!(matches!(
            unpacked(Format::Xz, cut, false),
            Err(Broken::Truncated)
        ));
        let mut bad = HELLO_XZ.to_vec();
        if let Some(byte) = bad.get_mut(30) {
            *byte = 0xff;
        }
        assert!(matches!(
            unpacked(Format::Xz, &bad, false),
            Err(Broken::Corrupt)
        ));
        let mut after = packed(Format::Lzma, b"hi");
        after.push(b'x');
        let got = unpacked(Format::Lzma, &after, false);
        assert!(matches!(got, Err(Broken::Corrupt)), "{got:?}");
    }

    #[test]
    fn the_index_says_what_xz_list_says() {
        let mut file = HELLO_XZ.to_vec();
        file.extend_from_slice(&[0; 4]);
        file.extend_from_slice(HELLO_XZ);
        file.extend_from_slice(&[0; 8]);
        let info = file_info(&mut io::Cursor::new(&file)).unwrap();
        assert_eq!(info.file_size(), 156);
        assert_eq!(
            (info.uncomp_size(), info.block_count(), info.padding()),
            (12, 2, 12)
        );
        let second = info.streams.get(1).unwrap();
        assert_eq!(
            (second.comp_offset, second.uncomp_offset, second.padding),
            (76, 6, 8)
        );
        let block = second.blocks.first().unwrap();
        assert_eq!(
            (block.comp_offset, block.total_size(), block.uncomp_offset),
            (88, 40, 6)
        );
        assert!(matches!(
            file_info(&mut io::Cursor::new(b"hello\n")),
            Err(InfoProblem::TooSmall)
        ));
        assert!(matches!(
            file_info(&mut io::Cursor::new(vec![b'a'; 40])),
            Err(InfoProblem::NotRecognized)
        ));
        assert!(matches!(
            file_info(&mut io::Cursor::new(HELLO_XZ.get(..30).unwrap())),
            Err(InfoProblem::Corrupt)
        ));
    }
}
