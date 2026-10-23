//! `figlet`: big letters made of small ones (decisions.md D84,
//! research/figlet-design.md).
//!
//! One command doing what figlet 2.2.5 and toilet 0.3 do, on `cash-figlet`, which lays
//! text out as figlet's own C does (61 cases match it byte for byte). The options are
//! figlet's `ADXLRI:xlcrpntvm:w:d:f:C:NskSWo` and toilet's `-F` filters (`--rainbow`,
//! `--gay`, `--metal`) and `-E` exports, with toilet's long names for those the two
//! share (`--font`, `--directory`, `--width`, `--termwidth`, `--infocode`), `-h`/`--help`,
//! and `--color=auto|always|never`.
//!
//! Where it differs, on purpose:
//! - `-E` is toilet's export, not figlet's "German off", which only undid `-D` (off by
//!   default); the exports are `utf8`, `ansi` and `text`;
//! - colour goes to a console and, with `NO_COLOR` unset, nowhere else, unless `-E`
//!   names a format or `--color=always` asks; toilet colours a pipe too;
//! - input is Unicode text, so `-C utf8` is never needed (and works when `utf8.flc` is
//!   not there); the control files' other input encodings are not decoded;
//! - with neither `-w` nor `-t`, the width is the console's when the output goes to one,
//!   and figlet's 80 otherwise, so a pipe gets what figlet gives it;
//! - a CRLF line end in the input is one newline;
//! - fonts are looked for under `-d` (or `FIGLET_FONTDIR`), then in
//!   `%LOCALAPPDATA%\cash\fonts`, then among the 23 cash carries, then in the current
//!   directory; a name with a slash or backslash is a path.

use std::io::{BufRead, BufReader, Write};

use cash_core::input::Dash;
use cash_core::{ExecutionResult, builtins};
use cash_figlet::export::{self, Depth, Format};
use cash_figlet::filter::{self, Filter};
use cash_figlet::render::{text_input, words_input};
use cash_figlet::{Block, Controls, Direction, Font, Justify, Layout, Renderer, Settings, builtin};
use cash_getopt::{Arg, Getopt, Item, Long, optstring};
use clap::Parser;

use crate::options;

/// Print text in big letters.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct FigletCommand {
    /// Options and the message, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// figlet 2.2.5's option string with toilet's `F:` and `E:` (its export, which takes
/// figlet's `-E`), and `h` for help.
const OPTSTRING: &str = "ADE:XLRI:xlcrpntvm:w:d:f:C:NskSWohF:";

/// toilet's long options, and `--color`. The long-only ones are known by letters no
/// short option has.
const LONGS: &[Long<'static, char>] = &[
    Long::new("font", Arg::Required, 'f'),
    Long::new("directory", Arg::Required, 'd'),
    Long::new("width", Arg::Required, 'w'),
    Long::new("termwidth", Arg::No, 't'),
    Long::new("filter", Arg::Required, 'F'),
    Long::new("gay", Arg::No, 'G'),
    Long::new("rainbow", Arg::No, 'G'),
    Long::new("metal", Arg::No, 'M'),
    Long::new("export", Arg::Required, 'E'),
    Long::new("irc", Arg::No, 'Q'),
    Long::new("html", Arg::No, 'H'),
    Long::new("infocode", Arg::Required, 'I'),
    Long::new("version", Arg::No, 'v'),
    Long::new("help", Arg::No, 'h'),
    Long::new("color", Arg::Optional, 'K'),
    Long::new("colour", Arg::Optional, 'K'),
    // cash's own (D84): the colour pack.
    Long::new("gradient", Arg::Required, 'J'),
    Long::new("direction", Arg::Required, 'j'),
    Long::new("lolcat", Arg::No, 'Y'),
    Long::new("seed", Arg::Required, 'y'),
    Long::new("spread", Arg::Required, 'z'),
    Long::new("freq", Arg::Required, 'Z'),
    Long::new("shadow", Arg::No, 'u'),
    Long::new("outline", Arg::No, 'U'),
    Long::new("border", Arg::Optional, 'b'),
    Long::new("padding", Arg::Required, 'B'),
    Long::new("background", Arg::Required, 'e'),
    Long::new("letter-spacing", Arg::Required, 'P'),
    Long::new("line-gap", Arg::Required, 'q'),
    Long::new("colors", Arg::Required, 'O'),
    Long::new("colours", Arg::Required, 'O'),
    Long::new("pick", Arg::No, 'V'),
    // Pixel fonts, installed Windows fonts, and DEC double-size lines.
    Long::new("pixel", Arg::Optional, 'i'),
    Long::new("winfont", Arg::Required, 'a'),
    Long::new("windows-font", Arg::Required, 'a'),
    Long::new("size", Arg::Required, 'T'),
    Long::new("bold", Arg::No, 'g'),
    Long::new("italic", Arg::No, '1'),
    Long::new("dec", Arg::Optional, '2'),
];

/// What a pixel font or an installed Windows font is drawn with.
#[derive(Default)]
struct Pixels {
    mode: Option<cash_figlet::pixel::Mode>,
    winfont: Option<String>,
    size: Option<i32>,
    bold: bool,
    italic: bool,
}

/// DEC's double-size lines: the text itself, twice as high and wide, or twice as wide.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Dec {
    Big,
    Wide,
}

/// When colour is written.
#[derive(Clone, Copy, PartialEq, Eq)]
enum When {
    Auto,
    Always,
    Never,
}

