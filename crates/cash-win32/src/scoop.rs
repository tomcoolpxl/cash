//! All cash knows about Scoop: how to read a shim's target.
//!
//! Commands still run a Scoop shim as the executable it is (D25); nothing here changes
//! what a command line starts. It is read in two places only: `cash doctor`, to see through
//! a shim to a BusyBox applet (D35), and Tab completion, to start carapace itself rather than
//! its shim on every Tab (D63). Neither needs Scoop: without a `.shim` file, the executable
//! is used as it was found.

use std::path::{Path, PathBuf};

/// The program a Scoop shim starts, or `None` for anything that is not a Scoop shim.
///
/// For `C:\…\shims\tool.exe` it is the `path = "…"` line of the `tool.shim` beside it,
/// resolved against the shim's folder when it is relative.
#[must_use]
pub fn shim_target(shim: &Path) -> Option<PathBuf> {
    let contents = std::fs::read_to_string(shim.with_extension("shim")).ok()?;
    let target = contents.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("path")
            .then(|| PathBuf::from(value.trim().trim_matches('"')))
    })?;
    if target.is_absolute() {
        Some(target)
    } else {
        Some(shim.parent()?.join(target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shim_names_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let shim = dir.path().join("tool.exe");
        std::fs::write(&shim, b"MZ").unwrap();
        std::fs::write(
            dir.path().join("tool.shim"),
            "path = \"C:\\scoop\\apps\\tool\\current\\tool.exe\"\r\nargs = --flag\r\n",
        )
        .unwrap();

        assert_eq!(
            shim_target(&shim),
            Some(PathBuf::from(r"C:\scoop\apps\tool\current\tool.exe"))
        );
    }

    #[test]
    fn a_relative_target_is_beside_the_shim() {
        let dir = tempfile::tempdir().unwrap();
        let shim = dir.path().join("tool.exe");
        std::fs::write(dir.path().join("tool.shim"), "path = ..\\apps\\tool.exe\n").unwrap();

        assert_eq!(
            shim_target(&shim),
            Some(dir.path().join(r"..\apps\tool.exe"))
        );
    }

    #[test]
    fn anything_else_has_no_target() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("plain.exe");
        std::fs::write(&plain, b"MZ").unwrap();
        assert_eq!(shim_target(&plain), None);

        let odd = dir.path().join("odd.exe");
        std::fs::write(dir.path().join("odd.shim"), "args = --flag\n").unwrap();
        assert_eq!(shim_target(&odd), None);
    }
}
