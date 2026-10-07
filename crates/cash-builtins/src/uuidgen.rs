//! `uuidgen`, util-linux's: a new UUID of the version asked for.
//!
//! Checked against util-linux 2.42.3 (`crates/cash/tests/oracle/uuidgen_cases.sh`): the
//! options as `getopt_long` reads them (clusters, abbreviated long options), which may be
//! combined, the error texts, and the hash-based UUIDs (`-m`, `-s`) byte for byte. The
//! random (`-r`, the default) and time-based (`-t`, `-6`, `-7`) values are checked for
//! their shape. Without a network card's address to read, util-linux makes a time-based
//! UUID's node from random bytes with the multicast bit set, and so does this; `-C` with
//! `-t` or `-6` gives UUIDs in sequence, from one clock.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use cash_getopt::{Arg, Getopt, Item, Long, Short};
use clap::Parser;
use uuid::Uuid;

/// The options, as util-linux lists them; `help uuidgen` and `--help` print this.
const OPTIONS: &str = "\
Options:
 -r, --random          generate random-based uuid
 -t, --time            generate time-based uuid
 -n, --namespace <ns>  generate hash-based uuid in this namespace
                        available namespaces: @dns @url @oid @x500
 -N, --name <name>     generate hash-based uuid from this name
 -m, --md5             generate md5 hash
 -C, --count <num>     generate more uuids in loop
 -s, --sha1            generate sha1 hash
 -6, --time-v6         generate time-based v6 uuid
 -7, --time-v7         generate time-based v7 uuid
 -x, --hex             interpret name as hex string

 -h, --help          display this help
 -V, --version       display version
";

/// Create a new UUID value.
#[derive(Parser)]
#[command(
    disable_help_flag = true,
    disable_version_flag = true,
    override_usage = "uuidgen [options]",
    after_help = OPTIONS
)]
pub(crate) struct UuidgenCommand {
    /// The options, read here as util-linux reads them.
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "OPTION"
    )]
    args: Vec<String>,
}

/// The long options in util-linux's order (the order an ambiguity lists them in), each
/// known by its letter.
const LONG_OPTIONS: &[Long<'static, char>] = &[
    Long::new("random", Arg::No, 'r'),
    Long::new("time", Arg::No, 't'),
    Long::new("version", Arg::No, 'V'),
    Long::new("help", Arg::No, 'h'),
    Long::new("namespace", Arg::Required, 'n'),
    Long::new("name", Arg::Required, 'N'),
    Long::new("md5", Arg::No, 'm'),
    Long::new("count", Arg::Required, 'C'),
    Long::new("sha1", Arg::No, 's'),
    Long::new("time-v6", Arg::No, '6'),
    Long::new("time-v7", Arg::No, '7'),
    Long::new("hex", Arg::No, 'x'),
];

/// The options that cannot be combined, a row each; the first one given of a row is
/// named with the next one given.
const EXCLUSIVE: &[&str] = &["67mrst", "Cms", "Nrt", "nrt"];

/// The long name of the option `letter`.
fn long_name(letter: char) -> &'static str {
    LONG_OPTIONS
        .iter()
        .find(|long| long.id == letter)
        .map_or("", |long| long.name)
}

/// What the options asked for.
#[derive(Debug, Default, PartialEq, Eq)]
struct Request {
    /// `r`, `t`, `m`, `s`, `6` or `7`; random without one.
    kind: Option<char>,
    /// `-n`.
    namespace: Option<String>,
    /// `-N`.
    name: Option<String>,
    /// `-x`.
    hex: bool,
    /// `-C`.
    count: Option<u32>,
}

/// What the options came to.
#[derive(Debug, PartialEq, Eq)]
enum Parsed {
    /// `-h`: the help, and nothing else.
    Help,
    /// `-V`: the version, and nothing else.
    Version,
    /// A UUID to make.
    Run(Request),
}

/// A refusal, as util-linux words it; `hint` adds its `Try 'uuidgen --help'` line.
#[derive(Debug, PartialEq, Eq)]
struct Failure {
    message: String,
    hint: bool,
}

impl Failure {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: true,
        }
    }

    fn plain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: false,
        }
    }
}

/// The options parsed in order, as `getopt_long` hands them over (`cash-getopt`) and
/// util-linux takes them.
///
/// `-h` and `-V` act at once, a conflict is reported where the second option is met,
/// and words that are not options are ignored.
fn parse(args: &[String]) -> Result<Parsed, Failure> {
    let mut request = Request::default();
    let mut first_of_row: Vec<Option<char>> = vec![None; EXCLUSIVE.len()];
    let shorts: Vec<Short<char>> = LONG_OPTIONS
        .iter()
        .map(|long| Short::new(long.id, long.arg, long.id))
        .collect();
    for next in Getopt::new(&shorts, LONG_OPTIONS).read(args) {
        let (letter, value) = match next.map_err(|problem| Failure::usage(problem.to_string()))? {
            Item::Option { id, value, .. } => (id, value),
            Item::Operand { .. } => continue,
        };
        if let Some(early) = apply(&mut request, &mut first_of_row, letter, value)? {
            return Ok(early);
        }
    }
    Ok(Parsed::Run(request))
}

