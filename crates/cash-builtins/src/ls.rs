//! Native Windows-optimized pure-Rust `ls` builtin.
//!
//! Provides accurate Windows file ownership (SIDs -> Account Names), NTFS hard link
//! counts, directory subfolder link counts, proper permissions, sorting, and colored
//! output matching POSIX / BusyBox conventions.
//!
//! cash (D67) adds what lsd shows beside GNU `ls`'s own options: icons (`--icons`, Nerd
//! Font glyphs from lsd's theme when Windows Terminal draws the tab in a Nerd Font, none
//! otherwise, or always with `--icons-theme=fancy`), a colour
//! for each kind of file (`LS_COLORS`, or `dircolors`' defaults without it), a tree
//! (`--tree`, `--depth`), `--group-directories-first`, and the sorts `-X`, `-v`, `-U` and
//! `--sort=WORD`.

use std::cell::OnceCell;
use std::cmp::Ordering;
use std::fs::Metadata;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use cash_core::builtins::{ContentOptions, ContentType};
use cash_core::timefmt::Zone;
use cash_core::{ExecutionResult, builtins};
use cash_getopt::{Arg, Getopt, Item, Long, Order};
use chrono::{DateTime, Utc};
use clap::Parser;
use unicode_width::UnicodeWidthStr as _;

use crate::ls_icon_table;
use crate::options;

const HELP: &str = "\
Usage: ls [OPTION]... [FILE]...
List information about the FILEs (the current directory by default).

  -a, --all                  do not ignore entries starting with . or files
                               both hidden and system, as Explorer hides them
  -A, --almost-all           like -a, but do not list implied . and ..
  -b, --escape               print C-style escapes for nongraphic characters
  -B, --ignore-backups       do not list implied entries ending with ~
  -c                         with -lt: sort by, and show, the change time (when
                               the file's contents, name or attributes last
                               changed, NTFS's ChangeTime); with -l: show it and
                               sort by name; otherwise: sort by it, newest first
  -C                         list entries by columns
  -d, --directory            list directories themselves, not their contents
  -F, --classify             append indicator (one of */@) to entries
      --file-type            likewise, except do not append '*'
      --format=WORD          across -x, commas -m, horizontal -x, long -l,
                               single-column -1, verbose -l, vertical -C
      --full-time            like -l --time-style=full-iso
  -g                         like -l, but do not list the owner
  -G, --no-group             in a long listing, do not print the group
  -h, --human-readable       with -l and -s, print sizes like 1K 234M 2G
      --si                   likewise, but use powers of 1000 not 1024
      --block-size=SIZE      with -l, scale sizes by SIZE when printing them, and
                               -s's blocks: a number, a unit (K, M, G, T, P, E;
                               KB for 1000, KiB for 1024, shown after each size
                               when there is no number) or both; human-readable,
                               si; LS_BLOCK_SIZE or BLOCK_SIZE give it otherwise
      --hide=PATTERN         do not list implied entries matching shell PATTERN
                               (overridden by -a or -A)
  -I, --ignore=PATTERN       do not list implied entries matching shell PATTERN
  -k, --kibibytes            -s's blocks and the totals in 1024 bytes
      --indicator-style=WORD  append an indicator with style WORD to names:
                               none (the default), slash (-p),
                               file-type (--file-type), classify (-F)
  -l                         use a long listing format
  -L, --dereference          show what a link names rather than the link
  -m                         fill the width with names separated by commas
  -N, --literal              print names as they are, without quoting
  -o                         like -l, but do not list the group
  -p, --indicator-style=slash
                             append / to directories
  -q, --hide-control-chars   print ? instead of nongraphic characters
      --show-control-chars   show nongraphic characters as they are (the default
                               unless the output is a terminal)
  -Q, --quote-name           enclose names in double quotes
      --quoting-style=WORD   how names are quoted: literal, locale, shell,
                               shell-always, shell-escape, shell-escape-always,
                               c, c-maybe, escape, clocale; QUOTING_STYLE gives it
                               otherwise, and on a terminal it is shell-escape
  -r, --reverse              reverse order while sorting
  -R, --recursive            list subdirectories recursively
  -s, --size                 print the space each file takes, in blocks
  -S                         sort by file size, largest first
  -t                         sort by time, newest first; see --time
      --time=WORD            the time -l shows and -t sorts by: atime, access,
                               use (-u); ctime, status (-c); mtime, modification
                               (the default); birth, creation; without -l, any
                               but mtime sorts by itself
  -u                         like -c, with the time it was last read
  -U                         do not sort; list entries in directory order
  -v                         natural sort of (version) numbers within text
  -w, --width=COLS           set the output width to COLS; 0 means no limit
  -x                         list entries by lines instead of by columns
  -X                         sort alphabetically by entry extension
  -1                         list one file per line
      --sort=WORD            sort by WORD instead of name: none (-U), size (-S),
                               time (-t), version (-v), extension (-X), name
      --group-directories-first
                             group directories before files
      --time-style=STYLE     how -l shows times: full-iso, long-iso, iso,
                               locale (the default), or +FORMAT, strftime's,
                               FORMAT1 and FORMAT2 on two lines for old and
                               recent files; posix-STYLE is the default here;
                               TIME_STYLE gives it when the option is not given
      --color[=WHEN]         color the output: always, auto (the default), never;
                               names take LS_COLORS's colors, or dircolors'
                               defaults; with -l, the other columns take lsd's
      --icons[=WHEN]         show an icon before each name: always, auto (when the
                               output is a terminal, the default), never
      --icons-theme=THEME    auto (the default: Nerd Font glyphs when Windows
                               Terminal draws this tab in a Nerd Font, no icons
                               anywhere else) or fancy (the glyphs regardless)
      --attributes           show Windows' attributes as lsd does: d (or .),
                               archive, read-only, hidden, system (`.a-h-`)
      --count-links          with -l, count a folder's links as Linux has them,
                               2 and one per folder in it, reading each folder;
                               without it a folder shows 1, as in Git Bash
      --tree                 list the directories as a tree
      --depth=NUM            descend at most NUM levels, with --tree or -R
      --help                 display this help and exit
";

/// List information about the FILEs (the current directory by default).
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one field per flag, as clap derives them"
)]
pub(crate) struct LsCommand {
    /// Display this help and exit.
    #[arg(long = "help")]
    help: bool,

    /// List files in the long format.
    #[arg(short = 'l')]
    long: bool,

    /// Do not ignore entries starting with .
    #[arg(short = 'a', long = "all")]
    all: bool,

    /// Do not list implied . and ..
    #[arg(short = 'A', long = "almost-all")]
    almost_all: bool,

    /// List directories themselves, not their contents.
    #[arg(short = 'd', long = "directory")]
    directory: bool,

    /// With -l, print sizes like 1K 234M 2G etc.
    #[arg(short = 'h', long = "human-readable")]
    human_readable: bool,

    /// Reverse order while sorting.
    #[arg(short = 'r', long = "reverse")]
    reverse: bool,

    /// List subdirectories recursively.
    #[arg(short = 'R', long = "recursive")]
    recursive: bool,

    /// Sort by time, newest first.
    #[arg(short = 't')]
    sort_by_time: bool,

    /// Sort by file size, largest first.
    #[arg(short = 'S')]
    sort_by_size: bool,

    /// Sort alphabetically by entry extension.
    #[arg(short = 'X')]
    sort_by_extension: bool,

    /// Natural sort of (version) numbers within text.
    #[arg(short = 'v')]
    sort_by_version: bool,

    /// Do not sort; list entries in directory order.
    #[arg(short = 'U')]
    unsorted: bool,

    /// Sort by WORD instead of name.
    #[arg(long = "sort", value_name = "WORD")]
    sort: Option<String>,

    /// Group directories before files.
    #[arg(long = "group-directories-first")]
    group_directories_first: bool,

    /// With -l, count each folder's links (2 and one per folder in it), reading it.
    #[arg(long = "count-links")]
    count_links: bool,

    /// Colorize the output (always, auto, never).
    #[arg(
        long = "color",
        value_name = "WHEN",
        num_args = 0..=1,
        default_missing_value = "always",
        require_equals = true
    )]
    color: Option<String>,

    /// Show an icon before each name (always, auto, never).
    #[arg(
        long = "icons",
        value_name = "WHEN",
        num_args = 0..=1,
        default_missing_value = "auto",
        require_equals = true
    )]
    icons: Option<String>,

    /// When the Nerd Font glyphs show: auto (by the tab's font) or fancy (always).
    #[arg(long = "icons-theme", value_name = "THEME")]
    icons_theme: Option<String>,

    /// Show Windows' attributes: directory, archive, read-only, hidden, system.
    #[arg(long = "attributes")]
    attributes: bool,

    /// List the directories as a tree.
    #[arg(long = "tree")]
    tree: bool,

    /// Descend at most this many levels, with --tree or -R.
    #[arg(long = "depth", value_name = "NUM")]
    depth: Option<usize>,

    /// Files or directories to list.
    #[arg(value_name = "FILE")]
    paths: Vec<String>,

    /// How entries are laid out outside the long format: `-1`, `-C`, `-x`, `-m`.
    #[arg(skip)]
    layout: Layout,

    /// The marks after names: `-p`, `--file-type`, `-F`, `--indicator-style`.
    #[arg(skip)]
    indicator: Indicator,

    /// `--classify=auto`: the marks only on a terminal, which `execute` says in `tty`.
    #[arg(skip)]
    classify_auto: bool,

    /// `-g`: the long format without the owner.
    #[arg(skip)]
    no_owner: bool,

    /// `-o`, `-G`: the long format without the group.
    #[arg(skip)]
    no_group: bool,

    /// `-w`: the line's width, 0 for no limit.
    #[arg(skip)]
    width: Option<usize>,

    /// `-I`: names not listed, however `-a` asks.
    #[arg(skip)]
    ignore: Vec<NamePattern>,

    /// `--hide`: names not listed unless `-a` or `-A` asks.
    #[arg(skip)]
    hide: Vec<NamePattern>,

    /// `-B`: names ending with `~` not listed.
    #[arg(skip)]
    ignore_backups: bool,

    /// `--time-style`, or `full-iso` from `--full-time`.
    #[arg(skip)]
    time_style: Option<String>,

    /// `-L`: what a link names is shown, and gone into with `-R`.
    #[arg(skip)]
    dereference: bool,

    /// A folder `-RL` would have gone round: said, status 2.
    #[arg(skip)]
    looped: std::sync::atomic::AtomicBool,

    /// Whether the command's output is a terminal, for `--classify=auto`.
    #[arg(skip)]
    tty: bool,

    /// `-c`, `-u`, `--time`: which of a file's times `-l` shows and `-t` sorts by.
    #[arg(skip)]
    time_kind: TimeKind,

    /// `-s`: each entry's blocks before it.
    #[arg(skip)]
    show_blocks: bool,

    /// `-k`: blocks of 1024 bytes, whatever `BLOCK_SIZE` says.
    #[arg(skip)]
    kibibytes: bool,

    /// `-h`, `--si` or `--block-size`, the last given: how sizes and blocks are shown.
    #[arg(skip)]
    scale: Option<Scale>,

    /// `-N`, `-b`, `-Q` or `--quoting-style`, the last given: how names are quoted.
    #[arg(skip)]
    quoting: Option<QuotingStyle>,

    /// `-q` or `--show-control-chars`, the last given: whether a nongraphic character
    /// left as it is shows as `?`.
    #[arg(skip)]
    hide_control: Option<bool>,

    /// The words as given, read in `execute`, where `POSIXLY_CORRECT` can be seen.
    #[arg(skip)]
    raw: Vec<String>,

    /// What ends `ls` before it lists: `--version`, or a command line it refuses.
    #[arg(skip)]
    early: Option<Early>,
}

/// What ends `ls` before it lists anything.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Early {
    Version,
    /// A mistake `getopt_long` finds, or a value refused: said with the Try line.
    Usage(String),
    /// A word that is none of an option's own (`--format=x`), said with the Try line and
    /// status 1, as GNU's `argmatch` says it.
    Argmatch(String),
    /// Said as it is, status 2: one of GNU's options cash's ls does not have, or a width
    /// that is no number.
    Said(String),
}

/// How entries are laid out outside the long format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Layout {
    /// One per line into a pipe or a file, in columns on a terminal.
    #[default]
    Default,
    /// `-1`.
    OneColumn,
    /// `-C`: down the columns.
    Columns,
    /// `-x`: across the lines.
    Across,
    /// `-m`: separated by commas.
    Commas,
}

/// The marks after names, GNU's `--indicator-style`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Indicator {
    #[default]
    None,
    /// `/` after a folder (`-p`).
    Slash,
    /// `-F`'s marks but `*` (`--file-type`).
    FileType,
    /// `*/@|` (`-F`).
    Classify,
}

/// A shell pattern of `-I` or `--hide`, matched as GNU ls matches it: `fnmatch` with
/// `FNM_PERIOD`, a leading dot matched only by a dot. One that does not parse matches
/// itself.
#[derive(Debug)]
struct NamePattern {
    text: String,
    pattern: Option<glob::Pattern>,
}

impl NamePattern {
    fn new(text: String) -> Self {
        let pattern = glob::Pattern::new(&text).ok();
        Self { text, pattern }
    }

    fn matches(&self, name: &str) -> bool {
        let options = glob::MatchOptions {
            case_sensitive: true,
            require_literal_separator: false,
            require_literal_leading_dot: true,
        };
        self.pattern.as_ref().map_or(self.text == name, |pattern| {
            pattern.matches_with(name, options)
        })
    }
}

/// GNU coreutils 9's short options, each known by its letter.
const SHORTS: &str = "abcdfghiklmnopqrstuvw:xABCDFGHI:LNQRST:UXZ1";

