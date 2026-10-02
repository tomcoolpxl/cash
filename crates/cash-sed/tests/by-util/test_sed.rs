// Integration tests
//
// SPDX-License-Identifier: MIT
// Copyright (c) 2025 Diomidis Spinellis
//
// This file is part of the uutils sed package.
// It is licensed under the MIT License.
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::fs;
use std::io::{Read, Write};

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use uutests::new_ucmd;

////////////////////////////////////////////////////////////
// Test application's invocation
#[test]
fn test_invalid_arg() {
    new_ucmd!().arg("--definitely-invalid").fails().code_is(1);
}

#[test]
fn test_version() {
    let short = new_ucmd!()
        .arg("-V")
        .succeeds()
        .no_stderr()
        .stdout_str()
        .to_owned();

    new_ucmd!()
        .arg("--version")
        .succeeds()
        .no_stderr()
        .stdout_is(&short);

    assert!(short.contains("uutils"));
}

#[test]
fn test_help_mentions_uutils() {
    new_ucmd!()
        .arg("--help")
        .succeeds()
        .no_stderr()
        .stdout_contains("part of uutils");
}

#[test]
fn test_debug() {
    new_ucmd!().args(&["--debug", ""]).succeeds();
}

#[test]
fn test_silent_alias() {
    new_ucmd!().args(&["--silent", ""]).succeeds();
}

#[test]
fn test_missing_script_argument() {
    new_ucmd!()
        .args(&["-n"])
        .fails()
        .code_is(1)
        .stderr_contains("missing script");
}

#[test]
fn test_no_arguments() {
    new_ucmd!()
        .fails()
        .code_is(1)
        .stdout_contains("Stream editor for filtering and transforming text");
}

#[test]
fn test_positional_script_ok() {
    new_ucmd!().arg("l").succeeds().code_is(0);
}

#[test]
fn test_empty_positional_script_ok() {
    new_ucmd!().arg("").succeeds().code_is(0);
}

#[test]
fn test_e_script_ok() {
    new_ucmd!().args(&["-e", "l"]).succeeds();
}

#[test]
fn test_f_script_ok() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-f", "script/hanoi.sed"])
        .succeeds()
        .stdout_is_bytes(b"");
}

////////////////////////////////////////////////////////////
// Test simple I/O processing
const INPUT_FILES: &[&str] = &[
    "input/two-lines.txt",
    "input/no-new-line.txt",
    "input/dots-4k.txt",
    "input/dots-8k.txt",
    "input/dots-64k.txt",
];

#[test]
fn test_no_script_stdin() {
    for fixture in INPUT_FILES {
        new_ucmd!()
            .arg("")
            .pipe_in_fixture(fixture)
            .succeeds()
            .stdout_is_fixture(fixture);
    }
}

#[test]
fn test_no_script_file() {
    for fixture in INPUT_FILES {
        new_ucmd!()
            .args(&["-e", "", fixture])
            .succeeds()
            .stdout_is_fixture(fixture);
    }
}

/// Test concatenation of multiple input files.
#[test]
fn test_multiple_input_files() {
    new_ucmd!()
        .args(&[
            "-e",
            "",
            "input/dots-64k.txt",
            "input/no-new-line.txt",
            "input/dots-64k.txt",
        ])
        .succeeds()
        .stdout_is_fixture("output/multiple_input_files");
}

#[test]
fn test_delete_stdin() {
    for fixture in INPUT_FILES {
        new_ucmd!()
            .arg("d")
            .pipe_in_fixture(fixture)
            .succeeds()
            .no_stdout();
    }
}

#[test]
fn test_delete_file() {
    for fixture in INPUT_FILES {
        new_ucmd!()
            .args(&["-e", "d", fixture])
            .succeeds()
            .no_stdout();
    }
}

/// Create a new test function to verify an execution for specified output.
macro_rules! check_output {
    ($name:ident, $args:expr) => {
        #[test]
        fn $name() {
            new_ucmd!()
                .args(&$args)
                .succeeds()
                .stdout_is_fixture(&format!("output/{}", stringify!($name)));
        }
    };
}

// Run ucmd twice to test POSIX conformance: Once where posix is "--posix"
// and once where it has the dummy value "--follow-symlinks".
// This shall be used to test commands that behave differently under POSIX.
macro_rules! check_output_posix {
    ($name:ident, [$($args:expr),* $(,)?]) => {
        #[test]
        fn $name() {
            for posix in ["--posix", "--follow-symlinks"] {
                new_ucmd!()
                    .args(&[posix $(, $args)*]) // prepend posix, then add args
                    .succeeds()
                    .stdout_is_fixture(&format!("output/{}", stringify!($name)));
            }
        }
    };
}

////////////////////////////////////////////////////////////
// Individual command tests

// Input files
const LINES1: &str = "input/lines1";
const LINES2: &str = "input/lines2";
const NO_NEW_LINE: &str = "input/no-new-line.txt";

////////////////////////////////////////////////////////////
// Comments
check_output!(comment, ["-n", "-e", "4p;# comment", LINES1]);
check_output!(comment_silent, ["-e", "#n", "-e", "4p", LINES1]);
check_output!(comment_no_silent, ["-e", "4p;#n comment", LINES1]);

////////////////////////////////////////////////////////////
// Address ranges: One and two, numeric and pattern
check_output!(addr_one_line, ["-n", "-e", "4p", LINES1]);
check_output!(addr_straddle, ["-n", "-e", "20p", LINES1, LINES2]);
check_output!(addr_last_one_file, ["-n", "-e", "$p", LINES1]);
check_output!(addr_last_two_files, ["-n", "-e", "$p", LINES1, LINES2]);

check_output!(addr_append_empty, ["-e", "$a\\\nhello", "input/empty"]);

check_output!(
    addr_last_empty,
    ["-n", "-e", "$p", LINES1, "input/empty", LINES2]
);

#[test]
fn addr_last_trailing_empty() {
    new_ucmd!()
        .args(&["-n", "-e", "$p", LINES1, "input/empty"])
        .succeeds()
        .stdout_is_fixture("output/addr_last_one_file");
}

check_output!(addr_past_last, ["-n", "-e", "20p", LINES1]);
check_output!(addr_not_found, ["-n", "-e", "/NOTFOUND/p", LINES1]);
check_output!(addr_found, ["-n", "/l1_7/p", LINES1]);
check_output!(addr_found_space, ["-n", " /l1_7/ p", LINES1]);
check_output!(addr_escaped_delimiter, ["-n", "\\_l1\\_7_p", LINES1]);
check_output!(addr_range_numeric, ["-n", "1,4p", LINES1]);
check_output!(addr_range_to_last, ["-n", "1,$p", LINES1, LINES2]);
check_output!(addr_range_to_pattern, ["-n", "1,/l2_9/p", LINES1, LINES2]);
check_output!(
    addr_range_straddle,
    ["-n", "/l1_3/,/l2_3/p", LINES1, LINES2]
);
check_output!(
    addr_range_separate,
    ["-n", "--separate", "/l1_3/,/l2_3/p", LINES1, LINES2]
);
check_output!(addr_range_from_zero_to_pattern, ["-n", "0,/_1/p", LINES1]);
check_output!(addr_pattern_to_last, ["-n", "/4/,$p", LINES1, LINES2]);
check_output!(addr_pattern_to_straddle, ["-n", "/4/,20p", LINES1, LINES2]);
check_output!(addr_pattern_to_pattern, ["-n", "/4/,/10/p", LINES1, LINES2]);
check_output!(
    addr_pattern_straddle,
    ["-n", "/l2_3/,/l1_8/p", LINES1, LINES2]
);
check_output!(addr_range_reverse, ["-n", "12,3p", LINES1, LINES2]);
check_output!(
    addr_pattern_range_reverse,
    ["-n", "/l1_7/,3p", LINES1, LINES2]
);
check_output!(addr_numeric_to_relative, ["-n", "13,+4p", LINES1, LINES2]);
check_output!(
    addr_pattern_to_relative,
    ["-n", "/l1_6/,+2p", LINES1, LINES2]
);
check_output!(addr_numeric_relative_straddle, ["-n", "12,+1p", LINES1]);
check_output!(
    addr_first_separate,
    ["-n", "--separate", "1p", LINES1, LINES2]
);
check_output!(addr_last_separate, ["-ns", "$p", LINES1, LINES2]);
check_output!(addr_two_lines_semicolon, ["-n", "-e", "4p;8p", LINES1]);
check_output!(addr_two_lines_newline, ["-n", "-e", "4p\n8p", LINES1]);
check_output!(addr_three_lines_semicolon, ["-n", "-e", "4p;8p;1p", LINES1]);
check_output!(addr_one_line_negate, ["-n", "-e", "4!p", LINES1]);
check_output!(addr_range_numeric_negate, ["-n", "1,4!p", LINES1]);
check_output!(
    addr_pattern_to_pattern_negate,
    ["-n", "/1_4/,/10/!p", LINES1]
);
check_output!(addr_empty_re_reuse, ["-n", "/_2/,//p", LINES1, LINES2]);
check_output!(addr_simple_negation, ["-e", r"4,12!s/^/^/", LINES1]);
check_output!(addr_range_even, ["-n", "0~2p", LINES1]);
check_output!(addr_range_odd, ["-n", "1~2p", LINES1]);
check_output!(addr_range_step_zero, ["-n", "10~0p", LINES1]);
check_output!(addr_range_end_multiple, ["-n", "/l1_2/,~10p", LINES1]);

