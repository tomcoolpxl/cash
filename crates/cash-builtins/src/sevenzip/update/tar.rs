//! The tar handler's update (`TarHandlerOut.cpp`, `TarUpdate.cpp`, `TarOut.cpp`): GNU
//! headers, or POSIX ones with `-mm=pax`; a name too long for its field in a
//! `././@LongLink` record (GNU) or a pax `path` record or the prefix field (POSIX); mode
//! 777 (555 for a read-only file), owner 0 and no owner's name, the time in seconds;
//! the archive ending in two empty records. Items kept as they were are copied whole,
//! headers and all; renamed ones get a new header over their old data.

use std::fs::File;
use std::io::{self, Read, Write};

use super::super::archive::Opened;
use super::super::methods::MethodError;
use super::super::scan::UNKNOWN_SIZE;
use super::super::tar7::{Tar, TarItem};
use super::{Input, Item, Job, Out, Stop, Warnings, win, win_error};

const RECORD: usize = 512;
const NAME_SIZE: usize = 100;
const USER_SIZE: usize = 32;
const PREFIX_SIZE: usize = 155;
/// The largest number seven octal digits hold.
const OCT7_MAX: u32 = (1 << 21) - 1;
const MAGIC_GNU: [u8; 8] = *b"ustar  \0";
const MAGIC_POSIX: [u8; 8] = *b"ustar\x0000";
/// FILETIME ticks at the Unix epoch, and in a second.
const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
const TICKS_PER_SECOND: u64 = 10_000_000;

/// `k_PropVar_TimePrec_*`: `-mtp`'s values.
const PREC_HIGH: u32 = 3;
const PREC_BASE: u32 = 16;

/// The tar handler's `-m` properties (`CHandler::SetProperties`).
struct Settings {
    posix: bool,
    mtime: bool,
    atime: bool,
    ctime: bool,
    /// The most digits of a second a pax time keeps.
    digits_max: u32,
}

fn boolean(value: Option<&str>) -> Result<bool, MethodError> {
    match value.map(str::to_ascii_lowercase).as_deref() {
        None | Some("" | "on" | "+") => Ok(true),
        Some("off" | "-") => Ok(false),
        _ => Err(MethodError::Invalid),
    }
}

/// `ParsePropToUInt32`: the number after the name, else the value's, else none.
fn number(rest: &str, value: Option<&str>) -> Result<Option<u32>, MethodError> {
    let text = match (rest, value) {
        ("", None) => return Ok(None),
        ("", Some(v)) => v,
        (r, None) => r,
        _ => return Err(MethodError::Invalid),
    };
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(MethodError::Invalid);
    }
    text.parse().map(Some).map_err(|_| MethodError::Invalid)
}

impl Settings {
    fn parse(properties: &[(String, Option<String>)]) -> Result<Self, MethodError> {
        let mut settings = Self {
            posix: false,
            mtime: true,
            atime: false,
            ctime: false,
            digits_max: 0,
        };
        let mut prec = None;
        for (name, value) in properties {
            let name = name.to_ascii_lowercase();
            let value = value.as_deref();
            if name.is_empty() {
                return Err(MethodError::Invalid);
            }
            if let Some(rest) = name.strip_prefix('x') {
                number(rest, value)?;
            } else if name == "cp" {
                number("", value)?;
            } else if name.starts_with("mt") || name.starts_with("memuse") {
            } else if name == "m" {
                settings.posix = match value.map(str::to_ascii_lowercase).as_deref() {
                    Some("pax" | "posix") => true,
                    Some("gnu") => false,
                    _ => return Err(MethodError::Invalid),
                };
            } else if name == "tm" {
                settings.mtime = boolean(value)?;
            } else if name == "ta" {
                settings.atime = boolean(value)?;
            } else if name == "tc" {
                settings.ctime = boolean(value)?;
            } else if let Some(rest) = name.strip_prefix("tp") {
                prec = Some(number(rest, value)?.unwrap_or(0));
            } else {
                return Err(MethodError::Invalid);
            }
        }
        if let Some(prec) = prec {
            settings.digits_max = match prec {
                0 => 7,
                PREC_HIGH => 9,
                p if p >= PREC_BASE + 9 => 9,
                p if p >= PREC_BASE => p - PREC_BASE,
                _ => 0,
            };
        }
        Ok(settings)
    }
}

