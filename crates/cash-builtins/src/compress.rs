//! What `gzip`, `bzip2`, `xz` and `zstd` share (D78): where a run's output goes, byte
//! counts for `-v`, the console's yes or no, and the error words.
//!
//! The four tools' flows stay their own: they check in different orders and say
//! different things (bzip2 names `-` a file and quotes a whole word in a bad flag, zstd
//! keeps its input unless asked), so a single engine would be all exceptions. What is
//! the same in all four lives here, and the codecs in cash-archive.

use std::fs;
use std::io::{self, Read, Write};

use cash_core::ShellFd;
use cash_core::openfiles::OpenFiles;
use cash_win32::unix::Replacement;

/// Where one file's output goes.
pub(crate) enum Output<'a> {
    /// Standard output, `-c`.
    Stdout(Box<dyn Write + 'a>),
    /// Nowhere: `-t` tests by decompressing into this.
    Sink,
    /// A file written beside its target and renamed over it at the end.
    File(Replacement),
}

impl Output<'_> {
    /// Finishes the output: a file gets `times` and the read-only bit and takes its
    /// name; standard output is flushed.
    pub(crate) fn finish(self, times: Option<fs::FileTimes>, read_only: bool) -> io::Result<()> {
        match self {
            Self::Stdout(mut out) => out.flush(),
            Self::Sink => Ok(()),
            Self::File(file) => file.finish(times.unwrap_or_default(), read_only, false),
        }
    }

    /// Gives the output up: a file being written leaves nothing behind.
    pub(crate) fn abandon(self) {
        if let Self::File(file) = self {
            file.abandon();
        }
    }
}

impl Write for Output<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Stdout(out) => out.write(buf),
            Self::Sink => Ok(buf.len()),
            Self::File(file) => file.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Stdout(out) => out.flush(),
            Self::Sink => Ok(()),
            Self::File(file) => file.flush(),
        }
    }
}

/// A reader or writer that counts the bytes through it.
pub(crate) struct Counted<T> {
    pub(crate) inner: T,
    pub(crate) count: u64,
}

impl<T> Counted<T> {
    pub(crate) const fn new(inner: T) -> Self {
        Self { inner, count: 0 }
    }
}

impl<R: Read> Read for Counted<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count += n as u64;
        Ok(n)
    }
}

impl<W: Write> Write for Counted<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.count += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// The words for an error, as the GNU tools' `strerror` gives them.
pub(crate) fn strerror(error: &io::Error) -> String {
    cash_core::error::os_error_text(error)
}

/// Whether the shell's descriptor `fd` is a console.
pub(crate) fn is_terminal<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    fd: ShellFd,
) -> bool {
    context.try_fd(fd).is_some_and(|f| f.is_terminal())
}

/// Reads one line of standard input, the console's as a line is typed there: whether
/// it starts with `y` or `Y`.
pub(crate) fn answer_is_yes<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> io::Result<bool> {
    let console = context
        .try_fd(OpenFiles::STDIN_FD)
        .and_then(|file| file.console(true, true));
    let line = if let Some(mut console) = console {
        match console.line()? {
            cash_win32::conin::Line::Typed(text) => text,
            cash_win32::conin::Line::EndOfInput | cash_win32::conin::Line::Interrupted => {
                String::new()
            }
        }
    } else {
        let mut stdin = context.stdin();
        let mut bytes = Vec::new();
        let mut byte = [0u8; 1];
        while stdin.read(&mut byte)? == 1 && byte[0] != b'\n' {
            bytes.push(byte[0]);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    };
    Ok(matches!(line.chars().next(), Some('y' | 'Y')))
}

/// A file's times, to give its output: the last change and the last read.
pub(crate) fn times_of(metadata: &fs::Metadata) -> fs::FileTimes {
    let mut times = fs::FileTimes::new();
    if let Ok(modified) = metadata.modified() {
        times = times.set_modified(modified);
    }
    if let Ok(accessed) = metadata.accessed() {
        times = times.set_accessed(accessed);
    }
    times
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_the_bytes_through() {
        let mut reader = Counted::new(&b"hello"[..]);
        let mut out = Counted::new(Vec::new());
        io::copy(&mut reader, &mut out).unwrap();
        assert_eq!((reader.count, out.count), (5, 5));
        let mut sink = Output::Sink;
        sink.write_all(b"x").unwrap();
        sink.finish(None, false).unwrap();
    }
}
