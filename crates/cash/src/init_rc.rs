//! `cash --init-rc`: a starter `~/.bashrc` for someone who has no startup file (spec D69).
//!
//! cash reads `~/.bashrc` and then `~/.cashrc` (D24). With neither, a new user gets bash's
//! bare defaults: a `$ ` prompt, 500 lines of history and no aliases. The starter is the
//! author's own `~/.bashrc`, less what needs setting up by hand: history kept large and
//! shared between windows, the usual aliases, a prompt showing the folder and git branch
//! with Windows Terminal's shell-integration marks, `ls` with icons, and `coolfetch` once
//! per window.
//!
//! It is written as `~/.bashrc`, and only when there is neither `~/.bashrc` nor
//! `~/.cashrc`: a Git Bash user's own file is left alone, and one written here is read by
//! Git Bash too, which is why it keeps its sections for cash alone behind `$CASH_VERSION`
//! (decided with the user, 2026-09-29).
//!
//! Scoop's manifest runs `cash --init-rc --once` after every install and update. `--once`
//! acts only the first time for a user, as the marker `%LOCALAPPDATA%\cash\init-rc`
//! records, so a starter deleted on purpose does not come back with the next update. Run
//! by hand, without it, the command writes one whenever there is none.

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The starter, as written.
const STARTER: &str = include_str!("starter.bashrc");

const USAGE: &str = "\
usage: cash --init-rc [--once]

Writes a starter ~/.bashrc, read by cash and by Git Bash, when there is neither
~/.bashrc nor ~/.cashrc.

  --once   only the first time for this user (Scoop runs it so after each update)";

/// Runs `cash --init-rc` when `args` asks for it; `None` otherwise.
pub fn command(args: &[String]) -> Option<u8> {
    if args.get(1).map(String::as_str) != Some("--init-rc") {
        return None;
    }
    let mut once = false;
    for arg in args.iter().skip(2) {
        match arg.as_str() {
            "--once" => once = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Some(0);
            }
            other => {
                eprintln!("cash: unexpected argument `{other}`\n{USAGE}");
                return Some(2);
            }
        }
    }
    let Some(home) = home() else {
        eprintln!("cash: neither HOME nor USERPROFILE is set");
        return Some(1);
    };
    let marker = std::env::var_os("LOCALAPPDATA")
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join("cash").join("init-rc"));
    // Scoop shows what `--once` prints after every update, so it speaks only when it
    // writes.
    Some(match init(&home, marker.as_deref(), once) {
        Ok(Outcome::Written(path)) => {
            println!(
                "cash: wrote {}, a starter read by cash and Git Bash",
                path.display()
            );
            0
        }
        Ok(Outcome::Exists(path)) => {
            if !once {
                println!("cash: {} exists; nothing written", path.display());
            }
            0
        }
        Ok(Outcome::DoneBefore) => 0,
        Err(message) => {
            eprintln!("cash: {message}");
            1
        }
    })
}

/// `cash --init-rc --once` without a word, for the installer: the starter's path when
/// one was written.
///
/// # Errors
///
/// No home folder, or the starter could not be written.
pub fn run_once() -> Result<Option<PathBuf>, String> {
    let home = home().ok_or_else(|| "neither HOME nor USERPROFILE is set".to_owned())?;
    let marker = std::env::var_os("LOCALAPPDATA")
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join("cash").join("init-rc"));
    Ok(match init(&home, marker.as_deref(), true)? {
        Outcome::Written(path) => Some(path),
        Outcome::Exists(_) | Outcome::DoneBefore => None,
    })
}

/// What `--init-rc` found.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// It wrote the starter here.
    Written(PathBuf),
    /// The user has this startup file already.
    Exists(PathBuf),
    /// `--once`, and it had run before for this user.
    DoneBefore,
}

/// The folder `~` names: `$HOME`, else `%USERPROFILE%`, as cash expands it.
fn home() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(std::env::var_os)
        .find(|dir| !dir.is_empty())
        .map(PathBuf::from)
}

