//! `cash --explorer-menu` and `cash --remove-explorer-menu`: "Open cash here" in
//! Explorer's menu of a directory and of a directory's background (D85).
//!
//! The entry opens Windows Terminal with cash's profile in that directory, `wt.exe -p cash
//! -d DIR`, or, on a Windows without Terminal, `cash.exe --cd DIR` in a console window.
//! Which one is settled when the entry is written; the installer writes it again on each
//! upgrade. The directory goes in as `"%V."`: a drive's top, `"C:\"`, would end in a
//! backslash that escapes the closing quote, and the dot falls away as Windows makes the
//! path whole. The keys are the user's own, under `HKEY_CURRENT_USER\Software\Classes`;
//! nothing needs an administrator. Windows 11 lists the entry under "Show more options".

use std::path::{Path, PathBuf};

use cash_win32::classes::{self, Verb};

/// The classes the entry goes on: a directory, and the background of an open one.
const CLASSES: [&str; 2] = ["Directory", r"Directory\Background"];

/// The verb's key name.
const VERB: &str = "cash";

/// The menu's text.
const LABEL: &str = "Open cash here";

/// The Terminal profile the entry opens, the one `cash --terminal-profile` writes.
const PROFILE: &str = "cash";

const USAGE: &str = "usage: cash --explorer-menu\n       cash --remove-explorer-menu";

/// `cash --explorer-menu` or `cash --remove-explorer-menu`: the process exit status, or
/// `None` when the command line is neither.
pub fn command(args: &[String]) -> Option<u8> {
    let remove = match args.get(1).map(String::as_str) {
        Some("--explorer-menu") => false,
        Some("--remove-explorer-menu") => true,
        _ => return None,
    };
    match args.get(2).map(String::as_str) {
        None => {}
        Some("-h" | "--help") => {
            println!("{USAGE}");
            return Some(0);
        }
        Some(other) => {
            eprintln!("cash: unexpected argument `{other}`\n{USAGE}");
            return Some(2);
        }
    }
    let result = if remove {
        remove_quietly().map(|removed| {
            if removed {
                println!("cash --remove-explorer-menu: \"{LABEL}\" removed");
            } else {
                println!("cash --remove-explorer-menu: there was no \"{LABEL}\"");
            }
        })
    } else {
        std::env::current_exe()
            .map_err(|e| format!("cannot find cash.exe: {e}"))
            .and_then(|exe| write_for(&exe))
            .map(|command| {
                println!("cash --explorer-menu: \"{LABEL}\" on directories, running");
                println!("  {command}");
            })
    };
    Some(match result {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("cash: {message}");
            1
        }
    })
}

/// Write the entry for `exe`, and return the command line it runs. The installer gives
/// the `current\cash.exe` that follows its upgrades.
///
/// # Errors
///
/// A registry key that could not be written, named.
pub fn write_for(exe: &Path) -> Result<String, String> {
    let exe = cash_win32::path::to_backslash(exe);
    let command = command_line(&exe, terminal().as_deref());
    let verb = Verb {
        label: LABEL,
        icon: &exe,
        command: &command,
    };
    for class in CLASSES {
        classes::set(class, VERB, &verb)
            .map_err(|e| format!(r"HKEY_CURRENT_USER\Software\Classes\{class}: {e}"))?;
    }
    Ok(command)
}

/// Remove the entry; whether there was one.
///
/// # Errors
///
/// A registry key that is there but could not be removed, named.
pub fn remove_quietly() -> Result<bool, String> {
    let mut removed = false;
    for class in CLASSES {
        removed |= classes::remove(class, VERB)
            .map_err(|e| format!(r"HKEY_CURRENT_USER\Software\Classes\{class}: {e}"))?;
    }
    Ok(removed)
}

/// Whether the entry is there, so an upgrade writes it again.
pub fn is_set() -> bool {
    CLASSES
        .iter()
        .any(|class| classes::command(class, VERB).is_some())
}

/// Windows Terminal's `wt.exe`, the alias the Store's and the inbox Terminal install in
/// the user's `WindowsApps`.
fn terminal() -> Option<String> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let wt = PathBuf::from(local).join(r"Microsoft\WindowsApps\wt.exe");
    wt.exists().then(|| cash_win32::path::to_backslash(&wt))
}

/// The command line the entry runs: Terminal with cash's profile when there is a
/// Terminal, cash in a console window when not.
fn command_line(exe: &str, terminal: Option<&str>) -> String {
    match terminal {
        Some(wt) => format!("\"{wt}\" -p \"{PROFILE}\" -d \"%V.\""),
        None => format!("\"{exe}\" --cd \"%V.\""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_terminal_the_entry_opens_cashs_profile_there() {
        assert_eq!(
            command_line(r"C:\cash\current\cash.exe", Some(r"C:\Apps\wt.exe")),
            r#""C:\Apps\wt.exe" -p "cash" -d "%V.""#
        );
    }

    #[test]
    fn without_terminal_cash_starts_there_itself() {
        assert_eq!(
            command_line(r"C:\cash\current\cash.exe", None),
            r#""C:\cash\current\cash.exe" --cd "%V.""#
        );
    }
}
