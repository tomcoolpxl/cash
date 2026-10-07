//! Changing an archive's members: `-A` and `--delete`, blocks copied as they are.

use std::fs;
use std::io::{self, Seek, SeekFrom, Write};

use cash_archive::codec::{Codec, Lookahead};
use cash_archive::select;
use cash_archive::tar::BLOCK;
use cash_archive::tar::read::{CopyError, Event, ReadError, Reader};
use cash_archive::tar::write::Writer;
use cash_win32::unix::Replacement;

use super::options::TRY;
use super::{Fatal, Tar, Wanted};
use crate::compress::strerror;

impl<SE: cash_core::ShellExtensions> Tar<'_, SE> {
    /// Copies every member of `reader` to `writer`, but those `skip` says no to.
    fn copy_members<R: io::Read, W: Write>(
        &mut self,
        reader: &mut Reader<R>,
        writer: &mut Writer<W>,
        mut skip: impl FnMut(&[u8]) -> bool,
    ) -> Result<(), Fatal> {
        loop {
            let event = match reader.next_event() {
                Ok(event) => event,
                Err(ReadError::UnexpectedEof) => {
                    self.say("Unexpected EOF in archive")?;
                    return Err(self.fatal_quiet());
                }
                Err(ReadError::Io(e)) => return Err(e.into()),
            };
            match event {
                Event::Member(entry) => {
                    if skip(&entry.member.name) {
                        continue;
                    }
                    writer.write_raw(&entry.raw)?;
                    let mut data = Vec::new();
                    match reader.copy_raw_data(&mut data) {
                        Ok(()) => writer.write_raw(&data)?,
                        Err(CopyError::Read(_)) => {
                            self.say("Unexpected EOF in archive")?;
                            return Err(self.fatal_quiet());
                        }
                        Err(CopyError::Write(e)) => return Err(e.into()),
                    }
                }
                Event::NotTar => self.error("This does not look like a tar archive")?,
                Event::Skipping => self.error("Skipping to next header")?,
                Event::LoneZero(_) | Event::End => return Ok(()),
            }
        }
    }

    /// `-A`: the members of each named archive, at the end of this one.
    pub(super) fn catenate(&mut self) -> Result<(), Fatal> {
        let archive = self.archive_name();
        if archive == "-" {
            writeln!(
                self.context.stderr(),
                "tar: Options '-Aru' are incompatible with '-f -'\n{TRY}"
            )?;
            self.status = 2;
            return Err(Fatal::Exit);
        }
        let names = self.names()?;
        let (end, _) = self.archive_end(&archive, false)?;
        let path = self.path(&archive);
        let mut file = match fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
        {
            Ok(file) => file,
            Err(e) => return Err(self.fatal(&format!("{archive}: Cannot open: {}", strerror(&e)))),
        };
        file.set_len(end * BLOCK as u64)?;
        file.seek(SeekFrom::Start(end * BLOCK as u64))?;
        let mut writer = Writer::new(io::BufWriter::new(file), self.write_options_for_edit());
        writer.start_at(end);
        for (name, base) in names {
            let other = base.join(&name);
            let input = match fs::File::open(&other) {
                Ok(file) => file,
                Err(e) => {
                    let shown = self.quote_colon(name.as_bytes());
                    self.error(&format!("{shown}: Cannot open: {}", strerror(&e)))?;
                    continue;
                }
            };
            let mut input = Lookahead::new(input);
            if Codec::sniff(input.peek(512)?).is_some() {
                return Err(self.fatal("Cannot concatenate compressed archives"));
            }
            let mut reader = Reader::new(input, self.options.ignore_zeros);
            self.copy_members(&mut reader, &mut writer, |_| false)?;
        }
        let blocks = writer.blocks();
        writer.finish()?;
        self.bytes = blocks * BLOCK as u64;
        self.totals(true)
    }

    /// `--delete`: the archive again without the members the names select.
    pub(super) fn delete(&mut self) -> Result<(), Fatal> {
        let archive = self.archive_name();
        let names = self.names()?;
        let mut wanted: Vec<Wanted> = names
            .into_iter()
            .map(|(name, base)| Wanted {
                pattern: name.into_bytes(),
                base,
                found: 0,
            })
            .collect();
        let flags = Self::member_flags(self.options.matching);
        let occurrence = self.options.occurrence;
        let input: Box<dyn io::Read> = if archive == "-" {
            Box::new(self.context.stdin())
        } else {
            match fs::File::open(self.path(&archive)) {
                Ok(file) => Box::new(file),
                Err(e) => {
                    return Err(self.fatal(&format!("{archive}: Cannot open: {}", strerror(&e))));
                }
            }
        };
        let mut input = Lookahead::new(input);
        if self.options.codec.is_some() || Codec::sniff(input.peek(512)?).is_some() {
            return Err(self.fatal("Cannot update compressed archives"));
        }
        let mut reader = Reader::new(input, self.options.ignore_zeros);
        let mut output = Vec::new();
        {
            let mut writer = Writer::new(&mut output, self.write_options_for_edit());
            let mut skip = |name: &[u8]| {
                for want in &mut wanted {
                    if select::matches(&want.pattern, name, flags) {
                        want.found += 1;
                        return occurrence.is_none_or(|n| want.found == n);
                    }
                }
                false
            };
            self.copy_members(&mut reader, &mut writer, &mut skip)?;
            writer.finish()?;
        }
        if archive == "-" {
            let mut stdout = self.context.stdout();
            stdout.write_all(&output)?;
            stdout.flush()?;
        } else {
            let mut replacement = match Replacement::create(self.path(&archive)) {
                Ok(replacement) => replacement,
                Err(e) => {
                    return Err(self.fatal(&format!("{archive}: Cannot open: {}", strerror(&e))));
                }
            };
            replacement.write_all(&output)?;
            if let Err(e) = replacement.finish(fs::FileTimes::new(), false, false) {
                return Err(self.fatal(&format!("{archive}: Cannot write: {}", strerror(&e))));
            }
        }
        for want in &wanted {
            if want.found == 0 {
                let name = self.quote_colon(&want.pattern);
                self.error(&format!("{name}: Not found in archive"))?;
            }
        }
        Ok(())
    }

    fn write_options_for_edit(&self) -> cash_archive::tar::write::Options {
        cash_archive::tar::write::Options {
            format: self
                .options
                .format
                .unwrap_or(cash_archive::tar::Format::Gnu),
            numeric_owner: self.options.numeric_owner,
            record_blocks: self.options.blocking,
        }
    }
}
