//! Windows-specific builtins — **D45**.
//!
//! Each justified by a decision rather than invented:
//!
//! | Builtin | Why it exists |
//! |---|---|
//! | `winpath` | D4 forbids cash rewriting arguments, so this is the deliberate escape hatch when a tool genuinely needs backslashes |
//! | `detach` | D6's escape hatch: start something meant to outlive the shell |
//! | `elevate` | So cash sees a UAC elevation rather than having it happen behind its back, and can register it for D42's tracking |
//! | `start` | The Windows `xdg-open` |
//! | `sudo` | A command elevated in this terminal, through gsudo or Windows' `sudo`, with cash choosing what runs |
//! | `su` | Unix's `su`: a shell elevated, or as another account, through what `sudo` uses |
//! | `sudoedit` | Unix's `sudoedit`: your editor on copies, the writing back elevated |

use std::io::Write;
use std::path::Path;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Which spelling `winpath` should produce.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    /// `C:/foo` — cash's canonical spelling (D3).
    Canonical,
    /// `C:\foo` — for a tool that genuinely requires backslashes.
    Windows,
    /// `/c/foo` — the Unix compatibility spelling.
    Unix,
}

/// Convert between path spellings.
///
/// D4 says cash never rewrites arguments on its own — no guessing which ones are paths,
/// because that is where MSYS2 needed `MSYS2_ARG_CONV_EXCL`. When a DOS-lineage tool
/// really does need backslashes, you convert deliberately:
///
/// ```text
/// some-old-tool.exe "$(winpath -w "$dir")"
/// ```
#[derive(Parser)]
pub(crate) struct WinPathCommand {
    /// Output Windows form with backslashes: `C:\foo`.
    #[arg(short = 'w', overrides_with_all = ["unix", "canonical"])]
    windows: bool,

    /// Output Unix compatibility form: `/c/foo`.
    #[arg(short = 'u', overrides_with_all = ["windows", "canonical"])]
    unix: bool,

    /// Output cash's canonical form: `C:/foo`. The default.
    #[arg(short = 'c', overrides_with_all = ["windows", "unix"])]
    canonical: bool,

    /// Paths to convert. With none, paths are read from standard input, one per line.
    paths: Vec<String>,
}

impl WinPathCommand {
    const fn form(&self) -> Form {
        if self.windows {
            Form::Windows
        } else if self.unix {
            Form::Unix
        } else {
            Form::Canonical
        }
    }

    fn convert(form: Form, input: &str) -> String {
        let accepted = cash_win32::path::accept_path(input);
        match form {
            Form::Canonical => cash_win32::path::render(&accepted),
            Form::Windows => cash_win32::path::to_backslash(&accepted),
            Form::Unix => cash_win32::path::to_unix(&accepted),
        }
    }
}

impl builtins::Command for WinPathCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let form = self.form();
        let mut stdout = context.stdout();

        if self.paths.is_empty() {
            // Reading from stdin makes `winpath` composable: `find ... | winpath -w`.
            let mut buffer = String::new();
            std::io::Read::read_to_string(&mut context.stdin(), &mut buffer)?;
            for line in cash_win32::text::split_lines(&buffer) {
                writeln!(stdout, "{}", Self::convert(form, line))?;
            }
        } else {
            for path in &self.paths {
                writeln!(stdout, "{}", Self::convert(form, path))?;
            }
        }

        Ok(ExecutionResult::success())
    }
}

/// Open a file or URL with its default handler — the Windows `xdg-open`.
#[derive(Parser)]
pub(crate) struct StartCommand {
    /// The file, directory or URL to open.
    target: String,
}

/// Opens `target` with its default program, as `start` and `xdg-open` do.
///
/// A file or folder that exists is resolved against the shell's working directory, not
/// the process's (cash never changes its own), in every spelling D3 accepts. Anything
/// else — a URL, `mailto:`, `ms-settings:`, a program's name — goes to the handler as
/// written. No command processor sees it, so `&` and `%` mean nothing.
pub(crate) fn open_with_default_program(
    shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
    target: &str,
) -> std::io::Result<()> {
    let path = shell.absolute_path(Path::new(target));
    let target = if path.exists() {
        cash_win32::path::to_backslash(&path)
    } else {
        target.to_owned()
    };
    cash_win32::shellopen::open(&target, shell.working_dir())
}

impl builtins::Command for StartCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        match open_with_default_program(context.shell, &self.target) {
            Ok(()) => Ok(ExecutionResult::success()),
            Err(e) => {
                writeln!(context.stderr(), "start: {}: {e}", self.target)?;
                Ok(ExecutionResult::new(1))
            }
        }
    }
}

/// This cash's own exe, which runs what has no program of its own.
fn own_exe() -> String {
    std::env::current_exe().map_or_else(
        |_| "cash.exe".to_owned(),
        |exe| exe.to_string_lossy().into_owned(),
    )
}

