//! rar's command line, as Rar.exe 7.23 reads it: the command, then the archive, then
//! names, list files and a destination, with switches anywhere until `--`; switches from
//! `rar.ini` and `RARINISWITCHES` before those typed.

use std::path::PathBuf;

/// What a listing shows: `l`, `lt`, `lta`, `lb`, and `v` for each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ListForm {
    /// One line a file.
    Plain,
    /// A block of fields a file.
    Technical,
    /// The same, service headers too.
    TechnicalAll,
    /// Names alone.
    Bare,
}

/// The command, the first word that is not a switch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Add,
    Comment,
    Change,
    CommentWrite,
    Delete,
    /// `e`: names without their folders.
    Extract,
    Freshen,
    /// `i`: what to look for, and how.
    Find(FindSpec),
    Lock,
    /// `l` or `v` (`verbose`), in a form.
    List {
        verbose: bool,
        form: ListForm,
    },
    /// `m`, or `mf` (`files_only`).
    Move {
        files_only: bool,
    },
    Print,
    Repair,
    Reconstruct,
    Rename,
    /// `rr`, with the size asked.
    RecoveryRecord(Option<String>),
    /// `rv`, with the count asked.
    RecoveryVolumes(Option<String>),
    /// `s`, with a module or `-`.
    Sfx(Option<String>),
    Test,
    Update,
    /// `x`: names with their folders.
    ExtractFull,
}

/// `i`'s parameters: `i`, `c`, `h`, `t`, and the text.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(super) struct FindSpec {
    pub(super) case_sensitive: bool,
    pub(super) hex: bool,
    pub(super) all_tables: bool,
    pub(super) text: String,
}

impl Command {
    /// The command a word names, in either case; `None` for none.
    pub(super) fn parse(word: &str) -> Option<Self> {
        let lower = word.to_ascii_lowercase();
        let mut chars = lower.chars();
        let first = chars.next()?;
        let rest = chars.as_str();
        let list_form = |rest: &str| match rest.as_bytes() {
            [b't', b'a', ..] => ListForm::TechnicalAll,
            [b't', ..] => ListForm::Technical,
            [b'b', ..] => ListForm::Bare,
            _ => ListForm::Plain,
        };
        Some(match first {
            'a' => Self::Add,
            'c' if rest.starts_with('h') => Self::Change,
            'c' if rest.starts_with('w') => Self::CommentWrite,
            'c' => Self::Comment,
            'd' => Self::Delete,
            'e' => Self::Extract,
            'f' => Self::Freshen,
            'i' => Self::Find(FindSpec::parse(word.get(1..).unwrap_or_default())),
            'k' => Self::Lock,
            'l' => Self::List {
                verbose: false,
                form: list_form(rest),
            },
            'v' => Self::List {
                verbose: true,
                form: list_form(rest),
            },
            'm' => Self::Move {
                files_only: rest.starts_with('f'),
            },
            'p' => Self::Print,
            'r' if rest.starts_with('c') => Self::Reconstruct,
            'r' if rest.starts_with('n') => Self::Rename,
            'r' if rest.starts_with('r') => {
                Self::RecoveryRecord(word.get(2..).filter(|s| !s.is_empty()).map(str::to_owned))
            }
            'r' if rest.starts_with('v') => {
                Self::RecoveryVolumes(word.get(2..).filter(|s| !s.is_empty()).map(str::to_owned))
            }
            'r' => Self::Repair,
            's' => Self::Sfx(word.get(1..).filter(|s| !s.is_empty()).map(str::to_owned)),
            't' => Self::Test,
            'u' => Self::Update,
            'x' => Self::ExtractFull,
            _ => return None,
        })
    }

    /// Whether unrar has the command.
    pub(super) const fn in_unrar(&self) -> bool {
        matches!(
            self,
            Self::Extract | Self::List { .. } | Self::Print | Self::Test | Self::ExtractFull
        )
    }

