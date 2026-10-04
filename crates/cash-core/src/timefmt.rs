//! Times formatted with strftime's specifiers, as a prompt's `\D{…}`, `printf`'s
//! `%(…)T` and `HISTTIMEFORMAT` format them, and the time the shell started.

use std::fmt::Write as _;
use std::sync::OnceLock;
use std::time::SystemTime;

/// `datetime` formatted by `format`, strftime's specifiers. A specifier chrono does not
/// know gives nothing at all, as Git Bash's strftime does, where chrono's `to_string`
/// panics.
#[must_use]
pub fn strftime<Tz: chrono::TimeZone>(datetime: &chrono::DateTime<Tz>, format: &str) -> String
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

static STARTED: OnceLock<SystemTime> = OnceLock::new();

/// When the shell started: when its first `Shell` was built. `printf`'s `%(…)T` formats
/// it for the argument `-2`, as Bash does.
pub fn shell_started() -> SystemTime {
    *STARTED.get_or_init(SystemTime::now)
}

#[cfg(test)]
mod tests {
    use super::strftime;

    #[test]
    fn an_unknown_specifier_gives_nothing_rather_than_a_panic() {
        let epoch = chrono::DateTime::from_timestamp(0, 0).unwrap();
        assert_eq!(strftime(&epoch, "%Y-%m-%d"), "1970-01-01");
        assert_eq!(strftime(&epoch, "[%Q]"), "");
    }
}
