//! Compressed streams: gzip, bzip2, xz, lzma, lzip and zstd, in pure Rust (D78).
//!
//! [`reader`] and [`writer`] wrap a stream in any of them; [`Codec::sniff`] knows one
//! by its first bytes and [`Codec::by_suffix`] by a file's name, as GNU tar does. What
//! goes wrong inside a stream comes back as an `io::Error` carrying a [`CodecError`]
//! ([`codec_error`] finds it), so a tool can say "corrupt" or "unexpected end of input"
//! in its own words; a failure to read or write the stream itself stays the plain error
//! it was.
//!
//! The backends: `flate2` on `miniz_oxide` for deflate, `bzip2` on `libbz2-rs-sys`,
//! `lzma-rust2` for xz, lzma and lzip, `ruzstd` for zstd, which writes at its fast level
//! only (about zstd's level 1), as one frame for each [`zstd::FRAME`] bytes.
//!
//! [`bzip2`] reads bzip2 streams one at a time, as the `bzip2` command needs them.
//! [`xz`] tells .xz, .lzma and .lz apart and reads a .xz index, as `xz` needs them.
//! [`gzip`] holds the gzip member reader and writer the `gzip` command needs, its
//! header's fields and `-l`'s numbers.

pub mod bzip2;
pub mod gzip;
pub mod xz;
pub mod zstd;

use std::fmt;
use std::io::{self, Read, Write};

/// A compression format.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Codec {
    /// gzip (RFC 1952), deflate inside.
    Gzip,
    /// bzip2.
    Bzip2,
    /// xz, LZMA2 inside.
    Xz,
    /// The old `.lzma` ("LZMA alone") format.
    Lzma,
    /// lzip.
    Lzip,
    /// Zstandard.
    Zstd,
}

impl Codec {
    /// Every codec, in the order they are tried.
    pub const ALL: [Self; 6] = [
        Self::Gzip,
        Self::Bzip2,
        Self::Xz,
        Self::Lzip,
        Self::Zstd,
        Self::Lzma,
    ];

    /// The program's name: `gzip`, `bzip2`, `xz`, `lzma`, `lzip`, `zstd`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Gzip => "gzip",
            Self::Bzip2 => "bzip2",
            Self::Xz => "xz",
            Self::Lzma => "lzma",
            Self::Lzip => "lzip",
            Self::Zstd => "zstd",
        }
    }

    /// The suffix a compressed file takes: `.gz`, `.bz2`, `.xz`, `.lzma`, `.lz`, `.zst`.
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Gzip => ".gz",
            Self::Bzip2 => ".bz2",
            Self::Xz => ".xz",
            Self::Lzma => ".lzma",
            Self::Lzip => ".lz",
            Self::Zstd => ".zst",
        }
    }

    /// The levels the codec takes, and the one used when none is asked for.
    pub const fn levels(self) -> (u32, u32, u32) {
        match self {
            Self::Gzip => (1, 9, 6),
            Self::Bzip2 => (1, 9, 9),
            Self::Xz | Self::Lzma | Self::Lzip => (0, 9, 6),
            Self::Zstd => (1, 19, 3),
        }
    }

    /// The codec whose stream starts with `first` (a few bytes are enough; eight make
    /// it sure), as GNU tar and `file` know them.
    pub const fn sniff(first: &[u8]) -> Option<Self> {
        match first {
            [0x1f, 0x8b, ..] => Some(Self::Gzip),
            [b'B', b'Z', b'h', b'1'..=b'9', ..] => Some(Self::Bzip2),
            [0xfd, b'7', b'z', b'X', b'Z', 0x00, ..] => Some(Self::Xz),
            [b'L', b'Z', b'I', b'P', ..] => Some(Self::Lzip),
            [0x28, 0xb5, 0x2f, 0xfd, ..] => Some(Self::Zstd),
            // A skippable frame, which only zstd's streams begin with.
            [0x50..=0x5f, 0x2a, 0x4d, 0x18, ..] => Some(Self::Zstd),
            // `.lzma`: the properties byte of every preset, then a dictionary size.
            [0x5d, 0x00, 0x00, ..] => Some(Self::Lzma),
            _ => None,
        }
    }

    /// The codec a file's name says, and what the name becomes without it, as GNU
    /// tar 1.35's table has them: `a.tgz` is gzip and `a.tar`, `a.gz` is gzip and `a`.
    pub fn by_suffix(name: &str) -> Option<(Self, String)> {
        const TABLE: [(&str, Codec, &str); 17] = [
            ("gz", Codec::Gzip, ""),
            ("tgz", Codec::Gzip, ".tar"),
            ("taz", Codec::Gzip, ".tar"),
            ("bz2", Codec::Bzip2, ""),
            ("tbz", Codec::Bzip2, ".tar"),
            ("tbz2", Codec::Bzip2, ".tar"),
            ("tz2", Codec::Bzip2, ".tar"),
            ("lz", Codec::Lzip, ""),
            ("lzma", Codec::Lzma, ""),
            ("tlz", Codec::Lzma, ".tar"),
            ("xz", Codec::Xz, ""),
            ("txz", Codec::Xz, ".tar"),
            ("zst", Codec::Zstd, ""),
            ("tzst", Codec::Zstd, ".tar"),
            ("gzip", Codec::Gzip, ""),
            ("bzip2", Codec::Bzip2, ""),
            ("zstd", Codec::Zstd, ""),
        ];
        let (stem, extension) = name.rsplit_once('.')?;
        if stem.is_empty() || stem.ends_with(['/', '\\']) {
            return None;
        }
        TABLE
            .iter()
            .find(|(suffix, _, _)| *suffix == extension)
            .map(|(_, codec, becomes)| (*codec, format!("{stem}{becomes}")))
    }
}