#[test]
fn command_may_end_before_closing_brace() {
    for script in ["{p}", "1{p}", "/a/{p}"] {
        new_ucmd!()
            .arg(script)
            .pipe_in("a\n")
            .succeeds()
            .stdout_is("a\na\n");
    }
}

////////////////////////////////////////////////////////////

// Quantifiers: {m,n}
// m and n are considered to be the first and second numbers in the interval, respectively.

const REGEX_QUANTIFIERS_INPUT: &str =
    "Hello World\nHelo World\nHelllllo World\nHeo Word\nHeo Worl}d\n";

#[test]
fn ere_quantifier_exactly_m() {
    new_ucmd!()
        .args(&["-n", "-E", "-e", "/l{2}/p"])
        .pipe_in(REGEX_QUANTIFIERS_INPUT)
        .succeeds()
        .stdout_is("Hello World\nHelllllo World\n");
}

#[test]
fn ere_quantifier_minimum_m() {
    new_ucmd!()
        .args(&["-n", "-E", "-e", "/l{1,}/p"])
        .pipe_in(REGEX_QUANTIFIERS_INPUT)
        .succeeds()
        .stdout_is("Hello World\nHelo World\nHelllllo World\nHeo Worl}d\n");
}

#[test]
fn ere_quantifier_m_to_n() {
    new_ucmd!()
        .args(&["-n", "-E", "-e", "/l{3,4}/p"])
        .pipe_in(REGEX_QUANTIFIERS_INPUT)
        .succeeds()
        .stdout_is("Helllllo World\n");
}

#[test]
fn ere_quantifier_comma_n() {
    new_ucmd!()
        .args(&["-n", "-E", "-e", "/l{,4}/p"])
        .pipe_in(REGEX_QUANTIFIERS_INPUT)
        .succeeds()
        .stdout_is(REGEX_QUANTIFIERS_INPUT);
}

#[test]
fn bre_quantifier_minimum_m() {
    new_ucmd!()
        .args(&["-n", "-e", "/l\\{3,\\}/p"])
        .pipe_in(REGEX_QUANTIFIERS_INPUT)
        .succeeds()
        .stdout_is("Helllllo World\n");
}

#[test]
fn bre_quantifier_comma() {
    new_ucmd!()
        .args(&["-n", "-e", "/l\\{,\\}/p"])
        .pipe_in(REGEX_QUANTIFIERS_INPUT)
        .succeeds()
        .stdout_is(REGEX_QUANTIFIERS_INPUT);
}

#[test]
fn bre_quantifier_only_closing_brace() {
    new_ucmd!()
        .args(&["-n", "-e", "/l\\}/p"])
        .pipe_in(REGEX_QUANTIFIERS_INPUT)
        .succeeds()
        .stdout_is("Heo Worl}d\n");
}

#[test]
fn test_ere_quantifier_n_gt_m() {
    new_ucmd!()
        .args(&["-E", "-e", "/l{3,2}/p"])
        .fails()
        .code_is(1)
        .stderr_contains("Invalid content of \\{\\}");
}

#[test]
fn test_ere_quantifier_negative_m() {
    new_ucmd!()
        .args(&["-E", "-e", "/l{-2,4}/p"])
        .fails()
        .code_is(1)
        .stderr_contains("Invalid content of \\{\\}");
}

#[test]
fn test_ere_quantifier_invalid_m() {
    new_ucmd!()
        .args(&["-E", "-e", "/l{d,}/p"])
        .fails()
        .code_is(1)
        .stderr_contains("Invalid content of \\{\\}");
}

#[test]
fn test_ere_quantifier_m_too_big() {
    new_ucmd!()
        .args(&["-E", "-e", "/l{32768,}/p"])
        .fails()
        .code_is(1)
        .stderr_contains("Regular expression too big");
}

#[test]
fn test_ere_quantifier_empty() {
    new_ucmd!()
        .args(&["-E", "-e", "/l{}/p"])
        .fails()
        .code_is(1)
        .stderr_contains("Invalid content of \\{\\}");
}

#[test]
fn test_ere_quantifier_whitespace() {
    new_ucmd!()
        .args(&["-E", "-e", "/l{ }/p"])
        .fails()
        .code_is(1)
        .stderr_contains("Invalid content of \\{\\}");
}

#[test]
fn test_ere_quantifier_unmatched_brace() {
    new_ucmd!()
        .args(&["-E", "-e", "/l{,/p"])
        .fails()
        .code_is(1)
        .stderr_contains("Unmatched \\{");
}

#[test]
fn test_ere_quantifier_unmatched_brace_2() {
    new_ucmd!()
        .args(&["-E", "-e", "/l{m,n/p"])
        .fails()
        .code_is(1)
        .stderr_contains("Unmatched \\{");
}

#[test]
fn test_bre_quantifier_unmatched_brace() {
    new_ucmd!()
        .args(&["-e", "/l\\{1,2}/p"])
        .fails()
        .code_is(1)
        .stderr_contains("Unmatched \\{");
}

#[test]
fn test_ere_quantifier_leading_comma_n_too_big() {
    // The {,n} form must enforce the RE_DUP_MAX upper bound on n.
    new_ucmd!()
        .args(&["-E", "-e", "/l{,32768}/p"])
        .fails()
        .code_is(1)
        .stderr_contains("Regular expression too big");
}

// A closing brace used as the regex delimiter must terminate the regex
// rather than being treated as a literal quantifier brace.
#[test]
fn ere_closing_brace_delimiter() {
    new_ucmd!()
        .args(&["-E", "-e", "s}x}-}g"])
        .pipe_in("axbxc\n")
        .succeeds()
        .stdout_is("a-b-c\n");
}

#[test]
fn ere_opening_brace_delimiter() {
    new_ucmd!()
        .args(&["-E", "-e", "s{x{-{g"])
        .pipe_in("axbxc\n")
        .succeeds()
        .stdout_is("a-b-c\n");
}

#[test]
fn bre_closing_brace_delimiter() {
    new_ucmd!()
        .args(&["-e", "s}x}-}g"])
        .pipe_in("axbxc\n")
        .succeeds()
        .stdout_is("a-b-c\n");
}

// Substitution: s
check_output!(subst_any, ["-e", r"s/./X/g", LINES1]);
check_output!(subst_any_global, ["-e", r"s,.,X,g", LINES1]);
check_output!(subst_escaped_magic_separator, ["-e", r"s.\..X.g", LINES1]);
check_output!(subst_escaped_braced_separator, ["-e", r"s/[\/]/Q/", LINES1]);
check_output!(subst_escaped_separator, ["-e", r"s_\__X_", LINES1]);
check_output!(subst_whole_match_group, ["-e", r"s/./(&)/g", LINES1]);
check_output!(subst_print, ["-ne", "s/1_1/S&/p", LINES1]);
check_output!(
    subst_escaped_whole_match_group,
    ["-e", r"s/./(\&)/g", LINES1]
);
check_output!(
    subst_numerical_groups,
    ["-e", r"s/\(.\)\(.\)\(.\)/x\3x\2x\1/g", LINES1]
);
check_output!(
    subst_ere_numerical_groups,
    [
        "--regexp-extended",
        "-e",
        r"s/(.)(.)(.)/x\3x\2x\1/g",
        LINES1
    ]
);
check_output!(
    subst_quantifier_zero_or_one,
    ["-e", r"s/_[0-9]\([0-9]\)\?/_x\1/g", LINES1]
);
check_output!(
    subst_quantifier_one_or_more,
    ["-e", r"s/_[0-9]\+/_x/g", LINES1]
);
check_output!(subst_alternation_operator, ["-e", r"s/0\|1/x/g", LINES1]);

