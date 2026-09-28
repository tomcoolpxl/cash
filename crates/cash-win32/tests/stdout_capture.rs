//! Capturing and re-rendering a bundled utility's output — **D3** via **D48**.
//!
//! The mechanism that stops `mktemp -d` from printing `C:\Users\...`. The interesting
//! cases are the ones where a blunt `replace('\\', "/")` would be wrong: NUL-delimited
//! output, invalid UTF-8, empty fields, and the extended-length prefix.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result. Shell snippets are spelled with hashes \
              throughout, including where they are not strictly needed, because \
              alternating the two forms by accident of content reads worse."
)]

use cash_win32::stdio::{render_paths, with_captured_stdout, write_stdout};
use std::sync::{Mutex, MutexGuard};

/// Serialises the capture cases.
///
/// The standard-output handle is process-wide, so two captures in flight at once would
/// collect each other's bytes — which is what `with_captured_stdout`'s documentation
/// warns callers about, made true here rather than hoped for. `main` already runs the
/// cases one at a time; the lock keeps that true if one ever starts a thread.
static CAPTURE_LOCK: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    CAPTURE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// How many capture files this process currently has in the temp directory.
///
/// Filtered by pid so a concurrently running test binary cannot perturb the count.
fn capture_files_outstanding() -> usize {
    let prefix = format!("cash-capture-{}-", std::process::id());
    std::fs::read_dir(std::env::temp_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .count()
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn a_windows_path_becomes_the_canonical_spelling() {
    assert_eq!(
        render_paths(br"C:\Users\me\tmp.AbC" as &[u8]),
        b"C:/Users/me/tmp.AbC"
    );
    assert_eq!(
        render_paths(br"c:\x" as &[u8]),
        b"C:/x",
        "the drive letter is uppercased"
    );
}

fn a_path_that_is_already_canonical_is_unchanged() {
    for already in [
        &b"C:/Users/me/tmp"[..],
        &b"./relative/path"[..],
        &b"plain-name"[..],
        &b"//server/share/file"[..],
    ] {
        assert_eq!(
            render_paths(already),
            already,
            "changed {:?}",
            String::from_utf8_lossy(already)
        );
    }
}

fn line_structure_is_preserved_exactly() {
    // A trailing newline stays; a missing one stays missing. `d=$(mktemp -d)` depends on
    // neither, but `mktemp a b | wc -l` depends on both.
    assert_eq!(render_paths(br"C:\a" as &[u8]), b"C:/a");
    assert_eq!(render_paths(b"C:\\a\n" as &[u8]), b"C:/a\n");
    assert_eq!(render_paths(b"C:\\a\nC:\\b\n" as &[u8]), b"C:/a\nC:/b\n");
    assert_eq!(render_paths(b"" as &[u8]), b"");
    assert_eq!(render_paths(b"\n" as &[u8]), b"\n");
    assert_eq!(render_paths(b"\n\n\n" as &[u8]), b"\n\n\n");
}

fn nul_delimited_output_is_handled_field_by_field() {
    // `realpath -z` and `readlink -z` exist precisely so paths survive a pipe; splitting
    // only on newlines would leave those untouched.
    assert_eq!(render_paths(b"C:\\a\0C:\\b\0" as &[u8]), b"C:/a\0C:/b\0");
    // Mixed, which nothing emits but which must not corrupt anything either.
    assert_eq!(render_paths(b"C:\\a\0C:\\b\n" as &[u8]), b"C:/a\0C:/b\n");
}

fn a_path_with_spaces_and_unicode_survives() {
    let input = "C:\\Program Files\\Ünïcodé Ordner\\файл.txt\n";
    assert_eq!(
        String::from_utf8(render_paths(input.as_bytes())).unwrap(),
        "C:/Program Files/Ünïcodé Ordner/файл.txt\n"
    );
}

fn the_extended_length_prefix_never_reaches_a_script() {
    // `std::fs::canonicalize` returns `\\?\C:\...`, and some utilities forward it
    // verbatim. D29 says that form is internal.
    assert_eq!(render_paths(br"\\?\C:\a\b" as &[u8]), b"C:/a/b");
    assert_eq!(
        render_paths(br"\\?\UNC\server\share\f" as &[u8]),
        b"//server/share/f"
    );
}

fn invalid_utf8_is_passed_through_rather_than_guessed_at() {
    // A path that is not valid UTF-16-to-UTF-8 round-trippable is rare but possible, and
    // mangling it is worse than leaving it native.
    let input = b"C:\\a\xFF\xFEb\nC:\\good\n";
    let out = render_paths(input);
    assert!(
        out.starts_with(b"C:\\a\xFF\xFEb\n"),
        "invalid field was altered: {out:?}"
    );
    assert!(
        out.ends_with(b"C:/good\n"),
        "the valid field was not rendered: {out:?}"
    );
}

fn an_empty_field_between_separators_stays_empty() {
    assert_eq!(
        render_paths(b"C:\\a\n\nC:\\b\n" as &[u8]),
        b"C:/a\n\nC:/b\n"
    );
}

// ---------------------------------------------------------------------------
// Capturing
// ---------------------------------------------------------------------------

fn stdout_written_inside_the_capture_is_returned_not_printed() {
    let _guard = exclusive();
    let (value, captured) = with_captured_stdout(|| {
        write_stdout(b"captured line\n").unwrap();
        41 + 1
    })
    .expect("capture failed");

    assert_eq!(value, 42, "the body's return value was lost");
    assert_eq!(String::from_utf8(captured).unwrap(), "captured line\n");
}

fn output_without_a_trailing_newline_is_captured() {
    // `d=$(mktemp -u xxxxXXXX)` with `printf`-style output and no newline is a real
    // shape; an unterminated write must not be stranded in a buffer and then land on the
    // caller's real stdout after the handle is restored.
    let _guard = exclusive();
    let ((), captured) =
        with_captured_stdout(|| write_stdout(b"no newline here").unwrap()).expect("capture failed");
    assert_eq!(String::from_utf8(captured).unwrap(), "no newline here");
}

fn a_large_output_does_not_deadlock_or_truncate() {
    // The reason this uses a temp file rather than a pipe: a pipe's buffer is 64 KiB and
    // nothing drains it while the utility runs.
    let _guard = exclusive();
    let line = "C:\\some\\reasonably\\long\\path\\name.txt\n";
    let count = 20_000;
    let payload: String = std::iter::repeat_n(line, count).collect();

    let ((), captured) =
        with_captured_stdout(|| write_stdout(payload.as_bytes()).unwrap()).expect("capture failed");

    assert_eq!(captured.len(), line.len() * count, "output was truncated");

    let rendered = render_paths(&captured);
    assert!(!rendered.contains(&b'\\'), "a backslash survived rendering");
    #[allow(
        clippy::naive_bytecount,
        reason = "counting 760 KiB once in a test is not worth a dependency"
    )]
    let newlines = rendered.iter().filter(|&&b| b == b'\n').count();
    assert_eq!(newlines, count);
}

