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
use std::path::Path;
use std::time::Duration;

use cash_win32::conpty::ConPtySession;

use crate::common::{
    CASH, Scratch, cash_command, output_of, run as cash, run_in, with_isolated_environment,
};

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
fn toilet_s_filters() {
    let out = cash("figlet -f term -F flip 'b/'; figlet -f term -F crop:border hi");
    assert_eq!(out.stdout, "\\d\n┌──┐\n│hi│\n└──┘", "{out:?}");
    let out = cash("figlet -f term --rainbow x | od -c | head -1");
    // Into a pipe, colour is off unless asked for.
    assert!(!out.stdout.contains("033"), "{out:?}");

    let out = cash("figlet -F list; figlet -E list");
    assert!(
        out.stdout
            .starts_with("Available filters:\n\"crop\": crop unused blanks"),
        "{out:?}"
    );
    assert!(out.stdout.contains("\"ansi\":"), "{out:?}");

    let out = cash("figlet -F sparkle x");
    assert_eq!(out.code, 1);
    assert!(
        out.stderr.starts_with("figlet: unknown filter: sparkle"),
        "{out:?}"
    );
    let out = cash("figlet --html x");
    assert_eq!(out.code, 1);
    assert!(
        out.stderr.contains("the html export is not carried"),
        "{out:?}"
    );
}

#[test]
fn colour_when_asked_or_on_a_console() {
    let metal = "\u{1b}[0;1;34;94mx\u{1b}[0m";
    let out = cash("figlet -f term --metal --color=always x");
    assert_eq!(out.stdout, metal, "{out:?}");
    // A format named with -E is written as toilet writes it, colour and all.
    let out = cash("figlet -f term --metal -E utf8 x");
    assert_eq!(out.stdout, metal, "{out:?}");
    let out = cash("figlet -f term --metal -E utf8 --color=never x");
    assert_eq!(out.stdout, "x", "{out:?}");
    let out = cash("figlet -f term --metal -E text --color=always x");
    assert_eq!(out.stdout, "x", "{out:?}");
    let out = cash("figlet --color=sometimes x");
    assert_eq!(out.code, 1, "{out:?}");
}

#[test]
fn gradients_lolcat_and_depth() {
    // ocean's first and last stops at either end of "ab".
    let out = cash("figlet -f term --gradient=ocean --color=always ab");
    assert_eq!(
        out.stdout, "\u{1b}[0;38;2;0;198;255ma\u{1b}[0;38;2;0;31;92mb\u{1b}[0m",
        "{out:?}"
    );
    // In sixteen colours, red and blue are the ANSI ones.
    let out = cash("figlet -f term --gradient=red,blue --colors=16 --color=always ab");
    assert_eq!(out.stdout, "\u{1b}[0;31ma\u{1b}[0;34mb\u{1b}[0m", "{out:?}");
    let out = cash("figlet -f term --gradient=#ff0000 --colors=256 --color=always x");
    assert!(out.stdout.contains("38;5;196"), "{out:?}");
    // lolcat with a seed is the same each time.
    let twice = cash(
        "figlet -f term --lolcat --seed=7 --color=always abc; figlet -f term --lolcat --seed=7 --color=always abc",
    );
    let lines: Vec<&str> = twice.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{twice:?}");
    assert_eq!(lines[0], lines[1], "{twice:?}");
    let out = cash("figlet --gradient=list");
    assert!(out.stdout.lines().any(|l| l == "sunset"), "{out:?}");
    let out = cash("figlet --gradient=red,nope x");
    assert_eq!(out.code, 1, "{out:?}");
}

