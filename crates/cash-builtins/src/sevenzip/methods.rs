//! 7-Zip's `-m` parameters for 7z (`7zHandlerOut.cpp`, `HandlerOut.cpp`): the level, the
//! methods and filters, solid blocks, the header's compression and encryption, the times
//! kept, the threads; the coders a block is written with; and the filter a file's first
//! bytes call for (`7zUpdate.cpp`'s analysis).

use cash_archive::sevenz::options::{
    AesEncoderOptions, EncodeMode, EncoderOptions, LzmaParams, MfType,
};
use cash_archive::sevenz::{EncoderConfiguration, EncoderMethod, Password};

/// Why a `-m` was refused: 7-Zip's `E_INVALIDARG` ("The parameter is incorrect.") or
/// `E_NOTIMPL` ("Not implemented").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MethodError {
    Invalid,
    NotImplemented,
}

/// One method of the chain, as `-mN=` names it, with its parameters.
#[derive(Debug, Clone, Default)]
struct Spec {
    name: Option<String>,
    params: Vec<(String, String)>,
}

/// The filter: chosen by each file's kind (`-mf=on`, the default), none, or one for all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterSetting {
    Auto,
    Off,
    Fixed(Filter),
}

/// A filter in front of the main method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Filter {
    Delta(u32),
    Bcj,
    Ppc,
    Ia64,
    Arm,
    ArmThumb,
    Sparc,
    Arm64,
    RiscV,
}

impl Filter {
    fn parse(text: &str) -> Option<Self> {
        let lower = text.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("delta") {
            let n = rest.strip_prefix(':').unwrap_or(rest);
            let n = if n.is_empty() { 1 } else { n.parse().ok()? };
            return (1..=256).contains(&n).then_some(Self::Delta(n));
        }
        Some(match lower.as_str() {
            "bcj" | "x86" => Self::Bcj,
            "ppc" => Self::Ppc,
            "ia64" => Self::Ia64,
            "arm" => Self::Arm,
            "armt" => Self::ArmThumb,
            "sparc" => Self::Sparc,
            "arm64" => Self::Arm64,
            "riscv" => Self::RiscV,
            _ => return None,
        })
    }

    /// 7-Zip's id for the method, and the distance or alignment: the order its filter
    /// groups are written in.
    pub(super) const fn sort_key(self) -> (u32, u32) {
        match self {
            Self::Delta(d) => (0x03, d),
            Self::Arm64 => (0x0A, 4),
            Self::RiscV => (0x0B, 2),
            Self::Bcj => (0x0303_0103, 1),
            Self::Ppc => (0x0303_0205, 4),
            Self::Ia64 => (0x0303_0401, 16),
            Self::Arm => (0x0303_0501, 4),
            Self::ArmThumb => (0x0303_0701, 2),
            Self::Sparc => (0x0303_0805, 4),
        }
    }

    /// The filter an existing block's last coder is (`Get_FilterGroup_for_Folder`).
    pub(super) fn of_coder(id: &[u8], props: &[u8]) -> Option<Self> {
        Some(match id {
            [0x03] => Self::Delta(u32::from(*props.first()?) + 1),
            [0x03, 0x03, 0x01, 0x03 | 0x1B] => Self::Bcj,
            [0x03, 0x03, 0x02, 0x05] => Self::Ppc,
            [0x03, 0x03, 0x04, 0x01] => Self::Ia64,
            [0x03, 0x03, 0x05, 0x01] => Self::Arm,
            [0x03, 0x03, 0x07, 0x01] => Self::ArmThumb,
            [0x03, 0x03, 0x08, 0x05] => Self::Sparc,
            [0x0A] => Self::Arm64,
            [0x0B] => Self::RiscV,
            _ => return None,
        })
    }

