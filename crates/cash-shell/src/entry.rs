//! Implements the command-line interface for the `brush` shell.

use crate::args::CommandLineArgs;
use crate::args::InputBackendType;
use crate::bundled;
use crate::cashctl::ShellBuilderCashBuiltinExt as _;
use crate::config;
use crate::error_formatter;
use crate::events;
use crate::productinfo;
use cash_builtins::ShellBuilderExt as _;
use clap::CommandFactory;
use std::sync::LazyLock;
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;

#[allow(unused_imports, reason = "only used in some configs")]
use std::io::IsTerminal;

static TRACE_EVENT_CONFIG: LazyLock<Arc<tokio::sync::Mutex<Option<events::TraceEventConfig>>>> =
    LazyLock::new(|| Arc::new(tokio::sync::Mutex::new(None)));

pub(crate) type BrushShellExtensions =
    cash_core::extensions::ShellExtensionsImpl<error_formatter::Formatter>;
type BrushShell = cash_core::Shell<BrushShellExtensions>;

/// The names of the builtins a shell of `set` has, made for the shell's own extensions:
/// made for any other, every builtin is compiled again, megabytes of `cash.exe`.
pub fn default_builtin_names(set: cash_builtins::BuiltinSet) -> std::collections::HashSet<String> {
    cash_builtins::default_builtins::<BrushShellExtensions>(set)
        .into_keys()
        .collect()
}

// WARN: this implementation shadows `clap::Parser::parse_from` one so it must be defined
// after the `use clap::Parser`
impl CommandLineArgs {
    // Work around clap's limitation handling `--` like a regular value
    // TODO(cmdline): We can safely remove this `impl` after the issue is resolved
    // https://github.com/clap-rs/clap/issues/5055
    // This function takes precedence over [`clap::Parser::parse_from`]
    pub(crate) fn try_parse_from(
        itr: impl IntoIterator<Item = String>,
    ) -> Result<Self, clap::Error> {
        let mut args: Vec<String> = itr.into_iter().collect();

        // `-ce` as Bash takes it: `c` anywhere in a cluster of flags, with the command
        // string the next word. clap took the rest of the cluster, `e`, for the command.
        // Only the shell's own options, up to the first word that is none.
        let mut takes_value = false;
        for arg in args.iter_mut().skip(1) {
            if std::mem::take(&mut takes_value) {
                continue;
            }
            if !arg.starts_with(['-', '+']) || arg == "--" {
                break;
            }
            takes_value = matches!(
                arg.as_str(),
                "-o" | "+o" | "-O" | "+O" | "--rcfile" | "--init-file"
            );
            if let Some(moved) = Self::c_flag_last(arg) {
                *arg = moved;
            }
        }

        // In bash, `-c` treats `--` as an option terminator and takes its
        // command string from the first argument *after* `--`. (Other
        // value-taking flags like `-o` and `-O` instead consume `--` as their
        // literal value in bash, rejecting it as an invalid option name.)
        //
        // Remove the `--` so that `-c` naturally consumes the next token as its
        // value via clap. Other value-taking flags are unaffected: for them
        // try_parse_known splits at `--` before clap sees it, so they still
        // produce an error for invocations like `-o --`/`-O --` (via a missing
        // value rather than an invalid option name). In both cases, we
        // intentionally do not treat `--` as an option terminator for those
        // flags.
        if let Some(dd_idx) = args.iter().position(|a| a == "--") {
            if let Some(flag_idx) = dd_idx
                .checked_sub(1)
                .filter(|&i| Self::has_pending_c_flag(&args[i]))
            {
                // Remove the option-terminating `--`.
                args.remove(dd_idx);

                // If the command value (now at dd_idx) is itself `--`, merge it
                // into the flag as an attached value (e.g., "-c" + "--" → "-c--").
                // Clap parses `-c--` as `-c` with value `"--"` (standard POSIX
                // short-option-with-attached-value syntax). This prevents
                // try_parse_known from splitting at it again.
                if args.get(dd_idx).map(String::as_str) == Some("--") {
                    let value = args.remove(dd_idx);
                    args[flag_idx].push_str(&value);
                }
            }
        }

        let (mut this, script_args) = cash_core::builtins::try_parse_known::<Self>(args)?;

        // Collect any args from after `--` (handled by try_parse_known) into
        // script_args, which become positional parameters ($0, $1, ...).
        if let Some(args) = script_args {
            let mut args = args.peekable();

            // Bash stops option parsing at the first non-option word: a script path
            // or the `-c` command string. A `--` seen before either is the option
            // terminator and is dropped (`bash -s -- a`, `bash -- script.sh`); one
            // seen after is an ordinary positional (`bash script.sh -- a`).
            if this.script_args.is_empty() && this.command.is_none() {
                args.next_if(|a| a == "--");
            }
            this.script_args.extend(args);
        }

        Ok(this)
    }

    /// Returns true if `arg` is `-c` or a combined short-flag group ending in
    /// `c` (like `-ec`) where all preceding characters are boolean flags.
    ///
    /// This specifically targets `-c` because it is the only short flag with
    /// special `--` option-terminator behavior in bash. Other value-taking flags
    /// (`-o`, `-O`) consume `--` as their literal value instead.
    ///
    /// Uses clap's argument definitions to validate preceding flags, avoiding
    /// a hardcoded list of boolean flag characters.
    fn has_pending_c_flag(arg: &str) -> bool {
        // Must be a short flag group ending in 'c': "-c", "-ec", "-xec", etc.
        let Some(flags) = arg.strip_prefix('-') else {
            return false;
        };
        let Some(preceding) = flags.strip_suffix('c') else {
            return false;
        };
        // Reject long-option-like args (e.g., "--c").
        if preceding.starts_with('-') {
            return false;
        }

        // For "-c" alone, preceding is empty and the check below is vacuously
        // true. For combined flags like `-ec`, verify all chars before the
        // trailing `c` are boolean flags. If any preceding char takes a value
        // (like `o`), then `c` is consumed as that flag's value, not as `-c`.
        Self::all_boolean_flags(preceding)
    }

