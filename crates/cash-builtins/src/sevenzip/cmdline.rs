//! 7-Zip's command line: its switch table and parser (`CommandLineParser.cpp`), and the
//! two passes of `ArchiveCommandLine.cpp`: the switches first, before the banner, then
//! the command, the archive's name and the names of items.

use super::censor::{Censor, MarkMode, PathMode, Recursion};

/// Where one of 7-Zip's three streams goes (`-bso`, `-bse`, `-bsp`: 0, 1 or 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Off,
    Out,
    Err,
}

/// A command line 7-Zip refuses: "Command Line Error:", the message, and the argument
/// it is about.
#[derive(Debug)]
pub(super) struct CmdLineError {
    pub(super) message: String,
    pub(super) line: Option<String>,
}

impl CmdLineError {
    pub(super) fn new(message: &str) -> Self {
        Self {
            message: message.to_owned(),
            line: None,
        }
    }

    fn with(message: &str, line: &str) -> Self {
        Self {
            message: message.to_owned(),
            line: Some(line.to_owned()),
        }
    }
}

/// 7-Zip's commands, in its `g_Commands` order (`audtexlbih`, and `rn`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Add,
    Update,
    Delete,
    Test,
    Extract,
    ExtractFull,
    List,
    Benchmark,
    Info,
    Hash,
    Rename,
}

impl Command {
    fn parse(word: &str) -> Option<Self> {
        let lower = word.to_ascii_lowercase();
        Some(match lower.as_str() {
            "a" => Self::Add,
            "u" => Self::Update,
            "d" => Self::Delete,
            "t" => Self::Test,
            "e" => Self::Extract,
            "x" => Self::ExtractFull,
            "l" => Self::List,
            "b" => Self::Benchmark,
            "i" => Self::Info,
            "h" => Self::Hash,
            "rn" => Self::Rename,
            _ => return None,
        })
    }

    pub(super) const fn is_extract_group(self) -> bool {
        matches!(self, Self::Test | Self::Extract | Self::ExtractFull)
    }

    pub(super) const fn is_update_group(self) -> bool {
        matches!(self, Self::Add | Self::Update | Self::Delete | Self::Rename)
    }
}

/// What to do with a file that is already there (`-ao`; asked unless `-y`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Overwrite {
    Ask,
    /// `-aoa`, or `-y`.
    All,
    /// `-aos`.
    Skip,
    /// `-aou`: the new file under a new name.
    Rename,
    /// `-aot`: the old file under a new name.
    RenameExisting,
}

/// How much of an item's path extraction keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PathKeep {
    /// `x`, `t`.
    Full,
    /// `e`.
    None,
    /// `-spf`: absolute paths kept.
    Absolute,
}

