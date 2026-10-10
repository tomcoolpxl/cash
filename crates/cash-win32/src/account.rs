//! Accounts, and what they may do: what `sudo`, `su` and `cash doctor` ask Windows.
//!
//! Who this process runs as and whether that account is an administrator; whether
//! another account may run a program, which `su USER` and `sudo -u USER` need of cash's
//! own exe; the local accounts Tab completes after `su`; and the default owner an
//! elevated command gives the files it creates.

use std::ffi::{OsStr, OsString};
use std::io;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::Path;

use windows_sys::Win32::Foundation::{
    ERROR_MORE_DATA, ERROR_SUCCESS, GENERIC_ALL, GENERIC_EXECUTE, LocalFree,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, EqualSid,
    GetAce, GetAclInformation, GetLengthSid, INHERIT_ONLY_ACE, IsValidSid, PSID, SID_NAME_USE,
    SidTypeUser, TOKEN_INFORMATION_CLASS,
};
use windows_sys::Win32::Storage::FileSystem::FILE_EXECUTE;

use crate::wide::to_wide_nul;

/// An access-allowed entry, as `ACE_HEADER::AceType` names it.
const ALLOWED: u8 = 0;
/// An access-denied entry.
const DENIED: u8 = 1;
/// The rights any one of which lets an account start a program.
const EXECUTE_RIGHTS: u32 = FILE_EXECUTE | GENERIC_EXECUTE | GENERIC_ALL;

/// The groups every interactive account is in, whose access a program grants to all:
/// Everyone, Authenticated Users and Users.
const EVERYONE_GROUPS: [&str; 3] = ["S-1-1-0", "S-1-5-11", "S-1-5-32-545"];
/// The Administrators group.
const ADMINISTRATORS: &str = "S-1-5-32-544";

/// A security identifier, held in a buffer of its own that is aligned as Windows reads
/// one.
#[derive(Clone, Debug)]
pub struct Sid {
    words: Vec<u32>,
}

impl PartialEq for Sid {
    fn eq(&self, other: &Self) -> bool {
        // SAFETY: both pointers are valid SIDs, which every `Sid` holds by construction.
        unsafe { EqualSid(self.as_psid(), other.as_psid()) != 0 }
    }
}

impl Eq for Sid {}

impl Sid {
    /// A copy of the SID at `sid`, or `None` when it is null or not a valid SID.
    fn copy_of(sid: PSID) -> Option<Self> {
        // SAFETY: `IsValidSid` accepts any pointer obtained as a PSID, and rejects null.
        if sid.is_null() || unsafe { IsValidSid(sid) } == 0 {
            return None;
        }
        // SAFETY: `sid` is a valid SID, checked above.
        let length = unsafe { GetLengthSid(sid) } as usize;
        // SAFETY: a valid SID is `length` bytes long.
        let bytes = unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), length) };
        let mut words = vec![0u32; length.div_ceil(4)];
        for (index, byte) in bytes.iter().enumerate() {
            let word = words.get_mut(index / 4)?;
            *word |= u32::from(*byte) << ((index % 4) * 8);
        }
        Some(Self { words })
    }

    /// The SID `text` spells (`S-1-5-21-…`), or `None` for one that is not a SID.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;

        let wide = to_wide_nul(text);
        let mut sid: PSID = std::ptr::null_mut();
        // SAFETY: `wide` is NUL-terminated and `sid` is a valid out-param.
        if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &raw mut sid) } == 0 {
            return None;
        }
        let copy = Self::copy_of(sid);
        // SAFETY: the call above allocated the SID with LocalAlloc.
        unsafe { LocalFree(sid) };
        copy
    }

    /// The SID as text, `S-1-5-21-…`.
    #[must_use]
    pub fn text(&self) -> String {
        use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;

        let mut text: *mut u16 = std::ptr::null_mut();
        // SAFETY: the SID is valid, and `text` is a valid out-param.
        if unsafe { ConvertSidToStringSidW(self.as_psid(), &raw mut text) } == 0 {
            return String::new();
        }
        // SAFETY: on success `text` is a NUL-terminated string the call allocated.
        let result = unsafe { crate::net::widestring_at(text) };
        // SAFETY: the string was allocated by the call above, with LocalAlloc.
        unsafe { LocalFree(text.cast()) };
        result
    }

    /// The SID as the pointer Windows' calls take; they only read through it.
    pub(crate) const fn as_psid(&self) -> PSID {
        self.words.as_ptr().cast_mut().cast()
    }
}

