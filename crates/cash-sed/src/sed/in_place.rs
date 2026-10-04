// Support for in-place editing
//
// SPDX-License-Identifier: MIT
// Copyright (c) 2025 Diomidis Spinellis
//
// This file is part of the uutils sed package.
// It is licensed under the MIT License.
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::ffi::OsStr;
use std::fs;
use std::io::stdout;
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;
use uucore::display::Quotable;
use uucore::error::{FromIo, UIoError, UResult, USimpleError};

use crate::sed::command::ProcessingContext;
use crate::sed::fast_io::OutputBuffer;

/// Context for in-place editing
pub struct InPlace {
    pub output: OutputBuffer,
    pub in_place: bool,
    pub in_place_suffix: Option<String>,
    pub follow_symlinks: bool,
    pub temp_file: Option<NamedTempFile>,
    pub original_path: Option<PathBuf>,
}

impl InPlace {
    /// Create an in-place editing engine based on ProcessingContext.
    /// Depending on its settings it may or may not perform in-place
    /// editing, backup the original file, or follow symlinks.
    pub fn new(context: ProcessingContext) -> Self {
        Self {
            output: OutputBuffer::new(Box::new(stdout())),
            in_place: context.in_place,
            in_place_suffix: context.in_place_suffix,
            follow_symlinks: context.follow_symlinks,
            temp_file: None,
            original_path: None,
        }
    }

    /// Return an OutputBuffer for outputting the edits to the specified file.
    /// The file may be a symbolic link, which will be processed according
    /// to the context specification.
    pub fn begin(&mut self, file_name: &Path) -> UResult<&mut OutputBuffer> {
        let resolved = if self.follow_symlinks {
            fs::canonicalize(file_name)
                .map_err_context(|| format!("resolving symlink {}", file_name.quote()))?
        } else {
            file_name.to_path_buf()
        };
        self.begin_resolved(&resolved)
    }

    /// Return an OutputBuffer for outputting the edits to the specified file.
    /// The passed file name should have resolved symbolic links according
    /// to the context settings.
    fn begin_resolved(&mut self, file_name: &Path) -> UResult<&mut OutputBuffer> {
        if !self.in_place {
            self.output = OutputBuffer::new(Box::new(stdout()));
            return Ok(&mut self.output);
        }

        let metadata = fs::metadata(file_name).map_err_context(|| {
            format!(
                "error Reading metadata of {} for in-place edit",
                file_name.quote()
            )
        })?;

        if !metadata.is_file() {
            return Err(USimpleError::new(
                2,
                format!(
                    "cannot in-place edit non-regular file {}",
                    file_name.quote()
                ),
            ));
        }

        let dir = file_name.parent().unwrap_or_else(|| Path::new("."));
        let temp_file = NamedTempFile::new_in(dir)
            .map_err_context(|| format!("error creating temporary file in {}", dir.quote()))?;

        let reopened = temp_file.reopen().map_err_context(|| {
            format!("couldn't open temporary file {}", temp_file.path().quote())
        })?;
        let output = OutputBuffer::new(Box::new(reopened));
        self.output = output;
        self.temp_file = Some(temp_file);
        self.original_path = Some(file_name.to_path_buf());

        Ok(&mut self.output)
    }

