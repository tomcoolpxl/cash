//
// Copyright (c) 2024-2026 Hemi Labs, Inc.
// Copyright (c) 2026 Cash project contributors.
//
// This file is part of the posixutils-rs project covered under
// the MIT License. For the full license text, please see the LICENSE
// file in the root directory of this project.
// SPDX-License-Identifier: MIT
//

//! POSIX `bc` for cash, absorbed from posixutils-rs (`calc/bc.rs` and `calc/bc_util`).
//! See README.md for what changed on the way in.

// The workspace lints apply here as everywhere, the unsafe ones and rustc's warnings
// included (REVIEW_REPORT.md ARCH-01); this crate came from posixutils-rs written to
// other rules, so the style lints it was not written to are allowed rather than
// rewritten, and the lints for code that can panic wait on TODO.md 14.5.
#![allow(
    elided_lifetimes_in_paths,
    clippy::assigning_clones,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::manual_let_else,
    clippy::manual_string_new,
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_value,
    clippy::or_fun_call,
    clippy::redundant_clone,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unnecessary_box_returns,
    clippy::unnecessary_trailing_comma,
    clippy::unnested_or_patterns,
    clippy::use_self,
    clippy::useless_let_if_seq,
    clippy::wildcard_imports,
    reason = "posixutils-rs code, not written to the workspace's style lints"
)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::string_slice,
    clippy::unwrap_in_result,
    clippy::unwrap_used,
    reason = "panicking code from posixutils-rs, to be reviewed (TODO.md 14.5)"
)]

mod bc_util;
mod diag;
mod gnu;

use std::ffi::OsString;
use std::io::{BufRead, BufWriter, IsTerminal};

use bc_util::{
    interpreter::Interpreter,
    output::OutputWriter,
    parser::{ParseError, parse_program},
};
use clap::Parser;

/// bc - arbitrary-precision arithmetic language
#[derive(Parser)]
#[command(
    name = "bc",
    version,
    about = "bc - arbitrary-precision arithmetic language"
)]
struct Args {
    /// Define the math functions and set scale to 20.
    #[arg(short = 'l', long = "mathlib")]
    define_math_functions: bool,

    /// Recover from errors as an interactive session does, and exit 0 after them.
    #[arg(short = 'i', long = "interactive")]
    interactive: bool,

    // GNU bc's -q (no banner), -s (POSIX only) and -w (warn about extensions) are in
    // countless scripts. This bc prints no banner and is POSIX bc, so they are accepted
    // and change nothing.
    /// Accepted for GNU bc compatibility: there is no banner to suppress.
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// Accepted for GNU bc compatibility: this bc is always POSIX bc.
    #[arg(short = 's', long = "standard")]
    standard: bool,

    /// Accepted for GNU bc compatibility: extensions are always errors that say so.
    #[arg(short = 'w', long = "warn")]
    warn: bool,

    files: Vec<OsString>,
}

/// The source name bc implementations conventionally use for standard input.
const STDIN_NAME: &str = "(standard_in)";

/// Stack for the interpreter thread: expression and function-call evaluation recurse on
/// the machine stack, and the interpreter's own depth limit should be what stops a
/// runaway recursion, with a diagnostic, rather than a guard page.
#[cfg(debug_assertions)]
const INTERPRETER_STACK_SIZE: usize = 512 * 1024 * 1024;
#[cfg(not(debug_assertions))]
const INTERPRETER_STACK_SIZE: usize = 128 * 1024 * 1024;

/// Runs `bc` with `args` (the first being the program name) and returns its exit status.
pub fn run_bc<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let parsed = match Args::try_parse_from(&args) {
        Ok(parsed) => parsed,
        Err(err) => {
            let _ = err.print();
            return if err.use_stderr() { 2 } else { 0 };
        }
    };
    let spawned = std::thread::Builder::new()
        .stack_size(INTERPRETER_STACK_SIZE)
        .spawn(move || run(parsed));
    let handle = match spawned {
        Ok(handle) => handle,
        // A constrained address space can refuse the large stack; run with less room
        // rather than not at all.
        Err(_) => match std::thread::Builder::new().spawn(move || run(Args::parse_from(&args))) {
            Ok(handle) => handle,
            Err(e) => {
                diag::error(&e.to_string());
                return 1;
            }
        },
    };
    handle.join().unwrap_or(1)
}

/// Report a runtime or write failure. Returns true so callers can record that
/// something went wrong.
fn report(e: impl std::fmt::Display) -> bool {
    diag::error(&format!("{}", e));
    true
}

