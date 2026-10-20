//! `find` — **D48**, and the worst case D35 exists for.
//!
//! On a clean Windows machine, `find` is `C:\Windows\System32\find.exe`: a DOS tool that
//! searches for *text inside files*. It is not a poor `find`, it is a different command
//! with the same name, and it has been on `PATH` since 1983. A script's first
//! `find . -name '*.log'` does not fail in a way anyone can read — it reports
//! `FIND: Parameter format not correct`.
//!
//! The alternative on a developer's machine is Git for Windows' MSYS build, which brings
//! its own trap: the MinGW runtime re-globs its own `argv`, so a *quoted* `"*.toml"` —
//! quoted precisely to keep the shell's hands off it — arrives already expanded, and
//! `find` reports `paths must precede expression`. cash passes the argument correctly;
//! there is nothing a shell can do about what the callee does next.
//!
//! So cash carries one. Two things make it cash's business rather than Scoop's:
//!
//! - **it prints paths**, and D3 says one canonical spelling on the way out. A `find`
//!   that printed `C:\src\a.txt` would feed `xargs` a string whose backslashes are
//!   escapes — the near-miss §4 records, where `find | xargs grep` silently produced
//!   `C:srca.txt`;
//! - **`-exec` spawns processes**, which is D6's territory.
//!
//! What is implemented is what scripts use. Anything else is **refused by name** rather
//! than ignored, because a `find` that quietly drops `-regex` returns the wrong file list
//! and nothing says so.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use cash_core::{ExecutionResult, builtins, patterns::Pattern};

use crate::walk::{self as walker, Links};
use clap::Parser;

/// Search a directory tree for files matching an expression.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct FindCommand {
    /// Starting points, then the expression — parsed here rather than by clap, because
    /// `find`'s grammar is an expression language, not a list of options.
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    args: Vec<String>,
}

/// What `-help` and `-version`, predicates GNU's find answers where it meets them, end
/// the parse with: no complaint can begin with a NUL.
const HELP_ASKED: &str = "\0help";
const VERSION_ASKED: &str = "\0version";

/// GNU findutils' `find --help`, cut to the predicates cash's find carries.
const HELP: &str = "\
Usage: find [-H] [-L] [-P] [-Olevel] [-D debugopts] [path...] [expression]

Default path is the current directory; default expression is -print.
Expression may consist of: operators, options, tests, and actions.

Operators (decreasing precedence; -and is implicit where no others are given):
      ( EXPR )   ! EXPR   -not EXPR   EXPR1 -a EXPR2   EXPR1 -and EXPR2
      EXPR1 -o EXPR2   EXPR1 -or EXPR2

Positional options (always true):
      -follow

Normal options (always true, specified before other expressions):
      -depth -maxdepth LEVELS -mindepth LEVELS

Tests (N can be +N or -N or N):
      -amin N -atime N -cmin N -ctime N -empty -false -iname PATTERN
      -ipath PATTERN -mmin N -mtime N -name PATTERN -newer FILE -path PATTERN
      -size N[bckMG] -true -type [fdl] -wholename PATTERN

Actions:
      -delete -print -print0 -prune -quit
      -exec COMMAND ; -exec COMMAND {} + -execdir COMMAND ; -execdir COMMAND {} +
      -ok COMMAND ; -okdir COMMAND ;

Other common options:
      --help                   display this help and exit
      --version                output version information and exit

On Windows, names match without regard to case, so -name and -iname agree,
and -H is taken as -P. A predicate GNU's find has beyond these is refused by
name rather than ignored.";

/// The walk and the expression `args` ask for, or what the command line ends with, said:
/// the help, the version, or a complaint.
fn read_command_line<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    args: &[String],
) -> Result<Result<(Walk, Expr), ExecutionResult>, cash_core::Error> {
    match parse(args, context.shell.working_dir()) {
        Ok(parsed) => Ok(Ok(parsed)),
        Err(asked) if asked == HELP_ASKED => {
            writeln!(context.stdout(), "{HELP}")?;
            Ok(Err(ExecutionResult::success()))
        }
        Err(asked) if asked == VERSION_ASKED => {
            writeln!(
                context.stdout(),
                "find (cash): GNU findutils 4.10's predicates"
            )?;
            Ok(Err(ExecutionResult::success()))
        }
        Err(complaint) => {
            writeln!(context.stderr(), "{}: {complaint}", context.command_name)?;
            Ok(Err(ExecutionResult::general_error()))
        }
    }
}

