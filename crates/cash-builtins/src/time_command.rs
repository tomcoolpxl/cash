//! `time` as a command: what a script runs through a variable, `command`, `env` or
//! `xargs`, where the `time` keyword is not read, as a keyword is one only when written as
//! such. GNU's `time` program and busybox's applet are such commands, with GNU's
//! interface here; cash had none, so shellspec, which runs `$SHELLSPEC_TIME COMMAND` with
//! `time -p` when `$PWD` begins with a drive, could not run. Written as the first word of
//! a command, `time` is still the keyword, as in Bash.
//!
//! The command runs as `nohup` runs its own (`run_for_builtin`), timed by the stopwatch
//! the keyword uses: the wall clock, and the user and system time of this process and of
//! the programs it waited for. Windows gives no count of a program's page faults, swaps or
//! context switches as a command ends, and GNU's `time` prints 0 for what a system does
//! not give it; so does this.

use std::fmt::Write as _;
use std::io::Write;
use std::time::Duration;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// GNU's default report.
const DEFAULT_FORMAT: &str = "%Uuser %Ssystem %Eelapsed %PCPU (%Xtext+%Ddata %Mmax)k\n\
%Iinputs+%Ooutputs (%Fmajor+%Rminor)pagefaults %Wswaps";

/// The POSIX report, `-p`.
const POSIX_FORMAT: &str = "real %e\nuser %U\nsys %S";

/// GNU's `-v` report.
const VERBOSE_FORMAT: &str = "\tCommand being timed: \"%C\"\n\
\tUser time (seconds): %U\n\
\tSystem time (seconds): %S\n\
\tPercent of CPU this job got: %P\n\
\tElapsed (wall clock) time (h:mm:ss or m:ss): %E\n\
\tAverage shared text size (kbytes): %X\n\
\tAverage unshared data size (kbytes): %D\n\
\tAverage stack size (kbytes): %p\n\
\tAverage total size (kbytes): %K\n\
\tMaximum resident set size (kbytes): %M\n\
\tAverage resident set size (kbytes): %t\n\
\tMajor (requiring I/O) page faults: %F\n\
\tMinor (reclaiming a frame) page faults: %R\n\
\tVoluntary context switches: %w\n\
\tInvoluntary context switches: %c\n\
\tSwaps: %W\n\
\tFile system inputs: %I\n\
\tFile system outputs: %O\n\
\tSocket messages sent: %s\n\
\tSocket messages received: %r\n\
\tSignals delivered: %k\n\
\tPage size (bytes): %Z\n\
\tExit status: %x";

/// Run COMMAND, then report how long it took.
#[derive(Parser)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one for each of GNU time's switches"
)]
pub(crate) struct TimeCommand {
    /// Report in the POSIX format: real, user and sys seconds.
    #[arg(short = 'p', long = "portability")]
    portability: bool,

    /// Report in FORMAT (%e %E %U %S %P %x %C %Z; % letters Windows does not measure
    /// give 0).
    #[arg(short = 'f', long = "format", value_name = "FORMAT")]
    format: Option<String>,

    /// Write the report to FILE, not to standard error.
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    output: Option<String>,

    /// With -o, add to FILE rather than replace it.
    #[arg(short = 'a', long = "append")]
    append: bool,

    /// Report every measure, one a line.
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    /// Do not say that the command ended with a status other than 0.
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// The command to run, and its arguments.
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

/// Where the command is among `time`'s words (D81): after the options and their values.
pub(crate) fn command_operand(words: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(word) = words.get(index) {
        match word.as_str() {
            "--" => return words.get(index + 1).map(|_| index + 1),
            "-f" | "-o" | "--format" | "--output" => index += 2,
            "--help" | "--version" | "-V" => return None,
            option if option.starts_with('-') && option.len() > 1 => index += 1,
            _ => return Some(index),
        }
    }
    None
}

/// `duration` in seconds to two places, as GNU's `time` writes them.
fn seconds(duration: Duration) -> String {
    format!(
        "{}.{:02}",
        duration.as_secs(),
        duration.subsec_millis() / 10
    )
}

/// The wall clock as `%E` writes it: `m:ss.cc`, or `h:mm:ss` past an hour.
fn elapsed(duration: Duration) -> String {
    let total = duration.as_secs();
    let (hours, minutes, secs) = (total / 3600, total % 3600 / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}.{:02}", duration.subsec_millis() / 10)
    }
}

/// What was measured, for [`report`].
struct Measures<'a> {
    wall: Duration,
    user: Duration,
    system: Duration,
    status: u8,
    command: &'a [String],
}

