//! 7-Zip's censor (`Wildcard.cpp`): the masks a command's names and `-i`/`-x` switches
//! give, kept as a tree of folders, and whether an item's path is in or out.
//!
//! Names are compared without case, as 7-Zip on Windows compares them, unless `-ssc`.

/// Whether a mask reaches into subfolders (`-r`, `-r-`, `-r0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Recursion {
    None,
    /// `-r0`: only masks with wildcards.
    WildcardOnly,
    Always,
}

/// Whether a mask names files, folders or both (`-spm`, `-im`, `-xm`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MarkMode {
    FileOrDir,
    StrictFile,
    StrictFileIfWildcard,
}

/// How a name's leading folders are taken: kept whole (extraction and listing, which
/// match against names inside an archive), or split into a prefix on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PathMode {
    Absolute,
    Relative,
}

#[derive(Clone, Debug)]
struct PreItem {
    include: bool,
    path: String,
    recursive: Recursion,
    wildcards: bool,
    mark: MarkMode,
}

/// One mask: its path parts, and what it may match.
#[derive(Clone, Debug)]
struct Item {
    parts: Vec<String>,
    for_file: bool,
    for_dir: bool,
    recursive: bool,
    wildcards: bool,
}

impl Item {
    fn all_allowed(&self) -> bool {
        self.for_file
            && self.for_dir
            && self.wildcards
            && self.parts.len() == 1
            && self.parts[0] == "*"
    }

    /// `CItem::CheckPath`.
    fn check(&self, path: &[String], is_file: bool, case: bool) -> bool {
        if !is_file && !self.for_dir {
            return false;
        }
        let Some(delta) = path.len().checked_sub(self.parts.len()) else {
            return false;
        };
        let mut start = 0;
        let mut finish = 0;
        if is_file {
            if !self.for_dir {
                if self.recursive {
                    start = delta;
                } else if delta != 0 {
                    return false;
                }
            }
            if !self.for_file && delta == 0 {
                return false;
            }
        }
        if self.recursive {
            finish = delta;
            if is_file && !self.for_file {
                match delta.checked_sub(1) {
                    Some(f) => finish = f,
                    None => return false,
                }
            }
        }
        (start..=finish).any(|d| {
            self.parts.iter().enumerate().all(|(i, mask)| {
                let name = &path[i + d];
                if self.wildcards {
                    mask_matches(mask, name, case)
                } else {
                    compare_names(mask, name, case)
                }
            })
        })
    }
}

/// A folder of the censor: its masks, and those of the folders under it.
#[derive(Clone, Debug, Default)]
pub(super) struct Node {
    name: String,
    includes: Vec<Item>,
    excludes: Vec<Item>,
    children: Vec<Self>,
}

impl Node {
    fn child_mut(&mut self, name: &str, case: bool) -> &mut Self {
        let at = if let Some(at) = self
            .children
            .iter()
            .position(|c| compare_names(&c.name, name, case))
        {
            at
        } else {
            self.children.push(Self {
                name: name.to_owned(),
                ..Self::default()
            });
            self.children.len() - 1
        };
        &mut self.children[at]
    }

    fn add(&mut self, include: bool, mut item: Item, ignore_wildcard_index: i32, case: bool) {
        if item.parts.len() <= 1 {
            if item.parts.len() == 1 && item.wildcards && !has_wildcard(&item.parts[0]) {
                item.wildcards = false;
            }
            if include {
                self.includes.push(item);
            } else {
                self.excludes.push(item);
            }
            return;
        }
        let front = item.parts[0].clone();
        if item.wildcards && ignore_wildcard_index != 0 && has_wildcard(&front) {
            if include {
                self.includes.push(item);
            } else {
                self.excludes.push(item);
            }
            return;
        }
        item.parts.remove(0);
        self.child_mut(&front, case)
            .add(include, item, ignore_wildcard_index - 1, case);
    }

    /// `CCensorNode::AreAllAllowed`: one `*` and nothing else.
    pub(super) fn all_allowed(&self) -> bool {
        self.name.is_empty()
            && self.children.is_empty()
            && self.excludes.is_empty()
            && self.includes.len() == 1
            && self.includes[0].all_allowed()
    }

    fn check_current(&self, include: bool, path: &[String], is_file: bool, case: bool) -> bool {
        let items = if include {
            &self.includes
        } else {
            &self.excludes
        };
        items.iter().any(|item| item.check(path, is_file, case))
    }

    /// `CCensorNode::NeedCheckSubDirs`: a mask here reaches below this folder.
    pub(super) fn need_check_sub_dirs(&self) -> bool {
        self.includes
            .iter()
            .any(|item| item.recursive || item.parts.len() > 1)
    }

