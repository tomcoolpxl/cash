//! Command execution

use std::{
    borrow::Cow,
    ffi::OsStr,
    fmt::Display,
    path::{Path, PathBuf},
    process::Stdio,
};

use cash_parser::ast;
use itertools::Itertools;
use sys::commands::{CommandExt, CommandFdInjectionExt, CommandFgControlExt};

use crate::{
    ErrorKind, ExecutionControlFlow, ExecutionExitCode, ExecutionParameters, ExecutionResult,
    Shell, ShellFd, builtins, commands, error, escape,
    extensions::{self, ShellExtensions},
    functions,
    interp::{self, Execute, ProcessGroupPolicy},
    openfiles::{self, OpenFile, OpenFiles},
    pathsearch, processes,
    results::ExecutionSpawnResult,
    sys, trace_categories, traps,
};

/// Encapsulates the result of waiting for a command to complete.
pub enum CommandWaitResult {
    /// The command completed.
    CommandCompleted(ExecutionResult),
    /// The command was stopped before it completed.
    CommandStopped(ExecutionResult, processes::ChildProcess),
}

/// Represents the context for executing a command.
pub struct ExecutionContext<'a, SE: ShellExtensions = extensions::DefaultShellExtensions> {
    /// The shell in which the command is being executed.
    pub shell: &'a mut Shell<SE>,
    /// The name of the command being executed.
    pub command_name: String,
    /// The parameters for the execution.
    pub params: ExecutionParameters,
}

impl<SE: ShellExtensions> ExecutionContext<'_, SE> {
    /// Returns the standard input file; usable with `write!` et al.
    pub fn stdin(&self) -> impl std::io::Read + 'static {
        self.params.stdin(self.shell)
    }

    /// Returns the standard output file; usable with `write!` et al.
    pub fn stdout(&self) -> impl std::io::Write + 'static {
        self.params.stdout(self.shell)
    }

    /// Returns the standard error file; usable with `write!` et al.
    pub fn stderr(&self) -> impl std::io::Write + 'static {
        self.params.stderr(self.shell)
    }

    /// Returns the file descriptor with the given number. Returns `None`
    /// if the file descriptor is not open.
    ///
    /// # Arguments
    ///
    /// * `fd` - The file descriptor number to retrieve.
    pub fn try_fd(&self, fd: ShellFd) -> Option<openfiles::OpenFile> {
        self.params.try_fd(self.shell, fd)
    }

    /// Iterates over all open file descriptors.
    pub fn iter_fds(&self) -> impl Iterator<Item = (ShellFd, openfiles::OpenFile)> {
        self.params.iter_fds(self.shell)
    }
}

/// An argument to a command.
#[derive(Clone, Debug)]
pub enum CommandArg {
    /// A simple string argument.
    String(String),
    /// An assignment/declaration; typically treated as a string, but will
    /// be specially handled by a limited set of built-in commands.
    Assignment(ast::Assignment),
}

impl Display for CommandArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(s) => f.write_str(s),
            Self::Assignment(a) => write!(f, "{a}"),
        }
    }
}

impl From<String> for CommandArg {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&String> for CommandArg {
    fn from(value: &String) -> Self {
        Self::String(value.clone())
    }
}

impl CommandArg {
    pub(crate) fn quote_for_tracing(&self) -> Cow<'_, str> {
        match self {
            Self::String(s) => escape::quote_if_needed(s, escape::QuoteMode::SingleQuote),
            Self::Assignment(a) => {
                let mut s = a.name.to_string();
                let op = if a.append { "+=" } else { "=" };
                s.push_str(op);
                s.push_str(&escape::quote_if_needed(
                    a.value.to_string().as_str(),
                    escape::QuoteMode::SingleQuote,
                ));
                s.into()
            }
        }
    }
}

/// Encapsulates a possibly-owned reference to a `Shell` for command execution.
pub enum ShellForCommand<'a, SE: extensions::ShellExtensions> {
    /// The command is run in the same shell as its parent; the provided
    /// mutable reference allows modifying the parent shell.
    ParentShell(&'a mut Shell<SE>),
    /// The command is run in its own owned shell (which is also provided).
    OwnedShell {
        /// The owned shell.
        target: Box<Shell<SE>>,
        /// The parent shell.
        parent: &'a mut Shell<SE>,
    },
}

impl<SE: extensions::ShellExtensions> std::ops::Deref for ShellForCommand<'_, SE> {
    type Target = Shell<SE>;

    fn deref(&self) -> &Self::Target {
        match self {
            ShellForCommand::ParentShell(shell) => shell,
            ShellForCommand::OwnedShell { target, .. } => target,
        }
    }
}

impl<SE: extensions::ShellExtensions> std::ops::DerefMut for ShellForCommand<'_, SE> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            ShellForCommand::ParentShell(shell) => shell,
            ShellForCommand::OwnedShell { target, .. } => target,
        }
    }
}

/// Writes the PowerShell runner script to the temp folder if it is not there yet, and
/// returns its path.
fn ensure_ps_runner() -> Result<PathBuf, error::Error> {
    let runner_path = std::env::temp_dir().join("cash_ps_runner.ps1");
    if !runner_path.exists() {
        const RUNNER_CONTENT: &str = "\
& ([scriptblock]::Create([System.IO.File]::ReadAllText($env:CASH_PS_SCRIPT))) @args\r\n\
if ($LASTEXITCODE -ne $null) {\r\n\
    exit $LASTEXITCODE\r\n\
} elseif (!$?) {\r\n\
    exit 1\r\n\
} else {\r\n\
    exit 0\r\n\
}\r\n";
        std::fs::write(&runner_path, RUNNER_CONTENT).map_err(|e| {
            error::ErrorKind::FailedToExecuteCommand(runner_path.to_string_lossy().into_owned(), e)
        })?;
    }
    Ok(runner_path)
}

fn find_powershell_binary<SE: extensions::ShellExtensions>(
    context: &ExecutionContext<'_, SE>,
) -> PathBuf {
    let path_var = context
        .shell
        .env()
        .get_str("PATH", context.shell)
        .unwrap_or_default();
    let path_entries: Vec<PathBuf> = crate::sys::fs::split_paths(path_var.as_ref()).collect();
    let pathext: Vec<String> = cash_win32::resolve::DEFAULT_PATHEXT
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    if let Some(d) =
        cash_win32::resolve::resolve("pwsh", &path_entries, &pathext, context.shell.working_dir())
    {
        return d.target().to_path_buf();
    }
    if let Some(d) = cash_win32::resolve::resolve(
        "powershell",
        &path_entries,
        &pathext,
        context.shell.working_dir(),
    ) {
        return d.target().to_path_buf();
    }
    PathBuf::from("powershell.exe")
}