/// The version figlet 2.2.5 reports with `-I1`, so a script checking for a figlet that
/// new finds one.
const FIGLET_VERSION_INT: u32 = 20205;

const HELP: &str = "\
Usage: figlet [OPTION]... [MESSAGE]
Print MESSAGE, or standard input, in big letters made of small ones.

Fonts and characters:
  -f, --font=FONT       the font, by name or path (default: standard)
  -d, --directory=DIR   look for fonts in DIR first (default: $FIGLET_FONTDIR)
  -C FILE               a control file, translating characters (repeatable)
  -N                    forget the control files given so far
  -D                    [\\]{|}~ as German letters

Layout:
  -s                    the font's own layout (the default)
  -S                    smush by the font's rules, overlapping where it has none
  -k                    kerning: letters touch
  -W                    full width: letters side by side
  -o                    overlap letters by one column
  -m MODE               a layout as a number: -1 full width, 0 kerning, rules
  -w, --width=COLUMNS   the output width (default: the console's, or 80)
  -t, --termwidth       the console's width
  -l, -c, -r, -x        left, centred, right, or as the font runs
  -L, -R, -X            left to right, right to left, or as the font runs
  -p, -n                paragraph mode (one newline is a blank), or not

Colour and output:
  -F, --filter=LIST     filters, colon-separated: crop, rainbow, metal, flip, flop,
                        180, left, right, border (-F list for what each does)
      --rainbow, --gay  the rainbow filter
      --metal           the metal filter
  -E, --export=FORMAT   utf8 (the default), ansi (code page 437) or text
      --color[=WHEN]    auto (a console, NO_COLOR unset), always or never
      --colors=DEPTH    true (the default), 256 or 16 colours

More colour and shape:
      --gradient=SPEC   a true-colour gradient: a palette (--gradient=list) or
                        colours, comma-separated (red,#ff8800,gold)
      --direction=DIR   horizontal (the default), vertical or diagonal
      --lolcat          lolcat's rainbow; --seed=N, --spread=F, --freq=F
      --shadow          box-line shadows, as cfonts' block font (solid letters)
      --outline         only the edges of solid letters
      --border[=STYLE]  a frame: single, double, rounded, heavy or ascii
      --padding=N       N blank columns inside the frame (half as many rows)
      --background=COLOR  every cell on COLOR
      --letter-spacing=N  N blank columns after every letter
      --line-gap=N      N blank rows after every line of text

Fonts:
      --pick            choose a font from a list, MESSAGE drawn in each as the
                        cursor moves; its name is printed (figlet -f \"$(figlet
                        --pick Hi)\" Hi)
  figlist, showfigfonts list the fonts, and show each

Pixels and Windows fonts:
      --pixel[=MODE]    the built-in 8x8 pixel font (or -f 8x8), drawn with half
                        (the default), full, quadrant, sextant, octant or braille
                        characters: more pixels to a cell, smaller letters
      --winfont=NAME    an installed Windows font (Consolas, \"Segoe UI\"), drawn
                        as pixels; --size=PIXELS (16), --bold, --italic
      --dec[=big|wide]  the text itself in the terminal's double-size lines

Information:
  -I CODE, --infocode=CODE  0 version, 1 version number, 2 font directory,
                        3 font, 4 width, 5 font formats
  -v, --version         the version
  -h, --help            this help

Fonts are looked for under -d or FIGLET_FONTDIR, then in %LOCALAPPDATA%\\cash\\fonts,
then among the 23 fonts cash carries, then in the current directory.
See `man figlet` for the fonts and examples.";

/// What the command line asked for.
struct Request {
    font: String,
    directory: Option<String>,
    controls: Vec<String>,
    settings: Settings,
    width: Option<usize>,
    info: Option<i64>,
    filters: Vec<Filter>,
    format: Option<Format>,
    color: Option<When>,
    extras: Extras,
    pick: bool,
    pixels: Pixels,
    dec: Option<Dec>,
    font_given: bool,
    message: Vec<String>,
}

/// The colour pack's settings, which apply to the filters wherever they were given.
struct Extras {
    direction: filter::Direction,
    seed: Option<f64>,
    spread: f64,
    frequency: f64,
    padding: usize,
    line_gap: usize,
    depth: Depth,
}

impl Default for Extras {
    fn default() -> Self {
        Self {
            direction: filter::Direction::Horizontal,
            seed: None,
            spread: 3.0,
            frequency: 0.1,
            padding: 0,
            line_gap: 0,
            depth: Depth::True,
        }
    }
}

/// How a command line ends early: what to print on each stream, and the status.
struct Early {
    stdout: String,
    stderr: String,
    status: ExecutionResult,
}

impl Early {
    /// `text` on standard output, and success.
    const fn print(text: String) -> Self {
        Self {
            stdout: text,
            stderr: String::new(),
            status: ExecutionResult::success(),
        }
    }

    /// `text` on standard error, and status 1.
    const fn fail(text: String) -> Self {
        Self {
            stdout: String::new(),
            stderr: text,
            status: ExecutionResult::general_error(),
        }
    }
}

impl builtins::Command for FigletCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let request = match read_command_line(&context, &self.args) {
            Ok(Ok(request)) => request,
            Ok(Err(early)) => {
                context.stdout().write_all(early.stdout.as_bytes())?;
                context.stderr().write_all(early.stderr.as_bytes())?;
                return Ok(early.status);
            }
            Err(problem) => return options::gnu_usage_error(&context, "figlet", problem, 1),
        };
        let mut settings = request.settings;
        settings.width = request.width.unwrap_or_else(|| default_width(&context));

        if let Some(code) = request.info {
            print_info(&context, &request, settings.width, code)?;
            return Ok(ExecutionResult::success());
        }

        let format = request.format.unwrap_or(Format::Utf8);
        let colour = match request.color {
            Some(When::Always) => true,
            Some(When::Never) => false,
            // A format asked for by name is written as toilet writes it, colour and all.
            None if request.format.is_some() => true,
            Some(When::Auto) | None => context
                .try_fd(cash_core::openfiles::OpenFiles::STDOUT_FD)
                .is_some_and(|fd| context.shell.colours(&fd)),
        };

        if let Some(dec) = request.dec {
            let text = plain_text(&context, &request)?;
            context
                .stdout()
                .write_all(&dec_lines(&request, dec, &text, colour, format))?;
            return Ok(ExecutionResult::success());
        }

        let message_text = request.message.join(" ");
        let loaded = match &request.pixels.winfont {
            Some(face) => windows_font(&request, face, &message_text),
            // --pixel alone draws the built-in pixel font.
            None if request.pixels.mode.is_some() && !request.font_given => {
                load_font(&context, &request, PIXEL_FONT)
            }
            None => load_font(&context, &request, &request.font),
        };
        let font = match loaded {
            Ok(font) => font,
            Err(message) => {
                writeln!(context.stderr(), "figlet: {message}")?;
                return Ok(ExecutionResult::general_error());
            }
        };
        let controls = match load_controls(&context, &request) {
            Ok(controls) => controls,
            Err(name) => {
                writeln!(
                    context.stderr(),
                    "figlet: {name}: Unable to open control file"
                )?;
                return Ok(ExecutionResult::general_error());
            }
        };

        if request.pick {
            return pick_font(&context, &request, &controls, &settings);
        }

        // Rows printed so far, so lolcat's waves go on from one line of text to the next.
        let mut rows_printed = 0;
        let mut out = |blocks: Vec<Block>| -> Vec<u8> {
            let blocks: Vec<Block> = blocks
                .iter()
                .map(|block| {
                    let mut block = filter::apply(&request.filters, block, rows_printed);
                    block
                        .rows
                        .extend(std::iter::repeat_with(Vec::new).take(request.extras.line_gap));
                    rows_printed += block.rows.len();
                    block
                })
                .collect();
            export::write(&blocks, format, colour, request.extras.depth)
        };

        let mut renderer = Renderer::new(&font, &controls, &settings);
        let mut stdout = context.stdout();
        if request.message.is_empty() {
            // A line of input is printed once it is read, as figlet prints it.
            let mut input = BufReader::new(context.stdin());
            let mut line = Vec::new();
            loop {
                line.clear();
                if input.read_until(b'\n', &mut line)? == 0 {
                    break;
                }
                let text = cash_figlet::font::decode(&line);
                renderer.push(&text_input(&text));
                stdout.write_all(&out(renderer.take_blocks()))?;
            }
        } else {
            renderer.push(&words_input(&request.message));
        }
        stdout.write_all(&out(renderer.finish()))?;
        Ok(ExecutionResult::success())
    }
}

/// A list as toilet prints `-F list` and `-E list`: a title, then `"name": what it is`.
fn list<'a>(title: &str, entries: impl Iterator<Item = (&'a str, &'a str)>) -> String {
    let mut text = format!("{title}:\n");
    for (name, about) in entries {
        text.push('"');
        text.push_str(name);
        text.push_str("\": ");
        text.push_str(about);
        text.push('\n');
    }
    text
}