    /// `CCensorNode::AreThereIncludeItems`: a mask here or under it takes something.
    pub(super) fn are_there_include_items(&self) -> bool {
        !self.includes.is_empty() || self.children.iter().any(Self::are_there_include_items)
    }

    /// `CCensorNode::FindSubNode`.
    pub(super) fn find_sub_node(&self, name: &str, case: bool) -> Option<usize> {
        self.children
            .iter()
            .position(|c| compare_names(&c.name, name, case))
    }

    /// `CanUseFsDirect`: when every mask here is one plain name, those names, each with
    /// whether it may be a file and a folder; the folder need not be listed.
    pub(super) fn direct_names(&self) -> Option<Vec<(String, bool, bool)>> {
        self.includes
            .iter()
            .map(|item| {
                let plain =
                    !item.recursive && item.parts.len() == 1 && !has_wildcard(&item.parts[0]);
                plain.then(|| (item.parts[0].clone(), item.for_file, item.for_dir))
            })
            .collect()
    }

    /// `CCensorNode::CheckPathVect`: whether some mask speaks of the path, and if so,
    /// whether it is in.
    fn check_vect(&self, path: &[String], is_file: bool, case: bool) -> Option<bool> {
        if self.check_current(false, path, is_file, case) {
            return Some(false);
        }
        if path.len() > 1
            && let Some(child) = self
                .children
                .iter()
                .find(|c| compare_names(&c.name, &path[0], case))
            && let Some(include) = child.check_vect(&path[1..], is_file, case)
        {
            return Some(include);
        }
        self.check_current(true, path, is_file, case)
            .then_some(true)
    }

    fn extend_exclude(&mut self, from: &Self, case: bool) {
        self.excludes.extend(from.excludes.iter().cloned());
        for node in &from.children {
            self.child_mut(&node.name, case).extend_exclude(node, case);
        }
    }

    /// The folders under this one that masks name.
    pub(super) fn children(&self) -> &[Self] {
        &self.children
    }

    /// This folder's name.
    pub(super) fn name(&self) -> &str {
        &self.name
    }

    /// The masks at this level, for walking a folder on disk.
    pub(super) fn masks(&self) -> impl Iterator<Item = (&[String], bool)> {
        self.includes
            .iter()
            .map(|i| (i.parts.as_slice(), i.wildcards))
    }
}

/// All of a command's masks, by the prefix their names began with.
#[derive(Clone, Debug, Default)]
pub(super) struct Censor {
    pre_items: Vec<PreItem>,
    pairs: Vec<(String, Node)>,
    pub(super) exclude_dirs: bool,
    pub(super) exclude_files: bool,
    /// Names compared with their case (`-ssc`).
    pub(super) case_sensitive: bool,
}

impl Censor {
    pub(super) fn add_pre_item(
        &mut self,
        include: bool,
        path: &str,
        recursive: Recursion,
        wildcards: bool,
        mark: MarkMode,
    ) {
        self.pre_items.push(PreItem {
            include,
            path: path.to_owned(),
            recursive,
            wildcards,
            mark,
        });
    }

    /// `CCensor::AddPathsToCensor`.
    pub(super) fn add_paths(&mut self, mode: PathMode) {
        for pre in std::mem::take(&mut self.pre_items) {
            self.add_item(mode, &pre);
        }
    }

    /// `CCensor::AddItem`.
    fn add_item(&mut self, mode: PathMode, pre: &PreItem) {
        let recursive = match pre.recursive {
            Recursion::None => false,
            Recursion::WildcardOnly => has_wildcard(&pre.path),
            Recursion::Always => true,
        };
        let mut parts = split_path(&pre.path);
        let mut for_file = true;
        let mut for_dir = true;
        let mut wildcards = pre.wildcards;
        let mut recursive = recursive;
        if parts.last().is_some_and(String::is_empty) {
            for_file = false;
            parts.pop();
        } else if let Some(back) = parts.last()
            && (pre.mark == MarkMode::StrictFile
                || (pre.mark == MarkMode::StrictFileIfWildcard && has_wildcard(back)))
        {
            for_dir = false;
        }
        let mut prefix = String::new();
        let mut ignore_wildcard_index =
            if parts.len() >= 3 && parts[0].is_empty() && parts[1].is_empty() && parts[2] == "?" {
                2
            } else {
                -1
            };
        if mode == PathMode::Relative {
            ignore_wildcard_index = -1;
            let prefix_parts = prefix_part_count(&parts);
            let dots = (prefix_parts..parts.len())
                .rev()
                .find(|&i| parts[i] == ".." || parts[i] == ".");
            let skip = match dots {
                Some(dots) if dots == parts.len() - 1 => parts.len(),
                Some(_) => parts.len() - 1,
                None if prefix_parts != 0 && parts.len() > prefix_parts => parts.len() - 1,
                None => prefix_parts,
            };
            for i in 0..skip {
                let Some(front) = parts.first() else {
                    break;
                };
                if wildcards && i >= prefix_parts && has_wildcard(front) {
                    break;
                }
                prefix.push_str(front);
                prefix.push('/');
                parts.remove(0);
            }
            if parts.is_empty() || (parts.len() == 1 && parts[0].is_empty()) {
                parts = vec!["*".to_owned()];
                for_file = true;
                for_dir = true;
                wildcards = true;
                recursive = false;
            }
        }
        let case = self.case_sensitive;
        let at = if let Some(at) = self
            .pairs
            .iter()
            .position(|(p, _)| compare_names(p, &prefix, case))
        {
            at
        } else {
            self.pairs.push((prefix, Node::default()));
            self.pairs.len() - 1
        };
        let item = Item {
            parts,
            for_file,
            for_dir,
            recursive,
            wildcards,
        };
        self.pairs[at]
            .1
            .add(pre.include, item, ignore_wildcard_index, case);
    }

