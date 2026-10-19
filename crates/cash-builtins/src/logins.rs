//! `who`, `users` and `pinky`: the accounts logged on, from Windows' own list of sessions
//! (`cash_win32::sessions`), in GNU coreutils' layouts.
//!
//! A session is what a Unix login is here: its station (`console`, `rdp-tcp#0`) is the
//! line, its logon the time, and its Windows shell, `explorer.exe`, the process `who -u`
//! gives. Windows keeps no records of dead processes, `init`'s children, run levels or
//! clock changes, so `-d`, `-l`, `-p`, `-r` and `-t` find none, and no login records file.

use std::fmt::Write as _;
use std::io::Write;
use std::time::SystemTime;

use clap::Parser;

use cash_core::{ExecutionResult, builtins};
use cash_win32::sessions::{self, Session};

/// The width of a time as these tools write it, `2026-10-19 08:12`.
const TIME_WIDTH: usize = 16;

/// `when` in the machine's time zone, as GNU's `who` writes it in a locale; `?` unknown.
fn time_string(when: Option<SystemTime>) -> String {
    when.map_or_else(
        || "?".to_owned(),
        |when| {
            chrono::DateTime::<chrono::Local>::from(when)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        },
    )
}

/// The seconds since `when`, when it is known and not in the future.
fn seconds_since(when: Option<SystemTime>) -> Option<u64> {
    SystemTime::now()
        .duration_since(when?)
        .ok()
        .map(|idle| idle.as_secs())
}

/// The line a session is on: its station, or `disconnected` when no one is connected.
fn line_of(session: &Session) -> &str {
    if session.station.is_empty() {
        "disconnected"
    } else {
        &session.station
    }
}

/// The sessions to name, refused with `tool`'s words when a login records file is
/// given, which Windows does not keep.
fn sessions_or_file_error(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    file: Option<&String>,
) -> Result<Option<Vec<Session>>, cash_core::Error> {
    if let Some(file) = file {
        writeln!(
            context.error_stream(),
            "{}: {file}: Windows keeps no login records file; it lists Windows' sessions \
             with no FILE",
            context.command_name
        )?;
        return Ok(None);
    }
    Ok(Some(sessions::sessions()))
}

/// Says that `extra` is one operand too many, as GNU does, with status 1.
fn extra_operand(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    extra: &str,
) -> Result<ExecutionResult, cash_core::Error> {
    writeln!(
        context.error_stream(),
        "{}: extra operand '{extra}'",
        context.command_name
    )?;
    Ok(ExecutionResult::general_error())
}

/// Print the user names of users currently logged in to the current host.
#[derive(Parser)]
pub(crate) struct UsersCommand {
    /// A login records file, which Windows does not keep.
    #[arg(value_name = "FILE")]
    files: Vec<String>,
}

impl builtins::Command for UsersCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if let Some(extra) = self.files.get(1) {
            return extra_operand(&context, extra);
        }
        let Some(sessions) = sessions_or_file_error(&context, self.files.first())? else {
            return Ok(ExecutionResult::general_error());
        };
        // One name for each session, sorted, as GNU's `users` gives one for each login.
        let mut names: Vec<String> = sessions.into_iter().map(|session| session.user).collect();
        names.sort();
        if !names.is_empty() {
            writeln!(context.stdout(), "{}", names.join(" "))?;
        }
        Ok(ExecutionResult::success())
    }
}

/// Show who is logged on.
#[derive(Parser)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one for each of who's options"
)]
pub(crate) struct WhoCommand {
    /// Same as -b -d --login -p -r -t -T -u.
    #[arg(short = 'a', long = "all")]
    all: bool,

    /// Time of last system boot.
    #[arg(short = 'b', long = "boot")]
    boot: bool,

    /// Print dead processes (Windows keeps no record of them).
    #[arg(short = 'd', long = "dead")]
    dead: bool,

    /// Print line of column headings.
    #[arg(short = 'H', long = "heading")]
    heading: bool,

