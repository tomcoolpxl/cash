//! `stty`, GNU coreutils 9.11's, on the Windows console modes.
//!
//! The vocabulary, the report formats and the error texts are GNU's. The terminal is the
//! console standard input is attached to, so with standard input redirected the command
//! fails as GNU's does on a pipe, with `Inappropriate ioctl for device`.
//!
//! Four settings are the console's own and are read from it and set on it: `echo`,
//! `icanon` (line collection), `isig` (Ctrl-C as a signal) and `opost` (output
//! processing). The other settings and the special characters have no counterpart on a
//! console; they are accepted so that scripts written for a terminal run, remembered for
//! the process (`cash_win32::termios`), and printed back by `stty -a` and `stty -g`.
//! `rows N` and `cols N` are accepted and ignored: `size` reports the real window, and a
//! script that sets the kernel's idea of the size does not expect the window to change.
//!
//! The main use is `stty -echo; read -r password; stty echo`: `read` at a console shows
//! the keys itself, and leaves them unshown when the console's echo is off.

use std::io::Write;

use cash_core::openfiles::OpenFiles;
use cash_core::{ExecutionResult, builtins};
use cash_win32::termios::{self, ECHO, ICANON, ISIG, NCCS, OPOST, Termios};
use clap::Parser;

/// Print or change terminal characteristics.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct SttyCommand {
    /// Options and settings, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

// Linux's `termios` bits, which the remembered settings are kept in. Input flags first.
const IGNBRK: u32 = 0o000_001;
const BRKINT: u32 = 0o000_002;
const IGNPAR: u32 = 0o000_004;
const PARMRK: u32 = 0o000_010;
const INPCK: u32 = 0o000_020;
const ISTRIP: u32 = 0o000_040;
const INLCR: u32 = 0o000_100;
const IGNCR: u32 = 0o000_200;
const ICRNL: u32 = 0o000_400;
const IUCLC: u32 = 0o001_000;
const IXON: u32 = 0o002_000;
const IXANY: u32 = 0o004_000;
const IXOFF: u32 = 0o010_000;
const IMAXBEL: u32 = 0o020_000;
const IUTF8: u32 = 0o040_000;
// Output flags; `OPOST` comes from `cash_win32::termios`.
const OLCUC: u32 = 0o000_002;
const ONLCR: u32 = 0o000_004;
const OCRNL: u32 = 0o000_010;
const ONOCR: u32 = 0o000_020;
const ONLRET: u32 = 0o000_040;
const OFILL: u32 = 0o000_100;
const OFDEL: u32 = 0o000_200;
const NLDLY: u32 = 0o000_400;
const NL0: u32 = 0o000_000;
const NL1: u32 = 0o000_400;
const CRDLY: u32 = 0o003_000;
const CR0: u32 = 0o000_000;
const CR1: u32 = 0o001_000;
const CR2: u32 = 0o002_000;
const CR3: u32 = 0o003_000;
const TABDLY: u32 = 0o014_000;
const TAB0: u32 = 0o000_000;
const TAB1: u32 = 0o004_000;
const TAB2: u32 = 0o010_000;
const TAB3: u32 = 0o014_000;
const BSDLY: u32 = 0o020_000;
const BS0: u32 = 0o000_000;
const BS1: u32 = 0o020_000;
const VTDLY: u32 = 0o040_000;
const VT0: u32 = 0o000_000;
const VT1: u32 = 0o040_000;
const FFDLY: u32 = 0o100_000;
const FF0: u32 = 0o000_000;
const FF1: u32 = 0o100_000;
// Control flags.
const CBAUD: u32 = 0o010_017;
const CSIZE: u32 = 0o000_060;
const CS5: u32 = 0o000_000;
const CS6: u32 = 0o000_020;
const CS7: u32 = 0o000_040;
const CS8: u32 = 0o000_060;
const CSTOPB: u32 = 0o000_100;
const CREAD: u32 = 0o000_200;
const PARENB: u32 = 0o000_400;
const PARODD: u32 = 0o001_000;
const HUPCL: u32 = 0o002_000;
const CLOCAL: u32 = 0o004_000;
const CMSPAR: u32 = 0o10_000_000_000;
const CRTSCTS: u32 = 0o20_000_000_000;
// Local flags; `ISIG`, `ICANON` and `ECHO` come from `cash_win32::termios`.
const XCASE: u32 = 0o000_004;
const ECHOE: u32 = 0o000_020;
const ECHOK: u32 = 0o000_040;
const ECHONL: u32 = 0o000_100;
const NOFLSH: u32 = 0o000_200;
const TOSTOP: u32 = 0o000_400;
const ECHOCTL: u32 = 0o001_000;
const ECHOPRT: u32 = 0o002_000;
const ECHOKE: u32 = 0o004_000;
const FLUSHO: u32 = 0o010_000;
const IEXTEN: u32 = 0o100_000;
const EXTPROC: u32 = 0o200_000;
// Special characters, by index.
const VINTR: usize = 0;
const VQUIT: usize = 1;
const VERASE: usize = 2;
const VKILL: usize = 3;
const VEOF: usize = 4;
const VTIME: usize = 5;
const VMIN: usize = 6;
const VSWTC: usize = 7;
const VSTART: usize = 8;
const VSTOP: usize = 9;
const VSUSP: usize = 10;
const VEOL: usize = 11;
const VREPRINT: usize = 12;
const VDISCARD: usize = 13;
const VWERASE: usize = 14;
const VLNEXT: usize = 15;
const VEOL2: usize = 16;

/// Which word of the settings a mode lives in; combinations set several.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Control,
    Input,
    Output,
    Local,
    Combination,
}

/// `sane` turns the mode on.
const SANE_SET: u8 = 1;
/// `sane` turns the mode off.
const SANE_UNSET: u8 = 2;
/// The mode can be turned off with a `-`.
const REV: u8 = 4;
/// Not shown by `-a`: a combination, or another name for a mode shown already.
const OMIT: u8 = 8;

/// A mode as GNU's table has it.
struct Mode {
    name: &'static str,
    kind: Kind,
    flags: u8,
    /// The bits the mode sets.
    bits: u32,
    /// The bits it clears first, when it is one value of a field (`cs8`, `tab3`).
    mask: u32,
}

const fn m(name: &'static str, kind: Kind, flags: u8, bits: u32, mask: u32) -> Mode {
    Mode {
        name,
        kind,
        flags,
        bits,
        mask,
    }
}

