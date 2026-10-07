//! An archive in several files, read and written as one.
//!
//! A split archive's volumes (`NAME.001`, `NAME.002` and on, as 7-Zip's `-v` cuts them),
//! a zip's `NAME.z01` parts, and any other format whose bytes run on from one file into
//! the next: [`Spanned`] reads the parts as one file, [`numbered`] finds a split
//! archive's volumes from its first, [`SpannedWriter`] cuts one into volumes.
//!
//! A format with its own naming (RAR's `NAME.part1.rar`, or `NAME.rar`, `NAME.r00`)
//! gets its own function here beside [`numbered`].

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Several files read as one, in order.
pub struct Spanned<F> {
    files: Vec<F>,
    /// Where each part starts in the whole.
    starts: Vec<u64>,
    size: u64,
    position: u64,
}

impl<F: Read + Seek> Spanned<F> {
    /// The parts, in order.
    ///
    /// # Errors
    ///
    /// When a part's size cannot be found.
    pub fn new(mut files: Vec<F>) -> io::Result<Self> {
        let mut starts = Vec::with_capacity(files.len());
        let mut size = 0u64;
        for file in &mut files {
            starts.push(size);
            size += file.seek(SeekFrom::End(0))?;
        }
        Ok(Self {
            files,
            starts,
            size,
            position: 0,
        })
    }

    /// Where each part starts in the whole.
    pub fn bases(&self) -> Vec<u64> {
        self.starts.clone()
    }

    /// The parts' sizes added up.
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// The part `position` is in, and where in it; of parts starting at the same place
    /// (the empty ones), the last.
    fn locate(&self, position: u64) -> (usize, u64) {
        let index = self
            .starts
            .partition_point(|&start| start <= position)
            .saturating_sub(1);
        let start = self.starts.get(index).copied().unwrap_or(0);
        (index, position - start)
    }
}

impl<F: Read + Seek> Read for Spanned<F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.size || buf.is_empty() {
            return Ok(0);
        }
        let (index, within) = self.locate(self.position);
        let end = self.starts.get(index + 1).copied().unwrap_or(self.size);
        let want = usize::try_from(end - self.position)
            .unwrap_or(usize::MAX)
            .min(buf.len());
        let Some(file) = self.files.get_mut(index) else {
            return Ok(0);
        };
        file.seek(SeekFrom::Start(within))?;
        let n = file.read(buf.get_mut(..want).unwrap_or_default())?;
        self.position += n as u64;
        Ok(n)
    }
}

impl<F: Read + Seek> Seek for Spanned<F> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(at) => Some(at),
            SeekFrom::End(delta) => self.size.checked_add_signed(delta),
            SeekFrom::Current(delta) => self.position.checked_add_signed(delta),
        };
        self.position = target
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before the start"))?;
        Ok(self.position)
    }
}

/// A split archive's volumes from its first, `NAME.001`: it and `.002`, `.003` and on
/// while they are there, the number as wide as the first's (7-Zip's split handler).
pub fn numbered(first: &Path) -> Vec<PathBuf> {
    let mut parts = vec![first.to_path_buf()];
    let name = first.to_string_lossy().into_owned();
    let Some((stem, digits)) = name.rsplit_once('.') else {
        return parts;
    };
    let width = digits.len();
    for n in 2u64.. {
        let next = PathBuf::from(format!("{stem}.{n:0width$}"));
        if !next.is_file() {
            break;
        }
        parts.push(next);
    }
    parts
}

/// Volume `n` (from 1) of a split archive named `stem`: `NAME.001` and on.
pub fn numbered_path(stem: &Path, n: usize) -> PathBuf {
    let mut name = stem.as_os_str().to_owned();
    name.push(format!(".{n:03}"));
    PathBuf::from(name)
}

/// A split archive being written (7-Zip's `-v`): `NAME.001` and on.
///
/// Each volume is as large as `sizes` says, every one after the last as large as the
/// last; a volume is made when the writing reaches it, and seeking back writes into
/// those already made.
pub struct SpannedWriter {
    stem: PathBuf,
    sizes: Vec<u64>,
    files: Vec<File>,
    position: u64,
    /// How far the writing has gone.
    end: u64,
}

impl SpannedWriter {
    /// Volumes of `stem` in `sizes`.
    ///
    /// # Errors
    ///
    /// When the first volume cannot be made.
    pub fn create(stem: &Path, sizes: Vec<u64>) -> io::Result<Self> {
        let first = File::create(numbered_path(stem, 1))?;
        Ok(Self {
            stem: stem.to_path_buf(),
            sizes: if sizes.is_empty() {
                vec![u64::MAX]
            } else {
                sizes
            },
            files: vec![first],
            position: 0,
            end: 0,
        })
    }

    /// Volume `index`'s size, at least one byte.
    fn size_of(&self, index: usize) -> u64 {
        let last = self.sizes.len() - 1;
        self.sizes.get(index.min(last)).copied().unwrap_or(1).max(1)
    }

    /// The volume `position` is in, where in it, and the room left there.
    fn locate(&self, position: u64) -> (usize, u64, u64) {
        let mut start = 0u64;
        let mut index = 0;
        loop {
            let size = self.size_of(index);
            if position < start.saturating_add(size) {
                return (index, position - start, start + size - position);
            }
            start += size;
            index += 1;
        }
    }

