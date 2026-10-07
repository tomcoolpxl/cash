use std::time::{Duration, SystemTime};

/// Why a time could not become an [`NtTime`].
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum NtTimeError {
    /// Before 1601-01-01.
    Negative,
    /// Past what 64 bits of 100 ns ticks hold.
    Overflow,
}

/// A Windows file time, as the 7z format stores times: 100 ns ticks since 1601-01-01 UTC.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NtTime(pub(crate) u64);

impl NtTime {
    const FILE_TIMES_PER_SEC: u64 = 10_000_000;

    /// Seconds from 1601-01-01 to 1970-01-01.
    const UNIX_EPOCH_SECONDS: u64 = 11_644_473_600;

    /// The [`NtTime`] of the Unix epoch (1970-01-01).
    pub const UNIX_EPOCH: Self = Self::new(Self::UNIX_EPOCH_SECONDS * Self::FILE_TIMES_PER_SEC);

    /// The epoch of the [`NtTime`] (1601-01-01).
    pub const NT_TIME_EPOCH: Self = Self::new(0);

    /// Creates a new [`NtTime`] with the given file time.
    #[must_use]
    #[inline]
    pub const fn new(ft: u64) -> Self {
        Self(ft)
    }

    /// The current time; the NT epoch on a clock set before 1601.
    #[must_use]
    pub fn now() -> Self {
        Self::try_from(SystemTime::now()).unwrap_or_default()
    }

    /// The time as a [`SystemTime`], or `None` past what the system's clock can hold.
    #[must_use]
    pub fn to_system_time(self) -> Option<SystemTime> {
        let ticks = Duration::new(
            self.0 / Self::FILE_TIMES_PER_SEC,
            // Below 10^7 ticks of 100 ns: under a second, so it fits.
            u32::try_from(self.0 % Self::FILE_TIMES_PER_SEC).unwrap_or(0) * 100,
        );
        SystemTime::UNIX_EPOCH
            .checked_sub(Duration::from_secs(Self::UNIX_EPOCH_SECONDS))?
            .checked_add(ticks)
    }
}

impl From<u64> for NtTime {
    /// Converts the file time to a [`NtTime`].
    #[inline]
    fn from(file_time: u64) -> Self {
        Self::new(file_time)
    }
}

impl From<NtTime> for u64 {
    /// Converts the [`NtTime`] into a file time.
    #[inline]
    fn from(nt_time: NtTime) -> Self {
        nt_time.0
    }
}

impl TryFrom<i64> for NtTime {
    type Error = NtTimeError;

    /// Converts the file time to a [`NtTime`].
    #[inline]
    fn try_from(file_time: i64) -> Result<Self, Self::Error> {
        file_time
            .try_into()
            .map_err(|_| NtTimeError::Negative)
            .map(Self::new)
    }
}

impl TryFrom<SystemTime> for NtTime {
    type Error = NtTimeError;

    /// Converts a [`SystemTime`] to a file time.
    fn try_from(st: SystemTime) -> Result<Self, Self::Error> {
        let start = SystemTime::UNIX_EPOCH
            .checked_sub(Duration::from_secs(Self::UNIX_EPOCH_SECONDS))
            .ok_or(NtTimeError::Negative)?;
        let elapsed = st
            .duration_since(start)
            .map_err(|_| NtTimeError::Negative)?
            .as_nanos();
        u64::try_from(elapsed / 100)
            .map(Self::new)
            .map_err(|_| NtTimeError::Overflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unix_epoch_round_trips() {
        assert_eq!(
            NtTime::try_from(SystemTime::UNIX_EPOCH),
            Ok(NtTime::UNIX_EPOCH)
        );
        assert_eq!(
            NtTime::UNIX_EPOCH.to_system_time(),
            Some(SystemTime::UNIX_EPOCH)
        );
    }

    #[test]
    fn the_largest_time_does_not_panic() {
        // Year 60056: past what Windows' clock holds, so `None` rather than a panic.
        let _ = NtTime::new(u64::MAX).to_system_time();
    }
}
