//! `tput`, as ncurses 6.6 answers it for `xterm-256color`, without terminfo.
//!
//! ConPTY and Windows Terminal speak VT, so the capabilities of `xterm-256color` are built
//! in: every one `infocmp -x` lists, the parametrised ones through a small `tparm`
//! ([`expand`]), and the numbers from the console. Checked against ncurses 6.6.20251230
//! (`crates/cash/tests/oracle/tput_cases.sh`): the bytes; the exit codes, 1 for a false
//! boolean or a string the terminal lacks, 2 for usage, 3 for an unknown terminal, 4 for
//! an unknown capability, and under `-S` 4 plus the number of failing lines; a
//! parametrised capability given no parameters printing its raw string; and the numbers
//! after a string capability being its parameters, with what follows them further
//! capabilities, as `tput cup 5 10 bold` writes both.
//!
//! Differences, deliberate: a `TERM` that is unset or not VT-like is ignored, since the
//! terminal cash runs in is VT whatever `TERM` says, and only an explicit `-T` is checked
//! (any xterm-, vt- or ms-terminal-like name, there being no terminfo to consult);
//! `longname` is always `xterm with 256 colors`; `reset` also puts back the console modes
//! cash relies on, as cash's `reset` does; `cols` and `lines` are the console window's
//! (an exported `COLUMNS` or `LINES` first, as ncurses honours them), 80 and 24 without
//! a console; `--help` prints the usage and succeeds.

use std::borrow::Cow;
use std::io::{Read, Write};

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

use crate::screen::{CLEAR, CLEAR_KEEP_SCROLLBACK, is_vt};

/// ncurses' usage text, to standard error with status 2.
const USAGE: &str = "Usage: tput [options] [command]\n\nOptions:\n  -S <<       read commands from standard input\n  -T TERM     use this instead of $TERM\n  -V          print curses-version\n  -v          verbose, show warnings\n  -x          do not try to clear scrollback\n\nCommands:\n  clear       clear the screen\n  init        initialize the terminal\n  reset       reinitialize the terminal\n  capname     unlike clear/init/reset, print value for capability \"capname\"\n";

/// The terminal's long name.
const LONGNAME: &str = "xterm with 256 colors";

/// The booleans xterm-256color has (`infocmp -x`).
const TRUE_BOOLEANS: &str = "am bce ccc km mc5i mir msgr npc xenl AX XF XT OTbs";
/// Every boolean terminfo names; those not in [`TRUE_BOOLEANS`] are false here.
const BOOLEANS: &str = "bw am xsb xhp xenl eo gn hc km hs in da db mir msgr os eslok xt hz ul xon nxon mc5i chts nrrmc npc ndscr ccc bce hls xhpa crxm daisy xvpa sam cpix lpix";
/// Every number terminfo names; those not answered in [`number`] are -1 here.
const NUMBERS: &str = "cols it lines lm xmc pb vt wsl nlab lh lw ma wnum colors pairs ncv bufsz spinv spinh maddr mjump mcs mls npins orc orl orhi orvi cps widcs btns bitwin bitype";
/// Every string terminfo names; those xterm-256color lacks print nothing, with status 1.
const STRINGS: &str = "cbt bel cr csr tbc clear el ed hpa cmdch cup cud1 home civis cub1 mrcup cnorm cuf1 ll cuu1 cvvis dch1 dl1 dsl hd smacs blink bold smcup smdc dim smir invis prot rev smso smul ech rmacs sgr0 rmcup rmdc rmir rmso rmul flash ff fsl is1 is2 is3 if ich1 il1 ip kbs ktbc kclr kctab kdch1 kdl1 kcud1 krmir kel ked kf0 kf1 kf10 kf2 kf3 kf4 kf5 kf6 kf7 kf8 kf9 khome kich1 kil1 kcub1 kll knp kpp kcuf1 kind kri khts kcuu1 rmkx smkx lf0 lf1 lf10 lf2 lf3 lf4 lf5 lf6 lf7 lf8 lf9 rmm smm nel pad dch dl cud ich indn il cub cuf rin cuu pfkey pfloc pfx mc0 mc4 mc5 rep rs1 rs2 rs3 rf rc vpa sc ind ri sgr hts wind ht tsl uc hu iprog ka1 ka3 kb2 kc1 kc3 mc5p rmp acsc pln kcbt smxon rmxon smam rmam xonc xoffc enacs smln rmln kbeg kcan kclo kcmd kcpy kcrt kend kent kext kfnd khlp kmrk kmsg kmov knxt kopn kopt kprv kprt krdo kref krfr krpl krst kres ksav kspd kund kBEG kCAN kCMD kCPY kCRT kDC kDL kslt kEND kEOL kEXT kFND kHLP kHOM kIC kLFT kMSG kMOV kNXT kOPT kPRV kPRT kRDO kRPL kRIT kRES kSAV kSPD kUND rfi kf11 kf12 kf13 kf14 kf15 kf16 kf17 kf18 kf19 kf20 kf21 kf22 kf23 kf24 kf25 kf26 kf27 kf28 kf29 kf30 kf31 kf32 kf33 kf34 kf35 kf36 kf37 kf38 kf39 kf40 kf41 kf42 kf43 kf44 kf45 kf46 kf47 kf48 kf49 kf50 kf51 kf52 kf53 kf54 kf55 kf56 kf57 kf58 kf59 kf60 kf61 kf62 kf63 el1 mgc smgl smgr fln sclk dclk rmclk cwin wingo hup dial qdial tone pulse hook pause wait u0 u1 u2 u3 u4 u5 u6 u7 u8 u9 op oc initc initp scp setf setb cpi lpi chr cvr defc swidm sdrfq sitm slm smicm snlq snrmq sshm ssubm ssupm sum rwidm ritm rlm rmicm rshm rsubm rsupm rum mhpa mcud1 mcub1 mcuf1 mvpa mcuu1 porder mcud mcub mcuf mcuu scs smgb smgbp smglp smgrp smgt smgtp sbim scsd rbim rcsd subcs supcs docr zerom csnm kmous minfo reqmp getm setaf setab pfxl devt csin s0ds s1ds s2ds s3ds smglr smgtb birep binel bicr colornm defbi endbi setcolor slines dispc smpch rmpch smsc rmsc pctrm scesc scesa ehhlm elhlm elohlm erhlm ethlm evhlm sgr1 slength";