/// The colour pack's options: whether `id` was one, and how it ends the command line
/// when it does.
fn extra_option(request: &mut Request, id: char, value: &str) -> Option<Result<(), Early>> {
    let bad = |what: &str| {
        Some(Err(Early::fail(format!(
            "figlet: invalid {what}: '{value}'\nTry 'figlet --help' for more information.\n"
        ))))
    };
    let number = |text: &str| text.trim().parse::<f64>().ok().filter(|n| n.is_finite());
    let count = |text: &str| text.trim().parse::<usize>().ok();
    let extras = &mut request.extras;
    match id {
        'J' if value == "list" => {
            let mut names = String::new();
            for (name, _) in cash_figlet::palette::PALETTES {
                names.push_str(name);
                names.push('\n');
            }
            return Some(Err(Early::print(names)));
        }
        'J' => match cash_figlet::palette::stops(value) {
            Ok(stops) => request.filters.push(Filter::Gradient {
                stops,
                direction: extras.direction,
            }),
            Err(_) => return bad("gradient"),
        },
        'j' => {
            extras.direction = match value {
                "horizontal" | "h" => filter::Direction::Horizontal,
                "vertical" | "v" => filter::Direction::Vertical,
                "diagonal" | "d" => filter::Direction::Diagonal,
                _ => return bad("direction"),
            };
        }
        'Y' => request.filters.push(Filter::Lolcat {
            seed: 0.0,
            spread: 3.0,
            frequency: 0.1,
        }),
        'y' => match number(value) {
            Some(seed) => extras.seed = Some(seed),
            None => return bad("seed"),
        },
        'z' => match number(value).filter(|n| *n > 0.0) {
            Some(spread) => extras.spread = spread,
            None => return bad("spread"),
        },
        'Z' => match number(value) {
            Some(frequency) => extras.frequency = frequency,
            None => return bad("frequency"),
        },
        'u' => request.filters.push(Filter::Shadow),
        'U' => request.filters.push(Filter::Outline),
        'b' => {
            let name = if value.is_empty() { "single" } else { value };
            match filter::FrameStyle::NAMES.iter().find(|(n, _)| *n == name) {
                Some((_, style)) => request.filters.push(Filter::Frame {
                    style: *style,
                    padding: 0,
                }),
                None => return bad("border style"),
            }
        }
        'B' => match count(value) {
            Some(padding) => extras.padding = padding,
            None => return bad("padding"),
        },
        'e' => match cash_figlet::Color::parse(value) {
            Some(colour) => request.filters.push(Filter::Background(colour)),
            None => return bad("colour"),
        },
        'P' => match count(value) {
            Some(spacing) => request.settings.letter_spacing = spacing,
            None => return bad("letter spacing"),
        },
        'q' => match count(value) {
            Some(gap) => extras.line_gap = gap,
            None => return bad("line gap"),
        },
        'O' => match Depth::NAMES.iter().find(|(n, _)| *n == value) {
            Some((_, depth)) => extras.depth = *depth,
            None => return bad("colour depth"),
        },
        _ => return pixel_option(request, id, value),
    }
    Some(Ok(()))
}

