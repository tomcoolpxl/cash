//! Ordering of pathname-expansion results under Bash 5.3's `GLOBSORT` variable.
//!
//! The rules follow `setup_globsort` and `sh_sortglob` in Bash's `pathexp.c`:
//!
//! * the value is an optional `+` (ascending, the default) or `-` (descending), then one
//!   of `name`, `size`, `mtime`, `atime`, `ctime`, `blocks`, `numeric` or `nosort`;
//! * unset, empty or unrecognised values keep the historical ascending name order, and a
//!   bare `+` or `-` sorts by name in that direction;
//! * `nosort` skips the final sort; cash's directory walk already yields name order;
//! * ties on size, a timestamp or block count fall back to the name, in the same
//!   direction; `numeric` orders all-digit names numerically, ahead of other names.
//!
//! Windows has no inode change time, so `ctime` uses the creation time, and it has no
//! block count, so `blocks` counts 512-byte units of the file size.

use std::cmp::Ordering;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SortKey {
    Name,
    Size,
    Mtime,
    Atime,
    Ctime,
    Blocks,
    Numeric,
    NoSort,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GlobSort {
    key: SortKey,
    reverse: bool,
}

impl GlobSort {
    /// Parses a `GLOBSORT` value.
    fn parse(value: Option<&str>) -> Self {
        let default = Self {
            key: SortKey::Name,
            reverse: false,
        };
        let Some(value) = value.map(|v| v.trim_start_matches([' ', '\t'])) else {
            return default;
        };
        if value.is_empty() {
            return default;
        }

        let (reverse, rest) = if let Some(rest) = value.strip_prefix('-') {
            (true, rest)
        } else {
            (false, value.strip_prefix('+').unwrap_or(value))
        };

        let key = match rest {
            "" | "name" => SortKey::Name,
            "size" => SortKey::Size,
            "mtime" => SortKey::Mtime,
            "atime" => SortKey::Atime,
            "ctime" => SortKey::Ctime,
            "blocks" => SortKey::Blocks,
            "numeric" => SortKey::Numeric,
            "nosort" => SortKey::NoSort,
            // Any other value is the historical behavior, ignoring the direction.
            _ => return default,
        };
        Self { key, reverse }
    }
}

/// What a sort key needs to know about one file. Stat failures sort first, as Bash's
/// `glob_nullstat` of -1 values does.
#[derive(Default)]
struct FileInfo {
    size: Option<u64>,
    mtime: Option<SystemTime>,
    atime: Option<SystemTime>,
    ctime: Option<SystemTime>,
}

impl FileInfo {
    fn read(path: &std::path::Path) -> Self {
        let Ok(metadata) = std::fs::symlink_metadata(path) else {
            return Self::default();
        };
        Self {
            size: Some(metadata.len()),
            mtime: metadata.modified().ok(),
            atime: metadata.accessed().ok(),
            ctime: metadata.created().ok(),
        }
    }
}

fn name_order(a: &str, b: &str, reverse: bool) -> Ordering {
    let order = a.as_bytes().cmp(b.as_bytes());
    if reverse { order.reverse() } else { order }
}

fn all_digits(name: &str) -> Option<u128> {
    if !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) {
        name.parse().ok()
    } else {
        None
    }
}

/// Sorts `results` as `$GLOBSORT` asks. `resolve` maps a result, which may be relative
/// to the shell's working directory, to a path that can be examined.
pub(crate) fn sort_results(
    results: &mut [String],
    globsort: Option<&str>,
    resolve: impl Fn(&str) -> PathBuf,
) {
    let spec = GlobSort::parse(globsort);
    let reverse = spec.reverse;

    match spec.key {
        SortKey::NoSort => {}
        SortKey::Name => results.sort_by(|a, b| name_order(a, b, reverse)),
        SortKey::Numeric => results.sort_by(|a, b| match (all_digits(a), all_digits(b)) {
            (Some(x), Some(y)) => {
                if reverse {
                    y.cmp(&x)
                } else {
                    x.cmp(&y)
                }
            }
            (None, None) => name_order(a, b, reverse),
            (Some(_), None) => {
                if reverse {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
            (None, Some(_)) => {
                if reverse {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
        }),
        key => {
            let mut keyed: Vec<(String, FileInfo)> = results
                .iter()
                .map(|name| (name.clone(), FileInfo::read(&resolve(name))))
                .collect();
            keyed.sort_by(|(a_name, a), (b_name, b)| {
                let order = match key {
                    SortKey::Size => a.size.cmp(&b.size),
                    SortKey::Blocks => a
                        .size
                        .map(|s| s.div_ceil(512))
                        .cmp(&b.size.map(|s| s.div_ceil(512))),
                    SortKey::Mtime => a.mtime.cmp(&b.mtime),
                    SortKey::Atime => a.atime.cmp(&b.atime),
                    SortKey::Ctime => a.ctime.cmp(&b.ctime),
                    SortKey::Name | SortKey::Numeric | SortKey::NoSort => Ordering::Equal,
                };
                let order = if reverse { order.reverse() } else { order };
                order.then_with(|| name_order(a_name, b_name, reverse))
            });
            for (slot, (name, _)) in results.iter_mut().zip(keyed) {
                *slot = name;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(names: &[&str], spec: Option<&str>) -> Vec<String> {
        let mut names: Vec<String> = names.iter().map(|s| (*s).to_owned()).collect();
        sort_results(&mut names, spec, |name| PathBuf::from(name));
        names
    }

    #[test]
    fn parses_direction_and_key() {
        assert_eq!(
            GlobSort::parse(Some("-size")),
            GlobSort {
                key: SortKey::Size,
                reverse: true
            }
        );
        assert_eq!(GlobSort::parse(Some("+mtime")).key, SortKey::Mtime);
        assert_eq!(
            GlobSort::parse(Some("-")),
            GlobSort {
                key: SortKey::Name,
                reverse: true
            }
        );
        // Unrecognised values are the historical order, even with a direction.
        assert_eq!(
            GlobSort::parse(Some("-bogus")),
            GlobSort {
                key: SortKey::Name,
                reverse: false
            }
        );
        assert_eq!(GlobSort::parse(Some("")).key, SortKey::Name);
        assert_eq!(GlobSort::parse(None).key, SortKey::Name);
    }

    #[test]
    fn name_and_numeric_orders() {
        assert_eq!(sorted(&["b", "a", "c"], None), ["a", "b", "c"]);
        assert_eq!(sorted(&["b", "a", "c"], Some("-name")), ["c", "b", "a"]);
        assert_eq!(
            sorted(&["10", "9", "x", "100", "1"], Some("numeric")),
            ["1", "9", "10", "100", "x"]
        );
        assert_eq!(
            sorted(&["10", "9", "x", "100", "1"], Some("-numeric")),
            ["x", "100", "10", "9", "1"]
        );
        assert_eq!(sorted(&["b", "a", "c"], Some("nosort")), ["b", "a", "c"]);
    }
}