/// Each argument quoted as the Microsoft C runtime parses it, joined by spaces.
fn quoted_arguments(args: &[String]) -> String {
    args.iter()
        .map(|arg| cash_win32::cmd::quote_argument(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Run a command elevated, via UAC.
///
/// A first-class verb rather than shelling out to an external helper, so cash knows the
/// elevation happened. D42 records why that matters: an elevated child cannot be
/// assigned to cash's job object — a medium-integrity process cannot acquire
/// `PROCESS_SET_QUOTA` on a high-integrity one — so D6's containment guarantee stops at
/// the integrity boundary, and cash should say so rather than imply otherwise.
///
/// The command starts in the shell's folder, found on the shell's `PATH`, with its
/// arguments as written. It used to go through PowerShell's `Start-Process`, which
/// started it in the folder cash was started in and split an argument with a space
/// (BI-19). It starts from the user's own environment: UAC takes none, so the shell's
/// exported variables do not follow it.
#[derive(Parser)]
pub(crate) struct ElevateCommand {
    /// Suppress the warning that the elevated process escapes cash's containment.
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// The command and its arguments.
    #[arg(trailing_var_arg = true, required = true)]
    command: Vec<String>,
}

impl builtins::Command for ElevateCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let (program, args) = self
            .command
            .split_first()
            .ok_or_else(|| cash_core::Error::from(std::io::Error::other("elevate: no command")))?;

        if !self.quiet {
            writeln!(
                context.stderr(),
                "elevate: the elevated process runs outside cash's job object and will \
                 not be reaped when cash exits (see D42); it is not waited for, so the \
                 status says only that it started, not how it ended"
            )?;
        }

        let (target, parameters) = elevation_request(context.shell, program, args);
        match cash_win32::shellopen::run_elevated(&target, &parameters, context.shell.working_dir())
        {
            Ok(()) => Ok(ExecutionResult::success()),
            Err(e) => {
                writeln!(
                    context.stderr(),
                    "elevate: {program}: {}",
                    cash_core::error::os_error_text(&e)
                )?;
                Ok(ExecutionResult::new(1))
            }
        }
    }
}

/// The program `elevate` asks UAC to start, and the rest of its command line, quoted.
///
/// What runs is what `sudo` runs ([`launch_words`]), so `elevate tool "a b"` hands
/// `tool` one argument, where PowerShell's `-ArgumentList` split it (BI-19). A name found
/// nowhere is left for Windows to find, as `App Paths` registers some programs only there.
fn elevation_request(
    shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
    program: &str,
    args: &[String],
) -> (String, String) {
    match launch_words(shell, program, args, &own_exe()) {
        Some(words) => match words.split_first() {
            Some((first, rest)) => (first.clone(), quoted_arguments(rest)),
            None => (program.to_string(), quoted_arguments(args)),
        },
        None => (program.to_string(), quoted_arguments(args)),
    }
}

/// Start a command that is meant to outlive the shell.
///
/// D6's deliberate escape hatch. Everything cash starts is normally reaped when cash
/// exits; `detach` is how you say that this one should not be.
///
/// D45 records the cost: for a child to leave the session job, that job must be created
/// with `JOB_OBJECT_LIMIT_BREAKAWAY_OK`, which means *any* child can then request
/// breakaway. Having this builtin therefore weakens D6's guarantee slightly for
/// everything — accepted as the price of an explicit escape hatch.
#[derive(Parser)]
pub(crate) struct DetachCommand {
    /// The command and its arguments.
    #[arg(trailing_var_arg = true, required = true)]
    command: Vec<String>,
}

impl builtins::Command for DetachCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let (program, args) = self
            .command
            .split_first()
            .ok_or_else(|| cash_core::Error::from(std::io::Error::other("detach: no command")))?;

        // What runs is what `sudo` runs (`launch_words`).
        let Some(words) = launch_words(context.shell, program, args, &own_exe()) else {
            writeln!(context.stderr(), "detach: {program}: command not found")?;
            return Ok(ExecutionResult::new(127));
        };

        // Started in the shell's working directory, with its exported environment (D5),
        // out of the session job, with no console and holding no handle of cash's.
        let command_line = quoted_arguments(&words);
        let mut env = cash_core::commands::exported_environment(context.shell);
        // What runs in a cash this starts says so: in a linked tool's process cash's exe
        // is the link, and the child would take itself for the tool (BIN-09).
        let program = words.first().cloned().unwrap_or_default();
        if std::env::current_exe()
            .is_ok_and(|own| own.to_string_lossy().eq_ignore_ascii_case(&program))
        {
            env.push((
                cash_core::commands::ARGV0_VARIABLE.to_owned(),
                "cash".to_owned(),
            ));
        }
        match cash_win32::spawn::spawn_detached(&command_line, context.shell.working_dir(), &env) {
            Ok(pid) => {
                // cash keeps no handle to it: it neither waits for the program nor ends it.
                writeln!(context.stdout(), "[detached] pid {pid}")?;
                Ok(ExecutionResult::success())
            }
            Err(e) => {
                writeln!(context.stderr(), "detach: {program}: {e}")?;
                Ok(ExecutionResult::new(126))
            }
        }
    }
}

/// The options `sudo --help` and `help sudo` list; `sudo` reads its words itself, so clap
/// knows none of them.
const SUDO_HELP: &str = "\
Options:
  -i, --login             a login shell in the account's home folder, or COMMAND run by one
  -s, --shell             a shell in this folder, or COMMAND run by one
  -u, --user USER         as USER, at that account's usual level (gsudo asks the password);
                          `-u root` is elevation
  -E, --preserve-env      the shell's exported variables go with the command
  -n, --non-interactive   fail at once when approval would be asked for
  -e, --edit FILE...      edit files as `sudoedit` does
  -l, --list              who you are, and how sudo elevates here
  -v, --validate          open gsudo's credentials cache: commands then run without asking
  -k, --reset-timestamp   close gsudo's cache; with COMMAND, then run it
  -K, --remove-timestamp  close gsudo's cache
  -h, --help              this help
  NAME=value              a variable for the command

cash elevates nothing itself: gsudo does, where it is installed, else Windows' own sudo,
else UAC in a new window. What runs is what the shell would run: `sudo ls` is cash's `ls`,
`sudo bash` is cash, a script runs through cash. A file an elevated command creates is
yours when you approve with your own account. In a shell already elevated, the command
runs here.

Examples:
  sudo winget upgrade --all
  sudo -i                   an elevated login shell
  sudo -u alice whoami
  sudo -E make install
  echo '127.0.0.1 dev' | sudo tee -a C:/Windows/System32/drivers/etc/hosts
  sudo -e C:/Windows/System32/drivers/etc/hosts";

/// Run a command elevated, or as another user, in this terminal, as a Unix `sudo` does.
///
/// cash elevates nothing itself: gsudo does it where it is installed, else Windows' own
/// `sudo`. What cash does is choose what runs, as the shell would: `sudo bash` is cash,
/// where `sudo.exe` looked `bash` up itself and found WSL's; `sudo ls` is cash's `ls`,
/// run by an elevated cash, where there is no `ls.exe` to run; a batch file or a script
/// goes through cash too. A function is not run, as a Unix `sudo` runs none. In a shell
/// already elevated, the command runs here.
#[derive(Parser)]
#[command(
    disable_help_flag = true,
    override_usage = "sudo [-E] [-n] [-u USER] [-i | -s] [NAME=value]... [COMMAND [ARG]...]\n       \
                      sudo -e FILE...\n       sudo -l | -v | -k | -K",
    after_help = SUDO_HELP
)]
pub(crate) struct SudoCommand {
    /// The options, then the command and its arguments.
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "ARG"
    )]
    command: Vec<String>,
}

/// What `sudo`'s options ask for.
#[derive(Debug, Default, PartialEq, Eq)]
struct SudoOptions {
    /// `-i`: a login shell in the account's home.
    login: bool,
    /// `-s`: a shell in this folder.
    shell: bool,
    /// `-u USER`.
    user: Option<String>,
    /// `-E`: the exported variables go with the command.
    preserve_env: bool,
    /// `-n`: never ask.
    non_interactive: bool,
    /// `-v`: open the credentials cache.
    validate: bool,
    /// `-k`: close it.
    reset: bool,
    /// `-K`: close it, with no command.
    remove: bool,
    /// `-l`: say how sudo works here.
    list: bool,
    /// `-e`: edit files.
    edit: bool,
    /// `-h`.
    help: bool,
}

