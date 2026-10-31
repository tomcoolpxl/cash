//! Times formatted with strftime's specifiers, as a prompt's `\D{…}`, `printf`'s
//! `%(…)T` and `HISTTIMEFORMAT` format them, in the zone `TZ` names, and the time the
//! shell started.

use std::fmt::Write as _;
use std::sync::OnceLock;
use std::time::SystemTime;

use chrono::{
    DateTime, Datelike as _, Days, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Offset as _,
    TimeDelta, TimeZone, Utc,
};

use crate::{Shell, extensions};

/// `datetime` formatted by `format`, strftime's specifiers. A specifier chrono does not
/// know gives nothing at all, as Git Bash's strftime does, where chrono's `to_string`
/// panics.
#[must_use]
pub fn strftime<Tz: TimeZone>(datetime: &DateTime<Tz>, format: &str) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let items = chrono::format::StrftimeItems::new(format);
    let mut formatted = String::new();
    if write!(formatted, "{}", datetime.format_with_items(items)).is_err() {
        formatted.clear();
    }
    formatted
}

/// The zone times are shown in: what an exported `TZ` names, as in Bash, else Windows'
/// own (the user, 2026-10-04).
#[derive(Clone, Debug)]
pub enum Zone {
    /// Windows' time zone: `TZ` is not exported.
    Local,
    /// An IANA name: `Europe/Brussels`, `UTC`, `EST5EDT`.
    Named(chrono_tz::Tz),
    /// A POSIX `NAME±h[:mm[:ss]]`, west of Greenwich positive: `JST-9`, `UTC0`,
    /// `<+0330>-3:30`. A `TZ` that is none of these, or empty, is UTC, as in Bash.
    Fixed {
        /// What `%Z` shows.
        name: String,
        /// Its offset from UTC.
        offset: FixedOffset,
    },
    /// A POSIX `TZ` with a daylight-saving part, `EST5EDT,M3.2.0,M11.1.0`.
    Rules(Rules),
}

impl Zone {
    /// The zone `tz`, a `TZ` value, names; `None` is Windows' own.
    #[must_use]
    pub fn from_tz(tz: Option<&str>) -> Self {
        let Some(tz) = tz else {
            return Self::Local;
        };
        let tz = tz.strip_prefix(':').unwrap_or(tz);
        if let Ok(named) = tz.parse::<chrono_tz::Tz>() {
            return Self::Named(named);
        }
        posix(tz).unwrap_or_else(|| Self::Fixed {
            name: String::from("UTC"),
            offset: Utc.fix(),
        })
    }

    /// The zone of `shell`'s exported `TZ`.
    #[must_use]
    pub fn of_shell(shell: &Shell<impl extensions::ShellExtensions>) -> Self {
        let tz = shell
            .env()
            .get("TZ")
            .filter(|(_, var)| var.is_exported())
            .map(|(_, var)| var.value().to_cow_str(shell).into_owned());
        Self::from_tz(tz.as_deref())
    }

    /// `when` in this zone, formatted by `format`, strftime's specifiers (see
    /// [`strftime`]).
    #[must_use]
    pub fn format(&self, when: DateTime<Utc>, format: &str) -> String {
        match self {
            Self::Local => strftime(&when.with_timezone(&chrono::Local), format),
            Self::Named(zone) => strftime(&when.with_timezone(zone), format),
            Self::Fixed { name, offset } => {
                strftime(&when.with_timezone(offset), &with_zone_name(format, name))
            }
            Self::Rules(rules) => {
                let period = rules.period_at(when);
                strftime(
                    &when.with_timezone(&period.offset),
                    &with_zone_name(format, &period.name),
                )
            }
        }
    }

    /// `when`, a time the system gave, formatted as [`Self::format`] does.
    #[must_use]
    pub fn format_system_time(&self, when: SystemTime, format: &str) -> String {
        self.format(DateTime::<Utc>::from(when), format)
    }

    /// `when` as this zone's wall time.
    #[must_use]
    pub fn to_local(&self, when: DateTime<Utc>) -> NaiveDateTime {
        match self {
            Self::Local => when.with_timezone(&chrono::Local).naive_local(),
            Self::Named(zone) => when.with_timezone(zone).naive_local(),
            Self::Fixed { offset, .. } => when.with_timezone(offset).naive_local(),
            Self::Rules(rules) => when
                .with_timezone(&rules.period_at(when).offset)
                .naive_local(),
        }
    }