    /// Whether the last name may be the folder to extract to (`path_to_extract\`).
    pub(super) const fn takes_destination(&self) -> bool {
        matches!(
            self,
            Self::Extract | Self::ExtractFull | Self::Repair | Self::Reconstruct
        )
    }

    /// Whether the command changes or makes an archive.
    pub(super) const fn writes(&self) -> bool {
        matches!(
            self,
            Self::Add
                | Self::Comment
                | Self::Change
                | Self::Delete
                | Self::Freshen
                | Self::Lock
                | Self::Move { .. }
                | Self::Rename
                | Self::RecoveryRecord(_)
                | Self::RecoveryVolumes(_)
                | Self::Sfx(_)
                | Self::Update
        )
    }
}

impl FindSpec {
    /// `i[i|c|h|t]=<string>`, or `i<string>` with no parameters.
    fn parse(rest: &str) -> Self {
        let mut spec = Self::default();
        let Some((params, text)) = rest.split_once('=') else {
            rest.clone_into(&mut spec.text);
            return spec;
        };
        for c in params.chars() {
            match c.to_ascii_lowercase() {
                'c' => spec.case_sensitive = true,
                'i' => spec.case_sensitive = false,
                'h' => spec.hex = true,
                't' => spec.all_tables = true,
                _ => {}
            }
        }
        text.clone_into(&mut spec.text);
        spec
    }
}

/// A switch's argument that may be left out: `-p` alone asks, `-ppw` gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Arg {
    /// The switch alone.
    Bare,
    /// The switch with its text.
    Given(String),
}

impl Arg {
    fn of(text: String) -> Self {
        if text.is_empty() {
            Self::Bare
        } else {
            Self::Given(text)
        }
    }

    /// The text given, if any.
    pub(super) fn text(&self) -> Option<&str> {
        match self {
            Self::Bare => None,
            Self::Given(text) => Some(text),
        }
    }
}

/// `-o`: what to do with a file that is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) enum Overwrite {
    /// Ask (extracting's default).
    #[default]
    Ask,
    /// `-o+`.
    All,
    /// `-o-`.
    Skip,
    /// `-or`: a new name.
    Rename,
}

/// `-sc`'s character sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Charset {
    Utf16,
    Utf8,
    Ansi,
    Oem,
}

/// Which files `-sc` applies to, each set alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Charsets {
    pub(super) log: Option<Charset>,
    pub(super) list: Option<Charset>,
    pub(super) comment: Option<Charset>,
    pub(super) redirect: Option<Charset>,
}

/// A time filter (`-ta`, `-tb`, `-tn`, `-to`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TimeFilter {
    /// `a` (after) or `b` (before) a date; `n` (newer) or `o` (older) than a period.
    pub(super) kind: char,
    /// The times it looks at: `m`, `c`, `a`; none means `m`.
    pub(super) times: String,
    /// `o`: one of these is enough.
    pub(super) or: bool,
    /// The date or the period, as typed.
    pub(super) value: String,
}

/// `-ts`: which times, at what precision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) struct TimeStore {
    pub(super) modified: Option<char>,
    pub(super) created: Option<char>,
    pub(super) accessed: Option<char>,
    pub(super) preserve: bool,
}

