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
use clap::Parser;

/// Parse command options for a script, util-linux style.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct GetoptCommand {
    /// getopt's own options, then the parameters to parse: parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Whether an option takes an argument.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Takes {
    Nothing,
    Required,
    Optional,
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

/// An option set, as `getopt_long` takes one.
struct Spec {
    shorts: Vec<(char, Takes)>,
    longs: Vec<(String, Takes)>,
    order: Order,
    long_only: bool,
}

/// One thing found on the command line.
enum Item {
    /// An option by its canonical name (`-a`, `--alpha`) and its argument, if it takes one.
    Opt(String, Option<String>),
    /// A non-option.
    Word(String),
}

/// A command line as `getopt_long` would see it: the options and non-options found, in
/// order, the non-options left after the options end, and the errors on the way.
struct Parsed {
    items: Vec<Item>,
    rest: Vec<String>,
    errors: Vec<String>,
}

impl Spec {
    fn short(&self, c: char) -> Option<Takes> {
        self.shorts.iter().find(|(s, _)| *s == c).map(|(_, t)| *t)
    }

    /// The long option `name` means: exact, or else a unique prefix.
    fn long(&self, name: &str) -> Result<(String, Takes), Vec<String>> {
        if let Some((n, t)) = self.longs.iter().find(|(n, _)| n == name) {
            return Ok((n.clone(), *t));
        }
        let mut candidates: Vec<&(String, Takes)> = self
            .longs
            .iter()
            .filter(|(n, _)| n.starts_with(name))
            .collect();
        candidates.dedup_by(|a, b| a.0 == b.0);
        match candidates.as_slice() {
            [(n, t)] => Ok((n.clone(), *t)),
            [] => Err(Vec::new()),
            many => Err(many.iter().map(|(n, _)| n.clone()).collect()),
        }
    }

    /// Parses `args` as `getopt_long` does.
    fn parse(&self, args: &[String]) -> Parsed {
        let mut parsed = Parsed {
            items: Vec::new(),
            rest: Vec::new(),
            errors: Vec::new(),
        };
        let mut index = 0;
        while let Some(arg) = args.get(index) {
            index += 1;
            if arg == "--" {
                parsed.rest.extend(args.iter().skip(index).cloned());
                break;
            }
            let is_option = arg.len() > 1 && arg.starts_with('-');
            if !is_option {
                match self.order {
                    Order::Permute => parsed.rest.push(arg.clone()),
                    Order::InPlace => parsed.items.push(Item::Word(arg.clone())),
                    Order::Stop => {
                        parsed.rest.extend(args.iter().skip(index - 1).cloned());
                        break;
                    }
                }
                continue;
            }
            if let Some(long) = arg.strip_prefix("--") {
                self.parse_long("--", long, args, &mut index, &mut parsed);
                continue;
            }
            let body = arg.get(1..).unwrap_or_default();
            if self.long_only {
                let name = body.split_once('=').map_or(body, |(n, _)| n);
                let first_is_short = body.chars().next().and_then(|c| self.short(c)).is_some();
                // `getopt_long_only`: a long option if one matches, unless the word is a
                // single short option; otherwise short options.
                let matches_long = self.long(name).is_ok() && !(body.len() == 1 && first_is_short);
                if matches_long || !first_is_short {
                    self.parse_long("-", body, args, &mut index, &mut parsed);
                    continue;
                }
            }
            self.parse_shorts(body, args, &mut index, &mut parsed);
        }
        parsed
    }

    fn parse_long(
        &self,
        prefix: &str,
        text: &str,
        args: &[String],
        index: &mut usize,
        parsed: &mut Parsed,
    ) {
        let (name, inline) = text
            .split_once('=')
            .map_or((text, None), |(n, v)| (n, Some(v.to_owned())));
        match self.long(name) {
            Ok((full, takes)) => {
                let canonical = std::format!("--{full}");
                match (takes, inline) {
                    (Takes::Nothing, Some(_)) => parsed.errors.push(std::format!(
                        "option '{prefix}{full}' doesn't allow an argument"
                    )),
                    (Takes::Nothing, None) => parsed.items.push(Item::Opt(canonical, None)),
                    (_, Some(value)) => parsed.items.push(Item::Opt(canonical, Some(value))),
                    (Takes::Optional, None) => {
                        parsed.items.push(Item::Opt(canonical, Some(String::new())));
                    }
                    (Takes::Required, None) => {
                        if let Some(value) = args.get(*index) {
                            *index += 1;
                            parsed.items.push(Item::Opt(canonical, Some(value.clone())));
                        } else {
                            parsed
                                .errors
                                .push(std::format!("option '{prefix}{full}' requires an argument"));
                        }
                    }
                }
            }
            Err(possibilities) if possibilities.is_empty() => {
                parsed
                    .errors
                    .push(std::format!("unrecognized option '{prefix}{text}'"));
            }
            Err(possibilities) => {
                let listed: Vec<String> = possibilities
                    .iter()
                    .map(|p| std::format!("'{prefix}{p}'"))
                    .collect();
                parsed.errors.push(std::format!(
                    "option '{prefix}{name}' is ambiguous; possibilities: {}",
                    listed.join(" ")
                ));
            }
        }
    }