    /// A wall time in this zone as Unix seconds: the earlier of two in the hour the
    /// clocks go back, none in the hour they skip.
    #[must_use]
    pub fn local_to_unix(&self, wall: NaiveDateTime) -> Option<i64> {
        match self {
            Self::Local => wall
                .and_local_timezone(chrono::Local)
                .earliest()
                .map(|t| t.timestamp()),
            Self::Named(zone) => wall
                .and_local_timezone(*zone)
                .earliest()
                .map(|t| t.timestamp()),
            Self::Fixed { offset, .. } => wall
                .and_local_timezone(*offset)
                .earliest()
                .map(|t| t.timestamp()),
            Self::Rules(rules) => rules.local_to_unix(wall),
        }
    }
}

/// A POSIX `TZ` with daylight-saving time.
///
/// `EST5EDT,M3.2.0,M11.1.0` is standard time `EST`, five hours west of Greenwich, and
/// daylight time `EDT` an hour ahead of it from the second Sunday of March at 02:00
/// standard time to the first Sunday of November at 02:00 daylight time. A daylight name
/// with no rules after it gets the United States' rules, as glibc gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    standard: Period,
    daylight: Period,
    /// When daylight time starts, read in standard time.
    start: Change,
    /// When it ends, read in daylight time.
    end: Change,
}

/// Standard or daylight time: its name, for `%Z`, and its offset.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Period {
    name: String,
    offset: FixedOffset,
}

/// A change of the clocks: a day of the year, and a time of that day in seconds after
/// its midnight, which may fall before it or after the next midnight (`/-1`, `/26`), up
/// to a week either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Change {
    day: Day,
    seconds: i32,
}

/// A day of the year, as a rule names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Day {
    /// `Jn`: the n-th day of the year, 1 to 365, February 29 never counted, so that a day
    /// after it keeps its number in a leap year.
    Julian(u16),
    /// `n`: the n-th day of the year, 0 to 365, February 29 counted.
    Ordinal(u16),
    /// `Mm.w.d`: the d-th day of the week (0 is Sunday) of the w-th week of month m,
    /// week 5 the last such day of the month.
    Weekday { month: u8, week: u8, weekday: u8 },
}

/// The United States' rules since 2007, for a daylight name without rules: the second
/// Sunday of March to the first Sunday of November, at 02:00.
const DEFAULT_START: Change = Change {
    day: Day::Weekday {
        month: 3,
        week: 2,
        weekday: 0,
    },
    seconds: 2 * 3600,
};
const DEFAULT_END: Change = Change {
    day: Day::Weekday {
        month: 11,
        week: 1,
        weekday: 0,
    },
    seconds: 2 * 3600,
};

impl Rules {
    /// The period `when` falls in.
    fn period_at(&self, when: DateTime<Utc>) -> &Period {
        // The changes of the year `when` falls in, read in standard time: the hours a
        // new year is ahead or behind in the other periods never hold a change.
        let year = when.with_timezone(&self.standard.offset).year();
        let start = self.start.at(year, self.standard.offset);
        let end = self.end.at(year, self.daylight.offset);
        let daylight = if start <= end {
            start <= when && when < end
        } else {
            // South of the equator daylight time spans the new year.
            !(end <= when && when < start)
        };
        if daylight {
            &self.daylight
        } else {
            &self.standard
        }
    }

    /// The Unix seconds of `wall` read in this zone: the earlier of two in the hour the
    /// clocks go back, none in the hour they skip.
    fn local_to_unix(&self, wall: NaiveDateTime) -> Option<i64> {
        [&self.standard, &self.daylight]
            .into_iter()
            .filter_map(|period| {
                let candidate = wall.and_local_timezone(period.offset).single()?;
                let candidate = candidate.with_timezone(&Utc);
                (self.period_at(candidate).offset == period.offset).then(|| candidate.timestamp())
            })
            .min()
    }
}

