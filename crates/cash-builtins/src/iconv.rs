//! `iconv`, glibc's: text from one character set to another, through Windows' code pages.
//!
//! Checked against glibc 2.44's `iconv` (`crates/cash/tests/oracle/iconv_cases.sh`). The
//! options, defaults, messages and statuses are glibc's: UTF-8 both ways unless told
//! otherwise, `-c` and `//IGNORE` drop what cannot be converted and exit 1, `//TRANSLIT`
//! approximates, a bad byte stops the conversion with its position in the file, and the
//! output up to it is still written.
//!
//! The conversions are Windows': `MultiByteToWideChar` and `WideCharToMultiByte` on the
//! code page a name stands for (`CP1252` is 1252, `LATIN1` 28591, `SHIFT_JIS` 932 ...).
//! The exceptions are converted here: UTF-8, UTF-16, UTF-32, UCS-2 and UCS-4, so that
//! their byte-order marks and errors come out as glibc's do; ASCII, whose Windows table
//! drops the high bit rather than object; and ISO-8859-10, -14 and -16, which Windows has
//! no code page for and three small tables supply.
//!
//! A conversion runs whole first, and only input with a problem is walked character by
//! character to name the position, or to skip the bad ones under `-c`.
//!
//! Deliberate differences from glibc:
//! - `//TRANSLIT` approximates with Windows' best-fit mappings and `?`, not glibc's
//!   tables: `€` to ASCII is `?`, not `EUR`.
//! - `//IGNORE` without `-c` is silent, as `-c` is; glibc also prints an "illegal input
//!   sequence" at the end of the input, a quirk of its own.
//! - A code page's table is Windows': `0x81` in CP1252 is U+0081, where glibc rejects
//!   it, and `SHIFT_JIS` is Microsoft's variant (CP932).
//! - The shift-state encodings (ISO-2022-JP, ISO-2022-KR, HZ, UTF-7) are converted whole,
//!   so an error's position in them is approximate, and `-c` resumes after the bad byte
//!   as if the shift state were the initial one.
//! - Names Windows has and glibc does not (`CP20127`, `WINDOWS-932`, `MACROMAN`) are
//!   accepted; `-l` lists the names this builtin knows, one per line as glibc prints
//!   them into a pipe.

use std::io::{Read, Write};
use std::path::Path;

use cash_core::{ExecutionResult, builtins};
use cash_getopt::{Arg, Getopt, Item, Long, Short};
use cash_win32::codepage;
use clap::Parser;

/// Convert text from one character set to another.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct IconvCommand {
    /// Options and files, parsed here: glibc's grammar, with abbreviated long options.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

const TRY: &str = "Try `iconv --help' or `iconv --usage' for more information.";

/// What glibc exits with for a usage error.
const USAGE_ERROR: u8 = 64;

const HELP: &str = "Usage: iconv [OPTION...] [FILE...]
Convert encoding of given files from one encoding to another.

 Input/Output format specification:
  -f, --from-code=NAME       encoding of original text
  -t, --to-code=NAME         encoding for output

 Information:
  -l, --list                 list all known coded character sets

 Output control:
  -c                         omit invalid characters from output
  -o, --output=FILE          output file
  -s, --silent               suppress warnings
      --verbose              print progress information

  -?, --help                 Give this help list
      --usage                Give a short usage message
  -V, --version              Print program version

Mandatory or optional arguments to long options are also mandatory or optional
for any corresponding short options.

The character sets are Windows' code pages, by their iconv names (CP1252, LATIN1,
SHIFT_JIS, UTF-16 ...); `iconv -l` lists them, and `help iconv` has the details.
";

const USAGE: &str = "Usage: iconv [-lcs?V] [-f NAME] [-t NAME] [-o FILE] [--from-code=NAME]
            [--to-code=NAME] [--list] [--output=FILE] [--silent] [--verbose]
            [--help] [--usage] [--version] [FILE...]
";

const VERSION: &str = "iconv (cash): glibc 2.44's options, on the Windows code pages";

// ---------------------------------------------------------------------------
// Character sets
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Endian {
    Little,
    Big,
}

/// A character set a name stands for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Charset {
    Utf8,
    /// 7-bit ASCII; Windows' code page 20127 drops the high bit instead of objecting.
    Ascii,
    /// `None` is "either": a byte-order mark is read and consumed, and one is written.
    Utf16(Option<Endian>),
    /// UTF-16 without surrogate pairs; `None` is host (little-endian) order, no mark.
    Ucs2(Option<Endian>),
    Utf32(Option<Endian>),
    /// Like UTF-32 without a mark; glibc's `UCS-4` is big-endian and `WCHAR_T` the host's.
    Ucs4(Endian),
    /// A single-byte set Windows has no code page for: the characters of 0xA0..=0xFF.
    Table(&'static [u16; 96]),
    /// A Windows code page.
    CodePage(u32),
}

/// ISO-8859-10 (Latin-6, Nordic), from 0xA0.
const ISO_8859_10: &[u16; 96] = &[
    0x00a0, 0x0104, 0x0112, 0x0122, 0x012a, 0x0128, 0x0136, 0x00a7, 0x013b, 0x0110, 0x0160, 0x0166,
    0x017d, 0x00ad, 0x016a, 0x014a, 0x00b0, 0x0105, 0x0113, 0x0123, 0x012b, 0x0129, 0x0137, 0x00b7,
    0x013c, 0x0111, 0x0161, 0x0167, 0x017e, 0x2015, 0x016b, 0x014b, 0x0100, 0x00c1, 0x00c2, 0x00c3,
    0x00c4, 0x00c5, 0x00c6, 0x012e, 0x010c, 0x00c9, 0x0118, 0x00cb, 0x0116, 0x00cd, 0x00ce, 0x00cf,
    0x00d0, 0x0145, 0x014c, 0x00d3, 0x00d4, 0x00d5, 0x00d6, 0x0168, 0x00d8, 0x0172, 0x00da, 0x00db,
    0x00dc, 0x00dd, 0x00de, 0x00df, 0x0101, 0x00e1, 0x00e2, 0x00e3, 0x00e4, 0x00e5, 0x00e6, 0x012f,
    0x010d, 0x00e9, 0x0119, 0x00eb, 0x0117, 0x00ed, 0x00ee, 0x00ef, 0x00f0, 0x0146, 0x014d, 0x00f3,
    0x00f4, 0x00f5, 0x00f6, 0x0169, 0x00f8, 0x0173, 0x00fa, 0x00fb, 0x00fc, 0x00fd, 0x00fe, 0x0138,
];

/// ISO-8859-14 (Latin-8, Celtic), from 0xA0.
const ISO_8859_14: &[u16; 96] = &[
    0x00a0, 0x1e02, 0x1e03, 0x00a3, 0x010a, 0x010b, 0x1e0a, 0x00a7, 0x1e80, 0x00a9, 0x1e82, 0x1e0b,
    0x1ef2, 0x00ad, 0x00ae, 0x0178, 0x1e1e, 0x1e1f, 0x0120, 0x0121, 0x1e40, 0x1e41, 0x00b6, 0x1e56,
    0x1e81, 0x1e57, 0x1e83, 0x1e60, 0x1ef3, 0x1e84, 0x1e85, 0x1e61, 0x00c0, 0x00c1, 0x00c2, 0x00c3,
    0x00c4, 0x00c5, 0x00c6, 0x00c7, 0x00c8, 0x00c9, 0x00ca, 0x00cb, 0x00cc, 0x00cd, 0x00ce, 0x00cf,
    0x0174, 0x00d1, 0x00d2, 0x00d3, 0x00d4, 0x00d5, 0x00d6, 0x1e6a, 0x00d8, 0x00d9, 0x00da, 0x00db,
    0x00dc, 0x00dd, 0x0176, 0x00df, 0x00e0, 0x00e1, 0x00e2, 0x00e3, 0x00e4, 0x00e5, 0x00e6, 0x00e7,
    0x00e8, 0x00e9, 0x00ea, 0x00eb, 0x00ec, 0x00ed, 0x00ee, 0x00ef, 0x0175, 0x00f1, 0x00f2, 0x00f3,
    0x00f4, 0x00f5, 0x00f6, 0x1e6b, 0x00f8, 0x00f9, 0x00fa, 0x00fb, 0x00fc, 0x00fd, 0x0177, 0x00ff,
];

/// ISO-8859-16 (Latin-10, South-Eastern European), from 0xA0.
const ISO_8859_16: &[u16; 96] = &[
    0x00a0, 0x0104, 0x0105, 0x0141, 0x20ac, 0x201e, 0x0160, 0x00a7, 0x0161, 0x00a9, 0x0218, 0x00ab,
    0x0179, 0x00ad, 0x017a, 0x017b, 0x00b0, 0x00b1, 0x010c, 0x0142, 0x017d, 0x201d, 0x00b6, 0x00b7,
    0x017e, 0x010d, 0x0219, 0x00bb, 0x0152, 0x0153, 0x0178, 0x017c, 0x00c0, 0x00c1, 0x00c2, 0x0102,
    0x00c4, 0x0106, 0x00c6, 0x00c7, 0x00c8, 0x00c9, 0x00ca, 0x00cb, 0x00cc, 0x00cd, 0x00ce, 0x00cf,
    0x0110, 0x0143, 0x00d2, 0x00d3, 0x00d4, 0x0150, 0x00d6, 0x015a, 0x0170, 0x00d9, 0x00da, 0x00db,
    0x00dc, 0x0118, 0x021a, 0x00df, 0x00e0, 0x00e1, 0x00e2, 0x0103, 0x00e4, 0x0107, 0x00e6, 0x00e7,
    0x00e8, 0x00e9, 0x00ea, 0x00eb, 0x00ec, 0x00ed, 0x00ee, 0x00ef, 0x0111, 0x0144, 0x00f2, 0x00f3,
    0x00f4, 0x0151, 0x00f6, 0x015b, 0x0171, 0x00f9, 0x00fa, 0x00fb, 0x00fc, 0x0119, 0x021b, 0x00ff,
];