/// The string capabilities of xterm-256color, as `infocmp -x` writes them, with `\E` as
/// ESC. The function keys past F12 and the modified cursor keys are made in
/// [`function_key`] and [`modified_key`]; `clear`, `init` and `reset` are commands.
const CAPABILITIES: &[(&str, &[u8])] = &[
    ("acsc", b"``aaffggiijjkkllmmnnooppqqrrssttuuvvwwxxyyzz{{||}}~~"),
    ("bel", b"\x07"),
    ("blink", b"\x1b[5m"),
    ("bold", b"\x1b[1m"),
    ("cbt", b"\x1b[Z"),
    ("civis", b"\x1b[?25l"),
    ("clear", b"\x1b[H\x1b[2J"),
    ("cnorm", b"\x1b[?12l\x1b[?25h"),
    ("cr", b"\r"),
    ("csr", b"\x1b[%i%p1%d;%p2%dr"),
    ("cub", b"\x1b[%p1%dD"),
    ("cub1", b"\x08"),
    ("cud", b"\x1b[%p1%dB"),
    ("cud1", b"\n"),
    ("cuf", b"\x1b[%p1%dC"),
    ("cuf1", b"\x1b[C"),
    ("cup", b"\x1b[%i%p1%d;%p2%dH"),
    ("cuu", b"\x1b[%p1%dA"),
    ("cuu1", b"\x1b[A"),
    ("cvvis", b"\x1b[?12;25h"),
    ("dch", b"\x1b[%p1%dP"),
    ("dch1", b"\x1b[P"),
    ("dim", b"\x1b[2m"),
    ("dl", b"\x1b[%p1%dM"),
    ("dl1", b"\x1b[M"),
    ("ech", b"\x1b[%p1%dX"),
    ("ed", b"\x1b[J"),
    ("el", b"\x1b[K"),
    ("el1", b"\x1b[1K"),
    ("flash", b"\x1b[?5h$<100/>\x1b[?5l"),
    ("home", b"\x1b[H"),
    ("hpa", b"\x1b[%i%p1%dG"),
    ("ht", b"\t"),
    ("hts", b"\x1bH"),
    ("ich", b"\x1b[%p1%d@"),
    ("il", b"\x1b[%p1%dL"),
    ("il1", b"\x1b[L"),
    ("ind", b"\n"),
    ("indn", b"\x1b[%p1%dS"),
    (
        "initc",
        b"\x1b]4;%p1%d;rgb:%p2%{255}%*%{1000}%/%2.2X/%p3%{255}%*%{1000}%/%2.2X/%p4%{255}%*%{1000}%/%2.2X\x1b\\",
    ),
    ("invis", b"\x1b[8m"),
    ("is2", b"\x1b[!p\x1b[?3;4l\x1b[4l\x1b>"),
    ("ka1", b"\x1bOw"),
    ("ka2", b"\x1bOx"),
    ("ka3", b"\x1bOy"),
    ("kb1", b"\x1bOt"),
    ("kb2", b"\x1bOu"),
    ("kb3", b"\x1bOv"),
    ("kbeg", b"\x1bOE"),
    ("kbs", b"\x7f"),
    ("kc1", b"\x1bOq"),
    ("kc2", b"\x1bOr"),
    ("kc3", b"\x1bOs"),
    ("kcbt", b"\x1b[Z"),
    ("kcub1", b"\x1bOD"),
    ("kcud1", b"\x1bOB"),
    ("kcuf1", b"\x1bOC"),
    ("kcuu1", b"\x1bOA"),
    ("kdch1", b"\x1b[3~"),
    ("kend", b"\x1bOF"),
    ("kent", b"\x1bOM"),
    ("kf1", b"\x1bOP"),
    ("kf2", b"\x1bOQ"),
    ("kf3", b"\x1bOR"),
    ("kf4", b"\x1bOS"),
    ("kf5", b"\x1b[15~"),
    ("kf6", b"\x1b[17~"),
    ("kf7", b"\x1b[18~"),
    ("kf8", b"\x1b[19~"),
    ("kf9", b"\x1b[20~"),
    ("kf10", b"\x1b[21~"),
    ("kf11", b"\x1b[23~"),
    ("kf12", b"\x1b[24~"),
    ("khome", b"\x1bOH"),
    ("kich1", b"\x1b[2~"),
    ("kind", b"\x1b[1;2B"),
    ("kmous", b"\x1b[<"),
    ("knp", b"\x1b[6~"),
    ("kp5", b"\x1bOE"),
    ("kpADD", b"\x1bOk"),
    ("kpCMA", b"\x1bOl"),
    ("kpDIV", b"\x1bOo"),
    ("kpDOT", b"\x1bOn"),
    ("kpMUL", b"\x1bOj"),
    ("kpSUB", b"\x1bOm"),
    ("kpZRO", b"\x1bOp"),
    ("kpp", b"\x1b[5~"),
    ("kri", b"\x1b[1;2A"),
    ("kxIN", b"\x1b[I"),
    ("kxOUT", b"\x1b[O"),
    ("mc0", b"\x1b[i"),
    ("mc4", b"\x1b[4i"),
    ("mc5", b"\x1b[5i"),
    ("meml", b"\x1bl"),
    ("memu", b"\x1bm"),
    ("mgc", b"\x1b[?69l"),
    ("nel", b"\x1bE"),
    ("oc", b"\x1b]104\x07"),
    ("op", b"\x1b[39;49m"),
    ("rc", b"\x1b8"),
    ("rep", b"%p1%c\x1b[%p2%{1}%-%db"),
    ("rev", b"\x1b[7m"),
    ("ri", b"\x1bM"),
    ("rin", b"\x1b[%p1%dT"),
    ("ritm", b"\x1b[23m"),
    ("rmacs", b"\x1b(B"),
    ("rmam", b"\x1b[?7l"),
    ("rmcup", b"\x1b[?1049l\x1b[23;0;0t"),
    ("rmir", b"\x1b[4l"),
    ("rmkx", b"\x1b[?1l\x1b>"),
    ("rmm", b"\x1b[?1034l"),
    ("rmso", b"\x1b[27m"),
    ("rmul", b"\x1b[24m"),
    ("rmxx", b"\x1b[29m"),
    ("rs1", b"\x1bc\x1b]104\x07"),
    ("rs2", b"\x1b[!p\x1b[?3;4l\x1b[4l\x1b>"),
    ("sc", b"\x1b7"),
    (
        "setab",
        b"\x1b[%?%p1%{8}%<%t4%p1%d%e%p1%{16}%<%t10%p1%{8}%-%d%e48;5;%p1%d%;m",
    ),
    (
        "setaf",
        b"\x1b[%?%p1%{8}%<%t3%p1%d%e%p1%{16}%<%t9%p1%{8}%-%d%e38;5;%p1%d%;m",
    ),
    (
        "sgr",
        b"%?%p9%t\x1b(0%e\x1b(B%;\x1b[0%?%p6%t;1%;%?%p5%t;2%;%?%p2%t;4%;%?%p1%p3%|%t;7%;%?%p4%t;5%;%?%p7%t;8%;m",
    ),
    ("sgr0", b"\x1b(B\x1b[m"),
    ("sitm", b"\x1b[3m"),
    ("smacs", b"\x1b(0"),
    ("smam", b"\x1b[?7h"),
    ("smcup", b"\x1b[?1049h\x1b[22;0;0t"),
    ("smglp", b"\x1b[?69h\x1b[%i%p1%ds"),
    ("smglr", b"\x1b[?69h\x1b[%i%p1%d;%p2%ds"),
    ("smgrp", b"\x1b[?69h\x1b[%i;%p1%ds"),
    ("smir", b"\x1b[4h"),
    ("smkx", b"\x1b[?1h\x1b="),
    ("smm", b"\x1b[?1034h"),
    ("smso", b"\x1b[7m"),
    ("smul", b"\x1b[4m"),
    ("smxx", b"\x1b[9m"),
    ("tbc", b"\x1b[3g"),
    ("u6", b"\x1b[%i%d;%dR"),
    ("u7", b"\x1b[6n"),
    ("u8", b"\x1b[?%[;0123456789]c"),
    ("u9", b"\x1b[c"),
    ("vpa", b"\x1b[%i%p1%dd"),
    // xterm's extensions, which `infocmp -x` lists.
    ("BD", b"\x1b[?2004l"),
    ("BE", b"\x1b[?2004h"),
    ("Cr", b"\x1b]112\x07"),
    ("Cs", b"\x1b]12;%p1%s\x07"),
    ("E3", b"\x1b[3J"),
    ("Ms", b"\x1b]52;%p1%s;%p2%s\x07"),
    ("PE", b"\x1b[201~"),
    ("PS", b"\x1b[200~"),
    ("RV", b"\x1b[>c"),
    ("Se", b"\x1b[2 q"),
    ("Ss", b"\x1b[%p1%d q"),
    ("XM", b"\x1b[?1006;1000%?%p1%{1}%=%th%el%;"),
    ("XR", b"\x1b[>0q"),
    ("fd", b"\x1b[?1004l"),
    ("fe", b"\x1b[?1004h"),
    ("rv", b"\x1b\\[>41;[1-6][0-9][0-9];0c"),
    ("xm", b"\x1b[<%i%p3%d;%p1%d;%p2%d;%?%p4%tM%em%;"),
    ("xr", b"\x1bP>\\|XTerm\\(([1-9][0-9]+)\\)\x1b\\\\"),
];

