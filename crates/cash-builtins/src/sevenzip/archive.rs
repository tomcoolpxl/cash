//! An archive opened for `l`, `t`, `x` and `e`: its facts and items as 7-Zip lists them,
//! and each item's data, whatever the format.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use cash_archive::sevenz::{self, ArchiveReader, Block, Password, Problem};

/// The formats 7z knows, by 7-Zip's names for them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    SevenZ,
    Zip,
    Tar,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
    Lzma,
}

impl Kind {
    /// `-t`'s name, case aside.
    pub(super) fn by_name(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "7z" => Self::SevenZ,
            "zip" => Self::Zip,
            "tar" => Self::Tar,
            "gzip" | "gz" => Self::Gzip,
            "bzip2" | "bz2" => Self::Bzip2,
            "xz" => Self::Xz,
            "zstd" | "zst" => Self::Zstd,
            "lzma" => Self::Lzma,
            _ => return None,
        })
    }

    /// 7-Zip's name for the format, as `Type =` shows it.
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::SevenZ => "7z",
            Self::Zip => "zip",
            Self::Tar => "tar",
            Self::Gzip => "gzip",
            Self::Bzip2 => "bzip2",
            Self::Xz => "xz",
            Self::Zstd => "zstd",
            Self::Lzma => "lzma",
        }
    }

    /// The format a name's suffix suggests.
    pub(super) fn by_extension(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
        Some(match ext.as_str() {
            "7z" => Self::SevenZ,
            "zip" | "z01" | "zipx" | "jar" => Self::Zip,
            "tar" | "ova" => Self::Tar,
            "gz" | "gzip" | "tgz" | "tpz" => Self::Gzip,
            "bz2" | "bzip2" | "tbz2" | "tbz" => Self::Bzip2,
            "xz" | "txz" => Self::Xz,
            "zst" | "tzst" => Self::Zstd,
            "lzma" => Self::Lzma,
            _ => return None,
        })
    }

    /// The format the first bytes say, of those with a signature.
    fn sniff(head: &[u8]) -> Option<Self> {
        if head.starts_with(&[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C]) {
            return Some(Self::SevenZ);
        }
        if head.get(257..262) == Some(b"ustar") {
            return Some(Self::Tar);
        }
        [Self::Gzip, Self::Bzip2, Self::Xz, Self::Zstd]
            .into_iter()
            .find(|&kind| super::streams::signature(kind, head))
    }

    /// Whether the format is one cash's 7z opens.
    const fn readable(self) -> bool {
        matches!(
            self,
            Self::SevenZ
                | Self::Tar
                | Self::Gzip
                | Self::Bzip2
                | Self::Xz
                | Self::Zstd
                | Self::Lzma
        )
    }
}

/// One property of an item in a technical listing (`-slt`), in 7-Zip's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Prop {
    Path,
    Size,
    PackedSize,
    Modified,
    Anti,
    Created,
    Accessed,
    Attributes,
    Crc,
    Encrypted,
    Method,
    Block,
    HostOs,
    Folder,
    Mode,
    User,
    Group,
    UserId,
    GroupId,
    SymLink,
    HardLink,
    Characteristics,
    Comment,
    DeviceMajor,
    DeviceMinor,
}

impl Prop {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Path => "Path",
            Self::Size => "Size",
            Self::PackedSize => "Packed Size",
            Self::Modified => "Modified",
            Self::Anti => "Anti",
            Self::Created => "Created",
            Self::Accessed => "Accessed",
            Self::Attributes => "Attributes",
            Self::Crc => "CRC",
            Self::Encrypted => "Encrypted",
            Self::Method => "Method",
            Self::Block => "Block",
            Self::HostOs => "Host OS",
            Self::Folder => "Folder",
            Self::Mode => "Mode",
            Self::User => "User",
            Self::Group => "Group",
            Self::UserId => "User ID",
            Self::GroupId => "Group ID",
            Self::SymLink => "Symbolic Link",
            Self::HardLink => "Hard Link",
            Self::Characteristics => "Characteristics",
            Self::Comment => "Comment",
            Self::DeviceMajor => "Device Major",
            Self::DeviceMinor => "Device Minor",
        }
    }
}