/// The SID of the user account `name` names (`tom`, `PC\tom`), or `None` when there is no
/// such account or it is not a user (a group, say).
#[must_use]
pub fn lookup_user(name: &str) -> Option<Sid> {
    use windows_sys::Win32::Security::LookupAccountNameW;

    let wide = to_wide_nul(name);
    let mut sid = vec![0u32; 32];
    let mut sid_len = u32::try_from(sid.len() * 4).unwrap_or(0);
    let mut domain = [0u16; 256];
    let mut domain_len = u32::try_from(domain.len()).unwrap_or(0);
    let mut kind: SID_NAME_USE = 0;
    // SAFETY: the name is NUL-terminated, and each buffer is described by its length; 128
    // bytes holds the largest SID (15 sub-authorities).
    let ok = unsafe {
        LookupAccountNameW(
            std::ptr::null(),
            wide.as_ptr(),
            sid.as_mut_ptr().cast(),
            &raw mut sid_len,
            domain.as_mut_ptr(),
            &raw mut domain_len,
            &raw mut kind,
        )
    };
    if ok == 0 || kind != SidTypeUser {
        return None;
    }
    Sid::copy_of(sid.as_mut_ptr().cast())
}

/// The account a SID names, as `DOMAIN\name`.
#[must_use]
pub fn account_name(sid: &Sid) -> Option<String> {
    use windows_sys::Win32::Security::LookupAccountSidW;

    let mut name = [0u16; 256];
    let mut name_len = u32::try_from(name.len()).unwrap_or(0);
    let mut domain = [0u16; 256];
    let mut domain_len = u32::try_from(domain.len()).unwrap_or(0);
    let mut kind: SID_NAME_USE = 0;
    // SAFETY: the SID is valid, and both buffers are described by their lengths.
    let ok = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            sid.as_psid(),
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
    let name = String::from_utf16_lossy(name.get(..name_len as usize)?);
    let domain = String::from_utf16_lossy(domain.get(..domain_len as usize)?);
    Some(if domain.is_empty() {
        name
    } else {
        format!(r"{domain}\{name}")
    })
}

/// This process's token, opened for `access`.
fn process_token(access: u32) -> Option<OwnedHandle> {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    // SAFETY: returns the current process's pseudo-handle; nothing to uphold.
    let process = unsafe { GetCurrentProcess() };
    let mut token = std::ptr::null_mut();
    // SAFETY: a process handle and a valid out-param.
    if unsafe { OpenProcessToken(process, access, &raw mut token) } == 0 {
        return None;
    }
    // SAFETY: the call succeeded, so `token` is an open handle that is ours.
    Some(unsafe { OwnedHandle::from_raw_handle(token) })
}

/// What `GetTokenInformation` gives for `class`, in a buffer aligned for the pointers the
/// structures it holds start with.
fn token_information(token: &OwnedHandle, class: TOKEN_INFORMATION_CLASS) -> Option<Vec<u64>> {
    use windows_sys::Win32::Security::GetTokenInformation;

    let mut needed: u32 = 0;
    // SAFETY: a null buffer with a zero length is the documented way to ask for the size.
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            class,
            std::ptr::null_mut(),
            0,
            &raw mut needed,
        );
    }
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
    // SAFETY: the buffer is at least the `needed` bytes the call above asked for.
    let ok = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            class,
            buffer.as_mut_ptr().cast(),
            needed,
            &raw mut needed,
        )
    };
    (ok != 0).then_some(buffer)
}