/// `kf13` to `kf63`: F1 to F12 with a modifier, twelve keys per modifier in the order
/// shift (2), control (5), control-shift (6), alt (3) and alt-shift (4).
fn function_key(name: &str) -> Option<Vec<u8>> {
    let number: usize = name.strip_prefix("kf")?.parse().ok()?;
    if !(13..=63).contains(&number) {
        return None;
    }
    let modifier = b"25634"[(number - 13) / 12];
    let key = (number - 13) % 12;
    if key < 4 {
        return Some(vec![0x1b, b'[', b'1', b';', modifier, b"PQRS"[key]]);
    }
    let code: &[u8] = [
        b"15".as_slice(),
        b"17",
        b"18",
        b"19",
        b"20",
        b"21",
        b"23",
        b"24",
    ][key - 4];
    let mut bytes = vec![0x1b, b'['];
    bytes.extend_from_slice(code);
    bytes.extend_from_slice(&[b';', modifier, b'~']);
    Some(bytes)
}

/// The cursor and editing keys with a modifier: `kDC` to `kUP` for shift, and `kDC3` to
/// `kUP7` for alt, alt-shift, control, control-shift and control-alt.
fn modified_key(name: &str) -> Option<Vec<u8>> {
    let last = name.as_bytes().last().copied().unwrap_or(b'2');
    let (key, modifier) = match name.strip_suffix(['3', '4', '5', '6', '7']) {
        Some(key) => (key, last),
        None => (name, b'2'),
    };
    let (code, last): (&[u8], u8) = match key {
        "kDC" => (b"3", b'~'),
        "kDN" => (b"1", b'B'),
        "kEND" => (b"1", b'F'),
        "kHOM" => (b"1", b'H'),
        "kIC" => (b"2", b'~'),
        "kLFT" => (b"1", b'D'),
        "kNXT" => (b"6", b'~'),
        "kPRV" => (b"5", b'~'),
        "kRIT" => (b"1", b'C'),
        "kUP" => (b"1", b'A'),
        _ => return None,
    };
    let mut bytes = vec![0x1b, b'['];
    bytes.extend_from_slice(code);
    bytes.extend_from_slice(&[b';', modifier, last]);
    Some(bytes)
}

/// The string capability `name`, as terminfo writes it, if xterm-256color has it.
fn capability(name: &str) -> Option<Cow<'static, [u8]>> {
    CAPABILITIES
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, bytes)| Cow::Borrowed(*bytes))
        .or_else(|| function_key(name).map(Cow::Owned))
        .or_else(|| modified_key(name).map(Cow::Owned))
}

/// Whether `list`, a space-separated list of names, has `name`.
fn lists(list: &str, name: &str) -> bool {
    list.split(' ').any(|known| known == name)
}

/// A number as C's `strtol(text, &end, 0)` reads it, when it reads all of `text`: leading
/// white space, a sign, then hex after `0x`, octal after `0`, or decimal; saturating as
/// `strtol` does.
fn strtol(text: &str) -> Option<i64> {
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
    if digits.is_empty() {
        return None;
    }
    let mut value: i64 = 0;
    for c in digits.chars() {
        let digit = i64::from(c.to_digit(radix)?);
        value = value.saturating_mul(radix.into()).saturating_add(digit);
    }
    Some(if negative {
        value.saturating_neg()
    } else {
        value
    })
}