impl fmt::Display for Codec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What went wrong inside a compressed stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodecError {
    /// Not a stream of this codec at all: its magic is not there.
    NotThisFormat,
    /// The stream breaks the format.
    Corrupt(String),
    /// A checksum in the stream does not match the data.
    Checksum,
    /// The stream ends before it is whole.
    Truncated,
    /// A feature of the format this reader does not have.
    Unsupported(String),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotThisFormat => f.write_str("not in the expected format"),
            Self::Corrupt(detail) => write!(f, "corrupt data ({detail})"),
            Self::Checksum => f.write_str("checksum mismatch"),
            Self::Truncated => f.write_str("unexpected end of input"),
            Self::Unsupported(what) => write!(f, "unsupported: {what}"),
        }
    }
}

impl std::error::Error for CodecError {}

impl CodecError {
    /// This problem as an `io::Error`, which a `Read` or `Write` can return.
    pub fn into_io(self) -> io::Error {
        let kind = match self {
            Self::Truncated => io::ErrorKind::UnexpectedEof,
            Self::Unsupported(_) => io::ErrorKind::Unsupported,
            _ => io::ErrorKind::InvalidData,
        };
        io::Error::new(kind, self)
    }
}

/// The [`CodecError`] an error from a [`reader`] or [`writer`] carries, when the
/// trouble was inside the stream rather than in reading or writing it.
pub fn codec_error(error: &io::Error) -> Option<&CodecError> {
    error.get_ref()?.downcast_ref::<CodecError>()
}

/// A reader whose stream errors become [`CodecError`]s.
struct Checked<R> {
    inner: R,
    classify: fn(&io::Error) -> Option<CodecError>,
}

impl<R: Read> Read for Checked<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner
            .read(buf)
            .map_err(|error| match (self.classify)(&error) {
                Some(problem) => problem.into_io(),
                None => error,
            })
    }
}

/// How an error from flate2 or lzma-rust2 reads: its kind, and its words.
fn classify_by_kind(error: &io::Error) -> Option<CodecError> {
    if codec_error(error).is_some() {
        return None;
    }
    let words = error.to_string();
    match error.kind() {
        io::ErrorKind::UnexpectedEof => Some(CodecError::Truncated),
        io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput => {
            if words.contains("checksum") || words.contains("crc") || words.contains("CRC") {
                Some(CodecError::Checksum)
            } else if words.contains("magic") || words.contains("header") {
                Some(CodecError::NotThisFormat)
            } else {
                Some(CodecError::Corrupt(words))
            }
        }
        io::ErrorKind::Unsupported => Some(CodecError::Unsupported(words)),
        _ => None,
    }
}