/// GNU coreutils 9's long options, known by their letter where they have one, and cash's
/// own (`--icons`, `--icons-theme`, `--attributes`, `--count-links`, `--tree`,
/// `--depth`), so that a name cut short is read, or found ambiguous, as GNU's would be.
const LONGS: &[Long<'static, &str>] = &[
    Long::new("all", Arg::No, "a"),
    Long::new("escape", Arg::No, "b"),
    Long::new("directory", Arg::No, "d"),
    Long::new("dired", Arg::No, "D"),
    Long::new("full-time", Arg::No, "full-time"),
    Long::new(
        "group-directories-first",
        Arg::No,
        "group-directories-first",
    ),
    Long::new("human-readable", Arg::No, "h"),
    Long::new("inode", Arg::No, "i"),
    Long::new("kibibytes", Arg::No, "k"),
    Long::new("numeric-uid-gid", Arg::No, "n"),
    Long::new("no-group", Arg::No, "G"),
    Long::new("hide-control-chars", Arg::No, "q"),
    Long::new("reverse", Arg::No, "r"),
    Long::new("size", Arg::No, "s"),
    Long::new("width", Arg::Required, "w"),
    Long::new("almost-all", Arg::No, "A"),
    Long::new("ignore-backups", Arg::No, "B"),
    Long::new("classify", Arg::Optional, "F"),
    Long::new("file-type", Arg::No, "file-type"),
    Long::new("si", Arg::No, "si"),
    Long::new("dereference-command-line", Arg::No, "H"),
    Long::new(
        "dereference-command-line-symlink-to-dir",
        Arg::No,
        "dereference-command-line-symlink-to-dir",
    ),
    Long::new("hide", Arg::Required, "hide"),
    Long::new("ignore", Arg::Required, "I"),
    Long::new("indicator-style", Arg::Required, "indicator-style"),
    Long::new("dereference", Arg::No, "L"),
    Long::new("literal", Arg::No, "N"),
    Long::new("quote-name", Arg::No, "Q"),
    Long::new("quoting-style", Arg::Required, "quoting-style"),
    Long::new("recursive", Arg::No, "R"),
    Long::new("format", Arg::Required, "format"),
    Long::new("show-control-chars", Arg::No, "show-control-chars"),
    Long::new("sort", Arg::Required, "sort"),
    Long::new("tabsize", Arg::Required, "T"),
    Long::new("time", Arg::Required, "time"),
    Long::new("time-style", Arg::Required, "time-style"),
    Long::new("zero", Arg::No, "zero"),
    Long::new("color", Arg::Optional, "color"),
    Long::new("hyperlink", Arg::Optional, "hyperlink"),
    Long::new("block-size", Arg::Required, "block-size"),
    Long::new("context", Arg::No, "Z"),
    Long::new("author", Arg::No, "author"),
    Long::new("help", Arg::No, "help"),
    Long::new("version", Arg::No, "version"),
    Long::new("icons", Arg::Optional, "icons"),
    Long::new("icons-theme", Arg::Required, "icons-theme"),
    Long::new("attributes", Arg::No, "attributes"),
    Long::new("count-links", Arg::No, "count-links"),
    Long::new("tree", Arg::No, "tree"),
    Long::new("depth", Arg::Required, "depth"),
];

impl LsCommand {
    /// Reads the command line as GNU's `getopt_long` loop does, in order, options anywhere
    /// until `--` unless `order` says otherwise; `--help` and `--version` end the reading
    /// where they stand.
    /// A command line with no words.
    const fn empty() -> Self {
        Self {
            help: false,
            long: false,
            all: false,
            almost_all: false,
            directory: false,
            human_readable: false,
            reverse: false,
            recursive: false,
            sort_by_time: false,
            sort_by_size: false,
            sort_by_extension: false,
            sort_by_version: false,
            unsorted: false,
            sort: None,
            group_directories_first: false,
            count_links: false,
            color: None,
            icons: None,
            icons_theme: None,
            attributes: false,
            tree: false,
            depth: None,
            paths: Vec::new(),
            layout: Layout::Default,
            indicator: Indicator::None,
            classify_auto: false,
            no_owner: false,
            no_group: false,
            width: None,
            ignore: Vec::new(),
            hide: Vec::new(),
            ignore_backups: false,
            time_style: None,
            dereference: false,
            looped: std::sync::atomic::AtomicBool::new(false),
            tty: false,
            time_kind: TimeKind::Modification,
            show_blocks: false,
            kibibytes: false,
            scale: None,
            quoting: None,
            hide_control: None,
            raw: Vec::new(),
            early: None,
        }
    }

    fn parse(args: &[String], order: Order) -> Self {
        let mut command = Self::empty();
        let shorts = cash_getopt::optstring_ids(SHORTS);
        for item in Getopt::new(&shorts, LONGS).order(order).read(args) {
            let (id, value, written) = match item {
                Err(problem) => {
                    command.early = Some(Early::Usage(problem.to_string()));
                    return command;
                }
                Ok(Item::Operand { value, .. }) => {
                    command.paths.push(value);
                    continue;
                }
                Ok(Item::Option {
                    id, value, written, ..
                }) => (id, value, written),
            };
            match id {
                "help" => {
                    command.help = true;
                    return command;
                }
                "version" => {
                    command.early = Some(Early::Version);
                    return command;
                }
                "l" => command.long = true,
                "a" => command.all = true,
                "A" => command.almost_all = true,
                "1" | "C" | "x" | "m" | "o" | "g" | "G" | "format" | "w" | "full-time"
                | "time-style" | "p" | "file-type" | "indicator-style" | "F" => {
                    let taken = if matches!(id, "p" | "file-type" | "indicator-style" | "F") {
                        command.indicator_option(id, value)
                    } else {
                        command.layout_option(id, value)
                    };
                    if let Err(early) = taken {
                        command.early = Some(early);
                        return command;
                    }
                }
                "I" => command
                    .ignore
                    .push(NamePattern::new(value.unwrap_or_default())),
                "hide" => command
                    .hide
                    .push(NamePattern::new(value.unwrap_or_default())),
                "B" => command.ignore_backups = true,
                "L" => command.dereference = true,
                "d" => command.directory = true,
                "h" | "si" | "block-size" | "k" | "s" | "c" | "u" | "time" => {
                    if let Err(early) = command.size_or_time_option(id, value) {
                        command.early = Some(early);
                        return command;
                    }
                }
                "N" | "b" | "Q" | "quoting-style" | "q" | "show-control-chars" => {
                    if let Err(early) = command.quoting_option(id, value) {
                        command.early = Some(early);
                        return command;
                    }
                }
                "r" => command.reverse = true,
                "R" => command.recursive = true,
                "t" => command.sort_by_time = true,
                "S" => command.sort_by_size = true,
                "X" => command.sort_by_extension = true,
                "v" => command.sort_by_version = true,
                "U" => command.unsorted = true,
                // GNU's -f is -a and -U.
                "f" => {
                    command.all = true;
                    command.unsorted = true;
                }
                "sort" => command.sort = value,
                "group-directories-first" => command.group_directories_first = true,
                "count-links" => command.count_links = true,
                "color" => command.color = Some(value.unwrap_or_else(|| "always".to_owned())),
                "icons" => command.icons = Some(value.unwrap_or_else(|| "auto".to_owned())),
                "icons-theme" => command.icons_theme = value,
                "attributes" => command.attributes = true,
                "tree" => command.tree = true,
                "depth" => {
                    let text = value.unwrap_or_default();
                    let Ok(depth) = text.trim().parse() else {
                        command.early = Some(Early::Usage(format!(
                            "invalid argument '{text}' for '--depth'"
                        )));
                        return command;
                    };
                    command.depth = Some(depth);
                }
                _ => {
                    command.early = Some(Early::Said(format!(
                        "{written} is not supported by cash's ls"
                    )));
                    return command;
                }
            }
        }
        command
    }

