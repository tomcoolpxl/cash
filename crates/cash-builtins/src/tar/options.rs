//! GNU tar 1.35's command line: its option table, in its own order so that an ambiguous
//! prefix lists what GNU's lists; old-style keys; `TAR_OPTIONS`; what each option
//! sets.

use cash_archive::codec::Codec;
use cash_archive::listing::Style;
use cash_archive::tar::Format;
use cash_getopt::{Arg, Getopt, Item, Long, Short};

/// The operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Op {
    Create,
    Append,
    Update,
    Catenate,
    List,
    Extract,
    Diff,
    Delete,
    TestLabel,
}

/// A command-line argument that is not an option, in order: a name, or a folder to
/// change to before the names after it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Name {
    Path(String),
    Chdir(String),
    /// `-T FILE`: names read from a file, in its place among the others.
    FilesFrom(String),
}

/// How names and patterns match, as the options before them set it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Matching {
    /// `--wildcards` or `--no-wildcards`; unset, members are literal and exclusions
    /// patterns.
    pub wildcards: Option<bool>,
    pub wildcards_match_slash: Option<bool>,
    pub anchored: Option<bool>,
    pub ignore_case: bool,
}

/// `--sort`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Sort {
    None,
    Name,
    Inode,
}

/// What the command line says.
#[derive(Clone, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "GNU tar's switches, one each"
)]
pub(super) struct Options {
    pub op: Option<Op>,
    pub archive: Option<String>,
    pub verbose: u8,
    /// `-z`, `-j`, `-J`, `--lzma`, `--lzip`, `--zstd`.
    pub codec: Option<Codec>,
    /// A compressor cash does not carry: `-Z`, `--lzop`, `-I`.
    pub refused_compressor: Option<String>,
    pub auto_compress: bool,
    pub names: Vec<Name>,
    pub null: bool,
    pub excludes: Vec<(String, Matching)>,
    pub exclude_from: Vec<(String, Matching)>,
    pub exclude_vcs: bool,
    pub exclude_backups: bool,
    /// `--exclude-caches*` and `--exclude-tag*`, in the order given.
    pub tags: Vec<Tag>,
    pub matching: Matching,
    pub recursion: bool,
    pub strip_components: usize,
    pub transforms: Vec<String>,
    pub show_transformed: bool,
    pub show_stored: bool,
    pub keep_old: bool,
    pub skip_old: bool,
    pub keep_newer: bool,
    pub overwrite: bool,
    pub unlink_first: bool,
    pub remove_files: bool,
    pub to_stdout: bool,
    pub touch: bool,
    pub numeric_owner: bool,
    pub owner: Option<String>,
    pub group: Option<String>,
    pub mode: Option<String>,
    pub mtime: Option<String>,
    pub clamp_mtime: bool,
    pub format: Option<Format>,
    pub blocking: u64,
    pub ignore_zeros: bool,
    pub occurrence: Option<u64>,
    pub starting_file: Option<String>,
    pub sort: Sort,
    pub dereference: bool,
    pub hard_dereference: bool,
    pub absolute_names: bool,
    pub totals: bool,
    pub utc: bool,
    pub full_time: bool,
    pub quoting: Style,
    pub quote_chars: Vec<u8>,
    pub label: Option<String>,
    pub newer: Option<String>,
    pub newer_mtime_only: bool,
    pub one_top_level: TopLevel,
    pub interactive: bool,
    pub show_defaults: bool,
    pub no_same_owner_letter_o: bool,
    /// An option cash's tar does not do, by its name: refused when the run starts.
    pub unsupported: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            op: None,
            archive: None,
            verbose: 0,
            codec: None,
            refused_compressor: None,
            auto_compress: false,
            names: Vec::new(),
            null: false,
            excludes: Vec::new(),
            exclude_from: Vec::new(),
            exclude_vcs: false,
            exclude_backups: false,
            tags: Vec::new(),
            matching: Matching::default(),
            recursion: true,
            strip_components: 0,
            transforms: Vec::new(),
            show_transformed: false,
            show_stored: false,
            keep_old: false,
            skip_old: false,
            keep_newer: false,
            overwrite: false,
            unlink_first: false,
            remove_files: false,
            to_stdout: false,
            touch: false,
            numeric_owner: false,
            owner: None,
            group: None,
            mode: None,
            mtime: None,
            clamp_mtime: false,
            format: None,
            blocking: 20,
            ignore_zeros: false,
            occurrence: None,
            starting_file: None,
            sort: Sort::None,
            dereference: false,
            hard_dereference: false,
            absolute_names: false,
            totals: false,
            utc: false,
            full_time: false,
            quoting: Style::Escape,
            quote_chars: Vec::new(),
            label: None,
            newer: None,
            newer_mtime_only: false,
            one_top_level: TopLevel::Off,
            interactive: false,
            show_defaults: false,
            no_same_owner_letter_o: false,
            unsupported: None,
        }
    }
}