/// GNU's modes, in the order `stty -a` prints them.
const MODES: &[Mode] = &[
    m("parenb", Kind::Control, REV, PARENB, 0),
    m("parodd", Kind::Control, REV, PARODD, 0),
    m("cmspar", Kind::Control, REV, CMSPAR, 0),
    m("cs5", Kind::Control, 0, CS5, CSIZE),
    m("cs6", Kind::Control, 0, CS6, CSIZE),
    m("cs7", Kind::Control, 0, CS7, CSIZE),
    m("cs8", Kind::Control, 0, CS8, CSIZE),
    m("hupcl", Kind::Control, REV, HUPCL, 0),
    m("hup", Kind::Control, REV | OMIT, HUPCL, 0),
    m("cstopb", Kind::Control, REV, CSTOPB, 0),
    m("cread", Kind::Control, SANE_SET | REV, CREAD, 0),
    m("clocal", Kind::Control, REV, CLOCAL, 0),
    m("crtscts", Kind::Control, REV, CRTSCTS, 0),
    m("ignbrk", Kind::Input, SANE_UNSET | REV, IGNBRK, 0),
    m("brkint", Kind::Input, SANE_SET | REV, BRKINT, 0),
    m("ignpar", Kind::Input, REV, IGNPAR, 0),
    m("parmrk", Kind::Input, REV, PARMRK, 0),
    m("inpck", Kind::Input, REV, INPCK, 0),
    m("istrip", Kind::Input, REV, ISTRIP, 0),
    m("inlcr", Kind::Input, SANE_UNSET | REV, INLCR, 0),
    m("igncr", Kind::Input, SANE_UNSET | REV, IGNCR, 0),
    m("icrnl", Kind::Input, SANE_SET | REV, ICRNL, 0),
    m("ixon", Kind::Input, REV, IXON, 0),
    m("ixoff", Kind::Input, SANE_UNSET | REV, IXOFF, 0),
    m("tandem", Kind::Input, REV | OMIT, IXOFF, 0),
    m("iuclc", Kind::Input, SANE_UNSET | REV, IUCLC, 0),
    m("ixany", Kind::Input, SANE_UNSET | REV, IXANY, 0),
    m("imaxbel", Kind::Input, SANE_SET | REV, IMAXBEL, 0),
    m("iutf8", Kind::Input, SANE_UNSET | REV, IUTF8, 0),
    m("opost", Kind::Output, SANE_SET | REV, OPOST, 0),
    m("olcuc", Kind::Output, SANE_UNSET | REV, OLCUC, 0),
    m("ocrnl", Kind::Output, SANE_UNSET | REV, OCRNL, 0),
    m("onlcr", Kind::Output, SANE_SET | REV, ONLCR, 0),
    m("onocr", Kind::Output, SANE_UNSET | REV, ONOCR, 0),
    m("onlret", Kind::Output, SANE_UNSET | REV, ONLRET, 0),
    m("ofill", Kind::Output, SANE_UNSET | REV, OFILL, 0),
    m("ofdel", Kind::Output, SANE_UNSET | REV, OFDEL, 0),
    m("nl1", Kind::Output, SANE_UNSET, NL1, NLDLY),
    m("nl0", Kind::Output, SANE_SET, NL0, NLDLY),
    m("cr3", Kind::Output, SANE_UNSET, CR3, CRDLY),
    m("cr2", Kind::Output, SANE_UNSET, CR2, CRDLY),
    m("cr1", Kind::Output, SANE_UNSET, CR1, CRDLY),
    m("cr0", Kind::Output, SANE_SET, CR0, CRDLY),
    m("tab3", Kind::Output, SANE_UNSET, TAB3, TABDLY),
    m("tab2", Kind::Output, SANE_UNSET, TAB2, TABDLY),
    m("tab1", Kind::Output, SANE_UNSET, TAB1, TABDLY),
    m("tab0", Kind::Output, SANE_SET, TAB0, TABDLY),
    m("bs1", Kind::Output, SANE_UNSET, BS1, BSDLY),
    m("bs0", Kind::Output, SANE_SET, BS0, BSDLY),
    m("vt1", Kind::Output, SANE_UNSET, VT1, VTDLY),
    m("vt0", Kind::Output, SANE_SET, VT0, VTDLY),
    m("ff1", Kind::Output, SANE_UNSET, FF1, FFDLY),
    m("ff0", Kind::Output, SANE_SET, FF0, FFDLY),
    m("isig", Kind::Local, SANE_SET | REV, ISIG, 0),
    m("icanon", Kind::Local, SANE_SET | REV, ICANON, 0),
    m("iexten", Kind::Local, SANE_SET | REV, IEXTEN, 0),
    m("echo", Kind::Local, SANE_SET | REV, ECHO, 0),
    m("echoe", Kind::Local, SANE_SET | REV, ECHOE, 0),
    m("crterase", Kind::Local, REV | OMIT, ECHOE, 0),
    m("echok", Kind::Local, SANE_SET | REV, ECHOK, 0),
    m("echonl", Kind::Local, SANE_UNSET | REV, ECHONL, 0),
    m("noflsh", Kind::Local, SANE_UNSET | REV, NOFLSH, 0),
    m("xcase", Kind::Local, SANE_UNSET | REV, XCASE, 0),
    m("tostop", Kind::Local, SANE_UNSET | REV, TOSTOP, 0),
    m("echoprt", Kind::Local, SANE_UNSET | REV, ECHOPRT, 0),
    m("prterase", Kind::Local, REV | OMIT, ECHOPRT, 0),
    m("echoctl", Kind::Local, SANE_SET | REV, ECHOCTL, 0),
    m("ctlecho", Kind::Local, REV | OMIT, ECHOCTL, 0),
    m("echoke", Kind::Local, SANE_SET | REV, ECHOKE, 0),
    m("crtkill", Kind::Local, REV | OMIT, ECHOKE, 0),
    m("flusho", Kind::Local, SANE_UNSET | REV, FLUSHO, 0),
    m("extproc", Kind::Local, SANE_UNSET | REV, EXTPROC, 0),
    m("evenp", Kind::Combination, REV | OMIT, 0, 0),
    m("parity", Kind::Combination, REV | OMIT, 0, 0),
    m("oddp", Kind::Combination, REV | OMIT, 0, 0),
    m("nl", Kind::Combination, REV | OMIT, 0, 0),
    m("ek", Kind::Combination, OMIT, 0, 0),
    m("sane", Kind::Combination, OMIT, 0, 0),
    m("cooked", Kind::Combination, REV | OMIT, 0, 0),
    m("raw", Kind::Combination, REV | OMIT, 0, 0),
    m("pass8", Kind::Combination, REV | OMIT, 0, 0),
    m("litout", Kind::Combination, REV | OMIT, 0, 0),
    m("cbreak", Kind::Combination, REV | OMIT, 0, 0),
    m("crt", Kind::Combination, OMIT, 0, 0),
    m("dec", Kind::Combination, OMIT, 0, 0),
    m("decctlq", Kind::Combination, REV | OMIT, 0, 0),
    m("tabs", Kind::Combination, REV | OMIT, 0, 0),
    m("lcase", Kind::Combination, REV | OMIT, 0, 0),
    m("LCASE", Kind::Combination, REV | OMIT, 0, 0),
];

/// A special character as GNU's table has it: its name, `sane`'s value and its slot.
struct Control {
    name: &'static str,
    sane: u8,
    index: usize,
}

const fn c(name: &'static str, sane: u8, index: usize) -> Control {
    Control { name, sane, index }
}

