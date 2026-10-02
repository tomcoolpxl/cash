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
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use cash_core::{ExecutionResult, builtins};
use chrono::{DateTime, Local};
use clap::Parser;
use unicode_width::UnicodeWidthStr as _;

use crate::ls_icon_table;

const HELP: &str = "\
Usage: ls [OPTION]... [FILE]...
List information about the FILEs (the current directory by default).

  -a, --all                  do not ignore entries starting with . or files
                               both hidden and system, as Explorer hides them
  -A, --almost-all           like -a, but do not list implied . and ..
  -C                         list entries by columns
  -d, --directory            list directories themselves, not their contents
  -F, --classify             append indicator (one of */@) to entries
  -h, --human-readable       with -l, print sizes like 1K 234M 2G
  -l                         use a long listing format
  -r, --reverse              reverse order while sorting
  -R, --recursive            list subdirectories recursively
  -S                         sort by file size, largest first
  -t                         sort by time, newest first
  -U                         do not sort; list entries in directory order
  -v                         natural sort of (version) numbers within text
  -X                         sort alphabetically by entry extension
  -1                         list one file per line
      --sort=WORD            sort by WORD instead of name: none (-U), size (-S),
                               time (-t), version (-v), extension (-X), name
      --group-directories-first
                             group directories before files
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
      --tree                 list the directories as a tree
      --depth=NUM            descend at most NUM levels, with --tree or -R
      --help                 display this help and exit
";

/// List information about the FILEs (the current directory by default).
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
#[allow(
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
    mtime: SystemTime,
    /// Windows' attribute bits: read-only, hidden, system, archive.
    attributes: u32,
    symlink_target: Option<String>,
    /// Whether a symlink's target exists, and whether it is a directory.
    target: Option<(bool, bool)>,
    /// Whether the file runs: decided once, since it may read the file's first bytes.
    executable: OnceCell<bool>,
    /// A device's numbers, major and minor, which `-l` shows in place of the size.
    device: Option<(u32, u32)>,
}

impl ItemInfo {
    fn is_executable(&self) -> bool {
        *self
            .executable
            .get_or_init(|| self.kind == EntryKind::File && is_executable(&self.path))
    }
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
}

/// A command line `ls` refuses, with its message.
struct Invalid(String);

fn when(value: Option<&str>, option: &str, is_tty: bool) -> Result<bool, Invalid> {
    match value {
        None => Ok(false),
        Some("always" | "yes" | "force") => Ok(true),
        Some("never" | "no" | "none") => Ok(false),
        Some("auto" | "tty" | "if-tty") => Ok(is_tty),
        Some(other) => Err(Invalid(std::format!(
            "invalid argument '{other}' for '--{option}'\n\
             Valid arguments are: 'always', 'auto', 'never'"
        ))),
    }
}

