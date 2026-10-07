//! rars' own tests, taken in with it: each file a module of this one binary.

#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::unwrap_in_result,
    clippy::string_slice,
    clippy::tests_outside_test_module,
    missing_docs,
    dead_code,
    reason = "rars' tests, as they were written"
)]

#[path = "rar/builder_directories.rs"]
mod builder_directories;
#[path = "rar/builder_file_comments.rs"]
mod builder_file_comments;
#[path = "rar/builder_identity.rs"]
mod builder_identity;
#[path = "rar/builder_metadata.rs"]
mod builder_metadata;
#[path = "rar/builder_output.rs"]
mod builder_output;
#[path = "rar/builder_symlinks.rs"]
mod builder_symlinks;
#[path = "rar/builder_timestamps.rs"]
mod builder_timestamps;
#[path = "rar/comment_options.rs"]
mod comment_options;
#[path = "rar/crypto_api.rs"]
mod crypto_api;
#[path = "rar/extraction_control.rs"]
mod extraction_control;
#[path = "rar/legacy_codec_adapters.rs"]
mod legacy_codec_adapters;
#[path = "rar/legacy_name_encoding.rs"]
mod legacy_name_encoding;
#[path = "rar/legacy_parallel_retention.rs"]
mod legacy_parallel_retention;
#[path = "rar/legacy_writer_cancellation.rs"]
mod legacy_writer_cancellation;
#[path = "rar/legacy_writer_errors.rs"]
mod legacy_writer_errors;
#[path = "rar/member_selection.rs"]
mod member_selection;
#[path = "rar/ppmd_regressions.rs"]
mod ppmd_regressions;
#[path = "rar/rar13_fixtures.rs"]
mod rar13_fixtures;
#[path = "rar/rar15_40_fixtures.rs"]
mod rar15_40_fixtures;
#[path = "rar/rar50_fixtures.rs"]
mod rar50_fixtures;
#[path = "rar/rar50_golden.rs"]
mod rar50_golden;
#[path = "rar/rar50_split_limits.rs"]
mod rar50_split_limits;
#[path = "rar/rarvm_regressions.rs"]
mod rarvm_regressions;
#[path = "rar/reader_32bit_lengths.rs"]
mod reader_32bit_lengths;
#[path = "rar/reader_cancellation.rs"]
mod reader_cancellation;
#[path = "rar/reader_dictionary_limit.rs"]
mod reader_dictionary_limit;
#[path = "rar/reader_failure_output.rs"]
mod reader_failure_output;
#[path = "rar/reader_failure_paths.rs"]
mod reader_failure_paths;
#[path = "rar/reader_header_limits.rs"]
mod reader_header_limits;
#[path = "rar/reader_output_limit.rs"]
mod reader_output_limit;
#[path = "rar/reader_scratch.rs"]
mod reader_scratch;
#[path = "rar/reader_signatures.rs"]
mod reader_signatures;
#[path = "rar/reader_source.rs"]
mod reader_source;
#[path = "rar/reader_workspace.rs"]
mod reader_workspace;
#[path = "rar/repair_cancellation.rs"]
mod repair_cancellation;
#[path = "rar/rewrite_settings.rs"]
mod rewrite_settings;
#[path = "rar/rewrite_staging.rs"]
mod rewrite_staging;
#[path = "rar/source_consistency.rs"]
mod source_consistency;
#[path = "rar/write_progress.rs"]
mod write_progress;
#[path = "rar/writer_aggregate_limit.rs"]
mod writer_aggregate_limit;
#[path = "rar/writer_preparation_limit.rs"]
mod writer_preparation_limit;
#[path = "rar/writer_spool_limit.rs"]
mod writer_spool_limit;
