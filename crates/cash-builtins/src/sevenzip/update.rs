//! `a`, `u`, `d` and `rn`: 7-Zip's update (`Update.cpp`, `UpdatePair.cpp`,
//! `UpdateProduce.cpp`, the 7z handler's `UpdateItems` and `7zUpdate.cpp`), with
//! `UpdateCallbackConsole`'s messages and `Main`'s `WarningsCheck`.
//!
//! The archive there is opened, the disk scanned, each name on disk paired with the
//! archive's item of the same name, and each pair's state given the action the
//! command's set says: copied, compressed anew, or left out. The new archive is
//! written beside the old one, then takes its place: items without data first, then
//! for each filter group the old blocks (copied as they are when all their items stay,
//! else decoded and compressed again) and the new files in solid blocks.

use std::cmp::Ordering;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use cash_archive::sevenz::{self, ArchiveEntry, ArchiveWriter, NtTime, TimesKept};

use super::archive::{Kind, OpenFailure, Opened};
use super::censor::compare_file_names;
use super::cmdline::{Action, ActionSet, CmdLineError, Command, NameMode, Options};
use super::extract::{Found, archive_props, ask_password, forced_kind, open_asking, open_error};
use super::methods::{Filter, MethodError, Settings};
use super::scan::{self, DirItem, Stat};
use super::{Console, Env, Stop, text};

/// Windows' codes for the errors 7-Zip's update reports.
mod win {
    /// `ERROR_INVALID_FUNCTION`: what 7-Zip shows for `S_FALSE`.
    pub(super) const INVALID_FUNCTION: u32 = 1;
    pub(super) const ACCESS_DENIED: u32 = 5;
    pub(super) const FILE_EXISTS: u32 = 80;
    pub(super) const E_NOTIMPL: u32 = 0x8000_4001;
    pub(super) const E_FAIL: u32 = 0x8000_4005;
    pub(super) const E_INVALIDARG: u32 = 0x8007_0057;
}

#[expect(
    clippy::cast_possible_wrap,
    reason = "HRESULTs are Windows' codes as they are"
)]
fn win_error(code: u32) -> io::Error {
    io::Error::from_raw_os_error(code as i32)
}

impl From<sevenz::Error> for Stop {
    fn from(error: sevenz::Error) -> Self {
        match error {
            sevenz::Error::Io(e, _) => Self::from(e),
            _ => Self::System(win_error(win::E_FAIL)),
        }
    }
}

impl From<MethodError> for Stop {
    fn from(error: MethodError) -> Self {
        Self::System(win_error(match error {
            MethodError::Invalid => win::E_INVALIDARG,
            MethodError::NotImplemented => win::E_NOTIMPL,
        }))
    }
}

/// 7-Zip's `CUpdateErrorInfo`: a message, the files it is about, and the system's
/// words, printed by `WarningsCheck` before the error stops the command.
struct ErrorInfo {
    message: String,
    files: Vec<String>,
    code: u32,
}

/// A pair's state (`NPairState`), in the order of an action set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    NotMasked,
    OnlyInArchive,
    OnlyOnDisk,
    NewInArchive,
    OldInArchive,
    SameFiles,
    UnknownNewer,
}

/// A name on disk and the archive's item of that name, either of them perhaps alone.
struct Pair {
    state: State,
    dir: Option<usize>,
    arc: Option<usize>,
}

/// An item of the archive there, as updating sees it (`CArcItem`).
struct ArcItem {
    name: String,
    is_dir: bool,
    size: u64,
    modified: Option<u64>,
    censored: bool,
}

/// What becomes of a pair (`CUpdatePair2`).
#[derive(Clone, Debug, Default)]
struct Up {
    dir: Option<usize>,
    arc: Option<usize>,
    new_data: bool,
    new_props: bool,
    use_arc_props: bool,
    is_anti: bool,
    new_name: Option<String>,
}

/// An item of the new archive, as the 7z handler takes it (`CUpdateItem`).
#[derive(Clone, Debug, Default)]
struct Item {
    up: Up,
    name: String,
    is_dir: bool,
    is_anti: bool,
    size: u64,
    attrib: Option<u32>,
    modified: Option<u64>,
    created: Option<u64>,
    accessed: Option<u64>,
}

impl Item {
    const fn has_stream(&self) -> bool {
        !self.is_dir && !self.is_anti && self.size != 0
    }

    fn entry(&self) -> ArchiveEntry {
        let time = |t: Option<u64>| NtTime::from(t.unwrap_or(0));
        ArchiveEntry {
            name: self.name.clone(),
            has_stream: self.has_stream(),
            is_directory: self.is_dir,
            is_anti_item: self.is_anti,
            has_creation_date: self.created.is_some(),
            has_last_modified_date: self.modified.is_some(),
            has_access_date: self.accessed.is_some(),
            creation_date: time(self.created),
            last_modified_date: time(self.modified),
            access_date: time(self.accessed),
            has_windows_attributes: self.attrib.is_some(),
            windows_attributes: self.attrib.unwrap_or(0),
            size: self.size,
            ..ArchiveEntry::default()
        }
    }

    /// An item without data, as the archive will hold it: the old one's entry when
    /// nothing about it changed.
    fn empty_entry(&self, files: &[ArchiveEntry]) -> ArchiveEntry {
        if !self.up.new_props
            && let Some(f) = self.up.arc.and_then(|a| files.get(a))
        {
            return f.clone();
        }
        self.entry()
    }
}

/// Counts of folders, files and bytes, and of anti-items (`CDirItemsStat2`).
#[derive(Default, Clone, Copy)]
struct Stat2 {
    dirs: u64,
    files: u64,
    size: u64,
    anti_dirs: u64,
    anti_files: u64,
}

impl Stat2 {
    const fn is_empty(&self) -> bool {
        self.dirs == 0 && self.files == 0 && self.anti_dirs == 0 && self.anti_files == 0
    }

    const fn add(&mut self, is_dir: bool, anti: bool, size: u64) {
        match (is_dir, anti) {
            (true, false) => self.dirs += 1,
            (true, true) => self.anti_dirs += 1,
            (false, false) => {
                self.files += 1;
                self.size += size;
            }
            (false, true) => self.anti_files += 1,
        }
    }

    /// `Print_DirItemsStat2`.
    fn text(&self) -> String {
        let mut s = stat_text(self.dirs, self.files, self.size);
        let mut first = true;
        for (n, one, many) in [
            (self.anti_dirs, "anti-folder", "anti-folders"),
            (self.anti_files, "anti-file", "anti-files"),
        ] {
            if n != 0 {
                s.push_str(if first { "\n" } else { ", " });
                first = false;
                s.push_str(&text::count(n, one, many));
            }
        }
        s
    }
}

/// `Print_DirItemsStat`: "N folders, N files, N bytes (N KiB)".
fn stat_text(dirs: u64, files: u64, size: u64) -> String {
    let mut s = String::new();
    if dirs != 0 {
        s.push_str(&text::count(dirs, "folder", "folders"));
        s.push_str(", ");
    }
    s.push_str(&text::count(files, "file", "files"));
    s.push_str(", ");
    s.push_str(&text::size_smart(size));
    s
}

