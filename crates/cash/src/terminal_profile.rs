//! `cash --terminal-profile`: a Windows Terminal profile for cash, written as a JSON
//! fragment (spec D38, ROADMAP item 18); `cash --remove-terminal-profile` takes it away.
//! Scoop's manifest runs both, on install and uninstall.
//!
//! Windows Terminal reads fragments from `%LOCALAPPDATA%\Microsoft\Windows
//! Terminal\Fragments\<app>\` and lists each profile in them. The profile runs the
//! `cash.exe` this command ran as, spelled as it was started: under Scoop that is
//! `apps\cash\current\cash.exe`, which follows upgrades, not the version folder behind
//! it. Where the user laid out Terminal's + menu profile by profile, cash adds itself to it
//! ([`crate::terminal_menu`]), or the profile could not be opened from there.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The fragment's folder name, which Windows Terminal takes as the application's.
const APP: &str = "cash";

/// The profile's GUID: the one Terminal derives for a profile named `cash` from the
/// application `cash`, written out so that the + menu entry and the profile agree.
const PROFILE_GUID: &str = "{43e4cdd3-eb67-5e13-bd17-fa0d7f8cf3ff}";

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

/// The Cascadia Nerd Fonts, asked for first so that the profile looks like Terminal's
/// own font. Each is Terminal's name for the family, then the names Windows lists its
/// installed fonts under (`CaskaydiaMono NFM Bold (TrueType)`). The first is what Scoop's
/// notes suggest: `scoop install nerd-fonts/CascadiaMono-NF`.
const CASCADIA_NERD_FONTS: &[(&str, &[&str])] = &[
    (
        "CaskaydiaMono Nerd Font Mono",
        &["CaskaydiaMono NFM", "CaskaydiaMono Nerd Font Mono"],
    ),
    ("Cascadia Mono NF", &["Cascadia Mono NF"]),
    (
        "CaskaydiaCove Nerd Font Mono",
        &["CaskaydiaCove NFM", "CaskaydiaCove Nerd Font Mono"],
    ),
    ("Cascadia Code NF", &["Cascadia Code NF"]),
];

/// The Nerd Font the profile asks for, so that `ls --icons` draws its file icons: `None`
/// when this machine has none, and the profile keeps Terminal's font.
fn nerd_font(local: &Path) -> Option<String> {
    let program_data = std::env::var_os("ProgramData").map(PathBuf::from);
    choose_nerd_font(
        &cash_win32::terminal::installed_fonts(),
        &cash_win32::terminal::nerd_fonts_in_use(local, program_data.as_deref()),
        &cash_win32::terminal::monospace_families(),
    )
}

/// Which Nerd Font, decided with the user, 2026-09-28: an installed Cascadia one, to look
/// like Terminal's own; else the Nerd Font the user already has Terminal draw in
/// (`in_use`, the default profile's first); else any Nerd Font Windows draws with a fixed
/// pitch (`monospace`), `Nerd Font Mono` ones first. A Nerd Font of a proportional face
/// (Ubuntu, Noto Sans) would put letters of every width in the grid, so it is never
/// picked from what is merely installed.
fn choose_nerd_font(
    installed: &[String],
    in_use: &[String],
    monospace: &[String],
) -> Option<String> {
    let has = |listed: &str| {
        installed.iter().any(|font| {
            font.strip_prefix(listed)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '(']))
        })
    };
    if let Some((family, _)) = CASCADIA_NERD_FONTS
        .iter()
        .find(|(_, listed)| listed.iter().any(|name| has(name)))
    {
        return Some((*family).to_owned());
    }
    if let Some(font) = in_use.first() {
        return Some(font.clone());
    }
    monospace
        .iter()
        .filter_map(|family| cash_win32::terminal::terminal_family(family))
        .min()
        .map(|(_, family)| family)
}

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

/// `%LOCALAPPDATA%`, where Terminal keeps its settings and reads fragments.
fn local_app_data() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "LOCALAPPDATA is not set".to_owned())
}

/// Where the fragment goes.
fn folder(local: &Path) -> PathBuf {
    local
        .join("Microsoft")
        .join("Windows Terminal")
        .join("Fragments")
        .join(APP)
}