/// What a name is being matched against.
#[derive(Clone, Copy)]
enum Subject {
    /// `-name`: the last component only.
    BaseName,
    /// `-path`: the whole path as it would be printed.
    WholePath,
}

/// The kinds `-type` can ask for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    File,
    Directory,
    Symlink,
}

/// How a numeric test compares: `-mtime 7`, `-mtime +7`, `-mtime -7`.
#[derive(Clone, Copy)]
enum Compare {
    Exactly,
    MoreThan,
    LessThan,
}

impl Compare {
    /// Reads the leading `+` or `-`, returning the comparison and the digits after it.
    fn parse(value: &str) -> (Self, &str) {
        match value.strip_prefix('+') {
            Some(rest) => (Self::MoreThan, rest),
            None => match value.strip_prefix('-') {
                Some(rest) => (Self::LessThan, rest),
                None => (Self::Exactly, value),
            },
        }
    }

    const fn holds(self, actual: u64, wanted: u64) -> bool {
        match self {
            Self::Exactly => actual == wanted,
            Self::MoreThan => actual > wanted,
            Self::LessThan => actual < wanted,
        }
    }
}

/// Which of an entry's times an age test reads.
#[derive(Clone, Copy)]
enum Stamp {
    /// `m`: its contents last changed.
    Modified,
    /// `a`: it was last read.
    Accessed,
    /// `c`: its contents, name or attributes last changed (NTFS's `ChangeTime`, which `ls
    /// -c` and `stat` show).
    Changed,
}

/// A condition on one entry.
enum Test {
    Name(Pattern, Subject),
    Type(Kind),
    /// `-size`, in whatever unit was given, rounded the way `find` rounds: up.
    Size(Compare, u64, u64),
    Newer(SystemTime),
    /// `-mtime`, `-atime` and `-ctime` in days, `-mmin`, `-amin` and `-cmin` in minutes:
    /// how long ago one of its times was, in whole units of the given seconds, as GNU
    /// `find` counts it.
    Age(Stamp, Compare, u64, u64),
    Empty,
    True,
    False,
}

/// Something done to an entry that matches.
enum Action {
    Print,
    Print0,
    Delete,
    /// `-exec cmd {} \;` runs once per entry; `-exec cmd {} +` batches.
    Exec(Exec),
    Prune,
    Quit,
}

/// A command an entry runs: `-exec`, `-execdir`, `-ok` or `-okdir`.
struct Exec {
    argv: Vec<String>,
    /// `+`: once for many entries, after the walk.
    batch: bool,
    /// `-execdir`, `-okdir`: in the entry's own directory, `{}` its name as `./name`.
    in_dir: bool,
    /// `-ok`, `-okdir`: asked on standard error first, run on a `y`.
    ask: bool,
}

/// The commands `-exec ... +` and `-execdir ... +` gathered for after the walk: each
/// command, the directory it runs in (`-execdir`'s), and its paths.
type Batches = Vec<(Vec<String>, Option<PathBuf>, Vec<String>)>;

/// The expression, as parsed.
enum Expr {
    Test(Test),
    Action(Action),
    Not(Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
}

impl Expr {
    /// Whether `-delete` is anywhere in it, which makes the walk depth-first.
    fn deletes(&self) -> bool {
        match self {
            Self::Action(Action::Delete) => true,
            Self::Test(_) | Self::Action(_) => false,
            Self::Not(inner) => inner.deletes(),
            Self::And(left, right) | Self::Or(left, right) => left.deletes() || right.deletes(),
        }
    }
}

/// Everything an expression can ask for or change while one entry is evaluated.
struct Visit<'a> {
    path: &'a Path,
    rendered: String,
    metadata: Option<std::fs::Metadata>,
    prune: bool,
    quit: bool,
    failed: bool,
}

/// The options that apply to the whole walk rather than to one entry.
struct Walk {
    starts: Vec<String>,
    min_depth: usize,
    max_depth: usize,
    /// `-P` (none), `-H` (the starting points') or `-L` and `-follow` (every one).
    links: Links,
    /// `-depth`, or implied by `-delete`: a directory after what is in it, as GNU `find`
    /// does. `-delete` went the other way, so it never removed a directory: it was not
    /// empty yet when its turn came (BI-04).
    contents_first: bool,
}