/// `sudo`'s options at the start of `words`, and the index of the first word after them
/// (a variable or the command); an error names an option it does not know or one
/// without its value. Short options combine (`-nE`), and `-u` takes the rest of its word
/// or the next one.
fn parse_sudo(words: &[String]) -> Result<(SudoOptions, usize), String> {
    let mut options = SudoOptions::default();
    let mut index = 0;
    while let Some(word) = words.get(index) {
        index += 1;
        if word == "--" {
            return Ok((options, index));
        }
        if let Some(long) = word.strip_prefix("--") {
            let (name, value) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value.to_owned())),
                None => (long, None),
            };
            let flag = match name {
                "login" => &mut options.login,
                "shell" => &mut options.shell,
                "preserve-env" => &mut options.preserve_env,
                "non-interactive" => &mut options.non_interactive,
                "validate" => &mut options.validate,
                "reset-timestamp" => &mut options.reset,
                "remove-timestamp" => &mut options.remove,
                "list" => &mut options.list,
                "edit" => &mut options.edit,
                "help" => &mut options.help,
                "user" => {
                    options.user = Some(option_value(
                        value,
                        words,
                        &mut index,
                        "--user: a user is needed",
                    )?);
                    continue;
                }
                _ => return Err(format!("{word}: unknown option; `sudo --help` lists them")),
            };
            if value.is_some() {
                return Err(format!("--{name}: takes no value"));
            }
            *flag = true;
            continue;
        }
        let Some(cluster) = word.strip_prefix('-').filter(|cluster| !cluster.is_empty()) else {
            return Ok((options, index - 1));
        };
        for (at, letter) in cluster.char_indices() {
            let flag = match letter {
                'i' => &mut options.login,
                's' => &mut options.shell,
                'E' => &mut options.preserve_env,
                'n' => &mut options.non_interactive,
                'v' => &mut options.validate,
                'k' => &mut options.reset,
                'K' => &mut options.remove,
                'l' => &mut options.list,
                'e' => &mut options.edit,
                'h' => &mut options.help,
                'u' => {
                    let rest = cluster.get(at + 1..).filter(|rest| !rest.is_empty());
                    options.user = Some(option_value(
                        rest.map(str::to_owned),
                        words,
                        &mut index,
                        "-u: a user is needed",
                    )?);
                    break;
                }
                _ => {
                    return Err(format!(
                        "-{letter}: unknown option; `sudo --help` lists them"
                    ));
                }
            };
            *flag = true;
        }
    }
    Ok((options, index))
}

/// An option's value: the one in its own word (`--user=NAME`, `-uNAME`), else the next
/// word, which `index` then moves past; `missing` when there is none.
fn option_value(
    inline: Option<String>,
    words: &[String],
    index: &mut usize,
    missing: &str,
) -> Result<String, String> {
    if let Some(value) = inline {
        return Ok(value);
    }
    let value = words
        .get(*index)
        .cloned()
        .ok_or_else(|| missing.to_owned())?;
    *index += 1;
    Ok(value)
}

/// The help `--help` prints for the builtin `C`, as `help NAME` shows it.
fn help_text<C: builtins::Command>(name: &str) -> String {
    C::get_content(
        name,
        builtins::ContentType::DetailedHelp,
        &builtins::ContentOptions::default(),
    )
    .unwrap_or_default()
}

impl builtins::Command for SudoCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let (options, start) = match parse_sudo(&self.command) {
            Ok(parsed) => parsed,
            Err(message) => {
                writeln!(context.stderr(), "sudo: {message}")?;
                return Ok(ExecutionResult::new(1));
            }
        };
        if options.help {
            write!(context.stdout(), "{}", help_text::<Self>("sudo"))?;
            return Ok(ExecutionResult::success());
        }
        let words = self.command.get(start..).unwrap_or_default();
        // `root` is the administrator, as for `su`: elevation, not an account.
        let user = options
            .user
            .as_deref()
            .filter(|name| !name.eq_ignore_ascii_case("root"));
        let elevated =
            user.is_none() && cash_win32::process::current_process_is_elevated() == Some(true);

        if let Some(result) = sudo_itself(&context, &options, words, elevated)? {
            return Ok(result);
        }
        if options.edit {
            return edit_files(&context, words, user, options.non_interactive, "sudo");
        }

        // `sudo NAME=value COMMAND`: the variables are the command's, as a Unix sudo passes
        // them; they were taken for the command's name.
        let assignment_count = words.iter().take_while(|word| is_assignment(word)).count();
        let (assignments, words) = words.split_at(assignment_count);
        if words.is_empty() && !(options.login || options.shell) {
            writeln!(
                context.stderr(),
                "usage: sudo [-E] [-n] [-u USER] [-i | -s] [NAME=value]... [COMMAND [ARG]...]"
            )?;
            return Ok(ExecutionResult::new(1));
        }

        let cash = own_exe();
        if elevated {
            if options.login {
                let target = login_words(&cash, words);
                let target = if assignments.is_empty() {
                    target
                } else {
                    with_assignments(&cash, assignments, &target)
                };
                return run_program(&context, &cash, &cash, target.get(1..).unwrap_or(&[]));
            }
            if words.is_empty() {
                return run_program(&context, &cash, &cash, &[]);
            }
            if !assignments.is_empty() {
                let wrapped = with_assignments(&cash, assignments, words);
                return run_program(&context, &cash, &cash, wrapped.get(1..).unwrap_or(&[]));
            }
            let command = crate::command::CommandCommand {
                command_and_args: words.to_vec(),
                ..Default::default()
            };
            return command.execute(context).await;
        }
        if options.non_interactive && !runs_without_asking(context.shell, user) {
            writeln!(context.stderr(), "sudo: a password is required")?;
            return Ok(ExecutionResult::new(1));
        }

        // What runs elevated, as the shell would run it.
        let target: Vec<String> = if options.login {
            login_words(&cash, words)
        } else {
            match words.split_first() {
                None => vec![cash.clone()],
                Some((name, args)) => {
                    let Some(target) = launch_words(context.shell, name, args, &cash) else {
                        writeln!(context.stderr(), "sudo: {name}: command not found")?;
                        return Ok(ExecutionResult::new(1));
                    };
                    target
                }
            }
        };
        let target = if assignments.is_empty() {
            target
        } else {
            with_assignments(&cash, assignments, &target)
        };

        run_as(
            &context,
            &target,
            &Elevation {
                user,
                preserve_env: options.preserve_env,
                who: "sudo",
            },
        )
    }
}

/// What `sudo` does for itself rather than for a command: `-l`, `-k` and `-K`, `-v`. `None`
/// when a command is still to run.
fn sudo_itself<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    options: &SudoOptions,
    words: &[String],
    elevated: bool,
) -> Result<Option<ExecutionResult>, cash_core::Error> {
    if options.list {
        return list(context).map(Some);
    }
    if options.remove && !words.is_empty() {
        writeln!(context.stderr(), "sudo: -K takes no command")?;
        return Ok(Some(ExecutionResult::new(1)));
    }
    if options.remove || options.reset {
        reset_cache(context.shell);
        if words.is_empty() && !(options.validate || options.login || options.shell) {
            return Ok(Some(ExecutionResult::success()));
        }
    }
    if options.validate {
        if !words.is_empty() {
            writeln!(context.stderr(), "sudo: -v takes no command")?;
            return Ok(Some(ExecutionResult::new(1)));
        }
        return validate(context, elevated).map(Some);
    }
    Ok(None)
}

/// A login shell started in the account's home, running `command` when there is one, as
/// `sudo -i` and `su -` give it. Only the shell started as that account knows its home:
/// it goes there and then becomes the login shell.
fn login_words(cash: &str, command: &[String]) -> Vec<String> {
    let script = if command.is_empty() {
        r#"cd ~ || exit; exec "$0" -l"#
    } else {
        r#"cd ~ || exit; exec "$0" -l -c '"$0" "$@"' "$@""#
    };
    let mut all = vec![
        cash.to_owned(),
        "-c".to_owned(),
        script.to_owned(),
        cash.to_owned(),
    ];
    all.extend(command.iter().cloned());
    all
}

/// Whether a command can be elevated without anyone being asked, for `sudo -n`: gsudo's
/// credentials cache is open for this shell. Another account always asks its password.
fn runs_without_asking(
    shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
    user: Option<&str>,
) -> bool {
    user.is_none()
        && shell
            .resolve_command_in_path("gsudo")
            .is_some_and(|gsudo| cash_win32::gsudo::cache_open(&gsudo) == Some(true))
}

