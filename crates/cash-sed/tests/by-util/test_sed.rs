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
        .stderr_contains("Invalid content of \\{\\}");
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
        .stderr_is("sed: -e expression #1, char 14: unknown option to `s'\n");
}

#[test]
fn test_subst_e_flag_rejected_with_sandbox() {
    new_ucmd!()
        .args(&["--sandbox", "s/.*/echo hi/e"])
        .fails()
        .stderr_is("sed: -e expression #1, char 14: e/r/w commands disabled in sandbox mode\n");
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
        .stderr_is("sed: -e expression #1, char 1: unknown command: `e'\n");
}

#[test]
fn test_e_command_rejected_with_sandbox() {
    new_ucmd!()
        .args(&["--sandbox", "e echo hi"])
        .fails()
        .stderr_is("sed: -e expression #1, char 1: e/r/w commands disabled in sandbox mode\n");
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
        .stderr_is("sed: -e expression #1, char 1: unknown command: `z'\n");
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
        .stderr_contains("unknown command: ");
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
        .stderr_contains("e/r/w commands disabled in sandbox mode");
}

#[test]
fn sandbox_rejects_subst_write_flag() -> std::io::Result<()> {
    let temp = NamedTempFile::new()?;
    let cmd = format!("s/l1/x/w {}", temp.path().display());

    new_ucmd!()
        .args(&["--sandbox", &cmd, LINES1])
        .fails()
        .stderr_contains("e/r/w commands disabled in sandbox mode");

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
        .stderr_contains("e/r/w commands disabled in sandbox mode");

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
        .stderr_contains("e/r/w commands disabled in sandbox mode");

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
        .stderr_is("sed: -e expression #1, char 1: unknown command: `W'\n");
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
        .stderr_contains("unknown command: ");
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
/// A group the reused regex does not have is empty, as in GNU sed, which checks the
/// groups of an empty regex nowhere; it was a run-time error (TODO.md phase 15).
#[test]
fn test_invalid_backreference() {
    new_ucmd!()
        .args(&["-e", r"s/./X/;s//\1/"])
        .pipe_in("ab\ncd\n")
        .succeeds()
        .stdout_only("b\nd\n");
}

/// A label defined twice is no error, as in GNU sed, where a branch goes to the last
/// definition; it was an error, as in BSD sed (TODO.md phase 15).
#[test]
fn test_duplicate_label() {
    new_ucmd!()
        .args(&[":foo;:foo"])
        .pipe_in("a\n")
        .succeeds()
        .stdout_only("a\n");
    new_ucmd!()
        .args(&["-n", "s/^/0/;tfoo;:foo;s/^/1/p;:foo;s/^/2/p"])
        .pipe_in("a\n")
        .succeeds()
        .stdout_only("20a\n");
}

#[test]
fn test_undefined_label() {
    new_ucmd!()
        .args(&["b foo"])
        .fails()
        .code_is(4)
        .stderr_is("sed: can't find label for jump to `foo'\n");
}

#[test]
fn test_incomplete_test_command_posix() {
    new_ucmd!()
        .args(&["--posix", "i\\"])
        .fails()
        .code_is(1)
        .stderr_is("sed: -e expression #1, char 2: incomplete command\n");
}

#[test]
fn test_empty_text_commands_fail() {
    for command in ["a", "c", "i"] {
        new_ucmd!()
            .args(&["-e", command])
            .fails()
            .code_is(1)
            .stderr_contains("expected \\ after `a', `c' or `i'");
    }
}

#[test]
fn test_addr0_non_posix() {
    new_ucmd!()
        .args(&["--posix", "0,/foo/p"])
        .fails()
        .code_is(1)
        .stderr_is("sed: -e expression #1, char 8: invalid usage of line address 0\n");
}

#[test]
fn test_addr0_second_required() {
    new_ucmd!()
        .args(&["0p"])
        .fails()
        .code_is(1)
        .stderr_is("sed: -e expression #1, char 2: invalid usage of line address 0\n");
}

#[test]
fn test_addr0_second_re_only() {
    new_ucmd!()
        .args(&["0,4p"])
        .fails()
        .code_is(1)
        .stderr_is("sed: -e expression #1, char 4: invalid usage of line address 0\n");
}

#[test]
fn test_step_match_non_posix() {
    new_ucmd!()
        .args(&["--posix", "3~2p"])
        .fails()
        .code_is(1)
        .stderr_is("sed: -e expression #1, char 2: unknown command: `~'\n");
}

#[test]
fn test_step_end_non_posix() {
    new_ucmd!()
        .args(&["--posix", "3,~2p"])
        .fails()
        .code_is(1)
        .stderr_is("sed: -e expression #1, char 3: unexpected `,'\n");
}

// The following test diverse ways in which regexes are matched.
// Search for 'regex\.' to find them in the code. An error of the engine at run time,
// which GNU sed's has none of, is in the form of GNU sed's run-time errors, with their
// status 4; it was placed at the command and the input line, with status 2 (TODO.md
// phase 15).
#[test]
fn test_fancy_regex_is_match_error() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-E", r"/(\.+)+\1b$/p", "input/dots-4k.txt"])
        .fails()
        .code_is(4)
        .stderr_is("sed: Error executing regex: Max limit for backtracking count exceeded\n");
}

#[test]
fn test_fancy_regex_find_error() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-E", r"p;s/(\.+)+\1b$/X/", "input/dots-4k.txt"])
        .fails()
        .code_is(4)
        .stderr_is("sed: Error executing regex: Max limit for backtracking count exceeded\n");
}

#[test]
fn test_fancy_regex_captures_error() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-E", r"p;s/(\.+)+\1b$/\1/", "input/dots-4k.txt"])
        .fails()
        .code_is(4)
        .stderr_is("sed: Error executing regex: Max limit for backtracking count exceeded\n");
}

#[test]
fn test_fancy_regex_captures_iter_error() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-E", r"p;s/(\.+)+\1b$/\1/3", "input/dots-4k.txt"])
        .fails()
        .code_is(4)
        .stderr_is("sed: error retrieving RE captures: Error executing regex: Max limit for backtracking count exceeded\n");
}

#[test]
fn test_write_file_failure() {
    new_ucmd!()
        .args(&["w /xyzzy/xyzy", LINES1])
        .fails()
        .code_is(4)
        .stderr_is("sed: couldn't open file /xyzzy/xyzy: No such file or directory\n");
}

/// An empty regex with none before it is GNU sed's script error, placed where the
/// script's reading ended, with status 1; it was a run-time error of cash's own form
/// with status 2 (TODO.md phase 15).
#[test]
fn test_missing_substitute_re() {
    new_ucmd!()
        .args(&["l;s//foo/", LINES1])
        .fails()
        .code_is(1)
        .stderr_is("sed: -e expression #1, char 0: no previous regular expression\n");
}

#[test]
fn test_missing_address_re() {
    new_ucmd!()
        .args(&["l\np;//s/foo/bar/", LINES1])
        .fails()
        .code_is(1)
        .stderr_is("sed: -e expression #1, char 0: no previous regular expression\n");
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
        .stderr_is("sed: -e expression #1, char 5: expected newer version of sed\n");
}

