use std::{
    io::{self, Write},
    num::NonZeroU64,
};

use lzma_rust2::{
    Lzma2Options, Lzma2Writer, Lzma2WriterMt, LzmaWriter,
    filter::{bcj::BcjWriter, delta::DeltaWriter},
};

use crate::sevenz::{
    Aes256Sha256Encoder, Error,
    archive::{EncoderConfiguration, EncoderMethod},
    options::{EncoderOptions, LzmaParams},
    writer::CountingWriter,
};

/// A writer that ends its stream when finished, and then the stream it writes into.
pub(crate) trait Finish: Write {
    /// Writes what the coder still holds, its end, and finishes the writer it wraps.
    fn finish(self: Box<Self>) -> io::Result<()>;
}

/// A chain of coders, outermost last, ending in the archive's output.
pub(crate) type Chain<'a> = Box<dyn Finish + 'a>;

/// The end of a chain: the archive's output, flushed when finished.
pub(crate) struct Sink<W>(pub(crate) W);

impl<W: Write> Write for Sink<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<W: Write> Finish for Sink<W> {
    fn finish(mut self: Box<Self>) -> io::Result<()> {
        self.0.flush()
    }
}

type Inner<'a> = CountingWriter<Chain<'a>>;

/// One coder of a block being written, writing into the next one out.
pub(crate) enum Encoder<'a> {
    Copy(Inner<'a>),
    Bcj(BcjWriter<Inner<'a>>),
    Delta(DeltaWriter<Inner<'a>>),
    Lzma(LzmaWriter<Inner<'a>>),
    Lzma2(Lzma2Writer<Inner<'a>>),
    Lzma2Mt(Lzma2WriterMt<Inner<'a>>),
    Ppmd(Box<ppmd_rust::Ppmd7Encoder<Inner<'a>>>),
    Bzip2(bzip2::write::BzEncoder<Inner<'a>>),
    Deflate(flate2::write::DeflateEncoder<Inner<'a>>),
    Aes(Aes256Sha256Encoder<Inner<'a>>),
}

impl Write for Encoder<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Copy(w) => w.write(buf),
            Self::Bcj(w) => w.write(buf),
            Self::Delta(w) => w.write(buf),
            Self::Lzma(w) => w.write(buf),
            Self::Lzma2(w) => w.write(buf),
            Self::Lzma2Mt(w) => w.write(buf),
            Self::Ppmd(w) => w.write(buf),
            Self::Bzip2(w) => w.write(buf),
            Self::Deflate(w) => w.write(buf),
            Self::Aes(w) => w.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Copy(w) => w.flush(),
            Self::Bcj(w) => w.flush(),
            Self::Delta(w) => w.flush(),
            Self::Lzma(w) => w.flush(),
            Self::Lzma2(w) => w.flush(),
            Self::Lzma2Mt(w) => w.flush(),
            Self::Ppmd(w) => w.flush(),
            Self::Bzip2(w) => w.flush(),
            Self::Deflate(w) => w.flush(),
            Self::Aes(w) => w.flush(),
        }
    }
}

impl Finish for Encoder<'_> {
    fn finish(self: Box<Self>) -> io::Result<()> {
        let inner = match *self {
            Self::Copy(w) => w,
            Self::Bcj(w) => w.finish().map_err(io::Error::other)?,
            Self::Delta(w) => w.into_inner(),
            Self::Lzma(w) => w.finish().map_err(io::Error::other)?,
            Self::Lzma2(w) => w.finish().map_err(io::Error::other)?,
            Self::Lzma2Mt(w) => w.finish()?,
            Self::Ppmd(w) => w.finish(false)?,
            Self::Bzip2(w) => w.finish()?,
            Self::Deflate(w) => w.finish()?,
            Self::Aes(w) => w.finish()?,
        };
        inner.into_inner().finish()
    }
}

fn validate_lzma_dictionary_size(dict_size: u32) -> Result<(), Error> {
    // Keep the binary tree's two indices per dictionary position within i32,
    // and its Vec<i32> allocation within the platform's isize::MAX bytes.
    // This also leaves room for the encoder's lookahead and reserve buffers.
    let max_dict_size = ((1u64 << 30) - 1).min(isize::MAX as u64 / 8 - 1);
    if !(u64::from(lzma_rust2::DICT_SIZE_MIN)..=max_dict_size).contains(&u64::from(dict_size)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported LZMA dictionary size: {dict_size} (maximum {max_dict_size})"),
        )
        .into());
    }
    Ok(())
}

/// Puts the coder `config` describes in front of `input`.
pub(crate) fn add_encoder<'a>(
    input: Inner<'a>,
    config: &EncoderConfiguration,
) -> Result<Encoder<'a>, Error> {
    let options = config.options.as_ref();
    Ok(match config.method.id() {
        EncoderMethod::ID_COPY => Encoder::Copy(input),
        EncoderMethod::ID_DELTA => {
            let distance = match options {
                Some(EncoderOptions::Delta { distance }) => *distance,
                _ => 1,
            };
            Encoder::Delta(DeltaWriter::new(input, distance.clamp(1, 256) as usize))
        }
        EncoderMethod::ID_BCJ_X86 => Encoder::Bcj(BcjWriter::new_x86(input, 0)),
        EncoderMethod::ID_BCJ_ARM => Encoder::Bcj(BcjWriter::new_arm(input, 0)),
        EncoderMethod::ID_BCJ_ARM_THUMB => Encoder::Bcj(BcjWriter::new_arm_thumb(input, 0)),
        EncoderMethod::ID_BCJ_ARM64 => Encoder::Bcj(BcjWriter::new_arm64(input, 0)),
        EncoderMethod::ID_BCJ_IA64 => Encoder::Bcj(BcjWriter::new_ia64(input, 0)),
        EncoderMethod::ID_BCJ_SPARC => Encoder::Bcj(BcjWriter::new_sparc(input, 0)),
        EncoderMethod::ID_BCJ_PPC => Encoder::Bcj(BcjWriter::new_ppc(input, 0)),
        EncoderMethod::ID_BCJ_RISCV => Encoder::Bcj(BcjWriter::new_riscv(input, 0)),
        EncoderMethod::ID_LZMA => {
            let params = match options {
                Some(EncoderOptions::Lzma(params)) => params.clone(),
                _ => LzmaParams::with_preset(6),
            };
            validate_lzma_dictionary_size(params.dict_size)?;
            Encoder::Lzma(LzmaWriter::new_no_header(input, &params, false)?)
        }
        EncoderMethod::ID_LZMA2 => {
            let (params, threads, chunk_size) = match options {
                Some(EncoderOptions::Lzma2 {
                    params,
                    threads,
                    chunk_size,
                }) => (params.clone(), *threads, *chunk_size),
                _ => (LzmaParams::with_preset(6), 1, 0),
            };
            validate_lzma_dictionary_size(params.dict_size)?;
            let chunk = NonZeroU64::new(chunk_size);
            if threads > 1
                && let Some(chunk) = chunk
            {
                let options = Lzma2Options {
                    lzma_options: params,
                    chunk_size: Some(chunk),
                };
                Encoder::Lzma2Mt(Lzma2WriterMt::new(input, options, threads)?)
            } else {
                let options = Lzma2Options {
                    lzma_options: params,
                    chunk_size: chunk,
                };
                Encoder::Lzma2(Lzma2Writer::new(input, options))
            }
        }
        EncoderMethod::ID_PPMD => {
            let (order, memory) = match options {
                Some(EncoderOptions::Ppmd { order, memory }) => (*order, *memory),
                _ => (6, 16 << 20),
            };
            let encoder = ppmd_rust::Ppmd7Encoder::new(input, order, memory)
                .map_err(|err| Error::other(err.to_string()))?;
            Encoder::Ppmd(Box::new(encoder))
        }
        EncoderMethod::ID_BZIP2 => {
            let level = match options {
                Some(EncoderOptions::Bzip2 { level }) => *level,
                _ => 9,
            };
            Encoder::Bzip2(bzip2::write::BzEncoder::new(
                input,
                bzip2::Compression::new(level.clamp(1, 9)),
            ))
        }
        EncoderMethod::ID_DEFLATE => {
            let level = match options {
                Some(EncoderOptions::Deflate { level }) => *level,
                _ => 6,
            };
            Encoder::Deflate(flate2::write::DeflateEncoder::new(
                input,
                flate2::Compression::new(level.min(9)),
            ))
        }
        EncoderMethod::ID_AES256_SHA256 => match options {
            Some(EncoderOptions::Aes(aes)) => Encoder::Aes(Aes256Sha256Encoder::new(input, aes)?),
            _ => return Err(Error::PasswordRequired),
        },
        _ => {
            return Err(Error::UnsupportedCompressionMethod(
                config.method.name().to_string(),
            ));
        }
    })
}

/// The smallest LZMA2 dictionary property whose size holds `dict_size`, as 7-Zip rounds
/// it: sizes are 2^n and 3·2^(n-1), from 4 KiB; 40 means 4 GiB less one.
pub(crate) fn lzma2_dictionary_property(dict_size: u32) -> u8 {
    (0u8..40)
        .find(|&p| u64::from(dict_size) <= (2 | u64::from(p & 1)) << (p / 2 + 11))
        .unwrap_or(40)
}

/// The properties a coder is stored with.
pub(crate) fn properties(config: &EncoderConfiguration) -> Vec<u8> {
    let options = config.options.as_ref();
    match config.method.id() {
        EncoderMethod::ID_DELTA => {
            let distance = match options {
                Some(EncoderOptions::Delta { distance }) => *distance,
                _ => 1,
            };
            vec![u8::try_from(distance.clamp(1, 256) - 1).unwrap_or(0)]
        }
        EncoderMethod::ID_LZMA2 => {
            let dict_size = match options {
                Some(EncoderOptions::Lzma2 { params, .. }) => params.dict_size,
                _ => LzmaParams::with_preset(6).dict_size,
            };
            vec![lzma2_dictionary_property(dict_size)]
        }
        EncoderMethod::ID_LZMA => {
            let params = match options {
                Some(EncoderOptions::Lzma(params)) => params.clone(),
                _ => LzmaParams::with_preset(6),
            };
            let mut props = vec![params.get_props()];
            props.extend_from_slice(&params.dict_size.to_le_bytes());
            props
        }
        EncoderMethod::ID_PPMD => {
            let (order, memory) = match options {
                Some(EncoderOptions::Ppmd { order, memory }) => (*order, *memory),
                _ => (6, 16 << 20),
            };
            let mut props = vec![u8::try_from(order).unwrap_or(u8::MAX)];
            props.extend_from_slice(&memory.to_le_bytes());
            props
        }
        EncoderMethod::ID_AES256_SHA256 => match options {
            Some(EncoderOptions::Aes(aes)) => aes.properties(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_properties_round_up_as_7_zip_does() {
        assert_eq!(lzma2_dictionary_property(1), 0);
        assert_eq!(lzma2_dictionary_property(4 << 10), 0);
        assert_eq!(lzma2_dictionary_property(96 << 10), 9);
        assert_eq!(lzma2_dictionary_property(10 << 20), 23);
        assert_eq!(lzma2_dictionary_property(16 << 20), 24);
        assert_eq!(lzma2_dictionary_property(u32::MAX), 40);
    }
}