/// The low 32 bits of `value`, as C's `int` holds a `long` parameter.
fn truncate(value: i64) -> i32 {
    u32::try_from(value & 0xFFFF_FFFF).map_or(0, u32::cast_signed)
}

/// Whether a word after a string capability is one of its parameters rather than the
/// next capability: a number `strtol` reads whole, not negative.
fn is_parameter(word: &str) -> bool {
    strtol(word).is_some_and(|value| value >= 0)
}

/// Whether `template`'s parameter `index` (from 0) is text, as `%p1%s` makes it: then the
/// word given for it is taken whatever it is, as ncurses takes `Cs red`.
fn takes_text(template: &[u8], index: usize) -> bool {
    let Ok(digit) = u8::try_from(index + 1) else {
        return false;
    };
    template.windows(5).any(|w| {
        w[0] == b'%' && w[1] == b'p' && w[2] == b'0' + digit && w[3] == b'%' && w[4] == b's'
    })
}

/// What is on the `tparm` stack: a number, or a parameter, which is a number or text as
/// the format that pops it asks.
#[derive(Clone, Copy)]
enum Value {
    Number(i32),
    Param(usize),
}

/// The printf-style conversion of a `tparm` format: `%[:][-+# ][width][.precision]c`.
#[derive(Default)]
struct Format {
    left: bool,
    plus: bool,
    space: bool,
    alternate: bool,
    width: usize,
    precision: Option<usize>,
}

impl Format {
    /// `value` as this format writes it, with the conversion `conversion`.
    fn number(&self, value: i32, conversion: u8) -> Vec<u8> {
        let (sign, mut digits) = match conversion {
            b'd' => (
                if value < 0 {
                    "-"
                } else if self.plus {
                    "+"
                } else if self.space {
                    " "
                } else {
                    ""
                },
                value.unsigned_abs().to_string(),
            ),
            b'o' => ("", format!("{:o}", value.cast_unsigned())),
            b'x' => ("", format!("{:x}", value.cast_unsigned())),
            _ => ("", format!("{:X}", value.cast_unsigned())),
        };
        if let Some(precision) = self.precision {
            while digits.len() < precision {
                digits.insert(0, '0');
            }
        }
        let prefix = match conversion {
            b'o' if self.alternate && !digits.starts_with('0') => "0",
            b'x' if self.alternate && value != 0 => "0x",
            b'X' if self.alternate && value != 0 => "0X",
            _ => "",
        };
        self.pad(format!("{sign}{prefix}{digits}").into_bytes())
    }

    /// `text` as this format writes it.
    fn text(&self, text: &str) -> Vec<u8> {
        let shown = match self.precision {
            Some(precision) => text.chars().take(precision).collect::<String>(),
            None => text.to_owned(),
        };
        self.pad(shown.into_bytes())
    }

    fn pad(&self, mut body: Vec<u8>) -> Vec<u8> {
        if body.len() >= self.width {
            return body;
        }
        let fill = vec![b' '; self.width - body.len()];
        if self.left {
            body.extend(fill);
            body
        } else {
            [fill, body].concat()
        }
    }
}

