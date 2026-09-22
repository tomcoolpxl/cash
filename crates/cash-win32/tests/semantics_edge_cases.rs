//! Edge cases for the text, exit-code, environment and cmd-quoting layers.
//!
//! D20, D41, D15, D5, D31 and D32. The happy paths are covered elsewhere; these are the
//! inputs that break naive implementations — empty and whitespace-only data, lone
//! carriage returns, binary bytes, boundary exit codes, PATH entries that contain
//! separators, and arguments built to defeat quoting.

#![cfg(windows)]
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

use cash_win32::cmd::{
    CmdHazard, build_cmd_command_line, build_command_line, escape_for_cmd, is_safe_for_cmd,
    quote_argument,
};
use cash_win32::env::{Environment, canonical_name, path_to_unix, path_to_windows, split_path};
use cash_win32::exit::{from_windows, is_ntstatus_error};
use cash_win32::text::{
    looks_crlf, split_lines, strip_bom, strip_bom_str, trim_line_terminator,
    trim_substitution_output,
};

// ---------------------------------------------------------------------------
// D20 — line terminators
// ---------------------------------------------------------------------------

#[test]
fn trimming_handles_empty_and_newline_only_input() {
    assert_eq!(trim_substitution_output(""), "");
    assert_eq!(trim_substitution_output("\n"), "");
    assert_eq!(trim_substitution_output("\r\n"), "");
    assert_eq!(trim_substitution_output("\r\n\r\n"), "");
    // A lone \r is data: nothing terminates it.
    assert_eq!(trim_substitution_output("\r"), "\r");
}

#[test]
fn trimming_does_not_eat_significant_whitespace() {
    // bash strips trailing *newlines* from $(), not spaces or tabs.
    assert_eq!(trim_substitution_output("x  \n"), "x  ");
    assert_eq!(trim_substitution_output("x\t\r\n"), "x\t");
    assert_eq!(trim_substitution_output("  \n"), "  ");
}

#[test]
fn trimming_stops_at_the_first_non_terminator() {
    assert_eq!(trim_substitution_output("a\n\nb\n"), "a\n\nb");
    assert_eq!(trim_substitution_output("a\r\n\r\nb\r\n"), "a\r\n\r\nb");
}

#[test]
fn a_carriage_return_not_before_a_newline_is_data() {
    // Progress-bar output uses bare \r. Deleting it would corrupt captured data.
    assert_eq!(trim_substitution_output("50%\r100%"), "50%\r100%");
    assert_eq!(trim_line_terminator("50%\r100%"), "50%\r100%");
    assert_eq!(trim_substitution_output("x\r\r\n"), "x\r");
}

