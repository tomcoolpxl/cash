// This file is part of the uutils diffutils package.
//
// For the full copyright and license information, please view the LICENSE-*
// files that was distributed with this source code.

//! What the tools share: tab expansion, the time stamps of the `---`/`+++` headers,
//! GNU's words for an error, and GNU's quotes.

use std::ffi::OsStr;
use std::io::Write;
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, FixedOffset, Offset as _, Utc};
use unicode_width::UnicodeWidthStr;

/// Replace tabs by spaces in the input line.
/// Correctly handle multi-bytes characters.
/// This assumes that line does not contain any line breaks (if it does, the result is undefined).
#[must_use]
pub fn do_expand_tabs(line: &[u8], tabsize: usize) -> Vec<u8> {
    let tab = b'\t';
    let ntabs = line.iter().filter(|c| **c == tab).count();
    if ntabs == 0 {
        return line.to_vec();
    }
    let mut result = Vec::with_capacity(line.len() + ntabs * (tabsize - 1));
    let mut offset = 0;

    let mut iter = line.split(|c| *c == tab).peekable();
    while let Some(chunk) = iter.next() {
        match String::from_utf8(chunk.to_vec()) {
            Ok(s) => offset += UnicodeWidthStr::width(s.as_str()),
            Err(_) => offset += chunk.len(),
        }
        result.extend_from_slice(chunk);
        if iter.peek().is_some() {
            result.resize(result.len() + tabsize - offset % tabsize, b' ');
            offset = 0;
        }
    }

    result
}

/// Write a single line to an output stream, expanding tabs to space if necessary.
/// This assumes that line does not contain any line breaks
/// (if it does and tabs are to be expanded to spaces, the result is undefined).
pub fn do_write_line(
    output: &mut Vec<u8>,
    line: &[u8],
    expand_tabs: bool,
    tabsize: usize,
) -> std::io::Result<()> {
    if expand_tabs {
        output.write_all(do_expand_tabs(line, tabsize).as_slice())
    } else {
        output.write_all(line)
    }
}

/// The zone a time stamp is shown in: what an exported `TZ` names, as GNU diff shows
/// the file's time in the zone `TZ` names, else Windows' own zone (cash).
enum Zone {
    Local,
    Named(chrono_tz::Tz),
    Fixed(FixedOffset),
}

impl Zone {
    /// The zone `TZ` names: an IANA name (`Europe/Brussels`, `UTC`), a POSIX
    /// `NAME[+-]h[:mm]` (`JST-9`, `UTC0`; west of Greenwich positive, a daylight part
    /// after it not followed), or UTC for anything else, as in Bash. Unset: the
    /// machine's zone.
    fn from_env() -> Self {
        let Some(tz) = std::env::var_os("TZ") else {
            return Self::Local;
        };
        let tz = tz.to_string_lossy();
        let tz = tz.strip_prefix(':').unwrap_or(&tz);
        if let Ok(named) = tz.parse::<chrono_tz::Tz>() {
            return Self::Named(named);
        }
        Self::Fixed(posix_offset(tz).unwrap_or_else(|| Utc.fix()))
    }

    fn format(&self, when: DateTime<Utc>) -> String {
        const FORMAT: &str = "%Y-%m-%d %H:%M:%S%.9f %z";
        match self {
            Self::Local => when
                .with_timezone(&chrono::Local)
                .format(FORMAT)
                .to_string(),
            Self::Named(zone) => when.with_timezone(zone).format(FORMAT).to_string(),
            Self::Fixed(offset) => when.with_timezone(offset).format(FORMAT).to_string(),
        }
    }
}

/// A POSIX `TZ` of a name of three or more letters and an offset: `JST-9`, `UTC0`,
/// `CET-1CEST` (its daylight part not followed).
fn posix_offset(tz: &str) -> Option<FixedOffset> {
    let name_end = tz
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(tz.len());
    let (name, rest) = tz.split_at_checked(name_end)?;
    if name.len() < 3 {
        return None;
    }
    let (sign, rest) = match rest.as_bytes().first() {
        Some(b'-') => (-1, rest.get(1..)?),
        Some(b'+') => (1, rest.get(1..)?),
        _ => (1, rest),
    };
    let digits_end = rest
        .find(|c: char| !c.is_ascii_digit() && c != ':')
        .unwrap_or(rest.len());
    let mut parts = rest.get(..digits_end)?.split(':');
    let mut seconds = 0i32;
    for (unit, part) in [3600, 60, 1].into_iter().zip(parts.by_ref()) {
        seconds += unit * part.parse::<i32>().ok()?;
    }
    if parts.next().is_some() || seconds > 24 * 3600 {
        return None;
    }
    // POSIX counts west positive; chrono counts east positive.
    FixedOffset::east_opt(-sign * seconds)
}

/// `when` as a `---`/`+++` header shows it: `2024-01-02 03:04:05.000000000 +0000`.
#[must_use]
pub fn format_time(when: SystemTime) -> String {
    Zone::from_env().format(DateTime::<Utc>::from(when))
}

/// The modification time of `path` for a header; a file that cannot be asked (standard
/// input) shows the present, and an absent file (`-N`) the epoch, as GNU's does.
#[must_use]
pub fn modification_time(path: Option<&Path>) -> String {
    let when = match path {
        Some(path) => std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| SystemTime::now()),
        None => SystemTime::UNIX_EPOCH,
    };
    format_time(when)
}