/// GNU's special characters, in the order `stty -a` prints them; `min` and `time` last.
const CONTROLS: &[Control] = &[
    c("intr", 0o03, VINTR),
    c("quit", 0o34, VQUIT),
    c("erase", 0o177, VERASE),
    c("kill", 0o25, VKILL),
    c("eof", 0o04, VEOF),
    c("eol", 0, VEOL),
    c("eol2", 0, VEOL2),
    c("swtch", 0, VSWTC),
    c("start", 0o21, VSTART),
    c("stop", 0o23, VSTOP),
    c("susp", 0o32, VSUSP),
    c("rprnt", 0o22, VREPRINT),
    c("werase", 0o27, VWERASE),
    c("lnext", 0o26, VLNEXT),
    c("discard", 0o17, VDISCARD),
    c("min", 1, VMIN),
    c("time", 0, VTIME),
];

/// The speeds GNU knows, and Linux's code for each in the control flags.
const SPEEDS: &[(&str, u32)] = &[
    ("0", 0o0),
    ("50", 0o1),
    ("75", 0o2),
    ("110", 0o3),
    ("134", 0o4),
    ("134.5", 0o4),
    ("150", 0o5),
    ("200", 0o6),
    ("300", 0o7),
    ("600", 0o10),
    ("1200", 0o11),
    ("1800", 0o12),
    ("2400", 0o13),
    ("4800", 0o14),
    ("9600", 0o15),
    ("19200", 0o16),
    ("38400", 0o17),
    ("exta", 0o16),
    ("extb", 0o17),
    ("57600", 0o10_001),
    ("115200", 0o10_002),
    ("230400", 0o10_003),
    ("460800", 0o10_004),
    ("500000", 0o10_005),
    ("576000", 0o10_006),
    ("921600", 0o10_007),
    ("1000000", 0o10_010),
    ("1152000", 0o10_011),
    ("1500000", 0o10_012),
    ("2000000", 0o10_013),
    ("2500000", 0o10_014),
    ("3000000", 0o10_015),
    ("3500000", 0o10_016),
    ("4000000", 0o10_017),
];

/// The word of `mode` a kind of setting lives in; none for a combination.
const fn word(mode: &mut Termios, kind: Kind) -> Option<&mut u32> {
    match kind {
        Kind::Control => Some(&mut mode.cflag),
        Kind::Input => Some(&mut mode.iflag),
        Kind::Output => Some(&mut mode.oflag),
        Kind::Local => Some(&mut mode.lflag),
        Kind::Combination => None,
    }
}

/// Whether `info` is on in `mode`, as `-a` decides it.
const fn is_on(info: &Mode, mode: &Termios) -> bool {
    let bits = match info.kind {
        Kind::Control => mode.cflag,
        Kind::Input => mode.iflag,
        Kind::Output => mode.oflag,
        Kind::Local => mode.lflag,
        Kind::Combination => return false,
    };
    let mask = if info.mask == 0 { info.bits } else { info.mask };
    bits & mask == info.bits
}

/// The speed in baud that `code` stands for; 0 for one Linux has no name for.
fn baud_of(code: u32) -> &'static str {
    SPEEDS
        .iter()
        .find(|(_, speed_code)| *speed_code == code)
        .map_or("0", |(text, _)| text)
}

/// Linux's code for the speed `text` names, if GNU knows it.
fn speed_code(text: &str) -> Option<u32> {
    SPEEDS
        .iter()
        .find(|(name, _)| *name == text)
        .map(|(_, code)| *code)
}

/// `sane`: GNU's sane settings, the special characters included; what `sane` says
/// nothing about is left as it is.
fn sane(mode: &mut Termios) {
    for control in CONTROLS {
        mode.cc[control.index] = control.sane;
    }
    for info in MODES {
        let Some(bits) = word(mode, info.kind) else {
            continue;
        };
        if info.flags & SANE_SET != 0 {
            *bits = (*bits & !info.mask) | info.bits;
        } else if info.flags & SANE_UNSET != 0 {
            *bits = *bits & !info.mask & !info.bits;
        }
    }
}

/// Sets, or with `reversed` clears, the mode `info` in `mode`; `false` when the mode
/// cannot be turned off.
fn set_mode(info: &Mode, reversed: bool, mode: &mut Termios) -> bool {
    if reversed && info.flags & REV == 0 {
        return false;
    }
    match word(mode, info.kind) {
        Some(bits) if reversed => *bits = *bits & !info.mask & !info.bits,
        Some(bits) => *bits = (*bits & !info.mask) | info.bits,
        None => combine(info.name, reversed, mode),
    }
    true
}

/// GNU's combination settings.
fn combine(name: &str, reversed: bool, mode: &mut Termios) {
    match name {
        "evenp" | "parity" | "oddp" if reversed => {
            mode.cflag = (mode.cflag & !(PARENB | CSIZE)) | CS8;
        }
        "evenp" | "parity" => mode.cflag = (mode.cflag & !(PARODD | CSIZE)) | PARENB | CS7,
        "oddp" => mode.cflag = (mode.cflag & !CSIZE) | CS7 | PARODD | PARENB,
        "nl" if reversed => {
            mode.iflag = (mode.iflag | ICRNL) & !(INLCR | IGNCR);
            mode.oflag = (mode.oflag | ONLCR) & !(OCRNL | ONLRET);
        }
        "nl" => {
            mode.iflag &= !ICRNL;
            mode.oflag &= !ONLCR;
        }
        "ek" => {
            mode.cc[VERASE] = 0o177;
            mode.cc[VKILL] = 0o25;
        }
        "sane" => sane(mode),
        // `cooked` and `-raw` are cooked; `raw` and `-cooked` are raw. GNU's raw leaves
        // echo as it is.
        "cooked" | "raw" if (name == "raw") == reversed => {
            mode.iflag |= BRKINT | IGNPAR | ISTRIP | ICRNL | IXON;
            mode.oflag |= OPOST;
            mode.lflag |= ISIG | ICANON;
        }
        "cooked" | "raw" => {
            mode.iflag = 0;
            mode.oflag &= !OPOST;
            mode.lflag &= !(ISIG | ICANON | XCASE);
            mode.cc[VMIN] = 1;
            mode.cc[VTIME] = 0;
        }
        "pass8" | "litout" if reversed => {
            mode.cflag = (mode.cflag & !CSIZE) | CS7 | PARENB;
            mode.iflag |= ISTRIP;
            if name == "litout" {
                mode.oflag |= OPOST;
            }
        }
        "pass8" | "litout" => {
            mode.cflag = (mode.cflag & !(CSIZE | PARENB)) | CS8;
            mode.iflag &= !ISTRIP;
            if name == "litout" {
                mode.oflag &= !OPOST;
            }
        }
        "cbreak" if reversed => mode.lflag |= ICANON,
        "cbreak" => mode.lflag &= !ICANON,
        "crt" => mode.lflag |= ECHOE | ECHOCTL | ECHOKE,
        "dec" => {
            mode.lflag |= ECHOE | ECHOCTL | ECHOKE;
            mode.cc[VINTR] = 3;
            mode.cc[VERASE] = 127;
            mode.cc[VKILL] = 21;
            mode.iflag &= !IXANY;
        }
        "decctlq" if reversed => mode.iflag |= IXANY,
        "decctlq" => mode.iflag &= !IXANY,
        "tabs" if reversed => mode.oflag = (mode.oflag & !TABDLY) | TAB3,
        "tabs" => mode.oflag = (mode.oflag & !TABDLY) | TAB0,
        "lcase" | "LCASE" if reversed => {
            mode.lflag &= !XCASE;
            mode.iflag &= !IUCLC;
            mode.oflag &= !OLCUC;
        }
        "lcase" | "LCASE" => {
            mode.lflag |= XCASE;
            mode.iflag |= IUCLC;
            mode.oflag |= OLCUC;
        }
        _ => {}
    }
}

