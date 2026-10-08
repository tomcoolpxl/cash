//! RAR's packed data, read and decoded for the two front ends that read RAR, `7z` and
//! `rar`: an item's parts read as one stream across its volumes, each split part
//! checked; decrypted when they are; decoded by cash-archive's `rar` codecs into a sink
//! of the front end's.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

use cash_archive::rar::codec::rar13::Unpack15;
use cash_archive::rar::codec::rar20::Unpack20;
use cash_archive::rar::codec::rar29::Unpack29;
use cash_archive::rar::codec::rar50::{
    DecodedChunk, MemberFilter, StreamDecodeError, Unpack50Decoder,
};
use cash_archive::rar::crypto::rar13::Rar13Cipher;
use cash_archive::rar::crypto::rar15::Rar15Cipher;
use cash_archive::rar::crypto::rar20::Rar20Cipher;
use cash_archive::rar::crypto::rar30::Rar30Cipher;
use cash_archive::rar::crypto::rar50::{Rar50Cipher, Rar50Keys};
use cash_archive::rar::rar50::blake2sp;

/// How many bytes are read, decrypted or handed on at a time.
pub(crate) const CHUNK: usize = 1 << 16;

/// One part of an item's packed data, and the check its part header makes of it.
pub(crate) struct Part {
    pub(crate) volume: usize,
    pub(crate) pos: u64,
    pub(crate) size: u64,
    pub(crate) check: Option<PartCheck>,
}

/// A split part's own checksum of its packed bytes.
pub(crate) enum PartCheck {
    Crc(u32),
    Hashes {
        crc: Option<u32>,
        blake: Option<[u8; 32]>,
    },
}

/// An item's parts read as one stream, each split part checked (7-Zip's
/// `CVolsInStream`); `on_next` hears of each part after the first as it is begun.
pub(crate) struct VolsReader<'a> {
    volumes: &'a [PathBuf],
    files: &'a mut Vec<Option<File>>,
    parts: Vec<Part>,
    current: usize,
    rem: u64,
    open: bool,
    crc: crc32fast::Hasher,
    blake: Option<blake2sp::Hasher>,
    checking: bool,
    /// Whether every split part read so far checked.
    pub(crate) crc_ok: bool,
    on_next: Option<&'a mut dyn FnMut(usize) -> io::Result<()>>,
}

impl<'a> VolsReader<'a> {
    pub(crate) fn new(
        volumes: &'a [PathBuf],
        files: &'a mut Vec<Option<File>>,
        parts: Vec<Part>,
    ) -> Self {
        Self {
            volumes,
            files,
            parts,
            current: 0,
            rem: 0,
            open: false,
            crc: crc32fast::Hasher::new(),
            blake: None,
            checking: false,
            crc_ok: true,
            on_next: None,
        }
    }

    /// Calls `hook` with each part's index after the first, as reading reaches it.
    pub(crate) fn on_next(mut self, hook: &'a mut dyn FnMut(usize) -> io::Result<()>) -> Self {
        self.on_next = Some(hook);
        self
    }

    fn file(&mut self, volume: usize) -> io::Result<&mut File> {
        if self.files.len() < self.volumes.len() {
            self.files.resize_with(self.volumes.len(), || None);
        }
        let slot = &mut self.files[volume];
        if slot.is_none() {
            *slot = Some(File::open(&self.volumes[volume])?);
        }
        slot.as_mut()
            .ok_or_else(|| io::Error::other("a RAR volume is not open"))
    }

    /// The part just read to its end, checked.
    fn finish_part(&mut self) {
        let part = &self.parts[self.current];
        if self.checking {
            let crc = std::mem::replace(&mut self.crc, crc32fast::Hasher::new()).finalize();
            let ok = match &part.check {
                Some(PartCheck::Crc(want)) => crc == *want,
                Some(PartCheck::Hashes { crc: want, blake }) => {
                    want.is_none_or(|w| w == crc)
                        && match (blake, self.blake.take()) {
                            (Some(want), Some(hasher)) => hasher.finalize() == *want,
                            _ => true,
                        }
                }
                None => true,
            };
            if !ok {
                self.crc_ok = false;
            }
        }
        self.current += 1;
        self.open = false;
    }
}