/// One option taken: a conflict with an earlier one is refused first, as util-linux
/// refuses it; `-h` and `-V` end the parse.
fn apply(
    request: &mut Request,
    first_of_row: &mut [Option<char>],
    letter: char,
    value: Option<String>,
) -> Result<Option<Parsed>, Failure> {
    for (row, first) in EXCLUSIVE.iter().zip(first_of_row.iter_mut()) {
        if !row.contains(letter) {
            continue;
        }
        match first {
            None => *first = Some(letter),
            Some(earlier) if *earlier != letter => {
                return Err(Failure::plain(format!(
                    "options --{} and --{} cannot be combined",
                    long_name(*earlier),
                    long_name(letter)
                )));
            }
            Some(_) => {}
        }
    }
    match letter {
        'h' => return Ok(Some(Parsed::Help)),
        'V' => return Ok(Some(Parsed::Version)),
        'r' | 't' | 'm' | 's' | '6' | '7' => request.kind = Some(letter),
        'n' => request.namespace = value,
        'N' => request.name = value,
        'x' => request.hex = true,
        'C' => request.count = Some(parse_count(value.as_deref().unwrap_or_default())?),
        _ => {}
    }
    Ok(None)
}

/// `-C`'s value as `strtou32_or_err` takes it: leading blanks and a `+` allowed, decimal
/// only, nothing after the digits; a negative or too large one is out of range.
fn parse_count(text: &str) -> Result<u32, Failure> {
    let trimmed = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (negative, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Failure::plain(format!("invalid count: '{text}'")));
    }
    if negative {
        return Err(Failure::plain(format!(
            "invalid count: '{text}': Numerical result out of range"
        )));
    }
    digits.parse::<u32>().map_err(|_| {
        Failure::plain(format!(
            "invalid count: '{text}': Numerical result out of range"
        ))
    })
}

/// The namespace `-n` names: an alias, or a UUID written with its hyphens.
fn namespace(text: &str) -> Result<Uuid, Failure> {
    match text {
        "@dns" => return Ok(Uuid::NAMESPACE_DNS),
        "@url" => return Ok(Uuid::NAMESPACE_URL),
        "@oid" => return Ok(Uuid::NAMESPACE_OID),
        "@x500" => return Ok(Uuid::NAMESPACE_X500),
        _ => {}
    }
    if text.starts_with('@') {
        return Err(Failure::usage(format!("unknown namespace alias: '{text}'")));
    }
    // libuuid's `uuid_parse`: exactly the hyphenated form, in either case.
    let hyphenated = text.len() == 36
        && text.bytes().enumerate().all(|(at, byte)| match at {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        });
    if !hyphenated {
        return Err(Failure::usage(format!(
            "invalid uuid for namespace: '{text}'"
        )));
    }
    Uuid::try_parse(text)
        .map_err(|_| Failure::usage(format!("invalid uuid for namespace: '{text}'")))
}

/// The bytes a `-x` name spells, two hex digits each.
fn hex_bytes(text: &str) -> Result<Vec<u8>, Failure> {
    let invalid = || Failure::usage("not a valid hex string");
    if !text.len().is_multiple_of(2) {
        return Err(invalid());
    }
    let (pairs, _none) = text.as_bytes().as_chunks::<2>();
    pairs
        .iter()
        .map(|&[high, low]| {
            let high = char::from(high).to_digit(16).ok_or_else(invalid)?;
            let low = char::from(low).to_digit(16).ok_or_else(invalid)?;
            u8::try_from(high * 16 + low).map_err(|_| invalid())
        })
        .collect()
}

/// Six random bytes with the multicast bit set, as libuuid makes a node id when it has
/// no network card's address: a value no real card has.
fn random_node() -> [u8; 6] {
    let random = Uuid::new_v4();
    let mut node = [0u8; 6];
    node.copy_from_slice(random.as_bytes().get(..6).unwrap_or(&[0; 6]));
    node[0] |= 0x01;
    node
}

