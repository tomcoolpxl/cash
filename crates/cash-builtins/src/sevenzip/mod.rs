//! `7z` and `7za`: 7-Zip 26.03's command line, messages and exit codes, on cash-archive's
//! 7z reader and writer.
//!
//! Checked against Scoop's 7-Zip on Windows (`crates/cash/tests/oracle/7z_cases.sh`):
//! listings, tests, extraction, messages and statuses, line for line.
//!
//! Where cash differs, on purpose (the user's choices, 2026-10-07):
//! - Lines end in LF and names show `/`, as cash's tar and zip print them, where 7-Zip on
//!   Windows writes CRLF and `\`.
//! - The banner's line is cash's.
//! - Times are shown in the zone the shell's `TZ` names, else Windows' own, as cash's other
//!   tools show them; 7-Zip always uses Windows' own.
//! - Switches start with `-` only, as in 7-Zip 26.03 itself.

mod archive;
mod censor;
mod cmdline;
mod extract;
mod help;
mod list;
mod methods;
mod scan;
mod streams;
mod text;
mod update;

use std::cell::{Cell, RefCell};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use cash_core::openfiles::OpenFiles;
use cash_core::timefmt::Zone;
use cash_core::{ExecutionResult, builtins};
use clap::Parser;

use self::cmdline::{CmdLineError, Command, Options, Target};

macro_rules! command {
    ($type:ident, $name:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Parser)]
        #[clap(disable_help_flag = true, disable_version_flag = true)]
        pub(crate) struct $type {
            /// The command, switches and names, read here as 7-Zip reads them.
            #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
            args: Vec<String>,
        }

        impl builtins::Command for $type {
            type Error = cash_core::Error;

            fn new<I>(args: I) -> Result<Self, clap::Error>
            where
                I: IntoIterator<Item = String>,
            {
                Ok(Self {
                    args: args.into_iter().skip(1).collect(),
                })
            }

            async fn execute<SE: cash_core::ShellExtensions>(
                &self,
                context: cash_core::ExecutionContext<'_, SE>,
            ) -> Result<ExecutionResult, Self::Error> {
                Ok(ExecutionResult::new(run($name, &self.args, &context)))
            }
        }
    };
}

command!(
    SevenZipCommand,
    "7z",
    "Add to, list, test or extract a 7z archive, and others, with 7-Zip's options."
);
command!(
    SevenZaCommand,
    "7za",
    "7z under 7-Zip's standalone name: the same command."
);

/// 7-Zip's exit codes.
mod code {
    pub(super) const FATAL: u8 = 2;
    pub(super) const USER: u8 = 7;
    pub(super) const BREAK: u8 = 255;
}

/// Why a command stopped short of its own summary: 7-Zip's exceptions, which its `main`
/// words.
#[derive(Debug)]
enum Stop {
    /// Ctrl-C, or the end of the input at a question: "Break signaled".
    Break,
    /// A system error: "System ERROR:" and Windows' words for it.
    System(io::Error),
    /// A message: "ERROR:" and the message.
    Message(String),
    /// A command line refused once the command had started: "Command Line Error:".
    CommandLine(CmdLineError),
}

impl From<io::Error> for Stop {
    fn from(error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::Interrupted {
            Self::Break
        } else {
            Self::System(error)
        }
    }
}

/// The two streams 7-Zip writes, as it writes them when they are not a console: each
/// holds what it is given until flushed, standard output first at the end. Messages,
/// errors and progress go to one or the other, or nowhere (`-bso`, `-bse`, `-bsp`).
struct Console<'a, SE: cash_core::ShellExtensions> {
    context: &'a cash_core::ExecutionContext<'a, SE>,
    held: RefCell<[String; 2]>,
    terminal: [bool; 2],
    messages: Cell<Target>,
    errors: Cell<Target>,
}

impl<'a, SE: cash_core::ShellExtensions> Console<'a, SE> {
    fn new(context: &'a cash_core::ExecutionContext<'a, SE>) -> Self {
        let terminal = [OpenFiles::STDOUT_FD, OpenFiles::STDERR_FD]
            .map(|fd| context.try_fd(fd).is_some_and(|f| f.is_terminal()));
        Self {
            context,
            held: RefCell::new([String::new(), String::new()]),
            terminal,
            messages: Cell::new(Target::Out),
            errors: Cell::new(Target::Err),
        }
    }

    const fn slot(target: Target) -> Option<usize> {
        match target {
            Target::Off => None,
            Target::Out => Some(0),
            Target::Err => Some(1),
        }
    }

    fn write(&self, target: Target, text: &str) {
        let Some(slot) = Self::slot(target) else {
            return;
        };
        self.held.borrow_mut()[slot].push_str(text);
        if self.terminal[slot] {
            self.flush(target);
        }
    }

    fn flush(&self, target: Target) {
        let Some(slot) = Self::slot(target) else {
            return;
        };
        let text = std::mem::take(&mut self.held.borrow_mut()[slot]);
        if text.is_empty() {
            return;
        }
        let result = if slot == 0 {
            let mut out = self.context.stdout();
            out.write_all(text.as_bytes()).and_then(|()| out.flush())
        } else {
            let mut err = self.context.stderr();
            err.write_all(text.as_bytes()).and_then(|()| err.flush())
        };
        let _ = result;
    }

