//! `figlet`: big letters made of small ones (decisions.md D84,
//! research/figlet-design.md).
//!
//! One command doing what figlet 2.2.5 does, on `cash-figlet`, which lays text out as
//! figlet's own C does (61 cases match it byte for byte). The options are figlet's
//! `ADEXLRI:xlcrpntvm:w:d:f:C:NskSWo`, with toilet's long names for those the two share
//! (`--font`, `--directory`, `--width`, `--termwidth`, `--infocode`) and `-h`/`--help`.
//!
//! Where it differs, on purpose:
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
use cash_figlet::render::{text_input, words_input};
use cash_figlet::{
    Controls, Direction, Font, Justify, Layout, Renderer, Settings, builtin, export,
};
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

/// figlet 2.2.5's option string, with `h` for help.
const OPTSTRING: &str = "ADEXLRI:xlcrpntvm:w:d:f:C:NskSWoh";

/// The long names toilet gives the options it shares with figlet.
const LONGS: &[Long<'static, char>] = &[
    Long::new("font", Arg::Required, 'f'),
    Long::new("directory", Arg::Required, 'd'),
    Long::new("width", Arg::Required, 'w'),
    Long::new("termwidth", Arg::No, 't'),
    Long::new("infocode", Arg::Required, 'I'),
    Long::new("version", Arg::No, 'v'),
    Long::new("help", Arg::No, 'h'),
];

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
  -D, -E                [\\]{|}~ as German letters, or not (the default)

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
    message: Vec<String>,
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
            Ok(Err(early)) => return Ok(early),
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
                stdout.write_all(export::text_blocks(&renderer.take_blocks()).as_bytes())?;
            }
        } else {
            renderer.push(&words_input(&request.message));
        }
        stdout.write_all(export::text_blocks(&renderer.finish()).as_bytes())?;
        Ok(ExecutionResult::success())
    }
}

/// Reads the command line: the request, or the status to end with when it was
/// `--help`, or what is wrong with it.
fn read_command_line<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    args: &[String],
) -> Result<Result<Request, ExecutionResult>, cash_getopt::Problem> {
    let mut request = Request {
        font: "standard".to_owned(),
        directory: None,
        controls: Vec::new(),
        settings: Settings::default(),
        width: None,
        info: None,
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
        let s = &mut request.settings;
        match id {
            'h' => {
                let status = if writeln!(context.stdout(), "{HELP}").is_ok() {
                    ExecutionResult::success()
                } else {
                    ExecutionResult::general_error()
                };
                return Ok(Err(status));
            }
            'A' => {}
            'D' => s.deutsch = true,
            'E' => s.deutsch = false,
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
