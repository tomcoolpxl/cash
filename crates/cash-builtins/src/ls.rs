//! Native Windows-optimized pure-Rust `ls` builtin.
//!
//! Provides accurate Windows file ownership (SIDs -> Account Names), NTFS hard link
//! counts, directory subfolder link counts, proper permissions, sorting, and colored
//! output matching POSIX / BusyBox conventions.

use std::cmp::Ordering;
use std::fs::Metadata;
use std::io::{IsTerminal as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use cash_core::{ExecutionResult, builtins};
use chrono::{DateTime, Local};
use clap::Parser;

/// List information about the FILEs (the current directory by default).
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
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

    /// List one file per line.
    #[arg(short = '1')]
    one_column: bool,

    /// List entries by columns.
    #[arg(short = 'C')]
    multi_column: bool,

    /// List directories themselves, not their contents.
    #[arg(short = 'd', long = "directory")]
    directory: bool,

    /// Append indicator (one of */=>@|) to entries.
    #[arg(short = 'F', long = "classify")]
    classify: bool,

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

    /// Colorize the output (always, auto, never).
    #[arg(long = "color", value_name = "WHEN")]
    color: Option<String>,

    /// Files or directories to list.
    #[arg(value_name = "FILE")]
    paths: Vec<String>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum EntryKind {
    Dir,
    File,
    Symlink,
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
    mtime: SystemTime,
    symlink_target: Option<String>,
}

impl builtins::Command for LsCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.help {
            writeln!(context.stdout(), "Usage: {} [OPTION]... [FILE]...", context.command_name)?;
            writeln!(context.stdout(), "List information about the FILEs (the current directory by default).")?;
            return Ok(ExecutionResult::success());
        }

        let is_tty = std::io::stdout().is_terminal();
        let use_color = match self.color.as_deref() {
            Some("always" | "yes" | "force") => true,
            Some("never" | "no" | "none") => false,
            _ => is_tty,
        };

        let mut paths = self.paths.clone();
        if paths.is_empty() {
            paths.push(String::from("."));
        }

        let mut had_error = false;
        let multiple_paths = paths.len() > 1;

        // Separate inputs into existing files, existing dirs, and non-existing paths.
        let mut file_items = Vec::new();
        let mut dir_paths = Vec::new();

        for path_str in &paths {
            let path = PathBuf::from(path_str);
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

            if self.directory || !path.is_dir() {
                if let Some(item) = inspect_path(&path, path_str) {
                    file_items.push(item);
                }
            } else {
                dir_paths.push((path, path_str.clone()));
            }
        }

        // Print standalone files first.
        if !file_items.is_empty() {
            self.sort_items(&mut file_items);
            self.render_items(&context, &file_items, use_color, is_tty)?;
            if !dir_paths.is_empty() {
                writeln!(context.stdout())?;
            }
        }

        // Print directories.
        for (i, (dir_path, dir_str)) in dir_paths.iter().enumerate() {
            if multiple_paths || self.recursive {
                if i > 0 || !file_items.is_empty() {
                    writeln!(context.stdout())?;
                }
                writeln!(context.stdout(), "{dir_str}:")?;
            }

            if let Err(e) = self.list_directory(&context, dir_path, dir_str, use_color, is_tty) {
                writeln!(
                    context.stderr(),
                    "{}: cannot open directory '{}': {e}",
                    context.command_name,
                    dir_str
                )?;
                had_error = true;
            }
        }

        if had_error {
            Ok(ExecutionResult::new(2))
        } else {
            Ok(ExecutionResult::success())
        }
    }
}

