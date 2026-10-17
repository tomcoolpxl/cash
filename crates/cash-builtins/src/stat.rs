//! `stat` builtin — display file status.

use std::io::Write;
use std::path::{Path, PathBuf};

use cash_core::timefmt::Zone;
use cash_core::{ExecutionResult, builtins};
use cash_getopt::{Arg, Getopt, Item, Long, Order, Short};
use cash_win32::unix::{Kind, mode_string, permissions};
use clap::Parser;

use std::os::windows::fs::MetadataExt;

/// Display file or file system status.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct StatCommand {
    /// Options and files, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// GNU coreutils 9's `c:fLt` and its long names.
const SHORTS: &[Short<&str>] = &[
    Short::new('c', Arg::Required, "format"),
    Short::new('f', Arg::No, "file-system"),
    Short::new('L', Arg::No, "dereference"),
    Short::new('t', Arg::No, "terse"),
];
const LONGS: &[Long<'static, &str>] = &[
    Long::new("dereference", Arg::No, "dereference"),
    Long::new("file-system", Arg::No, "file-system"),
    Long::new("format", Arg::Required, "format"),
    Long::new("printf", Arg::Required, "printf"),
    Long::new("terse", Arg::No, "terse"),
    Long::new("cached", Arg::Required, "cached"),
    Long::new("help", Arg::No, "help"),
    Long::new("version", Arg::No, "version"),
];

/// GNU's terse forms, for a file and for a file system.
const TERSE_FILE: &str = "%n %s %b %f %u %g %D %i %h %t %T %X %Y %Z %W %o";
const TERSE_FILE_SYSTEM: &str = "%n %i %l %t %s %S %b %f %a %c %d";

const HELP: &str = "\
Usage: stat [OPTION]... FILE...
Display file or file system status.

Mandatory arguments to long options are mandatory for short options too.
  -L, --dereference     follow links
  -f, --file-system     display file system status instead of file status
      --cached=MODE     specify how to use cached attributes;
                          useful on remote file systems. See MODE below
  -c  --format=FORMAT   use the specified FORMAT instead of the default;
                          output a newline after each use of FORMAT
      --printf=FORMAT   like --format, but interpret backslash escapes,
                          and do not output a mandatory trailing newline;
                          if you want a newline, include \\n in FORMAT
  -t, --terse           print the information in terse form
      --help        display this help and exit
      --version     output version information and exit

The MODE argument of --cached can be: always, never, or default; Windows
keeps no attributes to choose between, so it changes nothing.

