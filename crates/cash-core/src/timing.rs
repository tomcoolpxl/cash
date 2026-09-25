//! Command timing

use crate::error;

struct StopwatchTime {
    now: std::time::SystemTime,
    self_user: std::time::Duration,
    self_system: std::time::Duration,
    children_user: std::time::Duration,
    children_system: std::time::Duration,
}

impl StopwatchTime {
    #[allow(clippy::unchecked_time_subtraction)]
    fn minus(&self, other: &Self) -> Result<StopwatchTiming, error::Error> {
        let user = (self.self_user - other.self_user) + (self.children_user - other.children_user);
        let system =
            (self.self_system - other.self_system) + (self.children_system - other.children_system);

        Ok(StopwatchTiming {
            wall: self.now.duration_since(other.now)?,
            user,
            system,
        })
    }
}

pub(crate) struct Stopwatch {
    start: StopwatchTime,
}

impl Stopwatch {
    pub fn stop(&self) -> Result<StopwatchTiming, error::Error> {
        let end = get_current_stopwatch_time()?;
        end.minus(&self.start)
    }
}
pub(crate) struct StopwatchTiming {
    pub wall: std::time::Duration,
    pub user: std::time::Duration,
    pub system: std::time::Duration,
}

pub(crate) fn start_timing() -> Result<Stopwatch, error::Error> {
    Ok(Stopwatch {
        start: get_current_stopwatch_time()?,
    })
}

fn get_current_stopwatch_time() -> Result<StopwatchTime, error::Error> {
    let now = std::time::SystemTime::now();
    let (self_user, self_system) = crate::sys::resource::get_self_user_and_system_time()?;
    let (children_user, children_system) =
        crate::sys::resource::get_children_user_and_system_time()?;

    Ok(StopwatchTime {
        now,
        self_user,
        self_system,
        children_user,
        children_system,
    })
}

/// Bash's report format for `time -p`.
pub(crate) const POSIX_TIMEFORMAT: &str = "real %2R\nuser %2U\nsys %2S";
/// Bash's report format when `$TIMEFORMAT` is unset.
pub(crate) const DEFAULT_TIMEFORMAT: &str = "\nreal\t%3lR\nuser\t%3lU\nsys\t%3lS";

/// Renders a `time` report from a `$TIMEFORMAT`-style format, as Bash does.
///
/// `%[p][l]R`, `%[p][l]U` and `%[p][l]S` print real, user and system seconds with `p`
/// digits after the decimal point (default 3; Bash 5.3 allows up to 6), `l` switching to
/// the `MmSS.FFs` form. `%P` is the CPU percentage and `%%` a literal `%`. A trailing
/// newline is appended. An invalid format character is returned as the error, and Bash
/// then prints nothing.
pub(crate) fn format_timing(format: &str, timing: &StopwatchTiming) -> Result<String, char> {
    use std::fmt::Write as _;

    let mut out = String::new();
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' || chars.peek().is_none() {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('%') => {
                chars.next();
                out.push('%');
            }
            Some('P') => {
                chars.next();
                let busy = (timing.user + timing.system).as_secs_f64();
                let wall = timing.wall.as_secs_f64();
                let percent = if wall > 0.0 { busy / wall * 100.0 } else { 0.0 };
                let _ = write!(out, "{percent:.2}");
            }
            _ => {
                let precision = if let Some(digit) = chars.peek().and_then(|d| d.to_digit(10)) {
                    chars.next();
                    digit.min(6)
                } else {
                    3
                };
                let long = chars.next_if_eq(&'l').is_some();
                let duration = match chars.next() {
                    Some('R' | 'E') => timing.wall,
                    Some('U') => timing.user,
                    Some('S') => timing.system,
                    other => return Err(other.unwrap_or('%')),
                };
                out.push_str(&format_seconds(&duration, precision, long));
            }
        }
    }
    out.push('\n');
    Ok(out)
}

/// Formats seconds with `precision` rounded decimal places, optionally as `MmSS.FFs`.
fn format_seconds(duration: &std::time::Duration, precision: u32, long: bool) -> String {
    use std::fmt::Write as _;

    let unit = 10_u64.pow(6 - precision);
    let micros = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
    let rounded = (micros + unit / 2) / unit * unit;
    let mut seconds = rounded / 1_000_000;
    let fraction = (rounded % 1_000_000) / unit;

    let mut out = String::new();
    if long {
        let _ = write!(out, "{}m", seconds / 60);
        seconds %= 60;
    }
    out.push_str(&seconds.to_string());
    if precision > 0 {
        let width = precision as usize;
        let _ = write!(out, ".{fraction:0width$}");
    }
    if long {
        out.push('s');
    }
    out
}

/// Format the given duration in a non-POSIX-y way.
///
/// # Arguments
///
/// * `duration` - The duration to format.
pub fn format_duration_non_posixly(duration: &std::time::Duration) -> String {
    let minutes = duration.as_secs() / 60;
    let seconds = duration.as_secs() % 60;
    let millis = duration.subsec_millis();
    format!("{minutes}m{seconds}.{millis:03}s")
}

/// Format the given duration in a POSIX-y way.
///
/// # Arguments
///
/// * `duration` - The duration to format.
pub fn format_duration_posixly(duration: &std::time::Duration) -> String {
    let seconds = duration.as_secs();
    let ten_millis = duration.subsec_millis() / 10;
    format!("{seconds}.{ten_millis:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_format_time() {
        assert_eq!(
            format_duration_non_posixly(&Duration::from_millis(0)),
            "0m0.000s"
        );
        assert_eq!(
            format_duration_non_posixly(&Duration::from_millis(1)),
            "0m0.001s"
        );
        assert_eq!(
            format_duration_non_posixly(&Duration::from_millis(123)),
            "0m0.123s"
        );
        assert_eq!(
            format_duration_non_posixly(&Duration::from_millis(1234)),
            "0m1.234s"
        );
        assert_eq!(
            format_duration_non_posixly(&Duration::from_millis(12345)),
            "0m12.345s"
        );
        assert_eq!(
            format_duration_non_posixly(&Duration::from_millis(123_456)),
            "2m3.456s"
        );
        assert_eq!(
            format_duration_non_posixly(&Duration::from_millis(1_234_567)),
            "20m34.567s"
        );

        assert_eq!(
            format_duration_non_posixly(&Duration::from_micros(1)),
            "0m0.000s"
        );
        assert_eq!(
            format_duration_non_posixly(&Duration::from_micros(999)),
            "0m0.000s"
        );
        assert_eq!(
            format_duration_non_posixly(&Duration::from_micros(1001)),
            "0m0.001s"
        );
    }
}