/// The tar handler's `SetProperties`, for its verdict on `-m`.
pub(super) fn check(properties: &[(String, Option<String>)]) -> Result<(), MethodError> {
    Settings::parse(properties).map(drop)
}

/// A time as pax keeps it (`CPaxTime`): seconds, nanoseconds and the digits it had.
#[derive(Clone, Copy, Debug)]
struct PaxTime {
    sec: i64,
    ns: u32,
    digits: u32,
}

impl PaxTime {
    /// `FILETIME_To_PaxTime`: seconds and 100 ns ticks, seven digits.
    fn of_ticks(ticks: u64) -> Self {
        let sec = i64::try_from(ticks / TICKS_PER_SECOND).unwrap_or(i64::MAX)
            - i64::try_from(UNIX_EPOCH_TICKS / TICKS_PER_SECOND).unwrap_or(0);
        let quantums = u32::try_from(ticks % TICKS_PER_SECOND).unwrap_or(0);
        Self {
            sec,
            ns: quantums * 100,
            digits: 7,
        }
    }
}

/// An item as the tar writer takes it (`CItem`).
#[derive(Clone, Debug, Default)]
struct Header {
    name: Vec<u8>,
    link_name: Vec<u8>,
    mode: u32,
    uid: u32,
    gid: u32,
    pack_size: u64,
    size: u64,
    mtime: i64,
    link_flag: u8,
    magic: [u8; 8],
    user: Vec<u8>,
    group: Vec<u8>,
    dev_major: Option<u32>,
    dev_minor: Option<u32>,
    sparse: Vec<(u64, u64)>,
    pax_mtime: Option<PaxTime>,
    pax_atime: Option<PaxTime>,
    pax_ctime: Option<PaxTime>,
}

impl Header {
    /// An old item as it was read.
    fn of_old(old: &TarItem) -> Self {
        let pax = |t: Option<super::super::tar7::PaxTime>| {
            t.map(|t| PaxTime {
                sec: t.sec,
                ns: t.ns,
                digits: u32::try_from(t.digits.unwrap_or(0)).unwrap_or(0),
            })
        };
        Self {
            name: old.name.clone(),
            link_name: old.link_name.clone(),
            mode: old.mode,
            uid: old.uid,
            gid: old.gid,
            pack_size: old.pack_size,
            size: old.size,
            mtime: old.mtime,
            link_flag: old.link_flag,
            magic: old.magic,
            user: old.user.clone(),
            group: old.group.clone(),
            dev_major: old.dev_major,
            dev_minor: old.dev_minor,
            sparse: old.sparse.clone(),
            pax_mtime: pax(old.mtime_pax),
            pax_atime: pax(old.atime_pax),
            pax_ctime: pax(old.ctime_pax),
        }
    }

    const fn is_sparse(&self) -> bool {
        self.link_flag == b'S'
    }

    /// `Set_LinkFlag_for_File`: a character or block device, a FIFO, else a file.
    const fn set_link_flag_for_file(&mut self, mode: u32) {
        self.link_flag = match mode & 0o170_000 {
            0o020_000 => b'3',
            0o060_000 => b'4',
            0o010_000 => b'6',
            _ => b'0',
        };
    }
}

/// `WriteOctal_8`: seven octal digits and a NUL, or zero when it does not fit.
fn octal_8(field: &mut [u8], val: u32) {
    let mut val = if val > OCT7_MAX { 0 } else { val };
    for i in (0..7).rev() {
        field[i] = b'0' + u8::try_from(val & 7).unwrap_or(0);
        val >>= 3;
    }
}

/// `WriteOctal_12`: eleven octal digits, or 0x80 and the number in binary.
fn octal_12(field: &mut [u8], val: u64) {
    if val >= 1 << 33 {
        field[0] = 0x80;
        field[1..4].fill(0);
        field[4..12].copy_from_slice(&val.to_be_bytes());
        return;
    }
    let mut val = val;
    for i in (0..11).rev() {
        field[i] = b'0' + u8::try_from(val & 7).unwrap_or(0);
        val >>= 3;
    }
}

/// `WriteOctal_12_Signed`: a negative time in two's complement, 0xFF first.
fn octal_12_signed(field: &mut [u8], val: i64) {
    if let Ok(v) = u64::try_from(val) {
        octal_12(field, v);
    } else {
        field[..4].fill(0xFF);
        field[4..12].copy_from_slice(&val.to_be_bytes());
    }
}

