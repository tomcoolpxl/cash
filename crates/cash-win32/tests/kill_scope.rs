//! What a kill target means — **D13**, **D21**, **D22**.
//!
//! POSIX gives the *sign* of a kill target a meaning, and on Windows getting it wrong is
//! not a cosmetic error. `GenerateConsoleCtrlEvent` with group 0 does not mean "no
//! group": it means every process attached to this console, which includes the terminal
//! and anything else sharing it. `kill 0` — an ordinary bash idiom, the one in
//! `trap 'kill 0' EXIT` — went straight down that path and took the whole console with
//! it. This is the layer that makes the broadcast unreachable.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result. Shell snippets are spelled with hashes \
              throughout, including where they are not strictly needed, because \
              alternating the two forms by accident of content reads worse."
)]

use std::io::ErrorKind;

use cash_win32::console::interrupt_process_group;

#[test]
fn group_zero_is_refused() {
    // The whole point. Group 0 reaches the terminal, cash itself, and every unrelated
    // program attached to the same console.
    let error = interrupt_process_group(0).expect_err("a console-wide broadcast was permitted");
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert!(
        error.to_string().contains("every process"),
        "the refusal does not say why: {error}"
    );
}

#[test]
fn a_nonexistent_group_fails_without_broadcasting() {
    // A group id that names nothing must fail as a no-op, not fall back to 0. Picking an
    // id that is almost certainly not a live process-group leader is enough to show the
    // call is genuinely targeted.
    //
    // Windows answers `ERROR_INVALID_PARAMETER`, which maps to the same `ErrorKind` as
    // cash's own refusal — so the two are told apart by the message, which only cash's
    // carries.
    let error = interrupt_process_group(0x7FFF_FFF0).expect_err("a bogus group id succeeded");
    assert!(
        !error.to_string().contains("every process"),
        "the id was rejected by cash rather than reaching Windows, which proves nothing"
    );
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

#[test]
fn this_test_process_is_still_alive() {
    // Not a tautology: before the refusal existed, calling into this module with group 0
    // terminated the test runner — and the shell that launched it — with no output at
    // all. Asserting that the harness survived the calls above is exactly the regression.
    let _ = interrupt_process_group(0);
    assert!(
        cash_win32::process::is_pid_alive(std::process::id()),
        "the test process did not survive its own test"
    );
}