#[derive(Clone, Copy)]
enum Kind {
    Simple,
    Minus,
    Char(&'static str),
    Str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum Key {
    Help1,
    Help2,
    Help3,
    DisableHeaders,
    DisablePercents,
    ShowTime,
    LogLevel,
    OutStream,
    ErrStream,
    PercentStream,
    Yes,
    ShowDialog,
    Overwrite,
    ArchiveType,
    ExcludedArcType,
    Property,
    OutputDir,
    WorkingDir,
    Include,
    Exclude,
    ArInclude,
    ArExclude,
    NoArName,
    Update,
    Volume,
    Recursed,
    Affinity,
    Sfx,
    Email,
    Hash,
    HashDir,
    ExtractMemLimit,
    StdIn,
    StdOut,
    LargePages,
    ListfileCharSet,
    ConsoleCharSet,
    TechMode,
    ListFields,
    ListPathSlash,
    ListTimestampUtc,
    PreserveATime,
    ShareForWrite,
    StopAfterOpenError,
    CaseSensitive,
    ArcNameMode,
    UseSlashMark,
    DisableWildcardParsing,
    ElimDup,
    FullPathMode,
    OutDirMode,
    HardLinks,
    SymLinksAllowDangerous,
    SymLinks,
    NtSecurity,
    StoreOwnerId,
    StoreOwnerName,
    ZoneFile,
    AltStreams,
    ReplaceColonForAltStream,
    WriteToAltStreamIfColon,
    NameTrailReplace,
    DeleteAfterCompressing,
    SetArcMTime,
    Password,
}

/// 7-Zip 26.03's `kSwitchForms`: the name, its kind, whether it may repeat, and how many
/// characters must follow it.
const FORMS: [(&str, Key, Kind, bool, usize); 65] = [
    ("?", Key::Help1, Kind::Simple, false, 0),
    ("h", Key::Help2, Kind::Simple, false, 0),
    ("-help", Key::Help3, Kind::Simple, false, 0),
    ("ba", Key::DisableHeaders, Kind::Simple, false, 0),
    ("bd", Key::DisablePercents, Kind::Simple, false, 0),
    ("bt", Key::ShowTime, Kind::Simple, false, 0),
    ("bb", Key::LogLevel, Kind::Str, false, 0),
    ("bso", Key::OutStream, Kind::Char("012"), false, 1),
    ("bse", Key::ErrStream, Kind::Char("012"), false, 1),
    ("bsp", Key::PercentStream, Kind::Char("012"), false, 1),
    ("y", Key::Yes, Kind::Simple, false, 0),
    ("ad", Key::ShowDialog, Kind::Simple, false, 0),
    ("ao", Key::Overwrite, Kind::Char("asut"), false, 1),
    ("t", Key::ArchiveType, Kind::Str, false, 1),
    ("stx", Key::ExcludedArcType, Kind::Str, true, 1),
    ("m", Key::Property, Kind::Str, true, 1),
    ("o", Key::OutputDir, Kind::Str, false, 1),
    ("w", Key::WorkingDir, Kind::Str, false, 0),
    ("i", Key::Include, Kind::Str, true, 2),
    ("x", Key::Exclude, Kind::Str, true, 2),
    ("ai", Key::ArInclude, Kind::Str, true, 2),
    ("ax", Key::ArExclude, Kind::Str, true, 2),
    ("an", Key::NoArName, Kind::Simple, false, 0),
    ("u", Key::Update, Kind::Str, true, 1),
    ("v", Key::Volume, Kind::Str, true, 1),
    ("r", Key::Recursed, Kind::Char("0-"), false, 0),
    ("stm", Key::Affinity, Kind::Str, false, 0),
    ("sfx", Key::Sfx, Kind::Str, false, 0),
    ("seml", Key::Email, Kind::Str, false, 0),
    ("scrc", Key::Hash, Kind::Str, true, 0),
    ("shd", Key::HashDir, Kind::Str, false, 1),
    ("smemx", Key::ExtractMemLimit, Kind::Str, false, 0),
    ("si", Key::StdIn, Kind::Str, false, 0),
    ("so", Key::StdOut, Kind::Simple, false, 0),
    ("slp", Key::LargePages, Kind::Str, false, 0),
    ("scs", Key::ListfileCharSet, Kind::Str, false, 0),
    ("scc", Key::ConsoleCharSet, Kind::Str, false, 0),
    ("slt", Key::TechMode, Kind::Simple, false, 0),
    ("slf", Key::ListFields, Kind::Str, false, 1),
    ("slsl", Key::ListPathSlash, Kind::Minus, false, 0),
    ("slmu", Key::ListTimestampUtc, Kind::Minus, false, 0),
    ("ssp", Key::PreserveATime, Kind::Simple, false, 0),
    ("ssw", Key::ShareForWrite, Kind::Simple, false, 0),
    ("sse", Key::StopAfterOpenError, Kind::Simple, false, 0),
    ("ssc", Key::CaseSensitive, Kind::Minus, false, 0),
    ("sa", Key::ArcNameMode, Kind::Char("sea"), false, 1),
    ("spm", Key::UseSlashMark, Kind::Str, false, 0),
    ("spd", Key::DisableWildcardParsing, Kind::Simple, false, 0),
    ("spe", Key::ElimDup, Kind::Minus, false, 0),
    ("spf", Key::FullPathMode, Kind::Str, false, 0),
    ("spo", Key::OutDirMode, Kind::Char("dcr"), false, 1),
    ("snh", Key::HardLinks, Kind::Minus, false, 0),
    ("snld", Key::SymLinksAllowDangerous, Kind::Str, false, 0),
    ("snl", Key::SymLinks, Kind::Minus, false, 0),
    ("sni", Key::NtSecurity, Kind::Simple, false, 0),
    ("snoi", Key::StoreOwnerId, Kind::Minus, false, 0),
    ("snon", Key::StoreOwnerName, Kind::Minus, false, 0),
    ("snz", Key::ZoneFile, Kind::Str, false, 0),
    ("sns", Key::AltStreams, Kind::Minus, false, 0),
    ("snr", Key::ReplaceColonForAltStream, Kind::Simple, false, 0),
    ("snc", Key::WriteToAltStreamIfColon, Kind::Simple, false, 0),
    ("snt", Key::NameTrailReplace, Kind::Minus, false, 0),
    ("sdel", Key::DeleteAfterCompressing, Kind::Simple, false, 0),
    ("stl", Key::SetArcMTime, Kind::Simple, false, 0),
    ("p", Key::Password, Kind::Str, false, 0),
];

#[derive(Clone, Default)]
struct Switch {
    there: bool,
    minus: bool,
    char_index: Option<usize>,
    strings: Vec<String>,
}

/// The first pass: the switches, read before the banner, and the words that are not
/// switches.
pub(super) struct Parsed {
    switches: Vec<Switch>,
    words: Vec<String>,
    /// Where `--` stood among the words: no `@listfile` after it.
    stop_index: Option<usize>,
    pub(super) help: bool,
    pub(super) headers: bool,
}

impl Parsed {
    fn get(&self, key: Key) -> &Switch {
        &self.switches[key as usize]
    }

    fn there(&self, key: Key) -> bool {
        self.get(key).there
    }

    fn string(&self, key: Key) -> Option<&str> {
        let switch = self.get(key);
        if switch.there {
            switch.strings.first().map(String::as_str)
        } else {
            None
        }
    }

    fn target(&self, key: Key) -> Option<Target> {
        let switch = self.get(key);
        if !switch.there {
            return None;
        }
        Some(match switch.char_index {
            Some(1) => Target::Out,
            Some(2) => Target::Err,
            _ => Target::Off,
        })
    }

    /// Where messages go: standard output, nowhere with `-so`, or as `-bso` says.
    pub(super) fn messages_target(&self) -> Target {
        let default = if self.there(Key::StdOut) {
            Target::Off
        } else {
            Target::Out
        };
        self.target(Key::OutStream).unwrap_or(default)
    }

    /// Where errors go: standard error, or as `-bse` says.
    pub(super) fn errors_target(&self) -> Target {
        self.target(Key::ErrStream).unwrap_or(Target::Err)
    }
}

/// `CParser::ParseStrings`: the switches by their longest name, case aside.
pub(super) fn parse_switches(args: &[String]) -> Result<Parsed, CmdLineError> {
    let mut switches = vec![Switch::default(); FORMS.len()];
    let mut words = Vec::new();
    let mut stop_index = None;
    for arg in args {
        if stop_index.is_none() {
            if arg == "--" {
                stop_index = Some(words.len());
                continue;
            }
            if arg.starts_with('-') {
                parse_switch(arg, &mut switches)
                    .map_err(|message| CmdLineError::with(message, arg))?;
                continue;
            }
        }
        words.push(arg.clone());
    }
    let mut parsed = Parsed {
        switches,
        words,
        stop_index,
        help: false,
        headers: true,
    };
    parsed.help = parsed.there(Key::Help1) || parsed.there(Key::Help2) || parsed.there(Key::Help3);
    parsed.headers = !parsed.there(Key::DisableHeaders);
    Ok(parsed)
}

fn parse_switch(arg: &str, switches: &mut [Switch]) -> Result<(), &'static str> {
    let rest = arg.get(1..).unwrap_or_default();
    let mut best: Option<usize> = None;
    for (index, form) in FORMS.iter().enumerate() {
        let key = form.0;
        if best.is_some_and(|b| FORMS[b].0.len() >= key.len()) || key.len() > rest.len() {
            continue;
        }
        if rest
            .get(..key.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(key))
        {
            best = Some(index);
        }
    }
    let Some(index) = best else {
        return Err("Unknown switch:");
    };
    let (key, _, kind, multi, min_len) = FORMS[index];
    let switch = &mut switches[index];
    if !multi && switch.there {
        return Err("Multiple instances for switch:");
    }
    switch.there = true;
    let tail = rest.get(key.len()..).unwrap_or_default();
    if tail.chars().count() < min_len {
        return Err("Too short switch:");
    }
    switch.minus = false;
    switch.char_index = None;
    match kind {
        Kind::Minus if tail.chars().count() == 1 => {
            if tail == "-" {
                switch.minus = true;
                return Ok(());
            }
            return Err("Incorrect switch postfix:");
        }
        Kind::Char(set) if tail.chars().count() == 1 => {
            let c = tail.chars().next().unwrap_or('\0');
            if let Some(at) = set.find(c) {
                switch.char_index = Some(at);
                return Ok(());
            }
            return Err("Incorrect switch postfix:");
        }
        Kind::Str => {
            switch.strings.push(tail.to_owned());
            return Ok(());
        }
        _ => {}
    }
    if tail.is_empty() {
        Ok(())
    } else {
        Err("Too long switch:")
    }
}

/// The second pass's result: what the command is to do.
#[derive(Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "7-Zip's options are switches"
)]
pub(super) struct Options {
    pub(super) command: Command,
    /// The items the command is about.
    pub(super) censor: Censor,
    /// The archives the command reads (its name, `-ai`, `-ax`).
    pub(super) archive_censor: Censor,
    /// The archive's name as given.
    pub(super) archive_name: Option<String>,
    pub(super) password: Option<String>,
    pub(super) archive_type: Option<String>,
    pub(super) output_dir: Option<String>,
    pub(super) overwrite: Overwrite,
    pub(super) stdout: bool,
    /// One of `-bso1`, `-bse1` or `-bsp1` sends messages to standard output.
    pub(super) stdout_shared: bool,
    pub(super) stdin: Option<String>,
    pub(super) headers: bool,
    pub(super) tech: bool,
    pub(super) log_level: u32,
    pub(super) path_keep: PathKeep,
    pub(super) exclude_dirs: bool,
    pub(super) exclude_files: bool,
    /// `-m`'s names and values.
    pub(super) properties: Vec<(String, Option<String>)>,
    /// `-scrc`'s hashes, when it was given (an empty name for CRC32).
    pub(super) hash_methods: Option<Vec<String>>,
    /// The update group's settings.
    pub(super) update: Option<Update>,
}

