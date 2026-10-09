//! `l`, `lt`, `lta`, `lb` and `v`, `vt`, `vta`, `vb`: an archive's files in columns, a
//! block of fields each, or bare names; with `-v`, its volumes after it.

use std::fmt::Write as _;

use super::cmdline::{ListForm, Name, Parsed};
use super::item::{Hash, Item, Kind};
use super::open::{self, Failure, Found};
use super::{Rar, Stop};

/// What one listing has counted: files, their sizes and packed sizes.
#[derive(Clone, Copy, Debug, Default)]
struct Totals {
    files: u64,
    size: u64,
    packed: u64,
}

pub(super) fn run<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    verbose: bool,
    form: ListForm,
    parsed: &Parsed,
) -> Result<(), Stop> {
    let masks = Masks::new(rar, parsed)?;
    let Some(archive) = parsed.archive.as_deref() else {
        return Ok(());
    };
    for found in open::find(rar, archive) {
        list_set(rar, &found, verbose, form, &masks)?;
    }
    Ok(())
}

/// One archive, and with `-v` the volumes after it.
fn list_set<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    first: &Found,
    verbose: bool,
    form: ListForm,
    masks: &Masks,
) -> Result<(), Stop> {
    let all_volumes = rar.switches.volumes.is_some();
    let mut found = first.clone();
    let mut grand = Totals::default();
    let mut listed = 0;
    loop {
        let opened = match open::open(rar, &found)? {
            Ok(opened) => opened,
            Err(failure) => {
                match &failure {
                    Failure::Missing(error) if listed == 0 && form == ListForm::Bare => {
                        open::report_missing(rar, &found, error);
                    }
                    Failure::Missing(_) if listed > 0 => {}
                    _ => open::report(rar, &found, &failure, form == ListForm::Plain),
                }
                break;
            }
        };
        let totals = list_one(rar, &found, &opened, verbose, form, masks);
        if let Some(damage) = opened.damage {
            // rar says it twice: once reading, once at the end; once when no block is
            // cut off.
            let words = damage.words();
            if damage == open::Damage::Unended {
                rar.console.err(&format!("\n{words}"));
            } else {
                rar.console.err(&format!("\n{words}\n{words}"));
            }
            rar.fail(if damage == open::Damage::Corrupt {
                super::code::CRC
            } else {
                super::code::WARNING
            });
        }
        grand.files += totals.files;
        grand.size += totals.size;
        grand.packed += totals.packed;
        listed += 1;
        if !all_volumes || !opened.facts.next_volume {
            break;
        }
        let Some(next) = open::next_volume(&found.display, opened.facts.new_numbering) else {
            break;
        };
        found = Found {
            path: rar.path(&next),
            display: next,
        };
    }
    if listed == 0 || form == ListForm::Bare {
        return Ok(());
    }
    if listed > 1 && form == ListForm::Plain {
        let mut text = format!("{:>21}", grand.size);
        if verbose {
            let _ = write!(
                text,
                "{:>10} {}",
                grand.packed,
                ratio(grand.packed, grand.size)
            );
            text.push_str(&" ".repeat(30));
        } else {
            text.push_str(&" ".repeat(20));
        }
        let _ = writeln!(text, "{}", grand.files);
        rar.console.msg(&text);
    } else {
        rar.console.msg("\n");
    }
    Ok(())
}

/// An archive's comment as rar reads it, or the word that it is corrupt.
pub(super) enum Comment {
    Text(String),
    Corrupt,
}

/// An archive's comment: RAR 5's in UTF-8; RAR 3's and 4's in UTF-16 when their header
/// says so, else in Windows' ANSI code page, Latin-1 to cash; each up to its first NUL.
/// The comment RAR 1.5 to 2.9 packed into the main header rar 7.23 calls corrupt.
pub(super) fn comment<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    found: &Found,
    opened: &open::Opened,
) -> Option<Comment> {
    let password = rar.password.borrow().clone();
    let password = password.as_deref().map(str::as_bytes);
    let mut unicode = opened.archive.as_rar50().is_some();
    let mut utf16 = false;
    let mut crc = None;
    if let Some(archive) = opened.archive.as_rar15_40() {
        if archive.main.has_archive_comment() {
            // The comment block after the main header's 13 bytes: its method at 10.
            let at = archive.sfx_offset as u64 + 7 + 13 + 10;
            let method = open::read_at(&found.path, at, 1)?;
            if method.first() != Some(&0x30) {
                return Some(Comment::Corrupt);
            }
        }
        if let Some(sub) = archive
            .new_subs()
            .find(|sub| sub.kind == cash_archive::rar::rar15_40::NewSubKind::ArchiveComment)
        {
            utf16 = sub.file.attr & 1 != 0;
            crc = Some(sub.file.file_crc);
        }
        unicode = false;
    }
    let bytes = opened.archive.comment(password).ok().flatten()?;
    if let Some(crc) = crc
        && crc32fast::hash(&bytes) != crc
    {
        return Some(Comment::Corrupt);
    }
    let text = if utf16 {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect();
        String::from_utf16_lossy(&units)
    } else if unicode {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        bytes.iter().map(|&b| char::from(b)).collect()
    };
    let end = text.find('\0').unwrap_or(text.len());
    Some(Comment::Text(
        text.get(..end).unwrap_or_default().to_owned(),
    ))
}

