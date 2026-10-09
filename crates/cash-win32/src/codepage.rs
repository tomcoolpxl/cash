//! Windows' code pages, for `iconv`: `MultiByteToWideChar` and `WideCharToMultiByte` on
//! a code page number, and what Windows says about one.
//!
//! The functions here are thin: they size the buffer, make the call and sort its failure
//! into something a caller can act on. Which bytes make a character, and where a bad one
//! is, are the caller's business.

use windows_sys::Win32::Foundation::{
    ERROR_INVALID_FLAGS, ERROR_INVALID_PARAMETER, ERROR_NO_UNICODE_TRANSLATION, GetLastError,
    SetLastError,
};
use windows_sys::Win32::Globalization::{
    CPINFOEXW, GetCPInfoExW, IsDBCSLeadByteEx, IsValidCodePage, MB_ERR_INVALID_CHARS,
    MB_USEGLYPHCHARS, MultiByteToWideChar, WC_NO_BEST_FIT_CHARS, WideCharToMultiByte,
};

/// Whether `code_page` is one this Windows can convert.
#[must_use]
pub fn is_valid(code_page: u32) -> bool {
    // SAFETY: a pure query.
    unsafe { IsValidCodePage(code_page) != 0 }
}

/// What Windows knows of a code page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    /// The most bytes one character takes.
    pub max_char_size: u32,
    /// Windows' description, such as `1252  (ANSI - Latin I)`.
    pub name: String,
}

/// `GetCPInfoExW`, or `None` for a code page Windows does not have.
#[must_use]
pub fn info(code_page: u32) -> Option<Info> {
    // SAFETY: a zeroed CPINFOEXW is a valid out-parameter.
    let mut raw: CPINFOEXW = unsafe { std::mem::zeroed() };
    // SAFETY: the pointer is to an initialised structure of the expected type.
    if unsafe { GetCPInfoExW(code_page, 0, &raw mut raw) } == 0 {
        return None;
    }
    let length = raw
        .CodePageName
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(raw.CodePageName.len());
    Some(Info {
        max_char_size: raw.MaxCharSize,
        name: String::from_utf16_lossy(raw.CodePageName.get(..length).unwrap_or_default()),
    })
}

/// Whether `byte` begins a two-byte character in `code_page`.
#[must_use]
pub fn is_lead_byte(code_page: u32, byte: u8) -> bool {
    // SAFETY: a pure query.
    unsafe { IsDBCSLeadByteEx(code_page, byte) != 0 }
}

/// Why [`decode`] could not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// A byte sequence the code page has no character for; only reported when `strict`.
    Invalid,
    /// The code page does not take `MB_ERR_INVALID_CHARS` (the ISO-2022 family and
    /// UTF-7 among them); decode it without `strict`.
    FlagsUnsupported,
    /// Another Win32 error, by its code.
    Failed(u32),
}

/// `bytes` in `code_page` as UTF-16. With `strict`, a sequence the code page has no
/// character for is [`DecodeError::Invalid`]; without it, Windows substitutes.
pub fn decode(code_page: u32, bytes: &[u8], strict: bool) -> Result<Vec<u16>, DecodeError> {
    decode_with(
        code_page,
        bytes,
        if strict { MB_ERR_INVALID_CHARS } else { 0 },
    )
}

/// `bytes` in `code_page` as UTF-16, control codes as the pictures the code page
/// shows for them on a console (`MB_USEGLYPHCHARS`: OEM 437's `☺` for 1, `◙` for a
/// line feed, `⌂` for 127).
pub fn decode_glyphs(code_page: u32, bytes: &[u8]) -> Result<Vec<u16>, DecodeError> {
    decode_with(code_page, bytes, MB_USEGLYPHCHARS)
}

