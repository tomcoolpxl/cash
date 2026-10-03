//! Argument encoding for `cmd.exe` — **D32** — and for native processes generally.
//!
//! Windows has no `execve`: `CreateProcessW` takes a single command-line *string*, and
//! the callee parses it again. For ordinary executables that means encoding arguments
//! the way the Microsoft C runtime expects. For `.bat` and `.cmd`, it means surviving a
//! second parse by `cmd.exe`, which has its own metacharacters — `& | ^ < > ( ) %` — and
//! its own idea of quoting.
//!
//! D32 accepts that "correct for all inputs" is unachievable here and requires the gap
//! to be documented rather than papered over. [`is_safe_for_cmd`] is that documentation
//! in executable form.
//!
//! This is **not** a conflict with D4. D4 forbids rewriting arguments *semantically* —
//! guessing which ones are paths. This is quoting for a specific, known interpreter that
//! cash is deliberately invoking.

/// Encode one argument the way the Microsoft C runtime parses it.
///
/// The rules are unintuitive but exact: backslashes are literal except when they
/// immediately precede a quote, where they must be doubled; a literal quote is escaped
/// with a backslash.
#[must_use]
pub fn quote_argument(arg: &str) -> String {
    // A non-empty argument with nothing special in it needs no quoting at all.
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\u{b}', '"']) {
        return arg.to_string();
    }

    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');

    let mut backslashes = 0usize;
    for ch in arg.chars() {
        match ch {
            '\\' => {
                backslashes += 1;
                out.push('\\');
            }
            '"' => {
                // Double the run of backslashes, then escape the quote itself.
                out.extend(std::iter::repeat_n('\\', backslashes + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                backslashes = 0;
                out.push(ch);
            }
        }
    }

    // Backslashes immediately before the closing quote would otherwise escape it.
    out.extend(std::iter::repeat_n('\\', backslashes));
    out.push('"');
    out
}

/// Build a full command line for `CreateProcessW`.
#[must_use]
pub fn build_command_line(program: &str, args: &[String]) -> String {
    let mut line = quote_argument(program);
    for arg in args {
        line.push(' ');
        line.push_str(&quote_argument(arg));
    }
    line
}

/// Characters `cmd.exe` treats specially outside of quotes.
// Quotes are syntax we deliberately add around arguments. Escaping those quotes with a
// caret makes them literal while `/s /c` is parsing the command, so a batch path such as
// `C:\Program Files\tool.cmd` is split at its first space. Keep the delimiters intact and
// escape the actual command metacharacters inside them.
const CMD_METACHARACTERS: &[char] = &['(', ')', '%', '!', '^', '<', '>', '&', '|'];

/// Encode one argument to survive `cmd.exe`'s parse *and* the callee's (D32); see
/// [`escape_words_for_cmd`].
#[must_use]
pub fn escape_for_cmd(arg: &str) -> String {
    escape_words_for_cmd([arg])
}

/// Encode the words of a command for `cmd.exe /s /c` (D32): the script and its
/// arguments, separated by spaces.
///
/// Each word gets CRT quoting first. A character `cmd` would act on then gets a caret
/// where `cmd` is outside quotes, which it removes before the callee sees the word; inside
/// quotes it is already literal to `cmd`, and so would a caret be. A caret everywhere
/// gave a batch file `Q^&A notes.txt` for `"Q&A notes.txt"` (W32-01). `cmd` knows no
/// escaped quote, so the `\"` of a quote in a word turns its quoting on or off too, which
/// is why the state goes on from one word to the next.
#[must_use]
pub fn escape_words_for_cmd<'a>(words: impl IntoIterator<Item = &'a str>) -> String {
    let mut out = String::new();
    let mut in_quotes = false;
    for (index, word) in words.into_iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        for ch in quote_argument(word).chars() {
            if ch == '"' {
                in_quotes = !in_quotes;
            } else if !in_quotes && CMD_METACHARACTERS.contains(&ch) {
                out.push('^');
            }
            out.push(ch);
        }
    }
    out
}

/// Build the command line for `cmd.exe /d /s /c` (D8's `.bat` / `.cmd` dispatch).
///
/// `/d` skips `AutoRun` commands from the registry, `/s` makes the outer quoting rules
/// predictable, and `/c` runs and exits.
#[must_use]
pub fn build_cmd_command_line(script: &str, args: &[String]) -> String {
    let inner =
        escape_words_for_cmd(std::iter::once(script).chain(args.iter().map(String::as_str)));

    // /s plus the outer quotes means cmd strips exactly the first and last quote and
    // treats everything between as the command.
    format!("cmd.exe /d /s /c \"{inner}\"")
}

/// Why an argument cannot be passed safely through `cmd.exe`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmdHazard {
    /// Contains a newline. `cmd` treats it as a command separator and there is no
    /// escape that survives.
    Newline,
    /// Contains a NUL byte, which cannot appear in a Win32 command line at all.
    Nul,
    /// Contains `%`, which `cmd` may expand as a variable reference. Caret-escaping
    /// helps outside quotes but is not reliable within them.
    PercentExpansion,
}

/// Report whether an argument survives `cmd.exe` intact — D32's residual gaps, made
/// checkable rather than merely documented.
///
/// A caller that wants D26's fail-loudly posture can refuse these; D32 as decided lets
/// them through and documents the risk. `cash doctor` (D35) can use this to explain a
/// `.bat` invocation that behaved strangely.
pub fn is_safe_for_cmd(arg: &str) -> Result<(), CmdHazard> {
    if arg.contains('\0') {
        return Err(CmdHazard::Nul);
    }
    if arg.contains('\n') || arg.contains('\r') {
        return Err(CmdHazard::Newline);
    }
    if arg.contains('%') {
        return Err(CmdHazard::PercentExpansion);
    }
    Ok(())
}
