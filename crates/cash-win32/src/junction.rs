//! Directory junctions, as `mklink /J` makes them: the installer's `current` folder.
//!
//! A junction is a folder whose reparse point names another folder. Unlike a symbolic
//! link it needs no privilege, and unlike a copy of the files it changes in one step:
//! setting a junction's reparse data in place is one call, so a `current` that is being
//! pointed at a new version is never missing or half-written. A running `cash.exe` under
//! the old version folder keeps running; the file itself is not touched.

use std::io;
use std::os::windows::io::AsRawHandle as _;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{
    ERROR_DIR_NOT_EMPTY, ERROR_REPARSE_TAG_MISMATCH, GENERIC_WRITE,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;

/// The reparse tag of a junction (`IO_REPARSE_TAG_MOUNT_POINT`).
const MOUNT_POINT: u32 = 0xA000_0003;

/// Point the junction `link` at the folder `target`: made when there is none, repointed
/// in place when there is one, and an empty folder at `link` becomes the junction.
///
/// # Errors
///
/// `target` is not a folder; `link` is a file, or a folder with something in it; or
/// Windows refused.
pub fn point(link: &Path, target: &Path) -> io::Result<()> {
    let target = std::fs::canonicalize(target)?;
    if !target.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            format!("{} is not a folder", crate::path::render(&target)),
        ));
    }
    match std::fs::symlink_metadata(link) {
        // A junction, or a symbolic link, which the tag mismatch below replaces.
        Ok(meta) if meta.file_type().is_symlink() || meta.is_dir() => {}
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} is a file, not a folder", crate::path::render(link)),
            ));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => std::fs::create_dir(link)?,
        Err(e) => return Err(e),
    }
    match set_reparse_point(link, &target) {
        Err(e) if e.raw_os_error() == Some(raw(ERROR_REPARSE_TAG_MISMATCH)) => {
            // A symbolic link where the junction should be: take it away and start over.
            std::fs::remove_dir(link)?;
            std::fs::create_dir(link)?;
            set_reparse_point(link, &target)
        }
        Err(e) if e.raw_os_error() == Some(raw(ERROR_DIR_NOT_EMPTY)) => Err(io::Error::new(
            io::ErrorKind::DirectoryNotEmpty,
            format!(
                "{} is a folder with files in it, where the junction should be",
                crate::path::render(link)
            ),
        )),
        other => other,
    }
}

/// The folder a junction (or a symbolic link to a folder) names, without the `\\?\`
/// prefix the reparse data carries; `None` for anything else.
#[must_use]
pub fn target(link: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_link(link).ok()?;
    Some(PathBuf::from(plain(&raw)))
}

/// Whether `path` is a junction or a symbolic link to a folder.
#[must_use]
pub fn is_junction(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
        && std::fs::read_link(path).is_ok()
}

/// Whether the junction `link` names the folder `target`, spelled any way.
#[must_use]
pub fn points_at(link: &Path, target: &Path) -> bool {
    let Some(named) = self::target(link) else {
        return false;
    };
    let Ok(target) = std::fs::canonicalize(target) else {
        return false;
    };
    plain(&named).eq_ignore_ascii_case(&plain(&target))
}