/// [`decode`] with `MultiByteToWideChar`'s `flags`.
fn decode_with(code_page: u32, bytes: &[u8], flags: u32) -> Result<Vec<u16>, DecodeError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let length =
        i32::try_from(bytes.len()).map_err(|_| DecodeError::Failed(ERROR_INVALID_PARAMETER))?;
    // SAFETY: always safe; it clears the stale value a zero return would be judged by.
    unsafe { SetLastError(0) };
    // SAFETY: the input pointer is valid for `length` bytes; a null buffer with no
    // capacity asks for the size.
    let needed = unsafe {
        MultiByteToWideChar(
            code_page,
            flags,
            bytes.as_ptr(),
            length,
            std::ptr::null_mut(),
            0,
        )
    };
    let Some(capacity) = successful(needed) else {
        return Err(decode_failure());
    };
    if capacity == 0 {
        return Ok(Vec::new());
    }
    let mut units = vec![0u16; capacity];
    // SAFETY: the buffer holds `needed` units, which the sizing call asked for.
    let written = unsafe {
        MultiByteToWideChar(
            code_page,
            flags,
            bytes.as_ptr(),
            length,
            units.as_mut_ptr(),
            needed,
        )
    };
    let Some(written) = successful(written) else {
        return Err(decode_failure());
    };
    units.truncate(written);
    Ok(units)
}

/// The failure the call just made left in `GetLastError`.
fn decode_failure() -> DecodeError {
    // SAFETY: always safe; called right after the failed call.
    match unsafe { GetLastError() } {
        ERROR_NO_UNICODE_TRANSLATION => DecodeError::Invalid,
        ERROR_INVALID_FLAGS => DecodeError::FlagsUnsupported,
        other => DecodeError::Failed(other),
    }
}

/// A conversion's return value as a count, or `None` when it reports a failure: zero is
/// a failure unless `GetLastError` was cleared and stayed so.
fn successful(returned: i32) -> Option<usize> {
    if returned == 0 {
        // SAFETY: always safe; called right after the call in question.
        if unsafe { GetLastError() } != 0 {
            return None;
        }
    }
    usize::try_from(returned).ok()
}

/// What [`encode`] made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoded {
    /// The text in the code page.
    pub bytes: Vec<u8>,
    /// Whether a character had no mapping and the code page's default character stands
    /// for it; `None` when the code page cannot say (UTF-7, GB18030, the ISO-2022
    /// family), and the caller must decode the bytes again to find out.
    pub lossy: Option<bool>,
}

/// `text` in `code_page`; the error is a Win32 code.
///
/// Without `best_fit`, a character the code page lacks is counted as lossy rather than
/// approximated (`ā` as `a`); with it, Windows' best-fit mapping applies and the default
/// character stands for what remains.
pub fn encode(code_page: u32, text: &[u16], best_fit: bool) -> Result<Encoded, u32> {
    if text.is_empty() {
        return Ok(Encoded {
            bytes: Vec::new(),
            lossy: Some(false),
        });
    }
    let length = i32::try_from(text.len()).map_err(|_| ERROR_INVALID_PARAMETER)?;
    let flags = if best_fit { 0 } else { WC_NO_BEST_FIT_CHARS };
    match encode_with(code_page, flags, text, length, true) {
        // The code pages that take neither flags nor a default-character answer.
        Err(ERROR_INVALID_FLAGS | ERROR_INVALID_PARAMETER) => {
            encode_with(code_page, 0, text, length, false)
        }
        other => other,
    }
}

/// One attempt at [`encode`]: `ask_default` passes the used-default-character flag,
/// which some code pages refuse.
fn encode_with(
    code_page: u32,
    flags: u32,
    text: &[u16],
    length: i32,
    ask_default: bool,
) -> Result<Encoded, u32> {
    let mut used_default: i32 = 0;
    let used_default_ptr = if ask_default {
        &raw mut used_default
    } else {
        std::ptr::null_mut()
    };
    // SAFETY: always safe; it clears the stale value a zero return would be judged by.
    unsafe { SetLastError(0) };
    // SAFETY: the text pointer is valid for `length` units; a null buffer with no
    // capacity asks for the size; the default-character pointers are null or valid.
    let needed = unsafe {
        WideCharToMultiByte(
            code_page,
            flags,
            text.as_ptr(),
            length,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
            used_default_ptr,
        )
    };
    let Some(capacity) = successful(needed) else {
        return Err(last_error());
    };
    if capacity == 0 {
        return Ok(Encoded {
            bytes: Vec::new(),
            lossy: ask_default.then_some(false),
        });
    }
    let mut bytes = vec![0u8; capacity];
    used_default = 0;
    // SAFETY: the buffer holds `needed` bytes, which the sizing call asked for.
    let written = unsafe {
        WideCharToMultiByte(
            code_page,
            flags,
            text.as_ptr(),
            length,
            bytes.as_mut_ptr(),
            needed,
            std::ptr::null(),
            used_default_ptr,
        )
    };
    let Some(written) = successful(written) else {
        return Err(last_error());
    };
    bytes.truncate(written);
    Ok(Encoded {
        bytes,
        lossy: ask_default.then_some(used_default != 0),
    })
}