/// An item as 7-Zip lists it.
#[derive(Clone, Debug, Default)]
pub(super) struct Item {
    /// The path, `/` between its parts.
    pub(super) path: String,
    pub(super) is_dir: bool,
    pub(super) size: Option<u64>,
    pub(super) packed: Option<u64>,
    /// Times as FILETIME ticks.
    pub(super) modified: Option<u64>,
    pub(super) created: Option<u64>,
    pub(super) accessed: Option<u64>,
    pub(super) anti: bool,
    pub(super) attrib: Option<u32>,
    pub(super) crc: Option<u32>,
    pub(super) encrypted: bool,
    pub(super) method: Option<String>,
    pub(super) block: Option<u64>,
    /// The digits of a second a technical listing gives the modified, created and
    /// accessed times: 7 for FILETIME, 0 for Unix times, a pax time's own.
    pub(super) time_digits: [usize; 3],
    /// The nanoseconds past each time's ticks, for 8 and 9 digits.
    pub(super) time_extra: [u8; 3],
    pub(super) host_os: Option<String>,
    /// The format's other properties, by name.
    pub(super) extra: Vec<(Prop, String)>,
}

/// Why an archive did not open, in 7-Zip's kinds.
#[derive(Debug)]
pub(super) enum OpenFailure {
    /// It is not an archive of any format tried: the format its name or `-t` named, if
    /// one did, with the flags that say why ("Is not archive", "Unexpected end of
    /// archive", "Headers Error").
    NotArchive {
        tried: Option<Kind>,
        flags: Vec<&'static str>,
    },
    /// Its header is encrypted and the password given or typed is not the one.
    WrongPassword,
    /// Its header is encrypted and no password was given: one is to be asked for.
    PasswordNeeded,
    Io(io::Error),
}

/// An archive, open.
pub(super) struct Opened {
    pub(super) kind: Kind,
    /// The archive's size, when its format knows it on opening.
    pub(super) physical_size: Option<u64>,
    /// The format its name says, when it opened as another (7-Zip's
    /// `ErrorFormatIndex`).
    pub(super) type_warning: Option<Kind>,
    /// What `Print_OpenArchive_Props` shows after the physical size.
    pub(super) props: Vec<(&'static str, String)>,
    /// The properties a technical listing shows for each item.
    pub(super) item_props: Vec<Prop>,
    pub(super) items: Vec<Item>,
    /// Bytes after the archive's end.
    pub(super) tail: u64,
    /// What went wrong reading it, though it opened ("Unexpected end of archive").
    pub(super) error_flags: Vec<&'static str>,
    /// What looked wrong ("Headers Error").
    pub(super) warning_flags: Vec<&'static str>,
    backend: Backend,
}

/// What reads an open archive's data.
enum Backend {
    SevenZ(Box<ArchiveReader<File>>),
    Stream(super::streams::Stream),
    Tar(super::tar7::Tar),
}

/// An item's data as extraction reads it: read it, then `finish` says whether all of
/// it was there and right.
pub(super) trait Data: Read {
    /// Reads what is left, and says what was wrong with the data, if anything.
    fn finish(&mut self) -> Result<(), Problem>;
    /// Whether the item's data is encrypted, which 7-Zip names in its errors.
    fn encrypted(&self) -> bool;
    /// The size the format reports once the data is decoded, when it knew none before.
    fn unpacked(&self) -> Option<u64> {
        None
    }
}

impl Data for sevenz::EntryReader<'_> {
    fn finish(&mut self) -> Result<(), Problem> {
        sevenz::EntryReader::finish(self)
    }

    fn encrypted(&self) -> bool {
        self.is_encrypted()
    }
}