/// The names, upper case, and what each stands for. `-l` lists them; a `CPnnn`, `IBMnnn`,
/// `WINDOWS-nnn` or `MSnnn` not here is taken as code page `nnn` when Windows has it.
const NAMES: &[(&str, Charset)] = {
    use Charset::{Ascii, CodePage as Cp, Table, Ucs2, Ucs4, Utf8, Utf16, Utf32};
    use Endian::{Big, Little};
    &[
        ("UTF-8", Utf8),
        ("UTF8", Utf8),
        ("UTF-16", Utf16(None)),
        ("UTF16", Utf16(None)),
        ("UTF-16LE", Utf16(Some(Little))),
        ("UTF16LE", Utf16(Some(Little))),
        ("UTF-16BE", Utf16(Some(Big))),
        ("UTF16BE", Utf16(Some(Big))),
        ("UCS-2", Ucs2(None)),
        ("UCS2", Ucs2(None)),
        ("ISO-10646-UCS-2", Ucs2(None)),
        ("UCS-2LE", Ucs2(Some(Little))),
        ("UNICODELITTLE", Ucs2(Some(Little))),
        ("UCS-2BE", Ucs2(Some(Big))),
        ("UNICODEBIG", Ucs2(Some(Big))),
        ("UTF-32", Utf32(None)),
        ("UTF32", Utf32(None)),
        ("UTF-32LE", Utf32(Some(Little))),
        ("UTF32LE", Utf32(Some(Little))),
        ("UTF-32BE", Utf32(Some(Big))),
        ("UTF32BE", Utf32(Some(Big))),
        ("UCS-4", Ucs4(Big)),
        ("UCS4", Ucs4(Big)),
        ("ISO-10646-UCS-4", Ucs4(Big)),
        ("UCS-4BE", Ucs4(Big)),
        ("UCS-4LE", Ucs4(Little)),
        ("WCHAR_T", Ucs4(Little)),
        ("UTF-7", Cp(65000)),
        ("UTF7", Cp(65000)),
        ("ASCII", Ascii),
        ("US-ASCII", Ascii),
        ("ANSI_X3.4-1968", Ascii),
        ("ANSI_X3.4-1986", Ascii),
        ("ISO646-US", Ascii),
        ("ISO-IR-6", Ascii),
        ("US", Ascii),
        ("CSASCII", Ascii),
        ("646", Ascii),
        ("LATIN1", Cp(28591)),
        ("L1", Cp(28591)),
        ("ISO-8859-1", Cp(28591)),
        ("ISO8859-1", Cp(28591)),
        ("ISO_8859-1", Cp(28591)),
        ("ISO88591", Cp(28591)),
        ("CP819", Cp(28591)),
        ("IBM819", Cp(28591)),
        ("ISO-IR-100", Cp(28591)),
        ("LATIN2", Cp(28592)),
        ("L2", Cp(28592)),
        ("ISO-8859-2", Cp(28592)),
        ("ISO8859-2", Cp(28592)),
        ("ISO_8859-2", Cp(28592)),
        ("ISO88592", Cp(28592)),
        ("LATIN3", Cp(28593)),
        ("L3", Cp(28593)),
        ("ISO-8859-3", Cp(28593)),
        ("ISO8859-3", Cp(28593)),
        ("ISO_8859-3", Cp(28593)),
        ("ISO88593", Cp(28593)),
        ("LATIN4", Cp(28594)),
        ("L4", Cp(28594)),
        ("ISO-8859-4", Cp(28594)),
        ("ISO8859-4", Cp(28594)),
        ("ISO_8859-4", Cp(28594)),
        ("ISO88594", Cp(28594)),
        ("CYRILLIC", Cp(28595)),
        ("ISO-8859-5", Cp(28595)),
        ("ISO8859-5", Cp(28595)),
        ("ISO_8859-5", Cp(28595)),
        ("ISO88595", Cp(28595)),
        ("ARABIC", Cp(28596)),
        ("ISO-8859-6", Cp(28596)),
        ("ISO8859-6", Cp(28596)),
        ("ISO_8859-6", Cp(28596)),
        ("ISO88596", Cp(28596)),
        ("GREEK", Cp(28597)),
        ("GREEK8", Cp(28597)),
        ("ISO-8859-7", Cp(28597)),
        ("ISO8859-7", Cp(28597)),
        ("ISO_8859-7", Cp(28597)),
        ("ISO88597", Cp(28597)),
        ("HEBREW", Cp(28598)),
        ("ISO-8859-8", Cp(28598)),
        ("ISO8859-8", Cp(28598)),
        ("ISO_8859-8", Cp(28598)),
        ("ISO88598", Cp(28598)),
        ("LATIN5", Cp(28599)),
        ("L5", Cp(28599)),
        ("ISO-8859-9", Cp(28599)),
        ("ISO8859-9", Cp(28599)),
        ("ISO_8859-9", Cp(28599)),
        ("ISO88599", Cp(28599)),
        ("LATIN6", Table(ISO_8859_10)),
        ("L6", Table(ISO_8859_10)),
        ("ISO-8859-10", Table(ISO_8859_10)),
        ("ISO8859-10", Table(ISO_8859_10)),
        ("ISO_8859-10", Table(ISO_8859_10)),
        ("ISO885910", Table(ISO_8859_10)),
        ("ISO-8859-11", Cp(874)),
        ("ISO8859-11", Cp(874)),
        ("ISO_8859-11", Cp(874)),
        ("TIS-620", Cp(874)),
        ("TIS620", Cp(874)),
        ("LATIN7", Cp(28603)),
        ("L7", Cp(28603)),
        ("ISO-8859-13", Cp(28603)),
        ("ISO8859-13", Cp(28603)),
        ("ISO_8859-13", Cp(28603)),
        ("ISO885913", Cp(28603)),
        ("LATIN8", Table(ISO_8859_14)),
        ("L8", Table(ISO_8859_14)),
        ("ISO-8859-14", Table(ISO_8859_14)),
        ("ISO8859-14", Table(ISO_8859_14)),
        ("ISO_8859-14", Table(ISO_8859_14)),
        ("ISO885914", Table(ISO_8859_14)),
        ("LATIN9", Cp(28605)),
        ("L9", Cp(28605)),
        ("LATIN-9", Cp(28605)),
        ("ISO-8859-15", Cp(28605)),
        ("ISO8859-15", Cp(28605)),
        ("ISO_8859-15", Cp(28605)),
        ("ISO885915", Cp(28605)),
        ("LATIN10", Table(ISO_8859_16)),
        ("L10", Table(ISO_8859_16)),
        ("ISO-8859-16", Table(ISO_8859_16)),
        ("ISO8859-16", Table(ISO_8859_16)),
        ("ISO_8859-16", Table(ISO_8859_16)),
        ("ISO885916", Table(ISO_8859_16)),
        ("CP1250", Cp(1250)),
        ("WINDOWS-1250", Cp(1250)),
        ("MS-EE", Cp(1250)),
        ("CP1251", Cp(1251)),
        ("WINDOWS-1251", Cp(1251)),
        ("MS-CYRL", Cp(1251)),
        ("CP1252", Cp(1252)),
        ("WINDOWS-1252", Cp(1252)),
        ("MS-ANSI", Cp(1252)),
        ("CP1253", Cp(1253)),
        ("WINDOWS-1253", Cp(1253)),
        ("MS-GREEK", Cp(1253)),
        ("CP1254", Cp(1254)),
        ("WINDOWS-1254", Cp(1254)),
        ("MS-TURK", Cp(1254)),
        ("CP1255", Cp(1255)),
        ("WINDOWS-1255", Cp(1255)),
        ("MS-HEBR", Cp(1255)),
        ("CP1256", Cp(1256)),
        ("WINDOWS-1256", Cp(1256)),
        ("MS-ARAB", Cp(1256)),
        ("CP1257", Cp(1257)),
        ("WINDOWS-1257", Cp(1257)),
        ("WINBALTRIM", Cp(1257)),
        ("CP1258", Cp(1258)),
        ("WINDOWS-1258", Cp(1258)),
        ("CP437", Cp(437)),
        ("IBM437", Cp(437)),
        ("437", Cp(437)),
        ("CP737", Cp(737)),
        ("IBM737", Cp(737)),
        ("CP775", Cp(775)),
        ("IBM775", Cp(775)),
        ("CP850", Cp(850)),
        ("IBM850", Cp(850)),
        ("850", Cp(850)),
        ("CP852", Cp(852)),
        ("IBM852", Cp(852)),
        ("852", Cp(852)),
        ("CP855", Cp(855)),
        ("IBM855", Cp(855)),
        ("855", Cp(855)),
        ("CP857", Cp(857)),
        ("IBM857", Cp(857)),
        ("857", Cp(857)),
        ("CP858", Cp(858)),
        ("IBM858", Cp(858)),
        ("858", Cp(858)),
        ("CP860", Cp(860)),
        ("IBM860", Cp(860)),
        ("860", Cp(860)),
        ("CP861", Cp(861)),
        ("IBM861", Cp(861)),
        ("861", Cp(861)),
        ("CP862", Cp(862)),
        ("IBM862", Cp(862)),
        ("862", Cp(862)),
        ("CP863", Cp(863)),
        ("IBM863", Cp(863)),
        ("863", Cp(863)),
        ("CP864", Cp(864)),
        ("IBM864", Cp(864)),
        ("864", Cp(864)),
        ("CP865", Cp(865)),
        ("IBM865", Cp(865)),
        ("865", Cp(865)),
        ("CP866", Cp(866)),
        ("IBM866", Cp(866)),
        ("866", Cp(866)),
        ("CP869", Cp(869)),
        ("IBM869", Cp(869)),
        ("869", Cp(869)),
        ("CP874", Cp(874)),
        ("WINDOWS-874", Cp(874)),
        ("CP932", Cp(932)),
        ("SHIFT_JIS", Cp(932)),
        ("SHIFT-JIS", Cp(932)),
        ("SJIS", Cp(932)),
        ("MS_KANJI", Cp(932)),
        ("CSSHIFTJIS", Cp(932)),
        ("WINDOWS-31J", Cp(932)),
        ("MS932", Cp(932)),
        ("EUC-JP", Cp(20932)),
        ("EUCJP", Cp(20932)),
        ("ISO-2022-JP", Cp(50220)),
        ("CSISO2022JP", Cp(50220)),
        ("CP936", Cp(936)),
        ("GBK", Cp(936)),
        ("GB2312", Cp(936)),
        ("EUC-CN", Cp(936)),
        ("EUCCN", Cp(936)),
        ("MS936", Cp(936)),
        ("WINDOWS-936", Cp(936)),
        ("GB18030", Cp(54936)),
        ("BIG5", Cp(950)),
        ("BIG-5", Cp(950)),
        ("BIG-FIVE", Cp(950)),
        ("BIGFIVE", Cp(950)),
        ("CN-BIG5", Cp(950)),
        ("CP950", Cp(950)),
        ("CSBIG5", Cp(950)),
        ("HZ", Cp(52936)),
        ("HZ-GB-2312", Cp(52936)),
        ("EUC-KR", Cp(949)),
        ("EUCKR", Cp(949)),
        ("CP949", Cp(949)),
        ("UHC", Cp(949)),
        ("ISO-2022-KR", Cp(50225)),
        ("CSISO2022KR", Cp(50225)),
        ("JOHAB", Cp(1361)),
        ("CP1361", Cp(1361)),
        ("KOI8-R", Cp(20866)),
        ("CSKOI8R", Cp(20866)),
        ("KOI8-U", Cp(21866)),
        ("MACINTOSH", Cp(10000)),
        ("MAC", Cp(10000)),
        ("MACROMAN", Cp(10000)),
        ("CSMACINTOSH", Cp(10000)),
        ("MACGREEK", Cp(10006)),
        ("MACCYRILLIC", Cp(10007)),
        ("MACUKRAINE", Cp(10017)),
        ("MACCENTRALEUROPE", Cp(10029)),
        ("MACICELAND", Cp(10079)),
        ("MACTURKISH", Cp(10081)),
        ("EBCDIC-US", Cp(37)),
        ("IBM037", Cp(37)),
        ("CP037", Cp(37)),
        ("IBM500", Cp(500)),
        ("CP500", Cp(500)),
        ("IBM1047", Cp(1047)),
        ("CP1047", Cp(1047)),
    ]
};

