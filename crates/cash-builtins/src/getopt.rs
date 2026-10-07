//! `getopt`, util-linux's: parses a command line for a script and prints it back in a
//! canonical, shell-quoted form for `eval set -- "$(getopt …)"`.
//!
//! Checked against util-linux 2.42.3, case by case (`crates/cash/tests/oracle`). The
//! parsing is GNU `getopt_long`'s: clustered short options, `--name=value` and unique
//! prefixes of long options, `-a` for long options with one dash, and non-options moved
//! after `--` unless the option string starts with `+` (or `POSIXLY_CORRECT` is set) to
//! stop at the first one, or with `-` to keep them in place. Errors are reported and
//! parsing goes on, as util-linux does, so every bad option is named.
//!
//! The output is single-quoted, which cash's parser reads exactly as bash does, and a
//! `winpaths` word (D53) never arises inside quotes. `-s csh` and `-s tcsh` are refused:
//! cash is not a C shell, and their quoting (`!`, newlines) means nothing to it.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use cash_getopt::{Arg, Getopt, Item, Long, Short};
use clap::Parser;

/// Parse command options for a script, util-linux style.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct GetoptCommand {
    /// getopt's own options, then the parameters to parse: parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// What to do with the first non-option.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    /// Move non-options after the options (GNU's default).
    Permute,
    /// Stop at the first non-option (`+`, or `POSIXLY_CORRECT`).
    Stop,
    /// Report non-options in place (`-`).
    InPlace,
}

/// An option set, as `getopt_long` takes one, read by `cash-getopt`: each option known
/// by its index in `names`, the canonical name getopt prints (`-a`, `--alpha`).
struct Spec {
    names: Vec<String>,
    kinds: Vec<Arg>,
    shorts: Vec<Short<usize>>,
    /// The long options' names, without their dashes, and their ids.
    longs: Vec<(String, usize)>,
    order: Order,
    long_only: bool,
}

/// One thing found on the command line, as getopt prints it.
enum Said {
    /// An option by its canonical name (`-a`, `--alpha`) and its argument, if it takes one.
    Opt(String, Option<String>),
    /// A non-option.
    Word(String),
}

/// A command line as `getopt_long` would see it: the options and non-options found, in
/// order, the non-options left after the options end, and the errors on the way.
struct Parsed {
    items: Vec<Said>,
    rest: Vec<String>,
    errors: Vec<String>,
}

impl Spec {
    /// The option set of an option string's letters and the `-l` lists' names.
    fn new(shorts: &[Short<char>], longs: &[(String, Arg)], order: Order, long_only: bool) -> Self {
        let mut spec = Self {
            names: Vec::new(),
            kinds: Vec::new(),
            shorts: Vec::new(),
            longs: Vec::new(),
            order,
            long_only,
        };
        for short in shorts {
            let id = spec.add(std::format!("-{}", short.letter), short.arg);
            spec.shorts.push(Short::new(short.letter, short.arg, id));
        }
        for (name, arg) in longs {
            // A name listed twice is one option.
            if spec.longs.iter().any(|(listed, _)| listed == name) {
                continue;
            }
            let id = spec.add(std::format!("--{name}"), *arg);
            spec.longs.push((name.clone(), id));
        }
        spec
    }

    fn add(&mut self, name: String, arg: Arg) -> usize {
        self.names.push(name);
        self.kinds.push(arg);
        self.names.len() - 1
    }

    fn kind(&self, id: usize) -> Arg {
        self.kinds.get(id).copied().unwrap_or(Arg::No)
    }