impl Change {
    /// The instant of this change in `year`, its time read in the period of `offset`.
    fn at(self, year: i32, offset: FixedOffset) -> DateTime<Utc> {
        let midnight = self.day.date(year).and_time(NaiveTime::MIN);
        let local = midnight + TimeDelta::seconds(i64::from(self.seconds));
        let utc = local - TimeDelta::seconds(i64::from(offset.local_minus_utc()));
        DateTime::<Utc>::from_naive_utc_and_offset(utc, Utc)
    }
}

impl Day {
    /// The date this day is in `year`.
    fn date(self, year: i32) -> NaiveDate {
        let first = NaiveDate::from_ymd_opt(year, 1, 1).unwrap_or_default();
        match self {
            Self::Julian(day) => {
                let past_february_29 = u64::from(first.leap_year() && day >= 60);
                first + Days::new(u64::from(day.saturating_sub(1)) + past_february_29)
            }
            Self::Ordinal(day) => first + Days::new(u64::from(day)),
            Self::Weekday {
                month,
                week,
                weekday,
            } => {
                let first_of_month =
                    NaiveDate::from_ymd_opt(year, u32::from(month), 1).unwrap_or(first);
                let ahead =
                    (u32::from(weekday) + 7 - first_of_month.weekday().num_days_from_sunday()) % 7;
                let weeks = u64::from(week.saturating_sub(1));
                let date = first_of_month + Days::new(u64::from(ahead) + 7 * weeks);
                // Week 5 is the last such day of the month, which a shorter month has in
                // its fourth week.
                if date.month() == u32::from(month) {
                    date
                } else {
                    date - Days::new(7)
                }
            }
        }
    }
}

/// A POSIX `TZ`: a name of three or more letters, or anything in `<…>`, and an offset
/// (`JST-9`, `UTC0`, `<+0330>-3:30`); after it, a daylight-saving name, its offset if
/// not an hour ahead, and the rules the clocks change by (`EST5EDT,M3.2.0,M11.1.0`).
/// `None` where it is not one.
fn posix(tz: &str) -> Option<Zone> {
    let (standard_name, rest) = zone_name(tz)?;
    let (standard_seconds, rest) = offset(rest, 24 * 3600)?;
    // West of Greenwich is positive in `TZ`, east in chrono.
    let standard = Period {
        name: standard_name,
        offset: FixedOffset::west_opt(standard_seconds)?,
    };
    if rest.is_empty() {
        return Some(Zone::Fixed {
            name: standard.name,
            offset: standard.offset,
        });
    }
    let (daylight_name, rest) = zone_name(rest)?;
    let (daylight_seconds, rest) = match offset(rest, 24 * 3600) {
        Some(given) => given,
        None => (standard_seconds - 3600, rest),
    };
    let daylight = Period {
        name: daylight_name,
        offset: FixedOffset::west_opt(daylight_seconds)?,
    };
    let (start, end) = match rest.strip_prefix(',') {
        None if rest.is_empty() => (DEFAULT_START, DEFAULT_END),
        None => return None,
        Some(rules) => {
            let (start, rules) = change(rules)?;
            let (end, rules) = change(rules.strip_prefix(',')?)?;
            if !rules.is_empty() {
                return None;
            }
            (start, end)
        }
    };
    Some(Zone::Rules(Rules {
        standard,
        daylight,
        start,
        end,
    }))
}

/// The name `tz` starts with, and what follows: three or more letters, or what stands in
/// `<…>`, where digits and signs are allowed too (`<+0330>`).
fn zone_name(tz: &str) -> Option<(String, &str)> {
    if let Some(quoted) = tz.strip_prefix('<') {
        let end = quoted.find('>')?;
        let (name, rest) = quoted.split_at_checked(end)?;
        let allowed = |c: char| c.is_ascii_alphanumeric() || c == '+' || c == '-';
        if name.len() < 3 || !name.chars().all(allowed) {
            return None;
        }
        return Some((name.to_owned(), rest.get(1..)?));
    }
    let end = tz
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(tz.len());
    if end < 3 {
        return None;
    }
    let (name, rest) = tz.split_at_checked(end)?;
    Some((name.to_owned(), rest))
}

