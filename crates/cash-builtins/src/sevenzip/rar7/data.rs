//! The data of RAR items, decoded as 7-Zip 26.03's handlers decode it (`Extract` in
//! `Rar5Handler.cpp` and `RarHandler.cpp`): the items a solid stream needs before the
//! ones wanted, each item's packed parts read across its volumes with their own checks
//! (`CVolsInStream`), decrypted when they are, and decoded by cash-archive's `rar`.
//!
//! rar's decoders write what they decode; 7z reads each item's data. A worker thread
//! decodes the items in order and hands their bytes over a channel, so that an item is
//! never held whole in memory.

use std::fs::File;
use std::io::{self, Read};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

use cash_archive::rar::codec::rar50::Unpack50Decoder;
use cash_archive::rar::crypto::rar20::Rar20Cipher;
use cash_archive::rar::crypto::rar30::Rar30Cipher;
use cash_archive::rar::crypto::rar50::{Rar50Cipher, Rar50Keys};
use cash_archive::rar::rar50::blake2sp;
use cash_archive::sevenz::Problem;

use super::super::archive::Data;
use super::{rar4, rar5};
use crate::rardata::{
    self, CHUNK, Cipher, DecodeError, Decoder4, DecryptReader, Part, PartCheck, Sink, VolsReader,
};

/// What the worker says about the item it is on.
pub(super) enum Message {
    Bytes(Vec<u8>),
    Done(Result<(), Problem>),
}

/// An item to decode: its index, and whether it is one wanted (else it is decoded for
/// the solid stream's sake).
#[derive(Clone, Copy)]
pub(super) struct Step {
    pub(super) index: usize,
    pub(super) wanted: bool,
}

/// What an item decodes to: its checksums taken, the bytes sent on in chunks, none past
/// the size where the format stops there (RAR 5's `COutStreamWithHash`).
struct Out<'a> {
    tx: &'a SyncSender<Message>,
    limit: Option<u64>,
    written: u64,
    crc: crc32fast::Hasher,
    blake: Option<blake2sp::Hasher>,
    pending: Vec<u8>,
}

impl<'a> Out<'a> {
    fn new(tx: &'a SyncSender<Message>, limit: Option<u64>, blake: bool) -> Self {
        Self {
            tx,
            limit,
            written: 0,
            crc: crc32fast::Hasher::new(),
            blake: blake.then(blake2sp::Hasher::new),
            pending: Vec::with_capacity(CHUNK),
        }
    }

    fn put(&mut self, data: &[u8]) -> io::Result<()> {
        let data = match self.limit {
            Some(limit) => {
                let room =
                    usize::try_from(limit.saturating_sub(self.written)).unwrap_or(usize::MAX);
                &data[..data.len().min(room)]
            }
            None => data,
        };
        self.crc.update(data);
        if let Some(blake) = &mut self.blake {
            blake.update(data);
        }
        self.written += data.len() as u64;
        self.pending.extend_from_slice(data);
        if self.pending.len() >= CHUNK {
            self.send()?;
        }
        Ok(())
    }

    fn send(&mut self) -> io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let chunk = std::mem::replace(&mut self.pending, Vec::with_capacity(CHUNK));
        self.tx
            .send(Message::Bytes(chunk))
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))
    }
}

impl Sink for Out<'_> {
    fn put(&mut self, data: &[u8]) -> io::Result<()> {
        Out::put(self, data)
    }
}

/// Whether a decoding error is the reading side gone, or the data's own.
fn stopped(error: &DecodeError) -> bool {
    matches!(error, DecodeError::Sink(e) if e.kind() == io::ErrorKind::BrokenPipe)
}

/// An item's data on the reading side: the chunks the worker sends, then its result.
struct ChannelData<'a> {
    rx: &'a Receiver<Message>,
    buf: Vec<u8>,
    at: usize,
    result: Option<Result<(), Problem>>,
    encrypted: bool,
}

