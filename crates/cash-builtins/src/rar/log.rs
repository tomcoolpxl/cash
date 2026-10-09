//! `-log[fmt][=name]`: archive and file names written to a file as rar works, as
//! `Rar.exe` 7.23 writes them: one a line, CRLF after each, `\` between folders. `A`
//! logs the archives (each volume), `F` the files added, extracted, tested, listed or
//! deleted, and with `F` the archives are not logged, `A` or not; neither is `A`. `P`
//! appends to the file, else it is made anew; `U` writes UTF-16 (no byte-order mark),
//! as `-sc…g` may; else UTF-8, cash's own, where `Rar.exe` writes Windows' ANSI code
//! page. The file is `rarinfo.log` unless named.

use std::io::Write as _;
use std::path::PathBuf;

use super::cmdline::Charset;

/// One `-log`'s file and what goes in it.
pub(super) struct Log {
    path: PathBuf,
    archives: bool,
    files: bool,
    charset: Option<Charset>,
}

impl Log {
    /// The logs the switches ask for, each made anew unless `P`; `path` places a name.
    pub(super) fn open_all(
        specs: &[String],
        charset: Option<Charset>,
        path: impl Fn(&str) -> PathBuf,
    ) -> Vec<Self> {
        specs
            .iter()
            .map(|spec| {
                let (format, name) = spec.split_once('=').unwrap_or((spec.as_str(), ""));
                let format = format.to_ascii_lowercase();
                let files = format.contains('f');
                let log = Self {
                    path: path(if name.is_empty() { "rarinfo.log" } else { name }),
                    archives: !files,
                    files,
                    charset: if format.contains('u') {
                        Some(Charset::Utf16)
                    } else {
                        charset
                    },
                };
                if !format.contains('p') {
                    let _ = std::fs::File::create(&log.path);
                }
                log
            })
            .collect()
    }

    pub(super) const fn wants_archives(&self) -> bool {
        self.archives
    }

    pub(super) const fn wants_files(&self) -> bool {
        self.files
    }

    /// A name, its line ended, added to the file.
    pub(super) fn write(&self, name: &str) {
        let line = format!("{}\r\n", name.replace('/', "\\"));
        let bytes = match self.charset {
            Some(Charset::Utf16) => line.encode_utf16().flat_map(u16::to_le_bytes).collect(),
            Some(Charset::Ansi | Charset::Oem) => {
                let code_page = u32::from(self.charset == Some(Charset::Oem));
                let wide: Vec<u16> = line.encode_utf16().collect();
                cash_win32::codepage::encode(code_page, &wide, true)
                    .map_or_else(|_| line.clone().into_bytes(), |encoded| encoded.bytes)
            }
            Some(Charset::Utf8) | None => line.into_bytes(),
        };
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = file.write_all(&bytes);
        }
    }
}
