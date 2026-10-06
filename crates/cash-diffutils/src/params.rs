// This file is part of cash's copy of the uutils diffutils package (CASH-PATCHES.md).
//
// For the full copyright and license information, please view the LICENSE-*
// files that was distributed with this source code.

//! `diff`'s command line, read as GNU diff 3.12 reads it.

use std::ffi::OsString;
use std::io::IsTerminal as _;

use crate::getopt::{self, Arg, Item, Long, Short};
use crate::utils::quote;

/// The output format.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Format {
    #[default]
    Normal,
    Unified,
    Context,
    Ed,
    SideBySide,
}

/// Everything the options said.
#[derive(Clone, Debug)]
pub struct Params {
    pub executable: OsString,
    pub from: OsString,
    pub to: OsString,
    pub format: Format,
    pub context_count: usize,
    pub report_identical_files: bool,
    pub brief: bool,
    pub expand_tabs: bool,
    pub initial_tab: bool,
    pub tabsize: usize,
    pub width: usize,
    pub recursive: bool,
    pub new_file: bool,
    pub text: bool,
    pub ignore_case: bool,
    pub ignore_tab_expansion: bool,
    pub ignore_trailing_space: bool,
    pub ignore_space_change: bool,
    pub ignore_all_space: bool,
    pub ignore_blank_lines: bool,
    pub ignore_regexps: Vec<regex::bytes::Regex>,
    pub strip_trailing_cr: bool,
    pub labels: Vec<String>,
    pub color: bool,
    pub excludes: Vec<glob::Pattern>,
    pub suppress_common_lines: bool,
    pub left_column: bool,
    /// The option words as written, for the `diff -r a/x b/x` line before each pair of
    /// files a directory comparison reports.
    pub option_words: Vec<OsString>,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            executable: OsString::from("diff"),
            from: OsString::default(),
            to: OsString::default(),
            format: Format::default(),
            context_count: 3,
            report_identical_files: false,
            brief: false,
            expand_tabs: false,
            initial_tab: false,
            tabsize: 8,
            width: 130,
            recursive: false,
            new_file: false,
            text: false,
            ignore_case: false,
            ignore_tab_expansion: false,
            ignore_trailing_space: false,
            ignore_space_change: false,
            ignore_all_space: false,
            ignore_blank_lines: false,
            ignore_regexps: Vec::new(),
            strip_trailing_cr: false,
            labels: Vec::new(),
            color: false,
            excludes: Vec::new(),
            suppress_common_lines: false,
            left_column: false,
            option_words: Vec::new(),
        }
    }
}

impl Params {
    /// Whether any option changes which lines count as equal, so that two files whose
    /// bytes differ may still be the same.
    #[must_use]
    pub const fn has_ignore_options(&self) -> bool {
        self.ignore_case
            || self.ignore_tab_expansion
            || self.ignore_trailing_space
            || self.ignore_space_change
            || self.ignore_all_space
            || self.ignore_blank_lines
            || !self.ignore_regexps.is_empty()
            || self.strip_trailing_cr
    }
}

/// What the command line asked for.
#[derive(Debug)]
pub enum Request {
    Compare(Box<Params>),
    Help,
    Version,
}

/// Why the command line was refused.
#[derive(Debug, Eq, PartialEq)]
pub enum Refusal {
    /// A usage error: GNU adds `Try 'diff --help' for more information.`
    Usage(String),
    /// An error GNU reports on its own line, without the hint.
    Plain(String),
}