/// What the run reports at its end: the paths the scan could not read and the files
/// that would not open, each with why.
#[derive(Default)]
struct Warnings {
    scan: Vec<(String, io::Error)>,
    failed: Vec<(String, io::Error)>,
}

/// `CCallbackConsoleBase::CommonError`: the messages so far, then the warning or error,
/// its words and the path.
fn common_error<SE: cash_core::ShellExtensions>(
    console: &Console<'_, SE>,
    path: &str,
    error: &io::Error,
    warning: bool,
) {
    console.flush_so();
    console.se(&format!(
        "\n{}{}\n{path}\n\n",
        if warning { "WARNING: " } else { "ERROR: " },
        text::system_message(error)
    ));
    console.flush_se();
}

/// A per-item line for `-bb`: `+`, `U`, `=`, `R`, `.`, `A`, `D` and the name.
fn progress<SE: cash_core::ShellExtensions>(
    console: &Console<'_, SE>,
    options: &Options,
    level: u32,
    mark: &str,
    name: &str,
) {
    if options.log_level >= level {
        console.so(&format!("{mark} {name}\n"));
    }
}

/// The archive's name with its extension as `-sa` has it (`CArchivePath`).
fn final_name(given: &str, mode: NameMode, ext: &str) -> String {
    let split = given.rfind(['/', '\\']).map_or(0, |at| at + 1);
    let (prefix, name) = given.split_at(split);
    match mode {
        NameMode::Add => format!("{given}.{ext}"),
        NameMode::Exact => given.to_owned(),
        NameMode::Smart => match name.rfind('.') {
            None => format!("{given}.{ext}"),
            Some(dot) if dot + 1 == name.len() => {
                format!("{prefix}{}", name.get(..dot).unwrap_or_default())
            }
            Some(_) => given.to_owned(),
        },
    }
}

/// `CUpdatePair`'s list: the scan's items and the archive's, each sorted by name, then
/// merged (`GetUpdatePairInfoList`).
fn update_pairs(
    dir_items: &[DirItem],
    arc_items: &[ArcItem],
    case: bool,
) -> Result<Vec<Pair>, Stop> {
    let mut arc_order: Vec<usize> = (0..arc_items.len()).collect();
    let arc_cmp = |a: &ArcItem, b: &ArcItem| {
        compare_file_names(&a.name, &b.name, case).then(b.is_dir.cmp(&a.is_dir))
    };
    arc_order.sort_by(|&a, &b| arc_cmp(&arc_items[a], &arc_items[b]).then(a.cmp(&b)));
    let mut dir_order: Vec<usize> = (0..dir_items.len()).collect();
    dir_order.sort_by(|&a, &b| {
        compare_file_names(&dir_items[a].name, &dir_items[b].name, case).then(a.cmp(&b))
    });
    for w in dir_order.windows(2) {
        let (a, b) = (&dir_items[w[0]].name, &dir_items[w[1]].name);
        if compare_file_names(a, b, case) == Ordering::Equal {
            return Err(Stop::Message(format!(
                "Duplicate filename on disk:\n{a}\n{b}"
            )));
        }
    }
    let duplicate = |at: usize| {
        let ai = &arc_items[arc_order[at]];
        [at.checked_sub(1), Some(at + 1)]
            .into_iter()
            .flatten()
            .filter_map(|other| arc_order.get(other))
            .find(|&&other| arc_cmp(ai, &arc_items[other]) == Ordering::Equal)
            .copied()
    };
    let mut pairs = Vec::new();
    let (mut d, mut a) = (0, 0);
    while d < dir_order.len() || a < arc_order.len() {
        let di = dir_order.get(d).map(|&i| (i, &dir_items[i]));
        let ai = arc_order.get(a).map(|&i| (i, &arc_items[i]));
        let order = match (di, ai) {
            (Some(_), None) => Ordering::Less,
            (None, _) => Ordering::Greater,
            (Some((_, di)), Some((_, ai))) => match compare_file_names(&di.name, &ai.name, case) {
                Ordering::Equal if di.is_dir != ai.is_dir => {
                    if ai.is_dir {
                        Ordering::Greater
                    } else {
                        Ordering::Less
                    }
                }
                other => other,
            },
        };
        match (order, di, ai) {
            (Ordering::Less, Some((i, _)), _) => {
                pairs.push(Pair {
                    state: State::OnlyOnDisk,
                    dir: Some(i),
                    arc: None,
                });
                d += 1;
            }
            (Ordering::Greater, _, Some((i, ai))) => {
                pairs.push(Pair {
                    state: if ai.censored {
                        State::OnlyInArchive
                    } else {
                        State::NotMasked
                    },
                    dir: None,
                    arc: Some(i),
                });
                a += 1;
            }
            (_, Some((di_index, di)), Some((ai_index, ai))) => {
                if let Some(other) = duplicate(a) {
                    return Err(Stop::Message(format!(
                        "Duplicate filename in archive:\n{}\n{}",
                        ai.name, arc_items[other].name
                    )));
                }
                if !ai.censored {
                    return Err(Stop::Message(format!(
                        "Internal file name collision (file on disk, file in archive):\n{}\n{}",
                        di.name, ai.name
                    )));
                }
                let state = match ai.modified.map(|m| di.modified.cmp(&m)) {
                    Some(Ordering::Less) => State::NewInArchive,
                    Some(Ordering::Greater) => State::OldInArchive,
                    _ if di.size == ai.size => State::SameFiles,
                    _ => State::UnknownNewer,
                };
                pairs.push(Pair {
                    state,
                    dir: Some(di_index),
                    arc: Some(ai_index),
                });
                d += 1;
                a += 1;
            }
            _ => break,
        }
    }
    Ok(pairs)
}

/// `UpdateProduce`: each pair's action; what is left out of an archive updated in
/// place is counted in `deleted`.
fn produce(
    pairs: &[Pair],
    actions: ActionSet,
    arc_items: &[ArcItem],
    mut deleted: Option<&mut Stat2>,
) -> Result<Vec<Up>, Stop> {
    let collision = || Stop::Message("Internal collision in update action set".to_owned());
    let mut ups = Vec::new();
    for pair in pairs {
        let mut up = Up {
            dir: pair.dir,
            arc: pair.arc,
            new_data: true,
            new_props: true,
            ..Up::default()
        };
        match actions[pair.state as usize] {
            Action::Ignore => {
                if let (Some(arc), Some(stat)) = (pair.arc, deleted.as_deref_mut()) {
                    let ai = &arc_items[arc];
                    stat.add(ai.is_dir, false, ai.size);
                }
                continue;
            }
            Action::Copy => {
                if pair.state == State::OnlyOnDisk {
                    return Err(collision());
                }
                up.new_data = false;
                up.new_props = false;
                up.use_arc_props = true;
            }
            Action::Compress => {
                if matches!(pair.state, State::OnlyInArchive | State::NotMasked) {
                    return Err(collision());
                }
            }
            Action::CompressAsAnti => {
                up.is_anti = true;
                up.use_arc_props = pair.arc.is_some();
            }
        }
        ups.push(up);
    }
    Ok(ups)
}