/// How many values a termcap-style string, one with `%d`s and no `%p`s, pops: that many
/// parameters are pushed before it runs, the first one deepest.
fn termcap_pops(template: &[u8]) -> usize {
    let mut pops = 0;
    let mut i = 0;
    while i + 1 < template.len() {
        if template[i] == b'%' {
            let mut j = i + 1;
            while j < template.len() && !template[j].is_ascii_alphabetic() && template[j] != b'%' {
                j += 1;
            }
            if let Some(b'd' | b'o' | b'x' | b'X' | b's' | b'c') = template.get(j) {
                pops += 1;
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    pops
}

/// Skips, from `i`, past the `%e` or `%;` that ends the branch a false `%t` opened
/// (`to_else`), or past the `%;` that ends the `%?` an executed `%e` belongs to.
fn skip_branch(template: &[u8], mut i: usize, to_else: bool) -> usize {
    let mut level = 0usize;
    while i < template.len() {
        if template[i] != b'%' {
            i += 1;
            continue;
        }
        let Some(&op) = template.get(i + 1) else {
            return template.len();
        };
        i += 2;
        match op {
            b'?' => level += 1,
            b';' if level > 0 => level -= 1,
            b';' => return i,
            b'e' if to_else && level == 0 => return i,
            _ => {}
        }
    }
    i
}

/// `template` with its parameters put in, as ncurses' `tparm` does: `%p1` to `%p9`,
/// `%{n}`, `%'c'`, the arithmetic, comparisons and logic, `%?%t%e%;`, `%i`, the
/// variables `%P`/`%g`, and printf-style `%d`, `%o`, `%x`, `%X`, `%s` and `%c`. A `%` with
/// nothing it knows after it writes nothing, as `tparm` does with `u8`'s `%[`.
#[expect(clippy::too_many_lines, reason = "one arm per tparm operator")]
fn expand(template: &[u8], params: &[(i32, &str)]) -> Vec<u8> {
    let mut numbers = [0i32; 9];
    let mut texts = [""; 9];
    for (i, (number, text)) in params.iter().take(9).enumerate() {
        numbers[i] = *number;
        texts[i] = text;
    }
    let mut stack: Vec<Value> = Vec::new();
    if !template.windows(2).any(|pair| pair == b"%p") {
        for i in 0..termcap_pops(template).min(9) {
            stack.push(Value::Param(i));
        }
    }
    let mut dynamic = [0i32; 26];
    let mut fixed = [0i32; 26];
    let mut out = Vec::with_capacity(template.len() + 16);
    let mut i = 0;
    while let Some(&byte) = template.get(i) {
        i += 1;
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        let pop_number = |stack: &mut Vec<Value>| match stack.pop() {
            Some(Value::Number(n)) => n,
            Some(Value::Param(p)) => numbers[p],
            None => 0,
        };
        let pop_text = |stack: &mut Vec<Value>| match stack.pop() {
            Some(Value::Param(p)) => texts[p],
            Some(Value::Number(_)) | None => "",
        };
        let mut format = Format::default();
        // Flags only follow `%:`, which is what keeps `%-` and `%+` the operators.
        if template.get(i) == Some(&b':') {
            i += 1;
            while let Some(flag @ (b'-' | b'+' | b'#' | b' ')) = template.get(i) {
                match flag {
                    b'-' => format.left = true,
                    b'+' => format.plus = true,
                    b'#' => format.alternate = true,
                    _ => format.space = true,
                }
                i += 1;
            }
        }
        while let Some(digit) = template.get(i).filter(|b| b.is_ascii_digit()) {
            format.width = format.width * 10 + usize::from(digit - b'0');
            i += 1;
        }
        if template.get(i) == Some(&b'.') {
            i += 1;
            let mut precision = 0;
            while let Some(digit) = template.get(i).filter(|b| b.is_ascii_digit()) {
                precision = precision * 10 + usize::from(digit - b'0');
                i += 1;
            }
            format.precision = Some(precision);
        }
        let Some(&op) = template.get(i) else {
            break;
        };
        i += 1;
        match op {
            b'%' => out.push(b'%'),
            b'd' | b'o' | b'x' | b'X' => {
                let value = pop_number(&mut stack);
                out.extend(format.number(value, op));
            }
            b's' => {
                let text = pop_text(&mut stack);
                out.extend(format.text(text));
            }
            b'c' => {
                // A NUL cannot be in a C string, so ncurses writes 0200 for it.
                let value = pop_number(&mut stack);
                let c = value.cast_unsigned().to_le_bytes()[0];
                out.push(if c == 0 { 0o200 } else { c });
            }
            b'p' => {
                if let Some(index @ b'1'..=b'9') = template.get(i) {
                    i += 1;
                    stack.push(Value::Param(usize::from(index - b'1')));
                }
            }
            b'P' | b'g' => {
                if let Some(&name) = template.get(i) {
                    i += 1;
                    let slot = match name {
                        b'a'..=b'z' => Some((&mut dynamic, usize::from(name - b'a'))),
                        b'A'..=b'Z' => Some((&mut fixed, usize::from(name - b'A'))),
                        _ => None,
                    };
                    if let Some((variables, slot)) = slot {
                        if op == b'P' {
                            variables[slot] = pop_number(&mut stack);
                        } else {
                            stack.push(Value::Number(variables[slot]));
                        }
                    }
                }
            }
            b'\'' => {
                if let Some(&c) = template.get(i) {
                    stack.push(Value::Number(i32::from(c)));
                    i += 1;
                    if template.get(i) == Some(&b'\'') {
                        i += 1;
                    }
                }
            }
            b'{' => {
                let mut value: i32 = 0;
                let negative = template.get(i) == Some(&b'-');
                if negative {
                    i += 1;
                }
                while let Some(digit) = template.get(i).filter(|b| b.is_ascii_digit()) {
                    value = value.wrapping_mul(10).wrapping_add(i32::from(digit - b'0'));
                    i += 1;
                }
                if template.get(i) == Some(&b'}') {
                    i += 1;
                }
                stack.push(Value::Number(if negative {
                    value.wrapping_neg()
                } else {
                    value
                }));
            }
            b'l' => {
                let length = pop_text(&mut stack).len();
                stack.push(Value::Number(i32::try_from(length).unwrap_or(i32::MAX)));
            }
            b'+' | b'-' | b'*' | b'/' | b'm' | b'&' | b'|' | b'^' | b'=' | b'<' | b'>' | b'A'
            | b'O' => {
                let y = pop_number(&mut stack);
                let x = pop_number(&mut stack);
                stack.push(Value::Number(match op {
                    b'+' => x.wrapping_add(y),
                    b'-' => x.wrapping_sub(y),
                    b'*' => x.wrapping_mul(y),
                    b'/' => x.checked_div(y).unwrap_or(0),
                    b'm' => x.checked_rem(y).unwrap_or(0),
                    b'&' => x & y,
                    b'|' => x | y,
                    b'^' => x ^ y,
                    b'=' => i32::from(x == y),
                    b'<' => i32::from(x < y),
                    b'>' => i32::from(x > y),
                    b'A' => i32::from(x != 0 && y != 0),
                    _ => i32::from(x != 0 || y != 0),
                }));
            }
            b'!' => {
                let x = pop_number(&mut stack);
                stack.push(Value::Number(i32::from(x == 0)));
            }
            b'~' => {
                let x = pop_number(&mut stack);
                stack.push(Value::Number(!x));
            }
            b'i' => {
                numbers[0] = numbers[0].wrapping_add(1);
                numbers[1] = numbers[1].wrapping_add(1);
            }
            b't' => {
                if pop_number(&mut stack) == 0 {
                    i = skip_branch(template, i, true);
                }
            }
            b'e' => i = skip_branch(template, i, false),
            _ => {}
        }
    }
    out
}

/// `bytes` without its padding, `$<ms[*][/]>`, and the delay of the mandatory padding
/// (`/`) in it, as `tputs` strips and sleeps: `flash` has `$<100/>` between its two
/// sequences.
fn strip_padding(bytes: &[u8]) -> (Vec<u8>, u64) {
    let mut out = Vec::with_capacity(bytes.len());
    let mut delay = 0u64;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'<') {
            let mut j = i + 2;
            let mut tenths = 0u64;
            let mut digits = 0;
            while let Some(digit) = bytes.get(j).filter(|b| b.is_ascii_digit()) {
                tenths = tenths
                    .saturating_mul(10)
                    .saturating_add(u64::from(digit - b'0'));
                digits += 1;
                j += 1;
            }
            tenths = tenths.saturating_mul(10);
            if bytes.get(j) == Some(&b'.') {
                j += 1;
                if let Some(digit) = bytes.get(j).filter(|b| b.is_ascii_digit()) {
                    tenths += u64::from(digit - b'0');
                    j += 1;
                }
                while bytes.get(j).is_some_and(u8::is_ascii_digit) {
                    j += 1;
                }
            }
            let mut mandatory = false;
            while let Some(flag @ (b'*' | b'/')) = bytes.get(j) {
                mandatory |= *flag == b'/';
                j += 1;
            }
            if digits > 0 && bytes.get(j) == Some(&b'>') {
                if mandatory {
                    delay += tenths / 10;
                }
                i = j + 1;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    (out, delay)
}

/// What a capability came to.
enum Outcome {
    /// Written, or a true boolean.
    Done,
    /// A false boolean, or a string the terminal lacks: nothing written, status 1.
    Lacking,
    /// A name terminfo does not know: status 4.
    Unknown,
}

/// Where the output goes: standard output is kept until the end, as ncurses' is when it
/// is not a terminal, so an error about a later capability comes before the bytes of an
/// earlier one, as its does; a padding delay flushes what came before it.
struct Sink<'a, W: Write> {
    pending: Vec<u8>,
    stdout: W,
    /// Standard error, written as it happens.
    stderr: &'a mut dyn Write,
}

impl<W: Write> Sink<'_, W> {
    fn write_sequence(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let (bytes, delay) = strip_padding(bytes);
        if delay > 0 {
            // The padding sits between the two halves of `flash`; the first half must be
            // on the screen before the wait, and the strip has lost where it was. Only
            // `flash` has padding, so the split is known.
            if let Some(at) = bytes.iter().position(|&b| b == b'h') {
                self.pending.extend_from_slice(&bytes[..=at]);
                self.flush()?;
                std::thread::sleep(std::time::Duration::from_millis(delay));
                self.pending.extend_from_slice(&bytes[at + 1..]);
                return Ok(());
            }
        }
        self.pending.extend_from_slice(&bytes);
        Ok(())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stdout.write_all(&self.pending)?;
        self.pending.clear();
        self.stdout.flush()
    }
}

/// The console window's columns and rows, as ncurses finds them: an exported `COLUMNS`
/// or `LINES` first, then the console, then a `COLUMNS` or `LINES` shell variable, then
/// the 80 and 24 of terminfo's `xterm-256color`.
fn screen_size<SE: cash_core::ShellExtensions>(shell: &cash_core::Shell<SE>) -> (i32, i32) {
    let variable = |name: &str, exported: bool| {
        shell
            .env()
            .get(name)
            .filter(|(_, variable)| variable.is_exported() || !exported)
            .and_then(|(_, variable)| variable.value().to_cow_str(shell).trim().parse().ok())
            .filter(|&value: &i32| value > 0)
    };
    let console = crossterm::terminal::size().ok();
    let size = |name: &str, index: usize, default: i32| {
        variable(name, true)
            .or_else(|| console.map(|(cols, rows)| i32::from(if index == 0 { cols } else { rows })))
            .or_else(|| variable(name, false))
            .unwrap_or(default)
    };
    (size("COLUMNS", 0, 80), size("LINES", 1, 24))
}

/// The numeric capability `name`, if xterm-256color has it.
fn number(name: &str, size: (i32, i32)) -> Option<i32> {
    match name {
        "cols" => Some(size.0),
        "lines" => Some(size.1),
        "colors" => Some(256),
        "pairs" => Some(65536),
        "it" => Some(8),
        _ => None,
    }
}

/// Write terminal capabilities as VT sequences, as `tput` does for xterm-256color.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct TputCommand {
    /// Options, capabilities and parameters, parsed here as getopt does.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// The options, parsed as getopt parses ncurses' `SVvxT:`: anywhere on the line until
/// `--`, clustered, `-T`'s argument attached or next.
#[derive(Default)]
struct Options {
    from_stdin: bool,
    version: bool,
    keep_scrollback: bool,
    help: bool,
    term: Option<String>,
    words: Vec<String>,
}

/// Why the options could not be parsed, with the text getopt prints.
enum OptionError {
    Invalid(char),
    MissingArgument,
}

fn parse_options(args: &[String]) -> Result<Options, OptionError> {
    let mut options = Options::default();
    let mut only_words = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if only_words || arg == "-" || !arg.starts_with('-') {
            options.words.push(arg.clone());
            continue;
        }
        if arg == "--" {
            only_words = true;
            continue;
        }
        if arg == "--help" {
            options.help = true;
            continue;
        }
        let mut letters = arg.strip_prefix('-').unwrap_or_default().chars();
        while let Some(letter) = letters.next() {
            match letter {
                'S' => options.from_stdin = true,
                'V' => options.version = true,
                'v' => {}
                'x' => options.keep_scrollback = true,
                'T' => {
                    let rest = letters.as_str();
                    let term = if rest.is_empty() {
                        args.next().ok_or(OptionError::MissingArgument)?.clone()
                    } else {
                        rest.to_owned()
                    };
                    options.term = Some(term);
                    break;
                }
                other => return Err(OptionError::Invalid(other)),
            }
        }
    }
    Ok(options)
}

/// One `tput` run: what it was asked and where it writes.
struct Run<'a, W: Write> {
    keep_scrollback: bool,
    size: (i32, i32),
    sink: Sink<'a, W>,
}

