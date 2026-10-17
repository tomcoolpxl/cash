//! `basename` and `dirname`, coreutils': the last component of a path, and what comes
//! before it.
//!
//! cash's own, not the bundled uutils ones (the user, 2026-10-16,
//! `research/in-process-tools-design.md`): pure string work that reads nothing and cannot
//! wait, the two tools BusyBox runs inside its shell, and among the commonest in scripts,
//! mostly as `name=$(basename "$f")` in a loop, which cost a process start, 20 ms, each.
//!
//! As Git Bash's GNU coreutils have them on Windows: `/` and `\` both separate; a drive
//! (`C:`) stays with what comes before (`dirname C:x` is `C:`, `dirname C:\dir\` is `C:`);
//! exactly two leading separators (`//srv`, `\\srv`) are a root of their own, and one or
//! three or more are one. A spelling comes back as the script gave it, never rewritten
//! (D3). uutils' had `basename /` print `\`. GNU's options, read by cash-getopt as
//! coreutils' `getopt_long` reads them: `+as:z` for basename, whose options end at the
//! first operand (`basename foo-bar -bar` is `foo`), and `z` for dirname, whose options
//! go anywhere until `--`; long ones by any unique prefix; GNU coreutils 9's messages,
//! with status 1.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use cash_getopt::{Arg, Getopt, Item, Long, Order, Short};
use clap::Parser;

/// Whether `c` separates path components on Windows.
const fn is_separator(c: u8) -> bool {
    c == b'/' || c == b'\\'
}

/// How long `path`'s drive is: 2 for `C:`, else 0.
const fn drive_length(path: &[u8]) -> usize {
    match path {
        [letter, b':', ..] if letter.is_ascii_alphabetic() => 2,
        _ => 0,
    }
}

/// How long a root of separators at the start of `path` is: exactly two stay two (a UNC
/// path's `//`), one or three or more are one.
fn root_length(path: &[u8]) -> usize {
    match path {
        [a, b, rest @ ..] if is_separator(*a) && is_separator(*b) => {
            if rest.first().copied().is_some_and(is_separator) {
                1
            } else {
                2
            }
        }
        [a, ..] if is_separator(*a) => 1,
        _ => 0,
    }
}

/// `path` from `start` to `end`: always next to an ASCII separator or a drive's colon,
/// so never inside a character.
fn cut(path: &str, start: usize, end: usize) -> &str {
    path.get(start..end).unwrap_or(path)
}

/// `path` without its directory, and without `suffix` when it ends so and is more than
/// it: GNU's `base_name` and `remove_suffix`.
fn base_name<'a>(path: &'a str, suffix: Option<&str>) -> &'a str {
    let bytes = path.as_bytes();
    let drive = drive_length(bytes);
    let after_drive = &bytes[drive..];
    if after_drive.iter().all(|&c| is_separator(c)) {
        // Only separators after the drive: the drive itself (`C:\` is `C:`), or the root.
        return if drive > 0 {
            cut(path, 0, drive)
        } else {
            cut(path, 0, root_length(bytes))
        };
    }
    let end = bytes
        .iter()
        .rposition(|&c| !is_separator(c))
        .map_or(drive, |last| last + 1);
    let start = bytes[..end]
        .iter()
        .rposition(|&c| is_separator(c))
        .map_or(drive, |sep| sep + 1)
        .max(drive);
    let name = cut(path, start, end);
    match suffix {
        Some(suffix) if !suffix.is_empty() && name.len() > suffix.len() => {
            name.strip_suffix(suffix).unwrap_or(name)
        }
        _ => name,
    }
}

/// `path` without its last component and the separators before it, or `.` when nothing
/// is left: GNU's `dir_len`.
fn dir_name(path: &str) -> &str {
    let bytes = path.as_bytes();
    let drive = drive_length(bytes);
    // A drive keeps none of the separators after it (`C:\x` is `C:`); a root keeps its
    // own (`/x` is `/`, `//srv` is `//`).
    let root = if drive > 0 { drive } else { root_length(bytes) };
    let end = bytes
        .iter()
        .rposition(|&c| !is_separator(c))
        .map_or(0, |last| last + 1)
        .max(drive);
    let mut length = bytes[..end]
        .iter()
        .rposition(|&c| is_separator(c))
        .map_or(drive, |sep| sep + 1)
        .max(drive)
        .max(root);
    while length > root && is_separator(bytes[length - 1]) {
        length -= 1;
    }
    if length == 0 {
        "."
    } else {
        cut(path, 0, length)
    }
}

