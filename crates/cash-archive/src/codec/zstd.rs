//! zstd streams on `ruzstd`.
//!
//! [`Reader`] reads every frame in turn, passes over skippable frames, and checks each
//! frame's content checksum, which `ruzstd`'s own streaming decoder reads but does not
//! compare. [`Writer`] compresses at `ruzstd`'s fast level, one frame for each
//! [`FRAME`] bytes written: `ruzstd`'s compressor pulls its input and stops the program
//! on a failed read or write, so each frame is made between two buffers in memory,
//! where nothing can fail, and written out here, where a failure is an error. A stream
//! of several frames is one stream to every zstd reader.

use std::io::{self, BufRead, BufReader, Read, Write};

use ruzstd::decoding::errors::{FrameDecoderError, ReadFrameHeaderError};
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

/// Writes a zstd stream at `ruzstd`'s fast level, a frame for each [`FRAME`] bytes.
pub struct Writer<W: Write> {
    inner: W,
    pending: Vec<u8>,
    /// Whether a frame was written: an empty stream is still one empty frame.
    wrote: bool,
}

impl<W: Write> Writer<W> {
    /// A writer that compresses into `inner`.
    pub const fn new(inner: W) -> Self {
        Self {
            inner,
            pending: Vec::new(),
            wrote: false,
        }
    }

    /// Compresses what is pending as one frame, and writes it.
    fn frame(&mut self) -> io::Result<()> {
        let mut compressed = Vec::with_capacity(self.pending.len() / 2 + 64);
        let mut compressor = FrameCompressor::new(CompressionLevel::Fastest);
        compressor.set_source(self.pending.as_slice());
        compressor.set_drain(&mut compressed);
        compressor.compress();
        drop(compressor);
        self.pending.clear();
        self.wrote = true;
        self.inner.write_all(&compressed)
    }

    /// Writes the last frame, and hands the writer back.
    ///
    /// # Errors
    ///
    /// When the frame cannot be written.
    pub fn finish(mut self) -> io::Result<W> {
        if !self.pending.is_empty() || !self.wrote {
            self.frame()?;
        }
        self.inner.flush()?;
        Ok(self.inner)
    }
}

impl<W: Write> Write for Writer<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let room = FRAME - self.pending.len();
        let taken = buf.len().min(room);
        self.pending
            .extend_from_slice(buf.get(..taken).unwrap_or_default());
        if self.pending.len() >= FRAME {
            self.frame()?;
        }
        Ok(taken)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
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
