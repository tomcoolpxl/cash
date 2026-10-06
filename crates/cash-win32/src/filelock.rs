//! Byte-range locks on an open file, for `flock`: `LockFileEx` and `UnlockFileEx`.
//!
//! Windows has no `flock(2)`. What it has are byte-range locks, and mandatory ones: a
//! byte one handle has locked cannot be read or written through any other handle, in any
//! process, this one included. So `flock` locks one byte far past any content a file
//! could have ([`Range::FAR_BYTE`]): a lock on
//! `queue.lock` stops nothing but another
//! `flock`, and `cat queue.lock` and a writer go on.
//!
//! A lock belongs to the handle it was taken on, not to the process: two handles on one
//! file contend even inside one process, and the lock goes when the handle is closed. A
//! child process never gets it, whether or not it inherits the handle.
//!
//! `LockFileEx` without `LOCKFILE_FAIL_IMMEDIATELY` blocks the thread with no way to
//! interrupt or time it out, so there is no blocking call here: a caller that waits tries
//! [`try_lock`] again until it succeeds.

use std::fs::File;
use std::io;
use std::os::windows::io::AsRawHandle as _;

use windows_sys::Win32::Foundation::{
    ERROR_IO_PENDING, ERROR_LOCK_VIOLATION, ERROR_NOT_LOCKED, GetLastError,
};
use windows_sys::Win32::Storage::FileSystem::{
    LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx, UnlockFileEx,
};
use windows_sys::Win32::System::IO::OVERLAPPED;

/// The bytes a lock covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    /// The first byte.
    pub offset: u64,
    /// How many bytes from it.
    pub length: u64,
}

impl Range {
    /// One byte at the far end of the 63-bit offsets a file can have, past any content a
    /// file could hold: `flock`'s lock, which no reader or writer of the file runs into.
    pub const FAR_BYTE: Self = Self {
        offset: 0x7FFF_FFFF_FFFF_FFFE,
        length: 1,
    };

    /// The bytes from `offset` to the end of what a file can hold, as a `--length` of
    /// zero means to `fcntl`.
    #[must_use]
    pub const fn from_offset(offset: u64) -> Self {
        Self {
            offset,
            length: 0x7FFF_FFFF_FFFF_FFFF_u64.saturating_sub(offset),
        }
    }
}

/// The `OVERLAPPED` that names where a lock starts: `LockFileEx` reads its offset and
/// nothing else, since a lock neither waits on an event nor returns a result.
const fn at(offset: u64) -> OVERLAPPED {
    // SAFETY: `OVERLAPPED` is plain data (integers, a pointer-sized union and a handle),
    // for which all zeroes is a valid value.
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    overlapped.Anonymous.Anonymous.Offset = low(offset);
    overlapped.Anonymous.Anonymous.OffsetHigh = high(offset);
    overlapped
}

#[allow(clippy::cast_possible_truncation, reason = "the low half is wanted")]
const fn low(value: u64) -> u32 {
    value as u32
}

#[allow(clippy::cast_possible_truncation, reason = "shifted into range")]
const fn high(value: u64) -> u32 {
    (value >> 32) as u32
}

/// Tries to lock `range` of `file` through its handle, at once.
///
/// `Ok(true)` when the lock is held, `Ok(false)` when another handle holds a lock that
/// conflicts (an exclusive one, or any one when this is to be exclusive).
///
/// The handle must have been opened for reading or writing, or both; a shared lock on a
/// read-only handle is fine. A lock on bytes this handle already holds is refused by
/// Windows like any other handle's: unlock first to convert one.
///
/// # Errors
///
/// Any failure other than a conflicting lock: a handle that is not a file's (a pipe, a
/// console, a directory), or one opened for neither reading nor writing.
pub fn try_lock(file: &File, exclusive: bool, range: Range) -> io::Result<bool> {
    let mut flags = LOCKFILE_FAIL_IMMEDIATELY;
    if exclusive {
        flags |= LOCKFILE_EXCLUSIVE_LOCK;
    }
    let mut overlapped = at(range.offset);
    // SAFETY: the handle is open for the life of `file`, and `overlapped` outlives the
    // call, which with `LOCKFILE_FAIL_IMMEDIATELY` returns before reading anything else.
    let locked = unsafe {
        LockFileEx(
            file.as_raw_handle(),
            flags,
            0,
            low(range.length),
            high(range.length),
            &raw mut overlapped,
        )
    };
    if locked != 0 {
        return Ok(true);
    }
    // SAFETY: a plain call, right after the failure it reports on.
    let code = unsafe { GetLastError() };
    if code == ERROR_LOCK_VIOLATION || code == ERROR_IO_PENDING {
        return Ok(false);
    }
    Err(io::Error::from_raw_os_error(code.cast_signed()))
}

