//! GNU `getopt_long`, once, for every cash tool that reads its command line as the GNU
//! tools do (spec D78).
//!
//! Short options clustered (`-ruN`); a value attached or in the next word (`-U3`,
//! `-U 3`, `--unified=3`); unique prefixes of long options (`--brie`), where prefixes of
//! one option under two names are no ambiguity, as glibc has it; operands anywhere, or,
//! when asked, ending the options at the first one; `--` ending them in any case; a lone
//! `-` an operand; `getopt_long_only`'s long options with one dash; and GNU tar's
//! old-style keys (`tar xzf a.tgz`, [`old_style`]).
//!
//! What the command line said comes back in its order, each item with the index of the
//! word it came from, so a tool to which order matters (`tar -C dir file`) or a run of
//! digits in one word (`grep -15`) can tell. What is wrong comes back typed
//! ([`Problem`]); its `Display` is glibc's wording, which a tool prefixes with its name,
//! or words otherwise.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::fmt;

/// Whether an option takes a value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Arg {
    /// Never.
    No,
    /// Always: attached, or the next word.
    Required,
    /// Only attached (`--color=always`, `-c5`; never `--color always`).
    Optional,
}

/// A short option: its letter, whether it takes a value, and what the caller knows it by.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Short<Id> {
    /// The letter after the dash.
    pub letter: char,
    /// Whether it takes a value.
    pub arg: Arg,
    /// What the caller knows it by; options that are one option share it.
    pub id: Id,
}

/// A long option, without its dashes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Long<'n, Id> {
    /// The name after the dashes.
    pub name: &'n str,
    /// Whether it takes a value.
    pub arg: Arg,
    /// What the caller knows it by; names of one option share it.
    pub id: Id,
}

impl<Id> Short<Id> {
    /// A short option, for a table.
    pub const fn new(letter: char, arg: Arg, id: Id) -> Self {
        Self { letter, arg, id }
    }
}

impl<'n, Id> Long<'n, Id> {
    /// A long option, for a table.
    pub const fn new(name: &'n str, arg: Arg, id: Id) -> Self {
        Self { name, arg, id }
    }
}

/// Short options from a C option string, each known by its letter: `b:` takes a value,
/// `c::` takes one only attached. A leading `+`, `-` or `:` (getopt's order and silence)
/// is the caller's to read first.
pub fn optstring(spec: &str) -> Vec<Short<char>> {
    let letters: Vec<char> = spec.chars().collect();
    let mut shorts = Vec::new();
    let mut at = 0;
    while let Some(&letter) = letters.get(at) {
        at += 1;
        if letter == ':' {
            continue;
        }
        let arg = match (letters.get(at), letters.get(at + 1)) {
            (Some(':'), Some(':')) => Arg::Optional,
            (Some(':'), _) => Arg::Required,
            _ => Arg::No,
        };
        shorts.push(Short::new(letter, arg, letter));
    }
    shorts
}

/// Short options from a C option string, each known by its letter as text.
///
/// As [`optstring`], but the id is the letter's own place in `spec`, for tables whose
/// long options are known by names (`"color"`) beside the letters (`"c"`).
pub fn optstring_ids(spec: &'static str) -> Vec<Short<&'static str>> {
    let mut shorts = Vec::new();
    let mut letters = spec.char_indices().peekable();
    while let Some((at, letter)) = letters.next() {
        if letter == ':' {
            continue;
        }
        let mut arg = Arg::No;
        if letters.next_if(|(_, c)| *c == ':').is_some() {
            arg = Arg::Required;
            if letters.next_if(|(_, c)| *c == ':').is_some() {
                arg = Arg::Optional;
            }
        }
        if let Some(id) = spec.get(at..at + letter.len_utf8()) {
            shorts.push(Short::new(letter, arg, id));
        }
    }
    shorts
}

/// When the options end, besides at `--`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Order {
    /// Never: options and operands mix (GNU's default).
    #[default]
    Permute,
    /// At the first operand (`+` in an option string, `POSIXLY_CORRECT`, util-linux's
    /// `flock` and `watch`).
    StopAtOperand,
}

/// A word of a command line: `String` for a builtin's arguments, `OsString` for a
/// bundled tool's.
pub trait Word: Clone {
    /// The word as text, lossily where it is not Unicode.
    fn text(&self) -> Cow<'_, str>;
    /// A word made of part of another: an attached value.
    fn from_text(text: &str) -> Self;
}

