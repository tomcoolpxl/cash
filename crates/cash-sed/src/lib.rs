// This file is part of the uutils sed package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! sed for cash, absorbed from uutils sed. See README.md for what changed on the way
//! in.

// The workspace lints apply here as everywhere, the unsafe ones and rustc's warnings
// included (REVIEW_REPORT.md ARCH-01); this crate came from uutils written to
// other rules, so the style lints it was not written to are allowed rather than
// rewritten. The lints for code that can panic apply in full: what input can reach
// is an error sed reports, and what cannot fail says why where it is.
#![allow(
    elided_lifetimes_in_paths,
    missing_docs,
    clippy::assigning_clones,
    clippy::branches_sharing_code,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::enum_glob_use,
    clippy::format_push_string,
    clippy::iter_on_single_items,
    clippy::many_single_char_names,
    clippy::missing_const_for_fn,
    clippy::needless_continue,
    clippy::needless_pass_by_ref_mut,
    clippy::needless_pass_by_value,
    clippy::or_fun_call,
    clippy::should_panic_without_expect,
    clippy::too_long_first_doc_paragraph,
    clippy::too_many_lines,
    clippy::unnecessary_map_or,
    clippy::unnecessary_sort_by,
    clippy::unnecessary_wraps,
    clippy::use_self,
    clippy::useless_let_if_seq,
    reason = "uutils code, not written to the workspace's style lints"
)]

pub mod sed;

pub use sed::processor::set_shell;
pub use sed::uumain;