#[test]
fn frames_shadows_and_spacing() {
    let out = cash("figlet -f term --border=double --padding=2 hi");
    assert_eq!(
        out.stdout, "╔══════╗\n║      ║\n║  hi  ║\n║      ║\n╚══════╝",
        "{out:?}"
    );
    let out = cash("figlet -f term --border hi");
    assert_eq!(out.stdout, "┌──┐\n│hi│\n└──┘", "{out:?}");
    let out = cash("figlet -f term --shadow ab");
    assert_eq!(out.stdout, "ab \n ░░", "{out:?}");
    // The spacing follows every letter; a gap follows every line of text.
    let out = cash("figlet -f term --letter-spacing=2 abc; figlet -f term --line-gap=1 a '' b");
    assert_eq!(out.stdout, "a  b  c  \na \n\nb", "{out:?}");
    let out = cash("figlet -f term --background=navy --color=always x");
    assert_eq!(out.stdout, "\u{1b}[0;48;2;0;0;128mx\u{1b}[0m", "{out:?}");
    let out = cash("figlet --border=wavy x");
    assert_eq!(out.code, 1, "{out:?}");
}

#[test]
fn figlist_and_showfigfonts() {
    let dir = Scratch::new("figlet-lists");
    std::fs::write(dir.join("brackets.flf"), bracket_font()).unwrap();
    std::fs::write(dir.join("upper.flc"), "flc2a\nt a-z A-Z\n").unwrap();
    let path = dir.as_script_path();
    let out = cash(&format!("figlist -d '{path}'"));
    assert!(
        out.stdout
            .starts_with("Default font: standard\nFont directory: "),
        "{out:?}"
    );
    assert!(
        out.stdout.contains("Fonts built into cash:\nbanner\nbig\n"),
        "{out:?}"
    );
    assert!(out.stdout.contains(":\nbrackets\n"), "{out:?}");
    assert!(out.stdout.ends_with(":\nupper"), "{out:?}");

    // The fonts come sorted, the directory's among the built-in ones.
    let out = cash(&format!(
        "showfigfonts -d '{path}' ab | grep ' :$' | head -3"
    ));
    // The pixel font, 8x8, sorts first.
    assert_eq!(out.stdout, "8x8 :\nbanner :\nbig :", "{out:?}");
    let out = cash(&format!(
        "showfigfonts -d '{path}' | grep -A1 '^brackets :'"
    ));
    assert_eq!(
        out.stdout, "brackets :\n[b][r][a][c][k][e][t][s]",
        "{out:?}"
    );
}

#[test]
fn the_picker_draws_the_font_under_the_cursor_and_prints_the_pick() {
    let mut session = with_isolated_environment(|env| {
        ConPtySession::start_sized(
            Path::new(CASH),
            &[
                "--no-config",
                "-c",
                "f=$(figlet --pick Hi); echo \"picked=$f\"",
            ],
            Some(env),
            None,
            100,
            40,
        )
    })
    .expect("start cash in a pseudo terminal");
    let (shown, text) = session
        .wait_for_screen(
            |t| t.contains("Fonts") && t.contains("Enter pick"),
            Duration::from_secs(20),
        )
        .unwrap();
    assert!(shown, "the list:\n{text}");
    session.send("slant").unwrap();
    // slant's "Hi", drawn above the list.
    let (shown, text) = session
        .wait_for_screen(|t| t.contains("/ /_/ / /"), Duration::from_secs(10))
        .unwrap();
    assert!(shown, "the preview:\n{text}");
    session.send("\r").unwrap();
    session
        .expect("picked=slant", Duration::from_secs(10))
        .expect("the pick is printed");
}

#[test]
fn the_pixel_font_in_its_modes() {
    let out = cash("figlet --pixel Hi");
    assert_eq!(
        out.stdout, "██  ██    ▀▀    \n██▄▄██   ▀██    \n██  ██    ██    \n▀▀  ▀▀   ▀▀▀▀",
        "{out:?}"
    );
    let out = cash("figlet -f 8x8 --pixel=quadrant Hi");
    assert_eq!(
        out.stdout, "█ █  ▀  \n█▄█ ▝█  \n█ █  █  \n▀ ▀ ▝▀▘",
        "{out:?}"
    );
    // Two by four pixels to a cell: an 8 by 8 letter in two rows of four.
    let out = cash("figlet --pixel=braille H");
    assert_eq!(out.stdout.lines().count(), 2, "{out:?}");
    assert!(
        out.stdout
            .chars()
            .all(|c| c == '\n' || ('\u{2800}'..='\u{28ff}').contains(&c)),
        "{out:?}"
    );
    let out = cash("figlet --pixel=sparkle x");
    assert_eq!(out.code, 1, "{out:?}");
}