    /// A cluster with `c` before its end, `-ce`, with `c` moved to the end, `-ec`, when
    /// the other letters are all boolean flags.
    fn c_flag_last(arg: &str) -> Option<String> {
        let flags = arg.strip_prefix('-')?;
        if flags.starts_with('-') || flags.ends_with('c') || flags.matches('c').count() != 1 {
            return None;
        }
        let others: String = flags.chars().filter(|&ch| ch != 'c').collect();
        Self::all_boolean_flags(&others).then(|| format!("-{others}c"))
    }

    /// Whether every letter of `flags` is a short option that takes no value.
    fn all_boolean_flags(flags: &str) -> bool {
        let cmd = Self::command();
        flags.chars().all(|ch| {
            cmd.get_arguments().any(|a| {
                a.get_short() == Some(ch)
                    && !matches!(
                        a.get_action(),
                        clap::ArgAction::Set | clap::ArgAction::Append
                    )
            })
        })
    }
}

/// The `set` letters a shell takes as options, and the `set -o` names they stand for.
const SET_LETTERS: [(char, &str); 19] = [
    ('a', "allexport"),
    ('b', "notify"),
    ('e', "errexit"),
    ('f', "noglob"),
    ('h', "hashall"),
    ('k', "keyword"),
    ('m', "monitor"),
    ('n', "noexec"),
    ('p', "privileged"),
    ('t', "onecmd"),
    ('u', "nounset"),
    ('v', "verbose"),
    ('x', "xtrace"),
    ('B', "braceexpand"),
    ('C', "noclobber"),
    ('E', "errtrace"),
    ('H', "histexpand"),
    ('P', "physical"),
    ('T', "functrace"),
];

/// The command line with the shell's own options as Bash takes them, made into what clap
/// reads: a `set` letter in a `-` cluster that clap has no flag for becomes `-o NAME`,
/// each letter of a `+` cluster `+o NAME`, and `+o` and `+O` clap's `--+o` and `--+O`.
/// Only the options before the first word that is none: `+O` was taken anywhere, a
/// script's operand too, and `-B`, `+B` and `+o NAME` were refused.
fn bash_style_options(args: &[String]) -> Vec<String> {
    let name_of = |letter: char| {
        SET_LETTERS
            .iter()
            .find(|(known, _)| *known == letter)
            .map(|(_, name)| *name)
    };
    let mut out: Vec<String> = args.iter().take(1).cloned().collect();
    let mut words = args.iter().skip(1);
    while let Some(word) = words.next() {
        match word.as_str() {
            "-o" | "-O" | "--rcfile" | "--init-file" | "+o" | "+O" => {
                let spelled = if word.starts_with('+') {
                    format!("--{word}")
                } else {
                    word.clone()
                };
                out.push(spelled);
                out.extend(words.next().cloned());
                continue;
            }
            _ => {}
        }
        if let Some(letters) = word.strip_prefix('+')
            && !letters.is_empty()
            && let Some(names) = letters.chars().map(name_of).collect::<Option<Vec<_>>>()
        {
            for name in names {
                out.extend(["--+o".to_owned(), name.to_owned()]);
            }
            continue;
        }
        if let Some(letters) = word.strip_prefix('-')
            && !letters.is_empty()
            && !letters.starts_with('-')
        {
            let own = CommandLineArgs::command();
            let clap_flag =
                |letter: char| own.get_arguments().any(|a| a.get_short() == Some(letter));
            if letters
                .chars()
                .all(|letter| clap_flag(letter) || name_of(letter).is_some())
            {
                let kept: String = letters
                    .chars()
                    .filter(|&letter| clap_flag(letter))
                    .collect();
                for letter in letters.chars().filter(|&letter| !clap_flag(letter)) {
                    out.extend([
                        "-o".to_owned(),
                        name_of(letter).unwrap_or_default().to_owned(),
                    ]);
                }
                if !kept.is_empty() {
                    out.push(format!("-{kept}"));
                }
                continue;
            }
        }
        out.push(word.clone());
        // The options end at `--`, `-`, and the first word that is none.
        if word == "--" || word == "-" || !word.starts_with(['-', '+']) {
            out.extend(words.cloned());
            break;
        }
    }
    out
}

/// Whether each name given to `-o`, `+o`, `-O` and `+O` is an option's, said as Bash says
/// one that is not, `shell` being the shell's name.
fn check_option_names(args: &CommandLineArgs, shell: &str) -> Result<(), String> {
    use cash_core::namedoptions::{ShellOptionKind, options};

    for name in args.enabled_options.iter().chain(&args.disabled_options) {
        if options(ShellOptionKind::SetO).get(name.as_str()).is_none() {
            return Err(format!(
                "{shell}: line 0: {shell}: {name}: invalid option name"
            ));
        }
    }
    for name in args
        .enabled_shopt_options
        .iter()
        .chain(&args.disabled_shopt_options)
    {
        if options(ShellOptionKind::Shopt).get(name.as_str()).is_none() {
            return Err(format!(
                "{shell}: line 0: {name}: invalid shell option name"
            ));
        }
    }
    Ok(())
}