impl Read for VolsReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while !buf.is_empty() {
            if !self.open {
                let Some(part) = self.parts.get(self.current) else {
                    return Ok(0);
                };
                if part.volume >= self.volumes.len() {
                    return Ok(0);
                }
                let (volume, pos, size) = (part.volume, part.pos, part.size);
                if self.current > 0
                    && let Some(hook) = self.on_next.as_mut()
                {
                    hook(self.current)?;
                }
                self.checking = self.crc_ok && part.check.is_some();
                self.blake = match &part.check {
                    Some(PartCheck::Hashes { blake: Some(_), .. }) if self.checking => {
                        Some(blake2sp::Hasher::new())
                    }
                    _ => None,
                };
                self.crc = crc32fast::Hasher::new();
                self.rem = size;
                self.file(volume)?.seek(SeekFrom::Start(pos))?;
                self.open = true;
                if size == 0 {
                    self.finish_part();
                    continue;
                }
            }
            let want = buf
                .len()
                .min(usize::try_from(self.rem).unwrap_or(usize::MAX));
            let volume = self.parts[self.current].volume;
            let got = self.file(volume)?.read(&mut buf[..want])?;
            if self.checking {
                self.crc.update(&buf[..got]);
                if let Some(blake) = &mut self.blake {
                    blake.update(&buf[..got]);
                }
            }
            self.rem -= got as u64;
            if self.rem == 0 {
                self.finish_part();
            }
            if got == 0 {
                return Ok(0);
            }
            return Ok(got);
        }
        Ok(0)
    }
}

/// The ciphers RAR's data is encrypted with.
pub(crate) enum Cipher {
    Rar5(Box<Rar50Cipher>),
    Rar3(Box<Rar30Cipher>),
    Rar2(Box<Rar20Cipher>),
    /// RAR 1.5's, a byte at a time.
    Rar15(Box<Rar15Cipher>),
    /// RAR 1.3's, a byte at a time.
    Rar13(Box<Rar13Cipher>),
}

impl Cipher {
    fn decrypt(&mut self, data: &mut [u8]) -> bool {
        match self {
            Self::Rar5(c) => c.decrypt_in_place(data).is_ok(),
            Self::Rar3(c) => c.decrypt_in_place(data).is_ok(),
            Self::Rar2(c) => c.decrypt_in_place(data).is_ok(),
            Self::Rar15(c) => {
                c.crypt_in_place(data);
                true
            }
            Self::Rar13(c) => {
                for byte in data {
                    *byte = c.decrypt_byte(*byte);
                }
                true
            }
        }
    }

    /// The block it decrypts whole: AES's 16 bytes, or a byte.
    const fn block(&self) -> usize {
        match self {
            Self::Rar5(_) | Self::Rar3(_) | Self::Rar2(_) => 16,
            Self::Rar15(_) | Self::Rar13(_) => 1,
        }
    }
}

/// A password as RAR 3 to 7 key their ciphers with: at most 127 UTF-16 units, as UTF-8.
pub(crate) fn password_utf8(password: &str) -> String {
    let mut units: Vec<u16> = password.encode_utf16().collect();
    units.truncate(127);
    String::from_utf16_lossy(&units)
}

/// Whether a RAR 5 encryption record's check, when its own checksum is right, says the
/// password is the one that made `keys`.
pub(crate) fn rar5_check_matches(keys: &Rar50Keys, check: [u8; 12]) -> bool {
    use sha2::Digest as _;
    let sum = sha2::Sha256::digest(&check[..8]);
    sum[..4] != check[8..] || keys.password_check == check[..8]
}

/// RAR 5's keys for a password, and whether the record's check (when its own checksum
/// is right) says the password is the one.
pub(crate) fn rar5_derive_keys(
    check: Option<[u8; 12]>,
    salt: [u8; 16],
    count: u8,
    password: &str,
) -> Option<(Rar50Keys, bool)> {
    let keys = Rar50Keys::derive(password_utf8(password).as_bytes(), salt, count).ok()?;
    let ok = check.is_none_or(|check| rar5_check_matches(&keys, check));
    Some((keys, ok))
}

/// The packed data, decrypted a block at a time; an incomplete block at the end is
/// dropped.
pub(crate) struct DecryptReader<R> {
    inner: R,
    cipher: Cipher,
    buf: Vec<u8>,
    at: usize,
}