    /// Print system login processes (Windows has none).
    #[arg(short = 'l', long = "login")]
    login: bool,

    /// Attempt to canonicalize hostnames via DNS.
    #[arg(long = "lookup")]
    _lookup: bool,

    /// Only the session of this shell ("who am i").
    #[arg(short = 'm')]
    only_mine: bool,

    /// Print active processes spawned by init (Windows has none).
    #[arg(short = 'p', long = "process")]
    process: bool,

    /// All login names and number of users logged on.
    #[arg(short = 'q', long = "count")]
    count: bool,

    /// Print current runlevel (Windows has none).
    #[arg(short = 'r', long = "runlevel")]
    runlevel: bool,

    /// Print only name, line, and time (default).
    #[arg(short = 's', long = "short")]
    short: bool,

    /// Print last system clock change (Windows keeps no record of it).
    #[arg(short = 't', long = "time")]
    time: bool,

    /// Add user's message status as +, - or ?.
    #[arg(short = 'T', short_alias = 'w', long = "mesg", aliases = ["message", "writable"])]
    mesg: bool,

    /// List users logged in.
    #[arg(short = 'u', long = "users")]
    users: bool,

    /// `am i` (or any two words), or a login records file, which Windows does not keep.
    #[arg(value_name = "FILE | ARG1 ARG2")]
    args: Vec<String>,
}

/// What `who` prints, from its options as GNU's `who` reads them.
#[allow(clippy::struct_excessive_bools, reason = "GNU's own switches")]
struct WhoLayout {
    boot: bool,
    users: bool,
    mesg: bool,
    idle: bool,
    exit: bool,
    short: bool,
}

impl WhoCommand {
    const fn layout(&self) -> WhoLayout {
        let mut assumptions = true;
        let mut layout = WhoLayout {
            boot: false,
            users: false,
            mesg: self.mesg,
            idle: false,
            exit: false,
            short: self.short,
        };
        if self.all {
            layout.boot = true;
            layout.users = true;
            layout.mesg = true;
            layout.idle = true;
            layout.exit = true;
            assumptions = false;
        }
        if self.boot {
            layout.boot = true;
            assumptions = false;
        }
        if self.dead {
            layout.idle = true;
            layout.exit = true;
            assumptions = false;
        }
        if self.login || self.runlevel {
            layout.idle = true;
            assumptions = false;
        }
        if self.process || self.time {
            assumptions = false;
        }
        if self.users {
            layout.users = true;
            layout.idle = true;
            assumptions = false;
        }
        if assumptions {
            layout.users = true;
            layout.short = true;
        }
        if layout.exit {
            layout.short = false;
        }
        layout
    }
}

impl WhoLayout {
    /// One line as GNU's `print_line` makes it, its trailing blanks taken off.
    #[allow(clippy::too_many_arguments, reason = "GNU's columns")]
    fn line(
        &self,
        user: &str,
        state: char,
        line: &str,
        time: &str,
        idle: &str,
        pid: &str,
        comment: &str,
        exit: &str,
    ) -> String {
        let mut out = format!("{user:<8}");
        if self.mesg {
            out.push(' ');
            out.push(state);
        }
        let _ = write!(out, " {line:<12} {time:<TIME_WIDTH$}");
        if self.idle && !self.short {
            let _ = write!(out, " {idle:<6}");
        }
        if !self.short {
            let _ = write!(out, " {pid:>10}");
        }
        let _ = write!(out, " {comment:<8}");
        if self.exit {
            let _ = write!(out, " {exit:<12}");
        }
        out.truncate(out.trim_end().len());
        out
    }
}

/// How long a session has been idle, as GNU's `who` writes it: `.` within the last
/// minute, `HH:MM` within a day, `old` past it, `?` when Windows does not say.
fn who_idle(last_input: Option<SystemTime>) -> String {
    match seconds_since(last_input) {
        None => "  ?".to_owned(),
        Some(seconds) if seconds < 60 => "  .  ".to_owned(),
        Some(seconds) if seconds < 24 * 60 * 60 => {
            format!("{:02}:{:02}", seconds / 3600, seconds % 3600 / 60)
        }
        Some(_) => " old ".to_owned(),
    }
}

