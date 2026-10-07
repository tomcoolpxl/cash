//! `l`, `t`, `x` and `e`: the archives a name gives (7-Zip's scan), then listing them, or
//! testing or extracting their items, with `ExtractCallbackConsole`'s messages and
//! `Main`'s summary.

use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use cash_archive::sevenz::Problem;
use cash_core::openfiles::OpenFiles;

use super::archive::{self, Data, Item, Kind, OpenFailure, Opened};
use super::censor::{has_wildcard, mask_matches, split_path};
use super::cmdline::{Command, Options, Overwrite, PathKeep};
use super::{Console, Env, Stop, code, links, list, text};

/// An archive the scan found: its name as given, its path, its size, and whether it is
/// standard input (`-si`), spooled to that path.
pub(super) struct Found {
    pub(super) name: String,
    pub(super) path: PathBuf,
    pub(super) size: u64,
    pub(super) stdin: bool,
}

pub(super) fn run<SE: cash_core::ShellExtensions>(
    options: &Options,
    env: &Env<'_, SE>,
    console: &Console<'_, SE>,
) -> Result<u8, Stop> {
    let found = scan(options, env, console)?;
    let _spools: Vec<archive::Spool> = found
        .iter()
        .filter(|f| f.stdin)
        .map(|f| archive::Spool(f.path.clone()))
        .collect();
    if options.command == Command::List {
        return list::run(options, env, console, &found);
    }
    extract_all(options, env, console, &found)
}

/// 7-Zip's scan for archives: the archive's name, or the files its wildcard names in
/// its folder, sorted; "Scanning the drive for archives:" and what was found.
fn scan<SE: cash_core::ShellExtensions>(
    options: &Options,
    env: &Env<'_, SE>,
    console: &Console<'_, SE>,
) -> Result<Vec<Found>, Stop> {
    if let Some(name) = &options.stdin {
        // 7-Zip reads standard input once as it opens it; cash keeps it in a file.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let path = std::env::temp_dir().join(format!("cash-7z-si-{}-{nanos}", std::process::id()));
        let size = fs::File::create(&path)
            .and_then(|mut file| io::copy(&mut env.context.stdin(), &mut file))
            .map_err(Stop::System)?;
        return Ok(vec![Found {
            name: name.clone(),
            path,
            size,
            stdin: true,
        }]);
    }
    if options.headers {
        console.so("Scanning the drive for archives:\n");
    }
    console.progress_quiet(|s| "Scan".clone_into(&mut s.command));
    let mut found = Vec::new();
    for (prefix, node) in options.archive_censor.pairs() {
        walk(env, console, prefix, node, &mut found)?;
    }
    // FinishScanning.
    console.close_progress();
    console.progress_quiet(super::percent::State::clear);
    found.sort_by_key(|a| a.name.to_lowercase());
    if options.headers {
        let size: u64 = found
            .iter()
            .filter(|f| f.size != u64::MAX)
            .map(|f| f.size)
            .sum();
        let files = found.len() as u64;
        console.so(&format!(
            "{}{}\n",
            text::count(files, "file", "files"),
            if files == 0 && size == 0 {
                ", 0 bytes".to_owned()
            } else {
                format!(", {}", text::size_smart(size))
            }
        ));
    }
    Ok(found)
}

/// The archives a censor's folder names: each mask's file, or the files its wildcard
/// names; then the folders under it.
fn walk<SE: cash_core::ShellExtensions>(
    env: &Env<'_, SE>,
    console: &Console<'_, SE>,
    dir: &str,
    node: &super::censor::Node,
    found: &mut Vec<Found>,
) -> Result<(), Stop> {
    // ScanProgress: what was found so far, and the folder looked in.
    let bytes = found
        .iter()
        .filter(|f| f.size != u64::MAX)
        .fold(0u64, |sum, f| sum.saturating_add(f.size));
    console.progress(|s| {
        s.files = found.len() as u64;
        s.completed = bytes;
        dir.clone_into(&mut s.file_name);
    });
    for (parts, wildcards) in node.masks() {
        if wildcards && parts.iter().any(|p| has_wildcard(p)) {
            found.extend(expand(env, dir, parts));
            continue;
        }
        let name = format!("{dir}{}", parts.join("/"));
        let path = env.path(&name);
        match fs::metadata(&path) {
            Ok(meta) => found.push(Found {
                name,
                size: if meta.is_dir() { u64::MAX } else { meta.len() },
                stdin: false,
                path,
            }),
            Err(error) => {
                console.flush_so();
                console.se(&format!(
                    "\nERROR: {}\n{name}\n\n",
                    text::system_message(&error)
                ));
                console.flush_se();
                return Err(Stop::System(error));
            }
        }
    }
    for child in node.children() {
        walk(
            env,
            console,
            &format!("{dir}{}/", child.name()),
            child,
            found,
        )?;
    }
    Ok(())
}