#[test]
fn subst_posix_alternation_longest() {
    new_ucmd!()
        .args(&["-E", "s/a|aa/X/"])
        .pipe_in("aa\n")
        .succeeds()
        .stdout_is_bytes(b"X\n");
}

#[test]
fn test_h_n_g_p_cycle() {
    new_ucmd!()
        .args(&["-n", "-e", "h;n;G;p"])
        .pipe_in("line1\nline2\n")
        .succeeds()
        .stdout_is_bytes(b"line2\nline1\n");
}
check_output!(subst_multiline, ["-e", "s/_/u0\\\nu1\\\nu2/g", LINES1]);
check_output!(subst_numbered_replacement, ["-e", r"s/./X/4", LINES1]);
check_output!(subst_brace, ["-e", r"s/[123]/X/g", LINES1]);
check_output!(subst_case_insensitive, ["-e", r"s/L/Line/", LINES1]);
check_output!(subst_no_new_line, ["-e", r"s/l/L/g", NO_NEW_LINE]);
check_output!(subst_re_reuse, ["-e", r"2s//M/;1s/l/L/", LINES1]);
check_output!(subst_newline_class, ["-n", r"1{;N;s/[\n]/X/;p;}", LINES1]);
check_output!(subst_newline_re, ["-n", r"1{;N;s/\n/X/;p;}", LINES1]);

#[test]
fn subst_dot_matches_newline() {
    new_ucmd!()
        .arg("N;s/foo.*bar/X/")
        .pipe_in("foo\nbar\n")
        .succeeds()
        .stdout_is("X\n");
}

#[test]
fn subst_multiline_dot_does_not_match_newline() {
    new_ucmd!()
        .arg("N;s/foo.*bar/X/m")
        .pipe_in("foo\nbar\n")
        .succeeds()
        .stdout_is("foo\nbar\n");
}

#[test]
fn subst_multiline_flag_matches_embedded_line_start() {
    new_ucmd!()
        .arg("N;s/^./X/gm")
        .pipe_in("foo\nbar\n")
        .succeeds()
        .stdout_is("Xoo\nXar\n");
}

#[test]
fn subst_multiline_flag_matches_embedded_line_end() {
    new_ucmd!()
        .arg("N;s/.$/X/gM")
        .pipe_in("foo\nbar\n")
        .succeeds()
        .stdout_is("foX\nbaX\n");
}

// Check appropriate selection and behavior of fast_Regex matcher
// Literal matcher
check_output!(subst_literal_start, ["-e", r"s/^l1/L1/", LINES1]);
check_output!(subst_literal_end, ["-e", r"s/2$/TWO/", LINES1]);
check_output!(subst_literal, ["-e", r"s/_/-/", LINES1]);

// Fancy matcher
/// Substitute using a backreference under an explicit UTF-8 locale.
#[test]
fn subst_backref() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-e", r"s/l\(.\)_\1/same-number/", LINES1])
        .succeeds()
        .stdout_is_fixture_bytes("output/subst_backref");
}

/// Substitute a Unicode range under an explicit UTF-8 locale.
#[test]
fn subst_greek() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-e", r"s/[α-ω]/G/g", "input/unicode"])
        .succeeds()
        .stdout_is_fixture_bytes("output/subst_greek");
}

/// Substitute the final Unicode scalar under an explicit UTF-8 locale.
#[test]
fn subst_any_unicode() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-e", r"s/.$/:-)/", "input/unicode"])
        .succeeds()
        .stdout_is_fixture_bytes("output/subst_any_unicode");
}

/// Substitute case-insensitive Unicode text under an explicit UTF-8 locale.
#[test]
fn subst_lcase() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-e", r"s/κ/*/gi", "input/unicode"])
        .succeeds()
        .stdout_is_fixture_bytes("output/subst_lcase");
}

/// Substitute Unicode word matches under an explicit UTF-8 locale.
#[test]
fn subst_word() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-E", "-e", r"s/\w+/WORD/g", "input/unicode"])
        .succeeds()
        .stdout_is_fixture_bytes("output/subst_word");
}

/// Allow substitution back-references in default/C locale.
#[test]
fn subst_backref_rejected_in_c_locale() {
    new_ucmd!()
        .env("LC_ALL", "C")
        .args(&["-e", r"s/\(.\)\1/X/"])
        .pipe_in("aa\n")
        .succeeds()
        .stdout_is_bytes(b"X\n");
}

/// Allow substitution back-references when the locale selects UTF-8 mode.
#[test]
fn subst_backref_allowed_in_c_utf8_locale() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-e", r"s/\(.\)\1/X/"])
        .pipe_in("aa\n")
        .succeeds()
        .stdout_is_bytes(b"X\n");
}

/// Match non-UTF-8 input bytes with byte escapes in byte mode.
#[test]
fn subst_byte_escape_matches_invalid_input_in_c_locale() {
    new_ucmd!()
        .env("LC_ALL", "C")
        .args(&["-e", r"s/\xE9/Z/"])
        .pipe_in(b"\xE9\n".to_vec())
        .succeeds()
        .stdout_is_bytes(b"Z\n");
}

/// Match raw invalid UTF-8 script bytes as literal bytes in UTF-8 mode.
#[test]
fn subst_raw_invalid_script_bytes_match_literal_in_c_utf8_locale() {
    let mut script = NamedTempFile::new().expect("create temporary sed script");
    script
        .write_all(b"s/\xC2\xE7\xCF\xC2/\xC6\xFC\xCB\xDC/\n")
        .expect("write temporary sed script");
    let script_path = script.path().to_str().expect("temporary path is UTF-8");

    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-f", script_path])
        .pipe_in(b"\xA4\xC4 \xC2\xE7\xCF\xC2\xA4\xCE\n".to_vec())
        .succeeds()
        .stdout_is_bytes(b"\xA4\xC4 \xC6\xFC\xCB\xDC\xA4\xCE\n");
}

#[test]
fn subst_write_file() -> std::io::Result<()> {
    let temp = NamedTempFile::new()?;
    let path = temp.path();
    let cmd = format!("s/_1/S_1/w {}", path.display());

    new_ucmd!().args(&["-n", &cmd, LINES1]).succeeds();

    let mut actual = String::new();
    temp.reopen()?.read_to_string(&mut actual)?;

    let expected = fs::read_to_string("tests/fixtures/sed/output/subst_write_file")?;
    assert_eq!(actual, expected, "Output did not match fixture");

    Ok(())
}

#[test]
fn test_subst_e_flag_basic() {
    new_ucmd!()
        .arg("s/.*/echo hi/e")
        .pipe_in("a\n")
        .succeeds()
        .stdout_is("hi\n");
}

#[test]
fn test_subst_e_flag_preserves_unmatched_lines() {
    new_ucmd!()
        .args(&["-e", "s/^match$/echo replaced/e"])
        .pipe_in("no\nmatch\nno\n")
        .succeeds()
        .stdout_is("no\nreplaced\nno\n");
}

#[test]
fn test_subst_e_flag_strips_trailing_newline() {
    // echo produces "hello\n", e flag should strip trailing newline
    new_ucmd!()
        .arg("s/.*/echo hello/e")
        .pipe_in("x\n")
        .succeeds()
        .stdout_is("hello\n");
}

#[test]
fn test_subst_e_flag_combined_with_g() {
    // e flag with other flags
    new_ucmd!().arg("s/x/echo y/ge").pipe_in("x\n").succeeds();
}

#[test]
fn test_subst_e_flag_rejected_with_posix() {
    // e flag is rejected at compile time if --posix or --sandbox is provided.
    new_ucmd!()
        .args(&["--posix", "s/.*/echo hi/e"])
        .fails()
        .stderr_contains("not allowed with --posix or --sandbox");
}

#[test]
fn test_subst_e_flag_rejected_with_sandbox() {
    new_ucmd!()
        .args(&["--sandbox", "s/.*/echo hi/e"])
        .fails()
        .stderr_contains("not allowed with --posix or --sandbox");
}

#[test]
fn test_subst_e_flag_command_failure() {
    // A non-existent command produces empty output but sed itself succeeds
    // (matching GNU sed behavior: the shell runs, the command inside fails)
    new_ucmd!()
        .arg("s/.*/nonexistent_command/e")
        .pipe_in("a\n")
        .succeeds()
        .stdout_is("\n");
}

#[test]
fn test_subst_e_flag_no_match_no_exec() {
    // If substitution doesn't match, command should not execute
    new_ucmd!()
        .arg("s/nomatch/echo bad/e")
        .pipe_in("hello\n")
        .succeeds()
        .stdout_is("hello\n");
}