    /// How many volumes there are.
    pub const fn volumes(&self) -> usize {
        self.files.len()
    }

    /// Drops what was written from `at` on: the volumes after the one `at` is in are
    /// removed, that one cut.
    ///
    /// # Errors
    ///
    /// When a volume cannot be cut or removed.
    pub fn truncate(&mut self, at: u64) -> io::Result<()> {
        if at >= self.end {
            return Ok(());
        }
        let (index, within, _) = self.locate(at);
        let keep = if within == 0 && index > 0 {
            index
        } else {
            index + 1
        };
        while self.files.len() > keep {
            self.files.pop();
            std::fs::remove_file(numbered_path(&self.stem, self.files.len() + 1))?;
        }
        if let Some(file) = self.files.get_mut(index) {
            file.set_len(within)?;
        }
        self.end = at;
        self.position = self.position.min(at);
        Ok(())
    }

    /// The whole size written, and how many volumes hold it.
    ///
    /// # Errors
    ///
    /// When a volume cannot be flushed.
    pub fn finish(mut self) -> io::Result<(u64, usize)> {
        for file in &mut self.files {
            file.flush()?;
        }
        Ok((self.end, self.files.len()))
    }
}

impl Write for SpannedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let (index, within, room) = self.locate(self.position);
        while self.files.len() <= index {
            let n = self.files.len() + 1;
            self.files.push(File::create(numbered_path(&self.stem, n))?);
        }
        let want = usize::try_from(room).unwrap_or(usize::MAX).min(buf.len());
        let Some(file) = self.files.get_mut(index) else {
            return Ok(0);
        };
        file.seek(SeekFrom::Start(within))?;
        let n = file.write(buf.get(..want).unwrap_or_default())?;
        self.position += n as u64;
        self.end = self.end.max(self.position);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        for file in &mut self.files {
            file.flush()?;
        }
        Ok(())
    }
}

impl Seek for SpannedWriter {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(at) => Some(at),
            SeekFrom::End(delta) => self.end.checked_add_signed(delta),
            SeekFrom::Current(delta) => self.position.checked_add_signed(delta),
        };
        self.position = target
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before the start"))?;
        Ok(self.position)
    }
}

/// The files at `paths`, open and read as one.
///
/// # Errors
///
/// When one cannot be opened.
pub fn open(paths: &[PathBuf]) -> io::Result<Spanned<File>> {
    Spanned::new(
        paths
            .iter()
            .map(File::open)
            .collect::<io::Result<Vec<_>>>()?,
    )
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn parts_read_as_one_empty_ones_and_all() {
        let parts = vec![
            Cursor::new(b"abc".to_vec()),
            Cursor::new(Vec::new()),
            Cursor::new(b"defg".to_vec()),
        ];
        let mut whole = Spanned::new(parts).unwrap();
        assert_eq!(whole.bases(), [0, 3, 3]);
        let mut all = Vec::new();
        whole.read_to_end(&mut all).unwrap();
        assert_eq!(all, b"abcdefg");
        whole.seek(SeekFrom::End(-5)).unwrap();
        let mut two = [0u8; 2];
        whole.read_exact(&mut two).unwrap();
        assert_eq!(&two, b"cd");
        whole.seek(SeekFrom::Start(3)).unwrap();
        whole.read_exact(&mut two).unwrap();
        assert_eq!(&two, b"de");
        assert!(whole.seek(SeekFrom::Current(-10)).is_err());
    }

    #[test]
    fn volumes_are_written_as_sized_and_rewritten_in_place() {
        let dir = std::env::temp_dir().join(format!("cash-volumes-w-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let stem = dir.join("x.7z");
        let mut out = SpannedWriter::create(&stem, vec![5, 3]).unwrap();
        out.write_all(b"0123456789AB").unwrap();
        out.seek(SeekFrom::Start(3)).unwrap();
        out.write_all(b"xyz").unwrap();
        out.seek(SeekFrom::End(0)).unwrap();
        assert_eq!(out.volumes(), 4);
        out.truncate(8).unwrap();
        assert_eq!(out.finish().unwrap(), (8, 2));
        let read = |n| std::fs::read(numbered_path(&stem, n)).unwrap();
        assert_eq!(read(1), b"012xy");
        assert_eq!(read(2), b"z67");
        assert!(!numbered_path(&stem, 3).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn numbered_volumes_stop_at_the_first_missing() {
        let dir = std::env::temp_dir().join(format!("cash-volumes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("x.7z.001");
        for (name, data) in [
            ("x.7z.001", &b"abc"[..]),
            ("x.7z.002", b""),
            ("x.7z.003", b"defg"),
            ("x.7z.005", b"zz"),
        ] {
            std::fs::write(dir.join(name), data).unwrap();
        }
        let parts = numbered(&first);
        assert_eq!(parts.len(), 3);
        let mut all = Vec::new();
        open(&parts).unwrap().read_to_end(&mut all).unwrap();
        assert_eq!(all, b"abcdefg");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
