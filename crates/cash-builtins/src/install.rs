//! `install` builtin — copy files and set attributes for Makefiles and build systems.

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

impl builtins::Command for InstallCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();

        // 1. Directory creation mode (-d)
        if self.directory {
            for dir in &self.operands {
                if let Err(e) = fs::create_dir_all(dir) {
                    writeln!(
                        context.stderr(),
                        "install: cannot create directory '{}': {}",
                        dir.display(),
                        e
                    )?;
                    return Ok(ExecutionResult::general_error());
                }
                if self.verbose {
                    writeln!(stdout, "install: creating directory '{}'", dir.display())?;
                }
            }
            return Ok(ExecutionResult::success());
        }

        // 2. Target directory mode (-t DIR)
        if let Some(target_dir) = &self.target_directory {
            if self.create_leading {
                let _ = fs::create_dir_all(target_dir);
            }
            if !target_dir.is_dir() {
                writeln!(
                    context.stderr(),
                    "install: target directory '{}' is not a directory",
                    target_dir.display()
                )?;
                return Ok(ExecutionResult::general_error());
            }

            for src in &self.operands {
                let Some(file_name) = src.file_name() else {
                    continue;
                };
                let dest = target_dir.join(file_name);
                if let Err(e) = self.copy_one(src, &dest, &context) {
                    writeln!(context.stderr(), "install: {e}")?;
                    return Ok(ExecutionResult::general_error());
                }
            }
            return Ok(ExecutionResult::success());
        }

        // 3. Positional operands
        if self.operands.len() < 2 {
            writeln!(
                context.stderr(),
                "install: missing destination file operand after '{}'",
                self.operands[0].display()
            )?;
            return Ok(ExecutionResult::general_error());
        }

        if self.operands.len() == 2 {
            let src = &self.operands[0];
            let dest_arg = &self.operands[1];

            let dest = if dest_arg.is_dir() {
                let file_name = src.file_name().unwrap_or(src.as_os_str());
                dest_arg.join(file_name)
            } else {
                if self.create_leading {
                    if let Some(parent) = dest_arg.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                }
                dest_arg.clone()
            };

            if let Err(e) = self.copy_one(src, &dest, &context) {
                writeln!(context.stderr(), "install: {e}")?;
                return Ok(ExecutionResult::general_error());
            }
            return Ok(ExecutionResult::success());
        }

        // More than 2 operands: last one MUST be an existing directory
        let (sources, dest_dir) = self.operands.split_at(self.operands.len() - 1);
        let target_dir = &dest_dir[0];

        if !target_dir.is_dir() {
            writeln!(
                context.stderr(),
                "install: target '{}' is not a directory",
                target_dir.display()
            )?;
            return Ok(ExecutionResult::general_error());
        }

        for src in sources {
            let Some(file_name) = src.file_name() else {
                continue;
            };
            let dest = target_dir.join(file_name);
            if let Err(e) = self.copy_one(src, &dest, &context) {
                writeln!(context.stderr(), "install: {e}")?;
                return Ok(ExecutionResult::general_error());
            }
        }

        Ok(ExecutionResult::success())
    }
}

impl InstallCommand {
    fn copy_one<SE: cash_core::ShellExtensions>(
        &self,
        src: &Path,
        dest: &Path,
        context: &cash_core::ExecutionContext<'_, SE>,
    ) -> std::io::Result<()> {
        if self.compare && dest.exists() {
            if let (Ok(src_bytes), Ok(dest_bytes)) = (fs::read(src), fs::read(dest)) {
                if src_bytes == dest_bytes {
                    return Ok(());
                }
            }
        }

        fs::copy(src, dest)?;

        if let Some(mode_str) = &self.mode {
            // Apply read-only mode if mode specifies non-writable (e.g. 444 or 555)
            if mode_str == "444" || mode_str == "0444" || mode_str == "555" || mode_str == "0555" {
                if let Ok(meta) = fs::metadata(dest) {
                    let mut perms = meta.permissions();
                    perms.set_readonly(true);
                    let _ = fs::set_permissions(dest, perms);
                }
            }
        }

        if self.verbose {
            writeln!(
                context.stdout(),
                "'{}' -> '{}'",
                src.display(),
                dest.display()
            )?;
        }

        Ok(())
    }
}