    /// `CCensor::ExtendExclude`: the excludes without a prefix apply under every prefix.
    pub(super) fn extend_exclude(&mut self) {
        let Some(base) = self.pairs.iter().position(|(p, _)| p.is_empty()) else {
            return;
        };
        let from = self.pairs[base].1.clone();
        let case = self.case_sensitive;
        for (i, (_, node)) in self.pairs.iter_mut().enumerate() {
            if i != base {
                node.extend_exclude(&from, case);
            }
        }
    }

    /// The first prefix's tree, which extraction and listing check items against.
    pub(super) fn head(&self) -> Option<&Node> {
        self.pairs.first().map(|(_, node)| node)
    }

    /// The prefixes and their trees, for walking the disk.
    pub(super) fn pairs(&self) -> &[(String, Node)] {
        &self.pairs
    }

    /// Whether an item inside an archive is wanted: `CensorNode_CheckPath`.
    pub(super) fn wants(&self, path: &str, is_dir: bool) -> bool {
        if self.exclude_dirs && is_dir || self.exclude_files && !is_dir {
            return false;
        }
        let Some(head) = self.head() else {
            return false;
        };
        let parts = split_path(path);
        head.check_vect(&parts, !is_dir, self.case_sensitive) == Some(true)
    }

    /// Whether every item is wanted without looking.
    pub(super) fn all_allowed(&self) -> bool {
        self.pairs.len() == 1 && self.pairs[0].0.is_empty() && self.pairs[0].1.all_allowed()
    }

    /// Whether an item already in an archive is one an update's names speak of
    /// (`Censor_CheckPath`): some prefix's tree takes it and none leaves it out, the
    /// prefixes aside.
    pub(super) fn takes(&self, path: &str, is_dir: bool) -> bool {
        if self.pairs.len() == 1 && self.pairs[0].1.all_allowed() {
            return true;
        }
        let parts = split_path(path);
        let mut found = false;
        for (_, node) in &self.pairs {
            match node.check_vect(&parts, !is_dir, self.case_sensitive) {
                Some(false) => return false,
                Some(true) => found = true,
                None => {}
            }
        }
        found
    }
}

/// `CCensorNode::CheckPathToRoot`: whether a mask of the last folder of `stack`, or of
/// a folder above it (the path growing by each folder's name), speaks of `path`.
pub(super) fn check_to_root(
    stack: &[&Node],
    include: bool,
    path: &[String],
    is_file: bool,
    case: bool,
) -> bool {
    let mut parts = path.to_vec();
    for (at, node) in stack.iter().enumerate().rev() {
        if node.check_current(include, &parts, is_file, case) {
            return true;
        }
        if at > 0 {
            parts.insert(0, node.name.clone());
        }
    }
    false
}

/// `CompareFileNames`: as 7-Zip sorts names on Windows, without case unless `case`, and
/// with `/` as the `\` it is there.
pub(super) fn compare_file_names(a: &str, b: &str, case: bool) -> std::cmp::Ordering {
    let key = |c: char| {
        let c = if c == '/' { '\\' } else { c };
        if case {
            c
        } else {
            c.to_uppercase().next().unwrap_or(c)
        }
    };
    a.chars().map(key).cmp(b.chars().map(key))
}

/// `SplitPathToParts`: at `/` and `\`.
pub(super) fn split_path(path: &str) -> Vec<String> {
    if path.is_empty() {
        return Vec::new();
    }
    path.split(['/', '\\']).map(str::to_owned).collect()
}