const SHORTS: &[Short] = &[
    Short {
        letter: 'a',
        arg: Arg::No,
        id: "text",
    },
    Short {
        letter: 'b',
        arg: Arg::No,
        id: "ignore-space-change",
    },
    Short {
        letter: 'B',
        arg: Arg::No,
        id: "ignore-blank-lines",
    },
    Short {
        letter: 'c',
        arg: Arg::No,
        id: "context",
    },
    Short {
        letter: 'C',
        arg: Arg::Required,
        id: "context",
    },
    Short {
        letter: 'd',
        arg: Arg::No,
        id: "minimal",
    },
    Short {
        letter: 'e',
        arg: Arg::No,
        id: "ed",
    },
    Short {
        letter: 'E',
        arg: Arg::No,
        id: "ignore-tab-expansion",
    },
    Short {
        letter: 'i',
        arg: Arg::No,
        id: "ignore-case",
    },
    Short {
        letter: 'I',
        arg: Arg::Required,
        id: "ignore-matching-lines",
    },
    Short {
        letter: 'L',
        arg: Arg::Required,
        id: "label",
    },
    Short {
        letter: 'N',
        arg: Arg::No,
        id: "new-file",
    },
    Short {
        letter: 'q',
        arg: Arg::No,
        id: "brief",
    },
    Short {
        letter: 'r',
        arg: Arg::No,
        id: "recursive",
    },
    Short {
        letter: 's',
        arg: Arg::No,
        id: "report-identical-files",
    },
    Short {
        letter: 't',
        arg: Arg::No,
        id: "expand-tabs",
    },
    Short {
        letter: 'T',
        arg: Arg::No,
        id: "initial-tab",
    },
    Short {
        letter: 'u',
        arg: Arg::No,
        id: "unified",
    },
    Short {
        letter: 'U',
        arg: Arg::Required,
        id: "unified",
    },
    Short {
        letter: 'v',
        arg: Arg::No,
        id: "version",
    },
    Short {
        letter: 'w',
        arg: Arg::No,
        id: "ignore-all-space",
    },
    Short {
        letter: 'W',
        arg: Arg::Required,
        id: "width",
    },
    Short {
        letter: 'x',
        arg: Arg::Required,
        id: "exclude",
    },
    Short {
        letter: 'y',
        arg: Arg::No,
        id: "side-by-side",
    },
    Short {
        letter: 'Z',
        arg: Arg::No,
        id: "ignore-trailing-space",
    },
    Short {
        letter: '0',
        arg: Arg::No,
        id: "0",
    },
    Short {
        letter: '1',
        arg: Arg::No,
        id: "1",
    },
    Short {
        letter: '2',
        arg: Arg::No,
        id: "2",
    },
    Short {
        letter: '3',
        arg: Arg::No,
        id: "3",
    },
    Short {
        letter: '4',
        arg: Arg::No,
        id: "4",
    },
    Short {
        letter: '5',
        arg: Arg::No,
        id: "5",
    },
    Short {
        letter: '6',
        arg: Arg::No,
        id: "6",
    },
    Short {
        letter: '7',
        arg: Arg::No,
        id: "7",
    },
    Short {
        letter: '8',
        arg: Arg::No,
        id: "8",
    },
    Short {
        letter: '9',
        arg: Arg::No,
        id: "9",
    },
    // GNU options cash's diff does not have, refused by name below.
    Short {
        letter: 'D',
        arg: Arg::Required,
        id: "unsupported",
    },
    Short {
        letter: 'F',
        arg: Arg::Required,
        id: "unsupported",
    },
    Short {
        letter: 'l',
        arg: Arg::No,
        id: "unsupported",
    },
    Short {
        letter: 'n',
        arg: Arg::No,
        id: "unsupported",
    },
    Short {
        letter: 'p',
        arg: Arg::No,
        id: "unsupported",
    },
    Short {
        letter: 'S',
        arg: Arg::Required,
        id: "unsupported",
    },
    Short {
        letter: 'X',
        arg: Arg::Required,
        id: "unsupported",
    },
];