/// A path's text without a `\\?\` or `\??\` prefix, backslashes throughout and no
/// trailing one.
fn plain(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\");
    let text = text
        .strip_prefix(r"\\?\")
        .or_else(|| text.strip_prefix(r"\??\"))
        .unwrap_or(&text);
    text.trim_end_matches('\\').to_owned()
}

fn raw(error: u32) -> i32 {
    i32::try_from(error).unwrap_or(i32::MAX)
}

/// The `REPARSE_DATA_BUFFER` of a junction to `target`: the substitute name Windows
/// follows (`\??\C:\…`) and the print name it shows, each NUL-terminated UTF-16.
fn reparse_data(target: &Path) -> io::Result<Vec<u8>> {
    let shown = plain(target);
    let substitute: Vec<u16> = format!(r"\??\{shown}").encode_utf16().collect();
    let print: Vec<u16> = shown.encode_utf16().collect();
    let substitute_bytes = substitute.len() * 2;
    let print_bytes = print.len() * 2;
    // The four offset and length fields, then both names with their terminators.
    let data_length = 8 + substitute_bytes + 2 + print_bytes + 2;
    let field = |n: usize| {
        u16::try_from(n)
            .map_err(|_| io::Error::other("the folder's path is too long for a junction"))
    };
    let mut buffer = Vec::with_capacity(8 + data_length);
    buffer.extend(MOUNT_POINT.to_le_bytes());
    buffer.extend(field(data_length)?.to_le_bytes());
    buffer.extend(0u16.to_le_bytes()); // Reserved
    buffer.extend(0u16.to_le_bytes()); // SubstituteNameOffset
    buffer.extend(field(substitute_bytes)?.to_le_bytes());
    buffer.extend(field(substitute_bytes + 2)?.to_le_bytes()); // PrintNameOffset
    buffer.extend(field(print_bytes)?.to_le_bytes());
    buffer.extend(substitute.iter().flat_map(|unit| unit.to_le_bytes()));
    buffer.extend([0, 0]);
    buffer.extend(print.iter().flat_map(|unit| unit.to_le_bytes()));
    buffer.extend([0, 0]);
    Ok(buffer)
}

/// Set `link`'s reparse point to a junction naming `target`. `link` is a folder: empty,
/// or a junction already, whose data this replaces.
fn set_reparse_point(link: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt as _;

    let data = reparse_data(target)?;
    let folder = std::fs::OpenOptions::new()
        .access_mode(GENERIC_WRITE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(link)?;
    let handle = folder.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
    let size = u32::try_from(data.len()).map_err(|_| io::Error::other("reparse data too long"))?;
    let mut returned = 0u32;
    // SAFETY: `handle` is open on the folder for writing; `data` is a complete
    // REPARSE_DATA_BUFFER of `size` bytes, which the call only reads (the kernel copies a
    // buffered FSCTL's input, so its alignment does not matter); no output buffer is
    // asked for, and `returned` receives the count.
    let ok = unsafe {
        DeviceIoControl(
            handle,
            FSCTL_SET_REPARSE_POINT,
            data.as_ptr().cast(),
            size,
            std::ptr::null_mut(),
            0,
            &raw mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_junction_is_made_followed_and_repointed_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let one = dir.path().join("1.0.0");
        let two = dir.path().join("2.0.0");
        std::fs::create_dir(&one).unwrap();
        std::fs::create_dir(&two).unwrap();
        std::fs::write(one.join("marker"), "one").unwrap();
        std::fs::write(two.join("marker"), "two").unwrap();
        let link = dir.path().join("current");

        point(&link, &one).unwrap();
        assert!(is_junction(&link));
        assert!(points_at(&link, &one), "{:?}", target(&link));
        assert_eq!(std::fs::read_to_string(link.join("marker")).unwrap(), "one");

        point(&link, &two).unwrap();
        assert!(points_at(&link, &two), "{:?}", target(&link));
        assert!(!points_at(&link, &one));
        assert_eq!(std::fs::read_to_string(link.join("marker")).unwrap(), "two");

        // Removing the junction leaves its target alone.
        std::fs::remove_dir(&link).unwrap();
        assert!(!link.exists());
        assert!(two.join("marker").is_file());
        assert_eq!(target(&link), None);
        assert!(!is_junction(&two));
    }

    #[test]
    fn a_folder_with_files_in_it_and_a_file_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let target_dir = dir.path().join("v");
        std::fs::create_dir(&target_dir).unwrap();

        let full = dir.path().join("full");
        std::fs::create_dir(&full).unwrap();
        std::fs::write(full.join("x"), "").unwrap();
        let error = point(&full, &target_dir).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::DirectoryNotEmpty, "{error}");
        assert!(full.join("x").is_file());

        let file = dir.path().join("file");
        std::fs::write(&file, "").unwrap();
        let error = point(&file, &target_dir).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists, "{error}");

        let error = point(&dir.path().join("link"), &file).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotADirectory, "{error}");
    }

    #[test]
    fn the_reparse_data_names_the_folder_twice() {
        let data = reparse_data(Path::new(r"\\?\C:\x\y")).unwrap();
        assert_eq!(&data[..4], &MOUNT_POINT.to_le_bytes());
        let text = |from: usize, len: usize| {
            let units: Vec<u16> = data[from..from + len]
                .chunks(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            String::from_utf16(&units).unwrap()
        };
        let substitute_len = usize::from(u16::from_le_bytes([data[10], data[11]]));
        let print_offset = usize::from(u16::from_le_bytes([data[12], data[13]]));
        let print_len = usize::from(u16::from_le_bytes([data[14], data[15]]));
        assert_eq!(text(16, substitute_len), r"\??\C:\x\y");
        assert_eq!(text(16 + print_offset, print_len), r"C:\x\y");
        assert_eq!(
            usize::from(u16::from_le_bytes([data[4], data[5]])),
            data.len() - 8
        );
    }
}
