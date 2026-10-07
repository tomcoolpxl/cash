//! bzip2 streams read one at a time, as bzip2 1.0.8 reads a file.
//!
//! A stream is read, then another while input is left. Where a stream should start but
//! none does is told apart from damage inside one, since bzip2 calls the first "not a
//! bzip2 file" (or trailing garbage, after a stream) and the second a data error.

use std::io::{self, BufRead, Read, Write};

/// What libbzip2 hands out at a time, as bzip2 1.0.8 asks for it.
const CHUNK: usize = 5000;

/// How a run of streams ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ending {
    /// A whole stream, then the end of the input.
    Whole,
    /// Where a stream should begin, bytes that are not one.
    NoStream {
        /// The number of the stream looked for, from 1: with 1 the input is not bzip2's
        /// at all, later it is trailing garbage.
        stream: u32,
    },
}

/// What stopped the reading.
#[derive(Debug)]
pub enum Broken {
    /// The data inside a stream is damaged, a block's CRC among the checks that failed:
    /// libbzip2's `BZ_DATA_ERROR`.
    Data,
    /// The input ends inside a stream, or before a stream's header is whole.
    Truncated,
    /// Reading the input failed.
    Read(io::Error),
    /// Writing the output failed.
    Write(io::Error),
}

/// Decompresses every stream of `input` into `output`.
///
/// What a stream holds is written as it is decoded, so a damaged stream leaves its
/// first blocks written.
///
/// # Errors
///
/// What stopped the reading, as [`Broken`] tells it.
pub fn decompress_streams<R: BufRead>(
    input: &mut R,
    output: &mut dyn Write,
) -> Result<Ending, Broken> {
    let mut buffer = vec![0_u8; CHUNK];
    let mut stream = 0_u32;
    loop {
        stream += 1;
        let mut decoder = ::bzip2::bufread::BzDecoder::new(&mut *input);
        loop {
            match decoder.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let decoded = buffer.get(..n).unwrap_or_default();
                    output.write_all(decoded).map_err(Broken::Write)?;
                }
                Err(error) => return stopped(error, stream),
            }
        }
        drop(decoder);
        match input.fill_buf() {
            Ok([]) => return Ok(Ending::Whole),
            Ok(_) => {}
            Err(error) => return Err(Broken::Read(error)),
        }
    }
}

/// Writes bzip2 streams of `level` hundred thousand bytes each, `threads` compressed
/// at once and written in order (pbzip2's layout); input of one stream's size is one
/// stream, as bzip2 writes it.
pub struct ParallelWriter<W: Write> {
    inner: W,
    pending: Vec<u8>,
    level: u32,
    threads: usize,
    /// Whether a stream was written: an empty input is still one empty stream.
    wrote: bool,
}

impl<W: Write> ParallelWriter<W> {
    /// A writer into `inner` of blocks of `level` (1 to 9) hundred thousand bytes.
    pub fn new(inner: W, level: u32, threads: usize) -> Self {
        Self {
            inner,
            pending: Vec::new(),
            level: level.clamp(1, 9),
            threads: threads.max(1),
            wrote: false,
        }
    }

    /// The input each stream holds.
    const fn chunk(&self) -> usize {
        self.level as usize * 100_000
    }

    /// What is pending made streams of [`Self::chunk`] bytes, at once, and written.
    fn streams(&mut self) -> io::Result<()> {
        let level = self.level;
        let chunks: Vec<&[u8]> = if self.pending.is_empty() {
            vec![&[]]
        } else {
            self.pending.chunks(self.chunk()).collect()
        };
        let streams = super::parallel::map_ordered(chunks, |chunk| {
            let mut out = Vec::with_capacity(chunk.len() / 3 + 64);
            let mut encoder =
                ::bzip2::write::BzEncoder::new(&mut out, ::bzip2::Compression::new(level));
            encoder.write_all(chunk)?;
            encoder.finish()?;
            Ok(out)
        })?;
        self.pending.clear();
        self.wrote = true;
        for stream in streams {
            self.inner.write_all(&stream)?;
        }
        Ok(())
    }

    /// Writes the last streams, and hands the writer back.
    ///
    /// # Errors
    ///
    /// When a stream cannot be written.
    pub fn finish(mut self) -> io::Result<W> {
        if !self.pending.is_empty() || !self.wrote {
            self.streams()?;
        }
        self.inner.flush()?;
        Ok(self.inner)
    }
}

impl<W: Write> Write for ParallelWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let batch = self.chunk() * self.threads;
        let taken = buf.len().min(batch - self.pending.len());
        self.pending
            .extend_from_slice(buf.get(..taken).unwrap_or_default());
        if self.pending.len() >= batch {
            self.streams()?;
        }
        Ok(taken)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// What an error from the decoder of stream number `stream` means.
fn stopped(error: io::Error, stream: u32) -> Result<Ending, Broken> {
    let inner = error
        .get_ref()
        .and_then(|e| e.downcast_ref::<::bzip2::Error>());
    match inner {
        Some(::bzip2::Error::DataMagic) => Ok(Ending::NoStream { stream }),
        Some(_) => Err(Broken::Data),
        None if error.kind() == io::ErrorKind::UnexpectedEof => Err(Broken::Truncated),
        None => Err(Broken::Read(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packed(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = ::bzip2::write::BzEncoder::new(&mut out, ::bzip2::Compression::best());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap();
        out
    }

    fn read(input: &[u8]) -> (Result<Ending, Broken>, Vec<u8>) {
        let mut out = Vec::new();
        let ending = decompress_streams(&mut io::BufReader::new(input), &mut out);
        (ending, out)
    }

    #[test]
    fn streams_are_read_in_turn_and_garbage_after_them_is_found() {
        let one = packed(b"hello\n");
        let mut two = one.clone();
        two.extend_from_slice(&packed(b"world\n"));
        assert!(matches!(read(&two), (Ok(Ending::Whole), out) if out == b"hello\nworld\n"));
        let mut tail = one;
        tail.extend_from_slice(b"junk");
        assert!(matches!(
            read(&tail),
            (Ok(Ending::NoStream { stream: 2 }), out) if out == b"hello\n"
        ));
        assert!(matches!(
            read(b"plain text"),
            (Ok(Ending::NoStream { stream: 1 }), out) if out.is_empty()
        ));
    }

    #[test]
    fn a_short_or_damaged_stream_is_broken() {
        let one = packed(b"hello\n");
        assert!(matches!(read(b""), (Err(Broken::Truncated), _)));
        assert!(matches!(read(b"BZh"), (Err(Broken::Truncated), _)));
        assert!(matches!(
            read(one.get(..20).unwrap()),
            (Err(Broken::Truncated), _)
        ));
        let mut bad = one;
        if let Some(byte) = bad.get_mut(20) {
            *byte = 0xff;
        }
        assert!(matches!(read(&bad), (Err(Broken::Data), _)));
    }
}
