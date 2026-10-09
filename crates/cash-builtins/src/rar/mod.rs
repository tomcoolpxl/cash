//! `rar` and `unrar`: `WinRAR` 7.23's console `Rar.exe` and `UnRAR.exe`, their commands,
//! messages and exit codes, on cash-archive's RAR reader and writer.
//!
//! Checked against Scoop's `WinRAR` 7.23 on Windows (`crates/cash/tests/oracle/rar_*.sh`).
//! Everything here was learned by running those two tools and reading their manual,
//! `Rar.txt`; never from `UnRAR`'s source, whose licence forbids using it to make a
//! RAR-compatible archiver (the user's choice, 2026-10-08).
//!
//! Where cash differs, on purpose:
//! - Lines end in LF and names show `/`, as cash's tar, zip and 7z print them.
//! - The banner is cash's, and `-?` is cash's own text.
//! - Names and messages are UTF-8 wherever they go; rar writes Windows' ANSI code page
//!   into files and pipes unless told `-scfr`.
//! - Times are shown in the zone the shell's `TZ` names, else Windows' own.
//! - `rar.ini` is read from `%APPDATA%\WinRAR`, one of the two places rar reads it from;
//!   the other, rar's own folder, cash has not.

mod add;
mod agname;
mod cmdline;
mod delete;
mod entry;
mod extract;
mod find;
mod help;
mod identical;
mod item;
mod list;
mod log;
mod modify;
mod open;
mod reconstruct;
mod repair;

use std::cell::{Cell, RefCell};
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};

use cash_core::openfiles::OpenFiles;
use cash_core::timefmt::Zone;
use cash_core::{ExecutionResult, builtins};
use clap::Parser;

use self::cmdline::{Command, Parsed, Switches};

macro_rules! command {
    ($type:ident, $tool:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(Parser)]
        #[clap(disable_help_flag = true, disable_version_flag = true)]
        pub(crate) struct $type {
            /// The command, switches and names, read here as rar reads them.
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
                Ok(ExecutionResult::new(run($tool, &self.args, &context)))
            }
        }
    };
}

command!(
    RarCommand,
    Tool::Rar,
    "Add to, list, test, extract or change a RAR archive, with rar's commands and switches."
);
command!(
    UnrarCommand,
    Tool::Unrar,
    "List, test or extract a RAR archive, with unrar's commands and switches."
);

/// Which of the two tools runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Rar,
    Unrar,
}

impl Tool {
    const fn name(self) -> &'static str {
        match self {
            Self::Rar => "rar",
            Self::Unrar => "unrar",
        }
    }
}

/// rar's exit codes.
mod code {
    pub(super) const SUCCESS: u8 = 0;
    pub(super) const WARNING: u8 = 1;
    pub(super) const FATAL: u8 = 2;
    pub(super) const CRC: u8 = 3;
    pub(super) const LOCKED: u8 = 4;
    pub(super) const WRITE: u8 = 5;
    pub(super) const OPEN: u8 = 6;
    pub(super) const USER: u8 = 7;
    pub(super) const CREATE: u8 = 9;
    pub(super) const NO_FILES: u8 = 10;
    pub(super) const PASSWORD: u8 = 11;
    pub(super) const READ: u8 = 12;
    pub(super) const BAD_ARCHIVE: u8 = 13;
    pub(super) const BREAK: u8 = 255;
}

/// Why a command stopped before its end: rar's "Program aborted" and the like.
#[derive(Debug)]
enum Stop {
    /// Ctrl-C: "User break".
    Break,
    /// Standard input ended at a question: "Program aborted", with the code.
    Aborted(u8),
    /// An archive the command may not change, its error already shown: "Program
    /// aborted" on a line of its own, with the code.
    Refused(u8),
    /// "Quit" at a question.
    Quit,
}

/// Where rar's words go: messages to standard output (standard error with `-ierr`),
/// errors to standard error, nothing with `-inul`; `-idq` keeps errors and questions.
struct Console<'a, SE: cash_core::ShellExtensions> {
    context: &'a cash_core::ExecutionContext<'a, SE>,
    silent: Cell<bool>,
    to_stderr: Cell<bool>,
    quiet: Cell<bool>,
    /// `p`: standard output is the files' data, and no message goes there.
    data_only: Cell<bool>,
    /// Standard error's last words left their line open.
    err_open: Cell<bool>,
}

