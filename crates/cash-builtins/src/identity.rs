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
    /// Accepted and ignored, as GNU's id does.
    #[arg(short = 'a', hide = true)]
    _ignored: bool,

    /// Print only the user's id: the RID, or 0 elevated.
    #[arg(short = 'u', long = "user")]
    user: bool,

    /// Print only the group's id.
    #[arg(short = 'g', long = "group")]
    group: bool,

    /// Print the ids of every group.
    #[arg(short = 'G', long = "groups")]
    groups: bool,

    /// Print names rather than numbers, with -u, -g or -G.
    #[arg(short = 'n', long = "name")]
    name: bool,

    /// Print the real ids, the account's own, rather than the effective ones, which are 0
    /// elevated; with -u, -g or -G.
    #[arg(short = 'r', long = "real")]
    real: bool,

    /// End each with a NUL, not a newline or a space; with -u, -g or -G.
    #[arg(short = 'z', long = "zero")]
    zero: bool,

    /// The accounts to report on: the one cash runs as, without.
    users: Vec<String>,
}

impl builtins::Command for IdCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let only = [self.user, self.group, self.groups]
            .iter()
            .filter(|only| **only)
            .count();
        let misuse = if only > 1 {
            Some("cannot print \"only\" of more than one choice")
        } else if only == 0 && (self.name || self.real) {
            Some("cannot print only names or real IDs in default format")
        } else if only == 0 && self.zero {
            Some("option --zero not permitted in default format")
        } else {
            None
        };
        if let Some(misuse) = misuse {
            writeln!(context.stderr(), "{}: {misuse}", context.command_name)?;
            return Ok(ExecutionResult::general_error());
        }

        let mut result = ExecutionResult::success();
        for account in accounts(&self.users, self.real) {
            match account {
                Ok(account) => write!(context.stdout(), "{}", self.report(&account))?,
                Err(problem) => {
                    writeln!(context.stderr(), "{}: {problem}", context.command_name)?;
                    result = ExecutionResult::general_error();
                }
            }
        }
        Ok(result)
    }
}

impl IdCommand {
    /// What `id` prints of `account`, its line ended.
    fn report(&self, account: &Account) -> String {
        let end = if self.zero { '\0' } else { '\n' };
        let id_or_name = |id: u32, name: &str| {
            if self.name {
                name.to_owned()
            } else {
                id.to_string()
            }
        };
        if self.user {
            format!("{}{end}", id_or_name(account.uid, &account.name))
        } else if self.group {
            format!("{}{end}", id_or_name(account.gid, &account.name))
        } else if self.groups {
            let between = if self.zero { "\0" } else { " " };
            let groups: Vec<String> = account
                .all_groups()
                .iter()
                .map(|(id, name)| id_or_name(*id, name))
                .collect();
            format!("{}{end}", groups.join(between))
        } else {
            let groups: Vec<String> = account
                .all_groups()
                .iter()
                .map(|(id, name)| format!("{id}({name})"))
                .collect();
            format!(
                "uid={}({}) gid={}({}) groups={} sid={}\n",
                account.uid,
                account.name,
                account.gid,
                account.name,
                groups.join(","),
                account.sid
            )
        }
    }
}

/// Print the groups a user belongs to.
#[derive(Parser)]
pub(crate) struct GroupsCommand {
    /// The accounts to report on: the one cash runs as, without.
    users: Vec<String>,
}

