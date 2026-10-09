//! Finding and opening archives as rar does: `.rar` added to a name without an
//! extension, wildcards matched in the archive's folder, a volume's followers by
//! their names, and an encrypted header's password asked for.

use std::io;
use std::path::{Path, PathBuf};

use cash_archive::rar::{self, ArchiveReadOptions, ArchiveReader, ErrorKind};

use super::item::{self, Facts, Item};
use super::{Rar, Stop, code};

/// An archive found: as shown, and where it is.
#[derive(Clone, Debug)]
pub(super) struct Found {
    pub(super) display: String,
    pub(super) path: PathBuf,
}

/// The archives a command's archive name means: the name itself, or what its wildcard
/// matches in its folder (and below, with `-r`).
pub(super) fn find<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>, name: &str) -> Vec<Found> {
    let name = super::cmdline::with_default_extension(name);
    let display = name.replace('\\', "/");
    if !has_wildcard(&display) {
        let path = rar.path(&display);
        return vec![Found { display, path }];
    }
    let (folder, mask) = match display.rfind('/') {
        Some(at) => (
            display.get(..=at).unwrap_or_default().to_owned(),
            display.get(at + 1..).unwrap_or_default().to_owned(),
        ),
        None => (String::new(), display.clone()),
    };
    let mut found = Vec::new();
    collect(
        rar,
        &folder,
        &mask,
        rar.switches.recurse == Some('r'),
        &mut found,
    );
    found
}

fn collect<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    folder: &str,
    mask: &str,
    recurse: bool,
    found: &mut Vec<Found>,
) {
    let base = rar.path(if folder.is_empty() { "." } else { folder });
    let Ok(entries) = std::fs::read_dir(&base) else {
        return;
    };
    let mut names: Vec<(String, bool)> = entries
        .filter_map(Result::ok)
        .map(|entry| {
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            (entry.file_name().to_string_lossy().into_owned(), is_dir)
        })
        .collect();
    names.sort_by_key(|(name, _)| name.to_lowercase());
    for (name, is_dir) in &names {
        if !is_dir && mask_matches(mask, name) {
            let display = format!("{folder}{name}");
            found.push(Found {
                path: base.join(name),
                display,
            });
        }
    }
    if recurse {
        for (name, is_dir) in &names {
            if *is_dir {
                collect(rar, &format!("{folder}{name}/"), mask, recurse, found);
            }
        }
    }
}

/// Whether a name has `*` or `?`.
pub(super) fn has_wildcard(name: &str) -> bool {
    name.contains(['*', '?'])
}

/// Windows' wildcard match, case aside: `*` any run, `?` one character; `*.*` all.
pub(super) fn mask_matches(mask: &str, name: &str) -> bool {
    if mask == "*" || mask == "*.*" {
        return true;
    }
    let mask: Vec<char> = mask.to_lowercase().chars().collect();
    let name: Vec<char> = name.to_lowercase().chars().collect();
    let (mut m, mut n) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while n < name.len() {
        match mask.get(m) {
            Some('*') => {
                star = Some(m);
                m += 1;
                mark = n;
            }
            Some(&c) if c == '?' || c == name[n] => {
                m += 1;
                n += 1;
            }
            _ => match star {
                Some(at) => {
                    m = at + 1;
                    mark += 1;
                    n = mark;
                }
                None => return false,
            },
        }
    }
    while mask.get(m) == Some(&'*') {
        m += 1;
    }
    // A mask ending in `.` or `.*` also takes a name without an extension.
    let rest: String = mask.get(m..).unwrap_or_default().iter().collect();
    m == mask.len() || rest == "." || rest == ".*"
}

/// An archive opened: its facts and its items.
pub(super) struct Opened {
    pub(super) archive: rar::Archive,
    pub(super) facts: Facts,
    pub(super) items: Vec<Item>,
    /// Damage after the headers read: whether the archive ended early ("Unexpected end
    /// of archive") or a header is wrong ("Corrupt header is found").
    pub(super) damage: Option<Damage>,
}

/// How a damaged archive's headers stopped reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Damage {
    Truncated,
    Corrupt,
}

impl Damage {
    /// rar's words for it.
    pub(super) const fn words(self) -> &'static str {
        match self {
            Self::Truncated => "Unexpected end of archive",
            Self::Corrupt => "Corrupt header is found",
        }
    }
}

