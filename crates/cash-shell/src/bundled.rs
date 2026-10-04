//! Bundled commands: utilities that ship inside the cash binary.
//!
//! Utilities are shipped busybox-style (one binary, many names) but execute
//! as a subprocess of cash so that shell redirections, pipes, and
//! process-group state are honored by code that reads/writes the host
//! process's standard fds (e.g., uutils crates).
//!
//! ## Protocol
//!
//! The cash binary recognizes a hidden first-position argument
//! [`DISPATCH_FLAG`] followed by `<NAME> [ARGS...]`. When present, cash
//! dispatches early in `main()` to the registered function for `NAME`, before
//! any shell state is built, and exits with the function's return code. The
//! dispatched function has the same signature as `uutils`' `uumain`:
//! `fn(Vec<OsString>) -> i32`, with the bundled name as `argv[0]`.
//!
//! ## Shell integration
//!
//! For every entry in the registry, [`register_shims`] installs a
//! builtin (using `register_builtin_if_unset`, so the shell's own builtins always
//! win on conflict). The builtin's execution path uses cash-core's existing
//! external-command machinery to spawn `current_exe() <DISPATCH_FLAG> <name>
//! <args...>`, inheriting the shell's redirection state for free.
//!
//! The mechanism is generic — the registry is just `name → fn pointer`. The
//! `bundled-coreutils` feature populates it with uutils, but
//! anything matching the signature can be registered.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use cash_core::ExecutionExitCode;
use cash_core::builtins::{BoxFuture, ContentOptions, ContentType, Registration};
use cash_core::commands::{self, CommandArg, ExecutionContext};
use cash_core::extensions::ShellExtensions;
use cash_win32::stdio::CaptureError;

/// The leading flag that signals a bundled-command dispatch.
///
/// Deliberately obscure so that it's unlikely to collide with future
/// first-class shell flags or with scripts that happen to contain the
/// literal token.
pub const DISPATCH_FLAG: &str = "--invoke-bundled";

/// Signature of a bundled command's entry point — matches `uu_*::uumain`.
pub type BundledFn = fn(args: Vec<OsString>) -> i32;

/// Process-wide registry. Set once at startup, read on each shim invocation
/// (and during bundled-dispatch fast path).
static REGISTRY: OnceLock<HashMap<String, BundledFn>> = OnceLock::new();

/// Cached path to the running cash executable. Populated lazily on first
/// shim invocation; left as `Err`-equivalent if `current_exe()` fails.
static SELF_EXE: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Installs the bundled-command registry. Idempotent: only the first call
/// takes effect.
#[expect(
    clippy::implicit_hasher,
    reason = "registry uses the default hasher; callers build with HashMap::new()"
)]
pub fn install(commands: HashMap<String, BundledFn>) {
    let _ = REGISTRY.set(commands);
}

fn run_awk_bundled(args: Vec<OsString>) -> i32 {
    cash_awk::run_awk(args)
}

fn run_bc_bundled(args: Vec<OsString>) -> i32 {
    cash_bc::run_bc(args)
}

fn run_ping_bundled(args: Vec<OsString>) -> i32 {
    cash_builtins::ping::run_ping(args)
}

fn run_sed_bundled(args: Vec<OsString>) -> i32 {
    cash_sed::uumain(args.into_iter())
}

/// Installs the registry from all compiled-in providers.
///
/// Providers are controlled by Cargo features. Binaries should call this
/// once, before [`maybe_dispatch`], so both the dispatch fast path and the
/// shell's shim builtins see a populated registry.
pub fn install_default_providers() {
    #[allow(unused_mut)]
    let mut commands: HashMap<String, BundledFn> = HashMap::new();

    #[cfg(feature = "bundled-coreutils")]
    commands.extend(cash_coreutils_builtins::bundled_commands());

    commands.insert("awk".to_string(), run_awk_bundled);
    commands.insert("sed".to_string(), run_sed_bundled);
    commands.insert("bc".to_string(), run_bc_bundled);
    commands.insert("ping".to_string(), run_ping_bundled);

    install(commands);
}

/// Returns the registered bundled commands, if [`install`] was called.
#[must_use]
pub fn registry() -> Option<&'static HashMap<String, BundledFn>> {
    REGISTRY.get()
}

