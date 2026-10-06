//! `xxd`, vim's: a hex dump, and a hex dump read back into bytes.
//!
//! Checked against vim's xxd of 2026-06-16 (`crates/cash/tests/oracle`). The layout keeps
//! xxd's own arithmetic: where a byte's digits go, where the text column starts and how a
//! little-endian group is turned round are the same integer expressions, so `-c`, `-g`,
//! `-b` and `-e` line up as there, partial groups included. `-a` folds a run of all-zero
//! lines into one `*` by xxd's rule (the first is printed, the second as well when the
//! run is two long), `-i` writes the C array with the file name made an identifier, `-p`
//! the plain hex, `-r` reads a dump back, seeking inside an output file or zero-filling a
//! stream, and `-R always` colours each byte class with xxd's sequences. Numbers are read
//! as C's `strtol` reads them: `0x` is hex, a leading `0` octal, and trailing text is
//! ignored.
//!
//! Standard input is read whole, so `-s` on it skips forward, and a backward seek is
//! refused as xxd refuses one on a pipe. `-E` (EBCDIC) is refused: cash has no EBCDIC.

use std::io::{Read, Write};

use cash_core::openfiles::OpenFiles;
use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Make a hex dump, or reverse one.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct XxdCommand {
    /// Options and files, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

const USAGE: &str = "Usage:
       xxd [options] [infile [outfile]]
    or
       xxd -r [-s [-]offset] [-c cols] [-ps] [infile [outfile]]
Options:
    -a          toggle autoskip: A single '*' replaces nul-lines. Default off.
    -b          binary digit dump (incompatible with -ps). Default hex.
    -C          capitalize variable names in C include file style (-i).
    -c cols     format <cols> octets per line. Default 16 (-i: 12, -ps: 30).
    -E          show characters in EBCDIC (not supported in cash).
    -e          little-endian dump (incompatible with -ps,-i,-r).
    -g bytes    number of octets per group in normal output. Default 2 (-e: 4).
    -h          print this summary.
    -i          output in C include file style.
    -t          append terminating zero to C include output (-i).
    -l len      stop after <len> octets.
    -n name     set the variable name used in C include output (-i).
    -o off      add <off> to the displayed file position.
    -ps         output in postscript plain hexdump style.
    -r          reverse operation: convert (or patch) hexdump into binary.
    -r -s off   revert with <off> added to file positions found in hexdump.
    -d          show offset in decimal instead of hex.
    -s [+][-]seek  start at <seek> bytes abs. (or +: rel.) infile offset.
    -u          use upper case hex letters.
    -R when     colorize the output; <when> can be 'always', 'auto' or 'never'. Default: 'auto'.
    -v          show version.
";

const VERSION: &str = "xxd (cash): vim's xxd, 2026-06-16 options";

/// The output styles, as bits so that two of them can be told apart from one.
const HEX_NORMAL: u8 = 0;
const HEX_POSTSCRIPT: u8 = 1;
const HEX_CINCLUDE: u8 = 2;
const HEX_BITS: u8 = 4;
const HEX_LITTLEENDIAN: u8 = 8;

/// The most columns a hex, bits or little-endian dump may have.
const MAX_COLS: i64 = 256;

/// xxd's colour classes, as the digit of the SGR code `ESC [ 1 ; 3 <digit> m`.
const COLOR_RED: u8 = b'1';
const COLOR_GREEN: u8 = b'2';
const COLOR_YELLOW: u8 = b'3';
const COLOR_BLUE: u8 = b'4';
const COLOR_WHITE: u8 = b'7';

/// When the output is coloured.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Colour {
    /// Not asked: a terminal, unless `NO_COLOR` is set.
    #[default]
    Unasked,
    /// `-R auto`: a terminal.
    Auto,
    /// `-R always`.
    Always,
    /// `-R never`.
    Never,
}

/// The command line, read as xxd reads it.
#[derive(Default)]
struct Options {
    autoskip: bool,
    hextype: u8,
    upper: bool,
    capitalize: bool,
    decimal: bool,
    revert: bool,
    ebcdic: bool,
    termination: bool,
    cols: i64,
    cols_given: bool,
    octets_per_group: i64,
    display_offset: u64,
    seek: i64,
    relative_seek: bool,
    negative_seek: bool,
    length: i64,
    varname: Option<String>,
    colour: Colour,
    infile: Option<String>,
    outfile: Option<String>,
}

/// Why the command stops before dumping anything.
enum Early {
    /// The usage text, to standard error, status 1.
    Usage,
    /// The version line, status 0.
    Version,
    /// `xxd: <message>`, with the status xxd gives it.
    Failed(Failure),
}

/// An error xxd reports as `xxd: <message>` and a status.
struct Failure {
    status: u8,
    message: String,
}