/// `CRenamePair::GetNewPath`: the new name `old` → `new` gives `name`, if it speaks of
/// it: the item itself, or what is under the folder it names.
fn renamed(old: &str, new: &str, name: &str, is_dir: bool, case: bool) -> Option<String> {
    let old_chars: Vec<char> = old.chars().collect();
    let name_chars: Vec<char> = name.chars().collect();
    let sep = |c: char| c == '/' || c == '\\';
    let same = |a: char, b: char| {
        a == b || (!case && a.to_uppercase().eq(b.to_uppercase())) || (sep(a) && sep(b))
    };
    let mut num = 0;
    while num < old_chars.len() && num < name_chars.len() && same(old_chars[num], name_chars[num]) {
        num += 1;
    }
    if num == old_chars.len() {
        if num < name_chars.len() && !sep(name_chars[num]) && num != 0 && !sep(name_chars[num - 1])
        {
            return None;
        }
    } else if !is_dir
        || num != name_chars.len()
        || !sep(old_chars[num])
        || num + 1 != old_chars.len()
    {
        return None;
    }
    let rest: String = name_chars[num..].iter().collect();
    Some(format!("{new}{rest}").replace('\\', "/"))
}

/// `NeedScanning`: whether some action takes from the disk.
fn need_scanning(actions: ActionSet) -> bool {
    actions.contains(&Action::Compress) || actions[1..].iter().any(|a| *a != Action::Ignore)
}

/// The archive being updated, open.
struct Source {
    opened: Opened,
    /// The archive's own time, for items without one.
    mtime: Option<u64>,
    /// The password its header was opened with, when it is encrypted.
    header_password: Option<String>,
}

#[expect(clippy::too_many_lines, reason = "7-Zip's UpdateArchive, step by step")]
pub(super) fn run<SE: cash_core::ShellExtensions>(
    options: &Options,
    env: &Env<'_, SE>,
    console: &Console<'_, SE>,
) -> Result<u8, Stop> {
    let Some(update) = &options.update else {
        return Ok(0);
    };
    if options.stdout {
        if console.terminal[0] {
            return Err(Stop::CommandLine(CmdLineError::new(
                "I won't write compressed data to a terminal",
            )));
        }
        if options.stdout_shared {
            return Err(Stop::CommandLine(CmdLineError::new(
                "I won't write data and program's messages to same stream",
            )));
        }
    }
    let given = options.archive_name.clone().unwrap_or_default();
    let kind = forced_kind(options).or_else(|| {
        (update.name_mode != NameMode::Add)
            .then(|| Kind::by_extension(Path::new(&given)))
            .flatten()
    });
    if kind.is_some_and(|k| k != Kind::SevenZ) || update.sfx {
        return Err(Stop::System(win_error(win::E_NOTIMPL)));
    }
    let arc_name = final_name(&given, update.name_mode, "7z");
    let arc_path = env.path(&arc_name);
    let rename = options.command == Command::Rename;
    let mut warnings = Warnings::default();

    let source = open_source(options, env, console, &arc_name, &arc_path, rename)?;
    let source = match source {
        Ok(source) => source,
        Err(info) => return Err(report_error(console, &warnings, &info)),
    };

    let commands = commands(options, update.itself, &update.others);
    let mut dir_items = Vec::new();
    if !rename
        && commands
            .iter()
            .any(|(_, actions, _)| need_scanning(*actions))
    {
        console.so("Scanning the drive:\n");
        dir_items = scan::scan(&options.censor, &|p| env.path(p), &mut |path, error| {
            common_error(console, path, error, true);
            warnings.scan.push((path.to_owned(), copy_error(error)));
        });
        let stat = Stat::of(&dir_items);
        console.so(&format!(
            "{}\n\n",
            stat_text(stat.dirs, stat.files, stat.size)
        ));
    }

    let create_temp = update.itself.is_some()
        && !options.stdout
        && (source.is_some() || update.working_dir.is_some())
        && update.volumes.is_empty();
    for (at, (name, _, _)) in commands.iter().enumerate() {
        if !options.stdout && (at > 0 || !create_temp) && env.path(name).exists() {
            let info = ErrorInfo {
                message: "The file already exists".to_owned(),
                files: vec![name.clone()],
                code: win::FILE_EXISTS,
            };
            return Err(report_error(console, &warnings, &info));
        }
    }

    let case = options.censor.case_sensitive;
    let mut source = source;
    let arc_items: Vec<ArcItem> = source.as_ref().map_or_else(Vec::new, |s| {
        s.opened
            .items
            .iter()
            .map(|item| ArcItem {
                name: item.path.clone(),
                is_dir: item.is_dir,
                size: item.size.unwrap_or(0),
                modified: item.modified.or(s.mtime),
                censored: options.censor.takes(&item.path, item.is_dir),
            })
            .collect()
    });
    let pairs = if rename {
        Vec::new()
    } else {
        update_pairs(&dir_items, &arc_items, case)?
    };

    let mut processed = vec![false; dir_items.len()];
    let mut temp_path = None;
    for (at, (name, actions, itself)) in commands.iter().enumerate() {
        let updating = at == 0 && *itself && source.is_some();
        console.so(&format!(
            "{}{name}\n\n",
            if updating {
                "Updating archive: "
            } else {
                "Creating archive: "
            }
        ));
        let out_path = if at == 0 && create_temp {
            let path = temp_name(env, update.working_dir.as_deref(), &arc_path)?;
            temp_path = Some(path.clone());
            path
        } else {
            env.path(name)
        };
        let job = Job {
            options,
            env,
            console,
            dir_items: &dir_items,
            arc_items: &arc_items,
            pairs: &pairs,
            actions,
            updating,
            rename,
            case,
        };
        let result = job.compress(&mut source, &out_path, &mut warnings, &mut processed);
        match result {
            Ok((files_read, size)) => {
                console.so(&format!(
                    "\nFiles read from disk: {files_read}\nArchive size: {}\n",
                    text::size_smart(size)
                ));
            }
            Err(stop) => {
                let _ = fs::remove_file(&out_path);
                return Err(stop);
            }
        }
        if update.set_arc_mtime {
            set_latest_mtime(&out_path, &dir_items, &arc_items, &pairs);
        }
    }

    drop(source);
    if let Some(temp) = temp_path {
        if arc_path.exists() {
            fs::remove_file(&arc_path).map_err(Stop::System)?;
        }
        if fs::rename(&temp, &arc_path).is_err() {
            fs::copy(&temp, &arc_path).map_err(Stop::System)?;
            let _ = fs::remove_file(&temp);
        }
    }

    if update.delete_after {
        delete_after(options, console, &dir_items, &processed);
    }
    Ok(warnings_check(console, &warnings))
}

/// A copy of an error, for keeping one that is also reported.
fn copy_error(error: &io::Error) -> io::Error {
    error.raw_os_error().map_or_else(
        || io::Error::new(error.kind(), error.to_string()),
        io::Error::from_raw_os_error,
    )
}

