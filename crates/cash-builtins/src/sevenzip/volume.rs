//! Where an archive's bytes are for 7z: one file, or the volumes of a split archive
//! (`name.001`, `name.002` and on, as 7-Zip's split handler reads them), read as one
//! through cash-archive's [`volumes`].

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use cash_archive::volumes;

/// An archive's bytes, its parts read as one file.
pub(super) type Source = volumes::Spanned<File>;

/// Where an archive's bytes are: one file, or a split archive's volumes, in order.
#[derive(Clone, Debug)]
pub(super) struct Location {
    parts: Vec<PathBuf>,
}

impl Location {
    /// One file.
    pub(super) fn single(path: &Path) -> Self {
        Self {
            parts: vec![path.to_path_buf()],
        }
    }

    /// `name.001` and the volumes after it that are there.
    pub(super) fn volumes(first: &Path) -> Self {
        Self {
            parts: volumes::numbered(first),
        }
    }

    /// The parts, in order.
    pub(super) fn parts(&self) -> &[PathBuf] {
        &self.parts
    }

    /// The parts, open, read as one file.
    pub(super) fn open(&self) -> io::Result<Source> {
        volumes::open(&self.parts)
    }

    /// Each part's size.
    pub(super) fn sizes(&self) -> Vec<u64> {
        self.parts
            .iter()
            .map(|p| p.metadata().map_or(0, |m| m.len()))
            .collect()
    }
}
