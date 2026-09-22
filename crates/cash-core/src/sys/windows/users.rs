#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::unnecessary_wraps)]

use crate::error;
use std::path::PathBuf;
use std::sync::LazyLock;

/// Fallback UID for a non-elevated process whose SID cannot be read.
///
/// Real Unix-style UIDs do not exist on Windows. The closest honest equivalent is the
/// **RID** — the last component of the account's SID — which is stable per account and
/// is what [`account_rid`] reports. This sentinel is only for the case where the SID is
/// unavailable.
const NON_ELEVATED_UID: u32 = 1000;

/// Fallback GID for a non-elevated process (see [`NON_ELEVATED_UID`]).
const NON_ELEVATED_GID: u32 = 1000;

/// The current account's RID, read once.
///
/// cash: `$UID` and `$EUID` reported a hardcoded 1000 while the `id` builtin reported the
/// real RID, so the shell disagreed with its own builtin about who was running it. A
/// script comparing `$UID` against `$(id -u)` — a normal way to check for a privilege
/// change — saw two different answers.
static ACCOUNT_RID: LazyLock<Option<u32>> = LazyLock::new(account_rid);

/// Read the account's RID from its SID.
fn account_rid() -> Option<u32> {
    // `whoami /user` is the documented way to reach the process token's SID without a
    // Win32 binding, and it ships with every Windows install.
    let output = std::process::Command::new("whoami.exe")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout);
    // `"DOMAIN\user","S-1-5-21-...-1001"`
    let sid = text.split('"').nth(3)?.trim();
    if !sid.starts_with("S-") {
        return None;
    }

    sid.rsplit('-').next()?.parse().ok()
}

/// Cached elevation status. The underlying check queries the process token,
/// which can't change after process start, so it's safe to memoize.
static IS_ELEVATED: LazyLock<bool> = LazyLock::new(|| {
    check_elevation::is_elevated().unwrap_or_else(|err| {
        tracing::warn!("failed to determine process elevation: {err}");
        false
    })
});

pub(crate) fn get_user_home_dir(_username: &str) -> Option<PathBuf> {
    // std::env::home_dir() doesn't support getting home dir for arbitrary users
    // For now, we only support getting the current user's home dir
    None
}

pub(crate) fn get_current_user_home_dir() -> Option<PathBuf> {
    std::env::home_dir()
}

pub(crate) fn get_current_user_default_shell() -> Option<PathBuf> {
    None
}

fn is_elevated() -> bool {
    *IS_ELEVATED
}

pub(crate) fn is_root() -> bool {
    is_elevated()
}

pub(crate) fn get_current_uid() -> Result<u32, error::Error> {
    // Elevated reports 0, matching the root convention scripts test for. Otherwise the
    // account's RID, so `$UID` agrees with `id -u`.
    Ok(if is_elevated() {
        0
    } else {
        ACCOUNT_RID.unwrap_or(NON_ELEVATED_UID)
    })
}

pub(crate) fn get_current_gid() -> Result<u32, error::Error> {
    // Windows has no primary group the way POSIX does; the account's own RID is the
    // closest honest answer, and it is what the `id` builtin reports too.
    Ok(if is_elevated() {
        0
    } else {
        ACCOUNT_RID.unwrap_or(NON_ELEVATED_GID)
    })
}

pub(crate) fn get_effective_uid() -> Result<u32, error::Error> {
    // Elevated reports 0, matching the root convention scripts test for. Otherwise the
    // account's RID, so `$UID` agrees with `id -u`.
    Ok(if is_elevated() {
        0
    } else {
        ACCOUNT_RID.unwrap_or(NON_ELEVATED_UID)
    })
}

pub(crate) fn get_effective_gid() -> Result<u32, error::Error> {
    // Windows has no primary group the way POSIX does; the account's own RID is the
    // closest honest answer, and it is what the `id` builtin reports too.
    Ok(if is_elevated() {
        0
    } else {
        ACCOUNT_RID.unwrap_or(NON_ELEVATED_GID)
    })
}

pub(crate) fn get_current_username() -> Result<String, error::Error> {
    let username = whoami::username().map_err(std::io::Error::from)?;
    Ok(username)
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_user_group_ids() -> Result<Vec<u32>, error::Error> {
    // TODO(windows): implement some version of this for Windows
    Ok(vec![])
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_all_users() -> Result<Vec<String>, error::Error> {
    // TODO(windows): implement some version of this for Windows
    Ok(vec![])
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_all_groups() -> Result<Vec<String>, error::Error> {
    // TODO(windows): implement some version of this for Windows
    Ok(vec![])
}