    /// Standard output itself, where the listing goes whatever `-bso` says.
    fn stdout(&self, text: &str) {
        self.write(Target::Out, text);
    }

    /// The messages' stream (7-Zip's `g_StdStream`).
    fn so(&self, text: &str) {
        self.write(self.messages.get(), text);
    }

    /// The errors' stream (7-Zip's `g_ErrStream`).
    fn se(&self, text: &str) {
        self.write(self.errors.get(), text);
    }

    fn flush_stdout(&self) {
        self.flush(Target::Out);
    }

    fn flush_so(&self) {
        self.flush(self.messages.get());
    }

    fn flush_se(&self) {
        self.flush(self.errors.get());
    }

    /// Writes raw data to standard output (`-so`), after what it holds.
    fn data(&self, bytes: &[u8]) -> io::Result<()> {
        self.flush_stdout();
        let mut out = self.context.stdout();
        out.write_all(bytes)
    }

    /// Everything held, standard output first, as the C library flushes at exit.
    fn finish(&self) {
        self.flush(Target::Out);
        self.flush(Target::Err);
    }

    /// 7-Zip's `PrintError`: what the messages hold, then the words on the errors' stream.
    fn print_error(&self, message: &str) {
        self.flush_so();
        self.se(&format!("\n\n{message}\n"));
    }
}

/// The shell's view of the world 7-Zip needs: paths made absolute against the shell's
/// folder, times shown in its zone.
struct Env<'a, SE: cash_core::ShellExtensions> {
    context: &'a cash_core::ExecutionContext<'a, SE>,
    zone: Zone,
}

impl<SE: cash_core::ShellExtensions> Env<'_, SE> {
    fn path(&self, name: &str) -> PathBuf {
        self.context.shell.absolute_path(Path::new(name))
    }
}

fn run<SE: cash_core::ShellExtensions>(
    name: &str,
    args: &[String],
    context: &cash_core::ExecutionContext<'_, SE>,
) -> u8 {
    let console = Console::new(context);
    let code = run_to_code(name, args, context, &console);
    console.finish();
    code
}

fn run_to_code<SE: cash_core::ShellExtensions>(
    name: &str,
    args: &[String],
    context: &cash_core::ExecutionContext<'_, SE>,
    console: &Console<'_, SE>,
) -> u8 {
    if args.is_empty() {
        console.so(help::BANNER);
        console.so(&help::usage(name));
        return 0;
    }
    let parsed = match cmdline::parse_switches(args) {
        Ok(parsed) => parsed,
        Err(error) => return command_line_error(console, &error),
    };
    console.messages.set(parsed.messages_target());
    console.errors.set(parsed.errors_target());
    if parsed.help {
        console.so(help::BANNER);
        console.so(&help::usage(name));
        return 0;
    }
    if parsed.headers {
        console.so(help::BANNER);
    }
    let options = match cmdline::parse_command(&parsed) {
        Ok(options) => options,
        Err(error) => return command_line_error(console, &error),
    };
    let env = Env {
        context,
        zone: Zone::of_shell(context.shell),
    };
    match dispatch(&options, &env, console) {
        Ok(code) => code,
        Err(Stop::Break) => {
            console.print_error("Break signaled");
            code::BREAK
        }
        Err(Stop::System(error)) => {
            console.print_error("System ERROR:");
            console.se(&format!("{}\n", text::system_message(&error)));
            code::FATAL
        }
        Err(Stop::Message(message)) => {
            console.print_error("ERROR:");
            console.se(&format!("{message}\n"));
            code::FATAL
        }
        Err(Stop::CommandLine(error)) => command_line_error(console, &error),
    }
}

fn command_line_error<SE: cash_core::ShellExtensions>(
    console: &Console<'_, SE>,
    error: &CmdLineError,
) -> u8 {
    console.print_error("Command Line Error:");
    let mut text = error.message.clone();
    if let Some(line) = &error.line {
        text.push('\n');
        text.push_str(line);
    }
    console.se(&format!("{text}\n"));
    code::USER
}

fn dispatch<SE: cash_core::ShellExtensions>(
    options: &Options,
    env: &Env<'_, SE>,
    console: &Console<'_, SE>,
) -> Result<u8, Stop> {
    if let Some(kind) = &options.archive_type
        && archive::Kind::by_name(kind).is_none()
    {
        return Err(Stop::Message("Unsupported archive type".to_owned()));
    }
    match options.command {
        Command::List | Command::Test | Command::Extract | Command::ExtractFull => {
            extract::run(options, env, console)
        }
        Command::Info => {
            console.so(&help::info());
            Ok(0)
        }
        Command::Benchmark => Err(Stop::Message(
            "the benchmark is not part of cash's 7z".to_owned(),
        )),
        Command::Add | Command::Update | Command::Delete | Command::Rename => {
            update::run(options, env, console)
        }
        Command::Hash => Err(Stop::Message("the h command is not implemented".to_owned())),
    }
}
