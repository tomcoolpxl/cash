//! Background jobs in process groups of their own — **D13**, **D21**, ROADMAP item 11.
//!
//! At the prompt (job control on), a background job's processes start with
//! `CREATE_NEW_PROCESS_GROUP`. The keyboard's Ctrl-C then passes them by, as it does a
//! Unix background job, and a Ctrl-Break can be aimed at them: `kill -TERM` sends one
//! (then terminates after the grace period), and after `fg`, a Ctrl-C is relayed as one,
//! with a second Ctrl-C terminating the job. Scripts keep the console's group.
//!
//! Every cash here runs in a console of its own, and the "keyboard" Ctrl-C is a
//! `GenerateConsoleCtrlEvent` sent inside that console, so nothing reaches the test
//! runner. `ping.exe` is the probe: a Ctrl-Break makes it print its statistics, whose
//! loss line is the only one with a `%`, whatever the display language.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::os::windows::process::CommandExt as _;
use std::path::Path;
use std::process::Stdio;

use crate::common::{CASH, cash_command};

/// A console of its own, without a window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Sends `CTRL_C_EVENT` to every process on its console, as pressing Ctrl-C does, while
/// surviving it itself.
const CTRL_C_PS1: &str = r#"
Add-Type -Namespace W -Name K -MemberDefinition '
[DllImport("kernel32.dll")] public static extern bool GenerateConsoleCtrlEvent(uint e, uint g);
[DllImport("kernel32.dll")] public static extern bool SetConsoleCtrlHandler(System.IntPtr h, bool add);
'
[W.K]::SetConsoleCtrlHandler([System.IntPtr]::Zero, $true) | Out-Null
[W.K]::GenerateConsoleCtrlEvent(0, 0) | Out-Null
"#;

/// Runs `body` as a cash script in a console of its own, `interactive` giving it job
/// control, with `ctrlc` defined to press Ctrl-C and `$T` naming a scratch directory.
fn run(dir: &Path, interactive: bool, body: &str) -> String {
    let helper = dir.join("ctrlc.ps1");
    std::fs::write(&helper, CTRL_C_PS1).unwrap();
    let t = dir.to_string_lossy().replace('\\', "/");
    let script = dir.join("script.sh");
    std::fs::write(
        &script,
        format!("T='{t}'\nctrlc() {{ powershell -NoProfile -File \"$T/ctrlc.ps1\"; }}\n{body}\n"),
    )
    .unwrap();
    // The test runner may start everything with Ctrl-C ignored, a flag Windows passes to
    // children. An interactive cash clears it (D13); a script keeps what it inherits, so
    // the script case runs under an interactive cash that has cleared it.
    let mut command = cash_command();
    if interactive {
        command.arg("-i").arg(&script);
    } else {
        let spelled = script.to_string_lossy().replace('\\', "/");
        let cash = CASH.replace('\\', "/");
        command.args(["-i", "-c", &format!("'{cash}' --no-config '{spelled}'")]);
    }
    let out = command
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn at_the_prompt_a_background_job_survives_ctrl_c() {
    let dir = tempfile::tempdir().unwrap();
    // `jobs -pr` lists the pids of the jobs cash knows to be running, from the handles it
    // holds. `kill -0 $bg` would ask Windows about the pid, which may be another
    // process's a second after the ping dies, and `kill -9 $bg` would then end that one
    // (see `process_identity.rs`).
    let body = "ping.exe -n 30 127.0.0.1 > /dev/null & bg=$!\n\
                sleep 1; ctrlc; sleep 1\n\
                jobs -pr > \"$T/running\"\n\
                if grep -qx \"$bg\" \"$T/running\"; then echo survived; kill -9 $bg\n\
                else echo killed; fi";
    let out = run(dir.path(), true, body);
    assert!(out.contains("survived"), "{out:?}");
    // A script's background commands still share the console's group, as before.
    let out = run(dir.path(), false, body);
    assert!(out.contains("killed"), "{out:?}");
}

#[test]
fn kill_term_sends_a_background_job_ctrl_break_then_terminates_it() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(
        dir.path(),
        true,
        "ping.exe -n 30 127.0.0.1 > \"$T/ping.out\" & p=$!\n\
         sleep 1; kill -TERM $p; sleep 1\n\
         grep -q '%' \"$T/ping.out\" && echo broke || echo 'no ctrl-break'\n\
         kill -0 $p 2>/dev/null && echo asked\n\
         wait $p; echo \"status=$?\"",
    );
    assert!(out.contains("broke\n"), "{out}");
    // A second after TERM the ping is still running, so the pid is still its own.
    assert!(out.contains("asked\n"), "{out}");
    // `wait` returns when the grace period ends the ping, with TERM's status. A ping that
    // nothing ended would keep `wait` for its 30 echoes, and then report 0.
    assert!(out.contains("status=143\n"), "{out}");
}

#[test]
fn after_fg_ctrl_c_is_relayed_and_a_second_one_terminates() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(
        dir.path(),
        true,
        "ping.exe -n 120 127.0.0.1 > \"$T/ping.out\" &\n\
         ( sleep 2; ctrlc; sleep 1; ctrlc ) &\n\
         start=$SECONDS\n\
         fg %1 > /dev/null\n\
         echo \"fg took $((SECONDS - start))\"\n\
         grep -q '%' \"$T/ping.out\" && echo relayed || echo 'not relayed'",
    );
    // ping.exe carries on after a Ctrl-Break, printing its statistics (the `%` line), so
    // only the second Ctrl-C can end it well before its 120 echoes — and ended early, the
    // statistics can only have come from the relayed Ctrl-Break. The bound leaves room for
    // a slow runner: each simulated Ctrl-C compiles its P/Invoke with Add-Type, which
    // took several seconds on GitHub's runner (fg returned after 22 s there).
    let took: u64 = out
        .lines()
        .find_map(|line| line.strip_prefix("fg took "))
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(u64::MAX);
    assert!(took < 90, "fg did not return early: {out}");
    assert!(out.contains("relayed\n"), "{out}");
}
