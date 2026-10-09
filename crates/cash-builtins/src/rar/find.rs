//! `i`: a string looked for in archived files, and what rar shows of where it is.
//!
//! rar shows the first match in a file: up to 50 bytes before it and 69 in all, read in
//! the character table that matched, control characters as spaces, a NUL ending what
//! comes before; with a hexadecimal string, five bytes on each side, as text and as
//! bytes, the match's joined by dashes.

use std::io;

use super::cmdline::FindSpec;
use crate::rardata::Sink;

/// Bytes shown before a match, at the most.
const BEFORE: usize = 50;
/// Bytes shown in all.
const WINDOW: usize = 69;
/// Bytes shown on each side of a hexadecimal match.
const HEX_SIDE: usize = 5;

/// How the bytes of a string, and of what is shown around it, are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Table {
    /// Windows' ANSI code page.
    Ansi,
    /// Windows' OEM code page, the console's.
    Oem,
    Utf8,
    Utf16,
}

/// A single-byte code page's characters by byte, as Windows decodes them: plainly,
/// as a console shows them (control codes as pictures), and as lower case letters.
/// Latin-1 where Windows cannot say, or the code page takes more than a byte.
struct CodePage {
    chars: [char; 256],
    shown: [char; 256],
    lower: [char; 256],
}

impl CodePage {
    /// Windows' code page `code_page`: 0 the ANSI one, 1 the OEM one.
    fn of(code_page: u32) -> Self {
        let bytes: Vec<u8> = (0..=255).collect();
        let table = |units: Option<Vec<u16>>| -> Option<[char; 256]> {
            let units = units.filter(|units| units.len() == 256)?;
            let mut chars = ['\0'; 256];
            for (slot, unit) in chars.iter_mut().zip(units) {
                *slot = char::from_u32(u32::from(unit))?;
            }
            Some(chars)
        };
        let latin1: [char; 256] =
            std::array::from_fn(|byte| char::from(u8::try_from(byte).unwrap_or(0)));
        let chars =
            table(cash_win32::codepage::decode(code_page, &bytes, false).ok()).unwrap_or(latin1);
        let shown =
            table(cash_win32::codepage::decode_glyphs(code_page, &bytes).ok()).unwrap_or(chars);
        let lower = chars.map(|c| c.to_lowercase().next().unwrap_or(c));
        Self {
            chars,
            shown,
            lower,
        }
    }

    /// `text` in the code page, when it has a byte for every character.
    fn encode(&self, text: &str) -> Option<Vec<u8>> {
        text.chars()
            .map(|c| {
                let byte = self.chars.iter().position(|&d| d == c)?;
                u8::try_from(byte).ok()
            })
            .collect()
    }
}

/// The string to look for, in each table it is looked for in.
pub(super) struct Needles {
    list: Vec<(Table, Vec<u8>)>,
    case_sensitive: bool,
    hex: bool,
    ansi: CodePage,
    oem: CodePage,
}

impl Needles {
    /// `i`'s string as its parameters ask: `h` hexadecimal bytes, `t` every table
    /// (ANSI, OEM, UTF-8, UTF-16), else ANSI alone; `c` case sensitive. `None` when
    /// there is nothing to look for.
    pub(super) fn new(spec: &FindSpec) -> Option<Self> {
        let ansi = CodePage::of(0);
        let oem = CodePage::of(1);
        let mut list = Vec::new();
        if spec.hex {
            let digits: Vec<u8> = spec.text.bytes().filter(u8::is_ascii_hexdigit).collect();
            let bytes: Vec<u8> = digits
                .as_chunks::<2>()
                .0
                .iter()
                .filter_map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
                .collect();
            if !bytes.is_empty() {
                list.push((Table::Ansi, bytes));
            }
        } else {
            if let Some(bytes) = ansi.encode(&spec.text) {
                list.push((Table::Ansi, bytes));
            }
            if spec.all_tables {
                if let Some(bytes) = oem.encode(&spec.text) {
                    list.push((Table::Oem, bytes));
                }
                list.push((Table::Utf8, spec.text.as_bytes().to_vec()));
                list.push((
                    Table::Utf16,
                    spec.text
                        .encode_utf16()
                        .flat_map(u16::to_le_bytes)
                        .collect(),
                ));
            }
        }
        list.retain(|(_, bytes)| !bytes.is_empty());
        (!list.is_empty()).then_some(Self {
            list,
            case_sensitive: spec.case_sensitive || spec.hex,
            hex: spec.hex,
            ansi,
            oem,
        })
    }

    /// The code page `table` reads by.
    const fn page(&self, table: Table) -> &CodePage {
        match table {
            Table::Oem => &self.oem,
            _ => &self.ansi,
        }
    }

    fn longest(&self) -> usize {
        self.list.iter().map(|(_, b)| b.len()).max().unwrap_or(0)
    }
}