impl Word for String {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(self)
    }

    fn from_text(text: &str) -> Self {
        text.to_owned()
    }
}

impl Word for OsString {
    fn text(&self) -> Cow<'_, str> {
        self.to_string_lossy()
    }

    fn from_text(text: &str) -> Self {
        Self::from(text)
    }
}

/// How an option was written: its letter, or its long name in full (`--rec` is
/// `--recursive`), for a message that names it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Written {
    /// `-x`.
    Short(char),
    /// `--name`, the whole name.
    Long(String),
}

impl fmt::Display for Written {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Short(letter) => write!(f, "-{letter}"),
            Self::Long(name) => write!(f, "--{name}"),
        }
    }
}

/// One thing the command line said.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Item<Id, W> {
    /// An option, by its id, with its value when it has one.
    Option {
        /// The id of its table entry.
        id: Id,
        /// Its value: attached, or the next word.
        value: Option<W>,
        /// The index of the word the option was in.
        word: usize,
        /// How it was written.
        written: Written,
    },
    /// A word that is not an option.
    Operand {
        /// The word.
        value: W,
        /// Its index.
        word: usize,
    },
}

/// A command line, read.
#[derive(Clone, Debug)]
pub struct Parsed<Id, W> {
    /// Options and operands, in the order given.
    pub items: Vec<Item<Id, W>>,
    /// The index in `items` where the options ended, at `--` or, with
    /// [`Order::StopAtOperand`], at the first operand; `items.len()` when they never did.
    pub options_end: usize,
    /// The words that were options or their values, as written: what `diff -r` repeats
    /// before each pair of files it compares.
    pub option_words: Vec<W>,
}

impl<Id, W> Default for Parsed<Id, W> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            options_end: 0,
            option_words: Vec::new(),
        }
    }
}

impl<Id, W> Parsed<Id, W> {
    /// The operands, in order.
    pub fn operands(&self) -> impl Iterator<Item = &W> {
        self.items.iter().filter_map(|item| match item {
            Item::Operand { value, .. } => Some(value),
            Item::Option { .. } => None,
        })
    }
}

/// What is wrong with a command line. `Display` words it as glibc does, without the
/// program's name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Problem {
    /// `-x` is no option.
    UnknownShort {
        /// The letter.
        letter: char,
    },
    /// `-x` needs a value and the words ran out.
    ShortNeedsValue {
        /// The letter.
        letter: char,
    },
    /// `--foo` is no option, nor the prefix of one.
    UnknownLong {
        /// The dashes it was written with: `--`, or `-` for a long-only option.
        dashes: &'static str,
        /// The word after the dashes, `=value` and all.
        text: String,
    },
    /// `--re` is the prefix of more than one option.
    Ambiguous {
        /// The dashes it was written with.
        dashes: &'static str,
        /// The word after the dashes, `=value` and all, as glibc repeats it.
        text: String,
        /// The options it could be, in the table's order.
        candidates: Vec<String>,
    },
    /// `--label` needs a value and the words ran out.
    LongNeedsValue {
        /// The dashes it was written with.
        dashes: &'static str,
        /// The option's full name.
        name: String,
    },
    /// `--recursive=1`: the option takes no value.
    LongTakesNoValue {
        /// The dashes it was written with.
        dashes: &'static str,
        /// The option's full name.
        name: String,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownShort { letter } => write!(f, "invalid option -- '{letter}'"),
            Self::ShortNeedsValue { letter } => {
                write!(f, "option requires an argument -- '{letter}'")
            }
            Self::UnknownLong { dashes, text } => write!(f, "unrecognized option '{dashes}{text}'"),
            Self::Ambiguous {
                dashes,
                text,
                candidates,
            } => {
                write!(f, "option '{dashes}{text}' is ambiguous; possibilities:")?;
                for candidate in candidates {
                    write!(f, " '{dashes}{candidate}'")?;
                }
                Ok(())
            }
            Self::LongNeedsValue { dashes, name } => {
                write!(f, "option '{dashes}{name}' requires an argument")
            }
            Self::LongTakesNoValue { dashes, name } => {
                write!(f, "option '{dashes}{name}' doesn't allow an argument")
            }
        }
    }
}

impl std::error::Error for Problem {}

