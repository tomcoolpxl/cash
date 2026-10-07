//! The Unix face of a Windows file (D78): what `ls -l`, `stat`, `tar` and `zip` say a
//! file is, read in one place so they agree, and what they need to make files.
//!
//! Windows has an access list where Unix has mode bits, owner and group SIDs where Unix
//! has numbers, and no execute bit at all. The answers here:
//!
//! - **The mode**: `r` always; `w` when this process may write by the access list and,
//!   for a file, the read-only attribute is not set (a folder's only marks it as
//!   customised); `x` for every folder, a program by its extension (`exe`, `com`, `cmd`,
//!   `bat`, `ps1`) or a file that starts with `#!`. The group positions carry the
//!   owner's answer, others read and run only. A symbolic link is `0o777`.
//! - **The owner and the group**: the accounts the security descriptor names, each with
//!   the RID of its SID as its number, the number `id` gives; when the descriptor cannot
//!   be read, this process's account for both.
//! - **The times**: to the 100 nanoseconds Windows keeps.
//!
//! To make files: [`Replacement`] (written beside its target, renamed over it at the
//! end), [`symlink`], [`hard_link`], [`set_times`] (folders too), [`set_attributes`],
//! and [`check_name`] for the names Windows cannot hold.

use std::fs;
use std::io::{self, Write};
use std::os::windows::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::fs::{FileInfo, FileOwner};

/// An account with its number: a name, and the RID of its SID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// The account name, without its domain.
    pub name: String,
    /// The RID, as `id` and `stat` give it.
    pub id: u32,
}

impl From<FileOwner> for Account {
    fn from(owner: FileOwner) -> Self {
        Self {
            name: owner.name,
            id: owner.rid,
        }
    }
}

/// What kind of file it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A regular file.
    File,
    /// A folder.
    Dir,
    /// A symbolic link or a junction, when it is not followed.
    Symlink,
}

impl Kind {
    /// The `S_IFMT` bits of this kind.
    pub const fn type_bits(self) -> u32 {
        match self {
            Self::File => 0o100_000,
            Self::Dir => 0o040_000,
            Self::Symlink => 0o120_000,
        }
    }
}

/// A file's times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Times {
    /// The last change of its contents.
    pub modified: SystemTime,
    /// The last read, when the file system keeps it.
    pub accessed: Option<SystemTime>,
    /// Its making.
    pub created: Option<SystemTime>,
}

/// The read-only attribute.
pub const READ_ONLY: u32 = 0x1;
/// The hidden attribute.
pub const HIDDEN: u32 = 0x2;
/// The system attribute.
pub const SYSTEM: u32 = 0x4;
/// The archive attribute.
pub const ARCHIVE: u32 = 0x20;

/// A file as Unix tools see it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnixView {
    /// What kind of file it is.
    pub kind: Kind,
    /// The permission bits, `0o7777` at most; [`UnixView::mode`] adds the kind.
    pub permissions: u32,
    /// Its size in bytes.
    pub size: u64,
    /// Its owner.
    pub owner: Account,
    /// Its primary group.
    pub group: Account,
    /// Its hard links; a folder counts two and one for each folder in it, as on Unix.
    pub links: u32,
    /// Its times.
    pub times: Times,
    /// Its Windows attributes ([`READ_ONLY`], [`HIDDEN`], [`SYSTEM`], [`ARCHIVE`], …).
    pub attributes: u32,
    /// Where a symbolic link points, as stored.
    pub link_target: Option<PathBuf>,
    /// What identifies it across its names, when Windows says.
    pub identity: Option<FileInfo>,
}

impl UnixView {
    /// The whole `st_mode`: the kind's bits and the permissions.
    pub const fn mode(&self) -> u32 {
        self.kind.type_bits() | self.permissions
    }
}

