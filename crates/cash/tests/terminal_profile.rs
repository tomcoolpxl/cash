//! `cash --terminal-profile` and `--remove-terminal-profile` (spec D38, ROADMAP item 18):
//! the Windows Terminal fragment Scoop's manifest writes on install and removes on
//! uninstall. Each test points `LOCALAPPDATA` at a folder of its own.
#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction, and a failed \
              assumption in a test should abort it loudly"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const CASH: &str = env!("CARGO_BIN_EXE_cash");

fn local_app_data(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("terminal_profile")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cash(local: &Path, arg: &str) -> Output {
    Command::new(CASH)
        .arg(arg)
        .env("LOCALAPPDATA", local)
        .stdin(Stdio::null())
        .output()
        .unwrap()
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
    let out = cash(&local, "--terminal-profile");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let folder = fragments(&local);
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
    assert!(cash(&local, "--terminal-profile").status.success());
    assert_eq!(
        std::fs::read_to_string(folder.join("cash.json")).unwrap(),
        json
    );

    let gone = cash(&local, "--remove-terminal-profile");
    assert!(gone.status.success());
    assert!(!folder.exists());

    // Removing what is not there is not an error: Scoop may run it twice.
    let again = cash(&local, "--remove-terminal-profile");
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

#[test]
fn a_menu_listed_profile_by_profile_gets_cash_and_loses_it_on_removal() {
    let local = local_app_data("menu");
    let settings = store_settings(&local, LISTED_MENU);
    let unpackaged_dir = local.join("Microsoft").join("Windows Terminal");
    std::fs::create_dir_all(&unpackaged_dir).unwrap();
    let unpackaged = unpackaged_dir.join("settings.json");
    std::fs::write(&unpackaged, LISTED_MENU).unwrap();

    let out = cash(&local, "--terminal-profile");
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
    let fragment = std::fs::read_to_string(fragments(&local).join("cash.json")).unwrap();
    assert!(
        fragment.contains(&format!("\"guid\": \"{GUID}\"")),
        "{fragment}"
    );

    // An upgrade writes it again: the menu keeps one entry.
    let before = std::fs::read_to_string(&settings).unwrap();
    assert!(cash(&local, "--terminal-profile").status.success());
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), before);

    // Uninstalled: the menu is as the user left it, to the byte.
    assert!(cash(&local, "--remove-terminal-profile").status.success());
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), LISTED_MENU);
    assert_eq!(std::fs::read_to_string(&unpackaged).unwrap(), LISTED_MENU);
}

#[test]
fn a_menu_that_shows_every_profile_is_not_touched() {
    let local = local_app_data("menu-untouched");
    let plain = "{\n    \"profiles\": { \"list\": [] }\n}\n";
    let settings = store_settings(&local, plain);
    assert!(cash(&local, "--terminal-profile").status.success());
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), plain);

    let remaining = "{ \"newTabMenu\": [ { \"type\": \"remainingProfiles\" } ] }";
    std::fs::write(&settings, remaining).unwrap();
    assert!(cash(&local, "--terminal-profile").status.success());
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), remaining);
}

#[test]
fn an_extra_argument_is_refused() {
    let local = local_app_data("extra");
    let out = Command::new(CASH)
        .args(["--terminal-profile", "now"])
        .env("LOCALAPPDATA", &local)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(!fragments(&local).exists());
}
