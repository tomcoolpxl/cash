//! Standard builtins.

// `cash_core::builtins::Command::execute` is async by contract: the trait declares a desugared
// `-> impl Future<...> + Send` so that `cash_core::builtins::exec_builtin` can box and dispatch
// every builtin uniformly. Most builtins in this crate do purely synchronous work, so their
// `execute` bodies contain no `.await` -- that is the trait contract being honored, not a defect.
#![expect(
    clippy::unused_async_trait_impl,
    reason = "builtins implement a trait whose `execute` is async by contract"
)]

// cash (D45): Windows-specific builtins — winpath, start, elevate, detach.
mod carried;
mod win;
// cash: `xdg-open`, `start` under the name cross-platform scripts try first.
mod xdg_open;
// cash: `pbcopy` and `pbpaste`, macOS's names for the clipboard, on Windows'.
mod pbcopy;
// cash: `uuidgen`, util-linux's, checked against 2.42.3 case by case.
mod uuidgen;

// cash (D60): fish's abbreviations.
mod abbr;
// cash (D62): fish's directory history — prevd, nextd, cdh.
mod dirhistory;
// cash (D80): `z`, zoxide's directory jump.
mod z;

// cash (D48): `ps`, which uutils does not carry and whose PATH stand-in reports MSYS
// pids that `kill` cannot use.
// cash: `bzip2`, `bunzip2` and `bzcat`, bzip2 1.0.8's interface on libbz2-rs-sys; a clean
// Windows machine has none.
mod bzip2;
// cash: `column`, util-linux's, checked against 2.42.3 case by case.
mod column;
// cash: `logger`, util-linux's, writing to the Application event log.
mod logger;
// cash: what the compressors share (D78): where output goes, byte counts, the console.
mod compress;
mod croot;
// cash: `man`, its own manual's browser, on `cash-man`'s pages.
mod fileuse;
mod man;
// cash: `flock`, util-linux's, on LockFileEx; the lock is one byte far past the content.
mod flock;
// cash: `free`, procps-ng's, on Windows' memory counters; Windows has no `free`.
mod free;
mod fuser;
mod getopt;
// cash: `grep`, `egrep` and `fgrep`, GNU grep 3.12's interface on ripgrep's engine; the
// most common failure of a script on a bare Windows machine.
mod grep;
// cash: `gzip`, `gunzip` and `zcat`, GNU gzip 1.14's interface on miniz_oxide's deflate; a
// clean Windows machine has none, and `tar.exe` reads archives, not a bare `.gz`.
mod gzip;
// cash: `hexdump`, util-linux's, with its format language; a clean Windows machine has none.
mod hexdump;
// cash: `iconv`, glibc's, on Windows' code pages; a clean Windows machine has none.
mod iconv;
mod killfam;
mod lsof;
mod names;
// cash: `nc`, OpenBSD netcat's, on Windows sockets; a clean Windows machine has none.
mod nc;
// cash: `nice` and `renice` on Windows' six priority classes; Windows has neither.
mod nice;
// cash: what the builtins that read GNU's options share beside cash-getopt.
mod options;
mod pgrep;
pub mod ping;
mod procmatch;
mod ps;
mod pstree;
mod rev;
mod screen;
mod ss;
// cash: `tar`, GNU tar 1.35's interface on cash-archive; Windows' own `tar.exe` is bsdtar,
// with other options and other words.
mod sevenzip;
// cash: `rar` and `unrar`, WinRAR 7.23's console interfaces on cash-archive's RAR, which
// Windows does not have.
mod rar;
// RAR's packed data read and decoded, for `7z` and `rar` both.
mod rardata;
mod tar;
// cash: `zip`, `unzip` and `zipinfo`, Info-ZIP's interfaces on cash-archive: Windows has
// none of them, and Explorer's zip directories are not scriptable.
mod zip;
// cash: `tput`, ncurses' for xterm-256color without terminfo: the terminal is VT.
mod tput;
// cash: `getconf`, glibc's names with the values Windows has.
mod getconf;
// cash: `locale`, glibc's view of the environment, and the Windows locales by POSIX name.
mod locale;
// cash: `mkfifo`, refused with the way to a pipe in cash: a named pipe is no file here.
mod mkfifo;
// cash: `stdbuf`, which runs the command as it is: no preload on Windows.
mod stdbuf;
// cash: `suspend`, Bash's, refused in its own words: Windows has no stop signal.
mod suspend;
// cash: `stty`, GNU's words on the console's four modes; the rest is remembered.
mod stty;
mod tree;
// cash: the directory walks the builtins share, on walkdir.
mod walk;
// cash: `xxd`, vim's, dump and reverse; Windows has no hex dump a script can read back.
mod xxd;

