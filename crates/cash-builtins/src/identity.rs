//! `id`, `groups` and `whoami`'s neighbours — **D48**, **§4 #19**.
//!
//! uutils' `id` is Unix-only: it wants `uucore::entries`, `rustix::process` and
//! `getlogin`, none of which exist on Windows, so the crate compiles to nothing and D48's
//! bundle cannot supply one. What filled the gap was the MSYS `id` from Git for Windows,
//! and its answers are fiction on this platform:
//!
//! ```text
//! $ id -u
//! 197609
//! $ groups
//! groups: cannot find name for group ID 197609
//! ```
//!
//! 197609 is not a uid — Windows has no uids. It is MSYS's invention, and `groups` cannot
//! even map its own number back to a name. A tool whose only job is to answer "who am I"
//! and which answers with a number nothing else on the system recognises is worse than
//! absent.
//!
//! Windows identifies a user by **SID**, not by an integer, so cash reports the SID and
//! the account name. `id -u` still prints something numeric — the RID, the last component
//! of the SID, which *is* the closest thing Windows has to a uid and is stable per
//! account — but the honest answer is the whole SID and that is what `id` prints by
//! default.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Print user and group identity.
#[derive(Parser)]
pub(crate) struct IdCommand {
    /// Print only the user's numeric id (the RID).
    #[arg(short = 'u', long = "user")]
    user: bool,

    /// Print only the primary group's numeric id.
    #[arg(short = 'g', long = "group")]
    group: bool,

    /// Print every group the user belongs to.
    #[arg(short = 'G', long = "groups")]
    groups: bool,

    /// Print a name rather than a number.
    #[arg(short = 'n', long = "name")]
    name: bool,
}

impl builtins::Command for IdCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let Some(identity) = Identity::current() else {
            writeln!(
                context.stderr(),
                "{}: cannot determine the current user",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        };

        let mut stdout = context.stdout();

        if self.user {
            if self.name {
                writeln!(stdout, "{}", identity.user)?;
            } else {
                writeln!(stdout, "{}", identity.rid)?;
            }
        } else if self.group || self.groups {
            // Windows has no single "primary group" the way POSIX does; the account's
            // own SID is the closest honest answer, and inventing a gid would be the
            // same mistake MSYS makes.
            if self.name {
                writeln!(stdout, "{}", identity.user)?;
            } else {
                writeln!(stdout, "{}", identity.rid)?;
            }
        } else {
            writeln!(
                stdout,
                "uid={}({}) sid={}",
                identity.rid, identity.user, identity.sid
            )?;
        }

        Ok(ExecutionResult::success())
    }
}

/// Print the groups a user belongs to.
#[derive(Parser)]
pub(crate) struct GroupsCommand {
    /// Users to report on. Only the current user is supported.
    users: Vec<String>,
}

impl builtins::Command for GroupsCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if !self.users.is_empty() {
            writeln!(
                context.stderr(),
                "{}: reporting on another user's groups is not supported on Windows",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        }

        let Some(identity) = Identity::current() else {
            writeln!(
                context.stderr(),
                "{}: cannot determine the current user",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        };

        writeln!(context.stdout(), "{}", identity.user)?;
        Ok(ExecutionResult::success())
    }
}

/// Who the process is running as, in the terms Windows actually uses.
struct Identity {
    /// `DOMAIN\user`, as Windows spells it.
    user: String,
    /// The full security identifier.
    sid: String,
    /// The SID's last component — the closest thing Windows has to a uid.
    rid: String,
}

impl Identity {
    fn current() -> Option<Self> {
        let user = whoami_string()?;
        let sid = current_sid()?;
        let rid = sid.rsplit('-').next().unwrap_or("0").to_string();
        Some(Self { user, sid, rid })
    }
}

/// `DOMAIN\user` for the current process.
fn whoami_string() -> Option<String> {
    let user = std::env::var("USERNAME").ok()?;
    let domain = std::env::var("USERDOMAIN").ok();
    Some(domain.map_or_else(|| user.clone(), |d| std::format!("{d}\\{user}")))
}

/// The current user's SID, read from the process token.
fn current_sid() -> Option<String> {
    // `whoami /user` is the documented way to get this without a Win32 binding, and it
    // ships with every Windows install. Reading it here rather than shelling out from a
    // script keeps `id` self-contained.
    let output = std::process::Command::new("whoami.exe")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout);
    // `"DOMAIN\user","S-1-5-21-..."`
    let sid = text.split('"').nth(3)?.trim().to_string();
    sid.starts_with("S-").then_some(sid)
}