/// Why a long option name names no option.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LongMiss {
    /// Nothing starts with it.
    Unknown,
    /// More than one option starts with it: their names, in the table's order.
    Ambiguous(Vec<String>),
}

/// Reads command lines by two option tables.
#[derive(Clone, Copy, Debug)]
pub struct Getopt<'t, 'n, Id> {
    shorts: &'t [Short<Id>],
    longs: &'t [Long<'n, Id>],
    order: Order,
    long_only: bool,
}

impl<'t, 'n, Id: Copy + PartialEq> Getopt<'t, 'n, Id> {
    /// A parser by these tables, permuting, long options with two dashes.
    pub const fn new(shorts: &'t [Short<Id>], longs: &'t [Long<'n, Id>]) -> Self {
        Self {
            shorts,
            longs,
            order: Order::Permute,
            long_only: false,
        }
    }

    /// When the options end besides at `--`.
    #[must_use]
    pub const fn order(mut self, order: Order) -> Self {
        self.order = order;
        self
    }

    /// `getopt_long_only`: long options with one dash too.
    #[must_use]
    pub const fn long_only(mut self, long_only: bool) -> Self {
        self.long_only = long_only;
        self
    }

    /// The short option with this letter.
    pub fn short(&self, letter: char) -> Option<&'t Short<Id>> {
        self.shorts.iter().find(|short| short.letter == letter)
    }

    /// The long option `name` names: itself, or the one it is a unique prefix of. Names
    /// that are one option (the same id and the same kind of value) are no ambiguity.
    pub fn long(&self, name: &str) -> Result<&'t Long<'n, Id>, LongMiss> {
        if let Some(exact) = self.longs.iter().find(|long| long.name == name) {
            return Ok(exact);
        }
        let candidates: Vec<&'t Long<'n, Id>> = self
            .longs
            .iter()
            .filter(|long| long.name.starts_with(name))
            .collect();
        let Some(first) = candidates.first() else {
            return Err(LongMiss::Unknown);
        };
        if candidates
            .iter()
            .all(|long| long.id == first.id && long.arg == first.arg)
        {
            Ok(first)
        } else {
            Err(LongMiss::Ambiguous(
                candidates.iter().map(|long| long.name.to_owned()).collect(),
            ))
        }
    }

    /// Reads `args` (without the program's name) as `getopt_long`'s loop does: each
    /// option, operand and problem in turn, in the order of the command line. A tool that
    /// acts on an option as it comes, as GNU's do (`--help` before a bad option after it),
    /// reads this; [`Getopt::parse`] and [`Getopt::parse_all`] are built on it.
    pub const fn read<'a, W: Word>(&self, args: &'a [W]) -> Reader<'t, 'n, 'a, Id, W> {
        Reader {
            parser: *self,
            args,
            at: 0,
            ended: false,
            yielded: 0,
            current: 0,
            options_end: None,
            option_words: Vec::new(),
            queue: VecDeque::new(),
        }
    }

    /// Reads `args`, stopping at the first problem.
    pub fn parse<W: Word>(&self, args: &[W]) -> Result<Parsed<Id, W>, Problem> {
        let mut reader = self.read(args);
        let mut items = Vec::new();
        for next in reader.by_ref() {
            items.push(next?);
        }
        Ok(reader.finish(items))
    }

    /// Reads `args` past every problem, as util-linux's `getopt` does, so that each one
    /// is named: what was read, and the problems in order.
    pub fn parse_all<W: Word>(&self, args: &[W]) -> (Parsed<Id, W>, Vec<Problem>) {
        let mut reader = self.read(args);
        let mut items = Vec::new();
        let mut problems = Vec::new();
        for next in reader.by_ref() {
            match next {
                Ok(item) => items.push(item),
                Err(problem) => problems.push(problem),
            }
        }
        (reader.finish(items), problems)
    }
}

/// A command line being read: what it says, in order, as an iterator.
#[derive(Debug)]
pub struct Reader<'t, 'n, 'a, Id, W> {
    parser: Getopt<'t, 'n, Id>,
    args: &'a [W],
    /// The next word to read.
    at: usize,
    ended: bool,
    /// The options and operands handed out so far.
    yielded: usize,
    /// The word whose options, operand or problems are being handed out.
    current: usize,
    options_end: Option<usize>,
    option_words: Vec<W>,
    /// What the last word said and has not been handed out yet: a cluster can say
    /// several things.
    queue: VecDeque<Result<Item<Id, W>, Problem>>,
}

