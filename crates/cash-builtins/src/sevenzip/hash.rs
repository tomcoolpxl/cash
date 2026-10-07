//! `h` and `-scrc`: 7-Zip's hash bundle (`HashCalc.cpp`) and how the console shows it
//! (`HashCon.cpp`).
//!
//! Each file's data goes through every hash `-scrc` names (CRC32 unless it names
//! some); its digests are summed "for data", and a hash of a 16-byte prefix (1 first
//! for a folder), the digest and the name in UTF-16 is summed "for data and names".
//! The sums are added as little-endian numbers, what overflows counted after them.

use std::fmt::Write as _;
use std::fs::File;
use std::hash::Hasher as _;
use std::io::{self, Read};

use sha2::Digest as _;

use super::cmdline::Options;
use super::scan::{self, Stat};
use super::update::{Warnings, common_error, copy_error, warnings_check};
use super::{Console, Env, Stop};

/// The digests' largest size, and the bytes kept for what a sum overflows.
const DIGEST_MAX: usize = 64;
const EXTRA: usize = 8;

/// The digest groups (`k_HashCalc_Index_*`).
const CURRENT: usize = 0;
const DATA_SUM: usize = 1;
const NAMES_SUM: usize = 2;
const STREAMS_SUM: usize = 3;

/// 7-Zip's hashes, in the order of their ids, which is the order of the columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Algo {
    Crc32,
    Crc64,
    Sha256,
    Sha1,
    Blake2sp,
    Md5,
    Xxh64,
    Sha384,
    Sha512,
    Sha3_256,
}

const ALL: [Algo; 10] = [
    Algo::Crc32,
    Algo::Crc64,
    Algo::Sha256,
    Algo::Sha1,
    Algo::Blake2sp,
    Algo::Md5,
    Algo::Xxh64,
    Algo::Sha384,
    Algo::Sha512,
    Algo::Sha3_256,
];

impl Algo {
    const fn name(self) -> &'static str {
        match self {
            Self::Crc32 => "CRC32",
            Self::Crc64 => "CRC64",
            Self::Sha256 => "SHA256",
            Self::Sha1 => "SHA1",
            Self::Blake2sp => "BLAKE2sp",
            Self::Md5 => "MD5",
            Self::Xxh64 => "XXH64",
            Self::Sha384 => "SHA384",
            Self::Sha512 => "SHA512",
            Self::Sha3_256 => "SHA3-256",
        }
    }

    const fn size(self) -> usize {
        match self {
            Self::Crc32 => 4,
            Self::Crc64 | Self::Xxh64 => 8,
            Self::Md5 => 16,
            Self::Sha1 => 20,
            Self::Sha256 | Self::Blake2sp | Self::Sha3_256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
        }
    }

    fn by_name(name: &str) -> Option<Self> {
        ALL.into_iter()
            .find(|a| a.name().eq_ignore_ascii_case(name))
    }

    /// The column's width: the digest in hex, at least eight.
    const fn width(self) -> usize {
        let w = self.size() * 2;
        if w < 8 { 8 } else { w }
    }
}

static CRC64: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_XZ);

/// A hash under way.
enum State {
    Crc32(crc32fast::Hasher),
    Crc64(crc::Digest<'static, u64>),
    Sha256(sha2::Sha256),
    Sha1(sha1::Sha1),
    Blake2sp(Box<blake2s_simd::blake2sp::State>),
    Md5(md5::Md5),
    Xxh64(twox_hash::XxHash64),
    Sha384(sha2::Sha384),
    Sha512(sha2::Sha512),
    Sha3_256(sha3::Sha3_256),
}

impl State {
    fn new(algo: Algo) -> Self {
        match algo {
            Algo::Crc32 => Self::Crc32(crc32fast::Hasher::new()),
            Algo::Crc64 => Self::Crc64(CRC64.digest()),
            Algo::Sha256 => Self::Sha256(sha2::Sha256::new()),
            Algo::Sha1 => Self::Sha1(sha1::Sha1::new()),
            Algo::Blake2sp => Self::Blake2sp(Box::new(blake2s_simd::blake2sp::State::new())),
            Algo::Md5 => Self::Md5(md5::Md5::new()),
            Algo::Xxh64 => Self::Xxh64(twox_hash::XxHash64::with_seed(0)),
            Algo::Sha384 => Self::Sha384(sha2::Sha384::new()),
            Algo::Sha512 => Self::Sha512(sha2::Sha512::new()),
            Algo::Sha3_256 => Self::Sha3_256(sha3::Sha3_256::new()),
        }
    }