/// The user this process runs as.
#[must_use]
pub fn current_user() -> Option<Sid> {
    use windows_sys::Win32::Security::{TOKEN_QUERY, TOKEN_USER, TokenUser};

    let token = process_token(TOKEN_QUERY)?;
    let buffer = token_information(&token, TokenUser)?;
    if buffer.len() * 8 < size_of::<TOKEN_USER>() {
        return None;
    }
    // SAFETY: the buffer holds a TOKEN_USER, aligned for one by construction.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    Sid::copy_of(user.User.Sid)
}

/// Whether this process's account is a member of Administrators, elevated or not.
///
/// An unelevated administrator's token holds the group for denying only, which
/// `CheckTokenMembership` does not count, so the groups are read.
#[must_use]
pub fn is_administrator() -> Option<bool> {
    use windows_sys::Win32::Security::{
        SID_AND_ATTRIBUTES, TOKEN_GROUPS, TOKEN_QUERY, TokenGroups,
    };

    let administrators = Sid::parse(ADMINISTRATORS)?;
    let token = process_token(TOKEN_QUERY)?;
    let buffer = token_information(&token, TokenGroups)?;
    if buffer.len() * 8 < size_of::<TOKEN_GROUPS>() {
        return None;
    }
    let groups = buffer.as_ptr().cast::<TOKEN_GROUPS>();
    // SAFETY: the buffer holds a TOKEN_GROUPS, aligned for one by construction.
    let count = unsafe { (*groups).GroupCount } as usize;
    // SAFETY: the array of `count` entries starts at `Groups` and lies in the buffer.
    let first = unsafe { (&raw const (*groups).Groups).cast::<SID_AND_ATTRIBUTES>() };
    // SAFETY: as above: `count` entries, in the buffer the call filled.
    let entries = unsafe { std::slice::from_raw_parts(first, count) };
    Some(
        entries
            .iter()
            .any(|entry| Sid::copy_of(entry.Sid).is_some_and(|sid| sid == administrators)),
    )
}

/// Whether `user`, or with `None` every account, may start the program at `path`.
///
/// By its access list: an entry that grants execute to Everyone, Authenticated Users,
/// Users or `user` itself. `None` when the list cannot be read. A program under a user's profile (Scoop's `~/scoop`) is that user's and the
/// administrators' only, and `su OTHER` cannot start it there.
#[must_use]
pub fn may_execute(path: &Path, user: Option<&Sid>) -> Option<bool> {
    use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;

    let wide = to_wide_nul(path);
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: the name is NUL-terminated and the out-params are valid; the parts not
    // asked for may be null.
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut dacl,
            std::ptr::null_mut(),
            &raw mut descriptor,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let granted = dacl_grants_execute(dacl, &trustees(user));
    // SAFETY: the descriptor was allocated by the call above, with LocalAlloc; `dacl`
    // points into it and is not used after this.
    unsafe { LocalFree(descriptor) };
    Some(granted)
}

/// The SIDs whose access [`may_execute`] counts.
fn trustees(user: Option<&Sid>) -> Vec<Sid> {
    EVERYONE_GROUPS
        .iter()
        .filter_map(|text| Sid::parse(text))
        .chain(user.cloned())
        .collect()
}

