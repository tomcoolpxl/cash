//! GUI applications outlive cash; console programs do not — **D6**.
//!
//! `code .` from PowerShell leaves VS Code open after the shell exits. cash's session
//! job used to reap it with everything else. Now an orderly exit ends the console
//! programs cash started and lets GUI applications go, unless the session said
//! `cashctl gui-apps close`.
//!
//! `wscript //B` stands in for the GUI application: it is built for the GUI subsystem
//! and runs a script without showing a window.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result."
)]

use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::common::{cash_command, system32, unix_spelling};
use crate::process_identity::still_running;

fn wscript() -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(&system32()).join("wscript.exe");
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
/// the background, and returns their pids once cash has exited, and when it had.
///
/// The time tells each program from a later process given its pid (see
/// `process_identity.rs`): both ran until cash exited, and Windows hands a pid out again
/// only after its process has ended.
fn run_and_exit(setting: &str) -> (u32, u32, u64) {
    let dir = tempfile::tempdir().expect("scratch dir");
    std::fs::write(dir.path().join("wait.vbs"), "WScript.Sleep 30000\r\n").expect("vbs");
    let wscript = format!(
        "{}/wscript.exe",
        unix_spelling(std::path::Path::new(&system32()))
    );
    let script = format!(
        "{setting}\n\
         {wscript} //B //Nologo wait.vbs </dev/null >/dev/null 2>&1 &\n\
         echo gui=$!\n\
         ping -n 30 127.0.0.1 </dev/null >/dev/null 2>&1 &\n\
         echo con=$!\n\
         sleep 1\n"
    );
    // Output goes to a file, not a pipe: a surviving program inherits cash's handles,
    // and reading a pipe to its end would wait for that program as well as for cash.
    let log = dir.path().join("out.txt");
    let status = cash_command()
        .args(["-c", &script])
        .current_dir(dir.path())
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&log).expect("log"))
        .stderr(Stdio::null())
        .status()
        .expect("failed to run cash");
    let exited = cash_win32::process::now_filetime();
    let stdout = std::fs::read_to_string(&log).unwrap_or_default();
    let pid = |key: &str| -> u32 {
        stdout
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or_else(|| panic!("no {key} in {stdout:?}; cash exited {status}"))
    };
    (pid("gui="), pid("con="), exited)
}

/// Ends the program cash started with this pid, if it is still running: not a later
/// process that was given the pid.
fn kill(pid: u32, exited: u64) {
    if still_running(pid, exited) {
        let _ = cash_win32::process::terminate(pid, 1);
    }
}

#[test]
fn a_gui_application_outlives_cash_and_a_console_program_does_not() {
    // Windows Script Host comes with Windows; a machine without it fails the test
    // rather than passing it unrun (BIN-20).
    assert!(wscript().is_some(), "no wscript.exe in System32");
    let (gui, con, exited) = run_and_exit("");
    let console_gone = wait_until(Duration::from_secs(5), || !still_running(con, exited));
    let gui_alive = still_running(gui, exited);
    kill(gui, exited);
    kill(con, exited);
    assert!(gui_alive, "the GUI program {gui} was closed with cash");
    assert!(console_gone, "the console program {con} outlived cash");
}

#[test]
fn gui_apps_close_reaps_the_gui_application_too() {
    // Windows Script Host comes with Windows; a machine without it fails the test
    // rather than passing it unrun (BIN-20).
    assert!(wscript().is_some(), "no wscript.exe in System32");
    let (gui, con, exited) = run_and_exit("cashctl gui-apps close");
    let gui_gone = wait_until(Duration::from_secs(5), || !still_running(gui, exited));
    let console_gone = wait_until(Duration::from_secs(5), || !still_running(con, exited));
    kill(gui, exited);
    kill(con, exited);
    assert!(
        gui_gone,
        "the GUI program {gui} outlived cash under `gui-apps close`"
    );
    assert!(console_gone, "the console program {con} outlived cash");
}

#[test]
fn a_program_started_while_the_console_closes_does_not_outlive_cash() {
    // Closing the console lets go of the session job's hold for GUI applications, after
    // ending the console programs it holds; the script goes on starting programs
    // meanwhile, and one started between the two outlived cash, hung on the console that
    // was gone (a bundled `cat` held cash.exe so a build could not replace it). The
    // programs are a copy of cmd.exe by a name of this test's own, to be found by it;
    // one that runs its course writes `survivors`.
    let dir = tempfile::tempdir().expect("scratch dir");
    let name = format!("outlives{}.exe", std::process::id());
    std::fs::copy(
        std::path::Path::new(&system32()).join("cmd.exe"),
        dir.path().join(&name),
    )
    .expect("copy cmd");
    std::fs::write(
        dir.path().join("script.sh"),
        format!(
            "while :; do ./{name} /d /c \"ping -n 3 127.0.0.1 >nul && echo alive>>survivors\" \
             </dev/null >/dev/null 2>&1 & done\n"
        ),
    )
    .expect("write the script");
    let session = crate::common::with_isolated_environment(|env| {
        cash_win32::conpty::ConPtySession::start_in(
            std::path::Path::new(crate::common::CASH),
            &["--no-config", "script.sh"],
            Some(env),
            Some(dir.path()),
        )
    })
    .expect("start cash on a pseudo console");
    std::thread::sleep(Duration::from_secs(1));
    let cash = session.pid();
    drop(session);
    let running = |wanted: &dyn Fn(&cash_win32::process::ProcessInfo) -> bool| {
        cash_win32::process::list()
            .into_iter()
            .filter(|p| wanted(p))
            .map(|p| p.pid)
            .collect::<Vec<u32>>()
    };
    assert!(
        wait_until(Duration::from_secs(10), || running(&|p| p.pid == cash)
            .is_empty()),
        "cash did not end with its console"
    );
    std::thread::sleep(Duration::from_millis(500));
    let left = running(&|p| p.name.eq_ignore_ascii_case(&name));
    for &pid in &left {
        let _ = cash_win32::process::terminate(pid, 1);
    }
    // Long enough for one that outlived cash to write its line too.
    std::thread::sleep(Duration::from_secs(3));
    let survivors = std::fs::read_to_string(dir.path().join("survivors")).unwrap_or_default();
    assert_eq!(
        (left.len(), survivors.lines().count()),
        (0, 0),
        "programs outlived cash, whose console closed"
    );
}

#[test]
fn cashctl_gui_apps_reports_the_setting() {
    let out = cash_command()
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
