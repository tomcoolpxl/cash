//! Archives and compressed streams for cash's `tar`, `zip` and compressors (spec D78).
//!
//! A library that knows no shell and prints nothing: it reads and writes, and says
//! what it found or what went wrong as facts and typed problems, which each tool's
//! front end in cash-builtins words as its original does.
//!
//! - [`codec`]: gzip, bzip2, xz, lzma, lzip and zstd streams, recognised by their magic
//!   or their suffix, read and written in pure Rust.

pub mod codec;
