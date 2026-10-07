//! 7z archives, read and written, for cash's `7z` and `7za`.
//!
//! Taken from [sevenz-rust2](https://github.com/hasenbanck/sevenz-rust2) 0.23.0 (itself a
//! fork of sevenz-rust), © its authors, under the Apache License 2.0
//! (`licenses/sevenz-rust2-Apache-2.0.txt`); changed for cash from there on, as `NOTICE`
//! lists. Every file of this module came from it and has been modified.
//!
//! Codecs and filters, read and written unless noted: COPY, LZMA, LZMA2, bzip2, Deflate,
//! `PPMd`, AES-256 with SHA-256, the BCJ filters (x86, ARM, ARM Thumb, ARM64, RISC-V,
//! PowerPC, SPARC, IA-64), BCJ2 (read only) and Delta.

// Taken from sevenz-rust2, written to other rules: the style lints it was not written to
// are allowed rather than rewritten (as for cash-awk). The lints for code that can panic
// apply in full: what an archive can reach is an error, and what cannot fail says why.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::default_trait_access,
    clippy::too_many_lines,
    clippy::struct_field_names,
    clippy::branches_sharing_code,
    clippy::unreadable_literal,
    clippy::unused_self
)]

mod aes;
mod archive;
mod bitset;
mod block;
mod decoder;
mod encoder;
mod error;
/// Encoding options when compressing.
pub mod options;
mod password;
mod reader;
mod time;
mod writer;

use std::{
    io::Read,
    ops::{Deref, DerefMut},
};

pub(crate) use aes::*;
pub use archive::*;
pub use block::*;
pub use error::Error;
pub use password::Password;
pub use reader::{ArchiveReader, EntryReader, Problem};
pub use time::{NtTime, NtTimeError};
pub use writer::*;

trait ByteReader {
    fn read_u8(&mut self) -> std::io::Result<u8>;

    fn read_u32(&mut self) -> std::io::Result<u32>;

    fn read_u64(&mut self) -> std::io::Result<u64>;
}

impl<T: Read> ByteReader for T {
    #[inline]
    fn read_u8(&mut self) -> std::io::Result<u8> {
        let mut buf = [0; 1];
        self.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    #[inline]
    fn read_u32(&mut self) -> std::io::Result<u32> {
        let mut buf = [0; 4];
        self.read_exact(buf.as_mut())?;
        Ok(u32::from_le_bytes(buf))
    }

    #[inline]
    fn read_u64(&mut self) -> std::io::Result<u64> {
        let mut buf = [0; 8];
        self.read_exact(buf.as_mut())?;
        Ok(u64::from_le_bytes(buf))
    }
}

/// A trait for writers that finishes the stream on drop.
trait AutoFinish {
    /// Finish writing the stream without error handling.
    fn finish_ignore_error(self);
}

/// A wrapper around a writer that finishes the stream on drop.
#[expect(private_bounds, reason = "the finishing trait is the module's own")]
pub struct AutoFinisher<T: AutoFinish>(Option<T>);

impl<T: AutoFinish> Drop for AutoFinisher<T> {
    fn drop(&mut self) {
        if let Some(writer) = self.0.take() {
            writer.finish_ignore_error();
        }
    }
}

impl<T: AutoFinish> Deref for AutoFinisher<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match &self.0 {
            Some(writer) => writer,
            None => unreachable!("the writer is taken only on drop"),
        }
    }
}

impl<T: AutoFinish> DerefMut for AutoFinisher<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match &mut self.0 {
            Some(writer) => writer,
            None => unreachable!("the writer is taken only on drop"),
        }
    }
}

#[cfg(test)]
mod tests;
