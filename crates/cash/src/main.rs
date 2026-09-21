//! cash — Cool Again Shell.
//!
//! A bash-language shell whose execution model is Win32. The language comes from
//! `brush` (D1/D9); what cash adds is the Windows semantics layer — process containment,
//! signals, the path model, encoding.
//!
//! This entry point does the one thing that must happen before any other code runs:
//! installs the session job object, so that every process cash ever starts is already
//! contained. See `cash_win32::session`.

fn main() {
    #[cfg(windows)]
    let state = cash_win32::session::install_and_leak();

    #[cfg(windows)]
    if std::env::var_os("CASH_DEBUG_SESSION").is_some() {
        // Honest reporting rather than an implied guarantee: D6's containment either is
        // or is not in force, and cash should be able to say which.
        eprintln!(
            "cash: session job {}, utf8 console {}, nested in another job {}",
            if state.job_installed { "installed" } else { "UNAVAILABLE" },
            if state.utf8_console { "on" } else { "unavailable" },
            if state.nested { "yes" } else { "no" },
        );
    }

    brush_shell::entry::run();
}
