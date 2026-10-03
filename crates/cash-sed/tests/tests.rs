// This file is part of the uutils sed package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! uutils' sed tests, run against the bundled sed.

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::ignore_without_reason,
    clippy::literal_string_with_formatting_args,
    clippy::needless_raw_string_hashes,
    clippy::panic_in_result_fn,
    clippy::string_slice,
    clippy::tests_outside_test_module,
    reason = "uutils tests: an integration test is outside a test module by construction, \
              and a failed assumption in a test should abort it loudly"
)]

use std::env;

pub const TESTS_BINARY: &str = env!("CARGO_BIN_EXE_sed");

// Use the ctor attribute to run this function before any tests
#[ctor::ctor(unsafe)]
fn init() {
    // Necessary for uutests to be able to find the binary.
    // SAFETY: a constructor runs before `main`, so no other thread can read or write the
    // environment yet.
    unsafe { std::env::set_var("UUTESTS_BINARY_PATH", TESTS_BINARY) };
    // For single-call binaries, tell uutests not to auto-add the utility name
    // SAFETY: as above.
    unsafe { std::env::remove_var("UUTESTS_UTIL_NAME") };
    // SAFETY: as above.
    unsafe { std::env::set_var("UUTESTS_UTIL_NAME", "") };
    // SAFETY: as above.
    unsafe { std::env::set_var("UUTILS_MULTICALL", "0") };
    // Keep fixture-based integration tests in UTF-8 mode; individual
    // locale tests override LC_ALL to exercise byte mode.
    // SAFETY: as above.
    unsafe { std::env::set_var("LC_ALL", "C.UTF-8") };
}

#[cfg(feature = "sed")]
#[path = "by-util/test_sed.rs"]
mod test_sed;