impl builtins::Command for FindCommand {
    type Error = cash_core::Error;

    /// The words as they are: clap answered `-h` and `--help` itself and took a leading
    /// `--` for its own, where GNU's find reads both.
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
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let (walk, expr) = match read_command_line(&context, &self.args)? {
            Ok(parsed) => parsed,
            Err(ended) => return Ok(ended),
        };

        let mut failed = false;
        let mut batches: Batches = Vec::new();
        // A Ctrl-C, or `kill` of a background job, stops the walk.
        let stop = context.stop_token();

        'walking: for start in &walk.starts {
            // Two paths per entry: the one to print, spelled as the caller asked, and the
            // one to open, which is where that actually is. cash: the shell's working
            // directory is its own (D3/D10) — `cd` updates what the shell believes without
            // moving the process — so a start is resolved by the shell. Reading `.` from
            // the process's directory made `cd crates; find .` walk the repository root.
            let actual = context.shell.absolute_path(Path::new(start));
            // `-depth` (or `-delete`) visits a directory after what is in it; `-mindepth`
            // goes on through what it does not show.
            let options = walker::WalkOptions {
                links: walk.links,
                order: walker::Order::Ordinal,
                contents_first: walk.contents_first,
                min_depth: walk.min_depth,
                max_depth: walk.max_depth,
                one_file_system: false,
            };
            let mut entries = walker::Walk::new(start, &actual, &options, stop.clone());
            while let Some(next) = entries.next() {
                let entry = match next {
                    Ok(entry) => entry,
                    // GNU's `find -L` lists a link whose target is not there as the link.
                    Err(walker::WalkError::Dangling(entry)) => entry,
                    Err(walker::WalkError::Stopped) => break 'walking,
                    // Following links, a directory can be its own ancestor; it is said and
                    // left, as GNU `find` does. The walk went round it without end (BI-13).
                    Err(walker::WalkError::Loop { shown, ancestor }) => {
                        writeln!(
                            context.stderr(),
                            "{}: File system loop detected; '{}' is part of the same file system loop as '{}'.",
                            context.command_name,
                            render(Path::new(&shown)),
                            render(Path::new(&ancestor))
                        )?;
                        failed = true;
                        continue;
                    }
                    Err(
                        walker::WalkError::Io { shown, error }
                        | walker::WalkError::Unlisted { shown, error, .. },
                    ) => {
                        // Quoted, as GNU's find quotes a name it could not read.
                        writeln!(
                            context.stderr(),
                            "{}: '{}': {}",
                            context.command_name,
                            render(Path::new(&shown)),
                            cash_core::error::os_error_text(&error)
                        )?;
                        failed = true;
                        continue;
                    }
                };
                let mut visit = Visit {
                    path: entry.path.as_path(),
                    rendered: render(Path::new(&entry.shown)),
                    metadata: entry.metadata().ok(),
                    prune: false,
                    quit: false,
                    failed: false,
                };
                evaluate(&expr, &mut visit, &context, &mut batches).await?;
                failed |= visit.failed;
                if visit.quit {
                    break 'walking;
                }
                // `-prune` keeps the walk out of a directory; after its contents, as under
                // `-depth`, there is nothing left to keep out of.
                if visit.prune && !walk.contents_first && entry.kind == walker::Kind::Directory {
                    entries.skip_directory();
                }
            }
        }

        if stop.requested() {
            return context.stopped().await;
        }

        failed |= !run_batches(&context, batches).await?;
        if failed {
            return Ok(ExecutionResult::general_error());
        }

        Ok(ExecutionResult::success())
    }
}

/// `paths` split so that each part, with the command, fits a Windows command line, as
/// `xargs` splits its input.
fn command_line_chunks<'a>(argv: &[String], paths: &'a [String]) -> Vec<&'a [String]> {
    let base: usize = argv.iter().map(|part| part.len() + 3).sum();
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut length = base;
    for (index, path) in paths.iter().enumerate() {
        let size = path.len() + 3;
        if index > start && length + size > crate::xargs::MAX_COMMAND_LINE {
            chunks.extend(paths.get(start..index));
            start = index;
            length = base;
        }
        length += size;
    }
    if start < paths.len() {
        chunks.extend(paths.get(start..));
    }
    chunks
}