impl Failure {
    fn new(status: u8, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn cannot_seek() -> Self {
        Self::new(4, "Sorry, cannot seek.")
    }
}

impl From<Failure> for Early {
    fn from(failure: Failure) -> Self {
        Self::Failed(failure)
    }
}

/// `text` as C's `strtol(text, NULL, 0)` reads it: leading white space, a sign, hex after
/// `0x`, octal after `0`, else decimal; the digits that follow, saturating; anything
/// after them ignored, and 0 when there are none.
fn strtol(text: &str) -> i64 {
    let (negative, magnitude) = unsigned_prefix(text);
    let value = i64::try_from(magnitude).unwrap_or(i64::MAX);
    if negative {
        value.saturating_neg()
    } else {
        value
    }
}

/// `text` as C's `strtoul(text, NULL, 0)` reads it: as [`strtol`], but a negative number
/// wraps round, as `-o -5` relies on.
fn strtoul(text: &str) -> u64 {
    let (negative, magnitude) = unsigned_prefix(text);
    let value = u64::try_from(magnitude).unwrap_or(u64::MAX);
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

/// The sign and the magnitude at the start of `text`, by `strtol`'s rules.
fn unsigned_prefix(text: &str) -> (bool, u128) {
    let text = text.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (radix, digits) = match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(hex) if hex.starts_with(|c: char| c.is_ascii_hexdigit()) => (16, hex),
        _ if digits.starts_with('0') => (8, digits),
        _ => (10, digits),
    };
    let mut value: u128 = 0;
    for c in digits.chars() {
        let Some(digit) = c.to_digit(radix) else {
            break;
        };
        value = value
            .saturating_mul(u128::from(radix))
            .saturating_add(u128::from(digit));
    }
    (negative, value)
}

/// Reads the command line as xxd's option loop does: an option is known by its first two
/// characters, so `-ps`, `-cols` and `-au` are `-p`, `-c` and `-a`, and `--name` is
/// `-name`.
#[expect(clippy::too_many_lines, reason = "one arm per option, as xxd has them")]
fn parse(args: &[String]) -> Result<Options, Early> {
    let mut o = Options {
        octets_per_group: -1,
        relative_seek: true,
        length: -1,
        ..Options::default()
    };
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        let pp = if arg.len() > 2 && arg.starts_with("--") {
            arg.get(1..).unwrap_or(arg)
        } else {
            arg
        };
        let key = pp.get(..2).unwrap_or(pp);
        let rest = pp.get(2..).unwrap_or("");
        // The option's value: attached unless `attached` says the text is the option's
        // long name, else the next word.
        let value = |i: &mut usize, attached: bool| -> Result<String, Early> {
            if attached {
                return Ok(rest.to_owned());
            }
            *i += 1;
            args.get(*i).cloned().ok_or(Early::Usage)
        };
        match key {
            "-a" => o.autoskip = !o.autoskip,
            "-b" => o.hextype |= HEX_BITS,
            "-e" => o.hextype |= HEX_LITTLEENDIAN,
            "-u" => o.upper = true,
            "-p" => o.hextype |= HEX_POSTSCRIPT,
            "-i" => o.hextype |= HEX_CINCLUDE,
            "-C" => o.capitalize = true,
            "-d" => o.decimal = true,
            "-r" => o.revert = true,
            "-E" => o.ebcdic = true,
            "-t" => o.termination = true,
            "-v" => return Err(Early::Version),
            "-c" => {
                if rest.starts_with("apitalize") {
                    o.capitalize = true;
                } else {
                    let text = value(&mut i, !rest.is_empty() && !rest.starts_with("ols"))?;
                    o.cols_given = true;
                    o.cols = strtol(&text);
                }
            }
            "-g" => {
                let text = value(&mut i, !rest.is_empty() && !rest.starts_with("roup"))?;
                o.octets_per_group = strtol(&text);
            }
            "-o" => {
                let text = value(&mut i, !rest.is_empty() && !rest.starts_with("ffset"))?;
                let digits = text.strip_prefix('+').unwrap_or(&text);
                o.display_offset = match digits.strip_prefix('-') {
                    Some(digits) => strtoul(digits).wrapping_neg(),
                    None => strtoul(digits),
                };
            }
            "-s" => {
                let attached =
                    !rest.is_empty() && !rest.starts_with("kip") && !rest.starts_with("eek");
                let text = value(&mut i, attached)?;
                o.relative_seek = false;
                o.negative_seek = false;
                let digits = text.strip_prefix('+').map_or(text.as_str(), |digits| {
                    o.relative_seek = true;
                    digits
                });
                let digits = digits.strip_prefix('-').map_or(digits, |digits| {
                    o.negative_seek = true;
                    digits
                });
                o.seek = strtol(digits);
            }
            "-l" => {
                let text = value(&mut i, !rest.is_empty() && !rest.starts_with("en"))?;
                o.length = strtol(&text);
            }
            "-n" => {
                o.varname = Some(value(&mut i, !rest.is_empty() && !rest.starts_with("ame"))?);
            }
            "-R" => {
                let when = value(&mut i, !rest.is_empty())?;
                o.colour = if when.starts_with("always") {
                    Colour::Always
                } else if when.starts_with("never") {
                    Colour::Never
                } else if when.starts_with("auto") {
                    Colour::Auto
                } else {
                    return Err(Early::Usage);
                };
            }
            _ if arg == "--" => {
                i += 1;
                break;
            }
            _ if pp.starts_with('-') && pp.len() > 1 => return Err(Early::Usage),
            _ => break,
        }
        i += 1;
    }
    let files = args.get(i..).unwrap_or_default();
    if files.len() > 2 {
        return Err(Early::Usage);
    }
    o.infile = files.first().cloned();
    o.outfile = files.get(1).cloned();
    settle(&mut o)?;
    Ok(o)
}

