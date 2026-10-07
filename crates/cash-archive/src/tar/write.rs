//! Writing a tar archive as GNU tar 1.35 does, header by header: with the same options,
//! a member's blocks are GNU's byte for byte.

use std::io::{self, Read, Write};

use super::header::{
    BLOCK, Format, Header, field, negative_time_field, number_field, split_long_name,
};
use crate::member::{Kind, Member, Timestamp};

/// Why a member cannot be written, in GNU tar's terms.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NameProblem {
    /// "file name is too long (max N); not dumped".
    TooLong(usize),
    /// "file name is too long (cannot be split); not dumped".
    CannotSplit,
    /// "link name is too long; not dumped".
    LinkTooLong,
}

/// What writing a member came to.
#[derive(Debug)]
pub enum WriteError {
    /// The member's name or link cannot be held by the format.
    Name(NameProblem),
    /// Reading the file's data failed.
    Read(io::Error),
    /// Writing the archive failed.
    Write(io::Error),
}

/// How a file's data compared with the size it had when its header was written.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Written {
    /// The file gave this many bytes fewer than its size: GNU's "File shrank by N bytes;
    /// padding with zeros".
    pub shrank: u64,
}

/// How to write members.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// The format written.
    pub format: Format,
    /// `--numeric-owner`: no owner names.
    pub numeric_owner: bool,
    /// The record size in blocks: the archive ends padded to a whole record.
    pub record_blocks: u64,
}

/// A tar archive being written.
pub struct Writer<W: Write> {
    out: W,
    options: Options,
    /// Blocks written since the record count began.
    blocks: u64,
}

/// A pax record, `LENGTH KEY=VALUE\n`, its length counting itself.
fn pax_record(key: &str, value: &[u8]) -> Vec<u8> {
    let body = key.len() + value.len() + 3;
    let mut length = body + 1;
    while length != body + length.to_string().len() {
        length = body + length.to_string().len();
    }
    let mut record = format!("{length} {key}=").into_bytes();
    record.extend_from_slice(value);
    record.push(b'\n');
    record
}

/// A pax time: seconds, and the nanoseconds when there are any.
fn pax_time(time: Timestamp) -> Vec<u8> {
    if time.nanos == 0 {
        return time.seconds.to_string().into_bytes();
    }
    let (seconds, nanos) = if time.seconds < 0 {
        (time.seconds + 1, 1_000_000_000 - time.nanos)
    } else {
        (time.seconds, time.nanos)
    };
    let sign = if time.seconds < 0 && seconds == 0 {
        "-"
    } else {
        ""
    };
    let fraction = format!("{nanos:09}");
    format!("{sign}{seconds}.{}", fraction.trim_end_matches('0')).into_bytes()
}

/// The type bits of a kind, which only GNU's old format stores in the mode.
const fn type_bits(kind: Kind) -> u32 {
    match kind {
        Kind::Dir => 0o040_000,
        Kind::Symlink => 0o120_000,
        Kind::Char => 0o020_000,
        Kind::Block => 0o060_000,
        Kind::Fifo => 0o010_000,
        Kind::File | Kind::HardLink | Kind::Other(_) => 0o100_000,
    }
}

impl<W: Write> Writer<W> {
    /// An archive written to `out`.
    pub const fn new(out: W, options: Options) -> Self {
        Self {
            out,
            options,
            blocks: 0,
        }
    }

    /// Blocks written so far.
    pub const fn blocks(&self) -> u64 {
        self.blocks
    }

    /// Counts `blocks` already in the archive, written before this writer: the record
    /// padding at the end counts them.
    pub const fn start_at(&mut self, blocks: u64) {
        self.blocks = blocks;
    }

    /// A private header: GNU tar's `start_private_header`, for long names and pax.
    fn private_header(&self, name: &[u8], size: u64, mtime: i64, flag: u8) -> Header {
        let mut header = Header::default();
        header.set(field::NAME, name);
        let gnu = self.options.format.is_gnu();
        if let Some(size) = number_field(size, 12, gnu) {
            header.set(field::SIZE, &size);
        }
        if let Some(time) = number_field(u64::try_from(mtime.max(0)).unwrap_or(0), 12, gnu) {
            header.set(field::MTIME, &time);
        }
        let mode = if self.options.format == Format::OldGnu {
            0o100_644
        } else {
            0o644
        };
        if let Some(mode) = number_field(mode, 8, gnu) {
            header.set(field::MODE, &mode);
        }
        if let Some(zero) = number_field(0, 8, gnu) {
            header.set(field::UID, &zero);
            header.set(field::GID, &zero);
        }
        header.set_typeflag(flag);
        header
    }