/// GNU tar's short options; a colon follows one that takes a value.
pub(super) const SHORTS: &str = "AcdrtuxGnSkUWOmpsMBiajJzZhPlRvwo?g:C:T:X:f:F:L:b:H:V:I:K:N:";

macro_rules! no {
    ($name:literal) => {
        Long::new($name, Arg::No, $name)
    };
    ($name:literal, $id:literal) => {
        Long::new($name, Arg::No, $id)
    };
}
macro_rules! req {
    ($name:literal) => {
        Long::new($name, Arg::Required, $name)
    };
    ($name:literal, $id:literal) => {
        Long::new($name, Arg::Required, $id)
    };
}
macro_rules! opt {
    ($name:literal) => {
        Long::new($name, Arg::Optional, $name)
    };
}

/// GNU tar 1.35's long options, by their first letter in GNU's own order, the order its
/// ambiguity messages list them in. An alias shares its option's id.
pub(super) const LONGS: &[Long<'static, &'static str>] = &[
    no!("append"),
    opt!("atime-preserve"),
    no!("acls"),
    no!("auto-compress"),
    no!("absolute-names"),
    req!("after-date"),
    req!("add-file"),
    no!("anchored"),
    req!("blocking-factor"),
    no!("bzip2"),
    opt!("backup"),
    no!("block-number"),
    no!("create"),
    no!("compare", "diff"),
    no!("catenate"),
    no!("concatenate", "catenate"),
    no!("check-device"),
    no!("clamp-mtime"),
    no!("compress"),
    opt!("checkpoint"),
    req!("checkpoint-action"),
    no!("check-links"),
    no!("confirmation", "interactive"),
    no!("diff"),
    no!("delete"),
    no!("delay-directory-restore"),
    no!("dereference"),
    req!("directory"),
    no!("extract"),
    req!("exclude"),
    req!("exclude-from"),
    no!("exclude-caches"),
    no!("exclude-caches-under"),
    no!("exclude-caches-all"),
    req!("exclude-tag"),
    req!("exclude-ignore"),
    req!("exclude-ignore-recursive"),
    req!("exclude-tag-under"),
    req!("exclude-tag-all"),
    no!("exclude-vcs"),
    no!("exclude-vcs-ignores"),
    no!("exclude-backups"),
    req!("file"),
    no!("force-local"),
    req!("format"),
    no!("full-time"),
    req!("files-from"),
    no!("get", "extract"),
    req!("group"),
    req!("group-map"),
    no!("gzip"),
    no!("gunzip", "gzip"),
    req!("hole-detection"),
    no!("hard-dereference"),
    no!("help"),
    no!("incremental"),
    no!("ignore-failed-read"),
    no!("ignore-command-error"),
    req!("info-script"),
    no!("ignore-zeros"),
    req!("index-file"),
    no!("interactive"),
    no!("ignore-case"),
    no!("keep-old-files"),
    no!("keep-newer-files"),
    no!("keep-directory-symlink"),
    no!("list"),
    req!("listed-incremental"),
    req!("level"),
    req!("label"),
    no!("lzip"),
    no!("lzma"),
    no!("lzop"),
    req!("mtime"),
    req!("mode"),
    no!("multi-volume"),
    no!("no-seek"),
    no!("no-check-device"),
    no!("no-overwrite-dir"),
    no!("no-ignore-command-error"),
    no!("no-same-owner"),
    no!("numeric-owner"),
    no!("no-same-permissions"),
    no!("no-delay-directory-restore"),
    no!("no-xattrs"),
    no!("no-selinux"),
    no!("no-acls"),
    req!("new-volume-script", "info-script"),
    no!("no-auto-compress"),
    req!("newer"),
    req!("newer-mtime"),
    req!("no-quote-chars"),
    no!("null"),
    no!("no-null"),
    no!("no-unquote"),
    no!("no-verbatim-files-from"),
    no!("no-recursion"),
    no!("no-anchored"),
    no!("no-ignore-case"),
    no!("no-wildcards"),
    no!("no-wildcards-match-slash"),
    opt!("occurrence"),
    no!("overwrite"),
    no!("overwrite-dir"),
    opt!("one-top-level"),
    req!("owner"),
    req!("owner-map"),
    no!("old-archive"),
    no!("one-file-system"),
    no!("preserve-permissions"),
    no!("preserve-order", "same-order"),
    no!("portability", "old-archive"),
    no!("posix"),
    req!("pax-option"),
    req!("program-name"),
    req!("quoting-style"),
    req!("quote-chars"),
    no!("remove-files"),
    no!("recursive-unlink"),
    req!("rmt-command"),
    req!("rsh-command"),
    req!("record-size"),
    no!("read-full-records"),
    no!("restrict"),
    no!("recursion"),
    no!("sparse"),
    req!("sparse-version"),
    no!("seek"),
    no!("skip-old-files"),
    no!("same-owner"),
    no!("same-permissions", "preserve-permissions"),
    no!("same-order"),
    req!("sort"),
    no!("selinux"),
    req!("starting-file"),
    req!("suffix"),
    req!("strip-components"),
    no!("show-defaults"),
    req!("show-snapshot-field-ranges"),
    no!("show-omitted-dirs"),
    no!("show-transformed-names"),
    no!("show-stored-names"),
    no!("test-label"),
    no!("to-stdout"),
    req!("to-command"),
    no!("touch"),
    req!("tape-length"),
    req!("transform"),
    opt!("totals"),
    no!("update"),
    no!("unlink-first"),
    req!("use-compress-program"),
    no!("ungzip", "gzip"),
    no!("uncompress", "compress"),
    no!("utc"),
    no!("unquote"),
    no!("usage"),
    no!("verify"),
    req!("volno-file"),
    no!("verbose"),
    no!("verbatim-files-from"),
    no!("version"),
    req!("warning"),
    no!("wildcards"),
    no!("wildcards-match-slash"),
    no!("xattrs"),
    req!("xattrs-include"),
    req!("xattrs-exclude"),
    no!("xz"),
    req!("xform", "transform"),
    no!("zstd"),
];