impl<'a, SE: cash_core::ShellExtensions> Console<'a, SE> {
    const fn new(context: &'a cash_core::ExecutionContext<'a, SE>) -> Self {
        Self {
            context,
            silent: Cell::new(false),
            to_stderr: Cell::new(false),
            quiet: Cell::new(false),
            data_only: Cell::new(false),
            err_open: Cell::new(false),
        }
    }

    /// The run's end: with `-idq`, rar ends standard error's open line, as it ends the
    /// messages' last line when they show.
    fn finish(&self) {
        if self.quiet.get() && self.err_open.get() {
            self.write(true, "\n");
        }
    }

    fn set(&self, switches: &Switches) {
        self.silent.set(switches.silent);
        self.to_stderr.set(switches.to_stderr);
        self.quiet.set(switches.quiet);
    }

    fn write(&self, to_err: bool, text: &str) {
        if to_err && !text.is_empty() {
            self.err_open.set(!text.ends_with('\n'));
        }
        let result = if to_err {
            let mut err = self.context.stderr();
            err.write_all(text.as_bytes()).and_then(|()| err.flush())
        } else {
            let mut out = self.context.stdout();
            out.write_all(text.as_bytes()).and_then(|()| out.flush())
        };
        let _ = result;
    }

    /// A message: a listing, a file's progress, a summary.
    fn msg(&self, text: &str) {
        if !self.silent.get() && !self.quiet.get() && !self.data_only.get() {
            self.write(self.to_stderr.get(), text);
        }
    }

    /// An error, which `-idq` does not hide.
    fn err(&self, text: &str) {
        if !self.silent.get() {
            self.write(true, text);
        }
    }

    /// A message `-idq` does not hide, on the messages' stream.
    fn notice(&self, text: &str) {
        if !self.silent.get() && !self.data_only.get() {
            self.write(self.to_stderr.get(), text);
        }
    }

    /// Data, `p`'s: standard output always.
    fn data(&self, bytes: &[u8]) -> io::Result<()> {
        let mut out = self.context.stdout();
        out.write_all(bytes)
    }
}

/// The run's state: the shell, the switches, the status so far.
struct Rar<'a, SE: cash_core::ShellExtensions> {
    tool: Tool,
    context: &'a cash_core::ExecutionContext<'a, SE>,
    console: &'a Console<'a, SE>,
    zone: Zone,
    switches: Switches,
    status: Cell<u8>,
    /// The password given or typed, kept for the archives that follow.
    password: RefCell<Option<String>>,
    /// `-log`'s files.
    logs: Vec<log::Log>,
}

impl<'a, SE: cash_core::ShellExtensions> Rar<'a, SE> {
    /// A run of `tool` with `switches`, and the password typed before it if any.
    fn new(
        tool: Tool,
        context: &'a cash_core::ExecutionContext<'a, SE>,
        console: &'a Console<'a, SE>,
        switches: Switches,
        password: Option<String>,
    ) -> Self {
        Self {
            tool,
            context,
            console,
            zone: Zone::of_shell(context.shell),
            switches,
            status: Cell::new(code::SUCCESS),
            password: RefCell::new(password),
            logs: Vec::new(),
        }
    }

    /// The exit code of a run that ended with `result`, the words a stop ends with
    /// said.
    fn exit(&self, result: &Result<(), Stop>) -> u8 {
        let code = match *result {
            Ok(()) => self.status(),
            Err(Stop::Break) => {
                self.console.err("\nUser break\n");
                code::BREAK
            }
            Err(Stop::Aborted(code)) => {
                self.console.notice("\n\nProgram aborted\n");
                code
            }
            Err(Stop::Refused(code)) => {
                self.console.notice("\nProgram aborted\n");
                code
            }
            Err(Stop::Quit) => {
                self.console.notice("\nProgram aborted\n");
                code::BREAK
            }
        };
        self.console.finish();
        code
    }