/// The pixel fonts' options, the Windows fonts' and `--dec`: whether `id` was one, and
/// how it ends the command line when it does.
fn pixel_option(request: &mut Request, id: char, value: &str) -> Option<Result<(), Early>> {
    let bad = |what: &str| {
        Some(Err(Early::fail(format!(
            "figlet: invalid {what}: '{value}'\nTry 'figlet --help' for more information.\n"
        ))))
    };
    match id {
        'i' => {
            let name = if value.is_empty() { "half" } else { value };
            match cash_figlet::pixel::Mode::NAMES
                .iter()
                .find(|(n, _)| *n == name)
            {
                Some((_, mode)) => request.pixels.mode = Some(*mode),
                None => return bad("pixel mode"),
            }
        }
        'a' => request.pixels.winfont = Some(value.to_owned()),
        'T' => match value.trim().parse::<i32>() {
            Ok(size @ 4..=512) => request.pixels.size = Some(size),
            _ => return bad("size (4 to 512 pixels)"),
        },
        'g' => request.pixels.bold = true,
        '1' => request.pixels.italic = true,
        '2' => {
            request.dec = Some(match value {
                "" | "big" => Dec::Big,
                "wide" => Dec::Wide,
                _ => return bad("--dec (big or wide)"),
            });
        }
        _ => return None,
    }
    Some(Ok(()))
}

/// The colour pack's settings given anywhere on the command line, on the filters they
/// belong to: the direction on gradients, lolcat's seed, spread and frequency, the
/// padding on frames. Without `--seed`, lolcat starts somewhere new each time, as it
/// does.
fn finish_extras(request: &mut Request) {
    let extras = &request.extras;
    let seed = extras.seed.unwrap_or_else(|| {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        f64::from(nanos % 256)
    });
    for filter in &mut request.filters {
        match filter {
            Filter::Gradient { direction, .. } => *direction = extras.direction,
            Filter::Lolcat {
                seed: s,
                spread,
                frequency,
            } => {
                *s = seed;
                *spread = extras.spread;
                *frequency = extras.frequency;
            }
            Filter::Frame { padding, .. } => *padding = extras.padding,
            _ => {}
        }
    }
}

/// toilet's options, and `--color`: whether `id` was one, and how it ends the command
/// line when it does.
fn toilet_option(request: &mut Request, id: char, value: &str) -> Option<Result<(), Early>> {
    let ends = |message: String| Some(Err(Early::fail(message)));
    match id {
        'F' if value == "list" => Some(Err(Early::print(list(
            "Available filters",
            filter::FILTERS
                .iter()
                .filter_map(|(name, _, about)| about.map(|about| (*name, about))),
        )))),
        'F' => match filter::parse_list(value) {
            Ok(filters) => {
                request.filters.extend(filters);
                Some(Ok(()))
            }
            Err(name) => ends(format!(
                "figlet: unknown filter: {name}\nTry 'figlet -F list' for the filters.\n"
            )),
        },
        'G' => {
            request.filters.push(Filter::Rainbow);
            Some(Ok(()))
        }
        'M' => {
            request.filters.push(Filter::Metal);
            Some(Ok(()))
        }
        'E' if value == "list" => Some(Err(Early::print(list(
            "Available export formats",
            export::FORMATS
                .iter()
                .map(|(name, _, about)| (*name, *about)),
        )))),
        'E' => match export::format_named(value) {
            Some(format) => {
                request.format = Some(format);
                Some(Ok(()))
            }
            None => ends(format!(
                "figlet: unknown export format: {value}\nTry 'figlet -E list' for the formats.\n"
            )),
        },
        'Q' | 'H' => {
            let name = if id == 'Q' { "irc" } else { "html" };
            ends(format!(
                "figlet: the {name} export is not carried\nTry 'figlet -E list' for the formats.\n"
            ))
        }
        'K' => {
            request.color = Some(match value {
                "" | "always" | "yes" | "force" => When::Always,
                "never" | "no" | "none" => When::Never,
                "auto" | "tty" | "if-tty" => When::Auto,
                other => {
                    return ends(format!(
                        "figlet: invalid argument '{other}' for '--color'\n\
                         Valid arguments are 'always', 'never' and 'auto'.\n"
                    ));
                }
            });
            Some(Ok(()))
        }
        _ => None,
    }
}