/// One volume's listing; its totals, for `-v`'s.
fn list_one<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    found: &Found,
    opened: &open::Opened,
    verbose: bool,
    form: ListForm,
    masks: &Masks,
) -> Totals {
    let mut totals = Totals::default();
    let items: Vec<&Item> = opened
        .items
        .iter()
        .filter(|item| {
            (form == ListForm::TechnicalAll || item.is_file_like()) && masks.wants(&item.name)
        })
        .collect();
    rar.log_archive(&found.display);
    for item in &items {
        rar.log_file(&item.name);
    }
    if form == ListForm::Bare {
        let mut text = String::new();
        for item in &items {
            let _ = writeln!(text, "{}", item.name);
        }
        rar.console.msg(&text);
        return totals;
    }
    let mut text = String::new();
    let mut corrupt_comment = false;
    if !rar.switches.no_comments {
        match comment(rar, found, opened) {
            Some(Comment::Text(comment)) => {
                let _ = writeln!(
                    text,
                    "\nArchive comment:\n{}",
                    comment.replace("\r\n", "\n")
                );
            }
            Some(Comment::Corrupt) => corrupt_comment = true,
            None => {}
        }
    }
    let _ = writeln!(
        text,
        "\nArchive: {}\nDetails: {}",
        found.display,
        opened.facts.details()
    );
    if let Some(name) = &opened.facts.original_name {
        let _ = writeln!(text, "Original name: {name}");
    }
    if let Some(time) = opened.facts.original_time {
        let time = if form == ListForm::Plain {
            time.short()
        } else {
            time.long()
        };
        let _ = writeln!(text, "Original time: {time}");
    }
    let comment_check = |rar: &Rar<'_, SE>| {
        if corrupt_comment {
            rar.console.err("\nThe archive comment is corrupt");
            rar.fail(super::code::CRC);
        }
    };
    if items.is_empty() {
        if form == ListForm::Plain {
            text.push_str("  0 files\n");
        }
        rar.console.msg(&text);
        comment_check(rar);
        return totals;
    }
    match form {
        ListForm::Plain => totals = table(&mut text, &items, opened, verbose),
        ListForm::Technical | ListForm::TechnicalAll => {
            for item in &items {
                technical(&mut text, item);
                if !item.split_before && item.is_file_like() {
                    totals.files += 1;
                    totals.size += item.size;
                }
                totals.packed += item.packed;
            }
            if form == ListForm::TechnicalAll && opened.facts.end {
                text.push_str("\n     Service: EOF\n");
                if let Some(number) = opened.facts.end_volume {
                    let _ = writeln!(text, "       Flags: volume {}", number + 1);
                }
            }
        }
        ListForm::Bare => {}
    }
    rar.console.msg(&text);
    comment_check(rar);
    totals
}

/// `l`'s and `v`'s columns: a row an item, a total of what is not a part carried over.
fn table(text: &mut String, items: &[&Item], opened: &open::Opened, verbose: bool) -> Totals {
    let mut totals = Totals::default();
    text.push('\n');
    text.push_str(if verbose { VERBOSE_HEAD } else { PLAIN_HEAD });
    for item in items {
        row(text, item, verbose);
        if !item.split_before {
            totals.files += 1;
            totals.size += item.size;
        }
        totals.packed += item.packed;
    }
    text.push_str(if verbose { VERBOSE_RULE } else { PLAIN_RULE });
    let _ = write!(text, "{:>22}", totals.size);
    if verbose {
        let _ = write!(
            text,
            " {:>10} {}",
            totals.packed,
            ratio(totals.packed, totals.size)
        );
    }
    // A RAR 4 volume's number, from its end, in the date's column.
    let volume = opened
        .facts
        .end_volume
        .map(|number| format!("volume {}", number + 1))
        .unwrap_or_default();
    if verbose {
        let _ = write!(text, "  {volume:<28}");
    } else {
        let _ = write!(text, "  {volume:<18}");
    }
    let _ = writeln!(text, "{}", totals.files);
    totals
}

