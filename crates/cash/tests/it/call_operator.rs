//! PowerShell's call operator typed at the prompt from habit: `& C:\x\script.ps1 a b` runs
//! what follows, as cash runs it named alone. Only a line typed at the prompt is read so: a
//! script's `&` at the start of a line stays Bash's syntax error.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction, and a \
              failed set-up should abort it loudly"
)]

use std::path::Path;
use std::time::Duration;

use cash_win32::conpty::ConPtySession;

use crate::common::{CASH, run};

const WAIT: Duration = Duration::from_secs(10);

#[test]
fn a_leading_ampersand_at_the_prompt_runs_the_rest() {
    let temp = std::env::temp_dir();
    let temp = temp.to_string_lossy();
    let mut session = ConPtySession::start_sized(
        Path::new(CASH),
        &[
            "--noprofile",
            "--norc",
            "--no-config",
            "--disable-color",
            "--input-backend=reedline",
            "-i",
        ],
        Some(&[
            ("HISTFILE", ""),
            ("PS1", "PROMPT$ "),
            ("CASH_NO_OFFER", "1"),
            ("TEMP", &temp),
        ]),
        None,
        100,
        25,
    )
    .expect("start cash on a pseudo console");
    session.expect("PROMPT$", WAIT).expect("the first prompt");
    session.send("& echo call-op''-ran\r").unwrap();
    session
        .expect("call-op-ran", WAIT)
        .expect("the line after `& ` ran");
    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("cash exits"), 0);
}

#[test]
fn a_script_keeps_bashs_syntax_error() {
    let out = run("& echo no");
    assert_eq!(out.code, 2, "{}", out.stdout);
    assert!(out.stderr.contains("syntax error"), "{}", out.stderr);
}
