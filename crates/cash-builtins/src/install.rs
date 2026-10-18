//! `install` builtin — copy files and set attributes for Makefiles and build systems.
//!
//! Every operand is a path the shell resolves, against its own working directory (D10):
//! the process's is the directory cash was started in, so after `cd sub`, `install a b`
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

    /// Accepted and ignored, as GNU's install ignores it.
    #[arg(short = 'c', hide = true)]
    _copy: bool,

    /// Give each file its source's access and modification times, not the current time.
    #[arg(short = 'p', long = "preserve-timestamps")]
    preserve_timestamps: bool,

    /// Set the owner: Windows does not let it be set this way, so it is said once.
    #[arg(short = 'o', long = "owner")]
    owner: Option<String>,

    /// Set the group: Windows has no POSIX groups, so it is said once.
    #[arg(short = 'g', long = "group")]
    group: Option<String>,

    /// Keep a destination that is there as a backup, as --backup does.
    #[arg(short = 'b')]
    backup_short: bool,

    /// Make backups: none, numbered, existing or simple (existing without a word).
    #[arg(
        long = "backup",
        value_name = "CONTROL",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = ""
    )]
    backup: Option<String>,

    /// The suffix of a simple backup: `~` without; it makes backups by itself.
    #[arg(short = 'S', long = "suffix")]
    suffix: Option<String>,

    /// Treat DEST as a file, never as a directory to install into.
    #[arg(short = 'T', long = "no-target-directory")]
    no_target_directory: bool,

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

        let Some(plan) = self.prepare(&context)? else {
            return Ok(ExecutionResult::general_error());
        };

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

            return self.copy_into(&operands, &target, &plan, &context);
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

        if !self.file_target_fits(sources, last, &context)? {
            return Ok(ExecutionResult::general_error());
        }

        if sources.len() == 1 {
            let (shown, actual) = if last.actual.is_dir() && !self.no_target_directory {
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
            if let Err(e) = self.copy_one(first, &dest, &plan, &context) {
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

        self.copy_into(sources, last, &plan, &context)
    }
}

/// How a destination that is there is kept before it is replaced: `--backup`'s CONTROL.
#[derive(Clone, Copy)]
enum Backup {
    /// `FILE~`, or the suffix given.
    Simple,
    /// `FILE.~1~`, `FILE.~2~`, ...
    Numbered,
    /// Numbered when numbered backups are there already, else simple.
    Existing,
}

/// The backups to make, worked out once for the whole command.
struct Plan {
    backup: Option<Backup>,
    suffix: String,
}

impl Plan {
    /// The name `dest` is kept under, or `None` when no backup is made.
    fn backup_of(&self, dest: &Path) -> Option<PathBuf> {
        let backup = self.backup?;
        let numbered = || {
            let name = dest.file_name()?.to_string_lossy().into_owned();
            let prefix = format!("{name}.~");
            let highest = fs::read_dir(dest.parent()?)
                .ok()?
                .filter_map(|entry| {
                    let entry = entry.ok()?.file_name().to_string_lossy().into_owned();
                    entry
                        .strip_prefix(&prefix)?
                        .strip_suffix('~')?
                        .parse::<u64>()
                        .ok()
                })
                .max();
            Some((highest, name))
        };
        let simple = || {
            let mut name = dest.as_os_str().to_owned();
            name.push(&self.suffix);
            PathBuf::from(name)
        };
        match backup {
            Backup::Simple => Some(simple()),
            Backup::Numbered | Backup::Existing => match (numbered(), backup) {
                (Some((None, _)), Backup::Existing) | (None, _) => Some(simple()),
                (Some((highest, name)), _) => {
                    let next = highest.unwrap_or(0) + 1;
                    Some(dest.with_file_name(format!("{name}.~{next}~")))
                }
            },
        }
    }
}

impl InstallCommand {
    /// What is said once for the whole command, `-o` and `-g` as chown and chgrp say it,
    /// and the backups to make; `None` for a `--backup` word that is not one, said.
    fn prepare<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
    ) -> Result<Option<Plan>, cash_core::Error> {
        if self.owner.is_some() {
            writeln!(
                context.stderr(),
                "install: owners are not changed this way on Windows; the files are yours"
            )?;
        }
        if self.group.is_some() {
            writeln!(
                context.stderr(),
                "install: Windows has no POSIX groups; the group is not set"
            )?;
        }
        match self.plan(context) {
            Ok(plan) => Ok(Some(plan)),
            Err(word) => {
                writeln!(
                    context.stderr(),
                    "install: invalid argument '{word}' for '--backup'"
                )?;
                Ok(None)
            }
        }
    }

    /// Whether the operands suit `-T`, which makes DEST a file: one source, and a DEST
    /// that is not a directory. Without `-T`, they do.
    fn file_target_fits<SE: cash_core::ShellExtensions>(
        &self,
        sources: &[Operand<'_>],
        last: &Operand<'_>,
        context: &cash_core::ExecutionContext<'_, SE>,
    ) -> Result<bool, cash_core::Error> {
        if !self.no_target_directory {
            return Ok(true);
        }
        if sources.len() > 1 {
            writeln!(
                context.stderr(),
                "install: extra operand '{}'",
                last.shown.display()
            )?;
            return Ok(false);
        }
        if last.actual.is_dir() {
            writeln!(
                context.stderr(),
                "install: cannot overwrite directory '{}' with non-directory",
                last.shown.display()
            )?;
            return Ok(false);
        }
        Ok(true)
    }

    /// The backups `-b`, `--backup` and `-S` ask for, with `VERSION_CONTROL` and
    /// `SIMPLE_BACKUP_SUFFIX` read as GNU reads them; the word that is not a CONTROL.
    fn plan<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
    ) -> Result<Plan, String> {
        let variable = |name: &str| {
            context
                .shell
                .env()
                .get_str(name, context.shell)
                .map(|value| value.into_owned())
                .filter(|value| !value.is_empty())
        };
        let word = match &self.backup {
            Some(word) if !word.is_empty() => Some(word.clone()),
            Some(_) => Some(variable("VERSION_CONTROL").unwrap_or_default()),
            None if self.backup_short || self.suffix.is_some() => {
                Some(variable("VERSION_CONTROL").unwrap_or_default())
            }
            None => None,
        };
        let backup = match word.as_deref() {
            None | Some("none" | "off") => None,
            Some("numbered" | "t") => Some(Backup::Numbered),
            Some("" | "existing" | "nil") => Some(Backup::Existing),
            Some("simple" | "never") => Some(Backup::Simple),
            Some(other) => return Err(other.to_owned()),
        };
        let suffix = self
            .suffix
            .clone()
            .or_else(|| variable("SIMPLE_BACKUP_SUFFIX"))
            .unwrap_or_else(|| "~".to_owned());
        Ok(Plan { backup, suffix })
    }

    /// Copies each of `sources` into the directory `target`, under its own name.
    fn copy_into<SE: cash_core::ShellExtensions>(
        &self,
        sources: &[Operand<'_>],
        target: &Operand<'_>,
        plan: &Plan,
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
            if let Err(e) = self.copy_one(src, &dest, plan, context) {
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
        plan: &Plan,
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

        // A destination that is there is kept under its backup name first.
        if dest.actual.is_file()
            && let Some(backup) = plan.backup_of(&dest.actual)
        {
            fs::rename(&dest.actual, &backup).map_err(|e| {
                std::io::Error::new(
                    e.kind(),
                    format!(
                        "cannot backup '{}': {}",
                        dest.shown.display(),
                        cash_core::error::os_error_text(&e)
                    ),
                )
            })?;
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

        // Windows' copy keeps the source's modification time; GNU's install gives the
        // file the time it is made, or with -p the source's times.
        let times = if self.preserve_timestamps {
            fs::metadata(&src.actual).ok().and_then(|meta| {
                Some(
                    fs::FileTimes::new()
                        .set_accessed(meta.accessed().ok()?)
                        .set_modified(meta.modified().ok()?),
                )
            })
        } else {
            let now = std::time::SystemTime::now();
            Some(fs::FileTimes::new().set_accessed(now).set_modified(now))
        };
        if let Some(times) = times
            && let Ok(file) = fs::File::options().write(true).open(&dest.actual)
        {
            let _ = file.set_times(times);
        }

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