/// `sudo -k`: gsudo's credentials cache closed, every session of it; without gsudo there
/// is none, and nothing to do.
fn reset_cache(shell: &cash_core::Shell<impl cash_core::ShellExtensions>) {
    if let Some(gsudo) = shell.resolve_command_in_path("gsudo") {
        cash_win32::gsudo::reset(&gsudo);
    }
}

/// `sudo -v`: gsudo's credentials cache opened for this shell, which asks for approval
/// once. Windows' sudo and UAC keep none.
fn validate<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    elevated: bool,
) -> Result<ExecutionResult, cash_core::Error> {
    if elevated {
        return Ok(ExecutionResult::success());
    }
    if let Some(gsudo) = context.shell.resolve_command_in_path("gsudo") {
        let gsudo = gsudo.to_string_lossy().into_owned();
        return run_program(
            context,
            &gsudo,
            "gsudo",
            &["cache".to_owned(), "on".to_owned()],
        );
    }
    writeln!(
        context.stderr(),
        "sudo: without gsudo there is no credentials cache: Windows asks every time \
         (`scoop install gsudo` has one)"
    )?;
    Ok(ExecutionResult::success())
}

/// `sudo -l`: who you are, whether you are an administrator, what elevates, gsudo's
/// cache, and whether other accounts can run this cash.
fn list<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Result<ExecutionResult, cash_core::Error> {
    use cash_win32::account;

    let mut out = context.stdout();
    let name = account::current_user()
        .as_ref()
        .and_then(account::account_name)
        .unwrap_or_else(cash_win32::fs::current_user);
    writeln!(out, "User:          {name}")?;
    let elevated = cash_win32::process::current_process_is_elevated() == Some(true);
    let admin = match account::is_administrator() {
        Some(true) if elevated => "yes, and this shell is elevated: sudo runs commands here",
        Some(true) => "yes, a member of Administrators; this shell is not elevated",
        Some(false) => {
            "no: UAC asks an administrator to approve, and a command runs as the \
             administrator who does, with their files and ~"
        }
        None => "unknown",
    };
    writeln!(out, "Administrator: {admin}")?;
    let route = route(context.shell, false);
    writeln!(out, "Elevates with: {}", route.describe())?;
    if let Route::Gsudo(gsudo) = &route {
        let cache = match cash_win32::gsudo::cache_open(Path::new(gsudo)) {
            Some(true) => "open: commands run without asking (`sudo -k` closes it)",
            Some(false) => "closed (`sudo -v` opens it)",
            None => "unknown",
        };
        writeln!(out, "gsudo cache:   {cache}")?;
    }
    let cash = own_exe();
    let shared = match account::may_execute(Path::new(&cash), None) {
        Some(true) => "runs for every account: `su USER` and `sudo -u USER` can start it",
        Some(false) => {
            "readable by your account and administrators only: `su USER` and `sudo -u \
             USER` cannot start it (`scoop install -g cash`, or an install under Program \
             Files, makes it readable to all accounts)"
        }
        None => "its access list could not be read",
    };
    writeln!(
        out,
        "cash.exe:      {} {shared}",
        cash_win32::path::render(Path::new(&cash))
    )?;
    Ok(ExecutionResult::success())
}

/// What elevates a command here, or runs it as another account.
enum Route {
    /// gsudo, at this path: in this terminal, waited for.
    Gsudo(String),
    /// Windows' own sudo, at this path, in this mode.
    WindowsSudo(String, cash_win32::sysinfo::WindowsSudo),
    /// `runas`, at this path: another account, in a new window, not waited for.
    Runas(String),
    /// UAC's own request, as `elevate` makes it: a new window, not waited for.
    Uac,
}

impl Route {
    /// Whether the command's end, and its status, come back to the shell.
    const fn waits(&self) -> bool {
        match self {
            Self::Gsudo(_) => true,
            Self::WindowsSudo(_, mode) => {
                !matches!(mode, cash_win32::sysinfo::WindowsSudo::NewWindow)
            }
            Self::Runas(_) | Self::Uac => false,
        }
    }

    /// What `sudo -l` says of it.
    fn describe(&self) -> String {
        use cash_win32::sysinfo::WindowsSudo;
        let render = |path: &str| cash_win32::path::render(Path::new(path));
        match self {
            Self::Gsudo(path) => format!("gsudo ({}), in this terminal", render(path)),
            Self::WindowsSudo(path, mode) => format!(
                "Windows' sudo ({}), {}",
                render(path),
                match mode {
                    WindowsSudo::Inline | WindowsSudo::Off => "in this terminal",
                    WindowsSudo::InputClosed => "in this terminal, with its input closed",
                    WindowsSudo::NewWindow => "in a new window, not waited for",
                }
            ),
            Self::Runas(path) => format!("runas ({}), in a new window", render(path)),
            Self::Uac => "UAC, in a new window, not waited for (no gsudo or Windows sudo)".into(),
        }
    }
}

/// What runs `sudo`'s commands here: gsudo where it is installed, else, as a user, `runas`,
/// else Windows' own sudo where it is on, else UAC's request.
fn route(shell: &cash_core::Shell<impl cash_core::ShellExtensions>, as_user: bool) -> Route {
    if let Some(gsudo) = shell.resolve_command_in_path("gsudo") {
        return Route::Gsudo(gsudo.to_string_lossy().into_owned());
    }
    let system32 = shell.env_str("SystemRoot").map_or_else(
        || r"C:\Windows\System32".to_owned(),
        |root| format!(r"{root}\System32"),
    );
    if as_user {
        return Route::Runas(format!(r"{system32}\runas.exe"));
    }
    match cash_win32::sysinfo::windows_sudo() {
        cash_win32::sysinfo::WindowsSudo::Off => Route::Uac,
        mode => Route::WindowsSudo(format!(r"{system32}\sudo.exe"), mode),
    }
}

/// How [`run_as`] runs a command.
struct Elevation<'a> {
    /// The account, or `None` for this one elevated.
    user: Option<&'a str>,
    /// Whether the shell's exported variables go with it (`sudo -E`, `su -m`).
    preserve_env: bool,
    /// The builtin, for messages.
    who: &'a str,
}

/// The UNC form of `dir` when it is on a mapped network drive, which an elevated process
/// does not have: drive letters are mapped per logon.
fn mapped_drive_unc(dir: &Path) -> Option<String> {
    let text = dir.to_string_lossy();
    let on_drive = text.as_bytes().get(1) == Some(&b':');
    if on_drive && cash_win32::path::is_on_network(dir) {
        cash_win32::path::universal_name(dir)
    } else {
        None
    }
}

/// The shell's exported variables as `NAME=value` words, for a cash that exports them
/// first ([`with_assignments`]); a name a shell cannot hold (`ProgramFiles(x86)`) stays
/// behind.
fn exported_assignments(shell: &cash_core::Shell<impl cash_core::ShellExtensions>) -> Vec<String> {
    cash_core::commands::exported_environment(shell)
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .filter(|word| is_assignment(word))
        .collect()
}

