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

    // cash resolves the real name before it enters the directory, so one whose real name
    // cannot be found is not entered, and the status is 1 with `-e` or without it: it said
    // "not yet implemented". `-@` is refused as an invalid option, as in Git Bash: Windows
    // has no extended attributes to show as a directory.
    /// Exit with non zero exit status if current working directory resolution fails.
    #[arg(short = 'e')]
    _exit_on_failed_cwd_resolution: bool,

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
        let mut should_print = false;
        let mut target_dir = if let Some(target_dir) = &self.target_dir {
            // `cd -', equivalent to `cd $OLDPWD'
            if target_dir.as_os_str() == "-" {
                should_print = true;
                if let Some(oldpwd) = context.shell.env_str("OLDPWD") {
                    PathBuf::from(oldpwd.to_string())
                } else {
                    writeln!(context.error_stream(), "OLDPWD not set")?;
                    return Ok(ExecutionResult::general_error());
                }
            } else if let Some(root) = drive_root(target_dir) {
                // `cd C:` is the drive's root, as in Git Bash: Windows keeps a directory
                // for each drive, which cash does not, and `$PWD` was `C:`.
                root
            } else {
                target_dir.clone()
            }
        // `cd' without arguments is equivalent to `cd $HOME'
        } else {
            if let Some(home_var) = context.shell.env_str("HOME") {
                PathBuf::from(home_var.to_string())
            } else {
                writeln!(context.error_stream(), "HOME not set")?;
                return Ok(ExecutionResult::general_error());
            }
        };

        // CDPATH, as Bash searches it: a relative name not starting with `.` or `..` is
        // looked for in each entry first, an empty entry being the current directory; the
        // directory found through a non-empty entry is printed. It was not searched at all.
        if self
            .target_dir
            .as_ref()
            .is_some_and(|dir| dir.as_os_str() != "-")
        {
            if let Some(found) = search_cdpath(&context, &target_dir) {
                should_print = found.1;
                target_dir = found.0;
            }
        }

        // `shopt -s cdable_vars`: a name that is no directory is a variable's, holding the
        // directory to go to, which is printed, as Bash does. It had no effect.
        if context.shell.options().cdable_vars
            && !context.shell.absolute_path(&target_dir).is_dir()
            && let Some(dir) = self.target_dir.as_ref().and_then(|name| {
                let value = context.shell.env_str(&name.to_string_lossy())?;
                (!value.is_empty()).then(|| cash_win32::path::accept_path(&value))
            })
            && context.shell.absolute_path(&dir).is_dir()
        {
            target_dir = dir;
            should_print = true;
        }

        if self.use_physical_dir
            || context
                .shell
                .options()
                .do_not_resolve_symlinks_when_changing_dir
        {
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
            let shown = context.shell.shown_path(context.shell.working_dir());
            writeln!(context.stdout(), "{shown}")?;
        }

        Ok(ExecutionResult::success())
    }
}

/// The root of the drive `target` names when it is a drive alone, `C:` or `c:`.
fn drive_root(target: &std::path::Path) -> Option<PathBuf> {
    let text = target.to_str()?;
    let mut chars = text.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some(drive), Some(':'), None) if drive.is_ascii_alphabetic() => {
            Some(PathBuf::from(format!("{text}/")))
        }
        _ => None,
    }
}

/// The directory `target` names through `CDPATH`, and whether a non-empty entry gave it (so
/// `cd` prints it); `None` when `target` is absolute, starts with `.` or `..`, `CDPATH`
/// is unset or empty, or no entry holds it.
fn search_cdpath(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    target: &std::path::Path,
) -> Option<(PathBuf, bool)> {
    if target.has_root() || target.is_absolute() {
        return None;
    }
    let first = target.components().next()?;
    if matches!(
        first,
        std::path::Component::CurDir
            | std::path::Component::ParentDir
            | std::path::Component::Prefix(_)
    ) {
        return None;
    }
    let cdpath = context.shell.env_str("CDPATH")?;
    if cdpath.is_empty() {
        return None;
    }
    cash_win32::env::split_path_preserving_empty(&cdpath).find_map(|entry| {
        let base = if entry.is_empty() {
            context.shell.working_dir().to_path_buf()
        } else {
            context
                .shell
                .absolute_path(cash_win32::path::accept_path(entry))
        };
        let candidate = base.join(target);
        candidate.is_dir().then_some((candidate, !entry.is_empty()))
    })
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
    let reason = match (error.kind(), error.as_io_error()) {
        (error::ErrorKind::NotADirectory(_), _) => "Not a directory".to_owned(),
        (_, Some(io)) => error::os_error_text(io),
        _ => error.to_string(),
    };
    let spelled = target.to_string_lossy();
    writeln!(context.error_stream(), "cd: {spelled}: {reason}")?;
    if lost_its_backslashes(&spelled) {
        writeln!(
            context.error_stream(),
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