impl<W: Write> Run<'_, W> {
    /// Runs the capability `words[0]` with the parameters after it, saying how many words
    /// were taken: the name, and after a string capability every number that follows.
    fn one(&mut self, words: &[&str]) -> std::io::Result<(Outcome, usize)> {
        let name = words[0];
        let rest = &words[1..];
        match name {
            "clear" => {
                let bytes = if self.keep_scrollback {
                    CLEAR_KEEP_SCROLLBACK
                } else {
                    CLEAR
                };
                self.sink.write_sequence(bytes)?;
                return Ok((Outcome::Done, 1));
            }
            "init" | "reset" => {
                // is2 (or rs1 and rs2), then the margins cleared, as ncurses' do.
                let strings: &[&str] = if name == "init" {
                    &["is2", "mgc"]
                } else {
                    &["rs1", "rs2", "mgc"]
                };
                for string in strings {
                    if let Some(bytes) = capability(string) {
                        self.sink.write_sequence(&bytes)?;
                    }
                }
                if name == "reset"
                    && let Err(error) = cash_win32::console::restore_modes()
                {
                    let error = cash_core::error::os_error_text(&error);
                    writeln!(
                        self.sink.stderr,
                        "tput: could not restore the console modes: {error}"
                    )?;
                }
                return Ok((Outcome::Done, 1));
            }
            "longname" => {
                self.sink.pending.extend_from_slice(LONGNAME.as_bytes());
                return Ok((Outcome::Done, 1));
            }
            _ => {}
        }
        if lists(TRUE_BOOLEANS, name) {
            return Ok((Outcome::Done, 1));
        }
        if lists(BOOLEANS, name) {
            return Ok((Outcome::Lacking, 1));
        }
        if lists(NUMBERS, name) {
            let value = number(name, self.size).unwrap_or(-1);
            self.sink
                .pending
                .extend_from_slice(format!("{value}\n").as_bytes());
            return Ok((Outcome::Done, 1));
        }
        if let Some(template) = capability(name) {
            let used = 1 + rest
                .iter()
                .enumerate()
                .take_while(|(index, word)| is_parameter(word) || takes_text(&template, *index))
                .count();
            if rest.is_empty() {
                self.sink.write_sequence(&template)?;
            } else {
                let params: Vec<(i32, &str)> = rest
                    .iter()
                    .map(|word| (strtol(word).map_or(0, truncate), *word))
                    .collect();
                self.sink.write_sequence(&expand(&template, &params))?;
            }
            return Ok((Outcome::Done, used));
        }
        if lists(STRINGS, name) {
            return Ok((Outcome::Lacking, 1));
        }
        Ok((Outcome::Unknown, 1))
    }

    /// Runs the capabilities in `words`, one after the other. An unknown name is reported
    /// and ends the run, as it does ncurses'. A capability the terminal lacks ends a
    /// command line, and is counted and passed over on a line of `-S` (`stop_at_lacking`).
    fn line(&mut self, words: &[&str], stop_at_lacking: bool) -> std::io::Result<Failures> {
        let mut failures = Failures::default();
        let mut rest = words;
        while let Some(name) = rest.first() {
            let (outcome, used) = self.one(rest)?;
            match outcome {
                Outcome::Done => {}
                Outcome::Lacking => {
                    failures.lacking = failures.lacking.saturating_add(1);
                    if stop_at_lacking {
                        break;
                    }
                }
                Outcome::Unknown => {
                    writeln!(
                        self.sink.stderr,
                        "tput: unknown terminfo capability '{name}'"
                    )?;
                    failures.unknown = true;
                    break;
                }
            }
            rest = &rest[used..];
        }
        Ok(failures)
    }
}