/// The archives to write: the one named (unless `-u-`), then `-u…!name`'s.
fn commands(
    options: &Options,
    itself: Option<ActionSet>,
    others: &[(String, ActionSet)],
) -> Vec<(String, ActionSet, bool)> {
    let mode = options
        .update
        .as_ref()
        .map_or(NameMode::Smart, |u| u.name_mode);
    let given = options.archive_name.clone().unwrap_or_default();
    let mut list = Vec::new();
    if let Some(actions) = itself {
        list.push((final_name(&given, mode, "7z"), actions, true));
    }
    for (name, actions) in others {
        list.push((final_name(name, mode, "7z"), *actions, false));
    }
    list
}

/// Opens the archive there, if any: "Open archive:" and its facts, or why it would not
/// open; `Err` holds the errors `WarningsCheck` reports.
fn open_source<SE: cash_core::ShellExtensions>(
    options: &Options,
    env: &Env<'_, SE>,
    console: &Console<'_, SE>,
    arc_name: &str,
    arc_path: &Path,
    rename: bool,
) -> Result<Result<Option<Source>, ErrorInfo>, Stop> {
    let Ok(meta) = fs::metadata(arc_path) else {
        if rename {
            return Err(Stop::Message("can't find archive".to_owned()));
        }
        return Ok(Ok(None));
    };
    let update = options.update.as_ref();
    if meta.is_dir() {
        return Ok(Err(ErrorInfo {
            message: "There is a folder with the name of archive".to_owned(),
            files: vec![arc_name.to_owned()],
            code: win::ACCESS_DENIED,
        }));
    }
    if !options.stdout
        && update.is_some_and(|u| u.itself.is_some())
        && meta.permissions().readonly()
    {
        return Ok(Err(ErrorInfo {
            message: "The file is read-only".to_owned(),
            files: vec![arc_name.to_owned()],
            code: win::ACCESS_DENIED,
        }));
    }
    if update.is_some_and(|u| !u.volumes.is_empty()) {
        return Ok(Err(ErrorInfo {
            message: "Updating for multivolume archives is not implemented".to_owned(),
            files: vec![arc_name.to_owned()],
            code: win::E_NOTIMPL,
        }));
    }
    console.so(&format!("Open archive: {arc_name}\n"));
    let found = Found {
        name: arc_name.to_owned(),
        path: arc_path.to_path_buf(),
        size: meta.len(),
    };
    // The handler takes -m before it opens the archive: a bad one fails the opening.
    if let Err(error) = Settings::parse(&options.properties) {
        let failure = OpenFailure::NotArchive {
            tried: None,
            flags: Vec::new(),
        };
        console.flush_so();
        console.se(&format!(
            "ERROR: {arc_name}\n{}",
            open_error(&found, &failure, false)
        ));
        console.flush_se();
        return Err(Stop::from(error));
    }
    let mut password = options.password.clone().filter(|p| !p.is_empty());
    let mut asked = false;
    let prompt = |t: &str| {
        console.so(t);
        console.flush_so();
    };
    match open_asking(&found, options, env, &mut password, &prompt, &mut asked)? {
        Ok(opened) => {
            console.so(&archive_props(&found, &opened));
            console.so("\n");
            if opened.tail > 0 {
                return Ok(Err(ErrorInfo {
                    message: "There is some data block after the end of the archive".to_owned(),
                    files: Vec::new(),
                    code: win::E_NOTIMPL,
                }));
            }
            // Updating other formats comes later in the phase.
            let Some(archive) = opened.archive() else {
                return Err(Stop::System(win_error(win::E_NOTIMPL)));
            };
            let header_encrypted = archive.header_encrypted();
            Ok(Ok(Some(Source {
                opened,
                mtime: Some(meta.last_write_time()),
                header_password: password.filter(|_| header_encrypted),
            })))
        }
        Err(failure) => {
            console.flush_so();
            console.se(&format!(
                "ERROR: {arc_name}\n{}",
                open_error(&found, &failure, asked)
            ));
            console.flush_se();
            Err(Stop::System(match failure {
                OpenFailure::Io(error) => error,
                _ => win_error(win::INVALID_FUNCTION),
            }))
        }
    }
}

/// `WarningsCheck` for an error: the scan's warnings, then "Error:" with the message,
/// the files and the system's words; the command stops with that system error.
fn report_error<SE: cash_core::ShellExtensions>(
    console: &Console<'_, SE>,
    warnings: &Warnings,
    info: &ErrorInfo,
) -> Stop {
    report_scan_warnings(console, warnings);
    let error = win_error(info.code);
    let mut message = String::new();
    if !info.message.is_empty() {
        message.push_str(&info.message);
        message.push('\n');
    }
    for file in &info.files {
        message.push_str(file);
        message.push('\n');
    }
    message.push_str(&text::system_message(&error));
    message.push('\n');
    summary(console, &format!("\nError:\n{message}"));
    Stop::System(error)
}

/// `WarningsCheck`'s stream: the messages', or the errors' when messages are off.
fn summary<SE: cash_core::ShellExtensions>(console: &Console<'_, SE>, text: &str) {
    if console.messages.get() == super::cmdline::Target::Off {
        console.se(text);
    } else {
        console.so(text);
    }
}

fn report_scan_warnings<SE: cash_core::ShellExtensions>(
    console: &Console<'_, SE>,
    warnings: &Warnings,
) {
    if warnings.scan.is_empty() {
        return;
    }
    let mut s = String::from("\nScan WARNINGS for files and folders:\n\n");
    for (path, error) in &warnings.scan {
        let _ = writeln!(s, "{path} : {}", text::system_message(error));
    }
    let _ = writeln!(
        s,
        "----------------\nScan WARNINGS: {}",
        warnings.scan.len()
    );
    summary(console, &s);
}

/// `WarningsCheck` when all went through: the warnings, else "Everything is Ok".
fn warnings_check<SE: cash_core::ShellExtensions>(
    console: &Console<'_, SE>,
    warnings: &Warnings,
) -> u8 {
    let code = u8::from(!warnings.scan.is_empty());
    report_scan_warnings(console, warnings);
    if warnings.failed.is_empty() {
        if warnings.scan.is_empty() {
            console.flush_se();
            console.so("Everything is Ok\n");
        }
        return code;
    }
    let mut s = String::from("\nWARNINGS for files:\n\n");
    for (path, error) in &warnings.failed {
        let _ = writeln!(s, "{path} : {}", text::system_message(error));
    }
    let n = warnings.failed.len();
    let _ = writeln!(
        s,
        "----------------\nWARNING: Cannot open {n} file{}",
        if n > 1 { "s" } else { "" }
    );
    summary(console, &s);
    1
}