/// Runs the bundled-command fast path if the process was invoked for it.
///
/// If the process was invoked as `cash <DISPATCH_FLAG> <NAME> [ARGS...]`
/// (with `<DISPATCH_FLAG>` as the very first argument after `argv[0]`), runs
/// the registered function and returns its exit code as `Some(code)`. The
/// caller is responsible for exiting the process with that code —
/// centralizing the exit call in the binary's `main()` keeps destructors /
/// panic hooks / tracing guards in the loop.
///
/// Returns `None` when the process was not invoked as a bundled dispatch, so
/// normal shell startup can proceed.
///
/// The dispatch flag is only recognized in the leading position so that
/// ordinary scripts and command lines containing the literal token elsewhere
/// are not affected.
#[must_use]
pub fn maybe_dispatch() -> Option<i32> {
    let mut raw = std::env::args_os();
    let _argv0 = raw.next();
    let first = raw.next()?;
    if first != DISPATCH_FLAG {
        return None;
    }

    // Everything after `DISPATCH_FLAG` belongs to the bundled command. The
    // first such argument is the command name; subsequent arguments form its
    // argv (with the name itself supplied as argv[0] to match the convention
    // `uutils` and most CLI tools expect).
    let rest: Vec<OsString> = raw.collect();
    let Some((name, args)) = rest.split_first() else {
        eprintln!("cash: {DISPATCH_FLAG} requires a command name");
        return Some(exit_code(ExecutionExitCode::InvalidUsage));
    };

    // The registry is keyed by UTF-8 `String`, so a non-UTF-8 name can never
    // match. Reject up front rather than allocating a lossy-substituted
    // lookup key that could accidentally collide with a real registration.
    let Some(name_str) = name.to_str() else {
        eprintln!("cash: unknown bundled command: {}", name.to_string_lossy());
        return Some(exit_code(ExecutionExitCode::NotFound));
    };

    if name_str == MSYS_RELAY {
        let [tool, program, rest @ ..] = args else {
            eprintln!("cash: {DISPATCH_FLAG} {MSYS_RELAY} requires a tool and a program");
            return Some(exit_code(ExecutionExitCode::InvalidUsage));
        };
        return Some(cash_win32::msys::relay(
            &tool.to_string_lossy(),
            program,
            rest,
        ));
    }

    if name_str == SUDO_OWNER {
        let [sid, program, rest @ ..] = args else {
            eprintln!("cash: {DISPATCH_FLAG} {SUDO_OWNER} requires a SID and a program");
            return Some(exit_code(ExecutionExitCode::InvalidUsage));
        };
        return Some(cash_win32::account::run_owned_by(
            &sid.to_string_lossy(),
            program,
            rest,
        ));
    }

    let Some(func) = REGISTRY.get().and_then(|r| r.get(name_str)) else {
        eprintln!("cash: unknown bundled command: {name_str}");
        return Some(exit_code(ExecutionExitCode::NotFound));
    };

    // The tool opens `/dev/stdin`, `/dev/null` and the rest as a redirection does (D7):
    // in this process only, which runs nothing but the tool.
    cash_win32::devices::install();

    let mut argv: Vec<OsString> = Vec::with_capacity(1 + args.len());
    argv.push(name.clone());
    argv.extend(args.iter().cloned());

    // And it is `cut` to itself, not `cash.exe`, in its messages.
    cash_win32::cmdline::present(&argv);

    if path_emitting(name_str) && !asks_for_help(args) {
        return Some(run_rendering_paths(*func, argv));
    }

    if name_str == "uname" && !asks_for_help(args) {
        return Some(run_unified_uname(*func, argv));
    }

    #[cfg(feature = "bundled-coreutils")]
    let argv = relaying_msys_command(cash_for_shell_names(argv));

    // A tool that runs a command hands it its standard output, which would keep the
    // relay's pipe open after the tool is done.
    if matches!(name_str, "env" | "timeout") {
        return Some(func(argv));
    }
    // Its reader going away ends it with 141, as SIGPIPE does (D71).
    Some(cash_win32::stdio::with_broken_pipe_ending(|| func(argv)))
}

/// The bundled-dispatch name of [`cash_win32::msys::relay`]: `cash --invoke-bundled
/// --msys-relay TOOL PROGRAM [ARGS...]`. Not a utility, so never a builtin; the leading
/// dashes keep it from colliding with one.
const MSYS_RELAY: &str = "--msys-relay";

/// The bundled-dispatch name of [`cash_win32::account::run_owned_by`], the elevated side
/// of `sudo`: `cash --invoke-bundled --sudo-owner SID PROGRAM [ARGS...]`. Not a utility
/// either; `cash_builtins` builds the command line.
const SUDO_OWNER: &str = "--sudo-owner";