/// The charset `name` stands for, in any case, or `None` when neither the table nor
/// Windows has it. An empty name is the default, UTF-8, as glibc takes it.
fn lookup(name: &str) -> Option<Charset> {
    let upper = name.to_ascii_uppercase();
    if upper.is_empty() {
        return Some(Charset::Utf8);
    }
    if let Some((_, charset)) = NAMES.iter().find(|(known, _)| *known == upper) {
        return Some(*charset);
    }
    let digits = ["WINDOWS-", "CP", "IBM", "MS"]
        .iter()
        .find_map(|prefix| upper.strip_prefix(prefix))?;
    if digits.is_empty() || digits.len() > 5 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let number: u32 = digits.parse().ok()?;
    Some(match number {
        65001 => Charset::Utf8,
        20127 => Charset::Ascii,
        1200 => Charset::Utf16(Some(Endian::Little)),
        1201 => Charset::Utf16(Some(Endian::Big)),
        12000 => Charset::Utf32(Some(Endian::Little)),
        12001 => Charset::Utf32(Some(Endian::Big)),
        number if codepage::is_valid(number) => Charset::CodePage(number),
        _ => return None,
    })
}

/// A `-f`/`-t` argument: the name and its `//TRANSLIT` and `//IGNORE` suffixes. Any
/// other suffix is passed over in silence, as glibc passes it.
#[derive(Debug, PartialEq, Eq)]
struct Request<'a> {
    name: &'a str,
    translit: bool,
    ignore: bool,
}

fn parse_request(spec: &str) -> Request<'_> {
    let mut parts = spec.split("//");
    let mut request = Request {
        name: parts.next().unwrap_or_default(),
        translit: false,
        ignore: false,
    };
    for suffix in parts {
        if suffix.eq_ignore_ascii_case("TRANSLIT") {
            request.translit = true;
        } else if suffix.eq_ignore_ascii_case("IGNORE") {
            request.ignore = true;
        }
    }
    request
}

/// The names `-l` lists, sorted, each once.
fn known_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = NAMES.iter().map(|(name, _)| *name).collect();
    names.sort_unstable();
    names.dedup();
    names
}

// ---------------------------------------------------------------------------
// Decoding: input bytes to UTF-16
// ---------------------------------------------------------------------------

/// A character set with its byte order settled and its mark consumed: what a decoder
/// runs on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scheme {
    Utf8,
    Ascii,
    /// 16-bit units; `pairs` says whether surrogate pairs are characters (UTF-16) or
    /// errors (UCS-2).
    Wide16 {
        endian: Endian,
        pairs: bool,
    },
    Wide32(Endian),
    Table(&'static [u16; 96]),
    /// A Windows code page. A `stateful` one (ISO-2022, HZ, UTF-7) has shift sequences,
    /// so it is only ever decoded whole.
    CodePage {
        page: u32,
        max_len: usize,
        stateful: bool,
    },
}

/// Whether a code page carries shift state between its characters.
const fn is_stateful(page: u32) -> bool {
    matches!(page, 50220..=50229 | 52936 | 57002..=57011 | 65000)
}

/// `charset` applied to `input`: the scheme, and where the text starts after a consumed
/// byte-order mark.
fn scheme_for(charset: Charset, input: &[u8]) -> (Scheme, usize) {
    match charset {
        Charset::Utf8 => (Scheme::Utf8, 0),
        Charset::Ascii => (Scheme::Ascii, 0),
        Charset::Utf16(Some(endian)) => (
            Scheme::Wide16 {
                endian,
                pairs: true,
            },
            0,
        ),
        Charset::Utf16(None) => {
            let (endian, mark) = if input.starts_with(b"\xFF\xFE") {
                (Endian::Little, 2)
            } else if input.starts_with(b"\xFE\xFF") {
                (Endian::Big, 2)
            } else {
                (Endian::Little, 0)
            };
            (
                Scheme::Wide16 {
                    endian,
                    pairs: true,
                },
                mark,
            )
        }
        Charset::Ucs2(endian) => (
            Scheme::Wide16 {
                endian: endian.unwrap_or(Endian::Little),
                pairs: false,
            },
            0,
        ),
        Charset::Utf32(Some(endian)) | Charset::Ucs4(endian) => (Scheme::Wide32(endian), 0),
        Charset::Utf32(None) => {
            if input.starts_with(b"\xFF\xFE\x00\x00") {
                (Scheme::Wide32(Endian::Little), 4)
            } else if input.starts_with(b"\x00\x00\xFE\xFF") {
                (Scheme::Wide32(Endian::Big), 4)
            } else {
                (Scheme::Wide32(Endian::Little), 0)
            }
        }
        Charset::Table(table) => (Scheme::Table(table), 0),
        Charset::CodePage(page) => (
            Scheme::CodePage {
                page,
                max_len: codepage::info(page)
                    .and_then(|info| usize::try_from(info.max_char_size).ok())
                    .unwrap_or(1)
                    .max(1),
                stateful: is_stateful(page),
            },
            0,
        ),
    }
}

/// What stopped a conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure {
    /// A sequence no character starts with, or a character the target lacks, at this
    /// byte offset in the input.
    Illegal(usize),
    /// The input ends inside a character.
    Incomplete,
}

/// One step of a decoder: the character at a position, or why there is none.
#[derive(Debug, PartialEq, Eq)]
enum Piece {
    /// A character: its UTF-16 units (`count` of them) and the input bytes it took.
    Char {
        units: [u16; 4],
        count: usize,
        len: usize,
    },
    /// Bytes no character starts with; `len` is how many `-c` skips.
    Illegal { len: usize },
    /// The input ends inside a character.
    Incomplete,
}

const fn one_unit(unit: u16, len: usize) -> Piece {
    Piece::Char {
        units: [unit, 0, 0, 0],
        count: 1,
        len,
    }
}

const fn scalar(c: char, len: usize) -> Piece {
    let mut units = [0u16; 4];
    let count = c.encode_utf16(&mut units).len();
    Piece::Char { units, count, len }
}