/// What a command line asked of `basename` or `dirname`.
#[derive(Debug, Default, PartialEq, Eq)]
struct Request {
    names: Vec<String>,
    suffix: Option<String>,
    multiple: bool,
    zero: bool,
}

/// A command line that ends the command before it runs.
#[derive(Debug, PartialEq, Eq)]
enum Early {
    Help,
    Version,
    /// A usage error, said with GNU's "Try" line.
    Usage(String),
}

/// basename's options, coreutils' `+as:z`, and its long ones; help and version have no
/// letter.
const BASENAME_SHORTS: &[Short<char>] = &[
    Short::new('a', Arg::No, 'a'),
    Short::new('s', Arg::Required, 's'),
    Short::new('z', Arg::No, 'z'),
];
const BASENAME_LONGS: &[Long<'static, char>] = &[
    Long::new("help", Arg::No, 'h'),
    Long::new("multiple", Arg::No, 'a'),
    Long::new("suffix", Arg::Required, 's'),
    Long::new("version", Arg::No, 'v'),
    Long::new("zero", Arg::No, 'z'),
];
/// dirname's options, coreutils' `z`.
const DIRNAME_SHORTS: &[Short<char>] = &[Short::new('z', Arg::No, 'z')];
const DIRNAME_LONGS: &[Long<'static, char>] = &[
    Long::new("help", Arg::No, 'h'),
    Long::new("version", Arg::No, 'v'),
    Long::new("zero", Arg::No, 'z'),
];

/// Reads GNU's options for `tool`, `basename` or `dirname`, in order, as coreutils does:
/// `--help` before a mistake after it is the help.
fn parse(tool: &str, args: &[String]) -> Result<Request, Early> {
    let basename = tool == "basename";
    let getopt = if basename {
        Getopt::new(BASENAME_SHORTS, BASENAME_LONGS).order(Order::StopAtOperand)
    } else {
        Getopt::new(DIRNAME_SHORTS, DIRNAME_LONGS)
    };
    let mut request = Request::default();
    for item in getopt.read(args) {
        match item.map_err(|problem| Early::Usage(problem.to_string()))? {
            Item::Option { id: 'h', .. } => return Err(Early::Help),
            Item::Option { id: 'v', .. } => return Err(Early::Version),
            Item::Option { id: 'a', .. } => request.multiple = true,
            Item::Option { id: 's', value, .. } => {
                request.suffix = value;
                request.multiple = true;
            }
            Item::Option { .. } => request.zero = true,
            Item::Operand { value, .. } => request.names.push(value),
        }
    }
    if request.names.is_empty() {
        return Err(Early::Usage("missing operand".to_owned()));
    }
    if basename && !request.multiple {
        // `basename NAME [SUFFIX]`.
        match request.names.len() {
            1 => {}
            2 => request.suffix = request.names.pop(),
            _ => {
                return Err(Early::Usage(format!(
                    "extra operand '{}'",
                    request.names[2]
                )));
            }
        }
    }
    Ok(request)
}

const BASENAME_HELP: &str = "\
Usage: basename NAME [SUFFIX]
  or:  basename OPTION... NAME...
Print NAME with any leading directory components removed.
If specified, also remove a trailing SUFFIX.

Mandatory arguments to long options are mandatory for short options too.
  -a, --multiple       support multiple arguments and treat each as a NAME
  -s, --suffix=SUFFIX  remove a trailing SUFFIX; implies -a
  -z, --zero           end each output line with NUL, not newline
      --help        display this help and exit
      --version     output version information and exit

Examples:
  basename /usr/bin/sort          -> \"sort\"
  basename include/stdio.h .h     -> \"stdio\"
  basename -s .h include/stdio.h  -> \"stdio\"
  basename -a any/str1 any/str2   -> \"str1\" followed by \"str2\"
  basename 'C:\\Users\\me\\notes.txt' -> \"notes.txt\"