/// The files a wildcard names in its folder.
fn expand<SE: cash_core::ShellExtensions>(
    env: &Env<'_, SE>,
    prefix: &str,
    parts: &[String],
) -> Vec<Found> {
    let Some((mask, folders)) = parts.split_last() else {
        return Vec::new();
    };
    let mut dir = prefix.to_owned();
    for folder in folders {
        dir.push_str(folder);
        dir.push('/');
    }
    let Ok(entries) = fs::read_dir(env.path(if dir.is_empty() { "." } else { &dir })) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let file_name = e.file_name().to_string_lossy().into_owned();
            mask_matches(mask, &file_name, false).then(|| {
                let name = format!("{dir}{file_name}");
                Found {
                    size: e.metadata().map_or(0, |m| m.len()),
                    path: env.path(&name),
                    name,
                    stdin: false,
                }
            })
        })
        .collect()
}

/// The archive's format as `-t` names it.
pub(super) fn forced_kind(options: &Options) -> Option<Kind> {
    options.archive_type.as_deref().and_then(Kind::by_name)
}

/// Opens an archive, asking for its password at the console when its header is
/// encrypted and none was given: "Enter password (will not be echoed):".
pub(super) fn open_asking<SE: cash_core::ShellExtensions>(
    found: &Found,
    forced: Option<Kind>,
    env: &Env<'_, SE>,
    password: &mut Option<String>,
    prompt: &dyn Fn(&str),
    asked: &mut bool,
) -> Result<Result<Opened, OpenFailure>, Stop> {
    if found.stdin {
        return Ok(archive::open_stdin(
            &found.path,
            forced,
            &found.name,
            &env.zone,
        ));
    }
    let opened = archive::open(&found.path, forced, password.as_deref(), &env.zone);
    if !matches!(opened, Err(OpenFailure::PasswordNeeded)) {
        return Ok(opened);
    }
    *password = Some(ask_password(env, prompt)?);
    *asked = true;
    Ok(
        archive::open(&found.path, forced, password.as_deref(), &env.zone).map_err(|failure| {
            match failure {
                OpenFailure::PasswordNeeded => OpenFailure::WrongPassword,
                other => other,
            }
        }),
    )
}

/// `GetPassword`: the question, a line read without echo, a new line; the end of the
/// input is a break.
pub(super) fn ask_password<SE: cash_core::ShellExtensions>(
    env: &Env<'_, SE>,
    prompt: &dyn Fn(&str),
) -> Result<String, Stop> {
    prompt("\nEnter password (will not be echoed):");
    let line = read_line(env, false)?;
    prompt("\n");
    match line {
        Some(line) => Ok(line),
        None => Err(Stop::Break),
    }
}