    fn parse_shorts(&self, body: &str, args: &[String], index: &mut usize, parsed: &mut Parsed) {
        for (at, c) in body.char_indices() {
            let rest = body.get(at + c.len_utf8()..).unwrap_or_default();
            match self.short(c) {
                None => parsed.errors.push(std::format!("invalid option -- '{c}'")),
                Some(Takes::Nothing) => parsed.items.push(Item::Opt(std::format!("-{c}"), None)),
                Some(Takes::Optional) => {
                    parsed
                        .items
                        .push(Item::Opt(std::format!("-{c}"), Some(rest.to_owned())));
                    return;
                }
                Some(Takes::Required) => {
                    if !rest.is_empty() {
                        parsed
                            .items
                            .push(Item::Opt(std::format!("-{c}"), Some(rest.to_owned())));
                    } else if let Some(value) = args.get(*index) {
                        *index += 1;
                        parsed
                            .items
                            .push(Item::Opt(std::format!("-{c}"), Some(value.clone())));
                    } else {
                        parsed
                            .errors
                            .push(std::format!("option requires an argument -- '{c}'"));
                    }
                    return;
                }
            }
        }
    }
}

/// Short options from an option string, with its leading `+`/`-` and `:` taken off; the
/// returned order and silence come from those.
fn short_options(optstring: &str) -> (Vec<(char, Takes)>, Option<Order>, bool) {
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
    let mut shorts = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        i += 1;
        if c == ':' {
            continue;
        }
        let takes = match (chars.get(i), chars.get(i + 1)) {
            (Some(':'), Some(':')) => Takes::Optional,
            (Some(':'), _) => Takes::Required,
            _ => Takes::Nothing,
        };
        shorts.push((c, takes));
    }
    (shorts, order, silent)
}

/// Long options from `-l` lists: separated by commas or white space, `name:` taking an
/// argument and `name::` an optional one.
fn long_options(lists: &[String]) -> Vec<(String, Takes)> {
    lists
        .iter()
        .flat_map(|list| list.split(|c: char| c == ',' || c.is_whitespace()))
        .filter(|name| !name.is_empty())
        .map(|name| {
            if let Some(n) = name.strip_suffix("::") {
                (n.to_owned(), Takes::Optional)
            } else if let Some(n) = name.strip_suffix(':') {
                (n.to_owned(), Takes::Required)
            } else {
                (name.to_owned(), Takes::Nothing)
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
            Item::Opt(name, value) => {
                line.push_str(name);
                if let Some(value) = value {
                    line.push(' ');
                    line.push_str(&word(value));
                }
            }
            Item::Word(text) => line.push_str(&word(text)),
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
    let spec = Spec {
        shorts: vec![
            ('a', Takes::Nothing),
            ('h', Takes::Nothing),
            ('l', Takes::Required),
            ('n', Takes::Required),
            ('o', Takes::Required),
            ('q', Takes::Nothing),
            ('Q', Takes::Nothing),
            ('s', Takes::Required),
            ('T', Takes::Nothing),
            ('u', Takes::Nothing),
            ('V', Takes::Nothing),
        ],
        longs: [
            ("alternative", Takes::Nothing),
            ("help", Takes::Nothing),
            ("longoptions", Takes::Required),
            ("name", Takes::Required),
            ("options", Takes::Required),
            ("quiet", Takes::Nothing),
            ("quiet-output", Takes::Nothing),
            ("shell", Takes::Required),
            ("test", Takes::Nothing),
            ("unquoted", Takes::Nothing),
            ("version", Takes::Nothing),
        ]
        .iter()
        .map(|(n, t)| ((*n).to_owned(), *t))
        .collect(),
        order: Order::Stop,
        long_only: false,
    };
    let parsed = spec.parse(args);
    if let Some(error) = parsed.errors.into_iter().next() {
        return Err(error);
    }
    let mut own = Own {
        params: parsed.rest,
        ..Own::default()
    };
    for item in parsed.items {
        let Item::Opt(name, value) = item else {
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
        let spec = Spec {
            shorts,
            longs: long_options(&own.longs),
            order: order.unwrap_or(if posixly { Order::Stop } else { Order::Permute }),
            long_only: own.alternative,
        };
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
