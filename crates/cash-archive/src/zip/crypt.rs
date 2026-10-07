//! PKWARE's traditional encryption, the only kind Info-ZIP's zip writes and unzip 6.00
//! reads: three keys stirred by each plain byte, and a 12-byte header whose last byte
//! checks the password.

use std::io::{self, Read, Write};

/// The CRC-32 table the keys are stirred with.
#[expect(clippy::cast_possible_truncation, reason = "n is below 256")]
const TABLE: [u32; 256] = {
    let mut table = [0_u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};

fn crc_byte(crc: u32, byte: u8) -> u32 {
    let index = usize::from((crc.to_le_bytes()[0]) ^ byte);
    (crc >> 8) ^ TABLE.get(index).copied().unwrap_or(0)
}

/// The three keys.
#[derive(Clone, Copy, Debug)]
pub struct Keys([u32; 3]);

impl Keys {
    /// The keys a password starts.
    pub fn new(password: &[u8]) -> Self {
        let mut keys = Self([0x1234_5678, 0x2345_6789, 0x3456_7890]);
        for b in password {
            keys.update(*b);
        }
        keys
    }

    fn update(&mut self, byte: u8) {
        let [k0, k1, k2] = &mut self.0;
        *k0 = crc_byte(*k0, byte);
        *k1 = k1
            .wrapping_add(*k0 & 0xff)
            .wrapping_mul(134_775_813)
            .wrapping_add(1);
        *k2 = crc_byte(*k2, k1.to_be_bytes()[0]);
    }

    const fn stream_byte(&self) -> u8 {
        let t = (self.0[2] | 2) & 0xffff;
        (t.wrapping_mul(t ^ 1) >> 8).to_le_bytes()[0]
    }

    /// One byte decrypted.
    pub fn decrypt(&mut self, byte: u8) -> u8 {
        let plain = byte ^ self.stream_byte();
        self.update(plain);
        plain
    }

    /// One byte encrypted.
    pub fn encrypt(&mut self, byte: u8) -> u8 {
        let cipher = byte ^ self.stream_byte();
        self.update(byte);
        cipher
    }
}

/// The 12-byte header, decrypted: whether its last byte is `check`, the byte the
/// password must give.
pub fn check_header(keys: &mut Keys, header: &[u8; 12], check: u8) -> bool {
    let mut last = 0;
    for b in header {
        last = keys.decrypt(*b);
    }
    last == check
}

/// The 12-byte header that starts the data, encrypted: ten bytes of `random`, then the
/// check byte twice as Info-ZIP writes it (the second is the one read).
pub fn make_header(keys: &mut Keys, random: [u8; 10], check: u8, check_low: u8) -> [u8; 12] {
    let mut plain = [0_u8; 12];
    for (slot, b) in plain.iter_mut().zip(random) {
        *slot = b;
    }
    plain[10] = check_low;
    plain[11] = check;
    plain.map(|b| keys.encrypt(b))
}

/// Encrypted data read as plain.
pub struct Decrypt<R> {
    inner: R,
    keys: Keys,
}

impl<R: Read> Decrypt<R> {
    /// `inner` after its header, with the keys the header left.
    pub const fn new(inner: R, keys: Keys) -> Self {
        Self { inner, keys }
    }
}

impl<R: Read> Read for Decrypt<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        for b in buf.iter_mut().take(n) {
            *b = self.keys.decrypt(*b);
        }
        Ok(n)
    }
}

/// Plain data written encrypted.
pub struct Encrypt<W> {
    inner: W,
    keys: Keys,
}

impl<W: Write> Encrypt<W> {
    /// `inner`, after the header was written with these keys.
    pub const fn new(inner: W, keys: Keys) -> Self {
        Self { inner, keys }
    }

    /// The writer underneath.
    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for Encrypt<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut keys = self.keys;
        let cipher: Vec<u8> = buf.iter().map(|b| keys.encrypt(*b)).collect();
        self.inner.write_all(&cipher)?;
        self.keys = keys;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_encrypted_decrypts_with_the_password_only() {
        let mut keys = Keys::new(b"secret");
        let header = make_header(&mut keys, [7; 10], 0xab, 0xcd);
        let mut writer = Encrypt::new(Vec::new(), keys);
        writer.write_all(b"hello").unwrap_or_default();
        let cipher = writer.into_inner();
        let mut keys = Keys::new(b"secret");
        assert!(check_header(&mut keys, &header, 0xab));
        let mut plain = Vec::new();
        Decrypt::new(cipher.as_slice(), keys)
            .read_to_end(&mut plain)
            .unwrap_or_default();
        assert_eq!(plain, b"hello");
        let mut wrong = Keys::new(b"wrong");
        assert!(!check_header(&mut wrong, &header, 0xab));
    }
}