/// Writes the starter as `home\.bashrc` unless the user has a startup file, or, `once`,
/// unless the marker says this has run before. A `once` run leaves the marker behind,
/// whatever it found; so does any run that writes the starter.
fn init(home: &Path, marker: Option<&Path>, once: bool) -> Result<Outcome, String> {
    if once && marker.is_some_and(Path::exists) {
        return Ok(Outcome::DoneBefore);
    }
    let existing = [".bashrc", ".cashrc"]
        .into_iter()
        .map(|name| home.join(name))
        .find(|path| path.exists());
    let outcome = match existing {
        Some(path) => Outcome::Exists(path),
        None => Outcome::Written(write_starter(home)?),
    };
    if (once || matches!(outcome, Outcome::Written(_)))
        && let Some(marker) = marker
    {
        // Without the marker, a later `--once` writes a starter where the user deleted
        // one: worth a warning, not a failure.
        if let Err(e) = mark(marker) {
            eprintln!("cash: {}: {e}", marker.display());
        }
    }
    Ok(outcome)
}

fn write_starter(home: &Path) -> Result<PathBuf, String> {
    let path = home.join(".bashrc");
    // `create_new`: never over a file that appeared since the check.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(STARTER.as_bytes())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

fn mark(marker: &Path) -> std::io::Result<()> {
    if let Some(dir) = marker.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(
        marker,
        "`cash --init-rc --once` has run for this user; delete this file to let it run again.\n",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dirs {
        _root: tempfile::TempDir,
        home: PathBuf,
        marker: PathBuf,
    }

    fn dirs() -> Dirs {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        std::fs::create_dir(&home).unwrap();
        let marker = root.path().join("local").join("cash").join("init-rc");
        Dirs {
            _root: root,
            home,
            marker,
        }
    }

    #[test]
    fn a_user_with_no_startup_file_gets_the_starter() {
        let d = dirs();
        let outcome = init(&d.home, Some(d.marker.as_path()), false).unwrap();
        assert_eq!(outcome, Outcome::Written(d.home.join(".bashrc")));
        assert_eq!(
            std::fs::read_to_string(d.home.join(".bashrc")).unwrap(),
            STARTER
        );
        assert!(d.marker.exists(), "no marker left");
    }

    #[test]
    fn a_bashrc_or_a_cashrc_is_left_alone() {
        for name in [".bashrc", ".cashrc"] {
            let d = dirs();
            std::fs::write(d.home.join(name), "# mine\n").unwrap();
            let outcome = init(&d.home, Some(d.marker.as_path()), false).unwrap();
            assert_eq!(outcome, Outcome::Exists(d.home.join(name)));
            assert_eq!(
                std::fs::read_to_string(d.home.join(name)).unwrap(),
                "# mine\n"
            );
            assert!(!d.home.join(".bashrc").exists() || name == ".bashrc");
        }
    }

    #[test]
    fn once_acts_only_the_first_time() {
        let d = dirs();
        // The first run finds the user's own file, and still counts.
        std::fs::write(d.home.join(".bashrc"), "# mine\n").unwrap();
        init(&d.home, Some(d.marker.as_path()), true).unwrap();
        // Deleted since: a later update does not write a starter in its place.
        std::fs::remove_file(d.home.join(".bashrc")).unwrap();
        let outcome = init(&d.home, Some(d.marker.as_path()), true).unwrap();
        assert_eq!(outcome, Outcome::DoneBefore);
        assert!(!d.home.join(".bashrc").exists());
        // By hand, without --once, it writes one.
        let outcome = init(&d.home, Some(d.marker.as_path()), false).unwrap();
        assert_eq!(outcome, Outcome::Written(d.home.join(".bashrc")));
    }

    #[test]
    fn the_starter_has_lf_line_endings_as_git_bash_needs() {
        assert!(!STARTER.contains('\r'), "the starter has CRLF line endings");
        assert!(STARTER.starts_with("# ~/.bashrc"), "not the starter bashrc");
    }

    /// `cash: F1 for help` follows the banner, under the same once-per-window condition,
    /// in the section Git Bash skips.
    #[test]
    fn the_starter_says_where_help_is_after_the_banner() {
        let cash_only = STARTER
            .split("if [[ -n $CASH_VERSION ]]; then")
            .last()
            .unwrap();
        let banner = cash_only.find("coolfetch\n").unwrap();
        let help = cash_only.find("echo 'cash: F1 for help'\n").unwrap();
        assert!(banner < help, "{cash_only}");
        assert_eq!(cash_only.matches("fi\n").count(), 2, "{cash_only}");
        assert!(
            cash_only.contains(
                "<= 1)); then\n        coolfetch\n        echo 'cash: F1 for help'\n    fi\n"
            ),
            "{cash_only}"
        );
    }
}
