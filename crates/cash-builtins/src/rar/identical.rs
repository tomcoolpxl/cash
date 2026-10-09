//! `-oi`: the identical files among those archived, found before archiving, each set's
//! first kept as a file and the others as references to it, as `Rar.exe` 7.23 does; or,
//! with `-oi3` and `-oi4`, only listed.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Read as _;
use std::path::Path;

/// `-oi`'s mode and the least size it looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Identical {
    /// 1 keeps references, 2 lists the sets first as well; 3 lists the sets and 4 the
    /// names of their duplicates, and neither writes an archive.
    pub(super) level: u8,
    pub(super) least: u64,
}

/// `-oi[0-4][:SIZE]`, from the text after `oi`; `None` when off. SIZE is 64 KB unless
/// given, in bytes or with a unit: `k`, `m`, `g` or `t` in powers of 1,024, upper case in
/// powers of 1,000.
pub(super) fn parse(text: &str) -> Option<Identical> {
    let (mode, size) = match text.split_once(':') {
        Some((mode, size)) => (mode, Some(size)),
        None => (text, None),
    };
    let level = match mode {
        "" | "1" => 1,
        "2" => 2,
        "3" => 3,
        "4" => 4,
        _ => return None,
    };
    let least = size.and_then(parse_size).unwrap_or(64 * 1024);
    Some(Identical { level, least })
}

fn parse_size(text: &str) -> Option<u64> {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let (digits, unit) = text.split_at(end);
    let count: u64 = digits.parse().ok()?;
    let factor: u64 = match unit {
        "" | "b" | "B" => 1,
        "k" => 1 << 10,
        "K" => 1_000,
        "m" => 1 << 20,
        "M" => 1_000_000,
        "g" => 1 << 30,
        "G" => 1_000_000_000,
        "t" => 1 << 40,
        "T" => 1_000_000_000_000,
        _ => return None,
    };
    count.checked_mul(factor)
}

/// The sets of identical files among `files` (each its index, size and path): files of
/// `least` bytes or more with the same size and the same bytes, each set in the order
/// given and the sets by size, smallest first. Also whether any file was big enough to
/// look at.
pub(super) fn sets(files: &[(usize, u64, &Path)], least: u64) -> (bool, Vec<Vec<usize>>) {
    let mut by_size: BTreeMap<u64, Vec<(usize, &Path)>> = BTreeMap::new();
    for &(index, size, path) in files {
        if size >= least {
            by_size.entry(size).or_default().push((index, path));
        }
    }
    let looked = !by_size.is_empty();
    let mut sets = Vec::new();
    for same_size in by_size.into_values().filter(|files| files.len() > 1) {
        // Each content in the order its first file came.
        let mut contents: Vec<([u8; 32], Vec<usize>)> = Vec::new();
        for (index, path) in same_size {
            let Some(digest) = digest(path) else {
                continue;
            };
            match contents.iter_mut().find(|(seen, _)| *seen == digest) {
                Some((_, set)) => set.push(index),
                None => contents.push((digest, vec![index])),
            }
        }
        sets.extend(
            contents
                .into_iter()
                .map(|(_, set)| set)
                .filter(|set| set.len() > 1),
        );
    }
    (looked, sets)
}

/// A file's SHA-256, or `None` when it cannot be read.
fn digest(path: &Path) -> Option<[u8; 32]> {
    use sha2::Digest as _;
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            return Some(hasher.finalize().into());
        }
        hasher.update(&buffer[..read]);
    }
}

/// How many files the sets hold beyond their first ones: those kept as references.
pub(super) fn found(sets: &[Vec<usize>]) -> usize {
    sets.iter().map(|set| set.len() - 1).sum()
}

/// What `-oi` and `-oi2` say before the files are added, `line` giving a file's line in
/// the list: its size in twelve columns and its name.
pub(super) fn searched(
    level: u8,
    looked: bool,
    sets: &[Vec<usize>],
    line: &dyn Fn(usize) -> String,
) -> String {
    let mut text = String::from("\nSearching for identical files");
    if !looked {
        text.push('\n');
        return text;
    }
    let found = found(sets);
    if level == 2 && !sets.is_empty() {
        text.push_str("      \n\n");
        let listed: Vec<String> = sets
            .iter()
            .map(|set| {
                set.iter()
                    .map(|&index| line(index))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect();
        text.push_str(&listed.join("\n\n"));
        let _ = writeln!(text, "   {found} found.");
    } else {
        let _ = writeln!(text, "         {found} found.");
    }
    text
}

/// What `-oi3` and `-oi4` print in place of an archive: the sets with a blank line
/// between them and the count, or bare the names of the files beyond each set's first.
pub(super) fn listed(
    level: u8,
    looked: bool,
    sets: &[Vec<usize>],
    line: &dyn Fn(usize) -> String,
    name: &dyn Fn(usize) -> String,
) -> String {
    if level == 4 {
        let mut names = String::new();
        for &index in sets.iter().flat_map(|set| set.iter().skip(1)) {
            let _ = writeln!(names, "{}", name(index));
        }
        return names;
    }
    let mut text = String::from("\nSearching for identical files");
    if !looked {
        text.push_str("\n\n");
        return text;
    }
    text.push_str("      \n\n");
    for (number, set) in sets.iter().enumerate() {
        if number > 0 {
            text.push('\n');
        }
        for &index in set {
            let _ = writeln!(text, "{}", line(index));
        }
    }
    if !sets.is_empty() {
        text.push('\n');
    }
    let _ = write!(text, "{} found.\n\n", found(sets));
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_reads_its_mode_and_least_size_as_rar_txt_gives_them() {
        let at = |level, least| Some(Identical { level, least });
        assert_eq!(parse(""), at(1, 65_536));
        assert_eq!(parse("2"), at(2, 65_536));
        assert_eq!(parse("3:1M"), at(3, 1_000_000));
        assert_eq!(parse("4:64k"), at(4, 65_536));
        assert_eq!(parse(":10"), at(1, 10));
        assert_eq!(parse("1:1K"), at(1, 1_000));
        assert_eq!(parse("-"), None);
        assert_eq!(parse("0"), None);
    }

    #[test]
    fn the_lines_are_rar_s_with_its_percentages_taken_out() {
        let sets = vec![vec![0, 1], vec![2, 3, 4]];
        let line = |index: usize| format!("{:>12}  f{index}", 70_000 + index);
        let name = |index: usize| format!("f{index}");
        assert_eq!(
            searched(1, true, &sets, &line),
            "\nSearching for identical files         3 found.\n"
        );
        assert_eq!(
            searched(2, true, &sets, &line),
            "\nSearching for identical files      \n\n       70000  f0\n       70001  f1\n\n       70002  f2\n       70003  f3\n       70004  f4   3 found.\n"
        );
        assert_eq!(
            searched(2, true, &[], &line),
            "\nSearching for identical files         0 found.\n"
        );
        assert_eq!(
            searched(1, false, &[], &line),
            "\nSearching for identical files\n"
        );
        assert_eq!(
            listed(3, true, &sets, &line, &name),
            "\nSearching for identical files      \n\n       70000  f0\n       70001  f1\n\n       70002  f2\n       70003  f3\n       70004  f4\n\n3 found.\n\n"
        );
        assert_eq!(
            listed(3, true, &[], &line, &name),
            "\nSearching for identical files      \n\n0 found.\n\n"
        );
        assert_eq!(
            listed(3, false, &[], &line, &name),
            "\nSearching for identical files\n\n"
        );
        assert_eq!(listed(4, true, &sets, &line, &name), "f1\nf3\nf4\n");
    }
}
