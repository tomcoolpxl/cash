// This file is part of the uutils diffutils package.
//
// For the full copyright and license information, please view the LICENSE-*
// files that was distributed with this source code.

//! `cmp`: upstream's comparison, behind GNU cmp 3.12's command line and messages
//! (CASH-PATCHES.md).

use crate::utils::{format_failure_to_read_input_file, locale_is_posix, quote};
use cash_getopt::{Arg, Getopt, Item, Long, Short};
use std::ffi::OsString;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::{cmp, fs, io};

#[cfg(target_os = "windows")]
use std::os::windows::fs::MetadataExt;

/// for --bytes, so really large number limits can be expressed, like 1Y.
pub type BytesLimitU64 = u64;
// ignore initial is currently limited to u64, as take(skip) is used.
pub type SkipU64 = u64;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Params {
    executable: OsString,
    from: OsString,
    to: OsString,
    print_bytes: bool,
    skip_a: Option<SkipU64>,
    skip_b: Option<SkipU64>,
    max_bytes: Option<BytesLimitU64>,
    verbose: bool,
    quiet: bool,
}

pub const VERSION: &str = "cmp (cash): GNU diffutils 3.12's options, from uutils diffutils";

pub const HELP: &str = "\
Usage: cmp [OPTION]... FILE1 [FILE2 [SKIP1 [SKIP2]]]
Compare two files byte by byte.

The optional SKIP1 and SKIP2 specify the number of bytes to skip
at the beginning of each file (zero by default).

Mandatory arguments to long options are mandatory for short options too.
  -b, --print-bytes          print differing bytes
  -i, --ignore-initial=SKIP         skip first SKIP bytes of both inputs
  -i, --ignore-initial=SKIP1:SKIP2  skip first SKIP1 bytes of FILE1 and
                                      first SKIP2 bytes of FILE2
  -l, --verbose              output byte numbers and differing byte values
  -n, --bytes=LIMIT          compare at most LIMIT bytes
  -s, --quiet, --silent      suppress all normal output
      --help                 display this help and exit
  -v, --version              output version information and exit

SKIP values may be followed by the following multiplicative suffixes:
kB 1000, K 1024, MB 1,000,000, M 1,048,576,
GB 1,000,000,000, G 1,073,741,824, and so on for T, P, E, Z, Y.

If a FILE is '-' or missing, read standard input.
Exit status is 0 if inputs are the same, 1 if different, 2 if trouble.
";

/// What the command line asked for.
#[derive(Debug)]
pub enum Request {
    Compare(Params),
    Help,
    Version,
}

const SHORTS: &[Short<&str>] = &[
    Short {
        letter: 'b',
        arg: Arg::No,
        id: "print-bytes",
    },
    Short {
        letter: 'i',
        arg: Arg::Required,
        id: "ignore-initial",
    },
    Short {
        letter: 'l',
        arg: Arg::No,
        id: "verbose",
    },
    Short {
        letter: 'n',
        arg: Arg::Required,
        id: "bytes",
    },
    Short {
        letter: 's',
        arg: Arg::No,
        id: "quiet",
    },
    Short {
        letter: 'v',
        arg: Arg::No,
        id: "version",
    },
];

const LONGS: &[Long<'static, &str>] = &[
    Long {
        name: "bytes",
        arg: Arg::Required,
        id: "bytes",
    },
    Long {
        name: "help",
        arg: Arg::No,
        id: "help",
    },
    Long {
        name: "ignore-initial",
        arg: Arg::Required,
        id: "ignore-initial",
    },
    Long {
        name: "print-bytes",
        arg: Arg::No,
        id: "print-bytes",
    },
    Long {
        name: "quiet",
        arg: Arg::No,
        id: "quiet",
    },
    Long {
        name: "silent",
        arg: Arg::No,
        id: "quiet",
    },
    Long {
        name: "verbose",
        arg: Arg::No,
        id: "verbose",
    },
    Long {
        name: "version",
        arg: Arg::No,
        id: "version",
    },
];