/// A match: where it starts in the bytes kept, how long it is, its table.
#[derive(Clone, Copy, Debug)]
struct Hit {
    at: usize,
    len: usize,
    table: Table,
}

/// A file's bytes as they decode, looked through for the first match, then kept until
/// what is shown around it is all there; it stops the decoding then, unless the file
/// goes on into a solid stream that the next file's decoding needs whole.
pub(super) struct Matcher<'n> {
    needles: &'n Needles,
    stop: bool,
    kept: Vec<u8>,
    hit: Option<Hit>,
    complete: bool,
}

/// What the matcher returns to stop the decoding once it has all it shows.
pub(super) fn stopped(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::Other && error.to_string() == STOPPED
}

const STOPPED: &str = "found what i looks for";

impl<'n> Matcher<'n> {
    pub(super) const fn new(needles: &'n Needles, stop: bool) -> Self {
        Self {
            needles,
            stop,
            kept: Vec::new(),
            hit: None,
            complete: false,
        }
    }

    /// The bytes a match needs after its start to be shown.
    const fn wanted_after(&self, hit: Hit) -> usize {
        if self.needles.hex {
            hit.at + hit.len + HEX_SIDE
        } else {
            hit.at.saturating_sub(BEFORE) + WINDOW
        }
    }

    fn search(&self, from: usize) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        for (table, needle) in &self.needles.list {
            let found = self.kept.get(from..).and_then(|hay| {
                hay.windows(needle.len())
                    .position(|w| self.equal(*table, w, needle))
                    .map(|at| from + at)
            });
            if let Some(at) = found
                && best.is_none_or(|b| at < b.at)
            {
                best = Some(Hit {
                    at,
                    len: needle.len(),
                    table: *table,
                });
            }
        }
        best
    }

    /// Whether bytes `a` are the string `b` in `table`, letters compared without
    /// their case unless `c` asks.
    fn equal(&self, table: Table, a: &[u8], b: &[u8]) -> bool {
        if self.needles.case_sensitive || a == b {
            return a == b;
        }
        let lower = |chars: &mut dyn Iterator<Item = char>| -> Vec<char> {
            chars.flat_map(char::to_lowercase).collect()
        };
        match table {
            Table::Ansi | Table::Oem => {
                let page = self.needles.page(table);
                a.iter()
                    .zip(b)
                    .all(|(&x, &y)| page.lower[usize::from(x)] == page.lower[usize::from(y)])
            }
            Table::Utf8 => match (std::str::from_utf8(a), std::str::from_utf8(b)) {
                (Ok(x), Ok(y)) => lower(&mut x.chars()) == lower(&mut y.chars()),
                _ => false,
            },
            Table::Utf16 => {
                let decode = |bytes: &[u8]| {
                    let units = bytes
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|&pair| u16::from_le_bytes(pair));
                    char::decode_utf16(units)
                        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
                        .collect::<Vec<char>>()
                };
                lower(&mut decode(a).into_iter()) == lower(&mut decode(b).into_iter())
            }
        }
    }

    /// The line rar shows for the match, once decoding is over (`eof`) or stopped.
    pub(super) fn shown(&self) -> Option<String> {
        let hit = self.hit?;
        if self.needles.hex {
            return Some(self.hex_line(hit));
        }
        let mut start = hit.at.saturating_sub(BEFORE);
        if hit.table == Table::Utf16 && (hit.at - start) % 2 == 1 {
            start += 1;
        }
        // What is shown before the match starts after a line's end or a NUL.
        if let Some(cut) = self.kept[start..hit.at]
            .iter()
            .rposition(|&b| b == 0 || b == b'\n')
        {
            start += cut + 1;
        }
        let window_end = (hit.at.saturating_sub(BEFORE) + WINDOW).min(self.kept.len());
        let mut end = window_end.max(hit.at + hit.len);
        if let Some(nul) = self.kept[hit.at + hit.len..end]
            .iter()
            .position(|&b| b == 0)
        {
            end = hit.at + hit.len + nul;
        }
        let bytes = &self.kept[start..end];
        // ANSI's control codes are spaces, DEL kept; OEM's are the console's pictures;
        // UTF-8's and UTF-16's are spaces.
        let mut text: String = match hit.table {
            Table::Ansi => bytes
                .iter()
                .map(|&b| match self.needles.ansi.chars[usize::from(b)] {
                    c if u32::from(c) < 0x20 => ' ',
                    c => c,
                })
                .collect(),
            Table::Oem => bytes
                .iter()
                .map(|&b| self.needles.oem.shown[usize::from(b)])
                .collect(),
            Table::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
            Table::Utf16 => {
                let units: Vec<u16> = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&pair| u16::from_le_bytes(pair))
                    .collect();
                String::from_utf16_lossy(&units)
            }
        };
        if matches!(hit.table, Table::Utf8 | Table::Utf16) {
            text = text
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect();
        }
        // At the file's end, ANSI's text ends in a space where its NUL would be.
        if hit.table == Table::Ansi && !self.complete && end == self.kept.len() {
            text.push(' ');
        }
        Some(text)
    }

    fn hex_line(&self, hit: Hit) -> String {
        let start = hit.at.saturating_sub(HEX_SIDE);
        let end = (hit.at + hit.len + HEX_SIDE).min(self.kept.len());
        let bytes = &self.kept[start..end];
        let text: String = bytes
            .iter()
            .map(|&b| if b < 0x20 { '?' } else { char::from(b) })
            .collect();
        let mut parts: Vec<String> = Vec::new();
        parts.extend(self.kept[start..hit.at].iter().map(|b| format!("{b:02x}")));
        parts.push(
            self.kept[hit.at..hit.at + hit.len]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join("-"),
        );
        parts.extend(
            self.kept[hit.at + hit.len..end]
                .iter()
                .map(|b| format!("{b:02x}")),
        );
        format!("{text} ( {} )", parts.join(" "))
    }
}