/// Main entry point for the `brush` shell.
pub fn run() {
    run_with_args(std::env::args().collect());
}

/// [`run`], with the command line given rather than taken from the process: how cash
/// started under a linked tool's name (`ls.exe`, `cash --link-tools`) runs as that tool.
pub fn run_with_args(args: Vec<String>) {
    //
    // Install the bundled-command registry so it's available both for
    // bundled dispatch (handled next) and for builtin shim registration
    // during shell construction. With no bundled-providing features enabled
    // the registry is empty and both code paths become no-ops.
    //
    bundled::install_default_providers();

    //
    // If we were invoked as `brush <DISPATCH_FLAG> <name> [args...]`, run the
    // bundled command and exit before doing any shell setup. This is the
    // hot path for in-binary coreutils invocations.
    //
    if let Some(code) = bundled::maybe_dispatch() {
        std::process::exit(code);
    }
    // From here on this process is the shell, whatever its executable is named.
    cash_win32::resolve::note_running_as_shell();

    //
    // Install panic handlers to clean up on panic.
    //
    install_panic_handlers();

    //
    // Parse args.
    //
    let parsed_args = match CommandLineArgs::try_parse_from(bash_style_options(&args)) {
        Ok(parsed_args) => parsed_args,
        Err(e) => {
            let _ = e.print();

            // Check for whether this is something we'd truly consider fatal. clap returns
            // errors for `--help`, `--version`, etc.
            let exit_code = match e.kind() {
                clap::error::ErrorKind::DisplayVersion => 0,
                clap::error::ErrorKind::DisplayHelp => 0,
                _ => 2,
            };

            std::process::exit(exit_code);
        }
    };

    // An option name `-o` or `-O` does not know ends the shell with status 2, said as Bash
    // says it; it was passed over.
    let shell_name = args
        .first()
        .map_or_else(|| "cash".to_owned(), |name| name.replace('\\', "/"));
    if let Err(said) = check_option_names(&parsed_args, &shell_name) {
        eprintln!("{said}");
        std::process::exit(2);
    }

    //
    // Run, on a thread with the stack shell code needs: the main thread has only what
    // the executable's header reserves. `run_shell` ends the process itself, so the
    // runtime is never dropped (which would wait for every blocking thread).
    //
    let runner = std::thread::Builder::new()
        .name("cash-shell".into())
        .stack_size(cash_core::SHELL_THREAD_STACK_SIZE)
        .spawn(move || run_shell(&args, parsed_args));
    match runner.map(std::thread::JoinHandle::join) {
        Ok(Ok(never)) => match never {},
        Ok(Err(_)) => std::process::exit(1),
        Err(err) => {
            eprintln!("cash: failed to start the shell's thread: {err}");
            std::process::exit(1);
        }
    }
}

/// Runs the shell to its end and exits the process with its status.
fn run_shell(args: &[String], parsed_args: CommandLineArgs) -> std::convert::Infallible {
    let mut builder = tokio::runtime::Builder::new_multi_thread();

    // Each subshell running at once is a thread of the blocking pool (D70), and so is a
    // builtin's pipeline stage, which the limit does not count: the pool is made twice as
    // large as the limit, and never smaller than tokio's 512.
    let blocking_threads = (cash_core::jobs::max_concurrent_subshells() * 2).max(512);
    let Ok(runtime) = builder
        .enable_all()
        .thread_stack_size(cash_core::SHELL_THREAD_STACK_SIZE)
        .max_blocking_threads(blocking_threads)
        .build()
    else {
        tracing::error!("error: failed to create Tokio runtime");
        std::process::exit(1);
    };

    // Keep startup, script, and `-c` panics from becoming Windows crash dialogs or
    // abnormal process termination. Interactive command panics are recovered inside the
    // prompt loop so that session can continue; this outer boundary covers everything else.
    let Ok(result) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.block_on(run_async(args, parsed_args))
    })) else {
        // The panic hook has said what the error was.
        eprintln!("cash: stopped safely after the internal error");
        std::process::exit(1);
    };

    let exit_code = match result {
        Ok(code) => code,
        Err(err) => {
            tracing::error!("error: {err:#}");
            1
        }
    };

    // D17: what a `>(...)` still has to write, it writes before the shell is gone.
    cash_core::finish_output_substitutions();

    // D6: GUI applications cash started (`code .`) keep running after it; the console
    // programs it started do not.
    cash_win32::session::release_at_exit();

    std::process::exit(i32::from(exit_code));
}

/// Installs the panic hook: one line saying where the internal error happened.
///
/// Every panic of the shell's is caught, at the prompt or around the whole run, and
/// the catcher says what happens next, so the hook does not call it a crash. It wrote
/// "cash had a problem and crashed" and a report file to `%TEMP%` for each one through
/// `human-panic`, the recovered ones too (BIN-14).
fn install_panic_handlers() {
    let terminal = std::io::stdout().is_terminal();
    std::panic::set_hook(Box::new(move |panic_info| {
        if terminal {
            // Best-effort attempt to reset the terminal to defaults.
            let _ = try_reset_terminal_to_defaults();
        }
        let location = panic_info.location().map(|l| (l.file(), l.line()));
        let message = panic_message(panic_info.payload());
        eprintln!("{}", panic_report(message, location));
        if std::env::var_os("RUST_BACKTRACE").is_some_and(|v| v != "0") {
            eprintln!("{}", std::backtrace::Backtrace::force_capture());
        }
    }));
}

