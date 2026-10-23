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
];

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
      --shadow          a shadow down and right
      --outline         only the edges of solid letters
      --border[=STYLE]  a frame: single, double, rounded, heavy or ascii
      --padding=N       N blank columns inside the frame (half as many rows)
      --background=COLOR  every cell on COLOR
      --letter-spacing=N  N blank columns after every letter
      --line-gap=N      N blank rows after every line of text

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

        let font = match load_font(&context, &request) {
            Ok(font) => font,
            Err(message) => {
                writeln!(context.stderr(), "figlet: {message}")?;
                return Ok(ExecutionResult::general_error());
            }
        };
        let mut controls = Controls::new();
        let mut control_dirs = directories(&context, &request);
        control_dirs.push(".".to_owned());
        for name in &request.controls {
            let text = find_file(&context, &control_dirs, name, &[".flc"]).and_then(|bytes| {
                cash_figlet::font::unzipped(&bytes)
                    .ok()
                    .map(|t| t.into_owned())
            });
            match text {
                Some(text) => controls.add(&text),
                // cash's input is Unicode already: utf8.flc would change nothing.
                None if bare_name(name, ".flc") == "utf8" => {}
                None => {
                    writeln!(
                        context.stderr(),
                        "figlet: {name}: Unable to open control file"
                    )?;
                    return Ok(ExecutionResult::general_error());
                }
            }
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
            'f' => request.font = value,
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
) -> Result<Font, String> {
    let name = &request.font;
    let suffixes = [".flf", ".tlf"];
    let found = find_file(context, &directories(context, request), name, &suffixes)
        .or_else(|| {
            (!name.contains(['/', '\\']))
                .then(|| builtin::find(name))
                .flatten()
                .map(|font| font.bytes.to_vec())
        })
        .or_else(|| find_file(context, &[".".to_owned()], name, &suffixes));
    let bytes = found.ok_or_else(|| format!("{name}: Unable to open font file"))?;
    Font::parse(&bytes).map_err(|error| format!("{name}: {error}"))
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
        3 => writeln!(out, "{}", bare_name(&request.font, ".flf .tlf"))?,
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
    fn suffixes_come_off() {
        assert_eq!(bare_name("slant.flf", ".flf .tlf"), "slant");
        assert_eq!(bare_name("mono9.tlf", ".flf .tlf"), "mono9");
        assert_eq!(bare_name("utf8", ".flc"), "utf8");
    }
}