/// The defaults and checks xxd applies once the options are read.
fn settle(o: &mut Options) -> Result<(), Early> {
    let hextype = o.hextype;
    if hextype != (HEX_CINCLUDE | HEX_BITS) && hextype & hextype.wrapping_sub(1) != 0 {
        return Err(Failure::new(1, "only one of -b, -e, -u, -p, -i can be used").into());
    }
    if o.ebcdic {
        return Err(Failure::new(1, "-E (EBCDIC) is not supported").into());
    }
    if !o.cols_given || (o.cols == 0 && hextype != HEX_POSTSCRIPT) {
        o.cols = match hextype {
            HEX_POSTSCRIPT => 30,
            HEX_CINCLUDE => 12,
            HEX_BITS => 6,
            t if t == (HEX_CINCLUDE | HEX_BITS) => 6,
            _ => 16,
        };
    }
    if o.octets_per_group < 0 {
        o.octets_per_group = match hextype {
            HEX_BITS => 1,
            t if t == (HEX_CINCLUDE | HEX_BITS) => 1,
            HEX_NORMAL => 2,
            HEX_LITTLEENDIAN => 4,
            _ => 0,
        };
    }
    let plain_columns = matches!(hextype, HEX_NORMAL | HEX_BITS | HEX_LITTLEENDIAN);
    if (hextype == HEX_POSTSCRIPT && o.cols < 0)
        || (hextype != HEX_POSTSCRIPT && o.cols < 1)
        || (plain_columns && o.cols > MAX_COLS)
    {
        return Err(
            Failure::new(1, format!("invalid number of columns (max. {MAX_COLS}).")).into(),
        );
    }
    if o.octets_per_group < 1 || o.octets_per_group > o.cols {
        o.octets_per_group = o.cols;
    } else if hextype == HEX_LITTLEENDIAN && o.octets_per_group & (o.octets_per_group - 1) != 0 {
        return Err(Failure::new(
            1,
            "number of octets per group must be a power of 2 with -e.",
        )
        .into());
    }
    Ok(())
}

/// The colour class of the byte `e`: printable green, line ends yellow, nul white, 0xff
/// blue, the rest red.
const fn colour_of(e: u8) -> u8 {
    match e {
        0x20..=0x7e => COLOR_GREEN,
        9 | 10 | 13 => COLOR_YELLOW,
        0 => COLOR_WHITE,
        0xff => COLOR_BLUE,
        _ => COLOR_RED,
    }
}

/// One output line being laid out: characters at computed positions, each with a colour
/// class (0 for none), as xxd fills its line buffer and colour array side by side.
#[derive(Clone, Default)]
struct Canvas {
    chars: Vec<u8>,
    colours: Vec<u8>,
    /// Where the line ends: one past its last text-column character.
    end: usize,
}

impl Canvas {
    fn start(address: &[u8]) -> Self {
        Self {
            chars: address.to_vec(),
            colours: vec![0; address.len()],
            end: 0,
        }
    }

    /// Puts `ch` at column `at`, padding with spaces up to it.
    fn put(&mut self, at: usize, ch: u8, colour: u8) {
        if at >= self.chars.len() {
            self.chars.resize(at + 1, b' ');
            self.colours.resize(at + 1, 0);
        }
        self.chars[at] = ch;
        self.colours[at] = colour;
    }

    /// Forgets the colours, keeping the text, as xxd clears its colour array after a line.
    fn clear_colours(&mut self) {
        self.colours.iter_mut().for_each(|c| *c = 0);
    }

    /// The line and its newline, with an SGR sequence round each run of one colour.
    fn render(&self, coloured: bool, out: &mut Vec<u8>) {
        let mut current = 0;
        for (i, &ch) in self.chars.iter().take(self.end).enumerate() {
            let colour = if coloured { self.colours[i] } else { 0 };
            if colour != current {
                if current != 0 {
                    out.extend_from_slice(b"\x1b[0m");
                }
                if colour != 0 {
                    out.extend_from_slice(&[0x1b, b'[', b'1', b';', b'3', colour, b'm']);
                }
                current = colour;
            }
            out.push(ch);
        }
        if current != 0 {
            out.extend_from_slice(b"\x1b[0m");
        }
        out.push(b'\n');
    }
}

/// xxd's `xxdline`: the autoskip state. A line of zeros is printed when it is the first
/// of a run; the second is held back and printed when the run ends at two, and a longer
/// run prints `*` for everything after the first.
#[derive(Default)]
struct Skipper {
    zero_seen: i8,
    held: Canvas,
}

impl Skipper {
    /// `nz` is positive for a line to print, 0 for a line of zeros, negative to flush at
    /// the end of the input.
    fn line(&mut self, line: &Canvas, nz: i32, coloured: bool, out: &mut Vec<u8>) {
        if nz == 0 && self.zero_seen == 1 {
            self.held = line.clone();
        }
        let first_zero = nz == 0 && {
            let seen = self.zero_seen;
            self.zero_seen += 1;
            seen == 0
        };
        if nz != 0 || first_zero {
            if nz != 0 {
                if nz < 0 {
                    self.zero_seen -= 1;
                }
                if self.zero_seen == 2 {
                    self.held.render(coloured, out);
                }
                if self.zero_seen > 2 {
                    out.extend_from_slice(b"*\n");
                }
            }
            if nz >= 0 || self.zero_seen > 0 {
                line.render(coloured, out);
            }
            if nz != 0 {
                self.zero_seen = 0;
            }
        }
        if self.zero_seen == i8::MAX {
            self.zero_seen = 4;
        }
    }
}

/// What a dump needs to know, once the input is in hand.
struct Dump<'a> {
    options: &'a Options,
    /// Added to each displayed position, with the display offset: where the input started.
    seek: u64,
    coloured: bool,
}