impl Sink for Matcher<'_> {
    fn put(&mut self, data: &[u8]) -> io::Result<()> {
        if self.complete {
            return self.finished();
        }
        let longest = self.needles.longest();
        let from = self.kept.len().saturating_sub(longest.saturating_sub(1));
        self.kept.extend_from_slice(data);
        if self.hit.is_none() {
            let Some(mut hit) = self.search(from) else {
                // Only what a match further on could show before it is kept.
                let keep = BEFORE + longest;
                if self.kept.len() > keep {
                    let cut = self.kept.len() - keep;
                    self.kept.drain(..cut);
                }
                return Ok(());
            };
            // Only what is shown before the match is kept.
            let keep_from = hit.at.saturating_sub(BEFORE);
            self.kept.drain(..keep_from);
            hit.at -= keep_from;
            self.hit = Some(hit);
        }
        if let Some(hit) = self.hit
            && self.kept.len() >= self.wanted_after(hit)
        {
            self.kept.truncate(self.wanted_after(hit));
            self.complete = true;
            return self.finished();
        }
        Ok(())
    }
}

impl Matcher<'_> {
    /// What a write gets once all that is shown is kept: the decoding stopped, or the
    /// rest taken and dropped.
    fn finished(&self) -> io::Result<()> {
        if self.stop {
            Err(io::Error::other(STOPPED))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(spec: &str, data: &[u8]) -> Option<String> {
        let spec = FindSpec::parse(spec);
        let needles = Needles::new(&spec)?;
        let mut matcher = Matcher::new(&needles, true);
        for chunk in data.chunks(7) {
            if matcher.put(chunk).is_err() {
                break;
            }
        }
        matcher.shown()
    }

    #[test]
    fn i_reads_ansi_and_oem_by_windows_code_pages() {
        // OEM 437's é is 0x82, which ANSI 1252 reads as a low quotation mark; OEM shows
        // a line feed as its picture.
        let oem = b"before caf\x82 after\n";
        assert_eq!(
            find("t=CAFÉ", oem).as_deref(),
            Some("before café after\u{25d9}")
        );
        assert_eq!(
            find("=caf\u{201a}", oem).as_deref(),
            Some("before caf\u{201a} after  ")
        );
        assert_eq!(find("=café", oem), None);
        // Letters fold in UTF-8 too.
        assert_eq!(
            find("t=CAFÉ", "before café after\n".as_bytes()).as_deref(),
            Some("before café after ")
        );
    }

    #[test]
    fn i_shows_what_rar_7_23_shows() {
        let deep: Vec<u8> = (0..200u8)
            .map(|i| b'a' + i % 26)
            .chain(*b"NEEDLE")
            .chain((0..200u8).map(|i| b'A' + i % 26))
            .collect();
        assert_eq!(
            find("=NEEDLE", &deep).as_deref(),
            Some("uvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrNEEDLEABCDEFGHIJKLM")
        );
        assert_eq!(
            find("=NEEDLE", b"one\ttwo\rthree\x00four\x01NEEDLE five\nsix").as_deref(),
            Some("four NEEDLE five six ")
        );
        assert_eq!(
            find("=needle", b"aaaaNEEDLE").as_deref(),
            Some("aaaaNEEDLE ")
        );
        assert_eq!(find("c=needle", b"aaaaNEEDLE"), None);
        assert_eq!(
            find("h=4e4545444c45", b"nopqrNEEDLEABCDEF").as_deref(),
            Some("nopqrNEEDLEABCDE ( 6e 6f 70 71 72 4e-45-45-44-4c-45 41 42 43 44 45 )")
        );
        assert_eq!(
            find("h=4e4545444c45", b"four\x01NEEDLE five").as_deref(),
            Some("four?NEEDLE five ( 66 6f 75 72 01 4e-45-45-44-4c-45 20 66 69 76 65 )")
        );
    }
}
