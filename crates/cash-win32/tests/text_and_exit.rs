//! D20 (CRLF as a line terminator), D41 (BOM), D15 (exit codes).
//!
//! §9 measured all three failing in brush today, so these tests encode the target.

#![cfg(windows)]

use cash_win32::exit::{from_windows, is_ntstatus_error};
use cash_win32::text::{
    looks_crlf, split_lines, strip_bom, strip_bom_str, trim_line_terminator,
    trim_substitution_output,
};

#[test]
fn command_substitution_loses_the_carriage_return() {
    // The headline D20 bug. §9 measured 4 bytes — "val\r" — which then fails
    // [ "$v" = "val" ] while printing identically, because \r just moves the cursor.
    assert_eq!(trim_substitution_output("val\r\n"), "val");
    assert_eq!(trim_substitution_output("val\n"), "val");
    assert_eq!(trim_substitution_output("val"), "val");

    // Bash strips *all* trailing newlines, not just one.
    assert_eq!(trim_substitution_output("val\r\n\r\n\r\n"), "val");
    assert_eq!(trim_substitution_output("val\n\n"), "val");
}

#[test]
fn interior_carriage_returns_are_untouched() {
    // D20 is about line *terminators*, not about deleting \r from data.
    assert_eq!(trim_substitution_output("a\rb\r\n"), "a\rb");
    assert_eq!(trim_substitution_output("a\r\nb\r\n"), "a\r\nb");
}

#[test]
fn a_lone_trailing_carriage_return_is_data() {
    // No \n means no terminator. Stripping this would corrupt progress-bar output.
    assert_eq!(trim_substitution_output("val\r"), "val\r");
    assert_eq!(trim_line_terminator("val\r"), "val\r");
}

#[test]
fn read_strips_one_terminator_only() {
    assert_eq!(trim_line_terminator("b\r\n"), "b");
    assert_eq!(trim_line_terminator("b\n"), "b");
    assert_eq!(trim_line_terminator("b"), "b");
    // Only one: a blank CRLF line before this one is a separate line.
    assert_eq!(trim_line_terminator("b\r\n\r\n"), "b\r\n");
}

#[test]
fn splitting_treats_crlf_and_lf_alike() {
    let crlf: Vec<_> = split_lines("a\r\nb\r\n").collect();
    let lf: Vec<_> = split_lines("a\nb\n").collect();
    assert_eq!(crlf, vec!["a", "b"]);
    assert_eq!(crlf, lf);

    // Mixed endings, as a file touched by two tools on two platforms would have.
    let mixed: Vec<_> = split_lines("a\r\nb\nc").collect();
    assert_eq!(mixed, vec!["a", "b", "c"]);
}

#[test]
fn splitting_matches_bash_on_edges() {
    assert_eq!(split_lines("").count(), 0);
    assert_eq!(split_lines("\n").collect::<Vec<_>>(), vec![""]);
    // A trailing terminator does not invent a final empty line.
    assert_eq!(split_lines("a\n").collect::<Vec<_>>(), vec!["a"]);
    // An interior blank line is preserved.
    assert_eq!(split_lines("a\n\nb\n").collect::<Vec<_>>(), vec!["a", "", "b"]);
}

#[test]
fn bom_is_stripped() {
    // D41. §9 measured the failure precisely: `command not found: ﻿echo`.
    let with_bom = b"\xEF\xBB\xBFecho hi";
    assert_eq!(strip_bom(with_bom), b"echo hi");
    assert_eq!(strip_bom(b"echo hi"), b"echo hi");

    assert_eq!(strip_bom_str("\u{FEFF}echo hi"), "echo hi");
    assert_eq!(strip_bom_str("echo hi"), "echo hi");
}

#[test]
fn crlf_detection_is_for_diagnostics_only() {
    assert!(looks_crlf(b"a\r\nb"));
    assert!(!looks_crlf(b"a\nb"));
    assert!(!looks_crlf(b"a\rb"));
}

#[test]
fn ordinary_exit_codes_truncate_like_bash() {
    assert_eq!(from_windows(0), 0);
    assert_eq!(from_windows(1), 1);
    assert_eq!(from_windows(255), 255);
    // §9 measured this already working: cmd.exe /c "exit 300" -> 44.
    assert_eq!(from_windows(300), 44);
}

#[test]
fn access_violation_reports_139_exactly_like_a_segfault() {
    // The value a script written on Linux actually compares against.
    assert_eq!(from_windows(0xC000_0005), 139);
    assert_eq!(from_windows(0xC000_00FD), 139); // stack overflow
}

#[test]
fn a_crash_is_never_reported_as_success() {
    // The reason D15 exists. Naive truncation makes 0xC0000100 become 0 — a crash
    // indistinguishable from success, which would silently pass `&&` chains.
    assert_ne!(from_windows(0xC000_0100), 0);
    assert!(from_windows(0xC000_0100) >= 128);

    // Every NTSTATUS error must be non-zero, not just the ones in the table.
    for code in [0xC000_0100u32, 0xC000_0200, 0xC000_0300, 0xC000_0400] {
        assert_ne!(from_windows(code), 0, "NTSTATUS {code:#X} reported as success");
    }
}

#[test]
fn known_crash_classes_map_to_bash_signal_numbers() {
    assert_eq!(from_windows(0xC000_001D), 132); // illegal instruction -> SIGILL
    assert_eq!(from_windows(0xC000_0094), 136); // integer divide by zero -> SIGFPE
    assert_eq!(from_windows(0xC000_0409), 134); // stack buffer overrun -> SIGABRT
    assert_eq!(from_windows(0xC000_013A), 137); // Ctrl-C exit -> SIGKILL
}

#[test]
fn ntstatus_severity_detection() {
    assert!(is_ntstatus_error(0xC000_0005));
    assert!(!is_ntstatus_error(0));
    assert!(!is_ntstatus_error(1));
    assert!(!is_ntstatus_error(300));
}
