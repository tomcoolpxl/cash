//! Build script for cash-builtins: embeds the help catalogue's pages.
//!
//! `help` carries a page for many builtins and a set of topics, one markdown file each,
//! in `src/helpdocs/pages` and `src/helpdocs/topics`. Listing every file in Rust by hand
//! would let a new page sit unseen on the disk, so this writes the list: a page is
//! added by adding its file.

use std::fmt::Write as _;
use std::path::Path;

fn main() -> std::io::Result<()> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let docs = manifest.join("src").join("helpdocs");
    let mut out = String::new();
    for (constant, folder) in [("PAGES", "pages"), ("TOPICS", "topics")] {
        let dir = docs.join(folder);
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut files: Vec<_> = std::fs::read_dir(&dir)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
            .collect();
        files.sort();
        let _ = writeln!(out, "pub(super) const {constant}: &[(&str, &str)] = &[");
        for file in files {
            println!("cargo:rerun-if-changed={}", file.display());
            let stem = file
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "    ({stem:?}, include_str!(r#\"{}\"#)),",
                file.display()
            );
        }
        let _ = writeln!(out, "];");
    }
    let target = Path::new(&std::env::var_os("OUT_DIR").unwrap_or_default()).join("helpdocs.rs");
    std::fs::write(target, out)
}