    /// `-1`, `-C`, `-x`, `-m`, `-o`, `-g`, `-G`, `--format`, `-w`, `--full-time` and
    /// `--time-style`: how entries are laid out, and the long format's times.
    fn layout_option(&mut self, id: &str, value: Option<String>) -> Result<(), Early> {
        match id {
            // -1 does nothing after -l, as GNU's own source says; -C, -x and -m end it.
            "1" if !self.long => self.layout = Layout::OneColumn,
            "1" => {}
            "C" | "x" | "m" => {
                self.long = false;
                self.layout = match id {
                    "C" => Layout::Columns,
                    "x" => Layout::Across,
                    _ => Layout::Commas,
                };
            }
            "o" => {
                self.long = true;
                self.no_group = true;
            }
            "g" => {
                self.long = true;
                self.no_owner = true;
            }
            "G" => self.no_group = true,
            "w" => {
                let text = value.unwrap_or_default();
                let Ok(width) = text.trim().parse() else {
                    return Err(Early::Said(format!("invalid line width: '{text}'")));
                };
                self.width = Some(width);
            }
            "full-time" => {
                self.long = true;
                self.time_style = Some("full-iso".to_owned());
            }
            "time-style" => self.time_style = value,
            "format" => {
                let word = value.unwrap_or_default();
                match word.as_str() {
                    "verbose" | "long" => self.long = true,
                    "commas" | "horizontal" | "across" | "vertical" | "single-column" => {
                        self.long = false;
                        self.layout = match word.as_str() {
                            "commas" => Layout::Commas,
                            "vertical" => Layout::Columns,
                            "single-column" => Layout::OneColumn,
                            _ => Layout::Across,
                        };
                    }
                    _ => {
                        return Err(Early::Argmatch(format!(
                            "invalid argument '{word}' for '--format'\n\
                                 Valid arguments are:\n  - 'verbose', 'long'\n  - 'commas'\n  \
                                 - 'horizontal', 'across'\n  - 'vertical'\n  - 'single-column'"
                        )));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// `-h`, `--si`, `--block-size`, `-k` and `-s`: how sizes and blocks are shown; `-c`,
    /// `-u` and `--time`: which time is.
    fn size_or_time_option(&mut self, id: &str, value: Option<String>) -> Result<(), Early> {
        match id {
            "h" => {
                self.human_readable = true;
                self.scale = Some(Scale::Human { base: 1024 });
            }
            "si" => self.scale = Some(Scale::Human { base: 1000 }),
            "block-size" => {
                self.scale =
                    Some(parse_block_size(&value.unwrap_or_default()).map_err(Early::Said)?);
            }
            "k" => self.kibibytes = true,
            "s" => self.show_blocks = true,
            "c" => self.time_kind = TimeKind::Change,
            "u" => self.time_kind = TimeKind::Access,
            "time" => {
                let word = value.unwrap_or_default();
                self.time_kind = TimeKind::named(&word).ok_or_else(|| {
                    Early::Argmatch(format!(
                        "invalid argument '{word}' for '--time'\n\
                         Valid arguments are:\n  - 'atime', 'access', 'use'\n  \
                         - 'ctime', 'status'\n  - 'mtime', 'modification'\n  \
                         - 'birth', 'creation'"
                    ))
                })?;
            }
            _ => {}
        }
        Ok(())
    }

    /// `-N`, `-b`, `-Q`, `--quoting-style`, `-q` and `--show-control-chars`: how names
    /// are quoted.
    fn quoting_option(&mut self, id: &str, value: Option<String>) -> Result<(), Early> {
        match id {
            "N" => self.quoting = Some(QuotingStyle::Literal),
            "b" => self.quoting = Some(QuotingStyle::Escape),
            "Q" => self.quoting = Some(QuotingStyle::C),
            "quoting-style" => {
                let word = value.unwrap_or_default();
                self.quoting = Some(QuotingStyle::named(&word).ok_or_else(|| {
                    Early::Argmatch(format!(
                        "invalid argument '{word}' for '--quoting-style'\n\
                         Valid arguments are:\n  - 'literal'\n  - 'shell'\n  \
                         - 'shell-always'\n  - 'shell-escape'\n  - 'shell-escape-always'\n  \
                         - 'c'\n  - 'c-maybe'\n  - 'escape'\n  - 'locale'\n  - 'clocale'"
                    ))
                })?);
            }
            "q" => self.hide_control = Some(true),
            "show-control-chars" => self.hide_control = Some(false),
            _ => {}
        }
        Ok(())
    }

    /// `-p`, `--file-type`, `--indicator-style` and `-F`: the marks after names.
    fn indicator_option(&mut self, id: &str, value: Option<String>) -> Result<(), Early> {
        match id {
            "p" => self.indicator = Indicator::Slash,
            "file-type" => self.indicator = Indicator::FileType,
            "indicator-style" => {
                let word = value.unwrap_or_default();
                self.indicator = match word.as_str() {
                    "none" => Indicator::None,
                    "slash" => Indicator::Slash,
                    "file-type" => Indicator::FileType,
                    "classify" => Indicator::Classify,
                    _ => {
                        return Err(Early::Argmatch(format!(
                            "invalid argument '{word}' for '--indicator-style'\n\
                                 Valid arguments are:\n  - 'none'\n  - 'slash'\n  \
                                 - 'file-type'\n  - 'classify'"
                        )));
                    }
                };
                self.classify_auto = false;
            }
            "F" => match value.as_deref() {
                None | Some("always" | "yes" | "force") => {
                    self.indicator = Indicator::Classify;
                    self.classify_auto = false;
                }
                Some("never" | "no" | "none") => self.indicator = Indicator::None,
                Some("auto" | "tty" | "if-tty") => {
                    self.indicator = Indicator::Classify;
                    self.classify_auto = true;
                }
                Some(other) => return Err(Early::Argmatch(when_refused(other, "classify"))),
            },
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum EntryKind {
    Dir,
    File,
    Symlink,
    /// A character device: `/dev/null`, `/dev/tty`, `/dev/zero` and the like (D7).
    CharDevice,
}

struct ItemInfo {
    name: String,
    path: PathBuf,
    kind: EntryKind,
    permissions: String,
    links: u32,
    owner: String,
    group: String,
    size: u64,
    /// The time `-l` shows and `-t` sorts by: its modification time unless `-c`, `-u` or
    /// `--time` chose another.
    time: SystemTime,
    /// The bytes the volume gives it, when `-s`, a total or `-c` asked for its facts.
    allocation: Option<u64>,
    /// Windows' attribute bits: read-only, hidden, system, archive.
    attributes: u32,
    symlink_target: Option<String>,
    /// Whether a symlink's target exists, and whether it is a directory.
    target: Option<(bool, bool)>,
    /// Whether the file runs: decided once, since it may read the file's first bytes.
    executable: OnceCell<bool>,
    /// A device's numbers, major and minor, which `-l` shows in place of the size.
    device: Option<(u32, u32)>,
    /// What `-lF` marks a link's target with, when its kind is known beyond a folder:
    /// `|` for a descriptor's name whose descriptor is a pipe.
    target_mark: Option<char>,
}

impl ItemInfo {
    fn is_executable(&self) -> bool {
        *self.executable.get_or_init(|| {
            self.kind == EntryKind::File && cash_win32::unix::is_executable(&self.path)
        })
    }
}

/// Which of a file's times `-l` shows and `-t` sorts by.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum TimeKind {
    #[default]
    Modification,
    /// `-u`: when it was last read.
    Access,
    /// `-c`: when its contents, name or attributes last changed (NTFS's `ChangeTime`,
    /// Cygwin's `st_ctime`).
    Change,
    /// When it was made.
    Birth,
}

impl TimeKind {
    /// The kind a word of `--time` names, as GNU ls reads them.
    fn named(word: &str) -> Option<Self> {
        Some(match word {
            "atime" | "access" | "use" => Self::Access,
            "ctime" | "status" => Self::Change,
            "mtime" | "modification" => Self::Modification,
            "birth" | "creation" => Self::Birth,
            _ => return None,
        })
    }

    /// This kind of time from what the directory listing brought; the change time, which
    /// it does not bring, is the modification time until the file's facts are read.
    fn of(self, metadata: &Metadata) -> SystemTime {
        match self {
            Self::Access => metadata.accessed(),
            Self::Birth => metadata.created(),
            Self::Modification | Self::Change => metadata.modified(),
        }
        .unwrap_or(SystemTime::UNIX_EPOCH)
    }
}

/// How names are quoted, GNU's `--quoting-style` (quotearg's styles), for names that are
/// Unicode, as in a UTF-8 locale: a character is nongraphic when it is a control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QuotingStyle {
    /// As they are.
    Literal,
    /// In single quotes where a shell needs them, double where only a `'` needs them.
    Shell,
    /// Always quoted so.
    ShellAlways,
    /// As `Shell`, with each nongraphic character as `$'\ooo'`.
    ShellEscape,
    /// Always quoted so.
    ShellEscapeAlways,
    /// In double quotes, with C's escapes.
    C,
    /// As `C` where something needs escaping, else as they are.
    CMaybe,
    /// C's escapes, and `\ ` for a space, without quotes.
    Escape,
    /// Between ‘ and ’, with C's escapes.
    Locale,
}

impl QuotingStyle {
    /// The style a word of `--quoting-style` or `QUOTING_STYLE` names.
    fn named(word: &str) -> Option<Self> {
        Some(match word {
            "literal" => Self::Literal,
            "shell" => Self::Shell,
            "shell-always" => Self::ShellAlways,
            "shell-escape" => Self::ShellEscape,
            "shell-escape-always" => Self::ShellEscapeAlways,
            "c" => Self::C,
            "c-maybe" => Self::CMaybe,
            "escape" => Self::Escape,
            "locale" | "clocale" => Self::Locale,
            _ => return None,
        })
    }

    /// Whether names are quoted only where they need it, so that GNU ls puts a space before
    /// the others where some are (`-l`, `-C`, `-x`).
    const fn aligns(self) -> bool {
        matches!(self, Self::Shell | Self::ShellEscape)
    }

    /// `name` as this style writes it, and whether it came out in quotes.
    fn quote(self, name: &str) -> (String, bool) {
        match self {
            Self::Literal => (name.to_owned(), false),
            Self::Shell => shell_quote(name, false, false),
            Self::ShellAlways => shell_quote(name, true, false),
            Self::ShellEscape => shell_quote(name, false, true),
            Self::ShellEscapeAlways => shell_quote(name, true, true),
            Self::C => (std::format!("\"{}\"", c_escaped(name, true, false)), true),
            Self::CMaybe => {
                if name.chars().any(|c| c.is_control() || c == '"') {
                    (std::format!("\"{}\"", c_escaped(name, true, false)), true)
                } else {
                    (name.to_owned(), false)
                }
            }
            Self::Escape => (c_escaped(name, false, true), false),
            Self::Locale => (
                std::format!("\u{2018}{}\u{2019}", c_escaped(name, false, false)),
                true,
            ),
        }
    }
}

/// C's escape of a control character: its letter, else its UTF-8 bytes in octal.
fn push_c_escape(out: &mut String, c: char) {
    out.push('\\');
    match c {
        '\u{7}' => out.push('a'),
        '\u{8}' => out.push('b'),
        '\t' => out.push('t'),
        '\n' => out.push('n'),
        '\u{b}' => out.push('v'),
        '\u{c}' => out.push('f'),
        '\r' => out.push('r'),
        _ => {
            let mut bytes = [0; 4];
            for (index, byte) in c.encode_utf8(&mut bytes).bytes().enumerate() {
                if index > 0 {
                    out.push('\\');
                }
                for digit in [byte >> 6, (byte >> 3) & 7, byte & 7] {
                    out.push(char::from(b'0' + digit));
                }
            }
        }
    }
}

/// `name` with C's escapes for control characters and `\`; `"` escaped too for a C
/// string, and a space as `\ ` for the escape style.
fn c_escaped(name: &str, quote: bool, space: bool) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    for c in name.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' if quote => out.push_str("\\\""),
            ' ' if space => out.push_str("\\ "),
            c if c.is_control() => push_c_escape(&mut out, c),
            c => out.push(c),
        }
    }
    out
}

/// `name` for a shell, as GNU's quotearg quotes it in its shell styles: unquoted when the
/// shell takes it as it is, unless `always`; in double quotes when only a `'` needs them
/// and nothing in it is special between double quotes; else in single quotes, a `'` as
/// `'\''`. With `escape`, each control character is `$'\ooo'` between the quoted parts;
/// without, tab, CR and newline go into the quotes and the others stay as they are.
fn shell_quote(name: &str, always: bool, escape: bool) -> (String, bool) {
    let mut needs = always;
    // Whether every character can stand between double quotes as it is.
    let mut compatible = true;
    let mut single = false;
    let mut escaped = false;
    for (index, c) in name.chars().enumerate() {
        match c {
            '\'' => {
                single = true;
                needs = true;
            }
            ' ' => needs = true,
            '#' | '~' if index == 0 => needs = true,
            '#' | '~' => compatible = false,
            '!' | '"' | '$' | '&' | '(' | ')' | '*' | ';' | '<' | '=' | '>' | '?' | '[' | '\\'
            | '^' | '`' | '|' => {
                needs = true;
                compatible = false;
            }
            '\t' | '\n' | '\r' => {
                needs = true;
                compatible = false;
                escaped |= escape;
            }
            c if c.is_control() => {
                compatible = false;
                if escape {
                    needs = true;
                    escaped = true;
                }
            }
            _ => {}
        }
    }
    if !needs {
        return (name.to_owned(), false);
    }
    if escaped {
        let mut out = String::from("'");
        // Whether a `$'` is open: controls in a row share it, and the quote that ends the
        // name ends it.
        let mut dollar = false;
        for c in name.chars() {
            if c.is_control() {
                if !dollar {
                    out.push_str("'$'");
                    dollar = true;
                }
                push_c_escape(&mut out, c);
            } else if c == '\'' {
                out.push_str("'\\''");
                dollar = false;
            } else {
                if dollar {
                    out.push_str("''");
                    dollar = false;
                }
                out.push(c);
            }
        }
        out.push('\'');
        return (out, true);
    }
    if single && compatible {
        return (std::format!("\"{name}\""), true);
    }
    (std::format!("'{}'", name.replace('\'', "'\\''")), true)
}

/// What an entry is read for: the long format's columns, the time to keep, and the facts
/// (`cash_win32::fs::FileFacts`) that `-s`, the totals and `-c` need.
#[derive(Clone, Copy)]
struct Want {
    long: bool,
    time: TimeKind,
    facts: bool,
}

/// How sizes and blocks are shown, GNU's `human_options`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Scale {
    /// In units of `unit` bytes, rounded up, with `suffix` after each number when the size
    /// was named by its unit alone (`K`, `kB`, `MiB`).
    Units { unit: u64, suffix: String },
    /// In as few digits as say it, one decimal under 10, rounded up, in powers of `base`
    /// (`-h`, `--si`).
    Human { base: u64 },
}

impl Scale {
    const fn units(unit: u64) -> Self {
        Self::Units {
            unit,
            suffix: String::new(),
        }
    }

    fn format(&self, bytes: u64) -> String {
        match self {
            Self::Units { unit, suffix } => std::format!("{}{suffix}", bytes.div_ceil(*unit)),
            Self::Human { base } => human_size(bytes, *base),
        }
    }
}

/// The scales of a listing: of the long format's sizes, and of `-s`'s blocks and totals.
struct Sizes {
    file: Scale,
    blocks: Scale,
}

/// How to order entries.
#[derive(Clone, Copy, Eq, PartialEq)]
enum SortKey {
    Name,
    Time,
    Size,
    Extension,
    Version,
    None,
}

/// How names are dressed: their colours and their icons.
struct Look {
    colors: Option<lscolors::LsColors>,
    /// Whether each name gets its Nerd Font glyph (lsd's theme).
    icons: bool,
    /// This process's account, whose files `-l` shows in the owner's colour.
    me: String,
    /// The zone `-l` shows times in: an exported `TZ`'s, as GNU ls, else Windows'.
    zone: Zone,
    /// How `-l` shows times.
    dates: DateFormats,
    /// The width a line may take, none for no limit.
    width: Option<usize>,
    /// How sizes and blocks are shown.
    sizes: Sizes,
    /// How names are quoted.
    quoting: QuotingStyle,
    /// Whether a nongraphic character left as it is shows as `?` (`-q`).
    hide_control: bool,
}

/// How `-l` shows a time (`--time-style`): a strftime format for a file changed within
/// half a year, and one for the rest.
struct DateFormats {
    recent: String,
    old: String,
}

impl Default for DateFormats {
    fn default() -> Self {
        Self {
            recent: "%b %e %H:%M".to_owned(),
            old: "%b %e  %Y".to_owned(),
        }
    }
}

/// The formats a `--time-style` names, as GNU ls reads them. `posix-` before a style keeps
/// the default in the POSIX locale, which is cash's; `+FORMAT` is strftime's, and two
/// formats on two lines are the old files' and the recent ones'. `%N`, GNU's nanoseconds,
/// is chrono's `%f`.
fn date_formats(style: Option<&str>) -> Result<DateFormats, Invalid> {
    let both = |format: &str| DateFormats {
        recent: format.to_owned(),
        old: format.to_owned(),
    };
    let Some(style) = style else {
        return Ok(DateFormats::default());
    };
    if let Some(format) = style.strip_prefix('+') {
        let format = format.replace("%N", "%f");
        return Ok(match format.split_once('\n') {
            Some((old, recent)) => DateFormats {
                recent: recent.to_owned(),
                old: old.to_owned(),
            },
            None => both(&format),
        });
    }
    if style.starts_with("posix-") {
        return Ok(DateFormats::default());
    }
    match style {
        "full-iso" => Ok(both("%Y-%m-%d %H:%M:%S.%f %z")),
        "long-iso" => Ok(both("%Y-%m-%d %H:%M")),
        "iso" => Ok(DateFormats {
            recent: "%m-%d %H:%M".to_owned(),
            old: "%Y-%m-%d ".to_owned(),
        }),
        "locale" => Ok(DateFormats::default()),
        _ => Err(Invalid::status(
            2,
            format!(
                "invalid argument '{style}' for 'time style'\n\
             Valid arguments are:\n  - [posix-]full-iso\n  - [posix-]long-iso\n  \
             - [posix-]iso\n  - [posix-]locale\n  - +FORMAT (e.g., +%H:%M) for a 'date'-style format"
            ),
        )),
    }
}

/// A command line `ls` refuses, with its message and the status it ends with.
struct Invalid {
    message: String,
    status: u8,
}

impl Invalid {
    /// A value GNU's `argmatch` refuses: its list of the values, and the status 1.
    const fn argmatch(message: String) -> Self {
        Self { message, status: 1 }
    }

    const fn status(status: u8, message: String) -> Self {
        Self { message, status }
    }
}

/// GNU's words for a `WHEN` it does not know (`--color`, `--classify`, `--hyperlink`),
/// each value with the ones that mean the same on a line.
fn when_refused(value: &str, option: &str) -> String {
    std::format!(
        "invalid argument '{value}' for '--{option}'\n\
         Valid arguments are:\n  - 'always', 'yes', 'force'\n  - 'never', 'no', 'none'\n  \
         - 'auto', 'tty', 'if-tty'"
    )
}

fn when(value: Option<&str>, option: &str, is_tty: bool) -> Result<bool, Invalid> {
    match value {
        None => Ok(false),
        Some("always" | "yes" | "force") => Ok(true),
        Some("never" | "no" | "none") => Ok(false),
        Some("auto" | "tty" | "if-tty") => Ok(is_tty),
        Some(other) => Err(Invalid::argmatch(when_refused(other, option))),
    }
}

impl builtins::Command for LsCommand {
    type Error = cash_core::Error;

    /// `help ls` shows what `ls --help` shows.
    fn get_content(
        name: &str,
        content_type: ContentType,
        _options: &ContentOptions,
    ) -> Result<String, cash_core::Error> {
        Ok(match content_type {
            ContentType::DetailedHelp => HELP.to_owned(),
            ContentType::ShortUsage => format!("{name}: {name} [OPTION]... [FILE]...\n"),
            ContentType::ShortDescription => format!(
                "{name} - List information about the FILEs (the current directory by default).\n"
            ),
            ContentType::ManPage => {
                return cash_core::error::unimp("man page rendering is not yet implemented");
            }
        })
    }

    /// The words as they are, read by GNU's option loop in `execute`.
    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        let mut command = Self::empty();
        command.raw = args.into_iter().skip(1).collect();
        Ok(command)
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        // `POSIXLY_CORRECT`, a variable of the shell's, says where the options end. The
        // command's own standard output, which a pipeline or a redirection replaces even
        // though the shell's is still the terminal, says whether `--classify=auto` marks.
        let mut command = Self::parse(&self.raw, options::gnu_order(&context));
        command.tty = context.try_fd(1).is_some_and(|f| f.is_terminal());
        command.list(context).await
    }
}

impl LsCommand {
    /// What a command line that ends `ls` before it lists says: the help, the version, or
    /// the mistake; `None` for one that lists.
    fn ended<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
    ) -> Result<Option<ExecutionResult>, cash_core::Error> {
        let name = &context.command_name;
        match &self.early {
            Some(Early::Version) => {
                writeln!(context.stdout(), "{name} (cash): GNU coreutils' options")?;
                Ok(Some(ExecutionResult::success()))
            }
            Some(Early::Usage(message)) => {
                options::gnu_usage_error(context, name, message, 2).map(Some)
            }
            Some(Early::Argmatch(message)) => {
                options::gnu_usage_error(context, name, message, 1).map(Some)
            }
            Some(Early::Said(message)) => {
                writeln!(context.stderr(), "{name}: {message}")?;
                Ok(Some(ExecutionResult::new(2)))
            }
            None if self.help => {
                write!(context.stdout(), "{HELP}")?;
                Ok(Some(ExecutionResult::success()))
            }
            None => Ok(None),
        }
    }

