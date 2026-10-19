//! The machine's logon sessions, for `who`, `users` and `pinky`.
//!
//! From Windows' session API (wtsapi32), loaded when one of them asks rather than at
//! every start, as for the clipboard's lock (`clipboard::session_locked`).
//!
//! They read `quser.exe`'s text before, whose dates follow the machine's locale, whose
//! columns differ by language, and which Windows Home does not have.

use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::RemoteDesktop::{
    WTS_CURRENT_SERVER_HANDLE, WTS_SESSION_INFOW, WTSActive, WTSClientName, WTSINFOW,
    WTSSessionInfo,
};

/// One session an account is logged on to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// Windows' number for it.
    pub id: u32,
    /// The account's name, without its domain.
    pub user: String,
    /// The account's domain: the machine's name for a local account.
    pub domain: String,
    /// The station it is on, in lower case as `quser` gives it (`console`, `rdp-tcp#0`);
    /// empty for a session no one is connected to.
    pub station: String,
    /// Whether someone is connected to it, rather than it being disconnected.
    pub active: bool,
    /// When the account logged on to it.
    pub logon: Option<SystemTime>,
    /// The last keyboard or mouse input, where Windows keeps it: a remote session's, and
    /// this session's own.
    pub last_input: Option<SystemTime>,
    /// The computer a remote session is used from.
    pub client: Option<String>,
}

/// wtsapi32's functions, loaded once.
struct Api {
    enumerate:
        unsafe extern "system" fn(HANDLE, u32, u32, *mut *mut WTS_SESSION_INFOW, *mut u32) -> i32,
    query: unsafe extern "system" fn(HANDLE, u32, i32, *mut *mut u16, *mut u32) -> i32,
    free: unsafe extern "system" fn(*mut core::ffi::c_void),
}

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| {
        use windows_sys::Win32::System::LibraryLoader::{
            GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
        };
        // SAFETY: loads a system DLL from System32 alone; it stays loaded.
        let module = unsafe {
            LoadLibraryExW(
                windows_sys::core::w!("wtsapi32.dll"),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
        if module.is_null() {
            return None;
        }
        // SAFETY: the module is loaded and the name is NUL terminated.
        let address =
            |name: &core::ffi::CStr| unsafe { GetProcAddress(module, name.as_ptr().cast()) };
        let enumerate = address(c"WTSEnumerateSessionsW")?;
        let query = address(c"WTSQuerySessionInformationW")?;
        let free = address(c"WTSFreeMemory")?;
        // SAFETY: the documented signature of WTSEnumerateSessionsW.
        let enumerate = unsafe {
            std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                unsafe extern "system" fn(
                    HANDLE,
                    u32,
                    u32,
                    *mut *mut WTS_SESSION_INFOW,
                    *mut u32,
                ) -> i32,
            >(enumerate)
        };
        // SAFETY: the documented signature of WTSQuerySessionInformationW.
        let query = unsafe {
            std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                unsafe extern "system" fn(HANDLE, u32, i32, *mut *mut u16, *mut u32) -> i32,
            >(query)
        };
        // SAFETY: the documented signature of WTSFreeMemory.
        let free = unsafe {
            std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                unsafe extern "system" fn(*mut core::ffi::c_void),
            >(free)
        };
        Some(Api {
            enumerate,
            query,
            free,
        })
    })
    .as_ref()
}

/// The sessions accounts are logged on to, by session number. Empty when Windows does not
/// say.
#[must_use]
pub fn sessions() -> Vec<Session> {
    let Some(api) = api() else {
        return Vec::new();
    };
    let mut list: *mut WTS_SESSION_INFOW = std::ptr::null_mut();
    let mut count = 0u32;
    // SAFETY: the out-pointers are valid; the list is freed below.
    let ok = unsafe {
        (api.enumerate)(
            WTS_CURRENT_SERVER_HANDLE,
            0,
            1,
            &raw mut list,
            &raw mut count,
        )
    };
    if ok == 0 || list.is_null() {
        return Vec::new();
    }
    let own = current_session_id();
    // SAFETY: on success the list holds `count` entries.
    let mut sessions: Vec<Session> = unsafe { std::slice::from_raw_parts(list, count as usize) }
        .iter()
        .filter_map(|entry| session(api, entry.SessionId, own == Some(entry.SessionId)))
        .collect();
    // SAFETY: the list came from WTSEnumerateSessionsW and is not used again.
    unsafe { (api.free)(list.cast()) };
    sessions.sort_by_key(|session| session.id);
    sessions
}