const LONGS: &[Long] = &[
    Long {
        name: "brief",
        arg: Arg::No,
        id: "brief",
    },
    Long {
        name: "color",
        arg: Arg::Optional,
        id: "color",
    },
    Long {
        name: "context",
        arg: Arg::Optional,
        id: "context",
    },
    Long {
        name: "ed",
        arg: Arg::No,
        id: "ed",
    },
    Long {
        name: "exclude",
        arg: Arg::Required,
        id: "exclude",
    },
    Long {
        name: "expand-tabs",
        arg: Arg::No,
        id: "expand-tabs",
    },
    Long {
        name: "help",
        arg: Arg::No,
        id: "help",
    },
    Long {
        name: "horizon-lines",
        arg: Arg::Required,
        id: "minimal",
    },
    Long {
        name: "ignore-all-space",
        arg: Arg::No,
        id: "ignore-all-space",
    },
    Long {
        name: "ignore-blank-lines",
        arg: Arg::No,
        id: "ignore-blank-lines",
    },
    Long {
        name: "ignore-case",
        arg: Arg::No,
        id: "ignore-case",
    },
    Long {
        name: "ignore-matching-lines",
        arg: Arg::Required,
        id: "ignore-matching-lines",
    },
    Long {
        name: "ignore-space-change",
        arg: Arg::No,
        id: "ignore-space-change",
    },
    Long {
        name: "ignore-tab-expansion",
        arg: Arg::No,
        id: "ignore-tab-expansion",
    },
    Long {
        name: "ignore-trailing-space",
        arg: Arg::No,
        id: "ignore-trailing-space",
    },
    Long {
        name: "initial-tab",
        arg: Arg::No,
        id: "initial-tab",
    },
    Long {
        name: "label",
        arg: Arg::Required,
        id: "label",
    },
    Long {
        name: "left-column",
        arg: Arg::No,
        id: "left-column",
    },
    Long {
        name: "minimal",
        arg: Arg::No,
        id: "minimal",
    },
    Long {
        name: "new-file",
        arg: Arg::No,
        id: "new-file",
    },
    Long {
        name: "normal",
        arg: Arg::No,
        id: "normal",
    },
    Long {
        name: "recursive",
        arg: Arg::No,
        id: "recursive",
    },
    Long {
        name: "report-identical-files",
        arg: Arg::No,
        id: "report-identical-files",
    },
    Long {
        name: "side-by-side",
        arg: Arg::No,
        id: "side-by-side",
    },
    Long {
        name: "speed-large-files",
        arg: Arg::No,
        id: "minimal",
    },
    Long {
        name: "strip-trailing-cr",
        arg: Arg::No,
        id: "strip-trailing-cr",
    },
    Long {
        name: "suppress-common-lines",
        arg: Arg::No,
        id: "suppress-common-lines",
    },
    Long {
        name: "tabsize",
        arg: Arg::Required,
        id: "tabsize",
    },
    Long {
        name: "text",
        arg: Arg::No,
        id: "text",
    },
    Long {
        name: "unified",
        arg: Arg::Optional,
        id: "unified",
    },
    Long {
        name: "version",
        arg: Arg::No,
        id: "version",
    },
    Long {
        name: "width",
        arg: Arg::Required,
        id: "width",
    },
    // GNU options cash's diff does not have, refused by name below.
    Long {
        name: "changed-group-format",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "exclude-from",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "from-file",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "ifdef",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "ignore-file-name-case",
        arg: Arg::No,
        id: "unsupported",
    },
    Long {
        name: "line-format",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "new-group-format",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "new-line-format",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "no-dereference",
        arg: Arg::No,
        id: "unsupported",
    },
    Long {
        name: "no-ignore-file-name-case",
        arg: Arg::No,
        id: "unsupported",
    },
    Long {
        name: "old-group-format",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "old-line-format",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "paginate",
        arg: Arg::No,
        id: "unsupported",
    },
    Long {
        name: "palette",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "rcs",
        arg: Arg::No,
        id: "unsupported",
    },
    Long {
        name: "show-c-function",
        arg: Arg::No,
        id: "unsupported",
    },
    Long {
        name: "show-function-line",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "starting-file",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "suppress-blank-empty",
        arg: Arg::No,
        id: "unsupported",
    },
    Long {
        name: "to-file",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "unchanged-group-format",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "unchanged-line-format",
        arg: Arg::Required,
        id: "unsupported",
    },
    Long {
        name: "unidirectional-new-file",
        arg: Arg::No,
        id: "unsupported",
    },
];