The valid format sequences for files (without --file-system):
  %a %A  permissions in octal, in human readable form
  %b %B  number of blocks allocated, the size of each (512)
  %d %D  device number in decimal, in hex
  %f     raw mode in hex       %F  file type
  %g %G  group ID, name        %h  number of hard links
  %i     inode number (the file's index on its volume)
  %n %N  file name, quoted file name
  %o     optimal I/O transfer size hint
  %s     total size, in bytes
  %t %T  major and minor device type in hex, for a device
  %u %U  user ID, name
  %w %W  time of file birth, human-readable; in seconds since Epoch
  %x %X  time of last access, human-readable; in seconds since Epoch
  %y %Y  time of last data modification, human-readable; in seconds since Epoch
  %z %Z  time of last status change, human-readable; in seconds since Epoch

Valid format sequences for file systems:
  %a  free blocks available to non-superuser
  %b  total data blocks in file system
  %c  total file nodes in file system (0: Windows does not count them)
  %d  free file nodes in file system (0)
  %f  free blocks in file system
  %i  file system ID in hex
  %l  maximum length of filenames
  %n  file name
  %s  block size (for faster transfers)
  %S  fundamental block size (for block counts)
  %t  file system type in hex
  %T  file system type in human readable form";

/// What the command line asked for.
#[derive(Debug, Default)]
struct Options {
    dereference: bool,
    file_system: bool,
    terse: bool,
    format: Option<String>,
    printf: Option<String>,
    files: Vec<String>,
}

/// Why `stat` ends before it looks at a file.
#[derive(Debug, PartialEq, Eq)]
enum Stop {
    Help,
    Version,
    /// A mistake: said with GNU's Try line, status 1.
    Usage(String),
}

/// Reads the command line as GNU's `getopt_long` loop does, in order, the options anywhere
/// until `--` unless `order` says otherwise.
fn parse(args: &[String], order: Order) -> Result<Options, Stop> {
    let mut options = Options::default();
    for item in Getopt::new(SHORTS, LONGS).order(order).read(args) {
        match item.map_err(|problem| Stop::Usage(problem.to_string()))? {
            Item::Operand { value, .. } => options.files.push(value),
            Item::Option { id, value, .. } => match id {
                "dereference" => options.dereference = true,
                "file-system" => options.file_system = true,
                "terse" => options.terse = true,
                "format" => options.format = value,
                "printf" => options.printf = value,
                "cached" => match value.as_deref() {
                    Some("always" | "never" | "default") => {}
                    other => {
                        return Err(Stop::Usage(format!(
                            "invalid argument '{}' for '--cached'\n\
                             Valid arguments are:\n  - 'default'\n  - 'never'\n  - 'always'",
                            other.unwrap_or_default()
                        )));
                    }
                },
                "help" => return Err(Stop::Help),
                _ => return Err(Stop::Version),
            },
        }
    }
    if options.files.is_empty() {
        return Err(Stop::Usage("missing operand".to_owned()));
    }
    Ok(options)
}

impl builtins::Command for StatCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let options = match parse(&self.args, crate::options::gnu_order(&context)) {
            Ok(options) => options,
            Err(Stop::Help) => {
                writeln!(context.stdout(), "{HELP}")?;
                return Ok(ExecutionResult::success());
            }
            Err(Stop::Version) => {
                writeln!(context.stdout(), "stat (cash): GNU coreutils' options")?;
                return Ok(ExecutionResult::success());
            }
            Err(Stop::Usage(message)) => {
                return crate::options::gnu_usage_error(&context, "stat", message, 1);
            }
        };
        if options.file_system {
            return file_systems(&context, &options);
        }
        let mut had_error = false;
        let mut stdout = context.stdout();

        for path in options.files.iter().map(PathBuf::from) {
            let path = &path;
            // A builtin runs in the shell's process, whose working folder is not the
            // shell's: a path is resolved against the shell's, as `ls` resolves it.
            let resolved = context.shell.absolute_path(path);

            let info = if let Some(name) = cash_win32::devices::dev_name(&resolved) {
                let Some(info) = dev_info(&context, path, name, options.dereference) else {
                    writeln!(
                        context.stderr(),
                        "stat: cannot stat '{}': No such file or directory",
                        path.display()
                    )?;
                    had_error = true;
                    continue;
                };
                info
            } else {
                let metadata_res = if options.dereference {
                    std::fs::metadata(&resolved)
                } else {
                    std::fs::symlink_metadata(&resolved)
                };

                match metadata_res {
                    Ok(meta) => FileStatInfo::from_path_and_meta(
                        path,
                        &resolved,
                        &meta,
                        &Zone::of_shell(context.shell),
                    ),
                    Err(e) => {
                        writeln!(
                            context.stderr(),
                            "stat: cannot stat '{}': {}",
                            path.display(),
                            e
                        )?;
                        had_error = true;
                        continue;
                    }
                }
            };

            if let Some(format_str) = &options.format {
                let formatted = format_stat(&info, format_str, false);
                writeln!(stdout, "{formatted}")?;
            } else if options.terse {
                writeln!(stdout, "{}", format_stat(&info, TERSE_FILE, false))?;
            } else if let Some(printf_str) = &options.printf {
                let formatted = format_stat(&info, printf_str, true);
                write!(stdout, "{formatted}")?;
            } else {
                // Default human-readable report
                writeln!(stdout, "  File: {}", info.shown_name())?;
                writeln!(
                    stdout,
                    "  Size: {:<10} Blocks: {:<10} IO Block: 4096   {}",
                    info.size, info.blocks, info.file_type
                )?;
                write!(
                    stdout,
                    "Device: {:x}h/{}d\tInode: {:<16} Links: {}",
                    info.device, info.device, info.inode, info.links
                )?;
                if let Some((major, minor)) = info.device_type {
                    write!(stdout, "     Device type: {major},{minor}")?;
                }
                writeln!(stdout)?;
                writeln!(
                    stdout,
                    "Access: (0{}/{})  Uid: ({:>5}/{:>8})   Gid: ({:>5}/{:>8})",
                    info.octal_perms, info.human_perms, info.uid, info.user, info.gid, info.group
                )?;
                writeln!(stdout, "Access: {}", info.access_time_str)?;
                writeln!(stdout, "Modify: {}", info.modify_time_str)?;
                writeln!(stdout, "Change: {}", info.change_time_str)?;
                writeln!(stdout, " Birth: {}", info.birth_time_str)?;
            }
        }

        if had_error {
            Ok(ExecutionResult::general_error())
        } else {
            Ok(ExecutionResult::success())
        }
    }
}