/// `path` as Unix tools see it; `follow` follows a symbolic link to what it names.
///
/// # Errors
///
/// When the file cannot be found or its metadata read.
pub fn unix_view(path: &Path, follow: bool) -> io::Result<UnixView> {
    let metadata = if follow {
        fs::metadata(path)?
    } else {
        fs::symlink_metadata(path)?
    };
    let kind = if metadata.file_type().is_symlink() {
        Kind::Symlink
    } else if metadata.is_dir() {
        Kind::Dir
    } else {
        Kind::File
    };
    let security = crate::fs::file_security(path);
    let fallback = || {
        crate::fs::current_owner().map_or_else(
            || Account {
                name: crate::fs::current_user(),
                id: 0,
            },
            Account::from,
        )
    };
    let owner = security
        .owner_account
        .clone()
        .map_or_else(fallback, Account::from);
    let group = security
        .group
        .clone()
        .map_or_else(|| owner.clone(), Account::from);
    let attributes = metadata.file_attributes();
    let permissions = permissions(
        kind,
        security.writable,
        attributes & READ_ONLY != 0,
        kind == Kind::File && is_executable(path),
    );
    Ok(UnixView {
        kind,
        permissions,
        size: metadata.len(),
        owner,
        group,
        links: crate::fs::file_link_count(path, &metadata),
        times: Times {
            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            accessed: metadata.accessed().ok(),
            created: metadata.created().ok(),
        },
        attributes,
        link_target: (kind == Kind::Symlink)
            .then(|| fs::read_link(path).ok())
            .flatten(),
        identity: crate::fs::file_info(path).ok(),
    })
}

/// The permission bits of a file of `kind`, by the rule in the module's notes.
///
/// `writable` is the access list's answer (`None` when it could not be read, taken as
/// yes), `read_only` the attribute, `executable` a program's answer for a file.
pub const fn permissions(
    kind: Kind,
    writable: Option<bool>,
    read_only: bool,
    executable: bool,
) -> u32 {
    if matches!(kind, Kind::Symlink) {
        return 0o777;
    }
    let is_dir = matches!(kind, Kind::Dir);
    let write = matches!(writable, Some(true) | None) && (is_dir || !read_only);
    let run = is_dir || executable;
    let mut bits = 0o444;
    if write {
        bits |= 0o220;
    }
    if run {
        bits |= 0o111;
    }
    bits
}

/// `mode` (`st_mode`: the kind's bits and the permissions) as `ls -l` writes it:
/// `drwxr-xr-x`, the set-user-id, set-group-id and sticky bits as `s`, `S`, `t` and `T`,
/// as GNU's `filemode` has them.
pub fn mode_string(mode: u32) -> String {
    let kind = match mode & 0o170_000 {
        0o040_000 => 'd',
        0o120_000 => 'l',
        0o020_000 => 'c',
        0o060_000 => 'b',
        0o010_000 => 'p',
        0o140_000 => 's',
        _ => '-',
    };
    let bit = |mask: u32, letter: char| if mode & mask == 0 { '-' } else { letter };
    let special = |run: u32, flag: u32, set: char| match (mode & run != 0, mode & flag != 0) {
        (true, true) => set,
        (false, true) => set.to_ascii_uppercase(),
        (true, false) => 'x',
        (false, false) => '-',
    };
    [
        kind,
        bit(0o400, 'r'),
        bit(0o200, 'w'),
        special(0o100, 0o4000, 's'),
        bit(0o040, 'r'),
        bit(0o020, 'w'),
        special(0o010, 0o2000, 's'),
        bit(0o004, 'r'),
        bit(0o002, 'w'),
        special(0o001, 0o1000, 't'),
    ]
    .iter()
    .collect()
}

/// Whether `path` is a program: by its extension (`exe`, `com`, `cmd`, `bat`, `ps1`),
/// or a file that starts with `#!`.
pub fn is_executable(path: &Path) -> bool {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ["exe", "cmd", "bat", "com", "ps1"]
        .iter()
        .any(|known| extension.eq_ignore_ascii_case(known))
    {
        return true;
    }
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 2];
    io::Read::read_exact(&mut file, &mut magic).is_ok() && &magic == b"#!"
}

