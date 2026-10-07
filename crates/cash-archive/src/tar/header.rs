//! The 512-byte tar header, its fields as GNU tar 1.35 reads and writes them.

/// A tar block.
pub const BLOCK: usize = 512;

/// The archive formats GNU tar writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    /// Unix V7: no magic, no owner names, names of 99 bytes at most.
    V7,
    /// GNU's format before 1.12: the full mode with its type bits.
    OldGnu,
    /// GNU's format, GNU tar's default: long names in `././@LongLink` members.
    Gnu,
    /// POSIX 1003.1-1988: long names split between prefix and name.
    Ustar,
    /// POSIX 1003.1-2001, pax: long names and more in extended headers.
    Posix,
}

impl Format {
    /// The format `--format` names.
    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "v7" => Some(Self::V7),
            "oldgnu" => Some(Self::OldGnu),
            "gnu" => Some(Self::Gnu),
            "ustar" => Some(Self::Ustar),
            "posix" | "pax" => Some(Self::Posix),
            _ => None,
        }
    }

    /// Whether this is one of GNU's formats, which allow base-256 numbers.
    pub const fn is_gnu(self) -> bool {
        matches!(self, Self::Gnu | Self::OldGnu)
    }
}

/// Where each field is: offset and length.
pub mod field {
    /// The name, or its last part with ustar's prefix.
    pub const NAME: (usize, usize) = (0, 100);
    /// The permissions; with the type bits in GNU's old format.
    pub const MODE: (usize, usize) = (100, 8);
    /// The owner's number.
    pub const UID: (usize, usize) = (108, 8);
    /// The group's number.
    pub const GID: (usize, usize) = (116, 8);
    /// The size of the data.
    pub const SIZE: (usize, usize) = (124, 12);
    /// The last change, in seconds since 1970.
    pub const MTIME: (usize, usize) = (136, 12);
    /// The sum of the header's bytes.
    pub const CHKSUM: (usize, usize) = (148, 8);
    /// What the member is.
    pub const TYPEFLAG: usize = 156;
    /// A link's target.
    pub const LINKNAME: (usize, usize) = (157, 100);
    /// `ustar` and a NUL, or GNU's `ustar ` and a space.
    pub const MAGIC: (usize, usize) = (257, 6);
    /// `00`, or GNU's space and NUL.
    pub const VERSION: (usize, usize) = (263, 2);
    /// The owner's name.
    pub const UNAME: (usize, usize) = (265, 32);
    /// The group's name.
    pub const GNAME: (usize, usize) = (297, 32);
    /// A device's major number.
    pub const DEVMAJOR: (usize, usize) = (329, 8);
    /// A device's minor number.
    pub const DEVMINOR: (usize, usize) = (337, 8);
    /// ustar's prefix: the folders before the name.
    pub const PREFIX: (usize, usize) = (345, 155);
    /// GNU's: the access time, after the device numbers.
    pub const ATIME: (usize, usize) = (345, 12);
    /// GNU's: the change time.
    pub const CTIME: (usize, usize) = (357, 12);
}

/// One header block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Header(pub [u8; BLOCK]);

impl Default for Header {
    fn default() -> Self {
        Self([0; BLOCK])
    }
}

/// What a block read as a header is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Check {
    /// Every byte zero: one of the two blocks that end an archive.
    Zero,
    /// The checksum agrees.
    Good,
    /// The checksum does not agree, or cannot be read.
    Bad,
}

impl Header {
    /// The bytes of a field.
    pub fn field(&self, (at, len): (usize, usize)) -> &[u8] {
        self.0.get(at..at + len).unwrap_or_default()
    }

    /// A text field, up to its first NUL.
    pub fn text(&self, place: (usize, usize)) -> &[u8] {
        let field = self.field(place);
        let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
        field.get(..end).unwrap_or_default()
    }

    /// Copies `bytes` into a field, cut to its length.
    pub fn set(&mut self, (at, len): (usize, usize), bytes: &[u8]) {
        let n = bytes.len().min(len);
        if let (Some(to), Some(from)) = (self.0.get_mut(at..at + n), bytes.get(..n)) {
            to.copy_from_slice(from);
        }
    }

    /// The type byte.
    pub fn typeflag(&self) -> u8 {
        self.0.get(field::TYPEFLAG).copied().unwrap_or(0)
    }

    /// Sets the type byte.
    pub fn set_typeflag(&mut self, flag: u8) {
        if let Some(byte) = self.0.get_mut(field::TYPEFLAG) {
            *byte = flag;
        }
    }

    /// GNU tar's `tar_checksum`: the unsigned or the signed sum of the block, with the
    /// checksum field taken as spaces.
    pub fn check(&self) -> Check {
        let unsigned: i64 = self.0.iter().map(|b| i64::from(*b)).sum();
        if unsigned == 0 {
            return Check::Zero;
        }
        let signed: i64 = self
            .0
            .iter()
            .map(|b| i64::from(i8::from_ne_bytes([*b])))
            .sum();
        let stored = self.field(field::CHKSUM);
        let stored_unsigned: i64 = stored.iter().map(|b| i64::from(*b)).sum();
        let stored_signed: i64 = stored
            .iter()
            .map(|b| i64::from(i8::from_ne_bytes([*b])))
            .sum();
        let blanks = 8 * i64::from(b' ');
        let unsigned = unsigned - stored_unsigned + blanks;
        let signed = signed - stored_signed + blanks;
        match read_number(stored) {
            Some(recorded)
                if i64::try_from(recorded).is_ok_and(|r| r == unsigned || r == signed) =>
            {
                Check::Good
            }
            _ => Check::Bad,
        }
    }