/// A line of standard input, the console's read as typed (shown or not); `None` at
/// the end of the input.
pub(super) fn read_line<SE: cash_core::ShellExtensions>(
    env: &Env<'_, SE>,
    shown: bool,
) -> Result<Option<String>, Stop> {
    let context = env.context;
    let console = context
        .try_fd(OpenFiles::STDIN_FD)
        .and_then(|file| file.console(true, shown));
    if let Some(mut console) = console {
        return match console.line()? {
            cash_win32::conin::Line::Typed(text) => {
                Ok(Some(text.trim_end_matches(['\r', '\n']).to_owned()))
            }
            cash_win32::conin::Line::EndOfInput => Ok(None),
            cash_win32::conin::Line::Interrupted => Err(Stop::Break),
        };
    }
    let mut stdin = context.stdin();
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if stdin.read(&mut byte)? == 0 {
            if bytes.is_empty() {
                return Ok(None);
            }
            break;
        }
        if byte[0] == b'\n' {
            break;
        }
        bytes.push(byte[0]);
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// `Print_OpenArchive_Props`: the archive's facts.
pub(super) fn archive_props(found: &Found, opened: &Opened) -> String {
    let mut name = found.name.replace('\\', "/");
    let mut text = String::from("--\n");
    // A split archive: its level, then its one item, the archive inside.
    if let Some(split) = &opened.split {
        let _ = write!(
            text,
            "Path = {name}\nType = Split\nPhysical Size = {}\nVolumes = {}\n\
             Total Physical Size = {}\n----\n",
            split.first_size,
            split.parts.len(),
            split.total
        );
        name = inner_name(&name);
        let _ = write!(text, "Path = {name}\nSize = {}\n--\n", split.total);
    }
    let _ = writeln!(text, "Path = {name}");
    if let Some(named) = opened.type_warning {
        let _ = writeln!(
            text,
            "Open WARNING: Cannot open the file as [{}] archive",
            named.name()
        );
    }
    let _ = writeln!(text, "Type = {}", opened.kind.name());
    if !opened.error_flags.is_empty() {
        let _ = writeln!(text, "ERRORS:\n{}", opened.error_flags.join("\n"));
    }
    let warnings = opened.warnings();
    if !warnings.is_empty() {
        let _ = writeln!(text, "WARNINGS:\n{}", warnings.join("\n"));
    }
    if let Some(size) = opened.physical_size {
        let _ = writeln!(text, "Physical Size = {size}");
    }
    if opened.tail > 0 {
        let _ = writeln!(text, "Tail Size = {}", opened.tail);
    }
    for (name, value) in &opened.props {
        let _ = writeln!(text, "{name} = {value}");
    }
    text
}

/// The archive inside a split one: the first volume's name less `.001`.
fn inner_name(name: &str) -> String {
    name.rsplit_once('.')
        .map_or_else(|| name.to_owned(), |(stem, _)| stem.to_owned())
}

/// `Print_OpenArchive_Error`: why an archive did not open, and its flags.
pub(super) fn open_error(found: &Found, failure: &OpenFailure, password_asked: bool) -> String {
    let mut text = String::new();
    match failure {
        OpenFailure::WrongPassword => {
            text.push_str("Cannot open encrypted archive. Wrong password?");
        }
        _ if password_asked => text.push_str("Cannot open encrypted archive. Wrong password?"),
        OpenFailure::NotArchive {
            tried: Some(kind),
            in_split,
            ..
        } => {
            let mut name = found.name.replace('\\', "/");
            if *in_split {
                name = inner_name(&name);
            }
            let _ = write!(
                text,
                "{name}\nOpen ERROR: Cannot open the file as [{}] archive\n",
                kind.name()
            );
        }
        _ => text.push_str("Cannot open the file as archive"),
    }
    text.push_str("\n\n");
    if let OpenFailure::NotArchive { flags, .. } = failure
        && !flags.is_empty()
        && !password_asked
    {
        text.push_str("ERRORS:\n");
        text.push_str(&flags.join("\n"));
        text.push('\n');
    }
    text
}

/// What extraction counted over all the archives (`CExtractCallbackConsole`'s counts and
/// `CDecompressStat`).
#[derive(Default)]
struct Totals {
    tried: u64,
    ok: u64,
    cant_open: u64,
    with_errors: u64,
    with_warnings: u64,
    open_warnings: u64,
    open_errors: u64,
    file_errors: u64,
    folders: u64,
    files: u64,
    size: u64,
    packed: u64,
    /// `-scrc`'s hashes of what was tested or extracted.
    hash: Option<super::hash::Bundle>,
}

impl Totals {
    /// `Main`'s summary up to the counts of items: what it prints even when a break or
    /// an error stops the command.
    fn counts<SE: cash_core::ShellExtensions>(&self, console: &Console<'_, SE>) -> bool {
        console.so("\n");
        if self.tried > 1 {
            console.so(&format!(
                "Archives: {}\nOK archives: {}\n",
                self.tried, self.ok
            ));
        }
        let mut error = false;
        if self.cant_open != 0 {
            error = true;
            console.so(&format!("Can't open as archive: {}\n", self.cant_open));
        }
        if self.with_errors != 0 {
            error = true;
            console.so(&format!("Archives with Errors: {}\n", self.with_errors));
        }
        if self.with_warnings != 0 {
            console.so(&format!("Archives with Warnings: {}\n", self.with_warnings));
        }
        if self.open_warnings != 0 {
            console.so(&format!("\nWarnings: {}\n", self.open_warnings));
        }
        if self.open_errors != 0 {
            error = true;
            console.so(&format!("\nOpen Errors: {}\n", self.open_errors));
        }
        error
    }
}

fn extract_all<SE: cash_core::ShellExtensions>(
    options: &Options,
    env: &Env<'_, SE>,
    console: &Console<'_, SE>,
    found: &[Found],
) -> Result<u8, Stop> {
    let mut totals = Totals {
        hash: options
            .hash_methods
            .as_deref()
            .map(super::hash::Bundle::new)
            .transpose()?,
        ..Totals::default()
    };
    let mut password = options.password.clone();
    let outcome = (|| -> Result<(), Stop> {
        for archive in found {
            extract_archive(options, env, console, archive, &mut password, &mut totals)?;
        }
        Ok(())
    })();
    let error = totals.counts(console);
    let failed = totals.with_errors != 0 || totals.file_errors != 0;
    if failed {
        console.so("\n");
        if totals.file_errors != 0 {
            console.so(&format!("Sub items Errors: {}\n", totals.file_errors));
        }
    }
    outcome?;
    let code = if error { code::FATAL } else { 0 };
    if !failed {
        if totals.folders != 0 {
            console.so(&format!("Folders: {}\n", totals.folders));
        }
        if totals.files != 1 || totals.folders != 0 {
            console.so(&format!("Files: {}\n", totals.files));
        }
        console.so(&format!(
            "Size:       {}\nCompressed: {}\n",
            totals.size, totals.packed
        ));
        if let Some(hash) = &totals.hash {
            console.so(&format!("\n{}", hash.stat()));
        }
    }
    Ok(code)
}

/// One archive: `BeforeOpen`, `OpenResult`, its items, `ExtractResult`.
#[expect(
    clippy::too_many_lines,
    reason = "7-Zip's extraction of one archive, step by step"
)]
fn extract_archive<SE: cash_core::ShellExtensions>(
    options: &Options,
    env: &Env<'_, SE>,
    console: &Console<'_, SE>,
    archive: &Found,
    password: &mut Option<String>,
    totals: &mut Totals,
) -> Result<(), Stop> {
    let test = options.command == Command::Test;
    totals.tried += 1;
    console.so(&format!(
        "\n{}{}\n",
        if test {
            "Testing archive: "
        } else {
            "Extracting archive: "
        },
        archive.name.replace('\\', "/")
    ));
    let mut asked = false;
    let opened = open_asking(
        archive,
        forced_kind(options),
        env,
        password,
        &|t| console.so(t),
        &mut asked,
    )?;
    let mut opened = match opened {
        Ok(opened) => opened,
        Err(failure) => {
            totals.cant_open += 1;
            console.flush_so();
            if let OpenFailure::Io(error) = &failure {
                console.se(&format!(
                    "ERROR: {}\n{}\n",
                    archive.name.replace('\\', "/"),
                    text::system_message(error)
                ));
            } else if matches!(failure, OpenFailure::NotImplemented) {
                console.se(&format!(
                    "ERROR: {}\nCannot open the file as archive\n\nNot implemented\n",
                    archive.name.replace('\\', "/")
                ));
            } else {
                console.se(&format!(
                    "ERROR: {}\n{}",
                    archive.name.replace('\\', "/"),
                    open_error(archive, &failure, asked)
                ));
            }
            console.flush_se();
            return Ok(());
        }
    };
    // `OpenResult`: the progress line cleared; the errors on the errors' stream, the
    // warnings on the messages'.
    console.close_progress();
    console.progress_quiet(|s| {
        s.files = 0;
        s.command.clear();
        s.file_name.clear();
    });
    let open_error = !opened.error_flags.is_empty();
    if open_error {
        totals.open_errors += 1;
        console.flush_so();
        console.se(&format!("\nERRORS:\n{}\n\n", opened.error_flags.join("\n")));
        console.flush_se();
    }
    let warnings = opened.warnings();
    if !warnings.is_empty() {
        totals.open_warnings += 1;
        console.so(&format!("\nWARNINGS:\n{}\n\n", warnings.join("\n")));
        console.flush_so();
    }
    // `Print_ErrorFormatIndex_Warning`: opened as another format than its name says.
    if let Some(named) = opened.type_warning {
        console.so(&format!(
            "WARNING:\n{}\nCannot open the file as [{}] archive\nThe file is open as [{}] archive\n\n",
            archive.name.replace('\\', "/"),
            named.name(),
            opened.kind.name()
        ));
        console.flush_so();
    }
    let warned = !warnings.is_empty() || opened.type_warning.is_some();
    console.so(&archive_props(archive, &opened));
    console.so("\n");
    totals.packed += match opened.seq_headers {
        // A tar from standard input: its headers, and the data of the items skipped.
        Some(headers) => {
            let censor = &options.censor;
            let all = censor.all_allowed() && !options.exclude_dirs && !options.exclude_files;
            headers
                + opened
                    .items
                    .iter()
                    .filter(|item| !(all || censor.wants(&item.path, item.is_dir)))
                    .filter_map(|item| item.packed)
                    .sum::<u64>()
        }
        None => opened.split.as_ref().map_or(archive.size, |s| s.total),
    };
    let errors = extract_items(options, env, console, &mut opened, password, totals)?;
    console.flush_so();
    if errors == 0 && !open_error {
        if warned {
            totals.with_warnings += 1;
        } else {
            totals.ok += 1;
        }
        console.so("Everything is Ok\n");
    } else {
        totals.with_errors += 1;
        totals.file_errors += errors;
        console.so("\n");
        if errors != 0 {
            console.so(&format!("Sub items Errors: {errors}\n"));
        }
    }
    Ok(())
}