/// Every switch, as given; what each means is the command's business.
#[derive(Clone, Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "rar has that many on-off switches, each its own field"
)]
pub(super) struct Switches {
    pub(super) list_files: Option<bool>,
    pub(super) clear_archive_attr: bool,
    pub(super) alt_destination: Option<u8>,
    pub(super) generate_name: Option<String>,
    pub(super) generate_default: Option<String>,
    pub(super) ignore_attributes: bool,
    pub(super) archive_metadata: Option<char>,
    pub(super) only_archive_attr: bool,
    pub(super) archive_path: Option<String>,
    pub(super) synchronize: bool,
    pub(super) no_comments: bool,
    pub(super) no_config: bool,
    pub(super) case: Option<char>,
    pub(super) delete_files: bool,
    pub(super) shared: bool,
    pub(super) recycle: bool,
    pub(super) no_sort: bool,
    pub(super) wipe: bool,
    pub(super) exclude_attr: Option<u32>,
    pub(super) include_attr: Option<u32>,
    pub(super) no_empty_dirs: bool,
    /// `-ep` (0 alone), `-ep1`, `-ep2`, `-ep3`, `-ep4`.
    pub(super) exclude_paths: Option<u8>,
    pub(super) exclude_prefix: Option<String>,
    pub(super) freshen: bool,
    pub(super) header_password: Option<Arg>,
    pub(super) hash: Option<char>,
    pub(super) no_banner: bool,
    pub(super) no_done: bool,
    pub(super) no_names: bool,
    pub(super) no_percent: bool,
    pub(super) quiet: bool,
    pub(super) to_stderr: bool,
    pub(super) log_errors: Option<String>,
    pub(super) silent: bool,
    pub(super) version: bool,
    pub(super) lock: bool,
    pub(super) keep_broken: bool,
    pub(super) log_names: Vec<String>,
    pub(super) method: Option<u8>,
    pub(super) compression_params: Vec<String>,
    pub(super) dictionary: Option<u64>,
    pub(super) dictionary_limit: Option<u64>,
    pub(super) skip_encrypted: bool,
    pub(super) store_types: Vec<String>,
    pub(super) threads: Option<u32>,
    pub(super) include: Vec<String>,
    pub(super) include_lists: Vec<Option<String>>,
    pub(super) overwrite: Option<Overwrite>,
    pub(super) ntfs_compressed: bool,
    pub(super) hard_links: bool,
    pub(super) identical: Option<String>,
    /// `-ol` (empty), `-ola` (`a`), `-ol-` (`-`).
    pub(super) links: Option<String>,
    pub(super) incompatible_names: bool,
    pub(super) output_path: Option<String>,
    pub(super) streams: bool,
    pub(super) owners: bool,
    /// `-p` with the password, `-p` alone (ask), or `-p-` (`no_password`).
    pub(super) password: Option<Arg>,
    pub(super) no_password: bool,
    pub(super) quick_open: Option<char>,
    /// `-r` (`Some('r')`), `-r-` (`Some('-')`), `-r0` (`Some('0')`).
    pub(super) recurse: Option<char>,
    pub(super) recovery_record: Option<String>,
    pub(super) recovery_volumes: Option<String>,
    pub(super) solid: Option<String>,
    pub(super) charsets: Charsets,
    pub(super) sfx: Option<String>,
    pub(super) stdin: Option<String>,
    pub(super) size_less: Option<u64>,
    pub(super) size_more: Option<u64>,
    pub(super) test_after: bool,
    pub(super) time_filters: Vec<TimeFilter>,
    pub(super) keep_time: Option<String>,
    pub(super) latest_time: bool,
    pub(super) times: TimeStore,
    pub(super) update: bool,
    /// `-v` alone (`Some(None)`), or the sizes of `-v<size>`.
    pub(super) volumes: Option<Vec<Option<u64>>>,
    pub(super) erase_disk: bool,
    pub(super) versions: Option<Arg>,
    pub(super) pause: bool,
    pub(super) work_dir: Option<String>,
    pub(super) exclude: Vec<String>,
    pub(super) exclude_lists: Vec<Option<String>>,
    pub(super) yes: bool,
    pub(super) comment_file: Option<Arg>,
    pub(super) help: bool,
    /// Switches accepted and ignored: `-ieml`, `-ioff`, `-isnd`, `-mlp`, `-ri`, `-vd`.
    pub(super) ignored: Vec<String>,
}

/// A command line as rar reads it.
#[derive(Clone, Debug, Default)]
pub(super) struct Parsed {
    pub(super) switches: Switches,
    pub(super) command: Option<String>,
    pub(super) archive: Option<String>,
    /// Names, each `@list` already marked.
    pub(super) names: Vec<Name>,
}

/// A name after the archive's: a name or a mask, or a list file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Name {
    Plain(String),
    /// `@file`; `@` alone is standard input.
    List(String),
}