/// The UUIDs `request` asks for, each as text, after the checks util-linux makes once
/// the options are read.
fn generate(request: &Request) -> Result<Vec<String>, Failure> {
    let hashed = matches!(request.kind, Some('m' | 's'));
    match (&request.namespace, &request.name) {
        (Some(_), None) => {
            return Err(Failure::usage("--namespace requires --name argument"));
        }
        (None, Some(_)) => {
            return Err(Failure::usage("--name requires --namespace argument"));
        }
        (None, None) if hashed => {
            return Err(Failure::usage(
                "--md5 or --sha1 requires --namespace argument",
            ));
        }
        (Some(_), Some(_)) if !hashed => {
            return Err(Failure::usage("--namespace requires --md5 or --sha1"));
        }
        _ => {}
    }
    if let (Some(namespace_text), Some(name)) = (&request.namespace, &request.name) {
        let name = if request.hex {
            hex_bytes(name)?
        } else {
            name.as_bytes().to_vec()
        };
        let namespace = namespace(namespace_text)?;
        let uuid = if request.kind == Some('m') {
            Uuid::new_v3(&namespace, &name)
        } else {
            Uuid::new_v5(&namespace, &name)
        };
        return Ok(vec![uuid.to_string()]);
    }
    let count = request.count.unwrap_or(1);
    let mut made = Vec::with_capacity(usize::try_from(count).unwrap_or(1).min(1 << 16));
    match request.kind {
        Some('t' | '6') => {
            let context = uuid::ContextV1::new_random();
            let node = random_node();
            for _ in 0..count {
                let stamp = uuid::Timestamp::now(&context);
                let uuid = if request.kind == Some('t') {
                    Uuid::new_v1(stamp, &node)
                } else {
                    Uuid::new_v6(stamp, &node)
                };
                made.push(uuid.to_string());
            }
        }
        Some('7') => made.extend((0..count).map(|_| Uuid::now_v7().to_string())),
        _ => made.extend((0..count).map(|_| Uuid::new_v4().to_string())),
    }
    Ok(made)
}