impl LsCommand {
    fn sort_items(&self, items: &mut [ItemInfo]) {
        items.sort_by(|a, b| {
            let cmp = if self.sort_by_time {
                b.mtime.cmp(&a.mtime)
            } else if self.sort_by_size {
                b.size.cmp(&a.size)
            } else {
                // Natural alphabetical sort (case-insensitive on Windows)
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
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

    fn list_directory<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        dir_path: &Path,
        dir_str: &str,
        use_color: bool,
        is_tty: bool,
    ) -> Result<(), std::io::Error> {
        let mut items = Vec::new();

        // Include . and .. if -a is set.
        if self.all {
            if let Some(dot) = inspect_dot(dir_path, ".") {
                items.push(dot);
            }
            let parent = dir_path.parent().unwrap_or(dir_path);
            if let Some(dotdot) = inspect_dot(parent, "..") {
                items.push(dotdot);
            }
        }

        let read_dir = std::fs::read_dir(dir_path)?;
        for entry in read_dir.flatten() {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let is_dotfile = file_name.starts_with('.');

            if is_dotfile && !self.all && !self.almost_all {
                continue;
            }

            let entry_path = entry.path();
            if let Some(item) = inspect_path(&entry_path, &file_name) {
                items.push(item);
            }
        }

        self.sort_items(&mut items);

        // In long format, print total blocks at top of directory listing.
        if self.long {
            let total_kb: u64 = items
                .iter()
                .map(|item| item.size.div_ceil(1024))
                .sum();
            writeln!(context.stdout(), "total {total_kb}")?;
        }

        self.render_items(context, &items, use_color, is_tty)?;

        // Recursive descent if -R
        if self.recursive {
            for item in &items {
                if item.kind == EntryKind::Dir && item.name != "." && item.name != ".." {
                    let sub_str = format!("{dir_str}/{}", item.name);
                    writeln!(context.stdout())?;
                    writeln!(context.stdout(), "{sub_str}:")?;
                    let _ = self.list_directory(context, &item.path, &sub_str, use_color, is_tty);
                }
            }
        }

        Ok(())
    }

    fn render_items<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        items: &[ItemInfo],
        use_color: bool,
        is_tty: bool,
    ) -> Result<(), std::io::Error> {
        if items.is_empty() {
            return Ok(());
        }

        if self.long {
            self.render_long(context, items, use_color)?;
        } else if self.one_column || (!is_tty && !self.multi_column) {
            self.render_single_column(context, items, use_color)?;
        } else {
            self.render_grid(context, items, use_color)?;
        }

        Ok(())
    }

    fn render_long<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        items: &[ItemInfo],
        use_color: bool,
    ) -> Result<(), std::io::Error> {
        let max_links_len = items
            .iter()
            .map(|i| i.links.to_string().len())
            .max()
            .unwrap_or(1);
        let max_owner_len = items.iter().map(|i| i.owner.len()).max().unwrap_or(1);
        let max_group_len = items.iter().map(|i| i.group.len()).max().unwrap_or(1);
        let max_size_len = items
            .iter()
            .map(|i| {
                if self.human_readable {
                    format_human_size(i.size).len()
                } else {
                    i.size.to_string().len()
                }
            })
            .max()
            .unwrap_or(1);

        for item in items {
            let size_str = if self.human_readable {
                format_human_size(item.size)
            } else {
                item.size.to_string()
            };
            let date_str = format_date(item.mtime);
            let display_name = self.format_name(item, use_color);

            writeln!(
                context.stdout(),
                "{} {:>links_w$} {:<owner_w$} {:<group_w$} {:>size_w$} {} {}",
                item.permissions,
                item.links,
                item.owner,
                item.group,
                size_str,
                date_str,
                display_name,
                links_w = max_links_len,
                owner_w = max_owner_len,
                group_w = max_group_len,
                size_w = max_size_len,
            )?;
        }

        Ok(())
    }

    fn render_single_column<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        items: &[ItemInfo],
        use_color: bool,
    ) -> Result<(), std::io::Error> {
        for item in items {
            let display_name = self.format_name(item, use_color);
            writeln!(context.stdout(), "{display_name}")?;
        }
        Ok(())
    }

    fn render_grid<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        items: &[ItemInfo],
        use_color: bool,
    ) -> Result<(), std::io::Error> {
        let (term_width, _) = crossterm::terminal::size().unwrap_or((80, 24));
        let term_width = (term_width as usize).max(20);

        let pairs: Vec<(String, String)> = items
            .iter()
            .map(|i| {
                let plain = self.format_name_plain(i);
                let colored = self.format_name(i, use_color);
                (plain, colored)
            })
            .collect();

        let max_len = pairs.iter().map(|(p, _)| p.len()).max().unwrap_or(1);
        let col_width = max_len + 2;
        let num_cols = (term_width / col_width).max(1);
        let num_rows = pairs.len().div_ceil(num_cols);

        for r in 0..num_rows {
            let mut line = String::new();
            for c in 0..num_cols {
                let idx = c * num_rows + r;
                if idx < pairs.len() {
                    let (ref plain, ref colored) = pairs[idx];
                    line.push_str(colored);
                    if c + 1 < num_cols && (c + 1) * num_rows + r < pairs.len() {
                        let pad = col_width.saturating_sub(plain.len());
                        for _ in 0..pad {
                            line.push(' ');
                        }
                    }
                }
            }
            writeln!(context.stdout(), "{line}")?;
        }

        Ok(())
    }

    fn format_name_plain(&self, item: &ItemInfo) -> String {
        let mut s = item.name.clone();
        if self.classify {
            match item.kind {
                EntryKind::Dir => s.push('/'),
                EntryKind::Symlink => s.push('@'),
                EntryKind::File => {
                    if is_executable(&item.path) {
                        s.push('*');
                    }
                }
            }
        }
        if let Some(ref target) = item.symlink_target {
            s.push_str(" -> ");
            s.push_str(target);
        }
        s
    }

    fn format_name(&self, item: &ItemInfo, use_color: bool) -> String {
        let mut s = if use_color {
            match item.kind {
                EntryKind::Dir => format!("\x1b[1;34m{}\x1b[0m", item.name),
                EntryKind::Symlink => format!("\x1b[1;36m{}\x1b[0m", item.name),
                EntryKind::File => {
                    if is_executable(&item.path) {
                        format!("\x1b[1;32m{}\x1b[0m", item.name)
                    } else {
                        item.name.clone()
                    }
                }
            }
        } else {
            item.name.clone()
        };

        if self.classify {
            match item.kind {
                EntryKind::Dir => s.push('/'),
                EntryKind::Symlink => s.push('@'),
                EntryKind::File => {
                    if is_executable(&item.path) {
                        s.push('*');
                    }
                }
            }
        }

        if let Some(ref target) = item.symlink_target {
            s.push_str(" -> ");
            s.push_str(target);
        }

        s
    }
}

