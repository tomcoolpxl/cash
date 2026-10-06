//! `getconf`, glibc's: print a configuration value by name. The values are the ones
//! Windows has: the logical processor count for `_NPROCESSORS_ONLN` and
//! `_NPROCESSORS_CONF`, `GetSystemInfo`'s page size, `ARG_MAX` 32767 (a command line's
//! limit), `NAME_MAX` 255, `PATH_MAX` 260 or 32767 when long paths are on, `PATH` as
//! `$PATH` reads in the shell, and `limits.h`'s constants for a 64-bit `long`, as the
//! Cygwin `getconf` of Git for Windows reports them. `-a` lists them all in glibc's
//! two-column shape; a name it does not know is glibc's `Unrecognized variable`,
//! status 1. A path may follow the names `pathconf` answers (`NAME_MAX`, `PATH_MAX`,
//! `PIPE_BUF`, `FILESIZEBITS`, `LINK_MAX`) and is not looked at.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Print a configuration value.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct GetconfCommand {
    /// Options and operands, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

const USAGE: &str = "\
Usage: getconf [-v specification] variable_name [pathname]
       getconf -a [pathname]

Get configuration values

  -v specification     Indicate specific version for which configuration
                       values shall be fetched.
  -a, --all            Print all known configuration values

Other options:

  -h, --help           This text
  -V, --version        Print program version and exit";

/// The names `getconf NAME PATH` takes a path for.
const PATH_NAMES: [&str; 5] = [
    "NAME_MAX",
    "PATH_MAX",
    "PIPE_BUF",
    "FILESIZEBITS",
    "LINK_MAX",
];

/// A value, computed when asked.
struct Variable {
    name: &'static str,
    value: fn(&Machine) -> String,
}

/// What the values depend on, read once per run.
struct Machine {
    processors: usize,
    page_size: u32,
    long_paths: bool,
    path: String,
}

/// Every name, in the order `-a` lists them.
const VARIABLES: &[Variable] = &[
    Variable {
        name: "ARG_MAX",
        value: |_| "32767".into(),
    },
    Variable {
        name: "CHAR_BIT",
        value: |_| "8".into(),
    },
    Variable {
        name: "CHAR_MAX",
        value: |_| "127".into(),
    },
    Variable {
        name: "CHAR_MIN",
        value: |_| "-128".into(),
    },
    Variable {
        name: "FILESIZEBITS",
        value: |_| "64".into(),
    },
    Variable {
        name: "HOST_NAME_MAX",
        value: |_| "64".into(),
    },
    Variable {
        name: "INT_MAX",
        value: |_| i32::MAX.to_string(),
    },
    Variable {
        name: "INT_MIN",
        value: |_| i32::MIN.to_string(),
    },
    Variable {
        name: "LINE_MAX",
        value: |_| "2048".into(),
    },
    Variable {
        name: "LINK_MAX",
        value: |_| "1023".into(),
    },
    Variable {
        name: "LLONG_MAX",
        value: |_| i64::MAX.to_string(),
    },
    Variable {
        name: "LLONG_MIN",
        value: |_| i64::MIN.to_string(),
    },
    Variable {
        name: "LOGIN_NAME_MAX",
        value: |_| "256".into(),
    },
    Variable {
        name: "LONG_BIT",
        value: |_| "64".into(),
    },
    Variable {
        name: "LONG_MAX",
        value: |_| i64::MAX.to_string(),
    },
    Variable {
        name: "LONG_MIN",
        value: |_| i64::MIN.to_string(),
    },
    Variable {
        name: "NAME_MAX",
        value: |_| "255".into(),
    },
    Variable {
        name: "OPEN_MAX",
        value: |_| "8192".into(),
    },
    Variable {
        name: "PAGESIZE",
        value: |m| m.page_size.to_string(),
    },
    Variable {
        name: "PAGE_SIZE",
        value: |m| m.page_size.to_string(),
    },
    Variable {
        name: "PATH",
        value: |m| m.path.clone(),
    },
    Variable {
        name: "PATH_MAX",
        value: |m| if m.long_paths { "32767" } else { "260" }.into(),
    },
    Variable {
        name: "PIPE_BUF",
        value: |_| "4096".into(),
    },
    Variable {
        name: "SCHAR_MAX",
        value: |_| "127".into(),
    },
    Variable {
        name: "SCHAR_MIN",
        value: |_| "-128".into(),
    },
    Variable {
        name: "SHRT_MAX",
        value: |_| "32767".into(),
    },
    Variable {
        name: "SHRT_MIN",
        value: |_| "-32768".into(),
    },
    Variable {
        name: "SSIZE_MAX",
        value: |_| i64::MAX.to_string(),
    },
    Variable {
        name: "UCHAR_MAX",
        value: |_| "255".into(),
    },
    Variable {
        name: "UINT_MAX",
        value: |_| u32::MAX.to_string(),
    },
    Variable {
        name: "ULLONG_MAX",
        value: |_| u64::MAX.to_string(),
    },
    Variable {
        name: "ULONG_MAX",
        value: |_| u64::MAX.to_string(),
    },
    Variable {
        name: "USHRT_MAX",
        value: |_| "65535".into(),
    },
    Variable {
        name: "WORD_BIT",
        value: |_| "32".into(),
    },
    Variable {
        name: "_NPROCESSORS_CONF",
        value: |m| m.processors.to_string(),
    },
    Variable {
        name: "_NPROCESSORS_ONLN",
        value: |m| m.processors.to_string(),
    },
];