/// What a short option is, as its long name.
pub(super) const fn short_id(letter: char) -> &'static str {
    match letter {
        'A' => "catenate",
        'c' => "create",
        'd' => "diff",
        'r' => "append",
        't' => "list",
        'u' => "update",
        'x' => "extract",
        'G' => "incremental",
        'n' => "seek",
        'S' => "sparse",
        'k' => "keep-old-files",
        'U' => "unlink-first",
        'W' => "verify",
        'O' => "to-stdout",
        'm' => "touch",
        'p' => "preserve-permissions",
        's' => "same-order",
        'M' => "multi-volume",
        'B' => "read-full-records",
        'i' => "ignore-zeros",
        'a' => "auto-compress",
        'j' => "bzip2",
        'J' => "xz",
        'z' => "gzip",
        'Z' => "compress",
        'h' => "dereference",
        'P' => "absolute-names",
        'l' => "check-links",
        'R' => "block-number",
        'v' => "verbose",
        'w' => "interactive",
        'o' => "o",
        '?' => "help",
        'g' => "listed-incremental",
        'C' => "directory",
        'T' => "files-from",
        'X' => "exclude-from",
        'f' => "file",
        'F' => "info-script",
        'L' => "tape-length",
        'b' => "blocking-factor",
        'H' => "format",
        'V' => "label",
        'I' => "use-compress-program",
        'K' => "starting-file",
        'N' => "newer",
        _ => "",
    }
}

/// What reading the command line came to, besides options to run with.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum Stop {
    /// `--help`, `--usage`, `--version`, `--show-defaults`: said on standard output.
    Say(String),
    /// A usage error: said, with GNU's "Try" line, and this status.
    Usage(String, u8),
}

/// GNU's "Try" line.
pub(super) const TRY: &str = "Try 'tar --help' or 'tar --usage' for more information.";

fn usage(message: impl Into<String>, status: u8) -> Stop {
    Stop::Usage(message.into(), status)
}

/// What a tag file keeps out of the archive: the rest of its folder, everything in the
/// folder, or the folder itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TagWhat {
    Contents,
    Under,
    All,
}

/// A file whose presence in a folder keeps the folder's contents out of the archive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tag {
    pub file: String,
    /// `CACHEDIR.TAG`, which counts only with the Cache Directory Tagging signature.
    pub cachedir: bool,
    pub what: TagWhat,
}