fn build_powershell_command<S: AsRef<OsStr>, SE: extensions::ShellExtensions>(
    context: &ExecutionContext<'_, SE>,
    script: &Path,
    extra_args: &[String],
    args: &[S],
) -> Result<(std::process::Command, Option<String>), error::Error> {
    let pwsh_bin = find_powershell_binary(context);
    let mut c = std::process::Command::new(pwsh_bin);
    c.arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-ExecutionPolicy")
        .arg("Bypass");

    if script
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("ps1"))
    {
        c.arg("-File");
        c.arg(script);
        c.args(extra_args);
        c.args(args);
        Ok((c, None))
    } else {
        let runner_path = ensure_ps_runner()?;
        c.arg("-File");
        c.arg(runner_path);
        c.args(extra_args);
        c.args(args);
        Ok((c, Some(script.to_string_lossy().into_owned())))
    }
}

/// Adds `args` for the native program at `target`, in the command-line encoding that
/// program decodes: Cygwin's for an MSYS2 or Cygwin program, which Git's `usr/bin` tools
/// are, and the Microsoft C runtime's, which `Command::args` writes, for the rest.
fn push_native_args<S: AsRef<OsStr>>(c: &mut std::process::Command, target: &Path, args: &[S]) {
    cash_win32::msys::add_args(c, Some(target), args);
}

fn build_batch_command<S: AsRef<OsStr>>(
    command_name: &str,
    argv0: &str,
    args: &[S],
) -> std::process::Command {
    use std::os::windows::process::CommandExt as _;

    let comspec =
        std::env::var_os("COMSPEC").map_or_else(|| PathBuf::from("cmd.exe"), PathBuf::from);
    let mut c = std::process::Command::new(comspec);
    c.arg0(argv0);
    c.arg("/d").arg("/s").arg("/c");

    let string_args: Vec<String> = args
        .iter()
        .map(|a| a.as_ref().to_string_lossy().into_owned())
        .collect();
    // `cmd` reads `/` as a switch character, so `./showargs.cmd .` would run the command
    // `.` with the switch `/showargs.cmd`. Hand it the same path with backslashes; the
    // spelling stays relative, so `%0` and `%~dp0` still name the script.
    let command_name = command_name.replace('/', "\\");
    let mut inner = cash_win32::cmd::escape_for_cmd(&command_name);
    for arg in &string_args {
        inner.push(' ');
        inner.push_str(&cash_win32::cmd::escape_for_cmd(arg));
    }
    c.raw_arg(format!("\"{inner}\""));
    c
}

fn build_shebang_command<S: AsRef<OsStr>, SE: extensions::ShellExtensions>(
    context: &ExecutionContext<'_, SE>,
    interpreter: &str,
    shebang_args: &[String],
    script: &Path,
    argv0: &str,
    args: &[S],
) -> Result<(std::process::Command, Option<String>), error::Error> {
    let path_var = context
        .shell
        .env()
        .get_str("PATH", context.shell)
        .unwrap_or_default();
    let path_entries: Vec<PathBuf> =
        crate::sys::fs::split_paths_preserving_empty(path_var.as_ref()).collect();
    let pathext_var = context
        .shell
        .env()
        .get_str("PATHEXT", context.shell)
        .unwrap_or_default();
    let pathext = if pathext_var.is_empty() {
        cash_win32::resolve::DEFAULT_PATHEXT
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    } else {
        cash_win32::resolve::parse_pathext(pathext_var.as_ref())
    };

    let resolved = cash_win32::resolve::resolve_interpreter(
        interpreter,
        shebang_args,
        &path_entries,
        &pathext,
        context.shell.working_dir(),
    );

    let Some((dispatch, extra_args)) = resolved else {
        return Err(error::ErrorKind::CommandNotFound(interpreter.to_string()).into());
    };

    match dispatch {
        cash_win32::resolve::Dispatch::Exit(code) => {
            let own = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("cash.exe"));
            let mut c = std::process::Command::new(own);
            c.arg("-c").arg(format!("exit {code}"));
            Ok((c, None))
        }
        cash_win32::resolve::Dispatch::PowerShell(_) => {
            build_powershell_command(context, script, &extra_args, args)
        }
        cash_win32::resolve::Dispatch::Batch(ref batch_target) => {
            use std::os::windows::process::CommandExt as _;

            let comspec =
                std::env::var_os("COMSPEC").map_or_else(|| PathBuf::from("cmd.exe"), PathBuf::from);
            let mut c = std::process::Command::new(comspec);
            c.arg0(argv0);
            c.arg("/d").arg("/s").arg("/c");

            let mut inner = cash_win32::cmd::escape_for_cmd(&batch_target.to_string_lossy());
            for arg in &extra_args {
                inner.push(' ');
                inner.push_str(&cash_win32::cmd::escape_for_cmd(arg));
            }
            inner.push(' ');
            inner.push_str(&cash_win32::cmd::escape_for_cmd(&script.to_string_lossy()));
            for arg in args {
                inner.push(' ');
                let a = arg.as_ref().to_string_lossy();
                inner.push_str(&cash_win32::cmd::escape_for_cmd(&a));
            }
            c.raw_arg(format!("\"{inner}\""));
            Ok((c, None))
        }
        cash_win32::resolve::Dispatch::Native(ref target) => {
            let mut c = std::process::Command::new(target);
            c.arg0(argv0);
            push_native_args(&mut c, target, &extra_args);
            push_native_args(&mut c, target, &[script]);
            push_native_args(&mut c, target, args);
            Ok((c, None))
        }
        cash_win32::resolve::Dispatch::Shebang { .. } => {
            let mut c = std::process::Command::new(script);
            c.arg0(argv0);
            c.args(args);
            Ok((c, None))
        }
    }
}