/// Why an archive did not open, as rar words it.
pub(super) enum Failure {
    /// "Cannot open", with Windows' words.
    Missing(io::Error),
    /// "is not RAR archive".
    NotRar,
    /// A wrong password for encrypted headers: the facts known without it.
    WrongPassword(Facts),
    /// Headers that do not read: the main header's facts, and whether the archive
    /// ended early ("Unexpected end of archive") or a header is wrong ("Corrupt header
    /// is found").
    Corrupt { facts: Facts, truncated: bool },
}

/// Opens `found`, asking for the password its encrypted headers need.
pub(super) fn open<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    found: &Found,
) -> Result<Result<Opened, Failure>, Stop> {
    let given = rar.given_password()?;
    match try_open(rar, found, given.as_deref()) {
        Err(Failure::WrongPassword(_)) if given.is_none() => {
            let password = rar.ask_password(Some(&found.display))?;
            *rar.password.borrow_mut() = Some(password.clone());
            Ok(try_open(rar, found, Some(&password)))
        }
        other => Ok(other),
    }
}

fn try_open<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    found: &Found,
    password: Option<&str>,
) -> Result<Opened, Failure> {
    let metadata = std::fs::metadata(&found.path).map_err(Failure::Missing)?;
    if metadata.is_dir() {
        return Err(Failure::NotRar);
    }
    // The password opens encrypted headers; a file's encryption is the extraction's.
    let main = main_facts(&found.path);
    let password = password.filter(|_| main.encrypted_headers);
    let options = match password {
        Some(password) => ArchiveReadOptions::with_password(password.as_bytes()),
        None => ArchiveReadOptions::new(),
    }
    .with_lenient(true);
    let archive = match ArchiveReader::read_path_with_options(&found.path, options) {
        Ok(archive) => archive,
        Err(error) => return Err(failure(&found.path, &error)),
    };
    let (facts, items) = describe(rar, &found.path, &archive);
    let damage = match archive.damage() {
        None => None,
        Some(error) => {
            if items.is_empty() {
                if let Failure::WrongPassword(facts) = failure(&found.path, error) {
                    return Err(Failure::WrongPassword(facts));
                }
            }
            Some(damage_of(error))
        }
    };
    Ok(Opened {
        archive,
        facts,
        items,
        damage,
    })
}

/// What a failed read means to rar.
fn failure(path: &Path, error: &rar::Error) -> Failure {
    let facts = main_facts(path);
    match error.kind() {
        ErrorKind::UnsupportedFormat => Failure::NotRar,
        ErrorKind::PasswordRequired | ErrorKind::BadPassword => Failure::WrongPassword(facts),
        ErrorKind::Io if std::fs::File::open(path).is_err() => Failure::Missing(
            std::fs::File::open(path)
                .err()
                .unwrap_or_else(|| io::Error::other("")),
        ),
        _ => {
            let truncated =
                matches!(error.root_cause(), rar::Error::TooShort) || error.kind() == ErrorKind::Io;
            // Encrypted RAR 4 headers that do not check: the password is wrong.
            if facts.encrypted_headers && facts.format == "RAR 1.5" && !truncated {
                return Failure::WrongPassword(facts);
            }
            Failure::Corrupt { facts, truncated }
        }
    }
}

/// The damage an error that ended a lenient read means.
fn damage_of(error: &rar::Error) -> Damage {
    if matches!(error.root_cause(), rar::Error::TooShort) || error.kind() == ErrorKind::Io {
        Damage::Truncated
    } else {
        Damage::Corrupt
    }
}