/// Why a setting could not be applied, with GNU's words for it.
#[derive(Debug, PartialEq, Eq)]
enum Failure {
    /// Not a setting GNU knows, or one that cannot be turned off.
    Invalid(String),
    /// A setting that takes a value came last.
    MissingArgument(String),
    /// A value that is not a number, or too large.
    InvalidInteger(String),
    /// `ispeed` or `ospeed` with a speed GNU does not know.
    InvalidSpeed(&'static str, String),
}

impl Failure {
    /// The message, after `stty: `; each is followed by the `--help` hint.
    fn message(&self) -> String {
        match self {
            Self::Invalid(arg) => format!("invalid argument '{arg}'"),
            Self::MissingArgument(arg) => format!("missing argument to '{arg}'"),
            Self::InvalidInteger(arg) => format!("invalid integer argument '{arg}'"),
            Self::InvalidSpeed(which, arg) => format!("invalid {which} '{arg}'"),
        }
    }
}

/// A number as GNU reads one: decimal, octal with a leading `0`, or hex with `0x`; at
/// most `max`.
fn integer_arg(text: &str, max: u64) -> Result<u64, Failure> {
    let (digits, radix) =
        if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            (hex, 16)
        } else if let Some(octal) = text.strip_prefix('0').filter(|rest| !rest.is_empty()) {
            (octal, 8)
        } else {
            (text, 10)
        };
    u64::from_str_radix(digits, radix)
        .ok()
        .filter(|value| *value <= max)
        .ok_or_else(|| Failure::InvalidInteger(text.to_owned()))
}

/// A special character as GNU reads one: `^C`, `^?`, `^-` or `undef`, one character, or
/// a number; for `min` and `time` only a number.
fn control_char(control: &Control, arg: &str) -> Result<u8, Failure> {
    let as_byte = |value: u64| u8::try_from(value).unwrap_or(u8::MAX);
    if control.name == "min" || control.name == "time" {
        return integer_arg(arg, u64::from(u8::MAX)).map(as_byte);
    }
    let mut bytes = arg.bytes();
    let (first, second) = (bytes.next(), bytes.next());
    Ok(match (first, second) {
        (None, _) => 0,
        (Some(only), None) => only,
        _ if arg == "^-" || arg == "undef" => 0,
        (Some(b'^'), Some(b'?')) => 127,
        (Some(b'^'), Some(key)) => key & !0o140,
        _ => as_byte(integer_arg(arg, u64::from(u8::MAX))?),
    })
}

/// A special character as GNU prints it: `^C`, `^?`, `<undef>`, `M-x` for one above 127.
fn visible(ch: u8) -> String {
    match ch {
        0 => "<undef>".to_owned(),
        1..=31 => format!("^{}", char::from(ch + 64)),
        32..=126 => char::from(ch).to_string(),
        127 => "^?".to_owned(),
        _ => {
            let low = ch - 128;
            let rest = match low {
                32..=126 => char::from(low).to_string(),
                127 => "^?".to_owned(),
                _ => format!("^{}", char::from(low + 64)),
            };
            format!("M-{rest}")
        }
    }
}

/// Something a setting prints at once, as GNU does where it meets the word.
#[derive(Debug, PartialEq, Eq)]
enum Immediate {
    /// `size`: the window's rows and columns.
    Size,
    /// `speed`: the speed in baud.
    Speed(&'static str),
}

/// What a list of settings comes to.
#[derive(Debug)]
struct Plan {
    /// The settings once every word has been applied.
    mode: Termios,
    /// What to print, in order.
    prints: Vec<Immediate>,
    /// Whether a setting changed anything, so the settings are to be applied.
    changed: bool,
    /// Whether `sane` or `cooked` was asked for, which also puts the console back as a
    /// fresh one is.
    restore_console: bool,
}

/// Applies GNU's settings `args`, in order, to a copy of `mode`.
fn plan(mode: Termios, args: &[String]) -> Result<Plan, Failure> {
    let mut plan = Plan {
        mode,
        prints: Vec::new(),
        changed: false,
        restore_console: false,
    };
    let mut words = args.iter().map(String::as_str);
    while let Some(arg) = words.next() {
        let (name, reversed) = match arg.strip_prefix('-') {
            Some(name) => (name, true),
            None => (arg, false),
        };
        // `drain`: wait for output before applying the settings. Nothing is in transit.
        if name == "drain" {
            continue;
        }
        if let Some(info) = MODES.iter().find(|info| info.name == name) {
            if !set_mode(info, reversed, &mut plan.mode) {
                return Err(Failure::Invalid(arg.to_owned()));
            }
            let cooked = match name {
                "sane" => true,
                "cooked" => !reversed,
                "raw" => reversed,
                _ => false,
            };
            plan.restore_console |= cooked;
            plan.changed = true;
            continue;
        }
        if reversed {
            return Err(Failure::Invalid(arg.to_owned()));
        }
        let mut value = || {
            words
                .next()
                .ok_or_else(|| Failure::MissingArgument(arg.to_owned()))
        };
        if let Some(control) = CONTROLS.iter().find(|control| control.name == name) {
            plan.mode.cc[control.index] = control_char(control, value()?)?;
            plan.changed = true;
            continue;
        }
        match name {
            "ispeed" | "ospeed" => {
                let which = if name == "ispeed" { "ispeed" } else { "ospeed" };
                let text = value()?;
                let code = speed_code(text)
                    .ok_or_else(|| Failure::InvalidSpeed(which, text.to_owned()))?;
                // `ispeed 0` means the output speed, which is the one speed kept.
                if code != 0 || name == "ospeed" {
                    plan.mode.cflag = (plan.mode.cflag & !CBAUD) | code;
                    plan.changed = true;
                }
            }
            // The kernel's idea of the window: the real one is reported by `size`, and
            // the window itself is not to be resized under a script.
            "rows" | "cols" | "columns" => {
                integer_arg(value()?, u64::from(i32::MAX.unsigned_abs()))?;
            }
            "size" => plan.prints.push(Immediate::Size),
            "line" => {
                let line = integer_arg(value()?, u64::from(u8::MAX))?;
                plan.mode.line = u8::try_from(line).unwrap_or(u8::MAX);
                plan.changed = true;
            }
            "speed" => plan
                .prints
                .push(Immediate::Speed(baud_of(plan.mode.cflag & CBAUD))),
            _ => {
                if let Some(code) = speed_code(name) {
                    plan.mode.cflag = (plan.mode.cflag & !CBAUD) | code;
                } else if !recover_mode(name, &mut plan.mode) {
                    return Err(Failure::Invalid(arg.to_owned()));
                }
                plan.changed = true;
            }
        }
    }
    Ok(plan)
}

/// The settings as `stty -g` prints them: the four flag words and the special
/// characters, in hex, separated by colons.
fn recoverable(mode: &Termios) -> String {
    use std::fmt::Write as _;

    let mut text = format!(
        "{:x}:{:x}:{:x}:{:x}",
        mode.iflag, mode.oflag, mode.cflag, mode.lflag
    );
    for ch in mode.cc {
        // Writing to a `String` cannot fail.
        let _ = write!(text, ":{ch:x}");
    }
    text
}

/// Reads a string `stty -g` printed back into `mode`; `false` when `text` is not one.
fn recover_mode(text: &str, mode: &mut Termios) -> bool {
    let fields: Vec<Option<u32>> = text
        .split(':')
        .map(|field| u32::from_str_radix(field, 16).ok())
        .collect();
    if fields.len() != 4 + NCCS || fields.iter().any(Option::is_none) {
        return false;
    }
    let fields: Vec<u32> = fields.into_iter().flatten().collect();
    let mut cc = [0u8; NCCS];
    for (slot, field) in cc.iter_mut().zip(fields.iter().skip(4)) {
        let Ok(value) = u8::try_from(*field) else {
            return false;
        };
        *slot = value;
    }
    let [iflag, oflag, cflag, lflag] = [fields[0], fields[1], fields[2], fields[3]];
    *mode = Termios {
        iflag,
        oflag,
        cflag,
        lflag,
        line: mode.line,
        cc,
    };
    true
}

/// GNU's report writer: items separated by a blank, and a new line where the next would
/// reach the last column.
struct Report {
    text: String,
    col: usize,
    width: usize,
}

impl Report {
    const fn new(width: usize) -> Self {
        Self {
            text: String::new(),
            col: 0,
            width,
        }
    }