fn build_windows_command<S: AsRef<OsStr>, SE: extensions::ShellExtensions>(
    context: &ExecutionContext<'_, SE>,
    command_name: &str,
    argv0: &str,
    args: &[S],
) -> Result<(std::process::Command, Option<String>), error::Error> {
    let path = Path::new(command_name);
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        let joined = context.shell.working_dir().join(path);
        if joined.is_file() {
            joined
        } else {
            path.to_path_buf()
        }
    };

    // A virtual path from `which` (`C:/…/cash.exe/ls`) run as a process — `exec
    // "$(which ls)"` — re-enters cash to run the command it names (ROADMAP item 12).
    if !candidate.is_file()
        && let Some(tool) = cash_win32::path::virtual_tool(command_name)
    {
        let mut command = cash_win32::path::reentry_command(&tool);
        command.args(args);
        return Ok((command, None));
    }

    if !candidate.is_file() {
        // A bare name, as `exec grep` passes: `Command::new` searches PATH for it, so
        // search the shell's PATH too, to know which encoding its arguments need.
        let path_var = context
            .shell
            .env()
            .get_str("PATH", context.shell)
            .unwrap_or_default();
        let entries: Vec<PathBuf> =
            crate::sys::fs::split_paths_preserving_empty(path_var.as_ref()).collect();
        let target = cash_win32::msys::locate(
            OsStr::new(command_name),
            &entries,
            context.shell.working_dir(),
        );
        let mut c = std::process::Command::new(command_name);
        c.arg0(argv0);
        cash_win32::msys::add_args(&mut c, target.as_deref(), args);
        return Ok((c, None));
    }

    match cash_win32::resolve::classify(&candidate) {
        cash_win32::resolve::Dispatch::Native(_) => {
            let mut c = std::process::Command::new(command_name);
            c.arg0(argv0);
            push_native_args(&mut c, &candidate, args);
            Ok((c, None))
        }
        cash_win32::resolve::Dispatch::Batch(_) => {
            Ok((build_batch_command(command_name, argv0, args), None))
        }
        cash_win32::resolve::Dispatch::PowerShell(_) => {
            build_powershell_command(context, &candidate, &[], args)
        }
        cash_win32::resolve::Dispatch::Shebang {
            interpreter,
            args: shebang_args,
            script,
        } => build_shebang_command(context, &interpreter, &shebang_args, &script, argv0, args),
        cash_win32::resolve::Dispatch::Exit(code) => {
            let own = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("cash.exe"));
            let mut c = std::process::Command::new(own);
            c.arg("-c").arg(format!("exit {code}"));
            Ok((c, None))
        }
    }
}

/// The variables an external command started by `shell` receives: the exported ones that
/// are set, with `PATH` in the form a Windows program reads.
///
/// # Arguments
///
/// * `shell` - The shell whose environment to export.
pub fn exported_environment(
    shell: &Shell<impl extensions::ShellExtensions>,
) -> Vec<(String, String)> {
    shell
        .env()
        .iter_exported()
        // NOTE: To match bash behavior, we only include exported variables
        // that are set (i.e., have a value). This means a variable that
        // shows up in `declare -p` but has no *set* value will be omitted.
        .filter(|(_, v)| v.value().is_set())
        .map(|(k, v)| {
            let value = v.value().to_cow_str(shell);

            // cash (D5): PATH goes back to the semicolon-separated Windows form at
            // the process boundary. The shell holds and shows the Unix form so that
            // `IFS=: read -ra dirs <<< "$PATH"` works, but `git.exe` and
            // `terraform.exe` cannot read that — a child handed `/c/tools:/c/bin`
            // finds nothing at all.
            if k.eq_ignore_ascii_case("PATH") {
                return (k.clone(), cash_win32::env::path_to_windows(value.as_ref()));
            }

            (k.clone(), value.into_owned())
        })
        .collect()
}

/// Composes a `std::process::Command` to execute the given command. Appropriately
/// configures the command name and arguments, redirections, injected file
/// descriptors, environment variables, etc.
///
/// # Arguments
///
/// * `context` - The execution context in which the command is being composed.
/// * `command_name` - The name of the command to execute.
/// * `argv0` - The value to use for `argv[0]` (may be different from the command).
/// * `args` - The arguments to pass to the command.
/// * `empty_env` - If true, the command will be executed with an empty environment; if false, the
///   command will inherit environment variables marked as exported in the provided `Shell`.
pub fn compose_std_command<S: AsRef<OsStr>, SE: extensions::ShellExtensions>(
    context: &ExecutionContext<'_, SE>,
    command_name: &str,
    argv0: &str,
    args: &[S],
    empty_env: bool,
) -> Result<std::process::Command, error::Error> {
    let (mut cmd, target_ps_script) = build_windows_command(context, command_name, argv0, args)?;

    // Use the shell's current working dir.
    cmd.current_dir(context.shell.working_dir());

    // Start with a clear environment.
    cmd.env_clear();

    // Add in exported variables.
    if !empty_env {
        for (name, value) in exported_environment(context.shell) {
            cmd.env(name, value);
        }
        // Set _ to the resolved command path for external commands.
        cmd.env("_", command_name);
    }

    if let Some(ps_script) = target_ps_script {
        cmd.env("CASH_PS_SCRIPT", ps_script);
    }

    // Add in exported functions.
    if !empty_env {
        for (func_name, registration) in context.shell.funcs().iter() {
            if registration.is_exported() {
                let var_name = std::format!("BASH_FUNC_{func_name}%%");
                let value = std::format!("() {}", registration.definition().body);
                cmd.env(var_name, value);
            }
        }
    }

    // Redirect stdin, if applicable.
    match context.try_fd(OpenFiles::STDIN_FD) {
        Some(OpenFile::Stdin(_)) | None => (),
        Some(stdin_file) => {
            let as_stdio: Stdio = stdin_file.try_into()?;
            cmd.stdin(as_stdio);
        }
    }

    // Redirect stdout, if applicable.
    match context.try_fd(OpenFiles::STDOUT_FD) {
        Some(OpenFile::Stdout(_)) | None => (),
        Some(stdout_file) => {
            let as_stdio: Stdio = stdout_file.try_into()?;
            cmd.stdout(as_stdio);
        }
    }

    // Redirect stderr, if applicable.
    match context.try_fd(OpenFiles::STDERR_FD) {
        Some(OpenFile::Stderr(_)) | None => {}
        Some(stderr_file) => {
            let as_stdio: Stdio = stderr_file.try_into()?;
            cmd.stderr(as_stdio);
        }
    }

    // Inject any other fds.
    let other_files = context.iter_fds().filter(|(fd, _)| {
        *fd != OpenFiles::STDIN_FD && *fd != OpenFiles::STDOUT_FD && *fd != OpenFiles::STDERR_FD
    });

    // cash (D26): a native exe cannot be handed fd 3 and up, so only a descriptor
    // this command redirects itself (`tool.exe 3>x`) reaches `inject_fds` and fails
    // loudly. Descriptors the shell merely holds — `exec 3>&1 1>log`, or `{ ...; } 3>x`
    // around the command — are left behind, as the child could not use them anyway.
    let other_files =
        other_files.filter(|(fd, _)| context.params.command_redirected_fds.contains(fd));

    cmd.inject_fds(other_files)?;

    Ok(cmd)
}

