//! Archives and compressed streams for cash's `tar`, `zip` and compressors (spec D78).
//!
//! A library that knows no shell and prints nothing: it reads and writes, and says
//! what it found or what went wrong as facts and typed problems, which each tool's
//! front end in cash-builtins words as its original does.
//!
//! - [`codec`]: gzip, bzip2, xz, lzma, lzip and zstd streams, recognised by their magic
//!   or their suffix, read and written in pure Rust.
//! - [`member`]: an archive entry as every format has it.
//! - [`tar`]: tar archives, read and written as GNU tar 1.35 reads and writes them.
//! - [`select`]: which members a name or a pattern selects, as `fnmatch` decides.
//! - [`listing`]: mode strings, and names quoted as GNU tar quotes them.
//! - [`zip`]: zip archives, read and written as Info-ZIP's zip 3.0 and unzip 6.00 do.
//! - [`sevenz`]: 7z archives, read and written as 7-Zip does.

pub mod codec;
pub mod listing;
pub mod member;
pub mod select;
pub mod sevenz;
pub mod tar;
pub mod zip;