impl builtins::Command for UuidgenCommand {
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
        let made = parse(&self.args).and_then(|parsed| match parsed {
            Parsed::Help => Ok(None),
            Parsed::Version => Ok(Some(vec![
                "uuidgen (cash): util-linux 2.42.3's options".to_owned(),
            ])),
            Parsed::Run(request) => generate(&request).map(Some),
        });
        match made {
            Ok(None) => {
                write!(
                    context.stdout(),
                    "\nUsage:\n uuidgen [options]\n\nCreate a new UUID value.\n\n{OPTIONS}"
                )?;
                Ok(ExecutionResult::success())
            }
            Ok(Some(lines)) => {
                let mut stdout = context.stdout();
                for line in lines {
                    writeln!(stdout, "{line}")?;
                }
                Ok(ExecutionResult::success())
            }
            Err(failure) => {
                let mut stderr = context.stderr();
                writeln!(stderr, "uuidgen: {}", failure.message)?;
                if failure.hint {
                    writeln!(stderr, "Try 'uuidgen --help' for more information.")?;
                }
                Ok(ExecutionResult::general_error())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    fn run(line: &str) -> Result<Vec<String>, Failure> {
        match parse(&words(line))? {
            Parsed::Run(request) => generate(&request),
            Parsed::Help | Parsed::Version => Err(Failure::plain("ended early")),
        }
    }

    #[test]
    fn hash_based_uuids_are_util_linux_s() {
        assert_eq!(
            run("-m -n @dns -N www.example.com").unwrap(),
            ["5df41881-3aed-3515-88a7-2f4a814cf09e"]
        );
        assert_eq!(
            run("-s -n @dns -N www.example.com").unwrap(),
            ["2ed6657d-e927-568b-95e1-2665a8aea6a2"]
        );
        assert_eq!(
            run("--md5 --namespace=6BA7B810-9DAD-11D1-80B4-00C04FD430C8 --name www.example.com")
                .unwrap(),
            ["5df41881-3aed-3515-88a7-2f4a814cf09e"]
        );
        assert_eq!(
            run("-m -x -n @dns -N 7777").unwrap(),
            ["77761fed-d3fd-32b2-b263-f99be21115a7"]
        );
        assert_eq!(
            run("-mn @dns -N a").unwrap(),
            ["4c104dd0-4821-30d5-9ce3-0e7a1f8b7c0d"]
        );
    }

    #[test]
    fn options_are_read_as_getopt_long_reads_them() {
        assert_eq!(parse(&words("-h -Z")), Ok(Parsed::Help));
        assert_eq!(parse(&words("--vers extra")), Ok(Parsed::Version));
        assert_eq!(
            parse(&words("-Z -h")),
            Err(Failure::usage("invalid option -- 'Z'"))
        );
        assert_eq!(
            parse(&words("--bogus")),
            Err(Failure::usage("unrecognized option '--bogus'"))
        );
        assert_eq!(
            parse(&words("--na @dns")),
            Err(Failure::usage(
                "option '--na' is ambiguous; possibilities: '--namespace' '--name'"
            ))
        );
        assert_eq!(
            parse(&words("--count")),
            Err(Failure::usage("option '--count' requires an argument"))
        );
        assert_eq!(
            parse(&words("-C")),
            Err(Failure::usage("option requires an argument -- 'C'"))
        );
        assert_eq!(
            parse(&words("--hex=1")),
            Err(Failure::usage("option '--hex' doesn't allow an argument"))
        );
        assert_eq!(
            parse(&words("-rC 2")),
            Ok(Parsed::Run(Request {
                kind: Some('r'),
                count: Some(2),
                ..Request::default()
            }))
        );
        assert_eq!(
            parse(&words("-C2 --rand")),
            Ok(Parsed::Run(Request {
                kind: Some('r'),
                count: Some(2),
                ..Request::default()
            }))
        );
    }

    #[test]
    fn conflicts_and_missing_partners_are_refused_in_util_linux_s_words() {
        assert_eq!(
            parse(&words("-r -m")),
            Err(Failure::plain(
                "options --random and --md5 cannot be combined"
            ))
        );
        assert_eq!(
            parse(&words("-C 2 -t -r")),
            Err(Failure::plain(
                "options --time and --random cannot be combined"
            ))
        );
        assert_eq!(
            parse(&words("-m -n @dns -N a -C 2")),
            Err(Failure::plain(
                "options --md5 and --count cannot be combined"
            ))
        );
        assert_eq!(
            parse(&words("-t -N a")),
            Err(Failure::plain(
                "options --time and --name cannot be combined"
            ))
        );
        assert_eq!(
            run("-n @dns"),
            Err(Failure::usage("--namespace requires --name argument"))
        );
        assert_eq!(
            run("-m -N a"),
            Err(Failure::usage("--name requires --namespace argument"))
        );
        assert_eq!(
            run("-s"),
            Err(Failure::usage(
                "--md5 or --sha1 requires --namespace argument"
            ))
        );
        assert_eq!(
            run("-C 2 -n @dns -N a"),
            Err(Failure::usage("--namespace requires --md5 or --sha1"))
        );
        assert_eq!(
            run("-m -n @DNS -N a"),
            Err(Failure::usage("unknown namespace alias: '@DNS'"))
        );
        assert_eq!(
            run("-m -n 6ba7b8109dad11d180b400c04fd430c8 -N a"),
            Err(Failure::usage(
                "invalid uuid for namespace: '6ba7b8109dad11d180b400c04fd430c8'"
            ))
        );
        assert_eq!(
            run("-m -x -n nonsense -N zz"),
            Err(Failure::usage("not a valid hex string"))
        );
        assert_eq!(
            run("-m -x -n @dns -N 777"),
            Err(Failure::usage("not a valid hex string"))
        );
    }

    #[test]
    fn counts_are_read_as_strtou32_reads_them() {
        assert_eq!(parse_count("3"), Ok(3));
        assert_eq!(parse_count(" +2"), Ok(2));
        assert_eq!(parse_count("x"), Err(Failure::plain("invalid count: 'x'")));
        assert_eq!(
            parse_count("2x"),
            Err(Failure::plain("invalid count: '2x'"))
        );
        assert_eq!(
            parse_count("0x2"),
            Err(Failure::plain("invalid count: '0x2'"))
        );
        assert_eq!(parse_count(""), Err(Failure::plain("invalid count: ''")));
        assert_eq!(
            parse_count("-1"),
            Err(Failure::plain(
                "invalid count: '-1': Numerical result out of range"
            ))
        );
        assert_eq!(
            parse_count("4294967296"),
            Err(Failure::plain(
                "invalid count: '4294967296': Numerical result out of range"
            ))
        );
    }

    #[test]
    fn random_and_time_based_uuids_have_their_version_and_count() {
        let version = |text: &str| text.chars().nth(14);
        assert_eq!(run("").unwrap().len(), 1);
        assert_eq!(version(&run("").unwrap()[0]), Some('4'));
        assert_eq!(version(&run("-t").unwrap()[0]), Some('1'));
        assert_eq!(version(&run("-6").unwrap()[0]), Some('6'));
        assert_eq!(version(&run("-7").unwrap()[0]), Some('7'));
        let three = run("-C 3 -t").unwrap();
        assert_eq!(three.len(), 3);
        let distinct: std::collections::BTreeSet<&String> = three.iter().collect();
        assert_eq!(distinct.len(), 3, "{three:?}");
        // One clock and one node: the last twelve characters are the same.
        assert_eq!(three[0].get(24..), three[2].get(24..));
        assert_eq!(run("-C 0").unwrap(), Vec::<String>::new());
        assert_ne!(run("-x").unwrap(), run("-x").unwrap());
    }

    #[test]
    fn a_node_id_is_never_a_real_card_s() {
        assert_eq!(random_node()[0] & 0x01, 0x01);
    }
}