////////////////////////////////////////////////////////////
// e command (execute)
// The with-argument form writes the shell's raw, unmodified output to the
// stream, while sed's own pattern-space auto-print always uses LF.
#[test]
fn test_e_command_with_arg_basic() {
    // With an argument, the command runs immediately and its output is
    // written to the stream before the (unmodified) pattern space.
    new_ucmd!()
        .arg("e echo hi")
        .pipe_in("a\n")
        .succeeds()
        .stdout_is("hi\na\n");
}

#[test]
fn test_e_command_with_arg_no_space_required() {
    // No whitespace is required between 'e' and its argument.
    new_ucmd!()
        .arg("eecho hi")
        .pipe_in("a\n")
        .succeeds()
        .stdout_is("hi\na\n");
}

#[test]
fn test_e_command_no_arg_pattern_space_becomes_command() {
    // With no argument, the pattern space itself is executed and replaced
    // by the command's output.
    new_ucmd!()
        .arg("e")
        .pipe_in("echo hi\n")
        .succeeds()
        .stdout_is("hi\n");
}

#[test]
fn test_e_command_with_arg_does_not_strip_trailing_newline() {
    // Unlike the no-argument form, e-with-argument writes the child's
    // stdout unmodified (echo's own newline is preserved).
    new_ucmd!()
        .arg("e echo hi")
        .pipe_in("a\n")
        .succeeds()
        .stdout_is("hi\na\n");
}

#[test]
fn test_e_command_with_address() {
    new_ucmd!()
        .arg("1e echo address")
        .pipe_in("a\nb\n")
        .succeeds()
        .stdout_is("address\na\nb\n");
}

#[test]
fn test_e_command_dangling_backslash_falls_back_to_no_arg() {
    // A leading backslash with nothing left to continue into at the end
    // of the script is treated the same as no argument at all.
    new_ucmd!()
        .arg(r"e\")
        .pipe_in("echo hi\n")
        .succeeds()
        .stdout_is("hi\n");
}

#[test]
fn test_e_command_rejected_with_posix() {
    new_ucmd!()
        .args(&["--posix", "e echo hi"])
        .fails()
        .stderr_contains("not allowed with --posix or --sandbox");
}

#[test]
fn test_e_command_rejected_with_sandbox() {
    new_ucmd!()
        .args(&["--sandbox", "e echo hi"])
        .fails()
        .stderr_contains("not allowed with --posix or --sandbox");
}

#[test]
fn test_e_command_with_arg_command_failure() {
    // A failing command's own error goes to stderr. sed itself succeeds.
    new_ucmd!()
        .arg("e nonexistent_command")
        .pipe_in("a\n")
        .succeeds()
        .stdout_is("a\n")
        .stderr_contains("nonexistent_command");
}

#[test]
fn test_e_command_no_arg_command_failure() {
    new_ucmd!()
        .arg("e")
        .pipe_in("nonexistent_command\n")
        .succeeds()
        .stdout_is("\n")
        .stderr_contains("nonexistent_command");
}

////////////////////////////////////////////////////////////
// Transliteration: y
check_output!(trans_simple, ["-e", r"y/0123456789/9876543210/", LINES1]);
check_output!(
    trans_delimiter,
    ["-e", r"y10\123456789198765432\101", LINES1]
);
check_output!(trans_no_new_line, ["-e", r"y/l/L/", NO_NEW_LINE]);

/// Transliterate non-UTF-8 input bytes using byte escapes in byte mode.
#[test]
fn trans_byte_escape_matches_invalid_input_in_c_locale() {
    new_ucmd!()
        .env("LC_ALL", "C")
        .args(&["-e", r"y/\xE9/Z/"])
        .pipe_in(b"\xE9\n".to_vec())
        .succeeds()
        .stdout_is_bytes(b"Z\n");
}

/// Transliterate UTF-8 characters as characters when the locale selects UTF-8 mode.
#[test]
fn trans_utf8_character_in_c_utf8_locale() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-e", r"y/κ/K/"])
        .pipe_in("κa\n")
        .succeeds()
        .stdout_is_bytes(b"Ka\n");
}

/// Transliterate a decoded hex escape as UTF-8 in UTF-8 mode.
#[test]
fn trans_utf8_escape_in_c_utf8_locale() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-e", r"y/\xE9/Z/"])
        .pipe_in("é\n")
        .succeeds()
        .stdout_is_bytes(b"Z\n");
}

/// Reject raw invalid UTF-8 transliteration script bytes in UTF-8 mode.
#[test]
fn trans_raw_invalid_script_byte_rejected_in_c_utf8_locale() {
    let mut script = NamedTempFile::new().expect("create temporary sed script");
    script
        .write_all(b"y/\xE9/Z/")
        .expect("write temporary sed script");
    let script_path = script.path().to_str().expect("temporary path is UTF-8");

    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-f", script_path])
        .fails()
        .stderr_contains("invalid UTF-8 in transliteration string");
}

#[test]
fn pattern_clear_with_z_command() {
    new_ucmd!()
        .arg("z")
        .pipe_in("a\nb\n")
        .succeeds()
        .stdout_is("\n\n");
}

#[test]
fn pattern_clear_with_z_command_silent_print() {
    new_ucmd!()
        .args(&["-n", "z;p"])
        .pipe_in("a\nb\n")
        .succeeds()
        .stdout_is("\n\n");
}

#[test]
fn pattern_clear_with_z_preserves_substitution_flag() {
    new_ucmd!()
        .args(&["-n", "s/a/b/;z;t changed;b;:changed;c\\\nchanged"])
        .pipe_in("a\n")
        .succeeds()
        .stdout_is("changed\n");
}

#[test]
fn pattern_clear_with_z_is_non_posix() {
    new_ucmd!()
        .args(&["--posix", "z"])
        .fails()
        .code_is(1)
        .stderr_is("sed: <script argument 1>:1:1: error: invalid command code `z'\n");
}
check_output!(trans_newline, ["-e", r"1N;2y/\n/X/", LINES1]);

////////////////////////////////////////////////////////////
// Pattern space manipulation: D, d, H, h, N, n, P, p, q, x
check_output!(pattern_print_to_newline, ["-n", r"1{;N;P;P;p;}", LINES1]);
check_output!(pattern_next_print, ["-n", r"N;N;P", LINES1]);
check_output!(pattern_delete_to_newline, ["-n", r"2N;3p;3D;3p", LINES1]);
check_output!(pattern_delete_no_newline, ["-e", r"2D", LINES1]);
check_output!(pattern_delete_print, ["-n", r"4d;p", LINES1]);

// FreeBSD sed does not produce any output for the following two
check_output!(pattern_append_delete, ["-e", r"N;N;N;D", LINES1]);
check_output!(pattern_append_delete_2, ["-e", r"N;N;N;D", LINES1, LINES2]);

check_output!(
    pattern_append_delete2_separate,
    ["-s", r"N;N;N;D", LINES1, LINES2]
);
check_output!(
    pattern_hold_append_swap,
    ["-e", r"2h;3H;4g;5G;6x;6p;6x;6p", LINES1]
);
check_output!(
    pattern_swap_separate,
    ["--separate", r"4x;6x", LINES1, LINES2]
);
check_output!(pattern_next_output, ["-e", r"4n", LINES1]);
check_output!(pattern_next_no_output, ["-n", "-e", r"4n", LINES1]);
check_output!(pattern_next_print_output, ["-e", r"4n;p", LINES1]);
check_output!(pattern_next_print_no_output, ["-n", "-e", r"4n;p", LINES1]);
check_output!(pattern_quit, [r"5q", LINES1]);
check_output!(pattern_quit_2, [r"5q", LINES1, LINES2]);
check_output!(pattern_re_reuse, ["-n", r"/_1/p;//p", LINES1]);
check_output!(pattern_subst_re_reuse, ["-n", r"/_1/p;s//-N/p", LINES1]);

#[test]
fn test_quit_exit_code() {
    new_ucmd!()
        .args(&["5q 42", LINES1])
        .fails()
        .code_is(42)
        .stdout_is_fixture("output/pattern_quit");
}

#[test]
fn test_quit_now_exit_code() {
    new_ucmd!()
        .args(&["6Q 12", LINES1])
        .fails()
        .code_is(12)
        .stdout_is_fixture("output/pattern_quit");
}

// Test for delete command preventing automatic pattern printing
#[test]
fn test_delete_command_prevents_automatic_printing() {
    // Test 'd' command - delete line 2
    new_ucmd!()
        .args(&["2d"])
        .pipe_in("line1\nline2\nline3")
        .succeeds()
        .stdout_is("line1\nline3");
}