fn inspect_dot(dir_path: &Path, name: &str) -> Option<ItemInfo> {
    let metadata = std::fs::metadata(dir_path).ok()?;
    let owner = cash_win32::fs::get_file_owner(dir_path)
        .unwrap_or_else(cash_win32::fs::current_user);
    let group = owner.clone();
    let subdirs = cash_win32::fs::count_subdirectories(dir_path);
    let links = 2 + u32::try_from(subdirs).unwrap_or(0);
    let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    Some(ItemInfo {
        name: name.to_string(),
        path: dir_path.to_path_buf(),
        kind: EntryKind::Dir,
        permissions: String::from("drwxrwxr-x"),
        links,
        owner,
        group,
        size: metadata.len(),
        mtime,
        symlink_target: None,
    })
}

fn inspect_path(path: &Path, display_name: &str) -> Option<ItemInfo> {
    let symlink_metadata = std::fs::symlink_metadata(path).ok()?;
    let is_symlink = symlink_metadata.file_type().is_symlink();

    let metadata = if is_symlink {
        symlink_metadata
    } else {
        std::fs::metadata(path).unwrap_or(symlink_metadata)
    };

    let kind = if is_symlink {
        EntryKind::Symlink
    } else if metadata.is_dir() {
        EntryKind::Dir
    } else {
        EntryKind::File
    };

    let permissions = format_permissions(path, &metadata, is_symlink);
    let links = cash_win32::fs::file_link_count(path, &metadata);
    let owner =
        cash_win32::fs::get_file_owner(path).unwrap_or_else(cash_win32::fs::current_user);
    let group = owner.clone();
    let size = metadata.len();
    let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    let symlink_target = if is_symlink {
        std::fs::read_link(path)
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    } else {
        None
    };

    Some(ItemInfo {
        name: display_name.to_string(),
        path: path.to_path_buf(),
        kind,
        permissions,
        links,
        owner,
        group,
        size,
        mtime,
        symlink_target,
    })
}

fn format_permissions(path: &Path, metadata: &Metadata, is_symlink: bool) -> String {
    if is_symlink {
        return String::from("lrwxrwxrwx");
    }
    if metadata.is_dir() {
        return String::from("drwxrwxr-x");
    }
    if metadata.permissions().readonly() {
        String::from("-r--r--r--")
    } else if is_executable(path) {
        String::from("-rwxrwxr-x")
    } else {
        String::from("-rw-rw-r--")
    }
}

fn is_executable(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("exe")
        || ext.eq_ignore_ascii_case("cmd")
        || ext.eq_ignore_ascii_case("bat")
        || ext.eq_ignore_ascii_case("com")
        || ext.eq_ignore_ascii_case("ps1")
    {
        return true;
    }

    if let Ok(mut f) = std::fs::File::open(path) {
        use std::io::Read as _;
        let mut magic = [0u8; 2];
        if f.read_exact(&mut magic).is_ok() && &magic == b"#!" {
            return true;
        }
    }

    false
}

fn format_human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes < KB {
        return bytes.to_string();
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "byte counts this large do not lose visible precision in 2-4 digits"
    )]
    if bytes < MB {
        let val = bytes as f64 / KB as f64;
        if val < 10.0 {
            format!("{val:.1}K")
        } else {
            format!("{:.0}K", val.round())
        }
    } else if bytes < GB {
        let val = bytes as f64 / MB as f64;
        if val < 10.0 {
            format!("{val:.1}M")
        } else {
            format!("{:.0}M", val.round())
        }
    } else if bytes < TB {
        let val = bytes as f64 / GB as f64;
        if val < 10.0 {
            format!("{val:.1}G")
        } else {
            format!("{:.0}G", val.round())
        }
    } else {
        let val = bytes as f64 / TB as f64;
        format!("{val:.1}T")
    }
}

fn format_date(mtime: SystemTime) -> String {
    let dt: DateTime<Local> = mtime.into();
    let now = Local::now();
    let six_months = chrono::Duration::days(182);

    if dt > now - six_months && dt <= now + chrono::Duration::hours(1) {
        dt.format("%b %e %H:%M").to_string()
    } else {
        dt.format("%b %e  %Y").to_string()
    }
}
