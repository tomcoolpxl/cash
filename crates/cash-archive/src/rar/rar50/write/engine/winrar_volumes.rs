//! WinRAR 7's volume sets, as `Rar.exe` 7.23 writes them.
//!
//! Each volume is the size asked, counting its headers. While a file goes into a
//! volume, room is kept for the end header and, with quick open on, for a quick-open
//! block holding this file's header and those already cached: the file takes what is
//! left, and what the blocks do not use is zero-filled after the end header. The first
//! volume carries no volume number; with quick open on, every main header carries a
//! locator, and each volume its own quick-open block for the parts in it stored in
//! more than the threshold.

use super::*;
use crate::rar::rar50::write::winrar::{
    COMPRESSION_WIDTH, HFL_SKIP_IF_UNKNOWN, offset_width, size_width,
};

/// A quick-open entry's framing beyond the header it caches, as WinRAR reserves it.
const ENTRY_RESERVE: u64 = 18;

/// The end header: CRC, size, type, flags and end flags, one byte each but the CRC.
const END_LEN: u64 = 8;

struct Volume {
    index: u64,
    body: Spool,
    body_len: u64,
    /// The headers the quick-open block caches, each with its place in the body.
    cached: Vec<(u64, Bytes)>,
}

struct Writer<'a, 'p> {
    volume_size: u64,
    quick_open: bool,
    quick_open_over: Option<u64>,
    checksums: crate::rar::rar50::Checksums,
    solid: bool,
    offset_width: usize,
    progress: Option<ProgressReporter<'p>>,
    resources: &'a WriterResources,
    sink: &'a mut dyn crate::rar::rar50::VolumeSink,
}

pub(super) fn write(
    entries: &[ArchiveEntry],
    members: Records<VolumeMember<'_>>,
    plan: &EnginePlan<'_>,
    volume_size: u64,
    sink: &mut dyn crate::rar::rar50::VolumeSink,
    resources: &WriterResources,
) -> Result<()> {
    let mut writer = Writer {
        volume_size,
        quick_open: plan.quick_open,
        quick_open_over: plan.layout.quick_open_over,
        checksums: plan.layout.checksums,
        solid: plan.compress.solid,
        offset_width: offset_width(entries),
        progress: plan.progress,
        resources,
        sink,
    };
    let mut volume = writer.volume(0)?;
    for mut member in members {
        writer
            .member(&mut volume, &mut member)
            .map_err(|error| member_error(error, member.name, "writing volume member"))?;
    }
    writer.finish(volume, false)
}

impl Writer<'_, '_> {
    fn volume(&self, index: u64) -> Result<Volume> {
        Ok(Volume {
            index,
            body: Spool::create(self.resources)?,
            body_len: 0,
            cached: Vec::new(),
        })
    }