impl<Id: Copy + PartialEq, W: Word> Iterator for Reader<'_, '_, '_, Id, W> {
    type Item = Result<Item<Id, W>, Problem>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(next) = self.queue.pop_front() {
                if next.is_ok() {
                    self.yielded += 1;
                }
                return Some(next);
            }
            let index = self.at;
            self.current = index;
            let word = self.args.get(index)?.clone();
            self.at += 1;
            self.step(word, index);
        }
    }
}

impl<Id: Copy + PartialEq, W: Word> Reader<'_, '_, '_, Id, W> {
    /// How many options and operands came before the options ended, at `--` or, with
    /// [`Order::StopAtOperand`], at the first operand; `None` while they have not.
    pub const fn options_end(&self) -> Option<usize> {
        self.options_end
    }

    /// The index of the word the last thing handed out came from: for a problem, which
    /// the problem does not say.
    pub const fn current_word(&self) -> usize {
        self.current
    }

    /// The words read so far that were options or their values, as written.
    pub fn option_words(&self) -> &[W] {
        &self.option_words
    }

    /// The whole reading, once its items are collected.
    fn finish(self, items: Vec<Item<Id, W>>) -> Parsed<Id, W> {
        let options_end = self.options_end.unwrap_or(items.len());
        Parsed {
            items,
            options_end,
            option_words: self.option_words,
        }
    }

    /// One word: an operand, `--`, a long option or a cluster of short ones.
    fn step(&mut self, word: W, index: usize) {
        if self.ended {
            self.queue.push_back(Ok(Item::Operand {
                value: word,
                word: index,
            }));
            return;
        }
        let text = word.text().into_owned();
        if text == "--" {
            self.end();
            return;
        }
        if text.len() < 2 || !text.starts_with('-') {
            if self.parser.order == Order::StopAtOperand {
                self.end();
            }
            self.queue.push_back(Ok(Item::Operand {
                value: word,
                word: index,
            }));
            return;
        }
        self.option_words.push(word);
        // Without long options, `getopt` (not `getopt_long`) reads `--foo` as letters:
        // `invalid option -- '-'`.
        if let Some(long) = text.strip_prefix("--")
            && !self.parser.longs.is_empty()
        {
            self.long_option("--", long, index);
            return;
        }
        let body = text.get(1..).unwrap_or_default();
        if self.parser.long_only {
            let name = body.split_once('=').map_or(body, |(name, _)| name);
            let first_is_short = body
                .chars()
                .next()
                .and_then(|letter| self.parser.short(letter))
                .is_some();
            // `getopt_long_only`: a long option if one matches, unless the word is one
            // short option; else short options.
            let single_short = first_is_short && body.chars().count() == 1;
            if (self.parser.long(name).is_ok() && !single_short) || !first_is_short {
                self.long_option("-", body, index);
                return;
            }
        }
        self.short_cluster(body, index);
    }

    const fn end(&mut self) {
        if !self.ended {
            self.ended = true;
            // Nothing of the word that ends the options is waiting.
            self.options_end = Some(self.yielded);
        }
    }

    /// The next word, taken as an option's value.
    fn take_value(&mut self) -> Option<W> {
        let value = self.args.get(self.at)?.clone();
        self.at += 1;
        self.option_words.push(value.clone());
        Some(value)
    }

    fn option(&mut self, id: Id, value: Option<W>, word: usize, written: Written) {
        self.queue.push_back(Ok(Item::Option {
            id,
            value,
            word,
            written,
        }));
    }

    fn long_option(&mut self, dashes: &'static str, text: &str, index: usize) {
        let (name, attached) = match text.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (text, None),
        };
        let entry = match self.parser.long(name) {
            Ok(entry) => entry,
            Err(LongMiss::Unknown) => {
                self.queue.push_back(Err(Problem::UnknownLong {
                    dashes,
                    text: text.to_owned(),
                }));
                return;
            }
            Err(LongMiss::Ambiguous(candidates)) => {
                self.queue.push_back(Err(Problem::Ambiguous {
                    dashes,
                    text: text.to_owned(),
                    candidates,
                }));
                return;
            }
        };
        let value = match (entry.arg, attached) {
            (Arg::No, Some(_)) => {
                self.queue.push_back(Err(Problem::LongTakesNoValue {
                    dashes,
                    name: entry.name.to_owned(),
                }));
                return;
            }
            (Arg::No | Arg::Optional, None) => None,
            (Arg::Required | Arg::Optional, Some(value)) => Some(W::from_text(value)),
            (Arg::Required, None) => {
                let Some(next) = self.take_value() else {
                    self.queue.push_back(Err(Problem::LongNeedsValue {
                        dashes,
                        name: entry.name.to_owned(),
                    }));
                    return;
                };
                Some(next)
            }
        };
        self.option(entry.id, value, index, Written::Long(entry.name.to_owned()));
    }

    /// A cluster of short options: each letter in turn, a bad one said and the rest still
    /// read, as glibc reads them; the first that takes a value takes the rest of the
    /// cluster or the next word.
    fn short_cluster(&mut self, body: &str, index: usize) {
        for (at, letter) in body.char_indices() {
            let Some(entry) = self.parser.short(letter) else {
                self.queue.push_back(Err(Problem::UnknownShort { letter }));
                continue;
            };
            let rest = body.get(at + letter.len_utf8()..).unwrap_or_default();
            match entry.arg {
                Arg::No => self.option(entry.id, None, index, Written::Short(letter)),
                Arg::Optional => {
                    let value = (!rest.is_empty()).then(|| W::from_text(rest));
                    self.option(entry.id, value, index, Written::Short(letter));
                    return;
                }
                Arg::Required => {
                    if !rest.is_empty() {
                        self.option(
                            entry.id,
                            Some(W::from_text(rest)),
                            index,
                            Written::Short(letter),
                        );
                    } else if let Some(next) = self.take_value() {
                        self.option(entry.id, Some(next), index, Written::Short(letter));
                    } else {
                        self.queue
                            .push_back(Err(Problem::ShortNeedsValue { letter }));
                    }
                    return;
                }
            }
        }
    }
}