/// A SKIP or LIMIT: digits with one of GNU's multiplicative suffixes.
fn parse_size(text: &str) -> Option<SkipU64> {
    let suffix_start = text
        .find(|b: char| !b.is_ascii_digit())
        .unwrap_or(text.len());
    let (digits, suffix) = text.split_at(suffix_start);
    let num = match digits.parse::<SkipU64>() {
        Ok(num) => num,
        Err(e) if *e.kind() == std::num::IntErrorKind::PosOverflow => SkipU64::MAX,
        Err(_) => return None,
    };
    if suffix.is_empty() {
        return Some(num);
    }
    // Note that GNU cmp advertises supporting up to Y, but fails if you try
    // to actually use anything beyond E.
    let multiplier: SkipU64 = match suffix {
        "kB" => 1_000,
        "K" | "k" => 1_024,
        "MB" => 1_000_000,
        "M" => 1_048_576,
        "GB" => 1_000_000_000,
        "G" => 1_073_741_824,
        "TB" => 1_000_000_000_000,
        "T" => 1_099_511_627_776,
        "PB" => 1_000_000_000_000_000,
        "P" => 1_125_899_906_842_624,
        "EB" => 1_000_000_000_000_000_000,
        "E" => 1_152_921_504_606_846_976,
        "ZB" | "Z" | "YB" | "Y" => SkipU64::MAX,
        _ => return None,
    };
    Some(num.saturating_mul(multiplier))
}

/// Reads `args`, the whole command line with the tool's name first. The error is the
/// message, without the tool's name or the `Try` line.
pub fn parse_params(args: &[OsString]) -> Result<Request, String> {
    let (executable, rest) = args.split_first().map_or_else(
        || (OsString::from("cmp"), args),
        |(exe, rest)| (exe.clone(), rest),
    );
    let parsed = Getopt::new(SHORTS, LONGS)
        .parse(rest)
        .map_err(|problem| problem.to_string())?;
    let mut params = Params {
        executable,
        ..Default::default()
    };
    let mut operands: Vec<OsString> = Vec::new();
    for item in parsed.items {
        let (id, value) = match item {
            Item::Operand { value: word, .. } => {
                operands.push(word);
                continue;
            }
            Item::Option { id, value, .. } => (id, value),
        };
        let text = value
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default();
        match id {
            "help" => return Ok(Request::Help),
            "version" => return Ok(Request::Version),
            "print-bytes" => params.print_bytes = true,
            "verbose" => params.verbose = true,
            "quiet" => params.quiet = true,
            "bytes" => {
                params.max_bytes = Some(
                    parse_size(&text)
                        .ok_or_else(|| format!("invalid --bytes value {}", quote(&text)))?,
                );
            }
            "ignore-initial" => {
                let invalid = || format!("invalid --ignore-initial value {}", quote(&text));
                let (skip_a, skip_b) = match text.split_once(':') {
                    Some((a, b)) => (
                        parse_size(a).ok_or_else(invalid)?,
                        parse_size(b).ok_or_else(invalid)?,
                    ),
                    None => {
                        let skip = parse_size(&text).ok_or_else(invalid)?;
                        (skip, skip)
                    }
                };
                params.skip_a = Some(skip_a);
                params.skip_b = Some(skip_b);
            }
            _ => return Err(format!("unrecognized option '{id}'")),
        }
    }

    if params.quiet && params.verbose {
        return Err("options -l and -s are incompatible".to_string());
    }

    let mut operands = operands.into_iter();
    params.from = operands.next().ok_or_else(|| {
        format!(
            "missing operand after {}",
            quote(&params.executable.to_string_lossy())
        )
    })?;
    params.to = operands.next().unwrap_or_else(|| OsString::from("-"));
    // A positional SKIP wins over -i, as GNU cmp 3.12 has it.
    for slot in [&mut params.skip_a, &mut params.skip_b] {
        let Some(word) = operands.next() else {
            break;
        };
        let text = word.to_string_lossy();
        let skip = parse_size(&text).ok_or_else(|| format!("invalid operand {}", quote(&text)))?;
        *slot = Some(skip);
    }
    if let Some(extra) = operands.next() {
        return Err(format!("extra operand {}", quote(&extra.to_string_lossy())));
    }
    Ok(Request::Compare(params))
}

