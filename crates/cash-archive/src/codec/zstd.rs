//! zstd streams on `ruzstd`.
//!
//! [`Reader`] reads every frame in turn, passes over skippable frames, and checks each
//! frame's content checksum, which `ruzstd`'s own streaming decoder reads but does not
//! compare. [`Writer`] compresses at `ruzstd`'s fast level, one frame for each
//! [`FRAME`] bytes written: `ruzstd`'s compressor pulls its input and stops the program
//! on a failed read or write, so each frame is made between two buffers in memory,
//! where nothing can fail, and written out here, where a failure is an error. A stream
//! of several frames is one stream to every zstd reader. Each frame says its content
//! size, as zstd's own frames do, and its checksum can be left out.
//!
//! [`decompress_frame`] decodes one frame at a time, for the `zstd` command, which
//! looks at each frame's magic before it decodes it and words a failure its own way.

use std::io::{self, BufRead, BufReader, Read, Write};

use ruzstd::decoding::errors::{FrameDecoderError, FrameHeaderError, ReadFrameHeaderError};
use ruzstd::decoding::{BlockDecodingStrategy, FrameDecoder};
use ruzstd::encoding::{CompressionLevel, FrameCompressor};

use super::CodecError;

/// How many bytes each frame [`Writer`] makes holds.
pub const FRAME: usize = 4 << 20;

/// Reads a zstd stream of any number of frames.
pub struct Reader<R: Read> {
    source: BufReader<R>,
    decoder: FrameDecoder,
    in_frame: bool,
}

impl<R: Read> Reader<R> {
    /// A reader of the frames in `source`.
    pub fn new(source: R) -> Self {
        Self {
            source: BufReader::new(source),
            decoder: FrameDecoder::new(),
            in_frame: false,
        }
    }

    /// The problem a decoding error is.
    fn problem(error: &FrameDecoderError) -> CodecError {
        match error {
            FrameDecoderError::ReadFrameHeaderError(ReadFrameHeaderError::BadMagicNumber(_)) => {
                CodecError::NotThisFormat
            }
            FrameDecoderError::ReadFrameHeaderError(
                ReadFrameHeaderError::MagicNumberReadError(_),
            )
            | FrameDecoderError::FailedToReadChecksum(_) => CodecError::Truncated,
            FrameDecoderError::DictNotProvided { .. } => {
                CodecError::Unsupported("a frame that needs a dictionary".to_owned())
            }
            other => {
                let words = other.to_string();
                if words.contains("end of file") || words.contains("EOF") || words.contains("eof") {
                    CodecError::Truncated
                } else {
                    CodecError::Corrupt(words)
                }
            }
        }
    }

    /// Starts the next frame, passing over skippable ones; `false` at the end.
    fn next_frame(&mut self) -> io::Result<bool> {
        loop {
            if self.source.fill_buf()?.is_empty() {
                return Ok(false);
            }
            match self.decoder.reset(&mut self.source) {
                Ok(()) => return Ok(true),
                Err(FrameDecoderError::ReadFrameHeaderError(ReadFrameHeaderError::SkipFrame {
                    length,
                    ..
                })) => {
                    let skipped = io::copy(
                        &mut (&mut self.source).take(u64::from(length)),
                        &mut io::sink(),
                    )?;
                    if skipped < u64::from(length) {
                        return Err(CodecError::Truncated.into_io());
                    }
                }
                Err(error) => return Err(Self::problem(&error).into_io()),
            }
        }
    }
}

impl<R: Read> Read for Reader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            if !self.in_frame {
                if !self.next_frame()? {
                    return Ok(0);
                }
                self.in_frame = true;
            }
            if self.decoder.can_collect() > 0 {
                let n = self.decoder.read(buf)?;
                if n > 0 {
                    return Ok(n);
                }
            }
            if self.decoder.is_finished() {
                if let (Some(stored), Some(computed)) = (
                    self.decoder.get_checksum_from_data(),
                    self.decoder.get_calculated_checksum(),
                ) && stored != computed
                {
                    return Err(CodecError::Checksum.into_io());
                }
                self.in_frame = false;
                continue;
            }
            self.decoder
                .decode_blocks(
                    &mut self.source,
                    BlockDecodingStrategy::UptoBytes(buf.len()),
                )
                .map_err(|error| Self::problem(&error).into_io())?;
        }
    }
}

