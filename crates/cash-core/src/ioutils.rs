//! Internal I/O utilities.

use crate::openfiles;

/// Error type for `FailingReaderWriter`: Bash's words for a closed descriptor, `echo
/// hi >&-` saying `echo: write error: Bad file descriptor`.
#[derive(Clone, Debug, thiserror::Error)]
enum ReaderWriterError {
    /// I/O read error.
    #[error("read error: Bad file descriptor")]
    Read,
    /// I/O write error, a flush's too.
    #[error("write error: Bad file descriptor")]
    Write,
}

/// Whether `error` is a [`FailingReaderWriter`]'s: its words are said as they are, not as
/// an `i/o error`.
pub(crate) fn is_closed_descriptor(error: &std::io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|inner| inner.is::<ReaderWriterError>())
}

/// An implementation of `std::io::Read` and `std::io::Write` that always fails.
///
/// Also implements `openfiles::Stream` for use with `openfiles::OpenFile`. Useful
/// for providing a valid stream that gracefully fails all operations in cases where
/// no real I/O is possible: a closed descriptor.
#[derive(Clone)]
pub(crate) struct FailingReaderWriter;

impl FailingReaderWriter {
    /// Creates a new `FailingReaderWriter`.
    pub const fn new() -> Self {
        Self
    }
}

impl openfiles::Stream for FailingReaderWriter {
    fn clone_box(&self) -> Box<dyn openfiles::Stream> {
        Box::new(self.clone())
    }
}

impl std::io::Read for FailingReaderWriter {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other(ReaderWriterError::Read))
    }
}

impl std::io::Write for FailingReaderWriter {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other(ReaderWriterError::Write))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::other(ReaderWriterError::Write))
    }
}

impl From<FailingReaderWriter> for openfiles::OpenFile {
    fn from(frw: FailingReaderWriter) -> Self {
        Self::Stream(Box::new(frw))
    }
}
