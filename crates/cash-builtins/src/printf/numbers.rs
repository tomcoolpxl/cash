//! Numeric arguments of `printf`, read as Bash reads them.
//!
//! uucore read them itself, lazily, and said what it thought of a bad one straight to
//! the process's standard error, as `cash.exe: 'abc': expected a numeric value`: past
//! `2>/dev/null`, out of reach of `$(…)`, with a global exit code nobody read, so the
//! status was 0 (BI-09). Here the format is scanned for what each argument is read as,
//! in the order uucore will take them, and each one read as a number is read the way
//! Bash's `printf` reads it (`strtoimax`, `strtoumax`, `strtold` and `sh_invalidnum`):
//! a failure is said in Bash's words on the builtin's own standard error, and uucore is
//! handed the number as a clean decimal, which it reads without a word.

use std::ffi::OsString;

/// What an argument is read as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Use {
    Text,
    /// `%b`: text in which a `\c` ends all output.
    Escaped,
    Signed,
    Unsigned,
    Float,
}

/// Where the next argument comes from, as uucore's `FormatArguments` keeps it.
#[derive(Default)]
struct Cursor {
    next: usize,
    highest: Option<usize>,
    offset: usize,
}

impl Cursor {
    fn take(&mut self, position: Option<usize>) -> usize {
        match position {
            None => {
                let at = self.next;
                self.next += 1;
                at
            }
            Some(position) => {
                let at = position.saturating_sub(1).saturating_add(self.offset);
                self.highest = Some(self.highest.map_or(at, |highest| highest.max(at)));
                at
            }
        }
    }

    fn next_batch(&mut self) {
        self.offset = self.next.max(self.highest.map_or(0, |highest| highest + 1));
        self.next = self.offset;
    }
}

/// One thing a format does: an argument read (by its `n$`, if it has one) or a `\c`.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Read(Option<usize>, Use),
    Stop,
}

/// What the format reads, in order.
fn steps(format: &str) -> Vec<Step> {
    let bytes = format.as_bytes();
    let mut steps = Vec::new();
    let mut at = 0;
    // `digits$`, if there.
    let position = |at: &mut usize| -> Option<usize> {
        let start = *at;
        let mut end = start;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end > start && bytes.get(end) == Some(&b'$') {
            *at = end + 1;
            std::str::from_utf8(bytes.get(start..end)?)
                .ok()?
                .parse()
                .ok()
        } else {
            None
        }
    };
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'\\' => {
                if bytes.get(at + 1) == Some(&b'c') {
                    steps.push(Step::Stop);
                    return steps;
                }
                at += 2;
            }
            b'%' if bytes.get(at + 1) == Some(&b'%') => at += 2,
            b'%' => {
                at += 1;
                let value_position = position(&mut at);
                while bytes.get(at).is_some_and(|b| b"-+ #0'I".contains(b)) {
                    at += 1;
                }
                if bytes.get(at) == Some(&b'*') {
                    at += 1;
                    steps.push(Step::Read(position(&mut at), Use::Signed));
                } else {
                    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                        at += 1;
                    }
                }
                if bytes.get(at) == Some(&b'.') {
                    at += 1;
                    if bytes.get(at) == Some(&b'*') {
                        at += 1;
                        steps.push(Step::Read(position(&mut at), Use::Signed));
                    } else {
                        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                            at += 1;
                        }
                    }
                }
                while bytes.get(at).is_some_and(|b| b"hlLqjzt".contains(b)) {
                    at += 1;
                }
                let read_as = match bytes.get(at) {
                    Some(b'd' | b'i') => Use::Signed,
                    Some(b'o' | b'u' | b'x' | b'X') => Use::Unsigned,
                    Some(b'e' | b'E' | b'f' | b'F' | b'g' | b'G' | b'a' | b'A') => Use::Float,
                    Some(b'b') => Use::Escaped,
                    Some(_) => Use::Text,
                    None => return steps,
                };
                steps.push(Step::Read(value_position, read_as));
                at += 1;
            }
            _ => at += 1,
        }
    }
    steps
}