fn prepare_reader(
    path: &OsString,
    skip: &Option<SkipU64>,
    params: &Params,
) -> Result<Box<dyn BufRead>, String> {
    let mut reader: Box<dyn BufRead> = if path == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        // cash: Windows opens a directory and fails on the read with "Access is denied";
        // GNU says what it is.
        if fs::metadata(path).is_ok_and(|m| m.is_dir()) {
            return Err(format!(
                "{}: {}: Is a directory",
                params.executable.to_string_lossy(),
                path.to_string_lossy()
            ));
        }
        let file = fs::File::open(path)
            .map_err(|e| format_failure_to_read_input_file(&params.executable, path, &e))?;
        Box::new(BufReader::new(file))
    };

    if let Some(skip) = skip {
        // cast as u64 must remain, because value of IgnInit data type could be changed.
        io::copy(&mut reader.by_ref().take(*skip), &mut io::sink())
            .map_err(|e| format_failure_to_read_input_file(&params.executable, path, &e))?;
    }

    Ok(reader)
}

#[derive(Debug)]
pub enum Cmp {
    Equal,
    Different,
}

pub fn cmp(params: &Params) -> Result<Cmp, String> {
    let mut from = prepare_reader(&params.from, &params.skip_a, params)?;
    let mut to = prepare_reader(&params.to, &params.skip_b, params)?;

    let mut offset_width = params.max_bytes.unwrap_or(BytesLimitU64::MAX);

    if let (Ok(a_meta), Ok(b_meta)) = (fs::metadata(&params.from), fs::metadata(&params.to)) {
        #[cfg(not(target_os = "windows"))]
        let (a_size, b_size) = (a_meta.len(), b_meta.len());

        #[cfg(target_os = "windows")]
        let (a_size, b_size) = (a_meta.file_size(), b_meta.file_size());

        // If the files have different sizes, we already know they are not identical. If we have not
        // been asked to show even the first difference, we can quit early.
        if params.quiet && a_size != b_size {
            return Ok(Cmp::Different);
        }

        let smaller = cmp::min(a_size, b_size) as BytesLimitU64;
        offset_width = cmp::min(smaller, offset_width);
    }

    let offset_width = 1 + offset_width.checked_ilog10().unwrap_or(1) as usize;

    // Capacity calc: at_byte width + 2 x 3-byte octal numbers + 2 x 4-byte value + 4 spaces
    let mut output = Vec::<u8>::with_capacity(offset_width + 3 * 2 + 4 * 2 + 4);

    let mut at_byte: BytesLimitU64 = 1;
    let mut at_line: u64 = 1;
    let mut start_of_line = true;
    let mut stdout = BufWriter::new(io::stdout().lock());
    let mut compare = Cmp::Equal;
    loop {
        // Fill up our buffers.
        let from_buf = from
            .fill_buf()
            .map_err(|e| format_failure_to_read_input_file(&params.executable, &params.from, &e))?;

        let to_buf = to
            .fill_buf()
            .map_err(|e| format_failure_to_read_input_file(&params.executable, &params.to, &e))?;

        // Check for EOF conditions.
        if from_buf.is_empty() && to_buf.is_empty() {
            break;
        }

        if from_buf.is_empty() || to_buf.is_empty() {
            let eof_on = if from_buf.is_empty() {
                &params.from.to_string_lossy()
            } else {
                &params.to.to_string_lossy()
            };

            report_eof(at_byte, at_line, start_of_line, eof_on, params);
            return Ok(Cmp::Different);
        }

        // Fast path - for long files in which almost all bytes are the same we
        // can do a direct comparison to let the compiler optimize.
        let consumed = std::cmp::min(from_buf.len(), to_buf.len());
        if from_buf[..consumed] == to_buf[..consumed] {
            let last = from_buf[..consumed].last().unwrap();

            at_byte += consumed as BytesLimitU64;
            at_line += (from_buf[..consumed].iter().filter(|&c| *c == b'\n').count()) as u64;

            start_of_line = *last == b'\n';

            if let Some(max_bytes) = params.max_bytes {
                if at_byte > max_bytes {
                    break;
                }
            }

            from.consume(consumed);
            to.consume(consumed);

            continue;
        }

        // Iterate over the buffers, the zip iterator will stop us as soon as the
        // first one runs out.
        for (&from_byte, &to_byte) in from_buf.iter().zip(to_buf.iter()) {
            if from_byte != to_byte {
                compare = Cmp::Different;

                if params.verbose {
                    format_verbose_difference(
                        from_byte,
                        to_byte,
                        at_byte,
                        offset_width,
                        &mut output,
                        params,
                    )?;
                    stdout.write_all(output.as_slice()).map_err(|e| {
                        format!(
                            "{}: error printing output: {e}",
                            params.executable.to_string_lossy()
                        )
                    })?;
                    output.clear();
                } else {
                    report_difference(from_byte, to_byte, at_byte, at_line, params);
                    return Ok(Cmp::Different);
                }
            }

            start_of_line = from_byte == b'\n';
            if start_of_line {
                at_line += 1;
            }

            at_byte += 1;

            if let Some(max_bytes) = params.max_bytes {
                if at_byte > max_bytes {
                    break;
                }
            }
        }

        // Notify our readers about the bytes we went over.
        from.consume(consumed);
        to.consume(consumed);
    }

    Ok(compare)
}

