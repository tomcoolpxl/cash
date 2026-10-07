//! `WinZip`'s AES encryption, as 7-Zip and `WinZip` write it.
//!
//! The keys come from the password by PBKDF2 with HMAC-SHA1; the data is AES in counter
//! mode with a little-endian counter from one, and an HMAC-SHA1 of it follows.
//!
//! The member's method is 99; its real method and the key's strength are in the `0x9901`
//! extra field. The data starts with a salt and two bytes that check the password, and
//! ends with ten bytes of the check.

use std::io::{self, Read, Write};

use aes::cipher::{Array, BlockCipherEncrypt, KeyInit};
use hmac::{Mac, SimpleHmacReset};
use sha1::Sha1;

use super::{extra_id, fields, le16};

/// What the `0x9901` field says.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AesInfo {
    /// 1 (AE-1, the CRC kept) or 2 (AE-2, the CRC left out).
    pub vendor_version: u16,
    /// 1, 2 or 3: AES-128, -192 or -256.
    pub strength: u8,
    /// The method the data was compressed with before it was encrypted.
    pub method: u16,
}

impl AesInfo {
    /// The salt's length.
    pub const fn salt_len(self) -> usize {
        match self.strength {
            1 => 8,
            2 => 12,
            _ => 16,
        }
    }

    const fn key_len(self) -> usize {
        match self.strength {
            1 => 16,
            2 => 24,
            _ => 32,
        }
    }

    /// The bytes the encryption adds around the data: the salt, the password check and
    /// the authentication code.
    pub const fn overhead(self) -> u64 {
        self.salt_len() as u64 + 2 + 10
    }
}

/// The `0x9901` field of an extra field.
pub fn aes_info(extra: &[u8]) -> Option<AesInfo> {
    let (_, data) = fields(extra)
        .into_iter()
        .find(|(id, _)| *id == extra_id::AES)?;
    if data.len() < 7 || data.get(2..4) != Some(b"AE") {
        return None;
    }
    Some(AesInfo {
        vendor_version: le16(data, 0),
        strength: *data.get(4)?,
        method: le16(data, 5),
    })
}

/// The block cipher, at whichever strength.
enum Cipher {
    Aes128(aes::Aes128),
    Aes192(aes::Aes192),
    Aes256(aes::Aes256),
}

impl Cipher {
    fn new(key: &[u8]) -> Option<Self> {
        Some(match key.len() {
            16 => Self::Aes128(aes::Aes128::new_from_slice(key).ok()?),
            24 => Self::Aes192(aes::Aes192::new_from_slice(key).ok()?),
            _ => Self::Aes256(aes::Aes256::new_from_slice(key).ok()?),
        })
    }

    fn encrypt(&self, bytes: [u8; 16]) -> [u8; 16] {
        let mut block = Array::from(bytes);
        match self {
            Self::Aes128(c) => c.encrypt_block(&mut block),
            Self::Aes192(c) => c.encrypt_block(&mut block),
            Self::Aes256(c) => c.encrypt_block(&mut block),
        }
        block.into()
    }
}

/// An AES-encrypted member's data read as plain, its authentication code checked at
/// the end.
pub struct AesReader<R> {
    inner: R,
    cipher: Cipher,
    mac: SimpleHmacReset<Sha1>,
    counter: u128,
    keystream: [u8; 16],
    used: usize,
    /// Encrypted bytes not yet read.
    remaining: u64,
}

/// Why an AES member cannot be opened.
#[derive(Debug)]
pub enum AesError {
    /// The password's check bytes do not match.
    BadPassword,
    /// Reading the salt failed.
    Io(io::Error),
}

/// Opens an AES-encrypted member: `raw` is its stored data, salt first, and
/// `stored_size` how much of it there is.
///
/// # Errors
///
/// When the password does not open it, or the salt cannot be read.
pub fn aes_reader<R: Read>(
    mut raw: R,
    stored_size: u64,
    info: AesInfo,
    password: &[u8],
) -> Result<AesReader<R>, AesError> {
    let mut salt = vec![0_u8; info.salt_len()];
    raw.read_exact(&mut salt).map_err(AesError::Io)?;
    let mut check = [0_u8; 2];
    raw.read_exact(&mut check).map_err(AesError::Io)?;
    let key_len = info.key_len();
    let mut derived = vec![0_u8; 2 * key_len + 2];
    pbkdf2::pbkdf2::<SimpleHmacReset<Sha1>>(password, &salt, 1000, &mut derived)
        .map_err(|_| AesError::BadPassword)?;
    if derived.get(2 * key_len..) != Some(&check[..]) {
        return Err(AesError::BadPassword);
    }
    let cipher =
        Cipher::new(derived.get(..key_len).unwrap_or_default()).ok_or(AesError::BadPassword)?;
    let mac = <SimpleHmacReset<Sha1> as hmac::KeyInit>::new_from_slice(
        derived.get(key_len..2 * key_len).unwrap_or_default(),
    )
    .map_err(|_| AesError::BadPassword)?;
    Ok(AesReader {
        inner: raw,
        cipher,
        mac,
        counter: 1,
        keystream: [0; 16],
        used: 16,
        remaining: stored_size.saturating_sub(info.overhead()),
    })
}

