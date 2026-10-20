//! The directories of `/` under `unixpaths` — **D83**.
//!
//! Under `unixpaths` cash spells paths as Git Bash does, `/c/Users/...` (D82), and `/` is the
//! directory above the drives: `cd ..` from `/c` and `cd /` reach it, so a loop that climbs
//! with `cd ..` until `$PWD` is `/` ends, as it does in Git Bash. Windows has no such
//! directory, nor `/usr/bin` or `/dev`: they are cash's own, and this module is the one
//! place that says what they are, what each holds, and what a path under `/` names. The
//! shell's directory, `cd`, globs, `ls`, `test` and the directory walks ask it.
//!
//! | Path | What it is |
//! |---|---|
//! | `/` | the drives (`c`, `d`, ...), `bin`, `dev`, `tmp` and `usr` |
//! | `/usr` | `bin` |
//! | `/usr/bin`, `/bin` | one directory: the commands cash carries, which `/usr/bin/NAME` runs |
//! | `/dev` | cash's device names (D7, `crate::devices::NAMES`) |
//! | `/c`, `/tmp` | the drive, and the temporary directory: real ones (D3) |
//! | `/Windows`, any other name | that name at the root of the drive the shell came from |
//!
//! A program started in one of cash's own directories runs in the root of that drive, as
//! Windows needs a real directory to start a program in.

use std::path::{Path, PathBuf};

/// A directory of cash's own under `unixpaths`, with no directory on disk behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Virtual {
    /// `/`, above the drives.
    Root,
    /// `/usr`.
    Usr,
    /// `/usr/bin`, which `/bin` is too, as in Git Bash.
    Bin,
    /// `/dev`.
    Dev,
}

impl Virtual {
    /// The path it is, as the shell shows it.
    #[must_use]
    pub const fn spelled(self) -> &'static str {
        match self {
            Self::Root => "/",
            Self::Usr => "/usr",
            Self::Bin => "/usr/bin",
            Self::Dev => "/dev",
        }
    }

    /// The one `spelled` is exactly, as [`normalize`] leaves a path.
    #[must_use]
    pub fn of_normal(spelled: &str) -> Option<Self> {
        match spelled {
            "/" => Some(Self::Root),
            "/usr" => Some(Self::Usr),
            "/usr/bin" | "/bin" => Some(Self::Bin),
            "/dev" => Some(Self::Dev),
            _ => None,
        }
    }
}

/// What a path names under `unixpaths`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// One of cash's own directories.
    Virtual(Virtual),
    /// A file or directory on disk, in cash's `C:/` spelling; whether it is there is the
    /// disk's to say.
    Real(PathBuf),
    /// One of cash's device names, as spelled: `/dev/null`, `/dev/fd/3`.
    Device(String),
    /// A command cash carries, which `/usr/bin/NAME` runs: its name.
    Command(String),
}

impl Place {
    /// The path it is resolved to, for a caller that takes one: cash's own directory and
    /// a device as spelled (`/`, `/dev/null`), which the shell tells by that spelling, a
    /// command as `/usr/bin/NAME`, and a file on disk as itself.
    #[must_use]
    pub fn into_path(self) -> PathBuf {
        match self {
            Self::Virtual(directory) => PathBuf::from(directory.spelled()),
            Self::Real(path) => path,
            Self::Device(spelled) => PathBuf::from(spelled),
            Self::Command(name) => PathBuf::from(format!("/usr/bin/{name}")),
        }
    }
}

/// One of the names a directory of cash's own holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The name it is listed by.
    pub name: String,
    /// What it is.
    pub place: Place,
}

/// `path` with `.`, `..` and doubled slashes taken out, as `cd` takes them, when it starts
/// with a slash; `None` for one that does not (`C:/x`, `x`).
///
/// `..` at a drive's top (`/c/..`) or at `/` gives `/`, and `..` at a share's top
/// (`//server/share/..`) stays there, as a share has nothing above it. Either slash is read.
#[must_use]
pub fn normalize(path: &str) -> Option<String> {
    let unified = path.replace('\\', "/");
    if !unified.starts_with('/') {
        return None;
    }
    let share = unified.starts_with("//") && !unified.starts_with("///");
    let mut parts = unified.split('/').filter(|part| !part.is_empty());
    let mut root = String::from("/");
    if share {
        // `//` alone names no share: it is `/`, as it is to Windows.
        let Some(server) = parts.next() else {
            return Some(root);
        };
        root = match parts.next() {
            Some(name) => format!("//{server}/{name}"),
            None => return Some(format!("//{server}")),
        };
    }
    let mut kept: Vec<&str> = Vec::new();
    for part in parts {
        match part {
            "." => {}
            ".." => {
                kept.pop();
            }
            name => kept.push(name),
        }
    }
    if kept.is_empty() {
        return Some(root);
    }
    let separator = if root.ends_with('/') { "" } else { "/" };
    Some(format!("{root}{separator}{}", kept.join("/")))
}

