//! cash — Cool Again Shell.
//!
//! A bash-language shell whose execution model is Win32. The language comes from
//! `brush` (D1/D9); what cash adds is the Windows semantics layer — process containment,
//! signals, the path model, encoding.
//!
//! This entry point does the one thing that must happen before any other code runs:
//! installs the session job object, so that every process cash ever starts is already
//! contained. See `cash_win32::session`.

mod doctor;
mod init_rc;
mod link_tools;
mod terminal_menu;
mod terminal_profile;

fn main() {
    // Read first, while cash has a single thread, so it can be removed safely.
    let invoked_as = take_invoked_name();

    // D6's outermost guarantee, installed before anything can spawn.
    let state = cash_win32::session::install_and_leak();

    if std::env::var_os("CASH_DEBUG_SESSION").is_some() {
        // Honest reporting rather than an implied guarantee: D6's containment either is
        // or is not in force, and cash should be able to say which.
        eprintln!(
            "cash: session job {}, utf8 console {}, nested in another job {}",
            if state.job_installed {
                "installed"
            } else {
                "UNAVAILABLE"
            },
            if state.utf8_console {
                "on"
            } else {
                "unavailable"
            },
            if state.nested { "yes" } else { "no" },
        );
    }

    // Started as a link `cash --link-tools` made (`ls.exe`), cash runs as that tool (D65):
    // `cash -c '"$0" "$@"' ls ARGS`, the re-entry its virtual paths use (D58). Unless a
    // cash started it, as it starts its own exe, which in a tool's process is the link:
    // it names itself (`CASH_ARGV0`), or dispatches a bundled tool.
    let reentered =
        invoked_as.is_some() || std::env::args().nth(1).as_deref() == Some("--invoke-bundled");
    if !reentered && let Some(tool) = link_tools::linked_tool() {
        let mut args = std::env::args();
        let exe = args.next().unwrap_or_default();
        let mut command = vec![exe, "-c".to_owned(), "\"$0\" \"$@\"".to_owned(), tool];
        command.extend(args);
        cash_shell::entry::run_with_args(command);
        return;
    }

    // `cash --link-tools` and `--unlink-tools` (D65), `--terminal-profile` (D38) and
    // `--init-rc` (D69), like `cash doctor`, act on the installation rather than running
    // anything in a shell.
    let args: Vec<String> = std::env::args().collect();
    if let Some(status) = link_tools::command(&args)
        .or_else(|| terminal_profile::command(&args))
        .or_else(|| init_rc::command(&args))
    {
        std::process::exit(i32::from(status));
    }

    // `cash doctor` (D35) is handled before the shell sees argv, because it diagnoses
    // the environment rather than running anything in it.
    //
    // A file named `doctor` in the working directory wins, so the subcommand can never
    // shadow a script the user actually meant to run. `cash ./doctor` is unambiguous
    // either way.
    let mut args = std::env::args();
    let is_doctor = args.nth(1).as_deref() == Some("doctor") && args.next().is_none();
    if is_doctor && !std::path::Path::new("doctor").exists() {
        std::process::exit(i32::from(doctor::run()));
    }

    // `cash help [ARGS]` is the `help` builtin, from PowerShell or cmd, run by a shell
    // that reads no startup file: it says the same outside cash as inside, from the same
    // catalogue and the builtins this cash registers. A file named `help` in the working
    // directory wins, as for `doctor`. The hidden option makes its errors say `cash help:`
    // rather than name the `-c` script and its line.
    let mut args = std::env::args();
    let is_help = args.nth(1).as_deref() == Some("help");
    if is_help && !std::path::Path::new("help").exists() {
        let exe = std::env::args().next().unwrap_or_default();
        let script = format!(
            "builtin help --{} \"$@\"",
            cash_builtins::helpdocs::CASH_SUBCOMMAND_OPTION
        );
        let mut command = vec![
            exe,
            "--norc".to_owned(),
            "--noprofile".to_owned(),
            "-c".to_owned(),
            script,
            "cash".to_owned(),
        ];
        command.extend(args);
        cash_shell::entry::run_with_args(command);
        return;
    }

    let mut args: Vec<String> = std::env::args().collect();
    if let Some(name) = invoked_as {
        // Bash started as `sh` runs in POSIX mode, every builtin still there.
        let as_sh = std::path::Path::new(&name)
            .file_stem()
            .is_some_and(|stem| stem.eq_ignore_ascii_case("sh"));
        if let Some(argv0) = args.first_mut() {
            *argv0 = name;
        }
        if as_sh {
            args.insert(1.min(args.len()), "--posix".to_owned());
        }
    }
    cash_shell::entry::run_with_args(args);
}

/// The name a cash that started this one ran it by (`bash`, `sh`, `exec -a NAME`), which
/// Windows cannot put in `argv[0]` (EXE-12, `cash_core::commands::ARGV0_VARIABLE`). The
/// variable is removed, so a program this cash starts does not inherit it.
fn take_invoked_name() -> Option<String> {
    let name = std::env::var(cash_core::commands::ARGV0_VARIABLE).ok()?;
    // SAFETY: called first in `main`, before any other thread exists, so nothing reads
    // the environment while it changes.
    unsafe { std::env::remove_var(cash_core::commands::ARGV0_VARIABLE) };
    (!name.is_empty()).then_some(name)
}
