//
// Copyright (c) 2024-2026 Hemi Labs, Inc.
// Copyright (c) 2026 Cash project contributors.
//
// This file is part of the posixutils-rs project covered under
// the MIT License. For the full license text, please see the LICENSE
// file in the root directory of this project.
// SPDX-License-Identifier: MIT
//

//! The part of posixutils' `plib::diag` that `bc` uses, with the same output:
//! `bc: <message>`, or `bc: <source>:<line>[:<col>]: error: <message>`.

use std::cell::RefCell;
use std::io::{self, Write};

thread_local! {
    static SOURCE: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Names the input that positions in later diagnostics refer to.
pub fn set_source(name: &str) {
    SOURCE.with(|s| *s.borrow_mut() = name.to_owned());
}

/// `bc: <message>` on standard error.
pub fn error(message: &str) {
    let _ = writeln!(io::stderr().lock(), "bc: {message}");
}

/// `bc: <source>:<line>[:<col>]: error: <message>` on standard error.
pub fn error_at(line: usize, col: usize, message: &str) {
    let source = SOURCE.with(|s| s.borrow().clone());
    let source = if source.is_empty() {
        "<unknown>".to_owned()
    } else {
        source
    };
    let position = if col == 0 {
        format!("{source}:{line}")
    } else {
        format!("{source}:{line}:{col}")
    };
    let _ = writeln!(io::stderr().lock(), "bc: {position}: error: {message}");
}

/// An I/O error as a system utility words it, without Rust's "(os error N)".
pub fn io_error_text(e: &io::Error) -> String {
    if e.kind() == io::ErrorKind::NotFound {
        return "No such file or directory".to_owned();
    }
    let text = e.to_string();
    match text.find(" (os error ") {
        Some(at) => text[..at].to_owned(),
        None => text,
    }
}