/// `name` joined under `directory` in the shown spelling, and normalized: `/` and `c` give
/// `/c`, `/c/x` and `..` give `/c`. `name` alone when it starts with a slash.
#[must_use]
pub fn join(directory: &str, name: &str) -> String {
    let joined = if name.starts_with(['/', '\\']) {
        name.to_owned()
    } else {
        format!("{}/{name}", directory.trim_end_matches('/'))
    };
    normalize(&joined).unwrap_or(joined)
}

/// What the paths under `/` name: the drive the shell came from, for a name of none of
/// cash's own (`anchor`), and the commands `/usr/bin` holds.
#[derive(Clone, Debug)]
pub struct View {
    anchor: PathBuf,
    commands: Vec<String>,
}

impl View {
    /// The view from a shell whose directory is on the drive whose root is `anchor`
    /// (`C:/`), carrying `commands` (`cash_core::carried::carried_programs`).
    #[must_use]
    pub const fn new(anchor: PathBuf, commands: Vec<String>) -> Self {
        Self { anchor, commands }
    }

    /// The root of the drive a name of none of cash's own is read on, and the directory a
    /// program started in one of cash's own runs in.
    #[must_use]
    pub fn anchor(&self) -> &Path {
        &self.anchor
    }

    /// What the path `path`, which starts with a slash, names; any other path is a file on
    /// disk, read as cash reads every spelling (`crate::path::accept_path`).
    #[must_use]
    pub fn place(&self, path: &str) -> Place {
        let Some(normal) = normalize(path) else {
            return Place::Real(crate::path::accept_path(path));
        };
        if let Some(directory) = Virtual::of_normal(&normal) {
            return Place::Virtual(directory);
        }
        if let Some(name) = normal
            .strip_prefix("/usr/bin/")
            .or_else(|| normal.strip_prefix("/bin/"))
            && self.commands.iter().any(|command| command == name)
        {
            return Place::Command(name.to_owned());
        }
        if normal.starts_with("/dev/") {
            return Place::Device(normal);
        }
        let accepted = crate::path::accept_path(&normal);
        if crate::path::is_absolute(&accepted) {
            return Place::Real(PathBuf::from(crate::path::render(&accepted)));
        }
        let rest = normal.trim_start_matches('/');
        Place::Real(PathBuf::from(crate::path::render(&self.anchor.join(rest))))
    }

    /// What `directory` holds, by name.
    #[must_use]
    pub fn entries(&self, directory: Virtual) -> Vec<Entry> {
        let entry = |name: &str, place: Place| Entry {
            name: name.to_owned(),
            place,
        };
        let mut entries = match directory {
            Virtual::Root => {
                let mut entries: Vec<Entry> = crate::sysinfo::logical_drives()
                    .iter()
                    .filter_map(|drive| drive.chars().next())
                    .map(|letter| {
                        let root = format!("{}:/", letter.to_ascii_uppercase());
                        entry(
                            &letter.to_ascii_lowercase().to_string(),
                            Place::Real(PathBuf::from(root)),
                        )
                    })
                    .collect();
                entries.push(entry("bin", Place::Virtual(Virtual::Bin)));
                entries.push(entry("dev", Place::Virtual(Virtual::Dev)));
                entries.push(entry("tmp", self.place("/tmp")));
                entries.push(entry("usr", Place::Virtual(Virtual::Usr)));
                entries
            }
            Virtual::Usr => vec![entry("bin", Place::Virtual(Virtual::Bin))],
            Virtual::Bin => self
                .commands
                .iter()
                .map(|name| entry(name, Place::Command(name.clone())))
                .collect(),
            Virtual::Dev => crate::devices::NAMES
                .iter()
                .map(|name| entry(name, Place::Device(format!("/dev/{name}"))))
                .collect(),
        };
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    }
}

/// The directory of cash's own that a cash started by a shell in one is in, under
/// `unixpaths`; `None` for any other start.
///
/// The shell hands it on as `PWD` (`/`, `/usr/bin`), and starts its programs at a drive's
/// root, which is `working_dir`.
#[must_use]
pub fn inherited(working_dir: &Path) -> Option<Virtual> {
    if !crate::path::unix_paths_inherited() || drive_root(working_dir) != working_dir {
        return None;
    }
    let pwd = std::env::var("PWD").ok()?;
    Virtual::of_normal(&normalize(&pwd)?)
}

/// `operand`, a relative name a tool of cash's is handed in a directory of cash's own it
/// was started in ([`inherited`]), as the shell resolves it.
///
/// `c/x` at `/` is `C:/x`, `null` at `/dev` is `/dev/null`. `None` for any other operand
/// or start, and for a command under `/usr/bin`, which no tool reads as a file.
#[must_use]
pub fn inherited_operand(operand: &str) -> Option<String> {
    if operand.is_empty() || operand.starts_with(['-', '/', '\\']) {
        return None;
    }
    if crate::path::is_absolute(&crate::path::accept_path(operand)) {
        return None;
    }
    let working_dir = std::env::current_dir().ok()?;
    let working_dir = PathBuf::from(crate::path::render(&working_dir));
    let directory = inherited(&working_dir)?;
    let view = View::new(drive_root(&working_dir), Vec::new());
    match view.place(&join(directory.spelled(), operand)) {
        Place::Real(path) => Some(crate::path::render(&path)),
        Place::Device(spelled) => Some(spelled),
        Place::Virtual(own) => Some(own.spelled().to_owned()),
        Place::Command(_) => None,
    }
}

