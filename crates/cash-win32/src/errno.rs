//! Windows' errors in the words of the C library's `strerror`, as the GNU tools print
//! them: `No such file or directory`, not "The system cannot find the file specified.
//! (os error 2)".
//!
//! A Windows error code is first given the errno Cygwin gives it, which is what Git
//! Bash's tools then print; a code with none is read by its [`std::io::ErrorKind`]; what
//! is left keeps Windows' own words, without Rust's `(os error N)`.

use std::io;

/// The words for `error`, as glibc's `strerror` words its errno.
#[must_use]
pub fn strerror(error: &io::Error) -> String {
    if let Some(text) = error.raw_os_error().and_then(by_code) {
        return text.to_owned();
    }
    if let Some(text) = by_kind(error.kind()) {
        return text.to_owned();
    }
    let text = error.to_string();
    match text.find(" (os error ") {
        Some(at) => text.get(..at).unwrap_or_default().to_owned(),
        None => text,
    }
}

/// The errno words for a Windows error code, as Cygwin maps the code.
const fn by_code(code: i32) -> Option<&'static str> {
    Some(match code {
        // ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_INVALID_NAME,
        // ERROR_BAD_NETPATH, ERROR_BAD_NET_NAME, ERROR_BAD_PATHNAME
        2 | 3 | 53 | 67 | 123 | 161 => "No such file or directory",
        // ERROR_TOO_MANY_OPEN_FILES
        4 => "Too many open files",
        // ERROR_ACCESS_DENIED, ERROR_NETWORK_ACCESS_DENIED, ERROR_CANNOT_MAKE
        5 | 65 | 82 => "Permission denied",
        // ERROR_INVALID_HANDLE
        6 => "Bad file descriptor",
        // ERROR_NOT_ENOUGH_MEMORY, ERROR_OUTOFMEMORY
        8 | 14 => "Cannot allocate memory",
        // ERROR_INVALID_DRIVE
        15 => "No such device",
        // ERROR_NOT_SAME_DEVICE
        17 => "Invalid cross-device link",
        // ERROR_WRITE_PROTECT
        19 => "Read-only file system",
        // ERROR_NOT_READY
        21 => "No medium found",
        // ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION
        32 | 33 => "Device or resource busy",
        // ERROR_HANDLE_DISK_FULL, ERROR_DISK_FULL
        39 | 112 => "No space left on device",
        // ERROR_NOT_SUPPORTED
        50 => "Operation not supported",
        // ERROR_FILE_EXISTS, ERROR_ALREADY_EXISTS
        80 | 183 => "File exists",
        // ERROR_INVALID_PARAMETER, ERROR_NEGATIVE_SEEK
        87 | 131 => "Invalid argument",
        // ERROR_BROKEN_PIPE, ERROR_NO_DATA
        109 | 232 => "Broken pipe",
        // ERROR_DIR_NOT_EMPTY
        145 => "Directory not empty",
        // ERROR_FILENAME_EXCED_RANGE
        206 => "File name too long",
        // ERROR_DIRECTORY
        267 => "Not a directory",
        // ERROR_PRIVILEGE_NOT_HELD
        1314 => "Operation not permitted",
        // ERROR_CANT_RESOLVE_FILENAME
        1921 => "Too many levels of symbolic links",
        _ => return None,
    })
}

/// The errno words for an error that carries no Windows code, by its kind.
const fn by_kind(kind: io::ErrorKind) -> Option<&'static str> {
    use io::ErrorKind;
    Some(match kind {
        ErrorKind::NotFound => "No such file or directory",
        ErrorKind::PermissionDenied => "Permission denied",
        ErrorKind::AlreadyExists => "File exists",
        ErrorKind::DirectoryNotEmpty => "Directory not empty",
        ErrorKind::IsADirectory => "Is a directory",
        ErrorKind::NotADirectory => "Not a directory",
        ErrorKind::ResourceBusy => "Device or resource busy",
        ErrorKind::ReadOnlyFilesystem => "Read-only file system",
        ErrorKind::StorageFull => "No space left on device",
        ErrorKind::CrossesDevices => "Invalid cross-device link",
        ErrorKind::InvalidFilename | ErrorKind::InvalidInput => "Invalid argument",
        ErrorKind::BrokenPipe => "Broken pipe",
        ErrorKind::Interrupted => "Interrupted system call",
        ErrorKind::WouldBlock => "Resource temporarily unavailable",
        ErrorKind::TimedOut => "Connection timed out",
        ErrorKind::ConnectionRefused => "Connection refused",
        ErrorKind::ExecutableFileBusy => "Text file busy",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::strerror;
    use std::io;

    #[test]
    fn windows_codes_take_cygwins_errno() {
        for (code, words) in [
            (2, "No such file or directory"),
            (3, "No such file or directory"),
            (5, "Permission denied"),
            (32, "Device or resource busy"),
            (145, "Directory not empty"),
            (183, "File exists"),
            (267, "Not a directory"),
            (1314, "Operation not permitted"),
        ] {
            assert_eq!(
                strerror(&io::Error::from_raw_os_error(code)),
                words,
                "{code}"
            );
        }
    }

    #[test]
    fn a_kind_without_a_code_and_the_rest_keep_their_words() {
        assert_eq!(
            strerror(&io::Error::from(io::ErrorKind::NotFound)),
            "No such file or directory"
        );
        assert_eq!(
            strerror(&io::Error::other("its own words")),
            "its own words"
        );
        // A code Cygwin gives no errno keeps Windows' words, without `(os error N)`.
        let unmapped = strerror(&io::Error::from_raw_os_error(1_223));
        assert!(!unmapped.contains("os error"), "{unmapped}");
    }
}
