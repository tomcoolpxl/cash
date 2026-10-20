//! Which of the commands cash answers for itself are programs elsewhere.
//!
//! Bash's own builtins (`cd`, `read`, `echo`) change the shell running them, or are
//! answered by it, and have no program anywhere. Every other command cash carries (`ls`,
//! `sed`, `grep`, `rev`) is a program elsewhere, and cash gives it a path a script can run:
//! `which` its virtual path (`C:/…/cash.exe/ls`, D58), `type -P` the path Git Bash prints
//! (`/usr/bin/ls`), `cash --link-tools` a hard link, and under `unixpaths` `/usr/bin` lists
//! them (D83). They ask here, so that all of them agree on the set.

use crate::ShellExtensions;

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

/// Whether `name` is a command `shell` carries that is a program elsewhere: an enabled
/// builtin that is not one of Bash's own. Such a command has a path cash runs it by.
pub fn carries_program<SE: ShellExtensions>(shell: &crate::Shell<SE>, name: &str) -> bool {
    !is_bash_builtin(name) && shell.builtins().get(name).is_some_and(|b| !b.disabled)
}

/// What `/usr/bin` holds under `unixpaths` (D83), each a name `/usr/bin/NAME` runs.
///
/// Every command `shell` carries that is a program elsewhere ([`carries_program`]), by
/// name, and `bash`, `cash` and `sh`, which run cash itself (D7).
pub fn carried_programs<SE: ShellExtensions>(shell: &crate::Shell<SE>) -> Vec<String> {
    let mut names: Vec<String> = shell
        .builtins()
        .iter()
        .filter(|(name, registration)| !registration.disabled && !is_bash_builtin(name))
        .map(|(name, _)| name.clone())
        .chain(["bash", "cash", "sh"].map(String::from))
        .collect();
    names.sort();
    names.dedup();
    names
}