/// A panic's message, as `panic!` gave it.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic")
}

/// The line the panic hook prints.
fn panic_report(message: &str, location: Option<(&str, u32)>) -> String {
    let at = location.map_or_else(String::new, |(file, line)| format!(" at {file}:{line}"));
    format!(
        "cash: internal error{at}: {message}\n\
         cash: please report it at https://github.com/tomcoolpxl/cash/issues/new"
    )
}

/// cash: syntax highlighting is on by default, where upstream kept it behind the
/// `experimental` feature. Whether the command word exists comes from a background
/// listing of `PATH` (`cash_core::pathindex`), so highlighting costs no file-system
/// probes per keystroke.
pub(crate) const DEFAULT_ENABLE_HIGHLIGHTING: bool = true;

/// Run the brush shell. Returns the exit code.
///
/// # Arguments
///
/// * `cli_args` - The command-line arguments to the shell, in string form.
/// * `args` - The already-parsed command-line arguments.
#[doc(hidden)]
async fn run_async(
    cli_args: &[String],
    args: CommandLineArgs,
) -> Result<u8, cash_interactive::ShellError> {
    // Initializing tracing.
    let mut event_config = TRACE_EVENT_CONFIG.lock().await;
    *event_config = Some(events::TraceEventConfig::init(
        &args.enabled_debug_events,
        &args.disabled_events,
    ));
    drop(event_config);

    // Load configuration file.
    let file_config = config::load_config(args.no_config, args.config_file.as_deref())
        .into_config_or_log()
        .map_err(|e| cash_interactive::ShellError::IoError(std::io::Error::other(e)))?;

    // Instantiate an appropriately configured shell and wrap it in an `Arc`. Note that we do
    // *not* run any code in the shell yet. We'll delay loading profiles and such until after
    // we've set up everything else (in `run_in_shell`).
    let shell: BrushShell = instantiate_shell(&args, cli_args).await?;
    let shell = Arc::new(Mutex::new(shell));

    // Run with the selected input backend. Each branch instantiates the concrete
    // backend type and calls `run_in_shell`, preserving static dispatch.
    let default_backend = get_default_input_backend_type(&args);
    let selected_backend = args.input_backend.unwrap_or(default_backend);

    // Build UI options by merging config file with CLI args.
    #[allow(unused_variables, reason = "not used when no backend features enabled")]
    let ui_options = file_config.to_ui_options(&args);

    let result = match selected_backend {
        #[cfg(feature = "reedline")]
        InputBackendType::Reedline => {
            let mut input_backend =
                cash_interactive::ReedlineInputBackend::new(&ui_options, &shell)?;
            run_in_shell(&shell, args, &mut input_backend, &ui_options).await
        }
        #[cfg(not(feature = "reedline"))]
        InputBackendType::Reedline => Err(cash_interactive::ShellError::InputBackendNotSupported),

        #[cfg(feature = "basic")]
        InputBackendType::Basic => {
            let mut input_backend = cash_interactive::BasicInputBackend;
            run_in_shell(&shell, args, &mut input_backend, &ui_options).await
        }
        #[cfg(not(feature = "basic"))]
        InputBackendType::Basic => Err(cash_interactive::ShellError::InputBackendNotSupported),

        #[cfg(feature = "minimal")]
        InputBackendType::Minimal => {
            let mut input_backend = cash_interactive::MinimalInputBackend;
            run_in_shell(&shell, args, &mut input_backend, &ui_options).await
        }
        #[cfg(not(feature = "minimal"))]
        InputBackendType::Minimal => Err(cash_interactive::ShellError::InputBackendNotSupported),
    };

    // Display any error that percolated up.
    let exit_code = match result {
        Ok(code) => code,
        Err(cash_interactive::ShellError::ShellError(e)) => {
            let shell = shell.lock().await;
            let mut stderr = shell.stderr();
            let _ = shell.display_error(&mut stderr, &e);
            drop(shell);
            1
        }
        Err(err) => {
            tracing::error!("error: {err:#}");
            1
        }
    };

    Ok(exit_code)
}