// Exit codes are documented at
// https://www.gnu.org/software/diffutils/manual/html_node/Invoking-cmp.html
//     An exit status of 0 means no differences were found,
//     1 means some differences were found,
//     and 2 means trouble.
/// Runs cmp with `args`, the tool's name first; returns its exit status.
pub fn main(args: &[OsString]) -> i32 {
    let params = match parse_params(args) {
        Ok(Request::Compare(params)) => params,
        Ok(Request::Help) => {
            print!("{HELP}");
            return 0;
        }
        Ok(Request::Version) => {
            println!("{VERSION}");
            return 0;
        }
        Err(message) => {
            eprintln!("cmp: {message}");
            eprintln!("cmp: Try 'cmp --help' for more information.");
            return 2;
        }
    };

    if params.from == "-" && params.to == "-"
        || same_file::is_same_file(&params.from, &params.to).unwrap_or(false)
    {
        return 0;
    }

    match cmp(&params) {
        Ok(Cmp::Equal) => 0,
        Ok(Cmp::Different) => 1,
        Err(e) => {
            if !params.quiet {
                eprintln!("{e}");
            }
            2
        }
    }
}

#[inline]
fn format_octal(byte: u8, buf: &mut [u8; 3]) -> &str {
    *buf = *b"  0";

    let mut num = byte;
    let mut idx = 2; // Start at the last position in the buffer

    // Generate octal digits
    while num > 0 {
        buf[idx] = b'0' + num % 8;
        num /= 8;
        idx = idx.saturating_sub(1);
    }

    // SAFETY: the operations we do above always land within ascii range.
    unsafe { std::str::from_utf8_unchecked(&buf[..]) }
}

#[inline]
fn write_visible_byte(output: &mut Vec<u8>, byte: u8) -> usize {
    match byte {
        // Control characters: ^@, ^A, ..., ^_
        0..=31 => {
            output.push(b'^');
            output.push(byte + 64);
            2
        }
        // Printable ASCII (space through ~)
        32..=126 => {
            output.push(byte);
            1
        }
        // DEL: ^?
        127 => {
            output.extend_from_slice(b"^?");
            2
        }
        // High bytes with control equivalents: M-^@, M-^A, ..., M-^_
        128..=159 => {
            output.push(b'M');
            output.push(b'-');
            output.push(b'^');
            output.push(byte - 64);
            4
        }
        // High bytes: M-<space>, M-!, ..., M-~
        160..=254 => {
            output.push(b'M');
            output.push(b'-');
            output.push(byte - 128);
            3
        }
        // Byte 255: M-^?
        255 => {
            output.extend_from_slice(b"M-^?");
            4
        }
    }
}

