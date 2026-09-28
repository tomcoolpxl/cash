//! CRLF in *script source* — **D7** ("CRLF tolerated in script parsing").
//!
//! D20 is about CRLF in data; this is about CRLF in the script itself, which is a
//! separate rule with a separate mechanism. It matters more than it looks: `core.autocrlf`
//! defaults to `true` in Git for Windows, so a repository checked out on this machine has
//! CRLF script files, and without this every one of them fails to parse — `fi\r` is not
//! `fi`, and the error surfaces at the end of the file rather than anywhere near the
//! cause.
//!
//! The streaming adapter is where the interesting cases are: a `\r` may be the last byte
//! of a chunk, so whether to keep it cannot be decided until the next chunk arrives.

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

use std::io::Read;

use cash_win32::text::{NormalizeCrlf, looks_crlf, normalize_crlf};

/// Read through the adapter in chunks of exactly `chunk` bytes, so the buffer boundary
/// falls in a different place on every run.
fn through(input: &[u8], chunk: usize) -> Vec<u8> {
    struct Chunked<'a> {
        data: &'a [u8],
        chunk: usize,
    }
    impl Read for Chunked<'_> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let n = self.chunk.min(out.len()).min(self.data.len());
            out[..n].copy_from_slice(&self.data[..n]);
            self.data = &self.data[n..];
            Ok(n)
        }
    }

    let mut reader = NormalizeCrlf::new(Chunked { data: input, chunk });
    let mut out = Vec::new();
    reader.read_to_end(&mut out).expect("adapter must not fail");
    out
}

/// The same input read with every plausible chunk size, including 1 — which puts a
/// boundary between every `\r` and its `\n`.
fn through_all_chunkings(input: &[u8]) -> Vec<u8> {
    let first = through(input, 1);
    for chunk in [2, 3, 5, 7, 8, 64, 8192, 100_000] {
        assert_eq!(
            through(input, chunk),
            first,
            "chunk size {chunk} gave a different result for {input:?}"
        );
    }
    first
}

// ---------------------------------------------------------------------------
// The string form
// ---------------------------------------------------------------------------

#[test]
fn crlf_becomes_lf_and_nothing_else_moves() {
    assert_eq!(normalize_crlf("a\r\nb"), "a\nb");
    assert_eq!(normalize_crlf("a\nb"), "a\nb");
    assert_eq!(normalize_crlf(""), "");
    assert_eq!(normalize_crlf("\r\n"), "\n");
    assert_eq!(normalize_crlf("\r\n\r\n\r\n"), "\n\n\n");
}

#[test]
fn a_lone_carriage_return_is_data_and_survives() {
    // `printf 'progress\r'` inside a script is a real thing, and the `\r` is the point.
    assert_eq!(normalize_crlf("a\rb"), "a\rb");
    assert_eq!(normalize_crlf("trailing\r"), "trailing\r");
    assert_eq!(normalize_crlf("\r"), "\r");
    assert_eq!(normalize_crlf("\n\r"), "\n\r");
}

#[test]
fn doubled_returns_keep_all_but_the_terminating_one() {
    // `\r\r\n` is one terminator preceded by one datum `\r`, the same reading every
    // other tool gives it.
    assert_eq!(normalize_crlf("a\r\r\nb"), "a\r\nb");
    assert_eq!(normalize_crlf("a\r\r\r\nb"), "a\r\r\nb");
}

#[test]
fn input_without_crlf_is_borrowed_not_copied() {
    let input = "echo hello\nexit 0\n";
    assert!(
        matches!(normalize_crlf(input), std::borrow::Cow::Borrowed(_)),
        "an LF script should not be reallocated"
    );
    assert!(matches!(
        normalize_crlf("a\r\nb"),
        std::borrow::Cow::Owned(_)
    ));
}

// ---------------------------------------------------------------------------
// The streaming form
// ---------------------------------------------------------------------------