/// What GNU says for an error reading `path`: `No such file or directory`, `Permission
/// denied`, `Is a directory`, where Windows says `The system cannot find the file
/// specified. (os error 2)`.
#[must_use]
pub fn error_words(error: &std::io::Error) -> String {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::NotFound => "No such file or directory".into(),
        ErrorKind::PermissionDenied => "Permission denied".into(),
        ErrorKind::IsADirectory => "Is a directory".into(),
        ErrorKind::NotADirectory => "Not a directory".into(),
        _ => {
            let text = error.to_string();
            let text = text
                .rsplit_once(" (os error ")
                .map_or(text.as_str(), |(head, _)| head)
                .trim_end_matches('.');
            text.to_owned()
        }
    }
}

/// `executable: path: words`, the line a tool prints for a file it cannot read.
#[must_use]
pub fn format_failure_to_read_input_file(
    executable: &OsStr,
    filepath: &OsStr,
    error: &std::io::Error,
) -> String {
    format!(
        "{}: {}: {}",
        executable.to_string_lossy(),
        filepath.to_string_lossy(),
        error_words(error)
    )
}

/// The locale the environment names: `LC_ALL`, else `LC_MESSAGES`, else `LANG`.
fn locale() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()))
}

/// Whether the locale named is `C` or `POSIX`: GNU's cmp then counts in `char`s. With
/// none named, cash is UTF-8, as its console, awk and sed are, where GNU is `C`.
#[must_use]
pub fn locale_is_posix() -> bool {
    locale().is_some_and(|l| l == "C" || l == "POSIX")
}

/// `text` in GNU's quotes: `‘text’` when the locale names UTF-8, else `'text'`.
#[must_use]
pub fn quote(text: &str) -> String {
    let curly = locale().is_some_and(|l| {
        let l = l.to_ascii_lowercase();
        l.contains("utf-8") || l.contains("utf8")
    });
    if curly {
        format!("\u{2018}{text}\u{2019}")
    } else {
        format!("'{text}'")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod expand_tabs {
        use super::*;
        use pretty_assertions::assert_eq;

        fn assert_tab_expansion(line: &str, tabsize: usize, expected: &str) {
            assert_eq!(
                do_expand_tabs(line.as_bytes(), tabsize),
                expected.as_bytes()
            );
        }

        #[test]
        fn basics() {
            assert_tab_expansion("foo barr   baz", 8, "foo barr   baz");
            assert_tab_expansion("foo\tbarr\tbaz", 8, "foo     barr    baz");
            assert_tab_expansion("foo\tbarr\tbaz", 5, "foo  barr baz");
            assert_tab_expansion("foo\tbarr\tbaz", 2, "foo barr  baz");
        }

        #[test]
        fn multibyte_chars() {
            assert_tab_expansion("foo\tépée\tbaz", 8, "foo     épée    baz");
            assert_tab_expansion("foo\t😉\tbaz", 5, "foo  😉   baz");

            // Note: The Woman Scientist emoji (👩‍🔬) is a ZWJ sequence combining
            // the Woman emoji (👩) and the Microscope emoji (🔬). On supported platforms
            // it is displayed as a single emoji and has a print size of 2 columns.
            // Terminal emulators tend to not support this, and display the two emojis
            // side by side, thus accounting for a print size of 4 columns, but the
            // unicode_width crate reports a correct size of 2.
            assert_tab_expansion("foo\t👩‍🔬\tbaz", 6, "foo   👩‍🔬    baz");
        }

        #[test]
        fn invalid_utf8() {
            // [240, 240, 152, 137] is an invalid UTF-8 sequence, so it is handled as 4 bytes
            assert_eq!(
                do_expand_tabs(&[240, 240, 152, 137, 9, 102, 111, 111], 8),
                &[240, 240, 152, 137, 32, 32, 32, 32, 102, 111, 111]
            );
        }
    }

    mod write_line {
        use super::*;
        use pretty_assertions::assert_eq;

        fn assert_line_written(line: &str, expand_tabs: bool, tabsize: usize, expected: &str) {
            let mut output: Vec<u8> = Vec::new();
            assert!(do_write_line(&mut output, line.as_bytes(), expand_tabs, tabsize).is_ok());
            assert_eq!(output, expected.as_bytes());
        }

        #[test]
        fn basics() {
            assert_line_written("foo bar baz", false, 8, "foo bar baz");
            assert_line_written("foo bar\tbaz", false, 8, "foo bar\tbaz");
            assert_line_written("foo bar\tbaz", true, 8, "foo bar baz");
        }
    }

    mod zone {
        use super::*;

        #[test]
        fn posix_offsets() {
            assert_eq!(posix_offset("UTC0"), FixedOffset::east_opt(0));
            assert_eq!(posix_offset("JST-9"), FixedOffset::east_opt(9 * 3600));
            assert_eq!(posix_offset("EST5"), FixedOffset::east_opt(-5 * 3600));
            assert_eq!(posix_offset("CET-1CEST"), FixedOffset::east_opt(3600));
            assert_eq!(posix_offset("X1"), None);
        }

        #[test]
        fn the_epoch_in_a_named_zone() {
            let zone = Zone::Named(chrono_tz::Europe::Brussels);
            assert_eq!(
                zone.format(DateTime::<Utc>::from(SystemTime::UNIX_EPOCH)),
                "1970-01-01 01:00:00.000000000 +0100"
            );
        }
    }

    #[test]
    fn error_words_are_gnus() {
        let missing = std::fs::read("target/utils/invalid-file").unwrap_err();
        assert_eq!(error_words(&missing), "No such file or directory");
    }
}