const PLAIN_HEAD: &str = " Attributes       Size     Date    Time   Name\n\
----------- ----------  ---------- -----  ----\n";
const PLAIN_RULE: &str = "----------- ----------  ---------- -----  ----\n";
const VERBOSE_HEAD: &str = " Attributes       Size     Packed Ratio    Date    Time   Checksum  Name\n\
----------- ---------- ---------- ----- ---------- -----  --------  ----\n";
const VERBOSE_RULE: &str =
    "----------- ---------- ---------- ----- ---------- -----  --------  ----\n";

/// A ratio's column: the packed size as a whole percentage of the size, rounded down.
fn ratio(packed: u64, size: u64) -> String {
    let percent = if size == 0 {
        0
    } else {
        u128::from(packed) * 100 / u128::from(size)
    };
    format!("{percent:>3}%")
}

/// An item's ratio, or where a split file's part stands.
fn item_ratio(item: &Item) -> String {
    match (item.split_before, item.split_after) {
        (true, true) => "<->".to_owned(),
        (false, true) => "-->".to_owned(),
        (true, false) => "<--".to_owned(),
        (false, false) => {
            let percent = if item.size == 0 {
                0
            } else {
                u128::from(item.packed) * 100 / u128::from(item.size)
            };
            format!("{percent}%")
        }
    }
}

/// A checksum's column: CRC32 in hexadecimal, BLAKE2's first two and last bytes.
fn checksum_short(item: &Item) -> String {
    match &item.hash {
        Hash::Crc32(crc) => format!("{crc:08X}"),
        Hash::Blake2(bytes) if bytes.len() >= 3 => format!(
            "{:02x}{:02x}..{:02x}",
            bytes[0],
            bytes[1],
            bytes[bytes.len() - 1]
        ),
        // RAR 1.3 keeps no checksum rar shows: a file's is unknown, a folder has none.
        _ if item.kind == Kind::Directory => " ".repeat(8),
        _ => "????????".to_owned(),
    }
}

fn row(text: &mut String, item: &Item, verbose: bool) {
    let mark = if item.encrypted { '*' } else { ' ' };
    let (date, time) = item.modified.map_or_else(
        || ("????-??-??".to_owned(), "??:??".to_owned()),
        |stamp| {
            let short = stamp.short();
            let (date, time) = short.split_at(10);
            (date.to_owned(), time.trim_start().to_owned())
        },
    );
    let _ = write!(text, "{mark}{:>10} {:>10} ", item.attributes, item.size);
    if verbose {
        let _ = write!(
            text,
            "{:>10} {:>4}  {date} {time}  {}  ",
            item.packed,
            item_ratio(item),
            checksum_short(item)
        );
    } else {
        let _ = write!(text, " {date} {time}  ");
    }
    let _ = writeln!(text, "{}", item.name);
}

/// A label's line in a technical listing: right-aligned to the colon.
fn field(text: &mut String, label: &str, value: &str) {
    let _ = writeln!(text, "{label:>12}: {value}");
}

fn technical(text: &mut String, item: &Item) {
    text.push('\n');
    field(text, "Name", &item.name);
    field(text, "Type", item.kind.word());
    if let Some(target) = &item.target {
        field(text, "Target", target);
    }
    if !item.folder {
        field(text, "Size", &item.size.to_string());
        field(text, "Packed size", &item.packed.to_string());
        field(text, "Ratio", &item_ratio(item));
    }
    if let Some(percent) = item.recovery_percent {
        field(text, "RR%", &format!("{percent}%"));
    }
    if let Some(stamp) = item.modified {
        field(text, "Modified", &stamp.long());
    }
    if let Some(stamp) = item.created {
        field(text, "Created", &stamp.long());
    }
    if let Some(stamp) = item.accessed {
        field(text, "Accessed", &stamp.long());
    }
    field(text, "Attributes", &item.attributes);
    let pack = if item.split_after { "Pack-" } else { "" };
    let mac = if item.mac { " MAC" } else { "" };
    match &item.hash {
        Hash::Crc32(crc) => field(text, &format!("{pack}CRC32{mac}"), &format!("{crc:08X}")),
        Hash::Blake2(bytes) => {
            let hex = bytes.iter().fold(String::new(), |mut hex, b| {
                let _ = write!(hex, "{b:02x}");
                hex
            });
            field(text, &format!("{pack}BLAKE2{mac}"), &hex);
        }
        Hash::None => {}
    }
    if let Some(host) = item.host {
        field(text, "Host OS", host);
    }
    field(text, "Compression", &item.compression);
    if let Some(version) = item.version {
        field(text, "File version", &version.to_string());
    }
    let mut flags = String::new();
    if item.encrypted {
        flags.push_str("encrypted ");
    }
    if item.solid {
        flags.push_str("solid ");
    }
    if !flags.is_empty() {
        field(text, "Flags", &flags);
    }
}

/// The names a command picks: its masks and list files, less `-x`'s, within `-n`'s.
pub(super) struct Masks {
    include: Vec<String>,
    exclude: Vec<String>,
    filter: Vec<String>,
}