/// Whether an access list grants execute to one of `trustees`, entries read in order as
/// Windows reads them: a denying entry first denies. A null list grants everything.
fn dacl_grants_execute(dacl: *const ACL, trustees: &[Sid]) -> bool {
    if dacl.is_null() {
        return true;
    }
    let mut size = ACL_SIZE_INFORMATION::default();
    // SAFETY: `dacl` is a valid ACL, and `size` is the structure the class asks for.
    let ok = unsafe {
        GetAclInformation(
            dacl,
            (&raw mut size).cast(),
            u32::try_from(size_of::<ACL_SIZE_INFORMATION>()).unwrap_or(0),
            AclSizeInformation,
        )
    };
    if ok == 0 {
        return false;
    }
    for index in 0..size.AceCount {
        let mut ace = std::ptr::null_mut();
        // SAFETY: `index` is below the entry count, and `ace` is a valid out-param.
        if unsafe { GetAce(dacl, index, &raw mut ace) } == 0 {
            continue;
        }
        // SAFETY: every entry starts with an ACE_HEADER.
        let header = unsafe { *ace.cast::<ACE_HEADER>() };
        if u32::from(header.AceFlags) & INHERIT_ONLY_ACE != 0
            || !matches!(header.AceType, ALLOWED | DENIED)
        {
            continue;
        }
        // Allowed and denied entries have the same layout: header, mask, then the SID.
        let entry = ace.cast::<ACCESS_ALLOWED_ACE>();
        // SAFETY: an allowed or denied entry is an ACCESS_ALLOWED_ACE in layout.
        let mask = unsafe { (*entry).Mask };
        if mask & EXECUTE_RIGHTS == 0 {
            continue;
        }
        // SAFETY: the SID starts at `SidStart` and lies within the entry.
        let sid: PSID = unsafe { (&raw const (*entry).SidStart) }.cast_mut().cast();
        let Some(sid) = Sid::copy_of(sid) else {
            continue;
        };
        if trustees.contains(&sid) {
            return header.AceType == ALLOWED;
        }
    }
    false
}

/// The local accounts that are enabled, by name, for Tab after `su`: an account that is
/// switched off (Guest, the built-in Administrator) cannot be used.
#[must_use]
pub fn local_user_names() -> Vec<String> {
    use windows_sys::Win32::NetworkManagement::NetManagement::{
        FILTER_NORMAL_ACCOUNT, MAX_PREFERRED_LENGTH, NERR_Success, NetApiBufferFree, NetUserEnum,
        UF_ACCOUNTDISABLE, USER_INFO_1,
    };

    let mut names = Vec::new();
    let mut resume: u32 = 0;
    loop {
        // NetUserEnum's own allocation, aligned for the entries it holds.
        let mut buffer: *mut USER_INFO_1 = std::ptr::null_mut();
        let mut read: u32 = 0;
        let mut total: u32 = 0;
        // SAFETY: a null server is this machine; every out-param is valid.
        let status = unsafe {
            NetUserEnum(
                std::ptr::null(),
                1,
                FILTER_NORMAL_ACCOUNT,
                (&raw mut buffer).cast(),
                MAX_PREFERRED_LENGTH,
                &raw mut read,
                &raw mut total,
                &raw mut resume,
            )
        };
        if status != NERR_Success && status != ERROR_MORE_DATA {
            if !buffer.is_null() {
                // SAFETY: the buffer was allocated by the call above.
                unsafe { NetApiBufferFree(buffer.cast()) };
            }
            break;
        }
        if !buffer.is_null() {
            // SAFETY: on success the buffer holds `read` USER_INFO_1 entries.
            let users = unsafe { std::slice::from_raw_parts(buffer, read as usize) };
            for user in users {
                if user.usri1_flags & UF_ACCOUNTDISABLE != 0 || user.usri1_name.is_null() {
                    continue;
                }
                // SAFETY: the name is a NUL-terminated string in the buffer.
                names.push(unsafe { crate::net::widestring_at(user.usri1_name) });
            }
            // SAFETY: the buffer was allocated by the call above and is not used again.
            unsafe { NetApiBufferFree(buffer.cast()) };
        }
        if status != ERROR_MORE_DATA {
            break;
        }
    }
    names.sort_by_key(|name| name.to_lowercase());
    names
}