    fn item(&mut self, item: &str) {
        if self.col > 0 {
            if self.width.saturating_sub(self.col) <= item.len() {
                self.newline();
            } else {
                self.text.push(' ');
                self.col += 1;
            }
        }
        self.text.push_str(item);
        self.col += item.len();
    }

    fn newline(&mut self) {
        self.text.push('\n');
        self.col = 0;
    }

    /// Ends the line if anything is on it.
    fn end_line(&mut self) {
        if self.col != 0 {
            self.newline();
        }
    }
}

/// The special characters other than `min` and `time`.
fn named_controls() -> impl Iterator<Item = &'static Control> {
    CONTROLS.iter().take_while(|control| control.name != "min")
}

/// `stty -a`: everything, in GNU's layout, wrapped at `width` columns.
fn display_all(mode: &Termios, rows: u16, cols: u16, width: usize) -> String {
    let mut report = Report::new(width);
    report.item(&format!("speed {} baud;", baud_of(mode.cflag & CBAUD)));
    report.item(&format!("rows {rows}; columns {cols};"));
    report.item(&format!("line = {};", mode.line));
    report.newline();
    for control in named_controls() {
        report.item(&format!(
            "{} = {};",
            control.name,
            visible(mode.cc[control.index])
        ));
    }
    report.item(&format!(
        "min = {}; time = {};",
        mode.cc[VMIN], mode.cc[VTIME]
    ));
    report.end_line();
    let mut kind = Kind::Control;
    for info in MODES {
        if info.flags & OMIT != 0 {
            continue;
        }
        if info.kind != kind {
            report.newline();
            kind = info.kind;
        }
        if is_on(info, mode) {
            report.item(info.name);
        } else if info.flags & REV != 0 {
            report.item(&format!("-{}", info.name));
        }
    }
    report.newline();
    report.text
}

/// Bare `stty`: the speed and line, then only what differs from `sane`.
fn display_changed(mode: &Termios, width: usize) -> String {
    let mut report = Report::new(width);
    report.item(&format!("speed {} baud;", baud_of(mode.cflag & CBAUD)));
    report.item(&format!("line = {};", mode.line));
    report.newline();

    let mut empty = true;
    for control in named_controls() {
        if mode.cc[control.index] == control.sane {
            continue;
        }
        empty = false;
        report.item(&format!(
            "{} = {};",
            control.name,
            visible(mode.cc[control.index])
        ));
    }
    if mode.lflag & ICANON == 0 {
        report.item(&format!(
            "min = {}; time = {};",
            mode.cc[VMIN], mode.cc[VTIME]
        ));
        empty = false;
    }
    if !empty {
        report.newline();
    }

    let mut empty = true;
    let mut kind = Kind::Control;
    for info in MODES {
        if info.flags & OMIT != 0 {
            continue;
        }
        if info.kind != kind {
            if !empty {
                report.newline();
                empty = true;
            }
            kind = info.kind;
        }
        if is_on(info, mode) {
            if info.flags & SANE_UNSET != 0 {
                report.item(info.name);
                empty = false;
            }
        } else if info.flags & (SANE_SET | REV) == SANE_SET | REV {
            report.item(&format!("-{}", info.name));
            empty = false;
        }
    }
    if !empty {
        report.newline();
    }
    report.text
}

/// GNU's `--help`, trimmed to the settings taken here, with the Windows note last.
const HELP: &str = "\
Usage: stty [SETTING]...
  or:  stty [-a|--all]
  or:  stty [-g|--save]
Print or change terminal characteristics.

  -a, --all          print all current settings in human-readable form
  -g, --save         print all current settings in a stty-readable form
      --help         display this help and exit
      --version      output version information and exit

Optional - before SETTING indicates negation.  An * marks non-POSIX
settings.  The underlying system defines which settings are available.

Special characters:
   discard CHAR  CHAR will toggle discarding of output
   eof CHAR      CHAR will send an end of file (terminate the input)
   eol CHAR      CHAR will end the line
 * eol2 CHAR     alternate CHAR for ending the line
   erase CHAR    CHAR will erase the last character typed
   intr CHAR     CHAR will send an interrupt signal
   kill CHAR     CHAR will erase the current line
 * lnext CHAR    CHAR will enter the next character quoted
   quit CHAR     CHAR will send a quit signal
 * rprnt CHAR    CHAR will redraw the current line
   start CHAR    CHAR will restart the output after stopping it
   stop CHAR     CHAR will stop the output
   susp CHAR     CHAR will send a terminal stop signal
 * swtch CHAR    CHAR will switch to a different shell layer
 * werase CHAR   CHAR will erase the last word typed

Special settings:
   N             set the input and output speeds to N bauds
 * cols N        tell the kernel that the terminal has N columns
 * columns N     same as cols N
 * [-]drain      wait for transmission before applying settings (on by default)
   ispeed N      set the input speed to N
 * line N        use line discipline N
   min N         with -icanon, set N characters minimum for a completed read
   ospeed N      set the output speed to N
 * rows N        tell the kernel that the terminal has N rows
 * size          print the number of rows and columns according to the kernel
   speed         print the terminal speed
   time N        with -icanon, set read timeout of N tenths of a second

Control settings:
   [-]clocal     disable modem control signals
   [-]cread      allow input to be received
 * [-]crtscts    enable RTS/CTS handshaking
   csN           set character size to N bits, N in [5..8]
   [-]cstopb     use two stop bits per character (one with '-')
   [-]hup        send a hangup signal when the last process closes the tty
   [-]hupcl      same as [-]hup
   [-]parenb     generate parity bit in output and expect parity bit in input
   [-]parodd     set odd parity (or even parity with '-')
 * [-]cmspar     use \"stick\" (mark/space) parity