#[test]
fn an_installed_windows_font() {
    let out = cash("figlet --winfont=Arial --size=24 --pixel=half Hi");
    assert_eq!(out.code, 0, "{out:?}");
    assert!(out.stdout.lines().count() >= 6, "{out:?}");
    assert!(out.stdout.contains('█'), "{out:?}");
    let out = cash("figlet --winfont='No Such Font' x");
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        "figlet: No Such Font: no font called No Such Font is installed"
    );
}

/// A TheDraw font file with `A`, two red-on-blue full blocks over a white upper half
/// block, of type `kind` (2 colour, 0 outline), a column of spacing.
fn tiny_tdf(kind: u8) -> Vec<u8> {
    let mut file = b"\x13TheDraw FONTS file\x1a".to_vec();
    file.extend_from_slice(&[0x55, 0xaa, 0x00, 0xff, 4]);
    file.extend_from_slice(b"TINY\0\0\0\0\0\0\0\0\0\0\0\0");
    file.extend_from_slice(&[kind, 1]);
    let a: &[u8] = &[2, 2, 0xdb, 0x14, 0xdb, 0x14, b'\r', 0xdf, 0x0f, 0];
    file.extend_from_slice(&u16::try_from(a.len()).unwrap().to_le_bytes());
    for ch in '!'..='~' {
        let offset: u16 = if ch == 'A' { 0 } else { 0xffff };
        file.extend_from_slice(&offset.to_le_bytes());
    }
    file.extend_from_slice(a);
    file
}

#[test]
fn thedraw_fonts() {
    let dir = Scratch::new("figlet-tdf");
    std::fs::write(dir.join("tiny.tdf"), tiny_tdf(2)).unwrap();
    std::fs::write(dir.join("outline.tdf"), tiny_tdf(0)).unwrap();
    let path = dir.as_script_path();
    let out = cash(&format!("figlet -d '{path}' -f tiny A"));
    assert_eq!(out.stdout, "██ \n▀", "{out:?}");
    let out = cash(&format!(
        "figlet -f '{path}/tiny.tdf' --color=always A | head -1"
    ));
    // The colour, the two blocks, a reset, and the column of spacing (trimmed here).
    assert_eq!(out.stdout, "\u{1b}[0;31;44m██\u{1b}[0m", "{out:?}");
    let out = cash(&format!("figlet -d '{path}' -f outline A"));
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("outline fonts are not read"), "{out:?}");
    let out = cash(&format!("figlist -d '{path}'"));
    assert!(out.stdout.contains(":\noutline\ntiny"), "{out:?}");
}

#[test]
fn dec_double_size_lines() {
    let out = cash("figlet --dec Big | od -c | head -1");
    assert!(
        out.stdout
            .contains("033   #   3   B   i   g  \\n 033   #   4   B   i   g"),
        "{out:?}"
    );
    let out = cash("figlet --dec=wide Big; printf 'a\\nb\\n' | figlet --dec=wide");
    assert_eq!(out.stdout, "\u{1b}#6Big\n\u{1b}#6a\n\u{1b}#6b", "{out:?}");
}

#[test]
fn the_ansi_export_is_code_page_437() {
    let out = cash("figlet -f term -E ansi --color=never '£±é' | od -An -tx1");
    assert_eq!(
        out.stdout.split_whitespace().collect::<Vec<_>>(),
        ["9c", "f1", "82", "0d", "0a"],
        "{out:?}"
    );
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