#[test]
fn splitting_handles_every_terminator_arrangement() {
    assert_eq!(split_lines("").count(), 0);
    assert_eq!(split_lines("\n").collect::<Vec<_>>(), vec![""]);
    assert_eq!(split_lines("\r\n").collect::<Vec<_>>(), vec![""]);
    assert_eq!(split_lines("a").collect::<Vec<_>>(), vec!["a"]);
    assert_eq!(split_lines("a\n\n").collect::<Vec<_>>(), vec!["a", ""]);
    assert_eq!(split_lines("\n\na").collect::<Vec<_>>(), vec!["", "", "a"]);
    // Mixed endings, as a file touched on two platforms has.
    assert_eq!(
        split_lines("a\r\nb\nc\r\n").collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
}

#[test]
fn splitting_preserves_content_containing_carriage_returns() {
    assert_eq!(split_lines("a\rb\n").collect::<Vec<_>>(), vec!["a\rb"]);
}

#[test]
fn bom_handling_is_exact() {
    assert_eq!(strip_bom(b""), b"");
    assert_eq!(strip_bom(b"\xEF\xBB\xBF"), b"");
    // Only one BOM is stripped; a second is content.
    assert_eq!(strip_bom(b"\xEF\xBB\xBF\xEF\xBB\xBFx"), b"\xEF\xBB\xBFx");
    // Partial or mismatched prefixes are untouched.
    assert_eq!(strip_bom(b"\xEF\xBB"), b"\xEF\xBB");
    assert_eq!(
        strip_bom(b"\xFE\xFFx"),
        b"\xFE\xFFx",
        "UTF-16 BOM is not ours to strip"
    );
    assert_eq!(strip_bom_str(""), "");
    assert_eq!(strip_bom_str("\u{FEFF}"), "");
}

#[test]
fn crlf_detection_needs_an_actual_pair() {
    assert!(!looks_crlf(b""));
    assert!(!looks_crlf(b"\r"));
    assert!(!looks_crlf(b"\n\r"), "LF followed by CR is not CRLF");
    assert!(looks_crlf(b"\r\n"));
}

// ---------------------------------------------------------------------------
// D15 — exit codes
// ---------------------------------------------------------------------------

#[test]
fn exit_code_boundaries() {
    assert_eq!(from_windows(0), 0);
    assert_eq!(from_windows(255), 255);
    assert_eq!(from_windows(256), 0, "256 truncates to 0, as bash does");
    assert_eq!(from_windows(257), 1);
    assert_eq!(from_windows(65_536), 0);
}

#[test]
fn every_ntstatus_error_is_non_zero() {
    // The guarantee that matters: a crash must never look like success. Sweep the whole
    // severity-error space rather than trusting the hand-written table.
    for high in 0..=0xFFu32 {
        for low in [0x00u32, 0x05, 0x42, 0xFF] {
            let code = 0xC000_0000 | (high << 8) | low;
            assert_ne!(
                from_windows(code),
                0,
                "NTSTATUS {code:#010X} reported as success"
            );
        }
    }
}

#[test]
fn ntstatus_detection_covers_only_the_error_severity() {
    assert!(is_ntstatus_error(0xC000_0005));
    assert!(is_ntstatus_error(0xFFFF_FFFF));
    // Severity 0b10 is "warning", 0b01 "informational", 0b00 "success".
    assert!(
        !is_ntstatus_error(0x8000_0005),
        "warning severity is not an error"
    );
    assert!(!is_ntstatus_error(0x4000_0005));
    assert!(!is_ntstatus_error(0x0000_0005));
}

#[test]
fn known_crashes_keep_their_linux_equivalent_numbers() {
    // A script written on Linux compares against these exact values.
    assert_eq!(
        from_windows(0xC000_0005),
        139,
        "access violation != segfault"
    );
    assert_eq!(from_windows(0xC000_00FD), 139, "stack overflow != segfault");
    assert_eq!(from_windows(0xC000_0094), 136, "divide by zero != SIGFPE");
    assert_eq!(from_windows(0xC000_0409), 134, "buffer overrun != SIGABRT");
}

// ---------------------------------------------------------------------------
// D5 / D31 — environment
// ---------------------------------------------------------------------------

#[test]
fn path_splitting_handles_degenerate_values() {
    assert_eq!(split_path("").count(), 0);
    assert_eq!(split_path(";").count(), 0);
    assert_eq!(split_path(";;;").count(), 0);
    assert_eq!(split_path("::").count(), 0);
    assert_eq!(split_path("C:/only").collect::<Vec<_>>(), vec!["C:/only"]);
}

#[test]
fn path_splitting_keeps_entries_containing_spaces() {
    let value = r"C:\Program Files\Git\bin;C:\tools";
    assert_eq!(
        split_path(value).collect::<Vec<_>>(),
        vec![r"C:\Program Files\Git\bin", r"C:\tools"]
    );
}

#[test]
fn a_unix_path_with_consecutive_drive_entries_splits_correctly() {
    // Two drive-spelled entries back to back is where a naive `:` split fails.
    let entries: Vec<&str> = split_path("C:/a:D:/b:E:/c").collect();
    assert_eq!(entries, vec!["C:/a", "D:/b", "E:/c"]);
}

#[test]
fn path_round_trips_through_both_forms_repeatedly() {
    let original = r"C:\tools;D:\sdk\bin";
    let once = path_to_windows(&path_to_unix(original));
    let twice = path_to_windows(&path_to_unix(&once));
    assert_eq!(once, original);
    assert_eq!(twice, original, "not idempotent");
}

#[test]
fn environment_lookup_is_case_insensitive_in_every_direction() {
    let mut env = Environment::new();
    env.set("MiXeD", "value");
    for spelling in ["MIXED", "mixed", "MiXeD", "mIxEd"] {
        assert_eq!(env.get(spelling), Some("value"), "failed for {spelling}");
    }
    assert!(env.remove("mixed").is_some());
    assert_eq!(
        env.get("MIXED"),
        None,
        "remove should be case-insensitive too"
    );
}

#[test]
fn canonicalisation_only_touches_known_posix_names() {
    assert_eq!(canonical_name("path"), "PATH");
    assert_eq!(canonical_name("PATH"), "PATH");
    // Unknown names keep their exact spelling — inventing one would be guessing.
    assert_eq!(canonical_name("MyVar"), "MyVar");
    assert_eq!(canonical_name("path_extra"), "path_extra");
    assert_eq!(canonical_name(""), "");
}

#[test]
fn only_path_is_translated_for_children() {
    let mut env = Environment::new();
    env.set("PATH", r"C:\a;C:\b");
    env.set("PYTHONPATH", r"C:\a;C:\b");

    let block = env.to_child_block();
    let path = &block.iter().find(|(k, _)| k == "PATH").unwrap().1;
    let pythonpath = &block.iter().find(|(k, _)| k == "PYTHONPATH").unwrap().1;

    assert_eq!(path, r"C:\a;C:\b");
    assert_eq!(
        pythonpath, r"C:\a;C:\b",
        "PYTHONPATH must pass through verbatim"
    );
}

// ---------------------------------------------------------------------------
// D32 — argument quoting
// ---------------------------------------------------------------------------

#[test]
fn quoting_survives_adversarial_arguments() {
    // Each of these is a classic way to break a naive quoter.
    for arg in [
        r#"a"b"#,
        r#""quoted""#,
        r"trailing\",
        r"\\double",
        r#"\"escaped"#,
        r#"a b"c\d"#,
        "",
        " ",
        "\t",
    ] {
        let quoted = quote_argument(arg);
        // An argument needing protection must end up wrapped; one that doesn't must not
        // gain spurious quotes.
        let needs_quotes = arg.is_empty() || arg.contains([' ', '\t', '"']);
        assert_eq!(
            quoted.starts_with('"') && quoted.ends_with('"'),
            needs_quotes,
            "wrong quoting decision for {arg:?} -> {quoted:?}"
        );
    }
}

#[test]
fn a_command_line_keeps_arguments_separable() {
    let line = build_command_line("tool.exe", &["a b".into(), "c".into()]);
    assert_eq!(line, r#"tool.exe "a b" c"#);
}

#[test]
fn cmd_escaping_covers_every_metacharacter() {
    for ch in ['&', '|', '<', '>', '(', ')', '^', '%', '!'] {
        let escaped = escape_for_cmd(&format!("x{ch}y"));
        assert!(
            escaped.contains(&format!("^{ch}")),
            "{ch} not caret-escaped in {escaped}"
        );
    }
}

#[test]
fn cmd_hazards_are_classified_precisely() {
    assert_eq!(is_safe_for_cmd(""), Ok(()));
    assert_eq!(is_safe_for_cmd("plain"), Ok(()));
    assert_eq!(
        is_safe_for_cmd(r"C:\a b\c"),
        Ok(()),
        "spaces alone are fine"
    );

    assert_eq!(is_safe_for_cmd("a%b"), Err(CmdHazard::PercentExpansion));
    assert_eq!(is_safe_for_cmd("a\rb"), Err(CmdHazard::Newline));
    assert_eq!(is_safe_for_cmd("a\nb"), Err(CmdHazard::Newline));
    assert_eq!(is_safe_for_cmd("a\0b"), Err(CmdHazard::Nul));

    // NUL is reported ahead of the others: it cannot appear in a command line at all.
    assert_eq!(is_safe_for_cmd("a%b\0"), Err(CmdHazard::Nul));
}

#[test]
fn the_cmd_command_line_is_wrapped_exactly_once() {
    let line = build_cmd_command_line("s.bat", &["a".into()]);
    assert!(line.starts_with("cmd.exe /d /s /c \""));
    assert!(line.ends_with('"'));
    // `/s` makes cmd strip precisely the first and last quote, so there must be exactly
    // one outer pair.
    let after_c = line.strip_prefix("cmd.exe /d /s /c ").unwrap();
    assert!(!after_c.starts_with("\"\""), "double-wrapped: {line}");
}
