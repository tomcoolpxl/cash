//! An archive entry as every format has it: a name, a kind, a size, Unix's mode, owner
//! and times, and a link's target.
//!
//! tar reads into it and writes from it; `walk` makes it from files on disk; `extract`
//! and the listings take it. What only one format has travels beside it.

/// What an entry is.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Kind {
    /// A regular file.
    File,
    /// A folder.
    Dir,
    /// A symbolic link to `Member::link`.
    Symlink,
    /// A second name of `Member::link`, stored earlier in the archive.
    HardLink,
    /// A character device.
    Char,
    /// A block device.
    Block,
    /// A named pipe.
    Fifo,
    /// A tar type this reader does not know, by its type byte.
    Other(u8),
}

/// A time as Unix keeps it: seconds since 1970, and nanoseconds within the second.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Timestamp {
    /// Seconds since 1970-01-01 UTC; negative before.
    pub seconds: i64,
    /// Nanoseconds within the second.
    pub nanos: u32,
}

impl Timestamp {
    /// A whole number of seconds.
    pub const fn seconds(seconds: i64) -> Self {
        Self { seconds, nanos: 0 }
    }

    /// The time as `std` has it, when it can hold it.
    pub fn system_time(self) -> Option<std::time::SystemTime> {
        let base = std::time::UNIX_EPOCH;
        let whole = std::time::Duration::from_secs(self.seconds.unsigned_abs());
        let at = if self.seconds >= 0 {
            base.checked_add(whole)?
        } else {
            base.checked_sub(whole)?
        };
        at.checked_add(std::time::Duration::from_nanos(u64::from(self.nanos)))
    }

    /// The time `std` gives a file.
    pub fn from_system_time(time: std::time::SystemTime) -> Self {
        match time.duration_since(std::time::UNIX_EPOCH) {
            Ok(after) => Self {
                seconds: i64::try_from(after.as_secs()).unwrap_or(i64::MAX),
                nanos: after.subsec_nanos(),
            },
            Err(before) => {
                let before = before.duration();
                let seconds = i64::try_from(before.as_secs()).unwrap_or(i64::MAX);
                if before.subsec_nanos() == 0 {
                    Self::seconds(-seconds)
                } else {
                    Self {
                        seconds: -seconds - 1,
                        nanos: 1_000_000_000 - before.subsec_nanos(),
                    }
                }
            }
        }
    }
}

/// An archive entry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Member {
    /// The name, `/`-separated, as the archive has it.
    pub name: Vec<u8>,
    /// What it is; a regular file when unset.
    pub kind: Option<Kind>,
    /// The size of the data the archive holds for it.
    pub size: u64,
    /// The permission bits, with set-id and sticky: `0o7777` at most.
    pub mode: u32,
    /// The owner's number.
    pub uid: u64,
    /// The group's number.
    pub gid: u64,
    /// The owner's name, empty when the archive has none.
    pub uname: Vec<u8>,
    /// The group's name, empty when the archive has none.
    pub gname: Vec<u8>,
    /// The last change of its contents.
    pub mtime: Timestamp,
    /// The last read, when the archive keeps it.
    pub atime: Option<Timestamp>,
    /// The last change of its metadata, when the archive keeps it.
    pub ctime: Option<Timestamp>,
    /// A link's target.
    pub link: Vec<u8>,
    /// A device's major and minor numbers.
    pub device: (u32, u32),
}

impl Member {
    /// The kind, a regular file when unset.
    pub fn kind(&self) -> Kind {
        self.kind.unwrap_or(Kind::File)
    }

    /// The name as text, lossily where it is not UTF-8.
    pub fn name_text(&self) -> String {
        String::from_utf8_lossy(&self.name).into_owned()
    }
}