/// Writes a zstd stream at `ruzstd`'s fast level, a frame for each [`FRAME`] bytes,
/// frames compressed on several threads at once: the same bytes for any number.
pub struct Writer<W: Write> {
    inner: W,
    pending: Vec<u8>,
    /// Whether a frame was written: an empty stream is still one empty frame.
    wrote: bool,
    /// Whether each frame ends with its checksum.
    check: bool,
    /// How many frames are compressed at once.
    threads: usize,
}

/// One frame of `data`, with its checksum or without.
fn compress_frame(data: &[u8], check: bool) -> Vec<u8> {
    let mut compressed = Vec::with_capacity(data.len() / 2 + 64);
    let mut compressor = FrameCompressor::new(CompressionLevel::Fastest);
    compressor.set_source(data);
    compressor.set_drain(&mut compressed);
    compressor.compress();
    drop(compressor);
    finished_frame(compressed, data.len() as u64, check)
}

impl<W: Write> Writer<W> {
    /// A writer that compresses into `inner`, each frame with its checksum.
    pub const fn new(inner: W) -> Self {
        Self::with_check(inner, true)
    }

    /// A writer that compresses into `inner`, with checksums or without.
    pub const fn with_check(inner: W, check: bool) -> Self {
        Self {
            inner,
            pending: Vec::new(),
            wrote: false,
            check,
            threads: 1,
        }
    }

    /// The same writer compressing `threads` frames at once.
    #[must_use]
    pub fn with_threads(mut self, threads: usize) -> Self {
        self.threads = threads.max(1);
        self
    }

    /// Compresses what is pending as frames of [`FRAME`] bytes, at once, and writes
    /// them in order; nothing pending is one empty frame.
    fn frames(&mut self) -> io::Result<()> {
        let check = self.check;
        let chunks: Vec<&[u8]> = if self.pending.is_empty() {
            vec![&[]]
        } else {
            self.pending.chunks(FRAME).collect()
        };
        let frames =
            super::parallel::map_ordered(chunks, |chunk| Ok(compress_frame(chunk, check)))?;
        self.pending.clear();
        self.wrote = true;
        for frame in frames {
            self.inner.write_all(&frame)?;
        }
        Ok(())
    }

    /// Writes the last frames, and hands the writer back.
    ///
    /// # Errors
    ///
    /// When a frame cannot be written.
    pub fn finish(mut self) -> io::Result<W> {
        if !self.pending.is_empty() || !self.wrote {
            self.frames()?;
        }
        self.inner.flush()?;
        Ok(self.inner)
    }
}

impl<W: Write> Write for Writer<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let batch = FRAME * self.threads;
        let room = batch - self.pending.len();
        let taken = buf.len().min(room);
        self.pending
            .extend_from_slice(buf.get(..taken).unwrap_or_default());
        if self.pending.len() >= batch {
            self.frames()?;
        }
        Ok(taken)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// A frame of `ruzstd`'s (a window descriptor, no dictionary, no content size, a
/// checksum) with its content size, `size`, in a four-byte field, as zstd writes it,
/// and without its checksum unless `check`.
fn finished_frame(mut frame: Vec<u8>, size: u64, check: bool) -> Vec<u8> {
    let Some(mut descriptor) = frame.get(4).copied() else {
        return frame;
    };
    if descriptor & 0b100 != 0 && !check {
        descriptor &= !0b100;
        frame.truncate(frame.len().saturating_sub(4));
    }
    let single_segment = descriptor & 0x20 != 0;
    if descriptor & 0xc0 == 0 && !single_segment {
        if let Ok(size) = u32::try_from(size) {
            descriptor |= 0x80;
            let dictionary = match descriptor & 3 {
                0 => 0,
                1 => 1,
                2 => 2,
                _ => 4,
            };
            let at = 6 + dictionary;
            if at <= frame.len() {
                frame.splice(at..at, size.to_le_bytes());
            }
        }
    }
    if let Some(byte) = frame.get_mut(4) {
        *byte = descriptor;
    }
    frame
}