impl ChannelData<'_> {
    fn drain(&mut self) -> Result<(), Problem> {
        loop {
            if let Some(result) = self.result {
                return result;
            }
            match self.rx.recv() {
                Ok(Message::Bytes(_)) => {}
                Ok(Message::Done(result)) => self.result = Some(result),
                Err(_) => self.result = Some(Err(Problem::Data)),
            }
        }
    }
}

impl Read for ChannelData<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.at == self.buf.len() {
            if self.result.is_some() {
                return Ok(0);
            }
            match self.rx.recv() {
                Ok(Message::Bytes(chunk)) => {
                    self.buf = chunk;
                    self.at = 0;
                }
                Ok(Message::Done(result)) => self.result = Some(result),
                Err(_) => self.result = Some(Err(Problem::Data)),
            }
        }
        let n = out.len().min(self.buf.len() - self.at);
        out[..n].copy_from_slice(&self.buf[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

impl Data for ChannelData<'_> {
    fn finish(&mut self) -> Result<(), Problem> {
        self.drain()
    }

    fn encrypted(&self) -> bool {
        self.encrypted
    }
}

/// Decodes `steps` on a worker thread and hands each wanted item's data to `each`, in
/// order; stops where `each` says to.
pub(super) fn run<E: From<io::Error>>(
    steps: &[Step],
    encrypted: &dyn Fn(usize) -> bool,
    work: impl FnOnce(&[Step], &SyncSender<Message>) + Send,
    mut each: impl FnMut(usize, &mut dyn Data) -> Result<bool, E>,
) -> Result<(), E> {
    std::thread::scope(|scope| {
        let (tx, rx) = sync_channel(4);
        scope.spawn(move || work(steps, &tx));
        for step in steps {
            let mut data = ChannelData {
                rx: &rx,
                buf: Vec::new(),
                at: 0,
                result: None,
                encrypted: encrypted(step.index),
            };
            if step.wanted {
                let go_on = each(step.index, &mut data)?;
                let _ = data.drain();
                if !go_on {
                    break;
                }
            } else {
                let _ = data.drain();
            }
        }
        Ok(())
    })
}

// ---------- RAR 5 ----------

/// `Extract`'s first pass: the items wanted, and the solid ones before each that its
/// stream needs.
pub(super) fn plan5(rar: &rar5::Rar5, wanted: &dyn Fn(usize) -> bool) -> Vec<Step> {
    let count = rar.refs.len();
    let mut status = vec![0u8; count];
    let mut solid_limit = 0;
    for index in (0..count).filter(|&i| wanted(i)) {
        status[index] |= 1;
        let item = &rar.items[rar.refs[index].item];
        if item.is_service() {
            continue;
        }
        if item.is_solid() {
            let mut j = index;
            while j > solid_limit {
                j -= 1;
                let item2 = &rar.items[rar.refs[j].item];
                if !item2.is_service() {
                    status[j] |= 2;
                    if !item2.is_solid() {
                        break;
                    }
                }
            }
        }
        solid_limit = index + 1;
    }
    status
        .iter()
        .enumerate()
        .filter(|(_, s)| **s != 0)
        .map(|(index, s)| Step {
            index,
            wanted: s & 1 != 0,
        })
        .collect()
}

/// The keys of a file's encryption record, made once for each salt and count.
struct KeyCache {
    salt: [u8; 16],
    count: u8,
    keys: Rar50Keys,
    ok: bool,
}

/// The worker for RAR 5: each step's item decoded and sent.
#[expect(
    clippy::too_many_lines,
    reason = "7-Zip's Extract and CUnpacker, item by item"
)]
pub(super) fn work5(
    rar: &rar5::Rar5,
    password: Option<&str>,
    steps: &[Step],
    tx: &SyncSender<Message>,
) {
    let mut files: Vec<Option<File>> = Vec::new();
    // One decoder for files, one for service streams, as 7-Zip keeps them.
    let mut decoders: [Option<Unpack50Decoder>; 2] = [None, None];
    let mut solid_allowed = false;
    let mut cache: Option<KeyCache> = None;
    for step in steps {
        let r = rar.refs[step.index];
        let item = &rar.items[r.item];
        let last = &rar.items[r.last];
        let mut is_solid = false;
        if !item.is_service() {
            if item.is_solid() {
                is_solid = solid_allowed;
            }
            solid_allowed = is_solid;
        }
        let done = |result| tx.send(Message::Done(result)).is_ok();
        if item.is_dir() {
            if !done(Ok(())) {
                return;
            }
            continue;
        }
        if item.pack_size == 0 && item.is_link_with_no_data() {
            if !done(if item.is_copy_link() {
                Err(Problem::UnsupportedMethod)
            } else {
                Ok(())
            }) {
                return;
            }
            continue;
        }
        // `CUnpacker::Create`.
        if item.algo_raw() > 1 || item.method_number() > 5 || item.algo_huff_rev() > 1 {
            if !done(Err(Problem::UnsupportedMethod)) {
                return;
            }
            continue;
        }
        let (mac, mut cipher) = if let Some(record) = item.crypto_record() {
            let Some(props) = rar5::CryptoProps::parse(record, true) else {
                if !done(Err(Problem::UnsupportedMethod)) {
                    return;
                }
                continue;
            };
            let Some(password) = password else {
                if !done(Err(Problem::WrongPassword)) {
                    return;
                }
                continue;
            };
            let fresh = cache
                .as_ref()
                .is_none_or(|c| c.salt != props.salt || c.count != props.count);
            if fresh {
                let Some((keys, ok)) = rar5::derive_keys(
                    props.check,
                    props.salt,
                    props.count,
                    &rar5::password_bytes(password),
                ) else {
                    if !done(Err(Problem::UnsupportedMethod)) {
                        return;
                    }
                    continue;
                };
                cache = Some(KeyCache {
                    salt: props.salt,
                    count: props.count,
                    keys,
                    ok,
                });
            }
            let Some(entry) = cache.as_ref() else {
                unreachable!("the keys were made above");
            };
            let ok = match props.check {
                Some(check) => rar5::check_matches(&entry.keys, check),
                None => entry.ok,
            };
            if !ok {
                if !done(Err(Problem::WrongPassword)) {
                    return;
                }
                continue;
            }
            let mac = (props.flags & 2 != 0).then(|| entry.keys.clone());
            let cipher = props
                .iv
                .map(|iv| Cipher::Rar5(Box::new(Rar50Cipher::new(entry.keys.key, iv))));
            (mac, cipher)
        } else {
            (None, None)
        };
        // The whole item's checksums are the last part's, through the MAC when that
        // part's own record says so.
        let mac = if r.last == r.item {
            mac
        } else {
            last_mac(last, password, &mut cache)
        };
        // `CUnpacker::Code`.
        let size = (!last.is_unknown_size()).then_some(last.size);
        let mut out = Out::new(tx, size, last.blake_offset().is_some());
        let parts = parts5(rar, r.item);
        let mut vols = VolsReader::new(&rar.volumes, &mut files, parts);
        let pack_size = rar.pack_size(&r);
        let mut decoded: Result<(), Problem> = Ok(());
        let mut halted = false;
        if pack_size != 0 || size.is_none_or(|s| s != 0) {
            let method = item.method_number();
            let result = {
                let mut input: Box<dyn Read> = match cipher.take() {
                    Some(cipher) => Box::new(DecryptReader::new(&mut vols, cipher)),
                    None => Box::new(&mut vols),
                };
                if method == 0 {
                    rardata::copy(&mut input, &mut out)
                } else {
                    let Some(output_size) = size.and_then(|s| usize::try_from(s).ok()) else {
                        decoded = Err(Problem::UnsupportedMethod);
                        drop(input);
                        if !done(decoded) {
                            return;
                        }
                        continue;
                    };
                    let dict = usize::try_from(item.dict_size()).unwrap_or(usize::MAX);
                    let slot = usize::from(item.is_service());
                    let decoder = decoders[slot].get_or_insert_with(Unpack50Decoder::new);
                    rardata::decode5(
                        decoder,
                        &mut input,
                        u8::try_from(item.algo_huff_rev()).unwrap_or(0),
                        output_size,
                        dict,
                        is_solid,
                        &mut out,
                    )
                }
            };
            if !item.is_service() {
                solid_allowed = true;
            }
            match result {
                Ok(()) => {}
                Err(error) if stopped(&error) => halted = true,
                Err(_) => decoded = Err(Problem::Data),
            }
            if decoded.is_ok()
                && let Some(size) = size
                && out.written != size
                && !(pack_size == 0 && method == 0 && last.is_unix_symlink())
            {
                decoded = Err(Problem::Data);
            }
        }
        if halted || out.send().is_err() {
            return;
        }
        let vols_ok = vols.crc_ok;
        let mut result = decoded;
        if result.is_ok() {
            let crc_ok = hash_ok(
                last,
                out.crc.clone().finalize(),
                out.blake.take(),
                mac.as_ref(),
            );
            if !crc_ok || !vols_ok {
                result = Err(Problem::Crc);
            }
        }
        if !done(result) {
            return;
        }
    }
}