/// The `[+|-]hh[:mm[:ss]]` `text` starts with, in seconds, and what follows; `None` when
/// there is none, or it is beyond `limit` seconds either way.
fn offset(text: &str, limit: i32) -> Option<(i32, &str)> {
    let (sign, digits) = match text.as_bytes().first() {
        Some(b'-') => (-1, text.get(1..)?),
        Some(b'+') => (1, text.get(1..)?),
        _ => (1, text),
    };
    let end = digits
        .find(|c: char| !c.is_ascii_digit() && c != ':')
        .unwrap_or(digits.len());
    let (fields, rest) = digits.split_at_checked(end)?;
    if fields.is_empty() || fields.starts_with(':') || fields.ends_with(':') {
        return None;
    }
    let mut seconds = 0i32;
    let mut parts = fields.split(':');
    for (unit, part) in [3600i32, 60, 1].into_iter().zip(parts.by_ref()) {
        seconds = seconds.checked_add(unit.checked_mul(part.parse::<i32>().ok()?)?)?;
    }
    if parts.next().is_some() || seconds > limit {
        return None;
    }
    Some((sign * seconds, rest))
}

/// The change `text` starts with, `Jn`, `n` or `Mm.w.d` with `/time` after it or 02:00
/// without, and what follows. A time may be negative or beyond a day, up to 167 hours
/// either way, as glibc and Cygwin read it.
fn change(text: &str) -> Option<(Change, &str)> {
    let (day, rest) = if let Some(julian) = text.strip_prefix('J') {
        let (day, rest) = number(julian)?;
        if !(1..=365).contains(&day) {
            return None;
        }
        (Day::Julian(day), rest)
    } else if let Some(fields) = text.strip_prefix('M') {
        let (month, rest) = number(fields)?;
        let (week, rest) = number(rest.strip_prefix('.')?)?;
        let (weekday, rest) = number(rest.strip_prefix('.')?)?;
        if !(1..=12).contains(&month) || !(1..=5).contains(&week) || weekday > 6 {
            return None;
        }
        let day = Day::Weekday {
            month: u8::try_from(month).ok()?,
            week: u8::try_from(week).ok()?,
            weekday: u8::try_from(weekday).ok()?,
        };
        (day, rest)
    } else {
        let (day, rest) = number(text)?;
        if day > 365 {
            return None;
        }
        (Day::Ordinal(day), rest)
    };
    let (seconds, rest) = match rest.strip_prefix('/') {
        Some(time) => offset(time, 167 * 3600)?,
        None => (2 * 3600, rest),
    };
    Some((Change { day, seconds }, rest))
}

/// The decimal number `text` starts with, and what follows.
fn number(text: &str) -> Option<(u16, &str)> {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let (digits, rest) = text.split_at_checked(end)?;
    Some((digits.parse().ok()?, rest))
}

/// `format` with each `%Z` spelled out as `name`: chrono shows a fixed offset's name as
/// the offset.
fn with_zone_name(format: &str, name: &str) -> String {
    let mut out = String::with_capacity(format.len());
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('Z') => out.push_str(&name.replace('%', "%%")),
            Some(next) => {
                out.push('%');
                out.push(next);
            }
            None => out.push('%'),
        }
    }
    out
}

static STARTED: OnceLock<SystemTime> = OnceLock::new();

/// When the shell started: when its first `Shell` was built. `printf`'s `%(…)T` formats
/// it for the argument `-2`, as Bash does.
pub fn shell_started() -> SystemTime {
    *STARTED.get_or_init(SystemTime::now)
}

#[cfg(test)]
mod tests {
    use super::{Zone, strftime};

    #[test]
    fn an_unknown_specifier_gives_nothing_rather_than_a_panic() {
        let epoch = chrono::DateTime::from_timestamp(0, 0).unwrap();
        assert_eq!(strftime(&epoch, "%Y-%m-%d"), "1970-01-01");
        assert_eq!(strftime(&epoch, "[%Q]"), "");
    }

