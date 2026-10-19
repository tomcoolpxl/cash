// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// cash: Windows only; its writer runs `--filter` too (`windows::FilterWriter`).
pub use self::windows::{FilterWriter, instantiate_current_writer, paths_refer_to_same_file};

mod windows;

// todo: add .as_fd for std::io::copy's specialization for --bytes
pub enum Writer {
    File(std::fs::File),
    Filter(FilterWriter),
}

impl std::io::Write for Writer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::File(w) => w.write(buf),
            Self::Filter(w) => w.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::File(w) => w.flush(),
            Self::Filter(w) => w.flush(),
        }
    }
}