/// Reads the command line: the request, or the status to end with when it was
/// `--help`, or what is wrong with it.
fn read_command_line<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    args: &[String],
) -> Result<Result<Request, Early>, cash_getopt::Problem> {
    let mut request = Request {
        font: "standard".to_owned(),
        directory: None,
        controls: Vec::new(),
        settings: Settings::default(),
        width: None,
        info: None,
        filters: Vec::new(),
        format: None,
        color: None,
        extras: Extras::default(),
        pick: false,
        pixels: Pixels::default(),
        dec: None,
        font_given: false,
        message: Vec::new(),
    };
    let shorts = optstring(OPTSTRING);
    let getopt = Getopt::new(&shorts, LONGS).order(options::gnu_order(context));
    for item in getopt.read(args) {
        let (id, value) = match item? {
            Item::Option { id, value, .. } => (id, value),
            Item::Operand { value, .. } => {
                request.message.push(value);
                continue;
            }
        };
        let value = value.unwrap_or_default();
        match toilet_option(&mut request, id, &value)
            .or_else(|| extra_option(&mut request, id, &value))
        {
            Some(Ok(())) => continue,
            Some(Err(early)) => return Ok(Err(early)),
            None => {}
        }
        let s = &mut request.settings;
        match id {
            'h' => return Ok(Err(Early::print(format!("{HELP}\n")))),
            'V' => request.pick = true,
            'A' => {}
            'D' => s.deutsch = true,
            'X' => s.direction = None,
            'L' => s.direction = Some(Direction::LeftToRight),
            'R' => s.direction = Some(Direction::RightToLeft),
            'x' => s.justify = None,
            'l' => s.justify = Some(Justify::Left),
            'c' => s.justify = Some(Justify::Center),
            'r' => s.justify = Some(Justify::Right),
            'p' => s.paragraph = true,
            'n' => s.paragraph = false,
            's' => s.layout = Layout::Font,
            'k' => s.layout = Layout::Kerning,
            'S' => s.layout = Layout::Smush,
            'o' => s.layout = Layout::Overlap,
            'W' => s.layout = Layout::FullWidth,
            'm' => s.layout = Layout::Mode(i32::try_from(atoi(&value)).unwrap_or(-2)),
            't' => {
                if let Some(columns) = console_width(context) {
                    request.width = Some(columns);
                }
            }
            'w' => {
                if let Ok(columns @ 1..) = usize::try_from(atoi(&value)) {
                    request.width = Some(columns);
                }
            }
            'v' => request.info = Some(0),
            'I' => request.info = Some(atoi(&value)),
            'd' => request.directory = Some(value),
            'f' => {
                request.font = value;
                request.font_given = true;
            }
            'C' => request.controls.push(value),
            'N' => request.controls.clear(),
            _ => {}
        }
    }
    finish_extras(&mut request);
    // `-I -1` and other codes below 0 are figlet's "normal operation".
    if request.info.is_some_and(|code| code < 0) {
        request.info = None;
    }
    Ok(Ok(request))
}

/// A number as C's `atoi` reads one: blanks, a sign, digits up to the first that is not;
/// 0 for none.
fn atoi(text: &str) -> i64 {
    let text = text.trim_start();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let value = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0_i64, |n, d| {
            n.saturating_mul(10).saturating_add(i64::from(d - b'0'))
        });
    if negative { -value } else { value }
}

/// The console's width when standard output is the console.
fn console_width<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Option<usize> {
    let terminal = context
        .try_fd(cash_core::openfiles::OpenFiles::STDOUT_FD)
        .is_some_and(|fd| fd.is_terminal());
    terminal
        .then(|| crossterm::terminal::size().ok())
        .flatten()
        .map(|(columns, _)| usize::from(columns))
        .filter(|&columns| columns > 0)
}

/// The width with neither `-w` nor `-t`: the console's, or figlet's 80.
fn default_width<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> usize {
    console_width(context).unwrap_or(80)
}

/// The user's own font directory, `%LOCALAPPDATA%\cash\fonts`.
fn user_font_directory<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Option<String> {
    context
        .shell
        .env_str("LOCALAPPDATA")
        .map(|base| format!("{}/cash/fonts", base.replace('\\', "/")))
}

/// The directories fonts and control files are looked for in, in order, before the
/// built-in fonts: `-d` or `FIGLET_FONTDIR`, then the user's.
fn directories<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    request: &Request,
) -> Vec<String> {
    let mut dirs = Vec::new();
    if let Some(dir) = request.directory.clone().or_else(|| {
        context
            .shell
            .env_str("FIGLET_FONTDIR")
            .map(|d| d.to_string())
    }) {
        dirs.push(dir);
    }
    dirs.extend(user_font_directory(context));
    dirs
}

/// `name` without any of `suffixes`.
fn bare_name<'a>(name: &'a str, suffixes: &str) -> &'a str {
    suffixes
        .split(' ')
        .find_map(|suffix| name.strip_suffix(suffix))
        .unwrap_or(name)
}

/// The bytes of the file `name` stands for, with one of `suffixes` (the name given with
/// or without it): read as a path when it has a slash or backslash, else looked for in
/// `dirs`, in order.
fn find_file<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    dirs: &[String],
    name: &str,
    suffixes: &[&str],
) -> Option<Vec<u8>> {
    // Opened as the shell's `<` opens a file, relative to the shell's directory.
    let read = |path: String| {
        context
            .open_input(&path, Dash::File)
            .ok()
            .and_then(|input| input.read_all().ok())
    };
    let bare = bare_name(name, &suffixes.join(" "));
    let in_base = |base: &str| {
        suffixes
            .iter()
            .find_map(|suffix| read(format!("{base}{suffix}")))
    };
    if name.contains(['/', '\\']) {
        return in_base(bare);
    }
    dirs.iter()
        .find_map(|dir| in_base(&format!("{}/{bare}", dir.trim_end_matches(['/', '\\']))))
}