/// A switch rar does not know: its text without the `-`.
#[derive(Debug)]
pub(super) struct Unknown(pub(super) String);

/// Reads `args` after switches from `rar.ini` and `RARINISWITCHES` (`defaults`, lowest
/// priority first), a command's own `rar.ini` set chosen once the command is known.
pub(super) fn parse(args: &[String]) -> Result<Parsed, Unknown> {
    let mut parsed = Parsed::default();
    let mut stopped = false;
    let mut lists_allowed: Option<bool> = None;
    for arg in args {
        if !stopped && arg.starts_with('-') && arg.len() > 1 {
            if arg == "--" {
                stopped = true;
                continue;
            }
            let text = arg.get(1..).unwrap_or_default();
            apply(&mut parsed.switches, text)?;
            if let Some(list) = parsed.switches.list_files.take() {
                lists_allowed = Some(list);
            }
            continue;
        }
        if parsed.command.is_none() {
            parsed.command = Some(arg.clone());
        } else if parsed.archive.is_none() {
            parsed.archive = Some(arg.clone());
        } else if let Some(list) = arg.strip_prefix('@')
            && lists_allowed != Some(false)
        {
            parsed.names.push(Name::List(list.to_owned()));
        } else {
            parsed.names.push(Name::Plain(arg.clone()));
        }
    }
    Ok(parsed)
}

/// Applies switches given as one string, as `RARINISWITCHES` and `rar.ini` give them:
/// words split at spaces, switches only; one rar does not know is passed over.
pub(super) fn apply_defaults(switches: &mut Switches, text: &str) {
    for word in split_words(text) {
        if let Some(switch) = word.strip_prefix('-')
            && !switch.is_empty()
        {
            let _ = apply(switches, switch);
        }
    }
}

/// Splits `text` at spaces outside double quotes.
fn split_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            ' ' | '\t' if !quoted => {
                if any {
                    words.push(std::mem::take(&mut word));
                    any = false;
                }
            }
            _ => {
                word.push(c);
                any = true;
            }
        }
    }
    if any {
        words.push(word);
    }
    words
}

/// A size with an optional unit: `b`/`B` bytes, `k` KiB, `K` thousands, `m`, `M`, `g`,
/// `G`, `t`, `T` likewise; `default` multiplies a bare number. Decimal fractions are
/// allowed, as `-v1.5g`.
pub(super) fn parse_size(text: &str, default: u64) -> Option<u64> {
    let digits_end = text
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(digits_end);
    let unit_value = match unit {
        "" => default,
        "b" | "B" => 1,
        "k" => 1 << 10,
        "K" => 1000,
        "m" => 1 << 20,
        "M" => 1_000_000,
        "g" => 1 << 30,
        "G" => 1_000_000_000,
        "t" => 1 << 40,
        "T" => 1_000_000_000_000,
        _ => return None,
    };
    if number.is_empty() {
        return None;
    }
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    let whole: u64 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let mut value = whole.checked_mul(unit_value)?;
    if !fraction.is_empty() {
        let scale = 10u64.checked_pow(u32::try_from(fraction.len()).ok()?)?;
        let part: u64 = fraction.parse().ok()?;
        value = value.checked_add(part.checked_mul(unit_value)? / scale)?;
    }
    Some(value)
}

/// `-md[x]<size>[k|m|g]`: a dictionary rar can use, megabytes (`-md`) or gigabytes
/// (`-mdx`) when no unit is given: a power of two from 128 KiB to 4 GiB, or any whole
/// number of gigabytes up to 64 beyond.
fn parse_dictionary(text: &str, default_unit: u64) -> Option<u64> {
    let size = parse_size(text, default_unit)?;
    let valid = if size <= 4 << 30 {
        size >= 128 << 10 && size.is_power_of_two()
    } else {
        size <= 64 << 30 && size % (1 << 30) == 0
    };
    valid.then_some(size)
}