Input settings:
   [-]brkint     breaks cause an interrupt signal
   [-]icrnl      translate carriage return to newline
   [-]ignbrk     ignore break characters
   [-]igncr      ignore carriage return
   [-]ignpar     ignore characters with parity errors
 * [-]imaxbel    beep and do not flush a full input buffer on a character
   [-]inlcr      translate newline to carriage return
   [-]inpck      enable input parity checking
   [-]istrip     clear high (8th) bit of input characters
 * [-]iutf8      assume input characters are UTF-8 encoded
 * [-]iuclc      translate uppercase characters to lowercase
 * [-]ixany      let any character restart output, not only start character
   [-]ixoff      enable sending of start/stop characters
   [-]ixon       enable XON/XOFF flow control
   [-]parmrk     mark parity errors (with a 255-0-character sequence)
   [-]tandem     same as [-]ixoff

Output settings:
 * bsN           backspace delay style, N in [0..1]
 * crN           carriage return delay style, N in [0..3]
 * ffN           form feed delay style, N in [0..1]
 * nlN           newline delay style, N in [0..1]
 * [-]ocrnl      translate carriage return to newline
 * [-]ofdel      use delete characters for fill instead of NUL characters
 * [-]ofill      use fill (padding) characters instead of timing for delays
 * [-]olcuc      translate lowercase characters to uppercase
 * [-]onlcr      translate newline to carriage return-newline
 * [-]onlret     newline performs a carriage return
 * [-]onocr      do not print carriage returns in the first column
   [-]opost      postprocess output
 * tabN          horizontal tab delay style, N in [0..3]
 * tabs          same as tab0
 * -tabs         same as tab3
 * vtN           vertical tab delay style, N in [0..1]

Local settings:
   [-]crterase   echo erase characters as backspace-space-backspace
 * crtkill       kill all line by obeying the echoprt and echoe settings
 * -crtkill      kill all line by obeying the echoctl and echok settings
 * [-]ctlecho    echo control characters in hat notation ('^c')
   [-]echo       echo input characters
 * [-]echoctl    same as [-]ctlecho
   [-]echoe      same as [-]crterase
   [-]echok      echo a newline after a kill character
 * [-]echoke     same as [-]crtkill
   [-]echonl     echo newline even if not echoing other characters
 * [-]echoprt    echo erased characters backward, between '\\' and '/'
 * [-]extproc    enable \"LINEMODE\"; useful with high latency links
 * [-]flusho     discard output
   [-]icanon     enable special characters: erase, kill, werase, rprnt
   [-]iexten     enable non-POSIX special characters
   [-]isig       enable interrupt, quit, and suspend special characters
   [-]noflsh     disable flushing after interrupt and quit special characters
 * [-]prterase   same as [-]echoprt
 * [-]tostop     stop background jobs that try to write to the terminal
 * [-]xcase      with icanon, escape with '\\' for uppercase characters

Combination settings:
 * [-]LCASE      same as [-]lcase
   cbreak        same as -icanon
   -cbreak       same as icanon
   cooked        same as brkint ignpar istrip icrnl ixon opost isig
                 icanon, eof and eol characters to their default values
   -cooked       same as raw
   crt           same as echoe echoctl echoke
   dec           same as echoe echoctl echoke -ixany intr ^c erase 0177
                 kill ^u
 * [-]decctlq    same as [-]ixany
   ek            erase and kill characters to their default values
   evenp         same as parenb -parodd cs7
   -evenp        same as -parenb cs8
 * [-]lcase      same as xcase iuclc olcuc
   litout        same as -parenb -istrip -opost cs8
   -litout       same as parenb istrip opost cs7
   nl            same as -icrnl -onlcr
   -nl           same as icrnl -inlcr -igncr onlcr -ocrnl -onlret
   oddp          same as parenb parodd cs7
   -oddp         same as -parenb cs8
   [-]parity     same as [-]evenp
   pass8         same as -parenb -istrip cs8
   -pass8        same as parenb istrip cs7
   raw           same as -ignbrk -brkint -ignpar -parmrk -inpck -istrip
                 -inlcr -igncr -icrnl -ixon -ixoff -icanon -opost
                 -isig -iuclc -ixany -imaxbel -xcase min 1 time 0
   -raw          same as cooked
   sane          same as cread -ignbrk brkint -inlcr -igncr icrnl
                 icanon iexten echo echoe echok -echonl -noflsh
                 -ixoff -iutf8 -iuclc -ixany imaxbel -xcase -olcuc -ocrnl
                 opost -ofill onlcr -onocr -onlret nl0 cr0 tab0 bs0 vt0 ff0
                 isig -tostop -ofdel -echoprt echoctl echoke -extproc -flusho,
                 all special characters to their default values

Handle the tty line connected to standard input.  Without arguments,
prints baud rate, line discipline, and deviations from stty sane.  In
settings, CHAR is taken literally, or coded as in ^c, 0x37, 0177 or
127; special values ^- or undef used to disable special characters.

On Windows, the terminal is the console: echo, icanon, isig and opost are its
input and output modes and take effect at once; the other settings and the
special characters have no counterpart there, and are remembered for this shell
so that -a and -g print them back. rows and cols are accepted and ignored, and
size reports the console window. cash puts the console back before each prompt,
so a setting lasts for the current command line or script.
";

/// GNU's name for the device, in its errors.
const DEVICE: &str = "'standard input'";

/// What the command line asks for, once the options are told from the settings.
#[derive(Debug, Default, PartialEq, Eq)]
struct Request {
    all: bool,
    save: bool,
    help: bool,
    version: bool,
    /// `-F` or `--file`, which is refused.
    file: bool,
    settings: Vec<String>,
}

/// Tells GNU's options from the settings. `-a`, `-g` and the long options are options;
/// every other word, `-echo` included, is a setting.
fn request(args: &[String]) -> Request {
    let mut request = Request::default();
    let mut words = args.iter();
    while let Some(arg) = words.next() {
        match arg.as_str() {
            "-a" | "--all" => request.all = true,
            "-g" | "--save" => request.save = true,
            "-ag" | "-ga" => {
                request.all = true;
                request.save = true;
            }
            "--help" => request.help = true,
            "--version" => request.version = true,
            "-F" | "--file" => {
                request.file = true;
                words.next();
            }
            _ if arg.starts_with("--file=") => request.file = true,
            "--" => {}
            _ => request.settings.push(arg.clone()),
        }
    }
    request
}

impl builtins::Command for SttyCommand {
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
        let request = request(&self.args);
        if request.help {
            write!(context.stdout(), "{HELP}")?;
            return Ok(ExecutionResult::success());
        }
        if request.version {
            writeln!(
                context.stdout(),
                "stty (cash): GNU coreutils 9.11's words, on the Windows console modes"
            )?;
            return Ok(ExecutionResult::success());
        }
        if let Some(reason) = refused(&request) {
            writeln!(context.stderr(), "stty: {reason}")?;
            return Ok(ExecutionResult::general_error());
        }