impl builtins::Command for WhoCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        // `who am i`: any two words ask for this shell's session; one is a file.
        let (file, only_mine) = match self.args.as_slice() {
            [] => (None, self.only_mine),
            [file] => (Some(file), self.only_mine),
            [_, _] => (None, true),
            [_, _, extra, ..] => return extra_operand(&context, extra),
        };
        let Some(all) = sessions_or_file_error(&context, file)? else {
            return Ok(ExecutionResult::general_error());
        };
        let mine = sessions::current_session_id();
        let sessions: Vec<Session> = all
            .into_iter()
            .filter(|session| !only_mine || Some(session.id) == mine)
            .collect();
        let mut stdout = context.stdout();

        if self.count {
            let names: Vec<&str> = sessions.iter().map(|s| s.user.as_str()).collect();
            writeln!(stdout, "{}", names.join(" "))?;
            writeln!(stdout, "# users={}", names.len())?;
            return Ok(ExecutionResult::success());
        }

        let layout = self.layout();
        if self.heading {
            let heading = layout.line(
                "NAME", ' ', "LINE", "TIME", "IDLE", "PID", "COMMENT", "EXIT",
            );
            writeln!(stdout, "{heading}")?;
        }
        if layout.boot && !only_mine {
            let boot = time_string(Some(sessions::boot_time()));
            writeln!(
                stdout,
                "{}",
                layout.line("", ' ', "system boot", &boot, "", "", "", "")
            )?;
        }
        if layout.users {
            for session in &sessions {
                let pid = if layout.short {
                    String::new()
                } else {
                    sessions::shell_pid(session.id).map_or_else(String::new, |pid| pid.to_string())
                };
                let comment = session
                    .client
                    .as_ref()
                    .map_or_else(String::new, |client| format!("({client})"));
                let row = layout.line(
                    &session.user,
                    '?',
                    line_of(session),
                    &time_string(session.logon),
                    &who_idle(session.last_input),
                    &pid,
                    &comment,
                    "",
                );
                writeln!(stdout, "{row}")?;
            }
        }
        Ok(ExecutionResult::success())
    }
}

/// Lightweight finger: who is logged on, and what an account is.
#[derive(Parser)]
#[clap(disable_help_flag = true)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one for each of pinky's options"
)]
pub(crate) struct PinkyCommand {
    /// Produce long format output for the specified USERs.
    #[arg(short = 'l')]
    long: bool,

    /// Omit the user's home directory and shell in long format.
    #[arg(short = 'b')]
    no_home_and_shell: bool,

    /// Omit the user's project file in long format.
    #[arg(short = 'h')]
    no_project: bool,

    /// Omit the user's plan file in long format.
    #[arg(short = 'p')]
    no_plan: bool,

    /// Do short format output, this is the default.
    #[arg(short = 's')]
    short: bool,

    /// Omit the line of column headings in short format.
    #[arg(short = 'f')]
    no_heading: bool,

    /// Omit the user's full name in short format.
    #[arg(short = 'w')]
    no_full_name: bool,

    /// Omit the user's full name and remote host in short format.
    #[arg(short = 'i')]
    no_full_name_or_where: bool,

    /// Omit the user's full name, remote host and idle time in short format.
    #[arg(short = 'q')]
    no_full_name_where_or_idle: bool,

    /// The accounts to show.
    #[arg(value_name = "USER")]
    users: Vec<String>,
}

/// How long a session has been idle, as GNU's `pinky` writes it: blank within the last
/// minute, `HH:MM` within a day, days past it, `?????` when Windows does not say.
fn pinky_idle(last_input: Option<SystemTime>) -> String {
    match seconds_since(last_input) {
        None => "?????".to_owned(),
        Some(seconds) if seconds < 60 => "     ".to_owned(),
        Some(seconds) if seconds < 24 * 60 * 60 => {
            format!("{:02}:{:02}", seconds / 3600, seconds % 3600 / 60)
        }
        Some(seconds) => format!("{}d", seconds / (24 * 60 * 60)),
    }
}