fn nothing_is_captured_when_nothing_is_written() {
    let _guard = exclusive();
    let (value, captured) = with_captured_stdout(|| "done").expect("capture failed");
    assert_eq!(value, "done");
    assert!(
        captured.is_empty(),
        "captured {captured:?} from a body that wrote nothing"
    );
}

fn binary_output_survives_the_round_trip() {
    // Nothing in the allowlist emits binary, but the capture must not be the thing that
    // corrupts it if one ever does.
    let _guard = exclusive();
    let payload: Vec<u8> = (0u8..=255).collect();
    let ((), captured) =
        with_captured_stdout(|| write_stdout(&payload).unwrap()).expect("capture failed");
    assert_eq!(captured, payload);
}

fn stdout_still_works_after_a_capture() {
    // The handle must be put back, or every later write in the process goes nowhere.
    let _guard = exclusive();
    let ((), first) = with_captured_stdout(|| write_stdout(b"one").unwrap()).expect("first");
    assert_eq!(String::from_utf8(first).unwrap(), "one");

    let ((), second) = with_captured_stdout(|| write_stdout(b"two").unwrap()).expect("second");
    assert_eq!(
        String::from_utf8(second).unwrap(),
        "two",
        "the second capture saw the wrong handle"
    );
}

fn the_handle_is_restored_even_if_the_body_panics() {
    let _guard = exclusive();
    // The panic is the point; keep its message out of the output.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let panicked = std::panic::catch_unwind(|| {
        let _ = with_captured_stdout(|| panic!("boom"));
    });
    std::panic::set_hook(hook);
    assert!(panicked.is_err(), "the panic was swallowed");

    // If the guard had not run, this would be writing into a deleted temp file.
    let ((), captured) =
        with_captured_stdout(|| write_stdout(b"after panic").unwrap()).expect("capture failed");
    assert_eq!(String::from_utf8(captured).unwrap(), "after panic");
}

