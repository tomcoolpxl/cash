//! The tree the picker shows: a folder's entries, read on demand, and broot's layout of
//! several levels trimmed to fit with `… N more` lines (spec D73).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::context::Shows;

/// One file or folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Where it is.
    pub path: PathBuf,
    /// Its name, as listed.
    pub name: String,
    /// A folder rather than a file.
    pub folder: bool,
    /// A name starting with `.`, or the hidden attribute.
    pub hidden: bool,
}

/// Which entries are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter {
    /// Folders only, or files too.
    pub shows: Shows,
    /// Hidden entries too (Alt-.).
    pub hidden: bool,
    /// Entries a `.gitignore` excludes too (Alt-I).
    pub ignored: bool,
}

/// The entries of a folder that `filter` lists, folders first, then by name as Windows
/// orders them (case aside). An unreadable folder has none.
#[must_use]
pub fn read(folder: &Path, filter: Filter, ignores: &mut Ignores) -> Vec<Entry> {
    let Ok(listing) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut entries: Vec<Entry> = listing
        .flatten()
        .filter_map(|item| {
            let name = item.file_name().to_string_lossy().into_owned();
            // On Windows the listing carries each entry's attributes, so this reads no
            // more of the disk.
            let metadata = item.metadata().ok()?;
            let folder = metadata.is_dir();
            let hidden = name.starts_with('.') || attributes_hidden(&metadata);
            let entry = Entry {
                path: item.path(),
                name,
                folder,
                hidden,
            };
            let listed = (folder || filter.shows == Shows::Everything)
                && (filter.hidden || !entry.hidden)
                && (filter.ignored || !ignores.ignored(&entry.path, folder));
            listed.then_some(entry)
        })
        .collect();
    entries.sort_by(|a, b| {
        b.folder
            .cmp(&a.folder)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

fn attributes_hidden(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
}

/// `.gitignore` rules, read once per folder: an entry is ignored as `git status` would
/// ignore it, by the `.gitignore` files from its repository's root down to its folder.
#[derive(Default)]
pub struct Ignores {
    /// Each folder's own rules, `None` when it has no `.gitignore`.
    rules: HashMap<PathBuf, Option<ignore::gitignore::Gitignore>>,
    /// Each folder's repository root, `None` outside a repository.
    roots: HashMap<PathBuf, Option<PathBuf>>,
}

impl Ignores {
    /// Whether a `.gitignore` excludes `path`.
    pub fn ignored(&mut self, path: &Path, folder: bool) -> bool {
        if path.file_name().is_some_and(|name| name == ".git") {
            return true;
        }
        let Some(parent) = path.parent() else {
            return false;
        };
        let Some(root) = self.root(parent) else {
            return false;
        };
        // The deepest rule decides; a rule nearer the file overrides one above it.
        let mut folder_at = Some(parent);
        while let Some(at) = folder_at {
            if let Some(rules) = self.rules_of(at) {
                match rules.matched(path, folder) {
                    ignore::Match::Ignore(_) => return true,
                    ignore::Match::Whitelist(_) => return false,
                    ignore::Match::None => {}
                }
            }
            if at == root {
                break;
            }
            folder_at = at.parent();
        }
        // Inside an ignored folder, everything is ignored.
        path.parent()
            .filter(|parent| *parent != root)
            .is_some_and(|parent| self.ignored(parent, true))
    }

    fn rules_of(&mut self, folder: &Path) -> Option<&ignore::gitignore::Gitignore> {
        self.rules
            .entry(folder.to_path_buf())
            .or_insert_with(|| {
                let file = folder.join(".gitignore");
                file.is_file().then(|| {
                    let mut builder = ignore::gitignore::GitignoreBuilder::new(folder);
                    builder.add(&file);
                    builder.build().ok()
                })?
            })
            .as_ref()
    }

    fn root(&mut self, folder: &Path) -> Option<PathBuf> {
        if let Some(known) = self.roots.get(folder) {
            return known.clone();
        }
        let found = folder
            .ancestors()
            .find(|at| at.join(".git").exists())
            .map(Path::to_path_buf);
        self.roots.insert(folder.to_path_buf(), found.clone());
        found
    }
}

/// A line of the tree as drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// An entry, at a depth below the root (1 for the root's own entries), with the tree
    /// lines drawn before it: for each level above, whether a line continues there.
    Entry {
        /// What the line shows.
        entry: Entry,
        /// 1 for the root's own entries, 2 for theirs, and so on.
        depth: usize,
        /// Whether it is its folder's last line, so its branch ends here (`└─`).
        last: bool,
        /// For each level above it, whether that level's branch goes on below it (`│`).
        rails: Vec<bool>,
    },
    /// `… N more` entries not shown, at a depth.
    More {
        /// How many entries the line stands for.
        count: usize,
        /// The depth of the entries it stands for.
        depth: usize,
        /// As for an entry.
        rails: Vec<bool>,
    },
}

/// A folder in the layout, with the entries shown and how many are not.
struct Node {
    entry: Entry,
    children: Option<Vec<Self>>,
    more: usize,
}

/// broot's layout of `top`, the root's entries, in `rows` rows.
///
/// While rows remain, the entries of each shown folder are added, level by level, as many
/// as fit, the rest counted in a `… N more` line. `rows` is the room below the root's own
/// line. `children` reads a folder (cached by the caller), so a deeper level costs a read
/// only when there is room to show it.
pub fn layout(
    top: Vec<Entry>,
    rows: usize,
    children: &mut dyn FnMut(&Path) -> Vec<Entry>,
) -> Vec<Line> {
    let mut nodes: Vec<Node> = Vec::new();
    let top_total = top.len();
    let mut used = fit(top, rows, &mut nodes);
    let top_more = top_total - nodes.len();
    // `frontier`: index paths of the shown folders at the level being opened.
    let mut frontier: Vec<Vec<usize>> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.entry.folder)
        .map(|(i, _)| vec![i])
        .collect();
    while used < rows && !frontier.is_empty() {
        let mut next = Vec::new();
        for at in frontier {
            if used >= rows {
                break;
            }
            let Some(node) = node_at(&mut nodes, &at) else {
                continue;
            };
            let entries = children(&node.entry.path);
            if entries.is_empty() {
                continue;
            }
            let mut shown = Vec::new();
            let more = entries.len();
            used += fit(entries, rows - used, &mut shown);
            let Some(node) = node_at(&mut nodes, &at) else {
                continue;
            };
            node.more = more - shown.len();
            for (i, child) in shown.iter().enumerate() {
                if child.entry.folder {
                    let mut path = at.clone();
                    path.push(i);
                    next.push(path);
                }
            }
            node.children = Some(shown);
        }
        frontier = next;
    }
    let mut lines = Vec::new();
    draw(&nodes, 0, &mut Vec::new(), &mut lines);
    if top_more > 0 {
        lines.push(Line::More {
            count: top_more,
            depth: 1,
            rails: Vec::new(),
        });
    }
    lines
}

/// Takes from `entries` as many as fit in `room` rows, keeping one for a `… N more` line
/// when they do not all fit; returns the rows used.
fn fit(entries: Vec<Entry>, room: usize, into: &mut Vec<Node>) -> usize {
    if room == 0 {
        return 0;
    }
    let total = entries.len();
    let take = if total <= room { total } else { room - 1 };
    into.extend(entries.into_iter().take(take).map(|entry| Node {
        entry,
        children: None,
        more: 0,
    }));
    take + usize::from(take < total)
}

/// The node at the index path `at`; paths come from the layout itself, so one always
/// leads to a node.
fn node_at<'a>(nodes: &'a mut [Node], at: &[usize]) -> Option<&'a mut Node> {
    let (first, rest) = at.split_first()?;
    let mut node = nodes.get_mut(*first)?;
    for i in rest {
        node = node.children.as_mut()?.get_mut(*i)?;
    }
    Some(node)
}