/// Where the new archive is written before it replaces the old: beside it, or in
/// `-w`'s folder, as `name.tmp`, `name.tmp1` and on.
fn temp_name<SE: cash_core::ShellExtensions>(
    env: &Env<'_, SE>,
    working_dir: Option<&str>,
    arc_path: &Path,
) -> Result<PathBuf, Stop> {
    let file_name = arc_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir = match working_dir {
        Some("") => std::env::temp_dir(),
        Some(dir) => env.path(dir),
        None => arc_path.parent().map(PathBuf::from).unwrap_or_default(),
    };
    for i in 0..1 << 16 {
        let suffix = if i == 0 { String::new() } else { i.to_string() };
        let path = dir.join(format!("{file_name}.tmp{suffix}"));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(Stop::System(win_error(win::FILE_EXISTS)))
}

/// `-stl`: the archive takes the newest time of the items it holds.
fn set_latest_mtime(path: &Path, dir_items: &[DirItem], arc_items: &[ArcItem], pairs: &[Pair]) {
    let latest = pairs
        .iter()
        .filter_map(|p| {
            p.dir
                .map(|d| dir_items[d].modified)
                .or_else(|| p.arc.and_then(|a| arc_items[a].modified))
        })
        .max();
    if let (Some(ticks), Ok(file)) = (latest, OpenOptions::new().write(true).open(path)) {
        // FILETIME ticks: 100 ns since 1601, which is 116 444 736 000 000 000 of them
        // before 1970.
        let since_1970 = ticks.saturating_sub(116_444_736_000_000_000);
        let unix = std::time::Duration::from_nanos(since_1970.saturating_mul(100));
        let _ = file.set_modified(SystemTime::UNIX_EPOCH + unix);
    }
}

/// `-sdel`: the files taken in, then their folders when empty, deepest first.
fn delete_after<SE: cash_core::ShellExtensions>(
    options: &Options,
    console: &Console<'_, SE>,
    dir_items: &[DirItem],
    processed: &[bool],
) {
    let mut shown = false;
    let mut show = |path: &str| {
        if options.log_level > 0 {
            if !shown {
                console.so("\n: Removing files after including to archive\n");
                shown = true;
            }
            console.so(&format!("Removing {path}\n"));
        }
    };
    let mut dirs = Vec::new();
    for (item, done) in dir_items.iter().zip(processed) {
        if item.is_dir {
            dirs.push(item);
            continue;
        }
        if *done {
            show(&item.shown);
            let _ = fs::remove_file(&item.path);
        }
    }
    dirs.sort_by_key(|d| std::cmp::Reverse(d.shown.matches('/').count()));
    for dir in dirs {
        if dir.path.is_dir() {
            show(&dir.shown);
            let _ = fs::remove_dir(&dir.path);
        }
    }
    if shown {
        console.so("\n");
    }
}

/// The writing of one archive: what it needs from the run.
struct Job<'a, 'c, SE: cash_core::ShellExtensions> {
    options: &'a Options,
    env: &'a Env<'c, SE>,
    console: &'a Console<'c, SE>,
    dir_items: &'a [DirItem],
    arc_items: &'a [ArcItem],
    pairs: &'a [Pair],
    actions: &'a ActionSet,
    updating: bool,
    rename: bool,
    case: bool,
}

/// A filter group of `7zUpdate.cpp`: the filter, and whether the data is encrypted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Group {
    filter: Option<Filter>,
    encrypted: bool,
}

impl Group {
    /// `CFilterMode2::Compare`: plain before encrypted, then by the filter's id.
    fn key(self) -> (bool, (u32, u32)) {
        (self.encrypted, self.filter.map_or((0, 0), Filter::sort_key))
    }
}

/// The groups, each with its old blocks and new items, in the order they were met.
#[derive(Default)]
struct Groups {
    list: Vec<Planned>,
}

/// A group, the old blocks it takes (each with how many items stay) and its new items.
type Planned = (Group, Vec<(usize, usize)>, Vec<usize>);

impl Groups {
    fn index(&mut self, group: Group) -> usize {
        if let Some(at) = self.list.iter().position(|(g, _, _)| *g == group) {
            return at;
        }
        self.list.push((group, Vec::new(), Vec::new()));
        self.list.len() - 1
    }
}