/// `-e[+]<attr>`: a number, decimal, octal with a leading 0 or hexadecimal with 0x, or
/// the letters `D`, `S`, `H`, `A`, `R`.
fn parse_attributes(text: &str) -> u32 {
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return u32::from_str_radix(hex, 16).unwrap_or(0);
    }
    if text.starts_with(|c: char| c.is_ascii_digit()) {
        if text.len() > 1 && text.starts_with('0') {
            return u32::from_str_radix(text.get(1..).unwrap_or_default(), 8).unwrap_or(0);
        }
        return text.parse().unwrap_or(0);
    }
    text.chars().fold(0, |mask, c| {
        mask | match c.to_ascii_uppercase() {
            'R' => 0x1,
            'H' => 0x2,
            'S' => 0x4,
            'D' => 0x10,
            'A' => 0x20,
            _ => 0,
        }
    })
}

/// One switch, its text after `-`.
fn apply(s: &mut Switches, text: &str) -> Result<(), Unknown> {
    let unknown = || Unknown(text.to_owned());
    let lower = text.to_ascii_lowercase();
    // The argument keeps its case: a path, a password, a mask.
    let arg = |prefix: usize| text.get(prefix..).unwrap_or_default().to_owned();
    let opt_arg = |prefix: usize| Some(arg(prefix)).filter(|a| !a.is_empty());
    match lower.as_str() {
        "?" | "h" | "help" => s.help = true,
        "@" => s.list_files = Some(false),
        "@+" => s.list_files = Some(true),
        "ac" => s.clear_archive_attr = true,
        "ad" => s.alt_destination = Some(0),
        "ad1" => s.alt_destination = Some(1),
        "ad2" => s.alt_destination = Some(2),
        "ai" => s.ignore_attributes = true,
        "am" | "ams" => s.archive_metadata = Some('s'),
        "amr" => s.archive_metadata = Some('r'),
        "ao" => s.only_archive_attr = true,
        "as" => s.synchronize = true,
        "c-" => s.no_comments = true,
        "cfg-" => s.no_config = true,
        "cl" => s.case = Some('l'),
        "cu" => s.case = Some('u'),
        "df" => s.delete_files = true,
        "dh" => s.shared = true,
        "dr" => s.recycle = true,
        "ds" => s.no_sort = true,
        "dw" => s.wipe = true,
        "ed" => s.no_empty_dirs = true,
        "ep" => s.exclude_paths = Some(0),
        "ep1" => s.exclude_paths = Some(1),
        "ep2" => s.exclude_paths = Some(2),
        "ep3" => s.exclude_paths = Some(3),
        "f" => s.freshen = true,
        "ierr" => s.to_stderr = true,
        "inul" => s.silent = true,
        "iver" => s.version = true,
        "k" => s.lock = true,
        "kb" => s.keep_broken = true,
        "mlp" => s.ignored.push(text.to_owned()),
        "oc" => s.ntfs_compressed = true,
        "oh" => s.hard_links = true,
        "oni" => s.incompatible_names = true,
        "or" => s.overwrite = Some(Overwrite::Rename),
        "os" => s.streams = true,
        "ow" => s.owners = true,
        "o" => s.overwrite = Some(Overwrite::Ask),
        "o+" => s.overwrite = Some(Overwrite::All),
        "o-" => s.overwrite = Some(Overwrite::Skip),
        "p-" => s.no_password = true,
        "r" => s.recurse = Some('r'),
        "r-" => s.recurse = Some('-'),
        "r0" => s.recurse = Some('0'),
        "t" => s.test_after = true,
        "tl" => s.latest_time = true,
        "u" => s.update = true,
        "vd" => s.erase_disk = true,
        "vp" => s.pause = true,
        "y" => s.yes = true,
        _ => return apply_with_argument(s, text, &lower, &arg, &opt_arg).ok_or_else(unknown),
    }
    Ok(())
}

