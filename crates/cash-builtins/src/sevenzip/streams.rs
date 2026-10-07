//! The formats of one compressed stream, as 7-Zip's gzip, bzip2, xz and lzma handlers
//! open them (`GzHandler.cpp`, `Bz2Handler.cpp`, `XzHandler.cpp`, `LzmaHandler.cpp`):
//! one item, named by the stream or else by the archive's name, with the facts each
//! handler knows before decoding; and its data, decoded on reading.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use cash_archive::codec::{self, Codec, CodecError};
use cash_archive::sevenz::Problem;

use super::archive::{Data, Item, Kind, Prop};

/// FILETIME ticks at the Unix epoch.
const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;

/// A stream archive's data, for decoding.
pub(super) struct Stream {
    path: PathBuf,
    codec: Codec,
    /// Whether the handler gives the size it decoded (all but lzma's).
    reports_size: bool,
}

/// What opening found: the archive's facts and its one item.
pub(super) struct Opening {
    pub(super) physical_size: Option<u64>,
    pub(super) props: Vec<(&'static str, String)>,
    pub(super) item_props: Vec<Prop>,
    pub(super) item: Item,
    pub(super) stream: Stream,
}

/// The extensions a format's name takes, with the extension the item gets for each
/// (`REGISTER_ARC`'s lists: `tgz` holds a `.tar`).
const fn extensions(kind: Kind) -> &'static [(&'static str, &'static str)] {
    match kind {
        Kind::Gzip => &[
            ("gz", ""),
            ("gzip", ""),
            ("tgz", ".tar"),
            ("tpz", ".tar"),
            ("apk", ".tar"),
        ],
        Kind::Bzip2 => &[
            ("bz2", ""),
            ("bzip2", ""),
            ("tbz2", ".tar"),
            ("tbz", ".tar"),
        ],
        Kind::Xz => &[("xz", ""), ("txz", ".tar")],
        Kind::Zstd => &[("zst", ""), ("tzst", ".tar")],
        Kind::Lzma => &[("lzma", "")],
        _ => &[],
    }
}

/// `GetDefaultName2`: the item's name from the archive's, its extension taken off (and
/// `.tar` put on for `tgz` and the like), or `~` added when it has none.
pub(super) fn default_name(file_name: &str, kind: Kind) -> String {
    let ext_of_name = file_name
        .rsplit_once('.')
        .map(|(_, e)| e)
        .unwrap_or_default();
    let exts = extensions(kind);
    let (ext, add) = exts
        .iter()
        .find(|(e, _)| e.eq_ignore_ascii_case(ext_of_name))
        .or_else(|| exts.first())
        .copied()
        .unwrap_or(("", ""));
    let chars: Vec<char> = file_name.chars().collect();
    let ext_len = ext.chars().count();
    let name = 'name: {
        if chars.len() > ext_len + 1 {
            let dot = chars.len() - (ext_len + 1);
            let tail: String = chars[dot + 1..].iter().collect();
            if chars[dot] == '.' && tail.eq_ignore_ascii_case(ext) {
                break 'name format!("{}{add}", chars[..dot].iter().collect::<String>());
            }
        }
        if let Some(dot) = chars.iter().rposition(|&c| c == '.')
            && dot > 0
        {
            break 'name format!("{}{add}", chars[..dot].iter().collect::<String>());
        }
        if add.is_empty() {
            format!("{file_name}~")
        } else {
            format!("{file_name}{add}")
        }
    };
    name.trim_end().to_owned()
}

/// Whether `head` starts a stream of `kind` (each handler's `IsArc`).
pub(super) fn signature(kind: Kind, head: &[u8]) -> bool {
    match kind {
        Kind::Gzip => head.starts_with(&[0x1F, 0x8B, 0x08]),
        Kind::Bzip2 => {
            head.len() >= 10
                && head.starts_with(b"BZh")
                && (b'1'..=b'9').contains(&head[3])
                && (head[4..10] == [0x31, 0x41, 0x59, 0x26, 0x53, 0x59]
                    || head[4..10] == [0x17, 0x72, 0x45, 0x38, 0x50, 0x90])
        }
        Kind::Xz => head.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0]),
        Kind::Zstd => head.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]),
        Kind::Lzma => lzma_header(head).is_some(),
        _ => false,
    }
}