/// The MAC keys of a split item's last part, when its encryption record asks for the MAC.
fn last_mac(
    last: &rar5::RarItem,
    password: Option<&str>,
    cache: &mut Option<KeyCache>,
) -> Option<Rar50Keys> {
    let props = rar5::CryptoProps::parse(last.crypto_record()?, true)?;
    if props.flags & 2 == 0 {
        return None;
    }
    if let Some(entry) = cache
        .as_ref()
        .filter(|c| c.salt == props.salt && c.count == props.count)
    {
        return Some(entry.keys.clone());
    }
    let (keys, ok) = rar5::derive_keys(
        props.check,
        props.salt,
        props.count,
        &rar5::password_bytes(password?),
    )?;
    *cache = Some(KeyCache {
        salt: props.salt,
        count: props.count,
        keys: keys.clone(),
        ok,
    });
    Some(keys)
}

/// An item's parts, from its first header through those that continue it.
fn parts5(rar: &rar5::Rar5, first: usize) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut index = Some(first);
    while let Some(i) = index {
        let item = &rar.items[i];
        let check = item.is_split_after().then(|| PartCheck::Hashes {
            crc: item.has_crc().then_some(item.crc),
            blake: item
                .blake_offset()
                .and_then(|at| item.extra.get(at..at + 32))
                .and_then(|d| <[u8; 32]>::try_from(d).ok()),
        });
        parts.push(Part {
            volume: item.vol_index,
            pos: item.data_pos,
            size: item.pack_size,
            check,
        });
        index = item.next_item;
    }
    parts
}

