// This file is part of the uutils diffutils package.
//
// For the full copyright and license information, please view the LICENSE-*
// files that was distributed with this source code.

//! The bundled diff and cmp as a multi-call program of their own, for testing them
//! outside the shell: `diffutils diff A B`, `diffutils cmp A B`, or through a copy named
//! `diff` or `cmp`.

use std::ffi::OsString;
use std::path::Path;
use std::process;

fn main() {
    let mut args: Vec<OsString> = std::env::args_os().collect();
    let exe_name = args
        .first()
        .map(|arg| {
            Path::new(arg)
                .file_stem()
                .map_or_else(OsString::new, OsString::from)
        })
        .unwrap_or_default();

    let util_name = if exe_name.as_encoded_bytes().ends_with(b"diffutils") {
        if args.len() < 2 {
            eprintln!("Expected utility name as second argument, got nothing.");
            println!(
                "diffutils {} (multi-call binary)\n\nUsage: diffutils [function [arguments...]]\n\n\
                 Currently defined functions:\n\n    cmp, diff\n",
                env!("CARGO_PKG_VERSION")
            );
            process::exit(0);
        }
        args.remove(0);
        args[0].clone()
    } else {
        args[0].clone_from(&exe_name);
        exe_name
    };

    let code = match util_name.as_encoded_bytes() {
        name if name.ends_with(b"diff") => cash_diffutils::run_diff(args),
        name if name.ends_with(b"cmp") => cash_diffutils::run_cmp(args),
        name => {
            eprintln!("{}: utility not supported", String::from_utf8_lossy(name));
            2
        }
    };
    process::exit(code);
}