struct FileStatInfo {
    name: String,
    /// A link's target, in cash's spelling (`C:/…`), as `readlink` gives it.
    link_target: Option<String>,
    size: u64,
    blocks: u64,
    raw_mode: u32,
    octal_perms: String,
    human_perms: String,
    file_type: String,
    links: u32,
    inode: u64,
    device: u32,
    user: String,
    uid: u32,
    group: String,
    gid: u32,
    access_time_secs: u64,
    access_time_str: String,
    modify_time_secs: u64,
    modify_time_str: String,
    change_time_secs: u64,
    change_time_str: String,
    birth_time_secs: u64,
    birth_time_str: String,
    /// A device's numbers, major and minor (`%t`, `%T`).
    device_type: Option<(u32, u32)>,
}

impl FileStatInfo {
    /// The name the report's `File:` line shows: a link's with ` -> ` and its target.
    fn shown_name(&self) -> String {
        self.link_target.as_ref().map_or_else(
            || self.name.clone(),
            |target| format!("{} -> {target}", self.name),
        )
    }

    /// `path` as written, for the report, and as resolved, to ask the file; its times shown
    /// in `zone`.
    fn from_path_and_meta(
        written: &Path,
        path: &Path,
        meta: &std::fs::Metadata,
        zone: &Zone,
    ) -> Self {
        let name = written.to_string_lossy().to_string();
        // NTFS's facts of the file: its change time and the space the volume gives it.
        let facts = cash_win32::fs::file_facts(path, !meta.is_symlink());
        // The blocks of 512 bytes (`%B`) the volume gives it, as `st_blocks` counts them,
        // so `%b` times `%B` is its space on the disk, as Git Bash's stat gives it; the
        // size's where the facts cannot be read. They were the size's always: 10 for a
        // file of 5000 bytes on 8 KiB.
        let blocks = facts
            .as_ref()
            .map_or_else(|| meta.len(), |facts| facts.allocation)
            .div_ceil(512);
        let link_target = meta
            .is_symlink()
            .then(|| std::fs::read_link(path).ok())
            .flatten()
            .map(|target| cash_win32::path::render(&target));
        // A link's size is its target's length, as Git Bash's stat shows it.
        let size = link_target
            .as_ref()
            .map_or_else(|| meta.len(), |target| target.len() as u64);

        let (kind, file_type) = if meta.is_dir() {
            (Kind::Dir, "directory")
        } else if meta.is_symlink() {
            (Kind::Symlink, "symbolic link")
        } else if size == 0 {
            (Kind::File, "regular empty file")
        } else {
            (Kind::File, "regular file")
        };
        // The mode by the rule `ls -l` shows, `cash_win32::unix`'s: the access list
        // decides `w`, a program or a folder gets `x`.
        let security = cash_win32::fs::file_security(path);
        let bits = permissions(
            kind,
            security.writable,
            meta.permissions().readonly(),
            kind == Kind::File && cash_win32::unix::is_executable_keeping_access_time(path),
        );
        let raw_mode = kind.type_bits() | bits;
        let octal_perms = format!("{bits:o}");
        let human_perms = mode_string(raw_mode);
        let file_type = file_type.to_owned();

        let (inode, links, device) = query_file_index_and_links(path, kind == Kind::Dir);

        // The owner SID from the file's security descriptor, its RID the uid, which is
        // what `id -u` reports for that account; only when neither the file's nor the
        // process's SID can be read, 65534 (`nobody`) rather than a number that would
        // claim some real account (0 is root).
        let (user, uid) = security
            .owner_account
            .clone()
            .or_else(cash_win32::fs::current_owner)
            .map_or_else(
                || (cash_win32::fs::current_user(), 65534),
                |owner| (owner.name, owner.rid),
            );
        // The file's primary group, as `ls -l` shows it; the owner where it cannot be read.
        let (group, gid) = security
            .group
            .map_or_else(|| (user.clone(), uid), |group| (group.name, group.rid));

        let (atime, mtime, btime) = (
            meta.last_access_time(),
            meta.last_write_time(),
            meta.creation_time(),
        );

        // NTFS keeps a change time of its own, of the contents, the name or the attributes,
        // which Git Bash's stat shows as Change, as `ls -c` does; the modification time
        // where it cannot be read. It was always the modification time.
        let ctime = facts
            .map(|facts| facts.change_time)
            .filter(|&time| time != 0)
            .unwrap_or(mtime);

        Self {
            name,
            link_target,
            size,
            blocks,
            raw_mode,
            octal_perms,
            human_perms,
            file_type,
            links,
            inode,
            device,
            user,
            uid,
            group,
            gid,
            access_time_secs: filetime_to_unix(atime),
            access_time_str: human_time(filetime_to_utc(atime), zone),
            modify_time_secs: filetime_to_unix(mtime),
            modify_time_str: human_time(filetime_to_utc(mtime), zone),
            change_time_secs: filetime_to_unix(ctime),
            change_time_str: human_time(filetime_to_utc(ctime), zone),
            birth_time_secs: filetime_to_unix(btime),
            birth_time_str: human_time(filetime_to_utc(btime), zone),
            device_type: None,
        }
    }

