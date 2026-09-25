//
// Copyright (c) 2024-2026 Hemi Labs, Inc.
// Copyright (c) 2026 Cash project contributors.
//
// This file is part of the posixutils-rs project covered under
// the MIT License. For the full license text, please see the LICENSE
// file in the root directory of this project.
// SPDX-License-Identifier: MIT
//

pub mod compiler;
pub mod interpreter;
pub mod program;
pub mod regex;

use std::ffi::OsString;
use std::io::Read;

use clap::Parser;
use compiler::{SourceFile, compile_program};
use interpreter::interpret;

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
        let program = match compile_program(&sources) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        };
        match interpret(
            program,
            &parsed_args.arguments,
            &parsed_args.assignments,
            parsed_args.separator_string,
        ) {
            Ok(code) => code as i32,
            Err(e) => {
                eprintln!("{e}");
                1
            }
        }
    } else if !parsed_args.arguments.is_empty() {
        let program = match compile_program(&[SourceFile::stdin(parsed_args.arguments[0].clone())])
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        };
        match interpret(
            program,
            &parsed_args.arguments[1..],
            &parsed_args.assignments,
            parsed_args.separator_string,
        ) {
            Ok(code) => code as i32,
            Err(e) => {
                eprintln!("{e}");
                1
            }
        }
    } else {
        eprintln!("awk: missing program argument");
        1
    }
}
