//! `cash --terminal-profile` and `--remove-terminal-profile` (spec D38, ROADMAP item 18):
//! the Windows Terminal fragment Scoop's manifest writes on install and removes on
//! uninstall. Each test points `LOCALAPPDATA` at a folder of its own.
#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction, and a failed \
              assumption in a test should abort it loudly"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::common::{CASH, Scratch};

fn local_app_data(name: &str) -> Scratch {
    Scratch::new(&format!("terminal-profile-{name}"))
}

/// Registry key naming the installed fonts for a test: one that does not exist, so no
/// font is installed, whatever this machine has.
const NO_FONTS: &str = r"Software\cash-test-no-fonts";

fn cash(local: &Path, arg: &str) -> Output {
    cash_with_fonts(local, arg, NO_FONTS)
}

/// `cash ARG`. Here and below, not `cash_command()`: `--terminal-profile` and
/// `--remove-terminal-profile` are only the subcommands as cash's first argument, and
/// neither reads the config or runs a script.
fn cash_with_fonts(local: &Path, arg: &str, fonts_key: &str) -> Output {
    Command::new(CASH)
        .arg(arg)
        .env("LOCALAPPDATA", local)
        .env("CASH_FONTS_KEY", fonts_key)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

/// A registry key of one test's own under `HKEY_CURRENT_USER` listing `fonts` as Windows
/// lists installed ones; deleted when dropped.
struct Fonts(String);

impl Fonts {
    fn new(name: &str, fonts: &[&str]) -> Self {
        let key = format!(r"Software\cash-test-fonts-{}-{name}", std::process::id());
        for font in fonts {
            let status = Command::new("reg")
                .args(["add", &format!(r"HKCU\{key}"), "/v", font])
                .args(["/t", "REG_SZ", "/d", "x.ttf", "/f"])
                .stdout(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
        }
        Self(key)
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        let _ = Command::new("reg")
            .args(["delete", &format!(r"HKCU\{}", self.0), "/f"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[test]
fn an_installed_cascadia_nerd_font_becomes_the_profile_font() {
    let local = local_app_data("font");
    let fonts = Fonts::new(
        "caskaydia",
        &[
            "Cascadia Mono Regular (TrueType)",
            "CaskaydiaMono NFM Bold (TrueType)",
            "CaskaydiaMono NFM (TrueType)",
        ],
    );
    let out = cash_with_fonts(local.path(), "--terminal-profile", &fonts.0);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json = std::fs::read_to_string(fragments(local.path()).join("cash.json")).unwrap();
    assert!(
        json.contains(r#""font": { "face": "CaskaydiaMono Nerd Font Mono" }"#),
        "{json}"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("a Nerd Font"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );

    // No Cascadia one, but another monospaced Nerd Font installed: that one, Mono first.
    let other = Fonts::new(
        "other",
        &[
            "Cascadia Mono Regular (TrueType)",
            "Hack Nerd Font Propo Regular (TrueType)",
            "UbuntuSansMono NFM (TrueType)",
        ],
    );
    assert!(
        cash_with_fonts(local.path(), "--terminal-profile", &other.0)
            .status
            .success()
    );
    let json = std::fs::read_to_string(fragments(local.path()).join("cash.json")).unwrap();
    assert!(
        json.contains(r#""font": { "face": "UbuntuSansMono Nerd Font Mono" }"#),
        "{json}"
    );

    // No Nerd Font at all: Terminal's own font, and the suggestion.
    let plain = Fonts::new("plain", &["Cascadia Mono Regular (TrueType)"]);
    let out = cash_with_fonts(local.path(), "--terminal-profile", &plain.0);
    let json = std::fs::read_to_string(fragments(local.path()).join("cash.json")).unwrap();
    assert!(!json.contains("\"font\""), "{json}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("nerd-fonts/CascadiaMono-NF"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn the_nerd_font_the_user_already_uses_in_terminal_is_taken() {
    let local = local_app_data("font-in-use");
    let dir = local
        .join("Packages")
        .join("Microsoft.WindowsTerminal_8wekyb3d8bbwe")
        .join("LocalState");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("settings.json"),
        r#"{ "defaultProfile": "{465d1d2d-478a-4eee-8c87-cd7cafd28372}",
             "profiles": { "list": [ { "guid": "{465d1d2d-478a-4eee-8c87-cd7cafd28372}",
                                       "name": "Bash",
                                       "font": { "face": "UbuntuSansMono Nerd Font Mono" } } ] } }"#,
    )
    .unwrap();
    // Installed, the registry says only JetBrains Mono's; the user's own choice wins.
    let fonts = Fonts::new("in-use", &["JetBrainsMono NFM (TrueType)"]);
    assert!(
        cash_with_fonts(local.path(), "--terminal-profile", &fonts.0)
            .status
            .success()
    );
    let json = std::fs::read_to_string(fragments(local.path()).join("cash.json")).unwrap();
    assert!(
        json.contains(r#""font": { "face": "UbuntuSansMono Nerd Font Mono" }"#),
        "{json}"
    );
}

fn fragments(local: &Path) -> PathBuf {
    local
        .join("Microsoft")
        .join("Windows Terminal")
        .join("Fragments")
        .join("cash")
}

#[test]
fn the_profile_runs_this_cash_and_goes_away_again() {
    let local = local_app_data("write");
    let out = cash(local.path(), "--terminal-profile");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let folder = fragments(local.path());
    let json = std::fs::read_to_string(folder.join("cash.json")).unwrap();
    // The program as this test started it, quoted, with JSON's doubled backslashes.
    let exe = Path::new(CASH).display().to_string().replace('/', "\\");
    let quoted = format!("\\\"{}\\\"", exe.replace('\\', "\\\\"));
    assert!(
        json.to_lowercase().contains(&quoted.to_lowercase()),
        "{json}\nexpected {quoted}"
    );
    assert!(json.contains("\"name\": \"cash\""), "{json}");
    // Home, not the folder Terminal itself runs in (C:\WINDOWS\system32).
    assert!(
        json.contains("\"startingDirectory\": \"%USERPROFILE%\""),
        "{json}"
    );

    // The icon: an address whose file name Windows Terminal 1.24 and later finds beside
    // the fragment, and the logo written there under it.
    assert!(
        json.contains(
            "\"icon\": \"https://raw.githubusercontent.com/tomcoolpxl/cash/main/assets/cash_logo_small.png\""
        ),
        "{json}"
    );
    let logo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/cash_logo_small.png");
    assert_eq!(
        std::fs::read(folder.join("cash_logo_small.png")).unwrap(),
        std::fs::read(logo).unwrap()
    );

    // Written again, as every upgrade does: still one profile.
    assert!(cash(local.path(), "--terminal-profile").status.success());
    assert_eq!(
        std::fs::read_to_string(folder.join("cash.json")).unwrap(),
        json
    );

    let gone = cash(local.path(), "--remove-terminal-profile");
    assert!(gone.status.success());
    assert!(!folder.exists());

    // Removing what is not there is not an error: Scoop may run it twice.
    let again = cash(local.path(), "--remove-terminal-profile");
    assert!(again.status.success());
    assert!(String::from_utf8_lossy(&again.stdout).contains("no profile to remove"));
}

/// A Terminal settings file of the Store's install, under `local`.
fn store_settings(local: &Path, text: &str) -> PathBuf {
    let dir = local
        .join("Packages")
        .join("Microsoft.WindowsTerminal_8wekyb3d8bbwe")
        .join("LocalState");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("settings.json");
    std::fs::write(&file, text).unwrap();
    file
}

const GUID: &str = "{43e4cdd3-eb67-5e13-bd17-fa0d7f8cf3ff}";

/// Settings shaped like the author's, CRLF and all: a + menu listing profiles one by
/// one, in which a profile from a fragment shows nowhere.
const LISTED_MENU: &str = "{\r\n    \"$schema\": \"https://aka.ms/terminal-profiles-schema\",\r\n    \"defaultProfile\": \"{465d1d2d-478a-4eee-8c87-cd7cafd28372}\",\r\n    // the menu, as the user laid it out\r\n    \"newTabMenu\": \r\n    [\r\n        {\r\n            \"icon\": null,\r\n            \"profile\": \"{465d1d2d-478a-4eee-8c87-cd7cafd28372}\",\r\n            \"type\": \"profile\"\r\n        }\r\n    ],\r\n    \"profiles\": { \"list\": [] }\r\n}\r\n";

/// A settings.json that is a symbolic link, as a dotfile manager makes it, stays one, and
/// the file it names gets the entry: the link was replaced by a file (BIN-10).
#[test]
fn a_linked_settings_file_stays_a_link() {
    let local = local_app_data("menu-link");
    let settings = store_settings(local.path(), LISTED_MENU);
    let real = local.join("dotfiles-settings.json");
    std::fs::rename(&settings, &real).unwrap();
    if let Err(e) = std::os::windows::fs::symlink_file(&real, &settings) {
        // Without Developer Mode or elevation Windows makes no symbolic link; CI has it.
        eprintln!("skipped: no symbolic link could be made: {e}");
        return;
    }

    assert!(cash(local.path(), "--terminal-profile").status.success());
    let is_link = |path: &Path| {
        std::fs::symlink_metadata(path)
            .unwrap()
            .file_type()
            .is_symlink()
    };
    assert!(is_link(&settings), "settings.json is no longer a link");
    let text = std::fs::read_to_string(&real).unwrap();
    assert!(text.contains(&format!("\"profile\": \"{GUID}\"")), "{text}");

    assert!(
        cash(local.path(), "--remove-terminal-profile")
            .status
            .success()
    );
    assert!(is_link(&settings), "settings.json is no longer a link");
    assert_eq!(std::fs::read_to_string(&real).unwrap(), LISTED_MENU);
}

#[test]
fn a_menu_listed_profile_by_profile_gets_cash_and_loses_it_on_removal() {
    let local = local_app_data("menu");
    let settings = store_settings(local.path(), LISTED_MENU);
    let unpackaged_dir = local.join("Microsoft").join("Windows Terminal");
    std::fs::create_dir_all(&unpackaged_dir).unwrap();
    let unpackaged = unpackaged_dir.join("settings.json");
    std::fs::write(&unpackaged, LISTED_MENU).unwrap();

    let out = cash(local.path(), "--terminal-profile");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("added to the + menu"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    for file in [&settings, &unpackaged] {
        let text = std::fs::read_to_string(file).unwrap();
        assert!(text.contains(&format!("\"profile\": \"{GUID}\"")), "{text}");
        // Everything the user wrote is still there, comment included.
        assert!(
            text.contains("// the menu, as the user laid it out"),
            "{text}"
        );
        assert!(!text.contains("\n    {\n"), "CRLF kept: {text:?}");
    }
    // The fragment names the same profile.
    let fragment = std::fs::read_to_string(fragments(local.path()).join("cash.json")).unwrap();
    assert!(
        fragment.contains(&format!("\"guid\": \"{GUID}\"")),
        "{fragment}"
    );

    // An upgrade writes it again: the menu keeps one entry.
    let before = std::fs::read_to_string(&settings).unwrap();
    assert!(cash(local.path(), "--terminal-profile").status.success());
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), before);

    // Uninstalled: the menu is as the user left it, to the byte.
    assert!(
        cash(local.path(), "--remove-terminal-profile")
            .status
            .success()
    );
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), LISTED_MENU);
    assert_eq!(std::fs::read_to_string(&unpackaged).unwrap(), LISTED_MENU);
}

/// A fragment folder that will not go, held open by Terminal or a scanner, still lets
/// the menu entry go; the removal fails and names the folder (BIN-11).
#[test]
fn a_folder_that_will_not_go_still_takes_the_menu_entry_out() {
    use std::os::windows::fs::OpenOptionsExt;

    let local = local_app_data("menu-held");
    let settings = store_settings(local.path(), LISTED_MENU);
    assert!(cash(local.path(), "--terminal-profile").status.success());
    assert_ne!(std::fs::read_to_string(&settings).unwrap(), LISTED_MENU);

    // No sharing at all: the file can be neither deleted nor renamed while it is open.
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(fragments(local.path()).join("cash.json"))
        .unwrap();
    let out = cash(local.path(), "--remove-terminal-profile");
    drop(held);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Fragments"), "{stderr}");
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), LISTED_MENU);
}

#[test]
fn a_menu_that_shows_every_profile_is_not_touched() {
    let local = local_app_data("menu-untouched");
    let plain = "{\n    \"profiles\": { \"list\": [] }\n}\n";
    let settings = store_settings(local.path(), plain);
    assert!(cash(local.path(), "--terminal-profile").status.success());
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), plain);

    let remaining = "{ \"newTabMenu\": [ { \"type\": \"remainingProfiles\" } ] }";
    std::fs::write(&settings, remaining).unwrap();
    assert!(cash(local.path(), "--terminal-profile").status.success());
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), remaining);
}

#[test]
fn an_extra_argument_is_refused() {
    let local = local_app_data("extra");
    let out = Command::new(CASH)
        .args(["--terminal-profile", "now"])
        .env("LOCALAPPDATA", local.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(!fragments(local.path()).exists());
}
