//! `logger`, util-linux's, writing to the Application event log (decisions.md).
//!
//! Windows' system log is the event log, so a message goes there under the source `cash`,
//! its priority's level choosing the entry's kind: `emerg` to `err` an error, `warning` a
//! warning, the rest information. The entry's text is syslog's `TAG[ID]: MESSAGE`. `-s`
//! also prints the message as util-linux does, `<PRI>Mmm dd hh:mm:ss TAG: MESSAGE`. The
//! options for a remote or local syslog daemon have nothing behind them on Windows and are
//! refused by name.

use std::io::{BufRead, Write};

use cash_core::input::{Dash, InputError};
use cash_core::{ExecutionResult, builtins};
use cash_win32::eventlog;
use clap::Parser;

/// syslog's facilities, by name and number.
const FACILITIES: [(&str, u8); 22] = [
    ("kern", 0),
    ("user", 1),
    ("mail", 2),
    ("daemon", 3),
    ("auth", 4),
    ("security", 4),
    ("syslog", 5),
    ("lpr", 6),
    ("news", 7),
    ("uucp", 8),
    ("cron", 9),
    ("authpriv", 10),
    ("ftp", 11),
    ("local0", 16),
    ("local1", 17),
    ("local2", 18),
    ("local3", 19),
    ("local4", 20),
    ("local5", 21),
    ("local6", 22),
    ("local7", 23),
    ("mark", 24),
];

/// syslog's levels, by name and number.
const LEVELS: [(&str, u8); 11] = [
    ("emerg", 0),
    ("panic", 0),
    ("alert", 1),
    ("crit", 2),
    ("err", 3),
    ("error", 3),
    ("warning", 4),
    ("warn", 4),
    ("notice", 5),
    ("info", 6),
    ("debug", 7),
];

/// Write a message to the system log: on Windows, the Application event log.
#[derive(Parser)]
pub(crate) struct LoggerCommand {
    /// The priority, `FACILITY.LEVEL` or `LEVEL` (`user.notice` by default).
    #[arg(short = 'p', long = "priority", value_name = "PRIORITY")]
    priority: Option<String>,

    /// Mark each line with this tag (the login name by default).
    #[arg(short = 't', long = "tag", value_name = "TAG")]
    tag: Option<String>,

    /// Also write the message to standard error.
    #[arg(short = 's', long = "stderr")]
    stderr: bool,

    /// Log this process's id with each line.
    #[arg(short = 'i')]
    pid: bool,

    /// Log this id, or this process's, with each line.
    #[arg(long = "id", value_name = "ID", num_args = 0..=1, default_missing_value = "")]
    id: Option<String>,

    /// Log each line of this file.
    #[arg(short = 'f', long = "file", value_name = "FILE")]
    file: Option<String>,

    /// Skip empty lines of a file or standard input.
    #[arg(short = 'e', long = "skip-empty")]
    skip_empty: bool,

    /// Cut each message to this many bytes.
    #[arg(
        short = 'S',
        long = "size",
        value_name = "SIZE",
        default_value_t = 1024
    )]
    size: usize,

    /// Do everything but write to the log.
    #[arg(long = "no-act")]
    no_act: bool,

    #[command(flatten)]
    daemon: DaemonOptions,

    /// The message; without one, each line of `-f`'s file or of standard input.
    message: Vec<String>,
}

/// The options for a syslog daemon, refused by name: Windows has none to send to.
#[derive(clap::Args)]
struct DaemonOptions {
    /// Log to this syslog server.
    #[arg(short = 'n', long = "server", value_name = "NAME")]
    server: Option<String>,

    /// The server's port.
    #[arg(short = 'P', long = "port", value_name = "PORT")]
    port: Option<String>,

    /// Use TCP to the server.
    #[arg(short = 'T', long = "tcp")]
    tcp: bool,

    /// Use UDP to the server.
    #[arg(short = 'd', long = "udp")]
    udp: bool,

    /// Log to this socket.
    #[arg(short = 'u', long = "socket", value_name = "SOCKET")]
    socket: Option<String>,

    /// Log to the systemd journal.
    #[arg(long = "journald", value_name = "FILE", num_args = 0..=1, default_missing_value = "")]
    journald: Option<String>,
}

impl DaemonOptions {
    /// The first daemon option given, as it is spelled.
    fn given(&self) -> Option<&'static str> {
        [
            (self.server.is_some(), "--server"),
            (self.port.is_some(), "--port"),
            (self.tcp, "--tcp"),
            (self.udp, "--udp"),
            (self.socket.is_some(), "--socket"),
            (self.journald.is_some(), "--journald"),
        ]
        .into_iter()
        .find_map(|(given, name)| given.then_some(name))
    }
}

