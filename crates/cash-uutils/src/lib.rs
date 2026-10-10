// This file is part of cash. The tools below came from the uutils coreutils package;
// for their copyright and licence, see the LICENSE file beside this crate's manifest.

//! `rm`, `sort`, `tee`, `uniq` and `shuf`, taken from uutils coreutils 0.12.0 and made
//! cash's own: what changed on the way in is in this crate's `README.md`. cash runs each
//! through its `uumain`, as it runs the uutils tools it bundles unchanged.

// The workspace lints apply here as everywhere, rustc's warnings and the unsafe ones
// included; this code came from uutils, written to other rules, so the style lints it
// was not written to are allowed rather than rewritten, as for `cash-sed`. The lints
// for code that can panic apply in full.
#![allow(
    elided_lifetimes_in_paths,
    missing_docs,
    clippy::assert_is_empty,
    clippy::branches_sharing_code,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::ignored_unit_patterns,
    clippy::inline_always,
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_value,
    clippy::or_fun_call,
    clippy::too_many_lines,
    clippy::unnested_or_patterns,
    clippy::use_self,
    reason = "uutils code, not written to the workspace's style lints"
)]

mod messages;

pub mod rm;
pub mod shuf;
pub mod sort;
pub mod tee;
pub mod uniq;
