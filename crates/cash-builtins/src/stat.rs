//! `stat` builtin — display file status.

use std::io::Write;
use std::path::{Path, PathBuf};

use cash_core::{ExecutionResult, builtins};
use cash_win32::unix::{Kind, mode_string, permissions};
use clap::Parser;

use std::os::windows::fs::MetadataExt;

/// Display file status.
#[derive(Parser)]
pub(crate) struct StatCommand {
    /// Follow links
    #[arg(short = 'L', long = "dereference")]
    dereference: bool,

    /// Use the specified FORMAT instead of the default; output a newline after each use of FORMAT
    #[arg(short = 'c', long = "format")]
    format: Option<String>,

    /// Like --format, but interpret backslash escapes, and do not output a mandatory trailing newline
    #[arg(long = "printf")]
    printf: Option<String>,

    /// Display file system status instead of file status
    #[arg(short = 'f', long = "file-system")]
    file_system: bool,

    /// Files to stat
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

impl builtins::Command for StatCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut had_error = false;
        let mut stdout = context.stdout();

        for path in &self.files {
            // A builtin runs in the shell's process, whose working folder is not the
            // shell's: a path is resolved against the shell's, as `ls` resolves it.
            let resolved = context.shell.absolute_path(path);

            let info = if let Some(name) = cash_win32::devices::dev_name(&resolved) {
                let Some(info) = dev_info(&context, path, name, self.dereference) else {
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
                let metadata_res = if self.dereference {
                    std::fs::metadata(&resolved)
                } else {
                    std::fs::symlink_metadata(&resolved)
                };

                match metadata_res {
                    Ok(meta) => FileStatInfo::from_path_and_meta(path, &resolved, &meta),
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

            if let Some(format_str) = &self.format {
                let formatted = format_stat(&info, format_str, false);
                writeln!(stdout, "{formatted}")?;
            } else if let Some(printf_str) = &self.printf {
                let formatted = format_stat(&info, printf_str, true);
                write!(stdout, "{formatted}")?;
            } else {
                // Default human-readable report
                writeln!(stdout, "  File: {}", info.name)?;
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
    /// `path` as written, for the report, and as resolved, to ask the file.
    fn from_path_and_meta(written: &Path, path: &Path, meta: &std::fs::Metadata) -> Self {
        let name = written.to_string_lossy().to_string();
        let size = meta.len();
        let blocks = size.div_ceil(512);

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
            kind == Kind::File && cash_win32::unix::is_executable(path),
        );
        let raw_mode = kind.type_bits() | bits;
        let octal_perms = format!("{bits:o}");
        let human_perms = mode_string(raw_mode);
        let file_type = file_type.to_owned();

        let (inode, links, device) = query_file_index_and_links(path);

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

        let (atime, mtime, btime) = {
            let a = filetime_to_unix(meta.last_access_time());
            let m = filetime_to_unix(meta.last_write_time());
            let b = filetime_to_unix(meta.creation_time());
            (a, m, b)
        };

        let ctime = mtime; // Windows does not have a separate inode change time

        Self {
            name,
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
            access_time_secs: atime,
            access_time_str: format_unix_time(atime),
            modify_time_secs: mtime,
            modify_time_str: format_unix_time(mtime),
            change_time_secs: ctime,
            change_time_str: format_unix_time(ctime),
            birth_time_secs: btime,
            birth_time_str: format_unix_time(btime),
            device_type: None,
        }
    }

    /// What `stat` says of a `/dev` name that is no file (D7): of the type and mode
    /// given, empty, with one link, owned by the user, and as new as now.
    fn special(name: &Path, file_type: &str, raw_mode: u32) -> Self {
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        Self {
            name: name.to_string_lossy().to_string(),
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
            access_time_str: format_unix_time(now),
            modify_time_secs: now,
            modify_time_str: format_unix_time(now),
            change_time_secs: now,
            change_time_str: format_unix_time(now),
            birth_time_secs: now,
            birth_time_str: format_unix_time(now),
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

    let DevName::Descriptor(number) = name else {
        let mut info = FileStatInfo::special(written, "character special file", 0o020_666);
        info.device_type = name.numbers();
        return Some(info);
    };
    let open = context.try_fd(cash_core::ShellFd::try_from(number).ok()?)?;
    if !dereference {
        return Some(FileStatInfo::special(written, "symbolic link", 0o120_777));
    }
    Some(match open.kind() {
        FileKind::Pipe { reads, writes } => {
            let mode = 0o010_000 | if reads { 0o400 } else { 0 } | if writes { 0o200 } else { 0 };
            FileStatInfo::special(written, "fifo", mode)
        }
        FileKind::File(metadata) => FileStatInfo::from_path_and_meta(written, written, &metadata),
        FileKind::Socket => FileStatInfo::special(written, "socket", 0o140_777),
        FileKind::Device | FileKind::Other => {
            FileStatInfo::special(written, "character special file", 0o020_666)
        }
    })
}

/// The inode, links and device `stat` shows. A folder's are its own: it could not be
/// opened without backup semantics, and was inode 0 on device 0 (ARCH-06).
fn query_file_index_and_links(path: &Path) -> (u64, u32, u32) {
    cash_win32::fs::file_info(path).map_or((0, 1, 0), |info| (info.index, info.links, info.volume))
}

const fn filetime_to_unix(filetime: u64) -> u64 {
    // 100-ns intervals between 1601-01-01 and 1970-01-01 is 116444736000000000
    const UNIX_EPOCH_FILETIME: u64 = 116_444_736_000_000_000;
    filetime.saturating_sub(UNIX_EPOCH_FILETIME) / 10_000_000
}

fn format_unix_time(secs: u64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let mut days = secs / 86400;

    let mut year = 1970;
    loop {
        let leap = is_leap_year(year);
        let days_in_year = if leap { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let days_in_months: [u64; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for &dim in &days_in_months {
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }
    let day = days + 1;

    format!("{year:04}-{month:02}-{day:02} {h:02}:{m:02}:{s:02}.000000000 +0000")
}

const fn is_leap_year(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

fn format_stat(info: &FileStatInfo, spec: &str, interpret_escapes: bool) -> String {
    use std::fmt::Write as _;

    let mut result = String::new();
    let chars: Vec<char> = spec.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if interpret_escapes && chars[i] == '\\' && i + 1 < chars.len() {
            match chars[i + 1] {
                'n' => result.push('\n'),
                't' => result.push('\t'),
                'r' => result.push('\r'),
                '\\' => result.push('\\'),
                other => {
                    result.push('\\');
                    result.push(other);
                }
            }
            i += 2;
            continue;
        }

        if chars[i] == '%' && i + 1 < chars.len() {
            i += 1;
            match chars[i] {
                '%' => result.push('%'),
                'n' => result.push_str(&info.name),
                'N' => {
                    result.push('\'');
                    result.push_str(&info.name);
                    result.push('\'');
                }
                's' => result.push_str(&info.size.to_string()),
                'b' => result.push_str(&info.blocks.to_string()),
                'B' => result.push_str("512"),
                'o' => result.push_str("4096"),
                'f' => {
                    let _ = write!(result, "{:x}", info.raw_mode);
                }
                'a' => result.push_str(&info.octal_perms),
                'A' => result.push_str(&info.human_perms),
                'F' => result.push_str(&info.file_type),
                // A device's numbers, in hex as GNU's: 0 for a file.
                't' => {
                    let _ = write!(
                        result,
                        "{:x}",
                        info.device_type.map_or(0, |(major, _)| major)
                    );
                }
                'T' => {
                    let _ = write!(
                        result,
                        "{:x}",
                        info.device_type.map_or(0, |(_, minor)| minor)
                    );
                }
                'h' => result.push_str(&info.links.to_string()),
                'i' => result.push_str(&info.inode.to_string()),
                'd' => result.push_str(&info.device.to_string()),
                'D' => {
                    let _ = write!(result, "{:x}", info.device);
                }
                'u' => result.push_str(&info.uid.to_string()),
                'U' => result.push_str(&info.user),
                'g' => result.push_str(&info.gid.to_string()),
                'G' => result.push_str(&info.group),
                'x' => result.push_str(&info.access_time_str),
                'X' => result.push_str(&info.access_time_secs.to_string()),
                'y' => result.push_str(&info.modify_time_str),
                'Y' => result.push_str(&info.modify_time_secs.to_string()),
                'z' => result.push_str(&info.change_time_str),
                'Z' => result.push_str(&info.change_time_secs.to_string()),
                'w' => result.push_str(&info.birth_time_str),
                'W' => result.push_str(&info.birth_time_secs.to_string()),
                other => {
                    result.push('%');
                    result.push(other);
                }
            }
            i += 1;
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}