    fn config(self) -> EncoderConfiguration {
        let method = match self {
            Self::Delta(distance) => {
                return EncoderConfiguration::new(EncoderMethod::DELTA_FILTER)
                    .with_options(EncoderOptions::Delta { distance });
            }
            Self::Bcj => EncoderMethod::BCJ_X86_FILTER,
            Self::Ppc => EncoderMethod::BCJ_PPC_FILTER,
            Self::Ia64 => EncoderMethod::BCJ_IA64_FILTER,
            Self::Arm => EncoderMethod::BCJ_ARM_FILTER,
            Self::ArmThumb => EncoderMethod::BCJ_ARM_THUMB_FILTER,
            Self::Sparc => EncoderMethod::BCJ_SPARC_FILTER,
            Self::Arm64 => EncoderMethod::BCJ_ARM64_FILTER,
            Self::RiscV => EncoderMethod::BCJ_RISCV_FILTER,
        };
        EncoderConfiguration::new(method)
    }
}

/// The settings a 7z archive is written with.
#[derive(Debug, Clone)]
pub(super) struct Settings {
    pub(super) level: u32,
    methods: Vec<Spec>,
    filter: FilterSetting,
    /// The most files a solid block holds: one unless solid.
    solid_files: u64,
    /// `-ms`'s most bytes in a solid block.
    solid_bytes: Option<u64>,
    /// `-ms=e`: a block for each extension.
    pub(super) solid_by_extension: bool,
    pub(super) header_compress: bool,
    /// `-mhe`, when given.
    pub(super) header_encrypt: Option<bool>,
    pub(super) mtime: Option<bool>,
    pub(super) ctime: Option<bool>,
    pub(super) atime: Option<bool>,
    pub(super) attributes: Option<bool>,
    pub(super) threads: u32,
    /// `-mqs`: new files sorted by their kind.
    pub(super) sort_by_type: bool,
}

/// `on`, `off`, `+`, `-`, or nothing (on).
fn boolean(value: Option<&str>) -> Result<bool, MethodError> {
    match value.map(str::to_ascii_lowercase).as_deref() {
        None | Some("" | "on" | "+") => Ok(true),
        Some("off" | "-") => Ok(false),
        _ => Err(MethodError::Invalid),
    }
}

/// A size: a number of bytes, with `b`, `k`, `m`, `g` or `t`, or a bare power of two.
fn size(text: &str, bare_is_power: bool) -> Option<u64> {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let n: u64 = text.get(..digits)?.parse().ok()?;
    let shift = match text.get(digits..)?.to_ascii_lowercase().as_str() {
        "" if bare_is_power => return (n < 64).then(|| 1 << n),
        "" | "b" => 0,
        "k" => 10,
        "m" => 20,
        "g" => 30,
        "t" => 40,
        _ => return None,
    };
    n.checked_shl(shift).filter(|v| v >> shift == n)
}

fn number(text: &str) -> Result<u32, MethodError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(MethodError::Invalid);
    }
    text.parse().map_err(|_| MethodError::Invalid)
}

impl Settings {
    /// The settings `-m` parameters give, over 7-Zip's defaults.
    pub(super) fn parse(properties: &[(String, Option<String>)]) -> Result<Self, MethodError> {
        let threads =
            std::thread::available_parallelism().map_or(1, |n| u32::try_from(n.get()).unwrap_or(1));
        let mut settings = Self {
            level: 5,
            methods: Vec::new(),
            filter: FilterSetting::Auto,
            solid_files: u64::MAX,
            solid_bytes: None,
            solid_by_extension: false,
            header_compress: true,
            header_encrypt: None,
            mtime: None,
            ctime: None,
            atime: None,
            attributes: None,
            threads,
            sort_by_type: false,
        };
        for (name, value) in properties {
            settings.set(&name.to_ascii_lowercase(), value.as_deref())?;
        }
        Ok(settings)
    }

    fn method_mut(&mut self, index: usize) -> &mut Spec {
        if self.methods.len() <= index {
            self.methods.resize(index + 1, Spec::default());
        }
        &mut self.methods[index]
    }

