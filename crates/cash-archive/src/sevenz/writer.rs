mod counting_writer;

use std::{
    cell::Cell,
    io::{self, Read, Seek, SeekFrom, Write},
    rc::Rc,
};

pub(crate) use counting_writer::CountingWriter;
use crc32fast::Hasher;

use crate::sevenz::{
    ArchiveEntry, Block, Error,
    archive::{
        EncoderConfiguration, EncoderMethod, K_A_TIME, K_ANTI, K_C_TIME, K_CODERS_UNPACK_SIZE,
        K_CRC, K_DUMMY, K_EMPTY_FILE, K_EMPTY_STREAM, K_ENCODED_HEADER, K_END, K_FILES_INFO,
        K_FOLDER, K_HEADER, K_M_TIME, K_MAIN_STREAMS_INFO, K_NAME, K_NUM_UNPACK_STREAM,
        K_PACK_INFO, K_SIZE, K_SUB_STREAMS_INFO, K_UNPACK_INFO, K_WIN_ATTRIBUTES,
        SEVEN_Z_SIGNATURE, SIGNATURE_HEADER_SIZE,
    },
    encoder::{self, Chain, Sink},
    options::{EncoderOptions, LzmaParams},
};

type Result<T> = std::result::Result<T, Error>;

/// The coders of a block being written.
#[derive(Debug, Clone)]
enum Coders {
    /// Encoded here, with these methods, the packed side first.
    Methods(Vec<EncoderConfiguration>),
    /// Copied from another archive, as its block was.
    Copied(Block),
}

/// A block as the header describes it.
#[derive(Debug, Clone)]
struct BlockInfo {
    coders: Coders,
    /// Each coder's output size, by output stream.
    sizes: Vec<u64>,
    /// The block's own CRC, which 7-Zip keeps only for a header and for blocks copied
    /// from an archive that had one.
    crc: Option<u32>,
    /// How many entries' data it holds.
    sub_streams: usize,
}

/// Which times the header keeps, as 7-Zip's `-mtm`, `-mtc` and `-mta` choose: the
/// modification time unless told otherwise.
#[derive(Debug, Clone, Copy)]
pub struct TimesKept {
    /// The modification time (`-mtm`, on by default).
    pub modified: bool,
    /// The creation time (`-mtc`).
    pub created: bool,
    /// The access time (`-mta`).
    pub accessed: bool,
}

impl Default for TimesKept {
    fn default() -> Self {
        Self {
            modified: true,
            created: false,
            accessed: false,
        }
    }
}

/// Writes a 7z archive, as 7-Zip lays one out: the entries' packed data, then the header.
pub struct ArchiveWriter<W: Write + Seek> {
    output: W,
    /// Where the archive starts in `output`.
    start: u64,
    files: Vec<ArchiveEntry>,
    content_methods: Vec<EncoderConfiguration>,
    pack_sizes: Vec<u64>,
    blocks: Vec<BlockInfo>,
    compress_header: bool,
    /// AES for the header (`-mhe`), when it is encrypted.
    header_encryption: Option<EncoderConfiguration>,
    times: TimesKept,
}

impl<W: Write + Seek> ArchiveWriter<W> {
    /// Starts an archive at `writer`'s position; its start header is written last.
    pub fn new(mut writer: W) -> Result<Self> {
        let start = writer.stream_position()?;
        writer.seek(SeekFrom::Start(start + SIGNATURE_HEADER_SIZE))?;
        Ok(Self {
            output: writer,
            start,
            files: Vec::new(),
            content_methods: vec![EncoderConfiguration::new(EncoderMethod::LZMA2)],
            pack_sizes: Vec::new(),
            blocks: Vec::new(),
            compress_header: true,
            header_encryption: None,
            times: TimesKept::default(),
        })
    }

    /// The methods the next blocks are written with, the packed side first (AES, then
    /// LZMA2, then BCJ, as 7-Zip lists a block's coders). LZMA2 unless set.
    pub fn set_content_methods(&mut self, content_methods: Vec<EncoderConfiguration>) -> &mut Self {
        if !content_methods.is_empty() {
            self.content_methods = content_methods;
        }
        self
    }