/// The switches that take an argument after their name.
fn apply_with_argument(
    s: &mut Switches,
    text: &str,
    lower: &str,
    arg: &dyn Fn(usize) -> String,
    opt_arg: &dyn Fn(usize) -> Option<String>,
) -> Option<()> {
    let first = lower.chars().next()?;
    match first {
        'a' if lower.starts_with("agf") => s.generate_default = Some(arg(3)),
        'a' if lower.starts_with("ag") => s.generate_name = Some(arg(2)),
        'a' if lower.starts_with("ap") => s.archive_path = Some(arg(2)),
        'e' if lower.starts_with("ep4") => s.exclude_prefix = Some(arg(3)),
        'e' if lower.starts_with("ep") => return None,
        'e' if lower.starts_with("e+") => s.include_attr = Some(parse_attributes(&arg(2))),
        'e' => s.exclude_attr = Some(parse_attributes(&arg(1))),
        'h' if lower.starts_with("hp") => s.header_password = Some(Arg::of(arg(2))),
        'h' if lower == "ht" || lower == "htb" => s.hash = Some('b'),
        'h' if lower == "htc" => s.hash = Some('c'),
        'i' if lower.starts_with("id") => {
            for c in lower.chars().skip(2) {
                match c {
                    'c' => s.no_banner = true,
                    'd' => s.no_done = true,
                    'n' => s.no_names = true,
                    'p' => s.no_percent = true,
                    'q' => s.quiet = true,
                    _ => {}
                }
            }
        }
        'i' if lower.starts_with("ilog") => s.log_errors = Some(arg(4)),
        'i' if lower.starts_with("ieml")
            || lower.starts_with("ioff")
            || lower.starts_with("isnd") =>
        {
            s.ignored.push(text.to_owned());
        }
        'l' if lower.starts_with("log") => s.log_names.push(arg(3)),
        'm' => return apply_method(s, text, lower, arg),
        'n' if lower.starts_with("n@") => s.include_lists.push(opt_arg(2)),
        'n' => s.include.push(arg(1)),
        'o' if lower.starts_with("oi") => s.identical = Some(arg(2)),
        'o' if lower.starts_with("ol") => {
            let rest = arg(2);
            if !matches!(rest.as_str(), "" | "a" | "A" | "-") {
                return None;
            }
            s.links = Some(rest.to_ascii_lowercase());
        }
        'o' if lower.starts_with("om") => s.ignored.push(text.to_owned()),
        'o' if lower.starts_with("op") => s.output_path = Some(arg(2)),
        'p' => s.password = Some(Arg::of(arg(1))),
        'q' if lower.starts_with("qo") => {
            s.quick_open = Some(match lower.get(2..)? {
                "" => ' ',
                "-" => '-',
                "+" => '+',
                _ => return None,
            });
        }
        'r' if lower.starts_with("ri") => s.ignored.push(text.to_owned()),
        'r' if lower.starts_with("rr") => s.recovery_record = Some(arg(2)),
        'r' if lower.starts_with("rv") => s.recovery_volumes = Some(arg(2)),
        's' if lower.starts_with("sfx") => s.sfx = Some(arg(3)),
        's' if lower.starts_with("sc") => apply_charset(s, lower.get(2..)?)?,
        's' if lower.starts_with("si") => s.stdin = Some(arg(2)),
        's' if lower.starts_with("sl") => s.size_less = Some(parse_size(&arg(2), 1)?),
        's' if lower.starts_with("sm") => s.size_more = Some(parse_size(&arg(2), 1)?),
        's' => s.solid = Some(arg(1)),
        't' if lower.starts_with("tk") => s.keep_time = Some(arg(2)),
        't' if lower.starts_with("ts") => apply_times(s, lower.get(2..)?)?,
        't' if matches!(lower.get(1..2), Some("a" | "b" | "n" | "o")) => {
            let kind = lower.chars().nth(1)?;
            let rest = arg(2);
            let modifiers_end = rest
                .find(|c: char| !matches!(c.to_ascii_lowercase(), 'm' | 'c' | 'a' | 'o'))
                .unwrap_or(rest.len());
            let (modifiers, value) = rest.split_at(modifiers_end);
            let modifiers = modifiers.to_ascii_lowercase();
            s.time_filters.push(TimeFilter {
                kind,
                times: modifiers.replace('o', ""),
                or: modifiers.contains('o'),
                value: value.to_owned(),
            });
        }
        'v' if lower.starts_with("ver") => s.versions = Some(Arg::of(arg(3))),
        'v' if lower == "vn" => s.ignored.push(text.to_owned()),
        'v' => {
            let size = text.get(1..).unwrap_or_default();
            let volumes = s.volumes.get_or_insert_with(Vec::new);
            volumes.push(parse_size(size, 1000));
        }
        'w' => s.work_dir = Some(arg(1)),
        'x' if lower.starts_with("x@") => s.exclude_lists.push(opt_arg(2)),
        'x' => s.exclude.push(arg(1)),
        'z' => s.comment_file = Some(Arg::of(arg(1))),
        _ => return None,
    }
    Some(())
}