    /// Parses `args` as `getopt_long` does, past every error, as util-linux's getopt.
    fn parse(&self, args: &[String]) -> Parsed {
        let longs: Vec<Long<'_, usize>> = self
            .longs
            .iter()
            .map(|(name, id)| Long::new(name.as_str(), self.kind(*id), *id))
            .collect();
        let order = if self.order == Order::Stop {
            cash_getopt::Order::StopAtOperand
        } else {
            cash_getopt::Order::Permute
        };
        let (read, problems) = Getopt::new(&self.shorts, &longs)
            .order(order)
            .long_only(self.long_only)
            .parse_all(args);
        let mut parsed = Parsed {
            items: Vec::new(),
            rest: Vec::new(),
            errors: problems.iter().map(ToString::to_string).collect(),
        };
        for (at, item) in read.items.into_iter().enumerate() {
            match item {
                Item::Option { id, value, .. } => {
                    let name = self.names.get(id).cloned().unwrap_or_default();
                    // An optional argument is printed even when it is empty: `-c ''`.
                    let value = if self.kind(id) == Arg::Optional {
                        Some(value.unwrap_or_default())
                    } else {
                        value
                    };
                    parsed.items.push(Said::Opt(name, value));
                }
                Item::Operand { value, .. } => {
                    if self.order == Order::InPlace && at < read.options_end {
                        parsed.items.push(Said::Word(value));
                    } else {
                        parsed.rest.push(value);
                    }
                }
            }
        }
        parsed
    }
}

/// Short options from an option string, with its leading `+`/`-` and `:` taken off; the
/// returned order and silence come from those.
fn short_options(optstring: &str) -> (Vec<Short<char>>, Option<Order>, bool) {
    let mut text = optstring;
    let order = match text.chars().next() {
        Some('+') => Some(Order::Stop),
        Some('-') => Some(Order::InPlace),
        _ => None,
    };
    if order.is_some() {
        text = text.get(1..).unwrap_or_default();
    }
    let silent = text.starts_with(':');
    (cash_getopt::optstring(text), order, silent)
}

/// Long options from `-l` lists: separated by commas or white space, `name:` taking an
/// argument and `name::` an optional one.
fn long_options(lists: &[String]) -> Vec<(String, Arg)> {
    lists
        .iter()
        .flat_map(|list| list.split(|c: char| c == ',' || c.is_whitespace()))
        .filter(|name| !name.is_empty())
        .map(|name| {
            if let Some(n) = name.strip_suffix("::") {
                (n.to_owned(), Arg::Optional)
            } else if let Some(n) = name.strip_suffix(':') {
                (n.to_owned(), Arg::Required)
            } else {
                (name.to_owned(), Arg::No)
            }
        })
        .collect()
}

/// `text` single-quoted for a POSIX shell.
fn quote(text: &str) -> String {
    std::format!("'{}'", text.replace('\'', r"'\''"))
}

/// The canonical command line getopt prints: options and their arguments, `--`, then
/// the non-options, each quoted unless `unquoted`.
fn render(parsed: &Parsed, unquoted: bool) -> String {
    let word = |text: &str| {
        if unquoted {
            text.to_owned()
        } else {
            quote(text)
        }
    };
    let mut line = String::new();
    for item in &parsed.items {
        line.push(' ');
        match item {
            Said::Opt(name, value) => {
                line.push_str(name);
                if let Some(value) = value {
                    line.push(' ');
                    line.push_str(&word(value));
                }
            }
            Said::Word(text) => line.push_str(&word(text)),
        }
    }
    line.push_str(" --");
    for text in &parsed.rest {
        line.push(' ');
        line.push_str(&word(text));
    }
    line
}

/// getopt's own options, parsed.
#[derive(Default)]
struct Own {
    optstring: Option<String>,
    longs: Vec<String>,
    name: Option<String>,
    alternative: bool,
    quiet: bool,
    quiet_output: bool,
    unquoted: bool,
    test: bool,
    help: bool,
    version: bool,
    params: Vec<String>,
}

const HINT: &str = "Try 'getopt --help' for more information.";

const USAGE: &str = "\
Usage:
 getopt <optstring> <parameters>
 getopt [options] [--] <optstring> <parameters>
 getopt [options] -o|--options <optstring> [options] [--] <parameters>

Parse command options.

Options:
 -a, --alternative             allow long options starting with single -
 -l, --longoptions <longopts>  the long options to be recognized
 -n, --name <progname>         the name under which errors are reported
 -o, --options <optstring>     the short options to be recognized
 -q, --quiet                   disable error reporting by getopt(3)
 -Q, --quiet-output            no normal output
 -s, --shell <shell>           set quoting conventions to those of <shell>
 -T, --test                    test for getopt(1) version
 -u, --unquoted                do not quote the output

 -h, --help                    display this help
 -V, --version                 display version";