impl builtins::Command for GroupsCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();
        for (index, account) in accounts(&self.users, false).into_iter().enumerate() {
            match account {
                Ok(account) => {
                    let names: Vec<String> = account
                        .all_groups()
                        .into_iter()
                        .map(|(_, name)| name)
                        .collect();
                    // Named accounts are each named first, as GNU's groups names them.
                    match self.users.get(index) {
                        Some(user) => writeln!(context.stdout(), "{user} : {}", names.join(" "))?,
                        None => writeln!(context.stdout(), "{}", names.join(" "))?,
                    }
                }
                Err(problem) => {
                    writeln!(context.stderr(), "{}: {problem}", context.command_name)?;
                    result = ExecutionResult::general_error();
                }
            }
        }
        Ok(result)
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
        // process state: the account cash runs as, as whoami reports it.
        let user = std::env::var("USERNAME")
            // process state: the account cash runs as.
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
        // process state: the machine cash runs on, as hostname reports it.
        let name = std::env::var("COMPUTERNAME")
            // process state: the machine cash runs on.
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

        // process state: the account cash runs as.
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
                    // process state: the account cash runs as.
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
    if let Ok(output) =
        // process state: a system program asked about the machine's sessions.
        std::process::Command::new(cash_win32::fs::system_program("quser.exe")).output()
    {
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

    // process state: the account cash runs as.
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

/// An account as `id` and `groups` report it, its ids the RIDs of its SIDs (spec §4 row
/// 20).
struct Account {
    /// Its name without the domain, as `whoami` gives it.
    name: String,
    /// The full security identifier.
    sid: String,
    uid: u32,
    /// Windows has no primary group the way POSIX does: the account's own RID stands for
    /// it, as `$GROUPS` holds it.
    gid: u32,
    /// The groups besides its own: the token's for the process, the local ones for
    /// another account.
    groups: Vec<(u32, String)>,
}

impl Account {
    /// The account cash runs as: its ids 0 when elevated, unless `real`.
    fn current(real: bool) -> Option<Self> {
        let (name, rid) = cash_win32::process::current_process_account()?;
        let sid = cash_win32::process::current_user_sid_string()?;
        let (uid, gid) = if real {
            (rid, rid)
        } else {
            (
                cash_core::identity::effective_uid().unwrap_or(rid),
                cash_core::identity::effective_gid().unwrap_or(rid),
            )
        };
        let groups = cash_win32::account::current_groups()
            .iter()
            .map(named_group)
            .collect();
        Some(Self {
            name,
            sid,
            uid,
            gid,
            groups,
        })
    }

    /// The user account `user` names, or `None` when there is no such user.
    fn named(user: &str) -> Option<Self> {
        let sid = cash_win32::account::lookup_user(user)?;
        let name = cash_win32::account::account_name(&sid)
            .map_or_else(|| user.to_owned(), |full| short_name(&full).to_owned());
        let rid = sid.rid();
        let groups = cash_win32::account::local_groups_of(user)
            .iter()
            .map(named_group)
            .collect();
        Some(Self {
            name,
            sid: sid.text(),
            uid: rid,
            gid: rid,
            groups,
        })
    }

    /// Every group, by id and name: the account's own first, then the others.
    fn all_groups(&self) -> Vec<(u32, String)> {
        let mut all = vec![(self.gid, self.name.clone())];
        for group in &self.groups {
            if !all.contains(group) {
                all.push(group.clone());
            }
        }
        all
    }
}

/// The accounts `users` name, or the one cash runs as when they are none: each, or what
/// is wrong with it.
fn accounts(users: &[String], real: bool) -> Vec<Result<Account, String>> {
    if users.is_empty() {
        return vec![
            Account::current(real).ok_or_else(|| "cannot determine the current user".to_owned()),
        ];
    }
    users
        .iter()
        .map(|user| Account::named(user).ok_or_else(|| format!("'{user}': no such user")))
        .collect()
}

/// A group's id, the RID of its SID, and its name without the domain (`Users`,
/// `Authenticated Users`).
fn named_group(sid: &cash_win32::account::Sid) -> (u32, String) {
    let name = cash_win32::account::account_name(sid)
        .map_or_else(|| sid.text(), |full| short_name(&full).to_owned());
    (sid.rid(), name)
}

/// `name` without its domain: `PC\tom` is `tom`.
fn short_name(name: &str) -> &str {
    name.rsplit('\\').next().unwrap_or(name)
}
