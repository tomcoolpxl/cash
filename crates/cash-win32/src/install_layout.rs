//! How this `cash.exe` was put on the machine, told by where it is.
//!
//! The three channels, Scoop's install, the installer's, or a portable copy unpacked
//! from the release zip, are told apart by their paths alone, so that the installer and the
//! shell agree. Scoop keeps every app under `scoop\apps\<name>\`, and only Scoop should
//! upgrade or remove what it put there. The installer copies a release into
//! `<root>\<version>\` and points a `current` junction beside the version folder at it;
//! `cash --update` moves the junction on. A `cash.exe` in neither layout is portable: the
//! zip's, or a build in a `target` folder.

use std::path::{Path, PathBuf};

/// Whether `exe` is under a `scoop\apps\` folder: Scoop's install, which only Scoop
/// should upgrade or remove.
#[must_use]
pub fn installed_by_scoop(exe: &Path) -> bool {
    exe.to_string_lossy()
        .replace('/', "\\")
        .to_lowercase()
        .contains("\\scoop\\apps\\")
}

/// The installer's layout around an exe: `<root>\<version>\cash.exe` with a `current`
/// junction beside the version folder.
pub struct Layout {
    /// The install folder.
    pub root: PathBuf,
    /// The folder the exe is in.
    pub version_dir: PathBuf,
    /// The `current` junction.
    pub current: PathBuf,
}

/// The layout `exe` is in, if it is in one; `is_junction` says whether a path is a
/// junction, so the tests can decide.
#[must_use]
pub fn layout(exe: &Path, is_junction: impl Fn(&Path) -> bool) -> Option<Layout> {
    let version_dir = exe.parent()?;
    let root = version_dir.parent()?;
    let current = root.join("current");
    (version_dir.file_name().is_some() && is_junction(&current)).then(|| Layout {
        root: root.to_path_buf(),
        version_dir: version_dir.to_path_buf(),
        current,
    })
}

/// Whether `exe` was put where it is by neither Scoop nor the installer: unpacked from
/// the zip, or built here. Looks at the disk for the installer's junction.
#[must_use]
pub fn is_portable(exe: &Path) -> bool {
    !installed_by_scoop(exe) && layout(exe, crate::junction::is_junction).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_needs_a_current_junction_beside_the_version_folder() {
        let junction = Path::new(r"C:\Users\me\AppData\Local\Programs\cash\current");
        let is_junction = |path: &Path| path == junction;
        let found = layout(
            Path::new(r"C:\Users\me\AppData\Local\Programs\cash\1.5.0\cash.exe"),
            is_junction,
        )
        .unwrap();
        assert_eq!(
            found.root,
            Path::new(r"C:\Users\me\AppData\Local\Programs\cash")
        );
        assert_eq!(
            found.version_dir.file_name().unwrap().to_str(),
            Some("1.5.0")
        );
        assert_eq!(found.current, junction);
        // A plain zip, or a cargo build: no junction beside it.
        assert!(layout(Path::new(r"C:\tools\cash\cash.exe"), is_junction).is_none());
        assert!(layout(Path::new(r"D:\src\cash\target\debug\cash.exe"), is_junction).is_none());
    }

    #[test]
    fn scoops_install_is_told_by_its_path() {
        assert!(installed_by_scoop(Path::new(
            r"C:\Users\me\scoop\apps\cash\1.5.0\cash.exe"
        )));
        assert!(installed_by_scoop(Path::new(
            "C:/ProgramData/scoop/apps/cash/current/cash.exe"
        )));
        assert!(!installed_by_scoop(Path::new(
            r"C:\Users\me\AppData\Local\Programs\cash\1.5.0\cash.exe"
        )));
    }

    #[test]
    fn a_copy_in_a_plain_folder_is_portable_and_one_beside_a_junction_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("unpacked").join("cash.exe");
        std::fs::create_dir_all(plain.parent().unwrap()).unwrap();
        std::fs::write(&plain, "MZ").unwrap();
        assert!(is_portable(&plain));
        assert!(!is_portable(
            &dir.path()
                .join("scoop")
                .join("apps")
                .join("cash")
                .join("cash.exe")
        ));

        let root = dir.path().join("Programs").join("cash");
        let version = root.join("1.5.0");
        std::fs::create_dir_all(&version).unwrap();
        std::fs::write(version.join("cash.exe"), "MZ").unwrap();
        crate::junction::point(&root.join("current"), &version).unwrap();
        assert!(!is_portable(&version.join("cash.exe")));
    }
}
