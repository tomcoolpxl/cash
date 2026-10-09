//! `-ag`: an archive's name with the date and time in it, as rar makes it, learned from
//! the names `Rar.exe` 7.23 gave and Rar.txt.
//!
//! The format's letters stand each for one character of a field, taken from its right:
//! `Y` is the year's last digit, `YYYY` all four, and letters beyond a field's width
//! give nothing more. `M` is the month, its name from three on, and minutes in the
//! first two `M` after an hour; `O` and `K` are the month's and the day's names, from
//! their left; `W` the week, `A` the day of the week, `D` the day, `E` the day of the
//! year, `H` the hour, `I` the minutes, `S` the seconds, `N` a number that makes the
//! name new. Letters are taken in either case; `{text}` is text; `:` and the other
//! characters a name cannot hold become `_`. A leading `+` puts the date before the
//! name. The date goes before the name's extension.

use chrono::{Datelike as _, NaiveDateTime, Timelike as _};

/// The format without one given: the year to the second.
pub(super) const DEFAULT_FORMAT: &str = "YYYYMMDDHHMMSS";

/// `name` with the date `format` gives in it. `exists` says whether an archive is
/// there; with `N` in the format, the first number whose name is new is taken when
/// `archiving`, else the one before it (1 when there is none).
pub(super) fn generated(
    name: &str,
    format: &str,
    now: NaiveDateTime,
    archiving: bool,
    exists: impl Fn(&str) -> bool,
) -> String {
    let (before, format) = match format.strip_prefix('+') {
        Some(rest) => (true, rest),
        None => (false, format),
    };
    let make = |number: u32| place(name, &stamp(format, now, number), before);
    if !numbered(format) {
        return make(1);
    }
    let first_new = (1..u32::MAX)
        .find(|&number| !exists(&make(number)))
        .unwrap_or(1);
    if archiving || first_new == 1 {
        make(first_new)
    } else {
        make(first_new - 1)
    }
}

/// Whether a format has `N`, outside braces.
fn numbered(format: &str) -> bool {
    let mut text = false;
    format.chars().any(|c| {
        match c {
            '{' => text = true,
            '}' => text = false,
            _ => {}
        }
        !text && c.eq_ignore_ascii_case(&'n')
    })
}

/// `date` put in `name`: before its extension, or with `before` ahead of its name.
fn place(name: &str, date: &str, before: bool) -> String {
    let at = name.rfind(['/', '\\']).map_or(0, |at| at + 1);
    let (folder, file) = name.split_at(at);
    if before {
        return format!("{folder}{date}{file}");
    }
    match file.rfind('.') {
        Some(dot) => {
            let (stem, extension) = file.split_at(dot);
            format!("{folder}{stem}{date}{extension}")
        }
        None => format!("{folder}{file}{date}"),
    }
}

/// What a format gives at `now`, with `number` for `N`.
fn stamp(format: &str, now: NaiveDateTime, number: u32) -> String {
    let chars: Vec<char> = format.chars().collect();
    let mut out = String::new();
    let mut after_hours = false;
    let mut minutes_given = 0;
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c == '{' {
            let end = chars[at..]
                .iter()
                .position(|&c| c == '}')
                .map_or(chars.len(), |end| at + end);
            out.extend(&chars[at + 1..end]);
            at = end + 1;
            continue;
        }
        let letter = c.to_ascii_lowercase();
        let run = chars[at..]
            .iter()
            .take_while(|other| other.eq_ignore_ascii_case(&c))
            .count();
        let field = match letter {
            'y' => Some(digits(now.year().unsigned_abs(), 4, run)),
            'm' if after_hours => {
                let taken = run.min(2 - minutes_given);
                minutes_given += taken;
                Some(digits(now.minute(), 2, taken))
            }
            'm' if run >= 3 => Some(left(MONTHS[now.month0() as usize], run)),
            'm' => Some(digits(now.month(), 2, run)),
            'o' => Some(left(MONTHS[now.month0() as usize], run)),
            'w' => Some(digits(week(now), 2, run)),
            'a' => Some(digits(now.weekday().number_from_monday(), 1, run)),
            'k' => Some(left(
                DAYS[now.weekday().num_days_from_monday() as usize],
                run,
            )),
            'd' => Some(digits(now.day(), 2, run)),
            'e' => Some(digits(now.ordinal(), 3, run)),
            'h' => {
                after_hours = true;
                Some(digits(now.hour(), 2, run))
            }
            'i' => Some(digits(now.minute(), 2, run)),
            's' => Some(digits(now.second(), 2, run)),
            'n' => Some(digits(number, run.max(1), run)),
            _ => None,
        };
        if let Some(text) = field {
            out.push_str(&text);
            at += run;
        } else {
            out.push(if matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            });
            at += 1;
        }
    }
    out
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// A number's last `run` digits, written `width` wide.
fn digits(value: u32, width: usize, run: usize) -> String {
    let text = format!("{value:0width$}");
    let keep = run.min(text.len());
    text.get(text.len() - keep..).unwrap_or_default().to_owned()
}

