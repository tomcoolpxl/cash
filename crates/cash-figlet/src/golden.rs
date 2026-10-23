//! The renderer against figlet 2.2.5's own output: `tests/golden/cases.txt` lists the
//! cases, `generate.sh` wrote each one's `.out` from the real figlet in WSL.

use std::path::Path;

use crate::builtin;
use crate::control::Controls;
use crate::export;
use crate::font::Font;
use crate::render::{Direction, Justify, Layout, Settings, render, text_input, words_input};

/// One case: the options read as figlet reads them, then the message.
fn run(options: &str, message: &str) -> String {
    let mut settings = Settings::default();
    let mut font_name = "standard";
    let mut words = options.split_whitespace();
    while let Some(option) = words.next() {
        match option {
            "-f" => font_name = words.next().unwrap(),
            "-w" => settings.width = words.next().unwrap().parse().unwrap(),
            "-m" => settings.layout = Layout::Mode(words.next().unwrap().parse().unwrap()),
            "-C" => {
                // Only utf8, which changes nothing for cash's Unicode input.
                assert_eq!(words.next(), Some("utf8"));
            }
            "-c" => settings.justify = Some(Justify::Center),
            "-l" => settings.justify = Some(Justify::Left),
            "-r" => settings.justify = Some(Justify::Right),
            "-x" => settings.justify = None,
            "-k" => settings.layout = Layout::Kerning,
            "-W" => settings.layout = Layout::FullWidth,
            "-S" => settings.layout = Layout::Smush,
            "-o" => settings.layout = Layout::Overlap,
            "-s" => settings.layout = Layout::Font,
            "-p" => settings.paragraph = true,
            "-n" => settings.paragraph = false,
            "-L" => settings.direction = Some(Direction::LeftToRight),
            "-R" => settings.direction = Some(Direction::RightToLeft),
            "-X" => settings.direction = None,
            "-D" => settings.deutsch = true,
            "-E" => settings.deutsch = false,
            other => panic!("option {other} is not handled here"),
        }
    }
    let font = Font::parse(builtin::find(font_name).unwrap().bytes).unwrap();
    let input = match message.strip_prefix('<') {
        Some(text) => text_input(&text.replace("\\n", "\n").replace("\\t", "\t")),
        None => words_input(
            &message
                .split(' ')
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>(),
        ),
    };
    export::text(&render(&font, &Controls::new(), &settings, &input))
}

#[test]
fn same_output_as_figlet() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let cases = std::fs::read_to_string(dir.join("cases.txt")).unwrap();
    let mut failures = Vec::new();
    let mut count = 0;
    for line in cases.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut fields = line.splitn(3, '|');
        let (name, options, message) = (
            fields.next().unwrap(),
            fields.next().unwrap(),
            fields.next().unwrap(),
        );
        let expected = std::fs::read_to_string(dir.join(format!("{name}.out")))
            .unwrap_or_else(|e| panic!("{name}.out: {e}"))
            .replace("\r\n", "\n");
        let got = run(options, message);
        count += 1;
        if got != expected {
            failures.push(format!(
                "--- {name} ({options}): expected\n{expected}--- got\n{got}"
            ));
        }
    }
    assert!(count > 50, "only {count} cases read");
    assert!(
        failures.is_empty(),
        "{} of {count} cases differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