    /// An archive's name, to the `-log` files that log archives.
    fn log_archive(&self, name: &str) {
        for log in self.logs.iter().filter(|log| log.wants_archives()) {
            log.write(name);
        }
    }

    /// A file's name, to the `-log` files that log files.
    fn log_file(&self, name: &str) {
        for log in self.logs.iter().filter(|log| log.wants_files()) {
            log.write(name);
        }
    }

    /// A path made absolute against the shell's folder.
    fn path(&self, name: &str) -> PathBuf {
        self.context.shell.absolute_path(Path::new(name))
    }

    /// Records an error's code: the first error's code stays, over warnings.
    fn fail(&self, code: u8) {
        let now = self.status.get();
        if now == code::SUCCESS || now == code::WARNING {
            self.status.set(code);
        }
    }

    const fn status(&self) -> u8 {
        self.status.get()
    }

    /// A line of standard input, the console's read as typed (shown or not); `None` at
    /// the end of the input.
    fn read_line(&self, shown: bool) -> Result<Option<String>, Stop> {
        let console = self
            .context
            .try_fd(OpenFiles::STDIN_FD)
            .and_then(|file| file.console(true, shown));
        if let Some(mut console) = console {
            return match console.line() {
                Ok(cash_win32::conin::Line::Typed(text)) => {
                    Ok(Some(text.trim_end_matches(['\r', '\n']).to_owned()))
                }
                Ok(cash_win32::conin::Line::EndOfInput) | Err(_) => Ok(None),
                Ok(cash_win32::conin::Line::Interrupted) => Err(Stop::Break),
            };
        }
        let mut stdin = self.context.stdin();
        let mut bytes = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match stdin.read(&mut byte) {
                Ok(0) | Err(_) => {
                    if bytes.is_empty() {
                        return Ok(None);
                    }
                    break;
                }
                Ok(_) => {}
            }
            if byte[0] == b'\n' {
                break;
            }
            bytes.push(byte[0]);
        }
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
    }

    /// Asks for the password a bare `-p` or `-hp` leaves out, the line ended after it.
    fn ask_password(&self) -> Result<String, Stop> {
        let password = self.ask_password_for(None, false, true)?;
        self.console.err("\n");
        Ok(password)
    }

    /// "Enter password (will not be echoed)", `for` an archive or a file or neither,
    /// on a line of its own unless `again` follows rar's "The specified password is
    /// incorrect."; what is typed, without echo. At the end of the input, rar's words
    /// (`after_a_search` as [`stdin_end_words`](Self::stdin_end_words) takes it) and
    /// its abort.
    pub(super) fn ask_password_for(
        &self,
        name: Option<&str>,
        again: bool,
        after_a_search: bool,
    ) -> Result<String, Stop> {
        let start = if again { "" } else { "\n" };
        let question = match name {
            Some(name) => format!("{start}Enter password (will not be echoed) for {name}: "),
            None => format!("{start}Enter password (will not be echoed): "),
        };
        self.console.err(&question);
        if let Some(password) = self.read_password()? {
            return Ok(password);
        }
        self.console.err("Read error in the file stdin");
        if let Some(extra) = self.stdin_end_words(after_a_search) {
            self.console.err(&format!("\n{extra}"));
        }
        Err(Stop::Aborted(code::READ))
    }

    /// The password the switches give, asking for one `-p` alone wants.
    fn given_password(&self) -> Result<Option<String>, Stop> {
        if let Some(password) = self.password.borrow().clone() {
            return Ok(Some(password));
        }
        if self.switches.no_password {
            return Ok(None);
        }
        // A bare `-hp` or `-p` takes the password the other gave, as `-ppw -hp` does.
        let switches = &self.switches;
        let given = [&switches.header_password, &switches.password]
            .into_iter()
            .flatten()
            .find_map(|arg| arg.text().map(str::to_owned));
        let password = match given {
            Some(password) => password,
            None if switches.header_password.is_some() || switches.password.is_some() => {
                self.ask_password()?
            }
            None => return Ok(None),
        };
        *self.password.borrow_mut() = Some(password.clone());
        Ok(Some(password))
    }
}