    /// What `stat` says of a `/dev` name that is no file (D7): of the type and mode
    /// given, empty, with one link, owned by the user, and as new as now, shown in `zone`.
    fn special(name: &Path, file_type: &str, raw_mode: u32, zone: &Zone) -> Self {
        let perms = raw_mode & 0o777;
        let letter = match raw_mode & 0o170_000 {
            0o020_000 => 'c',
            0o010_000 => 'p',
            0o120_000 => 'l',
            _ => '-',
        };
        let human: String = std::iter::once(letter)
            .chain((0..9).rev().map(|bit| {
                if perms & (1 << bit) == 0 {
                    '-'
                } else {
                    ['x', 'w', 'r'][bit % 3]
                }
            }))
            .collect();
        let (user, uid) = cash_win32::fs::current_owner().map_or_else(
            || (cash_win32::fs::current_user(), 65534),
            |owner| (owner.name, owner.rid),
        );
        let clock = chrono::Utc::now();
        let now = u64::try_from(clock.timestamp()).unwrap_or(0);
        let shown = human_time(clock, zone);
        Self {
            name: name.to_string_lossy().to_string(),
            link_target: None,
            size: 0,
            blocks: 0,
            raw_mode,
            octal_perms: format!("{perms:o}"),
            human_perms: human,
            file_type: file_type.to_string(),
            links: 1,
            inode: 0,
            device: 0,
            group: user.clone(),
            gid: uid,
            user,
            uid,
            access_time_secs: now,
            access_time_str: shown.clone(),
            modify_time_secs: now,
            modify_time_str: shown.clone(),
            change_time_secs: now,
            change_time_str: shown.clone(),
            birth_time_secs: now,
            birth_time_str: shown,
            device_type: None,
        }
    }
}

