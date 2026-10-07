//! `hexdump`, util-linux's: bytes shown through a format language.
//!
//! Checked against util-linux 2.42.3 (`crates/cash/tests/oracle`). The formats of `-b`,
//! `-c`, `-C`, `-d`, `-o`, `-x` and `-X` are the format strings util-linux builds them
//! from, run through the same engine as `-e` and `-f`: a format string is units of
//! `[count][/bytes] "text"`, the text a printf format whose conversions each take their
//! byte count (`%d` four bytes, `%c` one, `%s` the unit's or its precision), with `%_a`
//! and `%_A` for the position, `%_c`, `%_p` and `%_u` for one byte as C escape, printable
//! or named control character. The block is as long as the longest format string; a last
//! unit without a count is repeated to fill it, and loses one trailing space on its last
//! turn; past the end of the input a conversion prints blanks of its width; and a block
//! equal to the one before is printed as `*` once unless `-v`. The conversions are printed
//! by a C `printf` of our own, flags, width and precision included, `%e`/`%f`/`%g` with
//! C's exponent and `%g` trimming. Even util-linux's quirks are kept where a format could
//! meet them: a `\` escape leaves the characters after it in place rather than shifting
//! them, and `%%` is not a conversion.
//!
//! Deliberate differences: `-s` on standard input skips the bytes (util-linux cannot seek
//! a pipe and stops with "Illegal seek"); a file that cannot be read, a directory among
//! them, gives status 1; and colour (`-L`, `_L[...]` in a format) is parsed but never
//! printed: `-L=always` is refused.

use std::io::{Read, Write};

use cash_core::{ExecutionResult, builtins};
use cash_getopt::{Arg, Getopt, Item, Long, Short};
use clap::Parser;

/// Display file contents in hexadecimal, decimal, octal, or ascii.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct HexdumpCommand {
    /// Options and files, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

const HINT: &str = "Try 'hexdump --help' for more information.";

const USAGE: &str = "
Usage:
 hexdump [options] <file>...

Display file contents in hexadecimal, decimal, octal, or ascii.

Options:
 -b, --one-byte-octal      one-byte octal display
 -X, --one-byte-hex        one-byte hexadecimal display
 -c, --one-byte-char       one-byte character display
 -C, --canonical           canonical hex+ASCII display
 -d, --two-bytes-decimal   two-byte decimal display
 -o, --two-bytes-octal     two-byte octal display
 -x, --two-bytes-hex       two-byte hexadecimal display
 -L, --color[=<mode>]      interpret color formatting specifiers
                             only 'never' and 'auto' are accepted
 -e, --format <format>     format string to be used for displaying data
 -f, --format-file <file>  file that contains format strings
 -n, --length <length>     interpret only length bytes of input
 -s, --skip <offset>       skip offset bytes from the beginning
 -v, --no-squeezing        output identical lines

 -h, --help                display this help
 -V, --version             display version

Arguments:
 Values for <length> and <offset> may be followed by a suffix: KiB, MiB,
 GiB, TiB, PiB, EiB, ZiB, or YiB (where the \"iB\" is optional).

For more details see hexdump(1).
";

const VERSION: &str = "hexdump (cash): util-linux 2.42.3's options";

/// The characters a conversion may have between `%` and its letter when the unit has a
/// byte count; without one, `.` starts the precision instead.
const SPEC_CHARS: &[u8] = b".+-# 0123456789";

/// An error reported as `hexdump: <message>`, status 1.
#[derive(Debug)]
struct Failure(String);

impl Failure {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    fn bad_format(fmt: &[u8]) -> Self {
        let text = String::from_utf8_lossy(fmt);
        Self(format!("bad format {{{text}}}"))
    }

    fn bad_conversion(conversion: &[u8]) -> Self {
        let text = String::from_utf8_lossy(conversion);
        Self(format!("bad conversion character %{text}"))
    }

    fn bad_count(conversion: &str) -> Self {
        Self(format!(
            "bad byte count for conversion character {conversion}"
        ))
    }
}

/// Flags, width and precision between `%` and the conversion character.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Spec {
    minus: bool,
    plus: bool,
    space: bool,
    hash: bool,
    zero: bool,
    width: usize,
    precision: Option<usize>,
}

impl Spec {
    /// The flags, width and precision in `text`, as C's `printf` reads them.
    fn parse(text: &[u8]) -> Self {
        let mut spec = Self::default();
        let mut i = 0;
        while let Some(&b) = text.get(i) {
            match b {
                b'-' => spec.minus = true,
                b'+' => spec.plus = true,
                b' ' => spec.space = true,
                b'#' => spec.hash = true,
                b'0' => spec.zero = true,
                _ => break,
            }
            i += 1;
        }
        let digits = text.get(i..).unwrap_or_default();
        let width_len = digits.iter().take_while(|b| b.is_ascii_digit()).count();
        spec.width = atoi(digits.get(..width_len).unwrap_or_default());
        i += width_len;
        if text.get(i) == Some(&b'.') {
            let digits = text.get(i + 1..).unwrap_or_default();
            let len = digits.iter().take_while(|b| b.is_ascii_digit()).count();
            spec.precision = Some(atoi(digits.get(..len).unwrap_or_default()));
        }
        spec
    }

    /// `body`, with `sign` and `prefix` before it, padded to the width: spaces on the
    /// left, or on the right with `-`, or zeros after the sign with `0` when `zeros` says
    /// the conversion allows it.
    fn pad(self, sign: &[u8], prefix: &[u8], body: &[u8], zeros: bool) -> Vec<u8> {
        let len = sign.len() + prefix.len() + body.len();
        let fill = self.width.saturating_sub(len);
        let mut out = Vec::with_capacity(len + fill);
        if self.minus {
            out.extend_from_slice(sign);
            out.extend_from_slice(prefix);
            out.extend_from_slice(body);
            out.resize(out.len() + fill, b' ');
        } else if zeros && self.zero {
            out.extend_from_slice(sign);
            out.extend_from_slice(prefix);
            out.resize(out.len() + fill, b'0');
            out.extend_from_slice(body);
        } else {
            out.resize(fill, b' ');
            out.extend_from_slice(sign);
            out.extend_from_slice(prefix);
            out.extend_from_slice(body);
        }
        out
    }

