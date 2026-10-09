//! `rc`: a volume set's missing volumes rebuilt from its recovery volumes, as rar does
//! it. RAR 5's `.rev` files hold each volume's size and CRC32, so a damaged volume is
//! found too, renamed `.bad` and rebuilt; RAR 3's hold neither, and only missing volumes
//! are rebuilt. RAR 3's come named two ways: `NAME.partN.rev`, each ending in its
//! numbers and a CRC32, and the older `NAME.partLAST_COUNT_N.rev`.

use std::path::{Path, PathBuf};

use cash_archive::rar::rar15_40::repair_rev3_volumes_to;
use cash_archive::rar::rar50::{Rev5Volume, repair_rev5_volumes_to};

use super::cmdline::Parsed;
use super::open::{self, Found};
use super::repair::{done, write};
use super::{Rar, code};

pub(super) fn run<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>, parsed: &Parsed) {
    let Some(archive) = parsed.archive.as_deref() else {
        return;
    };
    for found in open::find(rar, archive) {
        reconstruct(rar, &found);
    }
}

/// A volume set's names: its base, a number as wide as the one named, `.rar`.
struct Set {
    /// Up to the number, as shown, its folder and all.
    base: String,
    width: usize,
    /// Where the set is, and its base's file name, for the recovery volumes beside it.
    folder: PathBuf,
    file_base: String,
}

impl Set {
    fn of(found: &Found) -> Option<Self> {
        let stem = found.display.get(..found.display.rfind('.')?)?;
        let width = stem.len() - stem.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        if width == 0 {
            return None;
        }
        let base = stem.get(..stem.len() - width)?.to_owned();
        let file_base = base.rsplit('/').next().unwrap_or(&base).to_lowercase();
        let folder = found
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        Some(Self {
            base,
            width,
            folder,
            file_base,
        })
    }

    fn volume(&self, number: usize) -> String {
        format!("{}{number:0width$}.rar", self.base, width = self.width)
    }

    /// A file of the set's folder, as shown.
    fn shown(&self, file: &str) -> String {
        match self.base.rfind('/') {
            Some(at) => format!("{}{file}", self.base.get(..=at).unwrap_or_default()),
            None => file.to_owned(),
        }
    }

    /// The recovery volumes beside the set, in their order.
    fn revs(&self) -> Vec<Rev> {
        let mut revs: Vec<Rev> = std::fs::read_dir(&self.folder)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let file = entry.file_name().to_string_lossy().into_owned();
                let lower = file.to_lowercase();
                let middle = lower.strip_prefix(&self.file_base)?.strip_suffix(".rev")?;
                let numbers: Vec<usize> = middle
                    .split('_')
                    .map(|part| {
                        (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
                            .then(|| part.parse().ok())
                            .flatten()
                    })
                    .collect::<Option<_>>()?;
                let (number, old) = match numbers[..] {
                    [number] => (number, None),
                    [last, count, number] => (number, Some((last, count))),
                    _ => return None,
                };
                let bytes = std::fs::read(entry.path()).ok()?;
                Some(Rev {
                    shown: self.shown(&file),
                    bytes,
                    number,
                    old,
                })
            })
            .collect();
        revs.sort_by_key(|rev| rev.number);
        revs
    }
}

/// A recovery volume found.
struct Rev {
    shown: String,
    bytes: Vec<u8>,
    number: usize,
    /// The older RAR 3 naming's numbers: the last volume's and the recovery volumes'
    /// count.
    old: Option<(usize, usize)>,
}

fn reconstruct<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>, found: &Found) {
    if let Err(error) = std::fs::File::open(&found.path) {
        rar.console.msg("\n");
        open::report_missing(rar, found, &error);
        return;
    }
    let named_rev = found.display.to_lowercase().ends_with(".rev");
    let set = Set::of(found).filter(|_| named_rev || open::main_facts(&found.path).volume);
    let Some(set) = set else {
        rar.console.msg("\n");
        return;
    };
    let revs = set.revs();
    let (new, old): (Vec<Rev>, Vec<Rev>) = revs.into_iter().partition(|rev| rev.old.is_none());
    if new.iter().any(|rev| rev.bytes.starts_with(b"Rar!\x1aRev")) {
        rar5(rar, &set, &new);
    } else if !new.is_empty() {
        rar3_new(rar, &set, &new);
    } else if let Some((last, count)) = old.first().and_then(|rev| rev.old) {
        rar.console
            .msg(&format!("\n{} recovery volumes found", old.len()));
        let recovery = old
            .into_iter()
            .map(|rev| (rev.number.saturating_sub(1), rev.bytes))
            .collect::<Vec<_>>();
        rar3(rar, &set, last, count, &recovery);
    } else {
        rar.console.msg("\n0 recovery volumes found\n");
    }
}