/// An unsigned RAR 5 number: seven bits a byte, the last without its top bit.
fn vint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// The facts a file's main header gives, for an archive whose other headers do not
/// read: its format and flags; RAR 5's are hidden when its headers are encrypted.
fn main_facts(path: &Path) -> Facts {
    let Some(bytes) = read_at(path, 0, 64) else {
        return Facts {
            format: "RAR 5",
            ..Facts::default()
        };
    };
    if bytes.starts_with(b"Rar!\x1a\x07\x01\x00") {
        let mut facts = Facts {
            format: "RAR 5",
            ..Facts::default()
        };
        // After the CRC, the header's size, type and flags.
        let mut at = 12;
        let mut header = || -> Option<(u64, u64, u64)> {
            let _size = vint(&bytes, &mut at)?;
            let kind = vint(&bytes, &mut at)?;
            let flags = vint(&bytes, &mut at)?;
            if flags & 0x0001 != 0 {
                vint(&bytes, &mut at)?;
            }
            let archive_flags = if kind == 1 { vint(&bytes, &mut at)? } else { 0 };
            Some((kind, flags, archive_flags))
        };
        if let Some((kind, _, archive_flags)) = header() {
            if kind == 4 {
                facts.encrypted_headers = true;
            } else if kind == 1 {
                facts.volume = archive_flags & 0x0001 != 0;
                facts.solid = archive_flags & 0x0004 != 0;
                facts.recovery = archive_flags & 0x0008 != 0;
                facts.locked = archive_flags & 0x0010 != 0;
            }
        }
        facts
    } else if bytes.starts_with(b"Rar!\x1a\x07\x00") {
        let flags = bytes
            .get(10..12)
            .map_or(0, |pair| u16::from_le_bytes([pair[0], pair[1]]));
        Facts {
            format: "RAR 1.5",
            volume: flags & 0x0001 != 0,
            solid: flags & 0x0008 != 0,
            locked: flags & 0x0004 != 0,
            recovery: flags & 0x0040 != 0,
            encrypted_headers: flags & 0x0080 != 0,
            ..Facts::default()
        }
    } else if bytes.starts_with(b"RE~^") {
        Facts {
            format: "RAR 1.4",
            ..Facts::default()
        }
    } else {
        Facts {
            format: "RAR 5",
            ..Facts::default()
        }
    }
}

/// `len` bytes of a file at `offset`.
pub(super) fn read_at(path: &Path, offset: u64, len: usize) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = Vec::with_capacity(len);
    file.take(len as u64).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// An archive's facts and items, as rar lists them.
fn describe<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    path: &Path,
    archive: &rar::Archive,
) -> (Facts, Vec<Item>) {
    let (mut facts, items) = if let Some(archive) = archive.as_rar50() {
        (
            item::rar5_facts(archive, &rar.zone),
            item::rar5_items(archive, &rar.zone),
        )
    } else if let Some(archive) = archive.as_rar15_40() {
        (
            item::rar4_facts(archive, |offset, len| read_at(path, offset, len)),
            item::rar4_items(archive),
        )
    } else if let Some(archive) = archive.as_rar13() {
        (item::rar13_facts(archive), item::rar13_items(archive))
    } else {
        (Facts::default(), Vec::new())
    };
    // A volume is the first when its first file does not go on from another, whatever
    // its flag says, as rar judges it: RAR before 3.0 flags none, and a flag can lie.
    if facts.volume
        && let Some(first) = items.iter().find(|item| item.is_file_like())
    {
        facts.first_volume = !first.split_before;
    }
    (facts, items)
}

/// Words a failure, setting its code: on the messages' stream what rar puts there, on
/// standard error the rest.
pub(super) fn report<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    found: &Found,
    failure: &Failure,
    zero_files: bool,
) {
    let head = |facts: &Facts| {
        format!(
            "\nArchive: {}\nDetails: {}\n{}\n",
            found.display,
            facts.details(),
            if zero_files { "  0 files\n" } else { "" }
        )
    };
    match failure {
        Failure::Missing(error) => {
            rar.console.msg("\n");
            report_missing(rar, found, error);
        }
        Failure::NotRar => {
            rar.console
                .msg(&format!("\n{} is not RAR archive\n", found.display));
        }
        Failure::WrongPassword(facts) => {
            rar.console.msg(&head(facts));
            if facts.format == "RAR 5" {
                rar.console
                    .err(&format!("\nIncorrect password for {}", found.display));
                rar.fail(code::PASSWORD);
            } else {
                rar.console.err(&format!(
                    "\nChecksum error in the encrypted file {}. Corrupt file or wrong password.",
                    found.display
                ));
                rar.fail(code::CRC);
            }
        }
        Failure::Corrupt { facts, truncated } => {
            rar.console.msg(&head(facts));
            // rar says it twice: once reading, once at the end.
            if *truncated {
                rar.console
                    .err("\nUnexpected end of archive\nUnexpected end of archive");
                rar.fail(code::WARNING);
            } else {
                rar.console
                    .err("\nCorrupt header is found\nCorrupt header is found");
                rar.fail(code::CRC);
            }
        }
    }
}

