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
use clap::Parser;

/// Search a directory tree for files matching an expression.
#[derive(Parser)]
pub(crate) struct FindCommand {
    /// Starting points, then the expression — parsed here rather than by clap, because
    /// `find`'s grammar is an expression language, not a list of options.
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    args: Vec<String>,
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

/// A condition on one entry.
enum Test {
    Name(Pattern, Subject),
    Type(Kind),
    /// `-size`, in whatever unit was given, rounded the way `find` rounds: up.
    Size(Compare, u64, u64),
    Newer(SystemTime),
    /// `-mtime`, in days.
    MTime(Compare, u64),
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
    Exec(Vec<String>, bool),
    Prune,
    Quit,
}

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

/// Adds a folder to the ones above what is under it; if it is one of them already, how
/// that one was printed, for the loop it closes.
fn enter_folder(
    actual: &Path,
    path: &Path,
    ancestors: &mut Vec<((u32, u64), String)>,
) -> Option<String> {
    let info = cash_win32::fs::file_info(actual).ok()?;
    let identity = (info.volume, info.index);
    if let Some((_, first)) = ancestors.iter().find(|(seen, _)| *seen == identity) {
        return Some(first.clone());
    }
    ancestors.push((identity, render(path)));
    None
}

/// An entry waiting on the walk's stack.
struct Entry {
    /// The path to print, as the caller spelled it.
    path: PathBuf,
    /// Where it actually is.
    actual: PathBuf,
    depth: usize,
    /// A directory whose contents have been walked, in a contents-first walk.
    contents_done: bool,
    /// Under `-L`, the folders above it: their identity and how they were printed.
    ancestors: Vec<((u32, u64), String)>,
}

/// The options that apply to the whole walk rather than to one entry.
struct Walk {
    starts: Vec<String>,
    min_depth: usize,
    max_depth: usize,
    follow: bool,
    /// `-depth`, or implied by `-delete`: a directory after what is in it, as GNU `find`
    /// does. `-delete` went the other way, so it never removed a directory: it was not
    /// empty yet when its turn came (BI-04).
    contents_first: bool,
}

impl builtins::Command for FindCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let (walk, expr) = match parse(&self.args, context.shell.working_dir()) {
            Ok(parsed) => parsed,
            Err(complaint) => {
                writeln!(context.stderr(), "{}: {complaint}", context.command_name)?;
                return Ok(ExecutionResult::general_error());
            }
        };

        let mut failed = false;
        let mut batches: Vec<(Vec<String>, Vec<String>)> = Vec::new();

        'walking: for start in &walk.starts {
            // Two paths per entry: the one to print, spelled as the caller asked, and the
            // one to open, which is where that actually is. cash: the shell's working
            // directory is its own (D3/D10) — `cd` updates what the shell believes without
            // moving the process — so a start is resolved by the shell. Reading `.` from
            // the process's directory made `cd crates; find .` walk the repository root.
            let shown = PathBuf::from(start);
            let actual = context.shell.absolute_path(Path::new(start));
            // An entry is visited when it comes off the stack, unless the walk is
            // contents-first and it is a directory: it then goes back on, done, under its
            // contents, and is visited when it comes off again.
            let mut stack = vec![Entry {
                path: shown,
                actual,
                depth: 0,
                contents_done: false,
                ancestors: Vec::new(),
            }];

            while let Some(entry) = stack.pop() {
                let Entry {
                    path,
                    actual,
                    depth,
                    contents_done,
                    mut ancestors,
                } = entry;
                let metadata = if walk.follow {
                    std::fs::metadata(&actual).ok()
                } else {
                    std::fs::symlink_metadata(&actual).ok()
                };
                let is_dir = metadata.as_ref().is_some_and(std::fs::Metadata::is_dir);
                let descends = is_dir && depth < walk.max_depth;

                // Following links, a folder can be its own ancestor; it is said and left,
                // as GNU `find` does. The walk went round it without end (BI-13).
                if walk.follow
                    && is_dir
                    && !contents_done
                    && let Some(first) = enter_folder(&actual, &path, &mut ancestors)
                {
                    writeln!(
                        context.stderr(),
                        "{}: File system loop detected; '{}' is part of the same file system loop as '{first}'.",
                        context.command_name,
                        render(path.as_path())
                    )?;
                    failed = true;
                    continue;
                }

                if walk.contents_first && descends && !contents_done {
                    stack.push(Entry {
                        path: path.clone(),
                        actual: actual.clone(),
                        depth,
                        contents_done: true,
                        ancestors: ancestors.clone(),
                    });
                    if !push_contents(&context, &mut stack, &path, &actual, depth, &ancestors)? {
                        failed = true;
                    }
                    continue;
                }

                let mut visit = Visit {
                    path: actual.as_path(),
                    rendered: render(path.as_path()),
                    metadata,
                    prune: false,
                    quit: false,
                    failed: false,
                };

                if depth >= walk.min_depth {
                    evaluate(&expr, &mut visit, &context, &mut batches).await?;
                }

                failed |= visit.failed;
                if visit.quit {
                    break 'walking;
                }

                if walk.contents_first || !descends || visit.prune {
                    continue;
                }
                if !push_contents(&context, &mut stack, &path, &actual, depth, &ancestors)? {
                    failed = true;
                }
            }
        }

        // `-exec cmd {} +` holds its arguments until the walk is over, and runs as often
        // as a Windows command line needs to hold them all; it ran once, and a long list
        // failed to start (BI-11). A run that fails fails find, as in GNU `find`.
        for (argv, collected) in batches {
            for chunk in command_line_chunks(&argv, &collected) {
                if !run(&context, &argv, chunk).await? {
                    failed = true;
                }
            }
        }

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