fn write_profile() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find cash.exe: {e}"))?;
    let exe = cash_win32::path::to_backslash(&exe);
    let local = local_app_data()?;
    let dir = folder(&local);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", render(&dir)))?;

    let icon = dir.join(ICON_FILE);
    std::fs::write(&icon, ICON).map_err(|e| format!("{}: {e}", render(&icon)))?;
    let font = nerd_font(&local);
    let fragment = dir.join("cash.json");
    std::fs::write(&fragment, fragment_json(&exe, font.as_deref()))
        .map_err(|e| format!("{}: {e}", render(&fragment)))?;

    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "cash --terminal-profile: {}", render(&fragment));
    let _ = writeln!(out, "  a \"cash\" profile running {exe}");
    if let Some(font) = font {
        let _ = writeln!(
            out,
            "  in {font}, a Nerd Font, so `ls --icons` draws its file icons"
        );
    } else {
        let _ = writeln!(
            out,
            "  in Terminal's own font; for the file icons of `ls --icons`, install a Nerd Font \
             (scoop install nerd-fonts/CascadiaMono-NF) and run this again"
        );
    }
    let menus = edit_menus(&local, |text| {
        crate::terminal_menu::with_profile(text, PROFILE_GUID)
    });
    for settings in &menus {
        let _ = writeln!(
            out,
            "  added to the + menu listed in {}, which Terminal reloads at once",
            render(settings)
        );
    }
    if menus.is_empty() {
        let _ = writeln!(out, "  Windows Terminal lists it from its next start");
    }
    Ok(())
}

fn remove_profile() -> Result<(), String> {
    let local = local_app_data()?;
    let dir = folder(&local);
    let mut out = std::io::stdout().lock();
    // The menu entry goes whether or not the folder does: a folder that would not go
    // returned before the menus were looked at, and left the entry (BIN-11).
    let removed = if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map(|()| true)
            .map_err(|e| format!("{}: {e}", render(&dir)))
    } else {
        Ok(false)
    };
    match removed {
        Ok(true) => {
            let _ = writeln!(
                out,
                "cash --remove-terminal-profile: {} removed",
                render(&dir)
            );
        }
        Ok(false) => {
            let _ = writeln!(out, "cash --remove-terminal-profile: no profile to remove");
        }
        Err(_) => {}
    }
    for settings in edit_menus(&local, |text| {
        crate::terminal_menu::without_profile(text, PROFILE_GUID)
    }) {
        let _ = writeln!(out, "  taken out of the + menu in {}", render(&settings));
    }
    removed.map(|_| ())
}

/// Apply `edit` to each Terminal install's `settings.json`, writing back those it
/// changes; the files changed. A file that cannot be read or written is named on
/// standard error and left as it was: the profile still works without the menu entry.
fn edit_menus(local: &Path, edit: impl Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut changed = Vec::new();
    for settings in cash_win32::terminal::settings_files(local) {
        let result = std::fs::read_to_string(&settings).and_then(|text| match edit(&text) {
            Some(new) => replace_file(&settings, &new).map(|()| true),
            None => Ok(false),
        });
        match result {
            Ok(true) => changed.push(settings),
            Ok(false) => {}
            Err(e) => eprintln!("cash: {}: {e}", render(&settings)),
        }
    }
    changed
}