/// Opens `path` as `forced`, or as its first bytes say; a format its name suggests is
/// the one an error names.
pub(super) fn open(
    path: &Path,
    forced: Option<Kind>,
    password: Option<&str>,
) -> Result<Opened, OpenFailure> {
    let mut file = File::open(path).map_err(OpenFailure::Io)?;
    let mut head = [0u8; 1100];
    let read = read_up_to(&mut file, &mut head).map_err(OpenFailure::Io)?;
    file.seek(SeekFrom::Start(0)).map_err(OpenFailure::Io)?;
    let head = &head[..read];
    let sniffed = Kind::sniff(head).filter(|k| k.readable());
    let not_archive = |tried: Option<Kind>| OpenFailure::NotArchive {
        tried,
        flags: if tried.is_some() {
            vec!["Is not archive"]
        } else {
            Vec::new()
        },
    };
    // The format -t names; else the one the name's extension names, then the one the
    // first bytes say, then lzma, which has no signature (7-Zip's `CArc::OpenStream`).
    let by_name = Kind::by_extension(path);
    let (kind, type_warning) = match forced {
        Some(forced) if forced == Kind::Lzma && super::streams::signature(Kind::Lzma, head) => {
            (forced, None)
        }
        Some(forced) if forced == Kind::Tar && super::tar7::looks_like_tar(head) => (forced, None),
        Some(forced) if sniffed == Some(forced) => (forced, None),
        Some(forced) => return Err(not_archive(Some(forced))),
        None => match (by_name, sniffed) {
            (_, Some(sniffed)) => (sniffed, by_name.filter(|&n| n != sniffed)),
            (by_name, None) if super::tar7::looks_like_tar(head) => {
                (Kind::Tar, by_name.filter(|&n| n != Kind::Tar))
            }
            (by_name, None) if super::streams::signature(Kind::Lzma, head) => {
                (Kind::Lzma, by_name.filter(|&n| n != Kind::Lzma))
            }
            (by_name, None) => return Err(not_archive(by_name)),
        },
    };
    let mut opened = if kind == Kind::SevenZ {
        open_7z(file, password)?
    } else if kind == Kind::Tar {
        let len = file.seek(SeekFrom::End(0)).map_err(OpenFailure::Io)?;
        drop(file);
        let Some(opening) = super::tar7::open(path).map_err(OpenFailure::Io)? else {
            return Err(not_archive(by_name.or(Some(kind))));
        };
        Opened {
            kind,
            physical_size: Some(opening.physical_size),
            type_warning: None,
            props: opening.props,
            item_props: super::tar7::ITEM_PROPS.to_vec(),
            items: opening.items,
            tail: len.saturating_sub(opening.physical_size),
            error_flags: opening.error_flags,
            warning_flags: opening.warning_flags,
            backend: Backend::Tar(opening.tar),
        }
    } else {
        let mut file = file;
        let Some(opening) =
            super::streams::open(kind, &mut file, path, head).map_err(OpenFailure::Io)?
        else {
            return Err(not_archive(by_name.or(Some(kind))));
        };
        Opened {
            kind,
            physical_size: opening.physical_size,
            type_warning: None,
            props: opening.props,
            item_props: opening.item_props,
            items: vec![opening.item],
            tail: 0,
            error_flags: Vec::new(),
            warning_flags: Vec::new(),
            backend: Backend::Stream(opening.stream),
        }
    };
    opened.type_warning = type_warning;
    Ok(opened)
}

fn read_up_to(file: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut done = 0;
    while done < buf.len() {
        match file.read(&mut buf[done..])? {
            0 => break,
            n => done += n,
        }
    }
    Ok(done)
}

