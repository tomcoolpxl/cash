//! Which of the commands cash answers for itself are programs elsewhere.
//!
//! Bash's own builtins (`cd`, `read`, `echo`) change the shell running them, or are
//! answered by it, and have no program anywhere. Every other command cash carries (`ls`,
//! `sed`, `grep`, `rev`) is a program elsewhere, and cash gives it a path a script can run:
//! `which` its virtual path (`C:/…/cash.exe/ls`, D58), `type -P` the path Git Bash prints
//! (`/usr/bin/ls`), and `cash --link-tools` a hard link. They ask here, so that the three
//! agree on the set.

use cash_core::ShellExtensions;

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

/// Whether `name` is one of Bash's own builtins, which have no file and no path.
pub fn is_bash_builtin(name: &str) -> bool {
    BASH_BUILTINS.contains(&name)
}

/// The path a command cash carries that is no builtin of Bash's is run by: a hard link to
/// this cash on `PATH` for it (`cash --link-tools`), which programs outside cash can run
/// too, or else its virtual path (`C:/…/cash.exe/NAME`, D58). `which` prints it, and so
/// does `command -v`, as Bash's prints a program's path: `[ -x "$(command -v grep)" ]`
/// found the bare name and failed. `None` for one of Bash's own builtins.
pub fn carried_path<SE: ShellExtensions>(
    shell: &cash_core::Shell<SE>,
    name: &str,
) -> Option<std::path::PathBuf> {
    if is_bash_builtin(name) {
        return None;
    }
    if let Ok(exe) = std::env::current_exe() {
        // The first match that is this cash ends the search: every match of PATH was
        // gathered before any was compared. Each directory is looked at once
        // (`pathsearch`), so without a link the whole of PATH costs a look each.
        if let Some(link) = shell
            .find_executables_in_path(name)
            .find(|path| cash_win32::fs::same_file(path, &exe))
        {
            return Some(link);
        }
    }
    cash_win32::path::virtual_path(name).map(std::path::PathBuf::from)
}

/// Whether `name` is a command `shell` carries that is a program elsewhere: an enabled
/// builtin that is not one of Bash's own. Such a command has a path cash runs it by.
pub fn carries_program<SE: ShellExtensions>(shell: &cash_core::Shell<SE>, name: &str) -> bool {
    !is_bash_builtin(name) && shell.builtins().get(name).is_some_and(|b| !b.disabled)
}