/// Write `path` whole or not at all: a file beside it, renamed over it, so Terminal,
/// which rereads the file as it changes, never sees half of it.
fn replace_file(path: &Path, text: &str) -> std::io::Result<()> {
    // The file a symbolic link names, as dotfile managers link settings.json: the rename
    // put a file where the link was (BIN-10).
    let target = if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        std::fs::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    let path = target.as_path();
    let temporary = path.with_extension("json.cash-new");
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

/// The fragment: one profile, named `cash`, running `exe` in the user's home folder with
/// the logo as its icon, and with command marks on. Without `startingDirectory`, Windows
/// Terminal starts a fragment's profile in its own folder, `C:\WINDOWS\system32` when it
/// was started from the Start menu (seen 2026-09-28). `autoMarkPrompts` marks the line
/// where each Enter was pressed, even when the prompt sends no `OSC 133`, and
/// `showMarksOnScrollbar` shows the marks, which `scrollToMark` then jumps between
/// (decided with the user, 2026-09-28). The font is `font` when one is given, a Nerd Font
/// installed on this machine ([`nerd_font`]); otherwise Terminal's own, or whatever the
/// user's `profiles.defaults` names. Colours stay the user's.
fn fragment_json(exe: &str, font: Option<&str>) -> String {
    let font = font.map_or_else(String::new, |face| {
        format!("      \"font\": {{ \"face\": {} }},\n", json_string(face))
    });
    format!(
        "{{\n  \"profiles\": [\n    {{\n      \"guid\": \"{PROFILE_GUID}\",\n      \
         \"name\": \"cash\",\n      \
         \"commandline\": {},\n      \"startingDirectory\": \"%USERPROFILE%\",\n      \
         \"icon\": {},\n{font}      \
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
        let json = fragment_json(r"C:\Program Files\cash\cash.exe", None);
        assert!(!json.contains("\"font\""), "{json}");
        let with_font = fragment_json("x", Some("CaskaydiaMono Nerd Font Mono"));
        assert!(
            with_font.contains(r#""font": { "face": "CaskaydiaMono Nerd Font Mono" },"#),
            "{with_font}"
        );
        assert!(
            json.contains(r#""commandline": "\"C:\\Program Files\\cash\\cash.exe\"""#),
            "{json}"
        );
        assert!(json.contains(&format!(r#""icon": "{ICON_URL}""#)), "{json}");
        assert!(json.contains(r#""name": "cash""#), "{json}");
        assert!(json.contains(r#""showMarksOnScrollbar": true"#), "{json}");
        assert!(
            json.contains(r#""startingDirectory": "%USERPROFILE%""#),
            "{json}"
        );
        assert!(json.contains(r#""autoMarkPrompts": true"#), "{json}");
    }

    #[test]
    fn the_profile_guid_is_the_one_terminal_derives() {
        // UUIDv5 over UTF-16LE names: the application under Terminal's namespace for
        // fragments, then the profile's name under the application.
        let utf16 =
            |name: &str| -> Vec<u8> { name.encode_utf16().flat_map(u16::to_le_bytes).collect() };
        let fragments = uuid::Uuid::from_u128(0xf65d_db7e_706b_4499_8a50_4031_3caf_510a);
        let derive = |app: &str, profile: &str| {
            let app = uuid::Uuid::new_v5(&fragments, &utf16(app));
            format!("{{{}}}", uuid::Uuid::new_v5(&app, &utf16(profile)))
        };
        // Microsoft's own example: Git's "Git Bash".
        assert_eq!(
            derive("Git", "Git Bash"),
            "{2ece5bfe-50ed-5f3a-ab87-5cd4baafed2b}"
        );
        assert_eq!(derive(APP, "cash"), PROFILE_GUID);
        assert!(fragment_json("x", None).contains(&format!(r#""guid": "{PROFILE_GUID}""#)));
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn a_cascadia_nerd_font_comes_first_then_the_one_in_use_then_any_monospaced() {
        let caskaydia = names(&["CaskaydiaMono NFM Bold (TrueType)"]);
        let in_use = names(&["UbuntuSansMono Nerd Font Mono"]);
        let monospace = names(&["Consolas", "Hack Nerd Font Propo", "JetBrainsMono NFM"]);
        assert_eq!(
            choose_nerd_font(&caskaydia, &in_use, &monospace).as_deref(),
            Some("CaskaydiaMono Nerd Font Mono")
        );
        assert_eq!(
            choose_nerd_font(&[], &in_use, &monospace).as_deref(),
            Some("UbuntuSansMono Nerd Font Mono")
        );
        // A Mono one before a Propo one, whatever the alphabet says.
        assert_eq!(
            choose_nerd_font(&[], &[], &monospace).as_deref(),
            Some("JetBrainsMono Nerd Font Mono")
        );
        assert_eq!(choose_nerd_font(&[], &[], &names(&["Consolas"])), None);
        // Microsoft's own Cascadia NF, by its own name.
        assert_eq!(
            choose_nerd_font(&names(&["Cascadia Mono NF (TrueType)"]), &[], &[]).as_deref(),
            Some("Cascadia Mono NF")
        );
        // Cascadia Mono itself is no Nerd Font.
        assert_eq!(
            choose_nerd_font(&names(&["Cascadia Mono Regular (TrueType)"]), &[], &[]),
            None
        );
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