/// How an error from bzip2 reads: its own error kinds, wrapped.
fn classify_bzip2(error: &io::Error) -> Option<CodecError> {
    if let Some(inner) = error
        .get_ref()
        .and_then(|e| e.downcast_ref::<::bzip2::Error>())
    {
        return Some(match inner {
            ::bzip2::Error::DataMagic => CodecError::NotThisFormat,
            ::bzip2::Error::Data => CodecError::Checksum,
            other => CodecError::Corrupt(other.to_string()),
        });
    }
    (error.kind() == io::ErrorKind::UnexpectedEof).then_some(CodecError::Truncated)
}

/// A reader of `codec`'s stream from `inner`: every stream or member in turn, as their
/// own programs decompress concatenated ones.
pub fn reader<'a>(codec: Codec, inner: impl Read + 'a) -> Box<dyn Read + 'a> {
    match codec {
        Codec::Gzip => Box::new(Checked {
            inner: flate2::read::MultiGzDecoder::new(inner),
            classify: classify_by_kind,
        }),
        Codec::Bzip2 => Box::new(Checked {
            inner: ::bzip2::read::MultiBzDecoder::new(inner),
            classify: classify_bzip2,
        }),
        Codec::Xz => Box::new(Checked {
            inner: lzma_rust2::XzReader::new(inner, true),
            classify: classify_by_kind,
        }),
        Codec::Lzip => Box::new(Checked {
            inner: lzma_rust2::LzipReader::new(inner),
            classify: classify_by_kind,
        }),
        Codec::Lzma => Box::new(LazyLzma::Header(Some(Box::new(inner)))),
        Codec::Zstd => Box::new(zstd::Reader::new(inner)),
    }
}

/// `.lzma`'s reader, made once its header is read, at the first read.
enum LazyLzma<'a> {
    Header(Option<Box<dyn Read + 'a>>),
    Body(Box<Checked<lzma_rust2::LzmaReader<Box<dyn Read + 'a>>>>),
}

impl Read for LazyLzma<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Self::Header(inner) = self {
            let Some(inner) = inner.take() else {
                return Ok(0);
            };
            let body = lzma_rust2::LzmaReader::new_mem_limit(inner, u32::MAX, None)
                .map_err(|error| classify_by_kind(&error).map_or(error, CodecError::into_io))?;
            *self = Self::Body(Box::new(Checked {
                inner: body,
                classify: classify_by_kind,
            }));
        }
        match self {
            Self::Body(body) => body.read(buf),
            Self::Header(_) => Ok(0),
        }
    }
}

/// A stream being compressed: written to, then finished, which writes its end.
pub trait Encoder: Write {
    /// Writes the end of the stream and flushes it.
    ///
    /// # Errors
    ///
    /// When the end cannot be written.
    fn finish(self: Box<Self>) -> io::Result<()>;
}

impl<W: Write> Encoder for flate2::write::GzEncoder<W> {
    fn finish(self: Box<Self>) -> io::Result<()> {
        let mut inner = (*self).finish()?;
        inner.flush()
    }
}

impl<W: Write> Encoder for ::bzip2::write::BzEncoder<W> {
    fn finish(self: Box<Self>) -> io::Result<()> {
        let mut inner = (*self).finish()?;
        inner.flush()
    }
}

impl<W: Write> Encoder for lzma_rust2::XzWriter<W> {
    fn finish(self: Box<Self>) -> io::Result<()> {
        let mut inner = (*self).finish()?;
        inner.flush()
    }
}

impl<W: Write> Encoder for lzma_rust2::LzipWriter<W> {
    fn finish(self: Box<Self>) -> io::Result<()> {
        let mut inner = (*self).finish()?;
        inner.flush()
    }
}

impl<W: Write> Encoder for lzma_rust2::LzmaWriter<W> {
    fn finish(self: Box<Self>) -> io::Result<()> {
        let mut inner = (*self).finish()?;
        inner.flush()
    }
}

