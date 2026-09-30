//! Capturing and re-rendering this process's standard output — **D3** in service of
//! **D48**.
//!
//! D48 bundles uutils and findutils inside the cash binary so that a single executable
//! carries its own userland. Those utilities are excellent and cash does not want to
//! reimplement them — but they are portable Rust, so the handful that *synthesise* an
//! absolute path render it the way Windows spells it: `mktemp -d` prints
//! `C:\Users\me\AppData\Local\Temp\tmp.AbCdEf`.
//!
//! That collides with D3, which says cash renders `C:/...` and never backslashes. It is
//! not merely cosmetic. `d=$(mktemp -d)` seeds that spelling into a variable that the
//! script then composes into other commands, and the first one to treat `\` as an escape
//! rather than a separator silently mangles it — `find "$d" | xargs grep` turns
//! `C:\Users\me\...` into `C:Usersme...`, which then simply does not exist.
//!
//! Tools that merely *echo back* a path they were given — `find`, `wc`, `grep -l`,
//! `dirname` — need nothing: they preserve whatever spelling reached them, so fixing the
//! sources fixes the whole pipeline.
//!
//! The mechanism is deliberately blunt: redirect `STD_OUTPUT_HANDLE` at a temp file for
//! the duration of the call, then render each line of what was written. A temp file
//! rather than a pipe because a pipe would deadlock if the utility ever outran its 64 KiB
//! buffer, and these outputs are a few hundred bytes.
//!
//! # Standard input, without reading ahead
//!
//! [`RawStdin`] is the other half: how the shell itself reads the standard input it was
//! started with. See it for why `std::io::stdin()` will not do.

use std::io::{Read, Seek, Write};
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::sync::atomic::{AtomicPtr, Ordering};

use windows_sys::Win32::Foundation::{ERROR_INVALID_HANDLE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE, SetStdHandle,
};

use crate::path;

/// This process's standard input, read without reading ahead: a read takes what the
/// caller's buffer holds and leaves the rest where it was.
///
/// `std::io::stdin()` reads through a buffer that every `Stdin` of the process shares,
/// and its first read of a pipe takes all the pipe holds. For a shell that is wrong
/// twice over:
///
/// - What a command does not read belongs to the next one, and a program the shell
///   starts inherits the handle, not the buffer. `printf 'a\nb\n' | cash -c 'read x; cat'`
///   printed nothing for `cat`: `b` was in the buffer of a shell that did not want it.
/// - Nothing can ask whether that buffer holds anything. `read -t` waits on the handle
///   before each byte, and after the first byte it waited on a pipe the library had
///   emptied: with the writer still there, `read -t 1` kept `a` of a line that had
///   arrived whole, and timed out.
///
/// Bash reads a pipe a byte at a time for the same reasons. So every reader of standard
/// input inside the shell comes through here (`read`, `mapfile` and the builtins, by way
/// of cash-core's `OpenFile::Stdin`, and the reader of a script given on standard input),
/// and none may use `std::io::stdin()` to read: what one of them took into the library's
/// buffer, the others and every child would never see.
///
/// A console is left to the standard library. It hands over a line at a time, so nothing
/// is read ahead there, and the library reads it as UTF-16 and converts it, which a read
/// of the handle does not.
#[derive(Clone, Copy, Debug, Default)]
pub struct RawStdin;

/// The standard input handle last found not to be a console's, so that [`RawStdin`] asks
/// once. Null until then, which no standard input read here is.
static NOT_A_CONSOLE: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

impl Read for RawStdin {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use std::mem::ManuallyDrop;
        use std::os::windows::io::FromRawHandle;

        let mut stdin = std::io::stdin();
        let handle = stdin.as_raw_handle();

        // No standard input at all, or one since closed, reads as its end: what the
        // standard library makes of it, here and after the read below.
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Ok(0);
        }

        // Asked once for a handle that is not a console's: a reader that takes a byte at
        // a time would otherwise ask before every byte, which on a file costs as much
        // as the read.
        if NOT_A_CONSOLE.load(Ordering::Relaxed) != handle {
            let mut mode = 0u32;
            // SAFETY: `GetConsoleMode` writes the mode to a valid local, and fails for a
            // handle that is not a console's rather than faulting.
            if unsafe { GetConsoleMode(handle, &raw mut mode) } != 0 {
                return stdin.read(buf);
            }
            NOT_A_CONSOLE.store(handle, Ordering::Relaxed);
        }

        // SAFETY: `handle` is this process's standard input, open for as long as the
        // process lives. `ManuallyDrop` keeps the `File` from closing it: the handle is
        // borrowed, and closing it would take standard input away from everyone else.
        // Read as a `File`, the end of a pipe whose writers have gone is end of input,
        // and a handle opened for overlapped I/O is waited for.
        let file = ManuallyDrop::new(unsafe { std::fs::File::from_raw_handle(handle) });
        match (&*file).read(buf) {
            Err(error) if error.raw_os_error() == i32::try_from(ERROR_INVALID_HANDLE).ok() => Ok(0),
            result => result,
        }
    }
}