/// "Cannot open" an archive that is not there, with Windows' words; a bare listing has
/// no blank line before it, the others do.
pub(super) fn report_missing<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    found: &Found,
    error: &io::Error,
) {
    rar.console.err(&format!(
        "\nCannot open {}\n{}",
        found.display,
        system_message(error)
    ));
    rar.fail(code::NO_FILES);
}

/// Windows' own words for an error, without Rust's "(os error N)".
pub(super) fn system_message(error: &io::Error) -> String {
    let text = error.to_string();
    match text.find(" (os error ") {
        Some(at) => text.get(..at).unwrap_or_default().to_owned(),
        None => text,
    }
}

/// The volume after `path` in its set, by the set's naming: `name.partN.rar`, or
/// `name.rar`, `name.r00`, `name.r01` …
pub(super) fn next_volume(display: &str, new_numbering: bool) -> Option<String> {
    if new_numbering && let Some(next) = next_part_name(display) {
        return Some(next);
    }
    next_old_name(display)
}

/// `name.part01.rar` to `name.part02.rar`, the digits' count kept unless it grows.
fn next_part_name(name: &str) -> Option<String> {
    // The last run of digits before the extension: `x.part1.rar`, `x_volume_0.rar`.
    let stem_end = name.rfind('.')?;
    let stem = name.get(..stem_end)?;
    let digits_end = stem.rfind(|c: char| c.is_ascii_digit())? + 1;
    let digits_start = stem
        .get(..digits_end)?
        .rfind(|c: char| !c.is_ascii_digit())
        .map_or(0, |at| at + 1);
    let digits = name.get(digits_start..digits_end)?;
    let number: u64 = digits.parse().ok()?;
    let next = format!("{:0width$}", number + 1, width = digits.len());
    Some(format!(
        "{}{}{}",
        name.get(..digits_start)?,
        next,
        name.get(digits_end..)?
    ))
}

/// `name.rar` to `name.r00`, `name.r99` to `name.s00`, keeping the extension's case.
fn next_old_name(name: &str) -> Option<String> {
    let dot = name.rfind('.')?;
    let ext: Vec<char> = name.get(dot + 1..)?.chars().collect();
    if ext.len() != 3 {
        return None;
    }
    let lower: String = ext.iter().collect::<String>().to_ascii_lowercase();
    let upper = ext[0].is_ascii_uppercase();
    let next = if lower == "rar" {
        "r00".to_owned()
    } else {
        let digits: u32 = lower.get(1..)?.parse().ok()?;
        if digits == 99 {
            let letter = char::from(u8::try_from(u32::from(ext[0].to_ascii_lowercase()) + 1).ok()?);
            format!("{letter}00")
        } else {
            format!("{}{:02}", ext[0].to_ascii_lowercase(), digits + 1)
        }
    };
    let next = if upper {
        next.to_ascii_uppercase()
    } else {
        next
    };
    Some(format!("{}{}", name.get(..=dot)?, next))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_match_as_windows_does() {
        assert!(mask_matches("*.rar", "A.RAR"));
        assert!(mask_matches("multi*", "multivol.part1.rar"));
        assert!(mask_matches("h?llo.*", "hello.txt"));
        assert!(!mask_matches("*.rar", "a.zip"));
        assert!(mask_matches("*.", "noext"));
        assert!(mask_matches("*.*", "noext"));
    }

    #[test]
    fn volumes_follow_by_their_naming() {
        assert_eq!(
            next_volume("a/x.part1.rar", true).as_deref(),
            Some("a/x.part2.rar")
        );
        assert_eq!(
            next_volume("x.part09.rar", true).as_deref(),
            Some("x.part10.rar")
        );
        assert_eq!(next_volume("x.rar", false).as_deref(), Some("x.r00"));
        assert_eq!(next_volume("X.R00", false).as_deref(), Some("X.R01"));
        assert_eq!(next_volume("x.r99", false).as_deref(), Some("x.s00"));
        assert_eq!(next_volume("x.rar", true).as_deref(), Some("x.r00"));
    }
}