    /// Writes the checksum as GNU tar does: six octal digits, a NUL, a space.
    pub fn set_checksum(&mut self) {
        self.set(field::CHKSUM, b"        ");
        let sum: u32 = self.0.iter().map(|b| u32::from(*b)).sum();
        let digits = format!("{sum:06o}\0");
        self.set((field::CHKSUM.0, 7), digits.as_bytes());
    }
}

/// A number field as GNU tar's `from_header` reads it: octal, after spaces and before
/// a NUL or a space; or base-256, its first byte's high bit set. `None` when it is
/// neither.
pub fn read_number(field: &[u8]) -> Option<u64> {
    let first = *field.first()?;
    if first & 0x80 != 0 {
        if first == 0xff {
            return None;
        }
        let mut value = u64::from(first & 0x3f);
        for byte in field.get(1..).unwrap_or_default() {
            value = value.checked_mul(256)?.checked_add(u64::from(*byte))?;
        }
        return Some(value);
    }
    let text = field
        .iter()
        .skip_while(|b| **b == b' ')
        .take_while(|b| **b != 0 && **b != b' ');
    let mut value = 0_u64;
    let mut digits = 0;
    for byte in text {
        if !(b'0'..=b'7').contains(byte) {
            return None;
        }
        value = value.checked_mul(8)?.checked_add(u64::from(byte - b'0'))?;
        digits += 1;
    }
    let rest_ok = field
        .iter()
        .skip_while(|b| **b == b' ')
        .skip(digits)
        .all(|b| *b == 0 || *b == b' ');
    (rest_ok && (digits > 0 || field.iter().all(|b| *b == 0 || *b == b' '))).then_some(value)
}

/// A signed time field: base-256 may be negative.
pub fn read_time(field: &[u8]) -> Option<i64> {
    if field.first() == Some(&0xff) {
        let mut value: i64 = -1;
        for byte in field.get(1..).unwrap_or_default() {
            value = value.checked_mul(256)?.checked_add(i64::from(*byte))?;
        }
        return Some(value);
    }
    read_number(field).and_then(|v| i64::try_from(v).ok())
}

/// `value` in a number field of `len` bytes, as GNU tar's `to_chars` writes it: octal
/// in `len - 1` digits and a NUL when it fits; else, in GNU's formats, base-256.
/// `None` when it does not fit at all.
pub fn number_field(value: u64, len: usize, gnu: bool) -> Option<Vec<u8>> {
    let digits = len - 1;
    let octal_max = if digits * 3 >= 64 {
        u64::MAX
    } else {
        (1_u64 << (digits * 3)) - 1
    };
    if value <= octal_max {
        let mut out = format!("{value:0digits$o}").into_bytes();
        out.push(0);
        return Some(out);
    }
    if gnu {
        let bits = (len - 1) * 8;
        if bits >= 64 || value < (1_u64 << bits) {
            let mut out = vec![0_u8; len];
            if let Some(first) = out.first_mut() {
                *first = 0x80;
            }
            let mut rest = value;
            for byte in out.iter_mut().skip(1).rev() {
                *byte = (rest & 0xff) as u8;
                rest >>= 8;
            }
            return Some(out);
        }
    }
    None
}

/// A negative time in GNU's base-256.
pub fn negative_time_field(value: i64, len: usize) -> Vec<u8> {
    let mut out = vec![0xff_u8; len];
    let mut rest = value;
    for byte in out.iter_mut().skip(1).rev() {
        *byte = u8::try_from(rest & 0xff).unwrap_or(0);
        rest >>= 8;
    }
    out
}

/// Where GNU tar's `split_long_name` cuts a name for ustar's prefix: the index of the
/// slash, or 0 when there is none to cut at.
pub fn split_long_name(name: &[u8]) -> usize {
    let mut length = name.len();
    if length > 156 {
        length = 156;
    } else if name.get(length - 1) == Some(&b'/') {
        length -= 1;
    }
    (1..length)
        .rev()
        .find(|&i| name.get(i) == Some(&b'/'))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_and_write_as_gnu_tar_has_them() {
        assert_eq!(number_field(0o644, 8, true).unwrap(), b"0000644\0");
        assert_eq!(number_field(6, 12, true).unwrap(), b"00000000006\0");
        assert_eq!(read_number(b"0000644\0"), Some(0o644));
        assert_eq!(read_number(b"   644 \0"), Some(0o644));
        assert_eq!(read_number(b"\0\0\0\0"), Some(0));
        assert_eq!(read_number(b"12x\0"), None);
        let big = number_field(9 << 30, 12, true).unwrap();
        assert_eq!(big.first(), Some(&0x80));
        assert_eq!(read_number(&big), Some(9 << 30));
        assert_eq!(number_field(9 << 30, 12, false), None);
        assert_eq!(read_time(&negative_time_field(-5, 12)), Some(-5));
    }

    #[test]
    fn a_checksum_is_written_and_checked() {
        let mut header = Header::default();
        assert_eq!(header.check(), Check::Zero);
        header.set(field::NAME, b"src/");
        header.set_checksum();
        assert_eq!(header.check(), Check::Good);
        assert_eq!(
            header.field(field::CHKSUM).get(6..),
            Some(b"\0 ".as_slice())
        );
        header.set(field::NAME, b"src!");
        assert_eq!(header.check(), Check::Bad);
    }

    #[test]
    fn long_names_split_at_a_slash() {
        assert_eq!(split_long_name(b"abc/def"), 3);
        assert_eq!(split_long_name(b"abcdef"), 0);
        assert_eq!(split_long_name(b"abc/def/"), 3);
    }
}