/// The report `format` asks for, its `\n`, `\t` and `\\` read as GNU's `time` reads them.
fn report(format: &str, measures: &Measures<'_>) -> String {
    let mut out = String::new();
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        match c {
            '%' => match chars.next() {
                Some('e') => out.push_str(&seconds(measures.wall)),
                Some('E') => out.push_str(&elapsed(measures.wall)),
                Some('U') => out.push_str(&seconds(measures.user)),
                Some('S') => out.push_str(&seconds(measures.system)),
                Some('P') => {
                    let wall = measures.wall.as_secs_f64();
                    if wall > 0.0 {
                        let cpu = (measures.user + measures.system).as_secs_f64();
                        let _ = write!(out, "{:.0}%", cpu * 100.0 / wall);
                    } else {
                        out.push_str("?%");
                    }
                }
                Some('x') => {
                    let _ = write!(out, "{}", measures.status);
                }
                Some('C') => out.push_str(&measures.command.join(" ")),
                Some('Z') => {
                    let _ = write!(out, "{}", cash_win32::sysinfo::page_size());
                }
                Some('%') => out.push('%'),
                Some(
                    'D' | 'K' | 'M' | 'p' | 't' | 'X' | 'F' | 'R' | 'w' | 'c' | 'W' | 'I' | 'O'
                    | 's' | 'r' | 'k',
                ) => out.push('0'),
                Some(other) => {
                    out.push('?');
                    out.push(other);
                }
                None => out.push('?'),
            },
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('?');
                    out.push(other);
                }
                None => out.push('?'),
            },
            _ => out.push(c),
        }
    }
    out.push('\n');
    out
}

impl builtins::Command for TimeCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let stopwatch = cash_core::timing::start_timing()?;
        let result = match cash_core::commands::run_for_builtin(
            context.shell,
            context.params.clone(),
            &self.command,
        )
        .await
        {
            Ok(result) => result,
            Err(e) => {
                let program = self.command.first().map_or("", String::as_str);
                writeln!(
                    context.stderr(),
                    "time: cannot run {program}: {}",
                    crate::xargs::start_failure(&e)
                )?;
                return Ok(ExecutionResult::new(127));
            }
        };
        let timing = stopwatch.stop()?;
        let status = u8::from(result.exit_code);

        let format = if self.verbose {
            VERBOSE_FORMAT
        } else if self.portability {
            POSIX_FORMAT
        } else {
            self.format.as_deref().unwrap_or(DEFAULT_FORMAT)
        };
        let mut text = String::new();
        if status != 0 && !self.quiet {
            let _ = writeln!(text, "Command exited with non-zero status {status}");
        }
        text.push_str(&report(
            format,
            &Measures {
                wall: timing.wall,
                user: timing.user,
                system: timing.system,
                status,
                command: &self.command,
            },
        ));

        if let Some(file) = &self.output {
            let path = context.shell.absolute_path(std::path::Path::new(file));
            let opened = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(self.append)
                .truncate(!self.append)
                .open(&path);
            match opened {
                Ok(mut out) => out.write_all(text.as_bytes())?,
                Err(e) => {
                    writeln!(
                        context.stderr(),
                        "time: cannot open {file}: {}",
                        cash_win32::errno::strerror(&e)
                    )?;
                    return Ok(ExecutionResult::new(1));
                }
            }
        } else {
            context.stderr().write_all(text.as_bytes())?;
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_read_as_gnu_time_reads_them() {
        let command = vec!["sleep".to_owned(), "1".to_owned()];
        let measures = Measures {
            wall: Duration::from_millis(1234),
            user: Duration::from_millis(10),
            system: Duration::from_millis(20),
            status: 3,
            command: &command,
        };
        assert_eq!(
            report(POSIX_FORMAT, &measures),
            "real 1.23\nuser 0.01\nsys 0.02\n"
        );
        assert_eq!(
            report("%E %P %x [%C] %M %% %q\\t\\n", &measures),
            "0:01.23 2% 3 [sleep 1] 0 % ?q\t\n\n"
        );
        assert_eq!(elapsed(Duration::from_secs(3725)), "1:02:05");
    }

    #[test]
    fn the_command_is_after_the_options_and_their_values() {
        let words =
            |text: &str| -> Vec<String> { text.split_whitespace().map(str::to_owned).collect() };
        assert_eq!(command_operand(&words("ls -l")), Some(0));
        assert_eq!(command_operand(&words("-p -f %e ls")), Some(3));
        assert_eq!(command_operand(&words("-o out --verbose -- -x")), Some(4));
        assert_eq!(command_operand(&words("--help")), None);
    }
}
