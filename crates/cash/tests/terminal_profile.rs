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