    /// Lists what the command line asked for.
    async fn list<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, cash_core::Error> {
        if let Some(result) = self.ended(&context)? {
            return Ok(result);
        }

        // The command's own standard output, which a pipeline or a redirection replaces
        // even though the shell's is still the terminal.
        let is_tty = self.tty;
        let look = match self.look(&context, is_tty) {
            Ok(look) => look,
            Err(Invalid { message, status }) => {
                writeln!(context.stderr(), "{}: {message}", context.command_name)?;
                writeln!(
                    context.stderr(),
                    "Try '{} --help' for more information.",
                    context.command_name
                )?;
                return Ok(ExecutionResult::new(status));
            }
        };

        let mut paths = self.paths.clone();
        if paths.is_empty() {
            paths.push(String::from("."));
        }

        let mut had_error = false;
        let mut had_minor_error = false;
        let multiple_paths = paths.len() > 1;

        // Separate inputs into existing files, existing dirs, and non-existing paths.
        let mut file_items = Vec::new();
        let mut dir_paths = Vec::new();

        for path_str in &paths {
            // Builtins do not change the host process cwd when `cd` changes the shell's
            // cwd. Resolve every operand through the shell state; otherwise the default
            // `.` (and every relative name) silently lists the directory cash was
            // launched from.
            let path = context.shell.absolute_path(Path::new(path_str));
            // cash (D7): a `/dev` name is no file Windows has; it is listed as Git Bash
            // lists it, or, for a descriptor that is not open, not at all.
            if let Some(name) = cash_win32::devices::dev_name(&path) {
                if let Some(item) = dev_entry(&context, name, &path, path_str) {
                    file_items.push(item);
                } else {
                    writeln!(
                        context.stderr(),
                        "{}: cannot access '{}': No such file or directory",
                        context.command_name,
                        path_str
                    )?;
                    had_error = true;
                }
                continue;
            }
            if !path.exists() && !path.is_symlink() {
                writeln!(
                    context.stderr(),
                    "{}: cannot access '{}': No such file or directory",
                    context.command_name,
                    path_str
                )?;
                had_error = true;
                continue;
            }

            if self.shown_as_entry(&path) {
                let followed = self.followed(&path, None);
                if let Some(item) = inspect_path(&path, path_str, self.want(), followed) {
                    file_items.push(item);
                }
            } else {
                dir_paths.push((path, path_str.clone()));
            }
        }

        let mut out = context.stdout();

        // Print standalone files first.
        if !file_items.is_empty() {
            self.sort_items(&mut file_items);
            self.render_items(&mut out, &file_items, &look, is_tty)?;
        }

        // Print directories, each after one blank line when something came before it.
        for (i, (dir_path, dir_str)) in dir_paths.iter().enumerate() {
            // A Ctrl-C, or `kill` of a background job, stops a long listing (`-R`).
            if context.stop_token().requested() {
                break;
            }
            if self.tree {
                if i > 0 || !file_items.is_empty() {
                    writeln!(out)?;
                }
                match self.tree_operand(&context, &mut out, dir_path, dir_str, &look)? {
                    Some(unreadable) => had_minor_error |= unreadable,
                    None => had_error = true,
                }
                continue;
            }

            if multiple_paths || self.recursive {
                if i > 0 || !file_items.is_empty() {
                    writeln!(out)?;
                }
                writeln!(out, "{}:", shown_name(dir_str, &look))?;
            }

            match self.list_operand(&context, &mut out, dir_path, dir_str, &look, is_tty)? {
                Some(unreadable) => had_minor_error |= unreadable,
                None => had_error = true,
            }
        }

        drop(out);
        if context.stop_token().requested() {
            return context.stopped().await;
        }
        Ok(gnu_status(
            had_error || self.looped.load(std::sync::atomic::Ordering::Relaxed),
            had_minor_error,
        ))
    }
}

impl LsCommand {
    /// Lists a directory named on the command line: `Some` with whether one below it
    /// could not be read, `None` when it could not be read itself, which is reported.
    ///
    /// A write that fails goes up, so a reader that went away ends `ls` with 141 (D71).
    /// Both were "cannot open directory", status 2: `ls -1 C:/Windows/System32 | head -1`
    /// (BI-14).
    fn list_operand<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        out: &mut impl Write,
        dir_path: &Path,
        dir_str: &str,
        look: &Look,
        is_tty: bool,
    ) -> Result<Option<bool>, std::io::Error> {
        match self.read_entries(dir_path, true) {
            Ok(items) => self
                .show_directory(context, out, &items, dir_str, look, is_tty, 1)
                .map(Some),
            Err(e) => {
                writeln!(
                    context.stderr(),
                    "{}: cannot open directory '{dir_str}': {}",
                    context.command_name,
                    cash_core::error::os_error_text(&e)
                )?;
                Ok(None)
            }
        }
    }
}

impl LsCommand {
    /// `--tree` of a directory named on the command line, answered as
    /// [`Self::list_operand`] answers. Each folder it could not open is said in GNU ls's
    /// words, as `-R` says it: they were passed over in silence.
    fn tree_operand<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        out: &mut impl Write,
        dir_path: &Path,
        dir_str: &str,
        look: &Look,
    ) -> Result<Option<bool>, std::io::Error> {
        let mut answer = Some(false);
        for (shown, error, operand) in self.render_tree(out, dir_path, dir_str, look)? {
            writeln!(
                context.stderr(),
                "{}: cannot open directory '{shown}': {}",
                context.command_name,
                cash_core::error::os_error_text(&error)
            )?;
            answer = if operand { None } else { answer.map(|_| true) };
        }
        Ok(answer)
    }
}

/// GNU `ls`'s status: 2 for an operand it could not read, 1 for a directory below one
/// (a "minor problem"), else 0.
fn gnu_status(had_error: bool, had_minor_error: bool) -> ExecutionResult {
    if had_error {
        ExecutionResult::new(2)
    } else if had_minor_error {
        ExecutionResult::new(1)
    } else {
        ExecutionResult::success()
    }
}

