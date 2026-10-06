// This file is part of cash's copy of the uutils diffutils package (CASH-PATCHES.md).
//
// For the full copyright and license information, please view the LICENSE-*
// files that was distributed with this source code.

//! A `getopt_long`-style option parser, as the GNU tools read their command lines.
//!
//! Bundled short options (`-ruN`), an option's argument attached or in the next word
//! (`-U3`, `-U 3`, `--unified=3`), unambiguous prefixes of long options (`--brie`),
//! operands anywhere, `--` ending the options, and getopt's own messages for what is
//! wrong.

use std::ffi::OsString;

/// Whether an option takes an argument.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Arg {
    /// Never.
    No,
    /// Always: attached, or the next word.
    Required,
    /// Only attached (`--color=always`, never `--color always`).
    Optional,
}

/// A short option: its letter, whether it takes an argument, and the name the caller
/// knows it by.
#[derive(Clone, Copy, Debug)]
pub struct Short {
    pub letter: char,
    pub arg: Arg,
    pub id: &'static str,
}

/// A long option, without its dashes.
#[derive(Clone, Copy, Debug)]
pub struct Long {
    pub name: &'static str,
    pub arg: Arg,
    pub id: &'static str,
}

/// One thing the command line said.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Item {
    /// An option, by the id of its table entry, with its argument when it has one.
    Option {
        id: &'static str,
        value: Option<OsString>,
    },
    /// A word that is not an option.
    Operand(OsString),
}

/// The command line, read.
#[derive(Debug, Default)]
pub struct Parsed {
    /// Options and operands, in the order given.
    pub items: Vec<Item>,
    /// The words that were options or their arguments, as written: what `diff -r`
    /// repeats before each pair of files it compares.
    pub option_words: Vec<OsString>,
}

/// Reads `args` (without the program's name) by the two tables. The error is getopt's
/// message, without the program's name.
pub fn parse(args: &[OsString], shorts: &[Short], longs: &[Long]) -> Result<Parsed, String> {
    let mut parsed = Parsed::default();
    let mut words = args.iter();
    while let Some(word) = words.next() {
        let text = word.to_string_lossy();
        if text == "--" {
            parsed.items.extend(words.map(|w| Item::Operand(w.clone())));
            break;
        }
        if let Some(long) = text.strip_prefix("--") {
            parsed.option_words.push(word.clone());
            let (name, attached) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (long, None),
            };
            let entry = find_long(name, longs).map_err(|err| match err {
                LongError::Unknown => format!("unrecognized option '--{long}'"),
                LongError::Ambiguous(names) => {
                    let list: Vec<String> = names.iter().map(|n| format!("'--{n}'")).collect();
                    format!(
                        "option '--{name}' is ambiguous; possibilities: {}",
                        list.join(" ")
                    )
                }
            })?;
            let value = match (entry.arg, attached) {
                (Arg::No, Some(_)) => {
                    return Err(format!(
                        "option '--{}' doesn't allow an argument",
                        entry.name
                    ));
                }
                (Arg::No, None) => None,
                (Arg::Required | Arg::Optional, Some(value)) => Some(OsString::from(value)),
                (Arg::Optional, None) => None,
                (Arg::Required, None) => {
                    let Some(next) = words.next() else {
                        return Err(format!("option '--{}' requires an argument", entry.name));
                    };
                    parsed.option_words.push(next.clone());
                    Some(next.clone())
                }
            };
            parsed.items.push(Item::Option {
                id: entry.id,
                value,
            });
            continue;
        }
        if text.len() > 1 && text.starts_with('-') {
            parsed.option_words.push(word.clone());
            let mut letters = text.char_indices().skip(1);
            while let Some((at, letter)) = letters.next() {
                let Some(entry) = shorts.iter().find(|s| s.letter == letter) else {
                    return Err(format!("invalid option -- '{letter}'"));
                };
                let value = match entry.arg {
                    Arg::No => None,
                    Arg::Required | Arg::Optional => {
                        let rest = text.get(at + letter.len_utf8()..).unwrap_or_default();
                        if !rest.is_empty() {
                            // The rest of the word is the argument.
                            while letters.next().is_some() {}
                            Some(OsString::from(rest))
                        } else if entry.arg == Arg::Optional {
                            None
                        } else {
                            let Some(next) = words.next() else {
                                return Err(format!("option requires an argument -- '{letter}'"));
                            };
                            parsed.option_words.push(next.clone());
                            Some(next.clone())
                        }
                    }
                };
                parsed.items.push(Item::Option {
                    id: entry.id,
                    value,
                });
            }
            continue;
        }
        parsed.items.push(Item::Operand(word.clone()));
    }
    Ok(parsed)
}

