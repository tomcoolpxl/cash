//! `figlet` (D84): big letters, through the shell. How text is laid out is checked
//! against figlet 2.2.5 in `cash-figlet`'s own tests; these check the command around it:
//! where fonts and control files are found, standard input, the width, `-I`, errors.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly"
)]

use std::fmt::Write as _;

use crate::common::{Scratch, cash_command, output_of, run as cash, run_in};

/// A font one row high: each printable ASCII character drawn as itself in brackets, the
/// German letters as `?`.
fn bracket_font() -> String {
    let mut text = String::from("flf2a$ 1 1 5 -1 1\nbrackets, for the tests\n");
    for c in ' '..='~' {
        let glyph = if c == ' ' { '$' } else { c };
        writeln!(text, "[{glyph}]#").unwrap();
    }
    for _ in 0..7 {
        text.push_str("?#\n");
    }
    text
}

#[test]
fn it_is_a_builtin() {
    let out = cash("type figlet");
    assert!(out.stdout.contains("shell builtin"), "{out:?}");
}

#[test]
fn it_draws_what_figlet_draws() {
    let expected = include_str!("../../../cash-figlet/tests/golden/std-hello.out");
    let out = cash("figlet Hello, World! | cat");
    assert_eq!(out.stdout, expected.trim_end(), "{}", out.stderr);
    assert_eq!(out.code, 0);
}

#[test]
fn built_in_fonts_by_name_with_or_without_suffix() {
    let out = cash("figlet -f term hello; figlet -f term.flf there; figlet -f mono9.tlf x | wc -l");
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.first(), Some(&"hello"), "{out:?}");
    assert_eq!(lines.get(1), Some(&"there"), "{out:?}");
    // mono9 is zipped and eight rows high.
    assert_eq!(lines.get(2).map(|l| l.trim()), Some("8"), "{out:?}");
}

#[test]
fn fonts_are_found_in_their_directories() {
    let dir = Scratch::new("figlet-fonts");
    std::fs::write(dir.join("brackets.flf"), bracket_font()).unwrap();
    let path = dir.as_script_path();

    // -d, FIGLET_FONTDIR, a path, and the current directory.
    let out = cash(&format!("figlet -d '{path}' -f brackets ab"));
    assert_eq!(out.stdout, "[a][b]", "{out:?}");
    let out = cash(&format!("FIGLET_FONTDIR='{path}' figlet -f brackets ab"));
    assert_eq!(out.stdout, "[a][b]", "{out:?}");
    let out = cash(&format!("figlet -f '{path}/brackets.flf' ab"));
    assert_eq!(out.stdout, "[a][b]", "{out:?}");
    let out = run_in(dir.path(), "figlet -f brackets ab");
    assert_eq!(out.stdout, "[a][b]", "{out:?}");

    // The user's own font directory, %LOCALAPPDATA%\cash\fonts.
    let local = Scratch::new("figlet-localappdata");
    std::fs::create_dir_all(local.join("cash/fonts")).unwrap();
    std::fs::write(local.join("cash/fonts/brackets.flf"), bracket_font()).unwrap();
    let out = output_of(
        cash_command()
            .env("LOCALAPPDATA", local.path())
            .args(["-c", "figlet -f brackets ab; figlet -I2"]),
    );
    assert_eq!(out.stdout.lines().next(), Some("[a][b]"), "{out:?}");
    assert!(out.stdout.ends_with("cash/fonts"), "{out:?}");
}

#[test]
fn control_files_translate() {
    let dir = Scratch::new("figlet-controls");
    std::fs::write(dir.join("upper.flc"), "flc2a\nt a-z A-Z\n").unwrap();
    let path = dir.as_script_path();
    let out = cash(&format!(
        "figlet -f term -d '{path}' -C upper abc; figlet -f term -d '{path}' -C upper -N abc"
    ));
    assert_eq!(out.stdout, "ABC\nabc", "{out:?}");

    // utf8.flc is never needed, and missing is no error.
    let out = cash("figlet -f term -C utf8 'Ä x'");
    assert_eq!(out.code, 0, "{out:?}");
    let out = cash("figlet -f term -C nosuch x");
    assert_eq!(out.code, 1, "{out:?}");
    assert!(
        out.stderr.contains("nosuch: Unable to open control file"),
        "{out:?}"
    );
}

#[test]
fn standard_input_a_line_at_a_time() {
    // A CRLF line end is one newline, not a blank figure between the lines.
    let out = cash(r"printf 'one\r\ntwo\n\nfour' | figlet -f term");
    assert_eq!(out.stdout, "one\ntwo\n\nfour", "{out:?}");
    let out = cash(r"printf 'one\ntwo\n\nfour\n' | figlet -f term -p");
    assert_eq!(out.stdout, "one two\n\nfour", "{out:?}");
}

#[test]
fn into_a_pipe_the_width_is_80() {
    let out = cash("figlet -f term $(printf 'word %.0s' {1..30}) | awk '{ print length }'");
    let widths: Vec<usize> = out
        .stdout
        .lines()
        .map(|l| l.trim().parse().unwrap())
        .collect();
    assert!(widths.len() > 1, "{out:?}");
    assert!(widths.iter().all(|&w| w <= 79), "{out:?}");
    let out = cash("figlet -I4; figlet -w 40 -I4");
    assert_eq!(out.stdout, "80\n40", "{out:?}");
}

#[test]
fn info_codes() {
    let out = cash("figlet -I1; figlet -f slant.flf -I3; figlet -I5");
    assert_eq!(out.stdout, "20205\nslant\nflf2 tlf2", "{out:?}");
    let out = cash("figlet -v");
    assert!(out.stdout.starts_with("figlet (cash "), "{out:?}");
}

#[test]
fn errors_say_what_and_end_with_one() {
    let out = cash("figlet -f nosuch x");
    assert_eq!(out.code, 1);
    assert_eq!(out.stderr, "figlet: nosuch: Unable to open font file");

    let dir = Scratch::new("figlet-notafont");
    std::fs::write(dir.join("bad.flf"), "not a font\n").unwrap();
    let out = cash(&format!("figlet -f '{}/bad.flf' x", dir.as_script_path()));
    assert_eq!(out.code, 1);
    assert!(out.stderr.ends_with("Not a FIGlet 2 font file"), "{out:?}");

    let out = cash("figlet -q x");
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("invalid option -- 'q'"), "{out:?}");
}
