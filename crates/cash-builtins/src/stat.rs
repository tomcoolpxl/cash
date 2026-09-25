//! `stat` builtin — display file status.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

#[cfg(windows)]
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
            let metadata_res = if self.dereference {
                std::fs::metadata(path)
            } else {
                std::fs::symlink_metadata(path)
            };

            let meta = match metadata_res {
                Ok(m) => m,
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
            };

            let info = FileStatInfo::from_path_and_meta(path, &meta);

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
                writeln!(
                    stdout,
                    "Device: {:x}h/{}d\tInode: {:<16} Links: {}",
                    info.device, info.device, info.inode, info.links
                )?;
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
}

impl FileStatInfo {
    fn from_path_and_meta(path: &Path, meta: &std::fs::Metadata) -> Self {
        let name = path.to_string_lossy().to_string();
        let size = meta.len();
        let blocks = size.div_ceil(512);

        let (file_type, raw_mode, octal_perms, human_perms) = if meta.is_dir() {
            (
                "directory".to_string(),
                0o040_755,
                "755".to_string(),
                "drwxr-xr-x".to_string(),
            )
        } else if meta.is_symlink() {
            (
                "symbolic link".to_string(),
                0o120_777,
                "777".to_string(),
                "lrwxrwxrwx".to_string(),
            )
        } else {
            let readonly = meta.permissions().readonly();
            let is_exe = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("exe")
                        || ext.eq_ignore_ascii_case("bat")
                        || ext.eq_ignore_ascii_case("cmd")
                });

            let ftype = if size == 0 {
                "regular empty file".to_string()
            } else {
                "regular file".to_string()
            };

            if readonly {
                (
                    ftype,
                    0o100_444,
                    "444".to_string(),
                    "-r--r--r--".to_string(),
                )
            } else if is_exe {
                (
                    ftype,
                    0o100_755,
                    "755".to_string(),
                    "-rwxr-xr-x".to_string(),
                )
            } else {
                (
                    ftype,
                    0o100_644,
                    "644".to_string(),
                    "-rw-r--r--".to_string(),
                )
            }
        };

        #[cfg(windows)]
        let (inode, links, device) = query_file_index_and_links(path);
        #[cfg(not(windows))]
        let (inode, links, device) = (0u64, 1u32, 0u32);

        let (user, uid) = file_owner(path);
        // Windows files have no POSIX group, so the group is the owner, as in `ls -l`
        // and `id -g`.
        let group = user.clone();
        let gid = uid;

        #[cfg(windows)]
        let (atime, mtime, btime) = {
            let a = filetime_to_unix(meta.last_access_time());
            let m = filetime_to_unix(meta.last_write_time());
            let b = filetime_to_unix(meta.creation_time());
            (a, m, b)
        };
        #[cfg(not(windows))]
        let (atime, mtime, btime) = (0, 0, 0);

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
        }
    }
}

/// The file's owner name and uid.
///
/// cash: the owner came from `%USERNAME%` with a fixed uid of 1000, so a file owned by
/// another account — or by BUILTIN\Administrators, as an elevated admin's files are on
/// Windows Server — was reported as the current user's. The owner SID is read from the
/// file's security descriptor instead, as `ls -l` does; the uid is its RID, which is
/// what `id -u` reports for that account.
fn file_owner(path: &Path) -> (String, u32) {
    let owner = cash_win32::fs::get_file_owner_info(path).or_else(cash_win32::fs::current_owner);
    // Only when neither the file's nor the process's SID can be read: 65534 is `nobody`,
    // rather than a number that would claim some real account (0 is root).
    owner.map_or_else(
        || (cash_win32::fs::current_user(), 65534),
        |owner| (owner.name, owner.rid),
    )
}

#[cfg(windows)]
fn query_file_index_and_links(path: &Path) -> (u64, u32, u32) {
    use std::os::windows::io::AsRawHandle;

    if let Ok(file) = File::open(path) {
        #[repr(C)]
        struct ByHandleFileInformation {
            dw_file_attributes: u32,
            ft_creation_time: [u32; 2],
            ft_last_access_time: [u32; 2],
            ft_last_write_time: [u32; 2],
            dw_volume_serial_number: u32,
            n_file_size_high: u32,
            n_file_size_low: u32,
            n_number_of_links: u32,
            n_file_index_high: u32,
            n_file_index_low: u32,
        }
        unsafe extern "system" {
            fn GetFileInformationByHandle(
                h_file: *mut std::ffi::c_void,
                lp_file_information: *mut ByHandleFileInformation,
            ) -> i32;
        }
        // SAFETY: `ByHandleFileInformation` is a `#[repr(C)]` struct consisting
        // solely of integer fields, for which the all-zero bit pattern is valid.
        let mut info: ByHandleFileInformation = unsafe { std::mem::zeroed() };
        // SAFETY: `file` is open for the duration of the call, so its raw handle
        // is valid; `info` is a live, writable, correctly laid out
        // `BY_HANDLE_FILE_INFORMATION` that the call fills in and does not retain.
        let res = unsafe {
            GetFileInformationByHandle(file.as_raw_handle(), std::ptr::from_mut(&mut info))
        };
        if res != 0 {
            let inode =
                (u64::from(info.n_file_index_high) << 32) | u64::from(info.n_file_index_low);
            return (inode, info.n_number_of_links, info.dw_volume_serial_number);
        }
    }
    (0, 1, 0)
}

#[cfg(windows)]
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