    fn update(&mut self, data: &[u8]) {
        match self {
            Self::Crc32(h) => h.update(data),
            Self::Crc64(h) => h.update(data),
            Self::Sha256(h) => h.update(data),
            Self::Sha1(h) => h.update(data),
            Self::Blake2sp(h) => {
                h.update(data);
            }
            Self::Md5(h) => h.update(data),
            Self::Xxh64(h) => h.write(data),
            Self::Sha384(h) => h.update(data),
            Self::Sha512(h) => h.update(data),
            Self::Sha3_256(h) => h.update(data),
        }
    }

    /// The digest as 7-Zip's hasher writes it: the checksums little-endian.
    fn finish(self) -> Vec<u8> {
        match self {
            Self::Crc32(h) => h.finalize().to_le_bytes().to_vec(),
            Self::Crc64(h) => h.finalize().to_le_bytes().to_vec(),
            Self::Sha256(h) => h.finalize().to_vec(),
            Self::Sha1(h) => h.finalize().to_vec(),
            Self::Blake2sp(h) => h.finalize().as_bytes().to_vec(),
            Self::Md5(h) => h.finalize().to_vec(),
            Self::Xxh64(h) => h.finish().to_le_bytes().to_vec(),
            Self::Sha384(h) => h.finalize().to_vec(),
            Self::Sha512(h) => h.finalize().to_vec(),
            Self::Sha3_256(h) => h.finalize().to_vec(),
        }
    }
}

/// One hash and its digest groups (`CHasherState`).
struct HasherState {
    algo: Algo,
    state: State,
    /// The current file's digest and the sums, each with its overflow after it.
    digests: [[u8; DIGEST_MAX + EXTRA]; 4],
    num_sums: [u64; 4],
}

impl HasherState {
    /// `AddDigests`: `data` added to the group, as little-endian numbers.
    fn add(&mut self, group: usize, data: &[u8]) {
        self.num_sums[group] += 1;
        let size = self.algo.size();
        let dest = &mut self.digests[group];
        let mut next = 0u32;
        for i in 0..size {
            next += u32::from(dest[i]) + u32::from(data[i]);
            dest[i] = (next & 0xFF) as u8;
            next >>= 8;
        }
        for i in 0..EXTRA {
            next += u32::from(dest[DIGEST_MAX + i]);
            dest[DIGEST_MAX + i] = (next & 0xFF) as u8;
            next >>= 8;
        }
    }

