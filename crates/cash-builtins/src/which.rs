//! `which` — **D8**.
//!
//! D8 says cash owns command resolution. `which` did not: it resolved to the MSYS
//! `which.exe` from Git for Windows, which searches `PATH` and knows nothing about the
//! shell asking. So `which cat` answered `/usr/bin/cat` while cash ran its own builtin,
//! and `which ps` answered `/usr/bin/ps` while cash ran its own `ps`. A tool that
//! confidently reports the wrong thing is worse than one that is missing — it is the
//! first thing reached for when something behaves oddly, and it hands back a false lead.
//!
//! This one answers the question the user is actually asking: *what would this shell run
//! if I typed that?* It is the same lookup `type` performs, printed the way `which`
//! prints, so the `p=$(which git)` idiom keeps working.
//!
//! # Why a builtin shows a line rather than nothing
//!
//! GNU `which` is an external program, so a shell builtin is invisible to it and it
//! exits non-zero. zsh's `which` is a builtin and reports them. cash follows zsh,
//! because under cash the builtin *is* the answer — reporting "not found" for a command
//! that demonstrably runs would repeat the original sin in the opposite direction.
//!
//! For a command cash carries that is a program elsewhere (`ls`, `sed`, `ps`), the line
//! is a path a script can run: cash's own executable with the name appended,
//! `C:/…/cash.exe/ls`. No file is there, and cash runs the command it names, so
//! `LS=$(which ls); "$LS" -la` works (spec D58). bash's own builtins (`cd`, `read`) are no
//! program anywhere and still say `shell builtin`.
//!
//! A script wanting only a filesystem path should ask for one: `which -p` restricts the
//! search to `PATH`, and prints nothing for a builtin.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

use crate::lookup::{self, Resolved};

/// Bash 5.3's own builtins (`enable -a`). They change the shell running them, or only
/// make sense inside one, so there is no program to run by path; everything else cash
/// carries — `ls`, `sed`, `ps`, `rev` — is a program elsewhere, and `which` gives a path.
const BASH_BUILTINS: &[&str] = &[
    ".",
    ":",
    "[",
    "alias",
    "bg",
    "bind",
    "break",
    "builtin",
    "caller",
    "cd",
    "command",
    "compgen",
    "complete",
    "compopt",
    "continue",
    "declare",
    "dirs",
    "disown",
    "echo",
    "enable",
    "eval",
    "exec",
    "exit",
    "export",
    "false",
    "fc",
    "fg",
    "getopts",
    "hash",
    "help",
    "history",
    "jobs",
    "kill",
    "let",
    "local",
    "logout",
    "mapfile",
    "popd",
    "printf",
    "pushd",
    "pwd",
    "read",
    "readarray",
    "readonly",
    "return",
    "set",
    "shift",
    "shopt",
    "source",
    "suspend",
    "test",
    "times",
    "trap",
    "true",
    "type",
    "typeset",
    "ulimit",
    "umask",
    "unalias",
    "unset",
    "wait",
];

/// The path `which` prints for a command cash carries that is not one of bash's own
/// builtins: `C:/…/cash.exe/NAME`, which cash runs as that command (ROADMAP item 12), so
/// `LS=$(which ls); "$LS" -la` works as it does where `ls` is a file.
fn executable_path(name: &str) -> Option<String> {
    if is_bash_builtin(name) {
        return None;
    }
    cash_win32::path::virtual_path(name)
}

/// Whether `name` is one of Bash's own builtins, which have no file and no path, as
/// opposed to a command cash carries, which `which` gives a path and
/// `cash --link-tools` a link.
pub fn is_bash_builtin(name: &str) -> bool {
    BASH_BUILTINS.contains(&name)
}

/// A hard link to this cash on `PATH` for `name`, made by `cash --link-tools`: a real
/// file, which programs outside cash can run too, so `which` prefers it to the virtual
/// path.
fn linked_path<SE: cash_core::ShellExtensions>(
    shell: &cash_core::Shell<SE>,
    name: &str,
) -> Option<std::path::PathBuf> {
    if is_bash_builtin(name) {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let options = lookup::Options {
        force_path_search: true,
        suppress_func_lookup: true,
        all_locations: true,
        path_dirs: None,
    };
    lookup::resolve(shell, name, &options)
        .into_iter()
        .find_map(|how| match how {
            Resolved::File { path, .. } if cash_win32::fs::same_file(&path, &exe) => Some(path),
            _ => None,
        })
}

/// Report what the shell would run for a name.
#[derive(Parser)]
pub(crate) struct WhichCommand {
    /// Print every match, not just the first.
    #[arg(short = 'a', long = "all")]
    all: bool,

    /// Search only `PATH`, ignoring builtins, functions and aliases.
    #[arg(short = 'p', long = "path-only")]
    path_only: bool,

    /// Print nothing; report only through the exit status.
    #[arg(short = 's', long = "silent")]
    silent: bool,

    /// Names to look up.
    names: Vec<String>,
}

impl builtins::Command for WhichCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.names.is_empty() {
            writeln!(
                context.stderr(),
                "{}: usage: which NAME...",
                context.command_name
            )?;
            return Ok(ExecutionResult::from(
                cash_core::ExecutionExitCode::InvalidUsage,
            ));
        }

        let options = lookup::Options {
            force_path_search: self.path_only,
            suppress_func_lookup: self.path_only,
            all_locations: self.all,
            path_dirs: None,
        };

        let mut any_missing = false;

        for name in &self.names {
            let resolved = lookup::resolve(context.shell, name, &options);

            if resolved.is_empty() {
                any_missing = true;
                if !self.silent {
                    writeln!(
                        context.stderr(),
                        "{}: {name}: not found",
                        context.command_name
                    )?;
                }
                continue;
            }

            if self.silent {
                continue;
            }

            let mut stdout = context.stdout();
            // A link on PATH printed for the builtin is not printed again as a file.
            let mut printed_link = None;
            for how in &resolved {
                match how {
                    // A path alone, because that is what a script captures.
                    Resolved::File { path, .. } => {
                        if printed_link.as_ref() != Some(path) {
                            writeln!(stdout, "{}", cash_win32::path::render(path))?;
                        }
                    }
                    Resolved::Builtin => {
                        if let Some(link) = linked_path(context.shell, name) {
                            writeln!(stdout, "{}", cash_win32::path::render(&link))?;
                            printed_link = Some(link);
                        } else {
                            match executable_path(name) {
                                Some(path) => writeln!(stdout, "{path}")?,
                                None => writeln!(stdout, "{name}: shell builtin")?,
                            }
                        }
                    }
                    Resolved::Keyword => writeln!(stdout, "{name}: shell keyword")?,
                    Resolved::Function(_) => writeln!(stdout, "{name}: shell function")?,
                    Resolved::Alias(target) => writeln!(stdout, "{name}: aliased to {target}")?,
                }
            }
        }

        if any_missing {
            return Ok(ExecutionResult::general_error());
        }
        Ok(ExecutionResult::success())
    }
}