impl LsCommand {
    /// The colours and icons this command line asks for.
    fn look<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        is_tty: bool,
    ) -> Result<Look, Invalid> {
        // `--color` alone is `always`, as in GNU ls; no `--color` is `auto`.
        let color = when(
            Some(self.color.as_deref().unwrap_or("auto")),
            "color",
            is_tty,
        )?;
        let asked = when(self.icons.as_deref(), "icons", is_tty)?;
        let icons = match self.icons_theme.as_deref() {
            None | Some("auto") => asked && terminal_font_is_nerd(context),
            Some("fancy") => asked,
            Some(other) => {
                return Err(Invalid::status(
                    2,
                    std::format!(
                        "invalid argument '{other}' for '--icons-theme'\n\
                     Valid arguments are: 'auto', 'fancy'"
                    ),
                ));
            }
        };
        self.sort_key()?;

        // The time style is read for the long format only, as in GNU ls: elsewhere a wrong
        // one is not said. `TIME_STYLE`, a variable of the shell's, gives it without the
        // option.
        let dates = if self.long {
            let style = self
                .time_style
                .clone()
                .or_else(|| context.shell.env_str("TIME_STYLE").map(Into::into));
            date_formats(style.as_deref())?
        } else {
            DateFormats::default()
        };

        let colors = color.then(|| {
            // The shell's own variable, which `export` need not have reached the process.
            parsed_ls_colors(context.shell.env_str("LS_COLORS").as_deref())
        });
        Ok(Look {
            me: if colors.is_some() && self.long {
                cash_win32::fs::current_user()
            } else {
                String::new()
            },
            colors,
            icons,
            zone: Zone::of_shell(context.shell),
            dates,
            width: self.line_width(context, is_tty),
            sizes: self.sizes(context),
            quoting: self.quoting_style(context, is_tty),
            // GNU ls hides them on a terminal unless told not to.
            hide_control: self.hide_control.unwrap_or(is_tty),
        })
    }

    /// How names are quoted: the option, else `QUOTING_STYLE` (a word it does not know is
    /// said and passed over, as GNU ls says it), else shell-escape on a terminal, as GNU
    /// ls quotes there, and as they are elsewhere (the user, 2026-10-17).
    fn quoting_style<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        is_tty: bool,
    ) -> QuotingStyle {
        if let Some(style) = self.quoting {
            return style;
        }
        if let Some(value) = context.shell.env_str("QUOTING_STYLE") {
            match QuotingStyle::named(&value) {
                Some(style) => return style,
                None => {
                    let _ = writeln!(
                        context.stderr(),
                        "{}: ignoring invalid value of environment variable QUOTING_STYLE: '{value}'",
                        context.command_name
                    );
                }
            }
        }
        if is_tty {
            QuotingStyle::ShellEscape
        } else {
            QuotingStyle::Literal
        }
    }

    /// The scales of sizes and blocks, as GNU ls finds them: `-h`, `--si` or
    /// `--block-size` for both; else `LS_BLOCK_SIZE`, `BLOCK_SIZE` or `BLOCKSIZE` for the
    /// blocks (512 under `POSIXLY_CORRECT`, else 1024), the first two for the sizes too,
    /// and `-k` for 1024-byte blocks.
    fn sizes<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
    ) -> Sizes {
        if let Some(scale) = &self.scale {
            return Sizes {
                file: scale.clone(),
                blocks: scale.clone(),
            };
        }
        let env = |name: &str| context.shell.env_str(name).map(|value| value.to_string());
        let ls_block_size = env("LS_BLOCK_SIZE");
        let block_size = env("BLOCK_SIZE");
        let spec = ls_block_size
            .clone()
            .or_else(|| block_size.clone())
            .or_else(|| env("BLOCKSIZE"));
        // A value GNU's human_options refuses leaves the default, as it does.
        let mut blocks = spec
            .as_deref()
            .and_then(|spec| parse_block_size(spec).ok())
            .unwrap_or_else(|| {
                Scale::units(if env("POSIXLY_CORRECT").is_some() {
                    512
                } else {
                    1024
                })
            });
        let file = if ls_block_size.is_some() || block_size.is_some() {
            blocks.clone()
        } else {
            Scale::units(1)
        };
        if self.kibibytes {
            blocks = Scale::units(1024);
        }
        Sizes { file, blocks }
    }

    /// What entries are read for.
    fn want(&self) -> Want {
        Want {
            long: self.long,
            time: self.time_kind,
            facts: self.long || self.show_blocks || self.time_kind == TimeKind::Change,
        }
    }

    /// The width of `-s`'s blocks among `items`, for padding; `None` without `-s`.
    fn blocks_width(&self, items: &[ItemInfo], look: &Look) -> Option<usize> {
        self.show_blocks.then(|| {
            items
                .iter()
                .map(|item| look.sizes.blocks.format(block_bytes(item)).len())
                .max()
                .unwrap_or(1)
        })
    }

    /// The width a line may take, as GNU ls finds it: `-w`, else `COLUMNS`, else the
    /// terminal's when the output is one, else 80; none for no limit (`-w 0`). The
    /// console's width was taken into a pipe too.
    fn line_width<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        is_tty: bool,
    ) -> Option<usize> {
        let width = self
            .width
            .or_else(|| {
                context
                    .shell
                    .env_str("COLUMNS")
                    .and_then(|columns| columns.trim().parse().ok())
                    .filter(|&columns: &usize| columns > 0)
            })
            .unwrap_or_else(|| {
                if is_tty {
                    crossterm::terminal::size().map_or(80, |(columns, _)| usize::from(columns))
                } else {
                    80
                }
            });
        (width > 0).then_some(width)
    }

    /// Whether an operand is shown as an entry rather than listed as a folder: a file, `-d`,
    /// or a link named on the command line that `-l` or `-F` asks about itself, as GNU ls
    /// decides; `-L` goes through a link always.
    fn shown_as_entry(&self, path: &Path) -> bool {
        let link = path
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink());
        self.directory
            || !path.is_dir()
            || (link && !self.dereference && (self.long || self.indicator == Indicator::Classify))
    }

    /// `-L`: what `path`, a link, names, when it is there; otherwise what the listing knew.
    fn followed(&self, path: &Path, known: Option<Metadata>) -> Option<Metadata> {
        if self.dereference {
            std::fs::metadata(path).ok().or(known)
        } else {
            known
        }
    }

    fn sort_key(&self) -> Result<SortKey, Invalid> {
        if let Some(word) = &self.sort {
            return match word.as_str() {
                "name" => Ok(SortKey::Name),
                "none" => Ok(SortKey::None),
                "size" => Ok(SortKey::Size),
                "time" => Ok(SortKey::Time),
                "version" => Ok(SortKey::Version),
                "extension" => Ok(SortKey::Extension),
                other => Err(Invalid::argmatch(std::format!(
                    "invalid argument '{other}' for '--sort'\n\
                     Valid arguments are:\n  - 'none'\n  - 'name'\n  - 'time'\n  - 'size'\n  \
                     - 'extension'\n  - 'version'"
                ))),
            };
        }
        Ok(if self.unsorted {
            SortKey::None
        } else if self.sort_by_size {
            SortKey::Size
        } else if self.sort_by_time {
            SortKey::Time
        } else if self.sort_by_extension {
            SortKey::Extension
        } else if self.sort_by_version {
            SortKey::Version
        } else if !self.long && self.time_kind != TimeKind::Modification {
            // `-c`, `-u` or another `--time` without `-l` sorts by that time, as in GNU ls.
            SortKey::Time
        } else {
            SortKey::Name
        })
    }

    fn sort_items(&self, items: &mut [ItemInfo]) {
        let key = self.sort_key().unwrap_or(SortKey::Name);
        if key != SortKey::None {
            items.sort_by(|a, b| {
                let cmp = match key {
                    SortKey::Time => b.time.cmp(&a.time),
                    SortKey::Size => b.size.cmp(&a.size),
                    SortKey::Extension => extension_of(&a.name)
                        .to_lowercase()
                        .cmp(&extension_of(&b.name).to_lowercase())
                        .then_with(|| compare_names(&a.name, &b.name)),
                    SortKey::Version => compare_versions(&a.name, &b.name),
                    SortKey::Name | SortKey::None => compare_names(&a.name, &b.name),
                };

                if cmp == Ordering::Equal {
                    a.name.cmp(&b.name)
                } else {
                    cmp
                }
            });

            if self.reverse {
                items.reverse();
            }
        }

        // Directories first, each group keeping its order, as GNU ls does with -r too.
        if self.group_directories_first {
            items.sort_by_key(|item| item.kind != EntryKind::Dir);
        }
    }

    /// A directory's entries, with `.` and `..` under `-a` when `dots` is set, sorted.
    fn read_entries(&self, dir_path: &Path, dots: bool) -> Result<Vec<ItemInfo>, std::io::Error> {
        let mut items = Vec::new();

        // Include . and .. if -a is set, unless a pattern of -I leaves them out.
        if self.all && dots {
            if !self.ignored(".")
                && let Some(dot) = inspect_dot(dir_path, ".", self.want())
            {
                items.push(dot);
            }
            let parent = dir_path.parent().unwrap_or(dir_path);
            if !self.ignored("..")
                && let Some(dotdot) = inspect_dot(parent, "..", self.want())
            {
                items.push(dotdot);
            }
        }

        let read_dir = std::fs::read_dir(dir_path)?;
        for entry in read_dir.flatten() {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let is_dotfile = file_name.starts_with('.');

            if (is_dotfile && !self.all && !self.almost_all) || self.ignored(&file_name) {
                continue;
            }

            // The directory listing brought the entry's size, times and attributes.
            let known = entry.metadata().ok();
            if !self.all
                && !self.almost_all
                && known
                    .as_ref()
                    .is_some_and(|metadata| is_protected(attributes_of(metadata)))
            {
                continue;
            }

            let entry_path = entry.path();
            let known = self.followed(&entry_path, known);
            if let Some(item) = inspect_path(&entry_path, &file_name, self.want(), known) {
                items.push(item);
            }
        }

        self.sort_items(&mut items);
        Ok(items)
    }

    /// Lists a directory's `items`, read already, and under `-R` the directories below
    /// it. `Ok(true)` when one of those could not be read, which GNU `ls` reports and
    /// counts a minor problem, status 1: it said nothing, and printed the directory's
    /// header with nothing under it (BI-14). The header comes only once a directory is
    /// open, as in GNU `ls`.
    #[expect(
        clippy::too_many_arguments,
        reason = "the listing's options, and its depth"
    )]
    fn show_directory<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        out: &mut impl Write,
        items: &[ItemInfo],
        dir_str: &str,
        look: &Look,
        is_tty: bool,
        level: usize,
    ) -> Result<bool, std::io::Error> {
        // The blocks the directory's entries take, in the long format and with -s, as GNU
        // ls counts them: their allocations, where it counted their sizes.
        if self.long || self.show_blocks {
            let total: u64 = items.iter().map(block_bytes).sum();
            writeln!(out, "total {}", look.sizes.blocks.format(total))?;
        }

        self.render_items(out, items, look, is_tty)?;

        // Recursive descent if -R, as deep as --depth allows.
        let mut unreadable = false;
        if self.recursive && self.depth.is_none_or(|depth| level < depth) {
            for item in items {
                if context.stop_token().requested() {
                    break;
                }
                if item.kind == EntryKind::Dir && item.name != "." && item.name != ".." {
                    // Joined once: `ls -R foo/` printed `foo//bar:`.
                    let sub_str = crate::walk::join_shown(dir_str, &item.name);
                    // With -L a link to a folder above is not gone round, as GNU ls says.
                    if self.dereference
                        && crate::walk::loop_ancestor(&item.path, item.path.ancestors().skip(1))
                            .is_some()
                    {
                        writeln!(
                            context.stderr(),
                            "{}: {sub_str}: not listing already-listed directory",
                            context.command_name
                        )?;
                        self.looped
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                        continue;
                    }
                    match self.read_entries(&item.path, true) {
                        Ok(sub_items) => {
                            writeln!(out)?;
                            writeln!(out, "{}:", shown_name(&sub_str, look))?;
                            unreadable |= self.show_directory(
                                context,
                                out,
                                &sub_items,
                                &sub_str,
                                look,
                                is_tty,
                                level + 1,
                            )?;
                        }
                        Err(e) => {
                            writeln!(
                                context.stderr(),
                                "{}: cannot open directory '{sub_str}': {}",
                                context.command_name,
                                cash_core::error::os_error_text(&e)
                            )?;
                            unreadable = true;
                        }
                    }
                }
            }
        }

        Ok(unreadable)
    }

    /// `--tree`: the directory, then everything below it with the branches drawn, as
    /// deep as `--depth` allows. Links to directories are shown, not followed. What it
    /// gives back is each folder it could not open, by name, and whether that was the
    /// operand, for the caller to say.
    fn render_tree(
        &self,
        out: &mut impl Write,
        dir_path: &Path,
        dir_str: &str,
        look: &Look,
    ) -> Result<Vec<(String, std::io::Error, bool)>, std::io::Error> {
        let mut rows: Vec<(String, ItemInfo)> = Vec::new();
        if let Some(root) = inspect_path(dir_path, dir_str, self.want(), None) {
            rows.push((String::new(), root));
        }
        let mut unopened = Vec::new();
        self.tree_rows(dir_path, dir_str, "", 1, &mut rows, &mut unopened);

        if self.long {
            let (items, prefixes): (Vec<ItemInfo>, Vec<String>) = rows
                .into_iter()
                .map(|(prefix, item)| (item, prefix))
                .unzip();
            self.render_long(out, &items, look, Some(&prefixes))?;
        } else {
            let items: Vec<&ItemInfo> = rows.iter().map(|(_, item)| item).collect();
            let width = self.show_blocks.then(|| {
                items
                    .iter()
                    .map(|item| look.sizes.blocks.format(block_bytes(item)).len())
                    .max()
                    .unwrap_or(1)
            });
            for (prefix, item) in &rows {
                writeln!(
                    out,
                    "{}{prefix}{}",
                    fields(item, look, width),
                    self.format_name(item, look, false)
                )?;
            }
        }
        Ok(unopened)
    }

    /// The rows below the folder `dir`, shown as `shown`; a folder that cannot be opened
    /// goes into `unopened`.
    fn tree_rows(
        &self,
        dir: &Path,
        shown: &str,
        prefix: &str,
        level: usize,
        rows: &mut Vec<(String, ItemInfo)>,
        unopened: &mut Vec<(String, std::io::Error, bool)>,
    ) {
        let items = match self.read_entries(dir, false) {
            Ok(items) => items,
            Err(error) => {
                unopened.push((shown.to_owned(), error, level == 1));
                return;
            }
        };
        let count = items.len();
        for (index, item) in items.into_iter().enumerate() {
            let last = index + 1 == count;
            let branch = if last { "└── " } else { "├── " };
            let descend =
                item.kind == EntryKind::Dir && self.depth.is_none_or(|depth| level < depth);
            let path = item.path.clone();
            let name = crate::walk::join_shown(shown, &item.name);
            rows.push((std::format!("{prefix}{branch}"), item));
            if descend {
                let below = std::format!("{prefix}{}", if last { "    " } else { "│   " });
                self.tree_rows(&path, &name, &below, level + 1, rows, unopened);
            }
        }
    }

    fn render_items(
        &self,
        out: &mut impl Write,
        items: &[ItemInfo],
        look: &Look,
        is_tty: bool,
    ) -> Result<(), std::io::Error> {
        if items.is_empty() {
            return Ok(());
        }

        if self.long {
            return self.render_long(out, items, look, None);
        }
        match self.layout {
            Layout::OneColumn => self.render_single_column(out, items, look)?,
            Layout::Default if !is_tty => self.render_single_column(out, items, look)?,
            Layout::Default | Layout::Columns => self.render_grid(out, items, look, true)?,
            Layout::Across => self.render_grid(out, items, look, false)?,
            Layout::Commas => self.render_commas(out, items, look)?,
        }

        Ok(())
    }

    /// The long format, each name after its tree branch when there are `prefixes`.
    fn render_long(
        &self,
        out: &mut impl Write,
        items: &[ItemInfo],
        look: &Look,
        prefixes: Option<&[String]>,
    ) -> Result<(), std::io::Error> {
        // A folder shows 1, as in Git Bash; --count-links counts it, reading it.
        let links: Vec<u32> = items
            .iter()
            .map(|item| {
                if self.count_links && item.kind == EntryKind::Dir {
                    cash_win32::fs::folder_link_count(&item.path)
                } else {
                    item.links
                }
            })
            .collect();
        let max_links_len = links
            .iter()
            .map(|links| links.to_string().len())
            .max()
            .unwrap_or(1);
        let max_owner_len = items.iter().map(|i| i.owner.len()).max().unwrap_or(1);
        let max_group_len = items.iter().map(|i| i.group.len()).max().unwrap_or(1);
        let size_text = |item: &ItemInfo| {
            if let Some((major, minor)) = item.device {
                std::format!("{major}, {minor}")
            } else {
                look.sizes.file.format(item.size)
            }
        };
        let max_size_len = items.iter().map(|i| size_text(i).len()).max().unwrap_or(1);
        let blocks_width = self.blocks_width(items, look);
        let align = align_names(items, look);

        for (index, item) in items.iter().enumerate() {
            let size_str = size_text(item);
            let date_str = format_date(item.time, look);
            let display_name = self.format_name(item, look, align);
            let prefix = prefixes
                .and_then(|prefixes| prefixes.get(index))
                .map_or("", String::as_str);

            // Each column padded first, then coloured, so the escapes never count as width.
            let colour = look.colors.is_some();
            let mut permissions = if colour {
                paint_permissions(&item.permissions)
            } else {
                item.permissions.clone()
            };
            if self.attributes {
                permissions.push(' ');
                permissions.push_str(&attribute_letters(item));
            }
            let owner = std::format!("{:<max_owner_len$}", item.owner);
            let group = std::format!("{:<max_group_len$}", item.group);
            let size = std::format!("{size_str:>max_size_len$}");
            let (owner, group, size, date) = if colour {
                let me = look.me.as_str();
                (
                    paint(owner_colour(&item.owner, me), &owner),
                    paint(owner_colour(&item.group, me), &group),
                    paint(size_colour(item.size), &size),
                    paint(date_colour(item.time), &date_str),
                )
            } else {
                (owner, group, size, date_str)
            };
            // -g leaves the owner out, -o and -G the group.
            let mut line = std::format!(
                "{}{permissions} {:>max_links_len$} ",
                fields(item, look, blocks_width),
                links[index]
            );
            if !self.no_owner {
                line.push_str(&owner);
                line.push(' ');
            }
            if !self.no_group {
                line.push_str(&group);
                line.push(' ');
            }
            writeln!(out, "{line}{size} {date} {prefix}{display_name}")?;
        }

        Ok(())
    }

    fn render_single_column(
        &self,
        out: &mut impl Write,
        items: &[ItemInfo],
        look: &Look,
    ) -> Result<(), std::io::Error> {
        let width = self.blocks_width(items, look);
        for item in items {
            let display_name = self.format_name(item, look, false);
            writeln!(out, "{}{display_name}", fields(item, look, width))?;
        }
        Ok(())
    }

    /// Entries in columns as GNU ls lays them out: as many columns as fit the width, each
    /// as wide as its widest name and two spaces, down the columns (`-C`) or across the
    /// lines (`-x`). Spaces where GNU writes tabs. Every column took the widest name's
    /// width, so fewer fitted than GNU's.
    fn render_grid(
        &self,
        out: &mut impl Write,
        items: &[ItemInfo],
        look: &Look,
        down: bool,
    ) -> Result<(), std::io::Error> {
        // Columns are measured in the cells a name takes on screen, not its bytes: an
        // icon, an accented letter or a CJK name is one or two cells whatever its length.
        let blocks_width = self.blocks_width(items, look);
        let align = align_names(items, look);
        let names: Vec<(usize, String)> = items
            .iter()
            .map(|i| {
                let fields = fields(i, look, blocks_width);
                (
                    fields.len() + self.format_name_plain(i, look, align).width(),
                    fields + &self.format_name(i, look, align),
                )
            })
            .collect();
        let count = names.len();
        let lengths: Vec<usize> = names.iter().map(|(width, _)| *width).collect();
        let (columns, widths) = column_layout(&lengths, look.width, down);
        let rows = count.div_ceil(columns);
        let pad = |line: &mut String, column: usize, width: usize| {
            let room = widths.get(column).copied().unwrap_or(0);
            line.extend(std::iter::repeat_n(' ', room.saturating_sub(width)));
        };

        if down {
            for row in 0..rows {
                let mut line = String::new();
                let mut index = row;
                let mut column = 0;
                while let Some((width, shown)) = names.get(index) {
                    line.push_str(shown);
                    index += rows;
                    if index < count {
                        pad(&mut line, column, *width);
                    }
                    column += 1;
                }
                writeln!(out, "{line}")?;
            }
        } else {
            let mut line = String::new();
            for (index, (width, shown)) in names.iter().enumerate() {
                let column = index % columns;
                if column == 0 && index > 0 {
                    writeln!(out, "{line}")?;
                    line.clear();
                }
                line.push_str(shown);
                if column + 1 < columns && index + 1 < count {
                    pad(&mut line, column, *width);
                }
            }
            writeln!(out, "{line}")?;
        }
        Ok(())
    }

    /// Entries separated by commas, as many to a line as fit the width (`-m`), as GNU ls
    /// writes them.
    fn render_commas(
        &self,
        out: &mut impl Write,
        items: &[ItemInfo],
        look: &Look,
    ) -> Result<(), std::io::Error> {
        let mut text = String::new();
        let mut position = 0;
        // -m pads no blocks.
        let blocks_width = self.show_blocks.then_some(0);
        for (index, item) in items.iter().enumerate() {
            let fields = fields(item, look, blocks_width);
            let length = fields.len() + self.format_name_plain(item, look, false).width();
            if index > 0 {
                text.push(',');
                if look.width.is_none_or(|width| position + length + 2 < width) {
                    text.push(' ');
                    position += 2;
                } else {
                    text.push('\n');
                    position = 0;
                }
            }
            text.push_str(&fields);
            text.push_str(&self.format_name(item, look, false));
            position += length;
        }
        writeln!(out, "{text}")
    }

    /// Whether names get their `-F` marks: `--classify=auto` only on a terminal.
    fn classifies(&self) -> bool {
        self.indicator == Indicator::Classify && (!self.classify_auto || self.tty)
    }

    /// The marks names get: `--classify=auto` none off a terminal.
    fn indicator_style(&self) -> Indicator {
        match self.indicator {
            Indicator::Classify if !self.classifies() => Indicator::None,
            style => style,
        }
    }

    /// The mark after an entry's name: `/` for a folder with any style, `@` for a link
    /// with `--file-type` and `-F`, and `*` for a program with `-F` alone.
    fn indicator(&self, item: &ItemInfo) -> Option<char> {
        match (self.indicator_style(), item.kind) {
            (Indicator::None, _) | (_, EntryKind::CharDevice) => None,
            (_, EntryKind::Dir) => Some('/'),
            (Indicator::Slash, _) => None,
            (_, EntryKind::Symlink) => Some('@'),
            (Indicator::Classify, EntryKind::File) => item.is_executable().then_some('*'),
            (_, EntryKind::File) => None,
        }
    }

    /// Whether `-I`, `-B` or, without `-a` and `-A`, `--hide` leaves `name` out.
    fn ignored(&self, name: &str) -> bool {
        (self.ignore_backups && name.ends_with('~'))
            || self.ignore.iter().any(|pattern| pattern.matches(name))
            || (!self.all
                && !self.almost_all
                && self.hide.iter().any(|pattern| pattern.matches(name)))
    }

    /// The name as it shows, without colour: for measuring.
    /// The attribute letters and a space, before a name outside the long format.
    fn attribute_prefix(&self, item: &ItemInfo) -> String {
        if self.attributes && !self.long {
            std::format!("{} ", attribute_letters(item))
        } else {
            String::new()
        }
    }

    /// The name as `format_name` shows it, without colour: for measuring.
    fn format_name_plain(&self, item: &ItemInfo, look: &Look, align: bool) -> String {
        let mut s = self.attribute_prefix(item);
        if look.icons {
            s.push_str(icon_for(item));
            s.push(' ');
        }
        s.push_str(&quoted_name(&item.name, look, align));
        self.push_marks(&mut s, item, look);
        s
    }

    /// What follows a name: its `-F` mark, or in the long format a link's target and the
    /// target's mark, as GNU ls writes them. Outside the long format a link shows no
    /// target: `link@` with `-F`, `link` without.
    fn push_marks(&self, s: &mut String, item: &ItemInfo, look: &Look) {
        match &item.symlink_target {
            Some(target) if self.long => {
                s.push_str(" -> ");
                s.push_str(&shown_name(target, look));
                let style = self.indicator_style();
                if style != Indicator::None {
                    let mark = match item.target {
                        Some((_, true)) => Some('/'),
                        _ if style == Indicator::Slash => None,
                        _ => item.target_mark,
                    };
                    s.extend(mark);
                }
            }
            _ => s.extend(self.indicator(item)),
        }
    }

    /// The name as a listing shows it: quoted in the listing's style, coloured, with its
    /// marks; after a space where `align` lines it up with quoted names.
    fn format_name(&self, item: &ItemInfo, look: &Look, align: bool) -> String {
        let mut shown = String::new();
        if look.icons {
            shown.push_str(icon_for(item));
            shown.push(' ');
        }
        shown.push_str(&quoted_name(&item.name, look, align));

        let mut s = self.attribute_prefix(item);
        s.push_str(&match look
            .colors
            .as_ref()
            .and_then(|colors| style_for(colors, item))
        {
            Some(style) => style.paint(&shown).to_string(),
            None => shown,
        });

        self.push_marks(&mut s, item, look);
        s
    }
}