fn run<SE: cash_core::ShellExtensions>(
    tool: Tool,
    args: &[String],
    context: &cash_core::ExecutionContext<'_, SE>,
) -> u8 {
    let console = Console::new(context);
    run_with(tool, args, context, &console)
}

/// The switches from `rar.ini` and `RARINISWITCHES`, before the command line's.
fn default_switches<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    args: &[String],
) -> Switches {
    let mut switches = Switches::default();
    let typed_no_config = args
        .iter()
        .take_while(|a| a.as_str() != "--")
        .any(|a| a.eq_ignore_ascii_case("-cfg-"));
    if typed_no_config {
        return switches;
    }
    let env = |name: &str| env_var(context, name);
    if let Some(appdata) = env("APPDATA") {
        let command = args
            .iter()
            .find(|a| !a.starts_with('-'))
            .map(|a| a.to_ascii_lowercase())
            .unwrap_or_default();
        if let Ok(bytes) = std::fs::read(cmdline::ini_path(&appdata)) {
            let text = decode_text(&bytes);
            for line in cmdline::ini_switches(&text, &command) {
                cmdline::apply_defaults(&mut switches, &line);
            }
        }
    }
    if let Some(text) = env("RARINISWITCHES") {
        cmdline::apply_defaults(&mut switches, &text);
    }
    switches
}

/// An exported variable of the shell's.
fn env_var<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    name: &str,
) -> Option<String> {
    context
        .shell
        .env()
        .get(name)
        .filter(|(_, var)| var.is_exported())
        .map(|(_, var)| var.value().to_cow_str(context.shell).into_owned())
}

/// A text file as rar reads one: UTF-16 or UTF-8 by its byte order mark, else UTF-8
/// where it is valid, else Windows' ANSI code page, as cash reads it, Latin-1.
pub(super) fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_be_bytes(pair))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

fn run_with<SE: cash_core::ShellExtensions>(
    tool: Tool,
    args: &[String],
    context: &cash_core::ExecutionContext<'_, SE>,
    console: &Console<'_, SE>,
) -> u8 {
    let mut parsed = match cmdline::parse(args) {
        Ok(parsed) => parsed,
        Err(unknown) => {
            // rar reads its switches before it says who it is: no banner here.
            let switches = default_switches(context, args);
            console.set(&switches);
            console.notice(&format!("\nERROR: Unknown option: {}\n", unknown.0));
            return code::USER;
        }
    };
    let mut switches = default_switches(context, args);
    merge(&mut switches, std::mem::take(&mut parsed.switches));
    console.set(&switches);
    let command = parsed.command.as_deref().and_then(Command::parse);
    let command = command.filter(|c| tool == Tool::Rar || c.in_unrar());
    // rar asks for a bare `-p` or `-hp`'s password as it reads the switch, before
    // its banner or anything else; `p` says nothing but the question then.
    let early = if switches.early_password {
        let asking = Rar::new(tool, context, console, switches.clone(), None);
        console
            .data_only
            .set(matches!(command, Some(Command::Print)));
        match asking.ask_password() {
            Ok(password) => {
                console.data_only.set(false);
                Some(password)
            }
            Err(stop) => return asking.exit(&Err(stop)),
        }
    } else {
        None
    };
    if switches.version {
        console.notice(&format!("{}\n", help::VERSION));
        return code::SUCCESS;
    }
    // A bare listing and printed files have no banner, for scripts' sake.
    let bannerless = matches!(
        command,
        Some(
            Command::List {
                form: cmdline::ListForm::Bare,
                ..
            } | Command::Print
        )
    ) && parsed.archive.is_some()
        && !switches.help
        // `-oi4` lists bare names, rar's banner left out.
        || matches!(
            command,
            Some(Command::Add | Command::Update | Command::Freshen | Command::Move { .. })
        ) && switches
            .identical
            .as_deref()
            .and_then(identical::parse)
            .is_some_and(|identical| identical.level == 4);
    if !switches.no_banner && !bannerless {
        console.msg(help::banner(tool));
    }
    if matches!(command, Some(Command::Print)) && parsed.archive.is_some() {
        console.data_only.set(true);
    }
    let (Some(command), false) = (command, switches.help) else {
        console.msg(&help::usage(tool));
        let code = if parsed.command.is_none() || switches.help {
            code::SUCCESS
        } else {
            code::USER
        };
        return code;
    };
    if parsed.archive.is_none() {
        console.msg(&help::usage(tool));
        return code::USER;
    }
    let mut rar = Rar::new(tool, context, console, switches, early);
    rar.logs = log::Log::open_all(&rar.switches.log_names, rar.switches.charsets.log, |name| {
        context.shell.absolute_path(Path::new(name))
    });
    if let Some(name) = generated_name(&rar, &command, &parsed) {
        parsed.archive = Some(name);
    }
    let result = dispatch(&rar, &command, &parsed);
    rar.exit(&result)
}

