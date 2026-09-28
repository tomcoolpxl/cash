//! Which processes hold a file open, through the Restart Manager.
//!
//! The Restart Manager (`rstrtmgr.dll`) is the documented API behind Explorer's "file in
//! use" dialog and installers' "close these applications" prompts. Given file paths, it
//! reports the processes that have them open, including files opened with full sharing
//! and running executables. It does not need elevation for the caller's own processes.
//!
//! Measured limits, which `fuser` and `lsof` document rather than hide:
//!
//! * a directory cannot be registered (the call fails with access denied), so a process
//!   whose only use of a directory is having it as its working directory is invisible;
//! * the report is the union over every registered file, so attributing holders to one
//!   file means one query per file.

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS, WIN32_ERROR};
use windows_sys::Win32::System::RestartManager::{
    CCH_RM_SESSION_KEY, RM_PROCESS_INFO, RmEndSession, RmGetList, RmRegisterResources,
    RmStartSession,
};

/// A process that holds at least one of the queried files open.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Holder {
    /// The Windows process id — the one `kill` takes.
    pub pid: u32,
    /// The application's display name as the Restart Manager knows it.
    pub app_name: String,
    /// The service's short name when the holder is a Windows service.
    pub service: Option<String>,
}

/// The Restart Manager registers files in batches; this keeps each call's argument
/// array small.
const REGISTER_BATCH: usize = 512;

/// How many times [`holders`] asks again when the list outgrew the room it gave.
const MORE_DATA_ATTEMPTS: u32 = 8;

fn check(code: WIN32_ERROR) -> io::Result<()> {
    if code == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code.cast_signed()))
    }
}

/// Ends the session when dropped, on every path out of [`holders`].
struct Session(u32);

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful RmStartSession and is ended once.
        unsafe { RmEndSession(self.0) };
    }
}

fn decode(raw: &[u16]) -> String {
    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
    String::from_utf16_lossy(&raw[..end])
}

/// The processes holding any of `paths` open, ordered by pid.
///
/// # Errors
///
/// Fails when the Restart Manager does — in particular with access denied for a
/// directory, which it cannot register.
pub fn holders(paths: &[&Path]) -> io::Result<Vec<Holder>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut handle = 0u32;
    let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
    // SAFETY: `handle` and `key` are valid for writes; `key` has the documented
    // CCH_RM_SESSION_KEY + 1 elements.
    check(unsafe { RmStartSession(&raw mut handle, 0, key.as_mut_ptr()) })?;
    let session = Session(handle);

    for batch in paths.chunks(REGISTER_BATCH) {
        let wide: Vec<Vec<u16>> = batch
            .iter()
            .map(|path| {
                path.as_os_str()
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect()
            })
            .collect();
        let pointers: Vec<*const u16> = wide.iter().map(|w| w.as_ptr()).collect();
        let count = u32::try_from(pointers.len()).unwrap_or(u32::MAX);
        // SAFETY: `pointers` holds `count` NUL-terminated UTF-16 strings that outlive the
        // call; no applications or services are registered.
        check(unsafe {
            RmRegisterResources(
                session.0,
                count,
                pointers.as_ptr(),
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
            )
        })?;
    }

    // The list can grow between the sizing call and the real one, so retry, with room
    // for the processes that start meanwhile. A list that never settles is an error: the
    // buffer holds zeroed entries, and returning them reported pid 0 as a holder.
    let mut infos: Vec<RM_PROCESS_INFO> = Vec::new();
    let mut attempts = 0;
    loop {
        attempts += 1;
        let mut needed = 0u32;
        let mut count = u32::try_from(infos.len()).unwrap_or(u32::MAX);
        let mut reasons = 0u32;
        // SAFETY: `infos` has `count` writable elements (or none, with a null pointer
        // when empty), and the out parameters are valid for writes.
        let code = unsafe {
            RmGetList(
                session.0,
                &raw mut needed,
                &raw mut count,
                if infos.is_empty() {
                    std::ptr::null_mut()
                } else {
                    infos.as_mut_ptr()
                },
                &raw mut reasons,
            )
        };
        if code == ERROR_MORE_DATA && attempts < MORE_DATA_ATTEMPTS {
            let room = needed as usize;
            infos = vec![RM_PROCESS_INFO::default(); room + room / 2 + 8];
            continue;
        }
        check(code)?;
        infos.truncate(count as usize);
        break;
    }

    let mut result: Vec<Holder> = infos
        .iter()
        .map(|info| {
            let service = decode(&info.strServiceShortName);
            Holder {
                pid: info.Process.dwProcessId,
                app_name: decode(&info.strAppName),
                service: (!service.is_empty()).then_some(service),
            }
        })
        .collect();
    result.sort_by_key(|holder| holder.pid);
    result.dedup_by_key(|holder| holder.pid);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unused_file_has_no_holders() {
        let dir = std::env::temp_dir().join(format!("cash-rm-unused-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("unused.txt");
        std::fs::write(&file, "x").unwrap();
        assert_eq!(holders(&[file.as_path()]).unwrap(), Vec::new());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_file_this_process_holds_names_this_process() {
        let dir = std::env::temp_dir().join(format!("cash-rm-held-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("held.txt");
        std::fs::write(&file, "x").unwrap();
        let open = std::fs::File::open(&file).unwrap();
        let found = holders(&[file.as_path()]).unwrap();
        assert!(
            found.iter().any(|h| h.pid == std::process::id()),
            "{found:?}"
        );
        drop(open);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn nothing_to_query_is_not_an_error() {
        assert_eq!(holders(&[]).unwrap(), Vec::new());
    }
}