/// Whether names that need no quotes get a space before them, where some among `items`
/// came out quoted, as GNU ls lines them up (`-l`, `-C`, `-x`).
fn align_names(items: &[ItemInfo], look: &Look) -> bool {
    look.quoting.aligns() && items.iter().any(|item| look.quoting.quote(&item.name).1)
}

/// `name` quoted in the listing's style, with `-q`'s `?` for a nongraphic character the
/// style left as it is, as GNU ls applies it after quoting.
fn shown_name(name: &str, look: &Look) -> String {
    let (quoted, _) = look.quoting.quote(name);
    if look.hide_control {
        quoted
            .chars()
            .map(|c| if c.is_control() { '?' } else { c })
            .collect()
    } else {
        quoted
    }
}

/// [`shown_name`], after a space when `align` asks it of a name that needed no quotes.
fn quoted_name(name: &str, look: &Look, align: bool) -> String {
    let shown = shown_name(name, look);
    if align && !look.quoting.quote(name).1 {
        std::format!(" {shown}")
    } else {
        shown
    }
}

/// The colour an entry takes: its kind's, and for a plain file, its name's pattern in
/// `LS_COLORS` (`*.zip`, `*~`), as GNU ls chooses. An executable is known by its
/// extension or a `#!` line here, which `lscolors` cannot see on Windows.
fn style_for(colors: &lscolors::LsColors, item: &ItemInfo) -> Option<nu_ansi_term::Style> {
    use lscolors::Indicator;
    let indicator = match (item.kind, item.target) {
        (EntryKind::Dir, _) => Indicator::Directory,
        (EntryKind::Symlink, Some((false, _))) => Indicator::OrphanedSymbolicLink,
        (EntryKind::Symlink, _) => Indicator::SymbolicLink,
        (EntryKind::File, _) if item.is_executable() => Indicator::ExecutableFile,
        (EntryKind::File, _) => Indicator::RegularFile,
        (EntryKind::CharDevice, _) => Indicator::CharacterDevice,
    };
    let style = if indicator == Indicator::RegularFile {
        colors
            .style_for_str(&item.name)
            .or_else(|| colors.style_for_indicator(indicator))
    } else {
        colors.style_for_indicator(indicator)
    };
    style.map(lscolors::Style::to_nu_ansi_term_style)
}

/// Wraps `text` in an SGR colour.
fn paint(sgr: &str, text: &str) -> String {
    std::format!("\x1b[{sgr}m{text}\x1b[0m")
}

/// The permission string with lsd's colours letter by letter: the type blue, `r`
/// yellow, `w` red, `x` green, `-` grey.
fn paint_permissions(permissions: &str) -> String {
    let mut out = String::new();
    for (index, letter) in permissions.chars().enumerate() {
        let sgr = match letter {
            'd' | 'l' if index == 0 => "1;34",
            'r' => "33",
            'w' => "31",
            'x' | 's' | 't' => "32",
            _ => "90",
        };
        out.push_str(&paint(sgr, &letter.to_string()));
    }
    out
}

/// The owner or group column: pale yellow when it is this user, grey for any other
/// account (`Administrators`, `SYSTEM`, `TrustedInstaller`), so those stand out.
pub(crate) const fn owner_colour(account: &str, me: &str) -> &'static str {
    if account.eq_ignore_ascii_case(me) {
        "38;5;230"
    } else {
        "38;5;245"
    }
}

/// The size by magnitude, as lsd's theme: nothing grey, under a megabyte pale, under a
/// gigabyte orange, and a gigabyte or more a deeper, bold orange.
const fn size_colour(bytes: u64) -> &'static str {
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * MB;
    match bytes {
        0 => "38;5;245",
        b if b < MB => "38;5;229",
        b if b < GB => "38;5;216",
        _ => "1;38;5;172",
    }
}

/// The date by age, as lsd's theme: within the hour bright green, within the day green,
/// older teal.
fn date_colour(mtime: SystemTime) -> &'static str {
    let age = SystemTime::now()
        .duration_since(mtime)
        .unwrap_or_default()
        .as_secs();
    if age < 3600 {
        "38;5;40"
    } else if age < 86_400 {
        "38;5;42"
    } else {
        "38;5;36"
    }
}

/// `LS_COLORS` as `eval "$(dircolors)"` would set it: the colours for each kind of file,
/// and for archives, backups and temporary files by extension. uutils' copy of
/// `dircolors`' defaults.
/// `LS_COLORS` parsed, or dircolors' defaults without it, kept while the variable's text
/// stays the same: every coloured listing built and parsed the whole table again.
fn parsed_ls_colors(value: Option<&str>) -> lscolors::LsColors {
    static PARSED: std::sync::Mutex<Option<(Option<String>, lscolors::LsColors)>> =
        std::sync::Mutex::new(None);
    let mut kept = PARSED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((text, colors)) = kept.as_ref()
        && text.as_deref() == value
    {
        return colors.clone();
    }
    let colors = value.map_or_else(
        || lscolors::LsColors::from_string(&default_ls_colors()),
        lscolors::LsColors::from_string,
    );
    *kept = Some((value.map(str::to_owned), colors.clone()));
    colors
}

fn default_ls_colors() -> String {
    use std::fmt::Write as _;
    let mut spec = String::new();
    for (_, code, color) in uucore::colors::FILE_TYPES {
        let _ = write!(spec, "{code}={color}:");
    }
    for (extension, color) in uucore::colors::FILE_COLORS {
        let _ = write!(spec, "*{extension}={color}:");
    }
    spec
}

/// Whether Windows Terminal draws this tab in a Nerd Font, so that `--icons` shows its
/// glyphs. Anywhere else, or in any other font, `--icons` shows no icons at all: the
/// glyphs would come out as empty boxes, and the one other set, colour emoji, was too
/// loud (decided with the user, 2026-09-28). No terminal can be asked its font, but
/// Terminal names the tab's profile in `WT_PROFILE_ID`, and the profile's font is in
/// Terminal's settings. Read from the shell's variables, as `LS_COLORS` is.
fn terminal_font_is_nerd<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> bool {
    let var = |name: &str| {
        context
            .shell
            .env_str(name)
            .filter(|value| !value.is_empty())
    };
    let (Some(_), Some(profile), Some(local)) =
        (var("WT_SESSION"), var("WT_PROFILE_ID"), var("LOCALAPPDATA"))
    else {
        return false;
    };
    let program_data = var("ProgramData").or_else(|| var("PROGRAMDATA"));
    let font = cash_win32::terminal::profile_font(
        std::path::Path::new(local.as_ref()),
        program_data.as_deref().map(std::path::Path::new),
        &profile,
    );
    cash_win32::terminal::is_nerd_font(&font)
}

/// The Nerd Font glyph for an entry: lsd's order, a link as a link, then its name, then
/// its extension, then its kind.
fn icon_for(item: &ItemInfo) -> &'static str {
    let is_dir_link = matches!(item.target, Some((_, true)));
    if item.kind == EntryKind::Symlink {
        return if is_dir_link { "\u{f482}" } else { "\u{f481}" };
    }
    if item.kind == EntryKind::CharDevice {
        // lsd's `device-char`.
        return "\u{e601}";
    }
    let name = item.name.to_lowercase();
    if let Some(icon) = lookup(ls_icon_table::BY_NAME, &name) {
        return icon;
    }
    if item.kind == EntryKind::Dir {
        return "\u{f115}";
    }
    if let Some(icon) = name
        .rsplit_once('.')
        .and_then(|(_, extension)| lookup(ls_icon_table::BY_EXTENSION, extension))
    {
        return icon;
    }
    if item.is_executable() {
        "\u{f489}"
    } else {
        "\u{f016}"
    }
}

fn lookup(table: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table
        .binary_search_by(|(name, _)| (*name).cmp(key))
        .ok()
        .map(|index| table[index].1)
}

/// What `-X` sorts by: the text after the last `.`, empty for a name without one.
fn extension_of(name: &str) -> &str {
    name.rsplit_once('.').map_or("", |(_, extension)| extension)
}

