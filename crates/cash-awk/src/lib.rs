//
// Copyright (c) 2024-2026 Hemi Labs, Inc.
// Copyright (c) 2026 Cash project contributors.
//
// This file is part of the posixutils-rs project covered under
// the MIT License. For the full license text, please see the LICENSE
// file in the root directory of this project.
// SPDX-License-Identifier: MIT
//

//! POSIX awk for cash, absorbed from posixutils-rs (`posixutils-awk` 0.9.0). See
//! README.md for what changed on the way in.

// The workspace lints apply here as everywhere, the unsafe ones and rustc's warnings
// included (REVIEW_REPORT.md ARCH-01); this crate came from posixutils-rs written to
// other rules, so the style lints it was not written to are allowed rather than
// rewritten, and the lints for code that can panic wait on TODO.md 14.5.
#![allow(
    elided_lifetimes_in_paths,
    missing_docs,
    clippy::borrow_as_ptr,
    clippy::branches_sharing_code,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::default_trait_access,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::enum_glob_use,
    clippy::explicit_iter_loop,
    clippy::float_cmp,
    clippy::format_push_string,
    clippy::implicit_clone,
    clippy::inconsistent_struct_constructor,
    clippy::items_after_statements,
    clippy::manual_string_new,
    clippy::map_unwrap_or,
    clippy::match_wildcard_for_single_variants,
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_ref_mut,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::or_fun_call,
    clippy::range_plus_one,
    clippy::redundant_clone,
    clippy::ref_as_ptr,
    clippy::semicolon_if_nothing_returned,
    clippy::single_match_else,
    clippy::struct_field_names,
    clippy::too_long_first_doc_paragraph,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::uninlined_format_args,
    clippy::unnecessary_box_returns,
    clippy::unnecessary_cast,
    clippy::unnecessary_map_or,
    clippy::unnecessary_semicolon,
    clippy::unnecessary_sort_by,
    clippy::unnecessary_wraps,
    clippy::unnested_or_patterns,
    clippy::unreadable_literal,
    clippy::unused_self,
    clippy::use_self,
    clippy::useless_let_if_seq,
    reason = "posixutils-rs code, not written to the workspace's style lints"
)]
#![expect(
    clippy::expect_used,
    clippy::missing_panics_doc,
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::string_slice,
    clippy::unwrap_in_result,
    clippy::unwrap_used,
    reason = "panicking code from posixutils-rs, to be reviewed (TODO.md 14.5)"
)]

pub mod compiler;
pub mod interpreter;
pub mod program;
pub mod regex;

use std::ffi::OsString;
use std::io::Read;

use clap::Parser;
use compiler::{SourceFile, compile_program};
use interpreter::interpret_with_eol;

/// Returns true when the awk program text mentions a carriage return: an
/// escape `\r`, an octal escape whose value is 13 (`\15`, `\015`), or a
/// literal CR byte that is not the CR of a CRLF line ending of the program
/// file itself. Backslash parity is respected: `\\r` is an escaped backslash
/// followed by a plain `r`. The scan is lexical (a `\r` in a comment counts
/// too), which errs on the side of treating CR as data.
fn program_mentions_cr(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                let Some(&next) = bytes.get(i + 1) else {
                    return false;
                };
                if next == b'r' {
                    return true;
                }
                if (b'0'..=b'7').contains(&next) {
                    let mut value = 0u32;
                    let mut j = i + 1;
                    while j < bytes.len() && j < i + 4 && (b'0'..=b'7').contains(&bytes[j]) {
                        value = value * 8 + u32::from(bytes[j] - b'0');
                        j += 1;
                    }
                    if value == 13 {
                        return true;
                    }
                    i = j;
                } else {
                    // Any other escaped character (including `\\` and a
                    // line-continuation newline or CRLF) is skipped whole.
                    i += 2;
                }
            }
            b'\r' => {
                if bytes.get(i + 1) != Some(&b'\n') {
                    return true;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    false
}

/// Line-ending mode for this run: CR is ordinary data when `CASH_EOL=lf`
/// (case-insensitive) is set or when the program mentions a carriage return;
/// otherwise the CR of CRLF input records is hidden and restored by `print`.
fn cr_is_data(sources: &[SourceFile]) -> bool {
    let eol_lf = std::env::var("CASH_EOL").is_ok_and(|v| v.eq_ignore_ascii_case("lf"));
    eol_lf || sources.iter().any(|s| program_mentions_cr(&s.contents))
}

#[derive(Parser, Debug)]
#[command(name = "awk", about = "awk - pattern scanning and processing language")]
pub struct Args {
    #[arg(short = 'F', help = "Define the input field separator")]
    pub separator_string: Option<String>,

    #[arg(
        short = 'f',
        action = clap::ArgAction::Append,
        help = "Specify the program files"
    )]
    pub program_files: Vec<String>,

    #[arg(
        short = 'v',
        action = clap::ArgAction::Append,
        help = "Globals assignments, executed before the start of the program"
    )]
    pub assignments: Vec<String>,

    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub arguments: Vec<String>,
}