/// The root of the drive `directory` is on, `D:/` for `D:/src`, or the share's root for a
/// share: where a name of none of cash's own is read from it ([`View`]).
#[must_use]
pub fn drive_root(directory: &Path) -> PathBuf {
    match directory.components().next() {
        Some(std::path::Component::Prefix(prefix)) => {
            let prefix = crate::path::render(Path::new(prefix.as_os_str()));
            PathBuf::from(format!("{}/", prefix.trim_end_matches('/')))
        }
        _ => crate::fs::system_drive_root(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> View {
        View::new(PathBuf::from("Q:/"), vec!["ls".into(), "sed".into()])
    }

    #[test]
    fn a_climb_above_a_drive_or_the_top_reaches_slash() {
        assert_eq!(normalize("/c/..").as_deref(), Some("/"));
        assert_eq!(normalize("/d/x/../..").as_deref(), Some("/"));
        assert_eq!(normalize("/..").as_deref(), Some("/"));
        assert_eq!(normalize("//").as_deref(), Some("/"));
        assert_eq!(normalize("/e/./x//y/").as_deref(), Some("/e/x/y"));
        assert_eq!(normalize(r"\usr\bin").as_deref(), Some("/usr/bin"));
        assert_eq!(normalize("C:/x"), None);
        assert_eq!(normalize("x/.."), None);
    }

    #[test]
    fn a_share_has_nothing_above_it() {
        assert_eq!(
            normalize("//server/share/x/../..").as_deref(),
            Some("//server/share")
        );
        assert_eq!(
            normalize("//server/share/x").as_deref(),
            Some("//server/share/x")
        );
    }

    #[test]
    fn names_join_under_a_directory_as_shown() {
        assert_eq!(join("/", "c"), "/c");
        assert_eq!(join("/c", ".."), "/");
        assert_eq!(join("/usr", "bin/"), "/usr/bin");
        assert_eq!(join("/d/src", "/e/x"), "/e/x");
    }

    #[test]
    fn each_path_names_one_place() {
        let view = view();
        assert_eq!(view.place("/"), Place::Virtual(Virtual::Root));
        assert_eq!(view.place("/c/.."), Place::Virtual(Virtual::Root));
        assert_eq!(view.place("/bin"), Place::Virtual(Virtual::Bin));
        assert_eq!(view.place("/usr/bin/"), Place::Virtual(Virtual::Bin));
        assert_eq!(view.place("/usr"), Place::Virtual(Virtual::Usr));
        assert_eq!(view.place("/dev"), Place::Virtual(Virtual::Dev));
        assert_eq!(view.place("/bin/sed"), Place::Command("sed".into()));
        assert_eq!(view.place("/usr/bin/ls"), Place::Command("ls".into()));
        assert_eq!(view.place("/dev/null"), Place::Device("/dev/null".into()));
        assert_eq!(view.place("/d/x"), Place::Real(PathBuf::from("D:/x")));
        assert_eq!(view.place("/e"), Place::Real(PathBuf::from("E:/")));
        // A name of none of cash's own is the anchor drive's.
        assert_eq!(
            view.place("/Windows/x"),
            Place::Real(PathBuf::from("Q:/Windows/x"))
        );
        assert_eq!(
            view.place("/usr/bin/python3"),
            Place::Real(PathBuf::from("Q:/usr/bin/python3"))
        );
        assert_eq!(view.place("D:/x"), Place::Real(PathBuf::from("D:/x")));
    }

    #[test]
    fn each_directory_lists_what_it_holds() {
        let view = view();
        let names = |directory| {
            view.entries(directory)
                .into_iter()
                .map(|entry| entry.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(Virtual::Usr), ["bin"]);
        assert_eq!(names(Virtual::Bin), ["ls", "sed"]);
        assert_eq!(names(Virtual::Dev), crate::devices::NAMES);
        let root = names(Virtual::Root);
        for name in ["bin", "dev", "tmp", "usr"] {
            assert!(root.iter().any(|entry| entry == name), "{root:?}");
        }
        // The drive Windows is on is one of them, whichever it is.
        let system = crate::path::to_unix(&crate::fs::system_drive_root());
        let letter = system.trim_start_matches('/');
        assert!(
            root.iter().any(|entry| entry == letter),
            "{letter} in {root:?}"
        );
    }

    #[test]
    fn a_directory_s_drive_root_is_where_it_came_from() {
        assert_eq!(drive_root(Path::new("D:/src/x")), PathBuf::from("D:/"));
        assert_eq!(drive_root(Path::new("E:/")), PathBuf::from("E:/"));
        assert_eq!(
            drive_root(Path::new("//server/share/x")),
            PathBuf::from("//server/share/")
        );
    }
}