#[test]
fn test_delete_range_prevents_automatic_printing() {
    // Test 'd' command on range - delete lines 2-3
    new_ucmd!()
        .args(&["2,3d"])
        .pipe_in("line1\nline2\nline3\nline4")
        .succeeds()
        .stdout_is("line1\nline4");
}

#[test]
fn test_change_command_prevents_automatic_printing() {
    // Test 'c' command - change line 2
    new_ucmd!()
        .args(&["2c\\replaced"])
        .pipe_in("line1\nline2\nline3")
        .succeeds()
        .stdout_is("line1\nreplaced\nline3");
}

#[test]
fn test_uppercase_delete_prevents_automatic_printing() {
    // Test 'D' command - delete up to newline and restart
    new_ucmd!()
        .args(&["-e", "N", "-e", "D"])
        .pipe_in("line1\nline2\nline3")
        .succeeds()
        .stdout_is("line3\n");
}

////////////////////////////////////////////////////////////
// Command blocks: {}
check_output!(
    block_simple_range,
    [
        "-e",
        r#"
4,12 {
	s/^/^/
	s/$/$/
	s/_/T/
}"#,
        LINES1
    ]
);

check_output!(
    block_negative_range,
    [
        "-e",
        r#"
4,12 !{
	s/^/^/
	s/$/$/
	s/_/T/
}"#,
        LINES1
    ]
);

check_output!(
    block_negative_range_2,
    [
        "-e",
        r#"
4,12 !{
	s/^/^/
	s/$/$/
	s/_/T/
}"#,
        LINES1,
        LINES2
    ]
);

check_output!(
    block_nested_selection,
    [
        "-e",
        r#"
4,12 {
	s/^/^/
	/6/,/10/ {
		s/$/$/
		/8/ s/_/T/
	}
}"#,
        LINES1
    ]
);

check_output!(
    block_nested_negative_selection,
    [
        "-e",
        r#"
4,12 !{
	s/^/^/
	/6/,/10/ !{
		s/$/$/
		/8/ !s/_/T/
	}
}"#,
        LINES1
    ]
);

check_output!(
    branch_plain,
    [
        "-n",
        "-e",
        r#"
b label4
:label3
s/^/label3_/p
b end
:label4
2,12b label1
b label2
:label1
s/^/label1_/p
b
:label2
s/^/label2_/p
b label3
:end
"#,
        LINES1
    ]
);

////////////////////////////////////////////////////////////
// Branching: :, b, t
check_output!(
    branch_conditional_simple,
    [
        "-n",
        "-e",
        r#"
s/l1_/l2_/
t ok
b
:ok
s/^/tested /p
"#,
        LINES1,
        LINES2
    ]
);

// SunOS and GNU sed behave as follows: lines 9-$ aren"#,t printed at all
check_output!(
    branch_to_block,
    [
        "-n",
        "-e",
        r#"
5,8b inside
1,5 {
	s/^/^/p
	:inside
	s/$/$/p
}
"#,
        LINES1
    ]
);

// Check that t clears the substitution done flag
check_output!(
    branch_test_clears,
    [
        "-n",
        "-e",
        r#"
1,8s/^/^/
t l1
:l1
t l2
s/$/$/p
b
:l2
s/^/ERROR/
"#,
        LINES1
    ]
);

// Check that reading a line clears the substitution done flag
check_output!(
    branch_cycle_clears,
    [
        "-n",
        "-e",
        r#"
t l2
1,8s/^/^/p
2,7N
b
:l2
s/^/ERROR/p
"#,
        LINES1
    ]
);

check_output!(
    branch_conditional_boundary,
    [
        "-e",
        r#"
{
:b
}
s/l/m/
tb"#,
        LINES1
    ]
);

// T branches when NO substitution was made (inverse of t).
// Lines not ending in "2" leave s/2$/X/ with no match, so T branches to
// :skip and keeps the original line; lines ending in "2" fall through and
// get the "SUB " mark.
check_output!(
    branch_no_sub_simple,
    [
        "-n",
        "-e",
        r#"
s/2$/X/
Tskip
s/^/SUB /
:skip
p
"#,
        LINES1
    ]
);

// Check that T also clears the substitution-done flag, matching t.
// After s/// sets the flag, the first T sees a substitution so it does
// not branch but still clears the flag; the second T then sees no
// substitution and branches to :y. Reaching :y (CLEARED) rather than
// falling through to :x (NOTCLEARED) proves the first T cleared the flag.
check_output!(
    branch_no_sub_clears,
    [
        "-n",
        "-e",
        r#"
s/^/^/
Tx
Ty
:x
s/^/NOTCLEARED /p
b
:y
s/^/CLEARED /p
"#,
        LINES1
    ]
);

#[test]
fn test_branch_no_sub_non_posix() {
    new_ucmd!()
        .args(&["--posix", "T"])
        .fails()
        .code_is(1)
        .stderr_contains("invalid command code");
}

////////////////////////////////////////////////////////////
// Text: a, c, i

// Check both POSIX and GNU parsing routines.

check_output!(
    text_simple_insert,
    [
        "-e",
        r#"
2,4i\
extra
"#,
        LINES1
    ]
);

check_output!(
    text_simple_append,
    [
        "-e",
        r#"
2,4a\
extra
"#,
        LINES1
    ]
);

check_output_posix!(
    text_insert_quit,
    [
        "-e",
        r#"
5i\
hello
5q
"#,
        LINES1
    ]
);

check_output_posix!(
    text_insert_between_subst,
    [
        "-n",
        "-e",
        r#"
s/^/before_i/p
20i\
inserted
s/^/after_i/p
"#,
        LINES1,
        LINES2
    ]
);

check_output_posix!(
    text_append_between_subst,
    [
        "-n",
        "-e",
        r#"
5,12s/^/5-12/
s/^/before_a/p
/5-12/a\
appended
s/^/after_a/p
"#,
        LINES1,
        LINES2
    ]
);

check_output_posix!(
    text_append_before_next,
    [
        "-n",
        "-e",
        r#"
s/^/^/p
/l1_/a\
appended
8,10N
s/$/$/p
"#,
        LINES1,
        LINES2
    ]
);

check_output_posix!(
    text_change_global,
    [
        "-n",
        "-e",
        r#"
c\
hello
"#,
        LINES1
    ]
);

check_output_posix!(
    text_change_line,
    [
        "-e",
        r#"
8c\
hello
"#,
        LINES1
    ]
);

check_output_posix!(
    text_change_range,
    [
        "-e",
        r#"
3,14c\
hello
"#,
        LINES1
    ]
);

check_output_posix!(
    text_change_reverse_range,
    [
        "-e",
        r#"
8,3c\
hello
"#,
        LINES1
    ]
);

check_output!(text_delete, ["d", LINES1]);

// Check that the pattern space is deleted.
check_output_posix!(
    text_change_print,
    [
        "-n",
        "-e",
        r#"
c\
changed
p
"#,
        LINES1
    ]
);

// GNU syntax extensions:
// Text can follow the initial \.
// Character escapes are supported.
// Invalid escapes result in the escaped character.
check_output!(
    text_insert_gnu,
    ["-e", "i\\>\\h\\elll\x08o\\nto\\\nall\\a", LINES1]
);

////////////////////////////////////////////////////////////
// r, w, W commands
check_output!(read_ok, [format!("4r {LINES2}"), LINES1.to_string()]);
check_output!(read_missing, ["5r /xyzzyxyzy42", LINES1]);
check_output!(read_empty, ["6r input/empty", LINES1]);
check_output!(
    cmd_read_zero_addr,
    [format!("0r {LINES2}"), LINES1.to_string()]
);
check_output!(
    cmd_read_one_addr,
    [format!("1r {LINES2}"), LINES1.to_string()]
);

#[test]
fn sandbox_rejects_read_command() {
    new_ucmd!()
        .args(&["--sandbox", &format!("1r {LINES2}"), LINES1])
        .fails()
        .stderr_contains("command not allowed with --sandbox");
}

#[test]
fn sandbox_rejects_subst_write_flag() -> std::io::Result<()> {
    let temp = NamedTempFile::new()?;
    let cmd = format!("s/l1/x/w {}", temp.path().display());

    new_ucmd!()
        .args(&["--sandbox", &cmd, LINES1])
        .fails()
        .stderr_contains("command not allowed with --sandbox");

    let mut actual = String::new();
    temp.reopen()?.read_to_string(&mut actual)?;
    assert!(actual.is_empty());

    Ok(())
}