/// `CopyString`: as much of the string as the field holds, no NUL needed.
fn copy_string(field: &mut [u8], src: &[u8]) {
    let len = src.len().min(field.len());
    field[..len].copy_from_slice(&src[..len]);
}

/// `AddPaxLine`: "LEN name=value\n", LEN counting itself.
fn pax_line(s: &mut Vec<u8>, name: &str, val: &[u8]) {
    let len = 3 + name.len() + val.len();
    let mut digits = 1;
    let n = loop {
        let n = (digits + len).to_string();
        if n.len() == digits {
            break n;
        }
        digits += 1;
    };
    s.extend_from_slice(n.as_bytes());
    s.push(b' ');
    s.extend_from_slice(name.as_bytes());
    s.push(b'=');
    s.extend_from_slice(val);
    s.push(b'\n');
}

/// `AddPaxTime`: the seconds, and the digits of a second kept when there are some.
fn pax_time(s: &mut Vec<u8>, name: &str, pt: PaxTime, digits_max: u32) {
    let digits = pt.digits.min(digits_max);
    let mut need_ns = false;
    let mut ns = 0;
    if digits != 0 {
        ns = pt.ns;
        need_ns = ns != 0;
        let d = 10u32.pow(9 - digits.min(9));
        ns = ns / d * d;
    }
    let mut v = String::new();
    let mut sec = pt.sec;
    if sec < 0 {
        sec = -sec;
        v.push('-');
        if ns != 0 {
            ns = 1_000_000_000 - ns;
            sec -= 1;
        }
    }
    v.push_str(&sec.to_string());
    if need_ns {
        let mut d = format!("{ns:09}");
        d.truncate(digits as usize);
        if !d.is_empty() {
            v.push('.');
            v.push_str(&d);
        }
    }
    pax_line(s, name, v.as_bytes());
}

fn pax_u32_if_big(s: &mut Vec<u8>, name: &str, v: u32) {
    if v > OCT7_MAX {
        pax_line(s, name, v.to_string().as_bytes());
    }
}

/// The archive being written (`COutArchive`).
struct TarOut<W: Write> {
    out: W,
    posix: bool,
    digits_max: u32,
}

impl<W: Write> TarOut<W> {
    fn write_data_and_residual(&mut self, data: &[u8]) -> io::Result<()> {
        self.out.write_all(data)?;
        self.residual(data.len() as u64)
    }

    /// `Write_AfterDataResidual`: zeros to the end of the record.
    fn residual(&mut self, size: u64) -> io::Result<()> {
        let v = usize::try_from(size % RECORD as u64).unwrap_or(0);
        if v == 0 {
            return Ok(());
        }
        self.out.write_all(&[0u8; RECORD][..RECORD - v])
    }

    /// `WriteHeaderReal`: one 512-byte record, with its checksum.
    fn header_real(
        &mut self,
        item: &Header,
        is_pax: bool,
        glob_name: &[u8],
        prefix: &[u8],
    ) -> io::Result<()> {
        let mut record = [0u8; RECORD];
        let name = if !is_pax && !glob_name.is_empty() {
            glob_name
        } else {
            &item.name
        };
        copy_string(&mut record[0..100], name);
        octal_8(&mut record[100..108], item.mode);
        octal_8(&mut record[108..116], item.uid);
        octal_8(&mut record[116..124], item.gid);
        octal_12(&mut record[124..136], item.pack_size);
        octal_12_signed(&mut record[136..148], item.mtime);
        record[156] = item.link_flag;
        copy_string(&mut record[157..257], &item.link_name);
        record[257..265].copy_from_slice(&item.magic);
        copy_string(&mut record[265..297], &item.user);
        copy_string(&mut record[297..329], &item.group);
        let need_device = self.posix && !is_pax;
        for (at, dev) in [(329, item.dev_major), (337, item.dev_minor)] {
            match dev {
                Some(v) => octal_8(&mut record[at..at + 8], v),
                None if need_device => octal_8(&mut record[at..at + 8], 0),
                None => {}
            }
        }
        if !is_pax && !prefix.is_empty() {
            copy_string(&mut record[345..345 + PREFIX_SIZE], prefix);
        }
        if item.is_sparse() {
            record[482] = u8::from(item.sparse.len() > 4);
            octal_12(&mut record[483..495], item.size);
            for (i, &(offset, size)) in item.sparse.iter().take(4).enumerate() {
                let p = 386 + 24 * i;
                octal_12(&mut record[p..p + 12], offset);
                octal_12(&mut record[p + 12..p + 24], size);
            }
        }
        let mut sum: u32 = u32::from(b' ') * 8;
        for &b in &record {
            sum += u32::from(b);
        }
        for i in (0..6).rev() {
            record[148 + i] = b'0' + u8::try_from(sum & 7).unwrap_or(0);
            sum >>= 3;
        }
        record[155] = b' ';
        self.out.write_all(&record)?;
        if item.is_sparse() {
            let mut i = 4;
            while i < item.sparse.len() {
                let mut record = [0u8; RECORD];
                let mut t = 0;
                while t < 21 && i < item.sparse.len() {
                    let (offset, size) = item.sparse[i];
                    let p = 24 * t;
                    octal_12(&mut record[p..p + 12], offset);
                    octal_12(&mut record[p + 12..p + 24], size);
                    t += 1;
                    i += 1;
                }
                record[21 * 24] = u8::from(i < item.sparse.len());
                self.out.write_all(&record)?;
            }
        }
        Ok(())
    }