/// Runs pre-execution hooks. Returns a result when the DEBUG trap ran `exit` or
/// `return`; the command must then not run, and that result replaces it.
pub(crate) async fn on_preexecute(
    cmd: &mut commands::SimpleCommand<'_, impl extensions::ShellExtensions>,
) -> Result<Option<ExecutionResult>, error::Error> {
    // Fire the DEBUG trap if one is registered.
    if cmd.shell.traps().handles(traps::TrapSignal::Debug) {
        let result = cmd
            .shell
            .invoke_trap_handler(traps::TrapSignal::Debug, &cmd.params)
            .await?;
        if matches!(
            result.next_control_flow,
            ExecutionControlFlow::ExitShell | ExecutionControlFlow::ReturnFromFunctionOrScript
        ) {
            if let Some(post_execute) = cmd.post_execute.take() {
                let _ = post_execute(&mut cmd.shell);
            }
            return Ok(Some(result));
        }
    }

    Ok(None)
}

/// Represents a simple command to be executed.
pub struct SimpleCommand<'a, SE: extensions::ShellExtensions> {
    /// The shell to run the command in.
    shell: ShellForCommand<'a, SE>,

    /// The execution parameters for the command.
    pub params: ExecutionParameters,

    /// The name of the command to execute.
    pub command_name: String,

    /// The arguments to the command, including the command itself.
    pub args: Vec<CommandArg>,

    /// Whether to consider shell functions when looking up the command name.
    /// If true, shell functions will be checked; if false, they will be ignored.
    pub use_functions: bool,

    /// Optional list of directories to search for external commands. If left
    /// `None`, the default search logic will be used.
    pub path_dirs: Option<Vec<PathBuf>>,

    /// The process group ID to use for externally executed commands. This may be
    /// `None`, in which case the default behavior will be used.
    pub process_group_id: Option<i32>,

    /// Optional override for the `argv[0]` value presented to an externally
    /// spawned process. When `None`, `command_name` is used.
    pub argv0: Option<String>,

    /// Optionally provides a function that can run after execution occurs. Note
    /// that it is *not* invoked if the shell is discarded during the execution
    /// process.
    #[allow(clippy::type_complexity)]
    pub post_execute: Option<fn(&mut Shell<SE>) -> Result<(), error::Error>>,
}

impl<'a, SE: extensions::ShellExtensions> SimpleCommand<'a, SE> {
    /// Creates a new `SimpleCommand` instance.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell in which to execute the command.
    /// * `params` - The execution parameters for the command.
    /// * `command_name` - The name of the command to execute.
    /// * `args` - The arguments to the command, including the command itself.
    pub fn new<I>(
        shell: ShellForCommand<'a, SE>,
        params: ExecutionParameters,
        command_name: String,
        args: I,
    ) -> Self
    where
        I: IntoIterator<Item = CommandArg>,
    {
        Self {
            shell,
            params,
            command_name,
            args: args.into_iter().collect(),
            use_functions: true,
            path_dirs: None,
            process_group_id: None,
            argv0: None,
            post_execute: None,
        }
    }