    #[test]
    fn tz_names_a_zone_as_bash_reads_it() {
        let when = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let show = |tz: &str| Zone::from_tz(Some(tz)).format(when, "%Y-%m-%d %H:%M %Z");
        assert_eq!(show("UTC"), "2023-11-14 22:13 UTC");
        assert_eq!(show("UTC0"), "2023-11-14 22:13 UTC");
        assert_eq!(show("JST-9"), "2023-11-15 07:13 JST");
        assert_eq!(show("XYZ+3:30"), "2023-11-14 18:43 XYZ");
        assert_eq!(show("<+0330>-3:30"), "2023-11-15 01:43 +0330");
        assert_eq!(show("Europe/Brussels"), "2023-11-14 23:13 CET");
        assert_eq!(show(":Asia/Tokyo"), "2023-11-15 07:13 JST");
        // An unknown name, or none, is UTC; so is a name too short, or a quoted one with
        // a character a name may not hold, or an offset beyond a day.
        assert_eq!(show("Nowhere/Land"), "2023-11-14 22:13 UTC");
        assert_eq!(show(""), "2023-11-14 22:13 UTC");
        assert_eq!(show("XY-9"), "2023-11-14 22:13 UTC");
        assert_eq!(show("<X Y>-9"), "2023-11-14 22:13 UTC");
        assert_eq!(show("XYZ-25"), "2023-11-14 22:13 UTC");
        // `%%Z` is a literal `%Z`.
        assert_eq!(
            Zone::from_tz(Some("JST-9")).format(when, "%%Z %Z"),
            "%Z JST"
        );
    }

    /// `when`, a UTC time, shown in the zone `tz` names, as Git Bash 5.3's `date` shows
    /// it (`+%F %T %Z %z`), where it reads a POSIX `TZ` with Cygwin's rules.
    fn show(tz: &str, when: &str) -> String {
        let when = when.parse::<chrono::DateTime<chrono::Utc>>().unwrap();
        Zone::from_tz(Some(tz)).format(when, "%F %T %Z %z")
    }

    #[test]
    fn tz_rules_change_the_clocks_as_git_bash_does() {
        // The United States, at 02:00: the second Sunday of March to the first of November.
        let us = "EST5EDT,M3.2.0,M11.1.0";
        assert_eq!(
            show(us, "2026-01-15T12:00:00Z"),
            "2026-01-15 07:00:00 EST -0500"
        );
        assert_eq!(
            show(us, "2026-07-15T12:00:00Z"),
            "2026-07-15 08:00:00 EDT -0400"
        );
        assert_eq!(
            show(us, "2026-03-08T06:59:59Z"),
            "2026-03-08 01:59:59 EST -0500"
        );
        assert_eq!(
            show(us, "2026-03-08T07:00:00Z"),
            "2026-03-08 03:00:00 EDT -0400"
        );
        assert_eq!(
            show(us, "2026-11-01T05:59:59Z"),
            "2026-11-01 01:59:59 EDT -0400"
        );
        assert_eq!(
            show(us, "2026-11-01T06:00:00Z"),
            "2026-11-01 01:00:00 EST -0500"
        );
        // The same, spelled out, and with the changes at 03:00 and 01:00.
        let spelled = "EST5EDT,M3.2.0/2,M11.1.0/2";
        assert_eq!(
            show(spelled, "2026-03-08T07:00:00Z"),
            "2026-03-08 03:00:00 EDT -0400"
        );
        let other_hours = "EST5EDT,M3.2.0/3,M11.1.0/1";
        assert_eq!(
            show(other_hours, "2026-03-08T07:59:59Z"),
            "2026-03-08 02:59:59 EST -0500"
        );
        assert_eq!(
            show(other_hours, "2026-03-08T08:00:00Z"),
            "2026-03-08 04:00:00 EDT -0400"
        );
        assert_eq!(
            show(other_hours, "2026-11-01T05:59:59Z"),
            "2026-11-01 00:59:59 EST -0500"
        );
    }