/// Runs `-exec cmd {} +`'s commands once the walk is over, as often as a Windows command
/// line needs to hold all their arguments; it ran once, and a long list failed to start
/// (BI-11). Whether every run succeeded: one that fails fails find, as in GNU `find`.
async fn run_batches<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    batches: Batches,
) -> Result<bool, cash_core::Error> {
    let mut succeeded = true;
    for (argv, dir, collected) in batches {
        for chunk in command_line_chunks(&argv, &collected) {
            if !run(context, &argv, chunk, dir.as_deref()).await? {
                succeeded = false;
            }
        }
    }
    Ok(succeeded)
}

/// Renders a path the way cash renders every path it prints (D3).
fn render(path: &Path) -> String {
    cash_win32::path::render(path)
}

/// Splits the arguments into the walk's options, its starting points and the expression.
///
/// `here` is the shell's working directory, which a file an expression names (`-newer`)
/// is relative to, not the process's (D10).
fn parse(args: &[String], here: &Path) -> Result<(Walk, Expr), String> {
    let mut walk = Walk {
        starts: Vec::new(),
        min_depth: 0,
        max_depth: usize::MAX,
        links: Links::Never,
        contents_first: false,
    };

    let mut index = 0;

    // Anything before the first predicate is a starting point — except the `-H`/`-L`/`-P`
    // options, which `find` takes before them, with GNU's `-D DEBUGOPTS` and `-OLEVEL`
    // (which change nothing here) and a `--` that ends them.
    while index < args.len() {
        match args[index].as_str() {
            option @ ("-P" | "-H" | "-L" | "-follow") => {
                walk.links = match option {
                    "-P" => Links::Never,
                    "-H" => Links::Roots,
                    _ => Links::All,
                };
                index += 1;
            }
            "--" if walk.starts.is_empty() => index += 1,
            "-D" if walk.starts.is_empty() => index += 2,
            level
                if walk.starts.is_empty()
                    && level
                        .strip_prefix("-O")
                        .is_some_and(|digits| digits.chars().all(|c| c.is_ascii_digit())) =>
            {
                index += 1;
            }
            arg if arg.starts_with('-') || arg == "(" || arg == "!" => break,
            arg => {
                walk.starts.push(arg.to_string());
                index += 1;
            }
        }
    }

    if walk.starts.is_empty() {
        walk.starts.push(String::from("."));
    }

    // `-maxdepth`/`-mindepth` belong to the walk rather than to the expression, so they
    // are lifted out before it is parsed — which is also why `find . -maxdepth 1 -name x`
    // reads naturally.
    let mut rest: Vec<String> = Vec::new();
    while index < args.len() {
        match args[index].as_str() {
            "-maxdepth" | "-mindepth" => {
                let which = args[index].clone();
                let Some(value) = args.get(index + 1) else {
                    return Err(std::format!("missing argument to `{which}'"));
                };
                let depth: usize = value
                    .parse()
                    .map_err(|_| std::format!("invalid argument `{value}' to `{which}'"))?;
                if which == "-maxdepth" {
                    walk.max_depth = depth;
                } else {
                    walk.min_depth = depth;
                }
                index += 2;
            }
            "-follow" => {
                walk.links = Links::All;
                index += 1;
            }
            "-depth" | "-d" => {
                walk.contents_first = true;
                index += 1;
            }
            // The command an action runs is its own, to its `;` or to a `+` after `{}`:
            // `-exec find {} -maxdepth 0 ;` lost its `-maxdepth 0` to this walk.
            "-exec" | "-execdir" | "-ok" | "-okdir" => {
                rest.push(args[index].clone());
                index += 1;
                while let Some(word) = args.get(index) {
                    rest.push(word.clone());
                    index += 1;
                    let after_braces = rest.len() >= 2 && rest[rest.len() - 2] == "{}";
                    if word == ";" || (word == "+" && after_braces) {
                        break;
                    }
                }
            }
            _ => {
                rest.push(args[index].clone());
                index += 1;
            }
        }
    }

    let mut parser = ExprParser {
        args: &rest,
        at: 0,
        here,
    };
    let expr = parser.parse_or()?;
    if parser.at < rest.len() {
        return Err(std::format!("unexpected `{}'", rest[parser.at]));
    }

    // `find` prints what it finds unless the expression already says what to do with it.
    let expr = if has_action(&expr) {
        expr
    } else {
        Expr::And(Box::new(expr), Box::new(Expr::Action(Action::Print)))
    };
    walk.contents_first |= expr.deletes();

    Ok((walk, expr))
}