impl<SE: cash_core::ShellExtensions> Job<'_, '_, SE> {
    /// `Compress` and the 7z handler's update: the actions, the counts, then the
    /// archive; returns the files read and the archive's size.
    #[expect(
        clippy::too_many_lines,
        reason = "7-Zip's Compress and Update, step by step"
    )]
    fn compress(
        &self,
        source: &mut Option<Source>,
        out_path: &Path,
        warnings: &mut Warnings,
        processed: &mut [bool],
    ) -> Result<(u64, u64), Stop> {
        let console = self.console;
        let settings = Settings::parse(&self.options.properties)?;
        let mut deleted = Stat2::default();
        let ups = if self.rename {
            self.rename_ups()
        } else {
            produce(
                self.pairs,
                *self.actions,
                self.arc_items,
                self.updating.then_some(&mut deleted),
            )?
        };

        // SetNumItems.
        let (mut old, mut new) = (Stat2::default(), Stat2::default());
        for up in &ups {
            if up.new_data && !up.use_arc_props {
                if let Some(d) = up.dir {
                    let di = &self.dir_items[d];
                    new.add(di.is_dir, up.is_anti, di.size);
                }
            } else if let Some(a) = up.arc {
                let ai = &self.arc_items[a];
                let stat = if up.new_data { &mut new } else { &mut old };
                stat.add(ai.is_dir, up.is_anti, ai.size);
            }
        }
        let mut s = String::new();
        if !deleted.is_empty() {
            let _ = writeln!(s, "\nDelete data from archive: {}", deleted.text());
        }
        if !old.is_empty() {
            let _ = writeln!(s, "Keep old data in archive: {}", old.text());
        }
        let _ = writeln!(s, "Add new data to archive: {}\n", new.text());
        console.so(&s);

        if self.options.stdout {
            return Err(Stop::System(win_error(win::E_NOTIMPL)));
        }

        // The 7z handler's items, and which times and attributes it keeps.
        let db = source.as_ref().and_then(|s| s.opened.archive().cloned());
        let files = db.as_ref().map_or(&[][..], |db| db.files.as_slice());
        let kept = |explicit: Option<bool>, default: bool, has: &dyn Fn(&ArchiveEntry) -> bool| {
            explicit.unwrap_or_else(|| {
                if files.is_empty() {
                    default
                } else {
                    files.iter().any(has)
                }
            })
        };
        let need_ctime = kept(settings.ctime, false, &|f| f.has_creation_date);
        let need_atime = kept(settings.atime, false, &|f| f.has_access_date);
        let need_mtime = kept(settings.mtime, true, &|f| f.has_last_modified_date);
        let need_attrib = kept(settings.attributes, true, &|f| f.has_windows_attributes);
        let items: Vec<Item> = ups
            .iter()
            .map(|up| self.item(up, files, [need_mtime, need_ctime, need_atime, need_attrib]))
            .collect();

        // The password: -p, typed for -p without one, or the header's.
        let mut password = self.options.password.clone().filter(|p| !p.is_empty());
        if password.is_none() && self.options.password.is_some() {
            let prompt = |t: &str| {
                console.so(t);
                console.flush_so();
            };
            password = Some(ask_password(self.env, &prompt)?);
        }
        // The header: LZMA unless -mhc=off, AES with -mhe or when the old one had it;
        // compressed with a password, never for fewer than two items unless encrypted.
        let header_password = source.as_ref().and_then(|s| s.header_password.clone());
        if password.is_none() {
            password.clone_from(&header_password);
        }
        let mut compress_main_header = settings.header_compress;
        let mut encrypt_header = false;
        if password.is_some() {
            encrypt_header = settings
                .header_encrypt
                .unwrap_or_else(|| header_password.is_some());
            compress_main_header = true;
        }
        if items.len() < 2 {
            compress_main_header = false;
        }

        let file = File::create(out_path).map_err(Stop::System)?;
        let mut writer = ArchiveWriter::new(file)?;
        writer.set_times(TimesKept {
            modified: true,
            created: true,
            accessed: true,
        });

        // Items without data first: folders, empty files, anti-items.
        let mut empty: Vec<usize> = (0..items.len())
            .filter(|&i| {
                let item = &items[i];
                if item.up.new_data {
                    !item.has_stream()
                } else {
                    item.up
                        .arc
                        .and_then(|a| files.get(a))
                        .is_some_and(|f| !f.has_stream)
                }
            })
            .collect();
        empty.sort_by(|&a, &b| self.compare_empty(&items[a], &items[b]));
        for &i in &empty {
            writer.push_empty(items[i].empty_entry(files));
        }

        // The old blocks: which items each keeps.
        let use_filters = settings.use_filters();
        let mut groups = Groups::default();
        let mut reduce_repack = 0u64;
        let mut by_arc_file = vec![None; files.len()];
        for (index, item) in items.iter().enumerate() {
            if let Some(a) = item.up.arc {
                by_arc_file[a] = Some(index);
            }
        }
        if let Some(db) = &db {
            for (block_index, block) in db.blocks.iter().enumerate() {
                let block_files = &db.stream_map.block_files[block_index];
                let mut keep = 0usize;
                let mut repack_size = 0;
                for &f in block_files {
                    if let Some(i) = by_arc_file[f]
                        && !items[i].up.new_data
                    {
                        keep += 1;
                        repack_size += files[f].size;
                    }
                }
                if keep == 0 {
                    continue;
                }
                let copy = keep == block.sub_stream_count();
                let filter = if use_filters || copy {
                    block
                        .unpack_coder()
                        .and_then(|c| Filter::of_coder(c.encoder_method_id(), c.properties()))
                } else {
                    None
                };
                if !copy {
                    reduce_repack = reduce_repack.max(repack_size);
                }
                let at = groups.index(Group {
                    filter,
                    encrypted: block.is_encrypted(),
                });
                groups.list[at].1.push((block_index, keep));
            }
        }

        // The new data's size, which dictionaries shrink to.
        let (solid_files, solid_bytes) = settings.block_limits();
        let solid = solid_files > 1 && solid_bytes != 0;
        let mut reduce = 0u64;
        for item in items.iter().filter(|i| i.up.new_data) {
            if solid {
                reduce += item.size;
            } else {
                reduce = reduce.max(item.size);
            }
        }
        reduce = reduce.max(reduce_repack);

        // The new data's groups, by the filter each file's kind calls for.
        for (index, item) in items.iter().enumerate() {
            if !item.up.new_data || !item.has_stream() {
                continue;
            }
            let filter = match item.up.dir {
                Some(d) if use_filters && settings.needs_analysis(&item.name) => {
                    let di = &self.dir_items[d];
                    progress(console, self.options, 3, "A", &di.name);
                    let mut head = Vec::new();
                    if let Ok(file) = File::open(&di.path) {
                        let _ = file.take(1 << 14).read_to_end(&mut head);
                    }
                    settings.filter_for(&item.name, item.size, &head)
                }
                _ => None,
            };
            let at = groups.index(Group {
                filter,
                encrypted: password.is_some(),
            });
            groups.list[at].2.push(index);
        }
        groups.list.sort_by_key(|(group, _, _)| group.key());

        let mut files_read = 0u64;
        for (group, old_blocks, new_items) in &groups.list {
            let group_password = if group.encrypted {
                password.clone()
            } else {
                None
            };
            let chain = settings.chain(group.filter, reduce, group_password.as_deref())?;
            writer.set_content_methods(chain);
            for &(block_index, keep) in old_blocks {
                let Some(source) = source.as_mut() else {
                    continue;
                };
                let Some(db) = &db else {
                    continue;
                };
                self.old_block(
                    &mut writer,
                    source,
                    db,
                    block_index,
                    keep,
                    &items,
                    &by_arc_file,
                )?;
            }
            files_read += self.new_blocks(
                &mut writer,
                &settings,
                new_items,
                &items,
                warnings,
                processed,
            )?;
        }

        writer.set_compress_header(
            settings.header_compress && (encrypt_header || compress_main_header),
        );
        if encrypt_header && let Some(password) = &password {
            let aes =
                sevenz::options::AesEncoderOptions::new(sevenz::Password::from(password.as_str()))
                    .map_err(|_| Stop::System(win_error(win::E_FAIL)))?;
            writer.set_header_encryption(Some(
                sevenz::EncoderConfiguration::new(sevenz::EncoderMethod::AES256_SHA256)
                    .with_options(sevenz::options::EncoderOptions::Aes(aes)),
            ));
        }
        let mut file = writer.finish().map_err(Stop::System)?;
        let size = file.stream_position().map_err(Stop::System)?;
        file.set_len(size).map_err(Stop::System)?;
        for item in &items {
            if item.up.new_data
                && let Some(d) = item.up.dir
                && (self.dir_items[d].is_dir || self.dir_items[d].size == 0)
            {
                processed[d] = true;
            }
        }
        Ok((files_read, size))
    }

    /// `rn`: every item kept, those a pair names under their new names.
    fn rename_ups(&self) -> Vec<Up> {
        let pairs = self
            .options
            .update
            .as_ref()
            .map_or(&[][..], |u| u.rename_pairs.as_slice());
        self.arc_items
            .iter()
            .enumerate()
            .map(|(index, ai)| {
                let new_name = if ai.censored {
                    pairs
                        .iter()
                        .find_map(|(old, new)| renamed(old, new, &ai.name, ai.is_dir, self.case))
                } else {
                    None
                };
                Up {
                    arc: Some(index),
                    new_props: new_name.is_some(),
                    use_arc_props: true,
                    new_name,
                    ..Up::default()
                }
            })
            .collect()
    }

    /// The handler's item for a pair's outcome: the archive's facts, or the disk's,
    /// with the times and attributes kept.
    fn item(&self, up: &Up, files: &[ArchiveEntry], need: [bool; 4]) -> Item {
        let [need_mtime, need_ctime, need_atime, need_attrib] = need;
        let mut item = Item {
            up: up.clone(),
            ..Item::default()
        };
        let arc_file = up.arc.and_then(|a| files.get(a));
        if let Some(f) = arc_file {
            if !up.new_props {
                item.name.clone_from(&f.name);
            }
            item.is_dir = f.is_directory;
            item.size = f.size;
            item.is_anti = f.is_anti_item;
            if !up.new_props {
                item.created = f.has_creation_date.then(|| u64::from(f.creation_date));
                item.accessed = f.has_access_date.then(|| u64::from(f.access_date));
                item.modified = f
                    .has_last_modified_date
                    .then(|| u64::from(f.last_modified_date));
                item.attrib = f.has_windows_attributes.then_some(f.windows_attributes);
            }
        }
        if up.new_props {
            let disk = up.dir.map(|d| &self.dir_items[d]);
            let (attrib, mtime, ctime, atime, name, is_dir) = match (disk, arc_file) {
                (Some(di), _) => (
                    Some(di.attrib),
                    Some(di.modified),
                    Some(di.created),
                    Some(di.accessed),
                    di.name.clone(),
                    di.is_dir,
                ),
                (None, Some(f)) => (
                    f.has_windows_attributes.then_some(f.windows_attributes),
                    f.has_last_modified_date
                        .then(|| u64::from(f.last_modified_date)),
                    f.has_creation_date.then(|| u64::from(f.creation_date)),
                    f.has_access_date.then(|| u64::from(f.access_date)),
                    f.name.clone(),
                    f.is_directory,
                ),
                (None, None) => (None, None, None, None, String::new(), false),
            };
            item.attrib = if need_attrib { attrib } else { None };
            item.modified = if need_mtime { mtime } else { None };
            item.created = if need_ctime { ctime } else { None };
            item.accessed = if need_atime { atime } else { None };
            item.name = up.new_name.clone().unwrap_or(name).replace('\\', "/");
            item.is_dir = is_dir;
            item.is_anti = up.is_anti;
            if item.is_anti {
                item.attrib = None;
                item.modified = None;
                item.created = None;
                item.accessed = None;
                item.size = 0;
            }
        }
        if up.new_data {
            item.size = if item.is_dir {
                0
            } else {
                up.dir.map_or(0, |d| self.dir_items[d].size)
            };
        }
        item
    }

    /// `CompareEmptyItems`: folders, then files, then anti-files, then anti-folders,
    /// by name (anti-folders the other way round).
    fn compare_empty(&self, a: &Item, b: &Item) -> Ordering {
        if a.is_anti != b.is_anti {
            return if a.is_anti {
                Ordering::Greater
            } else {
                Ordering::Less
            };
        }
        if a.is_dir != b.is_dir {
            return match (a.is_dir, b.is_anti) {
                (true, _) if a.is_anti => Ordering::Greater,
                (true, _) => Ordering::Less,
                (false, true) => Ordering::Less,
                (false, false) => Ordering::Greater,
            };
        }
        let n = compare_file_names(&a.name, &b.name, self.case);
        if a.is_dir && a.is_anti {
            n.reverse()
        } else {
            n
        }
    }

    /// An old block: copied as it is when all its items stay, else decoded and the
    /// items that stay compressed again with the group's coders.
    #[expect(
        clippy::too_many_arguments,
        reason = "the block, the archives and the items"
    )]
    fn old_block(
        &self,
        writer: &mut ArchiveWriter<File>,
        source: &mut Source,
        db: &sevenz::Archive,
        block_index: usize,
        keep: usize,
        items: &[Item],
        by_arc_file: &[Option<usize>],
    ) -> Result<(), Stop> {
        let block = &db.blocks[block_index];
        let block_files = &db.stream_map.block_files[block_index];
        let entry_for = |f: usize| -> ArchiveEntry {
            let mut entry = db.files[f].clone();
            if let Some(i) = by_arc_file[f]
                && items[i].up.new_props
            {
                let item = &items[i];
                entry.name.clone_from(&item.name);
                entry.has_windows_attributes = item.attrib.is_some();
                entry.windows_attributes = item.attrib.unwrap_or(0);
                entry.has_last_modified_date = item.modified.is_some();
                entry.last_modified_date = NtTime::from(item.modified.unwrap_or(0));
                entry.has_creation_date = item.created.is_some();
                entry.creation_date = NtTime::from(item.created.unwrap_or(0));
                entry.has_access_date = item.accessed.is_some();
                entry.access_date = NtTime::from(item.accessed.unwrap_or(0));
            }
            entry
        };
        if keep == block.sub_stream_count() {
            progress(
                self.console,
                self.options,
                3,
                "=",
                &format!("#{block_index}"),
            );
            for &f in block_files {
                progress(self.console, self.options, 3, "=", &db.files[f].name);
            }
            let entries: Vec<ArchiveEntry> = block_files.iter().map(|&f| entry_for(f)).collect();
            let reader = source
                .opened
                .reader()
                .ok_or_else(|| Stop::System(win_error(win::E_NOTIMPL)))?;
            writer.push_copied_block(entries, block, |out| reader.copy_packed(block_index, out))?;
            return Ok(());
        }
        progress(
            self.console,
            self.options,
            2,
            "R",
            &format!("#{block_index}"),
        );
        let wanted: Vec<bool> = block_files
            .iter()
            .map(|&f| by_arc_file[f].is_some_and(|i| !items[i].up.new_data))
            .collect();
        let mut place = vec![None; db.files.len()];
        for (at, &f) in block_files.iter().enumerate() {
            place[f] = Some(at);
        }
        let reader = source
            .opened
            .reader()
            .ok_or_else(|| Stop::System(win_error(win::E_NOTIMPL)))?;
        let console = self.console;
        let options = self.options;
        writer.push_block_by(|sink| {
            let mut result: Result<(), Stop> = Ok(());
            reader.for_each_entries(
                &|f| place.get(f).is_some_and(Option::is_some),
                |f, entry, data| {
                    let at = place.get(f).copied().flatten().unwrap_or(0);
                    if !wanted[at] {
                        progress(console, options, 2, ".", &entry.name);
                        let _ = io::copy(data, &mut io::sink());
                        return Ok::<bool, Stop>(true);
                    }
                    progress(console, options, 2, "R", &entry.name);
                    if let Err(error) = sink.add(entry_for(f), data) {
                        result = Err(Stop::from(error));
                        return Ok(false);
                    }
                    if data.finish().is_err() {
                        result = Err(Stop::System(win_error(win::E_FAIL)));
                        return Ok(false);
                    }
                    Ok(true)
                },
            )?;
            result
        })?;
        Ok(())
    }

    /// The new files of a group, sorted and split into solid blocks; returns how many
    /// were read.
    fn new_blocks(
        &self,
        writer: &mut ArchiveWriter<File>,
        settings: &Settings,
        new_items: &[usize],
        items: &[Item],
        warnings: &mut Warnings,
        processed: &mut [bool],
    ) -> Result<u64, Stop> {
        let mut order: Vec<usize> = new_items.to_vec();
        let by_type = settings.sort_by_type;
        order.sort_by(|&a, &b| {
            self.compare_new(&items[a], &items[b], by_type)
                .then(a.cmp(&b))
        });
        let (solid_files, solid_bytes) = settings.block_limits();
        let mut files_read = 0u64;
        let mut i = 0;
        while i < order.len() {
            let mut total = 0u64;
            let mut count = 0usize;
            let mut first_ext: Option<String> = None;
            while i + count < order.len() && (count as u64) < solid_files {
                let item = &items[order[i + count]];
                total += item.size;
                if total > solid_bytes {
                    break;
                }
                if settings.solid_by_extension {
                    let ext = extension(&item.name).to_lowercase();
                    match &first_ext {
                        None => first_ext = Some(ext),
                        Some(prev) if *prev != ext => break,
                        Some(_) => {}
                    }
                }
                count += 1;
            }
            let count = count.max(1);
            let block = &order[i..i + count];
            i += count;
            let console = self.console;
            writer.push_block_by(|sink| {
                for &index in block {
                    let item = &items[index];
                    let Some(d) = item.up.dir else {
                        continue;
                    };
                    let di = &self.dir_items[d];
                    let mark = if item.up.arc.is_some() { "U" } else { "+" };
                    progress(console, self.options, 1, mark, &di.name);
                    let file = match File::open(&di.path) {
                        Ok(file) => file,
                        Err(error) => {
                            common_error(console, &di.shown, &error, true);
                            warnings.failed.push((di.shown.clone(), copy_error(&error)));
                            continue;
                        }
                    };
                    let mut watched = Watched { file, error: None };
                    if let Err(error) = sink.add(item.entry(), &mut watched) {
                        if let Some(read_error) = watched.error {
                            common_error(console, &di.shown, &read_error, false);
                            return Err(Stop::System(read_error));
                        }
                        return Err(Stop::from(error));
                    }
                    files_read += 1;
                    processed[d] = true;
                }
                Ok::<(), Stop>(())
            })?;
        }
        Ok(files_read)
    }

    /// `CompareUpdateItems`: by name, or with `-mqs` by the extension's place in
    /// 7-Zip's list, the extension, the name, the time and the size first.
    fn compare_new(&self, a: &Item, b: &Item, by_type: bool) -> Ordering {
        if by_type {
            let (ea, eb) = (extension(&a.name), extension(&b.name));
            let order = ext_index(ea)
                .cmp(&ext_index(eb))
                .then_with(|| compare_file_names(ea, eb, self.case))
                .then_with(|| compare_file_names(base_name(&a.name), base_name(&b.name), self.case))
                .then_with(|| match (a.modified, b.modified) {
                    (None, Some(_)) => Ordering::Greater,
                    (Some(_), None) => Ordering::Less,
                    (x, y) => x.cmp(&y),
                })
                .then(a.size.cmp(&b.size));
            if order != Ordering::Equal {
                return order;
            }
        }
        compare_file_names(&a.name, &b.name, self.case)
    }
}