/// What `stat` says of the `/dev` name `name` (D7), as Git Bash 5.3 says it; `None` for
/// a descriptor that is not open.
///
/// A device is a character special file, read and written by all, with Linux's numbers.
/// A descriptor's name is a link to `/proc/self/fd/N`, and with `-L` what is open under
/// the number: a fifo, read or written as its end is; a file, as the file is; a device.
fn dev_info<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    written: &Path,
    name: cash_win32::devices::DevName,
    dereference: bool,
) -> Option<FileStatInfo> {
    use cash_core::openfiles::FileKind;
    use cash_win32::devices::DevName;

    let zone = Zone::of_shell(context.shell);
    let DevName::Descriptor(number) = name else {
        let mut info = FileStatInfo::special(written, "character special file", 0o020_666, &zone);
        info.device_type = name.numbers();
        return Some(info);
    };
    let open = context.try_fd(cash_core::ShellFd::try_from(number).ok()?)?;
    if !dereference {
        return Some(FileStatInfo::special(
            written,
            "symbolic link",
            0o120_777,
            &zone,
        ));
    }
    Some(match open.kind() {
        FileKind::Pipe { reads, writes } => {
            let mode = 0o010_000 | if reads { 0o400 } else { 0 } | if writes { 0o200 } else { 0 };
            FileStatInfo::special(written, "fifo", mode, &zone)
        }
        FileKind::File(metadata) => {
            FileStatInfo::from_path_and_meta(written, written, &metadata, &zone)
        }
        FileKind::Socket => FileStatInfo::special(written, "socket", 0o140_777, &zone),
        FileKind::Device | FileKind::Other => {
            FileStatInfo::special(written, "character special file", 0o020_666, &zone)
        }
    })
}

/// The inode, links and device `stat` shows. A folder's are its own: it could not be
/// opened without backup semantics, and was inode 0 on device 0 (ARCH-06). A folder's
/// links are counted, 2 and one for each folder in it, as on Linux: `ls -l` shows 1, the
/// value for "not counted", since it would have to read every folder it lists, so this is
/// where the count is found.
fn query_file_index_and_links(path: &Path, is_dir: bool) -> (u64, u32, u32) {
    let (index, links, volume) = cash_win32::fs::file_info(path)
        .map_or((0, 1, 0), |info| (info.index, info.links, info.volume));
    let links = if is_dir {
        cash_win32::fs::folder_link_count(path)
    } else {
        links
    };
    (index, links, volume)
}

const fn filetime_to_unix(filetime: u64) -> u64 {
    // 100-ns intervals between 1601-01-01 and 1970-01-01 is 116444736000000000
    const UNIX_EPOCH_FILETIME: u64 = 116_444_736_000_000_000;
    filetime.saturating_sub(UNIX_EPOCH_FILETIME) / 10_000_000
}

/// A time Windows keeps, 100-ns ticks since 1601, as a time chrono knows: one before
/// 1970 too, which [`filetime_to_unix`] makes 0.
fn filetime_to_utc(filetime: u64) -> chrono::DateTime<chrono::Utc> {
    const UNIX_EPOCH_FILETIME: i64 = 116_444_736_000_000_000;
    let since = i64::try_from(filetime)
        .unwrap_or(i64::MAX)
        .saturating_sub(UNIX_EPOCH_FILETIME);
    let nanos = u32::try_from(since.rem_euclid(10_000_000) * 100).unwrap_or(0);
    chrono::DateTime::from_timestamp(since.div_euclid(10_000_000), nanos).unwrap_or_default()
}

/// A time as GNU stat shows it, in the zone `TZ` names, else Windows': to the nanosecond,
/// with the zone's offset, as Git Bash's stat shows it to the 100 ns NTFS keeps.
fn human_time(when: chrono::DateTime<chrono::Utc>, zone: &Zone) -> String {
    zone.format(when, "%Y-%m-%d %H:%M:%S.%f %z")
}

