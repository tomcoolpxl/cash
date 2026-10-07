//! A zip member's data where a compressed stream is not enough, as 7-Zip writes it.
//!
//! LZMA goes behind its zip header (the SDK's version, the properties' size and the
//! properties), `PPMd` variant I behind its two bytes of parameters, and the traditional
//! encryption starts with its header of random bytes and a check.

use std::io::{self, Write};

use lzma_rust2::{LzmaOptions, LzmaWriter};
use ppmd_rust::{Ppmd8Encoder, RestoreMethod};

use super::crypt::{Encrypt, Keys, make_header};
use crate::codec::Encoder;

/// The LZMA SDK version 7-Zip 26.03 puts in a zip's LZMA header.
const LZMA_SDK_VERSION: [u8; 2] = [26, 3];

/// Zip's LZMA (method 14): its header, then LZMA with or without the end marker.
///
/// # Errors
///
/// When the header cannot be written.
pub fn lzma_writer<'a>(
    mut inner: impl Write + 'a,
    options: &LzmaOptions,
    end_marker: bool,
) -> io::Result<Box<dyn Encoder + 'a>> {
    inner.write_all(&LZMA_SDK_VERSION)?;
    inner.write_all(&[5, 0, options.get_props()])?;
    inner.write_all(&options.dict_size.to_le_bytes())?;
    Ok(Box::new(LzmaWriter::new_no_header(
        inner, options, end_marker,
    )?))
}

/// `PPMd` variant I's encoder, finished with its end marker.
struct Ppmd8Writer<W: Write>(Ppmd8Encoder<W>);

impl<W: Write> Write for Ppmd8Writer<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<W: Write> Encoder for Ppmd8Writer<W> {
    fn finish(self: Box<Self>) -> io::Result<()> {
        let mut inner = self.0.finish(true)?;
        inner.flush()
    }
}

/// Zip's `PPMd` (method 98): the order, the memory in MiB and whether the model is cut
/// off rather than restarted when it fills, in two bytes; then variant I's data.
///
/// # Errors
///
/// When the parameters are out of range or cannot be written.
pub fn ppmd_writer<'a>(
    mut inner: impl Write + 'a,
    order: u32,
    memory_mb: u32,
    cut_off: bool,
) -> io::Result<Box<dyn Encoder + 'a>> {
    let params = (order.saturating_sub(1) & 0xF)
        | ((memory_mb.saturating_sub(1) & 0xFF) << 4)
        | (u32::from(cut_off) << 12);
    inner.write_all(&u16::try_from(params).unwrap_or(0).to_le_bytes())?;
    let restore = if cut_off {
        RestoreMethod::CutOff
    } else {
        RestoreMethod::Restart
    };
    let encoder = Ppmd8Encoder::new(inner, order, memory_mb << 20, restore)
        .map_err(|e| io::Error::other(format!("{e:?}")))?;
    Ok(Box::new(Ppmd8Writer(encoder)))
}

/// The traditional encryption: its 12-byte header, then what is written encrypted.
///
/// The header written to `inner` is ten random bytes and then `check`: the CRC's high
/// half, or the time's low half when a descriptor follows.
///
/// # Errors
///
/// When the random bytes cannot be had or the header written.
pub fn zip_crypto_writer<W: Write>(
    mut inner: W,
    password: &[u8],
    check: u16,
) -> io::Result<Encrypt<W>> {
    let mut keys = Keys::new(password);
    let mut random = [0_u8; 10];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    let [low, high] = check.to_le_bytes();
    let header = make_header(&mut keys, random, high, low);
    inner.write_all(&header)?;
    Ok(Encrypt::new(inner, keys))
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;
    use crate::zip::crypt::{Decrypt, check_header};

    #[test]
    fn lzma_members_start_with_7_zips_header() {
        let options = LzmaOptions::with_preset(1);
        let mut packed = Vec::new();
        let mut writer = lzma_writer(&mut packed, &options, true).unwrap();
        writer.write_all(b"hello hello hello").unwrap();
        writer.finish().unwrap();
        assert_eq!(&packed[..4], &[26, 3, 5, 0]);
        assert_eq!(packed[4], options.get_props());
        assert_eq!(&packed[5..9], &options.dict_size.to_le_bytes());
    }

    #[test]
    fn ppmd_members_read_back() {
        let data = b"abracadabra abracadabra abracadabra".repeat(20);
        let mut packed = Vec::new();
        let mut writer = ppmd_writer(&mut packed, 6, 16, false).unwrap();
        writer.write_all(&data).unwrap();
        writer.finish().unwrap();
        let params = u16::from_le_bytes([packed[0], packed[1]]);
        assert_eq!(params, 5 | (15 << 4));
        let mut decoder =
            ppmd_rust::Ppmd8Decoder::new(&packed[2..], 6, 16 << 20, RestoreMethod::Restart)
                .unwrap();
        let mut out = vec![0; data.len()];
        decoder.read_exact(&mut out).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn zip_crypto_headers_check_the_password() {
        let mut stored = Vec::new();
        let mut writer = zip_crypto_writer(&mut stored, b"pw", 0xABCD).unwrap();
        writer.write_all(b"plain").unwrap();
        let mut keys = Keys::new(b"pw");
        let header: [u8; 12] = stored[..12].try_into().unwrap();
        assert!(check_header(&mut keys, &header, 0xAB));
        let mut plain = Vec::new();
        Decrypt::new(&stored[12..], keys)
            .read_to_end(&mut plain)
            .unwrap();
        assert_eq!(plain, b"plain");
    }
}