/// `argv` for a bundled `env` or `timeout` whose command is a bare `sh`, `bash` or
/// `cash`, with cash itself in its place, as every other way of running those names
/// reaches it (D7). The tools search `PATH`, which finds Git's bash, or the WSL launcher
/// in `System32`. A path still reaches the program it names.
#[cfg(feature = "bundled-coreutils")]
fn cash_for_shell_names(argv: Vec<OsString>) -> Vec<OsString> {
    let Some(operand) = cash_coreutils_builtins::command_operand(&argv) else {
        return argv;
    };
    let Some(program) = operand.args.get(operand.index) else {
        return argv;
    };
    let name = program.to_string_lossy();
    let stem = std::path::Path::new(name.as_ref())
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_ascii_lowercase());
    let is_shell = !name.contains(['/', '\\'])
        && stem.is_some_and(|stem| matches!(stem.as_str(), "sh" | "bash" | "cash"));
    let (true, Some(exe)) = (is_shell, self_exe()) else {
        return argv;
    };
    // The cash it starts learns the name, `bash` or `sh` (POSIX mode), as one the shell
    // starts does (EXE-12). The tool starts it with this process's environment.
    // SAFETY: this runs before the tool does, while this process, which runs nothing but
    // the tool, has its one thread; nothing reads the environment while it changes.
    unsafe { std::env::set_var(cash_core::commands::ARGV0_VARIABLE, name.as_ref()) };
    let mut args = operand.args;
    if let Some(slot) = args.get_mut(operand.index) {
        exe.as_os_str().clone_into(slot);
    }
    args
}

/// `argv` for a bundled `env` or `timeout`, with an MSYS2 command routed through
/// [`MSYS_RELAY`] (D52).
///
/// Those tools spawn their command with `std::process::Command`, which encodes the
/// arguments the Microsoft way, and an MSYS2 program decodes them the Cygwin way. So the
/// tool is handed cash as its command instead, which decodes the Microsoft way, and cash
/// passes the arguments on in the program's own encoding. The program is looked up here
/// only to decide whether that detour is needed, along the PATH and in the directory the
/// tool will use as far as its options say; the relay looks it up again for real.
#[cfg(feature = "bundled-coreutils")]
fn relaying_msys_command(argv: Vec<OsString>) -> Vec<OsString> {
    let Some(operand) = cash_coreutils_builtins::command_operand(&argv) else {
        return argv;
    };
    let Some(program) = operand.args.get(operand.index) else {
        return argv;
    };
    let mut cwd = std::env::current_dir().unwrap_or_default();
    if let Some(dir) = &operand.chdir {
        cwd = cwd.join(dir);
    }
    let path: Vec<PathBuf> = operand
        .path
        .or_else(|| std::env::var_os("PATH"))
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    // A bundled tool runs as a process of its own, whose environment the shell built.
    let pathext = cash_win32::resolve::process_pathext();
    let is_msys = cash_win32::msys::locate(program, &path, &pathext, &cwd)
        .is_some_and(|target| cash_win32::msys::is_msys_program(&target));
    let (true, Some(exe), Some(tool)) = (is_msys, self_exe(), argv.first()) else {
        return argv;
    };

    let mut args = operand.args;
    let tail = args.split_off(operand.index);
    args.extend([
        exe.as_os_str().to_owned(),
        OsString::from(DISPATCH_FLAG),
        OsString::from(MSYS_RELAY),
        tool.clone(),
    ]);
    args.extend(tail);
    args
}

/// Bundled utilities whose standard output is a list of paths they *construct*, rather
/// than paths they were handed (D3, §4 #14).
///
/// Keep this list minimal and justified. A utility belongs here only if every line it
/// writes to standard output is a filesystem path that it built itself:
///
/// - `mktemp` joins a template onto `TMPDIR`, and its output is nearly always captured
///   straight into a variable — the single largest source of backslash paths in scripts.
/// - `realpath` and `readlink -f` canonicalise, which on Windows means asking the OS,
///   which answers in backslashes.
///
/// Not here, deliberately: `find`, `grep -l`, `wc`, `du`, `dirname` and `basename` all
/// echo back a spelling that reached them from the script, so they are already correct
/// once their inputs are. `pwd` is a shell builtin and never reaches this dispatcher.
fn path_emitting(name: &str) -> bool {
    matches!(name, "mktemp" | "realpath" | "readlink")
}

/// Whether the invocation is asking for help or a version rather than doing work.
///
/// Those outputs are prose, not paths, and some argument parsers exit the process while
/// printing them — which would strand the text in the capture file. Cheaper to skip.
fn asks_for_help(args: &[OsString]) -> bool {
    args.iter()
        .any(|a| a == "--help" || a == "--version" || a == "-h")
}