    /// Executes the simple command.
    ///
    /// The command may be a builtin, a shell function, or an externally
    /// executed command. This function's implementation is responsible for
    /// dispatching it appropriately according to the context provided.
    #[allow(
        clippy::missing_panics_doc,
        reason = "these unwrap calls should not panic"
    )]
    pub async fn execute(mut self) -> Result<ExecutionSpawnResult, error::Error> {
        // cash (ROADMAP item 12): `which ls` prints `C:/…/cash.exe/ls` for a command cash
        // carries, so that `"$(which ls)" -la` can be run. That path is no file; it names
        // the builtin, which runs as if typed by name — but never a function, as a path
        // never names one.
        if let Some(tool) = cash_win32::path::virtual_tool(&self.command_name)
            && self
                .shell
                .builtins()
                .get(&tool)
                .is_some_and(|r| !r.disabled)
        {
            if let Some(first) = self.args.first_mut() {
                *first = CommandArg::String(tool.clone());
            }
            self.command_name = tool;
            self.use_functions = false;
        }

        // First see if it's the name of a builtin.
        let builtin = self.shell.builtins().get(&self.command_name).cloned();

        // If we're in POSIX mode and found a special builtin (that's not disabled), then invoke it
        // without considering functions.
        if self.shell.options().posix_mode
            && builtin
                .as_ref()
                .is_some_and(|r| !r.disabled && r.special_builtin)
        {
            #[allow(clippy::unwrap_used, reason = "we just checked that builtin is Some")]
            let builtin = builtin.unwrap();
            return self.execute_via_builtin(builtin).await;
        }

        // Assuming we weren't requested not to do so, check if it's the name of
        // a shell function.
        if self.use_functions {
            if let Some(func_registration) =
                self.shell.funcs().get(self.command_name.as_str()).cloned()
            {
                return self.execute_via_function(func_registration).await;
            }
        }

        // If we haven't yet resolved the command name and found a builtin that's not disabled,
        // then invoke it.
        if let Some(builtin) = builtin {
            if !builtin.disabled {
                return self.execute_via_builtin(builtin).await;
            }
        }

        // We still haven't found a command to invoke. We'll need to look for an external command.
        if !sys::fs::contains_path_separator(&self.command_name) {
            // All else failed; if we were given path directories to search, look through them
            // for a match. Otherwise, use our default search logic.
            let path = if let Some(path_dirs) = &self.path_dirs {
                pathsearch::resolve_command(path_dirs, self.command_name.as_str())
            } else {
                self.shell
                    .resolve_command_in_path_using_cache(&self.command_name)
            };

            if let Some(path) = path {
                self.execute_via_external(&path)
            } else {
                // Bash updates $_ even when the command is not found, so mirror
                // that here before reporting the error.
                let last_arg = Self::take_last_arg(&self.args);
                self.shell.update_last_arg_variable(last_arg);

                if let Some(post_execute) = self.post_execute {
                    let _ = post_execute(&mut self.shell);
                }

                Err(ErrorKind::CommandNotFound(self.command_name).into())
            }
        } else {
            // cash (D3/D8): a command spelled with a path is a path *cash* resolves, so
            // every accepted spelling works here — `/c/tools/x.exe`, `C:/tools/x.exe`,
            // a quoted backslash form.
            //
            // D4 is not in tension with this: it forbids rewriting *arguments*, where
            // cash cannot know which are paths. The command name is unambiguous.
            //
            // This is what makes D37 possible. `starship init bash` emits
            // `eval -- "$('/c/Program Files/starship/bin/starship.exe' init bash ...)"`,
            // and without this the prompt fails with `command not found` on a path that
            // plainly exists.
            let command_name = cash_win32::path::accept_path(&self.command_name);

            self.execute_via_external(command_name.as_path())
        }
    }

    /// Extracts the owned string representation of the last argument of a
    /// command, suitable for recording into `$_`.
    fn take_last_arg(args: &[CommandArg]) -> Option<String> {
        args.last().map(ToString::to_string)
    }

    async fn execute_via_builtin(
        self,
        builtin: builtins::Registration<SE>,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        if let Some(spawn_func) = builtin.spawn_func {
            return self.execute_via_builtin_spawn(spawn_func).await;
        }

        match self.shell {
            ShellForCommand::OwnedShell { target, .. } => {
                Ok(Self::execute_via_builtin_in_owned_shell(
                    *target,
                    self.params,
                    builtin,
                    self.command_name,
                    self.args,
                ))
            }
            ShellForCommand::ParentShell(..) => {
                self.execute_via_builtin_in_parent_shell(builtin).await
            }
        }
    }

    async fn execute_via_builtin_spawn(
        mut self,
        spawn_func: builtins::CommandSpawnFunc<SE>,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        let last_arg = Self::take_last_arg(&self.args);

        let result = {
            let cmd_context = ExecutionContext {
                shell: &mut self.shell,
                command_name: self.command_name,
                params: self.params,
            };

            spawn_func(cmd_context, self.args, self.process_group_id).await
        };

        // Update $_ after command execution.
        self.shell.update_last_arg_variable(last_arg);

        if let Some(post_execute) = self.post_execute {
            let _ = post_execute(&mut self.shell);
        }

        result
    }

    fn execute_via_builtin_in_owned_shell(
        mut shell: Shell<SE>,
        params: ExecutionParameters,
        builtin: builtins::Registration<SE>,
        command_name: String,
        args: Vec<CommandArg>,
    ) -> ExecutionSpawnResult {
        let last_arg = Self::take_last_arg(&args);
        let join_handle = tokio::task::spawn_blocking(move || {
            let cmd_context = ExecutionContext {
                shell: &mut shell,
                command_name,
                params,
            };

            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(execute_builtin_command(&builtin, cmd_context, args));

            // Update $_ after command execution.
            shell.update_last_arg_variable(last_arg);

            result
        });

        ExecutionSpawnResult::StartedTask(join_handle)
    }

    async fn execute_via_builtin_in_parent_shell(
        self,
        builtin: builtins::Registration<SE>,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        let mut shell = self.shell;
        let last_arg = Self::take_last_arg(&self.args);

        let cmd_context = ExecutionContext {
            shell: &mut shell,
            command_name: self.command_name,
            params: self.params,
        };

        let result = execute_builtin_command(&builtin, cmd_context, self.args).await;

        // Update $_ after command execution.
        shell.update_last_arg_variable(last_arg);

        if let Some(post_execute) = self.post_execute {
            let _ = post_execute(&mut shell);
        }

        let result = result?;

        Ok(result.into())
    }

    async fn execute_via_function(
        self,
        func_registration: functions::Registration,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        // cash: a function with its own shell is a pipeline member, and running it
        // inline meant it finished before the next member was created — so nothing
        // drained the pipe and everything it wrote above the buffer was lost:
        //
        //     f() { seq 1 5000; }; f | wc -l   ->  0      (bash: 5000)
        //
        // Spawned as a task, exactly as a builtin pipeline member already was. The
        // parent-shell case still runs inline: its output is not going into a pipe
        // anybody has to drain, and moving it would lose the caller's variables.
        if let ShellForCommand::OwnedShell { target, .. } = self.shell {
            return Ok(Self::execute_via_function_in_owned_shell(
                *target,
                func_registration,
                self.command_name,
                self.params,
                self.args,
                self.post_execute,
            ));
        }

        let mut shell = self.shell;
        let last_arg = Self::take_last_arg(&self.args);

        let cmd_context = ExecutionContext {
            shell: &mut shell,
            command_name: self.command_name,
            params: self.params,
        };

        // Strip the function name off args.
        let result = invoke_shell_function(func_registration, cmd_context, &self.args[1..]).await;

        // $_ is reset *after* the function body runs, to the last argument of
        // the invocation (or the function name itself if zero args). Any
        // mutations made inside the body are overwritten — this matches bash,
        // where the caller observes only the invocation's last argument.
        shell.update_last_arg_variable(last_arg);

        if let Some(post_execute) = self.post_execute {
            let _ = post_execute(&mut shell);
        }

        result
    }

    /// Run a function pipeline member on its own shell, concurrently with the rest.
    #[allow(
        clippy::type_complexity,
        reason = "the hook's type is the field's; naming it separately would not clarify it"
    )]
    fn execute_via_function_in_owned_shell(
        mut shell: Shell<SE>,
        func_registration: functions::Registration,
        command_name: String,
        params: ExecutionParameters,
        args: Vec<CommandArg>,
        post_execute: Option<fn(&mut Shell<SE>) -> Result<(), error::Error>>,
    ) -> ExecutionSpawnResult {
        let last_arg = Self::take_last_arg(&args);

        let Ok(slot_guard) = crate::jobs::SubshellSlotGuard::try_acquire() else {
            use std::io::Write as _;
            let _ = writeln!(
                params.stderr(&shell),
                "cash: fork: retry: Resource temporarily unavailable"
            );
            return ExecutionSpawnResult::Completed(ExecutionResult::general_error());
        };

        let join_handle = tokio::task::spawn_blocking(move || {
            let _guard = slot_guard;
            let rt = tokio::runtime::Handle::current();

            let spawned = {
                let cmd_context = ExecutionContext {
                    shell: &mut shell,
                    command_name,
                    params,
                };
                rt.block_on(invoke_shell_function(
                    func_registration,
                    cmd_context,
                    &args[1..],
                ))
            };

            // A function body can itself start a process or a task; wait for whatever it
            // produced so the pipeline sees one finished result.
            let result = match spawned {
                Ok(spawned) => match rt.block_on(spawned.wait()) {
                    Ok(crate::results::ExecutionWaitResult::Completed(result)) => Ok(result),
                    Ok(crate::results::ExecutionWaitResult::Stopped(_)) => {
                        Ok(ExecutionResult::success())
                    }
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            };

            shell.update_last_arg_variable(last_arg);
            if let Some(post_execute) = post_execute {
                let _ = post_execute(&mut shell);
            }

            result
        });

        ExecutionSpawnResult::StartedTask(join_handle)
    }

    fn execute_via_external(self, path: &Path) -> Result<ExecutionSpawnResult, error::Error> {
        let mut shell = self.shell;
        let last_arg = Self::take_last_arg(&self.args);

        if path.is_file() {
            let dispatch = cash_win32::resolve::classify(path);
            let exit_code = match dispatch {
                cash_win32::resolve::Dispatch::Exit(code) => Some(code),
                cash_win32::resolve::Dispatch::Shebang {
                    ref interpreter,
                    ref args,
                    ..
                } => {
                    let path_var = shell.env().get_str("PATH", &shell).unwrap_or_default();
                    let path_entries: Vec<PathBuf> =
                        crate::sys::fs::split_paths_preserving_empty(path_var.as_ref()).collect();
                    let pathext_var = shell.env().get_str("PATHEXT", &shell).unwrap_or_default();
                    let pathext = if pathext_var.is_empty() {
                        cash_win32::resolve::DEFAULT_PATHEXT
                            .iter()
                            .map(|s| (*s).to_string())
                            .collect()
                    } else {
                        cash_win32::resolve::parse_pathext(pathext_var.as_ref())
                    };
                    cash_win32::resolve::resolve_interpreter(
                        interpreter,
                        args,
                        &path_entries,
                        &pathext,
                        shell.working_dir(),
                    )
                    .and_then(|(d, _)| match d {
                        cash_win32::resolve::Dispatch::Exit(c) => Some(c),
                        _ => None,
                    })
                }
                _ => None,
            };

            if let Some(code) = exit_code {
                shell.update_last_arg_variable(last_arg);
                if let Some(post_execute) = self.post_execute {
                    let _ = post_execute(&mut shell);
                }
                return Ok(ExecutionResult::new(code).into());
            }
        }

        let cmd_context = ExecutionContext {
            shell: &mut shell,
            command_name: self.command_name,
            params: self.params,
        };

        let resolved_path = path.to_string_lossy();
        let result = execute_external_command(
            cmd_context,
            resolved_path.as_ref(),
            self.process_group_id,
            self.argv0.as_deref(),
            &self.args[1..],
        );

        // Update $_ after command execution.
        shell.update_last_arg_variable(last_arg);

        if let Some(post_execute) = self.post_execute {
            let _ = post_execute(&mut shell);
        }

        result
    }
}