impl Masks {
    pub(super) fn new<SE: cash_core::ShellExtensions>(
        rar: &Rar<'_, SE>,
        parsed: &Parsed,
    ) -> Result<Self, Stop> {
        let mut include = Vec::new();
        for name in &parsed.names {
            match name {
                Name::Plain(mask) => include.push(normalize(mask)),
                Name::List(list) => include.extend(read_list(rar, list)?),
            }
        }
        let mut exclude = Vec::new();
        for mask in &rar.switches.exclude {
            exclude.push(normalize(mask));
        }
        for list in &rar.switches.exclude_lists {
            exclude.extend(read_list(rar, list.as_deref().unwrap_or_default())?);
        }
        let mut filter = Vec::new();
        for mask in &rar.switches.include {
            filter.push(normalize(mask));
        }
        for list in &rar.switches.include_lists {
            filter.extend(read_list(rar, list.as_deref().unwrap_or_default())?);
        }
        Ok(Self {
            include,
            exclude: exclude.into_iter().filter(|m| !m.is_empty()).collect(),
            filter: filter.into_iter().filter(|m| !m.is_empty()).collect(),
        })
    }

    /// Masks that take every name.
    pub(super) const fn all() -> Self {
        Self {
            include: Vec::new(),
            exclude: Vec::new(),
            filter: Vec::new(),
        }
    }

    /// The folders masks name before their last part, wildcard-free: `SUBDIR` of
    /// `SUBDIR\*`, what `-ep1` takes off the names it matches.
    pub(super) fn bases(&self) -> impl Iterator<Item = &str> {
        self.include.iter().filter_map(|mask| {
            let (base, _) = mask.rsplit_once('/')?;
            (!base.is_empty() && !open::has_wildcard(base)).then_some(base)
        })
    }

    /// Whether an archived name is wanted.
    pub(super) fn wants(&self, name: &str) -> bool {
        let name = name.replace('\\', "/");
        (self.include.is_empty() || self.include.iter().any(|mask| matches(mask, &name)))
            && !self.exclude.iter().any(|mask| matches(mask, &name))
            && (self.filter.is_empty() || self.filter.iter().any(|mask| matches(mask, &name)))
    }

    /// Whether a name is wanted by a mask that is that name, no wildcard in it: how an
    /// older version of a file, `name;N`, is chosen without `-ver`.
    pub(super) fn names(&self, name: &str) -> bool {
        let name = name.replace('\\', "/");
        self.include
            .iter()
            .any(|mask| !open::has_wildcard(mask) && mask.eq_ignore_ascii_case(&name))
            && self.wants(&name)
    }
}

fn normalize(mask: &str) -> String {
    mask.replace('\\', "/")
}

/// A list file's names, one a line, `//` starting a comment; `@` alone is standard
/// input. One that does not open aborts.
pub(super) fn read_list<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    list: &str,
) -> Result<Vec<String>, Stop> {
    let bytes = if list.is_empty() {
        let mut bytes = Vec::new();
        let _ = std::io::Read::read_to_end(&mut rar.context.stdin(), &mut bytes);
        bytes
    } else {
        match std::fs::read(rar.path(list)) {
            Ok(bytes) => bytes,
            Err(error) => {
                rar.console.err(&format!(
                    "\nCannot open {list}\n{}",
                    open::system_message(&error)
                ));
                return Err(Stop::Aborted(super::code::OPEN));
            }
        }
    };
    Ok(super::decode_text(&bytes)
        .lines()
        .map(|line| {
            let line = line.split("//").next().unwrap_or_default();
            normalize(line.trim_end())
        })
        .filter(|line| !line.is_empty())
        .collect())
}

/// rar's match of a mask with an archived name: a mask with no folder and a wildcard
/// matches the name's last part anywhere; a plain one, the name or a folder of it.
fn matches(mask: &str, name: &str) -> bool {
    let mask = mask.trim_end_matches('/');
    if !open::has_wildcard(mask) {
        let mask = mask.to_lowercase();
        let name = name.to_lowercase();
        return name == mask || name.starts_with(&format!("{mask}/"));
    }
    if !mask.contains('/') {
        let last = name.rsplit('/').next().unwrap_or(name);
        return open::mask_matches(mask, last);
    }
    open::mask_matches(mask, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios_round_down() {
        assert_eq!(ratio(2068, 4198), " 49%");
        assert_eq!(ratio(32, 18), "177%");
        assert_eq!(ratio(5, 0), "  0%");
    }

    #[test]
    fn masks_without_a_folder_match_anywhere() {
        assert!(matches("*.txt", "dir/a.txt"));
        assert!(matches("dir", "dir/a.txt"));
        assert!(!matches("a.txt", "dir/a.txt"));
        assert!(matches("HELLO.txt", "hello.txt"));
    }
}
