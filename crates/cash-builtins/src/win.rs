//! Windows-specific builtins — **D45**.
//!
//! Five, each justified by a decision rather than invented:
//!
//! | Builtin | Why it exists |
//! |---|---|
//! | `winpath` | D4 forbids cash rewriting arguments, so this is the deliberate escape hatch when a tool genuinely needs backslashes |
//! | `detach` | D6's escape hatch: start something meant to outlive the shell |
//! | `elevate` | So cash sees a UAC elevation rather than having it happen behind its back, and can register it for D42's tracking |
//! | `start` | The Windows `xdg-open` |
//! | `sudo` | A command elevated in this terminal, through gsudo or Windows' `sudo`, with cash choosing what runs |

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

impl builtins::Command for StartCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        // A file or folder that exists is resolved against the shell's working directory,
        // not the process's (cash never changes its own), in every spelling D3 accepts.
        // Anything else — a URL, `mailto:`, `ms-settings:`, a program's name — goes to the
        // handler as written. No command processor sees it, so `&` and `%` mean nothing.
        let path = context.shell.absolute_path(Path::new(&self.target));
        let target = if path.exists() {
            cash_win32::path::to_backslash(&path)
        } else {
            self.target.clone()
        };

        match cash_win32::shellopen::open(&target, context.shell.working_dir()) {
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
                 not be reaped when cash exits (see D42)"
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

/// Run a command elevated in this terminal, as a Unix `sudo` does.
///
/// cash elevates nothing itself: gsudo does it where it is installed, else Windows' own
/// `sudo`. What cash does is choose what runs, as the shell would: `sudo bash` is cash,
/// where `sudo.exe` looked `bash` up itself and found WSL's; `sudo ls` is cash's `ls`,
/// run by an elevated cash, where there is no `ls.exe` to run; a batch file or a script
/// goes through cash too. A function is not run, as a Unix `sudo` runs none. In a shell
/// already elevated, the command runs here.
#[derive(Parser)]
pub(crate) struct SudoCommand {
    /// The command and its arguments; `-i` or `-s` alone is an elevated shell.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

impl builtins::Command for SudoCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut words = self.command.as_slice();
        let mut shell_wanted = false;
        let mut user: Option<String> = None;
        while let Some((first, rest)) = words.split_first() {
            match first.as_str() {
                "--" => {
                    words = rest;
                    break;
                }
                "-i" | "-s" => shell_wanted = true,
                "-u" | "--user" => {
                    let Some((name, after)) = rest.split_first() else {
                        writeln!(context.stderr(), "sudo: -u: a user is needed")?;
                        return Ok(ExecutionResult::new(1));
                    };
                    // `root` is the administrator, as for `su`: elevation, not an account.
                    user = Some(name.clone()).filter(|name| !name.eq_ignore_ascii_case("root"));
                    words = after;
                    continue;
                }
                option if option.starts_with('-') => {
                    writeln!(
                        context.stderr(),
                        "sudo: {option}: not supported on Windows; usage: sudo [-i | -s] \
                         [-u USER] [COMMAND [ARG]...]"
                    )?;
                    return Ok(ExecutionResult::new(1));
                }
                _ => break,
            }
            words = rest;
        }
        // `sudo NAME=value COMMAND`: the variables are the command's, as a Unix sudo passes
        // them; they were taken for the command's name.
        let assignment_count = words.iter().take_while(|word| is_assignment(word)).count();
        let (assignments, words) = words.split_at(assignment_count);
        if words.is_empty() && !shell_wanted {
            writeln!(
                context.stderr(),
                "usage: sudo [-i | -s] [-u USER] [NAME=value]... [COMMAND [ARG]...]"
            )?;
            return Ok(ExecutionResult::new(1));
        }

        let cash = std::env::current_exe().map_or_else(
            |_| "cash.exe".to_owned(),
            |exe| exe.to_string_lossy().into_owned(),
        );
        if user.is_none() && cash_win32::process::current_process_is_elevated() == Some(true) {
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

        // What runs elevated, as the shell would run it.
        let target: Vec<String> = match words.split_first() {
            None => vec![cash.clone()],
            Some((name, args)) => {
                let Some(target) = launch_words(context.shell, name, args, &cash) else {
                    writeln!(context.stderr(), "sudo: {name}: command not found")?;
                    return Ok(ExecutionResult::new(1));
                };
                target
            }
        };
        let target = if assignments.is_empty() {
            target
        } else {
            with_assignments(&cash, assignments, &target)
        };

        run_as(&mut context, &target, user.as_deref(), "sudo")
    }
}

/// Run `target` elevated, or as `user`, through gsudo, else Windows' own `sudo` (elevated)
/// or `runas` (as a user), and give its status; `who` names the builtin in messages.
///
/// gsudo keeps the command in this terminal; Windows' sudo does in its inline mode only,
/// and runas never. `-d` keeps gsudo from looking at its parent, cash, to decide which
/// shell should run the command: the command is a program, run as it is. As a user, the
/// account's own token at its usual level (`-i Medium`), as Unix's `su` and `sudo -u`
/// give that user's rights, not an administrator's.
fn run_as<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    target: &[String],
    user: Option<&str>,
    who: &str,
) -> Result<ExecutionResult, cash_core::Error> {
    if let Some(gsudo) = context.shell.resolve_command_in_path_using_cache("gsudo") {
        let gsudo = gsudo.to_string_lossy().into_owned();
        let mut args = vec!["-d".to_owned()];
        if let Some(user) = user {
            args.extend(["-u", user, "-i", "Medium"].map(String::from));
        }
        args.extend(target.iter().cloned());
        return run_program(context, &gsudo, "gsudo", &args);
    }
    let system32 = context.shell.env_str("SystemRoot").map_or_else(
        || r"C:\Windows\System32".to_owned(),
        |root| format!(r"{root}\System32"),
    );
    if let Some(user) = user {
        writeln!(
            context.stderr(),
            "{who}: without gsudo, runas starts it as {user} in a new window \
             (`scoop install gsudo` keeps it here)"
        )?;
        let runas = format!(r"{system32}\runas.exe");
        let args = [format!("/user:{user}"), quoted_arguments(target)];
        return run_program(context, &runas, "runas", &args);
    }
    match cash_win32::sysinfo::windows_sudo() {
        cash_win32::sysinfo::WindowsSudo::Off => {
            // Neither tool: UAC itself, as `elevate` asks it, which works on every Windows
            // but starts the command in a new window and does not wait for it.
            writeln!(
                context.stderr(),
                "{who}: no gsudo or Windows sudo, so UAC starts it in a new window and it is \
                 not waited for (`scoop install gsudo` keeps it here)"
            )?;
            let Some((program, args)) = target.split_first() else {
                return Ok(ExecutionResult::new(1));
            };
            return match cash_win32::shellopen::run_elevated(
                program,
                &quoted_arguments(args),
                context.shell.working_dir(),
            ) {
                Ok(()) => Ok(ExecutionResult::success()),
                Err(e) => {
                    writeln!(
                        context.stderr(),
                        "{who}: {}",
                        cash_core::error::os_error_text(&e)
                    )?;
                    Ok(ExecutionResult::new(1))
                }
            };
        }
        cash_win32::sysinfo::WindowsSudo::NewWindow => {
            writeln!(
                context.stderr(),
                "{who}: Windows' sudo is set to open a new window, where the output stays; \
                 `sudo config --enable normal` in an elevated shell keeps it here"
            )?;
        }
        cash_win32::sysinfo::WindowsSudo::InputClosed
        | cash_win32::sysinfo::WindowsSudo::Inline => {}
    }
    let sudo_exe = format!(r"{system32}\sudo.exe");
    run_program(context, &sudo_exe, "sudo", target)
}

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
pub(crate) struct SuCommand {
    /// `[-|-l|--login] [-c COMMAND] [USER [ARG]...]`.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    words: Vec<String>,
}