    /// `CMultiMethodProps::SetProperty` and `COutHandler::SetProperty`.
    fn set(&mut self, name: &str, value: Option<&str>) -> Result<(), MethodError> {
        if name.is_empty() {
            return Err(MethodError::Invalid);
        }
        if let Some(rest) = name.strip_prefix('x') {
            self.level = match (rest, value) {
                ("", None | Some("")) => 9,
                ("", Some(v)) | (v, None) => number(v)?.min(9),
                _ => return Err(MethodError::Invalid),
            };
            return Ok(());
        }
        let digits = name.bytes().take_while(u8::is_ascii_digit).count();
        let rest = name.get(digits..).unwrap_or_default();
        if digits == 0 {
            match name {
                "s" => return self.set_solid(value.unwrap_or("")),
                "f" => {
                    self.filter = match value.map(str::to_ascii_lowercase).as_deref() {
                        None | Some("" | "on" | "+") => FilterSetting::Auto,
                        Some("off" | "-") => FilterSetting::Off,
                        Some(other) => {
                            FilterSetting::Fixed(Filter::parse(other).ok_or(MethodError::Invalid)?)
                        }
                    };
                    return Ok(());
                }
                "hc" => self.header_compress = boolean(value)?,
                "he" => self.header_encrypt = Some(boolean(value)?),
                "tm" => self.mtime = Some(boolean(value)?),
                "tc" => self.ctime = Some(boolean(value)?),
                "ta" => self.atime = Some(boolean(value)?),
                "tr" => self.attributes = Some(boolean(value)?),
                "qs" => self.sort_by_type = boolean(value)?,
                "mtf" | "rsfx" | "hcf" => {
                    boolean(value)?;
                }
                "tp" => {}
                _ if name.starts_with("mt") => {
                    let rest = name.get(2..).unwrap_or_default();
                    self.threads = match (rest, value.map(str::to_ascii_lowercase).as_deref()) {
                        ("", None | Some("" | "on" | "+")) => self.threads,
                        ("", Some("off" | "-")) => 1,
                        ("", Some(v)) | (v, None) => number(v)?.max(1),
                        _ => return Err(MethodError::Invalid),
                    };
                }
                _ if name.starts_with("yx") || name.starts_with("memuse") || name == "yv" => {}
                _ if name.starts_with('s') && value.is_none() => {
                    return self.set_solid(name.get(1..).unwrap_or_default());
                }
                _ => self.add_param(0, name, value)?,
            }
            return Ok(());
        }
        let index: usize = name
            .get(..digits)
            .unwrap_or_default()
            .parse()
            .map_err(|_| MethodError::Invalid)?;
        if index > 64 {
            return Err(MethodError::Invalid);
        }
        if rest.is_empty() {
            let value = value.unwrap_or_default();
            let mut parts = value.split(':');
            let method = parts.next().unwrap_or_default().to_ascii_lowercase();
            let spec = self.method_mut(index);
            spec.name = Some(method);
            for part in parts {
                spec.params.push(split_param(part));
            }
            return Ok(());
        }
        self.add_param(index, rest, value)
    }

    /// A method's parameter: `d=24`, `d24`, `fb=64`, `mf=bt4`, `o=8`, and the like.
    fn add_param(
        &mut self,
        index: usize,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), MethodError> {
        const KNOWN: [&str; 14] = [
            "d", "fb", "mf", "mc", "lc", "lp", "pb", "a", "o", "mem", "pass", "c", "eos", "x",
        ];
        let (key, val) = match value {
            Some(v) => (name.to_owned(), v.to_owned()),
            None => split_param(name),
        };
        if !KNOWN.contains(&key.as_str()) {
            return Err(MethodError::Invalid);
        }
        self.method_mut(index).params.push((key, val));
        Ok(())
    }

    /// `-ms`: `on`, `off`, or `e`, `Nf` and `N{b|k|m|g|t}` together.
    fn set_solid(&mut self, value: &str) -> Result<(), MethodError> {
        let lower = value.to_ascii_lowercase();
        match lower.as_str() {
            "" | "on" | "+" => {
                self.solid_files = u64::MAX;
                self.solid_bytes = None;
                self.solid_by_extension = false;
                return Ok(());
            }
            "off" | "-" => {
                self.solid_files = 1;
                return Ok(());
            }
            _ => {}
        }
        let mut rest = lower.as_str();
        while !rest.is_empty() {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 {
                let r = rest.strip_prefix('e').ok_or(MethodError::Invalid)?;
                self.solid_by_extension = true;
                rest = r;
                continue;
            }
            let n: u64 = rest
                .get(..digits)
                .unwrap_or_default()
                .parse()
                .map_err(|_| MethodError::Invalid)?;
            let mut after = rest.get(digits..).unwrap_or_default().chars();
            let unit = after.next().ok_or(MethodError::Invalid)?;
            rest = after.as_str();
            let shift = match unit {
                'f' => {
                    self.solid_files = n.max(1);
                    continue;
                }
                'b' => 0,
                'k' => 10,
                'm' => 20,
                'g' => 30,
                't' => 40,
                _ => return Err(MethodError::Invalid),
            };
            self.solid_bytes = Some(n << shift);
        }
        Ok(())
    }

