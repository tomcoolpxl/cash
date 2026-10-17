//! What builtins read the files they are named through (decisions.md, "Builtins share their
//! option parsing, input opening and folder walks"; `research/central-components-design.md`).
//!
//! A name is opened as the shell's own `<` opens it: against the shell's folder, which the
//! process's is not (D10); `/dev/null`, `/dev/tty`, `/dev/zero` and the random devices as
//! devices; `/dev/stdin`, `/dev/stdout`, `/dev/stderr` and `/dev/fd/N` as the descriptors
//! the shell has under those numbers. Before this each builtin opened a path itself, so
//! none could read `/dev/stdin` (the hook that serves the names lives only in bundled
//! tools' processes), and a folder read as "Permission denied", which is what Windows says
//! to a folder opened as a file.
//!
//! What is read goes through the stop token ([`crate::StopToken::reader`]): a Ctrl-C, or
//! `kill` of a background job, fails the read at the next block instead of ending it, so a
//! tool never takes half its input for the whole.
//!
//! `/dev/tcp` and `/dev/udp` stay the redirections' alone.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::commands::{ExecutionContext, StopReader};
use crate::extensions::ShellExtensions;
use crate::sys::fs::Access;

/// What `-` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dash {
    /// Standard input, as for most tools (`gzip -`, `grep x -`).
    Stdin,
    /// A file named `-`, as for util-linux's `rev`, `column` and `hexdump`.
    File,
}

/// What an [`Input`] reads.
#[derive(Debug)]
pub enum Source {
    /// A file, by its absolute path, with what its open handle says of it (nothing for a
    /// process substitution's pipe, which says nothing).
    File {
        /// Where the file is.
        path: PathBuf,
        /// What the open handle says of it.
        metadata: Option<std::fs::Metadata>,
    },
    /// Standard input, named `-`.
    Stdin,
    /// A device or a descriptor of the shell's: `/dev/null`, `/dev/tty`, `/dev/zero`,
    /// `/dev/stdin`, `/dev/fd/3`.
    Device,
}

/// A named input, open, read through the builtin's stop token.
pub struct Input {
    /// What it reads.
    pub source: Source,
    reader: StopReader<Box<dyn Read + Send>>,
}

impl std::fmt::Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Input")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

impl Input {
    /// What the open handle says of the file: its size, its times, its kind.
    pub const fn metadata(&self) -> Option<&std::fs::Metadata> {
        match &self.source {
            Source::File { metadata, .. } => metadata.as_ref(),
            Source::Stdin | Source::Device => None,
        }
    }

    /// Whether it is standard input, named `-`.
    pub const fn is_stdin(&self) -> bool {
        matches!(self.source, Source::Stdin)
    }

    /// Everything it holds, for the tools that read a whole file at once.
    ///
    /// # Errors
    ///
    /// When the read fails, or a stop fails it ([`crate::Stopped`]).
    pub fn read_all(mut self) -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

impl Read for Input {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }
}

/// Why a named input could not be opened.
#[derive(Debug)]
pub enum InputError {
    /// No such file, or a descriptor's name whose descriptor is not open.
    NotFound,
    /// The file may not be read.
    PermissionDenied,
    /// A folder, at this absolute path: a tool that walks folders goes on with it.
    IsADirectory(PathBuf),
    /// Anything else Windows said.
    Other(std::io::Error),
}

impl InputError {
    /// The C library's words for it, as GNU's tools print them after the name.
    pub fn reason(&self) -> String {
        match self {
            Self::NotFound => "No such file or directory".to_owned(),
            Self::PermissionDenied => "Permission denied".to_owned(),
            Self::IsADirectory(_) => "Is a directory".to_owned(),
            Self::Other(error) => crate::error::os_error_text(error),
        }
    }

    fn of(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::IsADirectory => Self::IsADirectory(PathBuf::new()),
            _ => Self::Other(error),
        }
    }
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason())
    }
}

impl std::error::Error for InputError {}

impl From<InputError> for std::io::Error {
    fn from(error: InputError) -> Self {
        match error {
            InputError::NotFound => Self::from(std::io::ErrorKind::NotFound),
            InputError::PermissionDenied => Self::from(std::io::ErrorKind::PermissionDenied),
            InputError::IsADirectory(_) => Self::from(std::io::ErrorKind::IsADirectory),
            InputError::Other(error) => error,
        }
    }
}

impl<SE: ShellExtensions> ExecutionContext<'_, SE> {
    /// Opens `name` for reading as the shell's `<` would, `-` as `dash` says, and reads it
    /// through the builtin's stop token.
    ///
    /// # Errors
    ///
    /// When the name cannot be opened: not found, not readable, a folder, or what else
    /// Windows says.
    pub fn open_input(&self, name: &str, dash: Dash) -> Result<Input, InputError> {
        let stop = self.stop_token();
        let input = |source, reader: Box<dyn Read + Send>| Input {
            source,
            reader: stop.reader(reader),
        };
        if name == "-" && dash == Dash::Stdin {
            return Ok(input(Source::Stdin, Box::new(self.stdin())));
        }
        if let Some(opened) = crate::sys::fs::try_open_special_file(Path::new(name), Access::Read) {
            let file = opened.map_err(InputError::of)?;
            return Ok(input(Source::Device, Box::new(file)));
        }
        let path = self.shell.absolute_path(Path::new(name));
        if let Some(fd) = crate::sys::fs::named_descriptor(&path) {
            let file = self.try_fd(fd).ok_or(InputError::NotFound)?;
            return Ok(input(Source::Device, Box::new(file)));
        }
        match std::fs::File::open(&path) {
            Ok(file) => {
                let metadata = file.metadata().ok();
                if metadata.as_ref().is_some_and(std::fs::Metadata::is_dir) {
                    return Err(InputError::IsADirectory(path));
                }
                Ok(input(Source::File { path, metadata }, Box::new(file)))
            }
            // Windows refuses a folder opened as a file with "Access is denied": one look
            // at what the name is tells the two apart, and only when the open failed.
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                if std::fs::metadata(&path).is_ok_and(|metadata| metadata.is_dir()) {
                    Err(InputError::IsADirectory(path))
                } else {
                    Err(InputError::PermissionDenied)
                }
            }
            Err(error) => Err(InputError::of(error)),
        }
    }
}