    /// Whether the header is packed with LZMA (`-mhc`, on unless set).
    pub const fn set_compress_header(&mut self, enabled: bool) {
        self.compress_header = enabled;
    }

    /// Encrypts the header with these AES settings (`-mhe`), or not.
    pub fn set_header_encryption(&mut self, aes: Option<EncoderConfiguration>) {
        self.header_encryption = aes;
    }

    /// Which times the header keeps.
    pub const fn set_times(&mut self, times: TimesKept) {
        self.times = times;
    }

    /// The entries so far, with the sizes and CRCs their blocks gave them.
    pub fn entries(&self) -> &[ArchiveEntry] {
        &self.files
    }

    /// Adds an entry without data: a folder, an empty file, an anti-item.
    pub fn push_empty(&mut self, mut entry: ArchiveEntry) {
        entry.has_stream = false;
        entry.size = 0;
        entry.has_crc = false;
        entry.compressed_size = 0;
        self.files.push(entry);
    }

    /// Adds one block holding the data of `entries`, each read from its reader in turn;
    /// the entries take the sizes and CRCs of what was read.
    pub fn push_block<R: Read>(
        &mut self,
        mut entries: Vec<ArchiveEntry>,
        readers: Vec<R>,
    ) -> Result<()> {
        if entries.len() != readers.len() || entries.is_empty() {
            return Err(Error::other("a block needs one reader for each entry"));
        }
        let methods = self.content_methods.clone();
        let packed = Rc::new(Cell::new(0));
        let mut sizes = Vec::new();
        {
            let sink: Chain<'_> = Box::new(Sink(Counted {
                inner: &mut self.output,
                count: Rc::clone(&packed),
            }));
            let mut chain = create_writer(&methods, sink, &mut sizes)?;
            let mut buf = vec![0u8; 64 * 1024];
            for (entry, mut reader) in entries.iter_mut().zip(readers) {
                let mut hasher = Hasher::new();
                let mut size = 0u64;
                loop {
                    let n = match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(e) => return Err(Error::io_msg(e, entry.name.clone())),
                    };
                    hasher.update(&buf[..n]);
                    size += n as u64;
                    chain.write_all(&buf[..n])?;
                }
                entry.has_stream = true;
                entry.size = size;
                entry.crc = u64::from(hasher.finalize());
                entry.has_crc = true;
            }
            chain.finish()?;
        }
        let total = entries.iter().map(|e| e.size).sum();
        let sizes = unpack_sizes(&sizes, total);
        self.pack_sizes.push(packed.get());
        if let Some(first) = entries.first_mut() {
            first.compressed_size = packed.get();
        }
        self.blocks.push(BlockInfo {
            coders: Coders::Methods(methods),
            sizes,
            crc: None,
            sub_streams: entries.len(),
        });
        self.files.extend(entries);
        Ok(())
    }

    /// Adds a block copied as it is from another archive: `copy` writes its packed
    /// streams here and returns their sizes; `entries` are the block's, with their sizes
    /// and CRCs, in its order.
    pub fn push_copied_block(
        &mut self,
        entries: Vec<ArchiveEntry>,
        block: &Block,
        copy: impl FnOnce(&mut dyn Write) -> io::Result<Vec<u64>>,
    ) -> Result<()> {
        if entries.len() != block.sub_stream_count() {
            return Err(Error::other("a copied block keeps all its entries"));
        }
        let sizes = copy(&mut self.output)?;
        self.pack_sizes.extend(&sizes);
        self.blocks.push(BlockInfo {
            coders: Coders::Copied(block.clone()),
            sizes: block.unpack_sizes().to_vec(),
            crc: block.has_crc.then(|| u32::try_from(block.crc).unwrap_or(0)),
            sub_streams: entries.len(),
        });
        self.files.extend(entries);
        Ok(())
    }

    /// Writes the header and the start header, and gives the output back, positioned
    /// at the archive's end.
    pub fn finish(mut self) -> io::Result<W> {
        let data_end = self.output.stream_position()?;
        let mut header = Vec::with_capacity(4096);
        if !self.files.is_empty() || !self.blocks.is_empty() {
            self.write_header(&mut header);
            if self.compress_header || self.header_encryption.is_some() {
                header = self.encode_header(&header, data_end)?;
            }
        }
        let header_pos = self.output.stream_position()?;
        self.output.write_all(&header)?;
        let end = self.output.stream_position()?;

        let mut start_header = [0u8; SIGNATURE_HEADER_SIZE as usize];
        start_header[..6].copy_from_slice(SEVEN_Z_SIGNATURE);
        start_header[6] = 0;
        start_header[7] = 4;
        let next_header_offset = if header.is_empty() {
            0
        } else {
            header_pos - self.start - SIGNATURE_HEADER_SIZE
        };
        start_header[12..20].copy_from_slice(&next_header_offset.to_le_bytes());
        start_header[20..28].copy_from_slice(&(header.len() as u64).to_le_bytes());
        start_header[28..32].copy_from_slice(&crc32fast::hash(&header).to_le_bytes());
        let crc = crc32fast::hash(&start_header[12..]);
        start_header[8..12].copy_from_slice(&crc.to_le_bytes());

        self.output.seek(SeekFrom::Start(self.start))?;
        self.output.write_all(&start_header)?;
        self.output.seek(SeekFrom::Start(end))?;
        self.output.flush()?;
        Ok(self.output)
    }

    /// Packs the header as 7-Zip does: LZMA with a 1 MiB dictionary unless `-mhc=off`,
    /// then AES when `-mhe`; returns the header that describes it.
    fn encode_header(&mut self, raw: &[u8], data_end: u64) -> io::Result<Vec<u8>> {
        let mut methods = Vec::new();
        if let Some(aes) = &self.header_encryption {
            methods.push(aes.clone());
        }
        if self.compress_header {
            let mut params = LzmaParams::with_preset(5);
            params.dict_size = 1 << 20;
            params.nice_len = 273;
            methods.push(
                EncoderConfiguration::new(EncoderMethod::LZMA)
                    .with_options(EncoderOptions::Lzma(params)),
            );
        }
        let packed = Rc::new(Cell::new(0));
        let mut sizes = Vec::new();
        {
            let sink: Chain<'_> = Box::new(Sink(Counted {
                inner: &mut self.output,
                count: Rc::clone(&packed),
            }));
            let mut chain = create_writer(&methods, sink, &mut sizes).map_err(io::Error::other)?;
            chain.write_all(raw)?;
            chain.finish()?;
        }
        let block = BlockInfo {
            coders: Coders::Methods(methods),
            sizes: unpack_sizes(&sizes, raw.len() as u64),
            crc: Some(crc32fast::hash(raw)),
            sub_streams: 1,
        };
        let mut header = Vec::with_capacity(64);
        header.push(K_ENCODED_HEADER);
        write_pack_info(
            &mut header,
            data_end - self.start - SIGNATURE_HEADER_SIZE,
            &[packed.get()],
        );
        write_unpack_info(&mut header, std::slice::from_ref(&block));
        header.push(K_END);
        Ok(header)
    }

    /// The header, as 7-Zip's `WriteHeader` lays it out, alignment included.
    fn write_header(&self, header: &mut Vec<u8>) {
        header.push(K_HEADER);
        if !self.blocks.is_empty() {
            header.push(K_MAIN_STREAMS_INFO);
            write_pack_info(header, 0, &self.pack_sizes);
            write_unpack_info(header, &self.blocks);
            self.write_sub_streams_info(header);
            header.push(K_END);
        }
        if self.files.is_empty() {
            header.push(K_END);
            return;
        }
        header.push(K_FILES_INFO);
        write_number(header, self.files.len() as u64);
        self.write_empty_streams(header);
        self.write_names(header);
        let files = &self.files;
        if self.times.created {
            write_times(header, K_C_TIME, files, |f| {
                f.has_creation_date.then_some(f.creation_date.0)
            });
        }
        if self.times.accessed {
            write_times(header, K_A_TIME, files, |f| {
                f.has_access_date.then_some(f.access_date.0)
            });
        }
        if self.times.modified {
            write_times(header, K_M_TIME, files, |f| {
                f.has_last_modified_date.then_some(f.last_modified_date.0)
            });
        }
        let defined: Vec<bool> = files.iter().map(|f| f.has_windows_attributes).collect();
        let count = defined.iter().filter(|d| **d).count();
        if count > 0 {
            write_aligned_bools(header, &defined, count, K_WIN_ATTRIBUTES, 2);
            for file in files.iter().filter(|f| f.has_windows_attributes) {
                header.extend_from_slice(&file.windows_attributes.to_le_bytes());
            }
        }
        header.push(K_END);
        header.push(K_END);
    }

    fn write_sub_streams_info(&self, header: &mut Vec<u8>) {
        header.push(K_SUB_STREAMS_INFO);
        if self.blocks.iter().any(|b| b.sub_streams != 1) {
            header.push(K_NUM_UNPACK_STREAM);
            for block in &self.blocks {
                write_number(header, block.sub_streams as u64);
            }
        }
        let streamed: Vec<&ArchiveEntry> = self.files.iter().filter(|f| f.has_stream).collect();
        if self.blocks.iter().any(|b| b.sub_streams > 1) {
            header.push(K_SIZE);
            let mut files = streamed.iter();
            for block in &self.blocks {
                for i in 0..block.sub_streams {
                    let size = files.next().map_or(0, |f| f.size);
                    if i + 1 != block.sub_streams {
                        write_number(header, size);
                    }
                }
            }
        }
        // The entries' CRCs, but for a block of one entry whose own CRC says it already.
        let mut digests: Vec<Option<u32>> = Vec::new();
        let mut files = streamed.iter();
        for block in &self.blocks {
            for _ in 0..block.sub_streams {
                let crc = files
                    .next()
                    .and_then(|f| f.has_crc.then(|| u32::try_from(f.crc).unwrap_or(0)));
                if !(block.sub_streams == 1 && block.crc.is_some()) {
                    digests.push(crc);
                }
            }
        }
        write_digests(header, &digests);
        header.push(K_END);
    }

    fn write_empty_streams(&self, header: &mut Vec<u8>) {
        let empty: Vec<bool> = self.files.iter().map(|f| !f.has_stream).collect();
        if !empty.contains(&true) {
            return;
        }
        write_bool_property(header, K_EMPTY_STREAM, &empty);
        let without: Vec<&ArchiveEntry> = self.files.iter().filter(|f| !f.has_stream).collect();
        let files: Vec<bool> = without.iter().map(|f| !f.is_directory).collect();
        if files.contains(&true) {
            write_bool_property(header, K_EMPTY_FILE, &files);
        }
        let anti: Vec<bool> = without.iter().map(|f| f.is_anti_item).collect();
        if anti.contains(&true) {
            write_bool_property(header, K_ANTI, &anti);
        }
    }

    fn write_names(&self, header: &mut Vec<u8>) {
        let size: u64 = (self
            .files
            .iter()
            .map(|f| (f.name.encode_utf16().count() + 1) * 2)
            .sum::<usize>()
            + 1) as u64;
        if self.files.iter().all(|f| f.name.is_empty()) {
            return;
        }
        skip_to_aligned(header, 2 + number_size(size), 4);
        header.push(K_NAME);
        write_number(header, size);
        header.push(0);
        for file in &self.files {
            for unit in file.name.encode_utf16() {
                header.extend_from_slice(&unit.to_le_bytes());
            }
            header.extend_from_slice(&[0, 0]);
        }
    }
}