/// The font the request names, read: a path as given; else from the font directories,
/// then the built-in fonts, then the current directory.
fn load_font<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    request: &Request,
    name: &str,
) -> Result<Font, String> {
    if name == PIXEL_FONT {
        let mode = request.pixels.mode.unwrap_or_default();
        let chars = cash_figlet::pixel::wanted_chars(&request.message.join(" "));
        return Ok(cash_figlet::pixel::font8x8_font(mode, &chars));
    }
    let found = find_file(context, &directories(context, request), name, FONT_SUFFIXES)
        .or_else(|| {
            (!name.contains(['/', '\\']))
                .then(|| builtin::find(name))
                .flatten()
                .map(|font| font.bytes.to_vec())
        })
        .or_else(|| find_file(context, &[".".to_owned()], name, FONT_SUFFIXES));
    let bytes = found.ok_or_else(|| format!("{name}: Unable to open font file"))?;
    if cash_figlet::tdf::is_tdf(&bytes) {
        // A line as high as the message's letters, as tdfiglet draws it; standard input,
        // read as it comes, gets the whole font.
        let message = request.message.join(" ");
        let chars = cash_figlet::pixel::wanted_chars(&message);
        let only = (!message.is_empty()).then_some(chars.as_slice());
        return cash_figlet::tdf::parse_for(&bytes, only)
            .map_err(|error| format!("{name}: {error}"));
    }
    Font::parse(&bytes).map_err(|error| format!("{name}: {error}"))
}

/// What a font file's name ends in: FIGlet's, TOIlet's and TheDraw's.
const FONT_SUFFIXES: &[&str] = &[".flf", ".tlf", ".tdf"];

/// The control files the request names, read: those in the font directories or the
/// current directory; the name of one that cannot be read.
fn load_controls<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    request: &Request,
) -> Result<Controls, String> {
    let mut controls = Controls::new();
    let mut dirs = directories(context, request);
    dirs.push(".".to_owned());
    for name in &request.controls {
        let text = find_file(context, &dirs, name, &[".flc"]).and_then(|bytes| {
            cash_figlet::font::unzipped(&bytes)
                .ok()
                .map(|t| t.into_owned())
        });
        match text {
            Some(text) => controls.add(&text),
            // cash's input is Unicode already: utf8.flc would change nothing.
            None if bare_name(name, ".flc") == "utf8" => {}
            None => return Err(name.clone()),
        }
    }
    Ok(controls)
}

/// The name of the built-in pixel font, font8x8, drawn with `--pixel`'s characters.
const PIXEL_FONT: &str = "8x8";

/// An installed Windows font as a pixel font: the characters of `text` and the common
/// ones, drawn by GDI and turned into block characters.
fn windows_font(request: &Request, face: &str, text: &str) -> Result<Font, String> {
    use cash_figlet::pixel::{self, Bitmap};

    let pixels = &request.pixels;
    let mut rasterizer = cash_win32::rasterize::Rasterizer::new(
        face,
        pixels.size.unwrap_or(16),
        pixels.bold,
        pixels.italic,
    )
    .map_err(|error| format!("{face}: {error}"))?;
    let mut glyphs: Vec<(char, Bitmap)> = pixel::wanted_chars(text)
        .into_iter()
        .filter_map(|ch| {
            let raster = rasterizer.glyph(ch).ok()?;
            Some((
                ch,
                Bitmap::from_coverage(raster.width, raster.height, &raster.coverage),
            ))
        })
        .collect();
    pixel::trim(&mut glyphs);
    Ok(pixel::font(glyphs, pixels.mode.unwrap_or_default()))
}

/// The text as it is, for `--dec`: the message as figlet reads its words, or standard
/// input, a CRLF one newline and the last newline gone.
fn plain_text<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    request: &Request,
) -> std::io::Result<String> {
    if !request.message.is_empty() {
        return Ok(words_input(&request.message).into_iter().collect());
    }
    let mut input = Vec::new();
    std::io::Read::read_to_end(&mut context.stdin(), &mut input)?;
    let text = cash_figlet::font::decode(&input).replace("\r\n", "\n");
    Ok(text.strip_suffix('\n').unwrap_or(&text).to_owned())
}

/// `--dec`: each line of the text itself, in DEC's double-size lines (ESC # 3 and # 4,
/// the top and bottom halves of a line twice as high and wide, or ESC # 6, twice as
/// wide), filtered and coloured like any figure.
fn dec_lines(request: &Request, dec: Dec, text: &str, colour: bool, format: Format) -> Vec<u8> {
    let mut out = Vec::new();
    for line in text.split('\n') {
        let block = Block {
            rows: vec![cash_figlet::canvas::cells(line, cash_figlet::Style::PLAIN)],
        };
        let block = filter::apply(&request.filters, &block, 0);
        let written = export::write(&[block], format, colour, request.extras.depth);
        for row in String::from_utf8_lossy(&written).lines() {
            match dec {
                Dec::Big => {
                    for half in ["\x1b#3", "\x1b#4"] {
                        out.extend_from_slice(half.as_bytes());
                        out.extend_from_slice(row.as_bytes());
                        out.push(b'\n');
                    }
                }
                Dec::Wide => {
                    out.extend_from_slice(b"\x1b#6");
                    out.extend_from_slice(row.as_bytes());
                    out.push(b'\n');
                }
            }
        }
    }
    out
}