fn read16(bytes: &[u8], at: usize, endian: Endian) -> Option<u16> {
    let pair = [*bytes.get(at)?, *bytes.get(at + 1)?];
    Some(match endian {
        Endian::Little => u16::from_le_bytes(pair),
        Endian::Big => u16::from_be_bytes(pair),
    })
}

fn read32(bytes: &[u8], at: usize, endian: Endian) -> Option<u32> {
    let quad = [
        *bytes.get(at)?,
        *bytes.get(at + 1)?,
        *bytes.get(at + 2)?,
        *bytes.get(at + 3)?,
    ];
    Some(match endian {
        Endian::Little => u32::from_le_bytes(quad),
        Endian::Big => u32::from_be_bytes(quad),
    })
}

/// The character of `scheme` at `input[at..]`, which must not be empty. Not for a
/// stateful code page.
fn piece(scheme: Scheme, input: &[u8], at: usize) -> Piece {
    let rest = input.get(at..).unwrap_or_default();
    let Some(&first) = rest.first() else {
        return Piece::Incomplete;
    };
    match scheme {
        Scheme::Utf8 => {
            // A character is at most four bytes, so a four-byte window holds it whole
            // whenever the input does.
            let window = rest.get(..4).unwrap_or(rest);
            let (valid, error_len) = match std::str::from_utf8(window) {
                Ok(text) => (text, None),
                Err(error) => (
                    std::str::from_utf8(window.get(..error.valid_up_to()).unwrap_or_default())
                        .unwrap_or_default(),
                    Some(error.error_len()),
                ),
            };
            if let Some(c) = valid.chars().next() {
                return scalar(c, c.len_utf8());
            }
            match error_len {
                Some(Some(len)) => Piece::Illegal { len },
                _ => Piece::Incomplete,
            }
        }
        Scheme::Ascii => {
            if first < 0x80 {
                one_unit(u16::from(first), 1)
            } else {
                Piece::Illegal { len: 1 }
            }
        }
        Scheme::Wide16 { endian, pairs } => {
            let Some(unit) = read16(rest, 0, endian) else {
                return Piece::Incomplete;
            };
            match unit {
                0xD800..=0xDBFF if pairs => match read16(rest, 2, endian) {
                    Some(low @ 0xDC00..=0xDFFF) => Piece::Char {
                        units: [unit, low, 0, 0],
                        count: 2,
                        len: 4,
                    },
                    Some(_) => Piece::Illegal { len: 2 },
                    None => Piece::Incomplete,
                },
                0xD800..=0xDFFF => Piece::Illegal { len: 2 },
                _ => one_unit(unit, 2),
            }
        }
        Scheme::Wide32(endian) => {
            let Some(value) = read32(rest, 0, endian) else {
                return Piece::Incomplete;
            };
            char::from_u32(value).map_or(Piece::Illegal { len: 4 }, |c| scalar(c, 4))
        }
        Scheme::Table(table) => {
            if first < 0xA0 {
                return one_unit(u16::from(first), 1);
            }
            match table.get(usize::from(first - 0xA0)) {
                Some(&unit) if unit != 0 => one_unit(unit, 1),
                _ => Piece::Illegal { len: 1 },
            }
        }
        Scheme::CodePage { page, max_len, .. } => {
            for len in 1..=max_len.min(rest.len()) {
                if let Ok(units) = codepage::decode(page, rest.get(..len).unwrap_or_default(), true)
                {
                    let mut buffer = [0u16; 4];
                    let count = units.len().min(buffer.len());
                    for (slot, unit) in buffer.iter_mut().zip(&units) {
                        *slot = *unit;
                    }
                    return Piece::Char {
                        units: buffer,
                        count,
                        len,
                    };
                }
            }
            if rest.len() < max_len && codepage::is_lead_byte(page, first) {
                Piece::Incomplete
            } else {
                Piece::Illegal { len: 1 }
            }
        }
    }
}

/// `text`, all of it, as UTF-16, or `None` when some of it is not a character.
fn decode_whole(scheme: Scheme, text: &[u8]) -> Option<Vec<u16>> {
    match scheme {
        Scheme::Utf8 => std::str::from_utf8(text)
            .ok()
            .map(|valid| valid.encode_utf16().collect()),
        Scheme::Ascii => text
            .iter()
            .map(|&b| (b < 0x80).then_some(u16::from(b)))
            .collect(),
        Scheme::Wide16 { endian, pairs } => {
            if !text.len().is_multiple_of(2) {
                return None;
            }
            let mut units = Vec::with_capacity(text.len() / 2);
            let mut high: Option<u16> = None;
            for pair in text.as_chunks::<2>().0 {
                let unit = match endian {
                    Endian::Little => u16::from_le_bytes(*pair),
                    Endian::Big => u16::from_be_bytes(*pair),
                };
                match (high.take(), unit) {
                    (Some(lead), 0xDC00..=0xDFFF) => {
                        units.push(lead);
                        units.push(unit);
                    }
                    (Some(_), _) | (None, 0xDC00..=0xDFFF) => return None,
                    (None, 0xD800..=0xDBFF) if pairs => high = Some(unit),
                    (None, 0xD800..=0xDBFF) => return None,
                    (None, _) => units.push(unit),
                }
            }
            high.is_none().then_some(units)
        }
        Scheme::Wide32(endian) => {
            if !text.len().is_multiple_of(4) {
                return None;
            }
            let mut units = Vec::with_capacity(text.len() / 4);
            for quad in text.as_chunks::<4>().0 {
                let value = match endian {
                    Endian::Little => u32::from_le_bytes(*quad),
                    Endian::Big => u32::from_be_bytes(*quad),
                };
                let mut buffer = [0u16; 2];
                units.extend_from_slice(char::from_u32(value)?.encode_utf16(&mut buffer));
            }
            Some(units)
        }
        Scheme::Table(table) => text
            .iter()
            .map(|&b| {
                if b < 0xA0 {
                    Some(u16::from(b))
                } else {
                    table
                        .get(usize::from(b - 0xA0))
                        .copied()
                        .filter(|&unit| unit != 0)
                }
            })
            .collect(),
        Scheme::CodePage { page, .. } => match codepage::decode(page, text, true) {
            Ok(units) => Some(units),
            Err(codepage::DecodeError::FlagsUnsupported) => {
                codepage::decode(page, text, false).ok()
            }
            Err(_) => None,
        },
    }
}

/// The input decoded as far as it goes.
#[derive(Debug)]
struct Decoded {
    scheme: Scheme,
    /// Where the text starts, after a consumed byte-order mark.
    body: usize,
    units: Vec<u16>,
    /// The input offset each unit's character starts at, when the input was walked.
    offsets: Option<Vec<usize>>,
    failure: Option<Failure>,
    /// Whether `-c` dropped something.
    dropped: bool,
}

/// `input` in `charset` as UTF-16, stopping at the first problem, or skipping it when
/// `discard`.
fn decode(charset: Charset, input: &[u8], discard: bool) -> Decoded {
    let (scheme, body) = scheme_for(charset, input);
    let text = input.get(body..).unwrap_or_default();
    if let Some(units) = decode_whole(scheme, text) {
        return Decoded {
            scheme,
            body,
            units,
            offsets: None,
            failure: None,
            dropped: false,
        };
    }
    if let Scheme::CodePage {
        page,
        stateful: true,
        ..
    } = scheme
    {
        let (units, failure, dropped) = decode_stateful(page, input, body, discard);
        return Decoded {
            scheme,
            body,
            units,
            offsets: None,
            failure,
            dropped,
        };
    }

    let mut units = Vec::new();
    let mut offsets = Vec::new();
    let mut failure = None;
    let mut dropped = false;
    let mut at = body;
    while at < input.len() {
        match piece(scheme, input, at) {
            Piece::Char {
                units: new,
                count,
                len,
            } => {
                units.extend_from_slice(new.get(..count).unwrap_or_default());
                offsets.resize(offsets.len() + count, at);
                at += len;
            }
            Piece::Illegal { len } if discard => {
                dropped = true;
                at += len;
            }
            Piece::Illegal { .. } => {
                failure = Some(Failure::Illegal(at));
                break;
            }
            Piece::Incomplete => {
                failure = Some(Failure::Incomplete);
                break;
            }
        }
    }
    Decoded {
        scheme,
        body,
        units,
        offsets: Some(offsets),
        failure,
        dropped,
    }
}

/// A shift-state code page is decoded whole, so its first bad byte is found by bisection
/// on the prefixes that decode (a prefix cut inside a character is taken as good when a
/// few more bytes complete it). With `-c`, the rest is decoded afresh from the byte after
/// it, as if the shift state were the initial one.
fn decode_stateful(
    page: u32,
    input: &[u8],
    body: usize,
    discard: bool,
) -> (Vec<u16>, Option<Failure>, bool) {
    let mut units = Vec::new();
    let mut dropped = false;
    let mut start = body;
    loop {
        let text = input.get(start..).unwrap_or_default();
        match codepage::decode(page, text, true) {
            Ok(decoded) => {
                units.extend(decoded);
                return (units, None, dropped);
            }
            Err(codepage::DecodeError::Invalid) => {}
            Err(_) => {
                units.extend(codepage::decode(page, text, false).unwrap_or_default());
                return (units, None, dropped);
            }
        }
        let good = |prefix: usize| {
            (prefix..=(prefix + 8).min(text.len())).any(|end| {
                codepage::decode(page, text.get(..end).unwrap_or_default(), true).is_ok()
            })
        };
        let (mut low, mut high) = (0, text.len());
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if good(middle) {
                low = middle;
            } else {
                high = middle;
            }
        }
        units.extend(
            codepage::decode(page, text.get(..low).unwrap_or_default(), true).unwrap_or_default(),
        );
        if !discard {
            return (units, Some(Failure::Illegal(start + low)), dropped);
        }
        dropped = true;
        start += low + 1;
    }
}

