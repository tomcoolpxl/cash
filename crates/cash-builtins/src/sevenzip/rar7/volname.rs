//! The next volume's name, as 7-Zip's RAR handlers make it (`RarVol.h`, `CVolumeName`):
//! the last run of digits before `.rar` counts up (`name.part1.rar`, `name.part2.rar`),
//! and a name without one goes on as `name.r00`, `name.r01`; in the old style of RAR 1.5
//! to 4, `name.rar` always does, and `name.r00` goes on as `name.r01`.

/// The parts of a volume's name, and how the next is made.
pub(super) struct VolumeName {
    before: String,
    changed: String,
    after: String,
    /// The first name asked for is `changed` as it is (`name.r00` after `name.rar`).
    change_next: bool,
}

impl VolumeName {
    /// `InitName`: from the name of the volume opened first, in the new style or not.
    pub(super) fn new(name: &str, new_style: bool) -> Self {
        let mut base = name.to_owned();
        let mut after = String::new();
        if let Some((stem, ext)) = name.rsplit_once('.') {
            if ext.eq_ignore_ascii_case("rar") {
                after = format!(".{ext}");
                stem.clone_into(&mut base);
            } else if ext.eq_ignore_ascii_case("exe") {
                ".rar".clone_into(&mut after);
                stem.clone_into(&mut base);
            } else if !new_style
                && ["000", "001", "r00", "r01"]
                    .iter()
                    .any(|e| ext.eq_ignore_ascii_case(e))
            {
                return Self {
                    before: format!("{stem}."),
                    changed: ext.to_owned(),
                    after: String::new(),
                    change_next: true,
                };
            }
        }
        let chars: Vec<char> = if new_style {
            base.chars().collect()
        } else {
            Vec::new()
        };
        let mut k = chars.len();
        while k != 0 && !chars[k - 1].is_ascii_digit() {
            k -= 1;
        }
        let mut i = k;
        while i != 0 && chars[i - 1].is_ascii_digit() {
            i -= 1;
        }
        if i != k {
            let tail: String = chars[k..].iter().collect();
            return Self {
                before: chars[..i].iter().collect(),
                changed: chars[i..k].iter().collect(),
                after: tail + &after,
                change_next: true,
            };
        }
        Self {
            before: base + ".",
            changed: "r00".to_owned(),
            after: String::new(),
            change_next: false,
        }
    }

    /// `GetNextName`: the number counted up, `9` carrying into a new leading `1`.
    pub(super) fn next_name(&mut self) -> String {
        if self.change_next {
            let mut digits: Vec<u8> = self.changed.bytes().collect();
            let mut i = digits.len();
            loop {
                if i == 0 {
                    digits.insert(0, b'1');
                    break;
                }
                i -= 1;
                if digits[i] == b'9' {
                    digits[i] = b'0';
                    if i == 0 {
                        digits.insert(0, b'1');
                        break;
                    }
                    continue;
                }
                digits[i] += 1;
                break;
            }
            self.changed = String::from_utf8_lossy(&digits).into_owned();
        }
        self.change_next = true;
        format!("{}{}{}", self.before, self.changed, self.after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_part_number_counts_up_with_its_width() {
        let mut name = VolumeName::new("set.part1.rar", true);
        assert_eq!(name.next_name(), "set.part2.rar");
        let mut name = VolumeName::new("set.part09.rar", true);
        assert_eq!(name.next_name(), "set.part10.rar");
        let mut name = VolumeName::new("set.part9.rar", true);
        assert_eq!(name.next_name(), "set.part10.rar");
    }

    #[test]
    fn a_name_without_digits_goes_on_as_r00_r01() {
        let mut name = VolumeName::new("old.rar", true);
        assert_eq!(name.next_name(), "old.r00");
        assert_eq!(name.next_name(), "old.r01");
        let mut name = VolumeName::new("sfx.exe", true);
        assert_eq!(name.next_name(), "sfx.r00");
    }

    #[test]
    fn digits_in_the_name_are_the_ones_counted() {
        let mut name = VolumeName::new("photos2026.rar", true);
        assert_eq!(name.next_name(), "photos2027.rar");
    }

    #[test]
    fn the_old_style_goes_on_from_r00_whatever_the_name_holds() {
        let mut name = VolumeName::new("set2026.rar", false);
        assert_eq!(name.next_name(), "set2026.r00");
        let mut name = VolumeName::new("set.r00", false);
        assert_eq!(name.next_name(), "set.r01");
        // Past 99 the letter counts up too, as 7-Zip's does.
        let mut name = VolumeName::new("set.r01", false);
        name.changed = "r99".to_owned();
        assert_eq!(name.next_name(), "set.s00");
    }
}