// cash (D48, §4 #20): `hostname`, so the machine has one name inside the shell rather
// than the DNS API's and Windows' own.
mod hostname;

// cash (D48, D3, D32): `find` and `xargs`. What answers `find` on a clean Windows
// machine is a DOS tool that searches inside files, and there is no `xargs` at all.
mod find;
mod xargs;

// cash: `coolfetch`, the banner. Every number is one cash already holds, so it spawns
// nothing — which is the whole difference from the tools that print this elsewhere.
mod coolfetch;

// cash (D48): `top`, which Windows has no equivalent of at all — procps was never
// ported, and the Cygwin build reports pids `kill` cannot use.
mod top;
// cash: `watch`, procps-ng's, which Windows and Git for Windows both lack.
mod watch;
// cash: `xz`, `unxz`, `xzcat`, `lzma`, `unlzma` and `lzcat`, XZ Utils 5.8's interface on
// lzma-rust2; a clean Windows machine has none.
mod xz;
// cash: `zstd`, `unzstd` and `zstdcat`, zstd 1.5.7's interface on ruzstd; a clean Windows
// machine has none.
mod zstd;

// cash (D48): one pager behind both `less` and `more`. `less` is not coreutils, and the
// bundled `more` corrupted a pipe.
mod pager;

// cash (D8): `which` must answer about cash's resolution, not about PATH.
mod which;

// cash (D66): `where`, Windows' where.exe with dashes for options and cash's paths.
mod where_files;

// cash (D23, D34): uutils' chmod is Unix-only, so the gap was filled by MSYS's, which
// writes mode bits nothing outside MSYS reads.
mod chmod;

// cash: Windows sets an owner only with privileges a user lacks and has no POSIX groups,
// so `chown` and `chgrp` warn that nothing changed and succeed, as `chmod -x` does.
mod chown;

// cash (D48): uutils' `id` is Unix-only, and the MSYS stand-in reports a uid Windows
// does not have.
mod identity;
mod logins;

// Native Windows-optimized pure-Rust `ls` builtin.
mod ls;
// cash (D67): lsd's Nerd Font icons, for `ls --icons`.
mod ls_icon_table;

mod tty;

// `sync` as GNU means it: a file flushed by itself, and only the drives Windows lets
// this process flush.
mod sync;

mod stat;

mod nohup;
mod time_command;

mod install;

// cash (D20, D48): `dos2unix`/`unix2dos`, which a clean Windows machine does not have and
// Git for Windows only supplies when its `usr/bin` is on PATH.
mod dos2unix;

