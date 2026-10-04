//! Text as Win32's `W` calls take it: UTF-16 with a terminating null (`REVIEW_REPORT.md`
//! XC-13, which counted a dozen copies of this).

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt as _;

/// `text` as a null-terminated UTF-16 string.
///
/// Takes an [`OsStr`], so a path is passed as it is, with the unpaired surrogates a
/// Windows file name may have, where going through `to_string_lossy` would put U+FFFD in
/// their place and name another file. A `&str` is passed as it is too.
#[must_use]
pub fn to_wide_nul(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt as _;

    use super::*;

    #[test]
    fn text_and_paths_end_in_one_null_and_keep_a_lone_surrogate() {
        assert_eq!(to_wide_nul(""), [0]);
        assert_eq!(
            to_wide_nul("a\u{e9}\u{1f600}"),
            [0x61, 0xe9, 0xd83d, 0xde00, 0]
        );
        assert_eq!(to_wide_nul(String::from("ab")), [0x61, 0x62, 0]);
        let odd = OsString::from_wide(&[0x61, 0xd800, 0x62]);
        assert_eq!(
            to_wide_nul(std::path::Path::new(&odd)),
            [0x61, 0xd800, 0x62, 0]
        );
    }
}