/// Runs the shell according to the provided command-line arguments.
/// Also responsible for loading profiles and rc files as appropriate.
///
/// # Arguments
///
/// * `shell_ref` - A reference to the shell to run.
/// * `args` - The parsed command-line arguments.
/// * `input_backend` - The input backend to use.
/// * `ui_options` - The user interface options to use.
async fn run_in_shell(
    shell_ref: &cash_interactive::ShellRef<impl cash_core::ShellExtensions>,
    args: CommandLineArgs,
    input_backend: &mut impl cash_interactive::InputBackend,
    ui_options: &cash_interactive::UIOptions,
) -> Result<u8, cash_interactive::ShellError> {
    let read_commands_from_stdin = args.will_read_commands_from_stdin();
    let interactive_options: cash_interactive::InteractiveOptions = ui_options.into();

    // Before config files run, so they (and what they source) observe it -- and only for a
    // shell that goes on to read commands interactively below, since only such a shell
    // dispatches hooks.
    if read_commands_from_stdin {
        cash_interactive::init_zsh_style_hooks(&mut *shell_ref.lock().await, &interactive_options)?;
    }

    // `--pretty-print` prints the script and runs nothing, startup files neither, as in
    // Bash; a `-c` string it does not print. It was refused.
    if args.pretty_print {
        return Ok(pretty_print(shell_ref, &args, read_commands_from_stdin).await);
    }

    // Load profile and rc files as appropriate.
    initialize_shell(shell_ref, &args).await?;

    // If a command was specified via -c, then run that command and then exit.
    if let Some(command) = args.command {
        shell_ref.lock().await.run_dash_c_command(command).await?;

    // Otherwise read commands interactively: -s was given (positional args become parameters;
    // a script named among them is *not* run), or nothing to run was specified.
    } else if read_commands_from_stdin {
        // A portable cash offers its setup once, after the startup files and before the
        // first prompt.
        crate::portable_offer::run(&*shell_ref.lock().await);
        cash_interactive::InteractiveShell::new(shell_ref, input_backend, &interactive_options)?
            .run_interactively()
            .await?;

    // Otherwise a script path was given; run it.
    } else {
        let mut shell = shell_ref.lock().await;
        let shell_name = shell.current_shell_name().map(|name| name.into_owned());
        let run = shell
            .run_script(
                Path::new(&args.script_args[0]),
                args.script_args.iter().skip(1),
            )
            .await;
        // A script that cannot be run exits with Bash's statuses: 127 when it does not
        // exist, 126 when it cannot be read or looks binary. It is said after the shell's
        // name, as Bash says it; it was said after the script's, with `line 1`.
        if let Err(err) = run {
            match (err.kind(), &shell_name) {
                (
                    cash_core::ErrorKind::FailedSourcingFile(..)
                    | cash_core::ErrorKind::CannotExecuteBinaryFile(_),
                    Some(name),
                ) => {
                    let mut stderr = shell.stderr();
                    let _ = std::io::Write::write_fmt(&mut stderr, format_args!("{name}: {err}\n"));
                }
                _ => {
                    let _ = shell.display_error(&mut shell.stderr(), &err);
                }
            }
            drop(shell);
            let status = match err.kind() {
                cash_core::ErrorKind::FailedSourcingFile(_, io)
                    if io.kind() == std::io::ErrorKind::NotFound =>
                {
                    cash_core::ExecutionExitCode::NotFound
                }
                cash_core::ErrorKind::FailedSourcingFile(..) => {
                    cash_core::ExecutionExitCode::CannotExecute
                }
                _ => cash_core::ExecutionExitCode::from(&err),
            };
            return Ok(status.into());
        }
        // What a script kept with `set -o history` goes to `HISTFILE` as it ends, as
        // Bash's `exit_shell` saves it: after the `EXIT` trap, to the file named then.
        let _ = shell.save_history();
        drop(shell);
    }

    // When a login shell exits, bash sources ~/.bash_logout if present.
    if shell_ref.lock().await.options().login_shell {
        let _ = shell_ref.lock().await.load_logout_scripts().await;
    }

    // Make sure to return the last result observed in the shell.
    let result = shell_ref.lock().await.last_exit_status();

    Ok(result)
}

/// `--pretty-print`: the script, or standard input's, printed as cash reads it, and its
/// status; nothing for a `-c` string. A script that cannot be read is said as running it
/// says it, status 127.
async fn pretty_print(
    shell_ref: &cash_interactive::ShellRef<impl cash_core::ShellExtensions>,
    args: &CommandLineArgs,
    from_stdin: bool,
) -> u8 {
    use std::io::Read as _;

    if args.command.is_some() {
        return 0;
    }
    let shell_name = shell_ref
        .lock()
        .await
        .current_shell_name()
        .unwrap_or_default()
        .into_owned();
    let (text, source) = if from_stdin {
        let mut text = Vec::new();
        let _ = cash_win32::stdio::RawStdin.read_to_end(&mut text);
        (text, shell_name)
    } else {
        let script = &args.script_args[0];
        let path = shell_ref.lock().await.absolute_path(Path::new(script));
        match std::fs::read(path) {
            Ok(text) => (text, script.clone()),
            Err(err) => {
                let err = cash_core::Error::from(cash_core::ErrorKind::FailedSourcingFile(
                    script.into(),
                    err,
                ));
                let mut stderr = shell_ref.lock().await.stderr();
                let _ =
                    std::io::Write::write_fmt(&mut stderr, format_args!("{shell_name}: {err}\n"));
                return 127;
            }
        }
    };
    let result = shell_ref
        .lock()
        .await
        .pretty_print(&text, &cash_core::SourceInfo::from(source.as_str()));
    u8::from(result.exit_code)
}

/// Initializes a shell by loading profile and rc files as appropriate.
///
/// # Arguments
///
/// * `shell_ref` - A reference to the shell to initialize.
/// * `args` - The parsed command-line arguments.
async fn initialize_shell(
    shell_ref: &cash_interactive::ShellRef<impl cash_core::ShellExtensions>,
    args: &CommandLineArgs,
) -> Result<(), cash_interactive::ShellError> {
    // Compute desired profile-loading behavior.
    let profile = if args.no_profile {
        cash_core::ProfileLoadBehavior::Skip
    } else {
        cash_core::ProfileLoadBehavior::LoadDefault
    };

    // Compute desired rc-loading behavior.
    let rc = if args.no_rc {
        cash_core::RcLoadBehavior::Skip
    } else if let Some(rc_file) = &args.rc_file {
        cash_core::RcLoadBehavior::LoadCustom(rc_file.clone())
    } else {
        cash_core::RcLoadBehavior::LoadDefault
    };

    shell_ref.lock().await.load_config(&profile, &rc).await?;

    Ok(())
}