    /// Cuts one member across as many volumes as it takes.
    fn member(&mut self, volume: &mut Volume, member: &mut VolumeMember<'_>) -> Result<()> {
        check_cancelled(self.progress)?;
        // Its header has the same length in every volume: the sizes are padded to
        // the unpacked size's width, whatever part of the data follows.
        let header_len = self.fragment_header(member, 0, false, None)?.len() as u64;
        let mut start = 0u64;
        let mut split_before = false;
        loop {
            let reserve = if self.quick_open {
                let cached: u64 = volume
                    .cached
                    .iter()
                    .map(|(_, header)| header.len() as u64 + ENTRY_RESERVE)
                    .sum();
                let entries = cached + header_len + ENTRY_RESERVE;
                self.quick_open_header_len(entries)? + entries
            } else {
                0
            };
            let used = RAR50_SIGNATURE.len() as u64
                + self.main_header_len(volume.index)?
                + volume.body_len
                + header_len
                + reserve
                + END_LEN;
            let remaining = member.payload_len - start;
            let room = self.volume_size.saturating_sub(used);
            if used > self.volume_size || (room == 0 && remaining > 0) {
                if volume.body_len == 0 {
                    return Err(Error::InvalidArgument(
                        "RAR 5 volume size is too small for a header",
                    ));
                }
                self.next_volume(volume)?;
                continue;
            }
            let fragment_len = remaining.min(room);
            let split_after = fragment_len < remaining;
            let fragment = if split_after {
                Some(
                    member
                        .source
                        .checksums_range(start, fragment_len, self.progress)?,
                )
            } else {
                None
            };
            let header = self.fragment_header(member, fragment_len, split_before, fragment)?;
            debug_assert_eq!(header.len() as u64, header_len);
            if self.quick_open && self.quick_open_over.is_none_or(|over| fragment_len > over) {
                let mut copy = Bytes::new(self.resources);
                copy.extend_from_slice(&header)?;
                volume.cached.push((volume.body_len, copy));
            }
            volume.body.write_all(&header)?;
            member.source.emit_range(
                start,
                fragment_len,
                &mut CancellableIo {
                    inner: &mut volume.body,
                    progress: self.progress,
                },
                fragment,
                self.resources,
            )?;
            volume.body_len += header_len + fragment_len;
            start += fragment_len;
            split_before = true;
            if start >= member.payload_len {
                return Ok(());
            }
            self.next_volume(volume)?;
        }
    }

    fn next_volume(&mut self, volume: &mut Volume) -> Result<()> {
        let next = self.volume(volume.index + 1)?;
        let full = std::mem::replace(volume, next);
        self.finish(full, true)
    }

    fn archive_flags(&self, index: u64) -> u64 {
        let mut flags = crate::rar::rar50::MHFL_VOLUME;
        if index > 0 {
            flags |= crate::rar::rar50::MHFL_VOLUME_NUMBER;
        }
        if self.solid {
            flags |= MHFL_SOLID;
        }
        flags
    }

    fn main_header(&self, index: u64, quick_open_offset: u64) -> Result<Bytes> {
        let mut extra = Bytes::new(self.resources);
        if self.quick_open {
            super::super::headers::write_locator_record(
                &mut extra,
                Some(quick_open_offset),
                None,
                self.offset_width,
            )?;
        }
        let mut main = Bytes::new(self.resources);
        write_main_header_with(
            &mut main,
            HFL_SKIP_IF_UNKNOWN,
            self.archive_flags(index),
            (index > 0).then_some(index),
            &extra,
            self.resources,
        )?;
        Ok(main)
    }

    fn main_header_len(&self, index: u64) -> Result<u64> {
        Ok(self.main_header(index, 0)?.len() as u64)
    }

    fn quick_open_header_len(&self, payload_len: u64) -> Result<u64> {
        let parts = super::super::headers::service_parts(
            b"QO",
            payload_len,
            0,
            None,
            true,
            true,
            self.resources,
        )?;
        Ok(super::super::headers::block_header_image_padded(
            HEAD_SERVICE,
            parts.flags,
            Some(payload_len),
            parts.data_width,
            &parts.specific,
            &parts.extra,
            self.resources,
        )?
        .len() as u64)
    }