/// Whether `user` is an account that can start `target`'s program, saying why not when
/// it is not.
///
/// A program under this user's profile (Scoop's `~/scoop`) is not the other account's to
/// read; Windows would say only that access is denied, after the password. Copying cash
/// somewhere shared is not the answer: a folder another account can write to first is a
/// way to run its program as you.
fn may_run_as<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    target: &[String],
    user: &str,
    who: &str,
) -> Result<bool, cash_core::Error> {
    let Some(sid) = cash_win32::account::lookup_user(user) else {
        writeln!(context.stderr(), "{who}: unknown user {user}")?;
        return Ok(false);
    };
    let Some(program) = target.first() else {
        return Ok(true);
    };
    if cash_win32::account::may_execute(Path::new(program), Some(&sid)) != Some(false) {
        return Ok(true);
    }
    let fix = if cash_core::commands::is_own_executable(std::ffi::OsStr::new(program)) {
        "`scoop install -g cash`, or an install under Program Files, makes cash readable to \
         all accounts"
    } else {
        "a copy under Program Files is readable to all accounts"
    };
    writeln!(
        context.stderr(),
        "{who}: {} is not readable by {user}, so it cannot run as {user}; {fix}",
        cash_win32::path::render(Path::new(program))
    )?;
    Ok(false)
}

/// `target` asked of UAC, as `elevate` asks it, in `directory`: a new window, not waited
/// for.
fn ask_uac<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    target: &[String],
    directory: &Path,
    who: &str,
) -> Result<ExecutionResult, cash_core::Error> {
    writeln!(
        context.stderr(),
        "{who}: no gsudo or Windows sudo, so UAC starts it in a new window and does not \
         wait: the status says only that it started, not how it ended (`scoop install \
         gsudo` keeps it here)"
    )?;
    let Some((program, args)) = target.split_first() else {
        return Ok(ExecutionResult::new(1));
    };
    match cash_win32::shellopen::run_elevated(program, &quoted_arguments(args), directory) {
        Ok(()) => Ok(ExecutionResult::success()),
        Err(e) => {
            writeln!(
                context.stderr(),
                "{who}: {}",
                cash_core::error::os_error_text(&e)
            )?;
            Ok(ExecutionResult::new(1))
        }
    }
}

/// Run `target` elevated, or as a user, and give its status, as [`route`] chooses: gsudo,
/// else Windows' own `sudo` (elevated) or `runas` (as a user), else UAC's request.
///
/// gsudo keeps the command in this terminal; Windows' sudo does in its inline mode only,
/// and runas never. `-d` keeps gsudo from looking at its parent, cash, to decide which
/// shell should run the command: the command is a program, run as it is. As a user, the
/// account's own token at its usual level (`-i Medium`), as Unix's `su` and `sudo -u`
/// give that user's rights, not an administrator's; that account must be able to read
/// the program. Elevated, the command runs under `cash --invoke-bundled --sudo-owner`,
/// which makes the files it creates the user's when the user approved with their own
/// account, and says so when another account approved.
fn run_as<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    target: &[String],
    how: &Elevation<'_>,
) -> Result<ExecutionResult, cash_core::Error> {
    let who = how.who;
    let cash = own_exe();
    if let Some(user) = how.user
        && !may_run_as(context, target, user, who)?
    {
        return Ok(ExecutionResult::new(1));
    }

    let route = route(context.shell, how.user.is_some());
    let mut target = target.to_vec();
    // Windows' sudo passes the environment on itself (`-E`); gsudo does in its usual mode
    // only, not when another account approves, and UAC's request takes none.
    if how.preserve_env && !matches!(route, Route::WindowsSudo(..)) {
        target = with_assignments(&cash, &exported_assignments(context.shell), &target);
    }
    if how.user.is_none()
        && let Some(me) = cash_win32::account::current_user()
    {
        let mut wrapped = vec![
            cash,
            "--invoke-bundled".to_owned(),
            "--sudo-owner".to_owned(),
            me.text(),
        ];
        wrapped.extend(target);
        target = wrapped;
    }
    // Windows' limit on a command line; the environment `-E` passes is what reaches it.
    if quoted_arguments(&target).encode_utf16().count() > 32_000 {
        writeln!(
            context.stderr(),
            "{who}: the command line is too long for Windows (over 32,000 characters)"
        )?;
        return Ok(ExecutionResult::new(1));
    }

    let dir = context.shell.working_dir().to_path_buf();
    let unc = mapped_drive_unc(&dir);
    match &route {
        Route::Gsudo(gsudo) => {
            let mut args = vec!["-d".to_owned()];
            // The elevated token has none of the user's drive letters; gsudo maps them.
            if unc.is_some() {
                args.push("--copyns".to_owned());
            }
            if let Some(user) = how.user {
                args.extend(["-u", user, "-i", "Medium"].map(String::from));
            }
            args.extend(target);
            run_program(context, gsudo, "gsudo", &args)
        }
        Route::Runas(runas) => {
            let user = how.user.unwrap_or_default();
            writeln!(
                context.stderr(),
                "{who}: without gsudo, runas starts it as {user} in a new window, and it is \
                 not waited for (`scoop install gsudo` keeps it here)"
            )?;
            let args = [format!("/user:{user}"), quoted_arguments(&target)];
            run_program(context, runas, "runas", &args)
        }
        // UAC itself, which works on every Windows but starts the command in a new window
        // and does not wait for it.
        Route::Uac => ask_uac(
            context,
            &target,
            &unc.map_or(dir, std::path::PathBuf::from),
            who,
        ),
        Route::WindowsSudo(sudo_exe, _) => {
            if !route.waits() {
                writeln!(
                    context.stderr(),
                    "{who}: Windows' sudo is set to open a new window, where the output stays, \
                     and does not wait: the status says only whether it started, not how it \
                     ended; `sudo config --enable normal` in an elevated shell keeps it here"
                )?;
            }
            // In its new-window mode Windows' sudo starts a command in System32 unless told
            // otherwise; an elevated process sees no mapped drive, so one is given by its
            // share.
            let chdir = if let Some(unc) = unc {
                writeln!(
                    context.stderr(),
                    "{who}: this folder is on a network drive, which the elevated command \
                     reaches as {unc}"
                )?;
                unc
            } else {
                let dir = cash_win32::path::process_directory(&dir).unwrap_or(dir);
                cash_win32::path::to_backslash(&dir)
            };
            let mut args = vec!["--chdir".to_owned(), chdir];
            if how.preserve_env {
                args.push("-E".to_owned());
            }
            args.extend(target);
            run_program(context, sudo_exe, "sudo", &args)
        }
    }
}

/// What `sudoedit --help` lists.
const SUDOEDIT_HELP: &str = "\
Options:
  -u, --user USER         write the files as USER
  -n, --non-interactive   fail at once when approval would be asked for
  -h, --help              this help

Each FILE is copied to your temporary folder and opened there in $SUDO_EDITOR, $VISUAL
or $EDITOR (the first that is set; notepad.exe without one), which runs as you, not
elevated. When the editor ends, each copy that changed is written back into its file by
an elevated cash, so the file keeps its access list and owner; a file that did not exist
is created. The copies are then removed.

Examples:
  sudoedit C:/Windows/System32/drivers/etc/hosts
  EDITOR='code --wait' sudoedit C:/ProgramData/ssh/sshd_config";