/// The command line's switches over the defaults: a switch given there wins.
fn merge(defaults: &mut Switches, typed: Switches) {
    // Lists and repeated switches add up; the rest are replaced when typed.
    let mut typed = typed;
    let mut include = std::mem::take(&mut defaults.include);
    include.append(&mut typed.include);
    let mut exclude = std::mem::take(&mut defaults.exclude);
    exclude.append(&mut typed.exclude);
    let base = std::mem::take(defaults);
    *defaults = overlay(base, typed);
    defaults.include = include;
    defaults.exclude = exclude;
}

/// Each switch of `typed` that is set, over `base`.
#[expect(clippy::too_many_lines, reason = "one line a switch, each kept apart")]
fn overlay(base: Switches, typed: Switches) -> Switches {
    macro_rules! pick {
        ($field:ident, bool) => {
            base.$field || typed.$field
        };
        ($field:ident, opt) => {
            typed.$field.or(base.$field)
        };
        ($field:ident, vec) => {
            if typed.$field.is_empty() {
                base.$field
            } else {
                typed.$field
            }
        };
    }
    Switches {
        list_files: pick!(list_files, opt),
        clear_archive_attr: pick!(clear_archive_attr, bool),
        alt_destination: pick!(alt_destination, opt),
        generate_name: pick!(generate_name, opt),
        generate_default: pick!(generate_default, opt),
        ignore_attributes: pick!(ignore_attributes, bool),
        archive_metadata: pick!(archive_metadata, opt),
        only_archive_attr: pick!(only_archive_attr, bool),
        archive_path: pick!(archive_path, opt),
        synchronize: pick!(synchronize, bool),
        no_comments: pick!(no_comments, bool),
        no_config: pick!(no_config, bool),
        case: pick!(case, opt),
        delete_files: pick!(delete_files, bool),
        shared: pick!(shared, bool),
        recycle: pick!(recycle, bool),
        no_sort: pick!(no_sort, bool),
        wipe: pick!(wipe, bool),
        exclude_attr: pick!(exclude_attr, opt),
        include_attr: pick!(include_attr, opt),
        no_empty_dirs: pick!(no_empty_dirs, bool),
        exclude_paths: pick!(exclude_paths, opt),
        exclude_prefix: pick!(exclude_prefix, opt),
        freshen: pick!(freshen, bool),
        header_password: pick!(header_password, opt),
        hash: pick!(hash, opt),
        no_banner: pick!(no_banner, bool),
        no_done: pick!(no_done, bool),
        no_names: pick!(no_names, bool),
        no_percent: pick!(no_percent, bool),
        quiet: pick!(quiet, bool),
        to_stderr: pick!(to_stderr, bool),
        log_errors: pick!(log_errors, opt),
        silent: pick!(silent, bool),
        version: pick!(version, bool),
        lock: pick!(lock, bool),
        keep_broken: pick!(keep_broken, bool),
        log_names: pick!(log_names, vec),
        method: pick!(method, opt),
        compression_params: pick!(compression_params, vec),
        dictionary: pick!(dictionary, opt),
        dictionary_limit: pick!(dictionary_limit, opt),
        skip_encrypted: pick!(skip_encrypted, bool),
        store_types: pick!(store_types, vec),
        threads: pick!(threads, opt),
        include: Vec::new(),
        include_lists: pick!(include_lists, vec),
        overwrite: pick!(overwrite, opt),
        ntfs_compressed: pick!(ntfs_compressed, bool),
        hard_links: pick!(hard_links, bool),
        identical: pick!(identical, opt),
        links: pick!(links, opt),
        incompatible_names: pick!(incompatible_names, bool),
        output_path: pick!(output_path, opt),
        streams: pick!(streams, bool),
        owners: pick!(owners, bool),
        password: pick!(password, opt),
        no_password: pick!(no_password, bool),
        early_password: pick!(early_password, bool),
        quick_open: pick!(quick_open, opt),
        recurse: pick!(recurse, opt),
        recovery_record: pick!(recovery_record, opt),
        recovery_volumes: pick!(recovery_volumes, opt),
        solid: pick!(solid, opt),
        charsets: cmdline::Charsets {
            log: typed.charsets.log.or(base.charsets.log),
            list: typed.charsets.list.or(base.charsets.list),
            comment: typed.charsets.comment.or(base.charsets.comment),
            redirect: typed.charsets.redirect.or(base.charsets.redirect),
        },
        sfx: pick!(sfx, opt),
        stdin: pick!(stdin, opt),
        size_less: pick!(size_less, opt),
        size_more: pick!(size_more, opt),
        test_after: pick!(test_after, bool),
        time_filters: pick!(time_filters, vec),
        keep_time: pick!(keep_time, opt),
        latest_time: pick!(latest_time, bool),
        times: if typed.times == cmdline::TimeStore::default() {
            base.times
        } else {
            typed.times
        },
        update: pick!(update, bool),
        volumes: pick!(volumes, opt),
        erase_disk: pick!(erase_disk, bool),
        versions: pick!(versions, opt),
        pause: pick!(pause, bool),
        work_dir: pick!(work_dir, opt),
        exclude: Vec::new(),
        exclude_lists: pick!(exclude_lists, vec),
        yes: pick!(yes, bool),
        comment_file: pick!(comment_file, opt),
        help: pick!(help, bool),
        ignored: pick!(ignored, vec),
    }
}

