//! `less` and `more` — **D48**.
//!
//! `less` is its own GNU project, not coreutils, so the bundle never carried it: on
//! Windows it comes from Git for Windows if that is installed and not at all otherwise.
//! `more` resolved to `C:\Windows\System32\more.com`, the DOS tool, which `cash doctor`
//! already warned about — and the bundled uutils `more` got the one case that matters in
//! a script wrong, emitting `~` filler lines into a redirected pipe.
//!
//! cash carries one pager under both names. These tests are almost entirely about the
//! **non-interactive** half, because that is the half a script sees and the half that was
//! broken: a pager whose stdout is not a terminal must behave exactly like `cat`.
//!
//! The interactive half is not asserted here. Driving it needs a PTY, which D43 already
//! records as absent on Windows; what *is* asserted is that nothing interactive leaks
//! into the non-terminal path — no prompt, no filler, no escape sequences.

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

use crate::common::cash_command;

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

fn cash(script: &str) -> Output {
    let out = cash_command()
        .args(["-c", script])
        .output()
        .expect("failed to run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// Both names, so every behavioural test covers the alias too.
const NAMES: [&str; 2] = ["less", "more"];

// ---------------------------------------------------------------------------
// It is ours
// ---------------------------------------------------------------------------

#[test]
fn both_names_are_builtins() {
    // If `more` resolves externally, it is System32's DOS tool; if `less` does, it is a
    // Git for Windows install that may not be there.
    for name in NAMES {
        let out = cash(&format!("type {name}"));
        assert!(
            out.stdout.contains("shell builtin"),
            "{name} is not the builtin: {}",
            out.stdout
        );
    }
}

// ---------------------------------------------------------------------------
// The rule: not a terminal means `cat`
// ---------------------------------------------------------------------------

#[test]
fn a_pipe_passes_text_through_byte_for_byte() {
    // The bug that prompted this: the bundled `more` added `~` filler lines here, so
    // `cmd | more > out` produced junk.
    for name in NAMES {
        let out = cash(&format!(r#"printf 'a\nb\nc\n' | {name}"#));
        assert_eq!(out.stdout, "a\nb\nc\n", "{name} did not pass text through");
        assert!(!out.stdout.contains('~'), "{name} emitted filler");
    }
}

#[test]
fn a_redirected_pager_writes_exactly_the_input() {
    for name in NAMES {
        let out = cash(&format!(
            r#"d=$(mktemp -d); printf 'one\ntwo\n' | {name} > "$d/o"; od -c "$d/o" | head -1; cd /; rm -rf "$d""#
        ));
        assert!(
            out.stdout.contains("o   n   e  \\n   t   w   o  \\n"),
            "{name} wrote something other than its input: {}",
            out.stdout
        );
    }
}

#[test]
fn no_prompt_or_escape_sequence_reaches_a_pipe() {
    // `:` and `--More--` are the interactive prompts; neither may leak.
    for name in NAMES {
        let out = cash(&format!(r#"printf 'a\nb\n' | {name}"#));
        assert!(!out.stdout.contains("--More--"), "{name} leaked a prompt");
        assert!(
            !out.stdout.contains('\u{1b}'),
            "{name} leaked an escape sequence"
        );
        assert!(
            !out.stdout.contains('\r'),
            "{name} leaked a carriage return"
        );
    }
}

#[test]
fn a_long_input_is_not_paginated_into_a_pipe() {
    // Far more than any screen holds. A pager that paged here would block forever.
    for name in NAMES {
        let out = cash(&format!(r#"seq 1 500 | {name} | wc -l | tr -d ' '"#));
        assert_eq!(out.stdout.trim(), "500", "{name} lost or added lines");
    }
}

// ---------------------------------------------------------------------------
// Input edges
// ---------------------------------------------------------------------------

#[test]
fn empty_input_produces_empty_output() {
    for name in NAMES {
        let out = cash(&format!(r#"printf '' | {name}; echo "rc=$?""#));
        assert_eq!(
            out.stdout, "rc=0\n",
            "{name} invented output for empty input"
        );
    }
}

#[test]
fn input_without_a_trailing_newline_gains_exactly_one() {
    // `cat` leaves it alone; a line-oriented pager necessarily terminates the last line.
    // Either is defensible — what is not is dropping the text or doubling the newline.
    for name in NAMES {
        let out = cash(&format!(r#"printf 'no-newline' | {name}"#));
        assert_eq!(
            out.stdout, "no-newline\n",
            "{name} mangled an unterminated line"
        );
    }
}

#[test]
fn a_blank_line_is_preserved() {
    for name in NAMES {
        let out = cash(&format!(r#"printf 'a\n\nb\n' | {name}"#));
        assert_eq!(out.stdout, "a\n\nb\n", "{name} dropped a blank line");
    }
}

#[test]
fn crlf_input_is_normalised_like_every_other_line_reader() {
    // D20: wherever cash decides where a line ends, `\r\n` ends it.
    for name in NAMES {
        let out = cash(&format!(r#"printf 'a\r\nb\r\n' | {name}"#));
        assert_eq!(
            out.stdout, "a\nb\n",
            "{name} kept a carriage return: {:?}",
            out.stdout
        );
    }
}

#[test]
fn a_lone_carriage_return_inside_a_line_survives() {
    // Progress output is written that way on purpose; it is data, not a terminator.
    for name in NAMES {
        let out = cash(&format!(r#"printf 'a\rb\n' | {name}"#));
        assert_eq!(out.stdout, "a\rb\n", "{name} ate a data carriage return");
    }
}

#[test]
fn unicode_and_spaces_survive() {
    for name in NAMES {
        let out = cash(&format!(r#"printf 'Ünïcodé  ünd  Leerzeichen\n' | {name}"#));
        assert_eq!(
            out.stdout, "Ünïcodé  ünd  Leerzeichen\n",
            "{name} mangled unicode"
        );
    }
}

#[test]
fn a_very_long_line_is_not_truncated() {
    for name in NAMES {
        let out = cash(&format!(
            r#"python -c "print('x'*5000)" 2>/dev/null | {name} | wc -c | tr -d ' '"#
        ));
        let counted: usize = out.stdout.trim().parse().unwrap_or(0);
        if counted == 0 {
            eprintln!("skipped: no python to generate the line");
            continue;
        }
        assert!(
            counted >= 5000,
            "{name} truncated a long line to {counted} bytes"
        );
    }
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

#[test]
fn a_file_argument_is_read() {
    for name in NAMES {
        let out = cash(&format!(
            r#"d=$(mktemp -d); printf 'from-file\n' > "$d/f"; {name} "$d/f"; cd /; rm -rf "$d""#
        ));
        assert_eq!(out.stdout, "from-file\n", "{name} did not read its file");
    }
}

#[test]
fn several_files_are_concatenated_in_order() {
    for name in NAMES {
        let out = cash(&format!(
            r#"d=$(mktemp -d); printf 'first\n' > "$d/1"; printf 'second\n' > "$d/2"; {name} "$d/1" "$d/2"; cd /; rm -rf "$d""#
        ));
        assert_eq!(
            out.stdout, "first\nsecond\n",
            "{name} mis-ordered its files"
        );
    }
}

#[test]
fn a_dash_means_standard_input() {
    for name in NAMES {
        let out = cash(&format!(r#"printf 'piped\n' | {name} -"#));
        assert_eq!(out.stdout, "piped\n", "{name} did not read `-` as stdin");
    }
}

#[test]
fn a_missing_file_is_reported_and_fails() {
    for name in NAMES {
        let out = cash(&format!(r#"{name} /definitely/not/here; echo "rc=$?""#));
        assert!(
            out.stderr.contains("/definitely/not/here"),
            "{name} did not name the missing file: {}",
            out.stderr
        );
        assert!(
            out.stdout.contains("rc=") && !out.stdout.contains("rc=0"),
            "{name} reported success for a missing file: {}",
            out.stdout
        );
    }
}

#[test]
fn a_readable_file_after_a_missing_one_is_still_shown() {
    // Failing the whole invocation because one of several files is gone would lose the
    // output the user can actually have.
    for name in NAMES {
        let out = cash(&format!(
            r#"d=$(mktemp -d); printf 'present\n' > "$d/ok"; {name} /nope "$d/ok" 2>/dev/null; cd /; rm -rf "$d""#
        ));
        assert!(
            out.stdout.contains("present"),
            "{name} dropped a readable file: {}",
            out.stdout
        );
    }
}

#[test]
fn a_directory_is_reported_rather_than_dumped() {
    for name in NAMES {
        let out = cash(&format!(
            r#"d=$(mktemp -d); {name} "$d"; echo "rc=$?"; cd /; rm -rf "$d""#
        ));
        assert!(
            out.stdout.contains("rc=") && !out.stdout.contains("rc=0"),
            "{name} claimed success for a directory: {} {}",
            out.stdout,
            out.stderr
        );
    }
}

#[test]
fn binary_input_does_not_panic() {
    // A pager gets pointed at a binary by accident constantly. Garbled output is fine;
    // a crash is not.
    for name in NAMES {
        let out = cash(&format!(
            r#"d=$(mktemp -d); printf 'a\000b\377c\n' > "$d/bin"; {name} "$d/bin" > /dev/null; echo "rc=$?"; cd /; rm -rf "$d""#
        ));
        assert!(
            out.stdout.contains("rc=0"),
            "{name} failed on binary: {}",
            out.stderr
        );
    }
}

// ---------------------------------------------------------------------------
// `less` options
// ---------------------------------------------------------------------------

#[test]
fn line_numbers_are_added_on_request() {
    let out = cash(r#"printf 'a\nb\n' | less -N"#);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "wrong line count: {:?}", out.stdout);
    assert!(
        lines[0].trim_start().starts_with("1 "),
        "no line number: {:?}",
        lines[0]
    );
    assert!(
        lines[1].trim_start().starts_with("2 "),
        "no line number: {:?}",
        lines[1]
    );
}

#[test]
fn the_options_git_sets_are_accepted() {
    // `git` exports `LESS=FRX` and passes those through; rejecting them would make cash
    // unusable as `git`'s pager.
    for flags in ["-F", "-R", "-X", "-FRX", "-F -R -X", "-S", "-e"] {
        let out = cash(&format!(r#"printf 'a\n' | less {flags}; echo "rc=$?""#));
        assert!(
            out.stdout.contains("rc=0"),
            "less {flags} was rejected: {} {}",
            out.stdout,
            out.stderr
        );
        assert!(
            out.stdout.starts_with('a'),
            "less {flags} lost the text: {}",
            out.stdout
        );
    }
}

#[test]
fn an_unknown_option_is_rejected_rather_than_ignored() {
    let out = cash(r#"printf 'a\n' | less --definitely-not-an-option; echo "rc=$?""#);
    assert!(
        !out.stdout.contains("rc=0"),
        "an unknown option was silently accepted: {}",
        out.stdout
    );
}

// ---------------------------------------------------------------------------
// Where a pager actually gets used
// ---------------------------------------------------------------------------

#[test]
fn it_works_as_the_middle_of_a_pipeline() {
    // 1, then 10 through 19: eleven lines beginning with a 1.
    let out = cash(r#"seq 1 20 | less | grep -c '^1'"#);
    assert_eq!(out.stdout.trim(), "11", "stderr: {}", out.stderr);
}

#[test]
fn it_works_in_a_command_substitution() {
    // `x=$(cmd | less)` is odd but happens, and command substitution is never a terminal.
    let out = cash(r#"x=$(printf 'captured\n' | less); echo "[$x]""#);
    assert_eq!(out.stdout, "[captured]\n", "stderr: {}", out.stderr);
}

#[test]
fn it_is_usable_as_a_pager_variable() {
    // The shape `PAGER=less git log | $PAGER` takes, reduced to something hermetic.
    let out = cash(r#"PAGER=less; printf 'paged\n' | $PAGER"#);
    assert_eq!(out.stdout, "paged\n", "stderr: {}", out.stderr);
}

/// Not to a terminal, each line goes on as it comes, as `cat` passes it: the pager read
/// all its input first, so `more <(tail -f log) > f` wrote nothing until the input ended.
#[test]
fn each_line_goes_on_as_it_comes() {
    for name in NAMES {
        let out = cash(&format!(
            r#"f=$(mktemp); {name} <(echo first; sleep 6; echo last) > "$f" & sleep 2
               printf '[%s]' "$(cat "$f")"; kill %1; wait 2>/dev/null; rm -f "$f""#
        ));
        assert_eq!(out.stdout, "[first]", "{name}: {}", out.stderr);
    }
}

#[test]
fn a_closed_pipe_does_not_produce_a_diagnostic() {
    // `less | head -1` closes the pipe early. That is normal and must be silent.
    let out = cash(r#"seq 1 500 | less | head -1"#);
    assert_eq!(out.stdout.trim(), "1", "stderr: {}", out.stderr);
    assert!(
        !out.stderr.to_lowercase().contains("broken pipe"),
        "a broken pipe was reported: {}",
        out.stderr
    );
    assert_eq!(out.code, 0);
}