        let is_terminal = context
            .try_fd(OpenFiles::STDIN_FD)
            .is_some_and(|file| file.is_terminal());
        if !is_terminal {
            writeln!(
                context.stderr(),
                "stty: {DEVICE}: Inappropriate ioctl for device"
            )?;
            return Ok(ExecutionResult::general_error());
        }
        let mode = match termios::current() {
            Ok(mode) => mode,
            Err(error) => {
                writeln!(context.stderr(), "stty: {}", device_error(&error))?;
                return Ok(ExecutionResult::general_error());
            }
        };
        let window = termios::window_size();
        let width = window.as_ref().map_or(80, |&(_, cols)| usize::from(cols));

        if request.save {
            writeln!(context.stdout(), "{}", recoverable(&mode))?;
            return Ok(ExecutionResult::success());
        }
        if request.all {
            let (rows, cols) = match window {
                Ok(size) => size,
                Err(error) => {
                    writeln!(context.stderr(), "stty: {}", device_error(&error))?;
                    return Ok(ExecutionResult::general_error());
                }
            };
            write!(
                context.stdout(),
                "{}",
                display_all(&mode, rows, cols, width)
            )?;
            return Ok(ExecutionResult::success());
        }
        if request.settings.is_empty() {
            write!(context.stdout(), "{}", display_changed(&mode, width))?;
            return Ok(ExecutionResult::success());
        }

        match plan(mode, &request.settings) {
            Ok(plan) => Ok(carry_out(
                &plan,
                &window,
                context.stdout(),
                context.stderr(),
            )?),
            Err(failure) => {
                writeln!(
                    context.stderr(),
                    "stty: {}\nTry 'stty --help' for more information.",
                    failure.message()
                )?;
                Ok(ExecutionResult::general_error())
            }
        }
    }
}

/// What stops a request before the terminal is looked at, in GNU's words.
const fn refused(request: &Request) -> Option<&'static str> {
    if request.file {
        Some(
            "--file is not supported: cash's stty acts on the console standard input is attached to",
        )
    } else if request.all && request.save {
        Some("the options for verbose and stty-readable output styles are\nmutually exclusive")
    } else if !request.settings.is_empty() && (request.all || request.save) {
        Some("when specifying an output style, modes may not be set")
    } else {
        None
    }
}

/// GNU's error about the device, after `stty: `.
fn device_error(error: &std::io::Error) -> String {
    format!("{DEVICE}: {}", cash_core::error::os_error_text(error))
}

