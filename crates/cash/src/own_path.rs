//! `cash --add-to-path` and `cash --remove-from-path`: the folder that stands for this
//! `cash.exe`, on the user PATH or off it, and what `cash doctor` says about it.
//!
//! Which folder that is depends on how cash was put on the machine
//! (`cash_win32::install_layout`): a portable copy is found by its own folder; the
//! installer's by `current`, the junction beside the version folder, so that an upgrade
//! moves the junction and PATH stays as it is; Scoop's by its shims folder, which Scoop's
//! own installer put on PATH and which serves every Scoop app, so cash never adds or
//! removes it. The user PATH is written the way `--install-finish` and `--link-tools
//! --add-to-path` write it (`cash_win32::userpath`): at the front, the stored value's
//! type and every other entry kept, the change announced to running programs. The
//! portable offer's first question runs `cash --add-to-path`, so it says what the command
//! says.
//!
//! `cash doctor`'s line about PATH asks the same question read-only: whether that folder is
//! on the user PATH, on the machine's (which an administrator set up), or on neither, in
//! which case new windows will not find `cash` and the fix is named.

use std::path::{Path, PathBuf};

use cash_win32::install_layout::{installed_by_scoop, layout};
use cash_win32::userpath::{self, Added};

const USAGE: &str = "usage: cash --add-to-path\n       cash --remove-from-path\n\
                     Puts the folder of this cash.exe on your user PATH, or takes it off.";

/// `cash --add-to-path` or `cash --remove-from-path`: the process exit status, or `None`
/// when the command line is neither.
pub fn command(args: &[String]) -> Option<u8> {
    let adding = match args.get(1).map(String::as_str) {
        Some("--add-to-path") => true,
        Some("--remove-from-path") => false,
        _ => return None,
    };
    // Neither command takes an argument.
    if let Some(arg) = args.get(2) {
        return Some(match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                0
            }
            other => {
                eprintln!("cash: unexpected argument `{other}`\n{USAGE}");
                2
            }
        });
    }
    let place = match own_place() {
        Ok(place) => place,
        Err(message) => {
            eprintln!("cash: {message}");
            return Some(1);
        }
    };
    Some(if adding { add(&place) } else { remove(&place) })
}

/// Where this cash is, and so which folder stands for it on PATH.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// Scoop's install: its shims folder, Scoop's to keep on PATH.
    Scoop(PathBuf),
    /// The installer's: `current`, which follows the upgrades.
    Installed(PathBuf),
    /// A portable copy: the folder the exe is in.
    Portable(PathBuf),
}

impl Place {
    /// The folder that stands for cash on PATH.
    pub fn folder(&self) -> &Path {
        match self {
            Self::Scoop(folder) | Self::Installed(folder) | Self::Portable(folder) => folder,
        }
    }

    /// The folder, as Windows shows it.
    fn shown(&self) -> String {
        cash_win32::path::to_backslash(self.folder())
    }
}

/// The place of `exe`, a real path; `is_junction` says whether a path is a junction, so
/// the tests can decide.
pub fn place(exe: &Path, is_junction: impl Fn(&Path) -> bool) -> Option<Place> {
    if installed_by_scoop(exe) {
        return scoop_shims(exe).map(Place::Scoop);
    }
    if let Some(layout) = layout(exe, &is_junction) {
        return Some(Place::Installed(layout.current));
    }
    exe.parent()
        .map(|folder| Place::Portable(folder.to_path_buf()))
}

/// Scoop's shims folder for an exe under `scoop\apps\`: `shims` beside `apps`.
fn scoop_shims(exe: &Path) -> Option<PathBuf> {
    exe.ancestors().find_map(|dir| {
        let is_apps = dir
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("apps"));
        let scoop = dir.parent()?;
        let under_scoop = scoop
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("scoop"));
        (is_apps && under_scoop).then(|| scoop.join("shims"))
    })
}