#[cfg(feature = "builtin.alias")]
mod alias;
#[cfg(feature = "builtin.bg")]
mod bg;
#[cfg(feature = "builtin.bind")]
mod bind;
#[cfg(feature = "builtin.break")]
mod break_;
#[cfg(feature = "builtin.builtin")]
mod builtin_;
#[cfg(feature = "builtin.caller")]
mod caller;
#[cfg(feature = "builtin.cd")]
mod cd;
#[cfg(feature = "builtin.colon")]
mod colon;
#[cfg(feature = "builtin.command")]
mod command;
#[cfg(any(
    feature = "builtin.complete",
    feature = "builtin.compgen",
    feature = "builtin.compopt"
))]
mod complete;
#[cfg(feature = "builtin.continue")]
mod continue_;
#[cfg(feature = "builtin.declare")]
mod declare;
#[cfg(feature = "builtin.dirs")]
mod dirs;
#[cfg(feature = "builtin.disown")]
mod disown;
#[cfg(feature = "builtin.dot")]
mod dot;
#[cfg(feature = "builtin.echo")]
mod echo;
#[cfg(feature = "builtin.enable")]
mod enable;
#[cfg(feature = "builtin.eval")]
mod eval;
#[cfg(feature = "builtin.exec")]
mod exec;
#[cfg(feature = "builtin.exit")]
mod exit;
#[cfg(feature = "builtin.export")]
mod export;
#[cfg(feature = "builtin.false")]
mod false_;
#[cfg(feature = "builtin.fc")]
mod fc;
#[cfg(feature = "builtin.fg")]
mod fg;
#[cfg(feature = "builtin.getopts")]
mod getopts;
#[cfg(feature = "builtin.hash")]
mod hash;
#[cfg(feature = "builtin.help")]
mod help;
// cash: what `help` says, one entry for every builtin, with pages and topics. Public so
// the tests can hold it against the builtins the shell registers.
pub mod helpdocs;
#[cfg(feature = "builtin.history")]
mod history;
#[cfg(feature = "builtin.jobs")]
mod jobs;
#[cfg(feature = "builtin.kill")]
mod kill;
#[cfg(feature = "builtin.let")]
mod let_;
#[cfg(feature = "builtin.exit")]
mod logout;
#[cfg(feature = "builtin.mapfile")]
mod mapfile;
#[cfg(feature = "builtin.popd")]
mod popd;
#[cfg(feature = "builtin.printf")]
mod printf;
#[cfg(feature = "builtin.pushd")]
mod pushd;
#[cfg(feature = "builtin.pwd")]
mod pwd;
#[cfg(feature = "builtin.read")]
mod read;
#[cfg(feature = "builtin.return")]
mod return_;
#[cfg(feature = "builtin.set")]
mod set;
#[cfg(feature = "builtin.shift")]
mod shift;
#[cfg(feature = "builtin.shopt")]
mod shopt;
#[cfg(feature = "builtin.test")]
mod test;
#[cfg(feature = "builtin.times")]
mod times;
#[cfg(feature = "builtin.trap")]
mod trap;
#[cfg(feature = "builtin.true")]
mod true_;
#[cfg(feature = "builtin.type")]
mod type_;
// cash: Windows has no rlimits, so `ulimit` is a deliberately small builtin of its own.
#[cfg(feature = "builtin.ulimit")]
mod ulimit_win;
#[cfg(feature = "builtin.umask")]
mod umask;
#[cfg(feature = "builtin.unalias")]
mod unalias;
#[cfg(feature = "builtin.unset")]
mod unset;
#[cfg(feature = "builtin.wait")]
mod wait;

mod builder;
mod factory;
#[cfg(any(feature = "builtin.command", feature = "builtin.type"))]
mod lookup;
mod unimp;

pub use builder::ShellBuilderExt;
pub use carried::{carries_program, is_bash_builtin};
pub use factory::{BuiltinSet, default_builtins, is_default_builtin};

/// What `break` or `continue` does with its count, `word`, as Bash's builtins take it:
/// outside a loop nothing (it says so, but in POSIX mode); a count past the loops there
/// are leaves them all; a count below 1 is out of range, leaves every loop, and is status
/// 1; a word that is no number ends a script with status 2. `flow` makes the control flow
/// that leaves that many loops but the last. Outside a loop they ended the script without a
/// word, and `break 2` in one loop did too.
#[cfg(any(feature = "builtin.break", feature = "builtin.continue"))]
fn leave_loops<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    word: Option<&str>,
    flow: fn(usize) -> cash_core::ExecutionControlFlow,
) -> Result<cash_core::ExecutionResult, cash_core::Error> {
    use cash_core::{ExecutionControlFlow, ExecutionResult};
    use std::io::Write as _;

    let depth = context.shell.loop_depth();
    if depth == 0 {
        if !context.shell.options().posix_mode {
            writeln!(
                context.error_stream(),
                "{}: only meaningful in a `for', `while', or `until' loop",
                context.command_name
            )?;
        }
        return Ok(ExecutionResult::success());
    }
    let count = match word {
        None => 1,
        Some(word) => {
            let Ok(count) = word.trim().parse::<i64>() else {
                writeln!(
                    context.error_stream(),
                    "{}: {word}: numeric argument required",
                    context.command_name
                )?;
                let mut result = ExecutionResult::new(2);
                if !context.shell.options().interactive {
                    result.next_control_flow = ExecutionControlFlow::ExitShell;
                }
                return Ok(result);
            };
            if count < 1 {
                writeln!(
                    context.error_stream(),
                    "{}: {word}: loop count out of range",
                    context.command_name
                )?;
                let mut result = ExecutionResult::general_error();
                result.next_control_flow = ExecutionControlFlow::BreakLoop { levels: depth - 1 };
                return Ok(result);
            }
            count
        }
    };
    let levels = usize::try_from(count).unwrap_or(usize::MAX).min(depth) - 1;
    let mut result = ExecutionResult::success();
    result.next_control_flow = flow(levels);
    Ok(result)
}