/// What went wrong on a line of capabilities.
#[derive(Default)]
struct Failures {
    /// How many capabilities the terminal lacked.
    lacking: u8,
    /// Whether a name was unknown, which ends the run.
    unknown: bool,
}

impl builtins::Command for TputCommand {
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
        let options = match parse_options(&self.args) {
            Ok(options) => options,
            Err(error) => {
                let mut stderr = context.stderr();
                match error {
                    OptionError::Invalid(letter) => {
                        writeln!(stderr, "tput: invalid option -- '{letter}'")?;
                    }
                    OptionError::MissingArgument => {
                        writeln!(stderr, "tput: option requires an argument -- 'T'")?;
                    }
                }
                stderr.write_all(USAGE.as_bytes())?;
                return Ok(ExecutionResult::new(2));
            }
        };
        if options.help {
            context.stdout().write_all(USAGE.as_bytes())?;
            return Ok(ExecutionResult::success());
        }
        if options.version {
            writeln!(
                context.stdout(),
                "tput (cash): VT sequences, as ncurses 6.6 writes them"
            )?;
            return Ok(ExecutionResult::success());
        }
        // Only a terminal named on the command line is checked: the one cash runs in is
        // VT, whatever `TERM` says of it.
        match options.term.as_deref() {
            Some("") => {
                writeln!(
                    context.stderr(),
                    "tput: No value for $TERM and no -T specified"
                )?;
                return Ok(ExecutionResult::new(2));
            }
            Some(term) if !is_vt(term) => {
                writeln!(context.stderr(), "tput: unknown terminal \"{term}\"")?;
                return Ok(ExecutionResult::new(3));
            }
            _ => {}
        }
        if !options.from_stdin && options.words.is_empty() {
            context.stderr().write_all(USAGE.as_bytes())?;
            return Ok(ExecutionResult::new(2));
        }