/// A name's first `run` letters.
fn left(name: &str, run: usize) -> String {
    name.chars().take(run).collect()
}

/// The week of the year, weeks starting on Monday, the one with the first of January
/// the first.
fn week(now: NaiveDateTime) -> u32 {
    let first = now.date().with_ordinal(1).unwrap_or_else(|| now.date());
    (now.ordinal0() + first.weekday().num_days_from_monday()) / 7 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 9)
            .and_then(|date| date.and_hms_opt(14, 51, 35))
            .unwrap_or_default()
    }

    #[test]
    fn letters_give_what_rar_7_23_gave() {
        for (format, want) in [
            ("YYYYMMDDHHMMSS", "20261009145135"),
            ("Y", "6"),
            ("YY", "26"),
            ("YYY", "026"),
            ("YYYYY", "2026"),
            ("M", "0"),
            ("MM", "10"),
            ("MMM", "Oct"),
            ("MMMMM", "Octob"),
            ("O", "O"),
            ("OO", "Oc"),
            ("W", "1"),
            ("WW", "41"),
            ("AA", "5"),
            ("KK", "Fr"),
            ("KKKKK", "Frida"),
            ("D", "9"),
            ("DDD", "09"),
            ("EE", "82"),
            ("EEEE", "282"),
            ("HHH", "14"),
            ("I", "1"),
            ("II", "51"),
            ("SS", "35"),
            ("NN", "01"),
            ("HHMM", "1451"),
            ("MMHH", "1014"),
            ("HH:MM", "14_51"),
            ("{at }HH", "at 14"),
            ("YYYY{-}", "2026-"),
            ("xyz", "x6z"),
            ("MM-HH-MM-MM", "10-14-51-"),
            ("Q", "Q"),
        ] {
            assert_eq!(stamp(format, at(), 1), want, "format {format}");
        }
    }

    #[test]
    fn the_date_goes_where_rar_7_23_put_it() {
        let none = |_: &str| false;
        for (name, format, want) in [
            ("x", "YYYY", "x2026"),
            ("x.rar", "YYYY", "x2026.rar"),
            ("x.part1.rar", "YYYY", "x.part12026.rar"),
            ("sub/x.rar", "YYYY", "sub/x2026.rar"),
            ("x.y", "YYYY", "x2026.y"),
            ("x.", "YYYY", "x2026."),
            ("x", "+YYYY", "2026x"),
            ("x", "+{x}", "xx"),
        ] {
            assert_eq!(generated(name, format, at(), true, none), want);
        }
        // N: the first new number archiving, the last there otherwise.
        let there = |name: &str| matches!(name, "g1.rar" | "g2.rar");
        assert_eq!(generated("g.rar", "N", at(), true, there), "g3.rar");
        assert_eq!(generated("g.rar", "N", at(), false, there), "g2.rar");
        assert_eq!(generated("g.rar", "N", at(), false, none), "g1.rar");
    }
}