fn stoi(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// `CArcCmdLineParser::Parse2`: the command, the archive's name, the items, and the
/// switches that need the command to mean something.
#[expect(clippy::too_many_lines, reason = "7-Zip's Parse2, switch by switch")]
pub(super) fn parse_command(parsed: &Parsed) -> Result<Options, CmdLineError> {
    let words = parsed.words.clone();
    let Some(first) = words.first() else {
        return Err(CmdLineError::new("The command must be specified"));
    };
    let command =
        Command::parse(first).ok_or_else(|| CmdLineError::with("Unsupported command:", first))?;

    let log_level = match parsed.string(Key::LogLevel) {
        None => 0,
        Some("") => 1,
        Some(s) => {
            stoi(s).ok_or_else(|| CmdLineError::with("Unsupported switch postfix -bb", s))?
        }
    };

    let recursion = if parsed.there(Key::Recursed) {
        match parsed.get(Key::Recursed).char_index {
            Some(0) => Recursion::WildcardOnly,
            Some(1) => Recursion::None,
            _ => Recursion::Always,
        }
    } else {
        Recursion::None
    };
    let wildcards = !parsed.there(Key::DisableWildcardParsing);
    let mark = match parsed.string(Key::UseSlashMark) {
        None => MarkMode::FileOrDir,
        Some("") => MarkMode::StrictFile,
        Some("-") => MarkMode::FileOrDir,
        Some("2") => MarkMode::StrictFileIfWildcard,
        Some(other) => return Err(CmdLineError::with("Unsupported -spm:", other)),
    };
    let names = NameOption {
        include: true,
        wildcards,
        mark,
        recursion,
    };

    if let Some(charset) = parsed.string(Key::ConsoleCharSet) {
        charset_known(charset, true)?;
    }
    if let Some(charset) = parsed.string(Key::ListfileCharSet) {
        charset_known(charset, false)?;
    }

    // Names are compared without case, as on Windows, unless -ssc.
    let case_sensitive = parsed.there(Key::CaseSensitive) && !parsed.get(Key::CaseSensitive).minus;
    let mut censor = Censor::default();
    censor.case_sensitive = case_sensitive;
    let includes = parsed.get(Key::Include).strings.clone();
    let there_are_includes = parsed.there(Key::Include);
    if there_are_includes {
        add_switch_wildcards(&mut censor, &includes, names)?;
    }
    if parsed.there(Key::Exclude) {
        let excludes = parsed.get(Key::Exclude).strings.clone();
        add_switch_wildcards(
            &mut censor,
            &excludes,
            NameOption {
                include: false,
                ..names
            },
        )?;
    }

    let mut index = 1;
    let mut has_archive_name = !parsed.there(Key::NoArName)
        && !matches!(command, Command::Benchmark | Command::Info | Command::Hash);
    let extract_or_list = command.is_extract_group() || command == Command::List;
    let stdin = parsed.string(Key::StdIn).map(str::to_owned);
    if (extract_or_list || command == Command::Rename) && stdin.is_some() {
        has_archive_name = false;
    }
    let mut archive_name = None;
    if has_archive_name {
        let Some(name) = words.get(index) else {
            return Err(CmdLineError::new("Cannot find archive name"));
        };
        if name.is_empty() {
            return Err(CmdLineError::new("Archive name cannot by empty"));
        }
        archive_name = Some(name.clone());
        index += 1;
    }

    // The names after the archive's: `*` when there are none and no -i; for rn, pairs of
    // old and new names, the censor `*` unless -i.
    let rename = command == Command::Rename;
    if (rename || words.len() == index) && !there_are_includes {
        censor.add_pre_item(true, "*", Recursion::None, true, MarkMode::FileOrDir);
    }
    let stop = parsed.stop_index.unwrap_or(words.len());
    let mut rename_pairs = Vec::new();
    let mut old_name: Option<String> = None;
    for (at, word) in words.iter().enumerate().skip(index) {
        if word.is_empty() {
            return Err(CmdLineError::new("Empty file path"));
        }
        if at < stop
            && let Some(list) = word.strip_prefix('@')
        {
            let list_names = read_list_file(list)?;
            if rename {
                if list_names.len() % 2 != 0 {
                    return Err(CmdLineError::with(
                        "Incorrect item in listfile.\nCheck charset encoding and -scs switch.",
                        list,
                    ));
                }
                for pair in list_names.chunks(2) {
                    rename_pairs.push(rename_pair(&pair[0], &pair[1], wildcards)?);
                }
                continue;
            }
            for name in list_names {
                censor.add_pre_item(true, &name, names.recursion, names.wildcards, names.mark);
            }
        } else if rename {
            match old_name.take() {
                None => old_name = Some(word.clone()),
                Some(old) => rename_pairs.push(rename_pair(&old, word, wildcards)?),
            }
        } else {
            censor.add_pre_item(true, word, names.recursion, names.wildcards, names.mark);
        }
    }
    if let Some(old) = old_name {
        return Err(CmdLineError::with(
            "There is no second file name for rename pair:",
            &old,
        ));
    }

    let mut path_keep = if matches!(command, Command::Extract) {
        PathKeep::None
    } else {
        PathKeep::Full
    };
    match parsed.string(Key::FullPathMode) {
        None => {}
        Some("") => path_keep = PathKeep::Absolute,
        Some("2") => path_keep = PathKeep::Full,
        Some(other) => return Err(CmdLineError::with("Unsupported -spf:", other)),
    }

    let mut overwrite = Overwrite::Ask;
    let stdout = parsed.there(Key::StdOut);
    let mut archive_censor = Censor::default();
    archive_censor.case_sensitive = case_sensitive;
    if extract_or_list {
        censor.add_paths(PathMode::Absolute);
        censor.extend_exclude();
        let arc_names = NameOption {
            include: true,
            wildcards,
            mark,
            recursion: Recursion::None,
        };
        if parsed.there(Key::ArInclude) {
            let list = parsed.get(Key::ArInclude).strings.clone();
            add_switch_wildcards(&mut archive_censor, &list, arc_names)?;
        }
        if parsed.there(Key::ArExclude) {
            let list = parsed.get(Key::ArExclude).strings.clone();
            add_switch_wildcards(
                &mut archive_censor,
                &list,
                NameOption {
                    include: false,
                    ..arc_names
                },
            )?;
        }
        if let Some(name) = &archive_name {
            archive_censor.add_pre_item(true, name, Recursion::None, wildcards, mark);
        }
        archive_censor.add_paths(PathMode::Relative);
        archive_censor.extend_exclude();
        if command.is_extract_group() {
            if let Some(index) = parsed.get(Key::Overwrite).char_index {
                overwrite = [
                    Overwrite::All,
                    Overwrite::Skip,
                    Overwrite::Rename,
                    Overwrite::RenameExisting,
                ][index.min(3)];
            } else if parsed.there(Key::Yes) {
                overwrite = Overwrite::All;
            }
        }
    } else if command.is_update_group() && parsed.there(Key::ArInclude) {
        return Err(CmdLineError::new(
            "-ai switch is not supported for this command",
        ));
    } else {
        censor.add_paths(PathMode::Relative);
        censor.extend_exclude();
    }

    let name_mode = match parsed.get(Key::ArcNameMode).char_index {
        Some(1) => NameMode::Exact,
        Some(2) => NameMode::Add,
        _ => NameMode::Smart,
    };
    let update = if command.is_update_group() {
        Some(update_options(parsed, command, rename_pairs, name_mode)?)
    } else {
        None
    };
    let properties = parsed
        .get(Key::Property)
        .strings
        .iter()
        .map(|s| match s.split_once('=') {
            Some((name, value)) => (name.to_owned(), Some(value.to_owned())),
            // A name ending in `-` or `+` is off or on (`SetProperties`): `-mtm-`.
            None => match s.strip_suffix(['-', '+']) {
                Some(name) => (
                    name.to_owned(),
                    Some(s.get(name.len()..).unwrap_or_default().to_owned()),
                ),
                None => (s.clone(), None),
            },
        })
        .collect();

    Ok(Options {
        command,
        exclude_dirs: censor.exclude_dirs,
        exclude_files: censor.exclude_files,
        censor,
        archive_censor,
        archive_name,
        password: parsed.string(Key::Password).map(str::to_owned),
        archive_type: parsed.string(Key::ArchiveType).map(str::to_owned),
        output_dir: parsed.string(Key::OutputDir).map(str::to_owned),
        overwrite,
        stdout,
        stdout_shared: [Key::OutStream, Key::ErrStream, Key::PercentStream]
            .into_iter()
            .any(|key| parsed.target(key) == Some(Target::Out)),
        stdin,
        headers: parsed.headers,
        tech: parsed.there(Key::TechMode),
        log_level,
        path_keep,
        properties,
        hash_methods: parsed
            .there(Key::Hash)
            .then(|| parsed.get(Key::Hash).strings.clone()),
        update,
    })
}

/// What 7-Zip does with an item in a state of an update (`NPairAction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Ignore,
    Copy,
    Compress,
    CompressAsAnti,
}