/// `spec` with each `%X` replaced by `field(X)` (an unknown one kept as written) and,
/// under `--printf`, its backslash escapes read.
fn expand(spec: &str, interpret_escapes: bool, field: impl Fn(char) -> Option<String>) -> String {
    let mut result = String::new();
    let mut chars = spec.chars().peekable();
    while let Some(c) = chars.next() {
        if interpret_escapes && c == '\\' && chars.peek().is_some() {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some('\\') => result.push('\\'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => {}
            }
            continue;
        }
        if c == '%'
            && let Some(letter) = chars.next()
        {
            if letter == '%' {
                result.push('%');
            } else if let Some(text) = field(letter) {
                result.push_str(&text);
            } else {
                result.push('%');
                result.push(letter);
            }
            continue;
        }
        result.push(c);
    }
    result
}

fn format_stat(info: &FileStatInfo, spec: &str, interpret_escapes: bool) -> String {
    expand(spec, interpret_escapes, |letter| file_field(info, letter))
}

/// The text of a file's `%X`, or `None` for a letter that names nothing.
fn file_field(info: &FileStatInfo, letter: char) -> Option<String> {
    Some(match letter {
        'n' => info.name.clone(),
        'N' => match &info.link_target {
            Some(target) => format!("'{}' -> '{target}'", info.name),
            None => format!("'{}'", info.name),
        },
        's' => info.size.to_string(),
        'b' => info.blocks.to_string(),
        'B' => "512".to_owned(),
        'o' => "4096".to_owned(),
        'f' => format!("{:x}", info.raw_mode),
        'a' => info.octal_perms.clone(),
        'A' => info.human_perms.clone(),
        'F' => info.file_type.clone(),
        // A device's numbers, in hex as GNU's: 0 for a file.
        't' => format!("{:x}", info.device_type.map_or(0, |(major, _)| major)),
        'T' => format!("{:x}", info.device_type.map_or(0, |(_, minor)| minor)),
        'h' => info.links.to_string(),
        'i' => info.inode.to_string(),
        'd' => info.device.to_string(),
        'D' => format!("{:x}", info.device),
        'u' => info.uid.to_string(),
        'U' => info.user.clone(),
        'g' => info.gid.to_string(),
        'G' => info.group.clone(),
        'x' => info.access_time_str.clone(),
        'X' => info.access_time_secs.to_string(),
        'y' => info.modify_time_str.clone(),
        'Y' => info.modify_time_secs.to_string(),
        'z' => info.change_time_str.clone(),
        'Z' => info.change_time_secs.to_string(),
        'w' => info.birth_time_str.clone(),
        'W' => info.birth_time_secs.to_string(),
        _ => return None,
    })
}

/// A volume's file system as GNU names it, and the magic number Linux gives it (0 for one
/// it has none for).
fn file_system_kind(name: &str) -> (String, u32) {
    match name.to_ascii_uppercase().as_str() {
        "NTFS" => ("ntfs".to_owned(), 0x5346_544e),
        "FAT" | "FAT32" => ("msdos".to_owned(), 0x4d44),
        "EXFAT" => ("exfat".to_owned(), 0x2011_bab0),
        "CDFS" => ("isofs".to_owned(), 0x9660),
        "UDF" => ("udf".to_owned(), 0x1501_3346),
        _ => (name.to_ascii_lowercase(), 0),
    }
}

/// The text of a file system's `%X`, for the file named `name`.
fn file_system_field(
    facts: &cash_win32::sysinfo::VolumeFacts,
    name: &str,
    letter: char,
) -> Option<String> {
    let (kind, magic) = file_system_kind(&facts.file_system);
    Some(match letter {
        'n' => name.to_owned(),
        'a' => facts.available_blocks.to_string(),
        'b' => facts.total_blocks.to_string(),
        // Windows counts no file nodes.
        'c' | 'd' => "0".to_owned(),
        'f' => facts.free_blocks.to_string(),
        'i' => format!("{:x}", u64::from(facts.serial) << 32),
        'l' => facts.name_max.to_string(),
        's' | 'S' => facts.block_size.to_string(),
        't' => format!("{magic:x}"),
        'T' => kind,
        _ => return None,
    })
}