/// `--one-top-level`: off, a folder named by the archive's name without its suffixes,
/// or a folder given.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TopLevel {
    Off,
    FromArchive,
    Named(String),
}

/// GNU's `strip_compression_suffix` of the archive's base name: `a.tar.gz` and
/// `a.tgz` are `a`; `None` for a name without a suffix it knows.
pub(super) fn top_level_of(archive: &str) -> Option<String> {
    const SUFFIXES: [&str; 19] = [
        "tar", "gz", "z", "tgz", "taz", "Z", "taZ", "bz2", "tbz", "tbz2", "tz2", "lz", "lzma",
        "tlz", "lzo", "xz", "txz", "zst", "tzst",
    ];
    let base = archive.rsplit(['/', '\\']).next().unwrap_or(archive);
    let (stem, suffix) = base.rsplit_once('.')?;
    if !SUFFIXES.contains(&suffix) {
        return None;
    }
    let stem = match stem.strip_suffix(".tar") {
        Some(inner) if !suffix.starts_with('t') && !inner.is_empty() => inner,
        _ => stem,
    };
    (!stem.is_empty()).then(|| stem.to_owned())
}

/// The operation, which may be given once.
fn set_op(options: &mut Options, op: Op) -> Result<(), Stop> {
    if options.op.is_some_and(|old| old != op) {
        return Err(usage(
            "You may not specify more than one '-Acdtrux', '--delete' or  '--test-label' option",
            2,
        ));
    }
    options.op = Some(op);
    Ok(())
}