#[test]
fn sandbox_rejects_write_command() -> std::io::Result<()> {
    let temp = NamedTempFile::new()?;
    let cmd = format!("w {}", temp.path().display());

    new_ucmd!()
        .args(&["--sandbox", &cmd, LINES1])
        .fails()
        .stderr_contains("command not allowed with --sandbox");

    let mut actual = String::new();
    temp.reopen()?.read_to_string(&mut actual)?;
    assert!(actual.is_empty());

    Ok(())
}

#[test]
fn sandbox_rejects_first_line_write_command() -> std::io::Result<()> {
    let temp = NamedTempFile::new()?;
    let cmd = format!("W {}", temp.path().display());

    new_ucmd!()
        .args(&["--sandbox", &cmd, LINES1])
        .fails()
        .stderr_contains("command not allowed with --sandbox");

    let mut actual = String::new();
    temp.reopen()?.read_to_string(&mut actual)?;
    assert!(actual.is_empty());

    Ok(())
}

#[test]
fn write_single_file() -> std::io::Result<()> {
    let temp = NamedTempFile::new()?;
    let cmd = format!("3,12w {}", temp.path().display());

    new_ucmd!().args(&["-e", &cmd, LINES1]).succeeds();

    let mut actual = String::new();
    temp.reopen()?.read_to_string(&mut actual)?;

    let expected = fs::read_to_string("tests/fixtures/sed/output/write_single_file")?;
    assert_eq!(actual, expected, "Output did not match fixture");

    Ok(())
}

#[test]
fn write_single_file_no_newline() -> std::io::Result<()> {
    let temp = NamedTempFile::new()?;
    let cmd = format!("w {}", temp.path().display());

    new_ucmd!()
        .args(&[cmd.as_str(), "input/no-new-line.txt"])
        .succeeds();

    let mut actual = String::new();
    temp.reopen()?.read_to_string(&mut actual)?;

    assert_eq!(actual, "Hello", "Output did not match expected");

    Ok(())
}

#[test]
fn write_two_files() -> std::io::Result<()> {
    let temp1 = NamedTempFile::new()?;
    let temp2 = NamedTempFile::new()?;
    let cmd = format!(
        "3,12w {}\n1,2w {}",
        temp1.path().display(),
        temp2.path().display()
    );

    new_ucmd!().args(&["-e", &cmd, LINES1]).succeeds();

    let mut actual = String::new();

    temp1.reopen()?.read_to_string(&mut actual)?;
    let expected = fs::read_to_string("tests/fixtures/sed/output/write_two_files_1")?;
    assert_eq!(actual, expected, "Output 1 did not match fixture");

    actual.clear();

    temp2.reopen()?.read_to_string(&mut actual)?;
    let expected = fs::read_to_string("tests/fixtures/sed/output/write_two_files_2")?;
    assert_eq!(actual, expected, "Output 2 did not match fixture");

    Ok(())
}

#[test]
fn write_first_line_newline() -> std::io::Result<()> {
    let temp = NamedTempFile::new()?;
    let cmd = format!("N;W {}", temp.path().display());

    new_ucmd!()
        .args(&["-n", "-e", &cmd])
        .pipe_in("abc\ndef\n")
        .succeeds();

    let mut actual = String::new();
    temp.reopen()?.read_to_string(&mut actual)?;
    assert_eq!(actual, "abc\n");

    Ok(())
}

#[test]
fn write_first_line_no_newline() -> std::io::Result<()> {
    let temp = NamedTempFile::new()?;
    let cmd = format!("W {}", temp.path().display());

    new_ucmd!()
        .args(&["-n", "-e", &cmd])
        .pipe_in("abc")
        .succeeds();

    let mut actual = String::new();
    temp.reopen()?.read_to_string(&mut actual)?;
    assert_eq!(actual, "abc");

    Ok(())
}

#[test]
fn write_first_line_with_w_command_is_non_posix() {
    new_ucmd!()
        .args(&["--posix", "W /tmp/out"])
        .fails()
        .code_is(1)
        .stderr_is("sed: <script argument 1>:1:1: error: invalid command code `W'\n");
}

////////////////////////////////////////////////////////////
// =, l, F commands
check_output!(number_continuous, ["/l2_/=", LINES1, LINES2]);
check_output!(number_separate, ["-s", "/l._8/=", LINES1, LINES2]);
check_output!(number_range, ["-e", "10,12=", LINES1]);
check_output!(number_range_out_of_bounds, ["-e", "47,60=", LINES1]);

check_output!(list_ascii, ["-n", "l 60", "input/ascii"]);
check_output!(list_empty, ["-n", "l 60", "input/empty"]);

check_output!(filename_file, ["-n", r"F", LINES1]);
// Non-ASCII filename
check_output!(filename_αρχείο1, [r"F", "input/αρχείο1"]);

#[test]
fn filename_stdin() {
    new_ucmd!()
        .args(&["-n", "F"])
        .pipe_in("a\nb\n")
        .succeeds()
        .stdout_is("-\n-\n");
}

#[test]
fn filename_non_posix() {
    new_ucmd!()
        .args(&["--posix", "F"])
        .fails()
        .code_is(1)
        .stderr_contains("invalid command code");
}

/// List Unicode input under an explicit UTF-8 locale
/// with uutil extensions enabled.
#[test]
fn list_unicode() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["--uutil-extensions", "l 60", "input/unicode"])
        .succeeds()
        .stdout_is_fixture_bytes("output/list_unicode");
}

// List Unicode input without uutil extensions should generate octal bytes.
check_output!(list_unicode_octal, ["l 60", "input/unicode"]);

/// List Unicode input without uutil extensions should generate octal bytes,
/// even under an explicit UTF-8 locale.
#[test]
fn list_unicode_octal_env() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["l 60", "input/unicode"])
        .succeeds()
        .stdout_is_fixture_bytes("output/list_unicode_octal");
}

/// List invalid UTF-8 bytes without decoding in byte mode.
#[test]
fn list_invalid_utf8_byte_locale() {
    new_ucmd!()
        .env("LC_ALL", "C")
        .args(&["-n", "l"])
        .pipe_in(b"\xE9\n".to_vec())
        .succeeds()
        .stdout_is_bytes(b"\\351$\n");
}

////////////////////////////////////////////////////////////
// In-place editing
#[test]
fn in_place_edit_replace() -> std::io::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("input");

    std::fs::write(&path, "hello, world\n")?;

    new_ucmd!()
        .args(&["-i", "-e", "s/world/universe/", path.to_str().unwrap()])
        .succeeds();

    let actual = std::fs::read_to_string(&path)?;

    assert_eq!(actual, "hello, universe\n");
    Ok(())
}

#[test]
fn in_place_edit_gnu_syntax() -> std::io::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("input");

    std::fs::write(&path, "hello, world\n")?;

    new_ucmd!()
        .args(&["-i", "s/world/universe/", path.to_str().unwrap()])
        .succeeds();

    let actual = std::fs::read_to_string(&path)?;
    assert_eq!(actual, "hello, universe\n");
    Ok(())
}

#[test]
fn in_place_edit_backup() -> std::io::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("input");

    std::fs::write(&path, b"hello, world\n")?;

    new_ucmd!()
        .args(&["-i.bak", "s/world/universe/", path.to_str().unwrap()])
        .succeeds();

    // Read edited file
    let actual = std::fs::read_to_string(&path)?;
    assert_eq!(actual, "hello, universe\n");

    // Read backup file
    let backup_path = path.with_file_name(format!(
        "{}.bak",
        path.file_name().unwrap().to_string_lossy()
    ));
    let backup = std::fs::read_to_string(&backup_path)?;
    assert_eq!(backup, "hello, world\n");

    Ok(())
}

#[test]
fn test_crlf_anchor_and_preservation() {
    new_ucmd!()
        .args(&["-e", "s/a$/X/"])
        .pipe_in(b"a\r\n".to_vec())
        .succeeds()
        .stdout_is_bytes(b"X\r\n");
}

#[test]
fn test_crlf_in_place_preservation() -> std::io::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("crlf.txt");

    std::fs::write(&path, b"line1\r\nline2\r\n")?;

    new_ucmd!()
        .args(&["-i", "s/line1/LINE1/", path.to_str().unwrap()])
        .succeeds();

    let actual = std::fs::read(&path)?;
    assert_eq!(actual, b"LINE1\r\nline2\r\n");
    Ok(())
}