#[test]
fn the_reader_agrees_with_the_string_form() {
    for input in [
        "",
        "\n",
        "\r\n",
        "\r",
        "a",
        "a\r\nb\r\nc\r\n",
        "a\nb\r\nc\n",
        "a\rb",
        "a\r\r\nb",
        "\r\n\r\n",
        "no terminators at all",
        "ends with cr\r",
        "ends with crlf\r\n",
    ] {
        let expected = normalize_crlf(input).into_owned();
        assert_eq!(
            String::from_utf8(through_all_chunkings(input.as_bytes())).unwrap(),
            expected,
            "streaming disagreed with the string form for {input:?}"
        );
    }
}

#[test]
fn a_return_split_across_a_chunk_boundary_is_still_resolved() {
    // The case the held-back `\r` exists for: the chunk ends between `\r` and `\n`.
    // Chunk size 1 guarantees it happens at every terminator.
    assert_eq!(through(b"a\r\nb", 1), b"a\nb");
    assert_eq!(through(b"a\r\nb", 2), b"a\nb");
    assert_eq!(through(b"\r\n", 1), b"\n");
    // And the other half of the decision: a `\r` at true end of input is emitted.
    assert_eq!(through(b"a\r", 1), b"a\r");
    assert_eq!(through(b"a\r", 8192), b"a\r");
}

#[test]
fn a_return_at_the_very_end_of_the_internal_buffer_is_resolved() {
    // The adapter's own buffer is 8192 bytes; place a `\r\n` straddling that boundary.
    for offset in [8190usize, 8191, 8192] {
        let mut input = vec![b'x'; offset];
        input.extend_from_slice(b"\r\ntail");
        let got = through(&input, 100_000);
        let mut expected = vec![b'x'; offset];
        expected.extend_from_slice(b"\ntail");
        assert_eq!(
            got, expected,
            "failed with {offset} bytes before the terminator"
        );
    }
}

#[test]
fn a_tiny_output_buffer_still_makes_progress() {
    // `read` into a one-byte slice must never return 0 while input remains, or the
    // caller spins forever thinking it hit EOF.
    let data = b"a\r\nb\r\n\r\nc\r";
    let mut reader = NormalizeCrlf::new(&data[..]);
    let mut out = Vec::new();
    let mut one = [0u8; 1];
    loop {
        match reader.read(&mut one).unwrap() {
            0 => break,
            _ => out.push(one[0]),
        }
    }
    assert_eq!(out, b"a\nb\n\nc\r");
}

#[test]
fn a_zero_length_read_is_not_mistaken_for_end_of_input() {
    let data = b"a\r\nb";
    let mut reader = NormalizeCrlf::new(&data[..]);
    assert_eq!(
        reader.read(&mut []).unwrap(),
        0,
        "a zero-length read consumed nothing"
    );
    let mut out = Vec::new();
    reader.read_to_end(&mut out).unwrap();
    assert_eq!(out, b"a\nb");
}

#[test]
fn binary_bytes_pass_through_untouched() {
    // Script source is text, but nothing here should corrupt a NUL, a high byte, or a
    // UTF-8 sequence that happens to contain 0x0D-adjacent values.
    let input = b"\x00\x01\xEF\xBB\xBF\xC3\xA9\r\n\xFF\xFE\r";
    let expected = b"\x00\x01\xEF\xBB\xBF\xC3\xA9\n\xFF\xFE\r";
    assert_eq!(through_all_chunkings(input), expected);
}

#[test]
fn a_long_crlf_file_normalises_completely() {
    let mut source = String::new();
    for i in 0..5000 {
        use std::fmt::Write as _;
        let _ = writeln!(source, "echo line {i}\r");
    }
    let got = String::from_utf8(through(source.as_bytes(), 8192)).unwrap();
    assert!(!got.contains('\r'), "a carriage return survived");
    assert_eq!(got.lines().count(), 5000);
    assert!(looks_crlf(source.as_bytes()));
    assert!(!looks_crlf(got.as_bytes()));
}