/// Counts what goes through to the archive's output.
struct Counted<'a, W> {
    inner: &'a mut W,
    count: Rc<Cell<u64>>,
}

impl<W: Write> Write for Counted<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.count.set(self.count.get() + n as u64);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Each coder's unpacked size, as the header wants it: what went into it. A coder's
/// input is what the next coder in wrote out (`outputs`, counted from the packed side),
/// and the last one's is the data itself.
fn unpack_sizes(outputs: &[Rc<Cell<u64>>], total: u64) -> Vec<u64> {
    outputs
        .iter()
        .skip(1)
        .map(|count| count.get())
        .chain(std::iter::once(total))
        .collect()
}

/// Builds the coder chain for `methods` (packed side first) in front of `out`; `sizes`
/// gets each coder's output size, the packed side first.
fn create_writer<'a>(
    methods: &[EncoderConfiguration],
    out: Chain<'a>,
    sizes: &mut Vec<Rc<Cell<u64>>>,
) -> Result<Chain<'a>> {
    let mut chain = out;
    for config in methods {
        let counting = CountingWriter::new(chain);
        sizes.push(counting.counting());
        chain = Box::new(encoder::add_encoder(counting, config)?);
    }
    Ok(chain)
}

/// 7-Zip's `WriteNumber`: the first byte's high bits say how many bytes follow.
pub(crate) fn write_number(header: &mut Vec<u8>, mut value: u64) {
    let mut first = 0u64;
    let mut mask = 0x80u64;
    let mut i = 0;
    while i < 8 {
        if value < (1u64 << (7 * (i + 1))) {
            first |= value >> (8 * i);
            break;
        }
        first |= mask;
        mask >>= 1;
        i += 1;
    }
    header.push((first & 0xFF) as u8);
    while i > 0 {
        header.push((value & 0xFF) as u8);
        value >>= 8;
        i -= 1;
    }
}

