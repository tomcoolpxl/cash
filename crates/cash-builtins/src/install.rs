//! `install` builtin — copy files and set attributes for Makefiles and build systems.
//!
//! Every operand is a path the shell resolves, against its own working directory (D10):
//! the process's is the folder cash was started in, so after `cd sub`, `install a b`
//! looked for `a` in the wrong place (`REVIEW_REPORT.md` BI-07). Messages name the
//! operands as they were written.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Copy files and set attributes.
#[derive(Parser)]
pub(crate) struct InstallCommand {
    /// Create all components of the specified directories.
    #[arg(short = 'd', long = "directory")]
    directory: bool,

    /// Create all leading components of DEST, then copy SOURCE to DEST.
    #[arg(short = 'D')]
    create_leading: bool,

    /// Set permission mode (octal or symbolic, e.g. 755 or 644).
    #[arg(short = 'm', long = "mode")]
    mode: Option<String>,

    /// Copy all SOURCE arguments into DIRECTORY.
    #[arg(short = 't', long = "target-directory")]
    target_directory: Option<PathBuf>,

    /// Print the name of each directory or file as it is created/copied.
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    /// Compare source and destination, and do not copy if identical.
    #[arg(short = 'C', long = "compare")]
    compare: bool,

    /// Strip symbol tables (accepted for Makefile compatibility; no-op on Windows).
    #[arg(short = 's', long = "strip")]
    strip: bool,

    /// Operands (sources and destination).
    #[arg(required = true)]
    operands: Vec<PathBuf>,
}

/// A path as it was written, and where the shell resolves it.
struct Operand<'a> {
    shown: &'a Path,
    actual: PathBuf,
}

impl<'a> Operand<'a> {
    fn new(path: &'a Path, shell: &cash_core::Shell<impl cash_core::ShellExtensions>) -> Self {
        Self {
            shown: path,
            actual: shell.absolute_path(path),
        }
    }
}

impl builtins::Command for InstallCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();
        let operands: Vec<Operand<'_>> = self
            .operands
            .iter()
            .map(|path| Operand::new(path, context.shell))
            .collect();

        // 1. Directory creation mode (-d)
        if self.directory {
            for dir in &operands {
                if let Err(e) = fs::create_dir_all(&dir.actual) {
                    writeln!(
                        context.stderr(),
                        "install: cannot create directory '{}': {}",
                        dir.shown.display(),
                        cash_core::error::os_error_text(&e)
                    )?;
                    return Ok(ExecutionResult::general_error());
                }
                if self.verbose {
                    writeln!(
                        stdout,
                        "install: creating directory '{}'",
                        dir.shown.display()
                    )?;
                }
            }
            return Ok(ExecutionResult::success());
        }

        // 2. Target directory mode (-t DIR)
        if let Some(target_dir) = &self.target_directory {
            let target = Operand::new(target_dir, context.shell);
            if self.create_leading {
                let _ = fs::create_dir_all(&target.actual);
            }
            if !target.actual.is_dir() {
                writeln!(
                    context.stderr(),
                    "install: target directory '{}' is not a directory",
                    target.shown.display()
                )?;
                return Ok(ExecutionResult::general_error());
            }

            return self.copy_into(&operands, &target, &context);
        }

        // 3. Positional operands
        let [sources @ .., last] = operands.as_slice() else {
            return Ok(ExecutionResult::general_error());
        };
        let Some(first) = sources.first() else {
            writeln!(
                context.stderr(),
                "install: missing destination file operand after '{}'",
                last.shown.display()
            )?;
            return Ok(ExecutionResult::general_error());
        };

        if sources.len() == 1 {
            let (shown, actual) = if last.actual.is_dir() {
                let file_name = first.shown.file_name().unwrap_or(first.shown.as_os_str());
                (last.shown.join(file_name), last.actual.join(file_name))
            } else {
                if self.create_leading
                    && let Some(parent) = last.actual.parent()
                {
                    let _ = fs::create_dir_all(parent);
                }
                (last.shown.to_path_buf(), last.actual.clone())
            };

            let dest = Operand {
                shown: &shown,
                actual,
            };
            if let Err(e) = self.copy_one(first, &dest, &context) {
                writeln!(context.stderr(), "install: {e}")?;
                return Ok(ExecutionResult::general_error());
            }
            return Ok(ExecutionResult::success());
        }

        // More than 2 operands: last one MUST be an existing directory
        if !last.actual.is_dir() {
            writeln!(
                context.stderr(),
                "install: target '{}' is not a directory",
                last.shown.display()
            )?;
            return Ok(ExecutionResult::general_error());
        }

        self.copy_into(sources, last, &context)
    }
}

impl InstallCommand {
    /// Copies each of `sources` into the folder `target`, under its own name.
    fn copy_into<SE: cash_core::ShellExtensions>(
        &self,
        sources: &[Operand<'_>],
        target: &Operand<'_>,
        context: &cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, cash_core::Error> {
        for src in sources {
            let Some(file_name) = src.shown.file_name() else {
                continue;
            };
            let dest = Operand {
                shown: &target.shown.join(file_name),
                actual: target.actual.join(file_name),
            };
            if let Err(e) = self.copy_one(src, &dest, context) {
                writeln!(context.stderr(), "install: {e}")?;
                return Ok(ExecutionResult::general_error());
            }
        }
        Ok(ExecutionResult::success())
    }

    /// Copies `src` to `dest`; an error names the file it is about.
    fn copy_one<SE: cash_core::ShellExtensions>(
        &self,
        src: &Operand<'_>,
        dest: &Operand<'_>,
        context: &cash_core::ExecutionContext<'_, SE>,
    ) -> std::io::Result<()> {
        if self.compare && dest.actual.exists() {
            if let (Ok(src_bytes), Ok(dest_bytes)) = (fs::read(&src.actual), fs::read(&dest.actual))
            {
                if src_bytes == dest_bytes {
                    return Ok(());
                }
            }
        }

        fs::copy(&src.actual, &dest.actual).map_err(|e| {
            // A source that is not there is one GNU install cannot `stat`.
            let message = if e.kind() == std::io::ErrorKind::NotFound && !src.actual.exists() {
                format!(
                    "cannot stat '{}': {}",
                    src.shown.display(),
                    cash_core::error::os_error_text(&e)
                )
            } else {
                format!(
                    "cannot install '{}' to '{}': {}",
                    src.shown.display(),
                    dest.shown.display(),
                    cash_core::error::os_error_text(&e)
                )
            };
            std::io::Error::new(e.kind(), message)
        })?;

        if let Some(mode_str) = &self.mode {
            // Apply read-only mode if mode specifies non-writable (e.g. 444 or 555)
            if mode_str == "444" || mode_str == "0444" || mode_str == "555" || mode_str == "0555" {
                if let Ok(meta) = fs::metadata(&dest.actual) {
                    let mut perms = meta.permissions();
                    perms.set_readonly(true);
                    let _ = fs::set_permissions(&dest.actual, perms);
                }
            }
        }

        if self.verbose {
            writeln!(
                context.stdout(),
                "'{}' -> '{}'",
                src.shown.display(),
                dest.shown.display()
            )?;
        }

        Ok(())
    }
}