/// The output of a file that replaces another, written beside it.
///
/// It is written under a temporary name and renamed over the target at the end, so an
/// interrupted run leaves no half-written file and the old one stays until the new one
/// is whole.
#[derive(Debug)]
pub struct Replacement {
    /// The name the output will have.
    path: PathBuf,
    /// Where it is being written.
    temporary: PathBuf,
    file: fs::File,
}

impl Replacement {
    /// Starts the replacement of `path`: a new file beside it named
    /// `.NAME.cash-PID-N.tmp`.
    ///
    /// # Errors
    ///
    /// When no such file can be made in the folder.
    pub fn create(path: PathBuf) -> io::Result<Self> {
        let directory = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let base = path
            .file_name()
            .map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
        let mut attempt = 0_u32;
        loop {
            let candidate =
                directory.join(format!(".{base}.cash-{}-{attempt}.tmp", std::process::id()));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        temporary: candidate,
                        file,
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && attempt < 100 => {
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// The file being written.
    pub const fn file(&mut self) -> &mut fs::File {
        &mut self.file
    }

    /// Finishes the file: its times and read-only bit, a sync when asked, and the
    /// rename over the target, which is removed first when it exists.
    ///
    /// # Errors
    ///
    /// The first step that failed; the temporary file is then removed.
    pub fn finish(
        self,
        times: fs::FileTimes,
        read_only: bool,
        synchronous: bool,
    ) -> io::Result<()> {
        let file = self.file;
        let result = file
            .set_times(times)
            .and_then(|()| if synchronous { file.sync_all() } else { Ok(()) })
            .and_then(|()| {
                if read_only {
                    let mut permissions = file.metadata()?.permissions();
                    permissions.set_readonly(true);
                    file.set_permissions(permissions)
                } else {
                    Ok(())
                }
            });
        drop(file);
        let result = result.and_then(|()| {
            if fs::symlink_metadata(&self.path).is_ok() {
                remove_even_read_only(&self.path)?;
            }
            fs::rename(&self.temporary, &self.path)
        });
        if result.is_err() {
            let _ = fs::remove_file(&self.temporary);
        }
        result
    }

    /// Gives the file up: nothing is left behind.
    pub fn abandon(self) {
        drop(self.file);
        let _ = fs::remove_file(&self.temporary);
    }
}

impl Write for Replacement {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// Removes `path`, clearing its read-only bit first: Windows refuses to delete a
/// read-only file, where Unix deletes by the folder's permissions.
///
/// # Errors
///
/// When the file cannot be removed, read-only bit cleared or not.
pub fn remove_even_read_only(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            let mut permissions = fs::metadata(path)?.permissions();
            if permissions.readonly() {
                #[expect(
                    clippy::permissions_set_readonly_false,
                    reason = "Windows only: the read-only attribute, not Unix modes"
                )]
                permissions.set_readonly(false);
                fs::set_permissions(path, permissions)?;
                fs::remove_file(path)
            } else {
                Err(e)
            }
        }
        other => other,
    }
}

/// What a symbolic link points at, which Windows must be told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// A file.
    File,
    /// A folder.
    Dir,
}

/// A symbolic link at `link` to `target`, made without elevation where Windows allows it
/// (Developer Mode); otherwise the error Windows gives, `ERROR_PRIVILEGE_NOT_HELD`.
///
/// # Errors
///
/// When Windows refuses, or `link` exists.
pub fn symlink(target: &Path, link: &Path, kind: LinkKind) -> io::Result<()> {
    // The standard library asks with SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE.
    match kind {
        LinkKind::File => std::os::windows::fs::symlink_file(target, link),
        LinkKind::Dir => std::os::windows::fs::symlink_dir(target, link),
    }
}