/// Natural alphabetical order, case-insensitive on Windows.
fn compare_names(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

/// `-v`: names compared with each run of digits as a number, so `file9` comes before
/// `file10` and `1.9` before `1.10`.
fn compare_versions(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut a, mut b) = (a.as_str(), b.as_str());
    loop {
        match (a.chars().next(), b.chars().next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let a_end = a.find(|c: char| !c.is_ascii_digit()).unwrap_or(a.len());
                let b_end = b.find(|c: char| !c.is_ascii_digit()).unwrap_or(b.len());
                let (a_digits, a_rest) = a.split_at(a_end);
                let (b_digits, b_rest) = b.split_at(b_end);
                let a_trimmed = a_digits.trim_start_matches('0');
                let b_trimmed = b_digits.trim_start_matches('0');
                let order = a_trimmed
                    .len()
                    .cmp(&b_trimmed.len())
                    .then_with(|| a_trimmed.cmp(b_trimmed))
                    .then_with(|| a_digits.len().cmp(&b_digits.len()));
                if order != Ordering::Equal {
                    return order;
                }
                a = a_rest;
                b = b_rest;
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a = a.split_at(x.len_utf8()).1;
                b = b.split_at(y.len_utf8()).1;
            }
        }
    }
}

/// Windows' file attribute bits `ls` reads (`GetFileAttributes`).
const ATTRIBUTE_READONLY: u32 = 0x1;
const ATTRIBUTE_HIDDEN: u32 = 0x2;
const ATTRIBUTE_SYSTEM: u32 = 0x4;
const ATTRIBUTE_ARCHIVE: u32 = 0x20;

fn attributes_of(metadata: &Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes()
}

/// Both hidden and system: what Explorer never shows and lsd leaves out without
/// `--system-protected`, such as `NTUSER.DAT`'s logs and the `My Documents` junctions.
const fn is_protected(attributes: u32) -> bool {
    attributes & (ATTRIBUTE_HIDDEN | ATTRIBUTE_SYSTEM) == ATTRIBUTE_HIDDEN | ATTRIBUTE_SYSTEM
}

/// lsd's attribute letters: `d` or `.`, then archive, read-only, hidden and system.
fn attribute_letters(item: &ItemInfo) -> String {
    let flag = |bit: u32, letter: char| {
        if item.attributes & bit == 0 {
            '-'
        } else {
            letter
        }
    };
    [
        if item.kind == EntryKind::Dir {
            'd'
        } else {
            '.'
        },
        flag(ATTRIBUTE_ARCHIVE, 'a'),
        flag(ATTRIBUTE_READONLY, 'r'),
        flag(ATTRIBUTE_HIDDEN, 'h'),
        flag(ATTRIBUTE_SYSTEM, 's'),
    ]
    .iter()
    .collect()
}

fn inspect_dot(dir_path: &Path, name: &str, want: Want) -> Option<ItemInfo> {
    let metadata = std::fs::metadata(dir_path).ok()?;

    let mut item = ItemInfo {
        name: name.to_string(),
        path: dir_path.to_path_buf(),
        kind: EntryKind::Dir,
        permissions: String::new(),
        links: 0,
        owner: String::new(),
        group: String::new(),
        size: metadata.len(),
        time: want.time.of(&metadata),
        allocation: None,
        attributes: attributes_of(&metadata),
        symlink_target: None,
        target: None,
        executable: OnceCell::new(),
        device: None,
        target_mark: None,
    };
    if want.long {
        // A folder's link count is 1, as Git Bash shows it (`file_link_count`).
        item.links = 1;
        fill_long(&mut item, &metadata);
    }
    if want.facts {
        take_facts(&mut item, cash_win32::fs::file_facts(dir_path, true), want);
    }
    Some(item)
}

/// What [`cash_win32::fs::FileFacts`] add to an entry: its allocation, and the change time
/// when that is the time kept.
fn take_facts(item: &mut ItemInfo, facts: Option<cash_win32::fs::FileFacts>, want: Want) {
    let Some(facts) = facts else {
        return;
    };
    item.allocation = Some(facts.allocation);
    if want.time == TimeKind::Change && facts.change_time != 0 {
        item.time = filetime_to_system(facts.change_time);
    }
}

/// What comes before an entry's name: its blocks under `-s`, padded to `width`, as GNU
/// ls writes them (`-m` pads nothing).
fn fields(item: &ItemInfo, look: &Look, width: Option<usize>) -> String {
    width.map_or_else(String::new, |width| {
        std::format!("{:>width$} ", look.sizes.blocks.format(block_bytes(item)))
    })
}

/// A `FILETIME`, 100 ns since 1601, as a [`SystemTime`].
fn filetime_to_system(filetime: u64) -> SystemTime {
    // 100-ns intervals between 1601-01-01 and 1970-01-01.
    const UNIX_EPOCH_FILETIME: u64 = 116_444_736_000_000_000;
    SystemTime::UNIX_EPOCH
        + std::time::Duration::from_nanos(
            filetime
                .saturating_sub(UNIX_EPOCH_FILETIME)
                .saturating_mul(100),
        )
}

/// The bytes `-s` and the totals count for an entry: its allocation, else its size, in
/// whole KiB, as Cygwin gives `st_blocks` and so Git Bash's `ls -s` shows them.
fn block_bytes(item: &ItemInfo) -> u64 {
    item.allocation
        .unwrap_or(item.size)
        .div_ceil(1024)
        .saturating_mul(1024)
}

/// The entry for a `/dev` name (D7), as Git Bash 5.3 lists it: a device as a character
/// device, `crw-rw-rw-`, with Linux's numbers where the size goes, and a descriptor's
/// name as a link to `/proc/self/fd/N`, if the descriptor is open. Owned by the user, and
/// as new as now.
fn dev_entry<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    name: cash_win32::devices::DevName,
    path: &Path,
    display_name: &str,
) -> Option<ItemInfo> {
    let mut item = ItemInfo {
        name: display_name.to_string(),
        path: path.to_path_buf(),
        kind: EntryKind::CharDevice,
        permissions: String::from("crw-rw-rw-"),
        links: 1,
        owner: cash_win32::fs::current_user(),
        group: cash_win32::fs::current_user(),
        size: 0,
        time: SystemTime::now(),
        allocation: Some(0),
        attributes: 0,
        symlink_target: None,
        target: None,
        executable: OnceCell::new(),
        device: name.numbers(),
        target_mark: None,
    };
    if let cash_win32::devices::DevName::Descriptor(number) = name {
        let fd = cash_core::ShellFd::try_from(number).ok()?;
        let open = context.try_fd(fd)?;
        item.kind = EntryKind::Symlink;
        item.permissions = String::from("lrwxrwxrwx");
        item.symlink_target = Some(std::format!("/proc/self/fd/{number}"));
        item.target = Some((true, false));
        if matches!(open.kind(), cash_core::openfiles::FileKind::Pipe { .. }) {
            item.target_mark = Some('|');
        }
    }
    Some(item)
}

/// An entry, from `known` metadata when the directory listing supplied it (no file is
/// opened for that), else read. The owner, link count and permissions, which cost a
/// read of each file's security and more, are looked up only for `-l`, which shows them.
fn inspect_path(
    path: &Path,
    display_name: &str,
    want: Want,
    known: Option<Metadata>,
) -> Option<ItemInfo> {
    let symlink_metadata = match known {
        Some(metadata) => metadata,
        None => std::fs::symlink_metadata(path).ok()?,
    };
    let is_symlink = symlink_metadata.file_type().is_symlink();

    let kind = if is_symlink {
        EntryKind::Symlink
    } else if symlink_metadata.is_dir() {
        EntryKind::Dir
    } else {
        EntryKind::File
    };

    let (symlink_target, target) = if is_symlink {
        let followed = std::fs::metadata(path);
        (
            std::fs::read_link(path)
                .ok()
                .map(|p| p.to_string_lossy().into_owned()),
            Some((followed.is_ok(), followed.is_ok_and(|m| m.is_dir()))),
        )
    } else {
        (None, None)
    };

    let mut item = ItemInfo {
        name: display_name.to_string(),
        path: path.to_path_buf(),
        kind,
        permissions: String::new(),
        links: 0,
        owner: String::new(),
        group: String::new(),
        size: symlink_metadata.len(),
        time: want.time.of(&symlink_metadata),
        allocation: None,
        attributes: attributes_of(&symlink_metadata),
        symlink_target,
        target,
        executable: OnceCell::new(),
        device: None,
        target_mark: None,
    };
    let mut facts_taken = false;
    if want.long {
        // One open of the file for its security, links and first bytes (a link keeps
        // the separate asks, which read the link itself where they should).
        let facts = (kind != EntryKind::Symlink).then(|| {
            let read_start = kind == EntryKind::File
                && item.size >= 2
                && !cash_win32::unix::has_program_extension(path);
            cash_win32::fs::listing_facts(path, read_start)
        });
        if let Some(Some(facts)) = facts {
            // A folder's link count is not counted in a listing (`--count-links` does).
            item.links = if kind == EntryKind::Dir {
                1
            } else {
                facts.links
            };
            if kind == EntryKind::File {
                let executable = cash_win32::unix::has_program_extension(path)
                    || facts.starts_with_shebang == Some(true);
                let _ = item.executable.set(executable);
            }
            take_facts(&mut item, Some(facts.facts), want);
            facts_taken = true;
            fill_long_from(&mut item, &symlink_metadata, facts.security);
        } else {
            item.links = cash_win32::fs::file_link_count(path, &symlink_metadata);
            fill_long(&mut item, &symlink_metadata);
        }
    }
    if want.facts && !facts_taken {
        // A link's own facts, unless -L went through it.
        take_facts(
            &mut item,
            cash_win32::fs::file_facts(path, !is_symlink),
            want,
        );
    }
    Some(item)
}

/// The owner and the permission string `-l` shows, from one read of the file's security.
fn fill_long(item: &mut ItemInfo, metadata: &Metadata) {
    let security = cash_win32::fs::file_security(&item.path);
    fill_long_from(item, metadata, security);
}

/// The owner, group and permission string `-l` shows, from the file's security.
fn fill_long_from(
    item: &mut ItemInfo,
    metadata: &Metadata,
    security: cash_win32::fs::FileSecurity,
) {
    item.owner = security
        .owner
        .clone()
        .unwrap_or_else(cash_win32::fs::current_user);
    // The file's primary group, read for what it is; where it cannot be read, the owner.
    item.group = security
        .group
        .map_or_else(|| item.owner.clone(), |group| group.name);
    item.permissions = format_permissions(item, metadata, security.writable);
}

/// Unix permission bits for a Windows file, which has an access list instead, by the
/// rule `cash_win32::unix` keeps for `ls`, `stat`, `tar` and `zip` alike.
fn format_permissions(item: &ItemInfo, metadata: &Metadata, writable: Option<bool>) -> String {
    use cash_win32::unix::{Kind, mode_string, permissions};
    let kind = match item.kind {
        EntryKind::Symlink => Kind::Symlink,
        EntryKind::Dir => Kind::Dir,
        EntryKind::File | EntryKind::CharDevice => Kind::File,
    };
    let bits = permissions(
        kind,
        writable,
        metadata.permissions().readonly(),
        item.is_executable(),
    );
    mode_string(kind.type_bits() | bits)
}

/// `bytes` in as few digits as say it, as GNU's `human_readable` writes them for `-h`
/// (`base` 1024) and `--si` (1000): one decimal under 10, and rounded up, so a size never
/// reads smaller than it is. Rounded to the nearest, 2,000,000 bytes read `1.9M`.
fn human_size(bytes: u64, base: u64) -> String {
    const LETTERS: [char; 8] = ['K', 'M', 'G', 'T', 'P', 'E', 'Z', 'Y'];
    if bytes < base {
        return bytes.to_string();
    }
    let mut power = 1;
    let mut divisor = u128::from(base);
    while u128::from(bytes) / divisor >= u128::from(base) && power < LETTERS.len() {
        power += 1;
        divisor *= u128::from(base);
    }
    loop {
        let letter = if base == 1000 && power == 1 {
            'k'
        } else {
            LETTERS.get(power - 1).copied().unwrap_or('Y')
        };
        let tenths = (u128::from(bytes) * 10).div_ceil(divisor);
        if tenths < 100 {
            return std::format!("{}.{}{letter}", tenths / 10, tenths % 10);
        }
        let whole = u128::from(bytes).div_ceil(divisor);
        if whole < u128::from(base) || power == LETTERS.len() {
            return std::format!("{whole}{letter}");
        }
        power += 1;
        divisor *= u128::from(base);
    }
}