impl Dump<'_> {
    const fn hex_digits(&self) -> &'static [u8; 16] {
        if self.options.upper {
            b"0123456789ABCDEF"
        } else {
            b"0123456789abcdef"
        }
    }

    /// The hex, bits or little-endian dump of `data`.
    fn lines(&self, data: &[u8], out: &mut Vec<u8>) {
        let o = self.options;
        let bits = o.hextype == HEX_BITS;
        let little_endian = o.hextype == HEX_LITTLEENDIAN;
        let cols = usize::try_from(o.cols).unwrap_or(16);
        let group = usize::try_from(o.octets_per_group).unwrap_or(cols).max(1);
        let group_len = if bits { 8 * group + 1 } else { 2 * group + 1 };
        let hexx = self.hex_digits();
        let mut skipper = Skipper::default();
        let mut line = Canvas::default();
        let mut address_len = 0;
        let mut p = 0;
        let mut nonzero = false;
        for (n, &e) in data.iter().enumerate() {
            if p == 0 {
                let position = u64::try_from(n)
                    .unwrap_or(u64::MAX)
                    .wrapping_add(self.seek)
                    .wrapping_add(o.display_offset);
                let address = if o.decimal {
                    format!("{position:08}:")
                } else {
                    format!("{position:08x}:")
                };
                address_len = address.len();
                line = Canvas::start(address.as_bytes());
            }
            let x = if little_endian { p ^ (group - 1) } else { p };
            let mut c = address_len + 1 + (group_len * x) / group;
            let colour = if self.coloured { colour_of(e) } else { 0 };
            if bits {
                for i in (0..8).rev() {
                    line.put(c, if e & (1 << i) == 0 { b'0' } else { b'1' }, colour);
                    c += 1;
                }
            } else {
                line.put(c, hexx[usize::from(e >> 4)], colour);
                line.put(c + 1, hexx[usize::from(e & 0xf)], colour);
            }
            if e != 0 {
                nonzero = true;
            }
            let mut c = if little_endian {
                // The last group is written whole, so round up.
                group_len * cols.div_ceil(group) - 1
            } else {
                (group_len * cols - 1) / group
            };
            c += address_len + 3 + p;
            let shown = if (0x20..0x7f).contains(&e) { e } else { b'.' };
            line.put(c, shown, colour);
            line.end = c + 1;
            p += 1;
            if p == cols {
                let nz = if o.autoskip { i32::from(nonzero) } else { 1 };
                skipper.line(&line, nz, self.coloured, out);
                line.clear_colours();
                nonzero = false;
                p = 0;
            }
        }
        if p > 0 {
            if self.coloured {
                let layout = Layout {
                    cols,
                    group,
                    group_len,
                    address_len,
                    little_endian,
                    bits,
                };
                layout.colour_padding(&mut line, p);
            }
            skipper.line(&line, 1, self.coloured, out);
        } else if o.autoskip {
            // The last chance to flush held-back lines.
            skipper.line(&line, -1, self.coloured, out);
        }
    }

    /// The plain hex dump of `-p`: `cols` digits pairs per line, one line when 0.
    fn postscript(&self, data: &[u8], out: &mut Vec<u8>) {
        let cols = usize::try_from(self.options.cols).unwrap_or(0);
        let hexx = self.hex_digits();
        let mut p = cols;
        for &e in data {
            out.push(hexx[usize::from(e >> 4)]);
            out.push(hexx[usize::from(e & 0xf)]);
            if cols > 0 {
                p -= 1;
                if p == 0 {
                    out.push(b'\n');
                    p = cols;
                }
            }
        }
        if cols == 0 || p < cols {
            out.push(b'\n');
        }
    }

    /// The C array of `-i`. `varname` is `None` for standard input without `-n`, when
    /// xxd writes the bytes alone; `terminate` adds the `-t` zero.
    fn c_include(&self, data: &[u8], varname: Option<&str>, terminate: bool, out: &mut Vec<u8>) {
        let o = self.options;
        let cols = usize::try_from(o.cols).unwrap_or(12).max(1);
        let identifier = |name: &str| -> Vec<u8> {
            let mut text = Vec::with_capacity(name.len() + 2);
            if name.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                text.extend_from_slice(b"__");
            }
            for &b in name.as_bytes() {
                text.push(match b {
                    _ if !b.is_ascii_alphanumeric() => b'_',
                    _ if o.capitalize => b.to_ascii_uppercase(),
                    _ => b,
                });
            }
            text
        };
        if let Some(name) = varname {
            out.extend_from_slice(b"unsigned char ");
            out.extend(identifier(name));
            out.extend_from_slice(b"[] = {\n");
        }
        let bits = o.hextype & HEX_BITS != 0;
        let element = |c: u8, p: usize, out: &mut Vec<u8>| {
            out.extend_from_slice(if p == 0 {
                b"  "
            } else if p.is_multiple_of(cols) {
                b",\n  "
            } else {
                b", "
            });
            if bits {
                out.extend_from_slice(b"0b");
                out.extend(
                    (0..8)
                        .rev()
                        .map(|i| if c & (1 << i) == 0 { b'0' } else { b'1' }),
                );
            } else if o.upper {
                out.extend_from_slice(format!("0X{c:02X}").as_bytes());
            } else {
                out.extend_from_slice(format!("0x{c:02x}").as_bytes());
            }
        };
        let mut p = 0;
        for &c in data {
            element(c, p, out);
            p += 1;
        }
        if terminate {
            element(0, p, out);
        }
        if p > 0 {
            out.push(b'\n');
        }
        if let Some(name) = varname {
            out.extend_from_slice(b"};\nunsigned int ");
            out.extend(identifier(name));
            let suffix = if o.capitalize { "_LEN" } else { "_len" };
            out.extend_from_slice(format!("{suffix} = {p};\n").as_bytes());
        }
    }
}

/// The column arithmetic of a hex, bits or little-endian line.
struct Layout {
    cols: usize,
    group: usize,
    group_len: usize,
    address_len: usize,
    little_endian: bool,
    bits: bool,
}

