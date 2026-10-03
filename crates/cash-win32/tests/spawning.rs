//! Whether cash runs inside a job object, for `cash doctor` (D35).
//!
//! The tests here were of `cash_win32::spawn::spawn`, a `CreateProcessW` spawn nothing ran
//! (W32-11). The containment they showed, a program in its job before it runs and its
//! children with it, is tested on the spawn the shell uses, in
//! `cash_core::sys::tokio_process`.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use cash_win32::spawn::in_any_job;

#[test]
fn we_can_tell_whether_we_are_already_in_a_job() {
    // Nested jobs have worked since Windows 8, so cash running inside Windows Terminal's
    // or VS Code's job is fine. This is for cash doctor (D35) to be able to say so.
    let _ = in_any_job();
}