    #[test]
    fn tz_rules_name_a_day_of_the_year_as_git_bash_does() {
        // Europe: the last Sundays of March and October, at 01:00 UTC.
        let eu = "CET-1CEST,M3.5.0,M10.5.0/3";
        assert_eq!(
            show(eu, "2026-03-29T00:59:59Z"),
            "2026-03-29 01:59:59 CET +0100"
        );
        assert_eq!(
            show(eu, "2026-03-29T01:00:00Z"),
            "2026-03-29 03:00:00 CEST +0200"
        );
        assert_eq!(
            show(eu, "2026-10-25T00:59:59Z"),
            "2026-10-25 02:59:59 CEST +0200"
        );
        assert_eq!(
            show(eu, "2026-10-25T01:00:00Z"),
            "2026-10-25 02:00:00 CET +0100"
        );
        // Days of the year: `J60` is March 1 whatever the year, `59` is in one that is
        // not a leap year.
        for days in ["EST5EDT,J60/2,J300/2", "EST5EDT,59,299"] {
            assert_eq!(
                show(days, "2026-03-01T06:59:59Z"),
                "2026-03-01 01:59:59 EST -0500"
            );
            assert_eq!(
                show(days, "2026-03-01T07:00:00Z"),
                "2026-03-01 03:00:00 EDT -0400"
            );
            assert_eq!(
                show(days, "2026-10-27T05:59:59Z"),
                "2026-10-27 01:59:59 EDT -0400"
            );
            assert_eq!(
                show(days, "2026-10-27T06:00:00Z"),
                "2026-10-27 01:00:00 EST -0500"
            );
        }
        // In a leap year, `59` is February 29 and `J60` still March 1.
        assert_eq!(
            show("EST5EDT,J60/2,J300/2", "2028-02-29T07:00:00Z"),
            "2028-02-29 02:00:00 EST -0500"
        );
        assert_eq!(
            show("EST5EDT,J60/2,J300/2", "2028-03-01T07:00:00Z"),
            "2028-03-01 03:00:00 EDT -0400"
        );
        assert_eq!(
            show("EST5EDT,59,299", "2028-02-29T07:00:00Z"),
            "2028-02-29 03:00:00 EDT -0400"
        );
    }

    #[test]
    fn tz_rules_south_of_the_equator_quoted_and_at_odd_hours_as_git_bash_does() {
        // South of the equator, daylight time spans the new year.
        let nz = "NZST-12NZDT,M9.5.0,M4.1.0/3";
        assert_eq!(
            show(nz, "2026-01-15T12:00:00Z"),
            "2026-01-16 01:00:00 NZDT +1300"
        );
        assert_eq!(
            show(nz, "2026-07-15T12:00:00Z"),
            "2026-07-16 00:00:00 NZST +1200"
        );
        assert_eq!(
            show(nz, "2026-04-04T13:59:59Z"),
            "2026-04-05 02:59:59 NZDT +1300"
        );
        assert_eq!(
            show(nz, "2026-04-04T14:00:00Z"),
            "2026-04-05 02:00:00 NZST +1200"
        );
        assert_eq!(
            show(nz, "2026-09-26T13:59:59Z"),
            "2026-09-27 01:59:59 NZST +1200"
        );
        assert_eq!(
            show(nz, "2026-09-26T14:00:00Z"),
            "2026-09-27 03:00:00 NZDT +1300"
        );
        // Quoted names, and times before midnight.
        let south = "<-03>3<-02>,M3.5.0/-2,M10.5.0/-1";
        assert_eq!(
            show(south, "2026-01-15T12:00:00Z"),
            "2026-01-15 09:00:00 -03 -0300"
        );
        assert_eq!(
            show(south, "2026-07-15T12:00:00Z"),
            "2026-07-15 10:00:00 -02 -0200"
        );
        assert_eq!(
            show(south, "2026-03-29T00:59:59Z"),
            "2026-03-28 21:59:59 -03 -0300"
        );
        assert_eq!(
            show(south, "2026-03-29T01:00:00Z"),
            "2026-03-28 23:00:00 -02 -0200"
        );
        assert_eq!(
            show(south, "2026-10-25T00:59:59Z"),
            "2026-10-24 22:59:59 -02 -0200"
        );
        assert_eq!(
            show(south, "2026-10-25T01:00:00Z"),
            "2026-10-24 22:00:00 -03 -0300"
        );
        // A daylight offset of its own, and a change beyond midnight or a week on.
        let half = "EST5EDT4:30,M3.2.0,M11.1.0";
        assert_eq!(
            show(half, "2026-07-15T12:00:00Z"),
            "2026-07-15 07:30:00 EDT -0430"
        );
        assert_eq!(
            show(half, "2026-03-08T07:00:00Z"),
            "2026-03-08 02:30:00 EDT -0430"
        );
        assert_eq!(
            show(half, "2026-11-01T06:00:00Z"),
            "2026-11-01 01:30:00 EDT -0430"
        );
        assert_eq!(
            show(half, "2026-11-01T06:59:59Z"),
            "2026-11-01 01:59:59 EST -0500"
        );
        let late = "EST5EDT,M3.2.0/26,M11.1.0/26";
        assert_eq!(
            show(late, "2026-03-08T08:00:00Z"),
            "2026-03-08 03:00:00 EST -0500"
        );
        assert_eq!(
            show(late, "2026-03-09T07:00:00Z"),
            "2026-03-09 03:00:00 EDT -0400"
        );
        assert_eq!(
            show(late, "2026-11-01T07:00:00Z"),
            "2026-11-01 03:00:00 EDT -0400"
        );
        let week_on = "EST5EDT,M3.2.0/167,M11.1.0";
        assert_eq!(
            show(week_on, "2026-03-08T08:00:00Z"),
            "2026-03-08 03:00:00 EST -0500"
        );
        assert_eq!(
            show(week_on, "2026-03-15T04:00:00Z"),
            "2026-03-15 00:00:00 EDT -0400"
        );
    }