/// The place of the running `cash.exe`.
fn own_place() -> Result<Place, String> {
    let exe = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|e| format!("cannot find cash.exe: {e}"))?;
    place(&exe, cash_win32::junction::is_junction)
        .ok_or_else(|| "cash.exe has no folder".to_owned())
}

/// Which PATH lists a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listed {
    /// The user's own PATH.
    User,
    /// The machine's, which Windows puts before the user's.
    Machine,
    /// Neither: a new window will not find what is in the folder.
    Nowhere,
}

/// Where `folder` is listed, the user PATH first.
fn listed(folder: &Path) -> Listed {
    if userpath::contains(folder) {
        Listed::User
    } else if userpath::machine_contains(folder) {
        Listed::Machine
    } else {
        Listed::Nowhere
    }
}

/// `cash --add-to-path`: the place's folder first on the user PATH, with one line
/// saying what happened. Scoop's shims folder is left to Scoop, and only reported on.
fn add(place: &Place) -> u8 {
    let shown = place.shown();
    match place {
        Place::Scoop(shims) => {
            if listed(shims) == Listed::Nowhere {
                eprintln!("cash: installed by Scoop, but its shims folder {shown} is on no PATH");
                1
            } else {
                println!("cash: installed by Scoop: its shims folder is already on PATH");
                0
            }
        }
        Place::Installed(folder) | Place::Portable(folder) => match userpath::add_first(folder) {
            Ok(outcome) => {
                println!("cash: {}", added_message(&shown, outcome));
                0
            }
            Err(e) => {
                eprintln!("cash: cannot change the user PATH: {e}");
                1
            }
        },
    }
}

/// What `--add-to-path` says, after the `cash: `.
fn added_message(shown: &str, outcome: Added) -> String {
    match outcome {
        Added::Added => {
            format!("{shown} added to your user PATH: windows opened from now on have cash")
        }
        Added::AlreadyThere => format!("{shown} is on your user PATH already"),
    }
}

/// `cash --remove-from-path`: every entry naming the place's folder off the user PATH.
fn remove(place: &Place) -> u8 {
    let shown = place.shown();
    match place {
        Place::Scoop(_) => {
            eprintln!(
                "cash: installed by Scoop: its shims folder {shown} serves every Scoop app and \
                 stays on PATH; scoop uninstall cash removes cash"
            );
            2
        }
        Place::Installed(folder) | Place::Portable(folder) => match userpath::remove(folder) {
            Ok(true) => {
                println!("cash: {shown} removed from your user PATH");
                0
            }
            Ok(false) => {
                println!("cash: {shown} was not on your user PATH");
                0
            }
            Err(e) => {
                eprintln!("cash: cannot change the user PATH: {e}");
                1
            }
        },
    }
}

/// What `cash doctor` says about PATH.
pub struct Report {
    /// Whether all is well; otherwise a note.
    pub ok: bool,
    /// The line's text.
    pub detail: String,
    /// What to do, when something is.
    pub fix: Option<String>,
}

/// The line about PATH for the running `cash.exe`, or `None` when it cannot be found.
pub fn doctor_report() -> Option<Report> {
    let place = own_place().ok()?;
    Some(report(&place, listed(place.folder())))
}