/// A file being read into a block, its read error kept for the message.
struct Watched {
    file: File,
    error: Option<io::Error>,
}

impl Read for Watched {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf).inspect_err(|error| {
            self.error = Some(copy_error(error));
        })
    }
}

/// The part of a name after its last `/`.
fn base_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// The extension of a name's last part, empty without one.
fn extension(name: &str) -> &str {
    let base = base_name(name);
    base.rfind('.')
        .map_or("", |dot| base.get(dot + 1..).unwrap_or_default())
}

/// `GetExtIndex`: an extension's place in 7-Zip's list of kinds, 0 for none.
fn ext_index(ext: &str) -> usize {
    const EXTS: &str = " 7z xz lzma ace arc arj bz tbz bz2 tbz2 cab deb gz tgz ha lha lzh lzo lzx pak rar rpm sit zoo zip jar ear war msi 3gp avi mov mpeg mpg mpe wmv aac ape fla flac la mp3 m4a mp4 ofr ogg pac ra rm rka shn swa tta wv wma wav swf chm hxi hxs gif jpeg jpg jp2 png tiff  bmp ico psd psp awg ps eps cgm dxf svg vrml wmf emf ai md cad dwg pps key sxi max 3ds iso bin nrg mdf img pdi tar cpio xpi vfd vhd vud vmc vsv vmdk dsk nvram vmem vmsd vmsn vmss vmtm inl inc idl acf asa h hpp hxx c cpp cxx m mm go swift rc java cs rs pas bas vb cls ctl frm dlg def f77 f f90 f95 asm s sql manifest dep mak clw csproj vcproj sln dsp dsw class bat cmd bash sh xml xsd xsl xslt hxk hxc htm html xhtml xht mht mhtml htw asp aspx css cgi jsp shtml awk sed hta js json php php3 php4 php5 phptml pl pm py pyo rb tcl ts vbs text txt tex ans asc srt reg ini doc docx mcw dot rtf hlp xls xlr xlt xlw ppt pdf sxc sxd sxi sxg sxw stc sti stw stm odt ott odg otg odp otp ods ots odf abw afp cwk lwp wpd wps wpt wrf wri abf afm bdf fon mgf otf pcf pfa snf ttf dbf mdb nsf ntf wdb db fdb gdb exe dll ocx vbx sfx sys tlb awx com obj lib out o so pdb pch idb ncb opt";
    if ext.is_empty() || !ext.is_ascii() {
        return 0;
    }
    let lower = ext.to_ascii_lowercase();
    let words: Vec<&str> = EXTS.split(' ').filter(|w| !w.is_empty()).collect();
    words
        .iter()
        .position(|w| *w == lower)
        .map_or(words.len() + 1, |at| at + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_names_get_7z_unless_they_have_an_extension() {
        assert_eq!(final_name("a", NameMode::Smart, "7z"), "a.7z");
        assert_eq!(final_name("d/a.7z", NameMode::Smart, "7z"), "d/a.7z");
        assert_eq!(final_name("a.dat", NameMode::Smart, "7z"), "a.dat");
        assert_eq!(final_name("a.", NameMode::Smart, "7z"), "a");
        assert_eq!(final_name("a.7z", NameMode::Add, "7z"), "a.7z.7z");
        assert_eq!(final_name("a", NameMode::Exact, "7z"), "a");
        assert_eq!(final_name("../x.y/a", NameMode::Smart, "7z"), "../x.y/a.7z");
    }

    #[test]
    fn renaming_takes_a_folder_and_what_is_under_it() {
        assert_eq!(
            renamed("d/sub", "d/folder", "d/sub", true, false).as_deref(),
            Some("d/folder")
        );
        assert_eq!(
            renamed("d/sub", "d/folder", "d/sub/b.txt", false, false).as_deref(),
            Some("d/folder/b.txt")
        );
        assert_eq!(renamed("d/sub", "d/folder", "d/subway", false, false), None);
        assert_eq!(
            renamed("D\\A.TXT", "x", "d/a.txt", false, false).as_deref(),
            Some("x")
        );
        assert_eq!(renamed("d/", "e", "d", true, false).as_deref(), Some("e"));
    }

    #[test]
    fn extensions_take_their_place_in_7_zips_list() {
        assert_eq!(ext_index(""), 0);
        assert_eq!(ext_index("7z"), 1);
        assert!(ext_index("txt") < ext_index("exe"));
        assert_eq!(ext_index("unknown"), ext_index("other"));
    }
}
