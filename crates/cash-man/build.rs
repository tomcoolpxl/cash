//! Build script for cash-man: embeds the manual's pages, deflated.
//!
//! A page is one markdown file in `pages/` (a command's) or `topics/` (one of cash's
//! topics). Listing them in Rust by hand would let a new page sit unseen on the disk, so
//! this writes the list: a page is added by adding its file. Each is stored deflated,
//! as the pages are text of a few kilobytes and `cash.exe` has a size budget.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;

fn main() -> std::io::Result<()> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = std::env::var_os("OUT_DIR").unwrap_or_default();
    let out = Path::new(&out);
    let mut list = String::new();
    for (constant, directory) in [("PAGES", "pages"), ("TOPICS", "topics")] {
        let dir = manifest.join(directory);
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut files: Vec<_> = match std::fs::read_dir(&dir) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
                .collect(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        files.sort();
        let _ = writeln!(list, "pub(crate) const {constant}: &[(&str, &[u8])] = &[");
        for file in files {
            println!("cargo:rerun-if-changed={}", file.display());
            let stem = file
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            let text = std::fs::read(&file)?;
            let mut encoder =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
            encoder.write_all(&text)?;
            let stored = out.join(format!("{directory}-{stem}.deflate"));
            std::fs::write(&stored, encoder.finish()?)?;
            let _ = writeln!(
                list,
                "    ({stem:?}, include_bytes!(r#\"{}\"#)),",
                stored.display()
            );
        }
        let _ = writeln!(list, "];");
    }
    std::fs::write(out.join("pages.rs"), list)
}