enum LongError {
    Unknown,
    Ambiguous(Vec<&'static str>),
}

/// The long option `name` names: itself, or the one it is an unambiguous prefix of.
fn find_long<'a>(name: &str, longs: &'a [Long]) -> Result<&'a Long, LongError> {
    if let Some(exact) = longs.iter().find(|l| l.name == name) {
        return Ok(exact);
    }
    let candidates: Vec<&Long> = longs.iter().filter(|l| l.name.starts_with(name)).collect();
    match candidates.as_slice() {
        [] => Err(LongError::Unknown),
        [one] => Ok(one),
        many => {
            // Prefixes of options that are the same option (`--unified` and a synonym)
            // are no ambiguity.
            let first = many[0].id;
            if many.iter().all(|l| l.id == first) {
                Ok(many[0])
            } else {
                Err(LongError::Ambiguous(many.iter().map(|l| l.name).collect()))
            }
        }
    }
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

    const SHORTS: &[Short] = &[
        Short {
            letter: 'r',
            arg: Arg::No,
            id: "recursive",
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
            letter: 'L',
            arg: Arg::Required,
            id: "label",
        },
    ];
    const LONGS: &[Long] = &[
        Long {
            name: "recursive",
            arg: Arg::No,
            id: "recursive",
        },
        Long {
            name: "report-identical-files",
            arg: Arg::No,
            id: "identical",
        },
        Long {
            name: "unified",
            arg: Arg::Optional,
            id: "unified",
        },
        Long {
            name: "label",
            arg: Arg::Required,
            id: "label",
        },
    ];

    fn words(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    fn opt(id: &'static str, value: Option<&str>) -> Item {
        Item::Option {
            id,
            value: value.map(OsString::from),
        }
    }

    #[test]
    fn bundles_and_attached_arguments() {
        let parsed = parse(
            &words(&["-ru", "-U3", "-U", "4", "a", "-Lx", "b"]),
            SHORTS,
            LONGS,
        )
        .expect("parses");
        assert_eq!(
            parsed.items,
            vec![
                opt("recursive", None),
                opt("unified", None),
                opt("unified", Some("3")),
                opt("unified", Some("4")),
                Item::Operand(OsString::from("a")),
                opt("label", Some("x")),
                Item::Operand(OsString::from("b")),
            ]
        );
        assert_eq!(
            parsed.option_words,
            words(&["-ru", "-U3", "-U", "4", "-Lx"])
        );
    }

    #[test]
    fn long_options_and_prefixes() {
        let parsed = parse(
            &words(&[
                "--rec",
                "--unified=2",
                "--unified",
                "--label",
                "L",
                "--",
                "-x",
            ]),
            SHORTS,
            LONGS,
        )
        .expect("parses");
        assert_eq!(
            parsed.items,
            vec![
                opt("recursive", None),
                opt("unified", Some("2")),
                opt("unified", None),
                opt("label", Some("L")),
                Item::Operand(OsString::from("-x")),
            ]
        );
    }

    #[test]
    fn getopts_messages() {
        let err = |list: &[&str]| parse(&words(list), SHORTS, LONGS).unwrap_err();
        assert_eq!(err(&["--foo"]), "unrecognized option '--foo'");
        assert_eq!(err(&["--foo=1"]), "unrecognized option '--foo=1'");
        assert_eq!(
            err(&["--recursive=1"]),
            "option '--recursive' doesn't allow an argument"
        );
        assert_eq!(err(&["--label"]), "option '--label' requires an argument");
        assert_eq!(
            err(&["--re"]),
            "option '--re' is ambiguous; possibilities: '--recursive' '--report-identical-files'"
        );
        assert_eq!(err(&["-z"]), "invalid option -- 'z'");
        assert_eq!(err(&["-L"]), "option requires an argument -- 'L'");
    }

    #[test]
    fn a_lone_dash_is_an_operand() {
        let parsed = parse(&words(&["-", "a"]), SHORTS, LONGS).expect("parses");
        assert_eq!(
            parsed.items,
            vec![
                Item::Operand(OsString::from("-")),
                Item::Operand(OsString::from("a"))
            ]
        );
        assert!(parsed.option_words.is_empty());
    }
}
