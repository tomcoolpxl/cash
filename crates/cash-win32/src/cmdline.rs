//! The command line a bundled tool's process shows the tool: see [`present`].
//!
//! A bundled tool runs as `cash.exe --invoke-bundled cut -f1`, and uutils takes a tool's
//! name for its messages from the first word of the process's command line, as Windows
//! gives it (`GetCommandLineW`). That word is the program's path, which Windows lets no
//! one choose, so `cut` said `cash.exe: you must specify a list of bytes, characters, or
//! fields` and `Try 'C:/…/cash.exe --help'`. GNU's says `cut:` and `Try 'cut --help'`.
//!
//! [`present`] points the process's imports of `GetCommandLineW` at a function that
//! answers with the command line the tool would have had run on its own, `cut -f1`, so
//! the tool names itself, and anything else that reads its arguments again reads its own.

use std::ffi::OsString;
use std::sync::OnceLock;
use std::sync::atomic::AtomicUsize;

use windows_sys::core::PWSTR;

/// The command line [`present`] made, wide and null-terminated.
static COMMAND_LINE: OnceLock<Vec<u16>> = OnceLock::new();

/// The function the import pointed at before [`present`]; 0 until then.
static GET_COMMAND_LINE: AtomicUsize = AtomicUsize::new(0);

/// Makes this process's command line, as the tool reads it, `argv`: the tool's name and
/// its arguments. For a bundled tool's process, before the tool runs.
///
/// Returns whether the import was found and pointed here.
pub fn present(argv: &[OsString]) -> bool {
    let line = argv
        .iter()
        .map(|arg| quoted(&arg.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ");
    let wide = crate::wide::to_wide_nul(&line);
    if COMMAND_LINE.set(wide).is_err() {
        return false;
    }
    crate::imports::redirect(
        "GetCommandLineW",
        get_command_line as unsafe extern "system" fn() -> PWSTR as usize,
        &GET_COMMAND_LINE,
    )
}

/// `arg` in double quotes, encoded as the Microsoft C runtime decodes it.
///
/// Every argument is quoted, plain or not: the tools' runtime (`wild`) expands `*` and `?`
/// in an argument that is not, and cash has expanded what the script meant to already.
fn quoted(arg: &str) -> String {
    let encoded = crate::cmd::quote_argument(arg);
    if encoded.starts_with('"') {
        return encoded;
    }
    // Plain, so no quote inside; backslashes before the closing quote are doubled.
    let trailing = arg.len() - arg.trim_end_matches('\\').len();
    format!("\"{arg}{}\"", "\\".repeat(trailing))
}

/// `GetCommandLineW`, answering with the command line [`present`] made.
unsafe extern "system" fn get_command_line() -> PWSTR {
    // A pointer the caller may read and must not write, as the real one's.
    COMMAND_LINE
        .get()
        .map_or(std::ptr::null_mut(), |line| line.as_ptr().cast_mut())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_argument_is_quoted_as_the_c_runtime_decodes_it() {
        assert_eq!(quoted("cut"), "\"cut\"");
        assert_eq!(quoted("a*b"), "\"a*b\"");
        assert_eq!(quoted("two words"), "\"two words\"");
        assert_eq!(quoted(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(quoted(r"C:\dir\"), r#""C:\dir\\""#);
        assert_eq!(quoted(""), "\"\"");
    }
}