    /// `%d`, `%i`, `%o`, `%u`, `%x` or `%X` of a value: `negative` is its sign, `magnitude`
    /// its absolute value; `conv` is the conversion letter.
    fn integer(self, negative: bool, magnitude: u128, conv: u8) -> Vec<u8> {
        let radix = match conv {
            b'o' => 8,
            b'x' | b'X' => 16,
            _ => 10,
        };
        let mut digits = Vec::new();
        let mut rest = magnitude;
        while rest > 0 {
            let digit = u8::try_from(rest % radix).unwrap_or(0);
            digits.push(match digit {
                0..=9 => b'0' + digit,
                _ if conv == b'X' => b'A' + digit - 10,
                _ => b'a' + digit - 10,
            });
            rest /= radix;
        }
        if magnitude == 0 && self.precision != Some(0) {
            digits.push(b'0');
        }
        digits.reverse();
        if let Some(precision) = self.precision
            && digits.len() < precision
        {
            let mut padded = vec![b'0'; precision - digits.len()];
            padded.extend(digits);
            digits = padded;
        }
        let prefix: &[u8] = match conv {
            b'x' if self.hash && magnitude != 0 => b"0x",
            b'X' if self.hash && magnitude != 0 => b"0X",
            b'o' if self.hash && !digits.starts_with(b"0") => b"0",
            _ => b"",
        };
        let signed = matches!(conv, b'd' | b'i');
        let sign: &[u8] = if negative {
            b"-"
        } else if signed && self.plus {
            b"+"
        } else if signed && self.space {
            b" "
        } else {
            b""
        };
        self.pad(sign, prefix, &digits, self.precision.is_none())
    }

    /// `%c` of a byte.
    fn character(self, byte: u8) -> Vec<u8> {
        self.pad(b"", b"", &[byte], false)
    }

    /// `%s` of `text`, which the precision shortens.
    fn string(self, text: &[u8]) -> Vec<u8> {
        let shown = match self.precision {
            Some(precision) => text.get(..precision).unwrap_or(text),
            None => text,
        };
        self.pad(b"", b"", shown, false)
    }

    /// `%e`, `%E`, `%f`, `%g` or `%G` of a value, as C prints it.
    fn float(self, value: f64, conv: u8) -> Vec<u8> {
        let negative = value.is_sign_negative();
        let sign: &[u8] = if negative {
            b"-"
        } else if self.plus {
            b"+"
        } else if self.space {
            b" "
        } else {
            b""
        };
        let magnitude = value.abs();
        let body = if value.is_nan() {
            "nan".to_owned()
        } else if value.is_infinite() {
            "inf".to_owned()
        } else {
            match conv.to_ascii_lowercase() {
                b'f' => fixed(magnitude, self.precision.unwrap_or(6), self.hash),
                b'e' => exponential(magnitude, self.precision.unwrap_or(6), self.hash),
                _ => general(magnitude, self.precision, self.hash),
            }
        };
        let body = if conv.is_ascii_uppercase() {
            body.to_ascii_uppercase()
        } else {
            body
        };
        self.pad(sign, b"", body.as_bytes(), value.is_finite())
    }
}

/// `%.<precision>f` of a non-negative finite value.
fn fixed(value: f64, precision: usize, hash: bool) -> String {
    let mut text = format!("{value:.precision$}");
    if hash && precision == 0 {
        text.push('.');
    }
    text
}