/// An action for each state (`CActionSet`), in 7-Zip's order `p q r x y z w`: an item
/// the names leave out, one only in the archive, one only on disk, one newer in the
/// archive, one older there, the same, and one of the same time and another size.
pub(super) type ActionSet = [Action; 7];

const ADD: ActionSet = [
    Action::Copy,
    Action::Copy,
    Action::Compress,
    Action::Compress,
    Action::Compress,
    Action::Compress,
    Action::Compress,
];

const UPDATE: ActionSet = [
    Action::Copy,
    Action::Copy,
    Action::Compress,
    Action::Copy,
    Action::Compress,
    Action::Copy,
    Action::Compress,
];

const DELETE: ActionSet = [
    Action::Copy,
    Action::Ignore,
    Action::Ignore,
    Action::Ignore,
    Action::Ignore,
    Action::Ignore,
    Action::Ignore,
];

/// How the archive's name gets its extension (`-sa`): added when it has none, never,
/// or always.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NameMode {
    Smart,
    Exact,
    Add,
}

/// What `a`, `u`, `d` and `rn` are to do beyond the names.
#[derive(Debug)]
pub(super) struct Update {
    /// The actions on the archive named, unless `-u-` said to leave it.
    pub(super) itself: Option<ActionSet>,
    /// The other archives `-u…!name` writes, each with its actions.
    pub(super) others: Vec<(String, ActionSet)>,
    /// `-w`: where the new archive is written before it replaces the old one; empty
    /// for the system's temporary folder.
    pub(super) working_dir: Option<String>,
    /// `-v`: the sizes of the volumes.
    pub(super) volumes: Vec<u64>,
    /// `-sfx`.
    pub(super) sfx: bool,
    /// `-sdel`: the files taken in are deleted afterwards.
    pub(super) delete_after: bool,
    /// `-stl`: the archive gets the newest time of its items.
    pub(super) set_arc_mtime: bool,
    pub(super) name_mode: NameMode,
    /// `rn`'s old and new names.
    pub(super) rename_pairs: Vec<(String, String)>,
}