/// The names of the fonts with the suffix `.flf` or `.tlf` in `dir`, sorted.
fn fonts_in<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    dir: &str,
    suffixes: &[&str],
) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(context.shell.absolute_path(dir)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            suffixes
                .iter()
                .find(|suffix| lower.ends_with(*suffix))
                .and_then(|suffix| name.get(..name.len() - suffix.len()).map(str::to_owned))
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Every font that can be named: those in the font directories and those built in, each
/// name once, sorted.
fn font_names<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    request: &Request,
) -> Vec<String> {
    let mut names: Vec<String> = directories(context, request)
        .iter()
        .flat_map(|dir| fonts_in(context, dir, FONT_SUFFIXES))
        .chain(builtin::FONTS.iter().map(|font| font.name.to_owned()))
        .chain(std::iter::once(PIXEL_FONT.to_owned()))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// `line` cut to `width` columns, its escapes kept and colour reset after a cut.
fn clip(line: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthChar as _;
    let mut out = String::with_capacity(line.len());
    let mut used = 0;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            out.push(c);
            for e in chars.by_ref() {
                out.push(e);
                if e.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        let w = c.width().unwrap_or(0);
        if used + w > width {
            out.push_str("\x1b[0m");
            return out;
        }
        used += w;
        out.push(c);
    }
    out
}

/// `--pick`: a list of the fonts, the message drawn in the one the cursor is on above
/// it; the name of the one picked is printed.
fn pick_font<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    request: &Request,
    controls: &Controls,
    settings: &Settings,
) -> Result<ExecutionResult, cash_core::Error> {
    use cash_picker::colours::Colours;
    use cash_picker::list::{Line, List, Scope, Setup};

    let Ok(mut console) = std::fs::OpenOptions::new().write(true).open("CONOUT$") else {
        writeln!(context.stderr(), "figlet: no terminal to draw on")?;
        return Ok(ExecutionResult::general_error());
    };
    let var = |name: &str| context.shell.env_str(name).map(|value| value.into_owned());
    let colour = var("NO_COLOR").is_none_or(|value| value.is_empty());
    let names = font_names(context, request);
    let mut list = List::new(Setup {
        scopes: vec![Scope {
            title: "Fonts".to_owned(),
            lines: names
                .iter()
                .map(|name| Line {
                    text: name.clone(),
                    when: None,
                })
                .collect(),
        }],
        typed: String::new(),
        hints: "Enter pick  Esc".to_owned(),
        colours: if colour {
            Colours::new(None, var("CASH_PICKER_COLORS").as_deref())
        } else {
            Colours::none()
        },
        paths: false,
    });
    let mut fonts: std::collections::HashMap<String, Option<Font>> =
        std::collections::HashMap::new();
    let mut preview = |_: usize, index: usize, width: usize, rows: usize| -> Vec<String> {
        let Some(name) = names.get(index) else {
            return Vec::new();
        };
        let font = fonts
            .entry(name.clone())
            .or_insert_with(|| load_font(context, request, name).ok());
        let Some(font) = font else {
            return vec![format!("{name}: Unable to open font file")];
        };
        let words = if request.message.is_empty() {
            vec![name.clone()]
        } else {
            request.message.clone()
        };
        let mut shown = *settings;
        // figlet takes a column less than its width; the preview may take them all.
        shown.width = width + 1;
        let figure = cash_figlet::render(font, controls, &shown, &words_input(&words));
        let blocks: Vec<Block> = figure
            .blocks
            .iter()
            .map(|block| filter::apply(&request.filters, block, 0))
            .collect();
        let bytes = export::write(&blocks, Format::Utf8, colour, request.extras.depth);
        String::from_utf8_lossy(&bytes)
            .lines()
            .take(rows)
            .map(|line| clip(line, width))
            .collect()
    };
    let height = var("CASH_PICKER_HEIGHT").unwrap_or_else(|| "75%".to_owned());
    match cash_picker::term::run_list_previewed(
        &mut list,
        &mut console,
        Some(&height),
        &mut Vec::new(),
        &mut preview,
    ) {
        Ok(Some((_, index, _))) => {
            if let Some(name) = names.get(index) {
                writeln!(context.stdout(), "{name}")?;
            }
            Ok(ExecutionResult::success())
        }
        Ok(None) => Ok(ExecutionResult::general_error()),
        Err(error) => {
            writeln!(context.stderr(), "figlet: {error}")?;
            Ok(ExecutionResult::general_error())
        }
    }
}

/// List the fonts and control files.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct FiglistCommand {
    /// Options, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Show each font, its name drawn in it.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct ShowfigfontsCommand {
    /// Options and the word, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// What `figlist` and `showfigfonts` take: `-d DIR`, `--help`, and words.
fn read_lister_line<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    args: &[String],
    tool: &str,
    help: &str,
) -> Result<Result<(Request, Vec<String>), Early>, cash_getopt::Problem> {
    const LISTER_LONGS: &[Long<'static, char>] = &[
        Long::new("directory", Arg::Required, 'd'),
        Long::new("help", Arg::No, 'h'),
    ];
    let shorts = optstring("d:h");
    let getopt = Getopt::new(&shorts, LISTER_LONGS).order(options::gnu_order(context));
    let mut directory = None;
    let mut words = Vec::new();
    for item in getopt.read(args) {
        match item? {
            Item::Option { id: 'h', .. } => {
                return Ok(Err(Early::print(format!("Usage: {tool} {help}\n"))));
            }
            Item::Option { value, .. } => directory = value,
            Item::Operand { value, .. } => words.push(value),
        }
    }
    let mut request = Request {
        font: "standard".to_owned(),
        directory,
        controls: Vec::new(),
        settings: Settings::default(),
        width: None,
        info: None,
        filters: Vec::new(),
        format: None,
        color: None,
        extras: Extras::default(),
        pick: false,
        pixels: Pixels::default(),
        dec: None,
        font_given: false,
        message: Vec::new(),
    };
    request.settings.width = default_width(context);
    Ok(Ok((request, words)))
}