    /// `WriteToString`: the digest in hex (a checksum's as a number, in capitals), and
    /// for a sum of other than one digest, what overflowed.
    fn text(&self, group: usize) -> String {
        let digest = &self.digests[group];
        let mut s = hex(&digest[..self.algo.size()]);
        if group != CURRENT && self.num_sums[group] != 1 {
            let extra = &digest[DIGEST_MAX..];
            let used = extra.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
            s.push('-');
            s.push_str(&hex(&extra[..if used > 4 { 8 } else { 4 }]));
        }
        s
    }
}

/// `HashHexToString`: up to eight bytes as a little-endian number in capitals, longer
/// digests byte by byte in small letters.
fn hex(data: &[u8]) -> String {
    let mut s = String::new();
    if data.len() > 8 {
        for b in data {
            let _ = write!(s, "{b:02x}");
        }
    } else {
        for b in data.iter().rev() {
            let _ = write!(s, "{b:02X}");
        }
    }
    s
}

/// The hashes and what they summed (`CHashBundle`).
pub(super) struct Bundle {
    hashers: Vec<HasherState>,
    pub(super) num_dirs: u64,
    pub(super) num_files: u64,
    pub(super) files_size: u64,
    cur_size: u64,
}

impl Bundle {
    /// `SetMethods`: the hashes `-scrc` names, in their ids' order; CRC32 for none or an
    /// empty name, every one for `*`; "Not implemented" for a name 7-Zip does not know.
    pub(super) fn new(names: &[String]) -> Result<Self, Stop> {
        let mut algos: Vec<Algo> = Vec::new();
        let names: Vec<&str> = if names.is_empty() {
            vec![""]
        } else {
            names.iter().map(String::as_str).collect()
        };
        for name in names {
            let method = name.split(':').next().unwrap_or_default();
            if method == "*" {
                algos = ALL.to_vec();
                break;
            }
            let algo = if method.is_empty() {
                Algo::Crc32
            } else {
                Algo::by_name(method).ok_or_else(|| {
                    Stop::System(super::update::win_error(super::update::win::E_NOTIMPL))
                })?
            };
            if !algos.contains(&algo) {
                algos.push(algo);
            }
        }
        algos.sort();
        Ok(Self {
            hashers: algos
                .into_iter()
                .map(|algo| HasherState {
                    algo,
                    state: State::new(algo),
                    digests: [[0; DIGEST_MAX + EXTRA]; 4],
                    num_sums: [0; 4],
                })
                .collect(),
            num_dirs: 0,
            num_files: 0,
            files_size: 0,
            cur_size: 0,
        })
    }

    /// `InitForNewFile`.
    pub(super) fn start(&mut self) {
        self.cur_size = 0;
        for h in &mut self.hashers {
            h.state = State::new(h.algo);
            h.digests[CURRENT] = [0; DIGEST_MAX + EXTRA];
        }
    }

    pub(super) fn update(&mut self, data: &[u8]) {
        self.cur_size += data.len() as u64;
        for h in &mut self.hashers {
            h.state.update(data);
        }
    }

    /// `Final`: the file's digests, and the hash of its digest and name, added to the
    /// sums.
    pub(super) fn finish(&mut self, is_dir: bool, path: &str) {
        if is_dir {
            self.num_dirs += 1;
        } else {
            self.num_files += 1;
            self.files_size += self.cur_size;
        }
        let mut pre = [0u8; 16];
        if is_dir {
            pre[0] = 1;
        }
        for h in &mut self.hashers {
            let size = h.algo.size();
            let state = std::mem::replace(&mut h.state, State::new(h.algo));
            if !is_dir {
                let digest = state.finish();
                h.digests[CURRENT][..size].copy_from_slice(&digest);
                h.add(DATA_SUM, &digest);
            }
            let mut names = State::new(h.algo);
            names.update(&pre);
            let current = h.digests[CURRENT];
            names.update(&current[..size]);
            for unit in path.replace('\\', "/").encode_utf16() {
                names.update(&unit.to_le_bytes());
            }
            let digest = names.finish();
            h.add(NAMES_SUM, &digest);
            h.add(STREAMS_SUM, &digest);
        }
    }

    /// `PrintHashStat`: each hash's sum for the data, and for the names when there was
    /// more than one file or a folder.
    pub(super) fn stat(&self) -> String {
        let mut s = String::new();
        for h in &self.hashers {
            let name = h.algo.name();
            let pad = " ".repeat(6usize.saturating_sub(name.len()));
            let _ = writeln!(s, "{name}{pad} for data:              {}", h.text(DATA_SUM));
            if self.num_files != 1 || self.num_dirs != 0 {
                let _ = writeln!(
                    s,
                    "{name}{pad} for data and names:    {}",
                    h.text(NAMES_SUM)
                );
            }
            s.push('\n');
        }
        s
    }