/// `SetAddCommandOptions` and the update group's part of `Parse2`.
fn update_options(
    parsed: &Parsed,
    command: Command,
    rename_pairs: Vec<(String, String)>,
    name_mode: NameMode,
) -> Result<Update, CmdLineError> {
    let default = match command {
        Command::Add => ADD,
        Command::Delete => DELETE,
        _ => UPDATE,
    };
    let mut itself = Some(default);
    let mut others = Vec::new();
    if parsed.there(Key::Update) {
        for text in &parsed.get(Key::Update).strings {
            if text == "-" {
                itself = None;
                continue;
            }
            let mut actions = default;
            let error = || CmdLineError::with("incorrect update switch command", text);
            let rest = parse_actions(text, &mut actions).ok_or_else(error)?;
            if rest.is_empty() {
                if itself.is_some() {
                    itself = Some(actions);
                }
                continue;
            }
            match rest.strip_prefix('!') {
                Some(name) if !name.is_empty() => others.push((name.to_owned(), actions)),
                _ => return Err(error()),
            }
        }
    }
    let working_dir = parsed.string(Key::WorkingDir).map(str::to_owned);
    let mut volumes = Vec::new();
    if parsed.there(Key::Volume) {
        let list = &parsed.get(Key::Volume).strings;
        for (at, text) in list.iter().enumerate() {
            let size = complex_size(text)
                .ok_or_else(|| CmdLineError::with("Incorrect volume size:", text))?;
            if at + 1 == list.len() && size == 0 {
                return Err(CmdLineError::new("zero size last volume is not allowed"));
            }
            volumes.push(size);
        }
    }
    if command == Command::Rename && usize::from(itself.is_some()) + others.len() != 1 {
        return Err(CmdLineError::new(
            "Only one archive can be created with rename command",
        ));
    }
    Ok(Update {
        itself,
        others,
        working_dir,
        volumes,
        sfx: parsed.there(Key::Sfx),
        delete_after: parsed.there(Key::DeleteAfterCompressing),
        set_arc_mtime: parsed.there(Key::SetArcMTime),
        name_mode,
        rename_pairs,
    })
}