/// The items to decode: those wanted, and in a solid block those before the last wanted
/// one, which 7-Zip decodes, passes over and counts.
fn decode_set(items: &[Item], wanted: &[bool]) -> Vec<bool> {
    let mut last: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    for (index, item) in items.iter().enumerate() {
        if wanted[index]
            && let Some(block) = item.block
        {
            last.insert(block, index);
        }
    }
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            wanted[index]
                || item
                    .block
                    .and_then(|b| last.get(&b))
                    .is_some_and(|&last| index < last)
        })
        .collect()
}

#[expect(
    clippy::too_many_lines,
    reason = "7-Zip's extraction of one archive, item by item"
)]
fn extract_items<SE: cash_core::ShellExtensions>(
    options: &Options,
    env: &Env<'_, SE>,
    console: &Console<'_, SE>,
    opened: &mut Opened,
    password: &mut Option<String>,
    totals: &mut Totals,
) -> Result<u64, Stop> {
    let test = options.command == Command::Test;
    let items = opened.items.clone();
    let censor = &options.censor;
    let all = censor.all_allowed() && !options.exclude_dirs && !options.exclude_files;
    let wanted: Vec<bool> = items
        .iter()
        .map(|item| all || censor.wants(&item.path, item.is_dir))
        .collect();
    if !wanted.contains(&true) {
        console.so("\nNo files to process\n");
        return Ok(0);
    }
    let decode = decode_set(&items, &wanted);
    if password.is_none()
        && items
            .iter()
            .zip(&decode)
            .any(|(item, d)| *d && item.encrypted)
    {
        let typed = ask_password(env, &|t| console.so(t))?;
        opened.set_password(&typed);
        *password = Some(typed);
    }
    let (out_dir, base) = match &options.output_dir {
        Some(dir) => {
            let mut base = dir.replace('\\', "/");
            if !base.ends_with('/') {
                base.push('/');
            }
            (env.path(dir), base)
        }
        None => (env.path("."), "./".to_owned()),
    };
    if !test && !options.stdout && options.output_dir.is_some() {
        fs::create_dir_all(&out_dir).map_err(|e| {
            Stop::Message(format!(
                "Cannot create output directory : {} : {}",
                text::system_message(&e),
                out_dir.display()
            ))
        })?;
    }
    let mut overwrite = options.overwrite;
    let mut errors = 0u64;
    let mut folder_times: Vec<(PathBuf, u64)> = Vec::new();
    let mut post_links: Vec<links::PostLink> = Vec::new();
    let mut stop: Option<Stop> = None;
    let mut hash = totals.hash.take();
    // SetTotal: what is to be decoded; SetCompleted from none.
    let total = items
        .iter()
        .zip(&decode)
        .filter(|(_, d)| **d)
        .map(|(item, _)| item.size.unwrap_or(0))
        .fold(0u64, u64::saturating_add);
    console.progress(|s| {
        s.total = total;
        s.completed = 0;
    });
    let mut done = 0u64;
    let result: Result<(), Stop> = opened.extract(&|index| decode[index], |index, data| {
        let item = &items[index];
        let skip = !wanted[index];
        // -scrc hashes what is tested, folders too, and the files extracted.
        let hashing = !skip && (test || !item.is_dir);
        // A link extracted is a placeholder now, the link at the end; the tar handler
        // asks no stream for a file one, so it is neither shown nor counted, unless
        // -scrc hashes it.
        let as_link = item
            .link
            .as_ref()
            .filter(|_| !test && !skip && !options.stdout);
        if let Some(link) = as_link {
            let (path, relative) = target(&out_dir, item, options.path_keep);
            let shown = format!("{base}{relative}");
            if link.hard || options.symlinks != Some(false) {
                let room = if item.is_dir {
                    Ok(Some(path))
                } else {
                    make_room(env, console, item, &path, &shown, &mut overwrite)
                };
                match room {
                    Ok(Some(path)) => match links::placeholder(&path, &shown) {
                        Ok(()) => post_links.push(links::PostLink {
                            item_path: item.path.clone(),
                            parts: relative.split('/').map(str::to_owned).collect(),
                            is_dir: item.is_dir,
                            link: link.clone(),
                            path,
                            shown,
                            modified: item.modified,
                        }),
                        Err(message) => {
                            errors += 1;
                            console.flush_so();
                            console.se(&format!("ERROR: {message}\n"));
                            console.flush_se();
                        }
                    },
                    Ok(None) => return Ok(true),
                    Err(Stop::System(error)) => {
                        errors += 1;
                        console.flush_so();
                        console.se(&format!("ERROR: {}\n", text::system_message(&error)));
                        console.flush_se();
                        return Ok(true);
                    }
                    Err(other) => {
                        stop = Some(other);
                        return Ok(false);
                    }
                }
            }
            if !item.is_dir && hash.is_none() {
                return Ok(true);
            }
        }
        // GetStream asks about a file already there before the item is shown; one not
        // to be replaced is shown as skipped.
        let room = if !test && !skip && !options.stdout && as_link.is_none() && !item.is_dir {
            let (path, relative) = target(&out_dir, item, options.path_keep);
            let shown = format!("{base}{relative}");
            Some(
                match make_room(env, console, item, &path, &shown, &mut overwrite) {
                    Ok(room) => Ok(room),
                    Err(Stop::System(e)) => Err(e),
                    Err(Stop::Message(m)) => Err(io::Error::other(m)),
                    Err(other) => {
                        stop = Some(other);
                        return Ok(false);
                    }
                },
            )
        } else {
            None
        };
        let passed = matches!(room, Some(Ok(None)));
        let mut hashed;
        let data: &mut dyn Data = match hash.as_mut().filter(|_| hashing) {
            Some(bundle) => {
                bundle.start();
                hashed = Hashed { data, bundle };
                &mut hashed
            }
            None => data,
        };
        let mut shown_data;
        let data: &mut dyn Data = if console.progress_on() {
            shown_data = Shown {
                data,
                console,
                done: &mut done,
            };
            &mut shown_data
        } else {
            data
        };
        let level = if skip || passed { 2 } else { 1 };
        let mark = if skip || passed {
            "."
        } else if test {
            "T"
        } else {
            "-"
        };
        let logged = options.log_level >= level;
        if logged {
            let mut shown = item.path.clone();
            if item.is_dir && !shown.ends_with('/') {
                shown.push('/');
            }
            console.so(&format!("{mark} {shown}\n"));
        }
        console.progress_item(mark, &item.path, logged);
        if item.is_dir {
            totals.folders += 1;
        } else {
            totals.files += 1;
        }
        let outcome = if test || skip || as_link.is_some() {
            io::copy(data, &mut io::sink()).map(|_| ())
        } else if options.stdout {
            if item.is_dir {
                Ok(())
            } else {
                copy_to_stdout(console, data)
            }
        } else if let Some(room) = room {
            match room {
                Ok(Some(path)) => write_file(item, &path, data),
                Ok(None) => Ok(()),
                Err(e) => Err(e),
            }
        } else {
            let (path, _) = target(&out_dir, item, options.path_keep);
            let made = fs::create_dir_all(&path);
            if made.is_ok()
                && let Some(time) = item.modified
            {
                folder_times.push((path, time));
            }
            made
        };
        let finished = data.finish();
        // The size known before decoding, else the one the format reports after; a
        // folder's is a link's name.
        if !item.is_dir || item.link.is_some() {
            totals.size += item.size.or_else(|| data.unpacked()).unwrap_or(0);
        }
        if let Err(problem) = finished {
            if !skip {
                errors += 1;
                console.flush_so();
                console.se(&format!(
                    "ERROR: {} : {}\n",
                    problem_words(problem, data.encrypted()),
                    item.path
                ));
                console.flush_se();
            }
        } else if let Err(error) = outcome
            && !options.stdout
        {
            errors += 1;
            console.flush_so();
            console.se(&format!("ERROR: {}\n", text::system_message(&error)));
            console.flush_se();
        }
        if hashing && let Some(bundle) = hash.as_mut() {
            bundle.finish(item.is_dir, &item.path);
        }
        console.progress_quiet(|s| {
            s.command.clear();
            s.file_name.clear();
            s.files += 1;
        });
        Ok(true)
    });
    totals.hash = hash;
    // ExtractResult: the progress line wiped.
    console.close_progress();
    console.progress_quiet(|s| {
        s.command.clear();
        s.file_name.clear();
    });
    // CloseArc: the links, even after a break.
    errors += links::make(console, &post_links, &out_dir, options.dangerous_level);
    // A break still counts the errors met before it.
    if let Some(stop) = result.err().or(stop) {
        totals.file_errors += errors;
        return Err(stop);
    }
    for (path, time) in folder_times.into_iter().rev() {
        let _ = cash_win32::unix::set_times(
            &path,
            &cash_win32::unix::Times {
                modified: system_time(time),
                accessed: None,
                created: None,
            },
        );
    }
    Ok(errors)
}