/// Whether the expression already says what to do with a match. `-prune` does not count,
/// which is why `find . -name .git -prune` still prints.
fn has_action(expr: &Expr) -> bool {
    match expr {
        Expr::Action(Action::Prune) => false,
        Expr::Action(_) => true,
        Expr::Test(_) => false,
        Expr::Not(inner) => has_action(inner),
        Expr::And(left, right) | Expr::Or(left, right) => has_action(left) || has_action(right),
    }
}

/// Recursive descent over `find`'s expression grammar.
struct ExprParser<'a> {
    args: &'a [String],
    at: usize,
    /// The shell's working directory, for a file an expression names.
    here: &'a Path,
}

impl ExprParser<'_> {
    fn peek(&self) -> Option<&str> {
        self.args.get(self.at).map(String::as_str)
    }

    fn take(&mut self) -> Option<String> {
        let value = self.args.get(self.at).cloned();
        if value.is_some() {
            self.at += 1;
        }
        value
    }

    fn argument_to(&mut self, predicate: &str) -> Result<String, String> {
        self.take()
            .ok_or_else(|| std::format!("missing argument to `{predicate}'"))
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Some("-o" | "-or")) {
            self.at += 1;
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        loop {
            match self.peek() {
                Some("-a" | "-and") => {
                    self.at += 1;
                    let right = self.parse_unary()?;
                    left = Expr::And(Box::new(left), Box::new(right));
                }
                // Two predicates side by side are an implicit `-a`.
                Some(token) if token != "-o" && token != "-or" && token != ")" => {
                    let right = self.parse_unary()?;
                    left = Expr::And(Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some("!" | "-not") => {
                self.at += 1;
                Ok(Expr::Not(Box::new(self.parse_unary()?)))
            }
            Some("(") => {
                self.at += 1;
                let inner = self.parse_or()?;
                if self.peek() != Some(")") {
                    return Err(String::from("expected `)'"));
                }
                self.at += 1;
                Ok(inner)
            }
            Some(_) => self.parse_primary(),
            None => Ok(Expr::Test(Test::True)),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per predicate; splitting it would only move the list"
    )]
    fn parse_primary(&mut self) -> Result<Expr, String> {
        let Some(token) = self.take() else {
            return Ok(Expr::Test(Test::True));
        };

        match token.as_str() {
            // cash: matching is case-insensitive, so `-name` and `-iname` agree. D16 made
            // the same call for globbing, for the same reason: on a case-insensitive
            // volume a case-sensitive match can only produce false negatives, and
            // `-name '*.TXT'` finding nothing on a disk full of `.txt` is exactly that.
            "-name" | "-iname" => {
                let value = self.argument_to(&token)?;
                Ok(Expr::Test(Test::Name(
                    Pattern::from(value.as_str()).set_case_insensitive(true),
                    Subject::BaseName,
                )))
            }
            "-path" | "-ipath" | "-wholename" => {
                let value = self.argument_to(&token)?;
                Ok(Expr::Test(Test::Name(
                    Pattern::from(value.as_str()).set_case_insensitive(true),
                    Subject::WholePath,
                )))
            }
            "-type" => {
                let value = self.argument_to(&token)?;
                let kind = match value.as_str() {
                    "f" => Kind::File,
                    "d" => Kind::Directory,
                    "l" => Kind::Symlink,
                    other => {
                        return Err(std::format!(
                            "unsupported `-type {other}'; cash knows f, d and l"
                        ));
                    }
                };
                Ok(Expr::Test(Test::Type(kind)))
            }
            "-size" => {
                let value = self.argument_to(&token)?;
                let (compare, digits) = Compare::parse(value.as_str());
                let (digits, unit) = match digits.chars().last() {
                    Some(suffix) if suffix.is_ascii_alphabetic() => {
                        let prefix_len = digits.len().saturating_sub(suffix.len_utf8());
                        (digits.get(..prefix_len).unwrap_or(""), suffix)
                    }
                    // `find`'s default unit is a 512-byte block, which surprises everyone
                    // and is nevertheless what a script written against it expects.
                    _ => (digits, 'b'),
                };
                let unit_size = match unit {
                    'b' => 512,
                    'c' => 1,
                    'k' => 1024,
                    'M' => 1024 * 1024,
                    'G' => 1024 * 1024 * 1024,
                    other => return Err(std::format!("unknown size unit `{other}'")),
                };
                let count: u64 = digits
                    .parse()
                    .map_err(|_| std::format!("invalid size `{value}'"))?;
                Ok(Expr::Test(Test::Size(compare, count, unit_size)))
            }
            "-newer" => {
                let value = self.argument_to(&token)?;
                let file = self.here.join(cash_win32::path::accept_path(&value));
                let when = std::fs::metadata(&file)
                    .and_then(|m| m.modified())
                    .map_err(|e| std::format!("{value}: {e}"))?;
                Ok(Expr::Test(Test::Newer(when)))
            }
            name @ ("-mtime" | "-atime" | "-ctime" | "-mmin" | "-amin" | "-cmin") => {
                let value = self.argument_to(&token)?;
                let (compare, digits) = Compare::parse(value.as_str());
                let count: u64 = digits
                    .parse()
                    .map_err(|_| std::format!("invalid argument `{value}' to `{name}'"))?;
                let stamp = match name.as_bytes().get(1) {
                    Some(b'm') => Stamp::Modified,
                    Some(b'a') => Stamp::Accessed,
                    _ => Stamp::Changed,
                };
                let unit = if name.ends_with("min") { 60 } else { 86_400 };
                Ok(Expr::Test(Test::Age(stamp, compare, count, unit)))
            }
            "-empty" => Ok(Expr::Test(Test::Empty)),
            "-true" => Ok(Expr::Test(Test::True)),
            "-false" => Ok(Expr::Test(Test::False)),
            "-print" => Ok(Expr::Action(Action::Print)),
            "-print0" => Ok(Expr::Action(Action::Print0)),
            "-delete" => Ok(Expr::Action(Action::Delete)),
            "-prune" => Ok(Expr::Action(Action::Prune)),
            "-quit" => Ok(Expr::Action(Action::Quit)),
            name @ ("-exec" | "-execdir" | "-ok" | "-okdir") => {
                let mut argv = Vec::new();
                let mut batch = false;
                // Asking for each file and running for many at once do not go together:
                // after `-ok` a `+` is a word of the command, as in GNU `find`, which then
                // finds no `;`.
                let ask = name.starts_with("-ok");
                loop {
                    let Some(part) = self.take() else {
                        return Err(if ask {
                            std::format!("missing argument to `{name}'")
                        } else {
                            std::format!("missing terminator to `{name}'")
                        });
                    };
                    match part.as_str() {
                        ";" => break,
                        "+" if !ask => {
                            batch = true;
                            break;
                        }
                        _ => argv.push(part),
                    }
                }
                if argv.is_empty() {
                    return Err(std::format!("missing command to `{name}'"));
                }
                // With `+` the paths are words of their own, so `{}` must be one, and
                // once, as GNU `find` requires; `x{}y {}` ran `x{}y` as it was.
                if batch {
                    let holders = argv.iter().filter(|part| part.contains("{}")).count();
                    if holders > 1 {
                        return Err(std::format!(
                            "Only one instance of {{}} is supported with {name} ... +"
                        ));
                    }
                    if let Some(part) = argv
                        .iter()
                        .find(|part| part.contains("{}") && *part != "{}")
                    {
                        return Err(std::format!(
                            "In '{name} ... {{}} +' the '{{}}' must appear by itself, but you specified '{part}'"
                        ));
                    }
                }
                Ok(Expr::Action(Action::Exec(Exec {
                    argv,
                    batch,
                    in_dir: name.ends_with("dir"),
                    ask,
                })))
            }
            "-help" | "--help" => Err(HELP_ASKED.to_owned()),
            "-version" | "--version" => Err(VERSION_ASKED.to_owned()),
            other => Err(std::format!("unknown predicate `{other}'")),
        }
    }
}

/// The time `stamp` names of the entry visited; a link's own unless the walk follows it.
fn stamp_of(visit: &Visit<'_>, stamp: Stamp) -> Option<SystemTime> {
    let metadata = visit.metadata.as_ref()?;
    match stamp {
        Stamp::Modified => metadata.modified().ok(),
        Stamp::Accessed => metadata.accessed().ok(),
        Stamp::Changed => cash_win32::fs::file_facts(visit.path, !metadata.is_symlink())?.changed(),
    }
}

/// Evaluates the expression against one entry, performing any actions it reaches.
async fn evaluate<SE: cash_core::ShellExtensions>(
    expr: &Expr,
    visit: &mut Visit<'_>,
    context: &cash_core::ExecutionContext<'_, SE>,
    batches: &mut Batches,
) -> Result<bool, cash_core::Error> {
    match expr {
        Expr::Test(test) => Ok(matches(test, visit)),
        Expr::Not(inner) => Ok(!Box::pin(evaluate(inner, visit, context, batches)).await?),
        Expr::And(left, right) => {
            if Box::pin(evaluate(left, visit, context, batches)).await? {
                Box::pin(evaluate(right, visit, context, batches)).await
            } else {
                Ok(false)
            }
        }
        Expr::Or(left, right) => {
            if Box::pin(evaluate(left, visit, context, batches)).await? {
                Ok(true)
            } else {
                Box::pin(evaluate(right, visit, context, batches)).await
            }
        }
        Expr::Action(action) => act(action, visit, context, batches).await,
    }
}

/// Whether one condition holds for the entry being visited.
fn matches(test: &Test, visit: &Visit<'_>) -> bool {
    match test {
        Test::True => true,
        Test::False => false,
        Test::Name(pattern, subject) => {
            let value = match subject {
                Subject::BaseName => visit
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default(),
                Subject::WholePath => visit.rendered.clone(),
            };
            pattern.exactly_matches(value.as_str()).unwrap_or(false)
        }
        Test::Type(kind) => visit.metadata.as_ref().is_some_and(|m| match kind {
            Kind::File => m.is_file(),
            Kind::Directory => m.is_dir(),
            Kind::Symlink => m.is_symlink(),
        }),
        Test::Size(compare, count, unit) => visit.metadata.as_ref().is_some_and(|m| {
            // `find` rounds up: a 1-byte file is one block, and `-size -1k` therefore
            // matches nothing at all, as it does on Linux.
            let units = m.len().div_ceil(*unit);
            compare.holds(units, *count)
        }),
        Test::Newer(when) => visit
            .metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .is_some_and(|modified| modified > *when),
        Test::Age(stamp, compare, count, unit) => stamp_of(visit, *stamp)
            .and_then(|time| SystemTime::now().duration_since(time).ok())
            .is_some_and(|age| compare.holds(age.as_secs() / unit, *count)),
        Test::Empty => visit.metadata.as_ref().is_some_and(|m| {
            if m.is_dir() {
                std::fs::read_dir(visit.path).is_ok_and(|mut entries| entries.next().is_none())
            } else {
                m.len() == 0
            }
        }),
    }
}

/// Performs an action, returning what it contributes to the expression's value.
async fn act<SE: cash_core::ShellExtensions>(
    action: &Action,
    visit: &mut Visit<'_>,
    context: &cash_core::ExecutionContext<'_, SE>,
    batches: &mut Batches,
) -> Result<bool, cash_core::Error> {
    match action {
        Action::Print => {
            writeln!(context.stdout(), "{}", visit.rendered)?;
            Ok(true)
        }
        Action::Print0 => {
            write!(context.stdout(), "{}\0", visit.rendered)?;
            Ok(true)
        }
        Action::Prune => {
            visit.prune = true;
            Ok(true)
        }
        Action::Quit => {
            visit.quit = true;
            Ok(true)
        }
        Action::Delete => {
            let removed = if visit
                .metadata
                .as_ref()
                .is_some_and(std::fs::Metadata::is_dir)
            {
                std::fs::remove_dir(visit.path)
            } else {
                std::fs::remove_file(visit.path)
            };

            match removed {
                Ok(()) => Ok(true),
                Err(e) => {
                    writeln!(
                        context.stderr(),
                        "{}: cannot delete '{}': {}",
                        context.command_name,
                        visit.rendered,
                        cash_core::error::os_error_text(&e)
                    )?;
                    visit.failed = true;
                    Ok(false)
                }
            }
        }
        Action::Exec(exec) => {
            // `-execdir`: in the entry's own directory, the entry as `./name`.
            let (path, dir) = if exec.in_dir {
                let name = visit.path.file_name().map_or_else(
                    || ".".to_owned(),
                    |name| name.to_string_lossy().into_owned(),
                );
                let dir = match visit.path.parent() {
                    Some(parent) if visit.path.file_name().is_some() => {
                        if parent.as_os_str().is_empty() {
                            PathBuf::from(".")
                        } else {
                            parent.to_path_buf()
                        }
                    }
                    _ => visit.path.to_path_buf(),
                };
                (std::format!("./{name}"), Some(dir))
            } else {
                (visit.rendered.clone(), None)
            };
            if exec.batch {
                // Held until the walk ends, so one command sees every match (in each
                // directory, for `-execdir`).
                if let Some(slot) = batches
                    .iter_mut()
                    .find(|(held, held_dir, _)| *held == exec.argv && *held_dir == dir)
                {
                    slot.2.push(path);
                } else {
                    batches.push((exec.argv.clone(), dir, vec![path]));
                }
                return Ok(true);
            }
            if exec.ask && !confirmed(context, &exec.argv, &path)? {
                return Ok(false);
            }
            // A command that fails, or cannot start, makes the test false; it is not
            // find's failure, as in GNU `find`, whose status stays 0.
            run(
                context,
                &exec.argv,
                std::slice::from_ref(&path),
                dir.as_deref(),
            )
            .await
        }
    }
}

/// `-ok`: asks on standard error whether to run the command on `path`, as GNU `find`
/// asks, by the command's name (`< rm ... ./a.txt > ? `), and reads the answer, a line
/// of standard input; yes when it starts with `y` or `Y`. No answer is no.
fn confirmed<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    argv: &[String],
    path: &str,
) -> Result<bool, cash_core::Error> {
    let program = argv.first().map_or("", String::as_str);
    write!(context.stderr(), "< {program} ... {path} > ? ")?;
    let mut answer = Vec::new();
    let mut byte = [0u8; 1];
    let mut input = context.stdin();
    while matches!(std::io::Read::read(&mut input, &mut byte), Ok(1)) {
        if byte[0] == b'\n' {
            break;
        }
        answer.push(byte[0]);
    }
    let answer = String::from_utf8_lossy(&answer);
    Ok(answer.trim_start().starts_with(['y', 'Y']))
}

/// Runs one `-exec` command, substituting `{}` with the paths it was given, and says
/// whether it succeeded.
///
/// The paths are already rendered (D3), so what the child receives is the spelling cash
/// prints everywhere else rather than the one Windows stores. The shell runs the command
/// (`run_for_builtin`): found on its `PATH`, in its working directory, with its exported
/// variables, its output where find's goes. One that cannot be started is reported.
async fn run<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    argv: &[String],
    paths: &[String],
    dir: Option<&Path>,
) -> Result<bool, cash_core::Error> {
    // `-exec cmd \;` with no `{}` still runs once per match, as `find` does. With one
    // path, `{}` is replaced inside a word too, `"<{}>"`, as in GNU `find`; it was left.
    let mut parts: Vec<String> = Vec::new();
    for part in argv {
        if part == "{}" {
            parts.extend(paths.iter().cloned());
        } else if let [path] = paths {
            parts.push(part.replace("{}", path));
        } else {
            parts.push(part.clone());
        }
    }
    let Some(program) = parts.first() else {
        return Ok(false);
    };

    let ran = match dir {
        Some(dir) => {
            cash_core::commands::run_for_builtin_in(
                context.shell,
                context.params.clone(),
                &parts,
                dir,
            )
            .await
        }
        None => {
            cash_core::commands::run_for_builtin(context.shell, context.params.clone(), &parts)
                .await
        }
    };
    match ran {
        Ok(result) => Ok(result.is_success()),
        Err(e) => {
            writeln!(
                context.stderr(),
                "{}: '{program}': {}",
                context.command_name,
                crate::xargs::start_failure(&e)
            )?;
            Ok(false)
        }
    }
}