    /// The methods' names, `-m0` first: Copy at level 0 when none is named, else LZMA2
    /// for any not named.
    fn method_names(&self) -> Vec<String> {
        if self.methods.is_empty() {
            return vec![if self.level == 0 { "copy" } else { "lzma2" }.to_owned()];
        }
        self.methods
            .iter()
            .map(|m| {
                m.name
                    .clone()
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "lzma2".to_owned())
            })
            .collect()
    }

    /// Whether files get the filter their kind calls for (`UseFilters`): unless level
    /// 0, `-mf=off`, or a filter is named.
    pub(super) fn use_filters(&self) -> bool {
        self.level != 0
            && self.filter == FilterSetting::Auto
            && !self
                .method_names()
                .iter()
                .any(|n| Filter::parse(n).is_some())
    }

    /// The most files and bytes a block holds (`NumSolidFiles`, `NumSolidBytes`): the
    /// bytes from the first method with a dictionary, at least 16 MiB; 4 GiB for other
    /// methods, nothing for Copy alone.
    pub(super) fn block_limits(&self) -> (u64, u64) {
        if let Some(bytes) = self.solid_bytes {
            return (self.solid_files, bytes);
        }
        let mut names = Vec::new();
        if let FilterSetting::Fixed(_) = self.filter {
            names.push(("filter".to_owned(), Vec::new()));
        }
        let params = |i: usize| {
            self.methods
                .get(i)
                .map(|m| m.params.clone())
                .unwrap_or_default()
        };
        names.extend(
            self.method_names()
                .into_iter()
                .enumerate()
                .map(|(i, n)| (n, params(i))),
        );
        let need_solid = names.iter().any(|(n, _)| n != "copy");
        for (name, params) in &names {
            let level = self.method_level(params);
            let dict = match name.as_str() {
                "lzma" | "lzma2" => u64::from(lzma_params(level, params, u64::MAX).dict_size),
                "ppmd" => u64::from(ppmd_settings(level, params, u64::MAX).1),
                "deflate" => 1 << 15,
                "deflate64" => 1 << 16,
                "bzip2" => u64::from(bzip2_block(level, params)) * 100_000,
                _ => continue,
            };
            let bytes = if name == "lzma2" {
                (lzma2_chunk(dict) << 6).min(1 << 34)
            } else {
                (dict << 7).min(1 << 32)
            };
            return (self.solid_files, bytes.max(1 << 24));
        }
        (self.solid_files, if need_solid { 1 << 32 } else { 0 })
    }

    /// A method's level: its own `x`, else the global one.
    fn method_level(&self, params: &[(String, String)]) -> u32 {
        param(params, "x")
            .and_then(|x| x.parse::<u32>().ok())
            .map_or(self.level, |x| x.min(9))
    }

    /// The filter a file gets when filters go by kind: the one its name and first bytes
    /// call for.
    pub(super) fn filter_for(&self, name: &str, size: u64, head: &[u8]) -> Option<Filter> {
        if !self.use_filters() {
            return None;
        }
        analyse(name, size, head)
    }

    /// Whether a file's first bytes are to be read to choose its filter.
    pub(super) fn needs_analysis(&self, name: &str) -> bool {
        self.use_filters() && analysis_wanted(name)
    }

    /// The coders of a block, the packed side first: AES when there is a password, then
    /// the methods from the last named to `-m0`, then the filter. `reduce` is what the
    /// dictionaries shrink to (7-Zip's `ReduceSize`).
    pub(super) fn chain(
        &self,
        filter: Option<Filter>,
        reduce: u64,
        password: Option<&str>,
    ) -> Result<Vec<EncoderConfiguration>, MethodError> {
        let mut chain = Vec::new();
        if let Some(password) = password {
            let aes = AesEncoderOptions::new(Password::from(password))
                .map_err(|_| MethodError::NotImplemented)?;
            chain.push(
                EncoderConfiguration::new(EncoderMethod::AES256_SHA256)
                    .with_options(EncoderOptions::Aes(aes)),
            );
        }
        let names = self.method_names();
        for (index, name) in names.iter().enumerate().rev() {
            let params = self
                .methods
                .get(index)
                .map(|m| m.params.as_slice())
                .unwrap_or_default();
            chain.push(self.config(name, params, reduce)?);
        }
        if let FilterSetting::Fixed(fixed) = self.filter {
            chain.push(fixed.config());
        } else if let Some(filter) = filter {
            chain.push(filter.config());
        }
        Ok(chain)
    }

    fn config(
        &self,
        name: &str,
        params: &[(String, String)],
        reduce: u64,
    ) -> Result<EncoderConfiguration, MethodError> {
        let level = self.method_level(params);
        Ok(match name {
            "copy" => EncoderConfiguration::new(EncoderMethod::COPY),
            "lzma" => {
                let params = lzma_params(level, params, reduce);
                EncoderConfiguration::new(EncoderMethod::LZMA)
                    .with_options(EncoderOptions::Lzma(params))
            }
            "lzma2" => {
                let params = lzma_params(level, params, reduce);
                let chunk = lzma2_chunk(u64::from(params.dict_size));
                let threads = if reduce > chunk { self.threads } else { 1 };
                EncoderConfiguration::new(EncoderMethod::LZMA2).with_options(
                    EncoderOptions::Lzma2 {
                        params,
                        threads,
                        chunk_size: if threads > 1 { chunk } else { 0 },
                    },
                )
            }
            "ppmd" => {
                let (order, memory) = ppmd_settings(level, params, reduce);
                EncoderConfiguration::new(EncoderMethod::PPMD)
                    .with_options(EncoderOptions::Ppmd { order, memory })
            }
            "bzip2" => EncoderConfiguration::new(EncoderMethod::BZIP2).with_options(
                EncoderOptions::Bzip2 {
                    level: bzip2_block(level, params),
                },
            ),
            "deflate" => EncoderConfiguration::new(EncoderMethod::DEFLATE).with_options(
                EncoderOptions::Deflate {
                    level: match level {
                        0 => 0,
                        1..=2 => 1,
                        3..=4 => 3,
                        5..=6 => 6,
                        7..=8 => 8,
                        _ => 9,
                    },
                },
            ),
            other => match Filter::parse(other) {
                Some(Filter::Delta(_)) => {
                    let distance = params
                        .first()
                        .and_then(|(k, v)| {
                            if v.is_empty() {
                                k.parse().ok()
                            } else {
                                v.parse().ok()
                            }
                        })
                        .unwrap_or(1);
                    Filter::Delta(distance).config()
                }
                Some(filter) => filter.config(),
                None if other == "deflate64" || other == "bcj2" => {
                    return Err(MethodError::NotImplemented);
                }
                None => return Err(MethodError::Invalid),
            },
        })
    }
}