impl Decoded {
    /// The input offset of the character that unit `index` belongs to.
    fn offset_of_unit(&self, input: &[u8], index: usize) -> usize {
        if let Some(offsets) = &self.offsets {
            return offsets.get(index).copied().unwrap_or(input.len());
        }
        if let Scheme::CodePage {
            page,
            stateful: true,
            ..
        } = self.scheme
        {
            // The shortest prefix that yields the character ends with its last byte.
            let text = input.get(self.body..).unwrap_or_default();
            let yields = |prefix: usize| {
                codepage::decode(page, text.get(..prefix).unwrap_or_default(), false)
                    .map_or(0, |units| units.len())
            };
            let (mut low, mut high) = (0, text.len());
            while high - low > 1 {
                let middle = low + (high - low) / 2;
                if yields(middle) > index {
                    high = middle;
                } else {
                    low = middle;
                }
            }
            return self.body + low;
        }
        let mut at = self.body;
        let mut seen = 0;
        while at < input.len() {
            match piece(self.scheme, input, at) {
                Piece::Char { count, len, .. } => {
                    if seen + count > index {
                        return at;
                    }
                    seen += count;
                    at += len;
                }
                Piece::Illegal { len } => at += len,
                Piece::Incomplete => break,
            }
        }
        input.len()
    }
}

// ---------------------------------------------------------------------------
// Encoding: UTF-16 to output bytes
// ---------------------------------------------------------------------------

/// What to do with a character the target has no mapping for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Unconvertible {
    /// Stop there: glibc's "illegal input sequence".
    Fail,
    /// Leave it out, and exit 1 at the end: `-c` and `//IGNORE`.
    Drop,
    /// Windows' best fit, or `?`: `//TRANSLIT`.
    Approximate,
}

/// The output of an encoder.
#[derive(Debug, Default, PartialEq, Eq)]
struct Encoded {
    bytes: Vec<u8>,
    /// The index of the first unit the target has no character for, with the output up to
    /// it in `bytes`.
    failed_at: Option<usize>,
    dropped: bool,
}

/// Runs `put` on each character of `units` in turn; `put` writes the character or returns
/// false for one the target lacks, which is then treated as `on` says, with `substitute`
/// standing for it under `Approximate`.
fn encode_each(
    units: &[u16],
    on: Unconvertible,
    substitute: &[u8],
    mut put: impl FnMut(char, &mut Vec<u8>) -> bool,
) -> Encoded {
    let mut encoded = Encoded::default();
    let mut at = 0;
    for item in char::decode_utf16(units.iter().copied()) {
        let (fits, len) = match item {
            Ok(c) => {
                let mark = encoded.bytes.len();
                let fits = put(c, &mut encoded.bytes);
                if !fits {
                    encoded.bytes.truncate(mark);
                }
                (fits, c.len_utf16())
            }
            Err(_) => (false, 1),
        };
        if !fits {
            match on {
                Unconvertible::Fail => {
                    encoded.failed_at = Some(at);
                    return encoded;
                }
                Unconvertible::Drop => encoded.dropped = true,
                Unconvertible::Approximate => encoded.bytes.extend_from_slice(substitute),
            }
        }
        at += len;
    }
    encoded
}

fn push16(out: &mut Vec<u8>, unit: u16, endian: Endian) {
    out.extend_from_slice(&match endian {
        Endian::Little => unit.to_le_bytes(),
        Endian::Big => unit.to_be_bytes(),
    });
}

fn push32(out: &mut Vec<u8>, value: u32, endian: Endian) {
    out.extend_from_slice(&match endian {
        Endian::Little => value.to_le_bytes(),
        Endian::Big => value.to_be_bytes(),
    });
}

/// `units` in `charset`.
fn encode(charset: Charset, units: &[u16], on: Unconvertible) -> Encoded {
    match charset {
        Charset::Utf8 => encode_each(units, on, b"?", |c, out| {
            let mut buffer = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
            true
        }),
        Charset::Ascii => {
            if on == Unconvertible::Approximate {
                // Windows' best fit for US-ASCII (`é` as `e`), then its `?`.
                return codepage::encode(20127, units, true)
                    .map(|encoded| Encoded {
                        bytes: encoded.bytes,
                        failed_at: None,
                        dropped: false,
                    })
                    .unwrap_or_default();
            }
            encode_each(units, on, b"?", |c, out| {
                if c.is_ascii() {
                    out.push(c as u8);
                    true
                } else {
                    false
                }
            })
        }
        Charset::Utf16(order) => {
            let endian = order.unwrap_or(Endian::Little);
            let mut substitute = Vec::new();
            push16(&mut substitute, u16::from(b'?'), endian);
            let mut encoded = encode_each(units, on, &substitute, |c, out| {
                let mut buffer = [0u16; 2];
                for &unit in c.encode_utf16(&mut buffer).iter() {
                    push16(out, unit, endian);
                }
                true
            });
            if order.is_none() && !units.is_empty() {
                let mut marked = Vec::with_capacity(encoded.bytes.len() + 2);
                push16(&mut marked, 0xFEFF, endian);
                marked.append(&mut encoded.bytes);
                encoded.bytes = marked;
            }
            encoded
        }
        Charset::Ucs2(order) => {
            let endian = order.unwrap_or(Endian::Little);
            let mut substitute = Vec::new();
            push16(&mut substitute, u16::from(b'?'), endian);
            encode_each(units, on, &substitute, |c, out| {
                u16::try_from(u32::from(c)).is_ok_and(|unit| {
                    push16(out, unit, endian);
                    true
                })
            })
        }
        Charset::Utf32(order) => {
            let endian = order.unwrap_or(Endian::Little);
            let mut substitute = Vec::new();
            push32(&mut substitute, u32::from(b'?'), endian);
            let mut encoded = encode_each(units, on, &substitute, |c, out| {
                push32(out, u32::from(c), endian);
                true
            });
            if order.is_none() && !units.is_empty() {
                let mut marked = Vec::with_capacity(encoded.bytes.len() + 4);
                push32(&mut marked, 0xFEFF, endian);
                marked.append(&mut encoded.bytes);
                encoded.bytes = marked;
            }
            encoded
        }
        Charset::Ucs4(endian) => {
            let mut substitute = Vec::new();
            push32(&mut substitute, u32::from(b'?'), endian);
            encode_each(units, on, &substitute, |c, out| {
                push32(out, u32::from(c), endian);
                true
            })
        }
        Charset::Table(table) => encode_each(units, on, b"?", |c, out| {
            let value = u32::from(c);
            if let Ok(byte @ ..0xA0) = u8::try_from(value) {
                out.push(byte);
                return true;
            }
            let Ok(unit) = u16::try_from(value) else {
                return false;
            };
            table
                .iter()
                .position(|&entry| entry == unit && unit != 0)
                .and_then(|index| u8::try_from(index).ok())
                .is_some_and(|index| {
                    out.push(0xA0 + index);
                    true
                })
        }),
        Charset::CodePage(page) => encode_code_page(page, units, on),
    }
}

/// `units` in a Windows code page: whole when every character has a mapping, else chunk by
/// chunk and character by character to find the ones that do not.
fn encode_code_page(page: u32, units: &[u16], on: Unconvertible) -> Encoded {
    let best_fit = on == Unconvertible::Approximate;
    let Ok(whole) = codepage::encode(page, units, best_fit) else {
        return Encoded {
            bytes: Vec::new(),
            failed_at: Some(0),
            dropped: false,
        };
    };
    match whole.lossy {
        Some(true) if !best_fit => encode_by_chunks(page, units, on),
        None if !best_fit => verify_round_trip(page, units, whole.bytes, on),
        _ => Encoded {
            bytes: whole.bytes,
            failed_at: None,
            dropped: false,
        },
    }
}

/// Whether `page` encodes `units` without using its default character.
fn encodes_exactly(page: u32, units: &[u16]) -> Option<Vec<u8>> {
    codepage::encode(page, units, false)
        .ok()
        .filter(|encoded| encoded.lossy == Some(false))
        .map(|encoded| encoded.bytes)
}

/// The characters of `units` one at a time, with the unit index each starts at.
fn each_char(units: &[u16]) -> impl Iterator<Item = (usize, Option<char>, usize)> + '_ {
    let mut at = 0;
    char::decode_utf16(units.iter().copied()).map(move |item| {
        let start = at;
        let (c, len) = match item {
            Ok(c) => (Some(c), c.len_utf16()),
            Err(_) => (None, 1),
        };
        at += len;
        (start, c, len)
    })
}

