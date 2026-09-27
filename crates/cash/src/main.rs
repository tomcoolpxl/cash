//! cash — Cool Again Shell.
//!
//! A bash-language shell whose execution model is Win32. The language comes from
//! `brush` (D1/D9); what cash adds is the Windows semantics layer — process containment,
//! signals, the path model, encoding.
//!
//! This entry point does the one thing that must happen before any other code runs:
//! installs the session job object, so that every process cash ever starts is already
//! contained. See `cash_win32::session`.

#[cfg(windows)]
mod doctor;
#[cfg(windows)]
mod link_tools;
#[cfg(windows)]
mod terminal_profile;

fn main() {
    // D6's outermost guarantee, installed before anything can spawn.
    #[cfg(windows)]
    let state = cash_win32::session::install_and_leak();

    #[cfg(windows)]
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
    // `cash -c '"$0" "$@"' ls ARGS`, the re-entry its virtual paths use (D58).
    #[cfg(windows)]
    if let Some(tool) = link_tools::linked_tool() {
        let mut args = std::env::args();
        let exe = args.next().unwrap_or_default();
        let mut command = vec![exe, "-c".to_owned(), "\"$0\" \"$@\"".to_owned(), tool];
        command.extend(args);
        cash_shell::entry::run_with_args(command);
        return;
    }

    // `cash --link-tools` and `--unlink-tools` (D65), and `--terminal-profile` (D38), like
    // `cash doctor`, act on the installation rather than running anything in a shell.
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().collect();
        if let Some(status) =
            link_tools::command(&args).or_else(|| terminal_profile::command(&args))
        {
            std::process::exit(i32::from(status));
        }
    }

    // `cash doctor` (D35) is handled before the shell sees argv, because it diagnoses
    // the environment rather than running anything in it.
    //
    // A file named `doctor` in the working directory wins, so the subcommand can never
    // shadow a script the user actually meant to run. `cash ./doctor` is unambiguous
    // either way.
    #[cfg(windows)]
    {
        let mut args = std::env::args();
        let is_doctor = args.nth(1).as_deref() == Some("doctor") && args.next().is_none();
        if is_doctor && !std::path::Path::new("doctor").exists() {
            std::process::exit(i32::from(doctor::run()));
        }
    }

    cash_shell::entry::run();
}
