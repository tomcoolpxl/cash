//! `sync` — write what Windows holds in its cache to disk, as GNU's sync asks Linux to.
//!
//! uutils' `sync` flushed every volume for `sync` and `sync FILE` alike, and Windows lets
//! only an elevated process open a volume to flush it, so both failed with status 5 for
//! a normal user, ending a script under `set -e`; its `-d` did nothing. Here, as in GNU,
//! `sync FILE` flushes that file and `-d` its data; `sync` alone and `-f` flush the
//! drives Windows lets this process flush (every one, elevated) and skip the rest, whose
//! cache Windows writes back by itself within seconds.

use std::io::Write;
use std::path::{Component, Path, PathBuf, Prefix};

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Write cached data to disk.
#[derive(Parser)]
pub(crate) struct SyncCommand {
    /// Flush only each file's data, not its metadata; needs a FILE.
    #[arg(short = 'd', long = "data")]
    data: bool,

    /// Flush the drive each file is on.
    #[arg(short = 'f', long = "file-system")]
    file_system: bool,

    /// The files to flush: every drive without.
    files: Vec<PathBuf>,
}

impl builtins::Command for SyncCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.data && self.files.is_empty() {
            writeln!(context.stderr(), "sync: --data needs at least one argument")?;
            return Ok(ExecutionResult::general_error());
        }
        if self.files.is_empty() {
            for drive in cash_win32::sysinfo::fixed_drives() {
                flush_drive(&drive);
            }
            return Ok(ExecutionResult::success());
        }

        let mut result = ExecutionResult::success();
        for file in &self.files {
            let path = context.shell.absolute_path(file);
            let flushed = if self.file_system {
                std::fs::metadata(&path).map(|_| {
                    if let Some(drive) = drive_of(&path) {
                        flush_drive(&drive);
                    }
                })
            } else {
                flush_file(&path)
            };
            if let Err(e) = flushed {
                writeln!(
                    context.stderr(),
                    "sync: error opening '{}': {}",
                    file.display(),
                    cash_core::error::os_error_text(&e)
                )?;
                result = ExecutionResult::general_error();
            }
        }
        Ok(result)
    }
}

/// Flushes `path`'s cached data. Windows flushes only through a handle that may write,
/// so a file this process may only read is left to Windows, as nothing of it is ours to
/// flush; one that is not there is the error.
fn flush_file(path: &Path) -> std::io::Result<()> {
    match std::fs::OpenOptions::new().write(true).open(path) {
        Ok(file) => file.sync_all(),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied || path.is_dir() => {
            std::fs::metadata(path).map(|_| ())
        }
        Err(e) => Err(e),
    }
}

/// Flushes the drive `drive` names (`C:`), when Windows lets this process open it:
/// elevated, it does; otherwise nothing is done, and Windows writes it back itself.
fn flush_drive(drive: &str) {
    let volume = format!(r"\\.\{drive}");
    if let Ok(handle) = std::fs::OpenOptions::new().write(true).open(&volume) {
        let _ = handle.sync_all();
    }
}

/// The drive letter `path` is on, as `C:`; `None` for a share.
fn drive_of(path: &Path) -> Option<String> {
    match path.components().next()? {
        Component::Prefix(prefix) => match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                Some(format!("{}:", char::from(letter)))
            }
            _ => None,
        },
        _ => None,
    }
}