/// GNU tar's `parse_opt` for one option.
#[expect(
    clippy::too_many_lines,
    reason = "GNU tar's parse_opt, one case per option"
)]
fn apply(options: &mut Options, id: &str, value: Option<&str>) -> Result<(), Stop> {
    let value_owned = value.unwrap_or_default().to_owned();
    match id {
        "catenate" => set_op(options, Op::Catenate)?,
        "create" => set_op(options, Op::Create)?,
        "diff" => set_op(options, Op::Diff)?,
        "append" => set_op(options, Op::Append)?,
        "list" => set_op(options, Op::List)?,
        "update" => set_op(options, Op::Update)?,
        "extract" => set_op(options, Op::Extract)?,
        "delete" => set_op(options, Op::Delete)?,
        "test-label" => set_op(options, Op::TestLabel)?,
        "file" => options.archive = Some(value_owned),
        "directory" => options.names.push(Name::Chdir(value_owned)),
        "files-from" => options.names.push(Name::FilesFrom(value_owned)),
        "exclude" => options.excludes.push((value_owned, options.matching)),
        "exclude-from" => options.exclude_from.push((value_owned, options.matching)),
        "exclude-vcs" => options.exclude_vcs = true,
        "exclude-backups" => options.exclude_backups = true,
        "exclude-caches"
        | "exclude-caches-under"
        | "exclude-caches-all"
        | "exclude-tag"
        | "exclude-tag-under"
        | "exclude-tag-all" => {
            let what = if id.ends_with("-under") {
                TagWhat::Under
            } else if id.ends_with("-all") {
                TagWhat::All
            } else {
                TagWhat::Contents
            };
            let cachedir = id.starts_with("exclude-caches");
            options.tags.push(Tag {
                file: if cachedir {
                    "CACHEDIR.TAG".to_owned()
                } else {
                    value_owned
                },
                cachedir,
                what,
            });
        }
        "verbose" => options.verbose = options.verbose.saturating_add(1),
        "gzip" => options.codec = Some(Codec::Gzip),
        "bzip2" => options.codec = Some(Codec::Bzip2),
        "xz" => options.codec = Some(Codec::Xz),
        "lzma" => options.codec = Some(Codec::Lzma),
        "lzip" => options.codec = Some(Codec::Lzip),
        "zstd" => options.codec = Some(Codec::Zstd),
        "compress" => options.refused_compressor = Some("compress".to_owned()),
        "lzop" => options.refused_compressor = Some("lzop".to_owned()),
        "use-compress-program" => options.refused_compressor = Some(value_owned),
        "auto-compress" => options.auto_compress = true,
        "no-auto-compress" => options.auto_compress = false,
        "null" => options.null = true,
        "no-null" => options.null = false,
        "wildcards" => options.matching.wildcards = Some(true),
        "no-wildcards" => options.matching.wildcards = Some(false),
        "wildcards-match-slash" => options.matching.wildcards_match_slash = Some(true),
        "no-wildcards-match-slash" => options.matching.wildcards_match_slash = Some(false),
        "anchored" => options.matching.anchored = Some(true),
        "no-anchored" => options.matching.anchored = Some(false),
        "ignore-case" => options.matching.ignore_case = true,
        "no-ignore-case" => options.matching.ignore_case = false,
        "recursion" => options.recursion = true,
        "no-recursion" => options.recursion = false,
        "strip-components" => {
            options.strip_components = value_owned
                .parse()
                .map_err(|_| usage(format!("{value_owned}: Invalid number of elements"), 2))?;
        }
        "transform" => options.transforms.push(value_owned),
        "show-transformed-names" => options.show_transformed = true,
        "show-stored-names" => options.show_stored = true,
        "keep-old-files" => options.keep_old = true,
        "skip-old-files" => options.skip_old = true,
        "keep-newer-files" => options.keep_newer = true,
        "overwrite" => options.overwrite = true,
        "unlink-first" => options.unlink_first = true,
        "remove-files" => options.remove_files = true,
        "to-stdout" => options.to_stdout = true,
        "touch" => options.touch = true,
        "numeric-owner" => options.numeric_owner = true,
        "owner" => options.owner = Some(value_owned),
        "group" => options.group = Some(value_owned),
        "mode" => options.mode = Some(value_owned),
        "mtime" => options.mtime = Some(value_owned),
        "clamp-mtime" => options.clamp_mtime = true,
        "format" => {
            options.format = Some(
                Format::by_name(&value_owned)
                    .ok_or_else(|| usage(format!("{value_owned}: Invalid archive format"), 2))?,
            );
        }
        "old-archive" => options.format = Some(Format::V7),
        "posix" => options.format = Some(Format::Posix),
        "o" => {
            options.no_same_owner_letter_o = true;
        }
        "blocking-factor" => {
            options.blocking = value_owned
                .parse::<u64>()
                .ok()
                .filter(|b| (1..=4096).contains(b))
                .ok_or_else(|| usage(format!("{value_owned}: Invalid blocking factor"), 2))?;
        }
        "record-size" => {
            let size: u64 = value_owned
                .parse()
                .map_err(|_| usage(format!("{value_owned}: Invalid record size"), 2))?;
            if !size.is_multiple_of(512) || size == 0 {
                return Err(usage(
                    format!("Record size must be a multiple of {}.", 512),
                    2,
                ));
            }
            options.blocking = size / 512;
        }
        "ignore-zeros" => options.ignore_zeros = true,
        "occurrence" => {
            options.occurrence = Some(match value {
                None => 1,
                Some(text) => text
                    .parse()
                    .map_err(|_| usage(format!("{text}: Invalid number"), 2))?,
            });
        }
        "starting-file" => options.starting_file = Some(value_owned),
        "sort" => {
            options.sort = match value_owned.as_str() {
                "none" => Sort::None,
                "name" => Sort::Name,
                "inode" => Sort::Inode,
                _ => {
                    return Err(usage(
                        format!(
                            "invalid argument '{value_owned}' for '--sort'\nValid arguments are:\n  - 'none'\n  - 'name'\n  - 'inode'"
                        ),
                        2,
                    ));
                }
            };
        }
        "dereference" => options.dereference = true,
        "hard-dereference" => options.hard_dereference = true,
        "absolute-names" => options.absolute_names = true,
        "totals" => options.totals = true,
        "utc" => options.utc = true,
        "full-time" => options.full_time = true,
        "quoting-style" => {
            options.quoting = Style::by_name(&value_owned).ok_or_else(|| {
                let mut text = format!(
                    "invalid argument '{value_owned}' for '--quoting-style'\nValid arguments are:"
                );
                for name in Style::NAMES {
                    text.push_str("\n  - '");
                    text.push_str(name);
                    text.push('\'');
                }
                usage(text, 2)
            })?;
        }
        "quote-chars" => options.quote_chars.extend(value_owned.bytes()),
        "no-quote-chars" => options
            .quote_chars
            .retain(|c| !value_owned.as_bytes().contains(c)),
        "label" => options.label = Some(value_owned),
        "newer" | "after-date" => options.newer = Some(value_owned),
        "newer-mtime" => {
            options.newer = Some(value_owned);
            options.newer_mtime_only = true;
        }
        "one-top-level" => {
            options.one_top_level =
                value.map_or(TopLevel::FromArchive, |dir| TopLevel::Named(dir.to_owned()));
        }
        "interactive" => options.interactive = true,
        "show-defaults" => options.show_defaults = true,
        "multi-volume"
        | "listed-incremental"
        | "incremental"
        | "tape-length"
        | "info-script"
        | "volno-file"
        | "to-command"
        | "verify"
        | "add-file"
        | "show-snapshot-field-ranges"
        | "level"
        | "index-file"
        | "rmt-command"
        | "rsh-command" => {
            options.unsupported.get_or_insert_with(|| format!("--{id}"));
        }
        // Read and of no effect here: owners, permissions, attributes, tapes, checkpoints.
        _ => {}
    }
    Ok(())
}