/// Report each diagnostic of a parse failure at its own position, then name the GNU bc
/// extension in `text`, if it used one.
fn report_parse_error(e: &ParseError, text: &str) -> bool {
    for (line, col, message) in e.diagnostics() {
        diag::error_at(line as usize, col as usize, message);
    }
    if let Some(note) = gnu::extension_note(text) {
        diag::error(&note);
    }
    true
}

fn run(args: Args) -> i32 {
    let mut interpreter = Interpreter::default();
    let mut had_error = false;
    // Block-buffered so a long-running loop streams rather than accumulating its whole
    // output in memory; flushed after each input item so interactive output appears
    // without delay, as POSIX requires.
    let stdout = std::io::stdout();
    let mut sink = BufWriter::new(stdout.lock());
    let mut out = OutputWriter::new(&mut sink);

    // POSIX describes error recovery for "an interactive invocation of bc". A session
    // at a terminal recovers and exits 0; a script fed on standard input reports
    // failure the way a file operand does.
    let interactive = args.interactive || std::io::stdin().is_terminal();

    if args.define_math_functions {
        diag::set_source("math library");
        let library = include_str!("bc_util/math_functions.bc");
        let load = parse_program(library, None)
            .map_err(|e| e.to_string())
            .and_then(|lib| interpreter.exec(lib, &mut out).map_err(|e| e.to_string()));
        if let Err(e) = load {
            diag::error(&format!(
                "internal error loading the standard math functions: {e}"
            ));
            return 1;
        }
    }

    for file in args.files {
        let name = file.to_string_lossy().into_owned();
        diag::set_source(&name);
        // Read bytes, so a file that exists but is not text is reported as that rather
        // than as a failure to access it.
        let bytes = match std::fs::read(&file) {
            Ok(bytes) => bytes,
            Err(e) => {
                // POSIX CONSEQUENCES OF ERRORS: if a file operand cannot be accessed,
                // write a diagnostic and terminate.
                diag::error(&format!("{}: {}", name, diag::io_error_text(&e)));
                let _ = out.flush();
                return 1;
            }
        };
        let text = match String::from_utf8(bytes) {
            Ok(text) => text.replace("\r\n", "\n"),
            Err(_) => {
                diag::error(&format!("{name}: not a text file"));
                had_error = true;
                continue;
            }
        };
        match parse_program(&text, file.to_str()) {
            Ok(program) => {
                if let Err(e) = interpreter.exec(program, &mut out) {
                    had_error |= report(e);
                }
                if let Err(e) = out.flush() {
                    had_error |= report(e);
                }
            }
            Err(e) => had_error |= report_parse_error(&e, &text),
        }
        if interpreter.has_quit() {
            if let Err(e) = out.flush() {
                had_error |= report(e);
            }
            return i32::from(had_error);
        }
    }

    // Standard input, a line at a time: a statement can span lines, so lines are
    // gathered until they parse. Upstream reads through rustyline with prompts; POSIX
    // bc prints none, and cash's own line editor is the shell's, so this reads plainly.
    diag::set_source(STDIN_NAME);
    let stdin = std::io::stdin();
    let mut lines = stdin.lock();
    let mut line_buffer = String::new();
    let mut line = String::new();
    while !interpreter.has_quit() {
        line.clear();
        match lines.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                // D20: a CRLF line is an LF line; a lone CR is still an illegal character.
                if line.ends_with("\r\n") {
                    line.truncate(line.len() - 2);
                    line.push('\n');
                }
                line_buffer.push_str(&line);
                if !line_buffer.ends_with('\n') {
                    line_buffer.push('\n');
                }
                match parse_program(&line_buffer, None) {
                    Ok(program) => {
                        // An interactive session recovers from a runtime error and its
                        // exit status is unaffected; a script does not.
                        let mut failed = false;
                        if let Err(e) = interpreter.exec(program, &mut out) {
                            failed = report(e);
                        }
                        if let Err(e) = out.flush() {
                            failed = report(e);
                        }
                        had_error |= failed && !interactive;
                        line_buffer.clear();
                    }
                    Err(e) if !e.is_incomplete => {
                        report_parse_error(&e, &line_buffer);
                        had_error |= !interactive;
                        line_buffer.clear();
                    }
                    _ => {}
                }
            }
            Err(e) => {
                had_error |= report(e);
                break;
            }
        }
    }

    // Input that ended in the middle of a construct is not simply discarded: a
    // truncated script has to say so rather than exit as if it had run.
    if !line_buffer.is_empty() {
        if let Err(e) = parse_program(&line_buffer, None) {
            report_parse_error(&e, &line_buffer);
            had_error = true;
        }
    }

    if let Err(e) = out.flush() {
        had_error |= report(e);
    }
    i32::from(had_error)
}