/// Releases the lock this handle holds on `range` of `file`. A range the handle does not
/// hold is not an error, as `flock(LOCK_UN)` on an unlocked file is not.
///
/// # Errors
///
/// A handle that is not a file's.
pub fn unlock(file: &File, range: Range) -> io::Result<()> {
    let mut overlapped = at(range.offset);
    // SAFETY: the handle is open for the life of `file`, and `overlapped` outlives the
    // call, which completes before it returns.
    let unlocked = unsafe {
        UnlockFileEx(
            file.as_raw_handle(),
            0,
            low(range.length),
            high(range.length),
            &raw mut overlapped,
        )
    };
    if unlocked != 0 {
        return Ok(());
    }
    // SAFETY: a plain call, right after the failure it reports on.
    let code = unsafe { GetLastError() };
    if code == ERROR_NOT_LOCKED {
        return Ok(());
    }
    Err(io::Error::from_raw_os_error(code.cast_signed()))
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::{Read as _, Write as _};

    use super::{Range, try_lock, unlock};

    /// A folder with a file `lock` in it, and two handles on that file.
    struct Two {
        dir: tempfile::TempDir,
        a: std::fs::File,
        b: std::fs::File,
    }

    impl Two {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("lock");
            let open = || {
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(&path)
                    .unwrap()
            };
            let a = open();
            let b = open();
            Self { dir, a, b }
        }

        fn read_only(&self) -> std::fs::File {
            std::fs::File::open(self.dir.path().join("lock")).unwrap()
        }
    }

    #[test]
    fn an_exclusive_lock_is_held_by_one_handle_at_a_time() {
        let two = Two::new();
        assert!(try_lock(&two.a, true, Range::FAR_BYTE).unwrap());
        assert!(!try_lock(&two.b, true, Range::FAR_BYTE).unwrap());
        assert!(!try_lock(&two.b, false, Range::FAR_BYTE).unwrap());
        unlock(&two.a, Range::FAR_BYTE).unwrap();
        assert!(try_lock(&two.b, true, Range::FAR_BYTE).unwrap());
    }

    #[test]
    fn shared_locks_coexist_and_keep_an_exclusive_one_out() {
        let two = Two::new();
        assert!(try_lock(&two.a, false, Range::FAR_BYTE).unwrap());
        assert!(try_lock(&two.b, false, Range::FAR_BYTE).unwrap());
        unlock(&two.b, Range::FAR_BYTE).unwrap();
        assert!(!try_lock(&two.b, true, Range::FAR_BYTE).unwrap());
    }

    #[test]
    fn closing_the_handle_releases_the_lock() {
        let two = Two::new();
        assert!(try_lock(&two.a, true, Range::FAR_BYTE).unwrap());
        drop(two.a);
        assert!(try_lock(&two.b, true, Range::FAR_BYTE).unwrap());
    }

    #[test]
    fn unlocking_what_is_not_locked_is_fine() {
        let two = Two::new();
        unlock(&two.a, Range::FAR_BYTE).unwrap();
    }

    #[test]
    fn the_far_byte_leaves_the_content_readable_and_writable() {
        let mut two = Two::new();
        assert!(try_lock(&two.a, true, Range::FAR_BYTE).unwrap());
        two.b.write_all(b"content\n").unwrap();
        let mut read_back = String::new();
        two.read_only().read_to_string(&mut read_back).unwrap();
        assert_eq!(read_back, "content\n");
    }

    #[test]
    fn a_locked_range_at_the_front_is_mandatory() {
        let mut two = Two::new();
        two.b.write_all(b"content\n").unwrap();
        assert!(try_lock(&two.a, true, Range::from_offset(0)).unwrap());
        let mut read_back = String::new();
        assert!(two.read_only().read_to_string(&mut read_back).is_err());
    }

    #[test]
    fn either_lock_works_on_a_read_only_handle() {
        let two = Two::new();
        let first = two.read_only();
        let second = two.read_only();
        assert!(try_lock(&first, true, Range::FAR_BYTE).unwrap());
        assert!(!try_lock(&second, false, Range::FAR_BYTE).unwrap());
        unlock(&first, Range::FAR_BYTE).unwrap();
        assert!(try_lock(&second, false, Range::FAR_BYTE).unwrap());
    }
}