/// Reads `words` into `options`; `from_environment` for `TAR_OPTIONS`, whose names are
/// files as the command line's are.
fn read(options: &mut Options, words: &[String]) -> Result<(), Stop> {
    let shorts: Vec<Short<&'static str>> = cash_getopt::optstring(SHORTS)
        .into_iter()
        .map(|short| Short::new(short.letter, short.arg, short_id(short.letter)))
        .collect();
    let words = cash_getopt::old_style(words, &shorts);
    for next in Getopt::new(&shorts, LONGS).read(&words) {
        match next {
            Ok(Item::Option { id, value, .. }) => match id {
                "help" => return Err(Stop::Say(super::help::HELP.to_owned())),
                "usage" => return Err(Stop::Say(super::help::USAGE.to_owned())),
                "version" => return Err(Stop::Say(format!("{}\n", super::VERSION))),
                _ => apply(options, id, value.as_deref())?,
            },
            Ok(Item::Operand { value, .. }) => options.names.push(Name::Path(value)),
            Err(problem) => return Err(usage(problem.to_string(), 64)),
        }
    }
    Ok(())
}

/// The whole command line, `TAR_OPTIONS` first.
pub(super) fn parse(environment: Option<&str>, args: &[String]) -> Result<Options, Stop> {
    let mut options = Options::default();
    if let Some(value) = environment {
        let words: Vec<String> = value.split_ascii_whitespace().map(str::to_owned).collect();
        if let Err(stop) = read(&mut options, &words) {
            return Err(match stop {
                Stop::Usage(message, status) => Stop::Usage(
                    format!("{message}\n{TRY}\nerror parsing TAR_OPTIONS"),
                    status,
                ),
                other @ Stop::Say(_) => other,
            });
        }
    }
    read(&mut options, args)?;
    if options.show_defaults {
        return Err(Stop::Say(
            "--format=gnu -f- -b20 --quoting-style=escape\n".to_owned(),
        ));
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(words: &[&str]) -> Result<Options, Stop> {
        let words: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
        parse(None, &words)
    }

    #[test]
    fn the_top_level_folder_is_the_archive_name_without_its_suffixes() {
        for (archive, dir) in [
            ("pack.tar", Some("pack")),
            ("dir/pack.tar.gz", Some("pack")),
            ("C:\\x\\pack.tgz", Some("pack")),
            ("pack.tar.zst", Some("pack")),
            ("pack.tbz2", Some("pack")),
            ("a.b.xz", Some("a.b")),
            ("pack", None),
            ("-", None),
            (".tar", None),
            ("pack.zip", None),
        ] {
            assert_eq!(top_level_of(archive).as_deref(), dir, "{archive}");
        }
    }

    #[test]
    fn old_style_keys_and_options_read_as_gnu_tar_reads_them() {
        let o = parsed(&["cvf", "a.tar", "src"]).unwrap();
        assert_eq!((o.op, o.verbose), (Some(Op::Create), 1));
        assert_eq!(o.archive.as_deref(), Some("a.tar"));
        assert_eq!(o.names, [Name::Path("src".into())]);
        let o = parsed(&["-x", "-C", "out", "-f", "a.tar", "--strip-components=1"]).unwrap();
        assert_eq!(o.names, [Name::Chdir("out".into())]);
        assert_eq!(o.strip_components, 1);
        assert_eq!(
            parsed(&["-ct"]).unwrap_err(),
            Stop::Usage(
                "You may not specify more than one '-Acdtrux', '--delete' or  '--test-label' option".into(),
                2
            )
        );
        assert!(matches!(parsed(&["--foo"]), Err(Stop::Usage(_, 64))));
        let Err(Stop::Usage(message, _)) = parsed(&["--e"]) else {
            unreachable!("--e is ambiguous");
        };
        assert!(
            message
                .starts_with("option '--e' is ambiguous; possibilities: '--extract' '--exclude'"),
            "{message}"
        );
    }
}