fn open_7z(mut file: File, password: Option<&str>) -> Result<Opened, OpenFailure> {
    let len = file.seek(SeekFrom::End(0)).map_err(OpenFailure::Io)?;
    file.seek(SeekFrom::Start(0)).map_err(OpenFailure::Io)?;
    let pw = password.map_or_else(Password::empty, Password::from);
    let archive = match sevenz::Archive::read(&mut file, &pw) {
        Ok(archive) => archive,
        Err(sevenz::Error::PasswordRequired) if password.is_none() => {
            return Err(OpenFailure::PasswordNeeded);
        }
        Err(sevenz::Error::PasswordRequired | sevenz::Error::MaybeBadPassword(_)) => {
            return Err(OpenFailure::WrongPassword);
        }
        Err(sevenz::Error::Truncated) => {
            return Err(OpenFailure::NotArchive {
                tried: Some(Kind::SevenZ),
                flags: vec!["Unexpected end of archive"],
            });
        }
        Err(sevenz::Error::Io(e, _)) if e.kind() == io::ErrorKind::UnexpectedEof => {
            return Err(OpenFailure::NotArchive {
                tried: Some(Kind::SevenZ),
                flags: vec!["Unexpected end of archive"],
            });
        }
        Err(_) => {
            return Err(OpenFailure::NotArchive {
                tried: Some(Kind::SevenZ),
                flags: vec!["Headers Error"],
            });
        }
    };
    let items = items_of(&archive);
    let mut props = vec![("Headers Size", archive.header_size.to_string())];
    let method = archive_method(&archive.blocks);
    if !method.is_empty() {
        props.push(("Method", method));
    }
    props.push(("Solid", if archive.is_solid { "+" } else { "-" }.to_owned()));
    props.push(("Blocks", archive.blocks.len().to_string()));
    let mut item_props = vec![Prop::Path, Prop::Size, Prop::PackedSize, Prop::Modified];
    let files = &archive.files;
    if files.iter().any(|f| f.is_anti_item) {
        item_props.push(Prop::Anti);
    }
    if files.iter().any(|f| f.has_creation_date) {
        item_props.push(Prop::Created);
    }
    if files.iter().any(|f| f.has_access_date) {
        item_props.push(Prop::Accessed);
    }
    if files.iter().any(|f| f.has_windows_attributes) {
        item_props.push(Prop::Attributes);
    }
    if files.iter().any(|f| f.has_crc) {
        item_props.push(Prop::Crc);
    }
    item_props.extend([Prop::Encrypted, Prop::Method, Prop::Block]);
    let physical_size = archive.physical_size;
    let reader = ArchiveReader::from_archive(archive, file, pw);
    Ok(Opened {
        kind: Kind::SevenZ,
        physical_size: Some(physical_size),
        type_warning: None,
        props,
        item_props,
        items,
        tail: len.saturating_sub(physical_size),
        error_flags: Vec::new(),
        warning_flags: Vec::new(),
        backend: Backend::SevenZ(Box::new(reader)),
    })
}

fn items_of(archive: &sevenz::Archive) -> Vec<Item> {
    let map = &archive.stream_map;
    archive
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            let block = map.file_block_index[index];
            let first_in_block = block.is_some_and(|b| map.block_files[b].first() == Some(&index));
            Item {
                path: file.name.clone(),
                is_dir: file.is_directory,
                size: Some(file.size),
                packed: match block {
                    None => Some(0),
                    Some(_) if first_in_block => Some(file.compressed_size),
                    Some(_) => None,
                },
                modified: file
                    .has_last_modified_date
                    .then(|| u64::from(file.last_modified_date)),
                created: file
                    .has_creation_date
                    .then(|| u64::from(file.creation_date)),
                accessed: file.has_access_date.then(|| u64::from(file.access_date)),
                anti: file.is_anti_item,
                attrib: file
                    .has_windows_attributes
                    .then_some(file.windows_attributes),
                crc: file.has_crc.then(|| u32::try_from(file.crc).unwrap_or(0)),
                encrypted: block.is_some_and(|b| archive.blocks[b].is_encrypted()),
                method: block.map(|b| block_method(&archive.blocks[b])),
                block: block.map(|b| b as u64),
                time_digits: [7; 3],
                time_extra: [0; 3],
                host_os: None,
                extra: Vec::new(),
            }
        })
        .collect()
}

fn method_id(id: &[u8]) -> u64 {
    id.iter().fold(0, |acc, b| (acc << 8) | u64::from(*b))
}