/// Parses getopt's own options, which stop at the first non-option.
fn parse_own(args: &[String]) -> Result<Own, String> {
    let longs: Vec<(String, Arg)> = [
        ("alternative", Arg::No),
        ("help", Arg::No),
        ("longoptions", Arg::Required),
        ("name", Arg::Required),
        ("options", Arg::Required),
        ("quiet", Arg::No),
        ("quiet-output", Arg::No),
        ("shell", Arg::Required),
        ("test", Arg::No),
        ("unquoted", Arg::No),
        ("version", Arg::No),
    ]
    .iter()
    .map(|(name, arg)| ((*name).to_owned(), *arg))
    .collect();
    let spec = Spec::new(
        &cash_getopt::optstring("ahl:n:o:qQs:TuV"),
        &longs,
        Order::Stop,
        false,
    );
    let parsed = spec.parse(args);
    if let Some(error) = parsed.errors.into_iter().next() {
        return Err(error);
    }
    let mut own = Own {
        params: parsed.rest,
        ..Own::default()
    };
    for item in parsed.items {
        let Said::Opt(name, value) = item else {
            continue;
        };
        let value = value.unwrap_or_default();
        match name.as_str() {
            "-a" | "--alternative" => own.alternative = true,
            "-h" | "--help" => own.help = true,
            "-l" | "--longoptions" => own.longs.push(value),
            "-n" | "--name" => own.name = Some(value),
            "-o" | "--options" => own.optstring = Some(value),
            "-q" | "--quiet" => own.quiet = true,
            "-Q" | "--quiet-output" => own.quiet_output = true,
            "-s" | "--shell" => match value.as_str() {
                "sh" | "bash" => {}
                "csh" | "tcsh" => {
                    return Err(std::format!(
                        "-s {value} is not supported: cash is not a C shell"
                    ));
                }
                _ => return Err("unknown shell after -s or --shell argument".to_owned()),
            },
            "-T" | "--test" => own.test = true,
            "-u" | "--unquoted" => own.unquoted = true,
            "-V" | "--version" => own.version = true,
            _ => {}
        }
    }
    Ok(own)
}

impl builtins::Command for GetoptCommand {
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
        // The traditional form: `getopt optstring parameters…`, unquoted output.
        let traditional = self
            .args
            .first()
            .is_some_and(|first| !first.starts_with('-'));
        let mut own = if traditional {
            Own {
                params: self.args.clone(),
                unquoted: true,
                ..Own::default()
            }
        } else {
            match parse_own(&self.args) {
                Ok(own) => own,
                Err(message) => {
                    writeln!(context.stderr(), "getopt: {message}\n{HINT}")?;
                    return Ok(ExecutionResult::new(2));
                }
            }
        };

        if own.help {
            writeln!(context.stdout(), "{USAGE}")?;
            return Ok(ExecutionResult::success());
        }
        if own.version {
            writeln!(
                context.stdout(),
                "getopt (cash), compatible with util-linux 2.42"
            )?;
            return Ok(ExecutionResult::success());
        }
        if own.test {
            // "An enhanced getopt": what scripts test for before relying on long options.
            return Ok(ExecutionResult::new(4));
        }
        let optstring = match own.optstring.take() {
            Some(optstring) => optstring,
            None if own.params.is_empty() => {
                writeln!(
                    context.stderr(),
                    "getopt: missing optstring argument\n{HINT}"
                )?;
                return Ok(ExecutionResult::new(2));
            }
            None => own.params.remove(0),
        };

        let (shorts, order, silent) = short_options(&optstring);
        let posixly = context
            .shell
            .env()
            .get_str("POSIXLY_CORRECT", context.shell)
            .is_some();
        let spec = Spec::new(
            &shorts,
            &long_options(&own.longs),
            order.unwrap_or(if posixly { Order::Stop } else { Order::Permute }),
            own.alternative,
        );
        let parsed = spec.parse(&own.params);

        if !own.quiet && !silent {
            let name = own.name.as_deref().unwrap_or("getopt");
            for error in &parsed.errors {
                writeln!(context.stderr(), "{name}: {error}")?;
            }
        }
        if !own.quiet_output {
            writeln!(context.stdout(), "{}", render(&parsed, own.unquoted))?;
        }

        Ok(if parsed.errors.is_empty() {
            ExecutionResult::success()
        } else {
            ExecutionResult::general_error()
        })
    }
}