    /// One fragment's header in WinRAR's framing; see [`fragment_header`] for what
    /// the checksums of a fragment that is not the last one hold.
    fn fragment_header(
        &self,
        member: &VolumeMember<'_>,
        fragment_len: u64,
        split_before: bool,
        fragment: Option<FragmentChecksums>,
    ) -> Result<Bytes> {
        let resources = self.resources;
        let mut extra = Bytes::new(resources);
        if let Some((salt, iv, check_value)) = member.encryption {
            write_file_encryption_record(&mut extra, salt, iv, check_value)?;
        }
        let checked = !member.is_directory;
        if checked && self.checksums.blake2() {
            write_hash_record_with_value(&mut extra, fragment.map_or(member.hash, |f| f.hash))?;
        }
        super::super::headers::write_mtime_record(
            &mut extra,
            member.mtime,
            member.mtime_nanoseconds,
        )?;
        if let Some(times) = member.file_times {
            write_extra_record(
                &mut extra,
                super::super::super::FHEXTRA_HTIME,
                &times.encode()?,
            )?;
        }
        let width = if member.is_directory {
            0
        } else {
            size_width(member.unpacked_size)
        };
        let specific = super::super::headers::file_fields(
            &super::super::headers::FileFields {
                name: member.name,
                unpacked_size: member.unpacked_size,
                size_width: width,
                crc32: (checked && self.checksums.crc32())
                    .then(|| fragment.map_or(member.crc32, |f| f.crc32)),
                attributes: member.attributes,
                mtime: member.mtime.filter(|_| member.mtime_nanoseconds.is_none()),
                compression_info: member.compression_info,
                compression_width: COMPRESSION_WIDTH,
                host_os: member.host_os,
                is_directory: member.is_directory,
            },
            resources,
        )?;
        let mut flags = HFL_DATA;
        if !extra.is_empty() {
            flags |= HFL_EXTRA;
        }
        if split_before {
            flags |= crate::rar::rar50::HFL_SPLIT_BEFORE;
        }
        if fragment.is_some() {
            flags |= crate::rar::rar50::HFL_SPLIT_AFTER;
        }
        super::super::headers::block_header_image_padded(
            HEAD_FILE,
            flags,
            Some(fragment_len),
            width,
            &specific,
            &extra,
            resources,
        )
    }

    /// Writes a volume out: signature, main header, its blocks, its quick-open
    /// block, the end header, and the zeros that make up its size when more follow.
    fn finish(&mut self, mut volume: Volume, more_volumes_follow: bool) -> Result<()> {
        check_cancelled(self.progress)?;
        let main_len = self.main_header_len(volume.index)?;
        let quick_open = if volume.cached.is_empty() {
            None
        } else {
            let mut payload = Vec::new();
            let mut checksum = crate::rar::crc32::Crc32::new();
            for (offset, header) in &volume.cached {
                append_quick_open_entry(
                    &mut payload,
                    &mut checksum,
                    volume.body_len - offset,
                    header,
                )?;
            }
            let header = service_header(
                b"QO",
                payload.len() as u64,
                checksum.finish(),
                None,
                true,
                true,
                None,
                self.resources,
            )?;
            Some((header, payload))
        };
        // Offsets count from the end of the signature.
        let quick_open_offset = if quick_open.is_some() {
            main_len + volume.body_len
        } else {
            0
        };
        let main = self.main_header(volume.index, quick_open_offset)?;

        let end_flags = if more_volumes_follow {
            crate::rar::rar50::EFL_NEXT_VOLUME
        } else {
            0
        };
        let mut end = Bytes::new(self.resources);
        write_end_header_with(&mut end, HFL_SKIP_IF_UNKNOWN, end_flags, self.resources)?;

        let raw_output = self.sink.start_volume(volume.index)?;
        let mut output = CancellableIo {
            inner: raw_output,
            progress: self.progress,
        };
        output.write_all(RAR50_SIGNATURE)?;
        output.write_all(&main)?;
        volume.body.rewind()?;
        std::io::copy(&mut volume.body, &mut output)?;
        let mut written = RAR50_SIGNATURE.len() as u64 + main.len() as u64 + volume.body_len;
        if let Some((header, payload)) = &quick_open {
            output.write_all(header)?;
            output.write_all(payload)?;
            written += header.len() as u64 + payload.len() as u64;
        }
        output.write_all(&end)?;
        written += end.len() as u64;
        if more_volumes_follow && written < self.volume_size {
            let zeros = vec![0u8; usize::try_from(self.volume_size - written).unwrap_or(0)];
            output.write_all(&zeros)?;
            written = self.volume_size;
        }
        output.flush()?;
        drop(output);
        self.sink.finish_volume(volume.index, written)
    }
}