/// `name=value`, or a name glued to its value (`d24`, `fb64`, `mt2`).
fn split_param(part: &str) -> (String, String) {
    if let Some((k, v)) = part.split_once('=') {
        return (k.to_ascii_lowercase(), v.to_owned());
    }
    let key: String = part.chars().take_while(char::is_ascii_alphabetic).collect();
    let value = part.get(key.len()..).unwrap_or_default().to_owned();
    (key.to_ascii_lowercase(), value)
}

fn param<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params
        .iter()
        .rev()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// `LzmaEncProps_Normalize`: the level's dictionary, fast bytes, match finder and mode,
/// what `-m` changes, the dictionary no larger than `reduce` (and at least 4 KiB).
fn lzma_params(level: u32, params: &[(String, String)], reduce: u64) -> LzmaParams {
    let mut p = LzmaParams::with_preset(6);
    p.dict_size = match level {
        0..=4 => 1 << (level * 2 + 16),
        5..=8 => 1 << (level + 20),
        _ => 1 << 28,
    };
    let fast = param(params, "a").map_or(level < 5, |a| a == "0");
    p.mode = if fast {
        EncodeMode::Fast
    } else {
        EncodeMode::Normal
    };
    p.mf = if fast { MfType::Hc4 } else { MfType::Bt4 };
    p.nice_len = if level < 7 { 32 } else { 64 };
    p.lc = 3;
    p.lp = 0;
    p.pb = 2;
    p.depth_limit = 0;
    if let Some(d) = param(params, "d").and_then(|d| size(d, true)) {
        p.dict_size = u32::try_from(d).unwrap_or(u32::MAX);
    }
    if let Some(fb) = param(params, "fb").and_then(|v| v.parse::<u32>().ok()) {
        p.nice_len = fb.clamp(5, 273);
    }
    if let Some(lc) = param(params, "lc").and_then(|v| v.parse::<u32>().ok()) {
        p.lc = lc.min(8);
    }
    if let Some(lp) = param(params, "lp").and_then(|v| v.parse::<u32>().ok()) {
        p.lp = lp.min(4);
    }
    if let Some(pb) = param(params, "pb").and_then(|v| v.parse::<u32>().ok()) {
        p.pb = pb.min(4);
    }
    if let Some(mf) = param(params, "mf") {
        p.mf = if mf.to_ascii_lowercase().starts_with("hc") {
            MfType::Hc4
        } else {
            MfType::Bt4
        };
    }
    if u64::from(p.dict_size) > reduce {
        p.dict_size = u32::try_from(reduce).unwrap_or(u32::MAX).max(1 << 12);
    }
    p
}