/// cash (D3/D4): explain the `/x/...` cliff rather than letting it fail silently.
///
/// D3 accepts Unix drive spellings for paths cash resolves itself — `cd /c/src`,
/// `[ -f /d/data/x ]`, `> /e/out` all work. But D4 forbids rewriting arguments, so a
/// command receives `/c/src/main.tf` verbatim, and any tool without its own MSYS-style
/// translation cannot open it. That includes MS Coreutils and the bundled builtins.
///
/// The tool's own error — "The system cannot find the path specified" — explains
/// nothing, and this is the single easiest mistake to make in cash. So: warn only when
/// the literal spelling does not exist *and* the translated one does. That makes false
/// positives essentially impossible, and says the useful thing.
fn warn_about_unix_drive_spellings(
    context: &ExecutionContext<'_, impl extensions::ShellExtensions>,
    cmd_args: &[&String],
) {
    use std::io::Write as _;

    for arg in cmd_args {
        let Some(translated) = cash_win32::path::unix_drive_spelling(arg) else {
            continue;
        };
        if std::path::Path::new(arg.as_str()).exists() || !translated.exists() {
            continue;
        }

        let mut stderr = context.stderr();
        let _ = writeln!(
            stderr,
            // The command is deliberately not named. A bundled builtin (D48) re-enters
            // this binary to dispatch, so `command_name` there is cash's own path rather
            // than the `cat` the user typed, which would mislead.
            "cash: {}: a command receives this path as written; cash does not \
             translate Unix path spellings in arguments (D4). Try {} or \
             \"$(winpath {})\"",
            arg,
            cash_win32::path::render(&translated),
            arg,
        );
        break;
    }
}

/// cash (D13): a background job's process leads a group of its own, and the registry is
/// what lets a Ctrl-Break be aimed at it safely later.
fn register_background_leader(child: &sys::process::Child, background: bool) {
    if background && let Some(raw) = child.id() {
        cash_win32::stop::register_group_leader(raw);
    }
}

/// cash (D11/D22): report a process upward so a background job can answer `$!` and
/// `kill %1`. There is a `sink` only when running under a background job; otherwise the
/// caller already holds the child.
///
/// The process is held open first, so its pid stays its own after it ends: Windows hands
/// a pid out again within a second, and a later `kill $!` must find no such process, not
/// whichever one was given the number.
fn report_to_job(child: &sys::process::Child, sink: Option<&std::sync::Mutex<Vec<i32>>>) {
    let (Some(raw), Some(sink)) = (child.id(), sink) else {
        return;
    };
    cash_win32::children::hold(raw);
    #[expect(clippy::cast_possible_wrap)]
    if let Ok(mut pids) = sink.lock() {
        pids.push(raw as i32);
    }
}