/// GNU tar's old-style keys, as dashed options.
///
/// When the first word does not start with `-`, each of its letters is an option, and
/// the letters that take a value take the words after it, in order (`tar cvfb a.tar 20
/// dir` is `tar -c -v -f a.tar -b 20 dir`). Other command lines come back as they are.
pub fn old_style<W: Word, Id>(args: &[W], shorts: &[Short<Id>]) -> Vec<W> {
    let Some(first) = args.first() else {
        return Vec::new();
    };
    let keys = first.text();
    if keys.is_empty() || keys.starts_with('-') {
        return args.to_vec();
    }
    let mut out = Vec::with_capacity(args.len() + keys.len());
    let mut next = 1;
    for letter in keys.chars() {
        let mut key = String::from("-");
        key.push(letter);
        out.push(W::from_text(&key));
        let takes = shorts
            .iter()
            .find(|short| short.letter == letter)
            .is_some_and(|short| short.arg == Arg::Required);
        if takes && let Some(value) = args.get(next) {
            out.push(value.clone());
            next += 1;
        }
    }
    out.extend(args.iter().skip(next).cloned());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHORTS: &[Short<&str>] = &[
        Short::new('r', Arg::No, "recursive"),
        Short::new('u', Arg::No, "unified"),
        Short::new('U', Arg::Required, "unified"),
        Short::new('L', Arg::Required, "label"),
        Short::new('c', Arg::Optional, "context"),
    ];
    const LONGS: &[Long<'static, &str>] = &[
        Long::new("recursive", Arg::No, "recursive"),
        Long::new("report-identical-files", Arg::No, "identical"),
        Long::new("unified", Arg::Optional, "unified"),
        Long::new("label", Arg::Required, "label"),
    ];

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|word| (*word).to_owned()).collect()
    }

    fn parser() -> Getopt<'static, 'static, &'static str> {
        Getopt::new(SHORTS, LONGS)
    }

    /// An item in one line: `id=value as written @word`, or `operand @word`.
    fn said(item: &Item<&str, String>) -> String {
        match item {
            Item::Option {
                id,
                value,
                word,
                written,
            } => match value {
                Some(value) => format!("{id}={value} {written} @{word}"),
                None => format!("{id} {written} @{word}"),
            },
            Item::Operand { value, word } => format!("{value} @{word}"),
        }
    }

    fn all_said(items: &[Item<&str, String>]) -> Vec<String> {
        items.iter().map(said).collect()
    }

    #[test]
    fn clusters_attached_values_and_words_in_order() {
        let parsed = parser()
            .parse(&words(&[
                "-ru", "-U3", "-U", "4", "a", "-Lx", "b", "-c", "-c7",
            ]))
            .unwrap();
        assert_eq!(
            all_said(&parsed.items),
            [
                "recursive -r @0",
                "unified -u @0",
                "unified=3 -U @1",
                "unified=4 -U @2",
                "a @4",
                "label=x -L @5",
                "b @6",
                "context -c @7",
                "context=7 -c @8",
            ]
        );
        assert_eq!(
            parsed.option_words,
            words(&["-ru", "-U3", "-U", "4", "-Lx", "-c", "-c7"])
        );
        assert_eq!(parsed.options_end, parsed.items.len());
        assert_eq!(parsed.operands().collect::<Vec<_>>(), ["a", "b"]);
    }

    #[test]
    fn long_options_prefixes_and_the_double_dash() {
        let parsed = parser()
            .parse(&words(&[
                "--rec",
                "--unified=2",
                "--unified",
                "--label",
                "L",
                "--",
                "-x",
            ]))
            .unwrap();
        assert_eq!(
            all_said(&parsed.items),
            [
                "recursive --recursive @0",
                "unified=2 --unified @1",
                "unified --unified @2",
                "label=L --label @3",
                "-x @6",
            ]
        );
        assert_eq!(parsed.options_end, 4);
    }

    #[test]
    fn names_of_one_option_are_no_ambiguity() {
        let entry = |name, arg, id| Long { name, arg, id };
        let longs = [
            entry("uncompress", Arg::No, 'd'),
            entry("unzip", Arg::No, 'd'),
            entry("stdout", Arg::No, 'c'),
            entry("stop", Arg::No, 's'),
            entry("level", Arg::Required, 'l'),
            entry("lever", Arg::Optional, 'l'),
        ];
        let getopt = Getopt::new(&[], &longs);
        // Two names of one option: the first is taken.
        assert_eq!(getopt.long("un").unwrap().name, "uncompress");
        assert_eq!(getopt.long("unz").unwrap().name, "unzip");
        assert_eq!(
            getopt.long("st"),
            Err(LongMiss::Ambiguous(vec!["stdout".into(), "stop".into()]))
        );
        // One id with another kind of value is another option, as glibc compares them.
        assert_eq!(
            getopt.long("lev"),
            Err(LongMiss::Ambiguous(vec!["level".into(), "lever".into()]))
        );
        assert_eq!(getopt.long("stop").unwrap().id, 's');
        assert_eq!(getopt.long("nope"), Err(LongMiss::Unknown));
    }

    #[test]
    fn glibcs_messages() {
        let err = |list: &[&str]| parser().parse(&words(list)).unwrap_err().to_string();
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
        // glibc repeats the whole word, its value too (checked against GNU gzip, grep,
        // diff and column).
        assert_eq!(
            err(&["--re=1"]),
            "option '--re=1' is ambiguous; possibilities: '--recursive' '--report-identical-files'"
        );
        assert_eq!(err(&["-z"]), "invalid option -- 'z'");
        assert_eq!(err(&["-L"]), "option requires an argument -- 'L'");
    }

    #[test]
    fn a_lone_dash_is_an_operand() {
        let parsed = parser().parse(&words(&["-", "a"])).unwrap();
        assert_eq!(all_said(&parsed.items), ["- @0", "a @1"]);
        assert_eq!(parsed.option_words, Vec::<String>::new());
    }

    #[test]
    fn stopping_at_the_first_operand() {
        let parsed = parser()
            .order(Order::StopAtOperand)
            .parse(&words(&["-r", "cmd", "-u", "--", "x"]))
            .unwrap();
        assert_eq!(
            all_said(&parsed.items),
            ["recursive -r @0", "cmd @1", "-u @2", "-- @3", "x @4"]
        );
        assert_eq!(parsed.options_end, 1);
    }

    #[test]
    fn every_problem_is_collected_and_the_reading_goes_on() {
        let (parsed, problems) = parser().parse_all(&words(&["-zry", "--foo", "a", "-L"]));
        assert_eq!(
            problems.iter().map(ToString::to_string).collect::<Vec<_>>(),
            [
                "invalid option -- 'z'",
                "invalid option -- 'y'",
                "unrecognized option '--foo'",
                "option requires an argument -- 'L'",
            ]
        );
        assert_eq!(all_said(&parsed.items), ["recursive -r @0", "a @2"]);
    }

    #[test]
    fn the_reader_hands_out_options_and_problems_in_their_order() {
        // As `getopt_long`'s loop: `gzip -h --foo` sees `-h` first and prints its help.
        let args = words(&["-rz", "--foo", "x", "--", "-u"]);
        let mut reader = parser().read(&args);
        let mut next = || {
            reader.next().map(|next| match next {
                Ok(item) => said(&item),
                Err(problem) => format!("error: {problem}"),
            })
        };
        assert_eq!(next().as_deref(), Some("recursive -r @0"));
        assert_eq!(next().as_deref(), Some("error: invalid option -- 'z'"));
        assert_eq!(
            next().as_deref(),
            Some("error: unrecognized option '--foo'")
        );
        assert_eq!(next().as_deref(), Some("x @2"));
        assert_eq!(next().as_deref(), Some("-u @4"));
        assert_eq!(next(), None);
        assert_eq!(reader.options_end(), Some(2));
        assert_eq!(reader.option_words(), ["-rz", "--foo"]);
    }

    #[test]
    fn long_only_takes_single_dash_long_options() {
        let getopt = parser().long_only(true);
        let parsed = getopt
            .parse(&words(&["-label", "x", "-r", "-rec", "-ru"]))
            .unwrap();
        assert_eq!(
            all_said(&parsed.items),
            [
                "label=x --label @0",
                "recursive -r @2",
                "recursive --recursive @3",
                "recursive -r @4",
                "unified -u @4",
            ]
        );
        assert_eq!(
            getopt.parse(&words(&["-zz"])).unwrap_err().to_string(),
            "unrecognized option '-zz'"
        );
    }

    #[test]
    fn os_strings_read_as_strings_do() {
        let args: Vec<OsString> = ["-U5", "f"].iter().map(OsString::from).collect();
        let parsed = parser().parse(&args).unwrap();
        assert_eq!(
            parsed.items,
            vec![
                Item::Option {
                    id: "unified",
                    value: Some(OsString::from("5")),
                    word: 0,
                    written: Written::Short('U'),
                },
                Item::Operand {
                    value: OsString::from("f"),
                    word: 1
                },
            ]
        );
    }

    #[test]
    fn without_long_options_two_dashes_are_letters() {
        // `getopt`, not `getopt_long`: as netcat reads its command line.
        let shorts = optstring("lp:");
        let getopt: Getopt<'_, '_, char> = Getopt::new(&shorts, &[]);
        assert_eq!(
            getopt.parse(&words(&["--lp"])).unwrap_err().to_string(),
            "invalid option -- '-'"
        );
        let parsed = getopt.parse(&words(&["-l", "--", "-p"])).unwrap();
        assert_eq!(parsed.operands().collect::<Vec<_>>(), ["-p"]);
    }

    #[test]
    fn option_strings_make_short_tables() {
        assert_eq!(
            optstring("ab:c::d"),
            [
                Short::new('a', Arg::No, 'a'),
                Short::new('b', Arg::Required, 'b'),
                Short::new('c', Arg::Optional, 'c'),
                Short::new('d', Arg::No, 'd'),
            ]
        );
        assert_eq!(optstring(""), []);
        assert_eq!(
            optstring_ids("ab:c::d"),
            [
                Short::new('a', Arg::No, "a"),
                Short::new('b', Arg::Required, "b"),
                Short::new('c', Arg::Optional, "c"),
                Short::new('d', Arg::No, "d"),
            ]
        );
    }

    #[test]
    fn tars_old_style_keys() {
        let tar_shorts = optstring("cvf:b:");
        assert_eq!(
            old_style(&words(&["cvfb", "a.tar", "20", "dir"]), &tar_shorts),
            words(&["-c", "-v", "-f", "a.tar", "-b", "20", "dir"])
        );
        assert_eq!(
            old_style(&words(&["-cf", "a.tar"]), &tar_shorts),
            words(&["-cf", "a.tar"])
        );
        // A key without its value is left for the parser to report.
        assert_eq!(
            old_style(&words(&["cf"]), &tar_shorts),
            words(&["-c", "-f"])
        );
        assert_eq!(
            old_style::<String, char>(&[], &tar_shorts),
            Vec::<String>::new()
        );
    }
}