/// An item's data counted on the progress line as it is read (`SetCompleted`).
struct Shown<'a, 'c, SE: cash_core::ShellExtensions> {
    data: &'a mut dyn Data,
    console: &'a Console<'c, SE>,
    done: &'a mut u64,
}

impl<SE: cash_core::ShellExtensions> Read for Shown<'_, '_, SE> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.data.read(buf)?;
        *self.done = self.done.saturating_add(n as u64);
        let done = *self.done;
        self.console.progress(|s| s.completed = done);
        Ok(n)
    }
}

impl<SE: cash_core::ShellExtensions> Data for Shown<'_, '_, SE> {
    fn finish(&mut self) -> Result<(), Problem> {
        self.data.finish()
    }

    fn encrypted(&self) -> bool {
        self.data.encrypted()
    }

    fn unpacked(&self) -> Option<u64> {
        self.data.unpacked()
    }
}

/// An item's data passing through `-scrc`'s hashes.
struct Hashed<'a> {
    data: &'a mut dyn Data,
    bundle: &'a mut super::hash::Bundle,
}

impl Read for Hashed<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.data.read(buf)?;
        self.bundle.update(&buf[..n]);
        Ok(n)
    }
}

impl Data for Hashed<'_> {
    fn finish(&mut self) -> Result<(), Problem> {
        self.data.finish()
    }

    fn encrypted(&self) -> bool {
        self.data.encrypted()
    }

    fn unpacked(&self) -> Option<u64> {
        self.data.unpacked()
    }
}