/// `CHash::Check` on the whole item: its CRC and `BLAKE2sp`, through the MAC key when the
/// encryption record says so.
fn hash_ok(
    last: &rar5::RarItem,
    crc: u32,
    blake: Option<blake2sp::Hasher>,
    mac: Option<&Rar50Keys>,
) -> bool {
    if last.has_crc() {
        let crc = mac.map_or(crc, |keys| keys.mac_crc32(crc));
        if crc != last.crc {
            return false;
        }
    }
    if let (Some(at), Some(hasher)) = (last.blake_offset(), blake) {
        let digest = hasher.finalize();
        let digest = mac.map_or(digest, |keys| keys.mac_hash32(digest));
        if last.extra.get(at..at + 32) != Some(&digest[..]) {
            return false;
        }
    }
    true
}

// ---------- RAR 1.5 to 4 ----------

/// `Extract`'s first pass: from each wanted item back to the start of its solid stream.
pub(super) fn plan4(rar: &rar4::Rar4, wanted: &dyn Fn(usize) -> bool) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut last_index = 0;
    for index in (0..rar.refs.len()).filter(|&i| wanted(i)) {
        last_index = (last_index..=index)
            .rev()
            .find(|&j| !rar.is_solid(j))
            .unwrap_or(last_index);
        for j in last_index..=index {
            steps.push(Step {
                index: j,
                wanted: j == index,
            });
        }
        last_index = index + 1;
    }
    steps
}