/// GNU sed compares versions with `strverscmp`, so `4.a` is newer than 4.9; it was
/// "invalid version of sed" (TODO.md phase 15).
#[test]
fn test_invalid_version() {
    new_ucmd!()
        .args(&["v4.a"])
        .fails()
        .code_is(1)
        .stderr_is("sed: -e expression #1, char 4: expected newer version of sed\n");
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
        .code_is(1)
        .stderr_is("sed: -e expression #1, char 4: expected newer version of sed\n");
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
        .stderr_is("sed: -e expression #1, char 7: unknown option to `s'\n");

    new_ucmd!()
        .args(&["--posix", "s/a/b/m"])
        .fails()
        .code_is(1)
        .stderr_is("sed: -e expression #1, char 7: unknown option to `s'\n");
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

/// `-z` separates lines by NUL, compared with GNU sed 4.9: it was accepted and ignored,
/// so `sed -z 's/\n/,/g'`, GNU's way to edit across lines, changed nothing
/// (`REVIEW_REPORT.md` TXT-09).
#[test]
fn null_data_separates_lines_by_nul() {
    let cases: [(&[&str], &[u8], &[u8]); 6] = [
        (&["-z", "-n", "2p"], b"a\0b\0c\0", b"b\0"),
        (&["-z", "s/$/Y/"], b"a\0b\0", b"aY\0bY\0"),
        (
            &["-z", "s/\\n/,/g"],
            b"one\ntwo\nthree\n",
            b"one,two,three,",
        ),
        (&["-z", "N;s/\\x00/+/"], b"a\0b\0c\0", b"a+b\0c\0"),
        (&["-z", "$!d"], b"x\ny\0z\0", b"z\0"),
        (&["-z", "G"], b"p\0q\0", b"p\0\0q\0\0"),
    ];
    for (args, input, expected) in cases {
        new_ucmd!()
            .args(args)
            .pipe_in(input)
            .succeeds()
            .stdout_is_bytes(expected);
    }
}

/// GNU sed's word and buffer anchors in a regex: `\b` a word boundary (not a backspace),
/// `\<` and `\>` a word's start and end, `` \` `` and `\'` the pattern space's start and
/// end, whatever `M` says. The outputs are GNU sed 4.9's; `\b`, `\<`, `\>` and `` \` ``
/// left the line as it was.
#[test]
fn test_gnu_word_and_buffer_anchors() {
    let cases: [(&str, &str, &str); 7] = [
        (r"s/\bb\b/X/", "a b ab\n", "a X ab\n"),
        (r"s/\<b/X/g", "a b ab\n", "a X ab\n"),
        (r"s/b\>/X/g", "a b ab\n", "a X aX\n"),
        (r"s/\Bb/X/g", "a b ab\n", "a b aX\n"),
        (r"s/\(a\) \<b\>/\1-/", "a b ab\n", "a- ab\n"),
        (r"N;s/\`a/X/Mg", "ab\nab\n", "Xb\nab\n"),
        (r"N;s/b\'/X/Mg", "ab\nab\n", "ab\naX\n"),
    ];
    for (script, input, expected) in cases {
        new_ucmd!()
            .arg(script)
            .pipe_in(input)
            .succeeds()
            .stdout_is(expected);
    }
}

/// GNU sed ends a label at a blank, and reads what follows as the next command, where
/// cash's sed said "extra characters at the end of the : command".
#[test]
fn test_gnu_label_ended_by_a_blank() {
    new_ucmd!()
        .arg(r":x /\\$/ { N; s/\\\n//; bx }")
        .pipe_in("a\\\nb\nc\n")
        .succeeds()
        .stdout_is("ab\nc\n");
    new_ucmd!()
        .args(&["-n", ":x p"])
        .pipe_in("a\n")
        .succeeds()
        .stdout_is("a\n");
}

/// What follows a comma or a `~` that cannot start an address is a script error, as
/// GNU sed has it; sed panicked on it (TODO.md 14.6).
#[test]
fn test_no_address_after_a_comma_is_an_error() {
    for (script, col) in [("1,xp", 3), ("$,xp", 3), ("1,p", 3), ("1, p", 4)] {
        new_ucmd!()
            .args(&["-n", script])
            .fails()
            .code_is(1)
            .stderr_only(format!(
                "sed: -e expression #1, char {col}: unexpected `,'\n"
            ));
    }
}

/// A step or count left out after `~`, `,~` or `,+` is 0, as in GNU sed: `1~p` and
/// `2,~p` are the first line alone, as `1~0p` and `2,+0p` are. Each was "expected context
/// address" or "number expected", and `addr1,~0` never ended (TODO.md 14.6). The expected
/// output is GNU sed 4.9's.
#[test]
fn test_missing_step_or_count_is_zero() {
    for (script, expected) in [
        ("1~p", "1\n"),
        ("2~p", "2\n"),
        ("2~ p", "2\n"),
        ("1~0p", "1\n"),
        ("1,~p", "1\n"),
        ("2,~p", "2\n"),
        ("2,~0p", "2\n"),
        ("2,~ 3p", "2\n3\n"),
        ("1,+p", "1\n"),
        ("2,+p", "2\n"),
        ("2,+ 1p", "2\n3\n"),
        ("$,+p", "5\n"),
        ("/3/,~p", "3\n"),
        ("/3/,+p", "3\n"),
        ("2,~0!p", "1\n3\n4\n5\n"),
    ] {
        new_ucmd!()
            .args(&["-n", script])
            .pipe_in("1\n2\n3\n4\n5\n")
            .succeeds()
            .stdout_only(expected);
    }
    // `c` prints its text at the end of a range, which `,~0` makes the first line.
    new_ucmd!()
        .args(&["2,~0c\\\nT"])
        .pipe_in("1\n2\n3\n")
        .succeeds()
        .stdout_only("1\nT\n3\n");
    // `0~` with no step is line 0, which only `r` may use.
    for script in ["0~p", "0~0p", "0,~p"] {
        new_ucmd!()
            .args(&["-n", script])
            .fails()
            .code_is(1)
            .no_stdout();
    }
    // What follows a missing step is the command, as in GNU sed, and `1~` has none.
    for script in ["1~/x/p", "1~"] {
        new_ucmd!()
            .args(&["-n", script])
            .fails()
            .code_is(1)
            .no_stdout();
    }
}

/// In a replacement, GNU sed's `\U` and `\L` turn what follows to upper or lower case,
/// until `\E`, and `\u` and `\l` the next character only; sed read `\u` and `\U` as
/// Unicode escapes and the others as text (TODO.md 14.6). The expected output is GNU sed
/// 4.9's in a UTF-8 locale.
#[test]
fn test_replacement_case_conversions() {
    for (script, input, expected) in [
        (r"s/a/\uxyz/", "a", "Xyz"),
        (r"s/a/\Uxyz/", "a", "XYZ"),
        (r"s/a/\Ux\Eyz/", "a", "Xyz"),
        (r"s/a/\lXYZ/", "a", "xYZ"),
        (r"s/a/\LXYZ\Eabc/", "a", "xyzabc"),
        (r"s/\(a\)\(b\)/\U\1\E\2/", "abc ÀbC", "Abc ÀbC"),
        (r"s/.*/\U&-x\E-y/", "abc ÀbC", "ABC ÀBC-X-y"),
        (r"s/\(.*\)/\L\u\1/", "abc ÀbC", "Abc àbc"),
        (r"s/\(.*\)/\U\l\1/", "abc ÀbC", "aBC ÀBC"),
        // `\L` cancels a `\u` still waiting.
        (r"s/.*/\u\L&/", "abc ÀbC", "abc àbc"),
        (r"s/a/\u\Ex/", "abc", "xbc"),
        // A `\u` waits past an empty group for the next text.
        (r"s/\(x*\)a/\u\1b/", "abc", "Bbc"),
        (r"s/a/\U\ux/", "abc", "Xbc"),
        (r"s/\(b\)/\u&\1/", "abc", "aBbc"),
        (r"s/a/\l\UXY/", "abc", "XYbc"),
        // A character whose capital is two stays as it is.
        (r"s/a/\uß/", "abc", "ßbc"),
        (r"s/c/\Ué\E/", "abc", "abÉ"),
        (r"s/\w\+/\u&/g", "foo bar", "Foo Bar"),
        (r"s/\w\+/\U&/2", "foo bar", "foo BAR"),
        // Escapes that make characters are converted too.
        (r"s/.*/\U\x61b/", "z", "AB"),
        (r"s/.*/\u\d097/", "z", "A"),
        (r"s/a/\t|\x41|\d066|\o103|\cA/", "a", "\t|A|B|C|\u{1}"),
    ] {
        new_ucmd!()
            .arg(script)
            .env("LC_ALL", "en_US.UTF-8")
            .pipe_in(format!("{input}\n"))
            .succeeds()
            .stdout_only(format!("{expected}\n"));
    }
    // In the C locale only ASCII letters change, as with GNU sed there.
    new_ucmd!()
        .arg(r"s/.*/\U&/")
        .env("LC_ALL", "C")
        .pipe_in("aé\n")
        .succeeds()
        .stdout_only("Aé\n");
    // --posix leaves them out: an unknown escape is the character.
    new_ucmd!()
        .args(&["--posix", r"s/a/\uxy/"])
        .pipe_in("abc\n")
        .succeeds()
        .stdout_only("uxybc\n");
}

/// GNU sed has no Unicode escapes anywhere: in a regex, in `y` and in `a` text `\u`
/// and `\U` are the letters, and in a bracket expression the backslash and the letter;
/// sed read them as Unicode escapes (TODO.md 14.6). The expected output is GNU sed 4.9's.
#[test]
fn test_no_unicode_escapes() {
    // Spelled in two pieces, so that no escape reader on the way makes them characters.
    let u0061 = concat!(r"\", "u0061");
    for (script, input, expected) in [
        (format!("s/{u0061}/X/"), "abc", "abc"),
        (format!("s/{u0061}/X/"), "u0061", "X"),
        (r"s/\U/X/g".to_string(), r"a\uU", r"a\uX"),
        (r"s/[\u]/X/g".to_string(), r"a\uU", "aXXU"),
        (r"y/\u/X/".to_string(), "uU", "XU"),
    ] {
        new_ucmd!()
            .arg(&script)
            .pipe_in(format!("{input}\n"))
            .succeeds()
            .stdout_only(format!("{expected}\n"));
    }
    new_ucmd!()
        .arg(format!("a x{u0061}y"))
        .pipe_in("a\n")
        .succeeds()
        .stdout_only("a\nxu0061y\n");
}

/// GNU sed has no Unicode escapes: in a replacement `\u` and `\U` are case conversions,
/// so hex digits after them are text, whatever value they would spell; sed panicked on
/// a surrogate or a value past U+10FFFF (TODO.md 14.6). The expected output is GNU
/// sed 4.9's.
#[test]
fn test_escape_of_no_character_is_text() {
    for (script, expected) in [
        (r"s/a/\uD800/", "D800\n"),
        (r"s/a/\U00110000/", "00110000\n"),
        (r"s/a/\UFFFFFFFF/", "FFFFFFFF\n"),
        (r"s/a/\u12/", "12\n"),
        (r"s/a/\Udead\Ebeef/", "DEADbeef\n"),
    ] {
        new_ucmd!()
            .arg(script)
            .pipe_in("a\n")
            .succeeds()
            .stdout_is(expected);
    }
}

/// A backslash before a character with no escape of its own in a replacement stands for
/// the character, as in GNU sed; it was kept, so `s/a/\q/` gave `\q`, and `\U` under
/// --posix gave `U` with its `\E` left as `\E` (TODO.md 14.6).
#[test]
fn test_unknown_escape_in_a_replacement_is_the_character() {
    for (args, expected) in [
        (&["s/a/\\q/"][..], "q1\n"),
        (&["s/a/\\z\\%\\-/"][..], "z%-1\n"),
        (&["--posix", "s/a/\\Uxy\\E/"][..], "UxyE1\n"),
        (&["s/a/\\Uxy\\E/"][..], "XY1\n"),
    ] {
        new_ucmd!()
            .args(args)
            .pipe_in("a1\n")
            .succeeds()
            .stdout_only(expected);
    }
}

/// `first~step` is one address, as in GNU sed: it starts a range (`1~3,5p`, refused),
/// and is the end of one (`1,2~3p`); a `~` after any other address is the command, an
/// unknown one (`$~2p` was taken as a step) (TODO.md 14.6).
#[test]
fn test_step_address_is_one_address() {
    for (script, expected) in [
        ("1~3,5p", "1\n2\n3\n4\n5\n7\n"),
        ("1,2~3p", "1\n2\n"),
        ("0~3,4p", "3\n4\n6\n"),
        ("2~3p", "2\n5\n"),
        ("1~0p", "1\n"),
    ] {
        new_ucmd!()
            .args(&["-n", script])
            .pipe_in("1\n2\n3\n4\n5\n6\n7\n")
            .succeeds()
            .stdout_only(expected);
    }
    for (script, error) in [
        ("$~2p", "2: unknown command: `~'"),
        ("/a/~2p", "4: unknown command: `~'"),
        ("1~2~3p", "4: unknown command: `~'"),
        ("+1p", "2: invalid usage of +N or ~N as first address"),
        ("~1p", "2: invalid usage of +N or ~N as first address"),
    ] {
        new_ucmd!()
            .args(&["-n", script])
            .fails()
            .code_is(1)
            .stderr_is(format!("sed: -e expression #1, char {error}\n"));
    }
}

/// `q` takes one address, as in GNU sed, also outside POSIX mode: `1,+0q` and `1,~0q`
/// were taken; and the errors for addresses a command does not take are GNU's
/// (TODO.md 14.6).
#[test]
fn test_one_address_commands_refuse_two() {
    for (script, error) in [
        ("1,+0q", "5: command only uses one address"),
        ("1,~0q", "5: command only uses one address"),
        ("1,2q", "4: command only uses one address"),
        ("1,2Q", "4: command only uses one address"),
        ("1:a", "2: : doesn't want any addresses"),
        ("{p;1}", "5: `}' doesn't want any addresses"),
        ("1#x", "2: comments don't accept any addresses"),
    ] {
        new_ucmd!()
            .arg(script)
            .fails()
            .code_is(1)
            .stderr_is(format!("sed: -e expression #1, char {error}\n"));
    }
}

/// An unterminated `s` at the end of the script is reported where the script ended; the
/// location was `::0:8`, no script and line 0. And in GNU sed's words (TODO.md 14.6).
#[test]
fn test_an_error_at_the_end_of_the_script_has_its_location() {
    new_ucmd!()
        .arg("sua\\uxu")
        .fails()
        .code_is(1)
        .stderr_is("sed: -e expression #1, char 7: unterminated `s' command\n");
    new_ucmd!()
        .args(&["-e", "p", "-e", "s/a/b"])
        .fails()
        .code_is(1)
        .stderr_is("sed: -e expression #2, char 5: unterminated `s' command\n");
}

/// Errors cash's sed worded its own way are in GNU sed's words (TODO.md 14.6).
#[test]
fn test_errors_are_in_gnu_seds_words() {
    for (args, error) in [
        (&["k"][..], "1: unknown command: `k'"),
        (&["0p"][..], "2: invalid usage of line address 0"),
        (&["--posix", "1~2p"][..], "2: unknown command: `~'"),
        (&["--posix", "1,+2p"][..], "3: unexpected `,'"),
        (&["1!!p"][..], "3: multiple `!'s"),
        (&["p x"][..], "3: extra characters after command"),
        (
            &["y/ab/c/"][..],
            "7: strings for `y' command are different lengths",
        ),
        (&["y/a/"][..], "4: unterminated `y' command"),
        (&["s/a/b/q"][..], "7: unknown option to `s'"),
        (&["1"][..], "1: missing command"),
        (
            &["--sandbox", "r x"][..],
            "1: e/r/w commands disabled in sandbox mode",
        ),
    ] {
        new_ucmd!()
            .args(args)
            .fails()
            .code_is(1)
            .stderr_is(format!("sed: -e expression #1, char {error}\n"));
    }
}

/// An `s` or `y` that ends the script is unterminated, as in GNU sed; reading its
/// delimiter past the end of the line panicked (TODO.md 14.6, found looking for panics).
#[test]
fn test_s_or_y_at_the_end_of_the_script_is_unterminated() {
    for (script, error) in [
        ("s", "1: unterminated `s' command"),
        ("y", "1: unterminated `y' command"),
        ("p;s", "3: unterminated `s' command"),
    ] {
        new_ucmd!()
            .arg(script)
            .fails()
            .code_is(1)
            .stderr_is(format!("sed: -e expression #1, char {error}\n"));
    }
}

/// `v` does nothing at run time, as in GNU sed; it stopped the run as an internal error.
/// A version compares part by part: 4.8.1 and 3.99 were refused (TODO.md 14.6).
#[test]
fn test_v_runs_and_takes_older_versions() {
    for script in ["v", "v 4.2", "v 4.8.1", "v 3.99", "v 4.9"] {
        new_ucmd!()
            .arg(script)
            .pipe_in("a\n")
            .succeeds()
            .stdout_only("a\n");
    }
    new_ucmd!()
        .arg("v 4.10")
        .fails()
        .code_is(1)
        .stderr_contains("expected newer version of sed");
}

/// More of the errors cash's sed worded its own way, in GNU sed's words (TODO.md 14.6).
#[test]
fn test_more_errors_are_in_gnu_seds_words() {
    for (script, error) in [
        ("s/a", "3: unterminated `s' command"),
        ("\\%a%p;\\%a", "9: unterminated address regex"),
        ("s/a/b/gg", "8: multiple `g' options to `s' command"),
        ("s/a/b/2g3", "9: multiple number options to `s' command"),
        ("s/a/b/0", "7: number option to `s' command may not be zero"),
        ("c", "1: expected \\ after `a', `c' or `i'"),
    ] {
        new_ucmd!()
            .arg(script)
            .fails()
            .code_is(1)
            .stderr_is(format!("sed: -e expression #1, char {error}\n"));
    }
}

/// A directory with the files `R` reads in these tests: `rf` has three lines, the last
/// without a newline, `rf2` two.
fn r_files() -> std::io::Result<(tempfile::TempDir, String, String)> {
    let dir = tempfile::tempdir()?;
    let rf = dir.path().join("rf");
    let rf2 = dir.path().join("rf2");
    fs::write(&rf, "r1\nr2\nr3")?;
    fs::write(&rf2, "x1\nx2\n")?;
    let rf = rf.to_string_lossy().into_owned();
    let rf2 = rf2.to_string_lossy().into_owned();
    Ok((dir, rf, rf2))
}

/// `R` queues a line of its file for the end of the cycle, a further one each time it
/// runs, and nothing once the file is read; a last line without a newline is written
/// without one, as GNU sed does (TODO.md 14.6: `R` was not supported).
#[test]
fn test_r_upper_reads_a_line_at_a_time() -> std::io::Result<()> {
    let (_dir, rf, _) = r_files()?;
    new_ucmd!()
        .arg(format!("R {rf}"))
        .pipe_in("a\nb\nc\nd\ne\n")
        .succeeds()
        .stdout_only("a\nr1\nb\nr2\nc\nr3d\ne\n");
    // With -n, only the lines.
    new_ucmd!()
        .args(&["-n", &format!("R {rf}")])
        .pipe_in("a\nb\n")
        .succeeds()
        .stdout_only("r1\nr2\n");
    Ok(())
}

/// Every `R` naming a file reads on from the same place in it; and `R` takes two
/// addresses (TODO.md 14.6).
#[test]
fn test_r_upper_commands_share_their_file() -> std::io::Result<()> {
    let (_dir, rf, rf2) = r_files()?;
    new_ucmd!()
        .args(&["-e", &format!("R {rf}"), "-e", &format!("R {rf}")])
        .args(&["-e", &format!("R {rf2}")])
        .pipe_in("a\nb\n")
        .succeeds()
        .stdout_only("a\nr1\nr2\nx1\nb\nr3x2\n");
    new_ucmd!()
        .args(&["-e", &format!("1,2R {rf}"), "-e", &format!("/c/R {rf}")])
        .pipe_in("a\nb\nc\nd\n")
        .succeeds()
        .stdout_only("a\nr1\nb\nr2\nc\nr3d\n");
    Ok(())
}

/// A file `R` cannot open is no error: it reads nothing, as in GNU sed (TODO.md 14.6).
#[test]
fn test_r_upper_of_a_missing_file_reads_nothing() {
    new_ucmd!()
        .arg("R no such file")
        .pipe_in("a\nb\n")
        .succeeds()
        .stdout_only("a\nb\n");
}

/// Its lines go out with what `a` and `i` queue, in order, and before `N` reads its
/// line; `-s` starts the file over for each input file, as GNU sed's `R` does
/// (TODO.md 14.6).
#[test]
fn test_r_upper_in_the_cycle() -> std::io::Result<()> {
    let (dir, rf, rf2) = r_files()?;
    new_ucmd!()
        .arg(format!("R {rf}\na\\\nAPP\ni\\\nINS"))
        .pipe_in("a\nb\n")
        .succeeds()
        .stdout_only("INS\na\nr1\nAPP\nINS\nb\nr2\nAPP\n");
    new_ucmd!()
        .arg(format!("R {rf}\nN"))
        .pipe_in("a\nb\nc\n")
        .succeeds()
        .stdout_only("r1\na\nb\nc\nr2\n");
    let input = dir.path().join("input");
    fs::write(&input, "c\nd\n")?;
    new_ucmd!()
        .args(&["-s", &format!("R {rf2}"), "-", input.to_str().unwrap()])
        .pipe_in("a\nb\n")
        .succeeds()
        .stdout_only("a\nx1\nb\nx2\nc\nx1\nd\nx2\n");
    // Without -s the file reads on.
    new_ucmd!()
        .args(&[&format!("R {rf2}"), "-", input.to_str().unwrap()])
        .pipe_in("a\nb\n")
        .succeeds()
        .stdout_only("a\nx1\nb\nx2\nc\nd\n");
    // With -z, a line ends at a NUL.
    new_ucmd!()
        .args(&["-z", &format!("R {rf}")])
        .pipe_in("a\0b\0")
        .succeeds()
        .stdout_only("a\0r1\nr2\nr3b\0");
    Ok(())
}

/// `R` is GNU's: POSIX mode does not know it, and the sandbox refuses it with `r` and
/// `w` (TODO.md 14.6).
#[test]
fn test_r_upper_under_posix_and_sandbox() {
    for (args, error) in [
        (&["--posix", "R x"][..], "1: unknown command: `R'"),
        (
            &["--sandbox", "R x"][..],
            "1: e/r/w commands disabled in sandbox mode",
        ),
        (&["R"][..], "1: missing filename in r/R/w/W commands"),
    ] {
        new_ucmd!()
            .args(args)
            .fails()
            .code_is(1)
            .stderr_is(format!("sed: -e expression #1, char {error}\n"));
    }
}

/// A second `p` flag is GNU sed's error; it was taken, and printed twice (TODO.md
/// 14.6).
#[test]
fn test_s_takes_one_p_flag() {
    for (script, column) in [("s/a/b/pp", 8), ("s/a/b/pgp", 9), ("s/a/b/ ; s/c/d/pp", 17)] {
        new_ucmd!()
            .arg(script)
            .fails()
            .code_is(1)
            .stderr_is(format!(
                "sed: -e expression #1, char {column}: multiple `p' options to `s' command\n"
            ));
    }
}

/// A regular expression GNU sed refuses is refused in its words, at the column where
/// GNU sed has read the command that holds it, flags and all; the engine's words were
/// reported, and some of these taken (TODO.md 14.6).
#[test]
fn test_regex_errors_are_gnu_seds() {
    for (args, error) in [
        (&["s/\\(a/b/"][..], "8: Unmatched ( or \\("),
        (&["-E", "s/(a/b/"][..], "7: Unmatched ( or \\("),
        (&["s/a\\)/b/"][..], "8: Unmatched ) or \\)"),
        (&["-E", "s/a)/b/"][..], "7: Unmatched ) or \\)"),
        (&["s/a\\{1/b/"][..], "9: Unmatched \\{"),
        (&["-E", "s/a{1/b/"][..], "8: Unmatched \\{"),
        (&["s/a\\{x\\}/b/"][..], "11: Invalid content of \\{\\}"),
        (&["-E", "s/a{2,1}/b/"][..], "11: Invalid content of \\{\\}"),
        (
            &["-E", "s/a{32768}/b/"][..],
            "13: Regular expression too big",
        ),
        (
            &["-E", "s/*a/b/"][..],
            "7: Invalid preceding regular expression",
        ),
        (
            &["s/a*\\{2\\}/b/"][..],
            "12: Invalid preceding regular expression",
        ),
        (&["s/a**/b/"][..], "8: Invalid preceding regular expression"),
        (&["s/\\(a\\)\\2/b/"][..], "12: Invalid back reference"),
        (&["-E", "s/(a)\\2/b/"][..], "10: Invalid back reference"),
        (&["s/[b-a]/b/"][..], "10: Invalid range end"),
        (&["s/[[:foo:]]/b/"][..], "14: Invalid character class name"),
        (&["s/[[.foo.]]/b/"][..], "14: Invalid collation character"),
        (&["s/\\x5c/b/"][..], "9: Trailing backslash"),
        // After the flags, a `;` that ends them included, a `}` or `#` not.
        (&["s/\\(a/x/I"][..], "9: Unmatched ( or \\("),
        (&["s/\\(a/x/ ;p"][..], "10: Unmatched ( or \\("),
        (&["{s/\\(a/x/}"][..], "9: Unmatched ( or \\("),
        (&["s/\\(a/x/#c"][..], "8: Unmatched ( or \\("),
        (&["s/\\(a/x/w out"][..], "13: Unmatched ( or \\("),
        (&["p;s/\\(a/b/"][..], "10: Unmatched ( or \\("),
        // An address's, after its flags and the blanks before the command.
        (&["/\\(a/p"][..], "5: Unmatched ( or \\("),
        (&["/\\(a/I p"][..], "7: Unmatched ( or \\("),
        (&["1,/\\(a/p"][..], "7: Unmatched ( or \\("),
        // A bracket the line does not end leaves the command unterminated.
        (&["s/[a/b/"][..], "7: unterminated `s' command"),
        (&["s/[[:alpha/x/"][..], "13: unterminated `s' command"),
        (&["/[a/p"][..], "5: unterminated address regex"),
    ] {
        new_ucmd!()
            .args(args)
            .fails()
            .code_is(1)
            .stderr_is(format!("sed: -e expression #1, char {error}\n"));
    }
}

/// Where GNU sed's BRE has a character, not an operator, so does cash's: `*`, `\+` and
/// `\?` where an expression starts, `^` and `$` but where they anchor. A leading `*`
/// was refused, and `\(^a\)` matched a `^` (TODO.md 14.6).
#[test]
fn test_bre_characters_where_gnu_sed_has_them() {
    for (script, input, output) in [
        ("s/*a/x/", "*a\n", "x\n"),
        ("s/*/x/", "*\n", "x\n"),
        ("s/\\(*a\\)/x/", "*a\n", "x\n"),
        ("s/^*a/x/", "*a\n", "x\n"),
        ("s/\\(^*\\)/x/", "*\n", "x\n"),
        ("s/a\\|*b/x/", "*b\n", "x\n"),
        ("s/\\+a/x/", "+a\n", "x\n"),
        ("s/\\?a/x/", "?a\n", "x\n"),
        ("s/\\b*/x/", "*a\n", "*a\n"),
        ("s/a\\|^b/x/g", "bab\n", "xxb\n"),
        ("s/\\(a$\\)/x/", "a\n", "x\n"),
        ("s/a$\\|b/x/g", "ab\n", "ax\n"),
        ("s/a^b/x/", "a^b\n", "x\n"),
        ("s/a$b/x/", "a$b\n", "x\n"),
        ("s/[$]/x/", "$\n", "x\n"),
        ("s/\\(ab\\)\\{2\\}/x/", "abab\n", "x\n"),
    ] {
        new_ucmd!()
            .arg(script)
            .pipe_in(input)
            .succeeds()
            .stdout_only(output);
    }
    // POSIX has no `\|`, `\+` or `\?`.
    for (script, input) in [("s/a\\|b/x/", "a|b\n"), ("s/a\\+/x/", "a+\n")] {
        new_ucmd!()
            .args(&["--posix", script])
            .pipe_in(input)
            .succeeds()
            .stdout_only("x\n");
    }
}

/// An address takes GNU sed's flags `I` and `M`, any number, blanks between; only one
/// `I` was taken, and `M` was an unknown command. POSIX mode has none (TODO.md 14.6).
#[test]
fn test_address_flags() {
    for (script, output) in [
        ("/a/I,/b/Mp", "a\nb\n"),
        ("/A/IMp", "a\n"),
        ("/A/I M I p", "a\n"),
    ] {
        new_ucmd!()
            .args(&["-n", script])
            .pipe_in("a\nb\nc\n")
            .succeeds()
            .stdout_only(output);
    }
    new_ucmd!()
        .args(&["--posix", "-n", "/a/Ip"])
        .fails()
        .code_is(1)
        .stderr_is("sed: -e expression #1, char 4: unknown command: `I'\n");
}

/// A `}` or a comment may follow `s` and its flags, and any command, as in GNU sed:
/// `{s/a/b/}` was an unknown option to `s` (TODO.md 14.6).
#[test]
fn test_brace_or_comment_after_a_command() {
    for script in [
        "{s/a/b/}",
        "{s/a/b/ }",
        "s/a/b/#c",
        "s/a/b/g#c",
        "{s/a/b/g}",
    ] {
        new_ucmd!()
            .arg(script)
            .pipe_in("a\n")
            .succeeds()
            .stdout_only("b\n");
    }
    new_ucmd!()
        .arg("p#c")
        .pipe_in("a\n")
        .succeeds()
        .stdout_only("a\na\n");
    new_ucmd!()
        .arg("s/a/b/ }")
        .fails()
        .code_is(1)
        .stderr_is("sed: -e expression #1, char 8: unexpected `}'\n");
}

/// More errors at GNU sed's column and in its words (TODO.md 14.6).
#[test]
fn test_more_errors_at_gnu_seds_column() {
    for (script, error) in [
        ("y/a", "3: unterminated `y' command"),
        ("s/a/b", "5: unterminated `s' command"),
        ("1 !  ! p", "6: multiple `!'s"),
        ("a", "1: expected \\ after `a', `c' or `i'"),
        ("r", "1: missing filename in r/R/w/W commands"),
        ("s/a/b/w", "7: missing filename in r/R/w/W commands"),
        ("s/a/\\1/", "7: invalid reference \\1 on `s' command's RHS"),
        (
            "{s/a/\\1/}",
            "8: invalid reference \\1 on `s' command's RHS",
        ),
        ("s//x/I", "6: cannot specify modifiers on empty regexp"),
        ("v 9.0", "5: expected newer version of sed"),
        ("/a", "2: unterminated address regex"),
        (":", "1: \":\" lacks a label"),
        (": ;p", "2: \":\" lacks a label"),
        ("y/a/\\n/;y/a/bc", "14: unterminated `y' command"),
    ] {
        new_ucmd!()
            .arg(script)
            .fails()
            .code_is(1)
            .stderr_is(format!("sed: -e expression #1, char {error}\n"));
    }
}

/// A number flag too big to hold is an occurrence no line has, as GNU sed takes it; it
/// was an error of cash's own.
#[test]
fn test_huge_number_flag() {
    new_ucmd!()
        .arg("s/a/b/99999999999999999999999")
        .pipe_in("a\n")
        .succeeds()
        .stdout_only("a\n");
}

/// What a cycle queues goes out after the pattern space `n` prints, and after the one
/// `N` prints at the end of the input, as in GNU sed: it went out before both.
#[test]
fn test_appends_after_n_and_n_upper() {
    new_ucmd!()
        .args(&["-e", "a X", "-e", "n"])
        .pipe_in("a\nb\n")
        .succeeds()
        .stdout_only("a\nX\nb\n");
    new_ucmd!()
        .args(&["-e", "a X", "-e", "N"])
        .pipe_in("a\n")
        .succeeds()
        .stdout_only("a\nX\n");
    new_ucmd!()
        .args(&["-e", "$!N", "-e", "a X"])
        .pipe_in("a\nb\nc\n")
        .succeeds()
        .stdout_only("a\nb\nX\nc\nX\n");
}

/// `r` after a last line without a newline puts the newline first, as GNU sed does;
/// the file's text was joined to the line.
#[test]
fn test_r_after_a_line_without_newline() -> std::io::Result<()> {
    let (_dir, _, rf2) = r_files()?;
    new_ucmd!()
        .arg(format!("r {rf2}"))
        .pipe_in("a")
        .succeeds()
        .stdout_only("a\nx1\nx2\n");
    Ok(())
}

/// `-i` implies `-s`, as in GNU sed: each file has its own line numbers, so `1d` edits
/// both files; only the first lost its line 1.
#[test]
fn test_in_place_files_are_separate() -> std::io::Result<()> {
    let dir = tempfile::tempdir()?;
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    fs::write(&first, "a\nb\n")?;
    fs::write(&second, "c\nd\n")?;
    new_ucmd!()
        .args(&[
            "-i",
            "1d",
            first.to_str().unwrap(),
            second.to_str().unwrap(),
        ])
        .succeeds();
    assert_eq!(fs::read_to_string(&first)?, "b\n");
    assert_eq!(fs::read_to_string(&second)?, "d\n");
    Ok(())
}

/// A compile error is placed as GNU sed places it: the `-e` expression, counted among
/// the expressions alone, and the characters of it read, newlines and earlier lines
/// included. cash's sed wrote `<script argument 1>:2:1: error:`. Where GNU sed reads the
/// newline that ends a line before it stops, so does the count: the newline after `1` is
/// an unknown command, and `s`, `1,`, `w` and the flags of `s` read it too.
#[test]
fn test_compile_errors_are_placed_as_gnu_sed_places_them() {
    for (args, error) in [
        (&["p\n1"][..], "-e expression #1, char 3: missing command"),
        (
            &["p\n1\n"][..],
            "-e expression #1, char 4: unknown command: `\n'",
        ),
        (
            &["1!\n"][..],
            "-e expression #1, char 3: unknown command: `\n'",
        ),
        (
            &["p\n\n  k"][..],
            "-e expression #1, char 6: unknown command: `k'",
        ),
        (
            &["p\ns/a/b\n"][..],
            "-e expression #1, char 7: unterminated `s' command",
        ),
        (
            &["s\n"][..],
            "-e expression #1, char 2: unterminated `s' command",
        ),
        (
            &["y\n"][..],
            "-e expression #1, char 2: unterminated `y' command",
        ),
        (&["1,\n"][..], "-e expression #1, char 3: unexpected `,'"),
        (
            &["s/a/b/w\n"][..],
            "-e expression #1, char 8: missing filename in r/R/w/W commands",
        ),
        (
            &["p\nr\n"][..],
            "-e expression #1, char 4: missing filename in r/R/w/W commands",
        ),
        (
            &["p\ns/\\(a/b/\np"][..],
            "-e expression #1, char 11: Unmatched ( or \\(",
        ),
        (
            &["p\ns/\\(a/b/"][..],
            "-e expression #1, char 10: Unmatched ( or \\(",
        ),
        (
            &["p\n/\\(a/p"][..],
            "-e expression #1, char 7: Unmatched ( or \\(",
        ),
        (
            &["s//x/I\n"][..],
            "-e expression #1, char 7: cannot specify modifiers on empty regexp",
        ),
        (
            &["s/a/\\1/\n"][..],
            "-e expression #1, char 8: invalid reference \\1 on `s' command's RHS",
        ),
        (
            &["p\ny/a\n"][..],
            "-e expression #1, char 5: unterminated `y' command",
        ),
        (
            &["p\n:\n"][..],
            "-e expression #1, char 3: \":\" lacks a label",
        ),
        (
            &["--sandbox", "p\nw x\n"][..],
            "-e expression #1, char 3: e/r/w commands disabled in sandbox mode",
        ),
        // The expressions are numbered, each counted from its start.
        (
            &["-e", "p", "-e", "k"][..],
            "-e expression #2, char 1: unknown command: `k'",
        ),
        (
            &["-e", "p", "-e", "p", "-e", "1!!p"][..],
            "-e expression #3, char 3: multiple `!'s",
        ),
        (
            &["-n", "-e", "p", "-e", "p\n1"][..],
            "-e expression #2, char 3: missing command",
        ),
        // An unmatched `{` is placed at its expression, at no character of it.
        (&["{\np"][..], "-e expression #1, char 0: unmatched `{'"),
        (
            &["-e", "p", "-e", "{p"][..],
            "-e expression #2, char 0: unmatched `{'",
        ),
        (
            &["-e", "{", "-e", "{p"][..],
            "-e expression #2, char 0: unmatched `{'",
        ),
    ] {
        new_ucmd!()
            .args(args)
            .fails()
            .code_is(1)
            .stderr_is(format!("sed: {error}\n"));
    }
}

/// In a script file, a compile error is placed at the file's line, as GNU sed places it;
/// a newline read moves it to the next line (TODO.md 14.6).
#[test]
fn test_compile_errors_in_a_script_file() -> std::io::Result<()> {
    let dir = tempfile::tempdir()?;
    for (script, error) in [
        ("p\n1\n", "line 3: unknown command: `\n'"),
        ("p\ns/a/b\n", "line 2: unterminated `s' command"),
        ("p\ns/a/b", "line 2: unterminated `s' command"),
        ("s/\\(a/b/\n", "line 2: Unmatched ( or \\("),
        ("p\n\n  k\n", "line 3: unknown command: `k'"),
        ("{\np\n", "line 1: unmatched `{'"),
        ("{\n{\np\n", "line 2: unmatched `{'"),
        ("1!!p\n", "line 1: multiple `!'s"),
        ("p\n/a", "line 2: unterminated address regex"),
    ] {
        let path = dir.path().join("script.sed");
        fs::write(&path, script)?;
        let path = path.to_string_lossy().into_owned();
        new_ucmd!()
            .args(&["-f", &path])
            .fails()
            .code_is(1)
            .stderr_is(format!("sed: file {path} {error}\n"));
    }
    // A script on standard input is the file `-`.
    new_ucmd!()
        .args(&["-f", "-"])
        .pipe_in("1!!p\n")
        .fails()
        .code_is(1)
        .stderr_is("sed: file - line 1: multiple `!'s\n");
    Ok(())
}

/// A file sed cannot open, a script or one `w` writes, is GNU sed's error, with its
/// status 4 (TODO.md 14.6).
#[test]
fn test_files_sed_cannot_open() {
    new_ucmd!()
        .args(&["-f", "/no/such/script.sed"])
        .fails()
        .code_is(4)
        .stderr_is("sed: couldn't open file /no/such/script.sed: No such file or directory\n");
    new_ucmd!()
        .arg("s/a/b/w /no/such/dir/out")
        .fails()
        .code_is(4)
        .stderr_is("sed: couldn't open file /no/such/dir/out: No such file or directory\n");
}

/// `a`, `i` or `c` that ends its line has an empty line of text, as in GNU sed, and so
/// does `a\` and a newline that end the script; they were refused, or wrote nothing
/// (TODO.md 14.6).
#[test]
fn test_text_commands_ending_their_line() {
    for (script, output) in [
        ("a\n", "a\n\n"),
        ("i\n", "\na\n"),
        ("p\nc\n", "a\n\n"),
        ("a\np", "a\na\n\n"),
        ("a\\\n", "a\n\n"),
        ("a\\", "a\n"),
    ] {
        new_ucmd!()
            .arg(script)
            .pipe_in("a\n")
            .succeeds()
            .stdout_only(output);
    }
}

////////////////////////////////////////////////////////////
// GNU sed 4.9 parity, as compared with it (TODO.md phase 15)

/// Run `sed` with `args` on `input` and check that it writes `output`.
fn check_output(args: &[&str], input: &[u8], output: &[u8]) {
    new_ucmd!()
        .args(args)
        .pipe_in(input.to_vec())
        .succeeds()
        .stdout_is_bytes(output);
}

/// Check that `sed` with `args`, given no input, fails reading its script with `error`.
fn check_script_error(args: &[&str], error: &str) {
    new_ucmd!()
        .args(args)
        .fails()
        .code_is(1)
        .stderr_is(format!("sed: {error}\n"));
}

/// An empty regex with none before it is found when it is first matched, so a script
/// that never matches it runs, and what was written before stays. The error is placed
/// where the reading of the script ended: the last `-e` expression at char 0, or the
/// last script file at the line after its last newline.
#[test]
fn test_missing_regex_found_at_its_first_match() -> std::io::Result<()> {
    check_output(&["1d;//p"], b"a\n", b"");
    new_ucmd!()
        .args(&["-n", "-e", "p", "-e", "//p", "-e", "p", LINES1])
        .fails()
        .code_is(1)
        .stdout_is("l1_1\n")
        .stderr_is("sed: -e expression #3, char 0: no previous regular expression\n");

    let dir = tempfile::tempdir()?;
    let path = dir.path().join("script.sed");
    fs::write(&path, "p\n//p\n")?;
    let path = path.to_string_lossy().into_owned();
    new_ucmd!()
        .args(&["-n", "-f", &path, LINES1])
        .fails()
        .code_is(1)
        .stdout_is("l1_1\n")
        .stderr_is(format!(
            "sed: file {path} line 3: no previous regular expression\n"
        ));
    Ok(())
}

/// `v` reads its version as a label is read and compares it with 4.9 as GNU sed's
/// `strverscmp` does; it takes addresses, which it ignores. Versions were split at their
/// dots and read as numbers, a letter in them refused as "invalid version of sed".
#[test]
fn test_version_compared_as_gnu_sed_does() {
    for script in [
        "v 4.2a", "v 4.8.99", "v 4.09", "v 04.9", "v 4.", "v", "1v", "1,2v", "/a/v", "{v 4.2}",
        "v 4.2#c",
    ] {
        check_output(&[script], b"a\n", b"a\n");
    }
    check_output(&["v 4.2 p"], b"a\n", b"a\na\n");
    check_output(&["v;p"], b"a\n", b"a\na\n");
    for (script, error) in [
        ("v 4.9.0", "char 7"),
        ("v 4.9a;p", "char 6"),
        ("v4.10", "char 5"),
        ("v abc", "char 5"),
        ("v 4.9-1", "char 7"),
    ] {
        check_script_error(
            &[script],
            &format!("-e expression #1, {error}: expected newer version of sed"),
        );
    }
}

/// A label runs to white space, `;`, `}`, `#` or the line's end, whatever its
/// characters, as in GNU sed; only letters, digits, `.`, `_` and `-` were taken.
#[test]
fn test_labels_read_as_gnu_sed_reads_them() {
    check_output(&["-n", "bx@y;p;:x@y;s/^/>/p"], b"a\n", b">a\n");
    check_output(&["-n", "bé;p;:é;s/^/>/p"], b"a\n", b">a\n");
    check_output(&["-n", "$!{N;bx@y};:x@y;p"], b"a\nb\n", b"a\nb\n");
    check_output(&["-n", "bx#c\np\n:x#c\ns/^/>/p"], b"a\n", b">a\n");
    check_script_error(&["ba}"], "-e expression #1, char 3: unexpected `}'");
}

/// A backslash in a bracket expression is an ordinary character, as in GNU sed: `[\]]`
/// is a backslash then `]`, and `[a\]` ends at its `]`. GNU decodes `\n`, `\t`, `\cX`,
/// `\x41` and the like there, but in POSIX mode. The backslash was an escape.
#[test]
fn test_backslash_in_bracket_expression() {
    for (args, input, output) in [
        (&["s/[\\]]/X/g"][..], &b"a]b\\]c\n"[..], &b"a]bXc\n"[..]),
        (&["s/[a\\]/X/g"], b"a]b\\c\n", b"X]bXc\n"),
        (&["s/[\\.]/X/g"], b"a.b\\c\n", b"aXbXc\n"),
        (&["s/[\\w]/X/g"], b"aw\\\n", b"aXX\n"),
        (&["s/[\\b]/X/g"], b"ab\\c\n", b"aXXc\n"),
        (&["s/[\\n]/X/g"], b"anb\\\n", b"anb\\\n"),
        (&["N;s/[\\n]/X/g"], b"a\nb\n", b"aXb\n"),
        (&["s/[\\t]/X/g"], b"t\tx\n", b"tXx\n"),
        (&["s/[\\\\n]/X/g"], b"a\\nb\n", b"aXXb\n"),
        (&["s/[\\x5c]/X/g"], b"a\\b\n", b"aXb\n"),
        (&["s/[\\c]]/X/g"], b"\x1d]\n", b"X]\n"),
        (&["--posix", "s/[\\n]/X/g"], b"a\\nb\n", b"aXXb\n"),
        (&["-E", "s/[\\]]/X/g"], b"a]b\\]c\n", b"a]bXc\n"),
        (&["s/[[]/X/g"], b"a[b\n", b"aXb\n"),
        (&["s/[a[]/X/g"], b"a[b\n", b"XXb\n"),
        (&["s/[a&&b]/X/g"], b"a&b-\n", b"XXX-\n"),
    ] {
        check_output(args, input, output);
    }
}

/// Collating elements and equivalence classes of one character, as GNU sed has them in
/// the C and UTF-8 locales: `[.-.]` is `-` and `[=a=]` is `a`. They were not supported.
#[test]
fn test_collating_elements_and_equivalence_classes() {
    for (script, input, output) in [
        ("s/[[.-.]]/X/g", "a-b.c\n", "aXb.c\n"),
        ("s/[a-[.c.]]/X/g", "abcd\n", "XXXd\n"),
        ("s/[[.a.]-c]/X/g", "a-bcd\n", "X-XXd\n"),
        ("s/[[=a=]]/X/g", "a=b\n", "X=b\n"),
        ("s/[[=a=]b]/X/g", "a=b\n", "X=X\n"),
        ("s/[[.].]]/X/g", "a]b\n", "aXb\n"),
        ("s/[^[.-.]]/X/g", "a-b\n", "X-X\n"),
        ("s/[[.-.]-0]/X/g", "-./01\n", "XXXX1\n"),
        ("s/[]-a]/X/g", "]^_a-\n", "XXXX-\n"),
    ] {
        check_output(&[script], input.as_bytes(), output.as_bytes());
    }
}

/// In POSIX mode GNU's operators `\w`, `\W`, `\s`, `\S`, `\b`, `\B`, `\<`, `\>`, `` \` ``
/// and `\'` are the characters after their backslash, as in GNU sed; they stayed
/// operators.
#[test]
fn test_posix_mode_takes_gnu_operators_for_characters() {
    for (script, input, output) in [
        ("s/\\w/X/g", "aw\n", "aX\n"),
        ("s/\\W/X/g", "a-W\n", "a-X\n"),
        ("s/\\s/X/g", "a sb\n", "a Xb\n"),
        ("s/\\S/X/g", "aS\n", "aX\n"),
        ("s/\\b/X/g", "abc\n", "aXc\n"),
        ("s/\\B/X/g", "aBc\n", "aXc\n"),
        ("s/\\</X/g", "a<b\n", "aXb\n"),
        ("s/\\>/X/g", "a>b\n", "aXb\n"),
        ("s/\\`/X/g", "a`b\n", "aXb\n"),
        ("s/\\'/X/g", "a'b\n", "aXb\n"),
    ] {
        check_output(&["--posix", script], input.as_bytes(), output.as_bytes());
        check_output(
            &["--posix", "-E", script],
            input.as_bytes(),
            output.as_bytes(),
        );
    }
    // Outside POSIX mode they are operators.
    check_output(&["s/\\w/X/g"], b"a-b\n", b"X-X\n");
    check_output(&["s/\\`a/X/g"], b"aa\n", b"Xa\n");
    check_output(&["s/a\\'/X/g"], b"aa\n", b"aX\n");
}

/// A written `\A`, `\z`, `\D` or `\p` is the letter, as in GNU sed: they were the RE
/// engine's anchors and classes, or an error.
#[test]
fn test_escapes_gnu_sed_takes_for_letters() {
    for extended in [false, true] {
        for (script, input, output) in [
            ("s/\\A/X/g", "bAz\n", "bXz\n"),
            ("s/\\z/X/g", "bzA\n", "bXA\n"),
            ("s/\\D/X/g", "aD1\n", "aX1\n"),
            ("s/\\p/X/g", "ap\n", "aX\n"),
            ("s/\\d/X/g", "ad1\n", "aX1\n"),
        ] {
            let args: &[&str] = if extended { &["-E", script] } else { &[script] };
            check_output(args, input.as_bytes(), output.as_bytes());
        }
    }
}

/// GNU sed's character classes in UTF-8 mode take in what the locale classes so:
/// `[[:alpha:]]` matches `é`. The RE engine's are ASCII. In byte mode they stay ASCII.
#[test]
fn test_character_classes_in_utf8_mode() {
    for (script, input, output) in [
        ("s/[[:alpha:]]/X/g", "aé1中\n", "XX1X\n"),
        ("s/[[:upper:]]/X/g", "éÉ\n", "éX\n"),
        ("s/[[:lower:]]/X/g", "éÉ\n", "XÉ\n"),
        ("s/[[:alnum:]]/X/g", "é٣-\n", "XX-\n"),
        ("s/[[:space:]]/X/g", "a\u{a0}b\u{2003}\n", "aXbX\n"),
        ("s/[[:blank:]]/X/g", "a\u{a0}b\n", "aXb\n"),
        ("s/[[:punct:]]/X/g", "a§!_\n", "aXXX\n"),
        ("s/[^[:alpha:]]/X/g", "é-\n", "éX\n"),
        ("s/\\w/X/g", "e\u{301}_\n", "X\u{301}X\n"),
    ] {
        new_ucmd!()
            .env("LC_ALL", "C.UTF-8")
            .arg(script)
            .pipe_in(input)
            .succeeds()
            .stdout_only(output);
    }
    new_ucmd!()
        .env("LC_ALL", "C")
        .arg("s/[[:alpha:]]/X/g")
        .pipe_in(b"a\xe9\n".to_vec())
        .succeeds()
        .stdout_is_bytes(b"X\xe9\n");
}

/// A backslash and newline in a `y` string or in an `s` or address regex is a newline,
/// as in GNU sed, where they were "unterminated". A backslash that ends a `-e`
/// expression has no newline after it, and a line that ends a replacement without one
/// leaves it unterminated, as there: the replacement went on in the next expression or
/// line.
#[test]
fn test_backslash_newline_in_patterns() {
    for (args, input, output) in [
        (&["N;y/\\\n/X/"][..], &b"a\nb\n"[..], &b"aXb\n"[..]),
        (&["N;y/a\\\nb/XYZ/"], b"a\nb\n", b"XYZ\n"),
        (&["N;y/ab/X\\\n/"], b"a\nb\n", b"X\n\n\n"),
        (&["N;s/a\\\nb/X/"], b"a\nb\n", b"X\n"),
        (&["-E", "N;s/a\\\nb/X/"], b"a\nb\n", b"X\n"),
        (&["N;/a\\\nb/s/^/>/"], b"a\nb\n", b">a\nb\n"),
        (&["N;\\,a\\\nb,s/^/>/"], b"a\nb\n", b">a\nb\n"),
        (&["s/a/x\\\ny/"], b"a\n", b"x\ny\n"),
    ] {
        check_output(args, input, output);
    }
    check_script_error(
        &["-e", "s/a/b\\", "-e", "/"],
        "-e expression #1, char 6: unterminated `s' command",
    );
    check_script_error(
        &["-e", "y/a\\", "-e", "/b/"],
        "-e expression #1, char 4: unterminated `y' command",
    );
    check_script_error(
        &["s/a/b\n/"],
        "-e expression #1, char 5: unterminated `s' command",
    );
}

/// `\cX` as GNU sed reads it: `\c\\` is the control character of a backslash, `\c`
/// before another escape is an error once the string is read, and `\c` before the
/// delimiter has nothing to stand for. `\c\\` left a backslash to escape the delimiter.
#[test]
fn test_control_character_escapes() {
    for (args, input, output) in [
        (&["s/\\c\\\\/X/"][..], &b"a\x1c\n"[..], &b"aX\n"[..]),
        (&["s/[\\c\\\\]/X/"], b"a\x1c\n", b"aX\n"),
        (&["s/b/\\c\\\\/"], b"ab\n", b"a\x1c\n"),
        (&["y/b\\c\\\\/xy/"], b"ab\x1c\n", b"axy\n"),
        (&["s/a/x\\c/"], b"ab\n", b"x\\b\n"),
        (&["s/\\c]/X/"], b"\x1d\n", b"X\n"),
    ] {
        check_output(args, input, output);
    }
    for (script, error) in [
        (
            "s/\\c\\d/X/",
            "char 9: recursive escaping after \\c not allowed",
        ),
        (
            "s/b\\c\\d/X/g;p",
            "char 12: recursive escaping after \\c not allowed",
        ),
        (
            "s/b/\\c\\d/g;p",
            "char 9: recursive escaping after \\c not allowed",
        ),
        (
            "y/b/\\c\\d/",
            "char 9: recursive escaping after \\c not allowed",
        ),
        (
            "/\\c\\d/p",
            "char 6: recursive escaping after \\c not allowed",
        ),
        ("s/\\c/X/", "char 7: Trailing backslash"),
        (
            "y/a/\\c/",
            "char 7: strings for `y' command are different lengths",
        ),
    ] {
        check_script_error(&[script], &format!("-e expression #1, {error}"));
    }
}

/// GNU sed has no backspace escape: `\b` is the letter in a replacement, in `y` and in
/// text, and so is any other character without an escape of its own in `y`.
#[test]
fn test_backslash_b_and_unknown_escapes() {
    check_output(&["s/a/\\b/"], b"ab\n", b"bb\n");
    check_output(&["y/\\b/X/"], b"b\x08\n", b"X\x08\n");
    check_output(&["a x\\by"], b"q\n", b"q\nxby\n");
    check_output(&["y/a\\q/XY/"], b"aq\\\n", b"XY\\\n");
    check_output(&["y/\\&/X/"], b"&\n", b"X\n");
}

/// Text that ends in `\c` ends there, with GNU sed's control character of the newline,
/// `J`, and no newline; it panicked. Text whose last line ends in a backslash and a
/// newline, at the end of the script, ends with an empty line, as in GNU sed.
#[test]
fn test_text_ending_in_escapes() {
    check_output(&["a x\\c"], b"a\n", b"a\nxJ");
    check_output(&["a x\\c\np"], b"a\n", b"a\na\nxJ");
    check_output(&["i\\\nx\\c\np"], b"a\n", b"x\na\na\n");
    check_output(&["a x\\qy\\\n"], b"a\n", b"a\nxqy\n\n");
}

/// Files for the run-time tests: `f1` and `f2` with lines, `last` whose last line has no
/// end, `empty`, and the directory `dir`, in a directory of their own; with the path of
/// a name in it.
fn run_time_files() -> std::io::Result<(tempfile::TempDir, impl Fn(&str) -> String)> {
    let dir = tempfile::tempdir()?;
    fs::write(dir.path().join("f1"), "x\ny\nz\n")?;
    fs::write(dir.path().join("f2"), "1\n2\n")?;
    fs::write(dir.path().join("last"), "a\nb")?;
    fs::write(dir.path().join("empty"), "")?;
    fs::create_dir(dir.path().join("dir"))?;
    let root = dir.path().to_path_buf();
    let path = move |name: &str| root.join(name).to_string_lossy().into_owned();
    Ok((dir, path))
}

/// An input file that cannot be read is GNU sed's "can't read F: ...", and passed over:
/// the others are read, and sed ends with status 2, whatever `q` says. It ended sed at
/// once, in cash's words, with status 1.
#[test]
fn test_unreadable_input_files_are_passed_over() -> std::io::Result<()> {
    let (_dir, path) = run_time_files()?;
    let (nosuch, f1, f2) = (path("nosuch"), path("f1"), path("f2"));
    let cant_read = format!("sed: can't read {nosuch}: No such file or directory\n");
    new_ucmd!()
        .args(&["p", &nosuch, &f1])
        .fails()
        .code_is(2)
        .stdout_is("x\nx\ny\ny\nz\nz\n")
        .stderr_is(&cant_read);
    new_ucmd!()
        .args(&["=", &f2, &nosuch, &f2])
        .fails()
        .code_is(2)
        .stdout_is("1\n1\n2\n2\n3\n1\n4\n2\n")
        .stderr_is(&cant_read);
    new_ucmd!()
        .args(&["2q5", &nosuch, &f1])
        .fails()
        .code_is(2)
        .stdout_is("x\ny\n");
    // `$` looks past it, as past an empty file.
    new_ucmd!()
        .args(&["-n", "$p", &f1, &nosuch, &path("empty")])
        .fails()
        .code_is(2)
        .stdout_is("z\n");
    new_ucmd!()
        .args(&["-i", "s/x/X/", &nosuch, &f1])
        .fails()
        .code_is(2)
        .stderr_is(&cant_read);
    assert_eq!(fs::read_to_string(&f1)?, "X\ny\nz\n");
    Ok(())
}

/// A directory as input is GNU sed's "read error on D: Is a directory", status 4, after
/// the files before it, and to an in-place edit "couldn't edit D: not a regular file";
/// they said "error opening input file 'D': Permission denied" with status 1. `r` and
/// `R` of a directory fail the same way, where they read nothing.
#[test]
fn test_directory_as_input() -> std::io::Result<()> {
    let (_dir, path) = run_time_files()?;
    let (dir, f1, f2) = (path("dir"), path("f1"), path("f2"));
    new_ucmd!()
        .args(&["p", &f2, &dir, &f1])
        .fails()
        .code_is(4)
        .stdout_is("1\n1\n2\n2\n")
        .stderr_is(format!("sed: read error on {dir}: Is a directory\n"));
    new_ucmd!()
        .args(&["-i", "s/1/one/", &f2, &dir, &f1])
        .fails()
        .code_is(4)
        .stderr_is(format!("sed: couldn't edit {dir}: not a regular file\n"));
    assert_eq!(fs::read_to_string(&f2)?, "one\n2\n");
    assert_eq!(fs::read_to_string(&f1)?, "x\ny\nz\n");
    new_ucmd!()
        .args(&[&format!("r {dir}"), &f1])
        .fails()
        .code_is(4)
        .stdout_is("x\n")
        .stderr_is(format!("sed: read error on {dir}: Is a directory\n"));
    new_ucmd!()
        .args(&[&format!("R {dir}"), &f1])
        .fails()
        .code_is(4)
        .stdout_is("")
        .stderr_is(format!("sed: read error on {dir}: Is a directory\n"));
    Ok(())
}

/// `--follow-symlinks` with a file that is not there is GNU sed's "couldn't readlink F:
/// ...", status 4.
#[test]
fn test_follow_symlinks_of_a_missing_file() -> std::io::Result<()> {
    let (_dir, path) = run_time_files()?;
    let nosuch = path("nosuch");
    new_ucmd!()
        .args(&["-i", "--follow-symlinks", "p", &nosuch])
        .fails()
        .code_is(4)
        .stderr_is(format!(
            "sed: couldn't readlink {nosuch}: No such file or directory\n"
        ));
    Ok(())
}

/// GNU sed's special files: `/dev/stdin` for `r` and `R` is standard input, and
/// `/dev/stdout` and `/dev/stderr` for `w`, `W` and the `w` flag of `s` are sed's
/// output and error, but in POSIX mode, where they are file names. Windows has none of
/// them: `r` and `R` read nothing, and `w` failed to open them.
#[test]
fn test_special_files() -> std::io::Result<()> {
    let (_dir, path) = run_time_files()?;
    let f1 = path("f1");
    new_ucmd!()
        .args(&["R /dev/stdin", &f1])
        .pipe_in("A\nB\n")
        .succeeds()
        .stdout_only("x\nA\ny\nB\nz\n");
    new_ucmd!()
        .args(&["1r /dev/stdin", &f1])
        .pipe_in("A\nB\n")
        .succeeds()
        .stdout_only("x\nA\nB\ny\nz\n");
    new_ucmd!()
        .args(&["w /dev/stdout", &f1])
        .succeeds()
        .stdout_only("x\nx\ny\ny\nz\nz\n");
    new_ucmd!()
        .args(&["-n", "s/x/X/w /dev/stderr", &f1])
        .succeeds()
        .stdout_is("")
        .stderr_is("X\n");
    new_ucmd!()
        .args(&["-n", "w /dev/null", &f1])
        .succeeds()
        .no_output();
    new_ucmd!()
        .args(&["--posix", "-n", "w /dev/stdout", &f1])
        .fails()
        .code_is(4)
        .stderr_is("sed: couldn't open file /dev/stdout: No such file or directory\n");
    // In an in-place edit `/dev/stdout` is still standard output.
    new_ucmd!()
        .args(&["-i", "s/y/Y/w /dev/stdout", &f1])
        .succeeds()
        .stdout_only("Y\n");
    assert_eq!(fs::read_to_string(&f1)?, "x\nY\nz\n");
    // `/dev/stdout` keeps its own count of a line without its end.
    let (last, f2) = (path("last"), path("f2"));
    new_ucmd!()
        .args(&["-s", "W /dev/stdout", &last, &f2])
        .succeeds()
        .stdout_only("a\na\nbb\n1\n\n1\n2\n2\n");
    Ok(())
}

/// The commands that name one file share it, as in GNU sed; each truncated it, and they
/// overwrote each other's lines. A line written without its end gets it before the next.
#[test]
fn test_commands_writing_one_file() -> std::io::Result<()> {
    let (_dir, path) = run_time_files()?;
    let (f1, out) = (path("f1"), path("out"));
    new_ucmd!()
        .args(&[
            "-e",
            &format!("w {out}"),
            "-e",
            &format!("s/x/X/w {out}"),
            &f1,
        ])
        .succeeds()
        .stdout_only("X\ny\nz\n");
    assert_eq!(fs::read_to_string(&out)?, "x\nX\ny\nz\n");
    new_ucmd!()
        .args(&["-s", "-n", &format!("w {out}"), &path("last"), &path("f2")])
        .succeeds();
    assert_eq!(fs::read_to_string(&out)?, "a\nb\n1\n2\n");
    Ok(())
}

/// A last line without its end gets one before the next file's first line, as in GNU
/// sed; the two were joined.
#[test]
fn test_last_line_without_end_before_next_file() -> std::io::Result<()> {
    let (_dir, path) = run_time_files()?;
    new_ucmd!()
        .args(&["p", &path("last"), &path("f2")])
        .succeeds()
        .stdout_only("a\na\nb\nb\n1\n1\n2\n2\n");
    Ok(())
}

/// `$` is the last line of the last input with lines: GNU sed looks past files that are
/// empty or cannot be read, and into standard input. `N` on that line prints what it
/// has.
#[test]
fn test_last_line_looks_ahead() -> std::io::Result<()> {
    let (_dir, path) = run_time_files()?;
    let (f1, f2, empty) = (path("f1"), path("f2"), path("empty"));
    new_ucmd!()
        .args(&["-n", "$p", &f1, "-"])
        .pipe_in("")
        .succeeds()
        .stdout_only("z\n");
    new_ucmd!()
        .args(&["-n", "$p", &f1, "-"])
        .pipe_in("q\n")
        .succeeds()
        .stdout_only("q\n");
    new_ucmd!()
        .args(&["$!N;s/\\n/-/", &f2, &empty, &f1])
        .succeeds()
        .stdout_only("1-2\nx-y\nz\n");
    new_ucmd!()
        .args(&["N", &f1, &empty])
        .succeeds()
        .stdout_only("x\ny\nz\n");
    Ok(())
}

/// Back-references in a pattern space that is not UTF-8: in byte mode every byte is a
/// character, and in UTF-8 mode a byte that is not UTF-8 matches nothing but itself, as
/// in GNU sed. They were an error, as was any byte above 0x7F in byte mode.
#[test]
fn test_back_references_on_bytes() {
    for (locale, script, input, output) in [
        ("C", "s/\\(.\\)\\1/X/g", &b"\xe9\xe9aa\n"[..], &b"XX\n"[..]),
        ("C", "s/\\(a\\)\\1/X/", b"aa\xff\n", b"X\xff\n"),
        (
            "C",
            "s/\\(.\\)\\1/<&>/g",
            b"\xc3\xa9\xc3\xa9\n",
            b"\xc3\xa9\xc3\xa9\n",
        ),
        (
            "C.UTF-8",
            "s/\\(.\\)\\1/X/g",
            b"\xff\xffaa\n",
            b"\xff\xffX\n",
        ),
        ("C.UTF-8", "s/\\(a\\)\\1/X/", b"aa\xff\n", b"X\xff\n"),
        (
            "C.UTF-8",
            "s/\\(.\\)\\1/X/g",
            b"\xc3\xa9\xc3\xa9\xff\n",
            b"X\xff\n",
        ),
        (
            "C.UTF-8",
            "s/\\([^a]\\)\\1/X/g",
            b"\xff\xffbb\n",
            b"\xff\xffX\n",
        ),
    ] {
        new_ucmd!()
            .env("LC_ALL", locale)
            .arg(script)
            .pipe_in(input.to_vec())
            .succeeds()
            .stdout_is_bytes(output);
    }
    new_ucmd!()
        .env("LC_ALL", "C")
        .args(&["-E", "s/(.)\\1/X/g"])
        .pipe_in(b"\xe9\xe9aa\n".to_vec())
        .succeeds()
        .stdout_is_bytes(b"XX\n");
}

/// What is not UTF-8 in UTF-8 mode stays as it is for `y` and `l`, as in GNU sed; it was
/// an error.
#[test]
fn test_invalid_utf8_for_y_and_l() {
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .arg("y/aé/éa/")
        .pipe_in(b"a\xff\xc3\xa9\n".to_vec())
        .succeeds()
        .stdout_is_bytes(b"\xc3\xa9\xffa\n");
    new_ucmd!()
        .env("LC_ALL", "C.UTF-8")
        .args(&["-n", "-U", "l"])
        .pipe_in(b"a\xff\n".to_vec())
        .succeeds()
        .stdout_is_bytes(b"a\\377$\n");
}