Both / and \\ separate, as in Git Bash.";

const DIRNAME_HELP: &str = "\
Usage: dirname [OPTION] NAME...
Output each NAME with its last non-slash component and trailing slashes
removed; if NAME contains no /'s, output '.' (meaning the current directory).

  -z, --zero     end each output line with NUL, not newline
      --help        display this help and exit
      --version     output version information and exit

Examples:
  dirname /usr/bin/          -> \"/usr\"
  dirname dir1/str dir2/str  -> \"dir1\" followed by \"dir2\"
  dirname stdio.h            -> \".\"
  dirname 'C:\\Users\\me'      -> \"C:\\Users\"

Both / and \\ separate, as in Git Bash; a drive stays with what comes before.";

/// Runs `basename` or `dirname`, by `tool`, on `args`.
fn run<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    tool: &str,
    args: &[String],
) -> Result<ExecutionResult, cash_core::Error> {
    let request = match crate::options::read_options(context, || parse(tool, args)) {
        Ok(request) => request,
        Err(Early::Help) => {
            let help = if tool == "basename" {
                BASENAME_HELP
            } else {
                DIRNAME_HELP
            };
            writeln!(context.stdout(), "{help}")?;
            return Ok(ExecutionResult::success());
        }
        Err(Early::Version) => {
            writeln!(
                context.stdout(),
                "{tool} (cash), with GNU coreutils' options"
            )?;
            return Ok(ExecutionResult::success());
        }
        Err(Early::Usage(message)) => {
            writeln!(
                context.stderr(),
                "{tool}: {message}\nTry '{tool} --help' for more information."
            )?;
            return Ok(ExecutionResult::general_error());
        }
    };
    let end = if request.zero { "\0" } else { "\n" };
    let mut out = String::new();
    for name in &request.names {
        let answer = if tool == "basename" {
            base_name(name, request.suffix.as_deref())
        } else {
            dir_name(name)
        };
        out.push_str(answer);
        out.push_str(end);
    }
    context.stdout().write_all(out.as_bytes())?;
    Ok(ExecutionResult::success())
}

/// Print NAME with any leading directory components removed.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct BasenameCommand {
    /// Options and names, parsed here as GNU's are.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for BasenameCommand {
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
        run(&context, "basename", &self.args)
    }
}