/// The session `id`, when an account is logged on to it.
fn session(api: &Api, id: u32, own: bool) -> Option<Session> {
    let info = query(api, id, WTSSessionInfo)?;
    if info.len() * 2 < size_of::<WTSINFOW>() {
        return None;
    }
    // SAFETY: Windows wrote a WTSINFOW of the size it said; read as it lies, the buffer
    // promising only a `u16`'s alignment.
    let info = unsafe { info.as_ptr().cast::<WTSINFOW>().read_unaligned() };
    let user = text(&info.UserName);
    if user.is_empty() {
        return None;
    }
    let active = info.State == WTSActive;
    let client = query(api, id, WTSClientName)
        .map(|name| text(&name))
        .filter(|name| !name.is_empty());
    let last_input = if own {
        own_last_input()
    } else {
        time(info.LastInputTime)
    };
    Some(Session {
        id,
        user,
        domain: text(&info.Domain),
        station: if active {
            text(&info.WinStationName).to_lowercase()
        } else {
            String::new()
        },
        active,
        logon: time(info.LogonTime),
        last_input,
        client,
    })
}

/// What `WTSQuerySessionInformationW` says of session `id` for `class`, as UTF-16 units.
fn query(api: &Api, id: u32, class: i32) -> Option<Vec<u16>> {
    let mut buffer: *mut u16 = std::ptr::null_mut();
    let mut bytes = 0u32;
    // SAFETY: the out-pointers are valid; the buffer is freed below.
    let ok = unsafe {
        (api.query)(
            WTS_CURRENT_SERVER_HANDLE,
            id,
            class,
            &raw mut buffer,
            &raw mut bytes,
        )
    };
    if ok == 0 || buffer.is_null() {
        return None;
    }
    // SAFETY: Windows wrote `bytes` bytes there.
    let units = unsafe { std::slice::from_raw_parts(buffer, bytes as usize / 2) }.to_vec();
    // SAFETY: the buffer came from the query and is not used again.
    unsafe { (api.free)(buffer.cast()) };
    Some(units)
}

/// The text in `units`, up to its first NUL.
fn text(units: &[u16]) -> String {
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

/// A time Windows gives in 100-nanosecond steps from 1601, `None` for 0.
fn time(filetime: i64) -> Option<SystemTime> {
    const FROM_1601_TO_1970: u64 = 116_444_736_000_000_000;
    let steps = u64::try_from(filetime)
        .ok()
        .filter(|&steps| steps > FROM_1601_TO_1970)?;
    Some(UNIX_EPOCH + Duration::from_nanos((steps - FROM_1601_TO_1970) * 100))
}

/// The last keyboard or mouse input in this session.
fn own_last_input() -> Option<SystemTime> {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut info = LASTINPUTINFO {
        cbSize: u32::try_from(size_of::<LASTINPUTINFO>()).ok()?,
        dwTime: 0,
    };
    // SAFETY: the structure is the size it says.
    if unsafe { GetLastInputInfo(&raw mut info) } == 0 {
        return None;
    }
    // SAFETY: no arguments. Both counts wrap after 49.7 days, alike.
    let now = unsafe { GetTickCount() };
    let idle = Duration::from_millis(u64::from(now.wrapping_sub(info.dwTime)));
    SystemTime::now().checked_sub(idle)
}

/// The session this process runs in.
#[must_use]
pub fn current_session_id() -> Option<u32> {
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    let mut id = 0u32;
    // SAFETY: the out-param is valid.
    (unsafe { ProcessIdToSessionId(std::process::id(), &raw mut id) } != 0).then_some(id)
}

/// The process id of session `id`'s Windows shell, `explorer.exe`: what stands for a Unix
/// login's shell, whose pid `who -u` gives.
#[must_use]
pub fn shell_pid(id: u32) -> Option<u32> {
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    crate::process::list()
        .into_iter()
        .filter(|process| process.name.eq_ignore_ascii_case("explorer.exe"))
        .find(|process| {
            let mut session = 0u32;
            // SAFETY: the out-param is valid.
            unsafe { ProcessIdToSessionId(process.pid, &raw mut session) != 0 && session == id }
        })
        .map(|process| process.pid)
}

/// When the machine started.
#[must_use]
pub fn boot_time() -> SystemTime {
    SystemTime::now()
        .checked_sub(crate::sysinfo::uptime())
        .unwrap_or(UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_count_from_1601() {
        assert_eq!(time(0), None);
        assert_eq!(
            time(116_444_736_000_000_000 + 10_000_000),
            Some(UNIX_EPOCH + Duration::from_secs(1))
        );
    }

    #[test]
    fn this_session_is_listed_with_its_account() {
        let Some(own) = current_session_id() else {
            return;
        };
        // A service's session (CI's) has no account logged on to it, and lists none.
        if let Some(session) = sessions().into_iter().find(|session| session.id == own) {
            assert_ne!(session.user, "");
            assert!(session.logon.is_some(), "{session:?}");
        }
    }
}