fn copy_to_stdout<SE: cash_core::ShellExtensions>(
    console: &Console<'_, SE>,
    data: &mut dyn Data,
) -> io::Result<()> {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match data.read(&mut buf) {
            Ok(0) | Err(_) => return Ok(()),
            Ok(n) => console.data(&buf[..n])?,
        }
    }
}

/// 7-Zip's words for what was wrong with an item's data.
const fn problem_words(problem: Problem, encrypted: bool) -> &'static str {
    match problem {
        Problem::UnsupportedMethod => "Unsupported Method",
        Problem::Crc if encrypted => "CRC Failed in encrypted file. Wrong password?",
        Problem::Crc => "CRC Failed",
        Problem::Data | Problem::PasswordNeeded if encrypted => {
            "Data Error in encrypted file. Wrong password?"
        }
        Problem::Data | Problem::PasswordNeeded => "Data Error",
        Problem::UnexpectedEnd => "Unexpected end of data",
        Problem::DataAfterEnd => "There are some data after the end of the payload data",
        Problem::WrongPassword => "Wrong password",
    }
}

/// Where an item goes on disk: the output folder, then its path (`x`) or its name
/// (`e`), each part made one Windows can hold (`Correct_FsPath`); and that path as
/// shown, relative to the output folder.
fn target(out_dir: &Path, item: &Item, keep: PathKeep) -> (PathBuf, String) {
    let mut parts = split_path(&item.path);
    if keep == PathKeep::None {
        parts = parts.split_off(parts.len().saturating_sub(1));
    }
    let count = parts.len();
    let mut corrected = Vec::new();
    for (i, part) in parts.into_iter().enumerate() {
        let fixed = correct_part(&part);
        if fixed.is_empty() {
            if item.is_dir || i + 1 != count {
                continue;
            }
            corrected.push("_".to_owned());
        } else {
            corrected.push(fixed);
        }
    }
    let mut path = out_dir.to_path_buf();
    for part in &corrected {
        path.push(part);
    }
    (path, corrected.join("/"))
}