/// The worker for RAR 1.5 to 4.
#[expect(clippy::too_many_lines, reason = "7-Zip's Extract, item by item")]
pub(super) fn work4(
    rar: &rar4::Rar4,
    password: Option<&str>,
    steps: &[Step],
    tx: &SyncSender<Message>,
) {
    let mut files: Vec<Option<File>> = Vec::new();
    let mut decoders: Vec<(u8, Decoder4)> = Vec::new();
    let mut solid_start = true;
    for step in steps {
        let index = step.index;
        let r = rar.refs[index];
        let item = &rar.items[r.item];
        let last = &rar.items[r.item + r.count - 1];
        let done = |result| tx.send(Message::Done(result)).is_ok();
        if !rar.is_solid(index) {
            solid_start = true;
        }
        if item.is_dir() {
            if !done(Ok(())) {
                return;
            }
            continue;
        }
        let mut cipher = None;
        if item.is_encrypted() {
            let Some(password) = password else {
                if !done(Err(Problem::UnsupportedMethod)) {
                    return;
                }
                continue;
            };
            if item.unpack_version >= 29 {
                let salt = item.has_salt().then_some(item.salt);
                let Ok(c) = Rar30Cipher::new(rar4::password_utf8(password).as_bytes(), salt) else {
                    if !done(Err(Problem::UnsupportedMethod)) {
                        return;
                    }
                    continue;
                };
                cipher = Some(Cipher::Rar3(Box::new(c)));
            } else if item.unpack_version >= 20 {
                cipher = Some(Cipher::Rar2(Box::new(Rar20Cipher::new(
                    password.as_bytes(),
                ))));
            } else {
                if !done(Err(Problem::UnsupportedMethod)) {
                    return;
                }
                continue;
            }
        }
        let size = last.is_size_defined().then_some(last.size);
        let mut out = Out::new(tx, None, false);
        let parts: Vec<Part> = (r.item..r.item + r.count)
            .enumerate()
            .map(|(n, i)| {
                let part = &rar.items[i];
                Part {
                    volume: r.volume + n,
                    pos: part.data_pos(),
                    size: part.pack_size,
                    check: part
                        .is_split_after()
                        .then_some(PartCheck::Crc(part.file_crc)),
                }
            })
            .collect();
        let mut vols = VolsReader::new(&rar.volumes, &mut files, parts);
        let result = {
            let mut input: Box<dyn Read> = match cipher.take() {
                Some(cipher) => Box::new(DecryptReader::new(&mut vols, cipher)),
                None => Box::new(&mut vols),
            };
            match item.method {
                b'0' => match size {
                    Some(size) => rardata::copy(&mut (&mut input).take(size), &mut out),
                    None => rardata::copy(&mut input, &mut out),
                },
                b'1'..=b'5' => {
                    let version = item.unpack_version;
                    let is_solid = (rar.is_solid(index) || item.is_split_before()) && !solid_start;
                    solid_start = false;
                    // 7-Zip's decoders read the tables first, so an item with no packed
                    // bytes is a data error even when it is empty.
                    if version > 40 || rar.pack_size(&r) == 0 {
                        Err(DecodeError::Data)
                    } else {
                        let output_size = size.and_then(|s| usize::try_from(s).ok()).unwrap_or(0);
                        rardata::decode4(
                            &mut decoders,
                            version,
                            is_solid,
                            &mut input,
                            output_size,
                            &mut out,
                            false,
                        )
                    }
                }
                _ => {
                    drop(input);
                    if !done(Err(Problem::UnsupportedMethod)) {
                        return;
                    }
                    continue;
                }
            }
        };
        let unsupported = item.method != b'0' && item.unpack_version > 40;
        let decoded = match result {
            Ok(()) => Ok(()),
            Err(error) if stopped(&error) => return,
            Err(_) if unsupported => Err(Problem::UnsupportedMethod),
            Err(_) => Err(Problem::Data),
        };
        if out.send().is_err() {
            return;
        }
        let crc_ok = vols.crc_ok && out.crc.clone().finalize() == last.file_crc;
        let result = match decoded {
            Ok(()) if crc_ok => Ok(()),
            Ok(()) => Err(Problem::Crc),
            Err(problem) => Err(problem),
        };
        if !done(result) {
            return;
        }
    }
}
