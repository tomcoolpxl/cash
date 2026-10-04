//! Case folding as Windows does it for the names it compares without regard to case:
//! file names, environment variable names, process images.
//!
//! Windows folds each UTF-16 unit by itself to its uppercase (NTFS's `$UpCase` table,
//! `RtlUpcaseUnicodeChar`), never one character into two. cash folded such names three
//! ways: ASCII only (`eq_ignore_ascii_case`), which leaves `Ä` and `ä` apart where
//! Windows has one name, and Rust's `to_lowercase`, which also turns one character into
//! two (`İ`); the PATH index that colours a command and the search that runs it could
//! then disagree (XC-16). Text cash knows to be ASCII (an extension, `PATH`, a signal
//! name) still folds as ASCII.

/// `name` folded as Windows folds names: each character to its uppercase where that is
/// one character, else left as it is. Two names are the same to Windows when their keys
/// are equal.
#[must_use]
pub fn name_key(name: &str) -> String {
    name.chars().map(upcase).collect()
}

/// Whether Windows takes `a` and `b` for the same name.
#[must_use]
pub fn same_name(a: &str, b: &str) -> bool {
    a.chars().map(upcase).eq(b.chars().map(upcase))
}

fn upcase(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_uppercase();
    }
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(single), None) => single,
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::{name_key, same_name};

    #[test]
    fn names_fold_one_character_to_one_as_windows_folds_them() {
        assert!(same_name("Ärger.EXE", "ärger.exe"));
        assert!(same_name("PATH", "Path"));
        assert!(!same_name("a", "b"));
        // ß's uppercase is two characters, so it stays; ﬀ likewise.
        assert_eq!(name_key("straße"), "STRAßE");
        assert_eq!(name_key("ﬀ"), "ﬀ");
        // İ's lowercase is two characters; as an uppercase it is itself.
        assert!(same_name("İ", "İ"));
        assert!(!same_name("ss", "ß"));
    }
}