    /// The columns' line of hashes: each digest in its column, or blanks.
    fn hash_columns(&self, group: usize, shown: bool) -> String {
        let mut s = String::new();
        for h in &self.hashers {
            if !s.is_empty() {
                s.push(' ');
            }
            let text = if shown { h.text(group) } else { String::new() };
            let _ = write!(s, "{text:<width$}", width = h.algo.width());
        }
        s
    }

    /// `PrintResultLine`: the hashes, the size in 13 columns, and the name.
    fn line(&self, group: usize, shown: bool, size: u64, name: &str) -> String {
        let mut s = self.hash_columns(group, shown);
        let size = if shown {
            size.to_string()
        } else {
            String::new()
        };
        let _ = write!(s, " {size:>13}  {name}");
        s
    }

    /// The headers' line and the line under it.
    fn header(&self) -> String {
        let mut names = String::new();
        for h in &self.hashers {
            if !names.is_empty() {
                names.push(' ');
            }
            let _ = write!(names, "{:<width$}", h.algo.name(), width = h.algo.width());
        }
        format!("{names} {:>13}  Name\n{}", "Size", self.separator())
    }

    fn separator(&self) -> String {
        let mut s = String::new();
        for h in &self.hashers {
            if !s.is_empty() {
                s.push(' ');
            }
            s.push_str(&"-".repeat(h.algo.width()));
        }
        s.push(' ');
        s.push_str(&"-".repeat(13));
        s.push_str("  ");
        s.push_str(&"-".repeat(12));
        s.push('\n');
        s
    }
}

/// `h`: 7-Zip's `HashCalc` with its console: "Scanning", the counts, a line for each
/// file and folder, the sums.
#[expect(
    clippy::too_many_lines,
    reason = "7-Zip's HashCalc and its console, step by step"
)]
pub(super) fn run<SE: cash_core::ShellExtensions>(
    options: &Options,
    env: &Env<'_, SE>,
    console: &Console<'_, SE>,
) -> Result<u8, Stop> {
    let mut warnings = Warnings::default();
    let headers = options.headers;
    let mut items = Vec::new();
    if let Some(name) = &options.stdin {
        items.push(scan::stdin_item(name, None));
    } else {
        if headers {
            console.so("Scanning\n");
        }
        console.progress_quiet(|s| {
            s.clear();
            "Scan".clone_into(&mut s.command);
        });
        items = scan::scan(
            &options.censor,
            options.symlinks.unwrap_or(false),
            &|p| env.path(p),
            &mut |path, error| {
                common_error(console, path, error, true);
                warnings.scan.push((path.to_owned(), copy_error(error)));
            },
            &mut |stat, path| super::update::scan_progress(console, stat, path),
        );
        // FinishScanning.
        console.close_progress();
        console.progress_quiet(super::percent::State::clear);
        if headers {
            let stat = Stat::of(&items);
            console.so(&format!(
                "{}\n\n",
                super::update::stat_text(stat.dirs, stat.files, stat.size)
            ));
        }
    }
    let mut bundle = Bundle::new(options.hash_methods.as_deref().unwrap_or_default())?;
    // SetTotal: the bytes found, before the header.
    if options.stdin.is_none() {
        let total = Stat::of(&items).size;
        console.progress(|s| s.total = total);
    }
    if headers {
        console.so(&bundle.header());
    }
    let mut buf = vec![0u8; 1 << 15];
    let mut done = 0u64;
    for item in &items {
        // A link -snl keeps is hashed as a file of its reparse data, a folder's too.
        let is_dir = item.is_dir && item.reparse.is_none();
        let mut input: Box<dyn Read> = if item.path.as_os_str().is_empty() {
            Box::new(env.context.stdin())
        } else if let Some(data) = &item.reparse {
            Box::new(io::Cursor::new(data.clone()))
        } else if is_dir {
            Box::new(io::empty())
        } else {
            match File::open(&item.path) {
                Ok(file) => Box::new(file),
                Err(error) => {
                    common_error(console, &item.shown, &error, true);
                    warnings
                        .failed
                        .push((item.shown.clone(), copy_error(&error)));
                    continue;
                }
            }
        };
        // GetStream: the line names the item.
        console.progress(|s| item.name.clone_into(&mut s.file_name));
        bundle.start();
        if !is_dir {
            // SetCompleted every 256 reads, the first before any.
            let mut step = 0u32;
            loop {
                if step.trailing_zeros() >= 8 {
                    console.progress(|s| s.completed = done);
                }
                step = step.wrapping_add(1);
                let n = match input.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(Stop::System(e)),
                };
                bundle.update(&buf[..n]);
                done = done.saturating_add(n as u64);
            }
        }
        let size = bundle.cur_size;
        bundle.finish(is_dir, &item.name);
        let mut shown = item.name.clone();
        if shown.is_empty() {
            shown.push_str("[Content]");
        } else if is_dir && !shown.ends_with('/') {
            shown.push('/');
        }
        console.so(&format!(
            "{}\n",
            bundle.line(CURRENT, !is_dir, size, &shown)
        ));
        console.progress(|s| s.files += 1);
        console.progress(|s| s.completed = done);
    }
    if headers {
        let mut s = bundle.separator();
        let _ = writeln!(
            s,
            "{}\n",
            bundle.line(DATA_SUM, true, bundle.files_size, "")
        );
        if bundle.num_files != 1 || bundle.num_dirs != 0 {
            if bundle.num_dirs != 0 {
                let _ = writeln!(s, "Folders: {}", bundle.num_dirs);
            }
            let _ = writeln!(s, "Files: {}", bundle.num_files);
        }
        let _ = writeln!(s, "Size: {}\n", bundle.files_size);
        s.push_str(&bundle.stat());
        console.so(&s);
    } else if warnings.scan.is_empty() && warnings.failed.is_empty() {
        return Ok(0);
    }
    Ok(warnings_check(console, &warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest_text(algo: Algo, data: &[u8]) -> String {
        let mut state = State::new(algo);
        state.update(data);
        let digest = state.finish();
        hex(&digest)
    }

    #[test]
    fn digests_read_as_7_zip_shows_them() {
        // `7z h -scrc*` of "alpha\n", 7-Zip 26.03.
        let data = b"alpha\n";
        assert_eq!(digest_text(Algo::Crc32, data), "9F606EEC");
        assert_eq!(digest_text(Algo::Crc64, data), "F29D99B8323EABCD");
        assert_eq!(digest_text(Algo::Xxh64, data), "E56631E04077C052");
        assert_eq!(
            digest_text(Algo::Blake2sp, data),
            "feba9b5e9bec38f4e3a61e028722f2c9ae849044fa66473460424d0902864922"
        );
        assert_eq!(
            digest_text(Algo::Md5, data),
            "9f9f90dbe3e5ee1218c86b8839db1995"
        );
        assert_eq!(
            digest_text(Algo::Sha3_256, data),
            "78ba0c354ff15c2c2423ef5fe725bd990cef933d75b970febe1ad7384fcfd518"
        );
    }

    #[test]
    fn sums_carry_into_what_overflows() {
        let mut bundle = Bundle::new(&[]).unwrap();
        for (data, name) in [
            (&b"alpha\n"[..], "d/a.txt"),
            (b"", "d/empty"),
            (b"beta\n", "d/sub/b.txt"),
        ] {
            bundle.start();
            bundle.update(data);
            bundle.finish(false, name);
        }
        bundle.start();
        bundle.finish(true, "d");
        bundle.start();
        bundle.finish(true, "d/sub");
        // `7z h d`: the same files and folders, in another order, which sums ignore.
        assert_eq!(bundle.hashers[0].text(DATA_SUM), "86441661-00000001");
        assert_eq!(bundle.hashers[0].text(NAMES_SUM), "7040DABF-00000002");
    }
}
