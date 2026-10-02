//! `/dev/zero`, `/dev/random` and `/dev/urandom`: input that never ends (**D7**).
//!
//! Windows has no such devices. `head -c 16 /dev/urandom` and
//! `dd if=/dev/zero of=f bs=1M count=8` are the common uses, and the second needs every
//! read to be answered in full, as a device answers it: `dd` counts each read as a block,
//! and a pipe hands over what it holds, so it made 512 KiB of the 8 MiB.
//!
//! - `/dev/zero` is a temp file a tebibyte long and all hole: sparse, so it takes no
//!   space, and read in full blocks of zeros. Windows deletes it when it is closed. Where
//!   the temp folder cannot hold a sparse file, it is the pipe below.
//! - `/dev/random` and `/dev/urandom` are the read end of a pipe that a thread of the
//!   process keeps full of the system's random bytes, a mebibyte at a time, until the
//!   reader closes it; the thread then ends. A read of more than the pipe holds can come
//!   back short.
//!
//! Written to, each discards, as the null device does.

use std::fs::File;
use std::io::{self, Write};
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::sync::atomic::{AtomicU64, Ordering};

use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

/// What an endless input gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endless {
    /// Zeros, `/dev/zero`.
    Zeros,
    /// The system's random bytes, `/dev/random` and `/dev/urandom` alike, as on Linux
    /// since 5.6.
    Random,
}

/// How much the pipe holds, and is written into it at a time.
const CHUNK: usize = 1 << 20;

/// How long `/dev/zero`'s file is.
const ZEROS: u64 = 1 << 40;

/// Opens `kind` for reading.
///
/// # Errors
///
/// Returns an error if neither the file nor the pipe can be made, or the thread started.
pub fn open(kind: Endless) -> io::Result<File> {
    if kind == Endless::Zeros
        && let Ok(file) = zero_file()
    {
        return Ok(file);
    }
    let (reader, mut writer) = pipe()?;
    std::thread::Builder::new()
        .name("cash-dev-endless".into())
        .spawn(move || {
            let mut chunk = vec![0u8; CHUNK];
            loop {
                if kind == Endless::Random && !fill_random(&mut chunk) {
                    break;
                }
                // Fails once the reader has closed its end: the input is no longer read.
                if writer.write_all(&chunk).is_err() {
                    break;
                }
            }
        })?;
    Ok(reader)
}

/// A temp file of [`ZEROS`] bytes, all of them a hole, deleted when it is closed.
fn zero_file() -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    /// Transient: kept in the cache, written out only under memory pressure.
    const FILE_ATTRIBUTE_TEMPORARY: u32 = 0x0000_0100;
    /// Deleted by Windows when the last handle is closed, even if the process is ended.
    const FILE_FLAG_DELETE_ON_CLOSE: u32 = 0x0400_0000;
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("cash-dev-zero-{}-{n}", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .attributes(FILE_ATTRIBUTE_TEMPORARY)
        .custom_flags(FILE_FLAG_DELETE_ON_CLOSE)
        .open(path)?;
    crate::pipe::make_sparse(&file)?;
    file.set_len(ZEROS)?;
    Ok(file)
}

/// An anonymous pipe that holds [`CHUNK`] bytes: its read end and its write end.
fn pipe() -> io::Result<(File, File)> {
    use windows_sys::Win32::System::Pipes::CreatePipe;

    let mut reader = std::ptr::null_mut();
    let mut writer = std::ptr::null_mut();
    let size = u32::try_from(CHUNK).unwrap_or(0);
    // SAFETY: both out-params are valid; no inheritance is asked for.
    if unsafe { CreatePipe(&raw mut reader, &raw mut writer, std::ptr::null(), size) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: both handles were just made, and each is owned by one file from here.
    let reader = unsafe { File::from(OwnedHandle::from_raw_handle(reader)) };
    // SAFETY: as above.
    let writer = unsafe { File::from(OwnedHandle::from_raw_handle(writer)) };
    Ok((reader, writer))
}

/// Fills `buffer` with the system's random bytes; `false` if it could not.
fn fill_random(buffer: &mut [u8]) -> bool {
    let Ok(length) = u32::try_from(buffer.len()) else {
        return false;
    };
    // SAFETY: `buffer` is valid for `length` bytes; with the system-preferred flag no
    // algorithm handle is needed.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buffer.as_mut_ptr(),
            length,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    status >= 0
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    #[test]
    fn zeros_are_zeros_and_random_is_not() {
        let mut zeros = vec![1u8; 100_000];
        open(Endless::Zeros)
            .unwrap()
            .read_exact(&mut zeros)
            .unwrap();
        assert!(zeros.iter().all(|&byte| byte == 0));

        let mut random = [0u8; 4096];
        open(Endless::Random)
            .unwrap()
            .read_exact(&mut random)
            .unwrap();
        assert!(random.iter().any(|&byte| byte != 0));
        assert!(random.iter().any(|&byte| byte != random[0]));
    }
}
