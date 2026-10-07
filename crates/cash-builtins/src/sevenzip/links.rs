//! Links an extraction makes once every item is out (`SetPostLinks`): a tar's hard and
//! symbolic links, each first an empty placeholder, then weighed for danger as 7-Zip
//! weighs them (`-snld`), then made.

use std::fs;
use std::os::windows::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use super::archive::Link;
use super::{Console, text};

/// `FILE_ATTRIBUTE_REPARSE_POINT`: what 7-Zip takes for a link on the disk.
const ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// The highest `-snld` level at which a link through another link is refused.
const LEVEL_MAX_FOR_LINK_OVER_LINK: u32 = 9;

/// A link to make at the end (`CPostLink`).
pub(super) struct PostLink {
    /// The item's path in the archive, as the messages name it.
    pub(super) item_path: String,
    /// The parts of its path under the output folder.
    pub(super) parts: Vec<String>,
    pub(super) is_dir: bool,
    pub(super) link: Link,
    /// The placeholder, made the link.
    pub(super) path: PathBuf,
    /// The placeholder as the messages show it.
    pub(super) shown: String,
    pub(super) modified: Option<u64>,
}

/// What a link names, as `ReadLink` and `Normalize_to_RelativeSafe` leave it: `\`
/// between the parts; an absolute path taken from the output folder.
struct LinkInfo {
    hard: bool,
    /// A symbolic link relative to its own folder; else from the output folder.
    relative: bool,
    path: String,
}

impl LinkInfo {
    fn of(link: &Link) -> Self {
        let mut path = link.path.replace('/', "\\");
        let mut relative = !link.hard;
        if let Some(rest) = path.strip_prefix(r"\??\") {
            relative = false;
            path = match rest.strip_prefix("UNC\\") {
                Some(unc) => format!(r"\\{unc}"),
                None => rest.to_owned(),
            };
        }
        if is_absolute(&path) {
            relative = false;
            let rest = if is_drive(&path) {
                path.get(2..).unwrap_or_default()
            } else {
                &path
            };
            path = rest.trim_start_matches('\\').to_owned();
        }
        Self {
            hard: link.hard,
            relative,
            path,
        }
    }
}

const fn is_drive(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// `IsAbsolutePath`: a separator or a drive first.
fn is_absolute(path: &str) -> bool {
    path.starts_with(['\\', '/']) || is_drive(path)
}

/// `CLinkLevelsInfo`: how far up and down a path goes.
#[derive(Default)]
struct Levels {
    absolute: bool,
    parent_after_name: bool,
    low: i32,
    last: i32,
}

impl Levels {
    fn of(path: &str) -> Self {
        let mut levels = Self {
            absolute: is_absolute(path),
            ..Self::default()
        };
        let mut named = false;
        let mut level = 0;
        for (i, part) in path.split(['\\', '/']).enumerate() {
            match part {
                "" => levels.absolute |= i == 0,
                "." => {}
                ".." => {
                    if levels.absolute || named {
                        levels.parent_after_name = true;
                    }
                    level -= 1;
                    levels.low = levels.low.min(level);
                }
                _ => {
                    named = true;
                    level += 1;
                }
            }
        }
        levels.last = level;
        levels
    }

    /// `IsSafePath`: below where it starts, and never above.
    fn safe(path: &str) -> bool {
        let levels = Self::of(path);
        !levels.absolute && levels.low >= 0 && levels.last > 0
    }
}

/// Whether a path on the disk is a link (`IsOsSymLink`): a reparse point.
fn is_link_on_disk(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_attributes() & ATTRIBUTE_REPARSE_POINT != 0)
}

/// `CheckLinkPath_in_FS_for_pathParts`: no part met on the way is a link.
fn parts_free_of_links(base: &Path, parts: &[&str]) -> bool {
    let mut path = base.to_path_buf();
    parts.iter().all(|part| {
        path.push(part);
        !is_link_on_disk(&path)
    })
}