fn normalize_awk_args(args: Vec<OsString>) -> Vec<OsString> {
    let mut normalized = Vec::with_capacity(args.len());
    let mut iter = args.into_iter();
    if let Some(argv0) = iter.next() {
        normalized.push(argv0);
    }
    let mut after_double_dash = false;
    for arg in iter {
        if after_double_dash {
            normalized.push(arg);
            continue;
        }
        if arg == "--" {
            after_double_dash = true;
            normalized.push(arg);
            continue;
        }
        if let Some(s) = arg.to_str() {
            if (s.starts_with("-F") || s.starts_with("-v") || s.starts_with("-f")) && s.len() > 2 {
                normalized.push(OsString::from(&s[..2]));
                normalized.push(OsString::from(&s[2..]));
                continue;
            }
        }
        normalized.push(arg);
    }
    normalized
}

/// Writes out what awk printed and has not written yet, and returns `code`, or 2 when the
/// output cannot be written.
fn finish(code: i32) -> i32 {
    match interpreter::flush_stdout() {
        Ok(()) => code,
        Err(e) => {
            eprintln!("awk: {e}");
            2
        }
    }
}

/// Runs awk with the given command-line arguments and returns the exit status.
pub fn run_awk<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let args_vec: Vec<OsString> = normalize_awk_args(args.into_iter().map(Into::into).collect());
    let parsed_args = match Args::try_parse_from(&args_vec) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("{err}");
            return if err.use_stderr() { 2 } else { 0 };
        }
    };

    if !parsed_args.program_files.is_empty() {
        let mut sources = Vec::new();
        for source_file in &parsed_args.program_files {
            let mut contents = String::new();
            if source_file == "-" {
                if let Err(e) = std::io::stdin().read_to_string(&mut contents) {
                    eprintln!("awk: could not read standard input: {e}");
                    return 1;
                }
            } else {
                match std::fs::File::open(source_file) {
                    Ok(mut file) => {
                        if let Err(e) = file.read_to_string(&mut contents) {
                            eprintln!("awk: could not read file '{source_file}': {e}");
                            return 1;
                        }
                    }
                    Err(e) => {
                        eprintln!("awk: could not open file '{source_file}': {e}");
                        return 1;
                    }
                }
            }
            sources.push(SourceFile {
                contents,
                filename: source_file.clone(),
            });
        }
        let cr_is_data = cr_is_data(&sources);
        let program = match compile_program(&sources) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        };
        match interpret_with_eol(
            program,
            &parsed_args.arguments,
            &parsed_args.assignments,
            parsed_args.separator_string,
            cr_is_data,
        ) {
            Ok(code) => finish(code as i32),
            Err(e) => {
                finish(1);
                eprintln!("{e}");
                1
            }
        }
    } else if !parsed_args.arguments.is_empty() {
        let sources = [SourceFile::stdin(parsed_args.arguments[0].clone())];
        let cr_is_data = cr_is_data(&sources);
        let program = match compile_program(&sources) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        };
        match interpret_with_eol(
            program,
            &parsed_args.arguments[1..],
            &parsed_args.assignments,
            parsed_args.separator_string,
            cr_is_data,
        ) {
            Ok(code) => finish(code as i32),
            Err(e) => {
                finish(1);
                eprintln!("{e}");
                1
            }
        }
    } else {
        eprintln!("awk: missing program argument");
        1
    }
}

#[cfg(test)]
mod tests {
    use super::program_mentions_cr;

    #[test]
    fn detects_carriage_return_mentions() {
        assert!(program_mentions_cr(r#"{ sub(/\r$/, "") } 1"#));
        assert!(program_mentions_cr(r#"{ printf "a\015" }"#));
        assert!(program_mentions_cr(r#"{ printf "a\15" }"#));
        // At most three octal digits: `\0151` is `\015` followed by `1`.
        assert!(program_mentions_cr(r#"{ printf "\0151" }"#));
        assert!(program_mentions_cr("{ x = \"a\rb\" }"));
        assert!(program_mentions_cr(r#"{ print "\\\r" }"#));
    }

    #[test]
    fn ignores_non_carriage_return_text() {
        assert!(!program_mentions_cr("{ print }"));
        assert!(!program_mentions_cr(r#"{ print "\\r" }"#));
        assert!(!program_mentions_cr(r#"{ printf "\151" }"#));
        assert!(!program_mentions_cr("BEGIN {\r\n  print 1\r\n}\r\n"));
        assert!(!program_mentions_cr("{ print \\\r\n $1 }"));
    }
}