/// The arguments as uucore should get them, and what to say about the bad ones, each with
/// the pass over the format that reads it (0 for the first), in the order `printf` reads
/// them, up to a `\c`. Bash says it when it gets there, after what earlier passes printed.
pub(super) fn prepare(format: &str, args: &[String]) -> (Vec<OsString>, Vec<(usize, String)>) {
    let steps = steps(format);
    let reads_any = steps.iter().any(|step| matches!(step, Step::Read(..)));
    let mut uses: Vec<Option<Use>> = vec![None; args.len()];
    let mut order = Vec::new();
    let mut cursor = Cursor::default();
    let mut batch = 0;

    'batches: loop {
        for step in &steps {
            let Step::Read(position, read_as) = step else {
                break 'batches;
            };
            let at = cursor.take(*position);
            let Some(arg) = args.get(at) else { continue };
            if let Some(slot) = uses.get_mut(at) {
                match slot {
                    None => {
                        *slot = Some(*read_as);
                        order.push((batch, at));
                    }
                    // Read as text somewhere, it is left as it is.
                    Some(_) if matches!(read_as, Use::Text | Use::Escaped) => {
                        *slot = Some(Use::Text);
                    }
                    Some(_) => {}
                }
            }
            if *read_as == Use::Escaped && arg.contains("\\c") {
                break 'batches;
            }
        }
        if !reads_any || args.is_empty() {
            break;
        }
        cursor.next_batch();
        batch += 1;
        if cursor.offset >= args.len() {
            break;
        }
    }

    let mut prepared: Vec<OsString> = args.iter().map(OsString::from).collect();
    let mut complaints = Vec::new();
    for (batch, at) in order {
        let (Some(arg), Some(Some(read_as))) = (args.get(at), uses.get(at)) else {
            continue;
        };
        let (value, complaint) = match read_as {
            Use::Signed => signed(arg),
            Use::Unsigned => unsigned(arg),
            Use::Float => float(arg),
            Use::Text | Use::Escaped => continue,
        };
        if let Some(slot) = prepared.get_mut(at) {
            *slot = OsString::from(value);
        }
        complaints.extend(complaint.map(|complaint| (batch, complaint)));
    }
    (prepared, complaints)
}

/// Bash's `sh_invalidnum`: what is said about a number that is not one.
fn invalid(arg: &str) -> String {
    let bytes = arg.as_bytes();
    let what = match bytes {
        [b'0', next, ..] if next.is_ascii_digit() => "invalid octal number",
        [b'0', b'x', ..] => "invalid hex number",
        _ => "invalid number",
    };
    std::format!("{arg}: {what}")
}

fn out_of_range(arg: &str) -> String {
    std::format!("{arg}: Numerical result out of range")
}

/// An integer as C's `strtoimax`/`strtoumax` with base 0 reads it: blanks, a sign, then
/// `0x` for hex or `0` for octal. The magnitude, its sign, whether it overflowed `u64`,
/// and whether it was all used; `None` without a digit.
fn c_integer(arg: &str) -> Option<(u64, bool, bool, bool)> {
    let text = arg.trim_start_matches([' ', '\t', '\n', '\u{b}', '\u{c}', '\r']);
    let (negative, text) = match text.as_bytes().first() {
        Some(b'-') => (true, text.get(1..)?),
        Some(b'+') => (false, text.get(1..)?),
        _ => (false, text),
    };
    let (base, digits) = if let Some(hex) = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .filter(|hex| hex.chars().next().is_some_and(|c| c.is_ascii_hexdigit()))
    {
        (16, hex)
    } else if text.starts_with('0') {
        (8, text)
    } else {
        (10, text)
    };
    let used = digits
        .char_indices()
        .find(|(_, c)| !c.is_digit(base))
        .map_or(digits.len(), |(at, _)| at);
    if used == 0 {
        return None;
    }
    let mut magnitude: u64 = 0;
    let mut overflow = false;
    for c in digits.get(..used)?.chars() {
        let digit = u64::from(c.to_digit(base)?);
        match magnitude
            .checked_mul(u64::from(base))
            .and_then(|m| m.checked_add(digit))
        {
            Some(next) => magnitude = next,
            None => overflow = true,
        }
    }
    Some((magnitude, negative, overflow, used == digits.len()))
}

/// The code of the character after a leading quote, which is what Bash makes of `'a`.
fn character_constant(arg: &str) -> Option<u32> {
    let rest = arg.strip_prefix(['\'', '"'])?;
    Some(rest.chars().next().map_or(0, u32::from))
}

fn signed(arg: &str) -> (String, Option<String>) {
    if let Some(code) = character_constant(arg) {
        return (code.to_string(), None);
    }
    let Some((magnitude, negative, overflow, whole)) = c_integer(arg) else {
        return (String::from("0"), Some(invalid(arg)));
    };
    let limit = if negative {
        i64::MIN.unsigned_abs()
    } else {
        i64::MAX.unsigned_abs()
    };
    if overflow || magnitude > limit {
        let clamped = if negative { i64::MIN } else { i64::MAX };
        return (clamped.to_string(), Some(out_of_range(arg)));
    }
    let value = if negative {
        0i128 - i128::from(magnitude)
    } else {
        i128::from(magnitude)
    };
    (value.to_string(), (!whole).then(|| invalid(arg)))
}