/// Carries `plan` out: what it prints, then the settings on the console. An error is
/// written to `stderr`, and the status says so.
fn carry_out(
    plan: &Plan,
    window: &std::io::Result<(u16, u16)>,
    mut stdout: impl Write,
    mut stderr: impl Write,
) -> std::io::Result<ExecutionResult> {
    for print in &plan.prints {
        match print {
            Immediate::Size => match window {
                Ok((rows, cols)) => writeln!(stdout, "{rows} {cols}")?,
                Err(error) => {
                    writeln!(stderr, "stty: {}", device_error(error))?;
                    return Ok(ExecutionResult::general_error());
                }
            },
            Immediate::Speed(baud) => writeln!(stdout, "{baud}")?,
        }
    }
    if !plan.changed {
        return Ok(ExecutionResult::success());
    }
    if plan.restore_console
        && let Err(error) = cash_win32::console::restore_modes()
    {
        writeln!(
            stderr,
            "stty: could not restore the console modes: {}",
            cash_core::error::os_error_text(&error)
        )?;
    }
    if let Err(error) = termios::apply(&plan.mode) {
        writeln!(stderr, "stty: {}", device_error(&error))?;
        return Ok(ExecutionResult::general_error());
    }
    Ok(ExecutionResult::success())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_owned()).collect()
    }

    fn planned(args: &[&str]) -> Plan {
        plan(Termios::default(), &words(args)).unwrap()
    }

    #[test]
    fn the_default_settings_are_sane() {
        let mut mode = Termios::default();
        sane(&mut mode);
        assert_eq!(mode, Termios::default());
    }

    #[test]
    fn the_mapped_modes_are_the_consoles_bits() {
        assert_eq!(planned(&["-echo"]).mode.lflag & ECHO, 0);
        assert_eq!(planned(&["-icanon"]).mode.lflag & ICANON, 0);
        assert_eq!(planned(&["-isig"]).mode.lflag & ISIG, 0);
        assert_eq!(planned(&["-opost"]).mode.oflag & OPOST, 0);
        let raw = planned(&["raw"]).mode;
        assert_eq!(raw.iflag, 0);
        assert_eq!(raw.lflag & (ISIG | ICANON), 0);
        assert_ne!(raw.lflag & ECHO, 0, "GNU's raw leaves echo alone");
        assert_eq!(raw.oflag & OPOST, 0);
        // `raw` cleared every input flag; `sane` says nothing about `ixon`, so it stays
        // off, as on Linux.
        let back = planned(&["raw", "-echo", "sane"]);
        assert_eq!(
            back.mode,
            Termios {
                iflag: Termios::default().iflag & !IXON,
                ..Termios::default()
            }
        );
        assert!(back.restore_console);
        assert!(planned(&["-raw"]).restore_console);
        assert!(planned(&["cooked"]).restore_console);
        assert!(!planned(&["-cooked"]).restore_console);
    }

    #[test]
    fn several_settings_apply_in_order() {
        let plan = planned(&["-echo", "-icanon", "min", "1", "time", "0", "erase", "^H"]);
        assert_eq!(plan.mode.lflag & (ECHO | ICANON), 0);
        assert_eq!(plan.mode.cc[VMIN], 1);
        assert_eq!(plan.mode.cc[VTIME], 0);
        assert_eq!(plan.mode.cc[VERASE], 8);
        assert!(plan.changed);
        assert_eq!(plan.prints, Vec::new());
    }

    #[test]
    fn special_characters_are_read_as_gnu_reads_them() {
        let erase = &CONTROLS[2];
        assert_eq!(control_char(erase, "^?"), Ok(127));
        assert_eq!(control_char(erase, "^h"), Ok(8));
        assert_eq!(control_char(erase, "^H"), Ok(8));
        assert_eq!(control_char(erase, "^-"), Ok(0));
        assert_eq!(control_char(erase, "undef"), Ok(0));
        assert_eq!(control_char(erase, "x"), Ok(b'x'));
        assert_eq!(control_char(erase, "0177"), Ok(127));
        assert_eq!(control_char(erase, "0x7f"), Ok(127));
        assert_eq!(control_char(erase, "127"), Ok(127));
        assert_eq!(
            control_char(erase, "abc"),
            Err(Failure::InvalidInteger("abc".to_owned()))
        );
        let min = &CONTROLS[15];
        assert_eq!(
            control_char(min, "x"),
            Err(Failure::InvalidInteger("x".to_owned()))
        );
        assert_eq!(
            control_char(min, "300"),
            Err(Failure::InvalidInteger("300".to_owned()))
        );
    }

    #[test]
    fn special_characters_are_shown_as_gnu_shows_them() {
        assert_eq!(visible(0), "<undef>");
        assert_eq!(visible(3), "^C");
        assert_eq!(visible(0x1c), "^\\");
        assert_eq!(visible(127), "^?");
        assert_eq!(visible(b'x'), "x");
        assert_eq!(visible(128), "M-^@");
        assert_eq!(visible(128 + b'a'), "M-a");
        assert_eq!(visible(255), "M-^?");
    }

    #[test]
    fn what_gnu_refuses_is_refused_in_its_words() {
        let failed = |args: &[&str]| plan(Termios::default(), &words(args)).unwrap_err();
        assert_eq!(failed(&["foo"]), Failure::Invalid("foo".to_owned()));
        assert_eq!(failed(&["-cs8"]), Failure::Invalid("-cs8".to_owned()));
        assert_eq!(failed(&["-sane"]), Failure::Invalid("-sane".to_owned()));
        assert_eq!(failed(&["-erase"]), Failure::Invalid("-erase".to_owned()));
        assert_eq!(
            failed(&["erase"]),
            Failure::MissingArgument("erase".to_owned())
        );
        assert_eq!(
            failed(&["rows"]),
            Failure::MissingArgument("rows".to_owned())
        );
        assert_eq!(
            failed(&["rows", "x"]),
            Failure::InvalidInteger("x".to_owned())
        );
        assert_eq!(
            failed(&["ispeed", "1234"]),
            Failure::InvalidSpeed("ispeed", "1234".to_owned())
        );
        assert_eq!(failed(&["1:2:3"]), Failure::Invalid("1:2:3".to_owned()));
        assert_eq!(failed(&["foo"]).message(), "invalid argument 'foo'");
    }

    #[test]
    fn rows_and_cols_are_accepted_and_change_nothing() {
        let plan = planned(&["rows", "50", "cols", "132", "columns", "80"]);
        assert!(!plan.changed);
        assert_eq!(plan.mode, Termios::default());
        assert!(!planned(&["drain", "-drain"]).changed);
    }

    #[test]
    fn size_and_speed_print_where_they_stand() {
        let plan = planned(&["size", "9600", "speed"]);
        assert_eq!(plan.prints, [Immediate::Size, Immediate::Speed("9600")]);
        assert_eq!(plan.mode.cflag & CBAUD, 0o15);
        assert!(plan.changed);
        let plan = planned(&["ispeed", "0", "ospeed", "115200"]);
        assert_eq!(plan.mode.cflag & CBAUD, 0o10_002);
        assert_eq!(baud_of(plan.mode.cflag & CBAUD), "115200");
    }

    #[test]
    fn a_saved_string_restores_the_settings() {
        let saved = recoverable(&Termios::default());
        assert_eq!(
            saved,
            "2502:5:bf:8a3b:3:1c:7f:15:4:0:1:0:11:13:1a:0:12:f:17:16:0:0:0:0:0:0:0:0:0:0:0:0:0:0:0:0"
        );
        let changed = planned(&["raw", "-echo", "erase", "^H", "line", "7"]).mode;
        let mut restored = Termios::default();
        assert!(recover_mode(&recoverable(&changed), &mut restored));
        // The line discipline is not part of the string, as in GNU.
        assert_eq!(restored, Termios { line: 0, ..changed });
        assert_eq!(
            plan(Termios::default(), &words(&[saved.as_str()]))
                .unwrap()
                .mode,
            Termios::default()
        );

        let mut untouched = Termios::default();
        assert!(!recover_mode("1:2:3:4", &mut untouched));
        assert!(!recover_mode(
            "x:5:bf:8a3b:3:1c:7f:15:4:0:1:0:11:13:1a:0:12:f:17:16:0:0:0:0:0:0:0:0:0:0:0:0:0:0:0:0",
            &mut untouched
        ));
        assert!(!recover_mode(
            "2502:5:bf:8a3b:300:1c:7f:15:4:0:1:0:11:13:1a:0:12:f:17:16:0:0:0:0:0:0:0:0:0:0:0:0:0:0:0:0",
            &mut untouched
        ));
        assert_eq!(untouched, Termios::default());
    }

    #[test]
    fn bare_stty_shows_only_what_differs_from_sane() {
        let mode = Termios::default();
        assert_eq!(display_changed(&mode, 80), "speed 38400 baud; line = 0;\n");
        let mode = planned(&["-echo"]).mode;
        assert_eq!(
            display_changed(&mode, 80),
            "speed 38400 baud; line = 0;\n-echo\n"
        );
        let mode = planned(&["raw"]).mode;
        assert_eq!(
            display_changed(&mode, 80),
            "speed 38400 baud; line = 0;\nmin = 1; time = 0;\n-brkint -icrnl -imaxbel\n-opost\n-isig -icanon\n"
        );
        let mode = planned(&["erase", "^H", "9600", "hupcl", "iutf8"]).mode;
        assert_eq!(
            display_changed(&mode, 80),
            "speed 9600 baud; line = 0;\nerase = ^H;\niutf8\n"
        );
    }

    #[test]
    fn stty_a_has_gnus_layout() {
        let expected = "\
speed 38400 baud; rows 24; columns 80; line = 0;
intr = ^C; quit = ^\\; erase = ^?; kill = ^U; eof = ^D; eol = <undef>;
eol2 = <undef>; swtch = <undef>; start = ^Q; stop = ^S; susp = ^Z; rprnt = ^R;
werase = ^W; lnext = ^V; discard = ^O; min = 1; time = 0;
-parenb -parodd -cmspar cs8 -hupcl -cstopb cread -clocal -crtscts
-ignbrk brkint -ignpar -parmrk -inpck -istrip -inlcr -igncr icrnl ixon -ixoff
-iuclc -ixany imaxbel -iutf8
opost -olcuc -ocrnl onlcr -onocr -onlret -ofill -ofdel nl0 cr0 tab0 bs0 vt0 ff0
isig icanon iexten echo echoe echok -echonl -noflsh -xcase -tostop -echoprt
echoctl echoke -flusho -extproc
";
        assert_eq!(display_all(&Termios::default(), 24, 80, 80), expected);

        let mode = planned(&["-echo", "cs7", "tab3"]).mode;
        let shown = display_all(&mode, 30, 120, 120);
        assert!(shown.starts_with("speed 38400 baud; rows 30; columns 120; line = 0;\n"));
        assert!(shown.contains(" cs7 "), "{shown}");
        assert!(shown.contains(" tab3 "), "{shown}");
        assert!(shown.contains(" -echo "), "{shown}");
        for line in shown.lines() {
            assert!(line.len() < 120, "{line}");
        }
    }

    #[test]
    fn options_are_told_from_settings() {
        let asked = request(&words(&["-a"]));
        assert!(asked.all && !asked.save && asked.settings.is_empty());
        let asked = request(&words(&["-echo", "-g", "--save", "raw"]));
        assert!(asked.save);
        assert_eq!(asked.settings, ["-echo", "raw"]);
        assert!(request(&words(&["--file=COM1"])).file);
        assert!(request(&words(&["-F", "COM1"])).file);
        assert!(request(&words(&["--help"])).help);
        assert!(request(&words(&["--version"])).version);
    }
}
