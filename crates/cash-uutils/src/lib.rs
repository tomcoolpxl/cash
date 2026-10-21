// This file is part of cash. The tools below came from the uutils coreutils package;
// for their copyright and licence, see the LICENSE file beside this crate's manifest.

//! `cat`, `cp`, `head`, `ln`, `mkdir`, `mv`, `paste`, `pr`, `ptx`, `rm`, `rmdir`, `shred`,
//! `shuf`, `sort`, `split`, `tac`, `tail`, `tee`, `touch` and `uniq`, taken from uutils coreutils 0.12.0
//! and made cash's own: what changed on the way in is in this crate's `README.md`. cash runs
//! each through its `uumain`, as it runs the uutils tools it bundles unchanged.

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
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::fn_params_excessive_bools,
    clippy::literal_string_with_formatting_args,
    clippy::must_use_candidate,
    clippy::redundant_clone,
    clippy::return_self_not_must_use,
    clippy::useless_let_if_seq,
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

/// The uutils release the tools came from, which `--version` names as the bundled ones
/// do: `uucore::crate_version!` gave this crate's version, a uutils release there never
/// was (`sort (uutils coreutils) 1.19.0`).
pub(crate) const UUTILS_VERSION: &str = "(uutils coreutils) 0.12.0";

pub mod cat;
pub mod cp;
pub mod head;
pub mod ln;
pub mod mkdir;
pub mod mv;
pub mod paste;
pub mod pr;
pub mod ptx;
pub mod rm;
pub mod rmdir;
pub mod shred;
pub mod shuf;
pub mod sort;
pub mod split;
pub mod tac;
pub mod tail;
pub mod tee;
pub mod touch;
pub mod uniq;