    /// `WriteHeader`: the records a long name or link needs, then the item's own.
    #[expect(clippy::too_many_lines, reason = "7-Zip's WriteHeader, step by step")]
    fn header(&mut self, item: &Header) -> io::Result<()> {
        let mut glob_name = Vec::new();
        let mut prefix = Vec::new();
        let mut name_pos = 0usize;
        let mut need_path_cut = false;
        let mut allow_prefix = false;
        let name = &item.name;
        if name.len() > NAME_SIZE {
            let mut p = name.len() - 1;
            while name[p] == b'/' && p != 0 {
                p -= 1;
            }
            while p != 0 && name[p - 1] != b'/' {
                p -= 1;
            }
            name_pos = p;
            need_path_cut = true;
        }
        if self.posix {
            let mut s = Vec::new();
            if need_path_cut {
                let name_len = name.len() - name_pos;
                if (b'0'..=b'5').contains(&item.link_flag)
                    && name_pos > 1
                    && name_len != 0
                    && item.magic == MAGIC_POSIX
                {
                    allow_prefix = true;
                    if name_pos <= PREFIX_SIZE + 1 && name_len <= NAME_SIZE {
                        need_path_cut = false;
                    }
                }
                if need_path_cut {
                    pax_line(&mut s, "path", name);
                }
            }
            if item.link_name.len() > NAME_SIZE {
                pax_line(&mut s, "linkpath", &item.link_name);
            }
            if item.pack_size >= 1 << 33 {
                pax_line(&mut s, "size", item.pack_size.to_string().as_bytes());
            }
            if let Some(v) = item.dev_major {
                pax_u32_if_big(&mut s, "devmajor", v);
            }
            if let Some(v) = item.dev_minor {
                pax_u32_if_big(&mut s, "devminor", v);
            }
            pax_u32_if_big(&mut s, "uid", item.uid);
            pax_u32_if_big(&mut s, "gid", item.gid);
            let zero_mtime = item.mtime < 0 || item.mtime >= 1 << 33;
            if let Some(mtime) = item.pax_mtime {
                let need = zero_mtime || (self.digits_max > 0 && mtime.ns != 0);
                if need {
                    pax_time(&mut s, "mtime", mtime, self.digits_max);
                }
            }
            if let Some(t) = item.pax_atime {
                pax_time(&mut s, "atime", t, self.digits_max);
            }
            if let Some(t) = item.pax_ctime {
                pax_time(&mut s, "ctime", t, self.digits_max);
            }
            if item.user.len() > USER_SIZE {
                pax_line(&mut s, "uname", &item.user);
            }
            if item.group.len() > USER_SIZE {
                pax_line(&mut s, "gname", &item.group);
            }
            if !s.is_empty() {
                let mi = Header {
                    name: b"PaxHeader/@PaxHeader".to_vec(),
                    link_name: Vec::new(),
                    user: Vec::new(),
                    group: Vec::new(),
                    uid: 0,
                    gid: 0,
                    dev_major: None,
                    dev_minor: None,
                    mode: 0o644,
                    mtime: if zero_mtime { 0 } else { item.mtime },
                    link_flag: b'x',
                    pack_size: s.len() as u64,
                    ..item.clone()
                };
                self.header_real(&mi, true, &[], &[])?;
                self.write_data_and_residual(&s)?;
            }
        } else if name.len() > NAME_SIZE || item.link_name.len() > NAME_SIZE {
            let mut mi = Header {
                link_name: Vec::new(),
                name: b"././@LongLink".to_vec(),
                mode: 0o644,
                mtime: 0,
                user: Vec::new(),
                group: Vec::new(),
                uid: 0,
                gid: 0,
                dev_major: None,
                dev_minor: None,
                ..item.clone()
            };
            for (flag, long) in [(b'K', &item.link_name), (b'L', &item.name)] {
                if long.len() <= NAME_SIZE {
                    continue;
                }
                mi.link_flag = flag;
                mi.pack_size = long.len() as u64 + 1;
                self.header_real(&mi, false, &[], &[])?;
                let mut data = long.clone();
                data.push(0);
                self.write_data_and_residual(&data)?;
            }
        }
        if name.len() > NAME_SIZE {
            let name_len = name.len() - name_pos;
            if need_path_cut {
                glob_name = b"@PathCut/_pc_".to_vec();
                if name_pos == 0 {
                    glob_name.extend_from_slice(b"root");
                } else {
                    let crc = crc32fast::hash(&name[..name_pos - 1]);
                    glob_name.extend_from_slice(format!("crc32/{crc:08X}").as_bytes());
                }
                if !allow_prefix || glob_name.len() + 1 + name_len <= NAME_SIZE {
                    glob_name.push(b'/');
                } else {
                    prefix = std::mem::take(&mut glob_name);
                }
            } else {
                prefix = name[..name_pos - 1].to_vec();
            }
            glob_name.extend_from_slice(&name[name_pos..]);
        }
        self.header_real(item, false, &glob_name, &prefix)
    }
}