/// Run a bundled utility with its standard output re-rendered in cash's canonical path
/// spelling (D3).
///
/// If the capture itself fails — no writable temp directory, say — the utility still
/// runs, just without the rendering. A wrong separator is a nuisance; refusing to run
/// `mktemp` is a broken shell.
fn run_rendering_paths(func: BundledFn, argv: Vec<OsString>) -> i32 {
    let name = tool_name(&argv);
    match cash_win32::stdio::with_captured_stdout(|| func(argv.clone())) {
        Ok((code, captured)) => {
            let rendered = cash_win32::stdio::render_paths(&captured);
            let _ = cash_win32::stdio::write_stdout(&rendered);
            code
        }
        Err(CaptureError::NotRun(_)) => func(argv),
        Err(CaptureError::Lost(_, error)) => output_lost(&name, &error),
    }
}

/// The tool's name, for a message about it.
fn tool_name(argv: &[OsString]) -> String {
    argv.first()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// A tool ran, and its output could not be read back to pass on: said, and not run again
/// (BIN-07), status 1.
fn output_lost(name: &str, error: &std::io::Error) -> i32 {
    eprintln!("{name}: could not pass its output on: {error}");
    1
}

/// Run a bundled `uname` with its nodename unified to cash's canonical hostname spelling.
fn run_unified_uname(func: BundledFn, argv: Vec<OsString>) -> i32 {
    let name = tool_name(&argv);
    match cash_win32::stdio::with_captured_stdout(|| func(argv.clone())) {
        Ok((code, captured)) => {
            if let Some(target) = cash_win32::process::computer_name() {
                let text = String::from_utf8_lossy(&captured);
                let unified = if let Some(dns) = cash_win32::process::dns_hostname() {
                    text.replace(&dns, &target)
                } else {
                    text.into_owned()
                };
                let _ = cash_win32::stdio::write_stdout(unified.as_bytes());
            } else {
                let _ = cash_win32::stdio::write_stdout(&captured);
            }
            code
        }
        Err(CaptureError::NotRun(_)) => func(argv),
        Err(CaptureError::Lost(_, error)) => output_lost(&name, &error),
    }
}

fn exit_code(code: ExecutionExitCode) -> i32 {
    u8::from(code).into()
}

/// Returns the path to the running cash executable (cached).
fn self_exe() -> Option<&'static PathBuf> {
    SELF_EXE
        .get_or_init(|| std::env::current_exe().ok())
        .as_ref()
}

/// Help/usage content provider for the shim builtin. The shell calls this for
/// `help <name>`, `type <name>`, etc.
#[expect(
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,
    reason = "signature dictated by cash_core::builtins::CommandContentFunc"
)]
fn shim_content(
    name: &str,
    content_type: ContentType,
    _options: &ContentOptions,
) -> Result<String, cash_core::Error> {
    match content_type {
        // Ended by a newline, as every other builtin's is: `help -d cd cat` printed
        // `cat - bundled command` with the next prompt or line glued on.
        ContentType::ShortDescription => Ok(format!("{name} - bundled command\n")),
        // The tool's own `--help`, so that `help cat` lists its options without cash
        // copying them. It named the internal dispatch once, as `brush --invoke-bundled
        // cat` (ARCH-09), and then only pointed at `cat --help`.
        ContentType::DetailedHelp | ContentType::ManPage => {
            Ok(own_help(name).unwrap_or_else(|| {
                format!(
                    "{name} - bundled command: cash carries it, and `{name} --help` describes it\n"
                )
            }))
        }
        // `help -s` takes the usage line from the help above.
        ContentType::ShortUsage => Ok(String::new()),
    }
}

/// What the bundled tool `name` prints for `--help`, or for `-h` when it takes no long
/// options (`ping`), run as `cash --invoke-bundled NAME --help` runs it.
fn own_help(name: &str) -> Option<String> {
    let exe = self_exe()?;
    ["--help", "-h"].iter().find_map(|flag| {
        // The help text depends on nothing of the shell's; the process's environment is
        // as good as any.
        let output = std::process::Command::new(exe)
            .args([DISPATCH_FLAG, name, flag])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        (output.status.success() && !text.trim().is_empty()).then_some(text)
    })
}