/// Makes `owner` the default owner of what this process and the ones it starts create.
///
/// A file an elevated command makes is then the user's, not Administrators'. Windows
/// allows it only when `owner` is this token's own user (or a group it may own with).
///
/// # Errors
///
/// When the token cannot be opened or Windows refuses the owner.
pub fn set_default_owner(owner: &Sid) -> io::Result<()> {
    use windows_sys::Win32::Security::{
        SetTokenInformation, TOKEN_ADJUST_DEFAULT, TOKEN_OWNER, TOKEN_QUERY, TokenOwner,
    };

    let token =
        process_token(TOKEN_ADJUST_DEFAULT | TOKEN_QUERY).ok_or_else(io::Error::last_os_error)?;
    let information = TOKEN_OWNER {
        Owner: owner.as_psid(),
    };
    // SAFETY: the token was opened for TOKEN_ADJUST_DEFAULT, and `information` is a
    // TOKEN_OWNER whose SID outlives the call.
    let ok = unsafe {
        SetTokenInformation(
            token.as_raw_handle(),
            TokenOwner,
            (&raw const information).cast(),
            u32::try_from(size_of::<TOKEN_OWNER>()).unwrap_or(0),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// What the elevated side of `sudo` does about who owns its files, given the user who
/// asked and the user it runs as.
#[derive(Debug, PartialEq, Eq)]
pub enum OwnerPlan {
    /// The user approved with their own account: their files stay theirs.
    SetOwner,
    /// An administrator approved for a standard user: it runs as that administrator,
    /// whose files and home they are.
    OtherAccount,
    /// Nothing to go on: leave it as Windows has it.
    Leave,
}

/// [`OwnerPlan`] for a command asked for by `requested` and running as `running`.
#[must_use]
pub fn owner_plan(requested: Option<&Sid>, running: Option<&Sid>) -> OwnerPlan {
    match (requested, running) {
        (Some(requested), Some(running)) if requested == running => OwnerPlan::SetOwner,
        (Some(_), Some(_)) => OwnerPlan::OtherAccount,
        _ => OwnerPlan::Leave,
    }
}

/// The elevated side of `sudo`: `cash --invoke-bundled --sudo-owner SID PROGRAM [ARGS]...`.
///
/// Runs `program` with `args`, its standard handles, folder and environment this
/// process's, waits for it and gives its status. First, when it runs as the user `sid`
/// names, it makes that user the default owner of what it and its children create;
/// when it runs as another account (an administrator approved for a standard user), it
/// says so once.
#[must_use]
pub fn run_owned_by(sid: &str, program: &OsStr, args: &[OsString]) -> i32 {
    use windows_sys::Win32::Foundation::TRUE;
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
    use windows_sys::core::BOOL;

    /// Swallows every console event, which the program gets too and answers itself.
    const unsafe extern "system" fn ignore(_event: u32) -> BOOL {
        TRUE
    }

    let requested = Sid::parse(sid);
    let running = current_user();
    match owner_plan(requested.as_ref(), running.as_ref()) {
        OwnerPlan::SetOwner => {
            if let Some(owner) = &requested
                && let Err(err) = set_default_owner(owner)
            {
                eprintln!("sudo: the files it creates belong to Administrators: {err}");
            }
        }
        OwnerPlan::OtherAccount => {
            let name = running
                .as_ref()
                .and_then(account_name)
                .unwrap_or_else(|| "another account".to_owned());
            eprintln!("sudo: running as {name}, not you; files and ~ are theirs");
        }
        OwnerPlan::Leave => {}
    }

    // SAFETY: `ignore` is a valid handler for the life of the process and touches nothing.
    unsafe { SetConsoleCtrlHandler(Some(ignore), TRUE) };
    let job = crate::job::JobObject::for_pipeline().ok();
    let mut child = match std::process::Command::new(program).args(args).spawn() {
        Ok(child) => child,
        Err(err) => {
            eprintln!("sudo: {}: {err}", program.to_string_lossy());
            return if err.kind() == io::ErrorKind::NotFound {
                127
            } else {
                126
            };
        }
    };
    if let Some(job) = &job {
        let _ = job.assign_child(&child);
    }
    match child.wait() {
        Ok(status) => status.code().unwrap_or(1),
        Err(err) => {
            eprintln!("sudo: {}: {err}", program.to_string_lossy());
            125
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the access list of the descriptor `sddl` spells grants execute to
    /// `trustees`.
    fn grants(sddl: &str, trustees: &[Sid]) -> bool {
        use windows_sys::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows_sys::Win32::Security::GetSecurityDescriptorDacl;

        let wide = to_wide_nul(sddl);
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: the text is NUL-terminated and the out-param valid; the size may be null.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &raw mut descriptor,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(ok, 0, "{sddl}");
        let mut present = 0;
        let mut defaulted = 0;
        let mut dacl = std::ptr::null_mut();
        // SAFETY: the descriptor was made by the call above; the out-params are valid.
        let ok = unsafe {
            GetSecurityDescriptorDacl(
                descriptor,
                &raw mut present,
                &raw mut dacl,
                &raw mut defaulted,
            )
        };
        assert_ne!(ok, 0);
        let granted = dacl_grants_execute(dacl, trustees);
        // SAFETY: allocated with LocalAlloc by the conversion; not used after this.
        unsafe { LocalFree(descriptor) };
        granted
    }

    #[test]
    fn a_program_in_a_profile_runs_for_its_owner_only() {
        let tom = Sid::parse("S-1-5-21-1-2-3-1001").unwrap();
        let ann = Sid::parse("S-1-5-21-1-2-3-1002").unwrap();
        // What `icacls` shows under `C:\Users\tom`: SYSTEM, Administrators and tom.
        let profile = "D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;S-1-5-21-1-2-3-1001)";
        assert!(grants(profile, &trustees(Some(&tom))));
        assert!(!grants(profile, &trustees(Some(&ann))));
        assert!(!grants(profile, &trustees(None)));
    }

    #[test]
    fn a_program_users_may_run_runs_for_everyone() {
        // `C:\Program Files`: Users read and execute.
        let program_files = "D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;0x1200a9;;;BU)";
        assert!(grants(program_files, &trustees(None)));
        // Read only, without execute, is not enough.
        assert!(!grants("D:P(A;;FR;;;WD)", &trustees(None)));
        // A denying entry first denies.
        assert!(!grants("D:P(D;;FX;;;WD)(A;;FA;;;WD)", &trustees(None)));
        // An entry only for what is created inside does not count.
        assert!(!grants("D:P(A;IO;FA;;;WD)", &trustees(None)));
    }

    #[test]
    fn the_owner_changes_only_for_the_user_who_asked() {
        let tom = Sid::parse("S-1-5-21-1-2-3-1001").unwrap();
        let admin = Sid::parse("S-1-5-21-1-2-3-500").unwrap();
        assert_eq!(owner_plan(Some(&tom), Some(&tom)), OwnerPlan::SetOwner);
        assert_eq!(
            owner_plan(Some(&tom), Some(&admin)),
            OwnerPlan::OtherAccount
        );
        assert_eq!(owner_plan(None, Some(&admin)), OwnerPlan::Leave);
        assert_eq!(tom.text(), "S-1-5-21-1-2-3-1001");
    }

    #[test]
    fn this_account_is_found_and_may_run_its_own_files() {
        let me = current_user().unwrap();
        assert!(account_name(&me).is_some_and(|name| !name.is_empty()));
        assert!(is_administrator().is_some());
        let file = tempfile::NamedTempFile::new().unwrap();
        assert_eq!(may_execute(file.path(), Some(&me)), Some(true));
        // A local account's name is among the local accounts.
        let name = account_name(&me).unwrap();
        let computer = crate::process::computer_name().unwrap_or_default();
        if let Some(local) = name.strip_prefix(&format!(r"{computer}\")) {
            assert!(
                local_user_names()
                    .iter()
                    .any(|user| user.eq_ignore_ascii_case(local)),
                "{local} not in {:?}",
                local_user_names()
            );
        }
    }
}