/// `%.<precision>e` of a non-negative finite value: C's exponent has a sign and two
/// digits at least.
fn exponential(value: f64, precision: usize, hash: bool) -> String {
    let text = format!("{value:.precision$e}");
    let (mantissa, exponent) = text.split_once('e').unwrap_or((text.as_str(), "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let point = if hash && precision == 0 { "." } else { "" };
    let sign = if exponent < 0 { '-' } else { '+' };
    format!("{mantissa}{point}e{sign}{:02}", exponent.abs())
}

/// `%g` of a non-negative finite value: `%e` or `%f` by the exponent, trailing zeros
/// removed unless `#`.
fn general(value: f64, precision: Option<usize>, hash: bool) -> String {
    let p = match precision {
        None => 6,
        Some(0) => 1,
        Some(p) => p,
    };
    let places = p - 1;
    let probe = format!("{value:.places$e}");
    let exponent: i32 = probe
        .split_once('e')
        .and_then(|(_, e)| e.parse().ok())
        .unwrap_or(0);
    let fits = exponent >= -4 && i64::from(exponent) < i64::try_from(p).unwrap_or(i64::MAX);
    let text = if fits {
        let places =
            usize::try_from(i64::try_from(p).unwrap_or(0) - 1 - i64::from(exponent)).unwrap_or(0);
        fixed(value, places, hash)
    } else {
        exponential(value, p - 1, hash)
    };
    if hash {
        return text;
    }
    let (mantissa, exponent) = text
        .split_once('e')
        .map_or((text.as_str(), None), |(m, e)| (m, Some(e)));
    let trimmed = if mantissa.contains('.') {
        mantissa.trim_end_matches('0').trim_end_matches('.')
    } else {
        mantissa
    };
    match exponent {
        Some(exponent) => format!("{trimmed}e{exponent}"),
        None => trimmed.to_owned(),
    }
}

/// Digits as `atoi` reads them, saturating.
fn atoi(digits: &[u8]) -> usize {
    digits.iter().fold(0usize, |n, &d| {
        n.saturating_mul(10).saturating_add(usize::from(d - b'0'))
    })
}

/// C's `isspace`.
const fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// C's `isprint` in the C locale.
const fn is_print(b: u8) -> bool {
    matches!(b, 0x20..=0x7e)
}

/// What a conversion prints.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// Text alone.
    Text,
    /// `%_a` or `%_A`: the position, in the base of the letter (`d`, `o`, `x`).
    Address(u8),
    /// `%c`.
    Char,
    /// `%d`, `%i`: a signed number.
    Int,
    /// `%o`, `%u`, `%x`, `%X`: an unsigned number.
    Uint,
    /// `%e`, `%E`, `%f`, `%g`, `%G`.
    Double,
    /// `%s`.
    Str,
    /// `%_c`: a byte as a C escape, a printable character or three octal digits.
    Escaped,
    /// `%_p`: a printable character or `.`.
    Printable,
    /// `%_u`: a control character's name, a printable character or two hex digits.
    Named,
    /// A conversion past the end of the input: blanks of its width.
    Blank,
}

/// One conversion and the text before it, as util-linux splits a unit's format.
#[derive(Clone, Debug)]
struct Print {
    kind: Kind,
    text: Vec<u8>,
    spec: Spec,
    /// The conversion letter, for numbers.
    conv: u8,
    /// The bytes it consumes.
    bcnt: usize,
}

/// `[count][/bytes] "text"`.
#[derive(Clone, Debug, Default)]
struct Unit {
    reps: usize,
    reps_given: bool,
    bcnt: usize,
    fmt: Vec<u8>,
    prints: Vec<Print>,
    /// Holds `%_A`: shown at the end only.
    ignore: bool,
    /// Repeated, so its last turn loses its trailing space.
    nospace: bool,
}

/// One `-e` string or `-f` line.
#[derive(Clone, Debug, Default)]
struct Format {
    units: Vec<Unit>,
    bcnt: usize,
}

/// util-linux's `escape`: `\a \b \f \n \r \t \v` become their characters, any other
/// escaped character stands for itself. As there, the characters after an escape are not
/// moved up, so a format with an escape before its conversions keeps its odd shape.
fn escape(fmt: &mut Vec<u8>) {
    let mut p1 = 0;
    let mut p2 = 0;
    while p1 < fmt.len() {
        if fmt[p1] == b'\\' {
            p1 += 1;
            let Some(&next) = fmt.get(p1) else {
                break;
            };
            fmt[p2] = match next {
                b'a' => 0x07,
                b'b' => 0x08,
                b'f' => 0x0c,
                b'n' => b'\n',
                b'r' => b'\r',
                b't' => b'\t',
                b'v' => 0x0b,
                other => other,
            };
        }
        p1 += 1;
        p2 += 1;
    }
    fmt.truncate(p2);
}

/// Skips white space from `i`.
fn skip_space(fmt: &[u8], mut i: usize) -> usize {
    while fmt.get(i).is_some_and(|&b| is_space(b)) {
        i += 1;
    }
    i
}

/// util-linux's `add`: the units of one format string.
fn add(fmt: &[u8], format: &mut Format) -> Result<(), Failure> {
    let mut p = 0;
    loop {
        p = skip_space(fmt, p);
        if p >= fmt.len() {
            return Ok(());
        }
        let mut unit = Unit {
            reps: 1,
            ..Unit::default()
        };
        if fmt[p].is_ascii_digit() {
            let start = p;
            while fmt.get(p).is_some_and(u8::is_ascii_digit) {
                p += 1;
            }
            if !fmt.get(p).is_some_and(|&b| is_space(b) || b == b'/') {
                return Err(Failure::bad_format(fmt));
            }
            unit.reps = atoi(&fmt[start..p]);
            unit.reps_given = true;
            p = skip_space(fmt, p + 1);
        }
        if fmt.get(p) == Some(&b'/') {
            p = skip_space(fmt, p + 1);
        }
        if fmt.get(p).is_some_and(u8::is_ascii_digit) {
            let start = p;
            while fmt.get(p).is_some_and(u8::is_ascii_digit) {
                p += 1;
            }
            if !fmt.get(p).is_some_and(|&b| is_space(b)) {
                return Err(Failure::bad_format(fmt));
            }
            unit.bcnt = atoi(&fmt[start..p]);
            p = skip_space(fmt, p + 1);
        }
        if fmt.get(p) != Some(&b'"') {
            return Err(Failure::bad_format(fmt));
        }
        p += 1;
        let start = p;
        while fmt.get(p) != Some(&b'"') {
            if p >= fmt.len() {
                return Err(Failure::bad_format(fmt));
            }
            p += 1;
        }
        unit.fmt = fmt[start..p].to_vec();
        escape(&mut unit.fmt);
        p += 1;
        format.units.push(unit);
    }
}

/// The index after the flags, width and precision of the conversion at `i` (just past
/// the `%`), by util-linux's two rules, and the precision when the unit has no byte
/// count.
fn skip_spec(fmt: &[u8], mut i: usize, has_bcnt: bool) -> (usize, Option<usize>) {
    if has_bcnt {
        while fmt.get(i).is_some_and(|b| SPEC_CHARS.contains(b)) {
            i += 1;
        }
        return (i, None);
    }
    while fmt
        .get(i)
        .is_some_and(|b| SPEC_CHARS.get(1..).unwrap_or_default().contains(b))
    {
        i += 1;
    }
    let mut precision = None;
    if fmt.get(i) == Some(&b'.') {
        i += 1;
        if fmt.get(i).is_some_and(u8::is_ascii_digit) {
            let start = i;
            while fmt.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            precision = Some(atoi(&fmt[start..i]));
        }
    }
    (i, precision)
}

/// util-linux's `size`: the bytes a format string interprets, before its units are
/// rewritten, from the byte counts given or the conversions' defaults.
fn block_size(format: &Format) -> usize {
    let mut size = 0;
    for unit in &format.units {
        if unit.bcnt != 0 {
            size += unit.bcnt * unit.reps;
            continue;
        }
        let fmt = &unit.fmt;
        let mut bcnt = 0;
        let mut precision = 0;
        let mut i = 0;
        while i < fmt.len() {
            if fmt[i] != b'%' {
                i += 1;
                continue;
            }
            let (at, prec) = skip_spec(fmt, i + 1, false);
            if let Some(prec) = prec {
                precision = prec;
            }
            i = at;
            match fmt.get(i) {
                Some(b'c') => bcnt += 1,
                Some(b'd' | b'i' | b'o' | b'u' | b'x' | b'X') => bcnt += 4,
                Some(b'e' | b'E' | b'f' | b'g' | b'G') => bcnt += 8,
                Some(b's') => bcnt += precision,
                Some(b'_') => {
                    i += 1;
                    if matches!(fmt.get(i), Some(b'c' | b'p' | b'u')) {
                        bcnt += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        size += bcnt * unit.reps;
    }
    size
}

/// The conversion at `fmt[i..]` (just past the `%`) of a unit with byte count `bcnt`:
/// the print, and the index after it.
#[expect(clippy::too_many_lines, reason = "one arm per conversion character")]
fn conversion(
    fmt: &[u8],
    i: usize,
    bcnt: usize,
    text: Vec<u8>,
) -> Result<(Print, usize, bool), Failure> {
    let (at, precision) = skip_spec(fmt, i, bcnt != 0);
    let Some(&letter) = fmt.get(at) else {
        return Err(Failure::bad_conversion(b""));
    };
    let mut spec = Spec::parse(fmt.get(i..at).unwrap_or_default());
    if bcnt == 0
        && let Some(precision) = precision
    {
        spec.precision = Some(precision);
    }
    let mut end = at + 1;
    let mut is_end_address = false;
    let (kind, count) = match letter {
        b'c' => match bcnt {
            0 | 1 => (Kind::Char, 1),
            _ => return Err(Failure::bad_count("c")),
        },
        b'd' | b'i' | b'o' | b'u' | b'x' | b'X' => {
            let kind = if matches!(letter, b'd' | b'i') {
                Kind::Int
            } else {
                Kind::Uint
            };
            match bcnt {
                0 | 4 => (kind, 4),
                1 | 2 | 8 => (kind, bcnt),
                _ => return Err(Failure::bad_count(&char::from(letter).to_string())),
            }
        }
        b'e' | b'E' | b'f' | b'g' | b'G' => match bcnt {
            0 | 8 => (Kind::Double, 8),
            4 => (Kind::Double, 4),
            _ => return Err(Failure::bad_count(&char::from(letter).to_string())),
        },
        b's' => match (bcnt, spec.precision) {
            (0, None) => return Err(Failure::new("%s requires a precision or a byte count")),
            (0, Some(precision)) => (Kind::Str, precision),
            _ => (Kind::Str, bcnt),
        },
        b'_' => {
            let sub = fmt.get(at + 1).copied();
            end = at + 2;
            match sub {
                Some(b'A' | b'a') => {
                    is_end_address = sub == Some(b'A');
                    end = at + 3;
                    match fmt.get(at + 2) {
                        Some(&base @ (b'd' | b'o' | b'x')) => (Kind::Address(base), 0),
                        _ => {
                            return Err(Failure::bad_conversion(
                                fmt.get(at..fmt.len().min(at + 3)).unwrap_or_default(),
                            ));
                        }
                    }
                }
                Some(b'c' | b'p' | b'u') => {
                    let kind = match sub {
                        Some(b'c') => Kind::Escaped,
                        Some(b'p') => Kind::Printable,
                        _ => Kind::Named,
                    };
                    match bcnt {
                        0 | 1 => (kind, 1),
                        _ => {
                            return Err(Failure::bad_count(&format!(
                                "_{}",
                                char::from(sub.unwrap_or(b'?'))
                            )));
                        }
                    }
                }
                _ => {
                    return Err(Failure::bad_conversion(
                        fmt.get(at..fmt.len().min(at + 2)).unwrap_or_default(),
                    ));
                }
            }
        }
        _ => return Err(Failure::bad_conversion(&[letter])),
    };
    // A colour specifier, `_L[...]`, is read and left out: cash's hexdump has no colour.
    if fmt.get(end..end + 2) == Some(b"_L") {
        if fmt.get(end + 2) != Some(&b'[') {
            return Err(Failure::bad_conversion(b"_L"));
        }
        let Some(close) = fmt
            .get(end + 3..)
            .and_then(|rest| rest.iter().position(|&b| b == b']'))
        else {
            return Err(Failure::bad_conversion(b"_L"));
        };
        end += 3 + close + 1;
    }
    Ok((
        Print {
            kind,
            text,
            spec,
            conv: letter,
            bcnt: count,
        },
        end,
        is_end_address,
    ))
}

/// util-linux's `rewrite`: each unit's format split into its prints, the byte counts
/// settled, the last unit repeated to fill the block. Returns whether a `%_A` was met.
fn rewrite(format: &mut Format, blocksize: usize) -> Result<bool, Failure> {
    let mut has_end = false;
    for unit in &mut format.units {
        let fmt = unit.fmt.clone();
        let mut conversions = 0;
        let mut i = 0;
        while i < fmt.len() {
            let Some(percent) = fmt
                .get(i..)
                .and_then(|rest| rest.iter().position(|&b| b == b'%'))
            else {
                unit.prints.push(Print {
                    kind: Kind::Text,
                    text: fmt[i..].to_vec(),
                    spec: Spec::default(),
                    conv: 0,
                    bcnt: 0,
                });
                break;
            };
            let text = fmt[i..i + percent].to_vec();
            let (print, end, is_end_address) = conversion(&fmt, i + percent + 1, unit.bcnt, text)?;
            if is_end_address {
                has_end = true;
                unit.ignore = true;
            }
            if !matches!(print.kind, Kind::Address(_)) && unit.bcnt != 0 {
                conversions += 1;
                if conversions > 1 {
                    return Err(Failure::new(
                        "byte count with multiple conversion characters",
                    ));
                }
            }
            unit.prints.push(print);
            i = end;
        }
        if unit.bcnt == 0 {
            unit.bcnt = unit.prints.iter().map(|print| print.bcnt).sum();
        }
    }
    let count = format.units.len();
    for (index, unit) in format.units.iter_mut().enumerate() {
        if index + 1 == count && format.bcnt < blocksize && !unit.reps_given && unit.bcnt != 0 {
            unit.reps += (blocksize - format.bcnt) / unit.bcnt;
        }
        if unit.reps > 1
            && let Some(last) = unit.prints.last()
            && last.kind == Kind::Text
            && last.text.last().is_some_and(|&b| is_space(b))
        {
            unit.nospace = true;
        }
    }
    Ok(has_end)
}

/// Whether identical blocks are being folded.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Squeeze {
    /// `-v`: every block is printed.
    All,
    /// Before the first block.
    First,
    /// A block was printed; a repeat prints `*`.
    Wait,
    /// A repeat was folded; further repeats print nothing.
    Dup,
}

/// The formats, and the state of a run through the input.
struct Engine {
    formats: Vec<Format>,
    blocksize: usize,
    /// The format and unit of the last `%_A`.
    end_unit: Option<(usize, usize)>,
    squeeze: Squeeze,
    address: u64,
    /// Where the input ended, once a partial block has been seen.
    end_address: Option<u64>,
    out: Vec<u8>,
}

impl Engine {
    fn new(mut formats: Vec<Format>, squeeze: bool, address: u64) -> Result<Self, Failure> {
        let mut blocksize = 0;
        for format in &mut formats {
            format.bcnt = block_size(format);
            blocksize = blocksize.max(format.bcnt);
        }
        let mut end_unit = None;
        for (f, format) in formats.iter_mut().enumerate() {
            if rewrite(format, blocksize)? {
                if let Some(u) = format.units.iter().rposition(|unit| unit.ignore) {
                    end_unit = Some((f, u));
                }
            }
        }
        Ok(Self {
            formats,
            blocksize,
            end_unit,
            squeeze: if squeeze {
                Squeeze::First
            } else {
                Squeeze::All
            },
            address,
            end_address: None,
            out: Vec::new(),
        })
    }

    /// Runs `data` through the formats: util-linux's `get` and `display`.
    fn run(&mut self, data: &[u8]) {
        let mut position = 0;
        let mut previous: Option<&[u8]> = None;
        let mut first = true;
        loop {
            if !first {
                self.address = self.address.wrapping_add(to_u64(self.blocksize));
            }
            first = false;
            let block = loop {
                let remaining = data.len() - position;
                if self.blocksize == 0 || remaining == 0 {
                    break None;
                }
                if remaining < self.blocksize {
                    let mut block = data[position..].to_vec();
                    block.resize(self.blocksize, 0);
                    position = data.len();
                    self.end_address = Some(self.address.wrapping_add(to_u64(remaining)));
                    break Some(block);
                }
                let block = &data[position..position + self.blocksize];
                position += self.blocksize;
                if matches!(self.squeeze, Squeeze::All | Squeeze::First) || previous != Some(block)
                {
                    if matches!(self.squeeze, Squeeze::Dup | Squeeze::First) {
                        self.squeeze = Squeeze::Wait;
                    }
                    previous = Some(block);
                    break Some(block.to_vec());
                }
                if self.squeeze == Squeeze::Wait {
                    self.out.extend_from_slice(b"*\n");
                }
                self.squeeze = Squeeze::Dup;
                self.address = self.address.wrapping_add(to_u64(self.blocksize));
            };
            let Some(block) = block else { break };
            self.display(&block);
        }
    }

    /// Prints one block through every format.
    fn display(&mut self, block: &[u8]) {
        let saved_address = self.address;
        for f in 0..self.formats.len() {
            let mut bp = 0;
            for u in 0..self.formats[f].units.len() {
                if self.formats[f].units[u].ignore {
                    break;
                }
                let reps = self.formats[f].units[u].reps;
                let nospace = self.formats[f].units[u].nospace;
                let prints = self.formats[f].units[u].prints.len();
                for cnt in (1..=reps).rev() {
                    for p in 0..prints {
                        let print = &mut self.formats[f].units[u].prints[p];
                        if self.end_address.is_some_and(|end| self.address >= end)
                            && !matches!(print.kind, Kind::Text | Kind::Blank)
                        {
                            print.kind = Kind::Blank;
                        }
                        let print = print.clone();
                        let cut = cnt == 1 && nospace && p + 1 == prints;
                        self.print(&print, block.get(bp..).unwrap_or_default(), cut);
                        self.address = self.address.wrapping_add(to_u64(print.bcnt));
                        bp += print.bcnt;
                    }
                }
            }
            self.address = saved_address;
        }
    }

    /// Prints one conversion of the bytes at `bytes`; `cut` drops a trailing space.
    fn print(&mut self, print: &Print, bytes: &[u8], cut: bool) {
        let spec = print.spec;
        let mut text = print.text.clone();
        let value = |n: usize| -> u128 {
            let mut buffer = [0u8; 16];
            for (i, byte) in bytes.iter().take(n.min(16)).enumerate() {
                buffer[i] = *byte;
            }
            u128::from_le_bytes(buffer)
        };
        match print.kind {
            Kind::Text => {
                if cut {
                    text.pop();
                }
            }
            Kind::Blank => text.extend(spec.pad(b"", b"", b"", false)),
            Kind::Address(base) => {
                text.extend(spec.integer(false, u128::from(self.address), base));
            }
            Kind::Char => text.extend(spec.character(bytes.first().copied().unwrap_or(0))),
            Kind::Int => {
                let raw = value(print.bcnt);
                let bits = 8 * u32::try_from(print.bcnt).unwrap_or(8);
                let signed = if raw & (1u128 << (bits - 1)) == 0 {
                    i128::try_from(raw).unwrap_or(0)
                } else {
                    i128::try_from(raw).unwrap_or(0) - (1i128 << bits)
                };
                text.extend(spec.integer(signed < 0, signed.unsigned_abs(), print.conv));
            }
            Kind::Uint => text.extend(spec.integer(false, value(print.bcnt), print.conv)),
            Kind::Double => {
                let number = if print.bcnt == 4 {
                    let mut buffer = [0u8; 4];
                    for (i, byte) in bytes.iter().take(4).enumerate() {
                        buffer[i] = *byte;
                    }
                    f64::from(f32::from_le_bytes(buffer))
                } else {
                    let mut buffer = [0u8; 8];
                    for (i, byte) in bytes.iter().take(8).enumerate() {
                        buffer[i] = *byte;
                    }
                    f64::from_le_bytes(buffer)
                };
                text.extend(spec.float(number, print.conv));
            }
            Kind::Str => {
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                text.extend(spec.string(&bytes[..end]));
            }
            Kind::Escaped => {
                let byte = bytes.first().copied().unwrap_or(0);
                let escaped: Option<&[u8]> = match byte {
                    0 => Some(b"\\0"),
                    0x07 => Some(b"\\a"),
                    0x08 => Some(b"\\b"),
                    0x0c => Some(b"\\f"),
                    b'\n' => Some(b"\\n"),
                    b'\r' => Some(b"\\r"),
                    b'\t' => Some(b"\\t"),
                    0x0b => Some(b"\\v"),
                    _ => None,
                };
                match escaped {
                    Some(escaped) => text.extend(spec.string(escaped)),
                    None if is_print(byte) => text.extend(spec.character(byte)),
                    None => text.extend(spec.string(format!("{byte:03o}").as_bytes())),
                }
            }
            Kind::Printable => {
                let byte = bytes.first().copied().unwrap_or(0);
                text.extend(spec.character(if is_print(byte) { byte } else { b'.' }));
            }
            Kind::Named => {
                let byte = bytes.first().copied().unwrap_or(0);
                if let Some(name) = CONTROL_NAMES.get(usize::from(byte)) {
                    text.extend(spec.string(name.as_bytes()));
                } else if byte == 0x7f {
                    text.extend(spec.string(b"del"));
                } else if is_print(byte) {
                    text.extend(spec.character(byte));
                } else {
                    text.extend(spec.integer(false, u128::from(byte), b'x'));
                }
            }
        }
        self.out.extend(text);
    }

    /// The `%_A` unit, printed once the input is done.
    fn finish(&mut self) {
        let Some((f, u)) = self.end_unit else { return };
        let end_address = match self.end_address {
            Some(end) => end,
            None if self.address == 0 => return,
            None => self.address,
        };
        let prints = self.formats[f].units[u].prints.clone();
        for print in prints {
            match print.kind {
                Kind::Address(base) => {
                    self.out.extend(print.text.iter());
                    self.out
                        .extend(print.spec.integer(false, u128::from(end_address), base));
                }
                Kind::Text => self.out.extend(print.text.iter()),
                _ => {}
            }
        }
    }
}

/// A count as an address.
fn to_u64(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// The names `%_u` gives the control characters.
const CONTROL_NAMES: [&str; 32] = [
    "nul", "soh", "stx", "etx", "eot", "enq", "ack", "bel", "bs", "ht", "lf", "vt", "ff", "cr",
    "so", "si", "dle", "dc1", "dc2", "dc3", "dc4", "nak", "syn", "etb", "can", "em", "sub", "esc",
    "fs", "gs", "rs", "us",
];

/// A size as util-linux's `strtosize` reads it: a number (`0x` hex, a leading `0` octal),
/// a fraction, and a suffix `K`, `M`, `G`, `T`, `P`, `E`, `Z` or `Y`, in either case, with
/// `iB` (or nothing) for powers of 1024 and `B` for powers of 1000.
fn parse_size(text: &str) -> Result<u64, &'static str> {
    const INVALID: &str = "Invalid argument";
    let text = text.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    let text = text.strip_prefix('+').unwrap_or(text);
    if text.is_empty() || text.starts_with('-') {
        return Err(INVALID);
    }
    let (radix, digits) = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) if hex.starts_with(|c: char| c.is_ascii_hexdigit()) => (16u32, hex),
        _ if text.len() > 1 && text.starts_with('0') => (8, text),
        _ => (10, text),
    };
    let count = digits.chars().take_while(|c| c.is_digit(radix)).count();
    if count == 0 {
        return Err(INVALID);
    }
    let (number, rest) = digits.split_at(count);
    let mut whole: u128 = 0;
    for c in number.chars() {
        whole = whole
            .checked_mul(u128::from(radix))
            .and_then(|n| n.checked_add(u128::from(c.to_digit(radix).unwrap_or(0))))
            .ok_or("Numerical result out of range")?;
    }
    let (fraction, rest) = match rest.strip_prefix('.') {
        Some(after) => {
            let count = after.chars().take_while(char::is_ascii_digit).count();
            if count == 0 || rest.len() == 1 + count {
                return Err(INVALID);
            }
            after.split_at(count)
        }
        None => ("", rest),
    };
    let mut chars = rest.chars();
    let power = match chars.next() {
        None => 0,
        Some(c) => match c.to_ascii_uppercase() {
            'K' => 1,
            'M' => 2,
            'G' => 3,
            'T' => 4,
            'P' => 5,
            'E' => 6,
            'Z' => 7,
            'Y' => 8,
            _ => return Err(INVALID),
        },
    };
    let base: u128 = match chars.as_str() {
        "" | "iB" | "ib" => 1024,
        "B" | "b" => 1000,
        _ => return Err(INVALID),
    };
    let multiplier = base
        .checked_pow(power)
        .ok_or("Numerical result out of range")?;
    let mut value = whole
        .checked_mul(multiplier)
        .ok_or("Numerical result out of range")?;
    if !fraction.is_empty() {
        let scale = 10u128.pow(u32::try_from(fraction.len()).unwrap_or(0));
        let frac: u128 = fraction.chars().fold(0, |n, c| {
            n.saturating_mul(10)
                .saturating_add(u128::from(c.to_digit(10).unwrap_or(0)))
        });
        value = value
            .checked_add(frac * multiplier / scale)
            .ok_or("Numerical result out of range")?;
    }
    u64::try_from(value).map_err(|_| "Numerical result out of range")
}

/// The long options, in util-linux's order, each known by the short option it stands
/// for.
const LONG_OPTIONS: &[Long<'static, char>] = &[
    Long::new("one-byte-octal", Arg::No, 'b'),
    Long::new("one-byte-hex", Arg::No, 'X'),
    Long::new("one-byte-char", Arg::No, 'c'),
    Long::new("canonical", Arg::No, 'C'),
    Long::new("two-bytes-decimal", Arg::No, 'd'),
    Long::new("two-bytes-octal", Arg::No, 'o'),
    Long::new("two-bytes-hex", Arg::No, 'x'),
    Long::new("color", Arg::Optional, 'L'),
    Long::new("format", Arg::Required, 'e'),
    Long::new("format-file", Arg::Required, 'f'),
    Long::new("length", Arg::Required, 'n'),
    Long::new("skip", Arg::Required, 's'),
    Long::new("no-squeezing", Arg::No, 'v'),
    Long::new("help", Arg::No, 'h'),
    Long::new("version", Arg::No, 'V'),
];

/// What the command line asks for, in order.
#[derive(Default)]
struct Request {
    formats: Vec<Format>,
    /// Format files to read, in the order they were given among the `-e` formats.
    length: Option<u64>,
    skip: u64,
    squeeze: bool,
    files: Vec<String>,
}

/// How the command ends before reading any input.
enum Early {
    Help,
    Version,
    /// `hexdump: <message>`, then the hint, status 1.
    Option(String),
    /// `hexdump: <message>`, status 1.
    Failed(Failure),
}

impl From<Failure> for Early {
    fn from(failure: Failure) -> Self {
        Self::Failed(failure)
    }
}

/// The format strings used when no format is given: two-byte hex words, unpadded.
const DEFAULT_FORMAT: &[&str] = &["\"%07.7_Ax\n\"", "\"%07.7_ax \" 8/2 \"%04x \" \"\\n\""];

/// The format string behind each display option.
const fn builtin_format(short: char) -> Option<&'static [&'static str]> {
    Some(match short {
        'b' => &["\"%07.7_Ax\n\"", "\"%07.7_ax \" 16/1 \"%03o \" \"\\n\""],
        'c' => &["\"%07.7_Ax\n\"", "\"%07.7_ax \" 16/1 \"%3_c \" \"\\n\""],
        'C' => &[
            "\"%08.8_Ax\n\"",
            "\"%08.8_ax  \" 8/1 \"%02x \" \"  \" 8/1 \"%02x \" ",
            "\"  |\" 16/1 \"%_p\" \"|\\n\"",
        ],
        'd' => &["\"%07.7_Ax\n\"", "\"%07.7_ax \" 8/2 \"  %05u \" \"\\n\""],
        'o' => &["\"%07.7_Ax\n\"", "\"%07.7_ax \" 8/2 \" %06o \" \"\\n\""],
        'x' => &["\"%07.7_Ax\n\"", "\"%07.7_ax \" 8/2 \"   %04x \" \"\\n\""],
        'X' => &["\"%07.7_Ax\n\"", "\"%07.7_ax \" 16/1 \" %02x \" \"\\n\""],
        _ => return None,
    })
}

/// Adds one format string as a `Format` of its own.
fn add_format(request: &mut Request, fmt: &[u8]) -> Result<(), Failure> {
    let mut format = Format::default();
    add(fmt, &mut format)?;
    request.formats.push(format);
    Ok(())
}

/// Adds the formats of a `-f` file: one per line, blank lines and `#` lines skipped.
fn add_format_file(request: &mut Request, contents: &[u8]) -> Result<(), Failure> {
    for line in contents.split_inclusive(|&b| b == b'\n') {
        let start = skip_space(line, 0);
        if start >= line.len() || line[start] == b'#' {
            continue;
        }
        add_format(request, &line[start..])?;
    }
    Ok(())
}

impl Request {
    /// Applies one option; `read_file` reads a `-f` file.
    fn apply(
        &mut self,
        short: char,
        value: Option<&str>,
        read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
    ) -> Result<(), Early> {
        match short {
            'b' | 'c' | 'C' | 'd' | 'o' | 'x' | 'X' => {
                for fmt in builtin_format(short).unwrap_or_default() {
                    add_format(self, fmt.as_bytes())?;
                }
            }
            'e' => add_format(self, value.unwrap_or_default().as_bytes())?,
            'f' => {
                let name = value.unwrap_or_default();
                match read_file(name) {
                    Ok(contents) => add_format_file(self, &contents)?,
                    Err(error) => {
                        let reason = cash_core::error::os_error_text(&error);
                        return Err(Failure::new(format!("can't read {name}: {reason}")).into());
                    }
                }
            }
            'n' => {
                let text = value.unwrap_or_default();
                self.length = Some(parse_size(text).map_err(|reason| {
                    Failure::new(format!("failed to parse length: '{text}': {reason}"))
                })?);
            }
            's' => {
                let text = value.unwrap_or_default();
                self.skip = parse_size(text).map_err(|reason| {
                    Failure::new(format!("failed to parse offset: '{text}': {reason}"))
                })?;
            }
            'v' => self.squeeze = false,
            'L' => {
                let mode = value.unwrap_or("auto");
                let mode = mode.strip_prefix('=').unwrap_or(mode);
                match mode {
                    "never" | "auto" => {}
                    "always" => {
                        return Err(Failure::new(
                            "colour output (-L=always) is not supported; formats are printed plain",
                        )
                        .into());
                    }
                    other => {
                        return Err(Failure::new(format!("unsupported color mode: {other}")).into());
                    }
                }
            }
            'h' => return Err(Early::Help),
            'V' => return Err(Early::Version),
            _ => {}
        }
        Ok(())
    }
}

/// Reads the command line as `getopt_long` does (`cash-getopt`): clusters, attached and
/// separate values, long options and their unique prefixes, files anywhere. Every long
/// option has its letter, which is the short option.
fn parse(
    args: &[String],
    read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
) -> Result<Request, Early> {
    let mut request = Request {
        squeeze: true,
        ..Request::default()
    };
    let shorts: Vec<Short<char>> = LONG_OPTIONS
        .iter()
        .map(|long| Short::new(long.id, long.arg, long.id))
        .collect();
    for next in Getopt::new(&shorts, LONG_OPTIONS).read(args) {
        match next.map_err(|problem| Early::Option(problem.to_string()))? {
            Item::Option { id, value, .. } => request.apply(id, value.as_deref(), read_file)?,
            Item::Operand { value, .. } => request.files.push(value),
        }
    }
    Ok(request)
}

impl builtins::Command for HexdumpCommand {
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
        let read_file =
            |name: &str| std::fs::read(context.shell.absolute_path(std::path::Path::new(name)));
        let mut request = match parse(&self.args, &read_file) {
            Ok(request) => request,
            Err(Early::Help) => {
                write!(context.stdout(), "{USAGE}")?;
                return Ok(ExecutionResult::success());
            }
            Err(Early::Version) => {
                writeln!(context.stdout(), "{VERSION}")?;
                return Ok(ExecutionResult::success());
            }
            Err(Early::Option(message)) => {
                writeln!(context.stderr(), "hexdump: {message}\n{HINT}")?;
                return Ok(ExecutionResult::general_error());
            }
            Err(Early::Failed(Failure(message))) => {
                writeln!(context.stderr(), "hexdump: {message}")?;
                return Ok(ExecutionResult::general_error());
            }
        };
        if request.formats.is_empty() {
            for fmt in DEFAULT_FORMAT {
                if let Err(Failure(message)) = add_format(&mut request, fmt.as_bytes()) {
                    writeln!(context.stderr(), "hexdump: {message}")?;
                    return Ok(ExecutionResult::general_error());
                }
            }
        }

        // `-n 0` reads nothing at all: util-linux opens no file, skips nothing and so
        // prints no `%_A` position either.
        if request.length == Some(0) {
            return Ok(ExecutionResult::success());
        }

        // The input: the files one after another, or standard input, with `-s` skipped
        // through them file by file (a file shorter than what is left to skip is passed
        // over whole, and counts whole).
        let mut status = ExecutionResult::success();
        let mut inputs: Vec<Vec<u8>> = Vec::new();
        if request.files.is_empty() {
            let mut input = Vec::new();
            context.stdin().read_to_end(&mut input)?;
            inputs.push(input);
        } else {
            let mut opened = 0;
            for name in &request.files {
                match read_file(name) {
                    Ok(bytes) => {
                        opened += 1;
                        inputs.push(bytes);
                    }
                    Err(error) => {
                        let reason = cash_core::error::os_error_text(&error);
                        writeln!(context.stderr(), "hexdump: {name}: {reason}")?;
                        status = ExecutionResult::general_error();
                    }
                }
            }
            if opened == 0 {
                writeln!(context.stderr(), "hexdump: all input file arguments failed")?;
                return Ok(ExecutionResult::general_error());
            }
        }
        let mut skip = request.skip;
        let mut address: u64 = 0;
        let mut data = Vec::new();
        for input in inputs {
            let len = u64::try_from(input.len()).unwrap_or(u64::MAX);
            if skip >= len {
                address = address.wrapping_add(len);
                skip -= len;
                continue;
            }
            address = address.wrapping_add(skip);
            data.extend_from_slice(
                input
                    .get(usize::try_from(skip).unwrap_or(usize::MAX)..)
                    .unwrap_or_default(),
            );
            skip = 0;
        }
        if let Some(length) = request.length {
            data.truncate(usize::try_from(length).unwrap_or(usize::MAX));
        }

        let mut engine = match Engine::new(request.formats, request.squeeze, address) {
            Ok(engine) => engine,
            Err(Failure(message)) => {
                writeln!(context.stderr(), "hexdump: {message}")?;
                return Ok(ExecutionResult::general_error());
            }
        };
        engine.run(&data);
        engine.finish();
        context.stdout().write_all(&engine.out)?;
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
    use super::{Engine, Format, Kind, Request, Spec, add, escape, parse, parse_size};

    fn hexdump(args: &[&str], input: &[u8]) -> Result<String, String> {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        let no_files = |name: &str| Err(std::io::Error::other(name.to_owned()));
        let mut request: Request = match parse(&args, &no_files) {
            Ok(request) => request,
            Err(super::Early::Failed(super::Failure(message)) | super::Early::Option(message)) => {
                return Err(message);
            }
            Err(_) => return Err("help or version".to_owned()),
        };
        if request.formats.is_empty() {
            for fmt in super::DEFAULT_FORMAT {
                super::add_format(&mut request, fmt.as_bytes()).map_err(|f| f.0)?;
            }
        }
        let mut engine =
            Engine::new(request.formats, request.squeeze, request.skip).map_err(|f| f.0)?;
        let skip = usize::try_from(request.skip).unwrap_or(usize::MAX);
        engine.run(input.get(skip..).unwrap_or_default());
        engine.finish();
        Ok(String::from_utf8_lossy(&engine.out).into_owned())
    }

    #[test]
    fn the_canonical_format_matches_util_linux() {
        // After `0a`: its own space, two blank bytes (one without its trailing space,
        // being the unit's last turn), the "  " unit, eight blank bytes, and "  |".
        let gap = " ".repeat(1 + 3 + 2 + 2 + 7 * 3 + 2 + 2);
        assert_eq!(
            hexdump(&["-C"], b"hello\n"),
            Ok(format!(
                "00000000  68 65 6c 6c 6f 0a{gap}|hello.|\n00000006\n"
            ))
        );
        assert_eq!(
            hexdump(&[], b"hello world!!"),
            Ok("0000000 6568 6c6c 206f 6f77 6c72 2164 0021     \n000000d\n".to_owned())
        );
        assert_eq!(
            hexdump(
                &["-c"],
                b"\x00\x01\x07\x08\x09\x0a\x0b\x0c\x0d\x1b\x1f\x20\x7e\x7f\x80\xff"
            ),
            Ok(
                "0000000  \\0 001  \\a  \\b  \\t  \\n  \\v  \\f  \\r 033 037       ~ 177 200 377\n\
                0000010\n"
                    .to_owned()
            )
        );
    }

    #[test]
    fn a_format_string_is_parsed_into_units() {
        let mut format = Format::default();
        add(b"16/1 \"%02x \" \"\\n\"", &mut format).ok();
        assert_eq!(format.units.len(), 2);
        assert_eq!((format.units[0].reps, format.units[0].bcnt), (16, 1));
        assert_eq!(format.units[0].fmt, b"%02x ");
        assert_eq!(format.units[1].fmt, b"\n");
        let mut canonical = Format::default();
        add(
            b"\"%08.8_ax  \" 8/1 \"%02x \" \"  \" 8/1 \"%02x \"",
            &mut canonical,
        )
        .ok();
        assert_eq!(canonical.units.len(), 4);
        assert_eq!(super::block_size(&canonical), 16);
        let mut engine = Engine::new(vec![canonical], true, 0).unwrap();
        let prints = &engine.formats[0].units[0].prints;
        assert_eq!(prints.len(), 2);
        assert_eq!(prints[0].kind, Kind::Address(b'x'));
        // C reads the `0` of `%08.8` as the zero flag, which a precision then disables.
        assert_eq!(
            prints[0].spec,
            Spec {
                zero: true,
                width: 8,
                precision: Some(8),
                ..Spec::default()
            }
        );
        assert_eq!(prints[1].text, b"  ");
        engine.run(b"hi");
        let line = String::from_utf8_lossy(&engine.out).into_owned();
        assert!(line.starts_with("00000000  68 69 "), "{line}");
        assert_eq!(line.len(), 10 + 48, "{line}");
        assert!(add(b"4/1 %02x", &mut Format::default()).is_err());
        assert!(add(b"16/1\"%02x\"", &mut Format::default()).is_err());
    }

    #[test]
    fn formats_repeat_pad_and_squeeze() {
        assert_eq!(
            hexdump(&["-e", "4/1 \"%02x \" \"\\n\""], b"hello"),
            Ok(format!("68 65 6c 6c\n6f{}\n", " ".repeat(9)))
        );
        // The last unit, with no count of its own, is repeated to fill the block.
        assert_eq!(
            hexdump(
                &["-e", "\"%_ad: \" /1 \"%02x \"", "-e", "4/1 \"%_p\" \"\\n\""],
                b"hello"
            ),
            Ok(format!("0: 68 65 6c 6chell\n4: 6f{}o\n", " ".repeat(9)))
        );
        assert_eq!(
            hexdump(
                &["-e", "2/1 \"%02x\" \"\\n\"", "-e", "\"%_Ax\\n\""],
                b"aaaaaaaa"
            ),
            Ok("6161\n*\n8\n".to_owned())
        );
        assert_eq!(
            hexdump(
                &["-v", "-e", "1/1 \"%_u|\"", "-e", "\"\\n\""],
                b"\x00\x7f\x80a"
            ),
            Ok("nul|\ndel|\n80|\na|\n".to_owned())
        );
        assert_eq!(
            hexdump(&["-e", "2/5 \"%s|\" \"\\n\""], b"hello world!!"),
            Ok("hello worl| worl|\nd!!||\n".to_owned())
        );
    }

    #[test]
    fn conversions_print_as_c_does() {
        assert_eq!(Spec::parse(b"-+5.2").integer(false, 7, b'd'), b"+07  ");
        assert_eq!(Spec::parse(b"#5").integer(false, 65, b'o'), b" 0101");
        assert_eq!(Spec::parse(b"#").integer(false, 0, b'x'), b"0");
        assert_eq!(Spec::parse(b"05").integer(true, 5, b'd'), b"-0005");
        assert_eq!(Spec::parse(b"").float(0.1, b'e'), b"1.000000e-01");
        assert_eq!(Spec::parse(b"").float(100_000.0, b'g'), b"100000");
        assert_eq!(Spec::parse(b"").float(1_000_000.0, b'g'), b"1e+06");
        assert_eq!(
            Spec::parse(b"").float(0.000_012_338_6, b'g'),
            b"1.23386e-05"
        );
        assert_eq!(Spec::parse(b"#.3").float(0.1, b'g'), b"0.100");
        assert_eq!(
            Spec::parse(b"010").float(f64::INFINITY, b'f'),
            b"       inf"
        );
        assert_eq!(Spec::parse(b"+").float(-f64::NAN, b'G'), b"-NAN");
        assert_eq!(Spec::parse(b"012.3").float(1.0, b'f'), b"00000001.000");
        assert_eq!(
            hexdump(&["-e", "8/1 \"%d \" \"\\n\""], b"\x80\xff"),
            Ok(format!("-128 -1{}\n", " ".repeat(6)))
        );
    }

    #[test]
    fn escapes_are_util_linuxs() {
        let mut fmt = b"%02x\\n".to_vec();
        escape(&mut fmt);
        assert_eq!(fmt, b"%02x\n");
        let mut odd = b"%02x\\x41".to_vec();
        escape(&mut odd);
        assert_eq!(odd, b"%02xxx4");
    }

    #[test]
    fn sizes_take_suffixes() {
        assert_eq!(parse_size("300"), Ok(300));
        assert_eq!(parse_size("0x10"), Ok(16));
        assert_eq!(parse_size("010"), Ok(8));
        assert_eq!(parse_size("1k"), Ok(1024));
        assert_eq!(parse_size("1KiB"), Ok(1024));
        assert_eq!(parse_size("1KB"), Ok(1000));
        assert_eq!(parse_size("2.5k"), Ok(2560));
        assert!(parse_size("1b").is_err());
        assert!(parse_size("-3").is_err());
        assert!(parse_size("").is_err());
        assert!(parse_size("99999999999999999999").is_err());
    }

    #[test]
    fn bad_formats_are_named() {
        assert_eq!(
            hexdump(&["-e", "4/1 \"%y\""], b"x"),
            Err("bad conversion character %y".to_owned())
        );
        assert_eq!(
            hexdump(&["-e", "2/3 \"%d\""], b"x"),
            Err("bad byte count for conversion character d".to_owned())
        );
        assert_eq!(
            hexdump(&["-e", "\"%s\""], b"x"),
            Err("%s requires a precision or a byte count".to_owned())
        );
        assert_eq!(
            hexdump(&["-Z"], b"x"),
            Err("invalid option -- 'Z'".to_owned())
        );
        assert_eq!(
            hexdump(&["--one"], b"x"),
            Err("option '--one' is ambiguous; possibilities: '--one-byte-octal' '--one-byte-hex' '--one-byte-char'".to_owned())
        );
    }
}
