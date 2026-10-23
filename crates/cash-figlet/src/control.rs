//! Control files (`.flc`): character translations applied to the input before it is
//! looked up in the font, read as figlet 2.2.5's `readcontrol` reads them.
//!
//! `t a-z A-Z` translates a range, `t \x41 b` one character, a line of two numbers maps
//! the first to the second, `f` freezes: what came before is one group, and a character
//! takes at most one translation from each group, in order. Every file starts a group of
//! its own.
//!
//! The commands that choose how input bytes are decoded (`b` DBCS, `u` UTF-8, `h` HZ,
//! `j` Shift-JIS, `g` ISO 2022 sets) are read and left alone: cash's input is already
//! Unicode text, so `utf8.flc` changes nothing and the other encodings are not decoded.

/// A control file's commands, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Controls {
    commands: Vec<Command>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    /// Ends a group of translations.
    Freeze,
    /// Adds `offset` to a character from `lo` to `hi`.
    Translate { lo: i64, hi: i64, offset: i64 },
}

impl Controls {
    /// No translations.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether no file has been read.
    pub const fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Reads one control file's text and adds its commands after those already read.
    pub fn add(&mut self, text: &[u8]) {
        self.commands.push(Command::Freeze);
        let mut input = Input { text, at: 0 };
        while let Some(command) = input.get() {
            match command {
                b't' => {
                    input.skip_blanks();
                    let first = input.t_char();
                    let last = if input.peek() == Some(b'-') {
                        input.get();
                        input.t_char()
                    } else {
                        first
                    };
                    input.skip_blanks();
                    let offset = input.t_char() - first;
                    input.skip_to_eol();
                    self.commands.push(Command::Translate {
                        lo: first,
                        hi: last,
                        offset,
                    });
                }
                b'0'..=b'9' | b'-' => {
                    input.unget();
                    let first = input.number();
                    input.skip_blanks();
                    let last = input.number();
                    input.skip_to_eol();
                    self.commands.push(Command::Translate {
                        lo: first,
                        hi: first,
                        offset: last - first,
                    });
                }
                b'f' => {
                    input.skip_to_eol();
                    self.commands.push(Command::Freeze);
                }
                // The input encodings, which cash's Unicode input does not need.
                b'b' | b'u' | b'h' | b'j' => {}
                b'\r' | b'\n' => {}
                // `g` and its sets, `#` comments and anything else: the rest of the line.
                _ => input.skip_to_eol(),
            }
        }
    }

    /// `c` after the translations, as figlet's `handlemapping` applies them.
    pub fn map(&self, c: u32) -> u32 {
        let mut c = i64::from(c);
        let mut commands = self.commands.iter();
        while let Some(command) = commands.next() {
            if let Command::Translate { lo, hi, offset } = *command {
                if (lo..=hi).contains(&c) {
                    c += offset;
                    // On to the next group.
                    for next in commands.by_ref() {
                        if *next == Command::Freeze {
                            break;
                        }
                    }
                }
            }
        }
        u32::try_from(c).unwrap_or(0)
    }
}

/// A control file being read, a byte at a time with one byte of push-back, as figlet's
/// `Zgetc` and `Zungetc` read it.
struct Input<'a> {
    text: &'a [u8],
    at: usize,
}

impl Input<'_> {
    fn get(&mut self) -> Option<u8> {
        let byte = self.text.get(self.at).copied();
        if byte.is_some() {
            self.at += 1;
        }
        byte
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    const fn unget(&mut self) {
        self.at = self.at.saturating_sub(1);
    }

    fn skip_blanks(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    fn skip_to_eol(&mut self) {
        while let Some(byte) = self.get() {
            if byte == b'\n' {
                break;
            }
        }
    }

    /// A number: blanks skipped, an optional `-`, then hex after `0x`, octal after `0`,
    /// or decimal (figlet's `readnum`).
    fn number(&mut self) -> i64 {
        self.skip_blanks();
        let negative = self.peek() == Some(b'-');
        if negative {
            self.at += 1;
        }
        let radix = if self.peek() == Some(b'0') {
            self.at += 1;
            if matches!(self.peek(), Some(b'x' | b'X')) {
                self.at += 1;
                16
            } else {
                8
            }
        } else {
            10
        };
        let mut value: i64 = 0;
        // figlet takes any of `0123456789ABCDEF` whatever the base.
        while let Some(digit) = self.peek().and_then(|b| char::from(b).to_digit(16)) {
            value = value.saturating_mul(radix).saturating_add(i64::from(digit));
            self.at += 1;
        }
        if negative { -value } else { value }
    }

    /// A character of a `t` command: itself, a backslash escape, or a backslashed
    /// number (figlet's `readTchar`).
    fn t_char(&mut self) -> i64 {
        let Some(byte) = self.get() else {
            return 0;
        };
        if byte == b'\n' || byte == b'\r' {
            self.unget();
            return 0;
        }
        if byte != b'\\' {
            return i64::from(byte);
        }
        match self.get() {
            Some(b'a') => 7,
            Some(b'b') => 8,
            Some(b'e') => 27,
            Some(b'f') => 12,
            Some(b'n') => 10,
            Some(b'r') => 13,
            Some(b't') => 9,
            Some(b'v') => 11,
            Some(next @ (b'-' | b'x' | b'0'..=b'9')) => {
                self.unget();
                if next == b'x' {
                    // `\x41`: figlet's `readnum` reads `x41` as a number only after a
                    // `0`, so this is hex by the escape's own letter.
                    self.at += 1;
                    let mut value: i64 = 0;
                    while let Some(digit) = self.peek().and_then(|b| char::from(b).to_digit(16)) {
                        value = value * 16 + i64::from(digit);
                        self.at += 1;
                    }
                    value
                } else {
                    self.number()
                }
            }
            Some(other) => i64::from(other),
            None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Controls;

    #[test]
    fn ranges_numbers_and_groups() {
        let mut controls = Controls::new();
        controls.add(b"# upper case\nt a-z A-Z\n");
        assert_eq!(controls.map(u32::from('q')), u32::from('Q'));
        assert_eq!(controls.map(u32::from('Q')), u32::from('Q'));

        let mut controls = Controls::new();
        controls.add(b"t a b\nt b c\n");
        // One translation per group: `a` becomes `b`, not `c`.
        assert_eq!(controls.map(u32::from('a')), u32::from('b'));
        assert_eq!(controls.map(u32::from('b')), u32::from('c'));

        let mut controls = Controls::new();
        controls.add(b"t a b\nf\nt b c\n");
        // A freeze makes a second group, which applies after the first.
        assert_eq!(controls.map(u32::from('a')), u32::from('c'));

        let mut controls = Controls::new();
        controls.add(b"0x41 0x100\n66 0301\n");
        assert_eq!(controls.map(0x41), 0x100);
        assert_eq!(controls.map(66), 0o301);
    }

    #[test]
    fn escapes_and_encodings() {
        let mut controls = Controls::new();
        controls.add(b"u\nt \\x41 \\66\nt \\t x\n");
        assert_eq!(controls.map(0x41), 66);
        assert_eq!(controls.map(9), u32::from('x'));
    }
}