impl Layout {
    /// Colours the padding of a partial line red where xxd does: the empty end of a
    /// little-endian group, and a run of spaces for the missing bytes, placed by xxd's own
    /// (odd) arithmetic. Writes stay inside the line.
    fn colour_padding(&self, line: &mut Canvas, written: usize) {
        let put = |at: i64, line: &mut Canvas| {
            if let Ok(at) = usize::try_from(at)
                && at < line.end
            {
                line.put(at, b' ', COLOR_RED);
            }
        };
        let group = self.group;
        let mut x = written;
        let mut p = written;
        if self.little_endian {
            let mut fill = group - (p % group);
            if fill == group {
                fill = 0;
            }
            let c = self.address_len + 1 + (self.group_len * (x - (group - fill))) / group;
            for at in c..c + fill {
                put(i64::try_from(at).unwrap_or(i64::MAX), line);
            }
            x += fill;
            p += fill;
        }
        if !self.bits {
            let as_i64 = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
            let missing = as_i64(self.cols) - as_i64(p);
            let c = as_i64(self.address_len + 1 + (self.group_len * x) / group)
                + missing
                + missing / as_i64(group);
            for at in c..c + missing.max(0) {
                put(at, line);
            }
        }
    }
}

/// The hex value of an ASCII digit, or -1.
fn hex_value(c: u8) -> i32 {
    match c {
        b'0'..=b'9' => i32::from(c - b'0'),
        b'a'..=b'f' => i32::from(c - b'a') + 10,
        b'A'..=b'F' => i32::from(c - b'A') + 10,
        _ => -1,
    }
}

/// The value of a binary digit, or -1.
const fn bit_value(c: u8) -> i32 {
    match c {
        b'0' => 0,
        b'1' => 1,
        _ => -1,
    }
}

/// Where `-r` writes: a stream that can only move forward (zero-filled), a named output
/// file's contents, seekable anywhere, or standard output redirected to a file, which xxd
/// seeks relative to where it stands, as `fseek` lets it.
enum Sink<'a> {
    Stream(&'a mut Vec<u8>),
    File {
        data: Vec<u8>,
        position: usize,
    },
    Handle {
        file: std::sync::Arc<std::fs::File>,
        pending: Vec<u8>,
    },
}

impl Sink<'_> {
    /// Moves to `target`, as xxd's `fseek` or zero-filling does; `have` tracks the
    /// position xxd knows.
    fn seek(&mut self, target: i128, have: &mut i128) -> Result<(), Failure> {
        let backwards = Failure::new(5, "Sorry, cannot seek backwards.");
        match self {
            Self::Stream(out) => {
                if target < *have {
                    return Err(backwards);
                }
                let gap = usize::try_from(target - *have).unwrap_or(usize::MAX);
                out.resize(out.len() + gap, 0);
            }
            Self::File { position, .. } => {
                // A file seeks anywhere from 0; before it, xxd's `fseek` fails and the
                // zero-filling that follows cannot go backwards either.
                if target < 0 {
                    return Err(backwards);
                }
                *position = usize::try_from(target).unwrap_or(usize::MAX);
            }
            Self::Handle { file, pending } => {
                use std::io::{Seek, Write};
                let mut file = &**file;
                let _ = file.write_all(pending);
                pending.clear();
                let delta = i64::try_from(target - *have).unwrap_or(i64::MAX);
                if file.seek(std::io::SeekFrom::Current(delta)).is_err() {
                    // A pipe, or standard output itself: forward only.
                    if target < *have {
                        return Err(backwards);
                    }
                    let gap = usize::try_from(target - *have).unwrap_or(usize::MAX);
                    pending.resize(gap, 0);
                }
            }
        }
        *have = target;
        Ok(())
    }

    fn put(&mut self, byte: u8) {
        match self {
            Self::Stream(out) => out.push(byte),
            Self::Handle { pending, .. } => pending.push(byte),
            Self::File { data, position } => {
                if *position >= data.len() {
                    data.resize(*position + 1, 0);
                }
                data[*position] = byte;
                *position += 1;
            }
        }
    }

    /// Writes what a handle still holds.
    fn finish(&mut self) -> std::io::Result<()> {
        if let Self::Handle { file, pending } = self {
            use std::io::Write;
            (&**file).write_all(pending)?;
            pending.clear();
        }
        Ok(())
    }
}

/// Skips to the end of the line; the newline, or `None` at the end of the input.
fn skip_to_eol(input: &[u8], i: &mut usize, c: Option<u8>) -> Option<u8> {
    let mut c = c;
    while c != Some(b'\n') && c.is_some() {
        c = input.get(*i).copied();
        *i += 1;
    }
    c
}