/// `Correct_PathPart` and `CorrectUnsupportedName`.
fn correct_part(part: &str) -> String {
    if part == "." || part == ".." {
        return String::new();
    }
    let mut chars: Vec<char> = part
        .chars()
        .map(|c| {
            if matches!(c, ':' | '*' | '?' | '<' | '>' | '|' | '"' | '/' | '\\')
                || (c as u32) < 0x20
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    for c in chars.iter_mut().rev() {
        if *c != '.' && *c != ' ' {
            break;
        }
        *c = '_';
    }
    let mut text: String = chars.into_iter().collect();
    if is_reserved(&text) {
        text.insert(0, '_');
    }
    text
}

fn is_reserved(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    for reserved in ["CON", "PRN", "AUX", "NUL", "COM", "LPT"] {
        let Some(rest) = upper.strip_prefix(reserved) else {
            continue;
        };
        let rest = if matches!(reserved, "COM" | "LPT") {
            match rest.strip_prefix(|c: char| c.is_ascii_digit()) {
                Some(rest) => rest,
                None => continue,
            }
        } else {
            rest
        };
        let rest = rest.trim_start_matches(' ');
        if rest.is_empty() || rest.starts_with('.') {
            return true;
        }
    }
    false
}

/// `AutoRenamePath`: `name_1.ext`, `name_2.ext` …, the first that is free.
fn auto_rename(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    let (stem, ext) = match name.rfind('.') {
        Some(at) if at > 0 => (
            name.get(..at).unwrap_or_default().to_owned(),
            name.get(at..).unwrap_or_default().to_owned(),
        ),
        _ => (name.clone(), String::new()),
    };
    (1..1_000_000)
        .map(|n| parent.join(format!("{stem}_{n}{ext}")))
        .find(|p| fs::symlink_metadata(p).is_err())
}

pub(super) fn system_time(ticks: u64) -> SystemTime {
    let epoch = SystemTime::UNIX_EPOCH - Duration::from_hours(3_234_576);
    epoch + Duration::from_nanos(ticks.saturating_mul(100))
}

/// The answer to the overwrite question.
enum Answer {
    Yes,
    No,
}

/// Writes one file where `make_room` made room for it, then its times and attributes.
fn write_file(item: &Item, path: &Path, data: &mut dyn Data) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match data.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => file.write_all(&buf[..n])?,
        }
    }
    drop(file);
    if let Some(time) = item.modified {
        let _ = cash_win32::unix::set_times(
            path,
            &cash_win32::unix::Times {
                modified: system_time(time),
                accessed: item.accessed.map(system_time),
                created: item.created.map(system_time),
            },
        );
    }
    if let Some(attrib) = item.attrib {
        let settable = attrib & 0x7 | attrib & 0x20;
        if settable != 0x20 {
            let _ = cash_win32::unix::set_attributes(path, settable & 0x27);
        }
    }
    Ok(())
}