/// Writes a byte in visible form with right-padding to 4 spaces.
#[inline]
fn write_visible_byte_padded(output: &mut Vec<u8>, byte: u8) {
    const SPACES: &[u8] = b"    ";
    const WIDTH: usize = SPACES.len();

    let display_width = write_visible_byte(output, byte);

    // Add right-padding spaces
    let padding = WIDTH.saturating_sub(display_width);
    output.extend_from_slice(&SPACES[..padding]);
}

/// Formats a byte as a visible string (for non-performance-critical path)
#[inline]
fn format_visible_byte(byte: u8) -> String {
    let mut result = Vec::with_capacity(4);
    write_visible_byte(&mut result, byte);
    // SAFETY: the checks and shifts in write_visible_byte match what cat and GNU
    // cmp do to ensure characters fall inside the ascii range.
    unsafe { String::from_utf8_unchecked(result) }
}

// This function has been optimized to not use the Rust fmt system, which
// leads to a massive speed up when processing large files: cuts the time
// for comparing 2 ~36MB completely different files in half on an M1 Max.
#[inline]
fn format_verbose_difference(
    from_byte: u8,
    to_byte: u8,
    at_byte: BytesLimitU64,
    offset_width: usize,
    output: &mut Vec<u8>,
    params: &Params,
) -> Result<(), String> {
    assert!(!params.quiet);

    let mut at_byte_buf = itoa::Buffer::new();
    let mut from_oct = [0u8; 3]; // for octal conversions
    let mut to_oct = [0u8; 3];

    if params.print_bytes {
        // "{:>width$} {:>3o} {:4} {:>3o} {}",
        let at_byte_str = at_byte_buf.format(at_byte);
        let at_byte_padding = offset_width.saturating_sub(at_byte_str.len());

        for _ in 0..at_byte_padding {
            output.push(b' ')
        }

        output.extend_from_slice(at_byte_str.as_bytes());

        output.push(b' ');

        output.extend_from_slice(format_octal(from_byte, &mut from_oct).as_bytes());

        output.push(b' ');

        write_visible_byte_padded(output, from_byte);

        output.push(b' ');

        output.extend_from_slice(format_octal(to_byte, &mut to_oct).as_bytes());

        output.push(b' ');

        write_visible_byte(output, to_byte);

        output.push(b'\n');
    } else {
        // "{:>width$} {:>3o} {:>3o}"
        let at_byte_str = at_byte_buf.format(at_byte);
        let at_byte_padding = offset_width.saturating_sub(at_byte_str.len());

        for _ in 0..at_byte_padding {
            output.push(b' ')
        }

        output.extend_from_slice(at_byte_str.as_bytes());

        output.push(b' ');

        output.extend_from_slice(format_octal(from_byte, &mut from_oct).as_bytes());

        output.push(b' ');

        output.extend_from_slice(format_octal(to_byte, &mut to_oct).as_bytes());

        output.push(b'\n');
    }

    Ok(())
}

#[inline]
fn report_eof(
    at_byte: BytesLimitU64,
    at_line: u64,
    start_of_line: bool,
    eof_on: &str,
    params: &Params,
) {
    if params.quiet {
        return;
    }

    if at_byte == 1 {
        eprintln!(
            "{}: EOF on {} which is empty",
            params.executable.to_string_lossy(),
            quote(eof_on)
        );
    } else if params.verbose {
        eprintln!(
            "{}: EOF on {} after byte {}",
            params.executable.to_string_lossy(),
            quote(eof_on),
            at_byte - 1,
        );
    } else if start_of_line {
        eprintln!(
            "{}: EOF on {} after byte {}, line {}",
            params.executable.to_string_lossy(),
            quote(eof_on),
            at_byte - 1,
            at_line - 1
        );
    } else {
        eprintln!(
            "{}: EOF on {} after byte {}, in line {}",
            params.executable.to_string_lossy(),
            quote(eof_on),
            at_byte - 1,
            at_line
        );
    }
}