fn captures_do_not_leave_temp_files_behind() {
    // A leak here would fill the user's temp directory one `mktemp` at a time.
    let _guard = exclusive();

    let before = capture_files_outstanding();
    for _ in 0..5 {
        let _ = with_captured_stdout(|| write_stdout(b"x").unwrap()).expect("capture failed");
    }
    assert_eq!(
        capture_files_outstanding(),
        before,
        "a capture file was left in the temp directory"
    );
}

/// Runs every case in turn, without libtest.
///
/// A capture redirects the whole process's standard output, and libtest prints each
/// finished test's result from its own thread, so under the harness another test's
/// `ok` landed inside a capture (`"captured line\nok\n"` on GitHub's runner). This
/// binary is built with `harness = false` so that nothing else writes to standard output
/// while a capture is open; progress goes to standard error.
fn main() {
    let cases: &[(&str, fn())] = &[
        (
            "a_windows_path_becomes_the_canonical_spelling",
            a_windows_path_becomes_the_canonical_spelling,
        ),
        (
            "a_path_that_is_already_canonical_is_unchanged",
            a_path_that_is_already_canonical_is_unchanged,
        ),
        (
            "line_structure_is_preserved_exactly",
            line_structure_is_preserved_exactly,
        ),
        (
            "nul_delimited_output_is_handled_field_by_field",
            nul_delimited_output_is_handled_field_by_field,
        ),
        (
            "a_path_with_spaces_and_unicode_survives",
            a_path_with_spaces_and_unicode_survives,
        ),
        (
            "the_extended_length_prefix_never_reaches_a_script",
            the_extended_length_prefix_never_reaches_a_script,
        ),
        (
            "invalid_utf8_is_passed_through_rather_than_guessed_at",
            invalid_utf8_is_passed_through_rather_than_guessed_at,
        ),
        (
            "an_empty_field_between_separators_stays_empty",
            an_empty_field_between_separators_stays_empty,
        ),
        (
            "stdout_written_inside_the_capture_is_returned_not_printed",
            stdout_written_inside_the_capture_is_returned_not_printed,
        ),
        (
            "output_without_a_trailing_newline_is_captured",
            output_without_a_trailing_newline_is_captured,
        ),
        (
            "a_large_output_does_not_deadlock_or_truncate",
            a_large_output_does_not_deadlock_or_truncate,
        ),
        (
            "nothing_is_captured_when_nothing_is_written",
            nothing_is_captured_when_nothing_is_written,
        ),
        (
            "binary_output_survives_the_round_trip",
            binary_output_survives_the_round_trip,
        ),
        (
            "stdout_still_works_after_a_capture",
            stdout_still_works_after_a_capture,
        ),
        (
            "the_handle_is_restored_even_if_the_body_panics",
            the_handle_is_restored_even_if_the_body_panics,
        ),
        (
            "captures_do_not_leave_temp_files_behind",
            captures_do_not_leave_temp_files_behind,
        ),
    ];
    for (name, case) in cases {
        eprint!("test {name} ... ");
        case();
        eprintln!("ok");
    }
    eprintln!("{} capture cases passed", cases.len());
}