/// A writer that compresses into `inner` as `codec` at `level` (clamped to the
/// codec's levels): a gzip stream as GNU tar's `gzip` writes one, no name, no time, the
/// OS byte Unix's.
///
/// # Errors
///
/// When the codec's header cannot be written.
pub fn writer<'a>(
    codec: Codec,
    inner: impl Write + 'a,
    level: u32,
) -> io::Result<Box<dyn Encoder + 'a>> {
    let (low, high, _) = codec.levels();
    let level = level.clamp(low, high);
    Ok(match codec {
        Codec::Gzip => Box::new(
            flate2::GzBuilder::new()
                .operating_system(gzip::OS_UNIX)
                .mtime(0)
                .write(inner, flate2::Compression::new(level)),
        ),
        Codec::Bzip2 => Box::new(::bzip2::write::BzEncoder::new(
            inner,
            ::bzip2::Compression::new(level),
        )),
        Codec::Xz => Box::new(lzma_rust2::XzWriter::new(
            inner,
            lzma_rust2::XzOptions::with_preset(level),
        )?),
        Codec::Lzip => Box::new(lzma_rust2::LzipWriter::new(
            inner,
            lzma_rust2::LzipOptions::with_preset(level),
        )),
        Codec::Lzma => Box::new(lzma_rust2::LzmaWriter::new_use_header(
            inner,
            &lzma_rust2::LzmaOptions::with_preset(level),
            None,
        )?),
        Codec::Zstd => Box::new(zstd::Writer::new(inner)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(codec: Codec, data: &[u8]) -> Vec<u8> {
        let mut packed = Vec::new();
        let mut encoder = writer(codec, &mut packed, codec.levels().2).unwrap();
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap();
        assert_eq!(Codec::sniff(&packed), Some(codec), "{codec}: {packed:02x?}");
        let mut out = Vec::new();
        reader(codec, packed.as_slice())
            .read_to_end(&mut out)
            .unwrap();
        out
    }

    #[test]
    fn every_codec_round_trips_and_knows_its_own_magic() {
        let data: Vec<u8> = (0..200_000u32).map(|n| (n % 251) as u8).collect();
        for codec in Codec::ALL {
            assert_eq!(round_trip(codec, &data), data, "{codec}");
            assert_eq!(round_trip(codec, b""), b"", "{codec} of nothing");
            assert_eq!(round_trip(codec, b"hello\n"), b"hello\n", "{codec}");
        }
    }

    #[test]
    fn concatenated_streams_read_as_one() {
        for codec in [
            Codec::Gzip,
            Codec::Bzip2,
            Codec::Xz,
            Codec::Lzip,
            Codec::Zstd,
        ] {
            let mut packed = Vec::new();
            for part in [b"one\n".as_slice(), b"two\n"] {
                let mut encoder = writer(codec, &mut packed, codec.levels().2).unwrap();
                encoder.write_all(part).unwrap();
                encoder.finish().unwrap();
            }
            let mut out = Vec::new();
            reader(codec, packed.as_slice())
                .read_to_end(&mut out)
                .unwrap();
            assert_eq!(out, b"one\ntwo\n", "{codec}");
        }
    }

    #[test]
    fn damage_is_told_apart_from_reading_trouble() {
        for codec in Codec::ALL {
            let mut packed = Vec::new();
            let mut encoder = writer(codec, &mut packed, codec.levels().2).unwrap();
            encoder.write_all(&[7u8; 5000]).unwrap();
            encoder.finish().unwrap();
            let cut = packed.get(..packed.len() / 2).unwrap().to_vec();
            let error = reader(codec, cut.as_slice())
                .read_to_end(&mut Vec::new())
                .unwrap_err();
            assert!(
                codec_error(&error).is_some(),
                "{codec}: a truncated stream said {error:?}"
            );
        }
    }

    #[test]
    fn codecs_are_known_by_name() {
        assert_eq!(
            Codec::by_suffix("a.tgz"),
            Some((Codec::Gzip, "a.tar".into()))
        );
        assert_eq!(
            Codec::by_suffix("dir/a.tar.xz"),
            Some((Codec::Xz, "dir/a.tar".into()))
        );
        assert_eq!(
            Codec::by_suffix("a.tzst"),
            Some((Codec::Zstd, "a.tar".into()))
        );
        assert_eq!(
            Codec::by_suffix("a.tlz"),
            Some((Codec::Lzma, "a.tar".into()))
        );
        assert_eq!(Codec::by_suffix("a.lz"), Some((Codec::Lzip, "a".into())));
        assert_eq!(Codec::by_suffix(".gz"), None);
        assert_eq!(Codec::by_suffix("a.tar"), None);
        assert_eq!(Codec::sniff(b"BZh9"), Some(Codec::Bzip2));
        assert_eq!(Codec::sniff(b"PK\x03\x04"), None);
    }
}