/// Plain data written AES-encrypted: the salt and the password check first, the
/// authentication code at [`AesWriter::finish`].
pub struct AesWriter<W> {
    inner: W,
    cipher: Cipher,
    mac: SimpleHmacReset<Sha1>,
    counter: u128,
    keystream: [u8; 16],
    used: usize,
}

/// Starts an AES-encrypted member's data in `inner` at `strength` (1, 2 or 3: AES-128,
/// -192 or -256) with a random salt.
///
/// # Errors
///
/// When the salt or the check cannot be made or written.
pub fn aes_writer<W: Write>(
    mut inner: W,
    strength: u8,
    password: &[u8],
) -> io::Result<AesWriter<W>> {
    let info = AesInfo {
        vendor_version: 2,
        strength,
        method: 0,
    };
    let mut salt = vec![0_u8; info.salt_len()];
    getrandom::fill(&mut salt).map_err(io::Error::other)?;
    let key_len = info.key_len();
    let mut derived = vec![0_u8; 2 * key_len + 2];
    pbkdf2::pbkdf2::<SimpleHmacReset<Sha1>>(password, &salt, 1000, &mut derived)
        .map_err(io::Error::other)?;
    let bad = || io::Error::other("AES key");
    let cipher = Cipher::new(derived.get(..key_len).unwrap_or_default()).ok_or_else(bad)?;
    let mac = <SimpleHmacReset<Sha1> as hmac::KeyInit>::new_from_slice(
        derived.get(key_len..2 * key_len).unwrap_or_default(),
    )
    .map_err(|_| bad())?;
    inner.write_all(&salt)?;
    inner.write_all(derived.get(2 * key_len..).unwrap_or_default())?;
    Ok(AesWriter {
        inner,
        cipher,
        mac,
        counter: 1,
        keystream: [0; 16],
        used: 16,
    })
}

impl<W: Write> AesWriter<W> {
    /// Writes the authentication code; returns the writer underneath.
    ///
    /// # Errors
    ///
    /// When the code cannot be written.
    pub fn finish(mut self) -> io::Result<W> {
        let code = self.mac.finalize_reset().into_bytes();
        self.inner.write_all(code.get(..10).unwrap_or_default())?;
        Ok(self.inner)
    }
}

impl<W: Write> Write for AesWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut cipher = buf.to_vec();
        for byte in &mut cipher {
            if self.used == 16 {
                self.keystream = self.cipher.encrypt(self.counter.to_le_bytes());
                self.counter = self.counter.wrapping_add(1);
                self.used = 0;
            }
            *byte ^= self.keystream.get(self.used).copied().unwrap_or(0);
            self.used += 1;
        }
        self.mac.update(&cipher);
        self.inner.write_all(&cipher)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<R: Read> Read for AesReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let want = buf
            .len()
            .min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
        let Some(slice) = buf.get_mut(..want) else {
            return Ok(0);
        };
        let n = self.inner.read(slice)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let data = slice.get_mut(..n).unwrap_or_default();
        self.mac.update(data);
        for byte in data.iter_mut() {
            if self.used == 16 {
                self.keystream = self.cipher.encrypt(self.counter.to_le_bytes());
                self.counter = self.counter.wrapping_add(1);
                self.used = 0;
            }
            *byte ^= self.keystream.get(self.used).copied().unwrap_or(0);
            self.used += 1;
        }
        self.remaining -= n as u64;
        if self.remaining == 0 {
            let mut code = [0_u8; 10];
            self.inner.read_exact(&mut code)?;
            let expected = self.mac.finalize_reset().into_bytes();
            if expected.get(..10) != Some(&code[..]) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "the AES authentication code does not match",
                ));
            }
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_aes_writes_reads_back_with_its_password_only() {
        for strength in 1..=3 {
            let mut writer = aes_writer(Vec::new(), strength, b"secret").unwrap();
            writer.write_all(b"hello, aes").unwrap();
            let stored = writer.finish().unwrap();
            let info = AesInfo {
                vendor_version: 2,
                strength,
                method: 0,
            };
            let size = stored.len() as u64;
            assert_eq!(size, 10 + info.overhead());
            let mut plain = Vec::new();
            aes_reader(stored.as_slice(), size, info, b"secret")
                .unwrap()
                .read_to_end(&mut plain)
                .unwrap();
            assert_eq!(plain, b"hello, aes");
            assert!(matches!(
                aes_reader(stored.as_slice(), size, info, b"wrong"),
                Err(AesError::BadPassword)
            ));
        }
    }
}
