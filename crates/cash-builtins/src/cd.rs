use std::io::Write;
use std::path::PathBuf;

use clap::Parser;

use cash_core::{ExecutionResult, builtins, error};

/// Change the current shell working directory.
#[derive(Parser)]
pub(crate) struct CdCommand {
    /// Force following symlinks.
    #[arg(short = 'L', overrides_with = "use_physical_dir")]
    force_follow_symlinks: bool,

    /// Use physical dir structure without following symlinks.
    #[arg(short = 'P', overrides_with = "force_follow_symlinks")]
    use_physical_dir: bool,

    /// Exit with non zero exit status if current working directory resolution fails.
    #[arg(short = 'e')]
    exit_on_failed_cwd_resolution: bool,

    /// Show file with extended attributes as a dir with extended
    /// attributes.
    #[arg(short = '@')]
    file_with_xattr_as_dir: bool,

    /// By default it is the value of the HOME shell variable. If `TARGET_DIR` is "-", it is
    /// converted to $OLDPWD.
    target_dir: Option<PathBuf>,
}

impl builtins::Command for CdCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        // TODO(cd): implement 'cd -@'
        if self.file_with_xattr_as_dir {
            return error::unimp("cd -@");
        }

        let mut should_print = false;
        let mut target_dir = if let Some(target_dir) = &self.target_dir {
            // `cd -', equivalent to `cd $OLDPWD'
            if target_dir.as_os_str() == "-" {
                should_print = true;
                if let Some(oldpwd) = context.shell.env_str("OLDPWD") {
                    PathBuf::from(oldpwd.to_string())
                } else {
                    writeln!(context.stderr(), "OLDPWD not set")?;
                    return Ok(ExecutionResult::general_error());
                }
            } else {
                // TODO(cd): remove clone, and use temporary lifetime extension after rust 1.75
                target_dir.clone()
            }
        // `cd' without arguments is equivalent to `cd $HOME'
        } else {
            if let Some(home_var) = context.shell.env_str("HOME") {
                PathBuf::from(home_var.to_string())
            } else {
                writeln!(context.stderr(), "HOME not set")?;
                return Ok(ExecutionResult::general_error());
            }
        };

        if self.use_physical_dir
            || context
                .shell
                .options()
                .do_not_resolve_symlinks_when_changing_dir
        {
            // -e is only relevant in physical mode.
            if self.exit_on_failed_cwd_resolution {
                return error::unimp("cd -e");
            }

            match context.shell.absolute_path(&target_dir).canonicalize() {
                Ok(resolved) => target_dir = resolved,
                Err(error) => return report_failure(&context, &target_dir, &error.into()),
            }
        }

        if let Err(error) = context.shell.set_working_dir(&target_dir) {
            return report_failure(&context, &target_dir, &error);
        }

        // Bash compatibility
        // https://www.gnu.org/software/bash/manual/bash.html#index-cd
        // If a non-empty directory name from CDPATH is used, or if '-' is the first argument, and
        // the directory change is successful, the absolute pathname of the new working
        // directory is written to the standard output.
        if should_print {
            writeln!(context.stdout(), "{}", target_dir.display())?;
        }

        Ok(ExecutionResult::success())
    }
}

/// Reports a failed change of directory as bash does, `cd: DIR: No such file or
/// directory`, naming the directory as the script spelled it. The generic error it
/// replaces, "i/o error: The system cannot find the file specified. (os error 2)",
/// named neither the path nor the reason.
fn report_failure(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    target: &std::path::Path,
    error: &error::Error,
) -> Result<ExecutionResult, error::Error> {
    let reason = match (error.kind(), error.as_io_error().map(std::io::Error::kind)) {
        (error::ErrorKind::NotADirectory(_), _) | (_, Some(std::io::ErrorKind::NotADirectory)) => {
            "Not a directory".to_owned()
        }
        (_, Some(std::io::ErrorKind::NotFound)) => "No such file or directory".to_owned(),
        (_, Some(std::io::ErrorKind::PermissionDenied)) => "Permission denied".to_owned(),
        _ => error.to_string(),
    };
    let spelled = target.to_string_lossy();
    writeln!(context.stderr(), "cd: {spelled}: {reason}")?;
    if lost_its_backslashes(&spelled) {
        writeln!(
            context.stderr(),
            "cd: hint: a backslash is an escape character, so an unquoted C:\\dir\\sub \
             arrives as C:dirsub; quote it ('C:\\dir\\sub'), use forward slashes \
             (C:/dir/sub), or turn on shopt -s winpaths"
        )?;
    }
    Ok(ExecutionResult::general_error())
}

/// Whether `path` looks like a pasted Windows path whose backslashes the lexer removed:
/// a drive letter and colon followed directly by a name, with no separator anywhere.
/// A drive-relative path is legal but rare, so it is only a hint, and only on failure.
fn lost_its_backslashes(path: &str) -> bool {
    let mut chars = path.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(drive), Some(':'), Some(first)) if drive.is_ascii_alphabetic() && first != '/' && first != '\\'
    ) && !path.contains(['/', '\\'])
}