/// `CompareUpdateItems`, for an archive with pax items: what is kept as it was first, in
/// the archive's order, then the rest in the update's.
fn pax_order(items: &[Item]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        let (ua, ub) = (&items[a].up, &items[b].up);
        match (ua.new_props, ub.new_props) {
            (false, false) => ua.arc.cmp(&ub.arc),
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            (true, true) => a.cmp(&b),
        }
    });
    order
}

/// The tar handler's `UpdateItems` and `UpdateArchive`; returns the files read.
pub(super) fn write<SE: cash_core::ShellExtensions>(
    job: &Job<'_, '_, SE>,
    opened: Option<&Opened>,
    items: &[Item],
    out: &mut Out<'_>,
    warnings: &mut Warnings,
    processed: &mut [bool],
) -> Result<u64, Stop> {
    let settings = Settings::parse(&job.options.properties)?;
    let tar: Option<&Tar> = opened.and_then(Opened::tar);
    if tar.is_some_and(|t| t.faulty) {
        return Err(Stop::System(win_error(win::E_NOTIMPL)));
    }
    let order = if tar.is_some_and(|t| t.pax_items) {
        pax_order(items)
    } else {
        (0..items.len()).collect()
    };
    let mut files_read = 0u64;
    let mut w = TarOut {
        out,
        posix: settings.posix,
        digits_max: settings.digits_max,
    };
    for i in order {
        let ui = &items[i];
        let old = ui.up.arc.and_then(|a| tar.and_then(|t| t.items.get(a)));
        let mut item = if ui.up.new_props {
            new_props_header(job, ui, old, &settings)
        } else if let Some(old) = old {
            Header::of_old(old)
        } else {
            continue;
        };
        // An old symbolic link keeps its target when its properties are new.
        let symlink = if ui.up.new_props && ui.up.dir.is_none() {
            old.filter(|o| o.is_symlink() && !o.link_name.is_empty())
                .map(|o| o.link_name.clone())
        } else {
            None
        };
        if let Some(target) = &symlink {
            item.link_flag = b'2';
            item.link_name.clone_from(target);
        }
        if ui.up.new_data {
            item.sparse.clear();
            item.pack_size = ui.size;
            item.size = ui.size;
            let Some(d) = ui.up.dir else {
                continue;
            };
            let di = &job.dir_items[d];
            job.announce(ui);
            let mut file = None;
            let mut need_write = true;
            if di.is_dir {
                item.pack_size = 0;
                item.size = 0;
            } else {
                match job.open_new(d, warnings) {
                    Some(mut input) => {
                        let size = input.size_or(di.size);
                        if size == UNKNOWN_SIZE {
                            return Err(Stop::System(win_error(win::E_INVALIDARG)));
                        }
                        item.pack_size = size;
                        item.size = size;
                        file = Some(input);
                    }
                    None => need_write = false,
                }
            }
            if need_write {
                write_new(&mut w, &item, file)?;
                processed[d] = true;
                files_read += 1;
            }
        } else if let Some(old) = old {
            let tar = tar.ok_or_else(|| Stop::System(win_error(win::E_FAIL)))?;
            let (size, pos) = if ui.up.new_props {
                if symlink.is_some() {
                    item.pack_size = 0;
                    item.size = 0;
                } else {
                    if ui.is_dir == old.is_dir() {
                        item.link_flag = old.link_flag;
                    }
                    item.sparse.clone_from(&old.sparse);
                    item.size = old.size;
                    item.pack_size = old.pack_size;
                }
                item.dev_major = old.dev_major;
                item.dev_minor = old.dev_minor;
                item.uid = old.uid;
                item.gid = old.gid;
                w.header(&item)?;
                (old.pack_size_aligned(), old.data_pos())
            } else {
                (old.header_size + old.pack_size_aligned(), old.header_pos)
            };
            copy_old(tar, pos, size, &mut *w.out)?;
        }
    }
    w.out.write_all(&[0u8; RECORD * 2])?;
    Ok(files_read)
}