/// xxd's `huntype`: reads a hex (or bits, or plain) dump back into `sink`. On a dump
/// line, the digits before the first non-digit are the position, then pairs of hex digits
/// are bytes until `cols` of them, and the rest of the line is ignored; two non-digits
/// in a row after a byte end the line early. A plain dump is hex digits with whitespace
/// ignored. `base` is added to every position.
fn revert(
    input: &[u8],
    cols: usize,
    hextype: u8,
    base: i128,
    sink: &mut Sink<'_>,
) -> Result<(), Failure> {
    let plain = hextype == HEX_POSTSCRIPT;
    let bits = hextype == HEX_BITS;
    let mut ignore_garbage = true;
    let (mut n1, mut n2, mut n3) = (-1i32, 0i32, 0i32);
    let mut p = cols;
    let mut bit = 0u32;
    let mut bit_count = 0;
    let mut have: i128 = 0;
    let mut want: u64 = 0;
    let mut i = 0;
    while let Some(&byte) = input.get(i) {
        i += 1;
        let mut c = Some(byte);
        if byte == b'\r' {
            continue;
        }
        if plain && matches!(byte, b' ' | b'\n' | b'\t') {
            continue;
        }
        if bits {
            n1 = hex_value(byte);
            if n1 == -1 && ignore_garbage {
                continue;
            }
            let b = bit_value(byte);
            if b != -1 {
                bit = (bit << 1) | u32::try_from(b).unwrap_or(0);
                bit_count += 1;
            }
        } else {
            n3 = n2;
            n2 = n1;
            n1 = hex_value(byte);
            if n1 == -1 && ignore_garbage {
                continue;
            }
        }
        ignore_garbage = false;
        if !plain && p >= cols {
            if n1 < 0 {
                p = 0;
                if bits {
                    bit_count = 0;
                }
                continue;
            }
            want = (want << 4) | u64::try_from(n1).unwrap_or(0);
            continue;
        }
        let target = base + i128::from(want.cast_signed());
        if target != have {
            sink.seek(target, &mut have)?;
        }
        if bits {
            if bit_count == 8 {
                sink.put(u8::try_from(bit & 0xff).unwrap_or(0));
                have += 1;
                want = want.wrapping_add(1);
                bit = 0;
                bit_count = 0;
                p += 1;
                if p >= cols {
                    c = skip_to_eol(input, &mut i, c);
                }
            }
        } else if n2 >= 0 && n1 >= 0 {
            sink.put(u8::try_from((n2 << 4) | n1).unwrap_or(0));
            have += 1;
            want = want.wrapping_add(1);
            n1 = -1;
            if !plain {
                p += 1;
                if p >= cols {
                    c = skip_to_eol(input, &mut i, c);
                }
            }
        } else if n1 < 0 && n2 < 0 && n3 < 0 {
            // Garbage after data: the rest of the line is garbage too.
            c = skip_to_eol(input, &mut i, c);
        }
        if c == Some(b'\n') {
            if !plain {
                want = 0;
            }
            p = cols;
            ignore_garbage = true;
        }
    }
    Ok(())
}

/// The part of `input` a dump shows after `-s`, and the position it starts at. Standard
/// input can only be skipped forward.
fn seek_into<'a>(
    input: &'a [u8],
    o: &Options,
    from_stdin: bool,
) -> Result<(&'a [u8], u64), Failure> {
    if o.seek == 0 && !o.negative_seek && o.relative_seek {
        return Ok((input, 0));
    }
    let len = i64::try_from(input.len()).unwrap_or(i64::MAX);
    let position = if from_stdin {
        if o.negative_seek || o.seek > len {
            return Err(Failure::cannot_seek());
        }
        o.seek
    } else if o.negative_seek {
        if o.relative_seek {
            -o.seek
        } else {
            len - o.seek
        }
    } else {
        o.seek
    };
    if position < 0 {
        return Err(Failure::cannot_seek());
    }
    let start = usize::try_from(position)
        .unwrap_or(usize::MAX)
        .min(input.len());
    Ok((&input[start..], position.cast_unsigned()))
}

/// Dumps `input` as the options say, into `out`.
fn dump(
    input: &[u8],
    o: &Options,
    from_stdin: bool,
    coloured: bool,
    out: &mut Vec<u8>,
) -> Result<(), Failure> {
    let (data, seek) = seek_into(input, o, from_stdin)?;
    let data = match usize::try_from(o.length) {
        Ok(limit) => data.get(..limit).unwrap_or(data),
        Err(_) => data,
    };
    let dump = Dump {
        options: o,
        seek,
        coloured,
    };
    if o.hextype & HEX_CINCLUDE != 0 {
        let varname = o
            .varname
            .as_deref()
            .or_else(|| (!from_stdin).then_some(o.infile.as_deref()).flatten());
        let terminate = o.termination
            && (o.length < 0
                || u64::try_from(data.len()).unwrap_or(u64::MAX) < o.length.cast_unsigned());
        dump.c_include(data, varname, terminate, out);
    } else if o.hextype == HEX_POSTSCRIPT {
        dump.postscript(data, out);
    } else {
        dump.lines(data, out);
    }
    Ok(())
}

