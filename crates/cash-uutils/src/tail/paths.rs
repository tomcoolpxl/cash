// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore tailable stdlib (stdlib)

use crate::messages::translate;
use crate::tail::MESSAGES;
use crate::tail::text;
use std::ffi::OsStr;
use std::fs::{File, Metadata};
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};
use uucore::error::UResult;

#[derive(Debug, Clone)]
pub enum InputKind {
    File(PathBuf),
    Stdin,
}

impl From<&OsStr> for InputKind {
    fn from(value: &OsStr) -> Self {
        if value == OsStr::new(text::DASH) {
            Self::Stdin
        } else {
            Self::File(PathBuf::from(value))
        }
    }
}

#[derive(Debug, Clone)]
pub struct Input {
    kind: InputKind,
    pub display_name: String,
}

impl Input {
    pub fn from<T: AsRef<OsStr>>(string: T) -> Self {
        let string = string.as_ref();

        let kind = string.into();
        let display_name = match kind {
            InputKind::File(_) => string.to_string_lossy().to_string(),
            InputKind::Stdin => translate!("tail-stdin-header"),
        };

        Self { kind, display_name }
    }

    pub fn kind(&self) -> &InputKind {
        &self.kind
    }

    pub fn is_stdin(&self) -> bool {
        match self.kind {
            InputKind::File(_) => false,
            InputKind::Stdin => true,
        }
    }

    pub fn resolve(&self) -> Option<PathBuf> {
        match &self.kind {
            InputKind::File(path) if path != &PathBuf::from(text::DEV_STDIN) => {
                path.canonicalize().ok()
            }
            // cash: Windows has no path for a standard input, so it is read as a pipe
            // is, as on macOS; uutils looked for `/dev/fd/0`, which is not there.
            InputKind::File(_) | InputKind::Stdin => None,
        }
    }

    pub fn is_tailable(&self) -> bool {
        match &self.kind {
            InputKind::File(path) => path_is_tailable(path),
            InputKind::Stdin => self.resolve().is_some_and(|path| path_is_tailable(&path)),
        }
    }
}

impl Default for Input {
    fn default() -> Self {
        Self {
            kind: InputKind::Stdin,
            display_name: translate!("tail-stdin-header"),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HeaderPrinter {
    verbose: bool,
    first_header: bool,
}

impl HeaderPrinter {
    pub fn new(verbose: bool, first_header: bool) -> Self {
        Self {
            verbose,
            first_header,
        }
    }

    pub fn print_input(&mut self, input: &Input) {
        self.print(input.display_name.as_str());
    }

    pub fn print(&mut self, string: &str) {
        if self.verbose {
            println!(
                "{}==> {string} <==",
                if self.first_header { "" } else { "\n" },
            );
            self.first_header = false;
        }
    }
}
pub trait FileExtTail {
    #[allow(clippy::wrong_self_convention)]
    fn is_seekable(&mut self, current_offset: u64) -> bool;
}

impl FileExtTail for File {
    /// Test if File is seekable.
    /// Set the current position offset to `current_offset`.
    fn is_seekable(&mut self, current_offset: u64) -> bool {
        self.stream_position().is_ok()
            && self.seek(SeekFrom::End(0)).is_ok()
            && self.seek(SeekFrom::Start(current_offset)).is_ok()
    }
}

pub trait MetadataExtTail {
    fn is_tailable(&self) -> bool;
    fn got_truncated(&self, other: &Metadata) -> UResult<bool>;
}

impl MetadataExtTail for Metadata {
    fn is_tailable(&self) -> bool {
        self.file_type().is_file()
    }

    /// Return true if the file was modified and is now shorter
    fn got_truncated(&self, other: &Metadata) -> UResult<bool> {
        Ok(other.len() < self.len() && other.modified()? != self.modified()?)
    }
}

/// A file's identity: the serial number of its volume and its index there, as Unix
/// has a device and an inode.
pub type FileId = (u32, u64);

/// Which file `path` names now, or `None` when that cannot be told.
///
/// cash: `Metadata` carries no file index on Windows without an unstable API, so the
/// identity is kept beside it (`PathData::file_id`) and compared; uutils compared
/// none on Windows (`file_id_eq` answered "another file").
pub fn file_id(path: &Path) -> Option<FileId> {
    cash_win32::fs::file_info(path)
        .ok()
        .map(|info| (info.volume, info.index))
}

pub trait PathExtTail {
    fn is_stdin(&self) -> bool;
    fn has_active_parent(&self) -> bool;
    fn is_tailable(&self) -> bool;
}

impl PathExtTail for Path {
    fn is_stdin(&self) -> bool {
        self.eq(Self::new(text::DASH))
            || self.eq(Self::new(text::DEV_STDIN))
            || self.eq(Self::new(&translate!("tail-stdin-header")))
    }

    /// Return true if `path` has an existing parent directory
    fn has_active_parent(&self) -> bool {
        self.parent().is_some_and(Self::is_dir)
    }

    /// Return true if `path` is a file type that can be tailed
    fn is_tailable(&self) -> bool {
        path_is_tailable(self)
    }
}

pub fn path_is_tailable(path: &Path) -> bool {
    path.is_file() || path.exists() && path.metadata().is_ok_and(|meta| meta.is_tailable())
}

/// Whether the standard input was closed before the tool started. cash: Unix alone
/// tells, so this is `false`, as uutils has it on Windows.
#[inline]
pub fn stdin_is_bad_fd() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::file_id;

    #[test]
    fn file_id_tells_a_file_from_the_one_renamed_over_it() {
        // cash: with polling, every change to a followed file read as its replacement.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log");
        std::fs::write(&path, "a\n").unwrap();
        let first = file_id(&path);
        assert!(first.is_some());
        // Written again, it is the same file.
        std::fs::write(&path, "a\nb\n").unwrap();
        assert_eq!(file_id(&path), first);
        // Another file renamed over it is not, as a rotated log is not.
        let other = dir.path().join("new");
        std::fs::write(&other, "c\n").unwrap();
        std::fs::rename(&other, &path).unwrap();
        assert_ne!(file_id(&path), first);
    }
}