impl PinkyCommand {
    fn short_format(&self, stdout: &mut impl Write) -> Result<(), cash_core::Error> {
        let full_name =
            !(self.no_full_name || self.no_full_name_or_where || self.no_full_name_where_or_idle);
        let place = !(self.no_full_name_or_where || self.no_full_name_where_or_idle);
        let idle = !self.no_full_name_where_or_idle;
        if !self.no_heading {
            let mut heading = format!("{:<8}", "Login");
            if full_name {
                let _ = write!(heading, " {:<19}", "Name");
            }
            let _ = write!(heading, " {:<9}", " TTY");
            if idle {
                let _ = write!(heading, " {:<6}", "Idle");
            }
            let _ = write!(heading, " {:<TIME_WIDTH$}", "When");
            if place {
                heading.push_str(" Where");
            }
            writeln!(stdout, "{heading}")?;
        }
        for session in sessions::sessions() {
            if !self.users.is_empty()
                && !self
                    .users
                    .iter()
                    .any(|user| user.eq_ignore_ascii_case(&session.user))
            {
                continue;
            }
            let mut row = format!("{:<8}", session.user);
            if full_name {
                let name = cash_win32::account::full_name(&session.user).unwrap_or_default();
                let name: String = name.chars().take(19).collect();
                let _ = write!(row, " {name:<19}");
            }
            let _ = write!(row, " ?{:<8}", line_of(&session));
            if idle {
                let _ = write!(row, " {:<6}", pinky_idle(session.last_input));
            }
            let _ = write!(row, " {}", time_string(session.logon));
            if place && let Some(client) = &session.client {
                let _ = write!(row, " {client}");
            }
            writeln!(stdout, "{row}")?;
        }
        Ok(())
    }

    fn long_format<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        stdout: &mut impl Write,
    ) -> Result<(), cash_core::Error> {
        for user in &self.users {
            write!(stdout, "Login name: {user:<28}In real life: ")?;
            let Some(sid) = cash_win32::account::lookup_user(user) else {
                writeln!(stdout, " ???")?;
                continue;
            };
            writeln!(
                stdout,
                " {}",
                cash_win32::account::full_name(user).unwrap_or_default()
            )?;
            let home = cash_win32::account::profile_directory(&sid);
            if !self.no_home_and_shell {
                let directory = home
                    .as_deref()
                    .map_or_else(String::new, cash_win32::path::render);
                // An account has no login shell on Windows: the shell is this one.
                let shell = context
                    .shell
                    .env_str("SHELL")
                    .map(|shell| shell.into_owned())
                    .or_else(|| {
                        // process state: the program this shell is.
                        std::env::current_exe()
                            .ok()
                            .map(|exe| cash_win32::path::render(&exe))
                    })
                    .unwrap_or_default();
                writeln!(stdout, "Directory: {directory:<29}Shell:  {shell}")?;
            }
            if let Some(home) = &home {
                if !self.no_project
                    && let Ok(project) = std::fs::read(home.join(".project"))
                {
                    write!(stdout, "Project: ")?;
                    stdout.write_all(&project)?;
                }
                if !self.no_plan
                    && let Ok(plan) = std::fs::read(home.join(".plan"))
                {
                    writeln!(stdout, "Plan:")?;
                    stdout.write_all(&plan)?;
                }
            }
            writeln!(stdout)?;
        }
        Ok(())
    }
}

impl builtins::Command for PinkyCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        // -s with -l is the short format.
        let long = self.long && !self.short;
        if long && self.users.is_empty() {
            writeln!(
                context.error_stream(),
                "{}: no username specified; at least one must be specified when using -l",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        }
        let mut stdout = context.stdout();
        if long {
            self.long_format(&context, &mut stdout)?;
        } else {
            self.short_format(&mut stdout)?;
        }
        Ok(ExecutionResult::success())
    }
}