// A script that names a carriage return sees the real line, so the classic dos2unix
// one-liners work; other scripts keep matching without the CR and writing it back.
#[test]
fn test_crlf_explicit_cr_in_script_is_matched_as_data() {
    for script in ["s/\\r$//", "s/\\x0D$//", "s/\\x0d$//"] {
        new_ucmd!()
            .args(&["-e", script])
            .pipe_in(b"one\r\ntwo\r\n".to_vec())
            .succeeds()
            .stdout_is_bytes(b"one\ntwo\n");
    }
    new_ucmd!()
        .args(&["-e", "y/\\r/Z/"])
        .pipe_in(b"a\r\n".to_vec())
        .succeeds()
        .stdout_is_bytes(b"aZ\n");
    // `s/.$//` names no CR: it still removes the last visible character.
    new_ucmd!()
        .args(&["-e", "s/.$//"])
        .pipe_in(b"one\r\n".to_vec())
        .succeeds()
        .stdout_is_bytes(b"on\r\n");
    // An escaped backslash followed by `r` is not a carriage return.
    new_ucmd!()
        .args(&["-e", "s/\\\\r/X/", "-e", "s/o$/0/"])
        .pipe_in(b"two\r\n".to_vec())
        .succeeds()
        .stdout_is_bytes(b"tw0\r\n");
}

// `-b`/`--binary` and `CASH_EOL=lf` make CR ordinary data for the whole run, as with sed
// on Linux: `$` no longer matches before it, and `.$` removes it.
#[test]
fn test_crlf_binary_mode_treats_cr_as_data() {
    for flag in ["-b", "--binary"] {
        new_ucmd!()
            .args(&[flag, "-e", "s/.$//"])
            .pipe_in(b"one\r\n".to_vec())
            .succeeds()
            .stdout_is_bytes(b"one\n");
        new_ucmd!()
            .args(&[flag, "-e", "s/e$/E/"])
            .pipe_in(b"one\r\n".to_vec())
            .succeeds()
            .stdout_is_bytes(b"one\r\n");
    }
    for value in ["lf", "LF"] {
        new_ucmd!()
            .env("CASH_EOL", value)
            .args(&["-e", "s/e$/E/"])
            .pipe_in(b"one\r\n".to_vec())
            .succeeds()
            .stdout_is_bytes(b"one\r\n");
    }
    // Any other value keeps the default.
    new_ucmd!()
        .env("CASH_EOL", "crlf")
        .args(&["-e", "s/e$/E/"])
        .pipe_in(b"one\r\n".to_vec())
        .succeeds()
        .stdout_is_bytes(b"onE\r\n");
}

////////////////////////////////////////////////////////////
// Large complex scripts

// Math expression evaluation
check_output!(math1, ["-f", "script/math.sed", "input/expression1"]);

// Calculate π (scaled) to several decimal places
check_output!(pi, ["-f", "script/math.sed", "input/pi"]);

/// Solve the Towers of Hanoi puzzle under an explicit UTF-8 locale.
#[test]
fn hanoi() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-f", "script/hanoi.sed", "input/hanoi"])
        .succeeds()
        .stdout_is_fixture_bytes("output/hanoi");
}

////////////////////////////////////////////////////////////
// Long-running scripts
// Test with cargo test -- --ignored

// Check the output of Bach's prelude in C major from WTC book I.
// Run with cargo test test_bach_prelude_matches -- --ignored.
#[test]
#[ignore] // Slow; produces 5.8 MB of raw audio.
fn test_bach_prelude_matches() {
    let res = new_ucmd!()
        .args(&["-E", "-f", "script/bach.sed"])
        .pipe_in("\n")
        .succeeds();

    // Compare SHA-256 output against GNU sed output.
    let mut hasher = Sha256::new();
    hasher.update(res.stdout());
    let digest = hasher.finalize();

    let got = hex::encode(digest);
    assert_eq!(
        got,
        "c4e50d6791a60692745e958dc48d43a40bccea2ce5cea31b7125a40604cc3219"
    );
}

// Draw the Mandelbrot set.
#[ignore] // Slow; takes > 15" on an i7 CPU
#[test]
fn test_mandelbrod() {
    new_ucmd!()
        .args(&["-En", "-f", "script/mandelbrot.sed", "input/newline"])
        .succeeds()
        .stdout_is_fixture("output/mandelbrot");
}

////////////////////////////////////////////////////////////
// Error handling
#[test]
fn test_invalid_backreference() {
    new_ucmd!()
        .args(&["-n", "-e", r"s/./X/;s//\1/", LINES1])
        .fails()
        .code_is(2)
        .stderr_is("sed: <script argument 1>:1:8: error: invalid reference \\1 on command's RHS\n");
}

#[test]
fn test_duplicate_label() {
    new_ucmd!()
        .args(&[":foo;:foo"])
        .fails()
        .code_is(1)
        .stderr_is("sed: <script argument 1>:1:6: error: duplicate label `foo'\n");
}

#[test]
fn test_undefined_label() {
    new_ucmd!()
        .args(&["b foo"])
        .fails()
        .code_is(1)
        .stderr_is("sed: <script argument 1>:1:1: error: undefined label `foo'\n");
}

#[test]
fn test_incomplete_test_command_posix() {
    new_ucmd!()
        .args(&["--posix", "i\\"])
        .fails()
        .code_is(1)
        .stderr_is("sed: :0:3: error: incomplete command\n");
}

#[test]
fn test_empty_text_commands_fail() {
    for command in ["a", "c", "i"] {
        new_ucmd!()
            .args(&["-e", command])
            .fails()
            .code_is(1)
            .stderr_contains(format!("command `{command}' expects \\ followed by text"));
    }
}

#[test]
fn test_addr0_non_posix() {
    new_ucmd!()
        .args(&["--posix", "0,/foo/p"])
        .fails()
        .code_is(1)
        .stderr_is("sed: <script argument 1>:1:2: error: address 0 is invalid in POSIX mode\n");
}

#[test]
fn test_addr0_second_required() {
    new_ucmd!()
        .args(&["0p"])
        .fails()
        .code_is(1)
        .stderr_is("sed: <script argument 1>:1:2: error: address 0 can only be used with ~step, a second regular expression, or a read command\n");
}

#[test]
fn test_addr0_second_re_only() {
    new_ucmd!()
        .args(&["0,4p"])
        .fails()
        .code_is(1)
        .stderr_is("sed: <script argument 1>:1:4: error: address 0 can only be used with ~step, a second regular expression, or a read command\n");
}

#[test]
fn test_step_match_non_posix() {
    new_ucmd!()
        .args(&["--posix", "3~2p"])
        .fails()
        .code_is(1)
        .stderr_is("sed: <script argument 1>:1:3: error: ~step is invalid in POSIX mode\n");
}

#[test]
fn test_step_end_non_posix() {
    new_ucmd!()
        .args(&["--posix", "3,~2p"])
        .fails()
        .code_is(1)
        .stderr_is("sed: <script argument 1>:1:4: error: ~step is invalid in POSIX mode\n");
}

// The following test diverse ways in which regexes are matched.
// Search for 'regex\.' to find them in the code.
#[test]
fn test_fancy_regex_is_match_error() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-E", r"/(\.+)+\1b$/p", "input/dots-4k.txt"])
        .fails()
        .code_is(2)
        .stderr_is("sed: <script argument 1>:1:1: 'input/dots-4k.txt':1 error: Error executing regex: Max limit for backtracking count exceeded\n");
}

#[test]
fn test_fancy_regex_find_error() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-E", r"p;s/(\.+)+\1b$/X/", "input/dots-4k.txt"])
        .fails()
        .code_is(2)
        .stderr_is("sed: <script argument 1>:1:3: 'input/dots-4k.txt':1 error: Error executing regex: Max limit for backtracking count exceeded\n");
}

#[test]
fn test_fancy_regex_captures_error() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-E", r"p;s/(\.+)+\1b$/\1/", "input/dots-4k.txt"])
        .fails()
        .code_is(2)
        .stderr_is("sed: <script argument 1>:1:3: 'input/dots-4k.txt':1 error: Error executing regex: Max limit for backtracking count exceeded\n");
}

#[test]
fn test_fancy_regex_captures_iter_error() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-E", r"p;s/(\.+)+\1b$/\1/3", "input/dots-4k.txt"])
        .fails()
        .code_is(2)
        .stderr_is("sed: <script argument 1>:1:3: 'input/dots-4k.txt':1 error: error retrieving RE captures: Error executing regex: Max limit for backtracking count exceeded\n");
}

#[test]
fn test_write_file_failure() {
    new_ucmd!()
        .args(&["w /xyzzy/xyzy", LINES1])
        .fails()
        .code_is(2)
        .stderr_contains("sed: <script argument 1>:1:1: error: creating file '/xyzzy/xyzy':");
}