/// Builtin execute function shared by all bundled commands. Looks up the
/// invoked name from `context.command_name` and re-executes the running
/// cash binary as `cash <DISPATCH_FLAG> <name> <args>`.
///
/// Reuses the same entry point the `command` builtin uses (see
/// `cash-builtins/src/command.rs`): constructs a [`commands::SimpleCommand`]
/// whose `command_name` is the absolute cash exe path. Because that contains
/// a path separator, `SimpleCommand::execute` routes directly to the
/// external-execution path, bypassing the builtin/function lookup that would
/// otherwise re-enter this very shim.
///
/// `use_functions = false` is defensive: even though the path-separator
/// branch already skips function dispatch, we don't want a hypothetical
/// refactor of `SimpleCommand` to silently break us.
//
// The shim waits for its child before returning, as a builtin returns a finished result.
// A bundled stage in the middle of a pipeline still runs alongside the others, as every
// builtin stage of a pipeline runs in a task of its own (`yes | head -1` ends), and
// `kill %1` reaches its process through the job's job object, not a process group, which
// Windows does not have. Two notes from brush said otherwise; both were checked on
// 2026-10-04 and were no longer so.
fn shim_spawn<SE: ShellExtensions>(
    context: ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
    process_group_id: Option<i32>,
) -> BoxFuture<'_, Result<cash_core::ExecutionSpawnResult, cash_core::Error>> {
    Box::pin(async move {
        let exe_path = if let Some(p) = self_exe() {
            p.to_string_lossy().into_owned()
        } else {
            let _ = writeln!(
                context.stderr(),
                "cash: cannot determine path to running executable"
            );
            return Ok(cash_core::ExecutionSpawnResult::Completed(
                ExecutionExitCode::CannotExecute.into(),
            ));
        };

        // Build the argv for the spawned cash process. `SimpleCommand::args[0]` is
        // dropped by the external-execution path (argv[0] of the spawned
        // process comes from `cmd.argv0` below), so a placeholder suffices;
        // args[1..] become the spawned process's argv[1..]. The caller's
        // `args[0]` is the bundled name by builtin-dispatch convention — we
        // replace it with an explicit `<name>` after `DISPATCH_FLAG` so the
        // child's dispatcher sees it in a fixed slot.
        let bundled_name = context.command_name.clone();
        let mut child_args: Vec<CommandArg> = Vec::with_capacity(args.len() + 2);
        child_args.push(CommandArg::String(String::new())); // args[0], dropped
        child_args.push(CommandArg::String(DISPATCH_FLAG.into()));
        child_args.push(CommandArg::String(bundled_name.clone()));
        child_args.extend(args.into_iter().skip(1));

        let mut cmd = commands::SimpleCommand::new(
            commands::ShellForCommand::ParentShell(context.shell),
            context.params,
            exe_path,
            child_args,
        );
        cmd.use_functions = false;
        // Override the spawned process's argv[0] so tools that report errors
        // via their own argv[0] (uutils' `uucore::util_name()` reads
        // `std::env::args_os()[0]` into a LazyLock at first use) render as
        // `<name>:` rather than `cash:`. Without this the child sees the
        // cash exe path as argv[0] and misattributes errors.
        cmd.argv0 = Some(bundled_name);
        cmd.process_group_id = process_group_id;

        cmd.execute().await
    })
}

fn shim_execute<SE: ShellExtensions>(
    context: ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> BoxFuture<'_, Result<cash_core::ExecutionResult, cash_core::Error>> {
    Box::pin(async move {
        let spawn_result = shim_spawn(context, args, None).await?;
        let wait_result = spawn_result.wait().await?;
        Ok(wait_result.into())
    })
}

/// Constructs a [`Registration`] for the bundled-shim builtin of `name`; per-name
/// dispatch happens via `context.command_name` at execution time.
fn shim_registration<SE: ShellExtensions>(name: &str) -> Registration<SE> {
    Registration {
        execute_func: shim_execute::<SE>,
        spawn_func: Some(shim_spawn::<SE>),
        content_func: shim_content,
        disabled: false,
        special_builtin: false,
        declaration_builtin: false,
        substitution_pipes: SUBSTITUTION_PIPES.contains(&name),
    }
}

/// The bundled tools that open a file they are to write as a pipe may be opened, when it
/// is a `>(...)`: patched for it, see `vendor/uutils/CASH-PATCHES.md` (D17). Every other
/// one opens it as a new file, which a named pipe does not allow, and is handed a temp file
/// instead.
const SUBSTITUTION_PIPES: &[&str] = &["tee", "sort", "uniq", "shuf"];

/// Registers a shim builtin for every name in the installed bundled-command
/// registry.
///
/// Uses `register_builtin_if_unset` so the shell's own builtins (echo, printf,
/// true, false, etc.) win on conflict.
pub fn register_shims<SE: ShellExtensions>(shell: &mut cash_core::Shell<SE>) {
    let Some(registry) = REGISTRY.get() else {
        return;
    };
    for name in registry.keys() {
        shell.register_builtin_if_unset(name.clone(), shim_registration::<SE>(name));
    }
}