impl<R> DecryptReader<R> {
    pub(crate) const fn new(inner: R, cipher: Cipher) -> Self {
        Self {
            inner,
            cipher,
            buf: Vec::new(),
            at: 0,
        }
    }
}

impl<R: Read> Read for DecryptReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.at == self.buf.len() {
            self.buf.clear();
            self.at = 0;
            let block = self.cipher.block();
            let mut chunk = vec![0u8; CHUNK];
            let mut got = 0;
            while got < chunk.len() {
                let n = self.inner.read(&mut chunk[got..])?;
                if n == 0 {
                    break;
                }
                got += n;
                if got % block == 0 && got >= block {
                    break;
                }
            }
            got -= got % block;
            chunk.truncate(got);
            if !self.cipher.decrypt(&mut chunk) {
                return Err(io::Error::other("RAR data does not decrypt"));
            }
            self.buf = chunk;
        }
        let n = out.len().min(self.buf.len() - self.at);
        out[..n].copy_from_slice(&self.buf[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// Where decoded bytes go: the front end's own, which may count, hash, write or send.
pub(crate) trait Sink {
    fn put(&mut self, data: &[u8]) -> io::Result<()>;
}

/// Why decoding stopped short.
#[derive(Debug)]
pub(crate) enum DecodeError {
    /// The sink refused the bytes.
    Sink(io::Error),
    /// The data does not decode.
    Data,
}

/// The copy method: the packed bytes as they are.
pub(crate) fn copy(input: &mut dyn Read, out: &mut dyn Sink) -> Result<(), DecodeError> {
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = match input.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(_) => return Err(DecodeError::Data),
        };
        out.put(&buf[..n]).map_err(DecodeError::Sink)?;
    }
}

/// RAR 5's filters on the decoded bytes: what a pending filter covers is held until the
/// filter's range is whole, then transformed and sent on.
struct Filtered<'a> {
    out: &'a mut dyn Sink,
    pending: VecDeque<MemberFilter>,
    /// The bytes held, and where in the output the first of them is.
    held: Vec<u8>,
    held_at: usize,
    /// Where in the output the next byte decoded goes.
    pos: usize,
    failed: bool,
}

impl Filtered<'_> {
    fn filter(&mut self, filter: MemberFilter) {
        self.pending.push_back(filter);
    }

    fn feed(&mut self, mut data: &[u8]) -> io::Result<()> {
        while !data.is_empty() {
            let Some(first) = self.pending.front().copied() else {
                self.release_all()?;
                self.out.put(data)?;
                self.pos += data.len();
                return Ok(());
            };
            if self.held.is_empty() && self.pos < first.start() {
                let n = data.len().min(first.start() - self.pos);
                self.out.put(&data[..n])?;
                self.pos += n;
                data = &data[n..];
                continue;
            }
            if self.held.is_empty() {
                self.held_at = self.pos;
            }
            let end = first.start() + first.length();
            let need = end.saturating_sub(self.held_at + self.held.len()).max(1);
            let n = data.len().min(need);
            self.held.extend_from_slice(&data[..n]);
            self.pos += n;
            data = &data[n..];
            self.apply_ready()?;
        }
        Ok(())
    }

    /// Applies each filter whose range is whole, and sends on what no filter still
    /// covers.
    fn apply_ready(&mut self) -> io::Result<()> {
        while let Some(first) = self.pending.front().copied() {
            let end = first.start() + first.length();
            if self.held_at + self.held.len() < end {
                break;
            }
            let from = first.start().saturating_sub(self.held_at);
            let to = end - self.held_at;
            match self.held.get_mut(from..to) {
                Some(range) => {
                    if first.apply(range).is_err() {
                        self.failed = true;
                    }
                }
                None => self.failed = true,
            }
            self.pending.pop_front();
            let keep_from = self.pending.front().map_or(self.held.len(), |next| {
                next.start()
                    .saturating_sub(self.held_at)
                    .min(self.held.len())
            });
            let sent: Vec<u8> = self.held.drain(..keep_from).collect();
            self.out.put(&sent)?;
            self.held_at += keep_from;
        }
        Ok(())
    }

    fn release_all(&mut self) -> io::Result<()> {
        if !self.held.is_empty() {
            let held = std::mem::take(&mut self.held);
            self.out.put(&held)?;
            self.held_at += held.len();
        }
        Ok(())
    }
}