/// How many bytes [`write_number`] takes for `value`.
fn number_size(value: u64) -> usize {
    (1..9).find(|i| value < 1u64 << (i * 7)).unwrap_or(9)
}

fn write_bools(header: &mut Vec<u8>, bits: &[bool]) {
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for (i, bit) in chunk.iter().enumerate() {
            if *bit {
                byte |= 0x80 >> i;
            }
        }
        header.push(byte);
    }
}

/// 7-Zip's `WritePropBoolVector`.
fn write_bool_property(header: &mut Vec<u8>, id: u8, bits: &[bool]) {
    header.push(id);
    write_number(header, bits.len().div_ceil(8) as u64);
    write_bools(header, bits);
}

/// 7-Zip's `SkipToAligned`: a dummy property so that what follows `extra` more bytes
/// starts on a 2^`shifts` boundary of the header.
fn skip_to_aligned(header: &mut Vec<u8>, extra: usize, shifts: u32) {
    let align = 1usize << shifts;
    let pos = (extra + header.len()) & (align - 1);
    if pos == 0 {
        return;
    }
    let mut skip = align - pos;
    if skip < 2 {
        skip += align;
    }
    skip -= 2;
    header.push(K_DUMMY);
    header.push(u8::try_from(skip).unwrap_or(0));
    header.resize(header.len() + skip, 0);
}