/// The value of `name`, if it is known.
fn lookup(name: &str, machine: &Machine) -> Option<String> {
    VARIABLES
        .iter()
        .find(|variable| variable.name == name)
        .map(|variable| (variable.value)(machine))
}

/// Every name and value, as `-a` prints them: the name in a 35-column field, a space,
/// the value.
fn listing(machine: &Machine) -> String {
    use std::fmt::Write as _;

    let mut text = String::new();
    for variable in VARIABLES {
        let _ = writeln!(text, "{:<35} {}", variable.name, (variable.value)(machine));
    }
    text
}

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
enum Request {
    Help,
    Version,
    All,
    One(String, Option<String>),
    Usage,
}

/// The options as glibc reads them: `-v SPEC` (ignored), `-a`, `-h`, `-V`, then a name
/// and perhaps a path.
fn parse(args: &[String]) -> Request {
    let mut all = false;
    let mut operands: Vec<&str> = Vec::new();
    let mut args = args.iter().map(String::as_str);
    let mut only_operands = false;
    while let Some(arg) = args.next() {
        if only_operands || !arg.starts_with('-') || arg == "-" {
            operands.push(arg);
            continue;
        }
        match arg {
            "--" => only_operands = true,
            "-a" | "--all" => all = true,
            "-h" | "--help" => return Request::Help,
            "-V" | "--version" => return Request::Version,
            "-v" | "--specification" => {
                if args.next().is_none() {
                    return Request::Usage;
                }
            }
            _ if arg.starts_with("-v") || arg.starts_with("--specification=") => {}
            _ => return Request::Usage,
        }
    }
    if all {
        return if operands.len() <= 1 {
            Request::All
        } else {
            Request::Usage
        };
    }
    match operands.as_slice() {
        [name] => Request::One((*name).to_owned(), None),
        [name, path] if PATH_NAMES.contains(name) => {
            Request::One((*name).to_owned(), Some((*path).to_owned()))
        }
        _ => Request::Usage,
    }
}

impl builtins::Command for GetconfCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let machine = Machine {
            processors: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
            page_size: cash_win32::sysinfo::page_size(),
            long_paths: cash_win32::sysinfo::long_paths_enabled(),
            path: context
                .shell
                .env_str("PATH")
                .map_or_else(String::new, |path| path.into_owned()),
        };
        match parse(&self.args) {
            Request::Help => {
                writeln!(context.stdout(), "{USAGE}")?;
                Ok(ExecutionResult::success())
            }
            Request::Version => {
                writeln!(
                    context.stdout(),
                    "getconf (cash): glibc's interface, with the values Windows has"
                )?;
                Ok(ExecutionResult::success())
            }
            Request::Usage => {
                writeln!(context.stderr(), "{USAGE}")?;
                Ok(ExecutionResult::general_error())
            }
            Request::All => {
                write!(context.stdout(), "{}", listing(&machine))?;
                Ok(ExecutionResult::success())
            }
            Request::One(name, _path) => {
                if let Some(value) = lookup(&name, &machine) {
                    writeln!(context.stdout(), "{value}")?;
                    Ok(ExecutionResult::success())
                } else {
                    writeln!(context.stderr(), "getconf: Unrecognized variable '{name}'")?;
                    Ok(ExecutionResult::general_error())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    fn machine() -> Machine {
        Machine {
            processors: 16,
            page_size: 4096,
            long_paths: false,
            path: "/c/tools:/usr/bin".into(),
        }
    }

    #[test]
    fn the_values_windows_has() {
        let m = machine();
        assert_eq!(lookup("_NPROCESSORS_ONLN", &m).as_deref(), Some("16"));
        assert_eq!(lookup("PAGESIZE", &m).as_deref(), Some("4096"));
        assert_eq!(lookup("PATH_MAX", &m).as_deref(), Some("260"));
        assert_eq!(lookup("PATH", &m).as_deref(), Some("/c/tools:/usr/bin"));
        assert_eq!(lookup("LONG_BIT", &m).as_deref(), Some("64"));
        assert_eq!(lookup("ARG_MAX", &m).as_deref(), Some("32767"));
        assert_eq!(lookup("UINT_MAX", &m).as_deref(), Some("4294967295"));
        assert_eq!(lookup("GNU_LIBC_VERSION", &m), None);
        let long = Machine {
            long_paths: true,
            ..machine()
        };
        assert_eq!(lookup("PATH_MAX", &long).as_deref(), Some("32767"));
    }

    #[test]
    fn the_listing_is_sorted_and_two_columns() {
        let names: Vec<&str> = VARIABLES.iter().map(|v| v.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        let text = listing(&machine());
        assert!(
            text.contains("PAGESIZE                            4096\n"),
            "{text}"
        );
        assert!(text.lines().all(|line| line.len() > 36), "{text}");
    }

    #[test]
    fn the_command_line_is_glibcs() {
        assert_eq!(parse(&args("-a")), Request::All);
        assert_eq!(parse(&args("-a /")), Request::All);
        assert_eq!(
            parse(&args("-v POSIX_V7 LONG_BIT")),
            Request::One("LONG_BIT".into(), None)
        );
        assert_eq!(
            parse(&args("NAME_MAX /tmp")),
            Request::One("NAME_MAX".into(), Some("/tmp".into()))
        );
        assert_eq!(parse(&args("LONG_BIT /tmp")), Request::Usage);
        assert_eq!(parse(&args("")), Request::Usage);
        assert_eq!(parse(&args("-x")), Request::Usage);
        assert_eq!(parse(&args("--help")), Request::Help);
        assert_eq!(parse(&args("-V")), Request::Version);
    }
}