/// `-m…`: `-m<0..5>`, `-ma5`, `-mc`, `-md`, `-me`, `-ms`, `-mt`.
fn apply_method(
    s: &mut Switches,
    text: &str,
    lower: &str,
    arg: &dyn Fn(usize) -> String,
) -> Option<()> {
    match lower.get(1..2)? {
        "0" | "1" | "2" | "3" | "4" | "5" if lower.len() == 2 => {
            s.method = Some(lower.get(1..2)?.parse().ok()?);
        }
        "a" => {
            if !matches!(lower, "ma" | "ma5") {
                return None;
            }
        }
        "c" => s.compression_params.push(arg(2)),
        "d" if lower.starts_with("mdx") => {
            s.dictionary_limit = Some(parse_dictionary(&arg(3), 1 << 30)?);
        }
        "d" => s.dictionary = Some(parse_dictionary(&arg(2), 1 << 20)?),
        "e" => {
            if lower.get(2..)?.contains('s') {
                s.skip_encrypted = true;
            }
        }
        "s" => s.store_types.push(arg(2)),
        "t" => {
            let threads: u32 = lower.get(2..)?.parse().ok()?;
            if !(1..=64).contains(&threads) {
                return None;
            }
            s.threads = Some(threads);
        }
        _ => {
            let _ = text;
            return None;
        }
    }
    Some(())
}

/// `-sc<charset>[objects]`.
fn apply_charset(s: &mut Switches, rest: &str) -> Option<()> {
    let mut chars = rest.chars();
    let charset = match chars.next()? {
        'u' => Charset::Utf16,
        'f' => Charset::Utf8,
        'a' => Charset::Ansi,
        'o' => Charset::Oem,
        _ => return None,
    };
    let objects = chars.as_str();
    let all = objects.is_empty();
    for (letter, slot) in [
        ('g', &mut s.charsets.log),
        ('l', &mut s.charsets.list),
        ('c', &mut s.charsets.comment),
        ('r', &mut s.charsets.redirect),
    ] {
        if all || objects.contains(letter) {
            *slot = Some(charset);
        }
    }
    Some(())
}

/// `-ts[m,c,a,p][+,-,1]`, its modifiers in any order.
fn apply_times(s: &mut Switches, rest: &str) -> Option<()> {
    let mut chars = rest.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        let precision = match chars.peek() {
            Some(&p @ ('+' | '-' | '1')) => {
                chars.next();
                p
            }
            _ => '+',
        };
        match c {
            'm' => s.times.modified = Some(precision),
            'c' => s.times.created = Some(precision),
            'a' => s.times.accessed = Some(precision),
            'p' => s.times.preserve = true,
            '+' | '-' | '1' => {
                s.times.modified = Some(c);
                s.times.created = Some(c);
                s.times.accessed = Some(c);
            }
            _ => return None,
        }
        any = true;
    }
    if !any {
        s.times.modified = Some('+');
        s.times.created = Some('+');
        s.times.accessed = Some('+');
    }
    Some(())
}

/// Where the archive's name, without an extension, gets `.rar`.
pub(super) fn with_default_extension(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    if last.contains('.') {
        name.to_owned()
    } else {
        format!("{name}.rar")
    }
}