/// The line about PATH for a cash at `place` whose folder is `listed` as said; the
/// folder as `cash doctor` writes paths.
fn report(place: &Place, listed: Listed) -> Report {
    let shown = crate::doctor::shown(place.folder());
    match listed {
        Listed::User => Report {
            ok: true,
            detail: format!("{shown} is on your user PATH"),
            fix: None,
        },
        Listed::Machine => Report {
            ok: true,
            detail: format!("{shown} is on the system PATH"),
            fix: None,
        },
        Listed::Nowhere => Report {
            ok: false,
            detail: format!("{shown} is on no PATH, so new windows will not find cash"),
            fix: Some(match place {
                Place::Scoop(_) => "scoop reset cash".to_owned(),
                Place::Installed(_) | Place::Portable(_) => "cash --add-to-path".to_owned(),
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn only_its_own_options_are_taken() {
        assert_eq!(command(&args("cash -c ls")), None);
        assert_eq!(command(&args("cash --link-tools --add-to-path")), None);
        assert_eq!(command(&args("cash --add-to-path --help")), Some(0));
        assert_eq!(command(&args("cash --remove-from-path -h")), Some(0));
        assert_eq!(command(&args("cash --add-to-path C:\\x")), Some(2));
        assert_eq!(command(&args("cash --remove-from-path --quiet")), Some(2));
    }

    #[test]
    fn the_place_is_told_by_the_layout() {
        let no_junction = |_: &Path| false;
        assert_eq!(
            place(
                Path::new(r"C:\Users\me\scoop\apps\cash\1.6.0\cash.exe"),
                no_junction
            ),
            Some(Place::Scoop(PathBuf::from(r"C:\Users\me\scoop\shims")))
        );
        assert_eq!(
            place(
                Path::new(r"C:\ProgramData\Scoop\apps\cash\current\cash.exe"),
                no_junction
            ),
            Some(Place::Scoop(PathBuf::from(r"C:\ProgramData\Scoop\shims")))
        );
        assert_eq!(
            place(Path::new(r"D:\tools\unpacked\cash.exe"), no_junction),
            Some(Place::Portable(PathBuf::from(r"D:\tools\unpacked")))
        );
        let current = Path::new(r"C:\Users\me\AppData\Local\Programs\cash\current");
        assert_eq!(
            place(
                Path::new(r"C:\Users\me\AppData\Local\Programs\cash\1.6.0\cash.exe"),
                |path: &Path| path == current
            ),
            Some(Place::Installed(current.to_path_buf()))
        );
    }

    #[test]
    fn the_doctor_line_names_the_folder_and_the_fix_for_the_layout() {
        let portable = Place::Portable(PathBuf::from(r"D:\tools\unpacked"));
        let installed = Place::Installed(PathBuf::from(r"C:\Programs\cash\current"));
        let scoop = Place::Scoop(PathBuf::from(r"C:\Users\me\scoop\shims"));

        let line = report(&portable, Listed::User);
        assert!(line.ok);
        assert_eq!(line.detail, "D:/tools/unpacked is on your user PATH");
        assert_eq!(line.fix, None);

        let line = report(&installed, Listed::Machine);
        assert!(line.ok);
        assert_eq!(
            line.detail,
            "C:/Programs/cash/current is on the system PATH"
        );

        let line = report(&scoop, Listed::User);
        assert_eq!(line.detail, "C:/Users/me/scoop/shims is on your user PATH");

        let line = report(&portable, Listed::Nowhere);
        assert!(!line.ok);
        assert_eq!(
            line.detail,
            "D:/tools/unpacked is on no PATH, so new windows will not find cash"
        );
        assert_eq!(line.fix.as_deref(), Some("cash --add-to-path"));

        let line = report(&installed, Listed::Nowhere);
        assert_eq!(
            line.detail,
            "C:/Programs/cash/current is on no PATH, so new windows will not find cash"
        );
        assert_eq!(line.fix.as_deref(), Some("cash --add-to-path"));

        let line = report(&scoop, Listed::Nowhere);
        assert_eq!(line.fix.as_deref(), Some("scoop reset cash"));
    }

    #[test]
    fn the_add_message_says_what_changed() {
        assert_eq!(
            added_message(r"D:\tools\unpacked", Added::Added),
            r"D:\tools\unpacked added to your user PATH: windows opened from now on have cash"
        );
        assert_eq!(
            added_message(r"D:\tools\unpacked", Added::AlreadyThere),
            r"D:\tools\unpacked is on your user PATH already"
        );
    }
}
