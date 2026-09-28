//! GUI applications outlive cash; console programs do not — **D6**.
//!
//! `code .` from PowerShell leaves VS Code open after the shell exits. cash's session
//! job used to reap it with everything else. Now an orderly exit ends the console
//! programs cash started and lets GUI applications go, unless the session said
//! `cashctl gui-apps close`.
//!
//! `wscript //B` stands in for the GUI application: it is built for the GUI subsystem
//! and runs a script without showing a window.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result."
)]

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use cash_win32::process::is_pid_alive;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

fn wscript() -> Option<std::path::PathBuf> {
    let path = std::path::PathBuf::from(r"C:\Windows\System32\wscript.exe");
    path.is_file().then_some(path)
}

fn wait_until<F: FnMut() -> bool>(timeout: Duration, mut predicate: F) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    predicate()
}

/// Runs cash with `setting` first, starts a hidden GUI program and a console program in
/// the background, and returns their pids once cash has exited.
fn run_and_exit(setting: &str) -> (u32, u32) {
    let dir = tempfile::tempdir().expect("scratch dir");
    std::fs::write(dir.path().join("wait.vbs"), "WScript.Sleep 30000\r\n").expect("vbs");
    let script = format!(
        "{setting}\n\
         /c/Windows/System32/wscript.exe //B //Nologo wait.vbs </dev/null >/dev/null 2>&1 &\n\
         echo gui=$!\n\
         ping -n 30 127.0.0.1 </dev/null >/dev/null 2>&1 &\n\
         echo con=$!\n\
         sleep 1\n"
    );
    // Output goes to a file, not a pipe: a surviving program inherits cash's handles,
    // and reading a pipe to its end would wait for that program as well as for cash.
    let log = dir.path().join("out.txt");
    let status = Command::new(CASH)
        .args(["-c", &script])
        .current_dir(dir.path())
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&log).expect("log"))
        .stderr(Stdio::null())
        .status()
        .expect("failed to run cash");
    let stdout = std::fs::read_to_string(&log).unwrap_or_default();
    let pid = |key: &str| -> u32 {
        stdout
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or_else(|| panic!("no {key} in {stdout:?}; cash exited {status}"))
    };
    (pid("gui="), pid("con="))
}

fn kill(pid: u32) {
    let _ = cash_win32::process::terminate(pid, 1);
}

#[test]
fn a_gui_application_outlives_cash_and_a_console_program_does_not() {
    if wscript().is_none() {
        return;
    }
    let (gui, con) = run_and_exit("");
    let console_gone = wait_until(Duration::from_secs(5), || !is_pid_alive(con));
    let gui_alive = is_pid_alive(gui);
    kill(gui);
    kill(con);
    assert!(gui_alive, "the GUI program {gui} was closed with cash");
    assert!(console_gone, "the console program {con} outlived cash");
}

#[test]
fn gui_apps_close_reaps_the_gui_application_too() {
    if wscript().is_none() {
        return;
    }
    let (gui, con) = run_and_exit("cashctl gui-apps close");
    let gui_gone = wait_until(Duration::from_secs(5), || !is_pid_alive(gui));
    let console_gone = wait_until(Duration::from_secs(5), || !is_pid_alive(con));
    kill(gui);
    kill(con);
    assert!(
        gui_gone,
        "the GUI program {gui} outlived cash under `gui-apps close`"
    );
    assert!(console_gone, "the console program {con} outlived cash");
}

#[test]
fn cashctl_gui_apps_reports_the_setting() {
    let out = Command::new(CASH)
        .args([
            "-c",
            "cashctl gui-apps; cashctl gui-apps close; cashctl gui-apps",
        ])
        .output()
        .expect("failed to run cash");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "outlive\nclose",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