/// RAR 5: every volume checked against the sizes and CRC32s the recovery volumes keep.
fn rar5<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>, set: &Set, revs: &[Rev]) {
    rar.console
        .msg(&format!("\n{} recovery volumes found", revs.len()));
    rar.console.msg("\nCalculating checksums of all volumes.");
    let parsed: Vec<Option<Rev5Volume>> = revs
        .iter()
        .map(|rev| Rev5Volume::parse(&rev.bytes).ok())
        .collect();
    let expected = parsed
        .iter()
        .flatten()
        .next()
        .map(|rev| rev.data_volumes.clone())
        .unwrap_or_default();
    let mut volumes: Vec<Option<Vec<u8>>> = Vec::new();
    let mut bad = Vec::new();
    for (index, expected) in expected.iter().enumerate() {
        let shown = set.volume(index + 1);
        let Ok(bytes) = std::fs::read(rar.path(&shown)) else {
            volumes.push(None);
            continue;
        };
        rar.console.msg(&format!("\n{shown}"));
        if bytes.len() as u64 == expected.file_size && crc32fast::hash(&bytes) == expected.crc32 {
            volumes.push(Some(bytes));
        } else {
            rar.console.msg(&format!("\n{shown:<20} - checksum error"));
            bad.push(index);
            volumes.push(None);
        }
    }
    for (rev, parsed) in revs.iter().zip(&parsed) {
        rar.console.msg(&format!("\n{}", rev.shown));
        if parsed.is_none() {
            rar.console
                .msg(&format!("\n{:<20} - checksum error", rev.shown));
        }
    }
    let good: Vec<Rev5Volume> = parsed.into_iter().flatten().collect();
    if good.is_empty() {
        rar.console.msg("\nReconstruction impossible\n");
        return;
    }
    let missing = volumes.iter().filter(|volume| volume.is_none()).count();
    if !begin(rar, missing, good.len()) {
        return;
    }
    let slices: Vec<Option<&[u8]>> = volumes.iter().map(Option::as_deref).collect();
    let result = repair_rev5_volumes_to(&slices, &good, |index, bytes| {
        if slices.get(index).is_some_and(Option::is_some) {
            return Ok(());
        }
        let shown = set.volume(index + 1);
        if bad.contains(&index) {
            rar.console.msg(&format!("\nERROR: Bad archive {shown}\n"));
            rar.console
                .msg(&format!("\nRenaming {shown} to {shown}.bad"));
            let _ = std::fs::rename(rar.path(&shown), rar.path(&format!("{shown}.bad")));
        }
        rar.console.msg(&format!("\nCreating {shown}"));
        write(rar, &shown, bytes);
        Ok(())
    });
    finish(rar, result);
}

/// RAR 3 under the newer naming: each recovery volume checked by its own CRC32.
fn rar3_new<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>, set: &Set, revs: &[Rev]) {
    rar.console.msg("\nCalculating checksums of all volumes.");
    let mut counts = None;
    let mut recovery = Vec::new();
    for rev in revs {
        rar.console.msg(&format!("\n{}", rev.shown));
        if let Some((volumes, revs, number)) = trailer(&rev.bytes) {
            counts.get_or_insert((volumes, revs));
            let mut bytes = rev.bytes.clone();
            let len = bytes.len();
            bytes[len - 7..].fill(0);
            recovery.push((number, bytes));
        } else {
            rar.console
                .msg(&format!("\n{:<20} - checksum error", rev.shown));
        }
    }
    rar.console
        .msg(&format!("\n{} recovery volumes found", revs.len()));
    match counts {
        Some((volumes, count)) => rar3(rar, set, volumes, count, &recovery),
        None => rar.console.msg("\nReconstruction impossible\n"),
    }
}

/// A RAR 3 recovery volume's last seven bytes, under the newer naming: the count of
/// volumes less one, of recovery volumes less one, its own number from 0, and the CRC32
/// of what comes before that checksum.
fn trailer(bytes: &[u8]) -> Option<(usize, usize, usize)> {
    let len = bytes.len();
    let at = len.checked_sub(7)?;
    let crc = u32::from_le_bytes(bytes.get(len - 4..)?.try_into().ok()?);
    (crc32fast::hash(bytes.get(..len - 4)?) == crc).then(|| {
        (
            usize::from(bytes[at]) + 1,
            usize::from(bytes[at + 1]) + 1,
            usize::from(bytes[at + 2]),
        )
    })
}

/// RAR 3: the volumes missing rebuilt; those there are taken as they are.
fn rar3<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    set: &Set,
    count: usize,
    recovery_count: usize,
    recovery: &[(usize, Vec<u8>)],
) {
    let volumes: Vec<Option<Vec<u8>>> = (1..=count)
        .map(|number| {
            let shown = set.volume(number);
            let bytes = std::fs::read(rar.path(&shown)).ok();
            if bytes.is_some() {
                rar.console.msg(&format!("\n{shown}"));
            } else {
                rar.console.msg(&format!("\nCannot find volume {shown}"));
            }
            bytes
        })
        .collect();
    let missing = volumes.iter().filter(|volume| volume.is_none()).count();
    if !begin(rar, missing, recovery.len()) {
        return;
    }
    let slices: Vec<Option<&[u8]>> = volumes.iter().map(Option::as_deref).collect();
    let recovery: Vec<(usize, &[u8])> = recovery
        .iter()
        .map(|(number, bytes)| (*number, bytes.as_slice()))
        .collect();
    let result = repair_rev3_volumes_to(&slices, recovery_count, &recovery, |index, bytes| {
        if slices.get(index).is_some_and(Option::is_none) {
            write(rar, &set.volume(index + 1), bytes);
        }
        Ok(())
    });
    finish(rar, result);
}

/// "N volumes missing", and whether there is anything to rebuild and enough to rebuild
/// it with: then "Reconstructing...".
fn begin<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>, missing: usize, revs: usize) -> bool {
    rar.console.msg(&format!("\n{missing} volumes missing"));
    if missing == 0 {
        rar.console.msg("\nNothing to reconstruct\n");
        false
    } else if missing > revs {
        rar.console.msg("\nReconstruction impossible\n");
        false
    } else {
        rar.console.msg("\nReconstructing...");
        true
    }
}

/// The end: what rar's progress leaves on the last line, and "Done".
fn finish<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    result: Result<(), cash_archive::rar::Error>,
) {
    match result {
        Ok(()) => {
            rar.console.msg("     ");
            done(rar);
        }
        Err(error) => {
            rar.console.err(&format!("\n{error}"));
            rar.fail(code::CRC);
        }
    }
}