/// 7-Zip's codec names, for the methods it shows without properties.
const fn codec_name(id: u64) -> Option<&'static str> {
    Some(match id {
        0x00 => "Copy",
        0x03 => "Delta",
        0x0A => "ARM64",
        0x0B => "RISCV",
        0x21 => "LZMA2",
        0x02_0302 => "Swap2",
        0x02_0304 => "Swap4",
        0x03_0101 => "LZMA",
        0x03_0401 => "PPMD",
        0x04_0108 => "Deflate",
        0x04_0109 => "Deflate64",
        0x04_0202 => "BZip2",
        0x0303_011B => "BCJ2",
        0x0303_0103 => "BCJ",
        0x0303_0205 => "PPC",
        0x0303_0401 => "IA64",
        0x0303_0501 => "ARM",
        0x0303_0701 => "ARMT",
        0x0303_0805 => "SPARC",
        0x06F1_0701 => "7zAES",
        0x06F0_0181 => "AES256CBC",
        _ => return None,
    })
}

/// `GetStringForSizeValue`: `n` for 2^n, else `Nm`, `Nk` or `Nb`.
fn size_value(value: u32) -> String {
    if value.is_power_of_two() {
        return value.trailing_zeros().to_string();
    }
    if value & ((1 << 20) - 1) == 0 {
        format!("{}m", value >> 20)
    } else if value & ((1 << 10) - 1) == 0 {
        format!("{}k", value >> 10)
    } else {
        format!("{value}b")
    }
}

/// `GetLzma2String`: the dictionary an LZMA2 property byte gives.
fn lzma2_dictionary(prop: u8) -> String {
    let d = u32::from(prop);
    if d > 40 {
        return String::new();
    }
    if d & 1 == 0 {
        return ((d >> 1) + 12).to_string();
    }
    let mut d = (d >> 1) + 1;
    let mut unit = 'k';
    if d >= 10 {
        unit = 'm';
        d -= 10;
    }
    format!("{}{unit}", 3u32 << d)
}

fn hex_id(id: u64) -> String {
    let mut text = format!("{id:X}");
    if text.len() % 2 == 1 {
        text.insert(0, '0');
    }
    text
}

/// A block's coders as an item's `Method =` shows them, the last coder first:
/// `SetMethodToProp`.
pub(super) fn block_method(block: &Block) -> String {
    let mut names: Vec<String> = Vec::new();
    for coder in &block.coders {
        let id = method_id(coder.encoder_method_id());
        let props = coder.properties();
        let detail = match id {
            0x03_0101 if props.len() == 5 => {
                let dict = u32::from_le_bytes([props[1], props[2], props[3], props[4]]);
                let mut s = size_value(dict);
                let mut d = u32::from(props[0]);
                if d != 0x5D {
                    let lc = d % 9;
                    d /= 9;
                    let pb = d / 5;
                    let lp = d % 5;
                    if lc != 3 {
                        let _ = write!(s, ":lc{lc}");
                    }
                    if lp != 0 {
                        let _ = write!(s, ":lp{lp}");
                    }
                    if pb != 2 {
                        let _ = write!(s, ":pb{pb}");
                    }
                }
                s
            }
            0x21 if props.len() == 1 => lzma2_dictionary(props[0]),
            0x03_0401 if props.len() == 5 => format!(
                "o{}:mem{}",
                props[0],
                size_value(u32::from_le_bytes([props[1], props[2], props[3], props[4]]))
            ),
            0x03 if props.len() == 1 => (u32::from(props[0]) + 1).to_string(),
            0x0A | 0x0B if props.len() == 4 => {
                u32::from_le_bytes([props[0], props[1], props[2], props[3]]).to_string()
            }
            0x06F1_0701 if !props.is_empty() => (props[0] & 0x3F).to_string(),
            _ => String::new(),
        };
        let name = match id {
            0x03_0101 | 0x21 | 0x03_0401 | 0x03 | 0x0A | 0x0B | 0x0303_011B | 0x0303_0103
            | 0x06F1_0701 => codec_name(id).map(str::to_owned),
            _ => None,
        };
        let text = match name {
            Some(name) if detail.is_empty() => name,
            Some(name) => format!("{name}:{detail}"),
            None => codec_name(id).map_or_else(|| hex_id(id), str::to_owned),
        };
        names.push(text);
    }
    names.reverse();
    names.join(" ")
}