/// The chunk an LZMA2 thread compresses: four dictionaries, 1 MiB to 256 MiB, no less
/// than the dictionary, in whole MiB.
fn lzma2_chunk(dict: u64) -> u64 {
    let min = 1u64 << 20;
    let cs = (dict << 2).clamp(min, 1 << 28).max(dict);
    (cs + min - 1) & !(min - 1)
}

/// `PPMd`'s order and memory for the level, `o` and `mem`, the memory no more than the
/// data needs (`CEncProps::Normalize`).
fn ppmd_settings(level: u32, params: &[(String, String)], reduce: u64) -> (u32, u32) {
    const ORDERS: [u32; 10] = [3, 4, 4, 5, 5, 6, 8, 16, 24, 32];
    let level = level.min(9);
    let mut memory = param(params, "mem")
        .and_then(|m| size(m, true))
        .map_or(1 << (level + 19), |m| u32::try_from(m).unwrap_or(u32::MAX));
    let order = param(params, "o")
        .and_then(|o| o.parse().ok())
        .unwrap_or(ORDERS[level as usize]);
    if u64::from(memory / 16) > reduce {
        for i in 16..32 {
            let m = 1u32 << i;
            if reduce <= u64::from(m / 16) {
                memory = memory.min(m);
                break;
            }
        }
    }
    (order.clamp(2, 32), memory)
}

/// bzip2's block size in 100 kB: 9 from level 5, `2·level − 1` below, or `d`'s.
fn bzip2_block(level: u32, params: &[(String, String)]) -> u32 {
    if let Some(d) = param(params, "d").and_then(|d| size(d, false)) {
        return u32::try_from(d.div_ceil(100_000)).unwrap_or(9).clamp(1, 9);
    }
    match level {
        0 => 1,
        1..=4 => level * 2 - 1,
        _ => 9,
    }
}

/// Whether 7-Zip reads a file to choose its filter (`GetFilterGroup` at analysis level
/// 5): executables by their extension (`.so` with a version too), and WAV files.
fn analysis_wanted(name: &str) -> bool {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let ext = match base.rsplit_once('.') {
        Some((_, ext)) => ext.to_ascii_lowercase(),
        None => return false,
    };
    if matches!(ext.as_str(), "dll" | "exe" | "ocx" | "sfx" | "sys" | "wav") {
        return true;
    }
    // libstdc++.so.6.0.29: `so` or `dylib`, then only numbers.
    let mut parts: Vec<&str> = base.split('.').skip(1).collect();
    while let Some(last) = parts.last() {
        if *last == "so" || *last == "dylib" {
            return true;
        }
        if last.is_empty() || !last.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        parts.pop();
    }
    false
}