    /// Finish (potentially in-place) editing.
    pub fn end(&mut self) -> UResult<()> {
        self.output.flush()?;

        if !self.in_place {
            return Ok(());
        }

        // Both are set by `begin` of an in-place edit; without them no edit was begun,
        // and there is nothing to finish.
        let (Some(orig), Some(temp)) = (self.original_path.take(), self.temp_file.take()) else {
            return Ok(());
        };

        // Backup original if suffix is provided
        if let Some(ref suffix) = self.in_place_suffix {
            let mut backup_path = orig.clone();
            let Some(file_name) = backup_path.file_name().map(OsStr::to_os_string) else {
                return Err(USimpleError::new(
                    4,
                    format!("cannot back up {}: it has no file name", orig.quote()),
                ));
            };
            let mut backup_name = file_name;
            backup_name.push(suffix);
            backup_path.set_file_name(backup_name);

            // Try to remove to ensure the rename won't fail on Windows.
            let _ = fs::remove_file(&backup_path);

            fs::rename(&orig, &backup_path).map_err_context(|| {
                format!(
                    "error backing up {} to {}",
                    orig.quote(),
                    backup_path.quote()
                )
            })?;
        }

        // cash: the edited file replaces the original in one move (`persist` replaces an
        // existing file on Windows too), so there is no moment with neither. The original
        // used to be deleted first, and a failed move then dropped the temporary file as
        // well, losing both (`REVIEW_REPORT.md` TXT-15). A read-only original cannot be
        // replaced, so the attribute is lifted for the move and given to the new file, as
        // GNU sed gives it the original's mode.
        let read_only = fs::metadata(&orig).is_ok_and(|meta| meta.permissions().readonly());
        if read_only {
            set_read_only(&orig, false);
        }
        match temp.persist(&orig) {
            Ok(_) => {
                if read_only {
                    set_read_only(&orig, true);
                }
            }
            Err(e) => {
                if read_only {
                    set_read_only(&orig, true);
                }
                // Keep the edit rather than drop it with the error, and say where it is.
                let kept = e
                    .file
                    .keep()
                    .map_or_else(|_| PathBuf::new(), |(_, kept)| kept);
                return Err(UIoError::new(
                    e.error.kind(),
                    format!(
                        "error replacing {} with the edited file, which is kept at {}",
                        orig.quote(),
                        kept.quote()
                    ),
                ));
            }
        }

        Ok(())
    }
}

/// Sets or clears the read-only attribute of `path`, as far as it can.
fn set_read_only(path: &std::path::Path, read_only: bool) {
    if let Ok(meta) = fs::metadata(path) {
        let mut permissions = meta.permissions();
        permissions.set_readonly(read_only);
        let _ = fs::set_permissions(path, permissions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::TempDir;
    use assert_fs::fixture::PathChild;
    use std::fs;
    use std::io::{Read, Write};
    use std::path::Path;

    fn minimal_context() -> ProcessingContext {
        ProcessingContext {
            in_place: false,
            in_place_suffix: None,
            follow_symlinks: false,
            // fill in default values for the rest as needed
            ..Default::default()
        }
    }

    fn write_original(file: &Path, content: &str) {
        fs::write(file, content).unwrap();
    }

    fn read_file(file: &Path) -> String {
        let mut contents = String::new();
        fs::File::open(file)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        contents
    }

    #[test]
    fn test_in_place_editing() {
        let temp = TempDir::new().unwrap();
        let file = temp.child("file.txt");
        write_original(file.path(), "original\n");

        let mut ctx = minimal_context();
        ctx.in_place = true;

        let mut inplace = InPlace::new(ctx);
        let buf = inplace.begin(file.path()).unwrap();
        writeln!(buf, "updated").unwrap();
        inplace.end().unwrap();

        assert_eq!(read_file(file.path()), "updated\n");
    }

    #[test]
    fn test_in_place_backup() {
        let temp = TempDir::new().unwrap();
        let file = temp.child("file.txt");
        let backup = temp.child("file.txt.bak");
        write_original(file.path(), "original\n");

        let mut ctx = minimal_context();
        ctx.in_place = true;
        ctx.in_place_suffix = Some(".bak".to_string());

        let mut inplace = InPlace::new(ctx);
        let buf = inplace.begin(file.path()).unwrap();
        writeln!(buf, "new content").unwrap();
        inplace.end().unwrap();

        assert_eq!(read_file(file.path()), "new content\n");
        assert_eq!(read_file(backup.path()), "original\n");
    }

    #[test]
    fn test_no_in_place_outputs_to_stdout() {
        let mut ctx = minimal_context();
        ctx.in_place = false;

        let mut inplace = InPlace::new(ctx);
        let _buf = inplace.begin(Path::new("fake.txt")).unwrap();
        assert!(inplace.end().is_ok());
    }
}