impl builtins::Command for FiglistCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let help = "[-d DIRECTORY]\nList the fonts figlet can use, and the control files.";
        let request = match read_lister_line(&context, &self.args, "figlist", help) {
            Ok(Ok((request, _))) => request,
            Ok(Err(early)) => {
                context.stdout().write_all(early.stdout.as_bytes())?;
                return Ok(early.status);
            }
            Err(problem) => return options::gnu_usage_error(&context, "figlist", problem, 1),
        };
        let dirs = directories(&context, &request);
        let mut text = String::from("Default font: standard\n");
        text.push_str("Font directory: ");
        text.push_str(dirs.first().map_or("(none)", String::as_str));
        text.push_str("\nFonts built into cash:\n");
        for font in builtin::FONTS {
            text.push_str(font.name);
            text.push('\n');
        }
        for dir in &dirs {
            let fonts = fonts_in(&context, dir, FONT_SUFFIXES);
            if !fonts.is_empty() {
                text.push_str("Fonts in ");
                text.push_str(dir);
                text.push_str(":\n");
                for name in fonts {
                    text.push_str(&name);
                    text.push('\n');
                }
            }
            let controls = fonts_in(&context, dir, &[".flc"]);
            if !controls.is_empty() {
                text.push_str("Control files in ");
                text.push_str(dir);
                text.push_str(":\n");
                for name in controls {
                    text.push_str(&name);
                    text.push('\n');
                }
            }
        }
        context.stdout().write_all(text.as_bytes())?;
        Ok(ExecutionResult::success())
    }
}

impl builtins::Command for ShowfigfontsCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let help =
            "[-d DIRECTORY] [WORD]\nShow each font figlet can use, WORD (or its name) drawn in it.";
        let (request, words) = match read_lister_line(&context, &self.args, "showfigfonts", help) {
            Ok(Ok(read)) => read,
            Ok(Err(early)) => {
                context.stdout().write_all(early.stdout.as_bytes())?;
                return Ok(early.status);
            }
            Err(problem) => {
                return options::gnu_usage_error(&context, "showfigfonts", problem, 1);
            }
        };
        let controls = Controls::new();
        let mut stdout = context.stdout();
        for name in font_names(&context, &request) {
            let Ok(font) = load_font(&context, &request, &name) else {
                continue;
            };
            let shown = if words.is_empty() {
                vec![name.clone()]
            } else {
                words.clone()
            };
            let figure =
                cash_figlet::render(&font, &controls, &request.settings, &words_input(&shown));
            writeln!(stdout, "{name} :")?;
            stdout.write_all(export::text(&figure).as_bytes())?;
            writeln!(stdout)?;
        }
        Ok(ExecutionResult::success())
    }
}

/// `-I CODE`: what figlet tells about itself.
fn print_info<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    request: &Request,
    width: usize,
    code: i64,
) -> Result<(), cash_core::Error> {
    let mut out = context.stdout();
    match code {
        0 => writeln!(
            out,
            "figlet (cash {}): FIGlet 2.2.5 and TOIlet 0.3, rewritten in Rust for cash.\n\
             FIGlet is by Glenn Chappell, Ian Chai, John Cowan, Christiaan Keet and Claudio\n\
             Matsuoka; TOIlet and its fonts by Sam Hocevar.\n\n\
             Usage: figlet [OPTION]... [MESSAGE]",
            env!("CARGO_PKG_VERSION")
        )?,
        1 => writeln!(out, "{FIGLET_VERSION_INT}")?,
        2 => writeln!(
            out,
            "{}",
            directories(context, request)
                .first()
                .map_or("", String::as_str)
        )?,
        3 => writeln!(out, "{}", bare_name(&request.font, ".flf .tlf .tdf"))?,
        4 => writeln!(out, "{width}")?,
        5 => writeln!(out, "flf2 tlf2")?,
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{atoi, bare_name};

    #[test]
    fn numbers_as_atoi_reads_them() {
        assert_eq!(atoi("40"), 40);
        assert_eq!(atoi(" -1x"), -1);
        assert_eq!(atoi("abc"), 0);
        assert_eq!(atoi("+7"), 7);
    }

    #[test]
    fn clipping_keeps_escapes_and_counts_columns() {
        use super::clip;
        assert_eq!(clip("abcdef", 4), "abcd\u{1b}[0m");
        assert_eq!(
            clip("\u{1b}[0;31mabc\u{1b}[0m", 2),
            "\u{1b}[0;31mab\u{1b}[0m"
        );
        assert_eq!(clip("ab", 4), "ab");
        assert_eq!(clip("アイ", 3), "ア\u{1b}[0m");
    }

    #[test]
    fn suffixes_come_off() {
        assert_eq!(bare_name("slant.flf", ".flf .tlf"), "slant");
        assert_eq!(bare_name("mono9.tlf", ".flf .tlf"), "mono9");
        assert_eq!(bare_name("utf8", ".flc"), "utf8");
    }
}