/// `ParseFile`: PE, ELF, Mach-O or PCM WAV by the first bytes, and the filter for it;
/// none when the size does not fit the filter's alignment.
fn analyse(name: &str, size: u64, head: &[u8]) -> Option<Filter> {
    if !analysis_wanted(name) {
        return None;
    }
    let filter = parse_exe(head)
        .or_else(|| parse_elf(head))
        .or_else(|| parse_mach(head))
        .or_else(|| parse_wav(head))?;
    if let Filter::Delta(_) = filter {
        return Some(filter);
    }
    let align = u64::from(filter.sort_key().1);
    (align <= 1 || size.is_multiple_of(align)).then_some(filter)
}

fn le16(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_le_bytes(
        b.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn be16(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes(
        b.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// `Parse_EXE`.
fn parse_exe(b: &[u8]) -> Option<Filter> {
    if b.len() < 512 || le16(b, 0)? != 0x5A4D {
        return None;
    }
    let pe = le32(b, 0x3C)? as usize;
    if pe >= 0x1000 || pe + 512 > b.len() || pe & 7 != 0 || le32(b, pe)? != 0x0000_4550 {
        return None;
    }
    let p = pe + 4;
    let machine = le16(b, p)?;
    let mut filter = match machine {
        0x014C | 0x8664 => Filter::Bcj,
        0xAA64 => Filter::Arm64,
        0x01C0 | 0x01C2 => Filter::Arm,
        0x01C4 => Filter::ArmThumb,
        0x5032 | 0x5064 => Filter::RiscV,
        0x0200 => Filter::Ia64,
        _ => return None,
    };
    let sections = le16(b, p + 2)? as usize;
    let opt_size = le16(b, p + 16)? as usize;
    if opt_size > 1 << 10 {
        return None;
    }
    let opt = p + 20;
    if !matches!(le16(b, opt)?, 0x10B | 0x20B) {
        return None;
    }
    if opt + opt_size <= b.len() && sections <= 64 && machine == 0x8664 {
        let mut s = opt + opt_size;
        for _ in 0..sections {
            if s + 40 > b.len() {
                break;
            }
            if b.get(s..s + 8) == Some(b".a64xrm\0") || b.get(s..s + 7) == Some(b".a64xrm") {
                filter = Filter::Arm64;
                break;
            }
            s += 40;
        }
    }
    Some(filter)
}

/// `Parse_ELF`.
fn parse_elf(b: &[u8]) -> Option<Filter> {
    if b.len() < 512 || b[6] != 1 || le32(b, 0)? != 0x464C_457F || !matches!(b[4], 1 | 2) {
        return None;
    }
    let be = match b[5] {
        1 => false,
        2 => true,
        _ => return None,
    };
    let machine = if be { be16(b, 0x12)? } else { le16(b, 0x12)? };
    Some(match machine {
        3 | 6 | 62 => Filter::Bcj,
        2 | 18 | 43 => Filter::Sparc,
        20 | 21 if be => Filter::Ppc,
        40 if !be => Filter::Arm,
        183 if !be => Filter::Arm64,
        243 if !be => Filter::RiscV,
        _ => return None,
    })
}

/// `Parse_MACH`.
fn parse_mach(b: &[u8]) -> Option<Filter> {
    if b.len() < 512 {
        return None;
    }
    let be = match le32(b, 0)? {
        0xCEFA_EDFE | 0xCFFA_EDFE => true,
        0xFEED_FACE | 0xFEED_FACF => false,
        _ => return None,
    };
    let get = |at| if be { be32(b, at) } else { le32(b, at) };
    let filter = match get(4)? {
        7 | 0x0100_0007 => Filter::Bcj,
        12 if !be => Filter::Arm,
        14 if be => Filter::Sparc,
        18 | 0x0100_0012 if be => Filter::Ppc,
        0x0100_000C if !be => Filter::Arm64,
        _ => return None,
    };
    (get(0x14)? <= 1 << 24 && get(0x10)? <= 1 << 18).then_some(filter)
}

/// `Parse_WAV`: PCM, the delta the channels and sample size give.
fn parse_wav(b: &[u8]) -> Option<Filter> {
    if b.len() < 0x2C
        || le32(b, 0)? != 0x4646_4952
        || le32(b, 8)? != 0x4556_4157
        || le32(b, 0xC)? != 0x2074_6D66
    {
        return None;
    }
    let sub = le32(b, 0x10)? as usize;
    if !(0x10..=0x12).contains(&sub) || le16(b, 0x14)? != 1 {
        return None;
    }
    let bits = le16(b, 0x22)?;
    if bits & 7 != 0 {
        return None;
    }
    let delta = le16(b, 0x16)? * (bits >> 3);
    if delta == 0 || delta > 256 {
        return None;
    }
    let mut pos = 0x14 + sub;
    for _ in 0..10 {
        if pos + 8 > b.len() {
            return None;
        }
        let size = le32(b, pos + 4)? as usize;
        if le32(b, pos)? == 0x6174_6164 {
            return Some(Filter::Delta(delta));
        }
        if size > 1 << 16 {
            return None;
        }
        pos += size + 8;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(list: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
        list.iter()
            .map(|(k, v)| ((*k).to_owned(), v.map(str::to_owned)))
            .collect()
    }

    #[test]
    fn levels_and_methods() {
        let s = Settings::parse(&props(&[("x", Some("1"))])).unwrap();
        assert_eq!(s.level, 1);
        let s = Settings::parse(&props(&[("x0", None)])).unwrap();
        assert_eq!(s.block_limits().1, 0);
        let s = Settings::parse(&props(&[("0", Some("PPMd:o=8:mem=24"))])).unwrap();
        assert_eq!(s.method_names(), ["ppmd"]);
        assert_eq!(
            Settings::parse(&props(&[("foo", Some("1"))])).err(),
            Some(MethodError::Invalid)
        );
        let s = Settings::parse(&props(&[
            ("hc", Some("off")),
            ("he", None),
            ("s", Some("e10f64m")),
        ]))
        .unwrap();
        assert!(!s.header_compress && s.header_encrypt == Some(true));
        assert!(s.solid_by_extension);
        assert_eq!(s.block_limits(), (10, 64 << 20));
        let s = Settings::parse(&props(&[("s", Some("off"))])).unwrap();
        assert_eq!(s.block_limits().0, 1);
    }

    #[test]
    fn dictionaries_follow_7_zip() {
        assert_eq!(lzma_params(5, &[], u64::MAX).dict_size, 32 << 20);
        assert_eq!(lzma_params(1, &[], u64::MAX).dict_size, 256 << 10);
        assert_eq!(lzma_params(9, &[], u64::MAX).dict_size, 256 << 20);
        assert_eq!(lzma_params(5, &[], 31).dict_size, 4096);
        assert_eq!(ppmd_settings(5, &[], 31), (6, 1 << 16));
        assert_eq!(lzma2_chunk(32 << 20), 128 << 20);
        let s = Settings::parse(&[]).unwrap();
        assert_eq!(s.block_limits(), (u64::MAX, 8 << 30));
    }

    #[test]
    fn files_are_analysed_by_name() {
        assert!(analysis_wanted("d/app.EXE"));
        assert!(analysis_wanted("lib/libstdc++.so.6.0.29"));
        assert!(!analysis_wanted("notes.txt"));
        let mut wav = vec![0u8; 0x2C];
        wav[..4].copy_from_slice(b"RIFF");
        wav[8..16].copy_from_slice(b"WAVEfmt ");
        wav[0x10] = 0x10;
        wav[0x14] = 1;
        wav[0x16] = 2;
        wav[0x22] = 16;
        wav[0x24..0x28].copy_from_slice(b"data");
        assert_eq!(analyse("a.wav", 100, &wav), Some(Filter::Delta(4)));
    }
}