/// A RAR 5 item decoded, its filters applied, into `out`.
pub(crate) fn decode5(
    decoder: &mut Unpack50Decoder,
    input: &mut dyn Read,
    version: u8,
    output_size: usize,
    dictionary: usize,
    solid: bool,
    out: &mut dyn Sink,
) -> Result<(), DecodeError> {
    let mut filtered = Filtered {
        out,
        pending: VecDeque::new(),
        held: Vec::new(),
        held_at: 0,
        pos: 0,
        failed: false,
    };
    let filtered = std::cell::RefCell::new(&mut filtered);
    let mut input = input;
    let result = decoder.decode_member_with_filters_to_sink(
        &mut input,
        version,
        output_size,
        dictionary,
        solid,
        |chunk| {
            let mut f = filtered.borrow_mut();
            match chunk {
                DecodedChunk::Bytes(bytes) => f.feed(bytes),
                DecodedChunk::Repeated { byte, len } => f.feed(&vec![byte; len]),
            }
        },
        &mut |filter| {
            filtered.borrow_mut().filter(filter);
            Ok(())
        },
    );
    let mut f = filtered.borrow_mut();
    match result {
        Ok(()) => {
            let flushed = f.release_all();
            if f.failed || !f.pending.is_empty() {
                Err(DecodeError::Data)
            } else {
                flushed.map_err(DecodeError::Sink)
            }
        }
        Err(StreamDecodeError::Sink(e)) => Err(DecodeError::Sink(e)),
        Err(_) => Err(DecodeError::Data),
    }
}

/// The decoder of one RAR 1.5 to 4 unpack version, kept for the next item of its kind.
pub(crate) enum Decoder4 {
    V15(Box<Unpack15>),
    V20(Box<Unpack20>),
    V29(Box<Unpack29>),
}

/// The write side of a RAR 1.5 to 4 decoder: `out`, its refusals kept.
struct SinkWriter<'a> {
    out: &'a mut dyn Sink,
    failed: Option<io::Error>,
}

impl io::Write for SinkWriter<'_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        match self.out.put(data) {
            Ok(()) => Ok(data.len()),
            Err(e) => {
                let kind = e.kind();
                self.failed = Some(e);
                Err(io::Error::from(kind))
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A RAR 1.5 to 4 item decoded by the decoder of its unpack version, solid with the one
/// before or not; `standard_filters_only`, RAR 3's VM programs refused unless they are
/// standard filters, as `WinRAR` 7.23 refuses them.
pub(crate) fn decode4(
    decoders: &mut Vec<(u8, Decoder4)>,
    version: u8,
    is_solid: bool,
    input: &mut dyn Read,
    output_size: usize,
    out: &mut dyn Sink,
    standard_filters_only: bool,
) -> Result<(), DecodeError> {
    let at = if let Some(at) = decoders.iter().position(|(v, _)| *v == version) {
        at
    } else {
        let decoder = if version < 20 {
            Decoder4::V15(Box::new(Unpack15::new()))
        } else if version < 29 {
            Decoder4::V20(Box::new(Unpack20::new()))
        } else if standard_filters_only {
            Decoder4::V29(Box::new(Unpack29::new().with_standard_filters_only()))
        } else {
            Decoder4::V29(Box::new(Unpack29::new()))
        };
        decoders.push((version, decoder));
        decoders.len() - 1
    };
    let decoder = &mut decoders[at].1;
    let mut input = input;
    let mut writer = SinkWriter { out, failed: None };
    let result = match decoder {
        Decoder4::V15(d) => {
            d.decode_member_from_reader(&mut input, output_size, is_solid, &mut writer)
        }
        Decoder4::V20(d) => {
            if !is_solid {
                **d = Unpack20::new();
            }
            d.decode_member_from_reader(&mut input, output_size, &mut writer)
        }
        Decoder4::V29(d) => {
            if is_solid {
                d.decode_member_from_reader(&mut input, output_size, &mut writer)
            } else {
                d.decode_non_solid_member_from_reader(&mut input, output_size, &mut writer)
            }
        }
    };
    match (result, writer.failed) {
        (Ok(()), _) => Ok(()),
        (Err(_), Some(error)) => Err(DecodeError::Sink(error)),
        (Err(_), None) => Err(DecodeError::Data),
    }
}