    #[test]
    fn tz_rules_inverted_with_any_names_by_default_or_bad_as_git_bash_does() {
        // Ireland's winter is the daylight-saving part, an hour behind.
        let ireland = "IST-1GMT0,M10.5.0,M3.5.0/1";
        assert_eq!(
            show(ireland, "2026-01-15T12:00:00Z"),
            "2026-01-15 12:00:00 GMT +0000"
        );
        assert_eq!(
            show(ireland, "2026-07-15T12:00:00Z"),
            "2026-07-15 13:00:00 IST +0100"
        );
        // Any names; and without rules, the United States' at 02:00.
        let made_up = "XYZ3ABC,M3.2.0,M11.1.0";
        assert_eq!(
            show(made_up, "2026-01-15T12:00:00Z"),
            "2026-01-15 09:00:00 XYZ -0300"
        );
        assert_eq!(
            show(made_up, "2026-07-15T12:00:00Z"),
            "2026-07-15 10:00:00 ABC -0200"
        );
        assert_eq!(
            show("XYZ3ABC", "2026-07-15T12:00:00Z"),
            "2026-07-15 10:00:00 ABC -0200"
        );
        assert_eq!(
            show("XYZ3ABC", "2026-03-08T07:00:00Z"),
            "2026-03-08 05:00:00 ABC -0200"
        );
        // Rules that are not ones are no zone at all, UTC.
        for bad in [
            "EST5EDT,M13.1.0,M11.1.0",
            "EST5EDT,M3.2.0",
            "EST5EDT,J0,J300",
            "EST5EDT,M3.2.0/168,M11.1.0",
            "EST5EDT,M3.2.0,M11.1.0,",
        ] {
            assert_eq!(
                show(bad, "2026-07-15T12:00:00Z"),
                "2026-07-15 12:00:00 UTC +0000",
                "{bad}"
            );
        }
    }

    #[test]
    fn a_wall_time_under_rules_is_the_earlier_of_two_and_none_in_the_skipped_hour() {
        let us = Zone::from_tz(Some("EST5EDT,M3.2.0,M11.1.0"));
        let wall = |text: &str| text.parse::<chrono::NaiveDateTime>().unwrap();
        assert_eq!(
            us.local_to_unix(wall("2026-07-15T08:00:00")),
            Some(1_784_116_800)
        );
        assert_eq!(
            us.local_to_unix(wall("2026-11-01T01:30:00")),
            Some(1_793_511_000)
        );
        assert_eq!(us.local_to_unix(wall("2026-03-08T02:30:00")), None);
        let when = chrono::DateTime::from_timestamp(1_784_116_800, 0).unwrap();
        assert_eq!(us.to_local(when), wall("2026-07-15T08:00:00"));
    }
}