/// 7-Zip's `WriteAlignedBools`: a property of `count` items of 2^`shifts` bytes, its
/// items aligned.
fn write_aligned_bools(header: &mut Vec<u8>, defined: &[bool], count: usize, id: u8, shifts: u32) {
    let all = count == defined.len();
    let bits = if all { 0 } else { defined.len().div_ceil(8) };
    let size = ((count as u64) << shifts) + bits as u64 + 2;
    skip_to_aligned(header, 3 + bits + number_size(size), shifts);
    header.push(id);
    write_number(header, size);
    if all {
        header.push(1);
    } else {
        header.push(0);
        write_bools(header, defined);
    }
    header.push(0);
}

fn write_times(
    header: &mut Vec<u8>,
    id: u8,
    files: &[ArchiveEntry],
    time: impl Fn(&ArchiveEntry) -> Option<u64>,
) {
    let times: Vec<Option<u64>> = files.iter().map(time).collect();
    let count = times.iter().flatten().count();
    if count == 0 {
        return;
    }
    let defined: Vec<bool> = times.iter().map(Option::is_some).collect();
    write_aligned_bools(header, &defined, count, id, 3);
    for time in times.into_iter().flatten() {
        header.extend_from_slice(&time.to_le_bytes());
    }
}

/// 7-Zip's `WriteHashDigests`: nothing when no CRC is known.
fn write_digests(header: &mut Vec<u8>, digests: &[Option<u32>]) {
    let count = digests.iter().flatten().count();
    if count == 0 {
        return;
    }
    header.push(K_CRC);
    if count == digests.len() {
        header.push(1);
    } else {
        header.push(0);
        let defined: Vec<bool> = digests.iter().map(Option::is_some).collect();
        write_bools(header, &defined);
    }
    for crc in digests.iter().flatten() {
        header.extend_from_slice(&crc.to_le_bytes());
    }
}