/// Encoding in chunks, and a chunk with a lost character one character at a time.
fn encode_by_chunks(page: u32, units: &[u16], on: Unconvertible) -> Encoded {
    const CHUNK: usize = 1024;
    let mut encoded = Encoded::default();
    let mut start = 0;
    while start < units.len() {
        let mut end = (start + CHUNK).min(units.len());
        if end < units.len() && matches!(units.get(end - 1), Some(0xD800..=0xDBFF)) {
            end -= 1;
        }
        let chunk = units.get(start..end).unwrap_or_default();
        if let Some(bytes) = encodes_exactly(page, chunk) {
            encoded.bytes.extend(bytes);
        } else {
            for (offset, c, _) in each_char(chunk) {
                let mut buffer = [0u16; 2];
                let one = c.map_or(0, |c| c.encode_utf16(&mut buffer).len());
                let fitted = encodes_exactly(page, buffer.get(..one).unwrap_or_default())
                    .filter(|_| one > 0);
                match fitted {
                    Some(bytes) => encoded.bytes.extend(bytes),
                    None => match on {
                        Unconvertible::Fail => {
                            encoded.failed_at = Some(start + offset);
                            return encoded;
                        }
                        Unconvertible::Drop => encoded.dropped = true,
                        Unconvertible::Approximate => encoded.bytes.push(b'?'),
                    },
                }
            }
        }
        start = end;
    }
    encoded
}

/// For a code page that cannot say whether it used its default character: decode the
/// result and compare. On a difference, the characters that survive a round trip on their
/// own are kept and encoded together, since the page may carry shift state between them.
fn verify_round_trip(page: u32, units: &[u16], bytes: Vec<u8>, on: Unconvertible) -> Encoded {
    if codepage::decode(page, &bytes, false).is_ok_and(|back| back == units) {
        return Encoded {
            bytes,
            failed_at: None,
            dropped: false,
        };
    }
    let whole = |kept: &[u16]| codepage::encode(page, kept, false).map_or(Vec::new(), |e| e.bytes);
    let mut kept: Vec<u16> = Vec::with_capacity(units.len());
    let mut dropped = false;
    for (offset, c, _) in each_char(units) {
        let mut buffer = [0u16; 2];
        let one = c.map_or(0, |c| c.encode_utf16(&mut buffer).len());
        let character = buffer.get(..one).unwrap_or_default();
        let survives = one > 0
            && codepage::encode(page, character, false)
                .ok()
                .and_then(|encoded| codepage::decode(page, &encoded.bytes, false).ok())
                .is_some_and(|back| back == character);
        if survives {
            kept.extend_from_slice(character);
            continue;
        }
        match on {
            Unconvertible::Fail => {
                return Encoded {
                    bytes: whole(&kept),
                    failed_at: Some(offset),
                    dropped,
                };
            }
            Unconvertible::Drop => dropped = true,
            Unconvertible::Approximate => kept.push(u16::from(b'?')),
        }
    }
    Encoded {
        bytes: whole(&kept),
        failed_at: None,
        dropped,
    }
}

// ---------------------------------------------------------------------------
// A conversion
// ---------------------------------------------------------------------------

/// How `-c`, `//IGNORE` and `//TRANSLIT` shape a conversion.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Mode {
    discard: bool,
    translit: bool,
}

/// What converting one input produced.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    /// The output, up to the failure if there was one.
    bytes: Vec<u8>,
    failure: Option<Failure>,
    /// Whether something was left out, which makes the status 1.
    dropped: bool,
}

fn convert(from: Charset, to: Charset, mode: Mode, input: &[u8]) -> Outcome {
    let decoded = decode(from, input, mode.discard);
    let on = if mode.translit {
        Unconvertible::Approximate
    } else if mode.discard {
        Unconvertible::Drop
    } else {
        Unconvertible::Fail
    };
    let encoded = encode(to, &decoded.units, on);
    let failure = match encoded.failed_at {
        Some(index) => Some(Failure::Illegal(decoded.offset_of_unit(input, index))),
        None => decoded.failure,
    };
    Outcome {
        bytes: encoded.bytes,
        failure,
        dropped: decoded.dropped || encoded.dropped,
    }
}

// ---------------------------------------------------------------------------
// The command line
// ---------------------------------------------------------------------------

#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    from: String,
    to: String,
    discard: bool,
    output: Option<String>,
    list: bool,
    verbose: bool,
    files: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum Parsed {
    Run(Options),
    Help,
    Usage,
    Version,
    /// A usage error, worded; the status is 64.
    Fail(String),
}

/// The long options, each known by its name, as glibc's argp reads them.
const LONG_OPTIONS: &[Long<'static, &str>] = &[
    Long::new("from-code", Arg::Required, "from-code"),
    Long::new("to-code", Arg::Required, "to-code"),
    Long::new("output", Arg::Required, "output"),
    Long::new("list", Arg::No, "list"),
    Long::new("silent", Arg::No, "silent"),
    Long::new("verbose", Arg::No, "verbose"),
    Long::new("help", Arg::No, "help"),
    Long::new("usage", Arg::No, "usage"),
    Long::new("version", Arg::No, "version"),
];

/// The short options, each known by the long option it stands for; `-c` has none.
const SHORT_OPTIONS: &[Short<&str>] = &[
    Short::new('c', Arg::No, "discard"),
    Short::new('s', Arg::No, "silent"),
    Short::new('l', Arg::No, "list"),
    Short::new('?', Arg::No, "help"),
    Short::new('V', Arg::No, "version"),
    Short::new('f', Arg::Required, "from-code"),
    Short::new('t', Arg::Required, "to-code"),
    Short::new('o', Arg::Required, "output"),
];

/// Applies an option; `Some` is an early exit.
fn apply(options: &mut Options, name: &str, value: Option<String>) -> Option<Parsed> {
    match name {
        "from-code" => options.from = value.unwrap_or_default(),
        "to-code" => options.to = value.unwrap_or_default(),
        "output" => options.output = value,
        "list" => options.list = true,
        "discard" => options.discard = true,
        "silent" => {}
        "verbose" => options.verbose = true,
        "help" => return Some(Parsed::Help),
        "usage" => return Some(Parsed::Usage),
        "version" => return Some(Parsed::Version),
        _ => {}
    }
    None
}

/// Reads the command line as glibc's argp does, on `getopt_long` (`cash-getopt`).
fn parse(args: &[String]) -> Parsed {
    let mut options = Options::default();
    for next in Getopt::new(SHORT_OPTIONS, LONG_OPTIONS).read(args) {
        match next {
            Ok(Item::Option { id, value, .. }) => {
                if let Some(early) = apply(&mut options, id, value) {
                    return early;
                }
            }
            Ok(Item::Operand { value, .. }) => options.files.push(value),
            Err(problem) => return Parsed::Fail(format!("iconv: {problem}")),
        }
    }
    Parsed::Run(options)
}

/// The message for names that are not supported, worded as glibc words it.
fn unsupported(from: Option<Charset>, to: Option<Charset>, options: &Options) -> String {
    match (from, to) {
        (None, None) => format!(
            "conversions from `{}' and to `{}' are not supported",
            options.from, options.to
        ),
        (None, Some(_)) => format!("conversion from `{}' is not supported", options.from),
        _ => format!("conversion to `{}' is not supported", options.to),
    }
}