    /// GNU's `././@LongLink` member holding `name`, of type `flag`.
    fn long_link(&self, name: &[u8], flag: u8, out: &mut Vec<u8>) {
        let size = name.len() as u64 + 1;
        let mut header = self.private_header(b"././@LongLink", size, 0, flag);
        if !self.options.numeric_owner {
            header.set(field::UNAME, b"root");
            header.set(field::GNAME, b"root");
        }
        header.set((field::MAGIC.0, 8), b"ustar  \0");
        header.set_checksum();
        out.extend_from_slice(&header.0);
        let mut data = name.to_vec();
        data.push(0);
        data.resize(data.len().div_ceil(BLOCK) * BLOCK, 0);
        out.extend_from_slice(&data);
    }

    /// Every block of `member`'s headers: GNU's long name and link members or a pax
    /// header, then its own.
    ///
    /// # Errors
    ///
    /// When the format cannot hold its name or its link.
    #[expect(
        clippy::too_many_lines,
        reason = "GNU tar's start_header, field by field in its order"
    )]
    pub fn headers(&self, member: &Member) -> Result<Vec<u8>, NameProblem> {
        let format = self.options.format;
        let gnu = format.is_gnu();
        let kind = member.kind();
        let mut out = Vec::new();
        let mut pax: Vec<u8> = Vec::new();
        let name_limit = 100 - usize::from(format == Format::OldGnu);
        let ascii = |bytes: &[u8]| bytes.iter().all(u8::is_ascii);

        // The link first, as GNU's dump_hard_link and dump_file0 write it.
        if matches!(kind, Kind::HardLink | Kind::Symlink) && member.link.len() > name_limit {
            match format {
                Format::Posix => pax.extend(pax_record("linkpath", &member.link)),
                Format::V7 | Format::Ustar => return Err(NameProblem::LinkTooLong),
                Format::Gnu | Format::OldGnu => self.long_link(&member.link, b'K', &mut out),
            }
        }
        let mut header = Header::default();
        let name = &member.name;
        if format == Format::Posix && !ascii(name) {
            pax.extend(pax_record("path", name));
            header.set(field::NAME, name);
        } else if name.len() > name_limit {
            match format {
                Format::Posix => {
                    pax.extend(pax_record("path", name));
                    header.set(field::NAME, name);
                }
                Format::V7 => return Err(NameProblem::TooLong(99)),
                Format::Ustar => {
                    if name.len() > 256 {
                        return Err(NameProblem::TooLong(256));
                    }
                    let at = split_long_name(name);
                    let rest = name.len().saturating_sub(at + 1);
                    if at == 0 || rest > 100 || rest == 0 {
                        return Err(NameProblem::CannotSplit);
                    }
                    header.set(field::PREFIX, name.get(..at).unwrap_or_default());
                    header.set(field::NAME, name.get(at + 1..).unwrap_or_default());
                }
                Format::Gnu | Format::OldGnu => {
                    self.long_link(name, b'L', &mut out);
                    header.set(field::NAME, name);
                }
            }
        } else {
            header.set(field::NAME, name);
        }

        let mode = if format == Format::OldGnu {
            type_bits(kind) | (member.mode & 0o7777)
        } else {
            member.mode & 0o7777
        };
        if let Some(mode) = number_field(u64::from(mode), 8, gnu) {
            header.set(field::MODE, &mode);
        }
        for (place, value, key) in [
            (field::UID, member.uid, "uid"),
            (field::GID, member.gid, "gid"),
        ] {
            if let Some(bytes) = number_field(value, 8, gnu) {
                header.set(place, &bytes);
            } else {
                if format == Format::Posix {
                    pax.extend(pax_record(key, value.to_string().as_bytes()));
                }
                if let Some(zero) = number_field(0, 8, gnu) {
                    header.set(place, &zero);
                }
            }
        }
        let size = if matches!(kind, Kind::File | Kind::Other(_)) {
            member.size
        } else {
            0
        };
        if let Some(bytes) = number_field(size, 12, gnu) {
            header.set(field::SIZE, &bytes);
        } else {
            pax.extend(pax_record("size", size.to_string().as_bytes()));
            if let Some(zero) = number_field(0, 12, gnu) {
                header.set(field::SIZE, &zero);
            }
        }
        let mtime = member.mtime;
        let seconds = u64::try_from(mtime.seconds).ok();
        if format == Format::Posix
            && (mtime.nanos != 0 || seconds.is_none_or(|s| number_field(s, 12, false).is_none()))
        {
            pax.extend(pax_record("mtime", &pax_time(mtime)));
        }
        match seconds.and_then(|s| number_field(s, 12, gnu)) {
            Some(bytes) => header.set(field::MTIME, &bytes),
            None if gnu && mtime.seconds < 0 => {
                header.set(field::MTIME, &negative_time_field(mtime.seconds, 12));
            }
            None => {
                if let Some(zero) = number_field(0, 12, gnu) {
                    header.set(field::MTIME, &zero);
                }
            }
        }
        if matches!(kind, Kind::Char | Kind::Block) {
            for (place, value) in [
                (field::DEVMAJOR, member.device.0),
                (field::DEVMINOR, member.device.1),
            ] {
                if let Some(bytes) = number_field(u64::from(value), 8, gnu) {
                    header.set(place, &bytes);
                }
            }
        }
        if format == Format::Posix {
            if let Some(atime) = member.atime {
                pax.extend(pax_record("atime", &pax_time(atime)));
            }
            if let Some(ctime) = member.ctime {
                pax.extend(pax_record("ctime", &pax_time(ctime)));
            }
        }
        header.set_typeflag(match kind {
            Kind::File if format == Format::V7 => 0,
            Kind::File => b'0',
            Kind::HardLink => b'1',
            Kind::Symlink => b'2',
            Kind::Char => b'3',
            Kind::Block => b'4',
            Kind::Dir => b'5',
            Kind::Fifo => b'6',
            Kind::Other(flag) => flag,
        });
        match format {
            Format::V7 => {}
            Format::Gnu | Format::OldGnu => header.set((field::MAGIC.0, 8), b"ustar  \0"),
            Format::Ustar | Format::Posix => {
                header.set(field::MAGIC, b"ustar\0");
                header.set(field::VERSION, b"00");
            }
        }
        if format != Format::V7 && !self.options.numeric_owner {
            for (place, value, key) in [
                (field::UNAME, &member.uname, "uname"),
                (field::GNAME, &member.gname, "gname"),
            ] {
                if format == Format::Posix && (value.len() > 32 || !ascii(value)) {
                    pax.extend(pax_record(key, value));
                }
                header.set(place, value);
            }
        }
        if matches!(kind, Kind::HardLink | Kind::Symlink) {
            header.set(field::LINKNAME, &member.link);
        }
        if !pax.is_empty() {
            out.extend_from_slice(&self.pax_header(member, &pax));
        }
        header.set_checksum();
        out.extend_from_slice(&header.0);
        Ok(out)
    }

    /// A pax extended header for `member`: GNU's `%d/PaxHeaders/%f`.
    fn pax_header(&self, member: &Member, records: &[u8]) -> Vec<u8> {
        let name = member.name.strip_suffix(b"/").unwrap_or(&member.name);
        let (dir, base) = match name.iter().rposition(|b| *b == b'/') {
            Some(at) => (
                name.get(..at).unwrap_or_default(),
                name.get(at + 1..).unwrap_or_default(),
            ),
            None => (b".".as_slice(), name),
        };
        let mut pax_name = dir.to_vec();
        pax_name.extend_from_slice(b"/PaxHeaders/");
        pax_name.extend_from_slice(base);
        pax_name.truncate(100);
        let mut header =
            self.private_header(&pax_name, records.len() as u64, member.mtime.seconds, b'x');
        header.set(field::MAGIC, b"ustar\0");
        header.set(field::VERSION, b"00");
        header.set_checksum();
        let mut out = header.0.to_vec();
        out.extend_from_slice(records);
        out.resize(BLOCK + records.len().div_ceil(BLOCK) * BLOCK, 0);
        out
    }

    /// Writes blocks as they are: members copied from another archive.
    ///
    /// # Errors
    ///
    /// When writing fails.
    pub fn write_raw(&mut self, blocks: &[u8]) -> io::Result<()> {
        self.out.write_all(blocks)?;
        self.blocks += (blocks.len() / BLOCK) as u64;
        Ok(())
    }

    /// Writes `member`, its data read from `data` for a file: exactly its size, padded
    /// with zeros when the file gives less, as GNU tar pads a file that shrank.
    ///
    /// # Errors
    ///
    /// When its name cannot be held, or reading or writing fails.
    pub fn write_member(
        &mut self,
        member: &Member,
        data: Option<&mut dyn Read>,
    ) -> Result<Written, WriteError> {
        let headers = self.headers(member).map_err(WriteError::Name)?;
        self.write_raw(&headers).map_err(WriteError::Write)?;
        let mut written = Written::default();
        let size = if matches!(member.kind(), Kind::File | Kind::Other(_)) {
            member.size
        } else {
            0
        };
        if size == 0 {
            return Ok(written);
        }
        let mut left = size;
        let mut block = [0_u8; BLOCK];
        let mut source = data;
        while left > 0 {
            let want = usize::try_from(left.min(BLOCK as u64)).unwrap_or(BLOCK);
            let mut got = 0;
            if let Some(input) = source.as_deref_mut() {
                while got < want {
                    match input.read(block.get_mut(got..want).unwrap_or_default()) {
                        Ok(0) => break,
                        Ok(n) => got += n,
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                        Err(e) => return Err(WriteError::Read(e)),
                    }
                }
            }
            if got < want {
                written.shrank = left - got as u64;
                source = None;
            }
            if let Some(rest) = block.get_mut(got..) {
                rest.fill(0);
            }
            self.out.write_all(&block).map_err(WriteError::Write)?;
            self.blocks += 1;
            left -= want as u64;
        }
        Ok(written)
    }

    /// Ends the archive: two zero blocks, then zeros to the end of the record.
    ///
    /// # Errors
    ///
    /// When writing fails.
    pub fn finish(mut self) -> io::Result<W> {
        let record = self.options.record_blocks.max(1);
        let total = (self.blocks + 2).div_ceil(record) * record;
        let zeros = [0_u8; BLOCK];
        for _ in self.blocks..total {
            self.out.write_all(&zeros)?;
        }
        self.out.flush()?;
        Ok(self.out)
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test stops on what it did not expect")]
mod tests {
    use super::*;
    use crate::tar::read::{Event, Reader};

    fn member(name: &str, kind: Kind, size: u64) -> Member {
        Member {
            name: name.as_bytes().to_vec(),
            kind: Some(kind),
            size,
            mode: if kind == Kind::Dir { 0o755 } else { 0o644 },
            mtime: Timestamp::seconds(1_577_934_245),
            ..Member::default()
        }
    }

    #[test]
    fn gnu_headers_are_gnu_tars_bytes() {
        let writer = Writer::new(
            Vec::new(),
            Options {
                format: Format::Gnu,
                numeric_owner: true,
                record_blocks: 20,
            },
        );
        let blocks = writer.headers(&member("src/", Kind::Dir, 0)).unwrap();
        let header = Header(blocks.as_slice().try_into().unwrap());
        assert_eq!(header.field(field::MODE), b"0000755\0");
        assert_eq!(header.field(field::MTIME), b"13603256645\0");
        assert_eq!(header.field(field::CHKSUM), b"006545\0 ");
        assert_eq!(header.typeflag(), b'5');
    }

    #[test]
    fn what_is_written_reads_back() {
        let mut writer = Writer::new(
            Vec::new(),
            Options {
                format: Format::Gnu,
                numeric_owner: false,
                record_blocks: 20,
            },
        );
        let long = format!("{}/file", "d".repeat(120));
        let mut file = member(&long, Kind::File, 6);
        file.uname = b"me".to_vec();
        writer
            .write_member(&file, Some(&mut b"hello\n".as_slice()))
            .unwrap();
        let mut short = member("short", Kind::File, 10);
        let written = writer
            .write_member(&short, Some(&mut b"abc".as_slice()))
            .unwrap();
        assert_eq!(written.shrank, 7);
        short.size = 0;
        let archive = writer.finish().unwrap();
        assert_eq!(archive.len() % 10240, 0);
        let mut reader = Reader::new(archive.as_slice(), false);
        let Event::Member(first) = reader.next_event().unwrap() else {
            panic!("no member");
        };
        assert_eq!(first.member.name, long.as_bytes());
        assert_eq!(first.member.uname, b"me");
        let mut data = Vec::new();
        reader.copy_data(&mut data).unwrap();
        assert_eq!(data, b"hello\n");
        assert!(matches!(reader.next_event().unwrap(), Event::Member(_)));
        assert!(matches!(reader.next_event().unwrap(), Event::End));
    }

    #[test]
    fn pax_records_count_their_own_length() {
        assert_eq!(pax_record("path", b"abc"), b"12 path=abc\n");
        assert_eq!(
            pax_time(Timestamp {
                seconds: 5,
                nanos: 500_000_000
            }),
            b"5.5"
        );
    }
}