/// Edit files you may not write, as Unix's `sudoedit` does: your editor, unelevated, on
/// copies, and only the writing back elevated.
#[derive(Parser)]
#[command(
    disable_help_flag = true,
    override_usage = "sudoedit [-n] [-u USER] FILE...",
    after_help = SUDOEDIT_HELP
)]
pub(crate) struct SudoeditCommand {
    /// The options, then the files.
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "FILE"
    )]
    words: Vec<String>,
}

impl builtins::Command for SudoeditCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let (options, start) = match parse_sudo(&self.words) {
            Ok((options, _))
                if options.login
                    || options.shell
                    || options.preserve_env
                    || options.validate
                    || options.reset
                    || options.remove
                    || options.list
                    || options.edit =>
            {
                writeln!(
                    context.stderr(),
                    "sudoedit: only -u, -n and -h are options of sudoedit"
                )?;
                return Ok(ExecutionResult::new(1));
            }
            Ok(parsed) => parsed,
            Err(message) => {
                writeln!(context.stderr(), "sudoedit: {message}")?;
                return Ok(ExecutionResult::new(1));
            }
        };
        if options.help {
            write!(context.stdout(), "{}", help_text::<Self>("sudoedit"))?;
            return Ok(ExecutionResult::success());
        }
        let user = options
            .user
            .as_deref()
            .filter(|name| !name.eq_ignore_ascii_case("root"));
        let files = self.words.get(start..).unwrap_or_default();
        edit_files(&context, files, user, options.non_interactive, "sudoedit")
    }
}

/// A file `sudoedit` edits: where it is, the copy edited, and what it held.
struct Edited {
    /// The file.
    original: std::path::PathBuf,
    /// Its copy in the temporary folder.
    copy: std::path::PathBuf,
    /// What the file held; nothing for one that did not exist.
    before: Vec<u8>,
}

/// The editor `sudoedit` runs: `$SUDO_EDITOR`, `$VISUAL` or `$EDITOR`, the first that is
/// set, else Notepad.
fn editor(shell: &cash_core::Shell<impl cash_core::ShellExtensions>) -> String {
    ["SUDO_EDITOR", "VISUAL", "EDITOR"]
        .iter()
        .find_map(|name| {
            shell
                .env_str(name)
                .filter(|value| !value.trim().is_empty())
                .map(std::borrow::Cow::into_owned)
        })
        .unwrap_or_else(|| "notepad.exe".to_owned())
}

/// Removes the copies `sudoedit` made.
fn remove_copies(edits: &[Edited]) {
    for edit in edits {
        let _ = std::fs::remove_file(&edit.copy);
    }
}

/// A copy in `temp` of each of `files`, empty for one that does not exist yet; `None`, the
/// reason said and no copy left, when one cannot be read or made.
fn make_copies<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    files: &[String],
    temp: &Path,
    who: &str,
) -> Result<Option<Vec<Edited>>, cash_core::Error> {
    let mut edits: Vec<Edited> = Vec::new();
    for file in files {
        let original = context.shell.absolute_path(Path::new(file));
        let before = if original.is_dir() {
            Err(std::io::Error::from(std::io::ErrorKind::IsADirectory))
        } else {
            match std::fs::read(&original) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
                read => read,
            }
        };
        let made = before.and_then(|before| {
            edit_copy(temp, &original, &before).map(|copy| Edited {
                original,
                copy,
                before,
            })
        });
        match made {
            Ok(edit) => edits.push(edit),
            Err(e) => {
                writeln!(
                    context.stderr(),
                    "{who}: {file}: {}",
                    cash_core::error::os_error_text(&e)
                )?;
                remove_copies(&edits);
                return Ok(None);
            }
        }
    }
    Ok(Some(edits))
}

/// `sudoedit FILE...` and `sudo -e FILE...`: each file copied to the user's temporary
/// folder, the editor run on the copies as the shell runs a command (unelevated), and each
/// copy that changed written into its file by an elevated cash (`cat -- COPY > FILE`), so
/// the file keeps its access list and owner. The copies are removed; one whose writing
/// failed is kept, and named.
fn edit_files<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    files: &[String],
    user: Option<&str>,
    non_interactive: bool,
    who: &str,
) -> Result<ExecutionResult, cash_core::Error> {
    if files.is_empty() {
        writeln!(context.stderr(), "usage: sudoedit [-n] [-u USER] FILE...")?;
        return Ok(ExecutionResult::new(1));
    }
    let elevated =
        user.is_none() && cash_win32::process::current_process_is_elevated() == Some(true);
    if non_interactive && !elevated && !runs_without_asking(context.shell, user) {
        writeln!(context.stderr(), "{who}: a password is required")?;
        return Ok(ExecutionResult::new(1));
    }
    let editor = editor(context.shell);
    let Some(temp) = context
        .shell
        .env_str("TEMP")
        .or_else(|| context.shell.env_str("TMP"))
        .map(|dir| std::path::PathBuf::from(dir.as_ref()))
    else {
        writeln!(
            context.stderr(),
            "{who}: no temporary folder: TEMP is not set"
        )?;
        return Ok(ExecutionResult::new(1));
    };

    let Some(edits) = make_copies(context, files, &temp, who)? else {
        return Ok(ExecutionResult::new(1));
    };

    // The editor's words split as the shell splits them (`code --wait`), the copies as
    // its arguments.
    let cash = own_exe();
    let mut args = vec![
        "-c".to_owned(),
        format!("{editor} \"$@\""),
        "sudoedit".to_owned(),
    ];
    args.extend(
        edits
            .iter()
            .map(|edit| cash_win32::path::render(&edit.copy)),
    );
    let status = run_program(context, &cash, &cash, &args)?;
    if !status.is_success() {
        writeln!(
            context.stderr(),
            "{who}: the editor ({editor}) failed; no file was written"
        )?;
        remove_copies(&edits);
        return Ok(ExecutionResult::new(1));
    }

    let mut write_back = vec![
        cash.clone(),
        "-c".to_owned(),
        "status=0; while (($#)); do if cat -- \"$1\" > \"$2\"; then rm -f -- \"$1\"; \
         else status=1; fi; shift 2; done; exit $status"
            .to_owned(),
        "sudoedit".to_owned(),
    ];
    let mut changed = Vec::new();
    for edit in &edits {
        let after = std::fs::read(&edit.copy).unwrap_or_else(|_| edit.before.clone());
        if after == edit.before {
            let _ = std::fs::remove_file(&edit.copy);
            writeln!(
                context.stderr(),
                "{who}: {} unchanged",
                cash_win32::path::render(&edit.original)
            )?;
            continue;
        }
        write_back.push(cash_win32::path::render(&edit.copy));
        write_back.push(cash_win32::path::render(&edit.original));
        changed.push(edit);
    }
    if changed.is_empty() {
        return Ok(ExecutionResult::success());
    }

    let (result, waited) = if elevated {
        let result = run_program(context, &cash, &cash, write_back.get(1..).unwrap_or(&[]))?;
        (result, true)
    } else {
        let waited = route(context.shell, user.is_some()).waits();
        let result = run_as(
            context,
            &write_back,
            &Elevation {
                user,
                preserve_env: false,
                who,
            },
        )?;
        (result, waited)
    };
    // One not waited for may still be being written.
    if waited {
        name_kept_copies(context, &changed, who)?;
    }
    Ok(result)
}

