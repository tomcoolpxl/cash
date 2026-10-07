// This file is part of the uutils diffutils package.
//
// For the full copyright and license information, please view the LICENSE-*
// files that was distributed with this source code.

//! diff and cmp for cash, absorbed from uutils diffutils. See README.md and
//! CASH-PATCHES.md for what changed on the way in.
//!
//! The entry points take the tool's whole argv, its own name first, and return its exit
//! status: 0 when the inputs are the same, 1 when they differ, 2 for trouble. Output
//! goes to this process's standard output and error, as the tools run in a process of
//! their own under cash.

#![allow(missing_docs, reason = "uutils code, documented where cash changed it")]

// Upstream's cmp, kept in its own style.
#[allow(
    clippy::assigning_clones,
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::doc_markdown,
    clippy::manual_let_else,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::ref_option,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unnecessary_wraps,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::if_not_else,
    clippy::manual_string_new,
    clippy::needless_return,
    clippy::unreadable_literal,
    clippy::use_self,
    clippy::or_fun_call,
    clippy::redundant_slicing,
    clippy::single_match_else,
    clippy::missing_panics_doc,
    clippy::naive_bytecount,
    clippy::panic_in_result_fn,
    clippy::branches_sharing_code,
    clippy::semicolon_if_nothing_returned,
    reason = "uutils code, not written to the workspace's style lints"
)]
pub mod cmp;
pub mod context_diff;
pub mod diff;
pub mod ed_diff;
pub mod engine;
pub mod normal_diff;
pub mod params;
pub mod side_diff;
pub mod unified_diff;
// Upstream's tab expansion and time stamps, with cash's error words and zone.
#[allow(
    clippy::doc_markdown,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::naive_bytecount,
    clippy::too_long_first_doc_paragraph,
    reason = "uutils code, not written to the workspace's style lints"
)]
pub mod utils;

use std::ffi::OsString;

/// Runs `diff` with `args`, the tool's name first; returns its exit status.
#[must_use]
pub fn run_diff(args: impl IntoIterator<Item = OsString>) -> i32 {
    let args: Vec<OsString> = args.into_iter().collect();
    diff::main(&args)
}

/// Runs `cmp` with `args`, the tool's name first; returns its exit status.
#[must_use]
pub fn run_cmp(args: impl IntoIterator<Item = OsString>) -> i32 {
    let args: Vec<OsString> = args.into_iter().collect();
    cmp::main(&args)
}
