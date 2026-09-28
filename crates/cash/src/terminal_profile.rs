//! `cash --terminal-profile`: a Windows Terminal profile for cash, written as a JSON
//! fragment (spec D38, ROADMAP item 18); `cash --remove-terminal-profile` takes it away.
//! Scoop's manifest runs both, on install and uninstall.
//!
//! Windows Terminal reads fragments from `%LOCALAPPDATA%\Microsoft\Windows
//! Terminal\Fragments\<app>\` and lists each profile in them; it makes the profile's
//! GUID from the folder's name and the profile's, so rewriting the fragment keeps the
//! same profile. The profile runs the `cash.exe` this command ran as, spelled as it was
//! started: under Scoop that is `apps\cash\current\cash.exe`, which follows upgrades,
//! not the version folder behind it.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;

/// The fragment's folder name, which Windows Terminal takes as the application's.
const APP: &str = "cash";

/// The logo, written beside the fragment for the profile's icon.
const ICON: &[u8] = include_bytes!("../../../assets/cash_logo_small.png");

/// The profile's `icon`. Windows Terminal 1.24 and later loads a fragment's image from
/// the fragment's own folder, under this address's file name; earlier versions take
/// only a web address for a fragment's icon, and download it. With the logo written
/// beside the fragment under that name ([`ICON_FILE`]), every version shows it.
const ICON_URL: &str =
    "https://raw.githubusercontent.com/tomcoolpxl/cash/main/assets/cash_logo_small.png";

/// The name the logo is written under: [`ICON_URL`]'s file name.
const ICON_FILE: &str = "cash_logo_small.png";

const USAGE: &str = "usage: cash --terminal-profile\n       cash --remove-terminal-profile";

/// `cash --terminal-profile` or `cash --remove-terminal-profile`: the process exit
/// status, or `None` when the command line is neither.
pub fn command(args: &[String]) -> Option<u8> {
    let remove = match args.get(1).map(String::as_str) {
        Some("--terminal-profile") => false,
        Some("--remove-terminal-profile") => true,
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
        remove_profile()
    } else {
        write_profile()
    };
    Some(match result {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("cash: {message}");
            1
        }
    })
}

/// Where the fragment goes.
fn folder() -> Result<PathBuf, String> {
    let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
    Ok(PathBuf::from(local)
        .join("Microsoft")
        .join("Windows Terminal")
        .join("Fragments")
        .join(APP))
}

fn write_profile() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find cash.exe: {e}"))?;
    let exe = cash_win32::path::to_backslash(&exe);
    let dir = folder()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", render(&dir)))?;

    let icon = dir.join(ICON_FILE);
    std::fs::write(&icon, ICON).map_err(|e| format!("{}: {e}", render(&icon)))?;
    let fragment = dir.join("cash.json");
    std::fs::write(&fragment, fragment_json(&exe))
        .map_err(|e| format!("{}: {e}", render(&fragment)))?;

    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "cash --terminal-profile: {}", render(&fragment));
    let _ = writeln!(
        out,
        "  Windows Terminal lists a \"cash\" profile running {exe} from its next start"
    );
    Ok(())
}

fn remove_profile() -> Result<(), String> {
    let dir = folder()?;
    let mut out = std::io::stdout().lock();
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", render(&dir)))?;
        let _ = writeln!(
            out,
            "cash --remove-terminal-profile: {} removed",
            render(&dir)
        );
    } else {
        let _ = writeln!(out, "cash --remove-terminal-profile: no profile to remove");
    }
    Ok(())
}

/// The fragment: one profile, named `cash`, running `exe` with the logo as its icon, and
/// with command marks on. `autoMarkPrompts` marks the line where each Enter was pressed,
/// even when the prompt sends no `OSC 133`, and `showMarksOnScrollbar` shows the marks,
/// which `scrollToMark` then jumps between (decided with the user, 2026-09-28). Fonts and
/// colours stay the user's: a profile cannot know which fonts are installed.
fn fragment_json(exe: &str) -> String {
    format!(
        "{{\n  \"profiles\": [\n    {{\n      \"name\": \"cash\",\n      \
         \"commandline\": {},\n      \"icon\": {},\n      \
         \"showMarksOnScrollbar\": true,\n      \"autoMarkPrompts\": true\n    }}\n  ]\n}}\n",
        json_string(&format!("\"{exe}\"")),
        json_string(ICON_URL)
    )
}

/// `text` as a JSON string literal.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn render(path: &std::path::Path) -> String {
    cash_win32::path::render(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fragment_quotes_the_program_and_escapes_backslashes() {
        let json = fragment_json(r"C:\Program Files\cash\cash.exe");
        assert!(
            json.contains(r#""commandline": "\"C:\\Program Files\\cash\\cash.exe\"""#),
            "{json}"
        );
        assert!(json.contains(&format!(r#""icon": "{ICON_URL}""#)), "{json}");
        assert!(json.contains(r#""name": "cash""#), "{json}");
        assert!(json.contains(r#""showMarksOnScrollbar": true"#), "{json}");
        assert!(json.contains(r#""autoMarkPrompts": true"#), "{json}");
    }

    #[test]
    fn the_logo_is_written_under_the_icon_address_file_name() {
        assert_eq!(ICON_URL.rsplit('/').next(), Some(ICON_FILE));
    }

    #[test]
    fn control_characters_are_escaped() {
        assert_eq!(json_string("a\tb"), r#""a\u0009b""#);
    }
}