/// `ParseUpdateCommandString2`: a state letter and an action digit, again and again;
/// what follows them, or `None` for a pair 7-Zip refuses.
fn parse_actions<'a>(text: &'a str, actions: &mut ActionSet) -> Option<&'a str> {
    const STATES: &str = "pqrxyzw";
    // The action a state cannot take: copying what is only on disk, compressing what is
    // only in the archive or left out.
    const REFUSED: [Option<usize>; 7] = [Some(2), Some(2), Some(1), None, None, None, None];
    let mut rest = text;
    loop {
        let mut chars = rest.chars();
        let Some(c) = chars.next() else {
            return Some(rest);
        };
        let Some(state) = STATES.find(c.to_ascii_lowercase()).filter(|_| c.is_ascii()) else {
            return Some(rest);
        };
        let digit = chars.next()?.to_digit(10)? as usize;
        if digit >= 4 || REFUSED[state] == Some(digit) {
            return None;
        }
        actions[state] = [
            Action::Ignore,
            Action::Copy,
            Action::Compress,
            Action::CompressAsAnti,
        ][digit];
        rest = chars.as_str();
    }
}

/// `ParseComplexSize`: a number, then nothing or one of `b`, `k`, `m`, `g`, `t`.
fn complex_size(text: &str) -> Option<u64> {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let number: u64 = text.get(..digits)?.parse().ok()?;
    let bits = match text.get(digits..)?.to_ascii_lowercase().as_str() {
        "" | "b" => 0,
        "k" => 10,
        "m" => 20,
        "g" => 30,
        "t" => 40,
        _ => return None,
    };
    (bits == 0 || number < 1 << (64 - bits)).then(|| number << bits)
}

