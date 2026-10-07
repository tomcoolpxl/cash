//! tar archives, read and written as GNU tar 1.35 reads and writes them.
//!
//! The headers are cash's own, not a crate's: with the same options, the blocks cash
//! writes are GNU tar's byte for byte, down to the checksum's six digits and the
//! padding of the last record. [`read::Reader`] reads block by block with GNU's view of
//! damage; [`write::Writer`] writes a member's headers (a `././@LongLink` member, a
//! ustar prefix or a pax header for a long name) and its data.

pub mod header;
pub mod read;
pub mod write;

pub use header::{BLOCK, Format};
