//! Facts about the session that rarely change, worked out when first asked and kept, each
//! with its own rule for going stale:
//!
//! - the account running cash, its RID and its SID as text: never, since a process's
//!   token does not change user;
//! - the machine's name: never, since a rename takes a restart;
//! - the process that started cash: never, since Windows records it at creation;
//! - an account's name by its SID: never, asked of LSA once per SID, where `ps` asked once
//!   per process and `ls -l` once per file whose owner it could not read;
//! - what Windows Terminal draws a profile in, its font and the shape of its cells: while
//!   Terminal's settings files keep their modification times and sizes, their folders
//!   listed again at most every two seconds for a file that came or went.
//!
//! The functions callers know (`process::current_process_account`, `fs::current_user`,
//! `process::computer_name`, `terminal::profile_font`, ...) ask here, so nothing that
//! uses them had to change. `ls --icons` read and parsed every Terminal settings file for
//! each listing, a third of the time the listing took to appear.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use windows_sys::Win32::Security::{GetLengthSid, LookupAccountSidW, PSID, SID_NAME_USE};

/// The account running cash, without its domain, and its SID's RID.
#[must_use]
pub fn current_account() -> Option<(String, u32)> {
    static ACCOUNT: OnceLock<Option<(String, u32)>> = OnceLock::new();
    ACCOUNT
        .get_or_init(crate::process::read_current_account)
        .clone()
}

/// The SID of the account running cash, as text (`S-1-5-21-…`).
#[must_use]
pub fn current_user_sid_string() -> Option<String> {
    static SID: OnceLock<Option<String>> = OnceLock::new();
    SID.get_or_init(crate::process::read_current_user_sid_string)
        .clone()
}

/// The machine's name, as Windows spells it (`process::computer_name` says which).
#[must_use]
pub fn computer_name() -> Option<String> {
    static NAME: OnceLock<Option<String>> = OnceLock::new();
    NAME.get_or_init(crate::process::read_computer_name).clone()
}

/// The process that started cash, as Windows recorded it when cash was created (`$PPID`).
///
/// Asked of cash's own process, one call. `$PPID` listed every process on the machine to
/// find cash among them, 11 ms of each start's 14.
#[must_use]
pub fn parent_pid() -> Option<u32> {
    static PARENT: OnceLock<Option<u32>> = OnceLock::new();
    *PARENT.get_or_init(|| {
        // SAFETY: the pseudo-handle of the current process, valid without being opened.
        let own = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() };
        crate::handles::basic_information(own)
            .and_then(|basic| u32::try_from(basic.InheritedFromUniqueProcessId).ok())
            .filter(|&pid| pid != 0)
    })
}

/// Account names by their SID's bytes.
static ACCOUNT_NAMES: Mutex<Option<HashMap<Vec<u8>, String>>> = Mutex::new(None);

/// The account `sid` names, without its domain, asked of LSA once per SID.
///
/// `sid` must point at a valid SID, as one a token or a security descriptor holds.
pub(crate) fn account_name(sid: PSID) -> Option<String> {
    // SAFETY: the caller hands a valid SID.
    let length = unsafe { GetLengthSid(sid) } as usize;
    // SAFETY: a valid SID is `length` bytes long.
    let bytes = unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), length) };
    if let Some(name) = ACCOUNT_NAMES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|names| names.get(bytes))
    {
        return Some(name.clone());
    }

    let mut name = [0u16; 256];
    let mut name_len = u32::try_from(name.len()).unwrap_or(u32::MAX);
    let mut domain = [0u16; 256];
    let mut domain_len = u32::try_from(domain.len()).unwrap_or(u32::MAX);
    let mut kind: SID_NAME_USE = 0;
    // SAFETY: `sid` is valid, and the buffers are sized by their lengths.
    let ok = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            sid,
            name.as_mut_ptr(),
            &raw mut name_len,
            domain.as_mut_ptr(),
            &raw mut domain_len,
            &raw mut kind,
        )
    };
    if ok == 0 {
        return None;
    }
    let found = String::from_utf16_lossy(&name[..name_len as usize]);
    ACCOUNT_NAMES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .insert(bytes.to_vec(), found.clone());
    Some(found)
}

/// The font face Terminal draws the profile `guid` in (`terminal::profile_font` says how
/// it is resolved).
#[must_use]
pub fn profile_font(local_app_data: &Path, program_data: Option<&Path>, guid: &str) -> String {
    with_terminal(local_app_data, program_data, guid, |facts| {
        facts
            .font
            .get_or_insert_with(|| {
                crate::terminal::read_profile_font(local_app_data, program_data, guid)
            })
            .clone()
    })
}