/// Output each NAME with its last component and trailing slashes removed.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct DirnameCommand {
    /// Options and names, parsed here as GNU's are.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for DirnameCommand {
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
        run(&context, "dirname", &self.args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Git Bash's GNU coreutils 8.32, run on each (2026-10-16).
    const GIT_BASH: &[(&str, &str, &str)] = &[
        ("a/b/c.txt", "c.txt", "a/b"),
        ("a/b/", "b", "a"),
        ("a/b//", "b", "a"),
        ("/", "/", "/"),
        ("//", "//", "//"),
        ("///", "/", "/"),
        ("", "", "."),
        ("a", "a", "."),
        (".", ".", "."),
        ("..", "..", "."),
        ("../x", "x", ".."),
        ("/a", "a", "/"),
        ("/a/", "a", "/"),
        ("a//b", "b", "a"),
        ("C:\\keep\\this", "this", "C:\\keep"),
        ("C:\\keep\\", "keep", "C:"),
        ("C:\\", "C:", "C:"),
        ("C:", "C:", "C:"),
        ("C:/x/y", "y", "C:/x"),
        ("C:/", "C:", "C:"),
        ("C:x", "x", "C:"),
        ("C:/x", "x", "C:"),
        ("c:\\x\\", "x", "c:"),
        ("C:\\\\", "C:", "C:"),
        ("C:/x/", "x", "C:"),
        ("a\\b", "b", "a"),
        ("a\\\\", "a", "."),
        ("a/\\b", "b", "a"),
        ("/\\/", "/", "/"),
        ("\\", "\\", "\\"),
        ("\\\\", "\\\\", "\\\\"),
        ("\\\\\\", "\\", "\\"),
        ("\\/", "\\/", "\\/"),
        ("\\\\srv", "srv", "\\\\"),
        ("\\\\srv\\share", "share", "\\\\srv"),
        ("\\\\srv\\share\\", "share", "\\\\srv"),
        ("\\\\srv\\share\\f", "f", "\\\\srv\\share"),
        ("//srv", "srv", "//"),
        ("//srv/share/f", "f", "//srv/share"),
        ("/c/Users/x", "x", "/c/Users"),
        ("x y/z w", "z w", "x y"),
        ("a/b/c.tar.gz", "c.tar.gz", "a/b"),
    ];

    #[test]
    fn names_are_git_bashs() {
        for &(path, base, dir) in GIT_BASH {
            assert_eq!(base_name(path, None), base, "basename {path:?}");
            assert_eq!(dir_name(path), dir, "dirname {path:?}");
        }
    }

    #[test]
    fn a_suffix_goes_only_when_something_is_left() {
        assert_eq!(base_name("a/b/c.txt", Some(".txt")), "c");
        assert_eq!(base_name("c.txt", Some("c.txt")), "c.txt");
        assert_eq!(base_name("x.txt", Some(".txt.txt")), "x.txt");
        assert_eq!(base_name("y.txt.txt", Some(".txt")), "y.txt");
        assert_eq!(base_name("a.c", Some("")), "a.c");
    }

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn options_are_gnus() {
        let request = parse("basename", &words("a/b.txt .txt")).unwrap();
        assert_eq!(request.names, ["a/b.txt"]);
        assert_eq!(request.suffix.as_deref(), Some(".txt"));
        let request = parse("basename", &words("-s .txt -a x.txt y.txt")).unwrap();
        assert_eq!((request.names.len(), request.multiple), (2, true));
        let request = parse("basename", &words("-azs.c a.c")).unwrap();
        assert!(request.zero && request.multiple);
        assert_eq!(request.suffix.as_deref(), Some(".c"));
        let request = parse("basename", &words("--suf=.c --mul a.c b.c")).unwrap();
        assert_eq!(request.suffix.as_deref(), Some(".c"));
        assert_eq!(request.names, ["a.c", "b.c"]);
        let request = parse("basename", &words("-- -y")).unwrap();
        assert_eq!(request.names, ["-y"]);
        // The options end at the first operand, as Git Bash's: what follows is a suffix,
        // or more names under `-a`.
        let request = parse("basename", &words("foo-bar -bar")).unwrap();
        assert_eq!(request.suffix.as_deref(), Some("-bar"));
        let request = parse("basename", &words("x --version")).unwrap();
        assert_eq!(request.suffix.as_deref(), Some("--version"));
        let request = parse("basename", &words("-a x -s")).unwrap();
        assert_eq!(request.names, ["x", "-s"]);
        let request = parse("dirname", &words("a/b -z c/d")).unwrap();
        assert_eq!((request.names.len(), request.zero), (2, true));
    }

    #[test]
    fn mistakes_are_said_as_gnu_says_them() {
        let usage = |tool: &str, line: &str| match parse(tool, &words(line)) {
            Err(Early::Usage(message)) => message,
            other => panic!("{other:?}"),
        };
        assert_eq!(usage("basename", ""), "missing operand");
        assert_eq!(usage("basename", "a b c"), "extra operand 'c'");
        assert_eq!(usage("basename", "-q x"), "invalid option -- 'q'");
        assert_eq!(
            usage("basename", "--bogus=1 x"),
            "unrecognized option '--bogus=1'"
        );
        assert_eq!(
            usage("basename", "-s"),
            "option requires an argument -- 's'"
        );
        assert_eq!(usage("basename", "x -- -y"), "extra operand '-y'");
        assert_eq!(
            usage("basename", "--zero=1 x"),
            "option '--zero' doesn't allow an argument"
        );
        assert_eq!(usage("dirname", "-a x"), "invalid option -- 'a'");
        assert_eq!(usage("dirname", ""), "missing operand");
        assert_eq!(parse("dirname", &words("--he")), Err(Early::Help));
        assert_eq!(
            parse("basename", &words("--version --bogus")),
            Err(Early::Version)
        );
    }
}