fn unsigned(arg: &str) -> (String, Option<String>) {
    if let Some(code) = character_constant(arg) {
        return (code.to_string(), None);
    }
    let Some((magnitude, negative, overflow, whole)) = c_integer(arg) else {
        return (String::from("0"), Some(invalid(arg)));
    };
    if overflow {
        return (u64::MAX.to_string(), Some(out_of_range(arg)));
    }
    // `strtoumax` takes a negative number modulo 2^64, as Bash shows `-1` with `%u`.
    let value = if negative {
        magnitude.wrapping_neg()
    } else {
        magnitude
    };
    (value.to_string(), (!whole).then(|| invalid(arg)))
}

/// The longest start of `text` that is a number as `strtold` reads one.
fn float_prefix(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut at = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let lower = text.get(at..).unwrap_or_default().to_ascii_lowercase();
    for word in ["infinity", "inf", "nan"] {
        if lower.starts_with(word) {
            return at + word.len();
        }
    }
    let hex = lower.starts_with("0x");
    let digit = |b: &u8| {
        if hex {
            b.is_ascii_hexdigit()
        } else {
            b.is_ascii_digit()
        }
    };
    if hex {
        at += 2;
    }
    let mantissa_start = at;
    while bytes.get(at).is_some_and(digit) {
        at += 1;
    }
    let mut digits = at - mantissa_start;
    if bytes.get(at) == Some(&b'.') {
        let point = at;
        at += 1;
        while bytes.get(at).is_some_and(digit) {
            at += 1;
        }
        digits += at - point - 1;
    }
    if digits == 0 {
        // `0x` with nothing after it is the `0`.
        return if hex { mantissa_start - 1 } else { 0 };
    }
    let exponent = if hex { b'p' } else { b'e' };
    if bytes
        .get(at)
        .is_some_and(|b| b.to_ascii_lowercase() == exponent)
    {
        let mut end = at + 1;
        if matches!(bytes.get(end), Some(b'+' | b'-')) {
            end += 1;
        }
        let exponent_digits = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end > exponent_digits {
            at = end;
        }
    }
    at
}

fn float(arg: &str) -> (String, Option<String>) {
    if let Some(code) = character_constant(arg) {
        return (code.to_string(), None);
    }
    let text = arg.trim_start_matches([' ', '\t', '\n', '\u{b}', '\u{c}', '\r']);
    let used = float_prefix(text);
    if used == 0 {
        return (String::from("0"), Some(invalid(arg)));
    }
    let number = text.get(..used).unwrap_or("0").to_owned();
    let complaint = (used < text.len()).then(|| invalid(arg));
    (number, complaint)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(format: &str, arg: &str) -> (String, Vec<String>) {
        let (prepared, complaints) = prepare(format, &[arg.to_owned()]);
        (
            prepared
                .first()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_default(),
            complaints
                .into_iter()
                .map(|(_, complaint)| complaint)
                .collect(),
        )
    }

    #[test]
    fn numbers_are_read_as_bash_reads_them() {
        assert_eq!(
            one("%d", "abc"),
            ("0".into(), vec!["abc: invalid number".into()])
        );
        assert_eq!(
            one("%d", "2x"),
            ("2".into(), vec!["2x: invalid number".into()])
        );
        assert_eq!(one("%d", "010"), ("8".into(), vec![]));
        assert_eq!(
            one("%d", "08"),
            ("0".into(), vec!["08: invalid octal number".into()])
        );
        assert_eq!(
            one("%d", "0x1g"),
            ("1".into(), vec!["0x1g: invalid hex number".into()])
        );
        assert_eq!(one("%d", " 12"), ("12".into(), vec![]));
        assert_eq!(one("%d", ""), ("0".into(), vec![": invalid number".into()]));
        assert_eq!(one("%d", "'ab"), ("97".into(), vec![]));
        assert_eq!(
            one("%d", "99999999999999999999"),
            (
                "9223372036854775807".into(),
                vec!["99999999999999999999: Numerical result out of range".into()]
            )
        );
        assert_eq!(one("%u", "-1"), ("18446744073709551615".into(), vec![]));
        assert_eq!(
            one("%f", "1.5x"),
            ("1.5".into(), vec!["1.5x: invalid number".into()])
        );
        assert_eq!(one("%f", "inf"), ("inf".into(), vec![]));
        assert_eq!(one("%s", "abc"), ("abc".into(), vec![]));
    }

    #[test]
    fn arguments_are_followed_as_the_format_takes_them() {
        let (prepared, complaints) =
            prepare("%*d|%s %1$d\n", &["x".into(), "5".into(), "t".into()]);
        assert_eq!(prepared, ["0", "5", "t"]);
        assert_eq!(complaints, [(0, "x: invalid number".to_owned())]);
        // A bad number in the second pass is said in the second pass.
        let (_, complaints) = prepare("%d\n", &["1".into(), "x".into()]);
        assert_eq!(complaints, [(1, "x: invalid number".to_owned())]);
        // After a `\c` in `%b` nothing more is read.
        let (_, complaints) = prepare("%b %d", &["a\\c".into(), "x".into()]);
        assert!(complaints.is_empty());
    }
}