/// `GetFullPath`: `rel` under `base`, `.` and `..` resolved, `/` between the parts.
fn full_path(base: &Path, rel: &str) -> String {
    let joined = format!("{}\\{rel}", base.to_string_lossy()).replace('/', "\\");
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('\\') {
        match part {
            "" | "." if !parts.is_empty() => {}
            ".." => {
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}

/// One message on the errors' stream.
fn error<SE: cash_core::ShellExtensions>(console: &Console<'_, SE>, message: &str) {
    console.flush_so();
    console.se(&format!("ERROR: {message}\n"));
    console.flush_se();
}

/// Removes what is at `path` for the link (`DeleteLinkFileAlways_or_RemoveEmptyDir`);
/// with `check`, a placeholder that is not empty is kept, and said so.
fn clear(path: &Path, check: bool) -> Result<(), String> {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    let reparse = meta.file_attributes() & ATTRIBUTE_REPARSE_POINT != 0;
    let shown = path.to_string_lossy().replace('\\', "/");
    let removed = if meta.is_dir() {
        fs::remove_dir(path)
    } else {
        if check && !reparse && meta.len() != 0 {
            return Err(format!("Temporary link file is not empty : {shown}"));
        }
        if meta.permissions().readonly() {
            let _ = cash_win32::unix::set_attributes(path, 0);
        }
        fs::remove_file(path)
    };
    match removed {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            let what = if meta.is_dir() { "directory" } else { "file" };
            Err(format!(
                "Cannot delete {what} for symbolic link creation : {} : {shown}",
                text::system_message(&e)
            ))
        }
        _ => Ok(()),
    }
}

/// The placeholder a link is made in at the end (`SetLink`): what is there removed,
/// an empty file made in its place.
pub(super) fn placeholder(path: &Path, shown: &str) -> Result<(), String> {
    let _ = clear(path, false);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(drop)
        .map_err(|_| format!("Cannot create temporary link file : {shown}"))
}

/// `SetPostLinks`: each link weighed, then made; returns the errors.
pub(super) fn make<SE: cash_core::ShellExtensions>(
    console: &Console<'_, SE>,
    links: &[PostLink],
    out_dir: &Path,
    level: u32,
) -> u64 {
    let mut errors = 0;
    for post in links {
        if let Err(message) = make_one(post, out_dir, level) {
            error(console, &message);
            errors += 1;
        }
    }
    errors
}

/// `SetLink2`: one link.
fn make_one(post: &PostLink, out_dir: &Path, level: u32) -> Result<(), String> {
    let link = LinkInfo::of(&post.link);
    if link.path.is_empty() {
        return Ok(());
    }
    if level < 20 {
        let levels = Levels::of(&link.path);
        let mut prefix = String::new();
        let dangerous = if levels.absolute
            || levels.parent_after_name
            || (level <= 5 && link.relative && (levels.last < 1 || levels.low < 0))
        {
            true
        } else {
            if link.relative {
                // The link's folder: its parts less the last named one.
                let mut parts: Vec<&str> = post.parts.iter().map(String::as_str).collect();
                while let Some(last) = parts.pop() {
                    if !last.is_empty() {
                        break;
                    }
                }
                prefix = parts.join("\\");
                if !prefix.is_empty() {
                    prefix.push('\\');
                }
            }
            !Levels::safe(&format!("{prefix}{}", link.path))
        };
        let shown_link = link.path.replace('\\', "/");
        if dangerous {
            return Err(format!(
                "Dangerous link path was ignored : {} : {shown_link}",
                post.item_path
            ));
        }
        if level <= LEVEL_MAX_FOR_LINK_OVER_LINK && !checked_in_fs(post, out_dir, &link, &prefix) {
            return Err(format!(
                "Dangerous link via another link was ignored : {} : {shown_link}",
                post.item_path
            ));
        }
    }
    let target = if link.hard || !link.relative {
        full_path(out_dir, &link.path)
    } else {
        link.path.clone()
    };
    clear(&post.path, true)?;
    if link.hard {
        if level <= LEVEL_MAX_FOR_LINK_OVER_LINK && is_link_on_disk(Path::new(&target)) {
            return Err(format!(
                "Hard link to symbolic link was ignored : {} : {target}",
                post.shown
            ));
        }
        fs::hard_link(&target, &post.path).map_err(|e| {
            format!(
                "Cannot create hard link : {} : {} : {target}",
                text::system_message(&e),
                post.shown
            )
        })?;
        if let Some(time) = post.modified {
            let _ = cash_win32::unix::set_times(
                &post.path,
                &cash_win32::unix::Times {
                    modified: super::extract::system_time(time),
                    accessed: None,
                    created: None,
                },
            );
        }
        return Ok(());
    }
    let data = cash_win32::reparse::link_data(&target.replace('/', "\\"), false);
    cash_win32::reparse::set(&post.path, post.is_dir, &data).map_err(|e| {
        format!(
            "Cannot create symbolic link : {} : {}",
            text::system_message(&e),
            post.shown
        )
    })
}

/// `CheckLinkPath_in_FS`: neither the item's own path under the output folder nor the
/// one its link names goes through a link already on the disk.
fn checked_in_fs(post: &PostLink, out_dir: &Path, link: &LinkInfo, prefix: &str) -> bool {
    if post.parts.is_empty() {
        return false;
    }
    let item_parts: Vec<&str> = post.parts.iter().map(String::as_str).collect();
    if !parts_free_of_links(out_dir, &item_parts) {
        return false;
    }
    let mut base = out_dir.to_path_buf();
    for part in prefix.split('\\').filter(|p| !p.is_empty()) {
        base.push(part);
    }
    let link_parts: Vec<&str> = link.path.split('\\').collect();
    parts_free_of_links(&base, &link_parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_tell_paths_that_leave_their_folder() {
        assert!(Levels::safe(r"d\a.txt"));
        assert!(!Levels::safe(r"d\..\..\x"));
        assert!(!Levels::safe(r"d\.."));
        assert!(!Levels::safe(r"\etc"));
        assert!(Levels::of(r"sub\..\..\a.txt").parent_after_name);
        assert!(!Levels::of(r"..\a.txt").parent_after_name);
        assert_eq!(Levels::of(r"..\..\x").low, -2);
    }

    #[test]
    fn absolute_links_are_taken_from_the_output_folder() {
        let of = |hard, path: &str| {
            let info = LinkInfo::of(&Link {
                hard,
                path: path.to_owned(),
            });
            (info.relative, info.path)
        };
        assert_eq!(of(false, "/etc/passwd"), (false, r"etc\passwd".to_owned()));
        assert_eq!(of(false, "C:/Windows"), (false, "Windows".to_owned()));
        assert_eq!(of(false, "../a.txt"), (true, r"..\a.txt".to_owned()));
        assert_eq!(of(true, "/d/a.txt"), (false, r"d\a.txt".to_owned()));
    }

    #[test]
    fn full_paths_resolve_dots() {
        let base = Path::new(r"C:\t\x4");
        assert_eq!(full_path(base, r"..\a.txt"), "C:/t/a.txt");
        assert_eq!(full_path(base, r"d\none.txt"), "C:/t/x4/d/none.txt");
        assert_eq!(full_path(Path::new(r"C:/t/w/."), r"d\a"), "C:/t/w/d/a");
    }
}
