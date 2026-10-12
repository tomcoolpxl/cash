//! `chown` and `chgrp`, which change nothing and say so (decisions.md, as **D34**'s
//! `chmod -x`).
//!
//! Install scripts run `chown root:root f` and `chgrp staff f`. Windows sets a file's owner
//! only with privileges a normal user lacks (`SeRestorePrivilege`,
//! `SeTakeOwnershipPrivilege`) and has no POSIX groups, so both warn once that nothing was
//! changed and succeed, and the script carries on. A file that is not there is still an
//! error, as in GNU's. `enable -n chown` reaches a copy on `PATH`.

use std::io::Write;
use std::path::Path;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Change a file's owner or group: nothing, on Windows, and say so.
// `-h` is GNU's `--no-dereference`, so help is `--help` alone.
#[derive(Parser)]
#[clap(disable_help_flag = true)]
pub(crate) struct ChownCommand {
    /// Print help.
    #[arg(long = "help", action = clap::ArgAction::Help)]
    help: Option<bool>,

    /// Suppress the warning that nothing was changed.
    #[arg(short = 'f', long = "silent", alias = "quiet")]
    silent: bool,

    /// Say, for each file, that its ownership was kept.
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    /// Use this file's owner and group instead of naming them.
    #[arg(long = "reference", value_name = "RFILE")]
    reference: Option<String>,

    #[command(flatten)]
    unchanged: Unchanged,

    /// The owner (`chown`) or group (`chgrp`), then the files; only the files with
    /// `--reference`.
    #[arg(allow_hyphen_values = true)]
    args: Vec<String>,
}

/// GNU's options that choose what to change and how far: taken, so a script's line runs,
/// with nothing to act on when nothing changes.
#[derive(clap::Args)]
#[expect(
    dead_code,
    reason = "accepted for GNU's interface; nothing is changed to steer"
)]
struct Unchanged {
    /// Report only when a change is made: never, here.
    #[arg(short = 'c', long = "changes")]
    changes: bool,

    /// Operate on directories and their contents.
    #[arg(short = 'R', long = "recursive")]
    recursive: bool,

    /// Affect a symbolic link rather than what it points to.
    #[arg(short = 'h', long = "no-dereference")]
    no_dereference: bool,

    /// Affect what a symbolic link points to (the default).
    #[arg(long = "dereference")]
    dereference: bool,

    /// With -R, follow a symbolic link given on the command line.
    #[arg(short = 'H')]
    follow_command_line: bool,

    /// With -R, follow every symbolic link to a directory.
    #[arg(short = 'L')]
    follow_all: bool,

    /// With -R, follow no symbolic link (the default).
    #[arg(short = 'P')]
    follow_none: bool,

    /// Change only files whose owner and group match this.
    #[arg(long = "from", value_name = "CURRENT_OWNER:CURRENT_GROUP")]
    from: Option<String>,

    /// Refuse to operate recursively on `/`.
    #[arg(long = "preserve-root")]
    preserve_root: bool,

    /// Do not treat `/` specially (the default).
    #[arg(long = "no-preserve-root")]
    no_preserve_root: bool,
}

impl builtins::Command for ChownCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let name = context.command_name.clone();
        let files = if self.reference.is_some() {
            self.args.as_slice()
        } else {
            match self.args.split_first() {
                Some((_, files)) => files,
                None => &[],
            }
        };
        if self.args.is_empty() || files.is_empty() {
            match self.args.first().filter(|_| self.reference.is_none()) {
                Some(spec) => writeln!(context.stderr(), "{name}: missing operand after '{spec}'")?,
                None => writeln!(context.stderr(), "{name}: missing operand")?,
            }
            writeln!(
                context.stderr(),
                "Try '{name} --help' for more information."
            )?;
            return Ok(ExecutionResult::general_error());
        }

        if let Some(reference) = &self.reference {
            let path = context.shell.absolute_path(Path::new(reference));
            if let Err(e) = std::fs::metadata(&path) {
                let e = cash_core::error::os_error_text(&e);
                writeln!(
                    context.stderr(),
                    "{name}: failed to get attributes of '{reference}': {e}"
                )?;
                return Ok(ExecutionResult::general_error());
            }
        }

        // Said once for the whole invocation, as chmod says what it cannot store. A
        // Windows owner may change a file's access list but gets no access from being
        // owner, and its group is never consulted, so neither is set here; Windows' own
        // tool sets an owner for whoever wants one.
        if !self.silent {
            let why = if name == "chgrp" {
                "Windows has no POSIX groups; nothing changed"
            } else {
                "owners are not changed this way on Windows; nothing changed \
                 (sudo icacls FILE /setowner USER sets one)"
            };
            writeln!(context.stderr(), "{name}: {why}")?;
        }

        let mut failed = false;
        for file in files {
            let path = context.shell.absolute_path(Path::new(file));
            match std::fs::symlink_metadata(&path) {
                Err(e) => {
                    if !self.silent {
                        let e = cash_core::error::os_error_text(&e);
                        writeln!(context.stderr(), "{name}: cannot access '{file}': {e}")?;
                    }
                    failed = true;
                }
                Ok(_) if self.verbose => {
                    writeln!(context.stdout(), "ownership of '{file}' retained")?;
                }
                Ok(_) => {}
            }
        }

        if failed {
            return Ok(ExecutionResult::general_error());
        }
        Ok(ExecutionResult::success())
    }
}
