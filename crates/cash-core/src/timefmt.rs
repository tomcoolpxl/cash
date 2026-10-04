//! Times formatted with strftime's specifiers, as a prompt's `\D{…}`, `printf`'s
//! `%(…)T` and `HISTTIMEFORMAT` format them, in the zone `TZ` names, and the time the
//! shell started.

use std::fmt::Write as _;
use std::sync::OnceLock;
use std::time::SystemTime;

use chrono::{DateTime, FixedOffset, Offset as _, TimeZone, Utc};

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
    /// A POSIX `NAME±h[:mm[:ss]]`, west of Greenwich positive: `JST-9`, `UTC0`. A
    /// daylight-saving part after it is not followed. A `TZ` that is neither, or empty,
    /// is UTC, as in Bash.
    Fixed {
        /// What `%Z` shows.
        name: String,
        /// Its offset from UTC.
        offset: FixedOffset,
    },
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
        posix_offset(tz).unwrap_or_else(|| Self::Fixed {
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
        }
    }

    /// `when`, a time the system gave, formatted as [`Self::format`] does.
    #[must_use]
    pub fn format_system_time(&self, when: SystemTime, format: &str) -> String {
        self.format(DateTime::<Utc>::from(when), format)
    }
}

/// A POSIX `TZ` of a name of three or more letters and an offset: `JST-9`, `UTC0`,
/// `CET-1CEST` (its daylight part not followed).
fn posix_offset(tz: &str) -> Option<Zone> {
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
    // West of Greenwich is positive in `TZ`, east in chrono.
    let offset = FixedOffset::west_opt(sign * seconds)?;
    Some(Zone::Fixed {
        name: name.to_owned(),
        offset,
    })
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
        assert_eq!(show("Europe/Brussels"), "2023-11-14 23:13 CET");
        assert_eq!(show(":Asia/Tokyo"), "2023-11-15 07:13 JST");
        // An unknown name, or none, is UTC.
        assert_eq!(show("Nowhere/Land"), "2023-11-14 22:13 UTC");
        assert_eq!(show(""), "2023-11-14 22:13 UTC");
        // `%%Z` is a literal `%Z`.
        assert_eq!(
            Zone::from_tz(Some("JST-9")).format(when, "%%Z %Z"),
            "%Z JST"
        );
    }
}