impl builtins::Command for LsCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.help {
            write!(context.stdout(), "{HELP}")?;
            return Ok(ExecutionResult::success());
        }

        // The command's own standard output, which a pipeline or a redirection replaces
        // even though the shell's is still the terminal.
        let is_tty = context.try_fd(1).is_some_and(|f| f.is_terminal());
        let look = match self.look(&context, is_tty) {
            Ok(look) => look,
            Err(Invalid(message)) => {
                writeln!(context.stderr(), "{}: {message}", context.command_name)?;
                writeln!(
                    context.stderr(),
                    "Try '{} --help' for more information.",
                    context.command_name
                )?;
                return Ok(ExecutionResult::new(2));
            }
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

            if self.directory || !path.is_dir() {
                if let Some(item) = inspect_path(&path, path_str, self.long, None) {
                    file_items.push(item);
                }
            } else {
                dir_paths.push((path, path_str.clone()));
            }
        }

        // Print standalone files first.
        if !file_items.is_empty() {
            self.sort_items(&mut file_items);
            self.render_items(&context, &file_items, &look, is_tty)?;
            if !dir_paths.is_empty() {
                writeln!(context.stdout())?;
            }
        }

        // Print directories.
        for (i, (dir_path, dir_str)) in dir_paths.iter().enumerate() {
            if self.tree {
                if i > 0 || !file_items.is_empty() {
                    writeln!(context.stdout())?;
                }
                self.render_tree(&context, dir_path, dir_str, &look)?;
                continue;
            }

            if multiple_paths || self.recursive {
                if i > 0 || !file_items.is_empty() {
                    writeln!(context.stdout())?;
                }
                writeln!(context.stdout(), "{dir_str}:")?;
            }

            if let Err(e) = self.list_directory(&context, dir_path, dir_str, &look, is_tty, 1) {
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
                return Err(Invalid(std::format!(
                    "invalid argument '{other}' for '--icons-theme'\n\
                     Valid arguments are: 'auto', 'fancy'"
                )));
            }
        };
        self.sort_key()?;

        let colors = color.then(|| {
            // The shell's own variable, which `export` need not have reached the process.
            context.shell.env_str("LS_COLORS").map_or_else(
                || lscolors::LsColors::from_string(&default_ls_colors()),
                |value| lscolors::LsColors::from_string(&value),
            )
        });
        Ok(Look {
            me: if colors.is_some() && self.long {
                cash_win32::fs::current_user()
            } else {
                String::new()
            },
            colors,
            icons,
        })
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
                other => Err(Invalid(std::format!(
                    "invalid argument '{other}' for '--sort'\n\
                     Valid arguments are: 'name', 'none', 'size', 'time', 'version', 'extension'"
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
        } else {
            SortKey::Name
        })
    }

    fn sort_items(&self, items: &mut [ItemInfo]) {
        let key = self.sort_key().unwrap_or(SortKey::Name);
        if key != SortKey::None {
            items.sort_by(|a, b| {
                let cmp = match key {
                    SortKey::Time => b.mtime.cmp(&a.mtime),
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

        // Include . and .. if -a is set.
        if self.all && dots {
            if let Some(dot) = inspect_dot(dir_path, ".", self.long) {
                items.push(dot);
            }
            let parent = dir_path.parent().unwrap_or(dir_path);
            if let Some(dotdot) = inspect_dot(parent, "..", self.long) {
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
            if let Some(item) = inspect_path(&entry_path, &file_name, self.long, known) {
                items.push(item);
            }
        }

        self.sort_items(&mut items);
        Ok(items)
    }

    fn list_directory<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        dir_path: &Path,
        dir_str: &str,
        look: &Look,
        is_tty: bool,
        level: usize,
    ) -> Result<(), std::io::Error> {
        let items = self.read_entries(dir_path, true)?;

        // In long format, print total blocks at top of directory listing.
        if self.long {
            let total_kb: u64 = items.iter().map(|item| item.size.div_ceil(1024)).sum();
            writeln!(context.stdout(), "total {total_kb}")?;
        }

        self.render_items(context, &items, look, is_tty)?;

        // Recursive descent if -R, as deep as --depth allows.
        if self.recursive && self.depth.is_none_or(|depth| level < depth) {
            for item in &items {
                if item.kind == EntryKind::Dir && item.name != "." && item.name != ".." {
                    let sub_str = format!("{dir_str}/{}", item.name);
                    writeln!(context.stdout())?;
                    writeln!(context.stdout(), "{sub_str}:")?;
                    let _ =
                        self.list_directory(context, &item.path, &sub_str, look, is_tty, level + 1);
                }
            }
        }

        Ok(())
    }

    /// `--tree`: the directory, then everything below it with the branches drawn, as
    /// deep as `--depth` allows. Links to directories are shown, not followed.
    fn render_tree<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        dir_path: &Path,
        dir_str: &str,
        look: &Look,
    ) -> Result<(), std::io::Error> {
        let mut rows: Vec<(String, ItemInfo)> = Vec::new();
        if let Some(root) = inspect_path(dir_path, dir_str, self.long, None) {
            rows.push((String::new(), root));
        }
        self.tree_rows(dir_path, "", 1, &mut rows);

        if self.long {
            let (items, prefixes): (Vec<ItemInfo>, Vec<String>) = rows
                .into_iter()
                .map(|(prefix, item)| (item, prefix))
                .unzip();
            self.render_long(context, &items, look, Some(&prefixes))?;
        } else {
            for (prefix, item) in &rows {
                writeln!(context.stdout(), "{prefix}{}", self.format_name(item, look))?;
            }
        }
        Ok(())
    }

    fn tree_rows(
        &self,
        dir: &Path,
        prefix: &str,
        level: usize,
        rows: &mut Vec<(String, ItemInfo)>,
    ) {
        let Ok(items) = self.read_entries(dir, false) else {
            return;
        };
        let count = items.len();
        for (index, item) in items.into_iter().enumerate() {
            let last = index + 1 == count;
            let branch = if last { "└── " } else { "├── " };
            let descend =
                item.kind == EntryKind::Dir && self.depth.is_none_or(|depth| level < depth);
            let path = item.path.clone();
            rows.push((std::format!("{prefix}{branch}"), item));
            if descend {
                let below = std::format!("{prefix}{}", if last { "    " } else { "│   " });
                self.tree_rows(&path, &below, level + 1, rows);
            }
        }
    }

    fn render_items<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        items: &[ItemInfo],
        look: &Look,
        is_tty: bool,
    ) -> Result<(), std::io::Error> {
        if items.is_empty() {
            return Ok(());
        }

        if self.long {
            self.render_long(context, items, look, None)?;
        } else if self.one_column || (!is_tty && !self.multi_column) {
            self.render_single_column(context, items, look)?;
        } else {
            self.render_grid(context, items, look)?;
        }

        Ok(())
    }

    /// The long format, each name after its tree branch when there are `prefixes`.
    fn render_long<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        items: &[ItemInfo],
        look: &Look,
        prefixes: Option<&[String]>,
    ) -> Result<(), std::io::Error> {
        let max_links_len = items
            .iter()
            .map(|i| i.links.to_string().len())
            .max()
            .unwrap_or(1);
        let max_owner_len = items.iter().map(|i| i.owner.len()).max().unwrap_or(1);
        let max_group_len = items.iter().map(|i| i.group.len()).max().unwrap_or(1);
        let size_text = |item: &ItemInfo| {
            if let Some((major, minor)) = item.device {
                std::format!("{major}, {minor}")
            } else if self.human_readable {
                format_human_size(item.size)
            } else {
                item.size.to_string()
            }
        };
        let max_size_len = items.iter().map(|i| size_text(i).len()).max().unwrap_or(1);

        for (index, item) in items.iter().enumerate() {
            let size_str = size_text(item);
            let date_str = format_date(item.mtime);
            let display_name = self.format_name(item, look);
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
                    paint(date_colour(item.mtime), &date_str),
                )
            } else {
                (owner, group, size, date_str)
            };
            writeln!(
                context.stdout(),
                "{permissions} {:>max_links_len$} {owner} {group} {size} {date} {prefix}{display_name}",
                item.links,
            )?;
        }

        Ok(())
    }

    fn render_single_column<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        items: &[ItemInfo],
        look: &Look,
    ) -> Result<(), std::io::Error> {
        for item in items {
            let display_name = self.format_name(item, look);
            writeln!(context.stdout(), "{display_name}")?;
        }
        Ok(())
    }

    fn render_grid<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        items: &[ItemInfo],
        look: &Look,
    ) -> Result<(), std::io::Error> {
        let (term_width, _) = crossterm::terminal::size().unwrap_or((80, 24));
        let term_width = (term_width as usize).max(20);

        // Columns are measured in the cells a name takes on screen, not its bytes: an
        // icon, an accented letter or a CJK name is one or two cells whatever its length.
        let pairs: Vec<(usize, String)> = items
            .iter()
            .map(|i| {
                (
                    self.format_name_plain(i, look).width(),
                    self.format_name(i, look),
                )
            })
            .collect();

        let max_len = pairs.iter().map(|(width, _)| *width).max().unwrap_or(1);
        let col_width = max_len + 2;
        let num_cols = (term_width / col_width).max(1);
        let num_rows = pairs.len().div_ceil(num_cols);

        for r in 0..num_rows {
            let mut line = String::new();
            for c in 0..num_cols {
                let idx = c * num_rows + r;
                if idx < pairs.len() {
                    let (width, ref colored) = pairs[idx];
                    line.push_str(colored);
                    if c + 1 < num_cols && (c + 1) * num_rows + r < pairs.len() {
                        let pad = col_width.saturating_sub(width);
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

    /// The `-F` indicator for an entry.
    fn indicator(&self, item: &ItemInfo) -> Option<char> {
        if !self.classify {
            return None;
        }
        match item.kind {
            EntryKind::Dir => Some('/'),
            EntryKind::Symlink => Some('@'),
            EntryKind::File => item.is_executable().then_some('*'),
            EntryKind::CharDevice => None,
        }
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

    fn format_name_plain(&self, item: &ItemInfo, look: &Look) -> String {
        let mut s = self.attribute_prefix(item);
        if look.icons {
            s.push_str(icon_for(item));
            s.push(' ');
        }
        s.push_str(&item.name);
        if let Some(indicator) = self.indicator(item) {
            s.push(indicator);
        }
        if let Some(ref target) = item.symlink_target {
            s.push_str(" -> ");
            s.push_str(target);
        }
        s
    }

    fn format_name(&self, item: &ItemInfo, look: &Look) -> String {
        let mut shown = String::new();
        if look.icons {
            shown.push_str(icon_for(item));
            shown.push(' ');
        }
        shown.push_str(&item.name);

        let mut s = self.attribute_prefix(item);
        s.push_str(&match look
            .colors
            .as_ref()
            .and_then(|colors| style_for(colors, item))
        {
            Some(style) => style.paint(&shown).to_string(),
            None => shown,
        });

        if let Some(indicator) = self.indicator(item) {
            s.push(indicator);
        }

        if let Some(ref target) = item.symlink_target {
            s.push_str(" -> ");
            s.push_str(target);
        }

        s
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
pub(crate) fn owner_colour(account: &str, me: &str) -> &'static str {
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

fn inspect_dot(dir_path: &Path, name: &str, long: bool) -> Option<ItemInfo> {
    let metadata = std::fs::metadata(dir_path).ok()?;
    let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    let mut item = ItemInfo {
        name: name.to_string(),
        path: dir_path.to_path_buf(),
        kind: EntryKind::Dir,
        permissions: String::new(),
        links: 0,
        owner: String::new(),
        group: String::new(),
        size: metadata.len(),
        mtime,
        attributes: attributes_of(&metadata),
        symlink_target: None,
        target: None,
        executable: OnceCell::new(),
        device: None,
    };
    if long {
        let subdirs = cash_win32::fs::count_subdirectories(dir_path);
        item.links = 2 + u32::try_from(subdirs).unwrap_or(0);
        fill_long(&mut item, &metadata);
    }
    Some(item)
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
        mtime: SystemTime::now(),
        attributes: 0,
        symlink_target: None,
        target: None,
        executable: OnceCell::new(),
        device: name.numbers(),
    };
    if let cash_win32::devices::DevName::Descriptor(number) = name {
        let fd = cash_core::ShellFd::try_from(number).ok()?;
        context.try_fd(fd)?;
        item.kind = EntryKind::Symlink;
        item.permissions = String::from("lrwxrwxrwx");
        item.symlink_target = Some(std::format!("/proc/self/fd/{number}"));
        item.target = Some((true, false));
    }
    Some(item)
}

/// An entry, from `known` metadata when the directory listing supplied it (no file is
/// opened for that), else read. The owner, link count and permissions, which cost a
/// read of each file's security and more, are looked up only for `-l`, which shows them.
fn inspect_path(
    path: &Path,
    display_name: &str,
    long: bool,
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
        mtime: symlink_metadata
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH),
        attributes: attributes_of(&symlink_metadata),
        symlink_target,
        target,
        executable: OnceCell::new(),
        device: None,
    };
    if long {
        item.links = cash_win32::fs::file_link_count(path, &symlink_metadata);
        fill_long(&mut item, &symlink_metadata);
    }
    Some(item)
}

/// The owner and the permission string `-l` shows, from one read of the file's security.
fn fill_long(item: &mut ItemInfo, metadata: &Metadata) {
    let security = cash_win32::fs::file_security(&item.path);
    item.owner = security.owner.unwrap_or_else(cash_win32::fs::current_user);
    item.group = item.owner.clone();
    item.permissions = format_permissions(item, metadata, security.writable);
}

/// Unix permission bits for a Windows file, which has an access list instead. `r` is
/// always there. `w` is whether this process may write it: its access list allows it
/// (`writable`, when it could be read) and, for a file, the read-only attribute is not
/// set; a folder's read-only attribute only marks it as customised, and does not stop
/// writing. `x` is a program by extension or a `#!` line, and every folder. The owner
/// and group positions carry the answer; others read only.
fn format_permissions(item: &ItemInfo, metadata: &Metadata, writable: Option<bool>) -> String {
    if item.kind == EntryKind::Symlink {
        return String::from("lrwxrwxrwx");
    }
    let is_dir = item.kind == EntryKind::Dir;
    let write = writable.unwrap_or(true) && (is_dir || !metadata.permissions().readonly());
    let run = is_dir || item.is_executable();
    let (w, x) = (if write { 'w' } else { '-' }, if run { 'x' } else { '-' });
    std::format!("{}r{w}{x}r{w}{x}r-{x}", if is_dir { 'd' } else { '-' })
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