/// Instantiates a shell from command-line arguments. Does *not* run any code in the shell.
///
/// # Arguments
///
/// * `args` - The parsed command-line arguments.
/// * `cli_args` - The raw command-line arguments.
async fn instantiate_shell(
    args: &CommandLineArgs,
    cli_args: &[String],
) -> Result<BrushShell, cash_interactive::ShellError> {
    #[cfg(feature = "experimental-load")]
    let mut shell = if let Some(load_file) = &args.load_file {
        instantiate_shell_from_file(load_file.as_path())?
    } else {
        instantiate_shell_from_args(args, cli_args).await?
    };

    #[cfg(not(feature = "experimental-load"))]
    let mut shell = instantiate_shell_from_args(args, cli_args).await?;

    // Register shims for any bundled commands in the installed registry.
    // Done here (not inside the inner instantiators) so both paths are
    // covered from a single site.
    bundled::register_shims(&mut shell);

    Ok(shell)
}

#[cfg(feature = "experimental-load")]
fn instantiate_shell_from_file(
    file_path: &Path,
) -> Result<BrushShell, cash_interactive::ShellError> {
    let mut shell: BrushShell = serde_json::from_reader(std::fs::File::open(file_path)?)
        .map_err(|e| cash_interactive::ShellError::IoError(std::io::Error::other(e)))?;

    // NOTE: We need to manually register builtins because we can't serialize/deserialize them.
    // TODO(serde): we should consider whether we could/should at least track *which* are enabled.
    let builtin_set = if shell.options().sh_mode {
        cash_builtins::BuiltinSet::ShMode
    } else {
        cash_builtins::BuiltinSet::BashMode
    };

    let builtins = cash_builtins::default_builtins(builtin_set);

    for (builtin_name, builtin) in builtins {
        shell.register_builtin(&builtin_name, builtin);
    }

    Ok(shell)
}

/// Instantiates a shell from command-line arguments. Does *not* run any code in the shell.
///
/// # Arguments
///
/// * `args` - The parsed command-line arguments.
/// * `cli_args` - The raw command-line arguments.
async fn instantiate_shell_from_args(
    args: &CommandLineArgs,
    cli_args: &[String],
) -> Result<BrushShell, cash_interactive::ShellError> {
    // Compute login flag.
    let login = args.login || cli_args.first().is_some_and(|argv0| argv0.starts_with('-'));

    // Compute shell name.
    let shell_name = if args.command.is_some() && !args.script_args.is_empty() {
        Some(args.script_args[0].clone())
    } else if !cli_args.is_empty() {
        Some(cli_args[0].clone())
    } else if args.sh_mode {
        // Simulate having been run as "sh".
        Some(String::from("sh"))
    } else {
        None
    };

    // cash (D3): $0 is a path when cash was invoked by one, and the OS hands it over with
    // backslashes. A script doing `dirname "$0"` or `case "$0" in */foo) …` then sees a
    // spelling D3 says cash never renders.
    let shell_name = shell_name.map(|name| {
        if name.contains('\\') {
            cash_win32::path::render(std::path::Path::new(&name))
        } else {
            name
        }
    });

    // Compute positional shell arguments.
    let shell_args = if args.command.is_some() {
        Some(args.script_args.iter().skip(1).cloned().collect())
    } else if args.read_commands_from_stdin {
        Some(args.script_args.clone())
    } else {
        None
    };

    // Commands are read from stdin if -s was provided, or if no command was specified (either via
    // -c or as a positional argument).
    let read_commands_from_stdin = args.will_read_commands_from_stdin();

    let builtin_set = if args.sh_mode {
        cash_builtins::BuiltinSet::ShMode
    } else {
        cash_builtins::BuiltinSet::BashMode
    };

    // The descriptors above 2 the cash that started this one passed to it (D26).
    let fds: std::collections::HashMap<_, _> =
        cash_core::openfiles::OpenFile::received_from_parent()
            .into_iter()
            .collect();

    // cash (D13): an interactive shell takes Ctrl-C back from a parent that ignored it, so
    // that the programs it runs can be interrupted.
    if args.is_interactive() {
        cash_win32::console::enable_ctrl_c();
    }
    // A Ctrl-C while the shell runs commands of its own is kept for the shell to act on,
    // rather than ending it where it stands. Installed before anything else listens.
    cash_win32::console::keep_interrupts();

    // Set up the shell builder with the requested options.
    // NOTE: We skip loading profile and rc files here; that will be handled later after we've
    // fully instantiated everything we want set before running any code.
    let shell = cash_core::Shell::builder_with_extensions::<BrushShellExtensions>()
        .disable_options(args.disabled_options.clone())
        .disable_shopt_options(args.disabled_shopt_options.clone())
        .disallow_overwriting_regular_files_via_output_redirection(
            args.disallow_overwriting_regular_files_via_output_redirection,
        )
        .enable_options(args.enabled_options.clone())
        .enable_shopt_options(args.enabled_shopt_options.clone())
        .do_not_execute_commands(args.do_not_execute_commands)
        .exit_after_one_command(args.exit_after_one_command)
        .login(login)
        .interactive(args.is_interactive())
        .command_string_mode(args.command.is_some())
        .no_editing(args.no_editing)
        .profile(cash_core::ProfileLoadBehavior::Skip)
        .rc(cash_core::RcLoadBehavior::Skip)
        .do_not_inherit_env(args.do_not_inherit_env)
        .fds(fds)
        .maybe_shell_args(shell_args)
        .posix(args.posix || args.sh_mode)
        .print_commands_and_arguments(args.print_commands_and_arguments)
        .read_commands_from_stdin(read_commands_from_stdin)
        .maybe_shell_name(shell_name)
        .shell_product_display_str(productinfo::get_product_display_str())
        .sh_mode(args.sh_mode)
        .treat_unset_variables_as_error(args.treat_unset_variables_as_error)
        .exit_on_nonzero_command_exit(args.exit_on_nonzero_command_exit)
        .disable_pathname_expansion(args.disable_pathname_expansion)
        .verbose(args.verbose)
        .error_formatter(new_error_behavior(args))
        .shell_version(env!("CARGO_PKG_VERSION").to_string());

    // Add builtins.
    let shell = shell.default_builtins(builtin_set).cash_builtins();

    // Build the shell.
    let mut shell = shell.build().await?;

    // Make adjustments.
    if let Some(xtrace_file_path) = &args.xtrace_file_path {
        enable_xtrace_to_file(&mut shell, xtrace_file_path)?;
    }

    Ok(shell)
}