/// How many times as tall as wide the profile `guid`'s cells are
/// (`terminal::profile_cell_ratio` says how it is worked out).
#[must_use]
pub fn profile_cell_ratio(
    local_app_data: &Path,
    program_data: Option<&Path>,
    guid: &str,
) -> Option<f64> {
    let face = profile_font(local_app_data, program_data, guid);
    with_terminal(local_app_data, program_data, guid, |facts| {
        *facts.cell_ratio.get_or_init(|| {
            crate::terminal::read_profile_cell_ratio(local_app_data, program_data, guid, &face)
        })
    })
}

/// How long a listing of Terminal's settings folders is trusted to still hold the same
/// files; the files found are looked at every time.
const LISTING_LIFE: Duration = Duration::from_secs(2);

/// A settings file, with its modification time and size when it could be read.
type FileStamp = (PathBuf, Option<(Option<SystemTime>, u64)>);

/// What is known of one Terminal profile, and from which files as they were then.
struct TerminalFacts {
    key: (PathBuf, Option<PathBuf>, String),
    files: Vec<FileStamp>,
    listed: Instant,
    font: Option<String>,
    cell_ratio: std::cell::OnceCell<Option<f64>>,
}

static TERMINAL: Mutex<Option<TerminalFacts>> = Mutex::new(None);

fn stamped(files: impl IntoIterator<Item = PathBuf>) -> Vec<FileStamp> {
    files
        .into_iter()
        .map(|file| {
            let stamp = file
                .metadata()
                .ok()
                .map(|meta| (meta.modified().ok(), meta.len()));
            (file, stamp)
        })
        .collect()
}

fn listed(local_app_data: &Path, program_data: Option<&Path>) -> Vec<FileStamp> {
    stamped(
        crate::terminal::settings_files(local_app_data)
            .into_iter()
            .chain(crate::terminal::fragment_files(
                local_app_data,
                program_data,
            )),
    )
}

/// Runs `read` on what is known of the profile `guid`, forgotten first if one of the
/// files it came from changed, came or went.
fn with_terminal<T>(
    local_app_data: &Path,
    program_data: Option<&Path>,
    guid: &str,
    read: impl FnOnce(&mut TerminalFacts) -> T,
) -> T {
    let key = (
        local_app_data.to_owned(),
        program_data.map(Path::to_owned),
        guid.to_ascii_lowercase(),
    );
    let mut guard = TERMINAL.lock().unwrap_or_else(PoisonError::into_inner);
    let still = guard.as_mut().is_some_and(|facts| {
        if facts.key != key {
            return false;
        }
        if facts.listed.elapsed() < LISTING_LIFE {
            let files = facts.files.iter().map(|(file, _)| file.clone());
            return stamped(files) == facts.files;
        }
        let same = listed(local_app_data, program_data) == facts.files;
        if same {
            facts.listed = Instant::now();
        }
        same
    });
    let mut facts = match guard.take() {
        Some(facts) if still => facts,
        _ => TerminalFacts {
            key,
            files: listed(local_app_data, program_data),
            listed: Instant::now(),
            font: None,
            cell_ratio: std::cell::OnceCell::new(),
        },
    };
    let answer = read(&mut facts);
    *guard = Some(facts);
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_account_is_asked_once_and_agrees_with_the_token() {
        let first = current_account();
        assert_eq!(first, crate::process::read_current_account());
        assert_eq!(current_account(), first);
    }

    #[test]
    fn the_parent_is_the_one_the_process_list_names() {
        let own = std::process::id();
        let listed = crate::process::list()
            .into_iter()
            .find(|process| process.pid == own)
            .map(|process| process.parent_pid);
        assert_eq!(parent_pid(), listed);
    }

    #[test]
    fn a_profile_is_read_again_when_its_settings_change() {
        let local = tempfile::tempdir().unwrap();
        let settings = local.path().join("Microsoft").join("Windows Terminal");
        std::fs::create_dir_all(&settings).unwrap();
        let file = settings.join("settings.json");
        let guid = "{11111111-2222-3333-4444-555555555555}";
        let write = |face: &str| {
            std::fs::write(
                &file,
                format!(
                    r#"{{"profiles": {{"list": [{{"guid": "{guid}", "font": {{"face": "{face}"}}}}]}}}}"#
                ),
            )
            .unwrap();
        };
        write("Cascadia Mono");
        assert_eq!(profile_font(local.path(), None, guid), "Cascadia Mono");
        assert_eq!(profile_font(local.path(), None, guid), "Cascadia Mono");
        // A longer face makes a longer file: changed whatever the clock's grain.
        write("CaskaydiaMono Nerd Font");
        assert_eq!(
            profile_font(local.path(), None, guid),
            "CaskaydiaMono Nerd Font"
        );
    }
}