impl builtins::Command for SuCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut words = self.words.as_slice();
        let mut login = false;
        let mut command = None;
        while let Some((first, rest)) = words.split_first() {
            match first.as_str() {
                "--" => {
                    words = rest;
                    break;
                }
                "-" | "-l" | "--login" => login = true,
                "-c" | "--command" => {
                    let Some((text, after)) = rest.split_first() else {
                        writeln!(context.stderr(), "su: -c: a command is needed")?;
                        return Ok(ExecutionResult::new(1));
                    };
                    command = Some(text.clone());
                    words = after;
                    continue;
                }
                option if option.starts_with('-') => {
                    writeln!(
                        context.stderr(),
                        "su: {option}: not supported on Windows; usage: su [-|-l] [-c COMMAND] \
                         [USER [ARG]...]"
                    )?;
                    return Ok(ExecutionResult::new(1));
                }
                _ => break,
            }
            words = rest;
        }
        let (user, shell_args) = match words.split_first() {
            Some((user, args)) => (
                Some(user.as_str()).filter(|user| !user.eq_ignore_ascii_case("root")),
                args,
            ),
            None => (None, words),
        };

        let cash = std::env::current_exe().map_or_else(
            |_| "cash.exe".to_owned(),
            |exe| exe.to_string_lossy().into_owned(),
        );
        let mut args: Vec<String> = Vec::new();
        if let Some(command) = command {
            args.extend(["-c".to_owned(), command]);
        }
        args.extend(shell_args.iter().cloned());
        // A login shell starts in the account's home, which only the shell started as that
        // account knows: it goes there and then becomes the login shell.
        let target = if login {
            let mut all = vec![
                cash.clone(),
                "-c".to_owned(),
                "cd ~ || exit; exec \"$0\" -l \"$@\"".to_owned(),
                cash.clone(),
            ];
            all.extend(args);
            all
        } else {
            let mut all = vec![cash.clone()];
            all.extend(args);
            all
        };

        if user.is_none() && cash_win32::process::current_process_is_elevated() == Some(true) {
            return run_program(&context, &cash, &cash, target.get(1..).unwrap_or(&[]));
        }
        run_as(&mut context, &target, user, "su")
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
}