impl builtins::Command for IconvCommand {
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
        reason = "one pass over the inputs, kept together"
    )]
    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();
        let mut stderr = context.stderr();
        let options = match parse(&self.args) {
            Parsed::Run(options) => options,
            Parsed::Help => {
                stdout.write_all(HELP.as_bytes())?;
                return Ok(ExecutionResult::success());
            }
            Parsed::Usage => {
                stdout.write_all(USAGE.as_bytes())?;
                return Ok(ExecutionResult::success());
            }
            Parsed::Version => {
                writeln!(stdout, "{VERSION}")?;
                return Ok(ExecutionResult::success());
            }
            Parsed::Fail(message) => {
                writeln!(stderr, "{message}\n{TRY}")?;
                return Ok(ExecutionResult::new(USAGE_ERROR));
            }
        };

        if options.list {
            let mut listing = String::new();
            for name in known_names() {
                listing.push_str(name);
                listing.push_str("//\n");
            }
            stdout.write_all(listing.as_bytes())?;
            return Ok(ExecutionResult::success());
        }

        let from_request = parse_request(&options.from);
        let to_request = parse_request(&options.to);
        let (from, to) = (lookup(from_request.name), lookup(to_request.name));
        let (Some(from), Some(to)) = (from, to) else {
            writeln!(stderr, "iconv: {}\n{TRY}", unsupported(from, to, &options))?;
            return Ok(ExecutionResult::general_error());
        };
        let mode = Mode {
            discard: options.discard || to_request.ignore,
            translit: to_request.translit,
        };

        // Opened first, as glibc opens it: a bad output path is reported before anything
        // is read.
        let mut output_file = match &options.output {
            Some(path) if path != "-" => {
                match std::fs::File::create(context.shell.absolute_path(Path::new(path))) {
                    Ok(file) => Some(file),
                    Err(error) => {
                        let reason = cash_core::error::os_error_text(&error);
                        writeln!(stderr, "iconv: cannot open output file: {reason}")?;
                        return Ok(ExecutionResult::general_error());
                    }
                }
            }
            _ => None,
        };

        let named = !options.files.is_empty();
        let inputs = if named {
            options.files.clone()
        } else {
            vec!["-".to_owned()]
        };
        let mut status = 0;
        // Written once at the end, as glibc's buffered output is: into a pipe, a message
        // comes before any of the output.
        let mut output = Vec::new();
        for name in &inputs {
            if options.verbose && named {
                writeln!(stderr, "{name}:")?;
            }
            let input = if name == "-" {
                let mut bytes = Vec::new();
                context.stdin().read_to_end(&mut bytes)?;
                bytes
            } else {
                let path = context.shell.absolute_path(Path::new(name));
                if path.is_dir() {
                    writeln!(
                        stderr,
                        "iconv: error while reading the input: Is a directory"
                    )?;
                    status = 1;
                    break;
                }
                match std::fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        let reason = cash_core::error::os_error_text(&error);
                        writeln!(stderr, "iconv: cannot open input file `{name}': {reason}")?;
                        status = 1;
                        continue;
                    }
                }
            };
            let outcome = convert(from, to, mode, &input);
            output.extend(outcome.bytes);
            if outcome.dropped {
                status = 1;
            }
            match outcome.failure {
                None => {}
                Some(Failure::Illegal(position)) => {
                    writeln!(
                        stderr,
                        "iconv: illegal input sequence at position {position}"
                    )?;
                    status = 1;
                    break;
                }
                Some(Failure::Incomplete) => {
                    writeln!(
                        stderr,
                        "iconv: incomplete character or shift sequence at end of buffer"
                    )?;
                    status = 1;
                    break;
                }
            }
        }

        match output_file.as_mut() {
            Some(file) => {
                if let Err(error) = file.write_all(&output) {
                    let reason = cash_core::error::os_error_text(&error);
                    writeln!(
                        stderr,
                        "iconv: conversion stopped due to problem in writing the output: {reason}"
                    )?;
                    return Ok(ExecutionResult::general_error());
                }
            }
            None => stdout.write_all(&output)?,
        }
        Ok(ExecutionResult::new(status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(from: &str, to: &str, discard: bool, input: &[u8]) -> Outcome {
        let to_request = parse_request(to);
        let (Some(from), Some(to)) = (lookup(from), lookup(to_request.name)) else {
            panic!("unknown charset in {from} or {to}");
        };
        convert(
            from,
            to,
            Mode {
                discard: discard || to_request.ignore,
                translit: to_request.translit,
            },
            input,
        )
    }

    fn ok(bytes: &[u8]) -> Outcome {
        Outcome {
            bytes: bytes.to_vec(),
            failure: None,
            dropped: false,
        }
    }

    #[test]
    fn names_are_looked_up_in_any_case_with_aliases() {
        assert_eq!(lookup("cp1252"), Some(Charset::CodePage(1252)));
        assert_eq!(lookup("WINDOWS-1252"), Some(Charset::CodePage(1252)));
        assert_eq!(lookup("latin1"), Some(Charset::CodePage(28591)));
        assert_eq!(lookup("ISO_8859-1"), Some(Charset::CodePage(28591)));
        assert_eq!(lookup("l1"), Some(Charset::CodePage(28591)));
        assert_eq!(lookup("ISO-8859-15"), Some(Charset::CodePage(28605)));
        assert_eq!(lookup("ISO-8859-13"), Some(Charset::CodePage(28603)));
        assert_eq!(lookup("utf8"), Some(Charset::Utf8));
        assert_eq!(lookup(""), Some(Charset::Utf8));
        assert_eq!(lookup("US-ASCII"), Some(Charset::Ascii));
        assert_eq!(lookup("646"), Some(Charset::Ascii));
        assert_eq!(lookup("Utf-16"), Some(Charset::Utf16(None)));
        assert_eq!(lookup("UTF-16BE"), Some(Charset::Utf16(Some(Endian::Big))));
        assert_eq!(lookup("UCS-4"), Some(Charset::Ucs4(Endian::Big)));
        assert_eq!(lookup("sjis"), Some(Charset::CodePage(932)));
        assert_eq!(lookup("GB2312"), Some(Charset::CodePage(936)));
        assert_eq!(lookup("ISO-8859-10"), Some(Charset::Table(ISO_8859_10)));
        assert_eq!(lookup("LATIN10"), Some(Charset::Table(ISO_8859_16)));
        // Generic numbers go to Windows, when it has the code page.
        assert_eq!(lookup("cp65001"), Some(Charset::Utf8));
        assert_eq!(lookup("CP20127"), Some(Charset::Ascii));
        assert_eq!(lookup("ibm1140"), Some(Charset::CodePage(1140)));
        assert_eq!(lookup("CP1200"), Some(Charset::Utf16(Some(Endian::Little))));
        assert_eq!(lookup("CP99999"), None);
        assert_eq!(lookup("NOSUCH"), None);
        assert_eq!(lookup("CP"), None);
        assert_eq!(lookup("CP12x"), None);
    }

    #[test]
    fn suffixes_are_stripped_from_a_request() {
        assert_eq!(
            parse_request("LATIN1//TRANSLIT"),
            Request {
                name: "LATIN1",
                translit: true,
                ignore: false
            }
        );
        assert_eq!(
            parse_request("ascii//translit//IGNORE"),
            Request {
                name: "ascii",
                translit: true,
                ignore: true
            }
        );
        assert_eq!(
            parse_request("UTF-8//"),
            Request {
                name: "UTF-8",
                translit: false,
                ignore: false
            }
        );
        assert_eq!(parse_request("CP437//FOO").name, "CP437");
    }

    #[test]
    fn the_list_is_sorted_and_names_what_lookup_takes() {
        let names = known_names();
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(names.contains(&"CP1252") && names.contains(&"UTF-16"));
        for name in names {
            assert!(lookup(name).is_some(), "{name} is listed but unknown");
        }
    }

    #[test]
    fn utf16_marks_are_read_and_written_as_glibc_does() {
        // Written: a mark, then little-endian.
        assert_eq!(run("UTF-8", "UTF-16", false, b"ab"), ok(b"\xFF\xFEa\0b\0"));
        // Read: the mark picks the order and is consumed; none means little-endian.
        assert_eq!(run("UTF-16", "UTF-8", false, b"\xFF\xFEa\0"), ok(b"a"));
        assert_eq!(run("UTF-16", "UTF-8", false, b"\xFE\xFF\0a"), ok(b"a"));
        assert_eq!(run("UTF-16", "UTF-8", false, b"a\0"), ok(b"a"));
        // With an order given, no mark is written and a mark read is a character.
        assert_eq!(run("UTF-8", "UTF-16LE", false, b"a"), ok(b"a\0"));
        assert_eq!(run("UTF-8", "UTF-16BE", false, b"a"), ok(b"\0a"));
        assert_eq!(
            run("UTF-16LE", "UTF-8", false, b"\xFF\xFEa\0"),
            ok(b"\xEF\xBB\xBFa")
        );
        // Nothing in, nothing out: not even a mark.
        assert_eq!(run("UTF-8", "UTF-16", false, b""), ok(b""));
        assert_eq!(run("UTF-16", "UTF-16", false, b"\xFF\xFE"), ok(b""));
    }

    #[test]
    fn utf32_ucs2_and_ucs4() {
        assert_eq!(
            run("UTF-8", "UTF-32", false, b"a"),
            ok(b"\xFF\xFE\0\0a\0\0\0")
        );
        assert_eq!(run("UTF-8", "UTF-32BE", false, b"a"), ok(b"\0\0\0a"));
        assert_eq!(
            run("UTF-32", "UTF-8", false, b"\0\0\xFE\xFF\0\0\0a"),
            ok(b"a")
        );
        assert_eq!(run("UTF-32", "UTF-8", false, b"a\0\0\0"), ok(b"a"));
        assert_eq!(run("UTF-8", "UCS-2", false, b"a"), ok(b"a\0"));
        assert_eq!(run("UTF-8", "UCS-4", false, b"a"), ok(b"\0\0\0a"));
        assert_eq!(run("UTF-8", "WCHAR_T", false, b"a"), ok(b"a\0\0\0"));
        // UCS-2 keeps a mark as a character and has no surrogate pairs.
        assert_eq!(
            run("UCS-2", "UTF-8", false, b"\xFF\xFEa\0"),
            ok(b"\xEF\xBB\xBFa")
        );
        assert_eq!(
            run("UTF-8", "UCS-2LE", false, "a😀b".as_bytes()),
            Outcome {
                bytes: b"a\0".to_vec(),
                failure: Some(Failure::Illegal(1)),
                dropped: false,
            }
        );
        assert_eq!(
            run("UTF-8", "UCS-2LE", true, "a😀b".as_bytes()),
            Outcome {
                bytes: b"a\0b\0".to_vec(),
                failure: None,
                dropped: true,
            }
        );
    }

    #[test]
    fn surrogates_in_utf16_input() {
        assert_eq!(
            run("UTF-16LE", "UTF-8", false, b"\x3D\xD8\x00\xDE"),
            ok("😀".as_bytes())
        );
        // A high surrogate followed by a non-low one, and a lone low one.
        assert_eq!(
            run("UTF-16LE", "UTF-8", false, b"a\0\x3D\xD8b\0").failure,
            Some(Failure::Illegal(2))
        );
        assert_eq!(
            run("UTF-16LE", "UTF-8", false, b"a\0\x00\xDEb\0").failure,
            Some(Failure::Illegal(2))
        );
        // A high surrogate at the very end is incomplete, as is an odd byte.
        assert_eq!(
            run("UTF-16LE", "UTF-8", false, b"a\0\x3D\xD8").failure,
            Some(Failure::Incomplete)
        );
        assert_eq!(
            run("UTF-16LE", "UTF-8", false, b"a\0b"),
            Outcome {
                bytes: b"a".to_vec(),
                failure: Some(Failure::Incomplete),
                dropped: false,
            }
        );
        assert_eq!(
            run("UTF-16LE", "UTF-8", true, b"a\0\x3D\xD8b\0"),
            Outcome {
                bytes: b"ab".to_vec(),
                failure: None,
                dropped: true,
            }
        );
    }

    #[test]
    fn a_bad_utf8_byte_stops_at_its_position() {
        assert_eq!(
            run("UTF-8", "LATIN1", false, b"ab\xFFcd"),
            Outcome {
                bytes: b"ab".to_vec(),
                failure: Some(Failure::Illegal(2)),
                dropped: false,
            }
        );
        // Surrogates and overlong forms are not UTF-8.
        assert_eq!(
            run("UTF-8", "UTF-16LE", false, b"a\xED\xA0\x80c").failure,
            Some(Failure::Illegal(1))
        );
        assert_eq!(
            run("UTF-8", "UTF-16LE", false, b"a\xC0\x80c").failure,
            Some(Failure::Illegal(1))
        );
        assert_eq!(
            run("UTF-8", "LATIN1", false, b"ab\xC3"),
            Outcome {
                bytes: b"ab".to_vec(),
                failure: Some(Failure::Incomplete),
                dropped: false,
            }
        );
        // `-c` skips the bad bytes but an incomplete end stays an error.
        assert_eq!(
            run("UTF-8", "LATIN1", true, b"ab\xFFc\xE2\x82(d"),
            Outcome {
                bytes: b"abc(d".to_vec(),
                failure: None,
                dropped: true,
            }
        );
        assert_eq!(
            run("UTF-8", "LATIN1", true, b"ab\xC3"),
            Outcome {
                bytes: b"ab".to_vec(),
                failure: Some(Failure::Incomplete),
                dropped: false,
            }
        );
    }

    #[test]
    fn an_unconvertible_character_is_an_illegal_sequence_at_its_first_byte() {
        assert_eq!(
            run("UTF-8", "LATIN1", false, "ab€cd".as_bytes()),
            Outcome {
                bytes: b"ab".to_vec(),
                failure: Some(Failure::Illegal(2)),
                dropped: false,
            }
        );
        assert_eq!(
            run("UTF-16LE", "LATIN1", false, b"a\0b\0\xAC\x20c\0").failure,
            Some(Failure::Illegal(4))
        );
        assert_eq!(
            run("GBK", "LATIN1", false, b"\xC4\xE3\xC4\xE3\xA4\xA4").failure,
            Some(Failure::Illegal(0))
        );
        assert_eq!(
            run("UTF-8", "ASCII", false, "abc\u{e9}d".as_bytes()).failure,
            Some(Failure::Illegal(3))
        );
        // A UTF-8 mark is a character, and LATIN1 has none for it.
        assert_eq!(
            run("UTF-8", "LATIN1", false, b"\xEF\xBB\xBFa").failure,
            Some(Failure::Illegal(0))
        );
        assert_eq!(
            run("UTF-8", "LATIN1", true, "ab€cd".as_bytes()),
            Outcome {
                bytes: b"abcd".to_vec(),
                failure: None,
                dropped: true,
            }
        );
        assert!(run("UTF-8", "LATIN1//IGNORE", false, "ab€cd".as_bytes()).dropped);
    }

    #[test]
    fn a_bad_cp932_lead_byte() {
        // A lone lead byte at the end is incomplete; a bad trail byte is illegal at the
        // lead byte, and `-c` skips the lead byte alone.
        assert_eq!(
            run("CP932", "UTF-8", false, b"ab\x81"),
            Outcome {
                bytes: b"ab".to_vec(),
                failure: Some(Failure::Incomplete),
                dropped: false,
            }
        );
        assert_eq!(
            run("CP932", "UTF-8", false, b"ab\x81\x20cd"),
            Outcome {
                bytes: b"ab".to_vec(),
                failure: Some(Failure::Illegal(2)),
                dropped: false,
            }
        );
        assert_eq!(
            run("CP932", "UTF-8", true, b"ab\x81\x20cd"),
            Outcome {
                bytes: b"ab cd".to_vec(),
                failure: None,
                dropped: true,
            }
        );
        assert_eq!(
            run("SHIFT_JIS", "UTF-8", false, b"\x82\xA0\x82\xA2"),
            ok("あい".as_bytes())
        );
    }

    #[test]
    fn code_pages_and_tables_both_ways() {
        assert_eq!(
            run("UTF-8", "CP1252", false, "café €".as_bytes()),
            ok(b"caf\xE9 \x80")
        );
        assert_eq!(
            run("CP1252", "UTF-8", false, b"caf\xE9 \x80"),
            ok("café €".as_bytes())
        );
        assert_eq!(
            run("CP437", "UTF-8", false, b"\x80\x01"),
            ok("Ç\u{1}".as_bytes())
        );
        assert_eq!(run("KOI8-R", "UTF-8", false, b"\xC1"), ok("а".as_bytes()));
        assert_eq!(
            run("UTF-8", "ISO-8859-15", false, "€".as_bytes()),
            ok(b"\xA4")
        );
        assert_eq!(
            run("ISO-8859-10", "UTF-8", false, b"a\xA4"),
            ok("aĪ".as_bytes())
        );
        assert_eq!(
            run("UTF-8", "ISO-8859-16", false, "€".as_bytes()),
            ok(b"\xA4")
        );
        assert_eq!(
            run("UTF-8", "ISO-8859-14", false, "aĪ".as_bytes()),
            Outcome {
                bytes: b"a".to_vec(),
                failure: Some(Failure::Illegal(1)),
                dropped: false,
            }
        );
        assert_eq!(
            run("ASCII", "UTF-8", false, b"a\xE9").failure,
            Some(Failure::Illegal(1))
        );
        // CRLF is bytes like any other.
        assert_eq!(
            run("UTF-8", "LATIN1", false, b"a\r\nb\r\n"),
            ok(b"a\r\nb\r\n")
        );
    }

    #[test]
    fn translit_approximates_and_never_fails() {
        let outcome = run(
            "UTF-8",
            "ASCII//TRANSLIT",
            false,
            "a\u{e9}\u{4e2d}b".as_bytes(),
        );
        assert_eq!(outcome.failure, None);
        assert!(!outcome.dropped);
        assert_eq!(outcome.bytes, b"ae?b");
        assert_eq!(
            run("UTF-8", "LATIN1//TRANSLIT", false, "a\u{4e2d}".as_bytes()).bytes,
            b"a?"
        );
        assert_eq!(
            run("UTF-8", "UCS-2LE//TRANSLIT", false, "a😀".as_bytes()).bytes,
            b"a\0?\0"
        );
        // Bad input is still dropped, and counted, under `-c` with TRANSLIT.
        assert_eq!(
            run("UTF-8", "ASCII//TRANSLIT", true, b"a\xFFb"),
            Outcome {
                bytes: b"ab".to_vec(),
                failure: None,
                dropped: true,
            }
        );
    }

    #[test]
    fn shift_state_code_pages_round_trip() {
        let encoded = run("UTF-8", "ISO-2022-JP", false, "あ".as_bytes());
        assert_eq!(encoded, ok(b"\x1B$B$\"\x1B(B"));
        assert_eq!(
            run("ISO-2022-JP", "UTF-8", false, b"\x1B$B$\"\x1B(B"),
            ok("あ".as_bytes())
        );
        assert_eq!(
            run("UTF-8", "GB18030", false, "你".as_bytes()),
            ok(b"\xC4\xE3")
        );
        // A character ISO-2022-JP lacks is found by the round trip.
        let outcome = run(
            "UTF-8",
            "ISO-2022-JP",
            false,
            "a\u{4e2d}\u{20ac}".as_bytes(),
        );
        assert_eq!(outcome.failure, Some(Failure::Illegal(4)));
        let outcome = run("UTF-8", "ISO-2022-JP", true, "a\u{20ac}b".as_bytes());
        assert_eq!(
            outcome,
            Outcome {
                bytes: b"ab".to_vec(),
                failure: None,
                dropped: true
            }
        );
    }

    #[test]
    fn options_parse_as_glibc_takes_them() {
        let args =
            |list: &[&str]| -> Vec<String> { list.iter().map(|s| (*s).to_owned()).collect() };
        assert_eq!(
            parse(&args(&[
                "-f", "LATIN1", "-tUTF-8", "-c", "a", "-", "--", "-x"
            ])),
            Parsed::Run(Options {
                from: "LATIN1".into(),
                to: "UTF-8".into(),
                discard: true,
                output: None,
                list: false,
                verbose: false,
                files: vec!["a".into(), "-".into(), "-x".into()],
            })
        );
        assert_eq!(
            parse(&args(&[
                "--from-code=A",
                "--to",
                "B",
                "--out=o",
                "--verbose",
                "-cs"
            ])),
            Parsed::Run(Options {
                from: "A".into(),
                to: "B".into(),
                discard: true,
                output: Some("o".into()),
                list: false,
                verbose: true,
                files: vec![],
            })
        );
        assert!(matches!(
            parse(&args(&["-l", "-f", "FOO"])),
            Parsed::Run(Options { list: true, .. })
        ));
        assert_eq!(parse(&args(&["-?"])), Parsed::Help);
        assert_eq!(parse(&args(&["--usage"])), Parsed::Usage);
        assert_eq!(parse(&args(&["-V"])), Parsed::Version);
        assert_eq!(
            parse(&args(&["-Z"])),
            Parsed::Fail("iconv: invalid option -- 'Z'".into())
        );
        assert_eq!(
            parse(&args(&["--bogus"])),
            Parsed::Fail("iconv: unrecognized option '--bogus'".into())
        );
        assert_eq!(
            parse(&args(&["-f"])),
            Parsed::Fail("iconv: option requires an argument -- 'f'".into())
        );
        assert_eq!(
            parse(&args(&["--ver"])),
            Parsed::Fail(
                "iconv: option '--ver' is ambiguous; possibilities: '--verbose' '--version'".into()
            )
        );
    }
}