/// Reads `args`, the whole command line with the tool's name first.
#[allow(
    clippy::too_many_lines,
    reason = "one arm per option, as GNU's diff.c has it"
)]
pub fn parse_params(args: &[OsString]) -> Result<Request, Refusal> {
    let (executable, rest) = args.split_first().map_or_else(
        || (OsString::from("diff"), args),
        |(exe, rest)| (exe.clone(), rest),
    );
    let parsed = getopt::parse(rest, SHORTS, LONGS).map_err(Refusal::Usage)?;
    let mut params = Params {
        executable,
        option_words: parsed.option_words,
        ..Default::default()
    };
    let mut operands: Vec<OsString> = Vec::new();
    let mut format: Option<Format> = None;
    let mut context: Option<usize> = None;
    let mut number: Option<usize> = None;
    let mut color: Option<String> = None;
    let mut last_was_digit = false;

    let set_format = |format: &mut Option<Format>, wanted: Format| -> Result<(), Refusal> {
        if format.is_some_and(|have| have != wanted) {
            return Err(Refusal::Usage("conflicting output style options".into()));
        }
        *format = Some(wanted);
        Ok(())
    };

    for item in parsed.items {
        let (id, value) = match item {
            Item::Operand(word) => {
                operands.push(word);
                continue;
            }
            Item::Option { id, value } => (id, value),
        };
        let was_digit = last_was_digit;
        last_was_digit = id.len() == 1 && id.as_bytes()[0].is_ascii_digit();
        let text = value.as_ref().map(|v| v.to_string_lossy().into_owned());
        match id {
            "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                // GNU's old `-NUM`: digits in a row form the context count.
                let digit: usize = id.parse().unwrap_or(0);
                number = Some(if was_digit {
                    number.unwrap_or(0).saturating_mul(10).saturating_add(digit)
                } else {
                    digit
                });
            }
            "help" => return Ok(Request::Help),
            "version" => return Ok(Request::Version),
            "text" => params.text = true,
            "ignore-space-change" => params.ignore_space_change = true,
            "ignore-blank-lines" => params.ignore_blank_lines = true,
            "ignore-all-space" => params.ignore_all_space = true,
            "ignore-case" => params.ignore_case = true,
            "ignore-tab-expansion" => params.ignore_tab_expansion = true,
            "ignore-trailing-space" => params.ignore_trailing_space = true,
            "ignore-matching-lines" => {
                let pattern = text.unwrap_or_default();
                let regex = regex::bytes::Regex::new(&pattern).map_err(|_| {
                    Refusal::Usage(format!("invalid regular expression {}", quote(&pattern)))
                })?;
                params.ignore_regexps.push(regex);
            }
            "strip-trailing-cr" => params.strip_trailing_cr = true,
            "brief" => params.brief = true,
            "report-identical-files" => params.report_identical_files = true,
            "expand-tabs" => params.expand_tabs = true,
            "initial-tab" => params.initial_tab = true,
            "new-file" => params.new_file = true,
            "recursive" => params.recursive = true,
            "suppress-common-lines" => params.suppress_common_lines = true,
            "left-column" => params.left_column = true,
            "minimal" => {}
            "normal" => set_format(&mut format, Format::Normal)?,
            "ed" => set_format(&mut format, Format::Ed)?,
            "side-by-side" => set_format(&mut format, Format::SideBySide)?,
            "context" | "unified" => {
                let wanted = if id == "context" {
                    Format::Context
                } else {
                    Format::Unified
                };
                set_format(&mut format, wanted)?;
                if let Some(text) = text {
                    context = Some(text.parse().map_err(|_| {
                        Refusal::Usage(format!("invalid context length {}", quote(&text)))
                    })?);
                }
            }
            "label" => {
                if params.labels.len() == 2 {
                    return Err(Refusal::Plain("too many file label options".into()));
                }
                params.labels.push(text.unwrap_or_default());
            }
            "width" => {
                let text = text.unwrap_or_default();
                params.width = text
                    .parse()
                    .ok()
                    .filter(|&width| width > 0)
                    .ok_or_else(|| Refusal::Usage(format!("invalid width {}", quote(&text))))?;
            }
            "tabsize" => {
                let text = text.unwrap_or_default();
                params.tabsize =
                    text.parse().ok().filter(|&size| size > 0).ok_or_else(|| {
                        Refusal::Usage(format!("invalid tabsize {}", quote(&text)))
                    })?;
            }
            "exclude" => {
                let text = text.unwrap_or_default();
                let pattern = glob::Pattern::new(&text).map_err(|_| {
                    Refusal::Usage(format!("invalid exclude pattern {}", quote(&text)))
                })?;
                params.excludes.push(pattern);
            }
            "color" => color = Some(text.unwrap_or_else(|| "auto".into())),
            "unsupported" => {
                let word = params
                    .option_words
                    .iter()
                    .rev()
                    .find(|w| w.to_string_lossy().starts_with('-'))
                    .map(|w| w.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let word = word.split('=').next().unwrap_or_default().to_owned();
                return Err(Refusal::Usage(format!(
                    "option {} is not supported by cash's diff",
                    quote(&word)
                )));
            }
            _ => return Err(Refusal::Usage(format!("unrecognized option '{id}'"))),
        }
    }

    params.format = format.unwrap_or_default();
    if let Some(count) = context.or(number) {
        params.context_count = count;
    }
    params.color = match color.as_deref() {
        None | Some("never" | "no" | "none") => false,
        Some("always" | "yes" | "force") => true,
        Some("auto" | "tty" | "if-tty") => std::io::stdout().is_terminal(),
        Some(other) => {
            return Err(Refusal::Usage(format!("invalid color {}", quote(other))));
        }
    };

    let mut operands = operands.into_iter();
    let from = operands.next().ok_or_else(|| {
        Refusal::Usage(format!(
            "missing operand after {}",
            quote(&params.executable.to_string_lossy())
        ))
    })?;
    let to = operands.next().ok_or_else(|| {
        Refusal::Usage(format!(
            "missing operand after {}",
            quote(&from.to_string_lossy())
        ))
    })?;
    if let Some(extra) = operands.next() {
        return Err(Refusal::Usage(format!(
            "extra operand {}",
            quote(&extra.to_string_lossy())
        )));
    }
    params.from = from;
    params.to = to;
    Ok(Request::Compare(Box::new(params)))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::field_reassign_with_default,
    clippy::assert_is_empty,
    clippy::panic_in_result_fn,
    clippy::format_collect,
    reason = "a test stops loudly, and sets the options it is about"
)]
mod tests {
    use super::*;