/// `CheckExistFile`: when a file is where an item goes, the question (`AskOverwrite`)
/// or `-ao`'s answer, and what was there moved or removed; the path to write, or `None`
/// for an item skipped.
fn make_room<SE: cash_core::ShellExtensions>(
    env: &Env<'_, SE>,
    console: &Console<'_, SE>,
    item: &Item,
    path: &Path,
    shown: &str,
    overwrite: &mut Overwrite,
) -> Result<Option<PathBuf>, Stop> {
    let mut path = path.to_path_buf();
    if let Ok(existing) = fs::symlink_metadata(&path) {
        let mut mode = *overwrite;
        if mode == Overwrite::Ask {
            match ask_overwrite(env, console, item, shown, &existing)? {
                Some((answer, new_mode)) => {
                    if let Some(new_mode) = new_mode {
                        *overwrite = new_mode;
                        mode = new_mode;
                    }
                    if matches!(answer, Answer::No) {
                        return Ok(None);
                    }
                    if mode == Overwrite::Ask {
                        mode = Overwrite::All;
                    }
                }
                None => return Err(Stop::Break),
            }
        }
        match mode {
            Overwrite::Skip => return Ok(None),
            Overwrite::Rename => {
                path = auto_rename(&path)
                    .ok_or_else(|| Stop::Message("Cannot create file with auto name".to_owned()))?;
            }
            Overwrite::RenameExisting => {
                let moved = auto_rename(&path)
                    .ok_or_else(|| Stop::Message("Cannot create file with auto name".to_owned()))?;
                fs::rename(&path, moved)?;
            }
            Overwrite::All | Overwrite::Ask => {
                if existing.is_dir() {
                    fs::remove_dir(&path)?;
                } else {
                    if existing.permissions().readonly() {
                        let _ = cash_win32::unix::set_attributes(&path, 0);
                    }
                    fs::remove_file(&path)?;
                }
            }
        }
    }
    Ok(Some(path))
}

/// `AskOverwrite` and `ScanUserYesNoAllQuit`: the question, then a line until it is
/// one of y, n, a, s, u or q. `None` for q or the end of the input.
fn ask_overwrite<SE: cash_core::ShellExtensions>(
    env: &Env<'_, SE>,
    console: &Console<'_, SE>,
    item: &Item,
    shown: &str,
    existing: &fs::Metadata,
) -> Result<Option<(Answer, Option<Overwrite>)>, Stop> {
    console.flush_so();
    let existing_time = existing
        .modified()
        .ok()
        .and_then(|t| cash_archive::sevenz::NtTime::try_from(t).ok())
        .map(u64::from);
    let mut question = String::from("\nWould you like to replace the existing file:\n");
    question.push_str(&file_info(env, shown, existing_time, Some(existing.len())));
    question.push_str("with the file from archive:\n");
    question.push_str(&file_info(env, &item.path, item.modified, item.size));
    question.push_str("? ");
    console.so(&question);
    loop {
        console.so("(Y)es / (N)o / (A)lways / (S)kip all / A(u)to rename all / (Q)uit? ");
        console.flush_so();
        let Some(line) = read_line(env, true)? else {
            return Ok(None);
        };
        let line = line.trim();
        let answer = match line.to_ascii_lowercase().as_str() {
            "y" => Some((Answer::Yes, None)),
            "n" => Some((Answer::No, None)),
            "a" => Some((Answer::Yes, Some(Overwrite::All))),
            "s" => Some((Answer::No, Some(Overwrite::Skip))),
            "u" => Some((Answer::Yes, Some(Overwrite::Rename))),
            "q" => return Ok(None),
            _ => None,
        };
        if let Some(answer) = answer {
            console.so("\n");
            return Ok(Some(answer));
        }
    }
}

fn file_info<SE: cash_core::ShellExtensions>(
    env: &Env<'_, SE>,
    path: &str,
    time: Option<u64>,
    size: Option<u64>,
) -> String {
    let mut text = format!("  Path:     {path}\n");
    if let Some(size) = size {
        let _ = writeln!(text, "  Size:     {}", text::size_smart(size));
    }
    if let Some(time) = time {
        let _ = writeln!(text, "  Modified: {}", text::time(&env.zone, time, 0));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_windows_cannot_hold_are_corrected() {
        assert_eq!(correct_part("a:b"), "a_b");
        assert_eq!(correct_part("dots..."), "dots___");
        assert_eq!(correct_part("CON"), "_CON");
        assert_eq!(correct_part("com1.txt"), "_com1.txt");
        assert_eq!(correct_part("console"), "console");
        assert_eq!(correct_part(".."), "");
    }

    #[test]
    fn renamed_names_keep_their_extension() {
        let dir = std::env::temp_dir().join(format!("cash-7z-rename-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.txt");
        assert_eq!(auto_rename(&path).unwrap(), dir.join("a_1.txt"));
        std::fs::write(dir.join("a_1.txt"), "").unwrap();
        assert_eq!(auto_rename(&path).unwrap(), dir.join("a_2.txt"));
        assert_eq!(
            auto_rename(&dir.join("empty")).unwrap(),
            dir.join("empty_1")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