/// cash: the locale rule is `utils::locale_is_posix`'s, where nothing named means
/// UTF-8, as it does for the rest of cash.
fn is_posix_locale() -> bool {
    locale_is_posix()
}

#[inline]
fn report_difference(
    from_byte: u8,
    to_byte: u8,
    at_byte: BytesLimitU64,
    at_line: u64,
    params: &Params,
) {
    if params.quiet {
        return;
    }

    let term = if is_posix_locale() && !params.print_bytes {
        "char"
    } else {
        "byte"
    };
    print!(
        "{} {} differ: {term} {}, line {}",
        params.from.to_string_lossy(),
        params.to.to_string_lossy(),
        at_byte,
        at_line
    );
    if params.print_bytes {
        let char_width = if to_byte >= 0x7F { 2 } else { 1 };
        print!(
            " is {:>3o} {:char_width$} {:>3o} {:char_width$}",
            from_byte,
            format_visible_byte(from_byte),
            to_byte,
            format_visible_byte(to_byte)
        );
    }
    println!();
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::field_reassign_with_default,
    clippy::assert_is_empty,
    clippy::panic_in_result_fn,
    clippy::format_collect,
    reason = "a test stops loudly, and sets the options it is about"
)]
mod tests {
    use super::*;

    fn parse(list: &[&str]) -> Result<Params, String> {
        let mut args = vec![OsString::from("cmp")];
        args.extend(list.iter().map(OsString::from));
        match parse_params(&args)? {
            Request::Compare(params) => Ok(params),
            other => panic!("not a comparison: {other:?}"),
        }
    }

    fn os(s: &str) -> OsString {
        OsString::from(s)
    }

    #[test]
    fn positional() {
        let base = Params {
            executable: os("cmp"),
            from: os("foo"),
            to: os("bar"),
            ..Default::default()
        };
        assert_eq!(parse(&["foo", "bar"]), Ok(base.clone()));
        assert_eq!(
            parse(&["foo"]),
            Ok(Params {
                to: os("-"),
                ..base.clone()
            })
        );
        assert_eq!(
            parse(&["foo", "bar", "1", "2K"]),
            Ok(Params {
                skip_a: Some(1),
                skip_b: Some(2048),
                ..base.clone()
            })
        );
        // The positional skips win over -i, as in GNU cmp 3.12.
        assert_eq!(
            parse(&["-i", "3:4", "foo", "bar", "1", "2"]),
            Ok(Params {
                skip_a: Some(1),
                skip_b: Some(2),
                ..base.clone()
            })
        );
        assert_eq!(
            parse(&["-lb", "-n5", "--bytes=6", "foo", "bar"]),
            Ok(Params {
                verbose: true,
                print_bytes: true,
                max_bytes: Some(6),
                ..base
            })
        );
    }

    #[test]
    fn gnus_messages() {
        assert_eq!(parse(&[]), Err("missing operand after 'cmp'".into()));
        assert_eq!(
            parse(&["a", "b", "1", "2", "3"]),
            Err("extra operand '3'".into())
        );
        assert_eq!(
            parse(&["-l", "-s", "a", "b"]),
            Err("options -l and -s are incompatible".into())
        );
        assert_eq!(
            parse(&["-i", "x", "a", "b"]),
            Err("invalid --ignore-initial value 'x'".into())
        );
        assert_eq!(
            parse(&["-n", "x", "a", "b"]),
            Err("invalid --bytes value 'x'".into())
        );
        assert_eq!(
            parse(&["--foo", "a", "b"]),
            Err("unrecognized option '--foo'".into())
        );
        assert!(matches!(
            parse_params(&[os("cmp"), os("--help")]),
            Ok(Request::Help)
        ));
    }

    #[test]
    fn sizes() {
        assert_eq!(parse_size("10"), Some(10));
        assert_eq!(parse_size("1kB"), Some(1000));
        assert_eq!(parse_size("1M"), Some(1_048_576));
        assert_eq!(parse_size("1Y"), Some(SkipU64::MAX));
        assert_eq!(parse_size("x"), None);
        assert_eq!(parse_size("1X"), None);
    }
}