/// `AddRenamePair`: an old name without wildcards and its new name.
fn rename_pair(old: &str, new: &str, wildcards: bool) -> Result<(String, String), CmdLineError> {
    if wildcards && super::censor::has_wildcard(old) {
        return Err(CmdLineError::with(
            "Unsupported rename command:",
            &format!("{old}\n{new}\n"),
        ));
    }
    Ok((old.to_owned(), new.to_owned()))
}

fn charset_known(name: &str, bytes_only: bool) -> Result<(), CmdLineError> {
    if stoi(name).is_some_and(|v| v < 1 << 16) {
        return Ok(());
    }
    let lower = name.to_ascii_lowercase();
    let known: &[&str] = if bytes_only {
        &["utf-8", "win", "dos"]
    } else {
        &["utf-8", "win", "dos", "utf-16le", "utf-16be"]
    };
    if known.contains(&lower.as_str()) {
        Ok(())
    } else {
        Err(CmdLineError::with("Unsupported charset:", &lower))
    }
}

#[derive(Clone, Copy)]
struct NameOption {
    include: bool,
    wildcards: bool,
    mark: MarkMode,
    recursion: Recursion,
}

/// `AddSwitchWildcardsToCensor`: `-i` and `-x` with their `r`, `w` and `m` flags, then `!`
/// and a name or `@` and a list file.
fn add_switch_wildcards(
    censor: &mut Censor,
    strings: &[String],
    names: NameOption,
) -> Result<(), CmdLineError> {
    for name in strings {
        let chars: Vec<char> = name.chars().collect();
        if chars.len() < 2 {
            return Err(CmdLineError::with("Too short switch", name));
        }
        if !names.include {
            if name.eq_ignore_ascii_case("td") {
                censor.exclude_dirs = true;
                continue;
            }
            if name.eq_ignore_ascii_case("tf") {
                censor.exclude_files = true;
                continue;
            }
        }
        let mut option = names;
        let (mut recursed_used, mut matching_used, mut type_used) = (false, false, false);
        let mut pos = 0;
        let error = loop {
            let c = chars.get(pos).copied().unwrap_or('\0').to_ascii_lowercase();
            if c == 'r' {
                if recursed_used {
                    break true;
                }
                recursed_used = true;
                pos += 1;
                let next = chars.get(pos).copied().unwrap_or('\0');
                option.recursion = match next {
                    '0' => Recursion::WildcardOnly,
                    '-' => Recursion::None,
                    _ => Recursion::Always,
                };
                if matches!(next, '0' | '-') {
                    pos += 1;
                }
                continue;
            }
            if c == 'w' {
                if matching_used {
                    break true;
                }
                matching_used = true;
                option.wildcards = true;
                pos += 1;
                if chars.get(pos) == Some(&'-') {
                    option.wildcards = false;
                    pos += 1;
                }
            } else if c == 'm' {
                if type_used {
                    break true;
                }
                type_used = true;
                pos += 1;
                option.mark = MarkMode::StrictFile;
                match chars.get(pos) {
                    Some('-') => {
                        option.mark = MarkMode::FileOrDir;
                        pos += 1;
                    }
                    Some('2') => {
                        option.mark = MarkMode::StrictFileIfWildcard;
                        pos += 1;
                    }
                    _ => {}
                }
            } else {
                break false;
            }
        };
        if error {
            return Err(CmdLineError::with("inorrect switch", name));
        }
        if chars.len() < pos + 2 {
            return Err(CmdLineError::with("Too short switch", name));
        }
        let tail: String = chars[pos + 1..].iter().collect();
        match chars[pos] {
            '!' => censor.add_pre_item(
                option.include,
                &tail,
                option.recursion,
                option.wildcards,
                option.mark,
            ),
            '@' => {
                for item in read_list_file(&tail)? {
                    censor.add_pre_item(
                        option.include,
                        &item,
                        option.recursion,
                        option.wildcards,
                        option.mark,
                    );
                }
            }
            _ => return Err(CmdLineError::with("Incorrect wildcard type marker", name)),
        }
    }
    Ok(())
}