/// The header of an item whose properties are new: the disk's, or the old item's under
/// its new name.
fn new_props_header<SE: cash_core::ShellExtensions>(
    job: &Job<'_, '_, SE>,
    ui: &Item,
    old: Option<&TarItem>,
    settings: &Settings,
) -> Header {
    let mut name = ui.name.replace('\\', "/").into_bytes();
    if ui.is_dir && !name.is_empty() && name.last() != Some(&b'/') {
        name.push(b'/');
    }
    let disk = ui.up.dir.map(|d| &job.dir_items[d]);
    let from_old = disk.is_none();
    // kpidPosixAttrib: the disk's 777 (555 read-only), or the old item's mode.
    let mode = match (disk, old) {
        (Some(di), _) => {
            let kind = if di.is_dir { 0o040_000 } else { 0o100_000 };
            kind | if di.attrib & 1 != 0 && !di.is_dir {
                0o555
            } else {
                0o777
            }
        }
        (None, Some(o)) => o.combined_mode(),
        (None, None) => 0o777 | if ui.is_dir { 0o040_000 } else { 0o100_000 },
    };
    let time = |ticks: Option<u64>, old_time: Option<PaxTime>| {
        if from_old {
            old_time
        } else {
            ticks.map(PaxTime::of_ticks)
        }
    };
    let old_pax = |t: Option<super::super::tar7::PaxTime>| {
        t.map(|t| PaxTime {
            sec: t.sec,
            ns: t.ns,
            digits: u32::try_from(t.digits.unwrap_or(0)).unwrap_or(0),
        })
    };
    // An old item's time without a pax record is Unix seconds: seven digits, no more.
    let old_mtime = old.map(|o| {
        old_pax(o.mtime_pax).unwrap_or(PaxTime {
            sec: o.mtime,
            ns: 0,
            digits: 7,
        })
    });
    let pax_mtime = if settings.mtime {
        time(ui.modified, old_mtime)
    } else {
        None
    };
    let pax_atime = if settings.atime {
        time(ui.accessed, old.and_then(|o| old_pax(o.atime_pax)))
    } else {
        None
    };
    let pax_ctime = if settings.ctime {
        time(ui.created, old.and_then(|o| old_pax(o.ctime_pax)))
    } else {
        None
    };
    let mut item = Header {
        name,
        mode: mode & !0o170_000,
        magic: if settings.posix {
            MAGIC_POSIX
        } else {
            MAGIC_GNU
        },
        pax_mtime,
        pax_atime,
        pax_ctime,
        mtime: pax_mtime.map_or(0, |t| t.sec),
        ..Header::default()
    };
    if let Some(o) = old.filter(|_| from_old) {
        item.user.clone_from(&o.user);
        item.group.clone_from(&o.group);
        item.uid = o.uid;
        item.gid = o.gid;
        if settings.posix {
            item.dev_major = o.dev_major;
            item.dev_minor = o.dev_minor;
        }
    }
    if ui.is_dir {
        item.link_flag = b'5';
        item.pack_size = 0;
    } else {
        item.pack_size = ui.size;
        item.set_link_flag_for_file(mode);
    }
    item
}

