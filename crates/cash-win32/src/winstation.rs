//! Letting another account onto this desktop, for `sudo -u USER` with a program that has
//! windows (research/sudo-in-terminal-design.md).
//!
//! A process started as USER runs on the caller's window station and desktop (`WinSta0\
//! Default`), which USER has no rights to, so a window it opens does not show —
//! `CreateProcessWithLogonW`'s documented limit, and gsudo's. The fix the docs give, and
//! what `runas` does, is to grant USER access to the window station and the desktop first.
//!
//! cash grants it only while the command runs: [`share_with`] adds the grant and returns a
//! guard that puts each object's access list back as it was when dropped, so another
//! account is not left with standing access to the desktop. Every step is best-effort — a
//! console command needs none of this, and a failure here must not stop it.

use windows_sys::Win32::Foundation::{GENERIC_ALL, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, GRANT_ACCESS, GetSecurityInfo, SE_WINDOW_OBJECT, SetEntriesInAclW,
    SetSecurityInfo, TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, NO_INHERITANCE, PSECURITY_DESCRIPTOR,
};
use windows_sys::Win32::System::StationsAndDesktops::{GetProcessWindowStation, GetThreadDesktop};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;

use crate::account::Sid;

/// One window object's access list put back as it was: the handle (the system's, never
/// closed), its original list, and the descriptor that holds it.
struct Restore {
    handle: isize,
    old_dacl: *mut ACL,
    descriptor: PSECURITY_DESCRIPTOR,
}

/// The grants [`share_with`] made, each undone when this is dropped.
pub struct DesktopGrant {
    restores: Vec<Restore>,
}

impl Drop for DesktopGrant {
    fn drop(&mut self) {
        for restore in &self.restores {
            // SAFETY: the handle is the window object's, still open; `old_dacl` is its
            // original list, valid until the descriptor is freed just below.
            unsafe {
                SetSecurityInfo(
                    restore.handle as _,
                    SE_WINDOW_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    restore.old_dacl,
                    std::ptr::null(),
                );
            }
            // SAFETY: allocated by `GetSecurityInfo`, not used after the restore above.
            unsafe { LocalFree(restore.descriptor) };
        }
    }
}

/// Grants `sid` access to this process's window station and desktop until the guard drops,
/// so a program run as that account can show a window. Best-effort.
#[must_use]
pub fn share_with(sid: &Sid) -> DesktopGrant {
    let mut restores = Vec::new();
    // SAFETY: a plain query of this thread.
    let thread = unsafe { GetCurrentThreadId() };
    // SAFETY: plain queries; the handles are the system's for this process, not closed.
    let winsta = unsafe { GetProcessWindowStation() };
    // SAFETY: as above.
    let desktop = unsafe { GetThreadDesktop(thread) };
    for handle in [winsta.cast::<core::ffi::c_void>(), desktop.cast()] {
        if !handle.is_null()
            && let Some(restore) = grant(handle, sid)
        {
            restores.push(restore);
        }
    }
    DesktopGrant { restores }
}

/// Adds a grant of `GENERIC_ALL` for `sid` to the window object `handle`'s access list,
/// keeping the original to put back; `None` when any step fails.
fn grant(handle: *mut core::ffi::c_void, sid: &Sid) -> Option<Restore> {
    let mut old_dacl: *mut ACL = std::ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: the out-params are valid; the descriptor is kept for the guard, or freed here.
    let status = unsafe {
        GetSecurityInfo(
            handle,
            SE_WINDOW_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut old_dacl,
            std::ptr::null_mut(),
            &raw mut descriptor,
        )
    };
    if status != 0 {
        return None;
    }
    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: GENERIC_ALL,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: sid.as_psid().cast(),
            ..Default::default()
        },
    };
    let mut new_dacl: *mut ACL = std::ptr::null_mut();
    // SAFETY: one entry, whose SID outlives the call; `old_dacl` is this object's own list.
    let merged = unsafe { SetEntriesInAclW(1, &raw const entry, old_dacl, &raw mut new_dacl) };
    if merged != 0 || new_dacl.is_null() {
        // SAFETY: allocated by `GetSecurityInfo`; nothing kept a pointer into it.
        unsafe { LocalFree(descriptor) };
        return None;
    }
    // SAFETY: `new_dacl` is the merged list, old plus the one grant.
    let set = unsafe {
        SetSecurityInfo(
            handle,
            SE_WINDOW_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            new_dacl,
            std::ptr::null(),
        )
    };
    // SAFETY: allocated by `SetEntriesInAclW`; the object kept its own copy.
    unsafe { LocalFree(new_dacl.cast()) };
    if set != 0 {
        // SAFETY: as above; the grant did not take, so nothing to put back.
        unsafe { LocalFree(descriptor) };
        return None;
    }
    Some(Restore {
        handle: handle as isize,
        old_dacl,
        descriptor,
    })
}