/// `-ag`: the archive's name with the date in it.
fn generated_name<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    command: &Command,
    parsed: &Parsed,
) -> Option<String> {
    let format = rar.switches.generate_name.as_deref()?;
    let archive = parsed.archive.as_deref()?;
    let format = if format.is_empty() {
        rar.switches
            .generate_default
            .as_deref()
            .filter(|format| !format.is_empty())
            .unwrap_or(agname::DEFAULT_FORMAT)
    } else {
        format
    };
    let archiving = matches!(
        command,
        Command::Add | Command::Update | Command::Freshen | Command::Move { .. }
    );
    let now = rar.zone.to_local(chrono::Utc::now());
    Some(agname::generated(archive, format, now, archiving, |name| {
        rar.path(&cmdline::with_default_extension(name)).is_file()
    }))
}

fn dispatch<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    command: &Command,
    parsed: &Parsed,
) -> Result<(), Stop> {
    match command {
        Command::List { verbose, form } => return list::run(rar, *verbose, *form, parsed),
        Command::Test
        | Command::Extract
        | Command::ExtractFull
        | Command::Print
        | Command::Find(_) => {
            return extract::run(rar, command, parsed);
        }
        Command::Add | Command::Update | Command::Freshen | Command::Move { .. } => {
            return add::run(rar, command, parsed);
        }
        Command::Delete => return delete::run(rar, parsed),
        Command::Repair => return repair::run(rar, parsed),
        Command::Reconstruct => {
            reconstruct::run(rar, parsed);
            return Ok(());
        }
        Command::Comment
        | Command::CommentWrite
        | Command::Rename
        | Command::Lock
        | Command::RecoveryRecord(_)
        | Command::Change => return modify::run(rar, command, parsed),
        Command::RecoveryVolumes(_) | Command::Sfx(_) => {}
    }
    let what = if matches!(command, Command::Sfx(_)) {
        "self-extracting archives"
    } else {
        "recovery volumes"
    };
    rar.console.err(&format!(
        "\n{}: cash's {} does not make {what}\n",
        rar.tool.name(),
        rar.tool.name()
    ));
    rar.fail(code::FATAL);
    Ok(())
}