    fn parse(list: &[&str]) -> Result<Params, Refusal> {
        let mut args = vec![OsString::from("diff")];
        args.extend(list.iter().map(OsString::from));
        match parse_params(&args)? {
            Request::Compare(params) => Ok(*params),
            other => panic!("not a comparison: {other:?}"),
        }
    }

    #[test]
    fn formats_and_context() {
        let p = parse(&["-u", "a", "b"]).expect("parses");
        assert_eq!((p.format, p.context_count), (Format::Unified, 3));
        let p = parse(&["-U2", "a", "b"]).expect("parses");
        assert_eq!((p.format, p.context_count), (Format::Unified, 2));
        let p = parse(&["-U", "0", "a", "b"]).expect("parses");
        assert_eq!((p.format, p.context_count), (Format::Unified, 0));
        let p = parse(&["--unified=5", "a", "b"]).expect("parses");
        assert_eq!(p.context_count, 5);
        let p = parse(&["-u1", "a", "b"]).expect("parses");
        assert_eq!((p.format, p.context_count), (Format::Unified, 1));
        let p = parse(&["-c12", "a", "b"]).expect("parses");
        assert_eq!((p.format, p.context_count), (Format::Context, 12));
        let p = parse(&["-C", "1", "a", "b"]).expect("parses");
        assert_eq!((p.format, p.context_count), (Format::Context, 1));
        let p = parse(&["-y", "-W", "40", "a", "b"]).expect("parses");
        assert_eq!((p.format, p.width), (Format::SideBySide, 40));
        let p = parse(&["--brie", "a", "b"]).expect("parses");
        assert!(p.brief);
    }

    #[test]
    fn bundled_and_permuted() {
        let p = parse(&["a", "b", "-ruN"]).expect("parses");
        assert!(p.recursive && p.new_file && p.format == Format::Unified);
        assert_eq!(p.option_words, vec![OsString::from("-ruN")]);
        let p = parse(&["-L", "x", "--label=y", "a", "b"]).expect("parses");
        assert_eq!(p.labels, vec!["x", "y"]);
    }

    #[test]
    fn refusals() {
        let err = |list: &[&str]| parse(list).unwrap_err();
        assert_eq!(
            err(&["a"]),
            Refusal::Usage("missing operand after 'a'".into())
        );
        assert_eq!(
            err(&[]),
            Refusal::Usage("missing operand after 'diff'".into())
        );
        assert_eq!(
            err(&["a", "b", "c"]),
            Refusal::Usage("extra operand 'c'".into())
        );
        assert_eq!(
            err(&["-U", "x", "a", "b"]),
            Refusal::Usage("invalid context length 'x'".into())
        );
        assert_eq!(
            err(&["-u", "-c", "a", "b"]),
            Refusal::Usage("conflicting output style options".into())
        );
        assert_eq!(
            err(&["-L", "a", "-L", "b", "-L", "c", "a", "b"]),
            Refusal::Plain("too many file label options".into())
        );
        assert_eq!(
            err(&["--color=foo", "a", "b"]),
            Refusal::Usage("invalid color 'foo'".into())
        );
        assert_eq!(
            err(&["-W", "x", "a", "b"]),
            Refusal::Usage("invalid width 'x'".into())
        );
        assert_eq!(
            err(&["--tabsize=0", "a", "b"]),
            Refusal::Usage("invalid tabsize '0'".into())
        );
        assert_eq!(
            err(&["--foo", "a", "b"]),
            Refusal::Usage("unrecognized option '--foo'".into())
        );
        assert_eq!(
            err(&["-p", "a", "b"]),
            Refusal::Usage("option '-p' is not supported by cash's diff".into())
        );
        assert_eq!(
            err(&["--from-file=x", "a", "b"]),
            Refusal::Usage("option '--from-file' is not supported by cash's diff".into())
        );
    }

    #[test]
    fn help_and_version() {
        let args = [OsString::from("diff"), OsString::from("--help")];
        assert!(matches!(parse_params(&args), Ok(Request::Help)));
        let args = [OsString::from("diff"), OsString::from("-v")];
        assert!(matches!(parse_params(&args), Ok(Request::Version)));
    }
}