/// The path `rar.ini` is read from: `%APPDATA%\WinRAR\rar.ini`.
pub(super) fn ini_path(appdata: &str) -> PathBuf {
    PathBuf::from(appdata).join("WinRAR").join("rar.ini")
}

/// `rar.ini`'s switches for `command` (its first letters as typed, lower case): the
/// general `switches=` line, then `switches_<command>=`.
pub(super) fn ini_switches(text: &str, command: &str) -> Vec<String> {
    let mut general = Vec::new();
    let mut own = Vec::new();
    for line in text.lines() {
        let line = line.trim_start_matches('\u{feff}').trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        if key == "switches" {
            general.push(value.to_owned());
        } else if let Some(name) = key.strip_prefix("switches_")
            && name == command
        {
            own.push(value.to_owned());
        }
    }
    general.extend(own);
    general
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn switches_stand_anywhere_until_two_dashes() {
        let parsed = parse(&args(&["-y", "l", "a.rar", "-idq", "--", "-x.rar"])).unwrap();
        assert!(parsed.switches.yes);
        assert!(parsed.switches.quiet);
        assert_eq!(parsed.command.as_deref(), Some("l"));
        assert_eq!(parsed.archive.as_deref(), Some("a.rar"));
        assert_eq!(parsed.names, [Name::Plain("-x.rar".to_owned())]);
    }

    #[test]
    fn unknown_switches_are_named_without_their_dash() {
        for bad in ["-qqq", "-m9", "-ma4", "-mt99", "-md7", "-m6", "-ep9"] {
            let error = parse(&args(&["l", bad, "a.rar"])).unwrap_err();
            assert_eq!(error.0, bad.get(1..).unwrap());
        }
        for good in [
            "-ma5", "-idz", "-v10k", "-vxyz", "-ver", "-x", "-n", "-e", "-ed", "-w",
        ] {
            assert!(parse(&args(&["l", good, "a.rar"])).is_ok(), "{good}");
        }
    }

    #[test]
    fn commands_are_read_in_either_case() {
        assert_eq!(
            Command::parse("LB"),
            Some(Command::List {
                verbose: false,
                form: ListForm::Bare
            })
        );
        assert_eq!(
            Command::parse("vta"),
            Some(Command::List {
                verbose: true,
                form: ListForm::TechnicalAll
            })
        );
        assert_eq!(Command::parse("q"), None);
        assert_eq!(
            Command::parse("ic=Hello"),
            Some(Command::Find(FindSpec {
                case_sensitive: true,
                hex: false,
                all_tables: false,
                text: "Hello".to_owned()
            }))
        );
    }

    #[test]
    fn sizes_take_rar_units_and_fractions() {
        assert_eq!(parse_size("10k", 1000), Some(10 << 10));
        assert_eq!(parse_size("10", 1000), Some(10_000));
        assert_eq!(parse_size("1.5g", 1), Some(3 << 29));
        assert_eq!(parse_size("2M", 1), Some(2_000_000));
        assert_eq!(parse_size("xyz", 1000), None);
        assert_eq!(parse_dictionary("128k", 1 << 20), Some(128 << 10));
        assert_eq!(parse_dictionary("7", 1 << 20), None);
        assert_eq!(parse_dictionary("6g", 1 << 20), Some(6 << 30));
    }

    #[test]
    fn archive_names_without_an_extension_get_rar() {
        assert_eq!(with_default_extension("a"), "a.rar");
        assert_eq!(with_default_extension("dir.x/a"), "dir.x/a.rar");
        assert_eq!(with_default_extension("b.zip"), "b.zip");
        assert_eq!(with_default_extension("noext."), "noext.");
    }

    #[test]
    fn rar_ini_gives_the_general_switches_then_the_commands_own() {
        let text = "switches=-m5 -s\r\nswitches_a=-idq\r\nswitches_x=-o+\r\n";
        assert_eq!(ini_switches(text, "a"), ["-m5 -s", "-idq"]);
        assert_eq!(ini_switches(text, "l"), ["-m5 -s"]);
    }
}