fn enable_xtrace_to_file(
    shell: &mut cash_core::Shell<impl cash_core::ShellExtensions>,
    file_path: &Path,
) -> Result<(), cash_interactive::ShellError> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(file_path)
        .map_err(|e| {
            cash_interactive::ShellError::FailedToCreateXtraceFile(file_path.to_path_buf(), e)
        })?;

    let file = cash_core::openfiles::OpenFile::from(file);
    let file_fd = shell.open_files_mut().add(file)?;

    shell.options_mut().print_commands_and_arguments = true;
    shell.set_env_global(
        "BASH_XTRACEFD",
        cash_core::ShellVariable::new(file_fd.to_string()),
    )?;

    Ok(())
}

const fn new_error_behavior(args: &CommandLineArgs) -> error_formatter::Formatter {
    error_formatter::Formatter {
        use_color: !args.disable_color,
    }
}

fn get_default_input_backend_type(args: &CommandLineArgs) -> InputBackendType {
    // If stdin isn't a terminal, then `reedline` doesn't do the right thing
    // (reference: https://github.com/nushell/reedline/issues/509). Switch to
    // the minimal input backend instead for that scenario.
    if std::io::stdin().is_terminal() && args.will_read_commands_from_stdin() {
        InputBackendType::Reedline
    } else {
        InputBackendType::Minimal
    }
}

pub(crate) fn get_event_config() -> Arc<tokio::sync::Mutex<Option<events::TraceEventConfig>>> {
    TRACE_EVENT_CONFIG.clone()
}