/// `--block-size`'s value (and `BLOCK_SIZE`'s), as GNU's `human_options` reads it: a leading
/// `'` (grouping, nothing in the C locale), `human-readable`, `si`, or a number (decimal,
/// `0x` hex, `0` octal) and a unit `K` to `Y`, of 1024, or with `B` (or `D`) of 1000, or
/// `iB` of 1024. A unit with no number is shown after each size. The error is GNU's words.
fn parse_block_size(spec: &str) -> Result<Scale, String> {
    let invalid = || std::format!("invalid --block-size argument '{spec}'");
    let invalid_suffix = || std::format!("invalid suffix in --block-size argument '{spec}'");
    let too_large = || std::format!("--block-size argument '{spec}' too large");
    let text = spec.strip_prefix('\'').unwrap_or(spec);
    match text {
        "human-readable" => return Ok(Scale::Human { base: 1024 }),
        "si" => return Ok(Scale::Human { base: 1000 }),
        _ => {}
    }
    if text.trim_start().starts_with('-') {
        return Err(invalid());
    }
    let (radix, start) = if text.len() > 2
        && (text.starts_with("0x") || text.starts_with("0X"))
        && text.as_bytes().get(2).is_some_and(u8::is_ascii_hexdigit)
    {
        (16, 2)
    } else if text.len() > 1 && text.starts_with('0') {
        (8, 1)
    } else {
        (10, 0)
    };
    let after_prefix = text.get(start..).unwrap_or_default();
    let digits_len = after_prefix
        .find(|c: char| !c.is_digit(radix))
        .unwrap_or(after_prefix.len());
    let digits = after_prefix.get(..digits_len).unwrap_or_default();
    let rest = after_prefix.get(digits_len..).unwrap_or_default();
    let number = if digits.is_empty() {
        // `0x` with nothing after, or the leading `0` of an octal number alone.
        (start > 0).then_some(0)
    } else {
        Some(u64::from_str_radix(digits, radix).map_err(|_| too_large())?)
    };

    let (multiplier, suffix) = if rest.is_empty() {
        (1_u64, String::new())
    } else {
        let mut chars = rest.chars();
        let letter = chars.next().unwrap_or_default();
        let power: u32 = match letter.to_ascii_uppercase() {
            'K' => 1,
            'M' => 2,
            'G' => 3,
            'T' => 4,
            'P' => 5,
            'E' => 6,
            'Z' => 7,
            'Y' => 8,
            _ if number.is_none() => return Err(invalid()),
            _ => return Err(invalid_suffix()),
        };
        let tail = chars.as_str();
        let (base, b) = match tail {
            "" => (1024_u64, false),
            "B" => (1000, true),
            "D" => (1000, false),
            "iB" => (1024, true),
            _ => return Err(invalid_suffix()),
        };
        let multiplier = base.checked_pow(power).ok_or_else(too_large)?;
        // GNU shows the unit as `human_readable` writes one: K for 1024, k for 1000, and
        // B, after an i for 1024, when the unit had one.
        let base_1024 = !b || tail == "iB";
        let shown = if base_1024 || power > 1 {
            letter.to_ascii_uppercase()
        } else {
            'k'
        };
        let mut suffix = shown.to_string();
        if b {
            if base_1024 {
                suffix.push('i');
            }
            suffix.push('B');
        }
        (multiplier, suffix)
    };
    let unit = match number {
        Some(number) => number.checked_mul(multiplier).ok_or_else(too_large)?,
        None if !rest.is_empty() => multiplier,
        None => return Err(invalid()),
    };
    if unit == 0 {
        return Err(invalid());
    }
    Ok(Scale::Units {
        unit,
        // A number given with the unit shows none.
        suffix: if number.is_some() {
            String::new()
        } else {
            suffix
        },
    })
}

/// `-l`'s time of a file, in the zone and the style `look` has: the recent format within
/// half a year, as GNU ls.
fn format_date(mtime: SystemTime, look: &Look) -> String {
    let when: DateTime<Utc> = mtime.into();
    let now = Utc::now();
    let recent =
        when > now - chrono::Duration::days(182) && when <= now + chrono::Duration::hours(1);
    let format = if recent {
        &look.dates.recent
    } else {
        &look.dates.old
    };
    look.zone.format(when, format)
}

/// The columns GNU ls lays `lengths` out in (`calculate_columns`): for each number of
/// columns the width allows, each column as wide as its widest name and two spaces, the
/// last without them, at least three; the most columns whose line stays under the width.
fn column_layout(lengths: &[usize], width: Option<usize>, down: bool) -> (usize, Vec<usize>) {
    const MIN_COLUMN_WIDTH: usize = 3;
    let count = lengths.len().max(1);
    let most = width.map_or(count, |width| (width / MIN_COLUMN_WIDTH).clamp(1, count));
    let mut candidates: Vec<(bool, usize, Vec<usize>)> = (1..=most)
        .map(|columns| {
            (
                true,
                columns * MIN_COLUMN_WIDTH,
                vec![MIN_COLUMN_WIDTH; columns],
            )
        })
        .collect();
    for (index, &length) in lengths.iter().enumerate() {
        for (offset, (fits, line, widths)) in candidates.iter_mut().enumerate() {
            if !*fits {
                continue;
            }
            let columns = offset + 1;
            let column = if down {
                index / count.div_ceil(columns)
            } else {
                index % columns
            };
            let wanted = length + if column + 1 == columns { 0 } else { 2 };
            if let Some(room) = widths.get_mut(column)
                && *room < wanted
            {
                *line += wanted - *room;
                *room = wanted;
                *fits = width.is_none_or(|width| *line < width);
            }
        }
    }
    let best = candidates.iter().rposition(|(fits, ..)| *fits).unwrap_or(0);
    let widths = candidates.swap_remove(best).2;
    (best + 1, widths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(line: &str) -> LsCommand {
        let words: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        LsCommand::parse(&words, Order::Permute)
    }

    #[test]
    fn the_command_line_is_read_as_gnu_ls_reads_it() {
        // Long names by any unique prefix, among GNU's whole table and cash's own.
        let command = parsed("--almost --recur --tr --col=never dir");
        assert!(command.almost_all && command.recursive && command.tree);
        assert_eq!(command.color.as_deref(), Some("never"));
        assert_eq!(command.paths, ["dir"]);
        assert_eq!(
            parsed("--al").early,
            Some(Early::Usage(
                "option '--al' is ambiguous; possibilities: '--all' '--almost-all'".to_owned()
            ))
        );
        assert_eq!(
            parsed("-y").early,
            Some(Early::Usage("invalid option -- 'y'".to_owned()))
        );
        // GNU's options cash's ls has not are named, not called invalid.
        assert_eq!(
            parsed("-i").early,
            Some(Early::Said("-i is not supported by cash's ls".to_owned()))
        );
        assert_eq!(
            parsed("--auth").early,
            Some(Early::Said(
                "--author is not supported by cash's ls".to_owned()
            ))
        );
        // The last of -N, -b, -Q and --quoting-style wins, and -q or --show-control-chars.
        let command = parsed("-b --quoting-style c -N -Q dir -q --show-control-chars");
        assert_eq!(command.quoting, Some(QuotingStyle::C));
        assert_eq!(command.hide_control, Some(false));
        assert_eq!(command.paths, ["dir"]);
        assert_eq!(
            parsed("-Q --quoting=shell").quoting,
            Some(QuotingStyle::Shell)
        );
        // coreutils 9's --classify[=WHEN]; -f is -a and -U.
        assert_eq!(parsed("-F --classify=never").indicator, Indicator::None);
        let auto = parsed("--classify=auto");
        assert!(auto.indicator == Indicator::Classify && auto.classify_auto && !auto.classifies());
        let command = parsed("-f");
        assert!(command.all && command.unsorted);
        // Help and version end the reading where they stand.
        assert!(parsed("--help -y").help);
        assert_eq!(parsed("--vers --bogus").early, Some(Early::Version));
        // POSIXLY_CORRECT: the options end at the first operand.
        let words: Vec<String> = ["dir", "-l"].map(str::to_owned).to_vec();
        let command = LsCommand::parse(&words, Order::StopAtOperand);
        assert_eq!((command.long, command.paths.len()), (false, 2));
    }

    #[test]
    fn version_sort_compares_numbers_as_numbers() {
        let mut names = vec!["file10", "file9", "file1", "v1.10", "v1.9", "v1.2"];
        names.sort_by(|a, b| compare_versions(a, b));
        assert_eq!(names, ["file1", "file9", "file10", "v1.2", "v1.9", "v1.10"]);
    }

    #[test]
    fn the_icon_tables_are_sorted_for_binary_search() {
        for table in [ls_icon_table::BY_NAME, ls_icon_table::BY_EXTENSION] {
            assert!(table.windows(2).all(|pair| pair[0].0 < pair[1].0));
        }
        assert_eq!(lookup(ls_icon_table::BY_EXTENSION, "rs"), Some("\u{e68b}"));
        assert_eq!(lookup(ls_icon_table::BY_NAME, ".bashrc"), Some("\u{f1183}"));
        assert_eq!(lookup(ls_icon_table::BY_NAME, "nope"), None);
    }

    #[test]
    fn default_colours_include_dircolors_extensions() {
        let colors = lscolors::LsColors::from_string(&default_ls_colors());
        assert!(colors.style_for_str("backup.zip").is_some());
        assert!(colors.style_for_str("notes.txt").is_none());
    }
    #[test]
    fn block_sizes_are_read_as_gnu_ls_reads_them() {
        // GNU coreutils 9.10's answers.
        let units = |unit: u64, suffix: &str| {
            Ok(Scale::Units {
                unit,
                suffix: suffix.to_owned(),
            })
        };
        for (spec, scale) in [
            ("1", units(1, "")),
            ("K", units(1024, "K")),
            ("k", units(1024, "K")),
            ("1K", units(1024, "")),
            ("KB", units(1000, "kB")),
            ("kB", units(1000, "kB")),
            ("MB", units(1_000_000, "MB")),
            ("KiB", units(1024, "KiB")),
            ("MiB", units(1_048_576, "MiB")),
            ("M", units(1_048_576, "M")),
            ("1000", units(1000, "")),
            ("'1", units(1, "")),
            ("0x10", units(16, "")),
            ("010", units(8, "")),
            ("human-readable", Ok(Scale::Human { base: 1024 })),
            ("si", Ok(Scale::Human { base: 1000 })),
        ] {
            assert_eq!(parse_block_size(spec), scale, "--block-size={spec}");
        }
        for (spec, message) in [
            ("nonsense", "invalid --block-size argument 'nonsense'"),
            ("0", "invalid --block-size argument '0'"),
            ("-1", "invalid --block-size argument '-1'"),
            ("", "invalid --block-size argument ''"),
            ("1Q", "invalid suffix in --block-size argument '1Q'"),
            ("1Kx", "invalid suffix in --block-size argument '1Kx'"),
            ("Z", "--block-size argument 'Z' too large"),
        ] {
            assert_eq!(
                parse_block_size(spec),
                Err(message.to_owned()),
                "--block-size={spec}"
            );
        }
    }

    #[test]
    fn human_sizes_round_up_as_gnu_ls_does() {
        for (bytes, base, shown) in [
            (1, 1024, "1"),
            (1023, 1024, "1023"),
            (1024, 1024, "1.0K"),
            (1500, 1024, "1.5K"),
            (5000, 1024, "4.9K"),
            (10_239, 1024, "10K"),
            (1_000_000, 1024, "977K"),
            (1_003_520, 1024, "980K"),
            (1_048_576, 1024, "1.0M"),
            (2_000_000, 1024, "2.0M"),
            (999, 1000, "999"),
            (4096, 1000, "4.1k"),
            (2_002_944, 1000, "2.1M"),
        ] {
            assert_eq!(human_size(bytes, base), shown, "{bytes} in {base}s");
        }
    }

    #[test]
    fn names_are_quoted_as_gnu_ls_quotes_them() {
        use QuotingStyle::{
            C, CMaybe, Escape, Literal, Locale, Shell, ShellAlways, ShellEscape, ShellEscapeAlways,
        };
        // GNU ls 9.10's, in C.UTF-8.
        for (style, name, shown) in [
            (Literal, "a b\u{1}", "a b\u{1}"),
            (Shell, "plain", "plain"),
            (Shell, "a b", "'a b'"),
            (Shell, "it's", "\"it's\""),
            (Shell, "it's here", "\"it's here\""),
            (Shell, "#it's", "\"#it's\""),
            (Shell, "it's!", "'it'\\''s!'"),
            (Shell, "it's#", "'it'\\''s#'"),
            (Shell, "it's\\", "'it'\\''s\\'"),
            (Shell, "q\"q", "'q\"q'"),
            (Shell, "back\\slash", "'back\\slash'"),
            (Shell, "~x", "'~x'"),
            (Shell, "x~", "x~"),
            (Shell, "=a", "'=a'"),
            (Shell, "a,b", "a,b"),
            (Shell, "a{b}", "a{b}"),
            (Shell, "%a", "%a"),
            (Shell, "end\u{1}", "end\u{1}"),
            (Shell, "tab\t", "'tab\t'"),
            (Shell, "it's\u{1}", "'it'\\''s\u{1}'"),
            (ShellAlways, "plain", "'plain'"),
            (ShellAlways, "it's", "\"it's\""),
            (ShellEscape, "plain", "plain"),
            (ShellEscape, "\u{1}", "''$'\\001'"),
            (ShellEscape, "end\u{1}", "'end'$'\\001'"),
            (ShellEscape, "c\u{1}d", "'c'$'\\001''d'"),
            (ShellEscape, "\u{1}\u{2}x", "''$'\\001\\002''x'"),
            (ShellEscape, "a\u{1}'", "'a'$'\\001'\\'''"),
            (ShellEscape, "a\u{1}'b", "'a'$'\\001'\\''b'"),
            (ShellEscape, "it's\u{1}", "'it'\\''s'$'\\001'"),
            (ShellEscape, "tab\t", "'tab'$'\\t'"),
            (ShellEscape, "x\u{80}y", "'x'$'\\302\\200''y'"),
            (ShellEscape, "a'", "\"a'\""),
            (ShellEscapeAlways, "plain", "'plain'"),
            (ShellEscapeAlways, "a'", "\"a'\""),
            (C, "plain", "\"plain\""),
            (C, "c\u{1}d", "\"c\\001d\""),
            (C, "a\u{1}'", "\"a\\001'\""),
            (C, "back\\slash", "\"back\\\\slash\""),
            (C, "q\"q", "\"q\\\"q\""),
            (CMaybe, "a b", "a b"),
            (CMaybe, "back\\slash", "back\\slash"),
            (CMaybe, "q\"q", "\"q\\\"q\""),
            (CMaybe, "tab\t", "\"tab\\t\""),
            (Escape, "it's here", "it's\\ here"),
            (Escape, "back\\slash", "back\\\\slash"),
            (Escape, "q\"q", "q\"q"),
            (Escape, "x\u{80}y", "x\\302\\200y"),
            (Locale, "it's", "\u{2018}it's\u{2019}"),
            (Locale, "back\\slash", "\u{2018}back\\\\slash\u{2019}"),
            (Locale, "q\"q", "\u{2018}q\"q\u{2019}"),
            (Locale, "tab\t", "\u{2018}tab\\t\u{2019}"),
        ] {
            assert_eq!(style.quote(name).0, shown, "{name:?} in {style:?}");
        }
    }
}