/// A second name `new` for the file `existing`.
///
/// # Errors
///
/// When Windows refuses: another volume, 1023 names already, no such file.
pub fn hard_link(existing: &Path, new: &Path) -> io::Result<()> {
    fs::hard_link(existing, new)
}

/// Sets a file's or a folder's times; a time left out stays as it is.
///
/// # Errors
///
/// When it cannot be opened to write its attributes, or Windows refuses the times.
pub fn set_times(path: &Path, times: &Times) -> io::Result<()> {
    use std::os::windows::fs::{FileTimesExt as _, OpenOptionsExt as _};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES,
    };
    let file = fs::OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let mut set = fs::FileTimes::new().set_modified(times.modified);
    if let Some(accessed) = times.accessed {
        set = set.set_accessed(accessed);
    }
    if let Some(created) = times.created {
        set = set.set_created(created);
    }
    file.set_times(set)
}

/// A file's Windows attributes.
///
/// # Errors
///
/// When its metadata cannot be read.
pub fn attributes(path: &Path) -> io::Result<u32> {
    Ok(fs::symlink_metadata(path)?.file_attributes())
}

/// Sets a file's Windows attributes, the ones a program may set ([`READ_ONLY`],
/// [`HIDDEN`], [`SYSTEM`], [`ARCHIVE`] and the rest `SetFileAttributesW` takes).
///
/// # Errors
///
/// When Windows refuses.
pub fn set_attributes(path: &Path, attributes: u32) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_NORMAL, SetFileAttributesW};
    let wide = crate::wide::to_wide_nul(path);
    let value = if attributes == 0 {
        FILE_ATTRIBUTE_NORMAL
    } else {
        attributes
    };
    // SAFETY: `wide` is a NUL-terminated wide path that outlives the call.
    if unsafe { SetFileAttributesW(wide.as_ptr(), value) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Why Windows cannot hold a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameProblem {
    /// Empty.
    Empty,
    /// A device's name: `CON`, `NUL`, `COM1`, … with any extension.
    Reserved,
    /// A character no file name may hold: `<>:"/\|?*` or a control character.
    Character(char),
    /// A trailing dot or space, which Windows drops.
    TrailingDotOrSpace,
}

/// Whether Windows can hold `name` as one component of a path, as given. `.` and `..`
/// are no names, and pass.
///
/// # Errors
///
/// Why it cannot.
pub fn check_name(name: &str) -> Result<(), NameProblem> {
    if name == "." || name == ".." {
        return Ok(());
    }
    if name.is_empty() {
        return Err(NameProblem::Empty);
    }
    if let Some(bad) = name.chars().find(|c| {
        matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control()
    }) {
        return Err(NameProblem::Character(bad));
    }
    if name.ends_with(['.', ' ']) {
        return Err(NameProblem::TrailingDotOrSpace);
    }
    if crate::path::is_reserved_name(Path::new(name)) {
        return Err(NameProblem::Reserved);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mode_follows_ls_rule() {
        assert_eq!(permissions(Kind::File, Some(true), false, false), 0o664);
        assert_eq!(permissions(Kind::File, Some(true), true, false), 0o444);
        assert_eq!(permissions(Kind::File, Some(false), false, true), 0o555);
        assert_eq!(permissions(Kind::File, None, false, true), 0o775);
        // A folder's read-only attribute does not stop writing in it.
        assert_eq!(permissions(Kind::Dir, Some(true), true, false), 0o775);
        assert_eq!(permissions(Kind::Symlink, Some(false), true, false), 0o777);
        assert_eq!(Kind::Dir.type_bits() | 0o755, 0o40_755);
    }

    #[test]
    fn modes_are_written_as_gnu_writes_them() {
        assert_eq!(mode_string(0o40_755), "drwxr-xr-x");
        assert_eq!(mode_string(0o100_644), "-rw-r--r--");
        assert_eq!(mode_string(0o120_777), "lrwxrwxrwx");
        assert_eq!(mode_string(0o104_755), "-rwsr-xr-x");
        assert_eq!(mode_string(0o102_644), "-rw-r-Sr--");
        assert_eq!(mode_string(0o41_777), "drwxrwxrwt");
        assert_eq!(mode_string(0o41_776), "drwxrwxrwT");
        assert_eq!(mode_string(0o20_620), "crw--w----");
        assert_eq!(mode_string(0o10_644), "prw-r--r--");
        assert_eq!(mode_string(0o60_660), "brw-rw----");
    }

    #[test]
    fn a_file_has_a_face_and_its_times_can_be_set() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.sh");
        fs::write(&file, b"#!/bin/sh\n").unwrap();
        let view = unix_view(&file, false).unwrap();
        assert_eq!(view.kind, Kind::File);
        assert_eq!(view.permissions & 0o111, 0o111, "{view:?}");
        assert_eq!(view.size, 10);
        assert!(!view.owner.name.is_empty(), "{view:?}");
        assert_eq!(view.links, 1);
        assert!(view.identity.is_some());

        let when = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
        let times = Times {
            modified: when,
            accessed: Some(when),
            created: None,
        };
        set_times(&file, &times).unwrap();
        set_times(dir.path(), &times).unwrap();
        assert_eq!(unix_view(&file, false).unwrap().times.modified, when);
        assert_eq!(unix_view(dir.path(), false).unwrap().times.modified, when);
        assert_eq!(unix_view(dir.path(), false).unwrap().kind, Kind::Dir);
    }

    #[test]
    fn a_replacement_appears_whole_or_not_at_all() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("t.txt");
        fs::write(&target, b"old").unwrap();
        let mut replacement = Replacement::create(target.clone()).unwrap();
        replacement.write_all(b"new").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"old");
        replacement
            .finish(fs::FileTimes::new(), true, false)
            .unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(fs::metadata(&target).unwrap().permissions().readonly());

        // Read-only, it is replaced all the same; given up, nothing is left.
        let mut again = Replacement::create(target.clone()).unwrap();
        again.write_all(b"newer").unwrap();
        again.finish(fs::FileTimes::new(), false, false).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"newer");
        let abandoned = Replacement::create(target).unwrap();
        abandoned.abandon();
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn attributes_are_set_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("h");
        fs::write(&file, b"").unwrap();
        set_attributes(&file, HIDDEN | READ_ONLY).unwrap();
        assert_eq!(
            attributes(&file).unwrap() & (HIDDEN | READ_ONLY),
            HIDDEN | READ_ONLY
        );
        set_attributes(&file, 0).unwrap();
        assert_eq!(attributes(&file).unwrap() & (HIDDEN | READ_ONLY), 0);
    }

    #[test]
    fn names_windows_cannot_hold() {
        assert_eq!(check_name("a.txt"), Ok(()));
        assert_eq!(check_name(".."), Ok(()));
        assert_eq!(check_name(""), Err(NameProblem::Empty));
        assert_eq!(check_name("a:b"), Err(NameProblem::Character(':')));
        assert_eq!(check_name("tab\there"), Err(NameProblem::Character('\t')));
        assert_eq!(check_name("dot."), Err(NameProblem::TrailingDotOrSpace));
        assert_eq!(check_name("space "), Err(NameProblem::TrailingDotOrSpace));
        assert_eq!(check_name("con"), Err(NameProblem::Reserved));
        assert_eq!(check_name("NUL.txt"), Err(NameProblem::Reserved));
        assert_eq!(check_name("console"), Ok(()));
    }

    #[test]
    fn hard_links_are_second_names() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("one");
        fs::write(&first, b"x").unwrap();
        hard_link(&first, &dir.path().join("two")).unwrap();
        assert_eq!(unix_view(&first, false).unwrap().links, 2);
    }
}
