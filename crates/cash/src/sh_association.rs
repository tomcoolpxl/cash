//! `cash --associate-sh` and `cash --remove-sh-association`: `.sh` scripts open with
//! cash, from Explorer and from `start x.sh` (D87).
//!
//! cash registers itself as the program class `cash.sh`, which runs `cash.exe "x.sh"`,
//! and lists it in the `Open with` of `.sh`. It makes it the one a double-click starts
//! only when no other program has `.sh`: Git for Windows registers its own, for every
//! user, and that stays. A choice made in Windows' "Open with" or Settings wins over both,
//! and only the user can make one. The keys are the user's own, under
//! `HKEY_CURRENT_USER\Software\Classes`; nothing needs an administrator.

use std::path::Path;

use cash_win32::classes;

/// The program class cash registers.
const PROG_ID: &str = "cash.sh";

/// The extension.
const EXTENSION: &str = ".sh";

/// What Explorer shows as the type of a `.sh` that opens with cash.
const TYPE_NAME: &str = "Shell script";

const USAGE: &str = "usage: cash --associate-sh\n       cash --remove-sh-association";

/// `cash --associate-sh` or `cash --remove-sh-association`: the process exit status, or
/// `None` when the command line is neither.
pub fn command(args: &[String]) -> Option<u8> {
    let remove = match args.get(1).map(String::as_str) {
        Some("--associate-sh") => false,
        Some("--remove-sh-association") => true,
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
                println!("cash --remove-sh-association: .sh scripts no longer open with cash");
            } else {
                println!("cash --remove-sh-association: .sh scripts did not open with cash");
            }
        })
    } else {
        std::env::current_exe()
            .map_err(|e| format!("cannot find cash.exe: {e}"))
            .and_then(|exe| write_for(&exe))
            .map(|written| {
                match &written.opener {
                    None => println!("cash --associate-sh: .sh scripts open with cash, running"),
                    Some(other) => println!(
                        "cash --associate-sh: cash is in the \"Open with\" of .sh scripts; \
                         a double-click still starts {other}, which Settings > Apps > \
                         Default apps can change"
                    ),
                }
                println!("  {}", written.command);
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

/// What [`write_for`] did.
pub struct Written {
    /// The command line `cash.sh` runs.
    pub command: String,
    /// The program class a double-click starts instead of cash's, when another has `.sh`.
    pub opener: Option<String>,
}

/// Register `cash.sh` for `exe` and list it for `.sh`, making it the default when no
/// other program has `.sh`. The installer gives the `current\cash.exe` that follows its
/// upgrades.
///
/// # Errors
///
/// A registry key that could not be written, named.
pub fn write_for(exe: &Path) -> Result<Written, String> {
    let exe = cash_win32::path::to_backslash(exe);
    let command = command_line(&exe);
    classes::set_value(PROG_ID, None, TYPE_NAME).map_err(named(PROG_ID))?;
    let icon = format!(r"{PROG_ID}\DefaultIcon");
    classes::set_value(&icon, None, &format!("\"{exe}\",0")).map_err(named(&icon))?;
    let open = format!(r"{PROG_ID}\shell\open\command");
    classes::set_value(&open, None, &command).map_err(named(&open))?;
    let open_with = format!(r"{EXTENSION}\OpenWithProgids");
    classes::set_value(&open_with, Some(PROG_ID), "").map_err(named(&open_with))?;

    let default = classes::merged_default(EXTENSION).filter(|class| !class.is_empty());
    let other_default = match default {
        Some(class) if !class.eq_ignore_ascii_case(PROG_ID) => Some(class),
        _ => {
            classes::set_value(EXTENSION, None, PROG_ID).map_err(named(EXTENSION))?;
            None
        }
    };
    let chosen =
        classes::user_choice(EXTENSION).filter(|class| !class.eq_ignore_ascii_case(PROG_ID));
    let opener = chosen.or(other_default);
    classes::announce_associations();
    Ok(Written { command, opener })
}

/// Remove `cash.sh` and its place in `.sh`; whether there was one. A default another
/// program set stays.
///
/// # Errors
///
/// A registry key that is there but could not be removed, named.
pub fn remove_quietly() -> Result<bool, String> {
    let mut removed = classes::remove_key(PROG_ID).map_err(named(PROG_ID))?;
    let open_with = format!(r"{EXTENSION}\OpenWithProgids");
    removed |= classes::remove_value(&open_with, Some(PROG_ID)).map_err(named(&open_with))?;
    if classes::value(EXTENSION, None).is_some_and(|class| class.eq_ignore_ascii_case(PROG_ID)) {
        removed |= classes::remove_value(EXTENSION, None).map_err(named(EXTENSION))?;
    }
    if removed {
        classes::announce_associations();
    }
    Ok(removed)
}

/// Whether cash is registered for `.sh`, so an upgrade writes it again.
pub fn is_set() -> bool {
    classes::value(&format!(r"{PROG_ID}\shell\open\command"), None).is_some()
}

/// An error about the key at `key` under the classes, named.
fn named(key: &str) -> impl Fn(std::io::Error) -> String + '_ {
    move |e| format!(r"HKEY_CURRENT_USER\Software\Classes\{key}: {e}")
}

/// The command line `cash.sh` runs: the script, then what else the caller passed.
fn command_line(exe: &str) -> String {
    format!("\"{exe}\" \"%1\" %*")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_runs_with_its_arguments() {
        assert_eq!(
            command_line(r"C:\cash\current\cash.exe"),
            r#""C:\cash\current\cash.exe" "%1" %*"#
        );
    }
}