/// A list file's names, one to a line, UTF-8 unless `-scs` said otherwise.
fn read_list_file(path: &str) -> Result<Vec<String>, CmdLineError> {
    let bytes = std::fs::read(path).map_err(|e| {
        CmdLineError::with(
            &format!(
                "The file operation error for listfile\n{}",
                super::text::system_message(&e)
            ),
            path,
        )
    })?;
    let text = String::from_utf8(bytes).map_err(|_| {
        CmdLineError::with(
            "Incorrect item in listfile.\nCheck charset encoding and -scs switch.",
            path,
        )
    })?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    Ok(text
        .lines()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_longest_switch_name_wins_case_aside() {
        let parsed = parse_switches(&args(&["l", "-SLT", "-bso0", "-x!a"])).unwrap();
        assert!(parsed.there(Key::TechMode));
        assert_eq!(parsed.messages_target(), Target::Off);
        assert_eq!(parsed.get(Key::Exclude).strings, ["!a"]);
    }

    #[test]
    fn seven_zips_words_for_bad_switches() {
        let error = parse_switches(&args(&["l", "-qq"])).err().unwrap();
        assert_eq!(error.message, "Unknown switch:");
        assert_eq!(error.line.as_deref(), Some("-qq"));
        let error = parse_switches(&args(&["l", "-yy"])).err().unwrap();
        assert_eq!(error.message, "Too long switch:");
        let error = parse_switches(&args(&["l", "-o"])).err().unwrap();
        assert_eq!(error.message, "Too short switch:");
        let error = parse_switches(&args(&["l", "-oa", "-ob"])).err().unwrap();
        assert_eq!(error.message, "Multiple instances for switch:");
        let error = parse_switches(&args(&["l", "-aox"])).err().unwrap();
        assert_eq!(error.message, "Incorrect switch postfix:");
    }

    #[test]
    fn the_command_and_the_archive() {
        let parsed = parse_switches(&args(&["X", "a.7z", "-y"])).unwrap();
        let options = parse_command(&parsed).unwrap();
        assert_eq!(options.command, Command::ExtractFull);
        assert_eq!(options.overwrite, Overwrite::All);
        let parsed = parse_switches(&args(&["l"])).unwrap();
        let error = parse_command(&parsed).err().unwrap();
        assert_eq!(error.message, "Cannot find archive name");
        let parsed = parse_switches(&args(&["q", "a.7z"])).unwrap();
        let error = parse_command(&parsed).err().unwrap();
        assert_eq!(error.message, "Unsupported command:");
    }
}