/// The status `return` or `exit` is given in `words`, as Bash's `get_exitstat` reads them:
/// a first `--` passed over; none, `None`; a number, its low eight bits (`return -1` is
/// 255); a word that is no number is said, and 2. A second word is too many: the result
/// that ends a script with status 1 comes back as the error. Clap refused `-1` as an
/// option and `abc`, and the function went on.
#[cfg(any(feature = "builtin.return", feature = "builtin.exit"))]
fn exit_status_word<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    words: &[String],
) -> Result<Result<Option<u8>, cash_core::ExecutionResult>, cash_core::Error> {
    use std::io::Write as _;

    let words = match words.split_first() {
        Some((first, rest)) if first == "--" => rest,
        _ => words,
    };
    let Some((word, rest)) = words.split_first() else {
        return Ok(Ok(None));
    };
    if !rest.is_empty() {
        writeln!(
            context.error_stream(),
            "{}: too many arguments",
            context.command_name
        )?;
        let mut result = cash_core::ExecutionResult::general_error();
        if !context.shell.options().interactive {
            result.next_control_flow = cash_core::ExecutionControlFlow::ExitShell;
        }
        return Ok(Err(result));
    }
    let Ok(number) = word.trim().parse::<i64>() else {
        writeln!(
            context.error_stream(),
            "{}: {word}: numeric argument required",
            context.command_name
        )?;
        return Ok(Ok(Some(2)));
    };
    Ok(Ok(Some(
        u8::try_from(number.rem_euclid(256)).unwrap_or_default(),
    )))
}

/// Writes an alias definition as `alias` and `command -v` print it: `reusable`, the form
/// that reads back as a definition, or bare `name='value'`, which `alias` lists in POSIX
/// mode without `-p`, as Bash does.
#[cfg(any(feature = "builtin.alias", feature = "builtin.command"))]
fn write_alias_definition(
    mut writer: impl std::io::Write,
    name: &str,
    value: &str,
    reusable: bool,
) -> std::io::Result<()> {
    // A name starting with `-` is listed after `--`, as Bash lists it, so the line reads
    // back as a definition and not as options.
    let prefix = match (reusable, name.starts_with('-')) {
        (false, _) => "",
        (true, false) => "alias ",
        (true, true) => "alias -- ",
    };
    writeln!(
        writer,
        "{prefix}{name}={}",
        cash_core::escape::single_quote(value)
    )
}

/// Macro to define a struct that represents a shell built-in flag argument that can be
/// enabled or disabled by specifying an option with a leading '+' or '-' character.
///
/// # Arguments
///
/// - `$struct_name` - The identifier to be used for the struct to define.
/// - `$flag_char` - The character to use as the flag.
/// - `$desc` - The string description of the flag.
#[macro_export]
macro_rules! minus_or_plus_flag_arg {
    ($struct_name:ident, $flag_char:literal, $desc:literal) => {
        #[derive(clap::Parser, Clone)]
        pub(crate) struct $struct_name {
            #[arg(short = $flag_char, name = concat!(stringify!($struct_name), "_enable"), action = clap::ArgAction::SetTrue, help = $desc)]
            _enable: bool,
            #[arg(long = concat!("+", $flag_char), name = concat!(stringify!($struct_name), "_disable"), action = clap::ArgAction::SetTrue, hide = true)]
            _disable: bool,
        }

        impl From<$struct_name> for Option<bool> {
            fn from(value: $struct_name) -> Self {
                value.to_bool()
            }
        }

        impl $struct_name {
            #[allow(dead_code, reason = "may not be used in all macro instantiations")]
            pub const fn is_some(&self) -> bool {
                self._enable || self._disable
            }

            pub const fn to_bool(&self) -> Option<bool> {
                match (self._enable, self._disable) {
                    (true, false) => Some(true),
                    (false, true) => Some(false),
                    _ => None,
                }
            }
        }
    };
}
