//! What each method is written with: 7-Zip's `-m` parameters, as the writer takes them.

pub use lzma_rust2::{EncodeMode, LzmaOptions as LzmaParams, MfType};

use crate::sevenz::Password;

/// The settings of one coder in a block being written.
#[derive(Debug, Clone)]
pub enum EncoderOptions {
    /// LZMA: dictionary, literal and position bits, fast bytes, match finder, mode.
    Lzma(LzmaParams),
    /// LZMA2: LZMA's settings, and chunks compressed on `threads` threads when more
    /// than one.
    Lzma2 {
        /// LZMA's settings.
        params: LzmaParams,
        /// Threads to compress chunks on; one compresses the block as one chunk.
        threads: u32,
        /// The size of each chunk the threads take, at least the dictionary.
        chunk_size: u64,
    },
    /// `PPMd` (variant H): model order and memory.
    Ppmd {
        /// Model order, 2 to 32.
        order: u32,
        /// Model memory in bytes.
        memory: u32,
    },
    /// bzip2: the block size in 100 KB, 1 to 9.
    Bzip2 {
        /// 1 to 9.
        level: u32,
    },
    /// Deflate: the level, 0 to 9.
    Deflate {
        /// 0 to 9.
        level: u32,
    },
    /// Delta: the distance in bytes, 1 to 256.
    Delta {
        /// 1 to 256.
        distance: u32,
    },
    /// 7-Zip's AES-256.
    Aes(AesEncoderOptions),
}

/// 7-Zip's AES-256: a key from 2^`num_cycles_power` rounds of SHA-256 over the password,
/// CBC with a random IV.
#[derive(Debug, Clone)]
pub struct AesEncoderOptions {
    /// The password, as 7-Zip hashes it (UTF-16LE).
    pub password: Password,
    /// The initialization vector.
    pub iv: [u8; 16],
    /// The key derivation's rounds, as a power of two: 19 in 7-Zip.
    pub num_cycles_power: u8,
}

impl AesEncoderOptions {
    /// 7-Zip's: 2^19 rounds, a random IV, no salt.
    pub fn new(password: Password) -> Result<Self, getrandom::Error> {
        let mut iv = [0; 16];
        getrandom::fill(&mut iv)?;
        Ok(Self {
            password,
            iv,
            num_cycles_power: 19,
        })
    }

    /// The coder's properties: the rounds, no salt, the IV.
    pub(crate) fn properties(&self) -> Vec<u8> {
        let mut props = Vec::with_capacity(18);
        props.push(0x40 | (self.num_cycles_power & 0x3F));
        props.push(0x0F);
        props.extend_from_slice(&self.iv);
        props
    }
}