/// The archive's `Method =`: each method once, by id, LZMA2's and LZMA's largest
/// dictionaries.
fn archive_method(blocks: &[Block]) -> String {
    let mut ids: Vec<u64> = Vec::new();
    let mut lzma2_prop = 0u8;
    let mut lzma_dict = 0u32;
    for block in blocks {
        for coder in &block.coders {
            let id = method_id(coder.encoder_method_id());
            if let Err(at) = ids.binary_search(&id) {
                ids.insert(at, id);
            }
            let props = coder.properties();
            if id == 0x21 && props.len() == 1 {
                lzma2_prop = lzma2_prop.max(props[0]);
            }
            if id == 0x03_0101 && props.len() == 5 {
                lzma_dict =
                    lzma_dict.max(u32::from_le_bytes([props[1], props[2], props[3], props[4]]));
            }
        }
    }
    ids.iter()
        .map(|&id| match id {
            0x21 => format!("LZMA2:{}", lzma2_dictionary(lzma2_prop)),
            0x03_0101 => format!("LZMA:{}", size_value(lzma_dict)),
            _ => codec_name(id).map_or_else(|| hex_id(id), str::to_owned),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl Opened {
    /// The warnings opening gave, in 7-Zip's order of its flags: the headers', then
    /// data after the archive's end.
    pub(super) fn warnings(&self) -> Vec<&'static str> {
        let mut list = self.warning_flags.clone();
        if self.tail > 0 {
            list.push("There are data after the end of archive");
        }
        list
    }

    /// The password encrypted data is read with, typed after the archive was opened.
    pub(super) fn set_password(&mut self, password: &str) {
        if let Backend::SevenZ(reader) = &mut self.backend {
            reader.set_password(Password::from(password));
        }
    }

    /// The 7z reader underneath, which updating copies blocks from.
    pub(super) fn reader(&mut self) -> Option<&mut ArchiveReader<File>> {
        match &mut self.backend {
            Backend::SevenZ(reader) => Some(reader),
            Backend::Stream(_) | Backend::Tar(_) => None,
        }
    }

    /// The 7z archive's blocks, items and header, as read.
    pub(super) fn archive(&self) -> Option<&sevenz::Archive> {
        match &self.backend {
            Backend::SevenZ(reader) => Some(reader.archive()),
            Backend::Stream(_) | Backend::Tar(_) => None,
        }
    }

    /// Reads the items `wanted` names in the archive's order, handing each its data.
    pub(super) fn extract<E: From<io::Error>>(
        &mut self,
        wanted: &dyn Fn(usize) -> bool,
        mut each: impl FnMut(usize, &mut dyn Data) -> Result<bool, E>,
    ) -> Result<(), E> {
        match &mut self.backend {
            Backend::SevenZ(reader) => {
                reader.for_each_entries(wanted, |index, _entry, reader| each(index, reader))
            }
            Backend::Stream(stream) => {
                if wanted(0) {
                    let mut data = stream.data()?;
                    each(0, &mut data)?;
                }
                Ok(())
            }
            Backend::Tar(tar) => {
                for index in 0..self.items.len() {
                    if !wanted(index) {
                        continue;
                    }
                    let mut data = tar.data(index)?;
                    if !each(index, &mut data)? {
                        break;
                    }
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionaries_as_7_zip_names_them() {
        assert_eq!(lzma2_dictionary(9), "96k");
        assert_eq!(lzma2_dictionary(0), "12");
        assert_eq!(lzma2_dictionary(24), "24");
        assert_eq!(lzma2_dictionary(25), "24m");
        assert_eq!(size_value(1 << 24), "24");
        assert_eq!(size_value(24 << 10), "24k");
    }
}
