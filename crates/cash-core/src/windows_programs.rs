//! `shopt winglob` — **D81**: a program of Windows' own globs for itself.
//!
//! cmd never expands a wildcard, so every command-line program Windows ships in
//! `System32` and `SysWOW64` reads `*` and `?` itself when it wants them (`xcopy`,
//! `findstr`, `taskkill`, OpenSSH's `scp`), or means something else by them: `net user
//! NAME *` asks for the password, and bash's globbing hands `net` the folder's file names
//! instead. With the option on, the words after such a program are not
//! pathname-expanded. A program anywhere else, `python` or `rg`, gets the shell's
//! globbing, which it needs. Scripts keep bash's globbing: the option is on at the
//! interactive prompt only, as `winpaths` is (D53).

use std::borrow::Cow;
use std::path::Path;

use crate::commands::CommandArg;
use crate::extensions;
use crate::shell::Shell;

/// Who expands the wildcards in the words of a command still to come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Globbing {
    /// Not known yet: a builtin that runs a command has not named it.
    Undecided,
    /// The shell, as in bash.
    Shell,
    /// The program, one of Windows' own.
    Program,
}

/// Who expands the wildcards in the words that follow `words`, the expanded words of a
/// simple command so far.
///
/// A builtin that runs the command among its words (`sudo`, `command`, `exec`, `env`,
/// `nice`, `nohup`, `timeout`; [`Registration::command_operand`]) is looked through to
/// it. A function, and any other builtin, is the shell's to serve.
///
/// [`Registration::command_operand`]: crate::builtins::Registration::command_operand
pub(crate) fn who_globs<SE: extensions::ShellExtensions>(
    shell: &mut Shell<SE>,
    words: &[CommandArg],
) -> Globbing {
    let mut index = 0;
    loop {
        let Some(word) = words.get(index) else {
            return Globbing::Undecided;
        };
        let CommandArg::String(name) = word else {
            return Globbing::Shell;
        };
        // A path to a bundled tool names the builtin, and never a function (ROADMAP item 12).
        let (name, functions) = match cash_win32::path::virtual_tool(name) {
            Some(tool) => (Cow::Owned(tool), false),
            None => (Cow::Borrowed(name.as_str()), true),
        };
        if functions && shell.funcs().get(&name).is_some() {
            return Globbing::Shell;
        }
        if let Some(builtin) = shell.builtins().get(name.as_ref())
            && !builtin.disabled
        {
            let Some(find) = builtin.command_operand else {
                return Globbing::Shell;
            };
            let rest: Vec<String> = words[index + 1..].iter().map(ToString::to_string).collect();
            let Some(at) = find(&rest) else {
                return Globbing::Undecided;
            };
            index += 1 + at;
            continue;
        }
        return if is_system_program(shell, &name) {
            Globbing::Program
        } else {
            Globbing::Shell
        };
    }
}

/// Whether `name`, a command's name as typed, runs a program of Windows' own
/// ([`cash_win32::fs::is_system_program`]): by its path, or found along PATH as the
/// shell will find it to run it, through the cache `hash` lists.
fn is_system_program<SE: extensions::ShellExtensions>(shell: &mut Shell<SE>, name: &str) -> bool {
    let path = if name.contains(['/', '\\']) {
        shell.absolute_path(Path::new(name))
    } else {
        let Some(found) = shell.find_first_executable_in_path_using_cache(name) else {
            return false;
        };
        found
    };
    cash_win32::fs::is_system_program(&path)
}