/// An `.lzma` header: the properties byte, the dictionary, the size if stored; the
/// range coder's first byte after it is zero (`CHeader::Parse` and `Open`).
fn lzma_header(head: &[u8]) -> Option<(u8, u32, Option<u64>)> {
    if head.len() < 13 + 2 || head[13] != 0 {
        return None;
    }
    let props = head[0];
    let dict = u32::from_le_bytes(head[1..5].try_into().ok()?);
    let size = u64::from_le_bytes(head[5..13].try_into().ok()?);
    let dict_ok =
        dict == 1 || dict == u32::MAX || (0..=30).any(|i| dict == 2 << i || dict == 3 << i);
    let known = (size != u64::MAX).then_some(size);
    let ok = props < 5 * 5 * 9 && known.is_none_or(|s| s < 1 << 56) && dict_ok;
    let empty_looking = head.len() - 13 > 10 && size == 0 && props == 0;
    (ok && !empty_looking).then_some((props, dict, known))
}

fn le16(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from(u16::from_le_bytes(
        b.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn le32(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from(u32::from_le_bytes(
        b.get(at..at + 4)?.try_into().ok()?,
    )))
}

/// Opens a stream archive of `kind` whose first bytes are `head`.
#[expect(clippy::too_many_lines, reason = "each format's facts in turn")]
pub(super) fn open(
    kind: Kind,
    file: &mut File,
    path: &Path,
    head: &[u8],
) -> io::Result<Option<Opening>> {
    let len = file.seek(SeekFrom::End(0))?;
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut item = Item {
        path: default_name(&file_name, kind),
        ..Item::default()
    };
    let codec = match kind {
        Kind::Gzip => Codec::Gzip,
        Kind::Bzip2 => Codec::Bzip2,
        Kind::Xz => Codec::Xz,
        Kind::Zstd => Codec::Zstd,
        _ => Codec::Lzma,
    };
    let mut stream = Stream {
        path: path.to_path_buf(),
        codec,
        reports_size: true,
    };
    let (physical_size, props, item_props) = match kind {
        Kind::Gzip => {
            let Some(header) = gzip_header(file)? else {
                return Ok(None);
            };
            if let Some(name) = header.name {
                item.path = name;
            }
            let mut footer = [0u8; 8];
            if len >= 8 {
                file.seek(SeekFrom::End(-8))?;
                file.read_exact(&mut footer)?;
            }
            item.crc = u32::try_from(le32(&footer, 0).unwrap_or(0)).ok();
            item.size = le32(&footer, 4);
            item.packed = Some(len);
            item.modified =
                (header.mtime != 0).then(|| header.mtime * 10_000_000 + UNIX_EPOCH_TICKS);
            item.host_os = Some(host_os(header.os));
            (
                None,
                vec![("Headers Size", header.size.to_string())],
                vec![
                    Prop::Path,
                    Prop::Size,
                    Prop::PackedSize,
                    Prop::Modified,
                    Prop::HostOs,
                    Prop::Crc,
                ],
            )
        }
        Kind::Bzip2 => (None, Vec::new(), vec![Prop::Size, Prop::PackedSize]),
        Kind::Xz => {
            let (methods, characteristics) = xz_first_block(head);
            file.seek(SeekFrom::Start(0))?;
            let info = codec::xz::file_info(file).ok();
            let mut method = methods;
            let mut props = Vec::new();
            let physical = info.as_ref().map(|_| len);
            if let Some(info) = &info {
                let mut mask = 0u32;
                for s in &info.streams {
                    mask |= 1 << (s.check & 15);
                }
                for check in 0..16 {
                    if mask >> check & 1 != 0 {
                        if !method.is_empty() {
                            method.push(' ');
                        }
                        match check {
                            0 => method.push_str("NoCheck"),
                            1 => method.push_str("CRC32"),
                            4 => method.push_str("CRC64"),
                            10 => method.push_str("SHA256"),
                            n => {
                                let _ = write!(method, "Check-{n}");
                            }
                        }
                    }
                }
                item.size = Some(info.streams.iter().map(|s| s.uncomp_size).sum());
                item.packed = Some(len);
            }
            if !method.is_empty() {
                props.push(("Method", method.clone()));
                item.method = Some(method);
            }
            if let Some(info) = &info {
                let blocks: Vec<u64> = info
                    .streams
                    .iter()
                    .flat_map(|s| s.blocks.iter().map(|b| b.uncomp_size))
                    .collect();
                props.push(("Streams", info.streams.len().to_string()));
                props.push(("Blocks", blocks.len().to_string()));
                if blocks.len() > 1 {
                    let largest = blocks.iter().copied().max().unwrap_or(0);
                    props.push(("Cluster Size", largest.to_string()));
                }
            }
            if !characteristics.is_empty() {
                props.push(("Characteristics", characteristics));
            }
            (
                physical,
                props,
                vec![Prop::Size, Prop::PackedSize, Prop::Method],
            )
        }
        Kind::Zstd => {
            file.seek(SeekFrom::Start(0))?;
            let mut first = Vec::new();
            file.by_ref().take(1 << 16).read_to_end(&mut first)?;
            let Some(method) = zstd_method(&first) else {
                return Ok(None);
            };
            (
                None,
                vec![("Method", method)],
                vec![Prop::Size, Prop::PackedSize],
            )
        }
        _ => {
            let Some((lc_lp_pb, dict, size)) = lzma_header(head) else {
                return Ok(None);
            };
            let method = lzma_method(lc_lp_pb, dict);
            item.size = size;
            item.method = Some(method.clone());
            stream.reports_size = false;
            (
                None,
                vec![("Method", method)],
                vec![Prop::Size, Prop::PackedSize, Prop::Method],
            )
        }
    };
    Ok(Some(Opening {
        physical_size,
        props,
        item_props,
        item,
        stream,
    }))
}

/// A gzip header's fields 7-Zip lists.
struct GzipHeader {
    size: u64,
    name: Option<String>,
    mtime: u64,
    os: u8,
}

/// `CItem::ReadHeader`: the fixed part, then the extra field, name, comment and CRC
/// its flags say are there.
fn gzip_header(file: &mut File) -> io::Result<Option<GzipHeader>> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.by_ref().take(1 << 16).read_to_end(&mut bytes)?;
    if bytes.len() < 10 || !bytes.starts_with(&[0x1F, 0x8B, 0x08]) {
        return Ok(None);
    }
    let flags = bytes[3];
    let mtime = le32(&bytes, 4).unwrap_or(0);
    let os = bytes[9];
    let mut pos = 10usize;
    if flags & 4 != 0 {
        let Some(xlen) = le16(&bytes, pos) else {
            return Ok(None);
        };
        pos += 2 + usize::try_from(xlen).unwrap_or(usize::MAX);
    }
    let zero_string = |pos: &mut usize| -> Option<Vec<u8>> {
        let rest = bytes.get(*pos..)?;
        let end = rest.iter().position(|&b| b == 0)?;
        let s = rest[..end].to_vec();
        *pos += end + 1;
        Some(s)
    };
    let name = if flags & 8 == 0 {
        None
    } else {
        let Some(raw) = zero_string(&mut pos) else {
            return Ok(None);
        };
        Some(raw.iter().map(|&b| char::from(b)).collect())
    };
    if flags & 16 != 0 && zero_string(&mut pos).is_none() {
        return Ok(None);
    }
    if flags & 2 != 0 {
        pos += 2;
    }
    if pos > bytes.len() {
        return Ok(None);
    }
    Ok(Some(GzipHeader {
        size: pos as u64,
        name,
        mtime,
        os,
    }))
}

/// gzip's host systems, as 7-Zip names them.
fn host_os(os: u8) -> String {
    const NAMES: [&str; 20] = [
        "FAT",
        "AMIGA",
        "VMS",
        "Unix",
        "VM/CMS",
        "Atari",
        "HPFS",
        "Macintosh",
        "Z-System",
        "CP/M",
        "TOPS-20",
        "NTFS",
        "SMS/QDOS",
        "Acorn",
        "VFAT",
        "MVS",
        "BeOS",
        "Tandem",
        "OS/400",
        "OS/X",
    ];
    NAMES
        .get(usize::from(os))
        .map_or_else(|| os.to_string(), |n| (*n).to_owned())
}

/// A .xz stream's first block header: its filters as 7-Zip names them, and which
/// sizes it holds (`BlockPackSize BlockUnpackSize`).
fn xz_first_block(head: &[u8]) -> (String, String) {
    let mut methods = String::new();
    let mut characts = String::new();
    let Some(&size_byte) = head.get(12) else {
        return (methods, characts);
    };
    if size_byte == 0 {
        return (methods, characts);
    }
    let end = 12 + (usize::from(size_byte) + 1) * 4;
    let Some(block) = head.get(12..end) else {
        return (methods, characts);
    };
    let flags = block[1];
    let mut pos = 2usize;
    let varint = |pos: &mut usize| -> Option<u64> {
        let mut value = 0u64;
        for i in 0..9 {
            let b = *block.get(*pos)?;
            *pos += 1;
            value |= u64::from(b & 0x7F) << (7 * i);
            if b & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    };
    if flags & 0x40 != 0 {
        characts.push_str("BlockPackSize");
        if varint(&mut pos).is_none() {
            return (methods, characts);
        }
    }
    if flags & 0x80 != 0 {
        if !characts.is_empty() {
            characts.push(' ');
        }
        characts.push_str("BlockUnpackSize");
        if varint(&mut pos).is_none() {
            return (methods, characts);
        }
    }
    for _ in 0..=(flags & 3) {
        let (Some(id), Some(size)) = (varint(&mut pos), varint(&mut pos)) else {
            break;
        };
        let props = block
            .get(pos..pos + usize::try_from(size).unwrap_or(usize::MAX))
            .unwrap_or_default();
        pos += props.len();
        if !methods.is_empty() {
            methods.push(' ');
        }
        methods.push_str(&xz_filter(id, props));
    }
    (methods, characts)
}

/// `AddMethodString`: a filter's name, and its properties.
fn xz_filter(id: u64, props: &[u8]) -> String {
    let name = match id {
        0x03 => "Delta".to_owned(),
        0x04 => "BCJ".to_owned(),
        0x05 => "PPC".to_owned(),
        0x06 => "IA64".to_owned(),
        0x07 => "ARM".to_owned(),
        0x08 => "ARMT".to_owned(),
        0x09 => "SPARC".to_owned(),
        0x0A => "ARM64".to_owned(),
        0x0B => "RISCV".to_owned(),
        0x21 => "LZMA2".to_owned(),
        other => other.to_string(),
    };
    if props.is_empty() {
        return name;
    }
    let detail = match (id, props) {
        (0x21, [p]) => {
            let p = u32::from(*p);
            if p & 1 == 0 {
                (p / 2 + 12).to_string()
            } else {
                let size = (2 | (p & 1)) << (p / 2 + 1);
                if p > 17 {
                    format!("{}m", size >> 10)
                } else {
                    format!("{size}k")
                }
            }
        }
        (0x03, [p]) => (u32::from(*p) + 1).to_string(),
        (0x0A, [p]) => (u32::from(*p) + 16 + 2).to_string(),
        _ => format!(
            "[{}]",
            props.iter().fold(String::new(), |mut hex, b| {
                let _ = write!(hex, "{b:02X}");
                hex
            })
        ),
    };
    format!("{name}:{detail}")
}

/// The zstd handler's `Method` on opening: what the first data frame's header says
/// ("header-open-only:"), after any skippable frames (`CFrameHeader::Parse` and
/// `GetArchiveProperty`).
#[expect(clippy::too_many_lines, reason = "the frame header, field by field")]
fn zstd_method(bytes: &[u8]) -> Option<String> {
    let mut pos = 0usize;
    let mut skip_frames = 0u64;
    let mut skip_size = 0u64;
    loop {
        let magic = le32(bytes, pos)?;
        if magic == 0xFD2F_B528 {
            break;
        }
        if magic & 0xFFFF_FFF0 != 0x184D_2A50 {
            return None;
        }
        let size = le32(bytes, pos + 4)?;
        skip_frames += 1;
        skip_size += size;
        pos += 8 + usize::try_from(size).ok()?;
    }
    pos += 4;
    let d = *bytes.get(pos)?;
    pos += 1;
    if d & 0x08 != 0 {
        return None;
    }
    let single = d & 0x20 != 0;
    let wd = if single {
        0
    } else {
        let w = *bytes.get(pos)?;
        pos += 1;
        w
    };
    let dict_flag = d & 3;
    let mut dict_id = 0u64;
    if dict_flag != 0 {
        let n = 1usize << (dict_flag - 1);
        for (i, b) in bytes.get(pos..pos + n)?.iter().enumerate() {
            dict_id |= u64::from(*b) << (8 * i);
        }
        pos += n;
    }
    let fcs = d >> 5;
    let mut content = 0u64;
    if fcs != 0 {
        let flag = fcs >> 1;
        let n = 1usize << flag;
        for (i, b) in bytes.get(pos..pos + n)?.iter().enumerate() {
            content |= u64::from(*b) << (8 * i);
        }
        if flag == 1 {
            content += 256;
        }
    }
    let content_defined = d & 0xE0 != 0;
    let (mut window, mut allocate) = (content, content);
    if !single {
        let e = u64::from(wd >> 3);
        let m = u64::from(wd & 7);
        window = (8 + m) << (e + 7);
        if !content_defined || dict_id != 0 || allocate > window {
            allocate = window;
        }
    }
    let size = |w: u64| {
        if w & ((1 << 30) - 1) == 0 && w != 0 {
            format!("{}GiB", w >> 30)
        } else if w & ((1 << 20) - 1) == 0 && w != 0 {
            format!("{}MiB", w >> 20)
        } else if w & ((1 << 10) - 1) == 0 && w != 0 {
            format!("{}KiB", w >> 10)
        } else {
            w.to_string()
        }
    };
    let mut words = vec!["header-open-only:".to_owned()];
    if dict_id != 0 {
        words.push(format!("dictionary-ID:{dict_id}"));
    }
    words.push(if d & 0x04 != 0 { "XXH64" } else { "NO-XXH64" }.to_owned());
    if d & 0x10 != 0 {
        words.push("unused_bit".to_owned());
    }
    if single {
        words.push("single-segments".to_owned());
    } else {
        let e = u32::from(wd >> 3) + 10;
        let m = wd & 7;
        words.push(if m == 0 {
            format!("wnd-desc-log-MAX:{e}")
        } else {
            format!("wnd-desc-log-MAX:{e}.{m}")
        });
    }
    if content_defined || !single {
        words.push(format!("wnd-MAX:{}", size(window)));
        if window != allocate {
            words.push(format!("wnd-use-MAX:{}", size(allocate)));
        }
    }
    if skip_frames != 0 {
        words.push(format!("skip-frames:{skip_frames}"));
        words.push(format!("skip-frames-size-total:{skip_size}"));
    }
    if !content_defined {
        words.push("unknown-content-size".to_owned());
    } else {
        words.push(format!("content-size-frame-max:{content}"));
        words.push(format!("content-size-total:{content}"));
    }
    Some(words.join(" "))
}

/// `GetMethod`: `LZMA:` and the dictionary (`23` for 2^23, else `Nm`, `Nk`, `Nb`), with
/// `lc`, `lp` and `pb` when not 3, 0 and 2.
fn lzma_method(props: u8, dict: u32) -> String {
    let mut s = String::from("LZMA:");
    if dict.is_power_of_two() {
        s.push_str(&dict.trailing_zeros().to_string());
    } else if dict & ((1 << 20) - 1) == 0 {
        let _ = write!(s, "{}m", dict >> 20);
    } else if dict & ((1 << 10) - 1) == 0 {
        let _ = write!(s, "{}k", dict >> 10);
    } else {
        let _ = write!(s, "{dict}b");
    }
    let d = u32::from(props);
    let (lc, lp, pb) = (d % 9, (d / 9) % 5, d / 45);
    if lc != 3 {
        let _ = write!(s, ":lc{lc}");
    }
    if lp != 0 {
        let _ = write!(s, ":lp{lp}");
    }
    if pb != 2 {
        let _ = write!(s, ":pb{pb}");
    }
    s
}

impl Stream {
    /// The item's data, decoded as it is read.
    pub(super) fn data(&self) -> io::Result<StreamData> {
        let file = File::open(&self.path)?;
        Ok(StreamData {
            reader: codec::members_reader(self.codec, io::BufReader::new(file)),
            out: 0,
            problem: None,
            reports_size: self.reports_size,
        })
    }
}

/// A stream archive's item data: decoded, counted, its fault kept for `finish`.
pub(super) struct StreamData {
    reader: Box<dyn Read>,
    out: u64,
    problem: Option<Problem>,
    reports_size: bool,
}

impl Read for StreamData {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.problem.is_some() {
            return Ok(0);
        }
        match self.reader.read(buf) {
            Ok(n) => {
                self.out += n as u64;
                Ok(n)
            }
            Err(error) => {
                let problem = match codec::codec_error(&error) {
                    Some(CodecError::Checksum) => Problem::Crc,
                    Some(CodecError::Truncated) => Problem::UnexpectedEnd,
                    Some(CodecError::TrailingData) => Problem::DataAfterEnd,
                    Some(CodecError::NotThisFormat) if self.out > 0 => Problem::DataAfterEnd,
                    Some(CodecError::Unsupported(_)) => Problem::UnsupportedMethod,
                    Some(_) => Problem::Data,
                    None if error.kind() == io::ErrorKind::UnexpectedEof => Problem::UnexpectedEnd,
                    None => return Err(error),
                };
                self.problem = Some(problem);
                Ok(0)
            }
        }
    }
}

impl Data for StreamData {
    fn finish(&mut self) -> Result<(), Problem> {
        let _ = io::copy(self, &mut io::sink());
        self.problem.map_or(Ok(()), Err)
    }

    fn encrypted(&self) -> bool {
        false
    }

    fn unpacked(&self) -> Option<u64> {
        self.reports_size.then_some(self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_names_follow_7_zip() {
        assert_eq!(default_name("a.txt.gz", Kind::Gzip), "a.txt");
        assert_eq!(default_name("x.tgz", Kind::Gzip), "x.tar");
        assert_eq!(default_name("noext", Kind::Gzip), "noext~");
        assert_eq!(default_name("gz.7z", Kind::Gzip), "gz");
        assert_eq!(default_name("A.TBZ", Kind::Bzip2), "A.tar");
        assert_eq!(default_name("l.bin", Kind::Lzma), "l");
    }

    #[test]
    fn methods_as_7_zip_names_them() {
        assert_eq!(lzma_method(0x5D, 1 << 23), "LZMA:23");
        assert_eq!(lzma_method(0x5D, 3 << 20), "LZMA:3m");
        assert_eq!(xz_filter(0x21, &[22]), "LZMA2:23");
        assert_eq!(xz_filter(0x21, &[19]), "LZMA2:3m");
        assert_eq!(xz_filter(0x21, &[1]), "LZMA2:6k");
        assert_eq!(xz_filter(0x03, &[3]), "Delta:4");
        assert_eq!(xz_filter(0x04, &[]), "BCJ");
    }
}