/// What stopped the decoding of one frame.
#[derive(Debug)]
pub enum FrameError {
    /// The input ends inside the frame: zstd's "premature end".
    PrematureEnd,
    /// The frame is damaged, by zstd's name for what is wrong with it.
    Decoding(&'static str),
    /// Reading the input failed.
    Read(io::Error),
    /// Writing the output failed.
    Write(io::Error),
}

/// zstd's output buffer: a frame's output is written a buffer at a time, its last part
/// only once the frame's checksum agrees.
const OUT_BUFFER: usize = 128 << 10;

/// What a decoding error of `ruzstd`'s is, at the point in `input` where it happened:
/// at the end of the input, the input ended early.
fn frame_failure<R: BufRead>(error: &FrameDecoderError, input: &mut R) -> FrameError {
    if input.fill_buf().map_or(true, <[u8]>::is_empty) {
        return FrameError::PrematureEnd;
    }
    FrameError::Decoding(match error {
        FrameDecoderError::WindowSizeTooBig { .. }
        | FrameDecoderError::FrameHeaderError(FrameHeaderError::WindowTooBig { .. })
        | FrameDecoderError::FailedToInitialize(FrameHeaderError::WindowTooBig { .. }) => {
            "Frame requires too much memory for decoding"
        }
        FrameDecoderError::ReadFrameHeaderError(ReadFrameHeaderError::InvalidFrameDescriptor(
            _,
        ))
        | FrameDecoderError::FrameHeaderError(_)
        | FrameDecoderError::FailedToInitialize(_) => "Unsupported frame parameter",
        FrameDecoderError::DictNotProvided { .. } => "Dictionary mismatch",
        _ => "Data corruption detected",
    })
}

/// Decodes the zstd frame, or the skippable frame, that `input` starts with into
/// `output`: the bytes written. The frame's checksum is checked when `check`.
///
/// # Errors
///
/// What stopped the decoding, as [`FrameError`] tells it.
pub fn decompress_frame<R: BufRead>(
    input: &mut R,
    output: &mut dyn Write,
    check: bool,
) -> Result<u64, FrameError> {
    let mut decoder = FrameDecoder::new();
    match decoder.reset(&mut *input) {
        Ok(()) => {}
        Err(FrameDecoderError::ReadFrameHeaderError(ReadFrameHeaderError::SkipFrame {
            length,
            ..
        })) => {
            let skipped = io::copy(&mut (&mut *input).take(u64::from(length)), &mut io::sink())
                .map_err(FrameError::Read)?;
            return if skipped < u64::from(length) {
                Err(FrameError::PrematureEnd)
            } else {
                Ok(0)
            };
        }
        Err(error) => return Err(frame_failure(&error, input)),
    }
    let mut pending = Vec::new();
    let mut written = 0_u64;
    loop {
        let finished = decoder.is_finished();
        if let Some(chunk) = decoder.collect() {
            pending.extend_from_slice(&chunk);
        }
        while pending.len() >= OUT_BUFFER && !finished {
            let rest = pending.split_off(OUT_BUFFER);
            output.write_all(&pending).map_err(FrameError::Write)?;
            written += pending.len() as u64;
            pending = rest;
        }
        if finished {
            break;
        }
        decoder
            .decode_blocks(&mut *input, BlockDecodingStrategy::UptoBytes(OUT_BUFFER))
            .map_err(|error| frame_failure(&error, input))?;
    }
    if check
        && let (Some(stored), Some(computed)) = (
            decoder.get_checksum_from_data(),
            decoder.get_calculated_checksum(),
        )
        && stored != computed
    {
        return Err(FrameError::Decoding("Restored data doesn't match checksum"));
    }
    output.write_all(&pending).map_err(FrameError::Write)?;
    Ok(written + pending.len() as u64)
}

impl<W: Write> super::Encoder for Writer<W> {
    fn finish(self: Box<Self>) -> io::Result<()> {
        (*self).finish().map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_made_at_once_are_the_bytes_made_one_by_one() {
        let data: Vec<u8> = (0..3 * FRAME + 12345)
            .map(|i| {
                u8::try_from(i * 7 % 251).unwrap_or(0) ^ u8::try_from(i / 4096 % 7).unwrap_or(0)
            })
            .collect();
        let make = |threads| {
            let mut writer = Writer::new(Vec::new()).with_threads(threads);
            for piece in data.chunks(100_000) {
                writer.write_all(piece).unwrap();
            }
            writer.finish().unwrap()
        };
        let one = make(1);
        assert_eq!(make(4), one);
        assert_eq!(make(16), one);
        let mut back = Vec::new();
        Reader::new(one.as_slice()).read_to_end(&mut back).unwrap();
        assert_eq!(back, data);
    }

    #[test]
    fn a_long_stream_is_several_frames_and_reads_as_one() {
        let data: Vec<u8> = (0..FRAME + 1000)
            .map(|n| u8::try_from(n % 7).unwrap())
            .collect();
        let mut writer = Writer::new(Vec::new());
        writer.write_all(&data).unwrap();
        let packed = writer.finish().unwrap();
        let mut out = Vec::new();
        Reader::new(packed.as_slice())
            .read_to_end(&mut out)
            .unwrap();
        assert_eq!(out.len(), data.len());
        assert_eq!(out, data);
    }

    /// `printf 'hello\n' | zstd` under zstd 1.5.7.
    const HELLO_ZST: &[u8] = &[
        0x28, 0xb5, 0x2f, 0xfd, 0x24, 0x06, 0x31, 0x00, 0x00, 0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x0a,
        0x53, 0x88, 0xbd, 0x91,
    ];

    fn one_frame(input: &[u8], check: bool) -> (Result<u64, FrameError>, Vec<u8>, usize) {
        let mut reader = io::BufReader::new(input);
        let mut out = Vec::new();
        let result = decompress_frame(&mut reader, &mut out, check);
        let left = reader.fill_buf().map_or(0, <[u8]>::len);
        (result, out, left)
    }

    #[test]
    fn frames_are_decoded_one_at_a_time_as_zstd_words_their_damage() {
        let mut two = HELLO_ZST.to_vec();
        two.extend_from_slice(b"junk");
        let (result, out, left) = one_frame(&two, true);
        assert!(matches!(result, Ok(6)), "{result:?}");
        assert_eq!((out.as_slice(), left), (b"hello\n".as_slice(), 4));
        let (result, _, _) = one_frame(HELLO_ZST.get(..12).unwrap(), true);
        assert!(
            matches!(result, Err(FrameError::PrematureEnd)),
            "{result:?}"
        );
        let mut bad = HELLO_ZST.to_vec();
        if let Some(byte) = bad.get_mut(12) {
            *byte = 0xff;
        }
        let (result, out, _) = one_frame(&bad, true);
        assert!(
            matches!(
                result,
                Err(FrameError::Decoding("Restored data doesn't match checksum"))
            ),
            "{result:?}"
        );
        assert!(out.is_empty(), "a bad frame's last part is held back");
        assert!(matches!(one_frame(&bad, false).0, Ok(6)));
        let skippable = [0x50, 0x2a, 0x4d, 0x18, 2, 0, 0, 0, b'a', b'b'];
        assert!(matches!(one_frame(&skippable, true).0, Ok(0)));
    }

    #[test]
    fn frames_say_their_size_and_drop_the_checksum_when_asked() {
        for check in [true, false] {
            let mut writer = Writer::with_check(Vec::new(), check);
            writer.write_all(b"hello\n").unwrap();
            let frame = writer.finish().unwrap();
            let descriptor = frame.get(4).copied().unwrap();
            assert_eq!(descriptor & 0xc0, 0x80, "a four-byte content size");
            assert_eq!(descriptor & 0b100 != 0, check);
            assert_eq!(frame.get(6..10), Some(6_u32.to_le_bytes().as_slice()));
            let (result, out, left) = one_frame(&frame, true);
            assert!(matches!(result, Ok(6)), "{result:?}");
            assert_eq!((out.as_slice(), left), (b"hello\n".as_slice(), 0));
        }
    }

    #[test]
    fn a_skippable_frame_is_passed_over_and_a_bad_checksum_is_caught() {
        let mut packed = vec![0x50, 0x2a, 0x4d, 0x18, 3, 0, 0, 0, b'x', b'y', b'z'];
        let mut writer = Writer::new(Vec::new());
        writer.write_all(b"hello\n").unwrap();
        let frame = writer.finish().unwrap();
        packed.extend_from_slice(&frame);
        let mut out = Vec::new();
        Reader::new(packed.as_slice())
            .read_to_end(&mut out)
            .unwrap();
        assert_eq!(out, b"hello\n");

        let mut bad = frame;
        let last = bad.len() - 1;
        if let Some(byte) = bad.get_mut(last) {
            *byte ^= 0xff;
        }
        let error = Reader::new(bad.as_slice())
            .read_to_end(&mut Vec::new())
            .unwrap_err();
        assert_eq!(
            super::super::codec_error(&error),
            Some(&CodecError::Checksum)
        );
    }
}