/// Names each copy the elevated cash could not write into its file: it is kept, for the
/// work in it.
fn name_kept_copies<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    changed: &[&Edited],
    who: &str,
) -> Result<(), cash_core::Error> {
    for edit in changed.iter().filter(|edit| edit.copy.exists()) {
        writeln!(
            context.stderr(),
            "{who}: {} was not written; your changes are in {}",
            cash_win32::path::render(&edit.original),
            cash_win32::path::render(&edit.copy)
        )?;
    }
    Ok(())
}

/// A new file in `temp` holding `contents`, named after `original` (`hosts.sudoedit-1f2e`,
/// `sshd_config.sudoedit-1f2e`, `app.sudoedit-1f2e.json`) so the editor shows which file
/// it is and still knows its kind by its extension.
fn edit_copy(temp: &Path, original: &Path, contents: &[u8]) -> std::io::Result<std::path::PathBuf> {
    let stem = original.file_stem().map_or_else(
        || "file".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let extension = original
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos())
        ^ std::process::id().rotate_left(16);
    for attempt in 0..100u32 {
        let path = temp.join(format!(
            "{stem}.sudoedit-{:x}{extension}",
            seed.wrapping_add(attempt.wrapping_mul(0x9e37_79b9))
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(contents)?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("no free name for a copy"))
}

/// What `su --help` and `help su` list.
const SU_HELP: &str = "\
Options:
  -, -l, --login                    a login shell, started in the account's home folder
  -c, --command COMMAND             run COMMAND instead of a shell
  -s, --shell SHELL                 run SHELL instead of cash
  -m, -p, --preserve-environment    the shell's exported variables go with it
  -h, --help                        this help

Without USER, or as root, the shell is this account elevated, as `sudo -s` gives it:
Windows has no root account and no root password. With USER, it is that account, at its
usual level, whose password gsudo asks for; that account must be able to read cash.exe
(`scoop install -g cash` installs it where every account can). The shell is a new cash:
Windows raises no process that is already running.

Examples:
  su                    an elevated cash
  su -                  an elevated login shell, in your home folder
  su -c 'net stop spooler'
  su alice              a cash as alice
  su -s pwsh            an elevated PowerShell";

/// Start a shell, or run a command, as another user or elevated, as Unix's `su` does.
///
/// Without a user (or as `root`) the shell is this account elevated, as `sudo -i` gives
/// it: Windows has no root account and no root password. With one, it is that account,
/// whose password gsudo asks for. `-`, `-l` or `--login` make it a login shell started in
/// the account's home folder; `-c COMMAND` runs a command instead of a shell. It reaches
/// the same tools as `sudo`, and the same limits: an elevated shell or one as another
/// user is outside cash's job object (D42). The shell is a new cash; the one `su` runs in
/// stays as it is, as Windows raises no process that is already running.
#[derive(Parser)]
#[command(
    disable_help_flag = true,
    override_usage = "su [-|-l] [-m] [-s SHELL] [-c COMMAND] [USER [ARG]...]",
    after_help = SU_HELP
)]
pub(crate) struct SuCommand {
    /// The options, then the user and the shell's arguments.
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "ARG"
    )]
    words: Vec<String>,
}

/// What `su`'s options ask for.
#[derive(Debug, Default, PartialEq, Eq)]
struct SuOptions {
    /// `-`, `-l`: a login shell.
    login: bool,
    /// `-c COMMAND`.
    command: Option<String>,
    /// `-m`, `-p`: the exported variables go with it.
    preserve_env: bool,
    /// `-s SHELL`.
    shell: Option<String>,
    /// `-h`.
    help: bool,
}

/// `su`'s options at the start of `words`, and the index of the first word after them:
/// the user.
fn parse_su(words: &[String]) -> Result<(SuOptions, usize), String> {
    let mut options = SuOptions::default();
    let mut index = 0;
    while let Some(word) = words.get(index) {
        index += 1;
        match word.as_str() {
            "--" => return Ok((options, index)),
            "-" => {
                options.login = true;
                continue;
            }
            _ => {}
        }
        if let Some(long) = word.strip_prefix("--") {
            let (name, value) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value.to_owned())),
                None => (long, None),
            };
            let missing = format!("--{name}: a value is needed");
            match name {
                "login" => options.login = true,
                "preserve-environment" => options.preserve_env = true,
                "help" => options.help = true,
                "command" => {
                    options.command = Some(option_value(value, words, &mut index, &missing)?);
                }
                "shell" => {
                    options.shell = Some(option_value(value, words, &mut index, &missing)?);
                }
                _ => return Err(format!("{word}: unknown option; `su --help` lists them")),
            }
            continue;
        }
        let Some(cluster) = word.strip_prefix('-') else {
            return Ok((options, index - 1));
        };
        for (at, letter) in cluster.char_indices() {
            match letter {
                'l' => options.login = true,
                'm' | 'p' => options.preserve_env = true,
                'h' => options.help = true,
                'c' | 's' => {
                    let rest = cluster.get(at + 1..).filter(|rest| !rest.is_empty());
                    let value = option_value(
                        rest.map(str::to_owned),
                        words,
                        &mut index,
                        &format!("-{letter}: a value is needed"),
                    )?;
                    if letter == 'c' {
                        options.command = Some(value);
                    } else {
                        options.shell = Some(value);
                    }
                    break;
                }
                _ => return Err(format!("-{letter}: unknown option; `su --help` lists them")),
            }
        }
    }
    Ok((options, index))
}

impl builtins::Command for SuCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let (options, start) = match parse_su(&self.words) {
            Ok(parsed) => parsed,
            Err(message) => {
                writeln!(context.stderr(), "su: {message}")?;
                return Ok(ExecutionResult::new(1));
            }
        };
        if options.help {
            write!(context.stdout(), "{}", help_text::<Self>("su"))?;
            return Ok(ExecutionResult::success());
        }
        let words = self.words.get(start..).unwrap_or_default();
        let (user, shell_args) = match words.split_first() {
            Some((user, args)) => (
                Some(user.as_str()).filter(|user| !user.eq_ignore_ascii_case("root")),
                args,
            ),
            None => (None, words),
        };

        let cash = own_exe();
        // The shell: cash, or what `-s` names, found as the shell would run it.
        let shell = match &options.shell {
            None => vec![cash.clone()],
            Some(name) => {
                let Some(shell) = launch_words(context.shell, name, &[], &cash) else {
                    writeln!(context.stderr(), "su: {name}: command not found")?;
                    return Ok(ExecutionResult::new(1));
                };
                shell
            }
        };
        let is_cash = shell.first().is_some_and(|first| first == &cash);
        let mut args: Vec<String> = Vec::new();
        if let Some(command) = &options.command {
            args.extend(["-c".to_owned(), command.clone()]);
        }
        args.extend(shell_args.iter().cloned());
        // A login shell starts in the account's home, which only the shell started as that
        // account knows: it goes there and then becomes the shell, a login shell if cash.
        let target = if options.login {
            let mut all = vec![
                cash,
                "-c".to_owned(),
                "cd ~ || exit; exec \"$@\"".to_owned(),
                "su".to_owned(),
            ];
            all.extend(shell);
            if is_cash {
                all.push("-l".to_owned());
            }
            all.extend(args);
            all
        } else {
            let mut all = shell;
            all.extend(args);
            all
        };

        if user.is_none() && cash_win32::process::current_process_is_elevated() == Some(true) {
            let Some((program, rest)) = target.split_first() else {
                return Ok(ExecutionResult::new(1));
            };
            return run_program(&context, program, program, rest);
        }
        run_as(
            &context,
            &target,
            &Elevation {
                user,
                preserve_env: options.preserve_env,
                who: "su",
            },
        )
    }
}