/// `stat -f`: the file system each file is on, as GNU reports it.
fn file_systems<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    options: &Options,
) -> Result<ExecutionResult, cash_core::Error> {
    let mut status = ExecutionResult::success();
    let mut stdout = context.stdout();
    for name in &options.files {
        // Windows finds a path's volume from its spelling, whether or not it exists; GNU
        // looks at the file, which must be there.
        let resolved = context.shell.absolute_path(Path::new(name));
        let facts = std::fs::metadata(&resolved)
            .map_err(|error| cash_core::error::os_error_text(&error))
            .and_then(|_| {
                cash_win32::sysinfo::volume_facts(&resolved)
                    .ok_or_else(|| "No such file or directory".to_owned())
            });
        let facts = match facts {
            Ok(facts) => facts,
            Err(reason) => {
                writeln!(
                    context.stderr(),
                    "stat: cannot read file system information for '{name}': {reason}"
                )?;
                status = ExecutionResult::general_error();
                continue;
            }
        };
        let field = |letter| file_system_field(&facts, name, letter);
        if let Some(format) = &options.format {
            writeln!(stdout, "{}", expand(format, false, field))?;
        } else if let Some(format) = &options.printf {
            write!(stdout, "{}", expand(format, true, field))?;
        } else if options.terse {
            writeln!(stdout, "{}", expand(TERSE_FILE_SYSTEM, false, field))?;
        } else {
            let (kind, _) = file_system_kind(&facts.file_system);
            writeln!(
                stdout,
                "  File: \"{name}\"\n    ID: {:<8} Namelen: {:<7} Type: {kind}\n\
                 Block size: {:<10} Fundamental block size: {}\n\
                 Blocks: Total: {:<10} Free: {:<10} Available: {}\n\
                 Inodes: Total: {:<10} Free: {}",
                format!("{:x}", u64::from(facts.serial) << 32),
                facts.name_max,
                facts.block_size,
                facts.block_size,
                facts.total_blocks,
                facts.free_blocks,
                facts.available_blocks,
                0,
                0
            )?;
        }
    }
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(line: &str) -> Result<Options, Stop> {
        let words: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        parse(&words, Order::Permute)
    }

    #[test]
    fn the_command_line_is_read_as_gnu_stat_reads_it() {
        let options = parsed("-L f -t --cached=never --form=%n g").unwrap();
        assert!(options.dereference && options.terse);
        assert_eq!(options.format.as_deref(), Some("%n"));
        assert_eq!(options.files, ["f", "g"]);
        assert!(parsed("-f .").unwrap().file_system);
        assert_eq!(
            parsed("").err(),
            Some(Stop::Usage("missing operand".to_owned()))
        );
        assert_eq!(
            parsed("-c").err(),
            Some(Stop::Usage("option requires an argument -- 'c'".to_owned()))
        );
        assert!(matches!(parsed("--cached=x f"), Err(Stop::Usage(_))));
        assert_eq!(parsed("--help --bogus").err(), Some(Stop::Help));
        assert_eq!(parsed("--vers").err(), Some(Stop::Version));
    }

    #[test]
    fn a_format_keeps_what_it_does_not_know() {
        let field = |letter| (letter == 'n').then(|| "name".to_owned());
        assert_eq!(expand("%n %% %q", false, field), "name % %q");
        assert_eq!(expand("%n\\t.", true, field), "name\t.");
        assert_eq!(expand("%n\\t.", false, field), "name\\t.");
        assert_eq!(file_system_kind("NTFS"), ("ntfs".to_owned(), 0x5346_544e));
        assert_eq!(file_system_kind("ReFS"), ("refs".to_owned(), 0));
    }
}