/// `GetNumPrefixParts`: a drive (`C:`), a root (`\`), a share (`\\server\share`).
fn prefix_part_count(parts: &[String]) -> usize {
    let Some(first) = parts.first() else {
        return 0;
    };
    if is_drive(first) {
        return 1;
    }
    if !first.is_empty() {
        return 0;
    }
    if parts.len() == 1 || !parts[1].is_empty() {
        return 1;
    }
    if parts.len() == 2 {
        return 2;
    }
    if parts[2] == "." {
        return 3;
    }
    let mut network = if parts[2] == "?" {
        if parts.len() == 3 {
            return 3;
        }
        if is_drive(&parts[3]) {
            return 4;
        }
        if !parts[3].eq_ignore_ascii_case("UNC") {
            return 3;
        }
        4
    } else {
        2
    };
    network += 1;
    if parts.len() <= network {
        parts.len()
    } else {
        network
    }
}

const fn is_drive(part: &str) -> bool {
    let bytes = part.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// `DoesNameContainWildcard`.
pub(super) fn has_wildcard(name: &str) -> bool {
    name.contains(['*', '?'])
}

/// `CompareFileNames` for equality.
fn compare_names(a: &str, b: &str, case: bool) -> bool {
    if case {
        a == b
    } else {
        a.chars()
            .flat_map(char::to_uppercase)
            .eq(b.chars().flat_map(char::to_uppercase))
    }
}

/// `EnhancedMaskTest`: `*` and `?`, without case unless `case`.
pub(super) fn mask_matches(mask: &str, name: &str, case: bool) -> bool {
    let mask: Vec<char> = mask.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let same = |m: char, c: char| m == c || (!case && m.to_uppercase().eq(c.to_uppercase()));
    // Iterative `*` matching with one backtrack point, as the recursion amounts to.
    let (mut m, mut n) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        if m < mask.len() && mask[m] == '*' {
            star = Some((m, n));
            m += 1;
        } else if m < mask.len() && (mask[m] == '?' || same(mask[m], name[n])) {
            m += 1;
            n += 1;
        } else if let Some((sm, sn)) = star {
            m = sm + 1;
            n = sn + 1;
            star = Some((sm, sn + 1));
        } else {
            return false;
        }
    }
    mask[m..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn censor(names: &[&str], excludes: &[&str], recursion: Recursion) -> Censor {
        let mut censor = Censor::default();
        if names.is_empty() {
            censor.add_pre_item(true, "*", Recursion::None, true, MarkMode::FileOrDir);
        }
        for name in names {
            censor.add_pre_item(true, name, recursion, true, MarkMode::FileOrDir);
        }
        for name in excludes {
            censor.add_pre_item(false, name, Recursion::None, true, MarkMode::FileOrDir);
        }
        censor.add_paths(PathMode::Absolute);
        censor.extend_exclude();
        censor
    }

    #[test]
    fn a_folder_name_takes_what_is_under_it() {
        let c = censor(&["d"], &[], Recursion::None);
        assert!(c.wants("d", true));
        assert!(c.wants("d/a.txt", false));
        assert!(c.wants("d/sub/b.txt", false));
        assert!(!c.wants("text.txt", false));
    }

    #[test]
    fn a_path_names_that_item_alone() {
        let c = censor(&["d/a.txt"], &[], Recursion::None);
        assert!(c.wants("d/a.txt", false));
        assert!(!c.wants("d", true));
        assert!(!c.wants("d/sub/b.txt", false));
    }

    #[test]
    fn wildcards_reach_down_only_with_r() {
        let c = censor(&["*.txt"], &[], Recursion::None);
        assert!(c.wants("text.txt", false));
        assert!(!c.wants("d/a.txt", false));
        let c = censor(&["*.txt"], &[], Recursion::Always);
        assert!(c.wants("d/a.txt", false));
        assert!(c.wants("d/sub/b.txt", false));
        assert!(!c.wants("d", true));
    }

    #[test]
    fn excludes_win() {
        let c = censor(&[], &["text.txt"], Recursion::None);
        assert!(!c.wants("text.txt", false));
        assert!(c.wants("d/a.txt", false));
        assert!(!c.all_allowed());
        assert!(censor(&[], &[], Recursion::None).all_allowed());
    }

    #[test]
    fn masks_match_as_7_zip_does() {
        assert!(mask_matches("*.TXT", "a.txt", false));
        assert!(!mask_matches("*.TXT", "a.txt", true));
        assert!(mask_matches("a*b*c", "aXbYbZc", false));
        assert!(mask_matches("?", "x", false));
        assert!(!mask_matches("?", "", false));
        assert!(mask_matches("*", "", false));
    }
}