/// A new item's header and data; when the file's size changed while it was read, the
/// header is written again with the size it had.
fn write_new(w: &mut TarOut<&mut Out<'_>>, item: &Header, file: Option<Input>) -> Result<(), Stop> {
    let header_pos = w.out.position();
    w.header(item)?;
    let Some(mut file) = file else {
        return Ok(());
    };
    let mut total = 0u64;
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(Stop::System(e)),
        };
        w.out.write_all(&buf[..n])?;
        total += n as u64;
    }
    w.residual(total)?;
    if total != item.pack_size {
        let mut fixed = item.clone();
        fixed.pack_size = total;
        let mut again = TarOut {
            out: Vec::new(),
            posix: w.posix,
            digits_max: w.digits_max,
        };
        again.header(&fixed)?;
        let header = again.out;
        let data_pos = header_pos + header.len() as u64;
        if data_pos + total.div_ceil(RECORD as u64) * RECORD as u64 != w.out.position()
            || !w.out.rewrite_at(header_pos, &header)?
        {
            return Err(Stop::System(win_error(win::E_FAIL)));
        }
    }
    Ok(())
}

/// Copies `size` bytes of the old archive from `pos`.
fn copy_old(tar: &Tar, pos: u64, size: u64, out: &mut Out<'_>) -> Result<(), Stop> {
    use std::io::Seek;
    if size == 0 {
        return Ok(());
    }
    let mut file = File::open(&tar.path).map_err(Stop::System)?;
    file.seek(io::SeekFrom::Start(pos)).map_err(Stop::System)?;
    let copied = io::copy(&mut file.take(size), out).map_err(Stop::System)?;
    if copied != size {
        return Err(Stop::System(win_error(win::E_FAIL)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pax_lines_count_their_own_length() {
        let mut s = Vec::new();
        pax_line(&mut s, "path", b"abc");
        assert_eq!(s, b"12 path=abc\n");
        let mut s = Vec::new();
        pax_line(&mut s, "path", &[b'a'; 95]);
        assert_eq!(&s[..4], b"105 ");
        assert_eq!(s.len(), 105);
    }

    #[test]
    fn pax_times_keep_the_digits_asked_for() {
        let t = PaxTime {
            sec: 5,
            ns: 123_456_789,
            digits: 9,
        };
        let mut s = Vec::new();
        pax_time(&mut s, "mtime", t, 3);
        assert_eq!(s, b"15 mtime=5.123\n");
        let mut s = Vec::new();
        pax_time(
            &mut s,
            "atime",
            PaxTime {
                sec: -2,
                ns: 300_000_000,
                digits: 1,
            },
            7,
        );
        assert_eq!(s, b"14 atime=-1.7\n");
    }

    #[test]
    fn numbers_too_big_for_octal_go_binary() {
        let mut f = [0u8; 12];
        octal_12(&mut f, 5);
        assert_eq!(&f, b"00000000005\0");
        octal_12(&mut f, 1 << 33);
        assert_eq!(f[0], 0x80);
        assert_eq!(&f[4..], &(1u64 << 33).to_be_bytes());
        let mut f = [0u8; 12];
        octal_12_signed(&mut f, -1);
        assert_eq!(&f[..4], &[0xFF; 4]);
    }

    #[test]
    fn settings_take_7_zip_names() {
        let props = |list: &[(&str, Option<&str>)]| -> Vec<(String, Option<String>)> {
            list.iter()
                .map(|(n, v)| ((*n).to_owned(), v.map(str::to_owned)))
                .collect()
        };
        let s = Settings::parse(&props(&[("m", Some("pax")), ("tp", Some("1"))])).unwrap();
        assert!(s.posix);
        assert_eq!(s.digits_max, 0);
        assert!(Settings::parse(&props(&[("m", Some("ustar"))])).is_err());
        assert!(Settings::parse(&props(&[("x9", None), ("mt4", None)])).is_ok());
        assert!(Settings::parse(&props(&[("d", Some("24"))])).is_err());
    }
}