impl builtins::Command for XxdCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "the input, the output file and the colour are settled in one place"
    )]
    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let failed = |failure: &Failure| -> Result<ExecutionResult, Self::Error> {
            writeln!(context.stderr(), "xxd: {}", failure.message)?;
            Ok(ExecutionResult::new(failure.status))
        };
        let o = match parse(&self.args) {
            Ok(o) => o,
            Err(Early::Usage) => {
                write!(context.stderr(), "{USAGE}")?;
                return Ok(ExecutionResult::general_error());
            }
            Err(Early::Version) => {
                writeln!(context.stderr(), "{VERSION}")?;
                return Ok(ExecutionResult::success());
            }
            Err(Early::Failed(failure)) => return failed(&failure),
        };

        let from_stdin = o.infile.as_deref().is_none_or(|name| name == "-");
        let mut input = Vec::new();
        if from_stdin {
            context.stdin().read_to_end(&mut input)?;
        } else if let Some(name) = &o.infile {
            let path = context.shell.absolute_path(std::path::Path::new(name));
            match std::fs::read(&path) {
                Ok(bytes) => input = bytes,
                Err(error) => {
                    let reason = cash_core::error::os_error_text(&error);
                    return failed(&Failure::new(2, format!("{name}: {reason}")));
                }
            }
        }
        let outfile = o
            .outfile
            .as_deref()
            .filter(|name| *name != "-")
            .map(|name| {
                (
                    name,
                    context.shell.absolute_path(std::path::Path::new(name)),
                )
            });

        let stdout_is_terminal = context
            .try_fd(OpenFiles::STDOUT_FD)
            .is_some_and(|f| f.is_terminal());
        let no_colour = context
            .shell
            .env()
            .get_str("NO_COLOR", context.shell)
            .is_some_and(|value| !value.is_empty());
        let coloured = match o.colour {
            Colour::Always => true,
            Colour::Never => false,
            Colour::Auto => stdout_is_terminal && outfile.is_none(),
            Colour::Unasked => stdout_is_terminal && !no_colour && outfile.is_none(),
        };

        let mut out = Vec::new();
        let result = if o.revert {
            match o.hextype {
                HEX_NORMAL | HEX_POSTSCRIPT | HEX_BITS => {
                    let base = if o.negative_seek {
                        -i128::from(o.seek)
                    } else {
                        i128::from(o.seek)
                    };
                    let cols = usize::try_from(o.cols).unwrap_or(16);
                    if let Some((name, path)) = &outfile {
                        let data = match std::fs::read(path) {
                            Ok(data) => data,
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                Vec::new()
                            }
                            Err(error) => {
                                let reason = cash_core::error::os_error_text(&error);
                                return failed(&Failure::new(3, format!("{name}: {reason}")));
                            }
                        };
                        let mut sink = Sink::File { data, position: 0 };
                        let result = revert(&input, cols, o.hextype, base, &mut sink);
                        if let Sink::File { data, .. } = sink
                            && let Err(error) = std::fs::write(path, data)
                        {
                            let reason = cash_core::error::os_error_text(&error);
                            return failed(&Failure::new(3, format!("{name}: {reason}")));
                        }
                        result
                    } else {
                        // Standard output redirected to a file is seeked in place, as
                        // xxd's `fseek` does; anything else is filled forward.
                        let mut sink = match context.try_fd(OpenFiles::STDOUT_FD) {
                            Some(cash_core::openfiles::OpenFile::File(file)) => Sink::Handle {
                                file,
                                pending: Vec::new(),
                            },
                            _ => Sink::Stream(&mut out),
                        };
                        let result = revert(&input, cols, o.hextype, base, &mut sink);
                        sink.finish()?;
                        result
                    }
                }
                _ => Err(Failure::new(
                    255,
                    "Sorry, cannot revert this type of hexdump",
                )),
            }
        } else {
            dump(&input, &o, from_stdin, coloured, &mut out)
        };

        // xxd's standard output is buffered, so into a pipe an error comes before the
        // output that preceded it.
        let status = match result {
            Ok(()) => ExecutionResult::success(),
            Err(failure) => {
                writeln!(context.stderr(), "xxd: {}", failure.message)?;
                ExecutionResult::new(failure.status)
            }
        };
        match &outfile {
            Some((name, path)) if !o.revert => {
                if let Err(error) = std::fs::write(path, &out) {
                    let reason = cash_core::error::os_error_text(&error);
                    return failed(&Failure::new(3, format!("{name}: {reason}")));
                }
            }
            _ => context.stdout().write_all(&out)?,
        }
        Ok(status)
    }
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::unwrap_used,
    reason = "a failed assumption in a test should abort it loudly"
)]
mod tests {
    use super::{
        Colour, Dump, HEX_BITS, HEX_LITTLEENDIAN, HEX_NORMAL, HEX_POSTSCRIPT, Options, Sink, dump,
        parse, revert, strtol, strtoul,
    };

    fn options(args: &[&str]) -> Options {
        parse(&args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>())
            .ok()
            .unwrap_or_else(|| panic!("options {args:?} did not parse"))
    }

    fn dumped(args: &[&str], input: &[u8]) -> String {
        let o = options(args);
        let mut out = Vec::new();
        dump(input, &o, true, o.colour == Colour::Always, &mut out).ok();
        String::from_utf8(out).unwrap_or_default()
    }

    #[test]
    fn numbers_read_as_strtol_does() {
        assert_eq!(strtol("16"), 16);
        assert_eq!(strtol("0x10"), 16);
        assert_eq!(strtol("010"), 8);
        assert_eq!(strtol("4x"), 4);
        assert_eq!(strtol("abc"), 0);
        assert_eq!(strtol("-3"), -3);
        assert_eq!(strtoul("-5"), u64::MAX - 4);
    }

    #[test]
    fn a_hex_line_has_xxds_spacing() {
        assert_eq!(
            dumped(&[], b"hello\n"),
            "00000000: 6865 6c6c 6f0a                           hello.\n"
        );
        assert_eq!(
            dumped(&["-c", "4"], b"hello"),
            "00000000: 6865 6c6c  hell\n00000004: 6f         o\n"
        );
        assert_eq!(
            dumped(&["-g", "3", "-u"], b"hello world!!"),
            "00000000: 68656C 6C6F20 776F72 6C6421 21         hello world!!\n"
        );
        assert_eq!(
            dumped(&["-d", "-o", "10", "-c", "2"], b"ab"),
            "00000010: 6162  ab\n"
        );
    }

    #[test]
    fn bits_and_little_endian_lines() {
        assert_eq!(
            dumped(&["-b", "-c", "3"], b"hell"),
            format!(
                "00000000: 01101000 01100101 01101100  hel\n00000003: 01101100{}l\n",
                " ".repeat(20)
            )
        );
        assert_eq!(
            dumped(&["-e"], b"hello world!!!!!!x"),
            "00000000: 6c6c6568 6f77206f 21646c72 21212121  hello world!!!!!\n\
             00000010:     7821                             !x\n"
        );
        assert_eq!(
            dumped(&["-e", "-c", "5"], b"hello"),
            "00000000: 6c6c6568       6f  hello\n"
        );
    }