/// 7-Zip's `WritePackInfo`, without the packed streams' CRCs it never writes.
fn write_pack_info(header: &mut Vec<u8>, offset: u64, sizes: &[u64]) {
    if sizes.is_empty() {
        return;
    }
    header.push(K_PACK_INFO);
    write_number(header, offset);
    write_number(header, sizes.len() as u64);
    header.push(K_SIZE);
    for size in sizes {
        write_number(header, *size);
    }
    header.push(K_END);
}

/// 7-Zip's `WriteUnpackInfo`.
fn write_unpack_info(header: &mut Vec<u8>, blocks: &[BlockInfo]) {
    if blocks.is_empty() {
        return;
    }
    header.push(K_UNPACK_INFO);
    header.push(K_FOLDER);
    write_number(header, blocks.len() as u64);
    header.push(0);
    for block in blocks {
        write_folder(header, &block.coders);
    }
    header.push(K_CODERS_UNPACK_SIZE);
    for block in blocks {
        for size in &block.sizes {
            write_number(header, *size);
        }
    }
    let crcs: Vec<Option<u32>> = blocks.iter().map(|b| b.crc).collect();
    write_digests(header, &crcs);
    header.push(K_END);
}

/// 7-Zip's `WriteFolder`: the coders, then the bonds between them, then which inputs are
/// packed streams when there is more than one.
fn write_folder(header: &mut Vec<u8>, coders: &Coders) {
    match coders {
        Coders::Methods(methods) => {
            write_number(header, methods.len() as u64);
            for config in methods {
                let id = config.method.id();
                let props = encoder::properties(config);
                write_coder(header, id, None, &props);
            }
            for i in 1..methods.len() {
                write_number(header, i as u64);
                write_number(header, i as u64 - 1);
            }
        }
        Coders::Copied(block) => {
            write_number(header, block.coders.len() as u64);
            for coder in &block.coders {
                let (inputs, outputs) = coder.stream_counts();
                let complex = (inputs != 1 || outputs != 1).then_some((inputs, outputs));
                write_coder(
                    header,
                    coder.encoder_method_id(),
                    complex,
                    coder.properties(),
                );
            }
            for (input, output) in block.bind_pairs() {
                write_number(header, input);
                write_number(header, output);
            }
            if block.packed_streams().len() > 1 {
                for stream in block.packed_streams() {
                    write_number(header, *stream);
                }
            }
        }
    }
}

fn write_coder(header: &mut Vec<u8>, id: &[u8], complex: Option<(u64, u64)>, props: &[u8]) {
    let mut flags = u8::try_from(id.len()).unwrap_or(0x0F) & 0x0F;
    if complex.is_some() {
        flags |= 0x10;
    }
    if !props.is_empty() {
        flags |= 0x20;
    }
    header.push(flags);
    header.extend_from_slice(id);
    if let Some((inputs, outputs)) = complex {
        write_number(header, inputs);
        write_number(header, outputs);
    }
    if !props.is_empty() {
        write_number(header, props.len() as u64);
        header.extend_from_slice(props);
    }
}

/// Writes `value` as 7-Zip's numbers are written, for the reader's tests.
#[cfg(test)]
pub(crate) fn number_bytes(value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    write_number(&mut out, value);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_take_7_zips_lengths() {
        assert_eq!(number_bytes(0x7F), [0x7F]);
        assert_eq!(number_bytes(0x7c60), [0xC0, 0x60, 0x7C]);
        assert_eq!(number_bytes(0x1_8000), [0xC1, 0x00, 0x80]);
        for value in [0, 0x7F, 0x80, 0x3FFF, 0x4000, u64::from(u32::MAX), u64::MAX] {
            assert_eq!(number_size(value), number_bytes(value).len());
        }
    }

    #[test]
    fn padding_lines_up_as_in_7_zips_headers() {
        // Before the names of a header whose files info starts at byte 53 (7-Zip's own).
        let mut header = vec![0u8; 53];
        skip_to_aligned(&mut header, 3, 4);
        assert_eq!(&header[53..], [K_DUMMY, 6, 0, 0, 0, 0, 0, 0]);
        let mut header = vec![0u8; 74];
        skip_to_aligned(&mut header, 3, 4);
        assert_eq!(&header[74..], [K_DUMMY, 1, 0]);
    }
}
