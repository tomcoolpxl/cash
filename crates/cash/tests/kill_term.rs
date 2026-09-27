//! `TERM`, `HUP`, `QUIT` and `INT` reach one process, not the console — **D21**.
//!
//! They used to be a `CTRL_BREAK_EVENT` aimed at the target's pid. Windows sends that to
//! a process group, and cash starts no child as a group leader; aimed at a pid that
//! leads no group, it goes to every process on the console. `kill -TERM $pid` killed
//! the shell that ran it (and whatever else shared the terminal), and `pkill`/`killall`,
//! which default to `TERM`, would have too.
//!
//! Now a program with a window is asked through it (`WM_CLOSE`, as `taskkill` without
//! `/F` asks) and terminated if it has not exited after a grace period; one without is
//! terminated at once. Every cash here runs in a console of its own, so a regression
//! can only kill that console, never the test runner.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction"
)]

use std::os::windows::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// A console of its own, without a window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct Output {
    stdout: String,
    code: i32,
}

/// Runs `script` in a cash that has a console to itself.
fn isolated_cash(script: &str) -> Output {
    let out = Command::new(CASH)
        .args(["-c", script])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

#[test]
fn kill_term_stops_a_background_job_and_the_shell_survives() {
    let out = isolated_cash(
        "ping.exe -n 60 127.0.0.1 >/dev/null & p=$!; sleep 1; kill -TERM $p; echo rc=$?; \
         sleep 0.5; kill -0 $p 2>/dev/null && echo alive || echo gone; echo shell-survived",
    );
    assert_eq!(out.stdout, "rc=0\ngone\nshell-survived");
    assert_eq!(out.code, 0);
}

#[test]
fn int_and_hup_reach_one_process_too() {
    let out = isolated_cash(
        "ping.exe -n 60 127.0.0.1 >/dev/null & a=$!; ping.exe -n 60 127.0.0.1 >/dev/null & b=$!; \
         sleep 1; kill -INT $a; kill -HUP $b; sleep 0.5; \
         kill -0 $a 2>/dev/null || kill -0 $b 2>/dev/null || echo both-gone; echo shell-survived",
    );
    assert_eq!(out.stdout, "both-gone\nshell-survived");
}

#[test]
fn pkill_and_killall_default_to_a_term_that_spares_the_shell() {
    let out = isolated_cash(
        "ping.exe -n 60 127.0.0.1 >/dev/null & a=$!; sleep 1; pkill -x ping; echo pkill=$?; \
         ping.exe -n 60 127.0.0.1 >/dev/null & b=$!; sleep 1; killall -w ping; echo killall=$?; \
         kill -0 $a 2>/dev/null || kill -0 $b 2>/dev/null || echo both-gone; echo shell-survived",
    );
    assert_eq!(out.stdout, "pkill=0\nkillall=0\nboth-gone\nshell-survived");
}

/// A PowerShell window: it exits 7 when closed, or refuses to close at all.
struct Form {
    child: Child,
    ready: PathBuf,
}

impl Form {
    /// Starts the window and waits for it to be shown; `None` where this session cannot
    /// show one (a runner without a desktop), which the caller treats as a skip.
    fn start(name: &str, refuses_to_close: bool) -> Option<Self> {
        let ready = std::env::temp_dir().join(format!("cash-kill-term-{name}.ready"));
        let _ = std::fs::remove_file(&ready);
        let closing = if refuses_to_close {
            "$form.Add_FormClosing({ param($s, $e) $e.Cancel = $true })"
        } else {
            "$form.Add_FormClosed({ [Environment]::Exit(7) })"
        };
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             $form = New-Object System.Windows.Forms.Form; {closing}; \
             $form.Add_Shown({{ Set-Content -LiteralPath '{}' -Value ready }}); \
             [System.Windows.Forms.Application]::Run($form)",
            ready.display()
        );
        let child = Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let form = Self { child, ready };
        let deadline = Instant::now() + Duration::from_secs(20);
        while !form.ready.exists() {
            if Instant::now() > deadline {
                eprintln!("skipping: no window could be shown in this session");
                return None;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        // Shown fires as the window appears; give it a moment to be enumerable.
        std::thread::sleep(Duration::from_millis(300));
        Some(form)
    }

    fn exit_code_within(&mut self, limit: Duration) -> Option<i32> {
        let deadline = Instant::now() + limit;
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                return status.code();
            }
            if Instant::now() > deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[test]
fn a_program_with_a_window_is_asked_to_close() {
    let Some(mut form) = Form::start("graceful", false) else {
        return;
    };
    let out = isolated_cash(&format!("kill -TERM {}; echo rc=$?", form.child.id()));
    assert_eq!(out.stdout, "rc=0");
    // 7 is the window's own exit on being closed; a terminated process reports 1.
    assert_eq!(form.exit_code_within(Duration::from_secs(10)), Some(7));
}

#[test]
fn a_program_that_will_not_close_is_terminated_after_the_grace_period() {
    let Some(mut form) = Form::start("stubborn", true) else {
        return;
    };
    // The escalation runs in the shell, so the shell stays up past the grace period.
    let out = isolated_cash(&format!(
        "kill -TERM {pid}; sleep 1; kill -0 {pid} && echo asked; sleep 6; \
         kill -0 {pid} 2>/dev/null && echo alive || echo terminated",
        pid = form.child.id()
    ));
    assert_eq!(out.stdout, "asked\nterminated");
    // Terminated by TERM, it exits 128 + 15, as a process ended by the signal reports.
    assert_eq!(form.exit_code_within(Duration::from_secs(5)), Some(143));
}

impl Drop for Form {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.ready);
    }
}