/// Puts what is in a directory on the walk's stack, in name order; whether it could be
/// read, with the reason said if not.
fn push_contents<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    stack: &mut Vec<Entry>,
    path: &Path,
    actual: &Path,
    depth: usize,
    ancestors: &[((u32, u64), String)],
) -> Result<bool, cash_core::Error> {
    match std::fs::read_dir(actual) {
        Ok(entries) => {
            // Reversed, because the stack hands them back in reverse — the listing then
            // comes out in name order.
            let mut names: Vec<std::ffi::OsString> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name())
                .collect();
            names.sort();
            for name in names.into_iter().rev() {
                stack.push(Entry {
                    path: path.join(&name),
                    actual: actual.join(&name),
                    depth: depth + 1,
                    contents_done: false,
                    ancestors: ancestors.to_vec(),
                });
            }
            Ok(true)
        }
        Err(e) => {
            writeln!(
                context.stderr(),
                "{}: {}: {e}",
                context.command_name,
                render(path)
            )?;
            Ok(false)
        }
    }
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
        follow: false,
        contents_first: false,
    };

    let mut index = 0;

    // Anything before the first predicate is a starting point — except the `-H`/`-L`/`-P`
    // options, which `find` takes before them.
    while index < args.len() {
        match args[index].as_str() {
            "-L" | "-follow" => {
                walk.follow = true;
                index += 1;
            }
            "-P" | "-H" => index += 1,
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
                walk.follow = true;
                index += 1;
            }
            "-depth" | "-d" => {
                walk.contents_first = true;
                index += 1;
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

    #[allow(
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
            "-mtime" => {
                let value = self.argument_to(&token)?;
                let (compare, digits) = Compare::parse(value.as_str());
                let days: u64 = digits
                    .parse()
                    .map_err(|_| std::format!("invalid argument `{value}' to `-mtime'"))?;
                Ok(Expr::Test(Test::MTime(compare, days)))
            }
            "-empty" => Ok(Expr::Test(Test::Empty)),
            "-true" => Ok(Expr::Test(Test::True)),
            "-false" => Ok(Expr::Test(Test::False)),
            "-print" => Ok(Expr::Action(Action::Print)),
            "-print0" => Ok(Expr::Action(Action::Print0)),
            "-delete" => Ok(Expr::Action(Action::Delete)),
            "-prune" => Ok(Expr::Action(Action::Prune)),
            "-quit" => Ok(Expr::Action(Action::Quit)),
            "-exec" => {
                let mut argv = Vec::new();
                let mut batch = false;
                loop {
                    let Some(part) = self.take() else {
                        return Err(String::from("missing terminator to `-exec'"));
                    };
                    match part.as_str() {
                        ";" => break,
                        "+" => {
                            batch = true;
                            break;
                        }
                        _ => argv.push(part),
                    }
                }
                if argv.is_empty() {
                    return Err(String::from("missing command to `-exec'"));
                }
                // With `+` the paths are words of their own, so `{}` must be one, and
                // once, as GNU `find` requires; `x{}y {}` ran `x{}y` as it was.
                if batch {
                    let holders = argv.iter().filter(|part| part.contains("{}")).count();
                    if holders > 1 {
                        return Err(String::from(
                            "Only one instance of {} is supported with -exec ... +",
                        ));
                    }
                    if let Some(part) = argv
                        .iter()
                        .find(|part| part.contains("{}") && *part != "{}")
                    {
                        return Err(std::format!(
                            "In '-exec ... {{}} +' the '{{}}' must appear by itself, but you specified '{part}'"
                        ));
                    }
                }
                Ok(Expr::Action(Action::Exec(argv, batch)))
            }
            other => Err(std::format!(
                "unknown predicate `{other}'. cash's find carries the predicates scripts \
                 use — a listing filtered by a predicate it had ignored would be wrong \
                 without saying so"
            )),
        }
    }
}

/// Evaluates the expression against one entry, performing any actions it reaches.
async fn evaluate<SE: cash_core::ShellExtensions>(
    expr: &Expr,
    visit: &mut Visit<'_>,
    context: &cash_core::ExecutionContext<'_, SE>,
    batches: &mut Vec<(Vec<String>, Vec<String>)>,
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
        Test::MTime(compare, days) => visit
            .metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| compare.holds(age.as_secs() / 86_400, *days)),
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
    batches: &mut Vec<(Vec<String>, Vec<String>)>,
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
        Action::Exec(argv, batch) => {
            if *batch {
                // Held until the walk ends, so one command sees every match.
                if let Some(slot) = batches.iter_mut().find(|(held, _)| held == argv) {
                    slot.1.push(visit.rendered.clone());
                } else {
                    batches.push((argv.clone(), vec![visit.rendered.clone()]));
                }
                return Ok(true);
            }

            // A command that fails, or cannot start, makes the test false; it is not
            // find's failure, as in GNU `find`, whose status stays 0.
            run(context, argv, std::slice::from_ref(&visit.rendered)).await
        }
    }
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

    match cash_core::commands::run_for_builtin(context.shell, context.params.clone(), &parts).await
    {
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