fn try_reset_terminal_to_defaults() -> Result<(), std::io::Error> {
    // Reset the console.
    let exec_result = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::terminal::EnableLineWrap,
        crossterm::style::ResetColor,
        crossterm::event::DisableMouseCapture,
        crossterm::event::DisableBracketedPaste,
        crossterm::cursor::Show,
        crossterm::cursor::MoveToNextLine(1),
    );

    let raw_result = crossterm::terminal::disable_raw_mode();

    exec_result?;
    raw_result?;

    Ok(())
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use anyhow::Result;
    use pretty_assertions::{assert_eq, assert_matches};

    /// A panic is an internal error with its place, never a crash: each is caught (BIN-14).
    #[test]
    fn a_panic_is_reported_as_an_internal_error_with_its_place() {
        let report = panic_report("index out of bounds", Some(("src/x.rs", 7)));
        assert_eq!(
            report.lines().next(),
            Some("cash: internal error at src/x.rs:7: index out of bounds")
        );
        assert!(!report.contains("crash"), "{report}");
        assert_eq!(
            panic_report("boom", None).lines().next(),
            Some("cash: internal error: boom")
        );
        let owned: Box<dyn std::any::Any + Send> = Box::new(String::from("owned"));
        assert_eq!(panic_message(owned.as_ref()), "owned");
    }

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_empty_args() -> Result<()> {
        let parsed_args = CommandLineArgs::try_parse_from(args(&["cash"]))?;
        assert_matches!(parsed_args.script_args.as_slice(), []);
        Ok(())
    }

    #[test]
    fn parse_script_and_args() -> Result<()> {
        let parsed_args =
            CommandLineArgs::try_parse_from(args(&["cash", "some-script", "-x", "1", "--option"]))?;
        assert_eq!(
            parsed_args.script_args,
            ["some-script", "-x", "1", "--option"]
        );
        Ok(())
    }

    #[test]
    fn c_anywhere_in_a_cluster_takes_the_next_word_as_the_command() -> Result<()> {
        // `-ce` took `e` for the command string; Bash runs the next word under `-e`.
        let parsed = CommandLineArgs::try_parse_from(args(&["cash", "-ce", "false", "zero"]))?;
        assert_eq!(parsed.command.as_deref(), Some("false"));
        assert!(parsed.exit_on_nonzero_command_exit);
        assert_eq!(parsed.script_args, ["zero"]);
        let parsed =
            CommandLineArgs::try_parse_from(args(&["cash", "-o", "posix", "-xce", "true"]))?;
        assert_eq!(parsed.command.as_deref(), Some("true"));
        let parsed = CommandLineArgs::try_parse_from(args(&["cash", "script", "-ce"]))?;
        assert_eq!(parsed.script_args, ["script", "-ce"]);
        Ok(())
    }

    #[test]
    fn parse_script_and_args_with_double_dash_in_script_args() -> Result<()> {
        let parsed_args = CommandLineArgs::try_parse_from(args(&["cash", "some-script", "--"]))?;
        assert_eq!(parsed_args.script_args, ["some-script", "--"]);
        Ok(())
    }

    #[test]
    fn parse_unknown_args() {
        let result = CommandLineArgs::try_parse_from(args(&["cash", "--unknown-option"]));
        assert!(result.is_err());
    }

    #[test]
    fn parse_c_with_double_dash_separator() -> Result<()> {
        let parsed_args =
            CommandLineArgs::try_parse_from(args(&["cash", "-c", "--", "echo hello", "arg0"]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert_eq!(parsed_args.script_args, ["arg0"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_double_dash_no_command() {
        assert!(CommandLineArgs::try_parse_from(args(&["cash", "-c", "--"])).is_err());
    }

    #[test]
    fn parse_c_with_double_dash_command_is_double_dash() -> Result<()> {
        let parsed_args =
            CommandLineArgs::try_parse_from(args(&["cash", "-c", "--", "--", "echo", "hi"]))?;
        assert_eq!(parsed_args.command, Some("--".to_string()));
        assert_eq!(parsed_args.script_args, ["echo", "hi"]);
        Ok(())
    }

    #[test]
    fn parse_ec_with_double_dash_separator() -> Result<()> {
        let parsed_args =
            CommandLineArgs::try_parse_from(args(&["cash", "-ec", "--", "echo hello", "arg0"]))?;
        assert_eq!(parsed_args.command, Some("echo hello".to_string()));
        assert!(parsed_args.exit_on_nonzero_command_exit);
        assert_eq!(parsed_args.script_args, ["arg0"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_value_before_double_dash_unchanged() -> Result<()> {
        let parsed_args =
            CommandLineArgs::try_parse_from(args(&["cash", "-c", "echo hi", "--", "arg0"]))?;
        assert_eq!(parsed_args.command, Some("echo hi".to_string()));
        assert_eq!(parsed_args.script_args, ["--", "arg0"]);
        Ok(())
    }

    #[test]
    fn parse_o_with_double_dash_is_not_transformed() {
        // Unlike -c, bash's -o consumes -- as its literal value (invalid option
        // name), not as an option terminator. Verify we don't transform it.
        let result = CommandLineArgs::try_parse_from(args(&["cash", "-o", "--"]));
        // Here, try_parse_from / try_parse_known splits at --, so -o ends up
        // without a value and parsing correctly fails. The key assertion is
        // that we MUST NOT reinterpret -- as an option terminator for -o and
        // then take any later argument as its value.
        assert!(result.is_err());
    }

    #[test]
    fn parse_oc_not_treated_as_pending_c() -> Result<()> {
        // -oc means -o with value "c", not -o flag + -c flag. The --
        // should NOT be treated as an option terminator for -c.
        let parsed_args = CommandLineArgs::try_parse_from(args(&["cash", "-oc", "--", "echo"]))?;
        // -o consumed "c" as its value; -- ended the options; no -c command.
        assert!(parsed_args.command.is_none());
        assert_eq!(parsed_args.script_args, ["echo"]);
        Ok(())
    }

    #[test]
    fn parse_bool_flag_before_double_dash_not_transformed() -> Result<()> {
        // -e is a boolean flag, not -c. The -- ends the options, so
        // everything after it is positional (including -c).
        let parsed_args =
            CommandLineArgs::try_parse_from(args(&["cash", "-e", "--", "-c", "echo"]))?;
        assert!(parsed_args.command.is_none());
        assert!(parsed_args.exit_on_nonzero_command_exit);
        assert_eq!(parsed_args.script_args, ["-c", "echo"]);
        Ok(())
    }

    #[test]
    fn parse_s_with_double_dash_separator() -> Result<()> {
        let parsed_args = CommandLineArgs::try_parse_from(args(&["cash", "-s", "--", "--", "a"]))?;
        assert!(parsed_args.read_commands_from_stdin);
        assert_eq!(parsed_args.script_args, ["--", "a"]);
        Ok(())
    }

    #[test]
    fn parse_double_dash_before_script() -> Result<()> {
        let parsed_args =
            CommandLineArgs::try_parse_from(args(&["cash", "--", "script.sh", "--", "x"]))?;
        assert_eq!(parsed_args.script_args, ["script.sh", "--", "x"]);
        Ok(())
    }

    #[test]
    fn parse_c_with_double_dash_and_later_double_dash() -> Result<()> {
        // After removing the first --, -c gets "echo". The second -- is
        // handled by try_parse_known and appears in script_args.
        let parsed_args =
            CommandLineArgs::try_parse_from(args(&["cash", "-c", "--", "echo", "--", "more"]))?;
        assert_eq!(parsed_args.command, Some("echo".to_string()));
        assert_eq!(parsed_args.script_args, ["--", "more"]);
        Ok(())
    }

    #[test]
    fn has_pending_c_flag_edge_cases() {
        // Direct tests for the detection function.
        assert!(CommandLineArgs::has_pending_c_flag("-c"));
        assert!(CommandLineArgs::has_pending_c_flag("-ec"));
        assert!(!CommandLineArgs::has_pending_c_flag("-C")); // uppercase, different flag
        assert!(!CommandLineArgs::has_pending_c_flag("-oc")); // -o takes a value
        assert!(!CommandLineArgs::has_pending_c_flag("--c")); // long-option-like
        assert!(!CommandLineArgs::has_pending_c_flag("-")); // bare dash
        assert!(!CommandLineArgs::has_pending_c_flag("c")); // no leading dash
        assert!(!CommandLineArgs::has_pending_c_flag("")); // empty
    }
}