/// The Win32 error the call just made left behind.
fn last_error() -> u32 {
    // SAFETY: always safe; called right after the failed call.
    unsafe { GetLastError() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn validity_and_info() {
        assert!(is_valid(1252));
        assert!(is_valid(932));
        assert!(!is_valid(99_999));
        assert_eq!(info(1252).map(|i| i.max_char_size), Some(1));
        assert_eq!(info(932).map(|i| i.max_char_size), Some(2));
        assert_eq!(info(54936).map(|i| i.max_char_size), Some(4));
        assert_eq!(info(99_999), None);
        assert!(is_lead_byte(932, 0x81));
        assert!(!is_lead_byte(932, 0x41));
        assert!(!is_lead_byte(1252, 0xC3));
    }

    #[test]
    fn decodes_single_and_double_byte_pages() {
        assert_eq!(decode(1252, b"\xE9\x80", true), Ok(utf16("é€")));
        assert_eq!(decode(28591, b"\xE9", true), Ok(utf16("é")));
        assert_eq!(decode(932, b"\x82\xA0", true), Ok(utf16("あ")));
        assert_eq!(decode(936, b"\xC4\xE3", true), Ok(utf16("你")));
        assert_eq!(decode(20866, b"\xC1", true), Ok(utf16("а")));
        assert_eq!(decode(1252, b"", true), Ok(Vec::new()));
    }

    #[test]
    fn strict_decoding_rejects_what_the_page_lacks() {
        assert_eq!(decode(932, b"\x81", true), Err(DecodeError::Invalid));
        assert_eq!(decode(932, b"ab\x81\x20", true), Err(DecodeError::Invalid));
        // Without `strict`, Windows substitutes and the call succeeds.
        assert!(decode(932, b"\x81", false).is_ok());
        // Windows' US-ASCII drops the high bit rather than object, even when strict:
        // the reason `iconv` decodes ASCII itself.
        assert_eq!(decode(20127, b"a\xE9", true), Ok(utf16("ai")));
    }

    #[test]
    fn iso_2022_takes_no_flags() {
        assert_eq!(
            decode(50220, b"\x1B$B$\"\x1B(B", true),
            Err(DecodeError::FlagsUnsupported)
        );
        assert_eq!(decode(50220, b"\x1B$B$\"\x1B(B", false), Ok(utf16("あ")));
    }

    #[test]
    fn encodes_and_reports_loss() {
        let euro = encode(1252, &utf16("€"), false).ok();
        assert_eq!(
            euro,
            Some(Encoded {
                bytes: vec![0x80],
                lossy: Some(false)
            })
        );
        let lost = encode(20127, &utf16("é"), false).ok();
        assert_eq!(lost.as_ref().and_then(|e| e.lossy), Some(true));
        let fitted = encode(20127, &utf16("é"), true).ok();
        assert_eq!(
            fitted,
            Some(Encoded {
                bytes: b"e".to_vec(),
                lossy: Some(false)
            })
        );
        assert_eq!(
            encode(932, &utf16("あ"), false).ok(),
            Some(Encoded {
                bytes: vec![0x82, 0xA0],
                lossy: Some(false)
            })
        );
    }

    #[test]
    fn pages_that_cannot_answer_about_loss_still_encode() {
        let encoded = encode(50220, &utf16("あ"), false).ok();
        assert_eq!(
            encoded,
            Some(Encoded {
                bytes: b"\x1B$B$\"\x1B(B".to_vec(),
                lossy: None
            })
        );
        let encoded = encode(54936, &utf16("你"), false).ok();
        assert_eq!(encoded.as_ref().map(|e| e.lossy), Some(None));
        assert_eq!(encoded.map(|e| e.bytes), Some(vec![0xC4, 0xE3]));
    }
}