fn draw(nodes: &[Node], depth: usize, rails: &mut Vec<bool>, lines: &mut Vec<Line>) {
    for (i, node) in nodes.iter().enumerate() {
        let last = i + 1 == nodes.len();
        lines.push(Line::Entry {
            entry: node.entry.clone(),
            depth: depth + 1,
            last,
            rails: rails.clone(),
        });
        if let Some(children) = &node.children {
            rails.push(!last);
            draw(children, depth + 1, rails, lines);
            if node.more > 0 {
                lines.push(Line::More {
                    count: node.more,
                    depth: depth + 2,
                    rails: rails.clone(),
                });
            }
            rails.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, folder: bool) -> Entry {
        let path = PathBuf::from(path);
        Entry {
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            path,
            folder,
            hidden: false,
        }
    }

    fn names(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|line| match line {
                Line::Entry { entry, depth, .. } => format!("{depth}:{}", entry.name),
                Line::More { count, depth, .. } => format!("{depth}:+{count}"),
            })
            .collect()
    }

    /// r/{a/{a1,a2,a3}, b/{b1}, c.txt}
    fn sample(path: &Path) -> Vec<Entry> {
        match path.to_str().unwrap() {
            "r/a" => vec![
                entry("r/a/a1", false),
                entry("r/a/a2", false),
                entry("r/a/a3", false),
            ],
            "r/b" => vec![entry("r/b/b1", false)],
            _ => Vec::new(),
        }
    }

    fn top() -> Vec<Entry> {
        vec![
            entry("r/a", true),
            entry("r/b", true),
            entry("r/c.txt", false),
        ]
    }

    #[test]
    fn everything_shows_when_there_is_room() {
        let lines = layout(top(), 20, &mut sample);
        assert_eq!(
            names(&lines),
            ["1:a", "2:a1", "2:a2", "2:a3", "1:b", "2:b1", "1:c.txt"]
        );
    }

    #[test]
    fn a_level_that_does_not_fit_ends_in_more() {
        // 3 rows for the top level, 2 left: a's three files become a1 and "+2".
        let lines = layout(top(), 5, &mut sample);
        assert_eq!(names(&lines), ["1:a", "2:a1", "2:+2", "1:b", "1:c.txt"]);
        let lines = layout(top(), 2, &mut sample);
        assert_eq!(names(&lines), ["1:a", "1:+2"]);
    }

    #[test]
    fn rails_show_where_branches_go_on() {
        let lines = layout(top(), 20, &mut sample);
        // a1 under a, which has siblings below it: a's branch goes on.
        assert!(
            matches!(&lines[1], Line::Entry { rails, last: false, .. } if rails == &[true]),
            "{:?}",
            lines[1]
        );
    }

    #[test]
    fn a_folder_lists_folders_first_and_leaves_out_hidden_and_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::write(root.join(".gitignore"), "target/\n*.log\n").unwrap();
        for folder in ["target", "src", "Docs"] {
            std::fs::create_dir(root.join(folder)).unwrap();
        }
        for file in ["b.txt", "a.log", ".env"] {
            std::fs::write(root.join(file), "").unwrap();
        }
        let filter = Filter {
            shows: Shows::Everything,
            hidden: false,
            ignored: false,
        };
        let listed = |filter: Filter| -> Vec<String> {
            read(root, filter, &mut Ignores::default())
                .into_iter()
                .map(|e| e.name)
                .collect()
        };
        assert_eq!(listed(filter), ["Docs", "src", "b.txt"]);
        assert_eq!(
            listed(Filter {
                shows: Shows::Folders,
                ..filter
            }),
            ["Docs", "src"]
        );
        assert_eq!(
            listed(Filter {
                ignored: true,
                ..filter
            }),
            ["Docs", "src", "target", "a.log", "b.txt"]
        );
        assert_eq!(
            listed(Filter {
                hidden: true,
                ..filter
            }),
            ["Docs", "src", ".env", ".gitignore", "b.txt"]
        );
    }
}