/// Whether `word` is a `NAME=value` that `sudo` passes on as a variable.
fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// `command` run by a cash that exports `assignments` first, the command's words passed
/// as they are.
fn with_assignments(cash: &str, assignments: &[String], command: &[String]) -> Vec<String> {
    let mut all = vec![
        cash.to_owned(),
        "-c".to_owned(),
        "while [[ $1 == *=* ]]; do export -- \"$1\"; shift; done; \"$@\"".to_owned(),
        "sudo".to_owned(),
    ];
    all.extend(assignments.iter().cloned());
    all.extend(command.iter().cloned());
    all
}

/// The program and arguments that run `name` with `args` as the shell would, for `sudo`,
/// `su`, `elevate` and `detach`: cash for `bash`, `sh`
/// and `cash` (`sh` in POSIX mode, D7), for a builtin and for anything that is not a
/// program of its own; the program itself otherwise. `None` for a name found nowhere.
fn launch_words(
    shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
    name: &str,
    args: &[String],
    cash: &str,
) -> Option<Vec<String>> {
    let through_cash = |word: &str| {
        let mut all = vec![
            cash.to_owned(),
            "-c".to_owned(),
            "\"$0\" \"$@\"".to_owned(),
            word.to_owned(),
        ];
        all.extend(args.iter().cloned());
        all
    };
    let stem = Path::new(name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_ascii_lowercase());
    if !cash_core::sys::fs::contains_path_separator(name)
        && matches!(stem.as_deref(), Some("bash" | "sh" | "cash"))
    {
        let mut all = vec![cash.to_owned()];
        if stem.as_deref() == Some("sh") {
            all.push("--posix".to_owned());
        }
        all.extend(args.iter().cloned());
        return Some(all);
    }
    if !cash_core::sys::fs::contains_path_separator(name)
        && shell
            .builtins()
            .get(name)
            .is_some_and(|builtin| !builtin.disabled)
    {
        return Some(through_cash(name));
    }
    let resolved = if cash_core::sys::fs::contains_path_separator(name) {
        Some(shell.absolute_path(Path::new(name)))
    } else {
        shell.resolve_command_in_path(name)
    }
    .filter(|path| path.is_file())?;
    let program = cash_win32::path::to_backslash(&resolved);
    match cash_win32::resolve::classify(&resolved) {
        cash_win32::resolve::Dispatch::Native(_) => {
            let mut all = vec![program];
            all.extend(args.iter().cloned());
            Some(all)
        }
        _ => Some(through_cash(&program)),
    }
}

/// Run `program` with `args` as the shell runs a command (its folder, environment and
/// redirections), and give its status.
fn run_program<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    program: &str,
    name: &str,
    args: &[String],
) -> Result<ExecutionResult, cash_core::Error> {
    let mut command =
        cash_core::commands::compose_std_command(context, program, name, args, false)?;
    match command.status() {
        Ok(status) => {
            let code = status.code().unwrap_or(1);
            #[expect(
                clippy::cast_sign_loss,
                reason = "a Windows exit code is a DWORD, which `code` holds as its bits"
            )]
            let code = cash_win32::exit::from_windows(code as u32);
            Ok(ExecutionResult::new(code))
        }
        Err(e) => {
            writeln!(
                context.stderr(),
                "sudo: {name}: {}",
                cash_core::error::os_error_text(&e)
            )?;
            Ok(ExecutionResult::new(1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_reach_the_program_whole() {
        // PowerShell's `-ArgumentList` split `"a b"` in two on the way to an elevated
        // program (BI-19); the C runtime's quoting keeps each argument one.
        let args = ["a b", "it's", r#"say "hi""#, r"C:\my dir\", ""].map(String::from);
        assert_eq!(
            quoted_arguments(&args),
            r#""a b" it's "say \"hi\"" "C:\my dir\\" """#
        );
    }

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn sudo_options_combine_and_end_at_the_command() {
        let (options, start) = parse_sudo(&words("-nE -u alice FOO=1 ls -l")).unwrap();
        assert!(options.non_interactive && options.preserve_env);
        assert_eq!(options.user.as_deref(), Some("alice"));
        assert_eq!(start, 3);

        let (options, start) = parse_sudo(&words("-ubob --login -- -x")).unwrap();
        assert_eq!(options.user.as_deref(), Some("bob"));
        assert!(options.login);
        assert_eq!(start, 3);

        let (options, start) = parse_sudo(&words("--user=carol -e hosts")).unwrap();
        assert_eq!(
            (options.user.as_deref(), options.edit, start),
            (Some("carol"), true, 2)
        );

        let (options, _) = parse_sudo(&words("-kKvlsih")).unwrap();
        assert!(options.reset && options.remove && options.validate && options.list);
        assert!(options.shell && options.login && options.help);

        assert!(parse_sudo(&words("-u")).is_err());
        assert!(parse_sudo(&words("-x ls")).unwrap_err().contains("-x"));
        assert!(parse_sudo(&words("--list=1")).is_err());
    }

    #[test]
    fn su_options_take_their_values() {
        let (options, start) = parse_su(&words("- -c whoami alice")).unwrap();
        assert!(options.login);
        assert_eq!(options.command.as_deref(), Some("whoami"));
        assert_eq!(start, 3);

        let (options, start) = parse_su(&words("-lm -spwsh --command=dir")).unwrap();
        assert!(options.login && options.preserve_env);
        assert_eq!(options.shell.as_deref(), Some("pwsh"));
        assert_eq!(options.command.as_deref(), Some("dir"));
        assert_eq!(start, 3);

        assert!(parse_su(&words("-c")).is_err());
        assert!(parse_su(&words("-x")).unwrap_err().contains("-x"));
        assert!(parse_su(&words("--help")).unwrap().0.help);
    }

    #[test]
    fn a_login_shell_runs_its_command_by_its_words() {
        assert_eq!(
            login_words("cash.exe", &words("ls -l")),
            [
                "cash.exe",
                "-c",
                r#"cd ~ || exit; exec "$0" -l -c '"$0" "$@"' "$@""#,
                "cash.exe",
                "ls",
                "-l"
            ]
        );
    }

    #[test]
    fn a_copy_to_edit_is_named_after_its_file() {
        let temp = std::env::temp_dir();
        let copy = edit_copy(&temp, Path::new("C:/x/app.json"), b"{}").unwrap();
        let name = copy.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("app.sudoedit-"), "{name}");
        assert_eq!(copy.extension().unwrap(), "json");
        assert_eq!(std::fs::read(&copy).unwrap(), b"{}");
        std::fs::remove_file(copy).unwrap();
    }
}
