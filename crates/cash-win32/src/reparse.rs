//! Reparse points as they are: the `REPARSE_DATA_BUFFER` of a symbolic link or a
//! junction (`FSCTL_GET_REPARSE_POINT`), and the path it names.

use std::io;
use std::os::windows::io::AsRawHandle as _;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{FSCTL_GET_REPARSE_POINT, FSCTL_SET_REPARSE_POINT};

/// The reparse tag of a junction (`IO_REPARSE_TAG_MOUNT_POINT`).
pub const MOUNT_POINT: u32 = 0xA000_0003;
/// The reparse tag of a symbolic link (`IO_REPARSE_TAG_SYMLINK`).
pub const SYMLINK: u32 = 0xA000_000C;

/// The largest reparse buffer Windows keeps (`MAXIMUM_REPARSE_DATA_BUFFER_SIZE`).
const MAX_SIZE: usize = 16 * 1024;

/// The reparse data of the link at `path`, which is not followed.
///
/// # Errors
///
/// When it cannot be opened, or is no reparse point.
pub fn data(path: &Path) -> io::Result<Vec<u8>> {
    use std::os::windows::fs::OpenOptionsExt as _;

    let file = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let handle = file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
    let mut buffer = vec![0u8; MAX_SIZE];
    let mut returned = 0u32;
    // SAFETY: `handle` is open on the link; the output buffer is `MAX_SIZE` bytes, the
    // largest reparse buffer there is, and `returned` receives how much was written.
    let ok = unsafe {
        DeviceIoControl(
            handle,
            FSCTL_GET_REPARSE_POINT,
            std::ptr::null(),
            0,
            buffer.as_mut_ptr().cast(),
            u32::try_from(MAX_SIZE).unwrap_or(u32::MAX),
            &raw mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    buffer.truncate(usize::try_from(returned).unwrap_or(0));
    // What the header says it holds, without the padding Windows may give after.
    if let Some(stated) = le16(&buffer, 4) {
        buffer.truncate(8 + stated);
    }
    Ok(buffer)
}

/// What a link's reparse data names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// The path, as 7-Zip's `CReparseAttr::GetPath` gives it: the substitute name, its
    /// `\??\` taken off before a drive and made `\\?\` before anything else.
    pub path: String,
    /// The substitute name as the data holds it, `\??\` and all: what `WinRAR` stores.
    pub substitute: String,
    /// A symbolic link whose path is relative to its folder.
    pub relative: bool,
    /// A junction rather than a symbolic link.
    pub junction: bool,
}

fn le16(data: &[u8], at: usize) -> Option<usize> {
    Some(usize::from(u16::from_le_bytes([
        *data.get(at)?,
        *data.get(at + 1)?,
    ])))
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_le_bytes(c))
        .collect();
    String::from_utf16_lossy(&units)
}

/// The path a junction's or a symbolic link's reparse data names; `None` for other
/// reparse points and damaged data.
#[must_use]
pub fn target(data: &[u8]) -> Option<Target> {
    let tag = u32::from_le_bytes(data.get(..4)?.try_into().ok()?);
    let (names_at, junction) = match tag {
        MOUNT_POINT => (16, true),
        SYMLINK => (20, false),
        _ => return None,
    };
    let name = |offset: usize, length: usize| -> Option<String> {
        let start = names_at + le16(data, offset)?;
        Some(utf16(data.get(start..start + le16(data, length)?)?))
    };
    let substitute = name(8, 10)?;
    let relative = !junction
        && data
            .get(16..20)
            .is_some_and(|f| u32::from_le_bytes([f[0], f[1], f[2], f[3]]) & 1 != 0);
    let path = match substitute.strip_prefix(r"\??\") {
        Some(rest) if is_drive(rest) && matches!(rest.as_bytes().get(2), Some(b'\\' | b'/')) => {
            rest.to_owned()
        }
        Some(rest) => format!(r"\\?\{rest}"),
        None => substitute.clone(),
    };
    Some(Target {
        path,
        substitute,
        relative,
        junction,
    })
}

/// Whether `path` is absolute as Windows names go: a separator or a drive first.
fn is_absolute(path: &str) -> bool {
    path.starts_with(['\\', '/']) || is_drive(path)
}