#[test]
fn test_missing_substitute_re() {
    new_ucmd!()
        .args(&["l;s//foo/", LINES1])
        .fails()
        .code_is(2)
        .stderr_is("sed: <script argument 1>:1:3: 'input/lines1':1 error: no previous regular expression\n");
}

#[test]
fn test_missing_address_re() {
    new_ucmd!()
        .args(&["l\np;//s/foo/bar/", LINES1])
        .fails()
        .code_is(2)
        .stderr_is("sed: <script argument 1>:2:3: 'input/lines1':1 error: no previous regular expression\n");
}

////////////////////////////////////////////////////////////
// issue #143: p with no trailing newline
#[test]
fn test_print_command_adds_newline() {
    new_ucmd!()
        .args(&["-e", "p"])
        .pipe_in("foo")
        .succeeds()
        .stdout_is("foo\nfoo");
}

#[test]
fn test_print_command_multiline_no_newline() {
    new_ucmd!()
        .args(&["-e", "p"])
        .pipe_in("a\nfoo")
        .succeeds()
        .stdout_is("a\na\nfoo\nfoo");
}

// -n p must not add a trailing newline when input has none
#[test]
fn test_print_command_silent_no_newline() {
    new_ucmd!()
        .args(&["-n", "p"])
        .pipe_in("foo")
        .succeeds()
        .stdout_is("foo");
}

// sanity: normal newline-terminated input is unaffected
#[test]
fn test_print_command_with_newline() {
    new_ucmd!()
        .args(&["-e", "p"])
        .pipe_in("foo\n")
        .succeeds()
        .stdout_is("foo\nfoo\n");
}

// issue #254: 2x missing newline
#[test]
fn test_exchange_command_adds_newline() {
    new_ucmd!()
        .args(&["2x"])
        .pipe_in("a\nb\nc\n")
        .succeeds()
        .stdout_is("a\n\nc\n");
}

// issue #306: 1x with no-newline input should output an empty line
#[test]
fn test_exchange_no_newline_outputs_empty_line() {
    new_ucmd!()
        .args(&["1x"])
        .pipe_in("abc")
        .succeeds()
        .stdout_is("\n");
}

// q with no newline input must not drop the trailing newline
#[test]
fn test_quit_no_newline() {
    new_ucmd!()
        .args(&["q"])
        .pipe_in("foo")
        .succeeds()
        .stdout_is("foo\n");
}

// P with single line no newline input
#[test]
fn test_print_first_line_no_newline() {
    new_ucmd!()
        .args(&["-n", "P"])
        .pipe_in("foo")
        .succeeds()
        .stdout_is("foo");
}

#[test]
fn test_expected_newer_version() {
    new_ucmd!()
        .args(&["v4.10"])
        .fails()
        .stderr_is("sed: <script argument 1>:1:6: error: expected newer version of sed\n");
}

#[test]
fn test_invalid_version() {
    new_ucmd!()
        .args(&["v4.a"])
        .fails()
        .stderr_is("sed: <script argument 1>:1:5: error: invalid version of sed\n");
}

#[test]
fn test_valid_version() {
    new_ucmd!().args(&["v4.9"]).succeeds();
}

#[test]
fn test_valid_only_major_version() {
    // v4.0.0
    new_ucmd!().args(&["v4"]).succeeds();
}

#[test]
fn test_invalid_only_major_version() {
    // v999.0.0
    new_ucmd!()
        .args(&["v999"])
        .fails()
        .stderr_is("sed: <script argument 1>:1:5: error: invalid version of sed\n");
}

#[test]
fn test_default_version() {
    // defaults to v4.9.0
    new_ucmd!().args(&["v"]).succeeds();
}

//--posix should reject GNU substitute flags i/I https://github.com/uutils/sed/issues/401
#[test]
fn test_posix_reject_flags() {
    new_ucmd!()
        .args(&["--posix", "s/a/b/i"])
        .fails()
        .code_is(1)
        .stderr_is("sed: <script argument 1>:1:7: error: unknown option to 's'\n");

    new_ucmd!()
        .args(&["--posix", "s/a/b/m"])
        .fails()
        .code_is(1)
        .stderr_is("sed: <script argument 1>:1:7: error: unknown option to 's'\n");
}

// `l` wraps at `-l N` (default 70) rather than the terminal width, and 0 never wraps, as
// in GNU sed. Found by the corpus (Pement's centring one-liner piped into `sed -n l`).
#[test]
fn test_l_wraps_at_the_length_option() {
    let long = "x".repeat(100) + "\n";
    // Each output line holds width - 1 characters and a trailing backslash.
    let wrapped = |width: usize| {
        let mut out = String::new();
        let mut rest = "x".repeat(100);
        while rest.len() > width - 1 {
            out.push_str(&rest[..width - 1]);
            out.push_str("\\\n");
            rest = rest[width - 1..].to_string();
        }
        out + &rest + "$\n"
    };
    new_ucmd!()
        .args(&["-n", "l"])
        .pipe_in(long.clone())
        .succeeds()
        .stdout_is(wrapped(70));
    new_ucmd!()
        .args(&["-l", "30", "-n", "l"])
        .pipe_in(long.clone())
        .succeeds()
        .stdout_is(wrapped(30));
    for args in [&["-n", "l 0"][..], &["-l", "0", "-n", "l"][..]] {
        new_ucmd!()
            .args(args)
            .pipe_in(long.clone())
            .succeeds()
            .stdout_is("x".repeat(100) + "$\n");
    }
}

/// Ranges ended by a line number, compared with GNU sed 4.9 (`REVIEW_REPORT.md` TXT-04,
/// TXT-07): the line after `addr1,+N` or `addr1,N` ended was refused without asking
/// whether it starts a new range, and `c` never printed its text at such a range's end.
#[test]
fn ranges_ended_by_a_line_number_match_gnu_sed() {
    let cases: [(&str, &[&str], &str, &str); 14] = [
        ("/x/,+1p", &["-n"], "x\nb\nx\nd\ne\n", "x\nb\nx\nd\n"),
        ("/x/,3p", &["-n"], "x\nb\nc\nx\ne\n", "x\nb\nc\nx\n"),
        ("/x/,+2d", &[], "x\na\nb\nx\nc\nd\ne\n", "e\n"),
        ("2,3c T", &[], "1\n2\n3\n4\n5\n6\n", "1\nT\n4\n5\n6\n"),
        ("/2/,+1c T", &[], "1\n2\n3\n4\n5\n6\n", "1\nT\n4\n5\n6\n"),
        ("2,4!c X", &[], "1\n2\n3\n4\n5\n", "X\n2\n3\n4\nX\n"),
        (
            "/[26]/,+1p",
            &["-n"],
            "1\n2\n3\n4\n5\n6\n7\n8\n",
            "2\n3\n6\n7\n",
        ),
        ("4,2p", &["-n"], "1\n2\n3\n4\n5\n6\n", "4\n"),
        ("/3/,1p", &["-n"], "1\n2\n3\n4\n5\n6\n", "3\n"),
        (
            "/[159]/,+0p",
            &["-n"],
            "1\n2\n5\n6\n9\n10\n",
            "1\n5\n9\n10\n",
        ),
        ("2,~4p", &["-n"], "1\n2\n3\n4\n5\n", "2\n3\n4\n"),
        ("/x/,/never/c T", &[], "a\nx\nb\n", "a\n"),
        ("/x/,$c T", &[], "a\nx\nb\n", "a\nT\n"),
        ("2c T", &[], "1\n2\n3\n", "1\nT\n3\n"),
    ];
    for (script, options, input, expected) in cases {
        new_ucmd!()
            .args(options)
            .arg(script)
            .pipe_in(input)
            .succeeds()
            .stdout_is(expected);
    }
}

/// Escaped anchors in a literal pattern, compared with GNU sed 4.9: the fast path for
/// literal patterns removed the escapes before deciding about anchors, so `\$` anchored
/// (`REVIEW_REPORT.md` TXT-10).
#[test]
fn escaped_anchors_in_a_literal_pattern_are_characters() {
    let cases = [
        ("s/a\\$/X/", "a$b\n", "Xb\n"),
        ("s/a\\$/X/", "ba\n", "ba\n"),
        ("s/\\^a/X/", "^ab\n", "Xb\n"),
        ("s/^a/X/", "ab\n", "Xb\n"),
        ("s/a$/X/", "ba\n", "bX\n"),
        ("s/a\\\\$/X/", "a\\\n", "X\n"),
        ("s/x^y/Z/", "x^y\n", "Z\n"),
    ];
    for (script, input, expected) in cases {
        new_ucmd!()
            .arg(script)
            .pipe_in(input)
            .succeeds()
            .stdout_is(expected);
    }
}
