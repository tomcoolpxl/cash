//! `id`, `groups`, `whoami`, `who`, `users`, `pinky`, `logname`, and `hostid` — **D48**, **§4 #19**.
//!
//! Windows identifies a user by **SID**, not by an integer, so cash reports the SID and
//! the account name. `id -u` prints the RID (last component of the SID), which is the
//! closest thing Windows has to a uid and is stable per account, or 0 in an elevated
//! shell, the same value as `$EUID` (spec §4 row 20).
//!
//! `who`, `users`, `pinky`, `logname`, and `hostid` query active Windows sessions,
//! local user profiles, and host identifiers.

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
        // The same numbers `$EUID` and `$GROUPS` hold: 0 when elevated, the RID otherwise.
        let uid = cash_core::identity::effective_uid()
            .map_or_else(|| identity.rid.clone(), |uid| uid.to_string());
        let gid = cash_core::identity::effective_gid()
            .map_or_else(|| identity.rid.clone(), |gid| gid.to_string());

        if self.user {
            if self.name {
                writeln!(stdout, "{}", identity.user)?;
            } else {
                writeln!(stdout, "{uid}")?;
            }
        } else if self.group || self.groups {
            // Windows has no single "primary group" the way POSIX does; the account's
            // own SID is the closest honest answer, and inventing a gid would be the
            // same mistake MSYS makes.
            if self.name {
                writeln!(stdout, "{}", identity.user)?;
            } else {
                writeln!(stdout, "{gid}")?;
            }
        } else {
            writeln!(stdout, "uid={uid}({}) sid={}", identity.user, identity.sid)?;
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

/// Print user's login name.
#[derive(Parser)]
pub(crate) struct LognameCommand {}

impl builtins::Command for LognameCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let user = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .ok();
        if let Some(user) = user {
            writeln!(context.stdout(), "{user}")?;
            Ok(ExecutionResult::success())
        } else {
            writeln!(context.stderr(), "logname: no login name")?;
            Ok(ExecutionResult::general_error())
        }
    }
}

/// Print the numeric identifier (in hexadecimal) for the current host.
#[derive(Parser)]
pub(crate) struct HostidCommand {}

impl builtins::Command for HostidCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let name = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "localhost".to_string());
        let mut hash: u32 = 0x811c_9dc5;
        for byte in name.bytes() {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
        writeln!(context.stdout(), "{hash:08x}")?;
        Ok(ExecutionResult::success())
    }
}

/// Print the user names of users currently logged in to the current host.
#[derive(Parser)]
pub(crate) struct UsersCommand {}

impl builtins::Command for UsersCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut users = query_logged_in_users();
        users.sort();
        users.dedup();
        writeln!(context.stdout(), "{}", users.join(" "))?;
        Ok(ExecutionResult::success())
    }
}

/// Show who is logged on.
#[derive(Parser)]
pub(crate) struct WhoCommand {
    /// Print only the username and terminal associated with standard input ("who am i").
    #[arg(short = 'm')]
    only_stdin: bool,

    /// Print headings.
    #[arg(short = 'H', long = "heading")]
    heading: bool,

    /// Quick mode: all login names and number of users logged on.
    #[arg(short = 'q', long = "count")]
    count: bool,

    /// Extra arguments (e.g. `who am i` or `who mom likes`).
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

impl builtins::Command for WhoCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();
        let sessions = query_sessions();

        if self.count {
            let names: Vec<_> = sessions.iter().map(|s| s.username.as_str()).collect();
            writeln!(stdout, "{}", names.join(" "))?;
            writeln!(stdout, "# users={}", names.len())?;
            return Ok(ExecutionResult::success());
        }

        // Two args like `who am i` behaves like -m
        let only_me = self.only_stdin || self.args.len() == 2;

        if self.heading {
            writeln!(stdout, "{:<12} {:<12} {:<16}", "NAME", "LINE", "TIME")?;
        }

        let my_user = std::env::var("USERNAME").unwrap_or_default();
        for s in &sessions {
            if only_me && !my_user.is_empty() && s.username != my_user {
                continue;
            }
            writeln!(
                stdout,
                "{:<12} {:<12} {:<16}",
                s.username, s.terminal, s.logon_time
            )?;
        }

        Ok(ExecutionResult::success())
    }
}

/// Lightweight user information query tool.
#[derive(Parser)]
pub(crate) struct PinkyCommand {
    /// Users to query.
    users: Vec<String>,
}

impl builtins::Command for PinkyCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();
        if self.users.is_empty() {
            let sessions = query_sessions();
            writeln!(
                stdout,
                "{:<10} {:<16} {:<10} {:<16}",
                "Login", "Name", "TTY", "When"
            )?;
            for s in sessions {
                writeln!(
                    stdout,
                    "{:<10} {:<16} {:<10} {:<16}",
                    s.username, s.username, s.terminal, s.logon_time
                )?;
            }
        } else {
            for user in &self.users {
                let profile =
                    std::env::var("USERPROFILE").unwrap_or_else(|_| format!("C:\\Users\\{user}"));
                writeln!(stdout, "Login name: {user:<20} In real life: {user}")?;
                writeln!(stdout, "Directory:  {profile:<20} Shell:        cash")?;
            }
        }
        Ok(ExecutionResult::success())
    }
}

struct SessionInfo {
    username: String,
    terminal: String,
    logon_time: String,
}

fn query_sessions() -> Vec<SessionInfo> {
    if let Ok(output) = std::process::Command::new("quser.exe").output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut list = Vec::new();
            for line in text.lines().skip(1) {
                let trimmed = line.trim_start_matches(['>', ' ']);
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 4 {
                    let username = parts[0].to_string();
                    let terminal = parts[1].to_string();
                    let logon_time = if parts.len() >= 6 {
                        format!("{} {}", parts[parts.len() - 2], parts[parts.len() - 1])
                    } else {
                        parts.last().unwrap_or(&"").to_string()
                    };
                    list.push(SessionInfo {
                        username,
                        terminal,
                        logon_time,
                    });
                }
            }
            if !list.is_empty() {
                return list;
            }
        }
    }

    let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".to_string());
    vec![SessionInfo {
        username: user,
        terminal: "console".to_string(),
        logon_time: "current".to_string(),
    }]
}

fn query_logged_in_users() -> Vec<String> {
    query_sessions().into_iter().map(|s| s.username).collect()
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
    let output = std::process::Command::new("whoami.exe")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout);
    let sid = text.split('"').nth(3)?.trim().to_string();
    sid.starts_with("S-").then_some(sid)
}