/// Restores the previous standard-output handle, including while unwinding.
struct RestoreStdout(HANDLE);

impl Drop for RestoreStdout {
    fn drop(&mut self) {
        // Nothing useful can be done if this fails, and failing here while already
        // unwinding would lose the original error.
        // SAFETY: `self.0` is the handle `GetStdHandle` returned before the swap, so
        // putting it back is restoring the value the OS gave us.
        unsafe { SetStdHandle(STD_OUTPUT_HANDLE, self.0) };
    }
}

/// Write bytes to this process's *current* standard-output handle.
///
/// Rust's `println!` goes through `std::io::stdout()`, which the test harness — and
/// anything else calling `set_output_capture` — can divert to a thread-local buffer.
/// That is the right behaviour for a test's own output and the wrong one for a utility's
/// results, which have to land wherever the shell's redirections put them.
///
/// # Errors
///
/// Returns an error if the handle cannot be written to.
pub fn write_stdout(bytes: &[u8]) -> std::io::Result<()> {
    use std::mem::ManuallyDrop;
    use std::os::windows::io::FromRawHandle;

    // SAFETY: `GetStdHandle` takes a valid standard-handle id and only reads process
    // state; it returns a handle or a sentinel, never dereferencing anything.
    let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };

    // SAFETY: `handle` is this process's standard output, which is open for writing for
    // as long as the process lives. `ManuallyDrop` keeps the `File` from closing it: the
    // handle is borrowed, not owned, and closing it would take stdout away from everyone
    // else.
    let mut file = ManuallyDrop::new(unsafe { std::fs::File::from_raw_handle(handle.cast()) });
    file.write_all(bytes)?;
    file.flush()
}

/// Run `body` with this process's standard output redirected to a temporary file, and
/// return what it wrote alongside its value.
///
/// Standard error is untouched: diagnostics should reach the user immediately and in
/// order, and they are not paths.
///
/// **Not safe to call concurrently from two threads of one process** — the standard
/// handle is process-wide, so overlapping captures would collect each other's output.
/// The bundled dispatcher calls this once, before any shell state or task runtime
/// exists, which is why that is not a constraint in practice.
///
/// # Errors
///
/// Returns an error if the temporary file cannot be created or read, or if the standard
/// handle cannot be replaced. In that case `body` is not run.
pub fn with_captured_stdout<T>(body: impl FnOnce() -> T) -> std::io::Result<(T, Vec<u8>)> {
    let path = capture_file_path();
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&path)?;

    // Whatever is already buffered belongs to the real standard output, not to the
    // capture.
    let _ = std::io::stdout().flush();

    // SAFETY: as above — reads the process's standard-handle table and returns a handle.
    let saved = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };

    // SAFETY: `file` is open and outlives the redirection, since it is dropped only
    // after `RestoreStdout` has put `saved` back. `SetStdHandle` stores the value and
    // does not take ownership of it.
    if unsafe { SetStdHandle(STD_OUTPUT_HANDLE, file.as_raw_handle() as HANDLE) } == 0 {
        let error = std::io::Error::last_os_error();
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }

    let value = {
        let _restore = RestoreStdout(saved);
        let value = body();
        // Flush inside the guard: buffered bytes must land in the capture file, not in
        // the caller's real standard output after it is restored.
        let _ = std::io::stdout().flush();
        value
    };

    file.rewind()?;
    let mut captured = Vec::new();
    file.read_to_end(&mut captured)?;
    drop(file);
    let _ = std::fs::remove_file(&path);

    Ok((value, captured))
}

/// A unique name for one capture. The process id plus a counter is enough: captures are
/// per-process and never concurrent, since the bundled dispatch owns the whole process.
fn capture_file_path() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let n = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("cash-capture-{}-{n}.tmp", std::process::id()))
}

/// Re-render every path in a utility's output in cash's canonical spelling (D3).
///
/// Output is treated as a list of fields separated by `\n` or `\0` — the latter because
/// `realpath -z` and `readlink -z` exist and are the safe way to pass paths onward. Each
/// separator is preserved exactly, so a trailing newline stays a trailing newline and a
/// NUL-delimited stream stays NUL-delimited.
///
/// A field that is not valid UTF-8 is passed through untouched rather than guessed at.
#[must_use]
pub fn render_paths(output: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(output.len());
    let mut field_start = 0usize;

    for (index, &byte) in output.iter().enumerate() {
        if byte == b'\n' || byte == b'\0' {
            push_rendered(&mut out, &output[field_start..index]);
            out.push(byte);
            field_start = index + 1;
        }
    }
    push_rendered(&mut out, &output[field_start..]);

    out
}

fn push_rendered(out: &mut Vec<u8>, field: &[u8]) {
    match std::str::from_utf8(field) {
        Ok(text) if !text.is_empty() => {
            out.extend_from_slice(path::render(Path::new(text)).as_bytes());
        }
        _ => out.extend_from_slice(field),
    }
}