/// A letter and a colon.
const fn is_drive(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// The reparse data of a symbolic link (or, with `junction`, a junction) naming
/// `target`, as 7-Zip's `FillLinkData` builds it.
///
/// An absolute path is substituted with `\??\` before it, a relative one is flagged so;
/// each name ends in a NUL.
#[must_use]
pub fn link_data(target: &str, junction: bool) -> Vec<u8> {
    let absolute = is_absolute(target);
    let substitute = if absolute && !target.starts_with(r"\\?\") {
        if let Some(unc) = target.strip_prefix(r"\\") {
            format!(r"\??\UNC\{unc}")
        } else {
            format!(r"\??\{target}")
        }
    } else {
        target.replace(r"\\?\", r"\??\")
    };
    let wide = |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(u16::to_le_bytes).collect() };
    let (sub, print) = (wide(&substitute), wide(target));
    let fixed: usize = if junction { 8 } else { 12 };
    let len = fixed + sub.len() + 2 + print.len() + 2;
    let field = |n: usize| u16::try_from(n).unwrap_or(u16::MAX).to_le_bytes();
    let mut data = Vec::with_capacity(8 + len);
    data.extend((if junction { MOUNT_POINT } else { SYMLINK }).to_le_bytes());
    data.extend(field(len));
    data.extend([0, 0]);
    data.extend(field(0));
    data.extend(field(sub.len()));
    data.extend(field(sub.len() + 2));
    data.extend(field(print.len()));
    if !junction {
        data.extend(u32::from(!absolute).to_le_bytes());
    }
    data.extend(sub);
    data.extend([0, 0]);
    data.extend(print);
    data.extend([0, 0]);
    data
}

/// Makes `path` the link `data` describes (`SetReparseData`): the file or folder there,
/// else a new one, gets the reparse data.
///
/// # Errors
///
/// When it cannot be made or opened, or Windows refuses the data: without the right to
/// make symbolic links, `ERROR_PRIVILEGE_NOT_HELD`.
pub fn set(path: &Path, is_dir: bool, data: &[u8]) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt as _;

    let open = || {
        std::fs::OpenOptions::new()
            .write(true)
            .share_mode(FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    };
    let file = match open() {
        Ok(file) => file,
        Err(_) if is_dir => {
            std::fs::create_dir(path)?;
            open()?
        }
        Err(_) => std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?,
    };
    let handle = file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
    let mut returned = 0u32;
    // SAFETY: `handle` is open for writing on the file or folder; the input is `data`,
    // its length given, and nothing is asked back.
    let ok = unsafe {
        DeviceIoControl(
            handle,
            FSCTL_SET_REPARSE_POINT,
            data.as_ptr().cast(),
            u32::try_from(data.len()).unwrap_or(u32::MAX),
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

    /// A reparse buffer as Windows writes one.
    fn buffer(tag: u32, substitute: &str, print: &str, flags: Option<u32>) -> Vec<u8> {
        let s: Vec<u8> = substitute
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let p: Vec<u8> = print.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut b = tag.to_le_bytes().to_vec();
        let header = if flags.is_some() { 12 } else { 8 };
        let len = header + s.len() + p.len();
        b.extend(u16::try_from(len).unwrap().to_le_bytes());
        b.extend([0, 0]);
        b.extend(0u16.to_le_bytes());
        b.extend(u16::try_from(s.len()).unwrap().to_le_bytes());
        b.extend(u16::try_from(s.len()).unwrap().to_le_bytes());
        b.extend(u16::try_from(p.len()).unwrap().to_le_bytes());
        if let Some(f) = flags {
            b.extend(f.to_le_bytes());
        }
        b.extend(s);
        b.extend(p);
        b
    }

    #[test]
    fn links_name_their_paths_as_7_zip_reads_them() {
        let junction = buffer(MOUNT_POINT, r"\??\C:\d\sub", r"C:\d\sub", None);
        assert_eq!(
            target(&junction),
            Some(Target {
                path: r"C:\d\sub".to_owned(),
                substitute: r"\??\C:\d\sub".to_owned(),
                relative: false,
                junction: true,
            })
        );
        let relative = buffer(SYMLINK, "a.txt", "a.txt", Some(1));
        assert_eq!(
            target(&relative),
            Some(Target {
                path: "a.txt".to_owned(),
                substitute: "a.txt".to_owned(),
                relative: true,
                junction: false,
            })
        );
        assert_eq!(target(&[0, 0, 0, 0x80]), None);
        // PowerShell's junctions have no print name.
        let bare = buffer(MOUNT_POINT, r"\??\C:\d\sub", "", None);
        assert_eq!(target(&bare).unwrap().path, r"C:\d\sub");
        let volume = buffer(MOUNT_POINT, r"\??\Volume{1}\", "", None);
        assert_eq!(target(&volume).unwrap().path, r"\\?\Volume{1}\");
    }

    #[test]
    fn link_data_names_what_target_reads() {
        let relative = target(&link_data(r"..\a.txt", false)).unwrap();
        assert_eq!(relative.path, r"..\a.txt");
        assert!(relative.relative);
        let absolute = target(&link_data(r"C:\d\a.txt", false)).unwrap();
        assert_eq!(absolute.path, r"C:\d\a.txt");
        assert!(!absolute.relative && !absolute.junction);
        assert!(target(&link_data(r"C:\d", true)).unwrap().junction);
    }

    /// A symbolic link is made where Windows allows it; elsewhere the refusal is the
    /// missing right, and the folder or file is there, as 7-Zip leaves it.
    #[test]
    fn a_symbolic_link_is_set_or_refused_for_the_right() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "alpha").unwrap();
        let link = dir.path().join("sl");
        match set(&link, false, &link_data("a.txt", false)) {
            Ok(()) => {
                let named = target(&data(&link).unwrap()).unwrap();
                assert_eq!(named.path, "a.txt");
                assert_eq!(std::fs::read_to_string(&link).unwrap(), "alpha");
            }
            Err(error) => {
                assert_eq!(error.raw_os_error(), Some(1314), "{error}");
                assert!(link.is_file());
            }
        }
        let dlink = dir.path().join("dl");
        if set(&dlink, true, &link_data(".", false)).is_err() {
            assert!(dlink.is_dir());
        }
    }

    #[test]
    fn a_junction_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let link = dir.path().join("jn");
        crate::junction::point(&link, &sub).unwrap();
        let named = target(&data(&link).unwrap()).unwrap();
        assert!(named.junction);
        let shown = std::fs::canonicalize(&sub).unwrap();
        let shown = shown.to_string_lossy();
        assert!(
            named
                .path
                .eq_ignore_ascii_case(shown.trim_start_matches(r"\\?\")),
            "{named:?}"
        );
    }
}