        let mut stderr = context.stderr();
        let mut run = Run {
            keep_scrollback: options.keep_scrollback,
            size: screen_size(context.shell),
            sink: Sink {
                pending: Vec::new(),
                stdout: context.stdout(),
                stderr: &mut stderr,
            },
        };
        let status = if options.from_stdin {
            let mut input = Vec::new();
            context.stdin().read_to_end(&mut input)?;
            let input = String::from_utf8_lossy(&input);
            // ncurses: 4 plus the number of capabilities that failed, or 4 at once for an
            // unknown name.
            let mut lacking: u8 = 0;
            let mut unknown = false;
            for line in input.lines() {
                let words: Vec<&str> = line.split_whitespace().collect();
                if words.is_empty() {
                    continue;
                }
                let failures = run.line(&words, false)?;
                lacking = lacking.saturating_add(failures.lacking);
                if failures.unknown {
                    unknown = true;
                    break;
                }
            }
            if unknown {
                4
            } else if lacking == 0 {
                0
            } else {
                4u8.saturating_add(lacking)
            }
        } else {
            let words: Vec<&str> = options.words.iter().map(String::as_str).collect();
            let failures = run.line(&words, true)?;
            if failures.unknown {
                4
            } else if failures.lacking > 0 {
                1
            } else {
                0
            }
        };
        run.sink.flush()?;
        Ok(ExecutionResult::new(status))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        capability, expand, function_key, is_parameter, strip_padding, strtol, takes_text, truncate,
    };

    /// `tput NAME PARAMS...` as bytes, through the same path as the builtin.
    fn tput(name: &str, params: &[&str]) -> Vec<u8> {
        let template = capability(name).unwrap_or_default();
        if params.is_empty() {
            return template.into_owned();
        }
        let params: Vec<(i32, &str)> = params
            .iter()
            .map(|word| (strtol(word).map_or(0, truncate), *word))
            .collect();
        expand(&template, &params)
    }

    #[test]
    fn colours_follow_the_three_ranges_of_setaf_and_setab() {
        // ncurses 6.6: `tput setaf N | od -c`.
        assert_eq!(tput("setaf", &["0"]), b"\x1b[30m");
        assert_eq!(tput("setaf", &["7"]), b"\x1b[37m");
        assert_eq!(tput("setaf", &["8"]), b"\x1b[90m");
        assert_eq!(tput("setaf", &["15"]), b"\x1b[97m");
        assert_eq!(tput("setaf", &["16"]), b"\x1b[38;5;16m");
        assert_eq!(tput("setaf", &["255"]), b"\x1b[38;5;255m");
        assert_eq!(tput("setaf", &["256"]), b"\x1b[38;5;256m");
        assert_eq!(tput("setab", &["3"]), b"\x1b[43m");
        assert_eq!(tput("setab", &["9"]), b"\x1b[101m");
        assert_eq!(tput("setab", &["42"]), b"\x1b[48;5;42m");
        // A missing parameter is 0, and none at all prints the terminfo string.
        assert_eq!(tput("setaf", &["abc"]), b"\x1b[30m");
        assert!(tput("setaf", &[]).starts_with(b"\x1b[%?%p1%{8}%<%t3%p1%d%e"));
    }

    #[test]
    fn cursor_addresses_count_from_one_and_wrap_as_ints() {
        assert_eq!(tput("cup", &["5", "10"]), b"\x1b[6;11H");
        assert_eq!(tput("cup", &["5"]), b"\x1b[6;1H");
        assert_eq!(tput("cup", &["0x10", "010"]), b"\x1b[17;9H");
        assert_eq!(tput("cup", &["2147483647", "1"]), b"\x1b[-2147483648;2H");
        assert_eq!(tput("cup", &["99999999999", "3"]), b"\x1b[1215752192;4H");
        assert_eq!(tput("cup", &["4294967295", "1"]), b"\x1b[0;2H");
        assert_eq!(tput("hpa", &["4"]), b"\x1b[5G");
        assert_eq!(tput("cuu", &["0"]), b"\x1b[0A");
        assert_eq!(tput("cuu", &[]), b"\x1b[%p1%dA");
    }

    #[test]
    fn sgr_rep_initc_and_the_termcap_style_u6() {
        assert_eq!(tput("sgr", &["0"; 9]), b"\x1b(B\x1b[0m");
        assert_eq!(tput("sgr", &["1"; 9]), b"\x1b(0\x1b[0;1;2;4;7;5;8m");
        assert_eq!(tput("sgr", &["0", "1"]), b"\x1b(B\x1b[0;4m");
        assert_eq!(tput("sgr", &["2", "0", "3"]), b"\x1b(B\x1b[0;7m");
        assert_eq!(tput("rep", &["65", "3"]), b"A\x1b[2b");
        assert_eq!(tput("rep", &["300", "2"]), b",\x1b[1b");
        assert_eq!(tput("rep", &["0", "2"]), b"\x80\x1b[1b");
        assert_eq!(tput("rep", &["65"]), b"A\x1b[-1b");
        assert_eq!(
            tput("initc", &["1", "1000", "500", "0"]),
            b"\x1b]4;1;rgb:FF/7F/00\x1b\\"
        );
        assert_eq!(
            tput("initc", &["1", "-100", "0", "0"]),
            b"\x1b]4;1;rgb:FFFFFFE7/00/00\x1b\\"
        );
        assert_eq!(tput("u6", &["1", "2"]), b"\x1b[3;2R");
        assert_eq!(tput("u6", &["1"]), b"\x1b[1;2R");
        assert_eq!(tput("u8", &["1"]), b"\x1b[?;0123456789]c");
        assert_eq!(tput("xm", &["1", "2", "3", "4"]), b"\x1b[<3;2;3;M");
        assert_eq!(tput("Cs", &["red"]), b"\x1b]12;red\x07");
    }

    #[test]
    fn every_function_and_modified_key_is_xterms() {
        assert_eq!(tput("kf1", &[]), b"\x1bOP");
        assert_eq!(tput("kf12", &[]), b"\x1b[24~");
        assert_eq!(tput("kf13", &[]), b"\x1b[1;2P");
        assert_eq!(tput("kf24", &[]), b"\x1b[24;2~");
        assert_eq!(tput("kf29", &[]), b"\x1b[15;5~");
        assert_eq!(tput("kf40", &[]), b"\x1b[1;6S");
        assert_eq!(tput("kf53", &[]), b"\x1b[15;3~");
        assert_eq!(tput("kf63", &[]), b"\x1b[1;4R");
        assert_eq!(function_key("kf64"), None);
        assert_eq!(function_key("kf0"), None);
        assert_eq!(tput("kDC", &[]), b"\x1b[3;2~");
        assert_eq!(tput("kEND", &[]), b"\x1b[1;2F");
        assert_eq!(tput("kUP", &[]), b"\x1b[1;2A");
        assert_eq!(tput("kLFT5", &[]), b"\x1b[1;5D");
        assert_eq!(tput("kNXT7", &[]), b"\x1b[6;7~");
        assert_eq!(tput("kHOM3", &[]), b"\x1b[1;3H");
        assert_eq!(capability("kUP8"), None);
        assert_eq!(capability("kUP2"), None);
        assert_eq!(capability("setf"), None);
    }

    #[test]
    fn padding_is_stripped_and_its_mandatory_delay_kept() {
        assert_eq!(
            strip_padding(b"\x1b[?5h$<100/>\x1b[?5l"),
            (b"\x1b[?5h\x1b[?5l".to_vec(), 100)
        );
        assert_eq!(strip_padding(b"a$<5>b"), (b"ab".to_vec(), 0));
        assert_eq!(strip_padding(b"a$<2.5*/>b"), (b"ab".to_vec(), 2));
        assert_eq!(strip_padding(b"$<x>"), (b"$<x>".to_vec(), 0));
    }

    #[test]
    fn parameters_are_the_numbers_strtol_reads_whole() {
        assert_eq!(strtol("5"), Some(5));
        assert_eq!(strtol(" 5"), Some(5));
        assert_eq!(strtol("+5"), Some(5));
        assert_eq!(strtol("-1"), Some(-1));
        assert_eq!(strtol("0x1f"), Some(31));
        assert_eq!(strtol("077"), Some(63));
        assert_eq!(strtol("08"), None);
        assert_eq!(strtol("5.5"), None);
        assert_eq!(strtol("1e3"), None);
        assert_eq!(strtol("0x"), None);
        assert_eq!(strtol(""), None);
        assert_eq!(strtol("99999999999999999999"), Some(i64::MAX));
        for word in ["5", "+5", "0x10", "-0", "99999999999999999999"] {
            assert!(is_parameter(word), "{word}");
        }
        for word in ["-1", "bold", "5.5", "08", "-x", "-", "", " "] {
            assert!(!is_parameter(word), "{word:?}");
        }
        // `Cs red`: a word for a `%s` parameter is taken whatever it is.
        let cs = capability("Cs").unwrap_or_default();
        assert!(takes_text(&cs, 0));
        assert!(!takes_text(&cs, 1));
        assert!(!takes_text(&capability("cup").unwrap_or_default(), 0));
    }
}