/// cash (D17): keeps the process substitutions handed to a program as a path from ending
/// before the program has exited.
///
/// The shell learns that a program in the background has ended only when it next looks,
/// so the end is waited for here, on a thread of its own, when there are substitutions
/// to end. The substitutions end as the program does, as in Bash.
fn hold_substitutions_until_exit(
    child: &sys::process::Child,
    ends: &[std::sync::Arc<crate::interp::SubstitutionEnd>],
) {
    if ends.is_empty() {
        return;
    }
    let Some(process) = child.id().and_then(cash_win32::process::Held::open) else {
        return;
    };
    let ends = ends.to_vec();
    let _ = std::thread::Builder::new()
        .name("cash-procsub-end".into())
        .spawn(move || {
            process.wait();
            drop(ends);
        });
}

pub(crate) fn execute_external_command(
    context: ExecutionContext<'_, impl extensions::ShellExtensions>,
    executable_path: &str,
    process_group_id: Option<i32>,
    argv0_override: Option<&str>,
    args: &[CommandArg],
) -> Result<ExecutionSpawnResult, error::Error> {
    // Filter out the args; we only want strings.
    let cmd_args = args
        .iter()
        .filter_map(|e| {
            if let CommandArg::String(s) = e {
                Some(s)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    warn_about_unix_drive_spellings(&context, cmd_args.as_slice());

    // Before we lose ownership of the open files, figure out if stdin will be a terminal.
    let child_stdin_is_terminal = context
        .try_fd(openfiles::OpenFiles::STDIN_FD)
        .is_some_and(|f| f.is_terminal());

    // Figure out if we should be setting up a new process group.
    let new_pg = matches!(
        context.params.process_group_policy,
        ProcessGroupPolicy::NewProcessGroup
    );

    // Compose the std::process::Command that encapsulates what we want to launch.
    // argv[0] defaults to context.command_name (the user-facing name of the
    // command) unless the caller specified an explicit override.
    let argv0 = argv0_override.unwrap_or(context.command_name.as_str());
    #[allow(unused_mut, reason = "only mutated on unix platforms")]
    let mut cmd = compose_std_command(
        &context,
        executable_path,
        argv0,
        cmd_args.as_slice(),
        false, /* empty environment? */
    )?;

    // Set up process group state.
    if new_pg {
        // Check if we'll be doing terminal control setup (which includes setsid)
        if child_stdin_is_terminal && context.shell.options().external_cmd_leads_session {
            // Don't set process_group(0) - setsid() in pre_exec will handle it
            cmd.lead_session();
        } else {
            // Normal case: create new process group in current session
            cmd.process_group(0);
            if child_stdin_is_terminal {
                cmd.take_foreground();
            }
        }
    } else {
        // We need to join an established process group.
        if let Some(pgid) = process_group_id {
            cmd.process_group(pgid);
        }
    }

    // When tracing is enabled, report.
    tracing::debug!(
        target: trace_categories::COMMANDS,
        "Spawning: cmd='{} {}'",
        cmd.get_program().to_string_lossy().to_string(),
        cmd.get_args()
            .map(|a| a.to_string_lossy().to_string())
            .join(" ")
    );

    let kill_on_drop = context.shell.options().kill_external_commands_on_drop;
    match sys::process::spawn(cmd, kill_on_drop, context.params.background) {
        Ok(child) => {
            register_background_leader(&child, context.params.background);

            // Retrieve the pid.
            #[expect(clippy::cast_possible_wrap)]
            let pid = child.id().map(|id| id as i32);
            let mut actual_pgid = process_group_id;
            if let Some(pid) = &pid {
                if new_pg {
                    actual_pgid = Some(*pid);
                }
            } else {
                tracing::warn!("could not retrieve pid for child process");
            }

            report_to_job(&child, context.params.spawned_pid_sink.as_deref());
            hold_substitutions_until_exit(&child, &context.params.substitution_ends);

            // The pid is now readable, so `&` may return and `$!` will answer.
            if let Some(ready) = &context.params.spawned_pid_ready {
                ready.notify_one();
            }

            Ok(ExecutionSpawnResult::StartedProcess(
                processes::ChildProcess::new(child, pid, actual_pgid),
            ))
        }
        Err(spawn_err) => {
            if context.shell.options().interactive {
                sys::terminal::move_self_to_foreground()?;
            }

            if spawn_err.kind() == std::io::ErrorKind::NotFound {
                if !context.shell.working_dir().exists() {
                    Err(
                        error::ErrorKind::WorkingDirMissing(context.shell.working_dir().to_owned())
                            .into(),
                    )
                } else {
                    Err(error::ErrorKind::CommandNotFound(context.command_name).into())
                }
            } else {
                Err(
                    error::ErrorKind::FailedToExecuteCommand(context.command_name, spawn_err)
                        .into(),
                )
            }
        }
    }
}

async fn execute_builtin_command<SE: extensions::ShellExtensions>(
    builtin: &builtins::Registration<SE>,
    context: ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> Result<ExecutionResult, error::Error> {
    // In POSIX mode, special builtins that return errors are to be treated as fatal.
    let mark_errors_fatal = builtin.special_builtin && context.shell.options().posix_mode;

    // Release a background job's `$!` waiter once this builtin is done. A builtin is the
    // shell's own code, so if it did not publish a pid along the way there will never be
    // one — and waiting for it to finish is what keeps `while true; do :; done &` from
    // hanging the shell.
    //
    // *After* the call, not before: a bundled utility (D48) is dispatched as a builtin
    // that re-enters the binary as a child process, so its pid appears partway through.
    // Releasing on entry would make `sleep 10 & echo $!` empty again.
    let pid_ready = context.params.spawned_pid_ready.clone();
    let outcome = (builtin.execute_func)(context, args).await;
    if let Some(ready) = pid_ready {
        ready.notify_one();
    }

    match outcome {
        Ok(result) => Ok(result),
        Err(e) => {
            // Broken pipe errors should silently return the appropriate exit code
            if let Some(io_err) = e.as_io_error() {
                if io_err.kind() == std::io::ErrorKind::BrokenPipe {
                    return Ok(ExecutionExitCode::from(io_err).into());
                }
            }

            Err(if mark_errors_fatal { e.into_fatal() } else { e })
        }
    }
}

pub(crate) async fn invoke_shell_function(
    function: functions::Registration,
    mut context: ExecutionContext<'_, impl extensions::ShellExtensions>,
    args: &[CommandArg],
) -> Result<ExecutionSpawnResult, error::Error> {
    let ast::FunctionBody(body, redirects) = &function.definition().body;

    // Apply any redirects specified at function definition-time.
    if let Some(redirects) = redirects {
        for redirect in &redirects.0 {
            interp::setup_redirect(context.shell, &mut context.params, redirect).await?;
        }
    }

    let positional_args = args.iter().map(|a| a.to_string());

    // Note that we're going deeper. Once we do this, we need to make sure we don't bail early
    // before "exiting" the function.
    context.shell.enter_function(
        context.command_name.as_str(),
        &function,
        positional_args,
        &context.params,
    )?;

    // A function executes within the current shell process and shares its caller's open files,
    // so the parameters are passed through by shared reference rather than cloned. This prevents
    // direct mutation of the caller's `ExecutionParameters` open-file table, though the function
    // may still change the shell's persistent open files via builtins (e.g. `exec`).
    let result = body.execute(context.shell, &context.params).await;

    // The RETURN trap runs in the function's own context, before its frame is popped, and
    // sees the caller-visible `$?`.
    if result.is_ok() {
        run_return_trap(context.shell, &context.params).await;
    } else {
        context.shell.take_status_before_return();
    }

    // We've come back out, reflect it.
    context.shell.leave_function()?;

    // Get the actual execution result from the body of the function.
    let mut result = result?;

    // Handle control-flow.
    match result.next_control_flow {
        ExecutionControlFlow::BreakLoop { .. } | ExecutionControlFlow::ContinueLoop { .. } => {
            return error::unimp("break or continue returned from function invocation");
        }
        ExecutionControlFlow::ReturnFromFunctionOrScript => {
            // It's now been handled.
            result.next_control_flow = ExecutionControlFlow::Normal;
        }
        _ => {}
    }

    Ok(result.into())
}

/// Fires the `RETURN` trap at the end of a function or sourced script, if one is set and
/// visible in the current scope. As in Bash, the handler sees the `$?` from before
/// any `return` that ended the body, not the returned status.
pub(crate) async fn run_return_trap(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
) {
    let before_return = shell.take_status_before_return();
    if !shell.traps().handles(traps::TrapSignal::Return) {
        return;
    }
    if let Some(status) = before_return {
        shell.set_last_exit_status(status);
    }
    let _ = shell
        .invoke_trap_handler(traps::TrapSignal::Return, params)
        .await;
}

pub(crate) async fn invoke_command_in_subshell_and_get_output(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    s: String,
) -> Result<String, error::Error> {
    // Instantiate a subshell to run the command in.
    let mut subshell = shell.clone();

    // Command substitutions don't inherit errexit by default. Only inherit it when
    // command_subst_inherits_errexit is enabled, otherwise disable errexit in the subshell.
    if !shell.options().command_subst_inherits_errexit {
        subshell.options_mut().exit_on_nonzero_command_exit = false;
    }

    // Get our own set of parameters we can customize and use.
    let mut params = params.clone();
    params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

    // Set up pipe so we can read the output.
    let (reader, writer) = std::io::pipe()?;
    params.set_fd(OpenFiles::STDOUT_FD, writer.into());

    let mut async_reader = sys::async_pipe::AsyncPipeReader::new(reader)?;

    let cmd_join_handle = tokio::spawn(run_substitution_command(subshell, params, s));

    let output_str = async_reader.read_to_string().await?;

    // Now observe the command's completion.
    let run_result = cmd_join_handle.await?;
    let cmd_result = run_result?;

    // Store the status.
    shell.set_last_exit_status(cmd_result.exit_code.into());

    // Note: $_ is naturally isolated from the parent because we cloned the
    // shell to run the substitution.

    Ok(output_str)
}

async fn run_substitution_command(
    mut shell: Shell<impl extensions::ShellExtensions>,
    mut params: ExecutionParameters,
    command: String,
) -> Result<ExecutionResult, error::Error> {
    // Parse the string into a whole shell program.
    let parse_result = shell.parse_string(command);

    // Check for a command that is only an input redirection ("< file").
    // If detected, emulate `cat file` to stdout and return immediately.
    // If we failed to parse, then we'll fall below and handle it there.
    if let Ok(program) = &parse_result {
        if let Some(redir) = try_unwrap_bare_input_redir_program(program) {
            interp::setup_redirect(&mut shell, &mut params, redir).await?;
            std::io::copy(&mut params.stdin(&shell), &mut params.stdout(&shell))?;
            return Ok(ExecutionResult::new(0));
        }
    }

    // TODO(source-info): review this
    let source_info = crate::SourceInfo::from("main");

    // Handle the parse result using default shell behavior.
    shell
        .run_parsed_result(parse_result, &source_info, &params)
        .await
}

// Detects a subshell command that consists solely of a single input redirection
// (e.g., "< file"), returning the IoRedirect when present.
fn try_unwrap_bare_input_redir_program(program: &ast::Program) -> Option<&ast::IoRedirect> {
    // We're looking for exactly one complete command...
    let [complete] = program.complete_commands.as_slice() else {
        return None;
    };

    // ...a single list item...
    let ast::CompoundList(items) = complete;
    let [item] = items.as_slice() else {
        return None;
    };

    // ...with a single pipeline (no && or || chaining)...
    let and_or = &item.0;
    if !and_or.additional.is_empty() {
        return None;
    }

    // ...not negated...
    let pipeline = &and_or.first;
    if pipeline.bang {
        return None;
    }

    // ...with a single command in the pipeline...
    let [ast::Command::Simple(simple_cmd)] = pipeline.seq.as_slice() else {
        return None;
    };

    // ...with no program word/name and no suffix...
    if simple_cmd.word_or_name.is_some() || simple_cmd.suffix.is_some() {
        return None;
    }

    // ...and exactly one prefix containing an I/O redirect...
    let prefix = simple_cmd.prefix.as_ref()?;
    let [ast::CommandPrefixOrSuffixItem::IoRedirect(redir)] = prefix.0.as_slice() else {
        return None;
    };

    // ...that is a file input redirection to a filename, targeting stdin.
    match redir {
        ast::IoRedirect::File(
            fd,
            ast::IoFileRedirectKind::Read,
            ast::IoFileRedirectTarget::Filename(..),
        ) if fd.is_none_or(|fd| fd == openfiles::OpenFiles::STDIN_FD) => Some(redir),
        _ => None,
    }
}