    #[test]
    fn autoskip_folds_runs_of_zero_lines() {
        let mut input = b"ab".to_vec();
        input.extend([0; 46]);
        input.extend(b"cd");
        assert_eq!(
            dumped(&["-a"], &input),
            "00000000: 6162 0000 0000 0000 0000 0000 0000 0000  ab..............\n\
             00000010: 0000 0000 0000 0000 0000 0000 0000 0000  ................\n\
             00000020: 0000 0000 0000 0000 0000 0000 0000 0000  ................\n\
             00000030: 6364                                     cd\n"
        );
        assert_eq!(
            dumped(&["-a"], &[0; 64]),
            "00000000: 0000 0000 0000 0000 0000 0000 0000 0000  ................\n\
             *\n\
             00000030: 0000 0000 0000 0000 0000 0000 0000 0000  ................\n"
        );
    }

    #[test]
    fn plain_and_c_include() {
        assert_eq!(dumped(&["-p", "-c", "3"], b"hello"), "68656c\n6c6f\n");
        assert_eq!(dumped(&["-i"], b"hi"), "  0x68, 0x69\n");
        assert_eq!(
            dumped(&["-i", "-C", "-t", "-n", "my-name"], b"hi"),
            "unsigned char MY_NAME[] = {\n  0x68, 0x69, 0x00\n};\nunsigned int MY_NAME_LEN = 2;\n"
        );
        assert_eq!(
            dumped(&["-i", "-n", "q"], b""),
            "unsigned char q[] = {\n};\nunsigned int q_len = 0;\n"
        );
    }

    #[test]
    fn colour_wraps_each_run_of_one_class() {
        assert_eq!(
            dumped(&["-R", "always", "-c", "4"], b"a\0\xff\n"),
            "00000000: \x1b[1;32m61\x1b[0m\x1b[1;37m00\x1b[0m \x1b[1;34mff\x1b[0m\x1b[1;33m0a\x1b[0m  \
             \x1b[1;32ma\x1b[0m\x1b[1;37m.\x1b[0m\x1b[1;34m.\x1b[0m\x1b[1;33m.\x1b[0m\n"
        );
    }

    fn reverted(args: &[&str], input: &[u8]) -> Result<Vec<u8>, String> {
        let o = options(args);
        let mut out = Vec::new();
        let base = if o.negative_seek {
            -i128::from(o.seek)
        } else {
            i128::from(o.seek)
        };
        let cols = usize::try_from(o.cols).unwrap_or(16);
        match revert(input, cols, o.hextype, base, &mut Sink::Stream(&mut out)) {
            Ok(()) => Ok(out),
            Err(failure) => Err(failure.message),
        }
    }

    #[test]
    fn revert_reads_dumps_back() {
        let dumped = dumped(&[], b"hello\n");
        assert_eq!(
            reverted(&["-r"], dumped.as_bytes()),
            Ok(b"hello\n".to_vec())
        );
        assert_eq!(
            reverted(&["-r"], b"00000000: 6869\n00000004: 6a6b\n"),
            Ok(b"hi\0\0jk".to_vec())
        );
        assert_eq!(
            reverted(&["-r", "-s", "2"], b"00000000: 6869\n"),
            Ok(b"\0\0hi".to_vec())
        );
        assert_eq!(
            reverted(&["-r"], b"00000004: 6869\n00000000: 6a\n"),
            Err("Sorry, cannot seek backwards.".to_owned())
        );
        assert_eq!(
            reverted(&["-r", "-c", "2"], b"00000000: 68696a6b\n"),
            Ok(b"hi".to_vec())
        );
        assert_eq!(
            reverted(&["-r"], b"00000000: 686 96a\n"),
            Ok(b"h\x96".to_vec())
        );
        assert_eq!(reverted(&["-r", "-p"], b"68 6g 69\n"), Ok(b"hi".to_vec()));
        assert_eq!(
            reverted(&["-r", "-b"], b"00000000: 01101000 01101001  hi\n"),
            Ok(b"hi".to_vec())
        );
    }

    #[test]
    fn revert_into_a_file_seeks_and_patches() {
        let mut sink = Sink::File {
            data: b"ABCDEFGH".to_vec(),
            position: 0,
        };
        assert!(
            revert(
                b"00000004: 7878\n00000000: 6a\n",
                16,
                HEX_NORMAL,
                0,
                &mut sink
            )
            .is_ok()
        );
        let Sink::File { data, .. } = sink else {
            panic!("the sink is a file")
        };
        assert_eq!(data, b"jBCDxxGH");
    }

    #[test]
    fn options_settle_as_xxds_do() {
        assert_eq!(options(&["-ps"]).hextype, HEX_POSTSCRIPT);
        assert_eq!(options(&["-ps"]).cols, 30);
        assert_eq!(options(&["-b"]).octets_per_group, 1);
        assert_eq!(options(&["-e"]).octets_per_group, 4);
        assert_eq!(options(&["-g", "0"]).octets_per_group, 16);
        assert_eq!(options(&["-c4", "-s", "+-3"]).cols, 4);
        assert!(options(&["-s", "+-3"]).negative_seek && options(&["-s", "+-3"]).relative_seek);
        assert_eq!(options(&["-o", "-5"]).display_offset, u64::MAX - 4);
        assert!(parse(&["-e".to_owned(), "-g".to_owned(), "3".to_owned()]).is_err());
        assert!(parse(&["-c".to_owned(), "300".to_owned()]).is_err());
        assert!(parse(&["-b".to_owned(), "-e".to_owned()]).is_err());
        let _ = HEX_BITS | HEX_LITTLEENDIAN;
        let dump = Dump {
            options: &options(&["-u"]),
            seek: 0,
            coloured: false,
        };
        assert_eq!(dump.hex_digits(), b"0123456789ABCDEF");
    }
}