/// A priority's facility and level, or util-linux's message for the name it does not know.
fn parse_priority(text: &str) -> Result<(u8, u8), String> {
    if let Ok(number) = text.parse::<u8>() {
        return Ok((number >> 3, number & 7));
    }
    let (facility, level) = match text.split_once('.') {
        Some((facility, level)) => (Some(facility), level),
        None => (None, text),
    };
    let facility = match facility {
        Some(name) => FACILITIES
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
            .map(|(_, number)| *number)
            .ok_or_else(|| std::format!("unknown facility name: {name}"))?,
        None => 1,
    };
    let level = LEVELS
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(level))
        .map(|(_, number)| *number)
        .ok_or_else(|| std::format!("unknown priority name: {level}"))?;
    Ok((facility, level))
}

/// The event log's kind for a syslog level.
const fn kind_of(level: u8) -> eventlog::Kind {
    match level {
        0..=3 => eventlog::Kind::Error,
        4 => eventlog::Kind::Warning,
        _ => eventlog::Kind::Information,
    }
}

/// `text` cut to at most `size` bytes, at a character's edge.
fn cut(text: &str, size: usize) -> &str {
    let mut end = text.len().min(size);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or(text)
}

impl builtins::Command for LoggerCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let name = context.command_name.clone();
        if let Some(option) = self.daemon.given() {
            writeln!(
                context.stderr(),
                "{name}: {option}: not supported: messages go to the Application event log"
            )?;
            return Ok(ExecutionResult::general_error());
        }
        let (facility, level) =
            match parse_priority(self.priority.as_deref().unwrap_or("user.notice")) {
                Ok(priority) => priority,
                Err(message) => {
                    writeln!(context.stderr(), "{name}: {message}")?;
                    return Ok(ExecutionResult::general_error());
                }
            };
        let tag = self.tag.clone().unwrap_or_else(|| {
            // process state: the account cash runs as, as logname reports it.
            std::env::var("USERNAME").unwrap_or_default()
        });
        let id = match (self.id.as_deref(), self.pid) {
            (Some(id), _) if !id.is_empty() => std::format!("[{id}]"),
            (Some(_), _) | (None, true) => std::format!("[{}]", std::process::id()),
            (None, false) => String::new(),
        };

        let lines: Vec<String> = if !self.message.is_empty() {
            vec![self.message.join(" ")]
        } else if let Some(file) = &self.file {
            // Opened as the shell's `<` opens it, with util-linux's words.
            match context.open_input(file, Dash::File) {
                Ok(input) => String::from_utf8_lossy(&input.read_all()?)
                    .lines()
                    .map(str::to_owned)
                    .collect(),
                // util-linux opens a directory and reads nothing from it: nothing is logged.
                Err(InputError::IsADirectory(_)) => Vec::new(),
                Err(error) => {
                    writeln!(context.stderr(), "{name}: file {file}: {}", error.reason())?;
                    return Ok(ExecutionResult::general_error());
                }
            }
        } else {
            let mut lines = Vec::new();
            for line in std::io::BufReader::new(context.stdin()).lines() {
                let line = line?;
                lines.push(line.strip_suffix('\r').unwrap_or(&line).to_owned());
            }
            lines
        };

        let priority = facility * 8 + level;
        for line in lines {
            if self.skip_empty && line.is_empty() {
                continue;
            }
            let entry = std::format!("{tag}{id}: {}", cut(&line, self.size));
            if self.stderr {
                let stamp = chrono::Local::now().format("%b %e %H:%M:%S");
                writeln!(context.stderr(), "<{priority}>{stamp} {entry}")?;
            }
            if self.no_act {
                continue;
            }
            if let Err(e) = eventlog::report(kind_of(level), &entry) {
                let e = cash_core::error::os_error_text(&e);
                writeln!(
                    context.stderr(),
                    "{name}: cannot write to the event log: {e}"
                )?;
                return Ok(ExecutionResult::general_error());
            }
        }
        Ok(ExecutionResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priorities_are_syslogs() {
        assert_eq!(parse_priority("user.notice"), Ok((1, 5)));
        assert_eq!(parse_priority("err"), Ok((1, 3)));
        assert_eq!(parse_priority("local3.warn"), Ok((19, 4)));
        assert_eq!(parse_priority("13"), Ok((1, 5)));
        assert_eq!(
            parse_priority("foo.info"),
            Err("unknown facility name: foo".to_owned())
        );
        assert_eq!(
            parse_priority("user.bogus"),
            Err("unknown priority name: bogus".to_owned())
        );
        assert_eq!(kind_of(3), eventlog::Kind::Error);
        assert_eq!(kind_of(4), eventlog::Kind::Warning);
        assert_eq!(kind_of(6), eventlog::Kind::Information);
        assert_eq!(cut("héllo", 2), "h");
    }
}
