//
// Copyright (c) 2024-2026 Hemi Labs, Inc.
//
// This file is part of the posixutils-rs project covered under
// the MIT License.  For the full license text, please see the LICENSE
// file in the root directory of this project.
// SPDX-License-Identifier: MIT
//

//! awk programs run against the expected output in `tests/awk`, most of it gawk's.

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::format_collect,
    clippy::manual_assert,
    clippy::manual_string_new,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::panic,
    clippy::semicolon_if_nothing_returned,
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    reason = "posixutils-rs tests: an integration test is outside a test module by \
              construction, and a failed assumption in a test should abort it loudly"
)]

use std::io::Write;
use std::process::{Command, Output, Stdio};

pub struct TestPlan {
    pub cmd: String,
    pub args: Vec<String>,
    pub stdin_data: String,
    pub expected_out: String,
    pub expected_err: String,
    pub expected_exit_code: i32,
}

fn run_test_base(args: &[String], stdin_data: &[u8]) -> Output {
    let awk_bin = env!("CARGO_BIN_EXE_awk");
    let mut cmd = Command::new(awk_bin);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LC_ALL", "C");

    let mut child = cmd.spawn().expect("failed to spawn awk");
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(stdin_data);
    }
    child.wait_with_output().expect("failed to wait on awk")
}

pub fn run_test(plan: TestPlan) {
    let output = run_test_base(&plan.args, plan.stdin_data.as_bytes());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.replace("\r\n", "\n"),
        plan.expected_out.replace("\r\n", "\n")
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.replace("\r\n", "\n"),
        plan.expected_err.replace("\r\n", "\n")
    );
    assert_eq!(output.status.code(), Some(plan.expected_exit_code));
}

pub fn run_test_with_checker<F: FnMut(&TestPlan, &Output)>(plan: TestPlan, mut checker: F) {
    let output = run_test_base(&plan.args, plan.stdin_data.as_bytes());
    checker(&plan, &output);
}

fn test_awk(args: Vec<String>, expected_output: &str) {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args,
        stdin_data: String::new(),
        expected_out: String::from(expected_output),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

macro_rules! test_awk {
    ($test_name:ident $(,$data_file:expr)*) => {
        test_awk(vec![
            "-f".to_string(),
            concat!("tests/awk/", stringify!($test_name), ".awk").to_string(),
            $($data_file.to_string(),)*
        ], include_str!(concat!("awk/", stringify!($test_name), ".out")))
    };
}

#[test]
fn test_awk_empty_program() {
    test_awk!(empty_program);
}

#[test]
fn test_awk_print() {
    test_awk!(print);
}

#[test]
fn test_awk_printf() {
    test_awk!(printf);
}

// `%g`, OFMT and CONVFMT, against gawk 5.4's output: `print 100000.4` printed `1`, and
// the mean of 100000 and 200001 printed `15` (REVIEW_REPORT.md TXT-01).
#[test]
fn test_awk_general_float_format_matches_gawk() {
    test_awk!(general_float_format_matches_gawk);
}

// Integers beyond an i64, against gawk 5.4's output: `print 2^64` printed
// 9223372036854775807, and `printf "%x", -1` was a fatal error (TXT-02).
#[test]
fn test_awk_integers_beyond_i64_match_gawk() {
    test_awk!(integers_beyond_i64_match_gawk);
}

// A `for (k in a)` left by `return` or `break` locked `a` against insertion for the rest
// of the run, so the usual dedup function died with "active iterator"; `for (k in a)
// delete a` panicked (TXT-03). Counts, not iteration order, which differs between awks;
// the expected output is gawk 5.4's.
#[test]
fn test_awk_array_loops_left_early_release_the_array() {
    test_awk!(array_loops_left_early_release_the_array);
}

// An escape awk does not define stands for the character, as in gawk, whose output this
// is, with its warnings, once for each character: `split(s, parts, "\.")` was a parse
// error (TXT-05), and the warnings were not given (TODO.md phase 15).
#[test]
fn test_awk_unknown_string_escapes_stand_for_the_character() {
    let file = "tests/awk/unknown_string_escapes_stand_for_the_character.awk";
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec!["-f".to_string(), file.to_string()],
        stdin_data: String::new(),
        expected_out: String::from(include_str!(
            "awk/unknown_string_escapes_stand_for_the_character.out"
        )),
        expected_err: format!(
            "awk: {file}:2: warning: escape sequence `\\.' treated as plain `.'\n\
             awk: {file}:4: warning: escape sequence `\\q' treated as plain `q'\n\
             awk: {file}:6: warning: escape sequence `\\&' treated as plain `&'\n\
             awk: {file}:9: warning: escape sequence `\\/' treated as plain `/'\n"
        ),
        expected_exit_code: 0,
    });
}

// What awk printed comes before what `system()` prints, and `print > "/dev/stdout"` keeps
// its place among `print`s: gawk 5.4's output (TXT-08, TXT-14).
#[test]
fn test_awk_output_is_flushed_before_another_program_writes() {
    test_awk!(output_is_flushed_before_another_program_writes);
}

// An element is found by its key when it is assigned, after the right-hand side ran:
// `b["x"] = split(s, b)` wrote into `b[1]`, and deleting an element on the right panicked
// (TXT-13). gawk 5.4's output.
#[test]
fn test_awk_array_element_assigned_after_the_array_changes() {
    test_awk!(array_element_assigned_after_the_array_changes);
}

// A variable not used yet, passed to a function that uses it as an array, is that array
// in the caller too; a scalar stays the function's own (TXT-18). gawk 5.4's output.
#[test]
fn test_awk_an_unused_variable_passed_to_a_function_can_become_an_array() {
    test_awk!(an_unused_variable_passed_to_a_function_can_become_an_array);
}

// A field is a numeric string only when all of it is a number: `9abc`, `1f` and `1e`
// compared as numbers, and `+inf` and `-nan` as text (TXT-16). gawk 5.4's output.
#[test]
fn test_awk_numeric_strings_are_whole_numbers() {
    test_awk!(
        numeric_strings_are_whole_numbers,
        "tests/awk/numeric_strings_are_whole_numbers.txt"
    );
}

// Dynamic regular expressions are compiled once and cached; a program using more of them
// than the cache holds still matches each against its own pattern (TXT-16).
#[test]
fn test_awk_dynamic_regexes_beyond_the_cache_still_match() {
    test_awk!(dynamic_regexes_beyond_the_cache_still_match);
}

// A NUL in a record stopped the program with "invalid string", and "\0" in a string was a
// parse error (TXT-11). gawk 5.4's output.
#[test]
fn test_awk_nul_bytes_are_ordinary_characters() {
    test_awk!(
        nul_bytes_are_ordinary_characters,
        "tests/awk/nul_bytes_are_ordinary_characters.txt"
    );
}

// A character split by the edge of what the reader had read was fatal with a regex RS
// (TXT-12). gawk 5.4's output in a UTF-8 locale.
#[test]
fn test_awk_regex_rs_reads_a_character_split_by_the_buffer() {
    test_awk!(
        regex_rs_reads_a_character_split_by_the_buffer,
        "tests/awk/regex_rs_reads_a_character_split_by_the_buffer.txt"
    );
}

// A separator at the end of what had been read was taken as whole, so each newline of a
// blank line ended a record of its own (TXT-12). gawk 5.4's output.
#[test]
fn test_awk_regex_rs_that_spans_lines_ends_one_record() {
    test_awk!(
        regex_rs_that_spans_lines_ends_one_record,
        "tests/awk/regex_rs_that_spans_lines_ends_one_record.txt"
    );
}

// A record of more than 65,535 fields stopped the program with "too many fields": the
// field index was a u16. gawk 5.4's output.
#[test]
fn test_awk_records_of_more_than_65535_fields_split() {
    test_awk!(records_of_more_than_65535_fields_split);
}

// Infinite and NaN numbers, as gawk 5.4 writes them (always signed, under every numeric
// conversion) and reads them (only `+inf`, `-inf`, `+nan` and `-nan`). `print` wrote
// `inf`, `%d` 9223372036854775807, `"inf" + 0` was infinite and `".5e" + 0` was 0.
#[test]
fn test_awk_infinite_and_nan_numbers_match_gawk() {
    // With gawk's warning for the log of a negative number.
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec![
            "-f".to_string(),
            "tests/awk/infinite_and_nan_numbers_match_gawk.awk".to_string(),
        ],
        stdin_data: String::new(),
        expected_out: String::from(include_str!("awk/infinite_and_nan_numbers_match_gawk.out")),
        expected_err: String::from(
            "awk: tests/awk/infinite_and_nan_numbers_match_gawk.awk:4: warning: log: received \
             negative argument -1\n",
        ),
        expected_exit_code: 0,
    });
}

// An RS that matches empty text looped forever on empty records (TXT-12). gawk 5.4 ends
// four empty records and loses `a` and `b`; this is the records the separators delimit.
#[test]
fn test_awk_rs_matching_empty_text_ends_no_record() {
    test_awk!(
        rs_matching_empty_text_ends_no_record,
        "tests/awk/rs_matching_empty_text_ends_no_record.txt"
    );
}

#[test]
fn test_awk_hello_world() {
    test_awk!(hello_world)
}

#[test]
fn test_awk_missing_pattern_matches_all_records() {
    test_awk!(
        missing_pattern_matches_all_records,
        "tests/awk/test_data.txt"
    );
}

#[test]
fn test_awk_missing_action_prints_the_record() {
    test_awk!(missing_action_prints_the_record, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_actions_execute_in_the_right_order() {
    test_awk!(
        actions_execute_in_the_right_order,
        "tests/awk/test_data.txt"
    );
}

#[test]
fn test_awk_variable_assignment() {
    test_awk!(variable_assignment);
}

#[test]
fn test_awk_arithmetic_operator_precedence() {
    test_awk!(arithmetic_operator_precedence);
}

#[test]
fn test_awk_logical_operator_precedence() {
    test_awk!(logical_operator_precedence);
}

#[test]
fn test_awk_comparison_operator_precedence() {
    test_awk!(comparison_operator_precedence);
}

#[test]
fn test_awk_conditional_expression() {
    test_awk!(conditional_expression);
}

#[test]
fn test_awk_array_assignment() {
    test_awk!(array_assignment);
}

#[test]
fn test_awk_ere_match() {
    test_awk!(ere_match);
}

#[test]
fn test_awk_in_operator() {
    test_awk!(in_operator)
}

#[test]
fn test_awk_close_returns_status() {
    test_awk!(close_returns_status);
}

#[test]
fn test_awk_dash_f_escape_processing() {
    // POSIX: `-F sepstring` == `-v FS=sepstring`, so `\t` is a tab (a single
    // character), not the two-character string backslash-t.
    test_awk(
        vec![
            "-F".to_string(),
            "\\t".to_string(),
            "{ print length(FS), $2 }".to_string(),
            "tests/awk/tab_separated.txt".to_string(),
        ],
        "1 b\n",
    );
}

#[test]
fn test_awk_multibyte_char_counts() {
    test_awk!(multibyte_char_counts);
}

#[test]
fn test_awk_printf_star_width() {
    test_awk!(printf_star_width);
}

#[test]
fn test_awk_substr_edges() {
    test_awk!(substr_edges);
}

#[test]
fn test_awk_case_mapping() {
    test_awk!(case_mapping);
}

#[test]
fn test_awk_getline_pipe_advances_nr() {
    test_awk!(getline_pipe_nr);
}

#[test]
fn test_awk_record_with_many_fields() {
    // A record with more than the old 1024-field cap must keep every field.
    test_awk!(many_fields, "tests/awk/many_fields.txt");
}

#[test]
fn test_awk_high_field_assignment() {
    test_awk!(high_field_assignment);
}

#[test]
fn test_awk_uninitialized_field_comparison() {
    // POSIX: nonexistent fields (85506) and empty fields from $0/FS (85511) have
    // the uninitialized value, so they compare numerically equal to 0 while
    // still being string-equal to "". Populated fields are unaffected.
    test_awk(
        vec![
            "-F".to_string(),
            ":".to_string(),
            "{ print ($5==0); print ($2==0); print ($2==\"\"); print ($1==\"a\") }".to_string(),
            "tests/awk/empty_fields.txt".to_string(),
        ],
        "1\n1\n1\n1\n",
    );
}

#[test]
fn test_awk_program_file_from_stdin() {
    // POSIX: a `-f` progfile of `-` denotes the standard input.
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec!["-f".to_string(), "-".to_string()],
        stdin_data: String::from("BEGIN { print \"okprog\" }\n"),
        expected_out: String::from("okprog\n"),
        expected_err: String::new(),
        expected_exit_code: 0,
    });
}

#[test]
fn test_awk_multidimensional_index() {
    test_awk!(multidimensional_index);
}

#[test]
fn test_awk_multidimensional_in_operator() {
    test_awk!(multidimensional_in_operator)
}

#[test]
fn test_awk_unary_lvalue_operators() {
    test_awk!(unary_lvalue_operators);
}

#[test]
fn test_awk_compound_assignment() {
    test_awk!(compound_assignment);
}

#[test]
fn test_awk_comparison_operators() {
    test_awk!(comparison_operators);
}

#[test]
fn test_awk_uninitialized_variables() {
    test_awk!(uninitialized_variables);
}

#[test]
fn test_awk_access_field_variables() {
    test_awk!(access_field_variables, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_assigning_to_a_non_existent_field_var_creates_it() {
    test_awk!(
        assigning_to_a_non_existent_field_var_creates_it,
        "tests/awk/test_data.txt"
    );
}

#[test]
fn test_awk_print_program_arguments() {
    test_awk!(print_program_arguments, "one", "two", "three");
}

#[test]
fn test_awk_set_arguments_in_begin() {
    test_awk!(set_arguments_in_begin, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_clear_input_file_in_begin() {
    test_awk!(clear_input_file_in_begin, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_setting_argc_to_one_ignores_all_arguments() {
    test_awk!(
        setting_argc_to_one_ignores_all_arguments,
        "one",
        "two",
        "three"
    );
}

#[test]
fn test_awk_change_default_number_to_string_conversion() {
    test_awk!(change_default_number_to_string_conversion);
}

#[test]
fn test_awk_filename() {
    test_awk!(
        filename,
        "tests/awk/test_data.txt",
        "tests/awk/test_data2.txt"
    );
}

#[test]
fn test_awk_file_record_number() {
    test_awk!(
        file_record_number,
        "tests/awk/test_data.txt",
        "tests/awk/test_data2.txt"
    );
}

#[test]
fn test_awk_record_number() {
    test_awk!(
        record_number,
        "tests/awk/test_data.txt",
        "tests/awk/test_data2.txt"
    );
}

#[test]
fn test_awk_output_float_format() {
    test_awk!(output_float_format);
}

#[test]
fn test_awk_output_field_separator() {
    test_awk!(output_field_separator);
}

#[test]
fn test_awk_output_record_separator() {
    test_awk!(output_record_separator);
}

#[test]
fn test_change_record_separator() {
    test_awk!(change_record_separator, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_subscript_separator() {
    test_awk!(subscript_separator);
}

#[test]
fn test_awk_default_field_separator_rules() {
    test_awk!(default_field_separator_rules, "tests/awk/test_data3.txt");
}

#[test]
fn test_awk_character_field_separator() {
    test_awk!(character_field_separator, "tests/awk/test_data.csv");
}

#[test]
fn test_awk_ere_field_separator() {
    test_awk!(ere_field_separator, "tests/awk/test_data4.txt");
}

#[test]
fn test_awk_program_with_only_end_actions_reads_input_files() {
    test_awk!(
        program_with_only_end_actions_reads_input_files,
        "tests/awk/test_data.txt",
        "tests/awk/test_data2.txt"
    );
}

#[test]
fn test_awk_pattern_range() {
    test_awk!(pattern_range, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_if_stmt() {
    test_awk!(if_stmt);
}

#[test]
fn test_awk_while_stmt() {
    test_awk!(while_stmt);
}

#[test]
fn test_awk_do_while_stmt() {
    test_awk!(do_while_stmt);
}

#[test]
fn test_awk_for_stmt() {
    test_awk!(for_stmt);
}

#[test]
fn test_awk_break_stmt() {
    test_awk!(break_stmt);
}

#[test]
fn test_awk_continue_stmt() {
    test_awk!(continue_stmt);
}

#[test]
fn test_awk_break_continue_in_for_in_and_do_while() {
    test_awk!(break_continue_in_for_in_and_do_while);
}

#[test]
fn test_awk_for_each() {
    test_awk!(for_each);
}

#[test]
fn test_awk_delete() {
    test_awk!(delete);
}

#[test]
fn test_awk_next() {
    test_awk!(next, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_nextfile() {
    test_awk!(
        nextfile,
        "tests/awk/test_data.txt",
        "tests/awk/test_data2.txt"
    );
}

#[test]
fn test_awk_exit() {
    test_awk!(exit, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_output_redirection() {
    let mut correct_stdout = true;
    let mut correct_stderr = true;
    let mut correct_exit_code = true;

    let previous_append_file_contents = include_str!("awk/output_redirection_append.txt");

    run_test_with_checker(
        TestPlan {
            cmd: String::from("awk"),
            args: vec![
                "-f".to_string(),
                "tests/awk/output_redirection.awk".to_string(),
            ],
            stdin_data: String::new(),
            expected_out: String::new(),
            expected_err: String::new(),
            expected_exit_code: 0,
        },
        |_, output| {
            correct_stdout = output.stdout.is_empty();
            correct_stderr = output.stderr.is_empty();
            correct_exit_code = output.status.code() == Some(0);
        },
    );

    let correct_truncate_output = include_str!("awk/output_redirection_truncate.correct.txt");
    let correct_append_output = include_str!("awk/output_redirection_append.correct.txt");

    let truncate_output_result =
        std::fs::read_to_string("tests/awk/output_redirection_truncate.txt");
    let append_output_result = std::fs::read_to_string("tests/awk/output_redirection_append.txt");

    std::fs::write(
        "tests/awk/output_redirection_truncate.txt",
        correct_truncate_output,
    )
    .expect("failed to write to file");
    std::fs::write(
        "tests/awk/output_redirection_append.txt",
        previous_append_file_contents,
    )
    .expect("failed to write to file");

    if !correct_stdout || !correct_stderr || !correct_exit_code {
        panic!("awk output redirection test failed");
    }

    if let (Ok(truncate_output), Ok(append_output)) = (truncate_output_result, append_output_result)
    {
        assert_eq!(truncate_output, correct_truncate_output);
        assert_eq!(append_output, correct_append_output);
    } else {
        panic!("failed to read output files");
    }
}

#[test]
fn test_awk_builtin_arithmetic_functions() {
    test_awk!(builtin_arithmetic_functions);
}

#[test]
fn builtin_string_functions() {
    test_awk!(builtin_string_functions, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_delete_array_elements_in_for_each() {
    test_awk!(delete_array_elements_in_for_each);
}

#[test]
fn test_awk_call_function_no_args() {
    test_awk!(call_function_no_args);
}

#[test]
fn test_awk_scalar_arguments_are_passed_by_copy() {
    test_awk!(scalar_arguments_are_passed_by_copy);
}

#[test]
fn test_awk_array_arguments_are_passed_by_reference() {
    test_awk!(array_arguments_are_passed_by_reference);
}

#[test]
fn test_awk_call_function_with_less_arguments() {
    test_awk!(call_function_with_less_arguments);
}

#[test]
fn test_awk_recursive_function() {
    test_awk!(recursive_function);
}

#[test]
fn test_awk_mutually_recursive_functions() {
    test_awk!(mutually_recursive_functions);
}

#[test]
fn test_awk_empty_print_prints_the_whole_record() {
    test_awk!(
        empty_print_prints_the_whole_record,
        "tests/awk/test_data.txt"
    );
}

#[test]
fn test_awk_ere_pattern() {
    test_awk!(ere_pattern, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_ere_outside_match_matches_record() {
    test_awk!(ere_outside_match_matches_record, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_simple_getline() {
    test_awk!(simple_getline, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_getline_into_var() {
    test_awk!(getline_into_var, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_getline_from_file() {
    test_awk!(getline_from_file, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_read_records_from_stdin() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec![
            "-f".to_string(),
            "tests/awk/read_records_from_stdin.awk".to_string(),
            "-".to_string(),
        ],
        stdin_data: String::from(include_str!("awk/test_data.txt")),
        expected_out: String::from(include_str!("awk/read_records_from_stdin.out")),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

#[test]
fn test_awk_cli_variable_assignment() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec![
            "-f".to_string(),
            "tests/awk/cli_variable_assignment.awk".to_string(),
            "-v".to_string(),
            "variable=value1".to_string(),
            "-v".to_string(),
            "_variable=val\\nue2".to_string(),
            "-v".to_string(),
            "v2ar4_iable=\"value3\"".to_string(),
        ],
        stdin_data: String::new(),
        expected_out: String::from(include_str!("awk/cli_variable_assignment.out")),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

#[test]
fn test_awk_variable_assignment_arguments() {
    test_awk!(variable_assignment_arguments, "tests/awk/test_data.txt");
}

// gawk 5.4's output: `+"nan"` is 0, as only a signed `+nan` or `-nan` is NaN in gawk.
#[test]
fn test_awk_correct_comparisons() {
    test_awk!(correct_comparisons, "tests/awk/test_data.txt");
}

#[test]
fn test_awk_execute_program_from_args() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec!["BEGIN { print \"Hello, World!\" }".to_string()],
        stdin_data: String::new(),
        expected_out: String::from("Hello, World!\n"),
        expected_err: String::from(""),
        expected_exit_code: 0,
    })
}

#[test]
fn test_awk_use_cli_provided_separator() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec![
            "-F".to_string(),
            ",".to_string(),
            "-f".to_string(),
            "tests/awk/use_cli_provided_separator.awk".to_string(),
            "tests/awk/test_data.csv".to_string(),
        ],
        stdin_data: String::new(),
        expected_out: String::from(include_str!("awk/use_cli_provided_separator.out")),
        expected_err: String::from(""),
        expected_exit_code: 0,
    })
}

#[test]
fn test_awk_no_file_arguments_reads_from_stdin() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec![
            "-f".to_string(),
            "tests/awk/no_file_arguments_reads_from_stdin.awk".to_string(),
        ],
        stdin_data: include_str!("awk/test_data.txt").to_string(),
        expected_out: include_str!("awk/no_file_arguments_reads_from_stdin.out").to_string(),
        expected_err: String::from(""),
        expected_exit_code: 0,
    })
}

#[test]
fn test_awk_multifile_program() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec![
            "-f".to_string(),
            "tests/awk/multifile_program1.awk".to_string(),
            "-f".to_string(),
            "tests/awk/multifile_program2.awk".to_string(),
        ],
        stdin_data: String::new(),
        expected_out: String::from(include_str!("awk/multifile_program.out")),
        expected_err: String::from(""),
        expected_exit_code: 0,
    })
}

#[test]
fn test_awk_modifying_nf_recomputes_the_record() {
    test_awk!(
        modifying_nf_recomputes_the_record,
        "tests/awk/test_data.txt"
    );
}

// Bug fix regression tests

#[test]
fn test_awk_bugfix_atan2() {
    test_awk!(bugfix_atan2);
}

#[test]
fn test_awk_bugfix_printf_c() {
    test_awk!(bugfix_printf_c);
}

#[test]
fn test_awk_bugfix_string_as_regex() {
    test_awk!(bugfix_string_as_regex);
}

#[test]
fn test_awk_bugfix_escaped_backslash() {
    test_awk!(bugfix_escaped_backslash);
}

#[test]
fn test_awk_bugfix_line_continuation() {
    test_awk!(bugfix_line_continuation);
}

#[test]
fn test_awk_bugfix_field_numeric() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec![
            "-f".to_string(),
            "tests/awk/bugfix_field_numeric.awk".to_string(),
        ],
        stdin_data: String::from("a 5\nb 30\nc 10\n"),
        expected_out: String::from(include_str!("awk/bugfix_field_numeric.out")),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

#[test]
fn test_awk_bugfix_fs_empty() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec![
            "-f".to_string(),
            "tests/awk/bugfix_fs_empty.awk".to_string(),
        ],
        stdin_data: String::from("abc\nhi\n"),
        expected_out: String::from(include_str!("awk/bugfix_fs_empty.out")),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

#[test]
fn test_awk_bugfix_field_max_index() {
    // Generate a record with exactly 1024 fields (MAX_FIELDS) to verify
    // that accessing $1024 works after the off-by-one fix (0..MAX_FIELDS -> 0..=MAX_FIELDS)
    let fields: Vec<String> = (1..=1024).map(|i| i.to_string()).collect();
    let input = fields.join(" ") + "\n";
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec!["{ print $1024 }".to_string()],
        stdin_data: input,
        expected_out: String::from("1024\n"),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

#[test]
fn test_awk_bugfix_trailing_newline_program() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec!["BEGIN { print \"ok\" }\n".to_string()],
        stdin_data: String::new(),
        expected_out: String::from("ok\n"),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

#[test]
fn test_awk_bugfix_multichar_rs() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec![
            "-v".to_string(),
            "RS=::".to_string(),
            "{ print NR, $0 }".to_string(),
        ],
        stdin_data: String::from("one::two::three"),
        expected_out: String::from("1 one\n2 two\n3 three\n"),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

// POSIX: when RS is null (paragraph mode), newline is always a field separator
// regardless of FS value. Test with single-char FS.
#[test]
fn test_awk_paragraph_mode_newline_is_field_separator_char_fs() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec!["BEGIN{RS=\"\"; FS=\":\"} {for(i=1;i<=NF;i++) print i, $i}".to_string()],
        stdin_data: String::from("a:b\nc:d\n\ne:f\n"),
        expected_out: String::from("1 a\n2 b\n3 c\n4 d\n1 e\n2 f\n"),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

// POSIX: paragraph mode with ERE FS - newline is also a field separator.
#[test]
fn test_awk_paragraph_mode_newline_is_field_separator_ere_fs() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec!["BEGIN{RS=\"\"; FS=\":+\"} {for(i=1;i<=NF;i++) print i, $i}".to_string()],
        stdin_data: String::from("a::b\nc::d\n\ne::f\n"),
        expected_out: String::from("1 a\n2 b\n3 c\n4 d\n1 e\n2 f\n"),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

#[test]
fn test_awk_bugfix_nextfile_nr() {
    test_awk!(
        bugfix_nextfile_nr,
        "tests/awk/test_data.txt",
        "tests/awk/test_data2.txt"
    );
}

// fflush() with no args should flush all and return 0
#[test]
fn test_awk_fflush_no_args() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec!["BEGIN { print \"hello\"; ret = fflush(); print ret }".to_string()],
        stdin_data: String::new(),
        expected_out: String::from("hello\n0\n"),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

// fflush("") should flush all and return 0
#[test]
fn test_awk_fflush_empty_string() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec!["BEGIN { print \"hello\"; ret = fflush(\"\"); print ret }".to_string()],
        stdin_data: String::new(),
        expected_out: String::from("hello\n0\n"),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

// POSIX: paragraph mode with default FS - newline is whitespace, so it
// naturally acts as a field separator already.
#[test]
fn test_awk_paragraph_mode_default_fs() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec!["BEGIN{RS=\"\"} {for(i=1;i<=NF;i++) print i, $i}".to_string()],
        stdin_data: String::from("a b\nc d\n\ne f\n"),
        expected_out: String::from("1 a\n2 b\n3 c\n4 d\n1 e\n2 f\n"),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

// Regression: numeric strings from input should compare numerically (POSIX)
#[test]
fn test_awk_bugfix_numstr_field_cmp() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec![
            "-f".to_string(),
            "tests/awk/bugfix_numstr_field_cmp.awk".to_string(),
        ],
        stdin_data: String::from("03 3\n"),
        expected_out: String::from(include_str!("awk/bugfix_numstr_field_cmp.out")),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

// Regression: gsub with zero-width match must not panic on multi-byte UTF-8. The output is
// gawk 5.4's in a UTF-8 locale: no empty match where the one before ended.
#[test]
fn test_awk_bugfix_gsub_multibyte() {
    test_awk!(bugfix_gsub_multibyte);
}

// Regression: default SUBSEP must be \034 (0x1c), not space
#[test]
fn test_awk_bugfix_subsep_default() {
    test_awk!(bugfix_subsep_default);
}

// Regression: & in gsub replacement inserts matched text; \& inserts literal &
#[test]
fn test_awk_bugfix_gsub_ampersand() {
    test_awk!(bugfix_gsub_ampersand);
}

// Regression: system() must return the command exit status
#[test]
fn test_awk_bugfix_system_return() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec!["BEGIN { ret = system(\"true\"); print ret }".to_string()],
        stdin_data: String::new(),
        expected_out: String::from("0\n"),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

// Regression: NF = 0 must produce an empty record without panic
#[test]
fn test_awk_bugfix_nf_zero() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec!["-f".to_string(), "tests/awk/bugfix_nf_zero.awk".to_string()],
        stdin_data: String::from("a b c\n"),
        expected_out: String::from(include_str!("awk/bugfix_nf_zero.out")),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
}

// Regression: > redirect must truncate existing files
#[test]
fn test_awk_bugfix_redirect_truncate() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let outfile = dir.path().join("out.txt");
    // Write a long initial content
    std::fs::write(&outfile, "this is long initial content\n").unwrap();
    let program = format!(
        "BEGIN {{ print \"short\" > \"{}\" }}",
        outfile.to_str().unwrap().replace('\\', "/")
    );
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec![program],
        stdin_data: String::new(),
        expected_out: String::new(),
        expected_err: String::from(""),
        expected_exit_code: 0,
    });
    let contents = std::fs::read_to_string(&outfile).unwrap();
    assert_eq!(
        contents, "short\n",
        "file should be truncated on > redirect"
    );
}

// Regression: every clause of `for (init; condition; update)` may be empty. An empty
// clause used to panic the compiler (Pement's `tac` one-liner, `for (;;)`).
#[test]
fn test_awk_bugfix_for_with_empty_clauses() {
    for (program, stdin, expected) in [
        (
            "{a[i++]=$0} END {for (j=i-1; j>=0;) print a[j--]}",
            "a\nb\nc\n",
            "c\nb\na\n",
        ),
        (
            "BEGIN {for (;;) {n++; if (n > 3) break}; print n}",
            "",
            "4\n",
        ),
        (
            "BEGIN {for (i = 0;; i++) if (i == 2) break; print i}",
            "",
            "2\n",
        ),
        ("BEGIN {i = 5; for (; i < 7;) i++; print i}", "", "7\n"),
        ("BEGIN {for (;;) if (++k == 3) break; print k}", "", "3\n"),
    ] {
        run_test(TestPlan {
            cmd: String::from("awk"),
            args: vec![program.to_string()],
            stdin_data: String::from(stdin),
            expected_out: String::from(expected),
            expected_err: String::new(),
            expected_exit_code: 0,
        });
    }
}

// Line-ending policy. `run_test` normalizes CRLF, which would hide exactly
// what these tests check, so they compare stdout byte-for-byte.

fn run_awk_bytes(args: &[&str], stdin_data: &[u8], eol_lf: bool) -> Vec<u8> {
    let awk_bin = env!("CARGO_BIN_EXE_awk");
    let mut cmd = Command::new(awk_bin);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LC_ALL", "C");
    if eol_lf {
        cmd.env("CASH_EOL", "LF");
    } else {
        cmd.env_remove("CASH_EOL");
    }
    let mut child = cmd.spawn().expect("failed to spawn awk");
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(stdin_data);
    }
    let output = child.wait_with_output().expect("failed to wait on awk");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn assert_awk_bytes(args: &[&str], stdin_data: &[u8], expected: &[u8]) {
    let stdout = run_awk_bytes(args, stdin_data, false);
    assert_eq!(
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(expected)
    );
}

#[test]
fn test_eol_crlf_input_round_trips_through_print() {
    assert_awk_bytes(&["{print}"], b"one\r\ntwo\r\n", b"one\r\ntwo\r\n");
    assert_awk_bytes(&["1"], b"one\r\ntwo\r\n", b"one\r\ntwo\r\n");
}

#[test]
fn test_eol_lf_input_stays_lf() {
    assert_awk_bytes(&["{print}"], b"one\ntwo\n", b"one\ntwo\n");
}

#[test]
fn test_eol_mixed_input_follows_each_record() {
    assert_awk_bytes(&["{print}"], b"a\r\nb\nc\r\n", b"a\r\nb\nc\r\n");
}

#[test]
fn test_eol_fields_and_length_ignore_cr() {
    assert_awk_bytes(
        &["{print length($0), length($2), NF}"],
        b"a b\r\n",
        b"3 1 2\r\n",
    );
    assert_awk_bytes(
        &["{printf \"[%s][%s]\\n\", $1, $2}"],
        b"a b\r\n",
        b"[a][b]\n",
    );
    assert_awk_bytes(&["$2 == \"b\" {print \"yes\"}"], b"a b\r\n", b"yes\r\n");
}

#[test]
fn test_eol_printf_is_verbatim() {
    assert_awk_bytes(&["{printf \"%s\\n\", $0}"], b"one\r\n", b"one\n");
}

#[test]
fn test_eol_explicit_ors_is_verbatim() {
    assert_awk_bytes(&["BEGIN{ORS=\";\"} {print}"], b"a\r\nb\r\n", b"a;b;");
}

#[test]
fn test_eol_end_uses_last_record() {
    assert_awk_bytes(&["END{print NR}"], b"a\r\nb\r\n", b"2\r\n");
    assert_awk_bytes(&["END{print NR}"], b"a\r\nb\n", b"2\n");
    assert_awk_bytes(&["BEGIN{print \"x\"}"], b"", b"x\n");
}

#[test]
fn test_eol_redirected_print_restores_crlf() {
    let dir = std::env::temp_dir().join(format!("cash-awk-eol-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out.txt");
    let program = format!(
        "{{print > \"{}\"}}",
        out.display().to_string().replace('\\', "/")
    );
    assert_awk_bytes(&[&program], b"a\r\nb\r\n", b"");
    assert_eq!(std::fs::read(&out).unwrap(), b"a\r\nb\r\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_eol_dos2unix_one_liner() {
    assert_awk_bytes(&["{sub(/\\r$/,\"\")};1"], b"a\r\nb\r\n", b"a\nb\n");
}

#[test]
fn test_eol_unix2dos_one_liner() {
    assert_awk_bytes(&["{sub(/$/,\"\\r\")};1"], b"a\nb\n", b"a\r\nb\r\n");
}

#[test]
fn test_eol_octal_cr_makes_cr_data() {
    assert_awk_bytes(&["{x = \"\\015\"; print length($0)}"], b"ab\r\n", b"3\n");
    assert_awk_bytes(&["{gsub(\"\\015\",\"\")};1"], b"ab\r\n", b"ab\n");
}

#[test]
fn test_eol_escaped_backslash_r_is_not_cr() {
    // `\\r` is a backslash followed by `r`, so the default mode stays on.
    assert_awk_bytes(&["{print $0 \"\\\\r\"}"], b"ab\r\n", b"ab\\r\r\n");
}

#[test]
fn test_eol_cash_eol_lf_makes_cr_data() {
    assert_eq!(
        run_awk_bytes(&["{print length}"], b"ab\r\ncd\n", true),
        b"3\n2\n"
    );
    assert_eq!(
        run_awk_bytes(&["{print}"], b"ab\r\ncd\n", true),
        b"ab\r\ncd\n"
    );
    assert_eq!(run_awk_bytes(&["{print $2}"], b"a b\r\n", true), b"b\r\n");
}

// Regression: regex literals accept POSIX awk's escapes. Octal `\ddd` failed to compile,
// and `\b` meant a word boundary (the Rust regex meaning) instead of a backspace.
#[test]
fn test_awk_bugfix_regex_literal_escapes() {
    for (program, stdin, expected) in [
        (r#"/\101/ { print "octal" }"#, "A\n", "octal\n"),
        (r#"{ gsub(/\102/, "x"); print }"#, "ABC\n", "AxC\n"),
        (
            r#"/a\bb/ { print "backspace" }"#,
            "a\u{8}b\n",
            "backspace\n",
        ),
        (r#"/a\bb/ { print "boundary" }"#, "a b\n", ""),
        (r#"/a\/b/ { print "slash" }"#, "a/b\n", "slash\n"),
        (r#"/a\\b/ { print "backslash" }"#, "a\\b\n", "backslash\n"),
    ] {
        run_test(TestPlan {
            cmd: String::from("awk"),
            args: vec![program.to_string()],
            stdin_data: String::from(stdin),
            expected_out: String::from(expected),
            expected_err: String::new(),
            expected_exit_code: 0,
        });
    }
}

fn plan(program: &str, stdin: &str, expected_out: &str, expected_exit_code: i32) -> TestPlan {
    TestPlan {
        cmd: String::from("awk"),
        args: vec![program.to_string()],
        stdin_data: String::from(stdin),
        expected_out: String::from(expected_out),
        expected_err: String::new(),
        expected_exit_code,
    }
}

// A target that cannot be assigned to, for `sub`, `gsub` or the right side of `in`, is a
// compile error (gawk's words for `sub`); it panicked (TODO.md 14.6).
#[test]
fn test_awk_unassignable_targets_are_errors() {
    for (program, message) in [
        (
            r#"BEGIN { gsub(/a/, "b", length) }"#,
            "gsub third parameter is not a changeable object",
        ),
        (
            r#"BEGIN { sub(/a/, "b", substr("x", 1)) }"#,
            "sub third parameter is not a changeable object",
        ),
        // gawk's syntax error, under the right side.
        (
            "BEGIN { print 1 in 2 }",
            "awk: cmd. line:1: BEGIN { print 1 in 2 }\n\
             awk: cmd. line:1:                    ^ syntax error",
        ),
        (
            r#"BEGIN { print 1 in "x" }"#,
            "awk: cmd. line:1:                    ^ syntax error",
        ),
    ] {
        run_test_with_checker(plan(program, "", "", 1), |_, output| {
            assert_eq!(output.status.code(), Some(1), "{program}");
            assert!(output.stdout.is_empty(), "{program}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains(message), "{program}: {stderr}");
        });
    }
}

// An empty statement as the body of `if`, `for` or `do` runs as in gawk; each panicked
// (TODO.md 14.6).
#[test]
fn test_awk_empty_statement_bodies() {
    for (program, expected) in [
        ("BEGIN { for (i = 0; i < 2; i++); print i }", "2\n"),
        ("BEGIN { for (;;) break; print \"t\" }", "t\n"),
        ("BEGIN { if (1); print \"x\" }", "x\n"),
        ("BEGIN { if (1)\n; print \"x\" }", "x\n"),
        ("BEGIN { do ; while (0); print \"z\" }", "z\n"),
        ("BEGIN { i = 0; do ; while (i++ < 3); print i }", "4\n"),
    ] {
        run_test(plan(program, "", expected, 0));
    }
}

// `exit`, `next` and `nextfile` in a function a pattern calls end the pattern as they
// would an action, as in gawk; they panicked (TODO.md 14.6).
#[test]
fn test_awk_exit_and_next_from_a_pattern() {
    for (program, expected, status) in [
        ("function f() { exit 3 } f() { print }", "", 3),
        (
            "function f() { exit 3 } f() { print } END { print \"end\" }",
            "end\n",
            3,
        ),
        ("function f() { exit 3 } NR == 1, f() { print }", "", 3),
        (
            "function f() { next } f() { print \"no\" } { print \"rule 2\" }",
            "",
            0,
        ),
        (
            "function f() { if ($0 == \"b\") next; return 1 } f() { print }",
            "a\nc\n",
            0,
        ),
        ("function f() { nextfile } f() { print }", "", 0),
    ] {
        run_test(plan(program, "a\nb\nc\n", expected, status));
    }
}

// The width and precision of `%s` and `%c` count characters, as `length` and `substr`
// do and as gawk does in a UTF-8 locale; they counted bytes, so `%.1s` of "é" printed
// nothing and `%3s` of "é" one space (TODO.md 14.6).
#[test]
fn test_awk_string_width_and_precision_count_characters() {
    for (program, expected) in [
        (
            r#"BEGIN { printf "[%.1s][%.2s][%.3s]\n", "é", "éa", "éa" }"#,
            "[é][éa][éa]\n",
        ),
        (
            r#"BEGIN { printf "[%3s][%.1s]\n", "é", "éa" }"#,
            "[  é][é]\n",
        ),
        (
            r#"BEGIN { printf "[%-4s][%5.1s][%-3.2s]\n", "éé", "éa", "aéb" }"#,
            "[éé  ][    é][aé ]\n",
        ),
        (
            r#"BEGIN { printf "[%3c][%-3c][%c]\n", "é", "éa", "€x" }"#,
            "[  é][é  ][€]\n",
        ),
        (
            r#"BEGIN { s = sprintf("%3s", "é"); print length(s) }"#,
            "3\n",
        ),
    ] {
        run_test(plan(program, "", expected, 0));
    }
}

/// Runs awk on `program` with no input, killing it should it run past a bound, so that a
/// hang fails the test rather than stalling the run.
fn run_bounded(program: &str) -> Output {
    use std::time::{Duration, Instant};
    let mut child = Command::new(env!("CARGO_BIN_EXE_awk"))
        .arg(program)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LC_ALL", "C")
        .spawn()
        .expect("failed to spawn awk");
    // Under nextest's own bound for a test (15 seconds), so that this one reports first.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().expect("failed to poll awk").is_some() {
            return child.wait_with_output().expect("failed to wait on awk");
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("awk did not finish {program:?} in time");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

// A syntax error is reported as gawk reports it, the first only, and ends the run: the
// search for further errors made no progress where the rest of the program started with
// `BEGIN`, `END` or `function`, and ran with memory growing without end (TODO.md 14.6).
// Each was checked against gawk 5.4.
#[test]
fn test_awk_syntax_errors_before_begin_end_or_function_end_the_run() {
    for (program, stderr) in [
        (
            "BEGIN { ( } BEGIN { ( }",
            "awk: cmd. line:1: BEGIN { ( } BEGIN { ( }\n\
             awk: cmd. line:1:           ^ syntax error\n",
        ),
        (
            "function ( function (",
            "awk: cmd. line:1: function ( function (\n\
             awk: cmd. line:1:          ^ syntax error\n",
        ),
        (
            "BEGIN { x = ( }\nBEGIN { y = 1 }\nEND { z = ( }",
            "awk: cmd. line:1: BEGIN { x = ( }\n\
             awk: cmd. line:1:               ^ syntax error\n",
        ),
        (
            "BEGIN { ( } END { ( } function ( BEGIN",
            "awk: cmd. line:1: BEGIN { ( } END { ( } function ( BEGIN\n\
             awk: cmd. line:1:           ^ syntax error\n",
        ),
        // gawk names the character by its first byte alone, which is no UTF-8.
        (
            "BEGINé{ ( }",
            "awk: cmd. line:1: BEGINé{ ( }\n\
             awk: cmd. line:1:      ^ invalid char '\u{FFFD}' in expression\n",
        ),
    ] {
        let output = run_bounded(program);
        assert_eq!(output.status.code(), Some(1), "{program}");
        assert!(output.stdout.is_empty(), "{program}");
        assert_eq!(String::from_utf8_lossy(&output.stderr), stderr, "{program}");
    }
}

// An empty statement is a body of its own: in `if (c); else s` the `else` branch is not
// the `if`'s body, which ran `s` when `c` was true (TODO.md 14.6). The expected output is
// gawk 5.4's.
#[test]
fn test_awk_empty_statement_before_else() {
    for (program, expected) in [
        (r#"BEGIN { if (1); else print "y" }"#, ""),
        (r#"BEGIN { if (0); else print "n" }"#, "n\n"),
        (
            r#"BEGIN { if (1) ; else print "y"; print "after" }"#,
            "after\n",
        ),
        ("BEGIN { if (1)\n;\nelse\nprint \"y\"\nprint \"z\" }", "z\n"),
        (
            r#"BEGIN { if (1) if (0); else print "inner else" }"#,
            "inner else\n",
        ),
        (
            r#"BEGIN { if (0) if (1); else print "x"; print "a" }"#,
            "a\n",
        ),
        (r#"BEGIN { if (0); else { print "b" } }"#, "b\n"),
        ("BEGIN { while (i++ < 3); print i }", "4\n"),
        ("BEGIN { a[1]; for (k in a); print k }", "1\n"),
    ] {
        run_test(plan(program, "", expected, 0));
    }
}

// `next` and `nextfile` have no record to go on from in BEGIN or END: written there they
// are compile errors, and run there by a function they are fatal, both in gawk's words.
// `BEGIN` and `END` are not variables, so `END END END` is a syntax error. Each ran
// silently with status 0 (TODO.md 14.6). A compile error exits 1 and a fatal one 2, as in
// gawk; the fatal ones exited 1.
#[test]
fn test_awk_next_in_begin_or_end_is_an_error() {
    for (program, message, status) in [
        ("BEGIN { next }", "`next' used in BEGIN action", 1),
        ("END { nextfile }", "`nextfile' used in END action", 1),
        (
            "BEGIN { if (1) { next } }",
            "`next' used in BEGIN action",
            1,
        ),
        (
            "function f() { next } BEGIN { f() }",
            "`next' cannot be called from a `BEGIN' rule",
            2,
        ),
        (
            "function f() { nextfile } END { f() }",
            "`nextfile' cannot be called from a `END' rule",
            2,
        ),
        (
            "END END END",
            "awk: cmd. line:1: END END END\nawk: cmd. line:1:     ^ syntax error",
            1,
        ),
        (
            "END",
            "awk: cmd. line:1: END blocks must have an action part",
            1,
        ),
        (
            "BEGIN",
            "awk: cmd. line:1: BEGIN blocks must have an action part",
            1,
        ),
    ] {
        run_test_with_checker(plan(program, "", "", status), |_, output| {
            assert_eq!(output.status.code(), Some(status), "{program}");
            assert!(output.stdout.is_empty(), "{program}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains(message), "{program}: {stderr}");
        });
    }
    // In a function that a rule calls, or in a rule, they are as before.
    run_test(plan(
        "function f() { next } { f(); print }\nEND { print NR }",
        "a\nb\n",
        "2\n",
        0,
    ));
}

// `begin`, `end` and `foreach` are plain names, as in gawk, where cash reserved them;
// `delete` and `nextfile` are reserved, where `delete=1` was taken (TODO.md 14.6).
#[test]
fn test_awk_reserved_words_are_gawks() {
    run_test(plan(
        "BEGIN { begin = 1; end = 2; foreach = 3; print begin, end, foreach }",
        "",
        "1 2 3\n",
        0,
    ));
    for program in [
        "BEGIN { delete = 1 }",
        "BEGIN { x = delete }",
        "BEGIN { nextfile = 1 }",
        "BEGIN { x = nextfile }",
    ] {
        let output = run_bounded(program);
        assert_eq!(output.status.code(), Some(1), "{program}");
        assert!(output.stdout.is_empty(), "{program}");
    }
}

// A precision does not cut `%c`'s one character, as in gawk: `%.0c` printed nothing
// (TODO.md 14.6).
#[test]
fn test_awk_char_conversion_ignores_a_precision() {
    run_test(plan(
        r#"BEGIN { printf "[%.0c][%.3c][%5.0c][%-3.0c]\n", "abc", "xyz", 65, "q" }"#,
        "",
        "[a][x][    A][q  ]\n",
        0,
    ));
}

// A fatal error exits 2, as in gawk; it exited 1 (TODO.md 14.6).
#[test]
fn test_awk_a_fatal_error_exits_2() {
    run_test_with_checker(plan("BEGIN { x = 1; x[1] = 2 }", "", "", 2), |_, output| {
        assert_eq!(output.status.code(), Some(2));
        assert!(!output.stderr.is_empty(), "{:?}", output.stderr);
    });
}

// A `%s` in CONVFMT or OFMT converts the number with `%.6g`: it was converted with the
// format itself, which recursed until the stack overflowed (TODO.md 14.6, found looking
// for panics). gawk prints 1.5 for OFMT and crashes for CONVFMT.
#[test]
fn test_awk_a_string_conversion_in_convfmt_or_ofmt() {
    run_test(plan(
        r#"BEGIN { OFMT = "%s"; print 1.5; CONVFMT = "%s"; x = 2.5 ""; print x; CONVFMT = "<%s>"; print 3.5 "" }"#,
        "",
        "1.5\n2.5\n<3.5>\n",
        0,
    ));
}

// `func` is gawk's other spelling of `function`, and so reserved (TODO.md 14.6).
#[test]
fn test_awk_func_defines_a_function() {
    run_test(plan(
        "func f(x) { return x * 2 }\nBEGIN { print f(21) }",
        "",
        "42\n",
        0,
    ));
    run_test(plan(
        "func f(x) { return x * 2 } BEGIN { func_x = f(2); print func_x }",
        "",
        "4\n",
        0,
    ));
    let output = run_bounded("BEGIN { func = 1 }");
    assert_eq!(output.status.code(), Some(1));
}

/// A plan that expects the fatal error `stderr`, exit status 2 and no output.
fn fatal_plan(args: &[&str], stdin: &str, stderr: &str) -> TestPlan {
    TestPlan {
        cmd: String::from("awk"),
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
        stdin_data: String::from(stdin),
        expected_out: String::new(),
        expected_err: format!("{stderr}\n"),
        expected_exit_code: 2,
    }
}

// A fatal error is reported as gawk reports it, in its words, with the variable's name:
// cash wrote "runtime error: scalar used in array context" and its call stack (TODO.md
// 14.6). Each was checked against gawk 5: the program, its input and the error.
const FATAL_ERRORS: &[(&str, &str, &str)] = &[
    (
        "BEGIN { x = 1; x[1] = 2 }",
        "",
        "awk: cmd. line:1: fatal: attempt to use scalar `x' as an array",
    ),
    (
        "BEGIN { x = 1; print x[1] }",
        "",
        "awk: cmd. line:1: fatal: attempt to use scalar `x' as an array",
    ),
    (
        "BEGIN { x = 1; for (k in x) print k }",
        "",
        "awk: cmd. line:1: fatal: attempt to use scalar `x' as an array",
    ),
    (
        "BEGIN { x = 1; delete x[1] }",
        "",
        "awk: cmd. line:1: fatal: attempt to use scalar `x' as an array",
    ),
    (
        "BEGIN { x = 1; delete x }",
        "",
        "awk: cmd. line:1: fatal: attempt to use scalar `x' as an array",
    ),
    (
        "BEGIN { x = 1; if (1 in x) print 1 }",
        "",
        "awk: cmd. line:1: fatal: attempt to use scalar `x' as an array",
    ),
    (
        "BEGIN { SUBSEP[1] = 2 }",
        "",
        "awk: cmd. line:1: fatal: attempt to use scalar `SUBSEP' as an array",
    ),
    (
        "function f(p) { p[1] = 1 } BEGIN { x = 1; f(x) }",
        "",
        "awk: cmd. line:1: fatal: attempt to use scalar parameter `p' as an array",
    ),
    (
        "BEGIN { x = 1; split(\"a b\", x) }",
        "",
        "awk: cmd. line:1: fatal: split: second argument is not an array",
    ),
    (
        "BEGIN { a[1] = 1; print a }",
        "",
        "awk: cmd. line:1: fatal: attempt to use array `a' in a scalar context",
    ),
    (
        "BEGIN { a[1] = 1; a = 2 }",
        "",
        "awk: cmd. line:1: fatal: attempt to use array `a' in a scalar context",
    ),
    (
        "BEGIN { a[1]; a++ }",
        "",
        "awk: cmd. line:1: fatal: attempt to use array `a' in a scalar context",
    ),
    (
        "BEGIN { a[1]; getline a < \"/dev/null\" }",
        "",
        "awk: cmd. line:1: fatal: attempt to use array `a' in a scalar context",
    ),
    (
        "function f(p) { return p + 1 } BEGIN { a[1]; f(a) }",
        "",
        "awk: cmd. line:1: fatal: attempt to use array `p (from a)' in a scalar context",
    ),
    // The line, and the input once a record is read.
    (
        "BEGIN { f(1); z = 1\n z[1] = 1 }\nfunction f(a) { return a }",
        "",
        "awk: cmd. line:2: fatal: attempt to use scalar `z' as an array",
    ),
    (
        "NR == 2 { x = 1; x[1] = 2 }",
        "a\nb\n",
        "awk: cmd. line:1: (FILENAME=- FNR=2) fatal: attempt to use scalar `x' as an array",
    ),
    (
        "BEGIN { print $-1 }",
        "",
        "awk: cmd. line:1: fatal: attempt to access field -1",
    ),
    (
        "BEGIN { x = -2; $x = 1 }",
        "",
        "awk: cmd. line:1: fatal: attempt to access field -2",
    ),
    (
        "{ NF = -1 }",
        "a b\n",
        "awk: cmd. line:1: (FILENAME=- FNR=1) fatal: NF set to negative value",
    ),
    (
        "BEGIN { printf \"%d %d\\n\", 1 }",
        "",
        "awk: cmd. line:1: fatal: not enough arguments to satisfy format string\n\
             \t`%d %d\n'\n\t    ^ ran out for this one",
    ),
    (
        "BEGIN { printf \"%5.*d\", 3 }",
        "",
        "awk: cmd. line:1: fatal: not enough arguments to satisfy format string\n\
             \t`%5.*d'\n\t    ^ ran out for this one",
    ),
    (
        "BEGIN { printf \"%*d\" }",
        "",
        "awk: cmd. line:1: fatal: not enough arguments to satisfy format string\n\
             \t`%*d'\n\t ^ ran out for this one",
    ),
    (
        "BEGIN { print \"x\" > \"/nonexistent/dir/x\" }",
        "",
        "awk: cmd. line:1: fatal: cannot redirect to `/nonexistent/dir/x': \
             No such file or directory",
    ),
];

#[test]
fn test_awk_fatal_errors_are_gawks() {
    for (program, stdin, stderr) in FATAL_ERRORS {
        run_test(fatal_plan(&[program], stdin, stderr));
    }
    run_test(fatal_plan(
        &["{ print }", "/nonexistent/file"],
        "",
        "awk: fatal: cannot open file `/nonexistent/file' for reading: No such file or directory",
    ));
}

// A program from a file is placed in it, as gawk places it.
#[test]
fn test_awk_fatal_error_in_a_program_file() {
    let dir = std::env::temp_dir().join(format!("cash-awk-fatal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create a directory");
    let program = dir.join("p.awk");
    std::fs::write(
        &program,
        "function f(p) {\n  p[1] = 2\n}\nBEGIN {\n  x = 1\n  f(x)\n}\n",
    )
    .expect("write the program");
    let program = program.to_string_lossy().into_owned();
    let output = run_test_base(&["-f".to_string(), program.clone()], b"");
    let _ = std::fs::remove_dir_all(&dir);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.replace("\r\n", "\n"),
        format!("awk: {program}:2: fatal: attempt to use scalar parameter `p' as an array\n")
    );
    assert_eq!(output.status.code(), Some(2));
}

// A format specifier that is no conversion is written out as it is, and size modifiers
// are passed over, as gawk does: each was a fatal error (TODO.md 14.6).
#[test]
fn test_awk_format_specifiers_as_gawk_reads_them() {
    run_test(plan(
        r#"BEGIN { printf "%z %d|%5z|%d|%ld|%hd|%Lf|%lld|a%-", 1, 2, 3, 4, 5 }"#,
        "",
        "%d|%5z|1|2|3|4.000000|%lld|a%-",
        0,
    ));
    run_test(plan(r#"BEGIN { printf "%q" }"#, "", "%q", 0));
    run_test(plan(r#"BEGIN { printf "a%5." }"#, "", "a%5.", 0));
}

/// Runs each `(program, stdin, stdout, stderr, status)` and checks all three outcomes.
fn run_cases(cases: &[(&str, &str, &str, &str, i32)]) {
    for (program, stdin, stdout, stderr, status) in cases {
        let output = run_test_base(&[(*program).to_string()], stdin.as_bytes());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
            *stdout,
            "{program}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
            *stderr,
            "{program}"
        );
        assert_eq!(output.status.code(), Some(*status), "{program}");
    }
}

// A division by zero is gawk's fatal error, in its words for each operator; the quotient
// was infinite or not a number, `+inf` or `-nan` (TODO.md phase 15). Each was checked
// against gawk 5.4.
#[test]
fn test_awk_division_by_zero_is_a_fatal_error() {
    run_cases(&[
        (
            "BEGIN { x = 0; print 1/x }",
            "",
            "",
            "awk: cmd. line:1: fatal: division by zero attempted\n",
            2,
        ),
        (
            "BEGIN { x = 0; print 1%x }",
            "",
            "",
            "awk: cmd. line:1: fatal: division by zero attempted in `%'\n",
            2,
        ),
        (
            "BEGIN { y = 1; y /= 0 }",
            "",
            "",
            "awk: cmd. line:1: fatal: division by zero attempted in `/='\n",
            2,
        ),
        (
            "BEGIN { x[1] = 3; x[1] %= 0 }",
            "",
            "",
            "awk: cmd. line:1: fatal: division by zero attempted in `%='\n",
            2,
        ),
        (
            "{ print 1/$1 }",
            "2\n0\n",
            "0.5\n",
            "awk: cmd. line:1: (FILENAME=- FNR=2) fatal: division by zero attempted\n",
            2,
        ),
        // Parenthesized, the zero is not a constant to gawk either.
        (
            "BEGIN { print 1/(0) }",
            "",
            "",
            "awk: cmd. line:1: fatal: division by zero attempted\n",
            2,
        ),
    ]);
}

// A divisor that is a zero constant is an error before the program runs, as gawk folds
// constants: exit status 1, every one reported, nothing run (TODO.md phase 15).
#[test]
fn test_awk_division_by_a_zero_constant_is_a_compile_error() {
    run_cases(&[
        (
            "BEGIN { print \"ran\"; print 1/0 }",
            "",
            "",
            "awk: cmd. line:1: error: division by zero attempted\n",
            1,
        ),
        (
            "BEGIN { print 1%0.0\n x = y / -0; z = 2 / 0^1 }",
            "",
            "",
            "awk: cmd. line:1: error: division by zero attempted in `%'\n\
             awk: cmd. line:2: error: division by zero attempted\n\
             awk: cmd. line:2: error: division by zero attempted\n",
            1,
        ),
        (
            "function f() { return 1/!1 }\nBEGIN { print 2 }",
            "",
            "",
            "awk: cmd. line:1: error: division by zero attempted\n",
            1,
        ),
    ]);
}

// A builtin given too few or too many arguments is gawk's error, with the source line
// and a caret under the call's closing parenthesis; `sprintf()` is its fatal error when
// it runs (TODO.md phase 15). cash said "incorrect number of arguments for builtin
// function" in pest's form.
#[test]
fn test_awk_builtin_argument_counts_are_gawks_errors() {
    run_cases(&[
        (
            "BEGIN { close() }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { close() }\n\
             awk: cmd. line:1:               ^ 0 is invalid as number of arguments for close\n",
            1,
        ),
        (
            "BEGIN {\n  x = substr(\"a\")\n}",
            "",
            "",
            "awk: cmd. line:2:   x = substr(\"a\")\n\
             awk: cmd. line:2:                 ^ 1 is invalid as number of arguments for \
             substr\n",
            1,
        ),
        (
            "BEGIN { cos(1, 2) }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { cos(1, 2) }\n\
             awk: cmd. line:1:                 ^ 2 is invalid as number of arguments for cos\n",
            1,
        ),
        (
            "BEGIN { isarray() }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { isarray() }\n\
             awk: cmd. line:1:                 ^ 0 is invalid as number of arguments for \
             isarray\n",
            1,
        ),
        (
            "BEGIN { print \"a\"; x = sprintf() }",
            "",
            "a\n",
            "awk: cmd. line:1: fatal: sprintf: no arguments\n",
            2,
        ),
        (
            "function f() { sprintf() } BEGIN { print \"a\" }",
            "",
            "a\n",
            "",
            0,
        ),
    ]);
}

// gawk's arrays of arrays, against gawk 5.4's output (TODO.md phase 15): they were not
// supported.
#[test]
fn test_awk_arrays_of_arrays() {
    test_awk!(arrays_of_arrays);
}

// What is wrong with a subarray is gawk's error, the element named as gawk names it.
#[test]
fn test_awk_subarray_errors_are_gawks() {
    for (program, stderr) in [
        (
            "BEGIN { a[1][2] = 3; print a[1] }",
            "awk: cmd. line:1: fatal: attempt to use array `a[\"1\"]' in a scalar context",
        ),
        (
            "BEGIN { a[1] = 1; a[1][2] = 3 }",
            "awk: cmd. line:1: fatal: attempt to use scalar `a[\"1\"]' as an array",
        ),
        (
            "BEGIN { a[1][2] = 1; x = a[1][2][3] }",
            "awk: cmd. line:1: fatal: attempt to use scalar `a[\"1\"][\"2\"]' as an array",
        ),
        (
            "BEGIN { a[1] = 3; for (k in a[1]) print k }",
            "awk: cmd. line:1: fatal: attempt to use a scalar value as array",
        ),
        (
            "BEGIN { a[1] = 3; print (2 in a[1]) }",
            "awk: cmd. line:1: fatal: attempt to use a scalar value as array",
        ),
        (
            "BEGIN { a[1] = 1; split(\"a b\", a[1]) }",
            "awk: cmd. line:1: fatal: split: second argument is not an array",
        ),
        (
            "function f(s) { return s } BEGIN { a[1][2] = 3; f(a[1]) }",
            "awk: cmd. line:1: fatal: attempt to use array `s (from a[\"1\"])' in a scalar \
             context",
        ),
        (
            "function f(s) { g(s) } function g(t) { return t + 1 } \
             BEGIN { a[1][1] = 5; f(a[1]) }",
            "awk: cmd. line:1: fatal: attempt to use array `t (from s, from a[\"1\"])' in a \
             scalar context",
        ),
        (
            "function f(s) { s[1] = 5; s[1][2] = 3 } BEGIN { f(a[0]) }",
            "awk: cmd. line:1: fatal: attempt to use scalar `a[\"0\"][\"1\"]' as an array",
        ),
        (
            "function f(s, l) { l[1][2] = 3; x = l[1] } BEGIN { f() }",
            "awk: cmd. line:1: fatal: attempt to use array `l[\"1\"]' in a scalar context",
        ),
        (
            "function f(s) { s[2] = 1 } BEGIN { a[1] = 5; f(a[1]) }",
            "awk: cmd. line:1: fatal: attempt to use scalar parameter `s' as an array",
        ),
        // A scalar assigned to a parameter linked to the caller's variable makes the
        // caller's a scalar, as in gawk.
        (
            "function f(s) { s = 4 } BEGIN { f(x); x[1] = 3 }",
            "awk: cmd. line:1: fatal: attempt to use scalar `x' as an array",
        ),
    ] {
        run_test(fatal_plan(&[program], "", stderr));
    }
    // An array where a pattern's truth value belongs; its copy was taken for one, which
    // panicked.
    run_test(fatal_plan(
        &["BEGIN { a[1] = 1 } a"],
        "x\n",
        "awk: cmd. line:1: (FILENAME=- FNR=1) fatal: attempt to use array `a' in a scalar \
         context",
    ));
}

// `for (a in b)` gives `a` a key only when `b` has one, so `a` may be an array for an
// empty `b`, as in gawk; it was always an error (TODO.md phase 15).
#[test]
fn test_awk_for_in_assigns_its_variable_only_for_a_key() {
    run_cases(&[
        (
            "BEGIN { a[1]; for (a in b) print \"x\"; print length(a) }",
            "",
            "1\n",
            "",
            0,
        ),
        (
            "BEGIN { for (b in b) print \"x\"; print \"done\" }",
            "",
            "done\n",
            "",
            0,
        ),
        (
            "BEGIN { a[1]; b[1]; for (a in b) print \"x\" }",
            "",
            "",
            "awk: cmd. line:1: fatal: attempt to use array `a' in a scalar context\n",
            2,
        ),
        (
            "function f(p) { for (p in b) print \"x\"; print \"done\" } BEGIN { a[1]; f(a) }",
            "",
            "done\n",
            "",
            0,
        ),
        // A variable with no type yet becomes a scalar all the same.
        (
            "BEGIN { for (a in b) ; a[2] = 1 }",
            "",
            "",
            "awk: cmd. line:1: fatal: attempt to use scalar `a' as an array\n",
            2,
        ),
    ]);
}

// An error met outside the program's code is placed at the code that ran last, as gawk
// places it, and bare only before any has run; it was always bare (TODO.md phase 15).
#[test]
fn test_awk_errors_outside_the_code_are_placed_as_gawk_places_them() {
    run_cases(&[
        (
            "function f() {\n next\n}\nBEGIN {\n f()\n}",
            "",
            "",
            "awk: cmd. line:2: fatal: `next' cannot be called from a `BEGIN' rule\n",
            2,
        ),
        (
            "function f() { nextfile } END { f() }",
            "",
            "",
            "awk: cmd. line:1: fatal: `nextfile' cannot be called from a `END' rule\n",
            2,
        ),
        (
            "BEGIN { print \"\" > \"\" }",
            "",
            "",
            "awk: cmd. line:1: fatal: expression for `>' redirection has null string value\n",
            2,
        ),
        (
            "BEGIN { \"\" | getline }",
            "",
            "",
            "awk: cmd. line:1: fatal: expression for `|' redirection has null string value\n",
            2,
        ),
    ]);
    let cases: &[(&[&str], &str, &str, &str, i32)] = &[
        (
            &["BEGIN { x = 1\n y = 2 } { print }", "/nonexistent/file"],
            "",
            "",
            "awk: cmd. line:2: fatal: cannot open file `/nonexistent/file' for reading: No \
             such file or directory\n",
            2,
        ),
        // Without the last file's place: FNR starts over before the next file is opened.
        (
            &["{ print }", "-", "/nonexistent/file"],
            "a\n",
            "a\n",
            "awk: cmd. line:1: fatal: cannot open file `/nonexistent/file' for reading: No \
             such file or directory\n",
            2,
        ),
        // gawk passes over a directory; reading it was a fatal error.
        (
            &["BEGIN { x = 1 } { print }", ".", "-"],
            "a\n",
            "a\n",
            "awk: cmd. line:1: warning: command line argument `.' is a directory: skipped\n",
            0,
        ),
        (
            &["{ print }", "."],
            "a\n",
            "",
            "awk: warning: command line argument `.' is a directory: skipped\n",
            0,
        ),
        // A backslash at the end of an assignment's value stands for itself.
        (&["-v", "x=a\\", "BEGIN { print x }"], "", "a\\\n", "", 0),
        (&["{ print x }", "x=a\\", "-"], "l\n", "a\\\n", "", 0),
        (
            &["-f", "/nonexistent.awk"],
            "",
            "",
            "awk: fatal: cannot open source file `/nonexistent.awk' for reading: No such file \
             or directory\n",
            2,
        ),
        (
            &["-f", "."],
            "",
            "",
            "awk: .:1: error: cannot read source file `.': Is a directory\n",
            1,
        ),
    ];
    for (args, stdin, stdout, stderr, status) in cases {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        let output = run_test_base(&args, stdin.as_bytes());
        let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).replace("\r\n", "\n");
        assert_eq!(text(&output.stdout), *stdout, "{args:?}");
        assert_eq!(text(&output.stderr), *stderr, "{args:?}");
        assert_eq!(output.status.code(), Some(*status), "{args:?}");
    }
}

// When the reader of awk's output goes away, awk ends there in silence with 141, as
// SIGPIPE ends gawk and as cash's own tools end (spec D71); it said "write error: Broken
// pipe" and ended with 2 (TODO.md phase 15).
#[test]
fn test_awk_ends_with_141_in_silence_when_its_reader_goes() {
    use std::io::Read;
    let mut child = Command::new(env!("CARGO_BIN_EXE_awk"))
        .arg("BEGIN { while (1) print \"y\" }")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn awk");
    let mut stdout = child.stdout.take().expect("awk's output");
    let mut first = [0_u8; 2];
    stdout.read_exact(&mut first).expect("awk's first line");
    drop(stdout);
    let output = child.wait_with_output().expect("failed to wait on awk");
    assert_eq!(&first, b"y\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert_eq!(output.status.code(), Some(141));
}

// Output that cannot be written for another reason is gawk's warning when awk ends, and
// a status of 1; while it runs, gawk's fatal error (TODO.md phase 15).
#[test]
fn test_awk_unwritable_output_is_gawks_warning_or_error() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("read-only");
    std::fs::write(&path, "").expect("a file");
    for (program, stderr, status) in [
        (
            "BEGIN { print \"x\" }",
            "awk: warning: error writing standard output: ",
            1,
        ),
        (
            "BEGIN { print \"x\"; exit 3 }",
            "awk: warning: error writing standard output: ",
            3,
        ),
        (
            "BEGIN { print \"x\"; fflush() }",
            "awk: cmd. line:1: fatal: fflush: cannot flush standard output: ",
            2,
        ),
        (
            "BEGIN { while (1) print \"xxxxxxxxxx\" }",
            "awk: cmd. line:1: fatal: print to \"standard output\" failed: ",
            2,
        ),
    ] {
        let file = std::fs::File::open(&path).expect("the file, to read");
        let output = Command::new(env!("CARGO_BIN_EXE_awk"))
            .arg(program)
            .stdin(Stdio::null())
            .stdout(Stdio::from(file))
            .stderr(Stdio::piped())
            .output()
            .expect("failed to run awk");
        let text = String::from_utf8_lossy(&output.stderr);
        assert!(text.starts_with(stderr), "{program}: {text}");
        assert_eq!(text.lines().count(), 1, "{program}: {text}");
        assert_eq!(output.status.code(), Some(status), "{program}");
    }
}

// `for (k in a)` goes through the keys `a` had when the loop began, as gawk does: a key
// deleted in the loop is still visited; `delete a` in the loop ended it (TODO.md phase
// 15). gawk 5.4's output.
#[test]
fn test_awk_for_in_visits_the_keys_it_began_with() {
    run_cases(&[
        (
            "BEGIN { a[1]; a[2]; a[3]; for (k in a) { delete a; n++ }; print n, length(a) }",
            "",
            "3 0\n",
            "",
            0,
        ),
        (
            "BEGIN { a[1][2]; a[1][3]; for (k in a[1]) { delete a[1]; n++ }; print n, length(a) }",
            "",
            "2 0\n",
            "",
            0,
        ),
    ]);
}

// A parameter given a subarray that is then deleted keeps an array of its own, as in
// gawk: writing to it made the subarray again in the caller's array (TODO.md phase 15).
// gawk 5.4's output.
#[test]
fn test_awk_a_parameter_keeps_a_subarray_deleted_under_it() {
    run_cases(&[
        (
            "function f(s) { delete a; s[1] = 1 } BEGIN { a[0][1] = 7; f(a[0]); print length(a) }",
            "",
            "0\n",
            "",
            0,
        ),
        (
            "function f(s) { delete a[0]; print length(s); s[1] = 1; print length(s) } \
             BEGIN { a[0][1] = 7; a[0][2] = 8; f(a[0]); print length(a), (0 in a) }",
            "",
            "0\n1\n0 0\n",
            "",
            0,
        ),
        (
            "function f(s) { delete a; s[5] = 1; a[0][9] = 2; print length(s), length(a[0]) } \
             BEGIN { a[0][1] = 7; f(a[0]); print length(a[0]) }",
            "",
            "1 1\n1\n",
            "",
            0,
        ),
        (
            "function f(s) { delete a; s = 3 } BEGIN { a[0][1] = 7; f(a[0]) }",
            "",
            "",
            "awk: cmd. line:1: fatal: attempt to use array `s' in a scalar context\n",
            2,
        ),
    ]);
}

// What is wrong with a regex is gawk's error, in its words, fatal for a dynamic one and
// an error before the program runs for a literal one; cash said "error parsing pattern
// 0", in pest's form for a literal (TODO.md phase 15). A `)` with no `(` and a `{` that
// starts no interval are characters, as in gawk; each was an error.
#[test]
fn test_awk_regex_errors_are_gawks() {
    let dynamic = |pattern: &str| format!("BEGIN {{ r = \"{pattern}\"; print (\"a\" ~ r) }}");
    let literal = |pattern: &str| format!("BEGIN {{ print (\"a\" ~ /{pattern}/) }}");
    for (pattern, message) in [
        ("(", "unbalanced ("),
        ("[a", "unbalanced ["),
        ("a{1", "unbalanced {"),
        ("a{1,", "invalid contents of {}"),
        ("a{2,1}", "invalid contents of {}"),
        ("a{256}", "invalid contents of {}"),
        ("*a", "? * + or {interval} not preceded by valid subpattern"),
        (
            "a|+b",
            "? * + or {interval} not preceded by valid subpattern",
        ),
        ("[z-a]", "invalid range endpoint"),
        ("[[:foo:]]", "invalid character class name"),
        ("[[.ab.]]", "invalid collating element"),
    ] {
        run_cases(&[(
            dynamic(pattern).as_str(),
            "",
            "",
            format!("awk: cmd. line:1: fatal: invalid regexp: {message}: /{pattern}/\n").as_str(),
            2,
        )]);
        // An unclosed bracket in a literal is unterminated, as below.
        if pattern != "[a" {
            run_cases(&[(
                literal(pattern).as_str(),
                "",
                "",
                format!("awk: cmd. line:1: error: {message}: /{pattern}/\n").as_str(),
                1,
            )]);
        }
    }
    run_cases(&[
        (
            dynamic("a\\\\").as_str(),
            "",
            "",
            "awk: cmd. line:1: fatal: invalid regexp: invalid trailing backslash: /a\\/\n",
            2,
        ),
        // An unclosed bracket expression goes on to the end of the line, as in gawk's lexer.
        (
            "BEGIN { x = /[/ }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = /[/ }\n\
             awk: cmd. line:1:              ^ unterminated regexp\n",
            1,
        ),
        (
            "BEGIN { print (\"a)\" ~ /a)/), (\"a{\" ~ /a{/), (\"{,2}\" ~ \"{,2}\") }",
            "",
            "1 1 1\n",
            "",
            0,
        ),
        // An escape in a bracket expression is the character it stands for.
        (
            "BEGIN { print (\"a\\tb\" ~ /a[\\t]b/), (\"atb\" ~ /a[\\t]b/), (\"a\\tb\" ~ /a[\\11]b/) }",
            "",
            "1 0 1\n",
            "",
            0,
        ),
        // A `/` in a bracket expression is part of it; a regex can end in `\/`.
        (
            "BEGIN { print (\"a/b\" ~ /[/]/), (\"a/\" ~ /a\\//) }",
            "",
            "1 1\n",
            "",
            0,
        ),
        (
            "{ FS = \"((\"; print $1 }",
            "a\n",
            "",
            "awk: cmd. line:1: (FILENAME=- FNR=1) fatal: invalid regexp: unbalanced (: /((/\n",
            2,
        ),
    ]);
}

// A syntax error is gawk's, worded and placed as gawk places it, after the errors gawk
// meets before it, and the rest is not read (TODO.md phase 15). It was pest's, ` --> 1:9`
// and the rules it expected, with every further error. Each was checked against gawk 5.4.
#[test]
fn test_awk_syntax_errors_are_gawks() {
    run_cases(&[
        (
            "BEGIN { print 1/0 } {",
            "",
            "",
            "awk: cmd. line:1: error: division by zero attempted\n\
             awk: cmd. line:1: BEGIN { print 1/0 } {\n\
             awk: cmd. line:1:                      ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { print 1 / 0; x( }",
            "",
            "",
            "awk: cmd. line:1: error: division by zero attempted\n\
             awk: cmd. line:1: BEGIN { print 1 / 0; x( }\n\
             awk: cmd. line:1:                         ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 +\n }",
            "",
            "",
            "awk: cmd. line:2: BEGIN { x = 1 +\n\
             awk: cmd. line:2:                ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN {\n\tx = (\n}",
            "",
            "",
            "awk: cmd. line:3: \tx = (\nawk: cmd. line:3: \t     ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { if x }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { if x }\nawk: cmd. line:1:            ^ syntax error\n",
            1,
        ),
    ]);
}

// What gawk's lexer finds wrong is its error, at the start of the token: a string or a
// regex the line ends first, a character awk has no use for, a backslash with more on its
// line (TODO.md phase 15). Each was checked against gawk 5.4.
#[test]
fn test_awk_lexical_errors_are_gawks() {
    run_cases(&[
        (
            "BEGIN { print \"abc }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { print \"abc }\n\
             awk: cmd. line:1:               ^ unterminated string\n",
            1,
        ),
        (
            "BEGIN { x = /abc }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = /abc }\n\
             awk: cmd. line:1:              ^ unterminated regexp\n",
            1,
        ),
        (
            "BEGIN { x = 1 ` 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 ` 2 }\n\
             awk: cmd. line:1:               ^ invalid char '`' in expression\n",
            1,
        ),
        (
            "BEGIN { x = 1 \\ 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 \\ 2 }\n\
             awk: cmd. line:1:               ^ backslash not last character on line\n",
            1,
        ),
        (
            "BEGIN { x = 1 } BEGIN\n{ y }",
            "",
            "",
            "awk: cmd. line:2: BEGIN blocks must have an action part\n",
            1,
        ),
        (
            "function f(a) { } function length(b) { }",
            "",
            "",
            "awk: cmd. line:1: function f(a) { } function length(b) { }\n\
             awk: cmd. line:1:                            ^ `length' is a built-in function, \
             it cannot be redefined\n",
            1,
        ),
        (
            "BEGIN { return; x( }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { return; x( }\n\
             awk: cmd. line:1:         ^ `return' used outside function context\n",
            1,
        ),
        (
            "BEGIN { (1)++ }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { (1)++ }\nawk: cmd. line:1:               ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 < 2 < 3 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 < 2 < 3 }\n\
             awk: cmd. line:1:                   ^ syntax error\n",
            1,
        ),
    ]);
}

// The other errors of reading a program are gawk's too, in its words: cash wrote its own,
// in pest's form, and stopped at the first of them (TODO.md phase 15). Each was checked
// against gawk 5.4.
#[test]
fn test_awk_program_errors_are_gawks() {
    run_cases(&[
        // gawk goes on after these, and writes the first two twice.
        (
            "BEGIN { if (x) break; else continue; next }",
            "",
            "",
            "awk: cmd. line:1: error: `break' is not allowed outside a loop or switch\n\
             awk: cmd. line:1: error: `break' is not allowed outside a loop or switch\n\
             awk: cmd. line:1: error: `continue' is not allowed outside a loop\n\
             awk: cmd. line:1: error: `continue' is not allowed outside a loop\n\
             awk: cmd. line:1: error: `next' used in BEGIN action\n",
            1,
        ),
        (
            "function f(a, b, a, NR, f) { } function f() { }",
            "",
            "",
            "awk: cmd. line:1: error: function `f': parameter #3, `a', duplicates parameter #1\n\
             awk: cmd. line:1: error: function `f': parameter `NR': POSIX disallows using a \
             special variable as a function parameter\n\
             awk: cmd. line:1: error: function `f': cannot use function name as parameter name\n\
             awk: cmd. line:1: error: function name `f' previously defined\n",
            1,
        ),
        (
            "BEGIN { x = 1; x(); print 1/0 }",
            "",
            "",
            "awk: cmd. line:1: error: attempt to use non-function `x' in function call\n\
             awk: cmd. line:1: error: division by zero attempted\n",
            1,
        ),
        (
            "function f() { } BEGIN { f = 1 }",
            "",
            "",
            "awk: cmd. line:1: error: function `f' called with space between name and `(',\n\
             or used as a variable or an array\n",
            1,
        ),
        // A function not defined is found once the program is read, fatal, with status 2.
        (
            "BEGIN { g(); h() }",
            "",
            "",
            "awk: cmd. line:1: fatal: function `g' not defined\n",
            2,
        ),
        // More arguments than parameters is gawk's warning, and the program runs.
        (
            "function f(a) { return a }\nBEGIN { print f(1, 2) }",
            "",
            "1\n",
            "awk: cmd. line:2: warning: function `f' called with more arguments than declared\n",
            0,
        ),
        // Another function's name is a parameter like any other.
        (
            "function g() { } function f(g) { return g } BEGIN { print f(4) }",
            "",
            "4\n",
            "",
            0,
        ),
    ]);
}

// gawk's grammar: `- -x`, `**` and `**=`, and a comparison in `print` other than `>`,
// which is a redirection; each was a syntax error (TODO.md phase 15). gawk 5.4's output.
#[test]
fn test_awk_operators_as_gawk_parses_them() {
    run_cases(&[
        (
            "BEGIN { print - -3, - - 3, !-1, -!0 }",
            "",
            "3 3 0 -1\n",
            "",
            0,
        ),
        (
            "BEGIN { print 2 ** 3, 2 ** 3 ** 2, -2 ** 2; x = 3; x **= 2; print x }",
            "",
            "8 512 -4\n9\n",
            "",
            0,
        ),
        (
            "BEGIN { a = 1; b = 2; print a == b, a != b, a < b, a <= b, a >= b; \
             print a == b ? \"y\" : \"n\"; print 1 == 1 1 }",
            "",
            "0 1 1 1 0\nn\n0\n",
            "",
            0,
        ),
        (
            "BEGIN { a = 1; print a == 1 > \"/dev/stdout\" }",
            "",
            "1\n",
            "",
            0,
        ),
        (
            "BEGIN { print 1/- -0 }",
            "",
            "",
            "awk: cmd. line:1: error: division by zero attempted\n",
            1,
        ),
    ]);
}

// gawk's third argument of `match`, fourth of `split` and second of `close`; each was an
// error (TODO.md phase 15). gawk 5.4's output.
#[test]
fn test_awk_match_split_and_close_take_gawks_arguments() {
    run_cases(&[
        (
            "BEGIN { n = match(\"foobar\", /o+(b)(x)?/, m); \
             print n, length(m), m[0], m[0, \"start\"], m[0, \"length\"], \
             m[1], m[1, \"start\"], m[1, \"length\"], (2 in m) }",
            "",
            "2 6 oob 2 3 b 4 1 0\n",
            "",
            0,
        ),
        (
            "BEGIN { m[9] = 1; print match(\"a\", /z/, m), length(m) }",
            "",
            "0 0\n",
            "",
            0,
        ),
        (
            "BEGIN { x = 1; match(\"a\", /a/, x) }",
            "",
            "",
            "awk: cmd. line:1: fatal: match: third argument is not an array\n",
            2,
        ),
        (
            "BEGIN { n = split(\" a  b \", f, \" \", s); \
             print n, length(s), \"[\" s[0] \"]\", \"[\" s[1] \"]\", \"[\" s[2] \"]\" }",
            "",
            "2 3 [ ] [  ] [ ]\n",
            "",
            0,
        ),
        (
            "BEGIN { n = split(\"a1b22c\", f, /[0-9]+/, s); print n, length(s), s[1], s[2] }",
            "",
            "3 2 1 22\n",
            "",
            0,
        ),
        (
            "BEGIN { split(\"a b\", a, \" \", a) }",
            "",
            "",
            "awk: cmd. line:1: fatal: split: cannot use the same array for second and fourth \
             args\n",
            2,
        ),
        (
            "BEGIN { split(\"a b\", a, \" \", \"q\") }",
            "",
            "",
            "awk: cmd. line:1: fatal: split: fourth argument is not an array\n",
            2,
        ),
        (
            "BEGIN { print \"x\" | \"cat\"; print close(\"cat\", \"TO\"), close(\"none\", \"from\") }",
            "",
            "x\n0 -1\n",
            "",
            0,
        ),
        (
            "BEGIN { close(\"cat\", \"bogus\") }",
            "",
            "",
            "awk: cmd. line:1: fatal: close: second argument must be `to' or `from'\n",
            2,
        ),
    ]);
}

// gawk's warnings for an argument out of a math function's range: the result was given
// in silence (TODO.md phase 15). gawk 5.4's.
#[test]
fn test_awk_math_functions_warn_as_gawk_does() {
    run_cases(&[
        (
            "BEGIN { print log(-1), sqrt(-2.5), exp(1000), exp(-1000), log(0), (exp(-745) > 0) }",
            "",
            "-nan -nan +inf 0 -inf 1\n",
            "awk: cmd. line:1: warning: log: received negative argument -1\n\
             awk: cmd. line:1: warning: sqrt: received negative argument -2.5\n\
             awk: cmd. line:1: warning: exp: argument 1000 is out of range\n\
             awk: cmd. line:1: warning: exp: argument -1000 is out of range\n",
            0,
        ),
        (
            "{ print log($1) }",
            "-1e10\n",
            "-nan\n",
            "awk: cmd. line:1: (FILENAME=- FNR=1) warning: log: received negative argument \
             -1e+10\n",
            0,
        ),
    ]);
}

// gawk's special files: `/dev/null` is Windows' `NUL`, `/dev/stdout`, `/dev/stderr` and
// `/dev/fd/0` to `2` are the standard streams, open files to `close` and `fflush`, and `-`
// and `/dev/stdin` are standard input to `getline <`; each was a file that could not be
// found (TODO.md phase 15). `fflush` of a file did not flush it, and of a name not open is
// gawk's warning. gawk 5.4's output.
#[test]
fn test_awk_special_files_are_gawks() {
    run_cases(&[
        (
            "BEGIN { print \"x\" > \"/dev/null\"; printf \"e\\n\" > \"/dev/stderr\"; print \"o\" > \"/dev/fd/1\"; print \"e2\" > \"/dev/fd/2\"; print close(\"/dev/null\"), close(\"/dev/stderr\"), close(\"/dev/fd/1\"), close(\"/dev/fd/2\") }",
            "",
            "o\n0 0 0 0\n",
            "e\ne2\n",
            0,
        ),
        (
            "BEGIN { print \"x\" > \"/dev/fd/0\" }",
            "",
            "",
            "awk: cmd. line:1: fatal: print to \"/dev/fd/0\" failed: Bad file descriptor\n",
            2,
        ),
        (
            "BEGIN { while ((getline l < \"-\") > 0) print \"got\", l; print (getline m < \"/dev/null\") }",
            "a\nb\n",
            "got a\ngot b\n0\n",
            "",
            0,
        ),
        (
            "BEGIN { print \"x\" > \"/dev/null\"; print fflush(\"/dev/null\"), fflush(\"nope\") }",
            "",
            "0 -1\n",
            "awk: cmd. line:1: warning: fflush: `nope' is not an open file, pipe or co-process\n",
            0,
        ),
    ]);
}

// A keyword is one only when no name character follows it, as in gawk: `printx` was
// `print x`, and `deletex` and `getlinex` syntax errors. `printf` with nothing to print is
// gawk's fatal error; it printed a newline (TODO.md phase 15). gawk 5.4's output.
#[test]
fn test_awk_keywords_end_where_gawk_ends_them() {
    run_cases(&[
        (
            "BEGIN { printx = 1; deletex = 2; getlinex = 3; returnx = 4; inx = 5; print printx deletex getlinex returnx inx }",
            "",
            "12345\n",
            "",
            0,
        ),
        (
            "BEGIN { printf }",
            "",
            "",
            "awk: cmd. line:1: fatal: printf: no arguments\n",
            2,
        ),
        (
            "BEGIN { printf > \"/dev/stderr\" }",
            "",
            "",
            "awk: cmd. line:1: fatal: printf: no arguments\n",
            2,
        ),
    ]);
}

// A plain `getline` is an operand, so `getline x y` joins `getline x` to `y`, and the
// target of a redirection holds no comparison, as in gawk: the first was a syntax error,
// and `print > "x" > "y"` printed to the file `"x" > "y"` (TODO.md phase 15). gawk 5.4's.
#[test]
fn test_awk_getline_and_redirections_parse_as_gawks() {
    run_cases(&[
        (
            "BEGIN { getline x y; print x \"|\" y \"|\" $0; r = getline z w; print r }",
            "a\nb\n",
            "a||\n1\n",
            "",
            0,
        ),
        (
            "BEGIN { while (getline line > 0) n++; print n }",
            "a\nb\n",
            "2\n",
            "",
            0,
        ),
        (
            "BEGIN { print > \"x\" > \"y\" }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { print > \"x\" > \"y\" }\nawk: cmd. line:1:                     ^ syntax error\n",
            1,
        ),
    ]);
}

// `sub` and `gsub` of a constant replace in a copy, as in gawk; it was a compile error.
// Anything else that is no variable is gawk's error, as before (TODO.md phase 15).
#[test]
fn test_awk_sub_of_a_constant_replaces_in_a_copy() {
    run_cases(&[
        (
            "BEGIN { print sub(/a/, \"b\", \"aa\"), gsub(/a/, \"b\", \"aaa\"), sub(/3/, \"x\", 1 + 2) }",
            "",
            "1 3 1\n",
            "",
            0,
        ),
        (
            "BEGIN { x = \"aa\"; print gsub(/a/, \"b\", x \"a\"), x }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = \"aa\"; print gsub(/a/, \"b\", x \"a\"), x }\nawk: cmd. line:1:                                             ^ gsub third parameter is not a changeable object\n",
            1,
        ),
    ]);
}

// A number used as a regex is its text, as in gawk; it was a fatal error (TODO.md phase
// 15).
#[test]
fn test_awk_a_number_is_a_regex() {
    run_cases(&[(
        "BEGIN { print (12 ~ 1), (\"1.5\" ~ 1.5), match(\"a12\", 12), split(\"a1b1c\", z, 1) }",
        "",
        "1 1 2 3\n",
        "",
        0,
    )]);
}

// gawk reports nothing after a function defined twice; cash went on (TODO.md phase 15).
#[test]
fn test_awk_a_function_defined_twice_ends_the_errors() {
    run_cases(&[
        (
            "function f() {} function f() {} BEGIN { x = 1; x(); print 1/0 }",
            "",
            "",
            "awk: cmd. line:1: error: function name `f' previously defined\n",
            1,
        ),
        (
            "BEGIN { x = 1; x() } function f() {} function f() {}",
            "",
            "",
            "awk: cmd. line:1: error: attempt to use non-function `x' in function call\nawk: cmd. line:1: error: function name `f' previously defined\n",
            1,
        ),
    ]);
}

// `for (k in a)` goes through the indices that are integers from 1 last and in order,
// as gawk does (TODO.md phase 15); the others are in the order they were made, where
// gawk's is a hash table's. gawk 5.4's output.
#[test]
fn test_awk_for_in_goes_through_integers_in_order() {
    run_cases(&[(
        "BEGIN { a[10]; a[\"x\"]; a[5]; a[1]; a[3]; a[\"07\"]; for (k in a) printf \"%s \", k; print \"\" }",
        "",
        "x 07 1 3 5 10 \n",
        "",
        0,
    )]);
}

// gawk's `asort` and `asorti`, with a destination and each of gawk's orders or a
// function of the program's; they were undefined functions (TODO.md phase 15). gawk
// 5.4's output.
#[test]
fn test_awk_asort_and_asorti_are_gawks() {
    run_cases(&[
        (
            "BEGIN { a[\"x\"] = \"b\"; a[\"y\"] = 10; a[\"z\"] = \"a\"; a[\"w\"] = 9; a[\"v\"] = \"\"; n = asort(a); for (i = 1; i <= n; i++) printf \"[%s]\", a[i]; print \"\", n, (\"x\" in a) }",
            "",
            "[9][10][][a][b] 5 0\n",
            "",
            0,
        ),
        (
            "BEGIN { split(\"10 9 x 1e1 abc 09\", a); n = asort(a, b); for (i = 1; i <= n; i++) printf \"%s \", b[i]; print a[1] }",
            "",
            "9 09 10 1e1 abc x 10\n",
            "",
            0,
        ),
        (
            "BEGIN { a[\"x\"] = 3; a[\"b\"] = 1; a[10] = 1; a[9] = 1; n = asorti(a, d); print n, d[1], d[2], d[3], d[4]; asorti(a, d, \"@ind_num_asc\"); print d[1], d[2], d[3], d[4] }",
            "",
            "4 10 9 b x\nb x 9 10\n",
            "",
            0,
        ),
        (
            "BEGIN { a[1] = \"b\"; a[2] = \"a\"; a[3] = 10; a[4] = 9; split(\"@val_str_asc @val_num_asc @val_type_desc @val_str_desc @val_num_desc\", how); for (h = 1; h <= 5; h++) { asort(a, d, how[h]); print how[h], d[1], d[2], d[3], d[4] } }",
            "",
            "@val_str_asc 10 9 a b\n@val_num_asc a b 9 10\n@val_type_desc b a 10 9\n@val_str_desc b a 9 10\n@val_num_desc 10 9 b a\n",
            "",
            0,
        ),
        (
            "BEGIN { a[\"b\"]; a[\"a\"]; a[10]; a[9]; asorti(a, d, \"@ind_str_desc\"); print d[1], d[2], d[3], d[4]; asorti(a, d, \"@ind_num_desc\"); print d[1], d[2], d[3], d[4] }",
            "",
            "b a 9 10\n10 9 b a\n",
            "",
            0,
        ),
        (
            "function cmp(i1, v1, i2, v2) { return v2 - v1 } BEGIN { a[1] = 2; a[2] = 5; a[3] = 1; a[4] = 3; n = asort(a, d, \"cmp\"); print n, d[1], d[2], d[3], d[4] }",
            "",
            "4 5 3 2 1\n",
            "",
            0,
        ),
        (
            "function bylen(i1, v1, i2, v2,   l1, l2) { l1 = length(i1); l2 = length(i2); return l1 < l2 ? -1 : l1 > l2 } BEGIN { a[\"ccc\"]; a[\"a\"]; a[\"bb\"]; n = asorti(a, d, \"bylen\"); print n, d[1], d[2], d[3] }",
            "",
            "3 a bb ccc\n",
            "",
            0,
        ),
    ]);
}

// What is wrong with an argument of `asort` or `asorti` is gawk's error (TODO.md phase 15).
#[test]
fn test_awk_asort_errors_are_gawks() {
    run_cases(&[
        (
            "BEGIN { a[1] = 2; asort(a, d, \"nosuch\") }",
            "",
            "",
            "awk: cmd. line:1: fatal: sort comparison function `nosuch' is not defined\n",
            2,
        ),
        (
            "BEGIN { x = 1; asort(x) }",
            "",
            "",
            "awk: cmd. line:1: fatal: asort: first argument is not an array\n",
            2,
        ),
        (
            "BEGIN { a[1]; asorti(a, \"q\") }",
            "",
            "",
            "awk: cmd. line:1: fatal: asorti: second argument is not an array\n",
            2,
        ),
        (
            "BEGIN { a[1] = 3; a[2] = 1; n = asort(a, a); print n, a[1], a[2] }",
            "",
            "2 1 3\n",
            "awk: cmd. line:1: warning: asort/asorti: using the same array as source and destination without a third argument is silly.\n",
            0,
        ),
        (
            "BEGIN { a[1][2] = 1; a[2] = 5; a[3] = \"x\"; n = asort(a, d); print n, d[1], d[2], isarray(d[3]) }",
            "",
            "3 5 x 1\n",
            "",
            0,
        ),
        (
            "BEGIN { n = asort(e); print n, length(e) }",
            "",
            "0 0\n",
            "",
            0,
        ),
        (
            "BEGIN { asort() }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { asort() }\nawk: cmd. line:1:               ^ 0 is invalid as number of arguments for asort\n",
            1,
        ),
        (
            "function asort() { }",
            "",
            "",
            "awk: cmd. line:1: function asort() { }\nawk: cmd. line:1:          ^ `asort' is a built-in function, it cannot be redefined\n",
            1,
        ),
    ]);
}

// An escape gawk has no meaning for is the character, with gawk's warning once for each:
// a string's at the program's line, and a regex's where the regex is made; `\x` takes two
// hexadecimal digits. gawk's regex operators `\y`, `\B`, `\<`, `\>`, `` \` `` and `\'`
// are its; they were errors or the engine's. No empty match comes where a match ended,
// and an empty match splits nothing (TODO.md phase 15). gawk 5.4's output.
#[test]
fn test_awk_escapes_and_regex_operators_are_gawks() {
    run_cases(&[
        (
            "BEGIN { print \"a\\.b\", \"\\q\", \"\\q\", \"\\x41\\x4a\\x4\", \"[\\x]\" }",
            "",
            "a.b q q AJ\u{4} [x]\n",
            "awk: cmd. line:1: warning: escape sequence `\\.' treated as plain `.'\nawk: cmd. line:1: warning: escape sequence `\\q' treated as plain `q'\nawk: cmd. line:1: warning: no hex digits in `\\x' escape sequence\n",
            0,
        ),
        (
            "BEGIN { s = \"foo bar\"; print gsub(/\\y/, \"|\", s), s; t = \"foo bar\"; print gsub(/\\B/, \"|\", t), t }",
            "",
            "4 |foo| |bar|\n4 f|o|o b|a|r\n",
            "",
            0,
        ),
        (
            "BEGIN { s = \"foo bar\"; print gsub(/\\</, \"<\", s), gsub(/\\>/, \">\", s), s }",
            "",
            "2 2 <foo> <bar>\n",
            "",
            0,
        ),
        (
            "BEGIN { s = \"foo\\nbar\"; print gsub(/\\`/, \"[\", s), gsub(/\\'/, \"]\", s), s }",
            "",
            "1 1 [foo\nbar]\n",
            "",
            0,
        ),
        (
            "BEGIN { r = \"\\\\yfoo\\\\y\"; print (\"a foo b\" ~ r), (\"afoob\" ~ r) }",
            "",
            "1 0\n",
            "",
            0,
        ),
        (
            "BEGIN { print (\"q\" ~ /\\q/), (\"d\" ~ /\\d/), (\"5\" ~ /\\d/), (\"a\\001b\" ~ \"a\\\\1b\"), (\"\\b\" ~ \"\\\\b\") }",
            "",
            "1 1 0 1 1\n",
            "awk: cmd. line:1: warning: regexp escape sequence `\\q' is not a known regexp operator\nawk: cmd. line:1: warning: regexp escape sequence `\\d' is not a known regexp operator\n",
            0,
        ),
        (
            "{ print ($0 ~ \"\\\\e\") }",
            "e\ne\n",
            "1\n1\n",
            "awk: cmd. line:1: (FILENAME=- FNR=1) warning: regexp escape sequence `\\e' is not a known regexp operator\n",
            0,
        ),
        (
            "BEGIN { s = \"xab\"; print gsub(/x*/, \"-\", s), s; t = \"baaac\"; print gsub(/a*/, \"-\", t), t; print split(\"abc\", p, /x*/) }",
            "",
            "3 -a-b-\n3 -b-c-\n1\n",
            "",
            0,
        ),
        (
            "BEGIN { print (\"a.b\" ~ /a\\.b/), (\"axb\" ~ \"a\\\\.b\"), (\"a(b\" ~ \"a\\\\(b\") }",
            "",
            "1 0 1\n",
            "",
            0,
        ),
    ]);
}

// A control character in the program is gawk's fatal error, a vertical tab or a form
// feed an invalid character, outside strings and comments (TODO.md phase 15). gawk 5.4's.
#[test]
fn test_awk_control_characters_in_the_program_are_gawks_errors() {
    run_cases(&[
        (
            "BEGIN { print 1/0; x = 1 \u{1} 2 }",
            "",
            "",
            "awk: cmd. line:1: error: division by zero attempted\nawk: cmd. line:1: fatal: error: invalid character '\\001' in source code\n",
            2,
        ),
        (
            "BEGIN { x = 1 \u{c} 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 \u{c} 2 }\nawk: cmd. line:1:               ^ invalid char '\u{c}' in expression\n",
            1,
        ),
        (
            "BEGIN { x = \"a\u{1}b\"; print length(x) } # \u{2}",
            "",
            "3\n",
            "",
            0,
        ),
    ]);
}

// A value given on the command line has its escapes made with gawk's warnings, unplaced
// (TODO.md phase 15).
#[test]
fn test_awk_command_line_escapes_warn_as_gawks() {
    let output = run_test_base(
        &[
            "-v".to_string(),
            "x=a\\qb".to_string(),
            "BEGIN { print x }".to_string(),
        ],
        b"",
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        "aqb\n"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
        "awk: warning: escape sequence `\\q' treated as plain `q'\n"
    );
}

// A variable in parentheses is its value, as in gawk: no lvalue for `sub` or `gsub`, no
// array for `split`, `length`, `match` or a function, nor the right side of `in`; each took
// the variable (TODO.md phase 15). gawk 5.4's output.
#[test]
fn test_awk_a_parenthesized_variable_is_a_value() {
    run_cases(&[
        (
            "BEGIN { x = \"aa\"; print gsub(/a/, \"b\", (x)), x }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = \"aa\"; print gsub(/a/, \"b\", (x)), x }\nawk: cmd. line:1:                                           ^ gsub third parameter is not a changeable object\n",
            1,
        ),
        (
            "BEGIN { $0 = \"aa\"; print sub(/a/, \"b\", ($1)), $0 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { $0 = \"aa\"; print sub(/a/, \"b\", ($1)), $0 }\nawk: cmd. line:1:                                            ^ sub third parameter is not a changeable object\n",
            1,
        ),
        (
            "BEGIN { n = split(\"a b\", (arr)); print n }",
            "",
            "",
            "awk: cmd. line:1: fatal: split: second argument is not an array\n",
            2,
        ),
        (
            "BEGIN { a[1]; print length((a)) }",
            "",
            "",
            "awk: cmd. line:1: fatal: attempt to use array `a' in a scalar context\n",
            2,
        ),
        (
            "function f(p) { p[1] = 1 } BEGIN { f((a)); print length(a) }",
            "",
            "",
            "awk: cmd. line:1: fatal: attempt to use scalar parameter `p' as an array\n",
            2,
        ),
        (
            "BEGIN { n = match(\"ab\", /(b)/, (m)) }",
            "",
            "",
            "awk: cmd. line:1: fatal: match: third argument is not an array\n",
            2,
        ),
        (
            "BEGIN { a[1]; x = 1 in (a) }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { a[1]; x = 1 in (a) }\nawk: cmd. line:1:                        ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = \"aa\"; print length((x)), (x) \"b\" }",
            "",
            "2 aab\n",
            "",
            0,
        ),
    ]);
}

// gawk's IGNORECASE: regexes, `index`, the comparison of strings and `asort` ignore case;
// a separator of one character, `in` and subscripts do not (TODO.md phase 15). A regex of
// one character in `split` is that character, as in gawk. gawk 5.4's output.
#[test]
fn test_awk_ignorecase_is_gawks() {
    run_cases(&[
        (
            "BEGIN { IGNORECASE = 1; print (\"ABC\" ~ /b/), (\"ABC\" ~ \"b\"), match(\"xABC\", /bc/), RSTART, RLENGTH; s = \"ABAB\"; print gsub(/b/, \"x\", s), s }",
            "",
            "1 1 3 3 2\n2 AxAx\n",
            "",
            0,
        ),
        (
            "BEGIN { IGNORECASE = 1; print split(\"aXXbxxc\", p, \"xx\"), split(\"aXbxc\", q, \"x\"), split(\"aXbxc\", r, /x/), index(\"ABC\", \"b\") }",
            "",
            "3 2 2 2\n",
            "",
            0,
        ),
        (
            "BEGIN { IGNORECASE = 1; print (\"ABC\" == \"abc\"), (\"_\" < \"A\"); a[\"A\"]; print (\"a\" in a); b[1] = \"B\"; b[2] = \"a\"; asort(b); print b[1], b[2] }",
            "",
            "1 1\n0\na B\n",
            "",
            0,
        ),
        (
            "BEGIN { r = \"b\"; print (\"B\" ~ r); IGNORECASE = 1; print (\"B\" ~ r); IGNORECASE = 0; print (\"B\" ~ r); IGNORECASE = \"0\"; print (\"B\" ~ r); IGNORECASE = \"\"; print (\"B\" ~ r) }",
            "",
            "0\n1\n0\n1\n0\n",
            "",
            0,
        ),
        (
            "BEGIN { IGNORECASE = 1; FS = \"xx\"; RS = \"yy\" } { print NR, NF, $1 }",
            "aXXbYYcxxd",
            "1 2 a\n2 2 c\n",
            "",
            0,
        ),
        (
            "BEGIN { IGNORECASE = 1 } /abc/ { print \"m\" }",
            "xABCx\n",
            "m\n",
            "",
            0,
        ),
        (
            "BEGIN { print split(\"a.b\", p, /./), split(\" a  b \", q, / /), split(\"a^b\", t, /^/), p[1] }",
            "",
            "2 5 2 a\n",
            "",
            0,
        ),
    ]);
}

// A sort comparison function gets a subarray as itself, named as the element in an error,
// as in gawk; it got a copy (TODO.md phase 15). gawk 5.4's output.
#[test]
fn test_awk_a_sort_function_gets_a_subarray_itself() {
    run_cases(&[
        (
            "function cmp(i1, v1, i2, v2) { return v1 - v2 } BEGIN { a[1][1]; a[2] = 1; asort(a, d, \"cmp\") }",
            "",
            "",
            "awk: cmd. line:1: fatal: attempt to use array `v1 (from a[\"1\"])' in a scalar context\n",
            2,
        ),
        (
            "function cmp(i1, v1, i2, v2) { return length(v1) - length(v2) } BEGIN { a[\"p\"][1]; a[\"p\"][2]; a[\"q\"][1]; n = asorti(a, d, \"cmp\"); print n, d[1], d[2] }",
            "",
            "2 q p\n",
            "",
            0,
        ),
    ]);
}

// `s = s x` appends in place (TODO.md phase 15); the value is what the assignment of the
// whole would give, whatever `s` held. gawk 5.4's output.
#[test]
fn test_awk_appending_to_a_string_in_place_keeps_its_value() {
    run_cases(&[
        (
            "BEGIN { s = \"ab\"; s = s s; print s; x = 5; x = x 1; print x, x + 1; x = x 2 + 3; print x }",
            "",
            "abab\n51 52\n515\n",
            "",
            0,
        ),
        (
            "BEGIN { a[\"k\"] = \"p\"; for (i = 0; i < 3; i++) a[\"k\"] = a[\"k\"] i; print a[\"k\"]; b[1][2] = \"q\"; b[1][2] = b[1][2] \"r\"; print b[1][2] }",
            "",
            "p012\nqr\n",
            "",
            0,
        ),
        (
            "BEGIN { s = \"a\"; y = (s = s \"b\"); print y, s; s = 1; s = s (s = 5); print s; CONVFMT = \"%.2f\"; z = 3.14159; z = z z; print z }",
            "",
            "ab ab\n15\n3.143.14\n",
            "",
            0,
        ),
        (
            "{ line = line $0 }\nEND { print line, length(line) }",
            "a\nb\nc\n",
            "abc 3\n",
            "",
            0,
        ),
    ]);
}

/// gawk's `gensub`: the result returned and the target left alone, and `&`, `\0` to
/// `\9` and escapes in the replacement.
#[test]
fn test_awk_gensub_replaces_as_gawk() {
    run_cases(&[
        (
            "BEGIN { s = \"hello world\"; print gensub(/o/, \"0\", \"g\", s), s }",
            "",
            "hell0 w0rld hello world\n",
            "",
            0,
        ),
        (
            "BEGIN { s = \"hello world\"; print gensub(/o/, \"0\", \"G\", s); print gensub(/o/, \"0\", 2, s); print gensub(/o/, \"0\", 1, s); print gensub(/o/, \"0\", 3, s) }",
            "",
            "hell0 w0rld\nhello w0rld\nhell0 world\nhello world\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/(a)(b)/, \"<\\\\2\\\\1>\", \"g\", \"abxab\") }",
            "",
            "<ba>x<ba>\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/b+/, \"[&]\", \"g\", \"abbcb\"), gensub(/b+/, \"[\\\\0]\", \"g\", \"abbcb\") }",
            "",
            "a[bb]c[b] a[bb]c[b]\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/b/, \"\\\\&\", \"g\", \"abc\"), gensub(/b/, \"\\\\\\\\&\", \"g\", \"abc\") }",
            "",
            "a&c a\\bc\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/b/, \"x\\\\qy\", \"g\", \"abc\") }",
            "",
            "axqyc\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/(b)/, \"\\\\3\", \"g\", \"abc\"), gensub(/(b)/, \"\\\\9\", \"g\", \"abc\") }",
            "",
            "ac ac\n",
            "",
            0,
        ),
        (
            "{ print gensub(/o/, \"0\", \"g\") ; print }",
            "foo boo\n",
            "f00 b00\nfoo boo\n",
            "",
            0,
        ),
    ]);
}

/// `gensub`'s `how`: `g` or `G` first, else a number, one not above 0 the first match
/// with gawk's warning; the empty matches are gsub's, and IGNORECASE is honoured.
#[test]
fn test_awk_gensub_takes_how_as_gawk() {
    run_cases(&[
        (
            "BEGIN { print gensub(/o/, \"0\", \"x\", \"foo\") }",
            "",
            "f0o\n",
            "awk: cmd. line:1: warning: gensub: third argument `x' treated as 1\n",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", \"\", \"foo\") }",
            "",
            "f0o\n",
            "awk: cmd. line:1: warning: gensub: third argument `' treated as 1\n",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", 0, \"foo\") }",
            "",
            "f0o\n",
            "awk: cmd. line:1: warning: gensub: third argument `0' treated as 1\n",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", -1, \"foo\") }",
            "",
            "f0o\n",
            "awk: cmd. line:1: warning: gensub: third argument `-1' treated as 1\n",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", 1.7, \"foo\") }",
            "",
            "f0o\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", \"2\", \"foo\") }",
            "",
            "fo0\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", \"gx\", \"foo\") }",
            "",
            "f00\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", \"10\", \"foo\") }",
            "",
            "foo\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/x*/, \"-\", \"g\", \"abc\"), gensub(/x*/, \"-\", 2, \"abc\") }",
            "",
            "-a-b-c- a-bc\n",
            "",
            0,
        ),
        (
            "BEGIN { IGNORECASE = 1; print gensub(/O/, \"0\", \"g\", \"foO\") }",
            "",
            "f00\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(\"o+\", \"0\", \"g\", \"foo\") }",
            "",
            "f0\n",
            "",
            0,
        ),
    ]);
}

/// gawk's errors for `gensub`'s arguments, and its result a string.
#[test]
fn test_awk_gensub_arguments_are_checked_as_gawk() {
    run_cases(&[
        (
            "BEGIN { print gensub(/o/, \"0\") }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { print gensub(/o/, \"0\") }\nawk: cmd. line:1:                              ^ 2 is invalid as number of arguments for gensub\n",
            1,
        ),
        (
            "BEGIN { print gensub(/o/) }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { print gensub(/o/) }\nawk: cmd. line:1:                         ^ 1 is invalid as number of arguments for gensub\n",
            1,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", \"g\", \"x\", 5) }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { print gensub(/o/, \"0\", \"g\", \"x\", 5) }\nawk: cmd. line:1:                                           ^ 5 is invalid as number of arguments for gensub\n",
            1,
        ),
        (
            "BEGIN { print gensub() }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { print gensub() }\nawk: cmd. line:1:                      ^ 0 is invalid as number of arguments for gensub\n",
            1,
        ),
        (
            "BEGIN { n = gensub(/o/, \"0\", \"g\", 1000); print n, n + 1 }",
            "",
            "1000 1001\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/a/, \"\\\\\", \"g\", \"bab\") }",
            "",
            "b\\b\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/a/, \"z\\\\\\\\\\\\0\", \"g\", \"bab\") }",
            "",
            "bz\\ab\n",
            "",
            0,
        ),
        (
            "BEGIN { a[1] = \"x\"; print gensub(/x/, \"y\", \"g\", a) }",
            "",
            "",
            "awk: cmd. line:1: fatal: attempt to use array `a' in a scalar context\n",
            2,
        ),
        (
            "function gensub() { }",
            "",
            "",
            "awk: cmd. line:1: function gensub() { }\nawk: cmd. line:1:          ^ `gensub' is a built-in function, it cannot be redefined\n",
            1,
        ),
        (
            "BEGIN { print gensub(/é/, \"e\", \"g\", \"café é\") }",
            "",
            "cafe e\n",
            "",
            0,
        ),
    ]);
}

/// `how` as a string, a field or a fraction, and the warning's text for each.
#[test]
fn test_awk_gensub_takes_any_how_as_gawk() {
    run_cases(&[
        (
            "BEGIN { print gensub(/o/, \"0\", 0.5, \"foo\") }",
            "",
            "f0o\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", \"0.5\", \"foo\") }",
            "",
            "f0o\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", u, \"foo\") }",
            "",
            "f0o\n",
            "awk: cmd. line:1: warning: gensub: third argument `' treated as 1\n",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", -0.25, \"foo\") }",
            "",
            "f0o\n",
            "awk: cmd. line:1: warning: gensub: third argument `-0.25' treated as 1\n",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", \" g\", \"foo\") }",
            "",
            "f0o\n",
            "awk: cmd. line:1: warning: gensub: third argument ` g' treated as 1\n",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", \"2x\", \"foo\") }",
            "",
            "fo0\n",
            "",
            0,
        ),
        (
            "{ print gensub(/o/, \"0\", $1, $2) }",
            "g foo\n2 foo\n-3 foo\n",
            "f00\nfo0\nf0o\n",
            "awk: cmd. line:1: (FILENAME=- FNR=3) warning: gensub: third argument `-3' treated as 1\n",
            0,
        ),
        (
            "BEGIN { CONVFMT = \"%.2f\"; x = 0.123456; print gensub(/o/, \"0\", x, \"foo\") }",
            "",
            "f0o\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", 1e300, \"foo\") }",
            "",
            "foo\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/\\</, \"|\", \"g\", \"ab cd\") }",
            "",
            "|ab |cd\n",
            "",
            0,
        ),
    ]);
}

/// `gensub`'s empty matches, missing groups, default target and trailing backslashes.
#[test]
fn test_awk_gensub_matches_as_gawk() {
    run_cases(&[
        (
            "BEGIN { print gensub(/b*/, \"<&>\", \"g\", \"abc\") }",
            "",
            "<>a<b>c<>\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/(x)?b/, \"[\\\\1]\", \"g\", \"abxb\") }",
            "",
            "a[][x]\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", \"g\", \"foo\") > \"/dev/stderr\" }",
            "",
            "",
            "f00\n",
            0,
        ),
        (
            "BEGIN { $0 = \"aaa\"; print gensub(/a/, \"b\", 2); print }",
            "",
            "aba\naaa\n",
            "",
            0,
        ),
        (
            "BEGIN { print length(gensub(/a/, \"bb\", \"g\", \"aa\")) }",
            "",
            "4\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/a/, \"\\\\\\\\\", \"g\", \"bab\") }",
            "",
            "b\\b\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/(a)/, \"\\\\10\", \"g\", \"bab\") }",
            "",
            "ba0b\n",
            "",
            0,
        ),
        (
            "BEGIN { s = \"x\"; print gensub(/x/, \"y\", \"g\", s); print s }",
            "",
            "y\nx\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\", 1, \"foo\", ) }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { print gensub(/o/, \"0\", 1, \"foo\", ) }\nawk: cmd. line:1:                                          ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { print gensub(/o/, \"0\" }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { print gensub(/o/, \"0\" }\nawk: cmd. line:1:                               ^ syntax error\n",
            1,
        ),
    ]);
}

/// Each match has its own groups. Git for Windows' gawk 5.4.0 leaves them empty after
/// the first match (the README's deliberate differences); these are the documented ones.
#[test]
fn test_awk_gensub_gives_each_match_its_groups() {
    run_cases(&[
        (
            "BEGIN { print gensub(/(a)(b)/, \"<\\\\2\\\\1>\", \"g\", \"abab\") }",
            "",
            "<ba><ba>\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/(a)(b)/, \"<\\\\2\\\\1>\", \"g\", \"xabab\") }",
            "",
            "x<ba><ba>\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/(a)(b)/, \"<\\\\2\\\\1>\", 2, \"abxab\") }",
            "",
            "abx<ba>\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/([a-z])([0-9])/, \"<\\\\2\\\\1>\", \"g\", \"a1 b2 c3\") }",
            "",
            "<1a> <2b> <3c>\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/(.)(.)/, \"\\\\2\\\\1\", \"g\", \"abcdef\") }",
            "",
            "badcfe\n",
            "",
            0,
        ),
        (
            "BEGIN { print gensub(/([^ ]+) ([^ ]+)/, \"\\\\2 \\\\1\", \"g\", \"one two three four\") }",
            "",
            "two one four three\n",
            "",
            0,
        ),
        (
            "BEGIN { s = \"abxab\"; while (match(s, /(a)(b)/, m)) { print m[1], m[2]; s = substr(s, RSTART + RLENGTH) } }",
            "",
            "a b\na b\n",
            "",
            0,
        ),
    ]);
}

/// An assignment to what is no variable fails at the operator's start, as gawk's lexer
/// has one token for `+=`.
#[test]
fn test_awk_compound_assignment_errors_are_placed_as_gawk() {
    run_cases(&[
        (
            "BEGIN { print gensub(/(o)|(x)/, \"[\\\\1|\\\\2]\", \"g\", \"fox\") }",
            "",
            "f[o|][|x]\n",
            "",
            0,
        ),
        (
            "BEGIN { x = 1; (x) += 2; print x }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1; (x) += 2; print x }\nawk: cmd. line:1:                    ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1; (x) -= 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1; (x) -= 2 }\nawk: cmd. line:1:                    ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1; (x) ^= 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1; (x) ^= 2 }\nawk: cmd. line:1:                    ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1; (x) = 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1; (x) = 2 }\nawk: cmd. line:1:                    ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1; (x)++ }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1; (x)++ }\nawk: cmd. line:1:                      ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1;  (x)   +=  2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1;  (x)   +=  2 }\nawk: cmd. line:1:                       ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { (x) **= 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { (x) **= 2 }\nawk: cmd. line:1:             ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { (x) *= 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { (x) *= 2 }\nawk: cmd. line:1:             ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { (x) %= 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { (x) %= 2 }\nawk: cmd. line:1:             ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { 1 += 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { 1 += 2 }\nawk: cmd. line:1:           ^ syntax error\n",
            1,
        ),
    ]);
}

/// More assignments to values, `++=` and `==` among them, placed as gawk places them.
#[test]
fn test_awk_assignments_to_values_are_placed_as_gawk() {
    run_cases(&[
        (
            "BEGIN { x+1 = 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x+1 = 2 }\nawk: cmd. line:1:             ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { \"a\" = 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { \"a\" = 2 }\nawk: cmd. line:1:             ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { \"a\" -= 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { \"a\" -= 2 }\nawk: cmd. line:1:             ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x++ = 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x++ = 2 }\nawk: cmd. line:1:             ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x++= 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x++= 2 }\nawk: cmd. line:1:            ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x--= 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x--= 2 }\nawk: cmd. line:1:            ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { (x)^=2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { (x)^=2 }\nawk: cmd. line:1:            ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { a[1] = 1; (a[1]) += 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { a[1] = 1; (a[1]) += 2 }\nawk: cmd. line:1:                          ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { ($1) *= 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { ($1) *= 2 }\nawk: cmd. line:1:              ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = y == = 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = y == = 2 }\nawk: cmd. line:1:                  ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { -x += 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { -x += 2 }\nawk: cmd. line:1:            ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { !x **= 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { !x **= 2 }\nawk: cmd. line:1:            ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = \"abc }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = \"abc }\nawk: cmd. line:1:             ^ unterminated string\n",
            1,
        ),
    ]);
}

/// `/=` after `)` starts a regex, as in gawk, and one the line ends in is gawk's
/// `unterminated regexp`.
#[test]
fn test_awk_slash_equals_after_a_parenthesis_starts_a_regex() {
    run_cases(&[
        (
            "BEGIN { (x) /= 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { (x) /= 2 }\nawk: cmd. line:1:              ^ unterminated regexp\n",
            1,
        ),
        (
            "BEGIN { x = /abc }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = /abc }\nawk: cmd. line:1:              ^ unterminated regexp\n",
            1,
        ),
        (
            "BEGIN { x = /abc \n}",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = /abc \nawk: cmd. line:1:              ^ unterminated regexp\n",
            1,
        ),
        (
            "BEGIN { (x) /= 2 \n}",
            "",
            "",
            "awk: cmd. line:1: BEGIN { (x) /= 2 \nawk: cmd. line:1:              ^ unterminated regexp\n",
            1,
        ),
        ("BEGIN { x = 4; x /= 2; print x }", "", "2\n", "", 0),
        (
            "BEGIN { x = 4; (x) /= 2; print x }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 4; (x) /= 2; print x }\nawk: cmd. line:1:                     ^ unterminated regexp\n",
            1,
        ),
        (
            "BEGIN { a[1] = 4; a[1] /= 2; print a[1] }",
            "",
            "2\n",
            "",
            0,
        ),
        (
            "BEGIN { x = 4; print x /=2/ }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 4; print x /=2/ }\nawk: cmd. line:1:                             ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = /[a-z }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = /[a-z }\nawk: cmd. line:1:              ^ unterminated regexp\n",
            1,
        ),
    ]);
}

/// Like `run_cases`, with each program written to a file as it is, with no newline added,
/// and run with `-f`; `{file}` in the expected errors stands for the file's path.
fn run_file_cases(cases: &[(&str, &str, &str, &str, i32)]) {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("prog.awk");
    let name = path.to_string_lossy().into_owned();
    for (program, stdin, stdout, stderr, status) in cases {
        std::fs::write(&path, program).expect("the program file");
        let output = run_test_base(&["-f".to_string(), name.clone()], stdin.as_bytes());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
            *stdout,
            "{program:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
            stderr.replace("{file}", &name),
            "{program:?}"
        );
        assert_eq!(output.status.code(), Some(*status), "{program:?}");
    }
}

/// `/=` after a variable that starts an expression is an assignment; after any other
/// operand it starts a regex (`(x) /= 2/` is `x` joined to the regex `/= 2/`), and it
/// divides nowhere, as in gawk's grammar. An assignment may follow a comparison, a match,
/// `&&` or `||`. Each was a syntax error, or not one where gawk has one.
#[test]
fn test_awk_slash_equals_as_gawk_1() {
    run_cases(&[
        ("BEGIN { x = 1; print (x) /= 2/ }", "", "10\n", "", 0),
        ("{ print 1 /=1/ }", "a=1\nb\n", "11\n10\n", "", 0),
        ("{ print \"a\" /=1/ }", "a=1\nb\n", "a1\na0\n", "", 0),
        (
            "{ x = 5; print x++ /=1/; print x }",
            "a=1\n",
            "51\n6\n",
            "",
            0,
        ),
        ("{ x = 5; print x-- /=1/ }", "a=1\n", "51\n", "", 0),
        ("{ x = 5; print -x /=1/ }", "a=1\n", "-51\n", "", 0),
        ("{ x = 5; print !x /=1/ }", "a=1\n", "01\n", "", 0),
        (
            "{ print $1 /=1/ }",
            "4 =1\n",
            "",
            "awk: cmd. line:1: { print $1 /=1/ }\nawk: cmd. line:1:                 ^ syntax error\n",
            1,
        ),
        (
            "{ a[1] = 4; print a[1] /=1/ }",
            "a=1\n",
            "",
            "awk: cmd. line:1: { a[1] = 4; print a[1] /=1/ }\nawk: cmd. line:1:                             ^ syntax error\n",
            1,
        ),
        (
            "{ x = 4; print x /=1/ }",
            "a=1\n",
            "",
            "awk: cmd. line:1: { x = 4; print x /=1/ }\nawk: cmd. line:1:                       ^ syntax error\n",
            1,
        ),
    ]);
}

/// `/=` after a variable that starts an expression is an assignment; after any other
/// operand it starts a regex (`(x) /= 2/` is `x` joined to the regex `/= 2/`), and it
/// divides nowhere, as in gawk's grammar. An assignment may follow a comparison, a match,
/// `&&` or `||`. Each was a syntax error, or not one where gawk has one.
#[test]
fn test_awk_slash_equals_as_gawk_2() {
    run_cases(&[
        ("{ y = (x) /=1/; print y }", "a=1\n", "1\n", "", 0),
        ("{ print 1, /=1/ }", "a=1\n", "1 1\n", "", 0),
        ("{ print !/=1/ }", "a=1\n", "0\n", "", 0),
        ("{ print length /=1/ }", "a=1\n", "31\n", "", 0),
        ("{ print $1/=2; print }", "4 x\n", "2\n2 x\n", "", 0),
        ("{ print 2 /=1/ 3 }", "a=1\n", "213\n", "", 0),
        (
            "{ print NR /=1/ }",
            "a=1\n",
            "",
            "awk: cmd. line:1: { print NR /=1/ }\nawk: cmd. line:1:                 ^ syntax error\n",
            1,
        ),
        (
            "function f() { return 7 } BEGIN { $0 = \"=1\"; print f() /=1/ }",
            "",
            "71\n",
            "",
            0,
        ),
        (
            "BEGIN { print 1 /= 2 }\n",
            "",
            "",
            "awk: cmd. line:1: BEGIN { print 1 /= 2 }\nawk: cmd. line:1:                  ^ unterminated regexp\n",
            1,
        ),
        ("{ x = 4; print (x) /=1/ }\n", "a=1\n", "41\n", "", 0),
    ]);
}

/// `/=` after a variable that starts an expression is an assignment; after any other
/// operand it starts a regex (`(x) /= 2/` is `x` joined to the regex `/= 2/`), and it
/// divides nowhere, as in gawk's grammar. An assignment may follow a comparison, a match,
/// `&&` or `||`. Each was a syntax error, or not one where gawk has one.
#[test]
fn test_awk_slash_equals_as_gawk_3() {
    run_cases(&[
        (
            "{ print $1 /=1/ }\n",
            "a=1\n",
            "",
            "awk: cmd. line:1: { print $1 /=1/ }\nawk: cmd. line:1:                 ^ syntax error\n",
            1,
        ),
        ("{ x = 4; print x++/=1/ }\n", "a=1\n", "41\n", "", 0),
        ("{ if (1 /=1/) print \"y\" }", "a=1\n", "y\n", "", 0),
        ("{ z = 1 /=1/; print z }", "a=1\n", "11\n", "", 0),
        ("{ print 1 /=1/ ? \"y\" : \"n\" }", "a=1\n", "y\n", "", 0),
        (
            "{ print $(1) /=1/ }",
            "8 =1\n",
            "",
            "awk: cmd. line:1: { print $(1) /=1/ }\nawk: cmd. line:1:                   ^ syntax error\n",
            1,
        ),
        (
            "{ print $NF /=2/ }",
            "8 4\n",
            "",
            "awk: cmd. line:1: { print $NF /=2/ }\nawk: cmd. line:1:                  ^ syntax error\n",
            1,
        ),
        ("{ print 1 /=/ }", "a=1\n", "11\n", "", 0),
        ("{ x = 5; print 1 + x /=1/ }", "a=1\n", "61\n", "", 0),
        (
            "{ x = 5; y = x /=1/; print y }",
            "a=1\n",
            "",
            "awk: cmd. line:1: { x = 5; y = x /=1/; print y }\nawk: cmd. line:1:                    ^ syntax error\n",
            1,
        ),
    ]);
}

/// `/=` after a variable that starts an expression is an assignment; after any other
/// operand it starts a regex (`(x) /= 2/` is `x` joined to the regex `/= 2/`), and it
/// divides nowhere, as in gawk's grammar. An assignment may follow a comparison, a match,
/// `&&` or `||`. Each was a syntax error, or not one where gawk has one.
#[test]
fn test_awk_slash_equals_as_gawk_4() {
    run_cases(&[
        (
            "{ x = 5; print 1, x /=1/ }",
            "a=1\n",
            "",
            "awk: cmd. line:1: { x = 5; print 1, x /=1/ }\nawk: cmd. line:1:                          ^ syntax error\n",
            1,
        ),
        ("{ a = 5; b = 6; print a b /=1/ }", "a=1\n", "561\n", "", 0),
        (
            "{ x = 5; print 1 < x /=1/ }",
            "a=1\n",
            "",
            "awk: cmd. line:1: { x = 5; print 1 < x /=1/ }\nawk: cmd. line:1:                           ^ syntax error\n",
            1,
        ),
        (
            "{ x = 5; print x ~ x /=1/ }",
            "a=1\n",
            "",
            "awk: cmd. line:1: { x = 5; print x ~ x /=1/ }\nawk: cmd. line:1:                           ^ syntax error\n",
            1,
        ),
        (
            "{ x = 5; print (x /=1/) }",
            "a=1\n",
            "",
            "awk: cmd. line:1: { x = 5; print (x /=1/) }\nawk: cmd. line:1:                       ^ syntax error\n",
            1,
        ),
        (
            "{ x = 5; print f(x /=1/) } function f(v) { return v }",
            "a=1\n",
            "",
            "awk: cmd. line:1: { x = 5; print f(x /=1/) } function f(v) { return v }\nawk: cmd. line:1:                        ^ syntax error\n",
            1,
        ),
        (
            "{ x = 5; print 1 ? x /=1/ : 2 }",
            "a=1\n",
            "",
            "awk: cmd. line:1: { x = 5; print 1 ? x /=1/ : 2 }\nawk: cmd. line:1:                           ^ syntax error\n",
            1,
        ),
        (
            "{ x = 5; print x && x /=1/ }",
            "a=1\n",
            "",
            "awk: cmd. line:1: { x = 5; print x && x /=1/ }\nawk: cmd. line:1:                            ^ syntax error\n",
            1,
        ),
        ("{ x = 5; print x ^ x /=1/ }", "a=1\n", "31251\n", "", 0),
        ("{ x = 4; print x * x /=1/ }", "a=1\n", "161\n", "", 0),
    ]);
}

/// `/=` after a variable that starts an expression is an assignment; after any other
/// operand it starts a regex (`(x) /= 2/` is `x` joined to the regex `/= 2/`), and it
/// divides nowhere, as in gawk's grammar. An assignment may follow a comparison, a match,
/// `&&` or `||`. Each was a syntax error, or not one where gawk has one.
#[test]
fn test_awk_slash_equals_as_gawk_5() {
    run_cases(&[
        (
            "{ x = 4; print $x /=1/ }",
            "a=1\n",
            "",
            "awk: cmd. line:1: { x = 4; print $x /=1/ }\nawk: cmd. line:1:                        ^ syntax error\n",
            1,
        ),
        (
            "{ x = 4; print x[1] }",
            "a=1\n",
            "",
            "awk: cmd. line:1: (FILENAME=- FNR=1) fatal: attempt to use scalar `x' as an array\n",
            2,
        ),
        ("{ x = 4; print x /=2; print x }", "a=1\n", "2\n2\n", "", 0),
        ("{ x = 4; print 1 + (x /=2) }", "a=1\n", "3\n", "", 0),
        ("{ x = 5; print 1 - x /=1/ }", "a=1\n", "-41\n", "", 0),
        (
            "{ x = 4; print (1 < x /= 2); print x }",
            "a=1\n",
            "1\n2\n",
            "",
            0,
        ),
        ("{ x = 4; if (0 || x = 2) print x }", "a=1\n", "2\n", "", 0),
        (
            "{ x = 4; print x ~ y = 4; print y }",
            "a=1\n",
            "1\n4\n",
            "",
            0,
        ),
        (
            "{ x = 4; print 1 + x = 2 }",
            "a=1\n",
            "",
            "awk: cmd. line:1: { x = 4; print 1 + x = 2 }\nawk: cmd. line:1:                      ^ syntax error\n",
            1,
        ),
    ]);
}

/// A backslash and a newline join a regex's lines, and a newline may follow a
/// parameter's comma, as in gawk; both were syntax errors.
#[test]
fn test_awk_backslash_newline_joins_regex_and_parameter_lines() {
    run_cases(&[
        (
            "BEGIN { print match(\"xab\", /a\\\nb/), RSTART, RLENGTH }",
            "",
            "2 2 2\n",
            "",
            0,
        ),
        ("BEGIN { print (\"a\\nb\" ~ /a\\\nb/) }", "", "0\n", "", 0),
        (
            "BEGIN { s = \"a\\\nb\"; print s, length(s) }",
            "",
            "ab 2\n",
            "",
            0,
        ),
        ("BEGIN { print \"ab\" ~ /[a\\\n]b/ }", "", "1\n", "", 0),
        (
            "function f(a,\n b) { return a + b } BEGIN { print f(1,\n 2) }",
            "",
            "3\n",
            "",
            0,
        ),
        (
            "function f(\na) { return a } BEGIN { print f(1) }",
            "",
            "",
            "awk: cmd. line:2: function f(\nawk: cmd. line:2:            ^ unexpected newline or end of string\n",
            1,
        ),
    ]);
}

/// gawk's errors where a program file ends, with a newline last and without: a rule
/// left incomplete is `(END OF FILE)`, a regex `unterminated regexp at end of file`.
#[test]
fn test_awk_errors_at_the_end_of_a_program_file_1() {
    run_file_cases(&[
        (
            "BEGIN {",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:       ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN {\n",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1: ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN {\n",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1: ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:             ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1\n",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1: ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 +",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2:               ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 +\n",
            "",
            "",
            "awk: {file}:2: BEGIN { x = 1 +\nawk: {file}:2:                ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN {\n  x = 1 +",
            "",
            "",
            "awk: {file}:3: (END OF FILE)\nawk: {file}:3:         ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN {\n  x = 1 +\n",
            "",
            "",
            "awk: {file}:3:   x = 1 +\nawk: {file}:3:          ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN {\n  x = 1\n",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2: ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN {\n\tx = 1 +",
            "",
            "",
            "awk: {file}:3: (END OF FILE)\nawk: {file}:3: \t      ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
    ]);
}

/// gawk's errors where a program file ends, with a newline last and without: a rule
/// left incomplete is `(END OF FILE)`, a regex `unterminated regexp at end of file`.
#[test]
fn test_awk_errors_at_the_end_of_a_program_file_2() {
    run_file_cases(&[
        (
            "BEGIN {\n\tx = 1 +\n",
            "",
            "",
            "awk: {file}:3: \tx = 1 +\nawk: {file}:3: \t       ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = 1 &&",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2:               ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 &&\n",
            "",
            "",
            "awk: {file}:3: (END OF FILE)\nawk: {file}:3:               ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 &&\n",
            "",
            "",
            "awk: {file}:3: (END OF FILE)\nawk: {file}:3:               ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 ? 2 :",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2:                   ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 ? 2 :\n",
            "",
            "",
            "awk: {file}:3: (END OF FILE)\nawk: {file}:3:                   ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1,",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1,\nawk: {file}:1:              ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1,\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1,\nawk: {file}:1:              ^ syntax error\n",
            1,
        ),
        (
            "function f(a,",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:             ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "function f(a,\n",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1: ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "function f(a)\n",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1: ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
    ]);
}

/// gawk's errors where a program file ends, with a newline last and without: a rule
/// left incomplete is `(END OF FILE)`, a regex `unterminated regexp at end of file`.
#[test]
fn test_awk_errors_at_the_end_of_a_program_file_3() {
    run_file_cases(&[
        (
            "BEGIN { x = 1 # c",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:               ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 # c\n",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:               ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 # c\n",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:               ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        ("# only", "", "", "", 0),
        ("# only\n", "", "", "", 0),
        (
            "BEGIN { x = /a/",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:              ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = /a/\n",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1: ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 } }",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } }\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } }\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } }\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { delete",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2:         ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { delete\n",
            "",
            "",
            "awk: {file}:2: BEGIN { delete\nawk: {file}:2:               ^ unexpected newline or end of string\n",
            1,
        ),
    ]);
}

/// gawk's errors where a program file ends, with a newline last and without: a rule
/// left incomplete is `(END OF FILE)`, a regex `unterminated regexp at end of file`.
#[test]
fn test_awk_errors_at_the_end_of_a_program_file_4() {
    run_file_cases(&[
        (
            "BEGIN { x = 1 } BEGIN",
            "",
            "",
            "awk: {file}:1: BEGIN blocks must have an action part\n",
            1,
        ),
        (
            "BEGIN { x = 1 } BEGIN\n",
            "",
            "",
            "awk: {file}:1: BEGIN blocks must have an action part\n",
            1,
        ),
        (
            "BEGIN { x = 1 } END",
            "",
            "",
            "awk: {file}:1: END blocks must have an action part\n",
            1,
        ),
        (
            "BEGIN { x = 1 } END\n",
            "",
            "",
            "awk: {file}:1: END blocks must have an action part\n",
            1,
        ),
        (
            "{ x = 1 +   ",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2:            ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "{ x = 1 +   \n",
            "",
            "",
            "awk: {file}:2: { x = 1 +   \nawk: {file}:2:             ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN   {   ",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:            ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN   {   \n",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1: ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN {\n x = 1   ",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2:         ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN {\n x = 1   \n",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2: ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 + # c",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2:                 ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
    ]);
}

/// gawk's errors where a program file ends, with a newline last and without: a rule
/// left incomplete is `(END OF FILE)`, a regex `unterminated regexp at end of file`.
#[test]
fn test_awk_errors_at_the_end_of_a_program_file_5() {
    run_file_cases(&[
        (
            "BEGIN { x = 1 + # c\n",
            "",
            "",
            "awk: {file}:2: BEGIN { x = 1 + # c\nawk: {file}:2:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN {\n x = 1 # c\n # d",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2:  ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN {\n x = 1 # c\n # d\n",
            "",
            "",
            "awk: {file}:3: (END OF FILE)\nawk: {file}:3:  ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN {\n # d",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:  ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN {\n # d\n",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2:  ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 +\n # d",
            "",
            "",
            "awk: {file}:2: BEGIN { x = 1 +\nawk: {file}:2:                ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = 1 +\n # d\n",
            "",
            "",
            "awk: {file}:2: BEGIN { x = 1 +\nawk: {file}:2:                ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "{ x = \"#\"",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:       ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "{ x = \"#\"\n",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1: ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { next",
            "",
            "",
            "awk: {file}:1: error: `next' used in BEGIN action\nawk: {file}:1: (END OF FILE)\nawk: {file}:1:         ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { next\n",
            "",
            "",
            "awk: {file}:1: error: `next' used in BEGIN action\nawk: {file}:1: (END OF FILE)\nawk: {file}:1: ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
    ]);
}

/// gawk's errors where a program file ends, with a newline last and without: a rule
/// left incomplete is `(END OF FILE)`, a regex `unterminated regexp at end of file`.
#[test]
fn test_awk_errors_at_the_end_of_a_program_file_6() {
    run_file_cases(&[
        (
            "@",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2: ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "@\n",
            "",
            "",
            "awk: {file}:2: @\nawk: {file}:2:  ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = /abc }",
            "",
            "",
            "awk: {file}:1: BEGIN { x = /abc }\nawk: {file}:1:              ^ unterminated regexp at end of file\n",
            1,
        ),
        (
            "BEGIN { x = /abc }\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = /abc }\nawk: {file}:1:              ^ unterminated regexp\n",
            1,
        ),
        (
            "BEGIN { x = /abc",
            "",
            "",
            "awk: {file}:1: BEGIN { x = /abc\nawk: {file}:1:              ^ unterminated regexp at end of file\n",
            1,
        ),
        (
            "BEGIN { x = /abc\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = /abc\nawk: {file}:1:              ^ unterminated regexp\n",
            1,
        ),
        (
            "BEGIN { x = \"abc",
            "",
            "",
            "awk: {file}:1: BEGIN { x = \"abc\nawk: {file}:1:             ^ unterminated string\n",
            1,
        ),
        (
            "BEGIN { x = \"abc\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = \"abc\nawk: {file}:1:             ^ unterminated string\n",
            1,
        ),
        (
            "BEGIN { print 1 /= 2 }",
            "",
            "",
            "awk: {file}:1: BEGIN { print 1 /= 2 }\nawk: {file}:1:                  ^ unterminated regexp at end of file\n",
            1,
        ),
        (
            "BEGIN { print 1 /= 2 }\n",
            "",
            "",
            "awk: {file}:1: BEGIN { print 1 /= 2 }\nawk: {file}:1:                  ^ unterminated regexp\n",
            1,
        ),
        (
            "BEGIN { (x) /= 2 }",
            "",
            "",
            "awk: {file}:1: BEGIN { (x) /= 2 }\nawk: {file}:1:              ^ unterminated regexp at end of file\n",
            1,
        ),
    ]);
}

/// gawk's errors where a program file ends, with a newline last and without: a rule
/// left incomplete is `(END OF FILE)`, a regex `unterminated regexp at end of file`.
#[test]
fn test_awk_errors_at_the_end_of_a_program_file_7() {
    run_file_cases(&[
        (
            "BEGIN { (x) /= 2 }\n",
            "",
            "",
            "awk: {file}:1: BEGIN { (x) /= 2 }\nawk: {file}:1:              ^ unterminated regexp\n",
            1,
        ),
        (
            "BEGIN { x = 1 \\",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 \\\nawk: {file}:1:               ^ backslash not last character on line\n",
            1,
        ),
        (
            "BEGIN { x = 1 \\\n",
            "",
            "",
            "awk: {file}:2: BEGIN { x = 1 \\\nawk: {file}:2:                ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = 1 } \\",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } \\\nawk: {file}:1:                 ^ backslash not last character on line\n",
            1,
        ),
        ("BEGIN { x = 1 } \\\n", "", "", "", 0),
        (
            "BEGIN { x = 1 }\n\\",
            "",
            "",
            "awk: {file}:2: \\\nawk: {file}:2: ^ backslash not last character on line\n",
            1,
        ),
        ("BEGIN { x = 1 }\n\\\n", "", "", "", 0),
        (
            "BEGIN { x = 1 \\\n",
            "",
            "",
            "awk: {file}:2: BEGIN { x = 1 \\\nawk: {file}:2:                ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = 1 \\\n  + 2",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2:     ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 \\\n  + 2\n",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2: ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 &&\\",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 &&\\\nawk: {file}:1:                 ^ backslash not last character on line\n",
            1,
        ),
    ]);
}

/// gawk's errors where a program file ends, with a newline last and without: a rule
/// left incomplete is `(END OF FILE)`, a regex `unterminated regexp at end of file`.
#[test]
fn test_awk_errors_at_the_end_of_a_program_file_8() {
    run_file_cases(&[
        (
            "BEGIN { x = 1 &&\\\n",
            "",
            "",
            "awk: {file}:3: BEGIN { x = 1 &&\\\nawk: {file}:3:                  ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = /a\\",
            "",
            "",
            "awk: {file}:1: BEGIN { x = /a\\\nawk: {file}:1:              ^ unterminated regexp ends with `\\' at end of file\n",
            1,
        ),
        (
            "BEGIN { x = /a\\\n",
            "",
            "",
            "awk: {file}:2: BEGIN { x = /a\\\nawk: {file}:2:              ^ unterminated regexp at end of file\n",
            1,
        ),
        (
            "BEGIN { x = \"a\\",
            "",
            "",
            "awk: {file}:1: BEGIN { x = \"a\\\nawk: {file}:1:             ^ unterminated string\n",
            1,
        ),
        (
            "BEGIN { x = \"a\\\n",
            "",
            "",
            "awk: {file}:2: BEGIN { x = \"a\\\nawk: {file}:2:             ^ unterminated string\n",
            1,
        ),
        (
            "BEGIN { x = \"a\\\nb",
            "",
            "",
            "awk: {file}:2: BEGIN { x = \"a\\\nawk: {file}:2:             ^ unterminated string\n",
            1,
        ),
        (
            "BEGIN { x = \"a\\\nb\n",
            "",
            "",
            "awk: {file}:2: BEGIN { x = \"a\\\nawk: {file}:2:             ^ unterminated string\n",
            1,
        ),
        (
            "BEGIN { x = /a\\\nb",
            "",
            "",
            "awk: {file}:2: BEGIN { x = /a\\\nawk: {file}:2:              ^ unterminated regexp at end of file\n",
            1,
        ),
        (
            "BEGIN { x = /a\\\nb\n",
            "",
            "",
            "awk: {file}:2: BEGIN { x = /a\\\nawk: {file}:2:              ^ unterminated regexp\n",
            1,
        ),
    ]);
}

/// gawk's errors where a program given as an argument ends: gawk adds a newline, so a
/// backslash last continues the line, and `&&` or a comment last reads to the end.
#[test]
fn test_awk_errors_at_the_end_of_a_program_argument_1() {
    run_cases(&[
        (
            "BEGIN {",
            "",
            "",
            "awk: cmd. line:1: BEGIN {\nawk: cmd. line:1:        ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN {\n",
            "",
            "",
            "awk: cmd. line:1: BEGIN {\nawk: cmd. line:1:        ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = 1",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1\nawk: cmd. line:1:              ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = 1 +",
            "",
            "",
            "awk: cmd. line:2: BEGIN { x = 1 +\nawk: cmd. line:2:                ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN {\n  x = 1 +",
            "",
            "",
            "awk: cmd. line:3:   x = 1 +\nawk: cmd. line:3:          ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN {\n  x = 1\n",
            "",
            "",
            "awk: cmd. line:2:   x = 1\nawk: cmd. line:2:        ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN {\n\tx = 1 +",
            "",
            "",
            "awk: cmd. line:3: \tx = 1 +\nawk: cmd. line:3: \t       ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = 1 &&",
            "",
            "",
            "awk: cmd. line:3: (END OF FILE)\nawk: cmd. line:3:               ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 &&\n",
            "",
            "",
            "awk: cmd. line:3: (END OF FILE)\nawk: cmd. line:3:               ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 ? 2 :",
            "",
            "",
            "awk: cmd. line:3: (END OF FILE)\nawk: cmd. line:3:                   ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1,",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1,\nawk: cmd. line:1:              ^ syntax error\n",
            1,
        ),
        (
            "function f(a,",
            "",
            "",
            "awk: cmd. line:1: function f(a,\nawk: cmd. line:1:              ^ unexpected newline or end of string\n",
            1,
        ),
    ]);
}

/// gawk's errors where a program given as an argument ends: gawk adds a newline, so a
/// backslash last continues the line, and `&&` or a comment last reads to the end.
#[test]
fn test_awk_errors_at_the_end_of_a_program_argument_2() {
    run_cases(&[
        (
            "function f(a)\n",
            "",
            "",
            "awk: cmd. line:1: function f(a)\nawk: cmd. line:1:              ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = 1 # c",
            "",
            "",
            "awk: cmd. line:1: (END OF FILE)\nawk: cmd. line:1:               ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 # c\n",
            "",
            "",
            "awk: cmd. line:1: (END OF FILE)\nawk: cmd. line:1:               ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        ("# only", "", "", "", 0),
        (
            "BEGIN { x = /a/",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = /a/\nawk: cmd. line:1:                ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = 1 } }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 } }\nawk: cmd. line:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { delete",
            "",
            "",
            "awk: cmd. line:2: BEGIN { delete\nawk: cmd. line:2:               ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = 1 } BEGIN",
            "",
            "",
            "awk: cmd. line:1: BEGIN blocks must have an action part\n",
            1,
        ),
        (
            "BEGIN { x = 1 } END",
            "",
            "",
            "awk: cmd. line:1: END blocks must have an action part\n",
            1,
        ),
        (
            "{ x = 1 +   ",
            "",
            "",
            "awk: cmd. line:2: { x = 1 +   \nawk: cmd. line:2:             ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN   {   ",
            "",
            "",
            "awk: cmd. line:1: BEGIN   {   \nawk: cmd. line:1:             ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN {\n x = 1   ",
            "",
            "",
            "awk: cmd. line:2:  x = 1   \nawk: cmd. line:2:          ^ unexpected newline or end of string\n",
            1,
        ),
    ]);
}

/// gawk's errors where a program given as an argument ends: gawk adds a newline, so a
/// backslash last continues the line, and `&&` or a comment last reads to the end.
#[test]
fn test_awk_errors_at_the_end_of_a_program_argument_3() {
    run_cases(&[
        (
            "BEGIN { x = 1 + # c",
            "",
            "",
            "awk: cmd. line:2: BEGIN { x = 1 + # c\nawk: cmd. line:2:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN {\n x = 1 # c\n # d",
            "",
            "",
            "awk: cmd. line:3: (END OF FILE)\nawk: cmd. line:3:  ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN {\n # d",
            "",
            "",
            "awk: cmd. line:2: (END OF FILE)\nawk: cmd. line:2:  ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 +\n # d",
            "",
            "",
            "awk: cmd. line:2: BEGIN { x = 1 +\nawk: cmd. line:2:                ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "{ x = \"#\"",
            "",
            "",
            "awk: cmd. line:1: { x = \"#\"\nawk: cmd. line:1:          ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { next",
            "",
            "",
            "awk: cmd. line:1: error: `next' used in BEGIN action\nawk: cmd. line:1: BEGIN { next\nawk: cmd. line:1:             ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "@",
            "",
            "",
            "awk: cmd. line:2: @\nawk: cmd. line:2:  ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = /abc }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = /abc }\nawk: cmd. line:1:              ^ unterminated regexp\n",
            1,
        ),
        (
            "BEGIN { x = /abc",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = /abc\nawk: cmd. line:1:              ^ unterminated regexp\n",
            1,
        ),
        (
            "BEGIN { x = \"abc",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = \"abc\nawk: cmd. line:1:             ^ unterminated string\n",
            1,
        ),
        (
            "BEGIN { print 1 /= 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { print 1 /= 2 }\nawk: cmd. line:1:                  ^ unterminated regexp\n",
            1,
        ),
        (
            "BEGIN { (x) /= 2 }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { (x) /= 2 }\nawk: cmd. line:1:              ^ unterminated regexp\n",
            1,
        ),
    ]);
}

/// gawk's errors where a program given as an argument ends: gawk adds a newline, so a
/// backslash last continues the line, and `&&` or a comment last reads to the end.
#[test]
fn test_awk_errors_at_the_end_of_a_program_argument_4() {
    run_cases(&[
        (
            "BEGIN { x = 1 \\",
            "",
            "",
            "awk: cmd. line:2: BEGIN { x = 1 \\\nawk: cmd. line:2:                ^ unexpected newline or end of string\n",
            1,
        ),
        ("BEGIN { x = 1 } \\", "", "", "", 0),
        ("BEGIN { x = 1 }\n\\", "", "", "", 0),
        (
            "BEGIN { x = 1 \\\n",
            "",
            "",
            "awk: cmd. line:2: BEGIN { x = 1 \\\nawk: cmd. line:2:                ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = 1 \\\n  + 2",
            "",
            "",
            "awk: cmd. line:2:   + 2\nawk: cmd. line:2:      ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = 1 &&\\",
            "",
            "",
            "awk: cmd. line:3: BEGIN { x = 1 &&\\\nawk: cmd. line:3:                  ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = /a\\",
            "",
            "",
            "awk: cmd. line:2: BEGIN { x = /a\\\nawk: cmd. line:2:              ^ unterminated regexp at end of file\n",
            1,
        ),
        (
            "BEGIN { x = \"a\\",
            "",
            "",
            "awk: cmd. line:2: BEGIN { x = \"a\\\nawk: cmd. line:2:             ^ unterminated string\n",
            1,
        ),
        (
            "BEGIN { x = \"a\\\nb",
            "",
            "",
            "awk: cmd. line:2: BEGIN { x = \"a\\\nawk: cmd. line:2:             ^ unterminated string\n",
            1,
        ),
        (
            "BEGIN { x = /a\\\nb",
            "",
            "",
            "awk: cmd. line:2: BEGIN { x = /a\\\nawk: cmd. line:2:              ^ unterminated regexp\n",
            1,
        ),
    ]);
}

/// gawk's lexer looks a character past most tokens, so one last in a file meets its
/// end: a syntax error there is `(END OF FILE)`, as in gawk (`do x++ while`); a `}` or a
/// `)` is not looked past. A function's header fails at the token gawk's parser meets,
/// a newline but after a comma `unexpected newline`; it failed at `function`.
#[test]
fn test_awk_errors_at_the_last_token_of_a_program_file_1() {
    run_file_cases(&[
        (
            "BEGIN { do x++ while",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:                ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { do x++ while\n",
            "",
            "",
            "awk: {file}:1: BEGIN { do x++ while\nawk: {file}:1:                ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { do x++ while   ",
            "",
            "",
            "awk: {file}:1: BEGIN { do x++ while   \nawk: {file}:1:                ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { do x++ while   \n",
            "",
            "",
            "awk: {file}:1: BEGIN { do x++ while   \nawk: {file}:1:                ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } }",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } }\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } }\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } }\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } )",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } )\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } )\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } )\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } ]",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:                 ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 } ]\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } ]\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
    ]);
}

/// gawk's lexer looks a character past most tokens, so one last in a file meets its
/// end: a syntax error there is `(END OF FILE)`, as in gawk (`do x++ while`); a `}` or a
/// `)` is not looked past. A function's header fails at the token gawk's parser meets,
/// a newline but after a comma `unexpected newline`; it failed at `function`.
#[test]
fn test_awk_errors_at_the_last_token_of_a_program_file_2() {
    run_file_cases(&[
        (
            "BEGIN { x = 1 } else",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:                 ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 } else\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } else\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } ,",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } ,\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } ,\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } ,\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } in",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:                 ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 } in\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } in\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } *",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:                 ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 } *\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } *\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } =",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:                 ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 } =\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } =\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
    ]);
}

/// gawk's lexer looks a character past most tokens, so one last in a file meets its
/// end: a syntax error there is `(END OF FILE)`, as in gawk (`do x++ while`); a `}` or a
/// `)` is not looked past. A function's header fails at the token gawk's parser meets,
/// a newline but after a comma `unexpected newline`; it failed at `function`.
#[test]
fn test_awk_errors_at_the_last_token_of_a_program_file_3() {
    run_file_cases(&[
        (
            "BEGIN { if (x) else",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:                ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { if (x) else\n",
            "",
            "",
            "awk: {file}:1: BEGIN { if (x) else\nawk: {file}:1:                ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } ~",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } ~\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } ~\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } ~\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } $",
            "",
            "",
            "awk: {file}:2: (END OF FILE)\nawk: {file}:2:                 ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 } $\n",
            "",
            "",
            "awk: {file}:2: BEGIN { x = 1 } $\nawk: {file}:2:                  ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = 1 } {",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:                 ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 } {\n",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1: ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        ("BEGIN { x = 1 } ;", "", "", "", 0),
        ("BEGIN { x = 1 } ;\n", "", "", "", 0),
    ]);
}

/// gawk's lexer looks a character past most tokens, so one last in a file meets its
/// end: a syntax error there is `(END OF FILE)`, as in gawk (`do x++ while`); a `}` or a
/// `)` is not looked past. A function's header fails at the token gawk's parser meets,
/// a newline but after a comma `unexpected newline`; it failed at `function`.
#[test]
fn test_awk_errors_at_the_last_token_of_a_program_file_4() {
    run_file_cases(&[
        (
            "BEGIN { x = 1 } [\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } [\nawk: {file}:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } BEGIN BEGIN",
            "",
            "",
            "awk: {file}:1: (END OF FILE)\nawk: {file}:1:                       ^ source files / command-line arguments must contain complete functions or rules\n",
            1,
        ),
        (
            "BEGIN { x = 1 } BEGIN BEGIN\n",
            "",
            "",
            "awk: {file}:1: BEGIN { x = 1 } BEGIN BEGIN\nawk: {file}:1:                       ^ syntax error\n",
            1,
        ),
        (
            "function f(a\n, b) { }",
            "",
            "",
            "awk: {file}:2: function f(a\nawk: {file}:2:             ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "function f(a\n, b) { }\n",
            "",
            "",
            "awk: {file}:2: function f(a\nawk: {file}:2:             ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "function f(a b) { }",
            "",
            "",
            "awk: {file}:1: function f(a b) { }\nawk: {file}:1:              ^ syntax error\n",
            1,
        ),
        (
            "function f(a b) { }\n",
            "",
            "",
            "awk: {file}:1: function f(a b) { }\nawk: {file}:1:              ^ syntax error\n",
            1,
        ),
        (
            "function f a) { }",
            "",
            "",
            "awk: {file}:1: function f a) { }\nawk: {file}:1:            ^ syntax error\n",
            1,
        ),
        (
            "function f a) { }\n",
            "",
            "",
            "awk: {file}:1: function f a) { }\nawk: {file}:1:            ^ syntax error\n",
            1,
        ),
        (
            "function f\n(a) { }",
            "",
            "",
            "awk: {file}:2: function f\nawk: {file}:2:           ^ unexpected newline or end of string\n",
            1,
        ),
    ]);
}

/// gawk's lexer looks a character past most tokens, so one last in a file meets its
/// end: a syntax error there is `(END OF FILE)`, as in gawk (`do x++ while`); a `}` or a
/// `)` is not looked past. A function's header fails at the token gawk's parser meets,
/// a newline but after a comma `unexpected newline`; it failed at `function`.
#[test]
fn test_awk_errors_at_the_last_token_of_a_program_file_5() {
    run_file_cases(&[
        (
            "function f\n(a) { }\n",
            "",
            "",
            "awk: {file}:2: function f\nawk: {file}:2:           ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "func f(a;) { }",
            "",
            "",
            "awk: {file}:1: func f(a;) { }\nawk: {file}:1:         ^ syntax error\n",
            1,
        ),
        (
            "func f(a;) { }\n",
            "",
            "",
            "awk: {file}:1: func f(a;) { }\nawk: {file}:1:         ^ syntax error\n",
            1,
        ),
        (
            "function f(a, \n\n b, \n c\n) { }",
            "",
            "",
            "awk: {file}:5:  c\nawk: {file}:5:   ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "function f(a, \n\n b, \n c\n) { }\n",
            "",
            "",
            "awk: {file}:5:  c\nawk: {file}:5:   ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "function f(a, # c\n b) { return b } BEGIN { print f(1, 2) }",
            "",
            "2\n",
            "",
            0,
        ),
        (
            "function f(a, # c\n b) { return b } BEGIN { print f(1, 2) }\n",
            "",
            "2\n",
            "",
            0,
        ),
        (
            "BEGIN { }\nfunction g(x y) { }",
            "",
            "",
            "awk: {file}:2: function g(x y) { }\nawk: {file}:2:              ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { }\nfunction g(x y) { }\n",
            "",
            "",
            "awk: {file}:2: function g(x y) { }\nawk: {file}:2:              ^ syntax error\n",
            1,
        ),
    ]);
}

/// The same programs given as an argument, which gawk ends with a newline.
#[test]
fn test_awk_errors_in_a_function_header_and_last_tokens_1() {
    run_cases(&[
        (
            "BEGIN { do x++ while",
            "",
            "",
            "awk: cmd. line:1: BEGIN { do x++ while\nawk: cmd. line:1:                ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { do x++ while   ",
            "",
            "",
            "awk: cmd. line:1: BEGIN { do x++ while   \nawk: cmd. line:1:                ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } }",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 } }\nawk: cmd. line:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } )",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 } )\nawk: cmd. line:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } ]",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 } ]\nawk: cmd. line:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } else",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 } else\nawk: cmd. line:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } ,",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 } ,\nawk: cmd. line:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } in",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 } in\nawk: cmd. line:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } *",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 } *\nawk: cmd. line:1:                 ^ syntax error\n",
            1,
        ),
    ]);
}

/// The same programs given as an argument, which gawk ends with a newline.
#[test]
fn test_awk_errors_in_a_function_header_and_last_tokens_2() {
    run_cases(&[
        (
            "BEGIN { x = 1 } =",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 } =\nawk: cmd. line:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { if (x) else",
            "",
            "",
            "awk: cmd. line:1: BEGIN { if (x) else\nawk: cmd. line:1:                ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } ~",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 } ~\nawk: cmd. line:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } $",
            "",
            "",
            "awk: cmd. line:2: BEGIN { x = 1 } $\nawk: cmd. line:2:                  ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "BEGIN { x = 1 } {",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 } {\nawk: cmd. line:1:                  ^ unexpected newline or end of string\n",
            1,
        ),
        ("BEGIN { x = 1 } ;", "", "", "", 0),
        (
            "BEGIN { x = 1 } [\n",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 } [\nawk: cmd. line:1:                 ^ syntax error\n",
            1,
        ),
        (
            "BEGIN { x = 1 } BEGIN BEGIN",
            "",
            "",
            "awk: cmd. line:1: BEGIN { x = 1 } BEGIN BEGIN\nawk: cmd. line:1:                       ^ syntax error\n",
            1,
        ),
        (
            "function f(a\n, b) { }",
            "",
            "",
            "awk: cmd. line:2: function f(a\nawk: cmd. line:2:             ^ unexpected newline or end of string\n",
            1,
        ),
    ]);
}

/// The same programs given as an argument, which gawk ends with a newline.
#[test]
fn test_awk_errors_in_a_function_header_and_last_tokens_3() {
    run_cases(&[
        (
            "function f(a b) { }",
            "",
            "",
            "awk: cmd. line:1: function f(a b) { }\nawk: cmd. line:1:              ^ syntax error\n",
            1,
        ),
        (
            "function f a) { }",
            "",
            "",
            "awk: cmd. line:1: function f a) { }\nawk: cmd. line:1:            ^ syntax error\n",
            1,
        ),
        (
            "function f\n(a) { }",
            "",
            "",
            "awk: cmd. line:2: function f\nawk: cmd. line:2:           ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "func f(a;) { }",
            "",
            "",
            "awk: cmd. line:1: func f(a;) { }\nawk: cmd. line:1:         ^ syntax error\n",
            1,
        ),
        (
            "function f(a, \n\n b, \n c\n) { }",
            "",
            "",
            "awk: cmd. line:5:  c\nawk: cmd. line:5:   ^ unexpected newline or end of string\n",
            1,
        ),
        (
            "function f(a, # c\n b) { return b } BEGIN { print f(1, 2) }",
            "",
            "2\n",
            "",
            0,
        ),
        (
            "BEGIN { }\nfunction g(x y) { }",
            "",
            "",
            "awk: cmd. line:2: function g(x y) { }\nawk: cmd. line:2:              ^ syntax error\n",
            1,
        ),
    ]);
}

// posixutils-rs awk tests from 96bd8a37 to its 0.9.1 (2026-10-15). Where cash-awk follows
// gawk, a test expects gawk's words and results; bytes as characters in the C locale,
// RS matches of the empty string and a concatenation in a `getline <` file are not taken.

/// Run the program file `tests/awk/<name>.awk` in a UTF-8 locale and compare
/// its output with `<name>.out`. Characters are bytes in the C locale the
/// harness defaults to, so a test of multibyte characters names its locale.

// XBD 12.2, Guideline 7: an option-argument may begin with '-'. Each option
// below used to have the word after it refused as an unknown option.
#[test]
fn awk_option_argument_may_begin_with_hyphen() {
    for opt in ["-F", "-f", "-v"] {
        plib::testing::assert_hyphen_option_argument("awk", &[opt, "-zq", "--help"]);
    }
}

#[test]
fn awk_field_separator_begins_with_hyphen() {
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec![
            String::from("-F"),
            String::from("-:"),
            String::from("{ print $2 }"),
        ],
        stdin_data: String::from("a-:b\n"),
        expected_out: String::from("b\n"),
        expected_err: String::new(),
        expected_exit_code: 0,
    });
}

// A '#' inside a regular expression literal, or inside a string, is part of
// that token and does not start a comment. autoconf's config.status uses
// `/^[\t ]*#[\t ]*(define|undef)[\t ]+/` and `sub(/#.*/, "")`.
#[test]
fn awk_hash_inside_regex_literal_is_not_a_comment() {
    let cases = [
        ("/#/", "x\na#b\n", "a#b\n"),
        ("/^#AT_START_/", "#AT_START_1\nAT_START_\n", "#AT_START_1\n"),
        (
            "/^[\\t ]*#[\\t ]*(define|undef)[\\t ]+/ { print $2 }",
            "# define FOO 1\n#undef BAR\nint x;\n",
            "define\nBAR\n",
        ),
        ("{ sub(/#.*/, \"\"); print }", "keep # drop\n", "keep \n"),
        ("/[#]/ { print \"br\" }", "a#\nb\n", "br\n"),
        ("{ print \"a#b\" } # trailing comment", "x\n", "a#b\n"),
        ("/a b/", "ab\na b\n", "a b\n"),
    ];
    for (program, input, output) in cases {
        run_test(TestPlan {
            cmd: String::from("awk"),
            args: vec![String::from(program)],
            stdin_data: String::from(input),
            expected_out: String::from(output),
            expected_err: String::new(),
            expected_exit_code: 0,
        });
    }
}

/// Run awk with `env` added to its environment and return its standard output
/// as raw bytes, asserting that it succeeded without diagnostics.
fn awk_bytes_with_env(args: &[&str], stdin: &[u8], env: &[(&str, &str)]) -> Vec<u8> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let output = plib::testing::run_test_base_with_env("awk", &args, stdin, env);
    assert_eq!(String::from_utf8_lossy(&output.stderr), "", "{args:?}");
    assert_eq!(output.status.code(), Some(0), "{args:?}");
    output.stdout
}

// In a single-byte locale a character is a byte: `%c` with a numeric
// argument writes the one byte whose value is the argument (modulo 256, as
// gawk and mawk do), never a UTF-8 encoding of it, and input bytes reach the
// output unchanged.
/// Bytes that are not UTF-8 are written back as they were read, as gawk writes them
/// (cash keeps UTF-8 in the C locale, where gawk counts bytes as characters).
#[test]
fn awk_bytes_that_are_not_utf8_pass_through() {
    let c = [("LC_ALL", "C")];
    let cases: [(&str, &[u8], &[u8]); 4] = [
        (
            "{ print; print $2 }",
            b"caf\xe9 cr\xe8me\n",
            b"caf\xe9 cr\xe8me\ncr\xe8me\n",
        ),
        (
            "{ sub(/b/, \"X\"); print toupper($0) }",
            b"a\xe9b\n",
            b"A\xe9X\n",
        ),
        (
            "{ printf \"%s|%c|\\n\", $0, substr($0, 2) }",
            b"x\xff\n",
            b"x\xff|\xff|\n",
        ),
        (
            "{ print | \"cat\" }",
            b"\x80\x81 \xc3\xa9\n",
            b"\x80\x81 \xc3\xa9\n",
        ),
    ];
    for (program, input, expected) in cases {
        assert_eq!(
            awk_bytes_with_env(&[program], input, &c),
            expected,
            "{program}"
        );
    }
}

// In a UTF-8 locale `%c` writes the character with that code point.
#[test]
fn awk_printf_c_writes_utf8_in_a_utf8_locale() {
    let Some(locale) = plib::testing::utf8_locale() else {
        return;
    };
    let env = [("LC_ALL", locale.as_str())];
    let out = awk_bytes_with_env(
        &["BEGIN { printf(\"%c|%c\", 200, \"éx\"); s = \"é\"; print \"\", length(s) }"],
        b"",
        &env,
    );
    assert_eq!(out, "È|é 1\n".as_bytes());
}

// gsub replaces non-overlapping matches, and an empty match right where the
// previous match ended is not another one: gawk, mawk and the one true awk
// all turn "abc" into "XaXcX" for gsub(/b*/, "X").
#[test]
fn awk_gsub_skips_an_empty_match_after_a_match() {
    let cases = [
        ("{ gsub(/b*/, \"X\"); print }", "abc\n", "XaXcX\n"),
        ("{ n = gsub(/b*/, \"X\"); print n }", "abbc\n", "3\n"),
        ("{ gsub(/x*/, \"-\"); print }", "abc\n", "-a-b-c-\n"),
        ("{ gsub(/a*/, \"X\"); print }", "aab\n", "XbX\n"),
    ];
    for (program, input, output) in cases {
        run_test(TestPlan {
            cmd: String::from("awk"),
            args: vec![String::from(program)],
            stdin_data: String::from(input),
            expected_out: String::from(output),
            expected_err: String::new(),
            expected_exit_code: 0,
        });
    }
}

/// Run awk on `program` with a 20 second limit, so that a hang fails the test
/// instead of the whole run; returns stdout, stderr and the exit status.
fn awk_with_deadline(program: &str) -> (String, String, Option<i32>) {
    awk_with_deadline_input(program, "")
}

/// `awk_with_deadline`, with `input` on standard input.
fn awk_with_deadline_input(program: &str, input: &str) -> (String, String, Option<i32>) {
    use std::io::Write;
    use std::time::{Duration, Instant};
    let mut child = std::process::Command::new(plib::testing::get_binary_path("awk"))
        .arg(program)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // the input is small, so writing it all before reading cannot deadlock;
    // awk may exit without reading it
    let mut stdin = child.stdin.take().unwrap();
    if let Err(error) = stdin.write_all(input.as_bytes()) {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }
    drop(stdin);
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(20) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("awk never finished with {program:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code(),
    )
}

// After a syntax error awk looks for more errors by reparsing from each later
// `}`, BEGIN, END or `function`.  When the reparse from a keyword failed too,
// the next search found that same keyword again at offset 0 and awk spun for
// ever; texinfo's texindex.awk hung bash's documentation build this way.
#[test]
fn awk_syntax_error_before_a_failing_function_terminates() {
    let programs = [
        "BEGIN { @ }\nfunction f() { @ }\n",
        "BEGIN { @ }\nBEGIN { @ }\n",
        "BEGIN { @ }\nEND { @ }\nEND { @ }\n",
        "{ @ } function",
    ];
    for program in programs {
        let (stdout, stderr, status) = awk_with_deadline(program);
        assert_eq!(stdout, "", "{program:?}");
        assert!(!stderr.is_empty(), "{program:?}");
        assert_ne!(status, Some(0), "{program:?}");
    }
}

// A keyword is a whole word, and only BEGIN and END are reserved, in
// capitals.  `begin`, `end` and `foreach` are ordinary names, and so is a
// name that starts with a keyword: `nextchar` is not `next` followed by
// `char`, `exitcode = 4` does not exit, and `elsewhere` after an `if` is not
// its `else`.  texindex.awk has `function join(array, start, end, sep)` and
// `nextchar = kchars[3]`.
#[test]
fn awk_keywords_are_whole_words() {
    let cases = [
        ("BEGIN { nextchar = 1; print nextchar }", "1\n"),
        (
            "BEGIN { breakx = 2; continued = 3; print breakx, continued }",
            "2 3\n",
        ),
        ("BEGIN { exitcode = 4; print exitcode }", "4\n"),
        (
            "BEGIN { returned = 5; doit = 6; print returned, doit }",
            "5 6\n",
        ),
        (
            "BEGIN { printer = 7; printfx = 8; print printer, printfx }",
            "7 8\n",
        ),
        (
            "BEGIN { deleted = 9; getlines = 10; print deleted, getlines }",
            "9 10\n",
        ),
        (
            "BEGIN { if (0) x = 1\nelsewhere = 11; print elsewhere }",
            "11\n",
        ),
        ("BEGIN { if (0) x = 1; else print 12 }", "12\n"),
        (
            "function j(a, start, end) { return start end } BEGIN { print j(0, 1, 2) }",
            "12\n",
        ),
        ("BEGIN { begin = 3; end = 4; print begin + end }", "7\n"),
        ("BEGIN { foreach = \"f\"; print foreach }", "f\n"),
        ("BEGIN { endx = 1; Begin = 2; print endx, Begin }", "1 2\n"),
    ];
    for (program, output) in cases {
        run_test(TestPlan {
            cmd: String::from("awk"),
            args: vec![String::from(program)],
            stdin_data: String::new(),
            expected_out: String::from(output),
            expected_err: String::new(),
            expected_exit_code: 0,
        });
    }
}

// A newline may follow `&&`, `||` and the comma of a parameter list, with a
// comment before it (POSIX awk, Lexical Conventions); texindex.awk relies on
// all three.
#[test]
fn awk_newline_after_and_or_and_parameter_comma() {
    let cases = [
        ("BEGIN { if (1 &&\n 2) print \"and\" }", "and\n"),
        ("BEGIN { if (0 ||  # why\n\n 2) print \"or\" }", "or\n"),
        ("BEGIN { x = 1 &&\n 0; print x }", "0\n"),
        (
            "function q(a,\t# parameters\n\tb) { return a b }\nBEGIN { print q(1, 2) }",
            "12\n",
        ),
        (
            "function q(a,\n\n b,\n c) { return c }\nBEGIN { print q(1, 2, 3) }",
            "3\n",
        ),
        ("/x/ &&\n/y/ { print }", "xy\n"),
    ];
    for (program, output) in cases {
        run_test(TestPlan {
            cmd: String::from("awk"),
            args: vec![String::from(program)],
            stdin_data: String::from("xy\nx\n"),
            expected_out: String::from(output),
            expected_err: String::new(),
            expected_exit_code: 0,
        });
    }
}

// Each of the three clauses of `for (init; cond; update)` may be empty, and
// so may the body; an empty condition is true.  The compiler took the
// children in order and panicked when one was missing (texindex.awk has
// `for (; i <= j; i++)`).
#[test]
fn awk_for_clauses_may_be_empty() {
    let cases = [
        (
            "BEGIN { i = 1; for (; i <= 3; i++) printf i; print \"\" }",
            "123\n",
        ),
        (
            "BEGIN { for (i = 1; ; i++) if (i > 2) break; print i }",
            "3\n",
        ),
        ("BEGIN { for (i = 1; i < 3;) i++; print i }", "3\n"),
        (
            "BEGIN { for (;;) { n++; if (n == 4) break }; print n }",
            "4\n",
        ),
        ("BEGIN { for (i = 0; i < 5; i++); print i }", "5\n"),
        ("BEGIN { for (i = 0; i < 6; i++) {}\nprint i }", "6\n"),
        (
            "BEGIN { for (i = 0; i < 4; i++) { if (i % 2) continue; s = s i }; print s }",
            "02\n",
        ),
        (
            "BEGIN { i = 0; for (;; i++) if (i == 7) break; print i }",
            "7\n",
        ),
    ];
    for (program, output) in cases {
        run_test(TestPlan {
            cmd: String::from("awk"),
            args: vec![String::from(program)],
            stdin_data: String::new(),
            expected_out: String::from(output),
            expected_err: String::new(),
            expected_exit_code: 0,
        });
    }
}

// An unset variable passed to a function becomes an array in the caller when
// the function uses its parameter as one (filling it with split, assigning
// an element, passing it on), as in gawk, mawk and the one true awk; a
// scalar assigned to the parameter stays local.  texindex.awk's char_split
// fills its caller's local with split(string, array, "").
#[test]
fn awk_unset_argument_becomes_the_callers_array() {
    let cases = [
        (
            "function f(arr) { split(\"x y\", arr) } BEGIN { f(b); print length(b), b[2] }",
            "2 y\n",
        ),
        (
            "function f(arr) { arr[1] = \"set\" } BEGIN { f(b); print b[1] }",
            "set\n",
        ),
        (
            "function f(arr) { return split(\"p q\", arr) }
             function g(  loc, n) { n = f(loc); return n \"-\" loc[2] }
             BEGIN { print g() }",
            "2-q\n",
        ),
        (
            "function h(a) { split(\"m n\", a) } function f(arr) { h(arr) }
             BEGIN { f(b); print b[2] }",
            "n\n",
        ),
        (
            "function g(b) { b[\"k\"] = \"v\" } function f(a) { g(a); return a[\"k\"] }
             BEGIN { print f(x), x[\"k\"] }",
            "v v\n",
        ),
        (
            "function f(a) { a = 5; return a } BEGIN { print f(x); print \"[\" x \"]\" }",
            "5\n[]\n",
        ),
        (
            "function f(a) { a++; a++; return a } function g(  l) { f(l); return \"[\" l \"]\" }
             BEGIN { print g() }",
            "[]\n",
        ),
        (
            "function f(a) { print \"r\" a } BEGIN { f(x); x = 3; print x }",
            "r\n3\n",
        ),
    ];
    for (program, output) in cases {
        run_test(TestPlan {
            cmd: String::from("awk"),
            args: vec![String::from(program)],
            stdin_data: String::new(),
            expected_out: String::from(output),
            expected_err: String::new(),
            expected_exit_code: 0,
        });
    }
}

// An original program written in the style of texinfo's texindex.awk, which
// hung, then failed to parse, then rejected every entry.  The expected
// output is gawk's (mawk agrees).
#[test]
fn test_awk_sort_index_entries() {
    test_awk!(sort_index_entries, "tests/awk/sort_index_entries.txt");
}

// An empty match is one match, wherever the search finds it: gsub(/$/, "X")
// appended "XX" to the record, because the empty match at the end was found
// again from its own position.
#[test]
fn awk_empty_match_at_end_is_counted_once() {
    let cases = [
        ("{ n = gsub(/$/, \"X\"); print n, $0 }", "1 abcX\n"),
        ("{ n = gsub(/b*$/, \"X\"); print n, $0 }", "1 abcX\n"),
        ("{ n = gsub(/c*$/, \"X\"); print n, $0 }", "1 abX\n"),
        ("{ n = gsub(/x*/, \"-\"); print n, $0 }", "4 -a-b-c-\n"),
    ];
    for (program, output) in cases {
        run_test(TestPlan {
            cmd: String::from("awk"),
            args: vec![String::from(program)],
            stdin_data: String::from("abc\n"),
            expected_out: String::from(output),
            expected_err: String::new(),
            expected_exit_code: 0,
        });
    }
}

// An empty match of a field separator does not separate fields, in split()
// and in FS alike, as in gawk and mawk: split("abc", a, /x*/) is one field,
// and /b*/ splits "abc" only at the "b".
#[test]
fn awk_empty_separator_match_does_not_split() {
    let cases = [
        ("{ n = split($0, a, /x*/); print n, a[1] }", "1 abc\n"),
        ("{ n = split($0, a, /b*/); print n, a[1], a[2] }", "2 a c\n"),
        ("{ n = split($0, a, /$/); print n, a[1] }", "1 abc\n"),
        ("BEGIN { FS = \"b*\" } { print NF, $1, $2 }", "2 a c\n"),
        ("BEGIN { FS = \"x*\" } { print NF, $1 }", "1 abc\n"),
    ];
    for (program, output) in cases {
        run_test(TestPlan {
            cmd: String::from("awk"),
            args: vec![String::from(program)],
            stdin_data: String::from("abc\n"),
            expected_out: String::from(output),
            expected_err: String::new(),
            expected_exit_code: 0,
        });
    }
}

// Each syntax error is reported once, at its own line and column: errors
// found after the first were placed relative to where the search for them
// resumed, and found again from every later checkpoint.
#[test]
fn awk_later_syntax_errors_report_their_own_position() {
    // gawk's form: the first error, its line and a caret under its column.
    let tmp = plib::tmp::TempDir::new().unwrap();
    let program = tmp.path().join("two.awk");
    std::fs::write(&program, "BEGIN { @ }\n\nfunction f() {\n  x = 1 @\n}\n").unwrap();
    let args = vec![String::from("-f"), program.to_string_lossy().into_owned()];
    let output = plib::testing::run_test_base("awk", &args, b"");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let file = program.to_string_lossy();
    assert_eq!(
        stderr,
        format!("awk: {file}:1: BEGIN {{ @ }}\nawk: {file}:1:           ^ syntax error\n")
    );
    assert_eq!(output.status.code(), Some(1));
}

// A string converts to a number as C's strtod reads it, skipping leading
// white space: " 12" + 1 is 13, and a field " 12 " is 12.
#[test]
fn awk_string_to_number_skips_leading_white_space() {
    let cases = [
        (
            "BEGIN { print \" 12\" + 1, \"\\t5\" * 2, \" +3\" + 0, \" -.5e1x\" + 0 }",
            "",
            "13 10 3 -5\n",
        ),
        (
            "BEGIN { print \"\\n7\" + 0, \"\\v8\" + 0, \"\\f9\" + 0, \"\\r4\" + 0 }",
            "",
            "7 8 9 4\n",
        ),
        ("{ print $1 + 0, $1 * 2 }", " 12 |x\n", "12 24\n"),
    ];
    for (program, input, output) in cases {
        run_test(TestPlan {
            cmd: String::from("awk"),
            args: vec![String::from("-F|"), String::from(program)],
            stdin_data: String::from(input),
            expected_out: String::from(output),
            expected_err: String::new(),
            expected_exit_code: 0,
        });
    }
}

// A field is a numeric string only if all of it, apart from leading and
// trailing blanks, looks like a number: "12abc", "12.5f" and "12 x" compare
// as strings ("12abc" > 9 is false), " 12 " and "+12" as numbers.
#[test]
fn awk_numeric_string_is_the_whole_field() {
    let cases = [
        ("12", "1"),
        ("12abc", "0"),
        ("12.5f", "0"),
        ("12 x", "0"),
        ("12e", "0"),
        ("+-12", "0"),
        ("--12", "0"),
        ("0x1A", "0"),
        (".", "0"),
        (" 12 ", "1"),
        ("\t12\t", "1"),
        ("+12", "1"),
        ("12e1", "1"),
        ("1e+1x", "0"),
        ("12.", "1"),
        (".12e2", "1"),
    ];
    let input: String = cases
        .iter()
        .map(|(field, _)| format!("{field}|\n"))
        .collect();
    let output: String = cases.iter().map(|(_, gt)| format!("{gt}\n")).collect();
    run_test(TestPlan {
        cmd: String::from("awk"),
        args: vec![String::from("-F|"), String::from("{ print ($1 > 9) }")],
        stdin_data: input,
        expected_out: output,
        expected_err: String::new(),
        expected_exit_code: 0,
    });
}

// Parsing must not take time exponential in how deeply expressions nest.
// Each parenthesised level was parsed about eight times over, by the
// alternatives that failed (`| getline`, `? :`, `(i, j) in`) before the one
// that matched, so five levels took a second and eight never finished.
#[test]
fn awk_deeply_nested_expressions_parse_quickly() {
    let depth = 40;
    let nested =
        |open: &str, close: &str| format!("{}1{}", open.repeat(depth), close.repeat(depth));
    let cases = [
        (format!("BEGIN {{ print {} }}", nested("(", ")")), "1\n"),
        (
            format!("BEGIN {{ x = {}; print x }}", nested("(", ")")),
            "1\n",
        ),
        (
            format!("BEGIN {{ print {} + 1 }}", nested("-(", ")")),
            "2\n",
        ),
        (
            format!("BEGIN {{ print {} }}", nested("length(", ")")),
            "1\n",
        ),
        (
            format!("BEGIN {{ print {} }}", nested("(1 ? ", " : 0)")),
            "1\n",
        ),
        (
            format!(
                "BEGIN {{ a[1, 2]; if (({}, 2) in a) print \"in\" }}",
                nested("(", ")")
            ),
            "in\n",
        ),
        (
            format!(
                "BEGIN {{ while ((\"echo x\" | getline v) > 0) print {} v }}",
                nested("(", ")")
            ),
            "1x\n",
        ),
    ];
    for (program, output) in cases {
        let (stdout, stderr, status) = awk_with_deadline(&program);
        assert_eq!(
            (stdout.as_str(), stderr.as_str()),
            (output, ""),
            "{program}"
        );
        assert_eq!(status, Some(0), "{program}");
    }
}

// Nested subscripts and field references parse in linear time too: each
// level was parsed once as a possible assignment target and again as an
// operand, so 20 levels of a[a[...]] never finished.  The check that skips
// the first parse must not miss a real assignment.
#[test]
fn awk_nested_lvalues_parse_quickly() {
    let depth = 40;
    let nested = |open: &str, inner: &str, close: &str| {
        format!("{}{inner}{}", open.repeat(depth), close.repeat(depth))
    };
    let cases = [
        (
            format!("BEGIN {{ a[1] = 1; print {} }}", nested("a[", "1", "]")),
            "1\n",
        ),
        (
            format!(
                "BEGIN {{ a[1] = 1; {} = 7; print a[1] }}",
                nested("a[", "1", "]")
            ),
            "7\n",
        ),
        (format!("{{ print {} }}", nested("$(", "1", ")")), "1\n"),
        (
            format!("{{ {} = \"z\"; print }}", nested("$(", "1", ")")),
            "z\n",
        ),
        // assignments the shape check cannot read must still be assignments
        (
            String::from("BEGIN { a[\"]\"] = 5; a[\"[\"] += 1; print a[\"]\"], a[\"[\"] }"),
            "5 1\n",
        ),
        (
            String::from("BEGIN { i = 4; a[i/2] = 3; a[i / 2] ^= 2; print a[2] }"),
            "9\n",
        ),
        (
            String::from("{ i = 0; $++i = \"z\"; $(i + 0) = $i \"y\"; print }"),
            "zy\n",
        ),
        (
            String::from("BEGIN { a[1, 2] = 3; a [1] = 4; print a[1, 2], a[1] }"),
            "3 4\n",
        ),
        (
            String::from("BEGIN { a[1] = 1; print (a[1] == 1), a[1] = 2, a[1] }"),
            "1 2 2\n",
        ),
        (String::from("{ x = $1; $1 = \"b\"; print x, $0 }"), "1 b\n"),
    ];
    for (program, output) in cases {
        let (stdout, stderr, status) = awk_with_deadline_input(&program, "1\n");
        assert_eq!(
            (stdout.as_str(), stderr.as_str()),
            (output, ""),
            "{program}"
        );
        assert_eq!(status, Some(0), "{program}");
    }
}

// `s = s t` appends to s in place, so building a string piece by piece is
// linear: it copied the whole of s two or three times per append, and two
// million appends never finished.  Half a million is a second here in a
// debug build, and copying, 10^11 bytes, would run past the test's time; two
// million took 10 s, near what the quick lane allows.  The result is the
// same as concatenating: a number on the left is converted with CONVFMT, a
// numeric string from input stops being numeric, and anything that may
// change s while the right-hand side is evaluated keeps the ordinary
// evaluation order.
#[test]
fn awk_append_to_a_variable_is_linear() {
    let cases = [
        (
            "BEGIN { for (i = 0; i < 500000; i++) s = s \"x\"; print length(s) }",
            "500000\n",
        ),
        (
            "function f(  l, i) { for (i = 0; i < 500000; i++) l = l \"ab\"; return length(l) }
             BEGIN { print f() }",
            "1000000\n",
        ),
        (
            "BEGIN { s = 0.1; CONVFMT = \"%.2f\"; s = s \"|\" 1 + 1 \"|\" 2; print s }",
            "0.10|2|2\n",
        ),
        ("{ s = $1; s = s \"\"; print (s < 9) }", "1\n"),
        ("BEGIN { s = \"a\"; s = s s s; print s }", "aaa\n"),
        ("BEGIN { s = \"a\"; s = s (s = \"b\"); print s }", "ab\n"),
        (
            "BEGIN { s = \"a\"; s = s sub(/a/, \"c\", s) s; print s }",
            "a1c\n",
        ),
        (
            "function g() { s = \"z\"; return \"y\" } BEGIN { s = \"a\"; s = s g(); print s }",
            "ay\n",
        ),
        ("BEGIN { s = \"a\"; s = s 1 < 2; print s }", "0\n"),
        ("BEGIN { s = \"a\"; s = s \"b\" ~ /ab/; print s }", "1\n"),
        ("BEGIN { s = 3; s = s - 1; print s }", "2\n"),
        (
            "BEGIN { x[1] = \"a\"; s = \"q\"; s = s x[1] substr(\"bcd\", 2) toupper(s); print s }",
            "qacdQ\n",
        ),
        ("{ $0 = $0 \"y\"; print $1, NF }", "10y 1\n"),
    ];
    for (program, output) in cases {
        let (stdout, stderr, status) = awk_with_deadline_input(program, "10\n");
        assert_eq!(
            (stdout.as_str(), stderr.as_str()),
            (output, ""),
            "{program}"
        );
        assert_eq!(status, Some(0), "{program}");
    }
}

// An empty match of a regular-expression RS does not end a record, as in
// mawk: RS = "()" or "x*" made every read return an empty record without
// consuming input, so awk printed empty records for ever (gawk's test
// rsnullre).  "b*" still separates at each run of b's.
#[test]
fn awk_empty_rs_match_does_not_end_a_record() {
    let cases = [
        (
            "BEGIN { RS = \"()\" } { printf \"<%s>\", $0 }",
            "foo\n",
            "<foo\n>",
        ),
        (
            "BEGIN { RS = \"x*\" } { printf \"<%s>\", $0 }",
            "foo\n",
            "<foo\n>",
        ),
    ];
    for (program, input, output) in cases {
        let (stdout, stderr, status) = awk_with_deadline_input(program, input);
        assert_eq!(
            (stdout.as_str(), stderr.as_str()),
            (output, ""),
            "{program}"
        );
        assert_eq!(status, Some(0), "{program}");
    }
}

// A runtime error after a function call reports the caller's location.
// Returning from a call kept the callee's table of source locations, so
// the report indexed past its end and awk panicked instead.
#[test]
fn awk_runtime_error_after_a_call_reports_the_caller() {
    // gawk prints a conversion it does not know as it is.
    let program = "function f(a, b, c, d, e) { return 1 }\nBEGIN { f(); printf(\"%z\") }";
    let (stdout, stderr, status) = awk_with_deadline(program);
    assert_eq!((stdout.as_str(), stderr.as_str()), ("%z", ""));
    assert_eq!(status, Some(0));
}

// `$` binds tighter than `++` and `--`, so `$NF++` increments the last
// field and `$i++` leaves i alone; awk incremented the field's index
// instead.  In `$$i++` the increment goes to the inner field, as in gawk
// and mawk, so `$$a++++` is `$($a++)++` (gawk's test parse1), and so is
// one after a unary operator: `$+i++` is `$(+(i++))` (gawk's test prec).
#[test]
fn awk_field_reference_binds_tighter_than_increment() {
    let cases = [
        ("{ $NF++; print }", "1 2\n", "1 3\n"),
        ("{ $2--; print }", "1 2\n", "1 1\n"),
        ("{ i = 1; print $i++; print i, $0 }", "5 6\n", "5\n1 6 6\n"),
        ("{ i = 1; print $$i++; print }", "2 5 6\n", "5\n3 5 6\n"),
        (
            "BEGIN { a = 3 } { print $$a++++; print }",
            "3 4 5 6 7 8 9\n",
            "7\n3 4 6 6 8 8 9\n",
        ),
        ("{ i = 1; print $++i, $i^2, $i-1 }", "5 6\n", "6 36 5\n"),
        ("{ i = 1; $!i++; $+i++; print i, $0 }", "5 6\n", "3 5 6\n"),
    ];
    for (program, input, output) in cases {
        let (stdout, stderr, status) = awk_with_deadline_input(program, input);
        assert_eq!(
            (stdout.as_str(), stderr.as_str()),
            (output, ""),
            "{program}"
        );
        assert_eq!(status, Some(0), "{program}");
    }
}

// An operand can have more than one unary operator: `!!x` was a syntax
// error.
#[test]
fn awk_operand_takes_several_unary_operators() {
    let (stdout, stderr, status) =
        awk_with_deadline("BEGIN { x = 2; print !!x, - -x, !-x, -!x, !!!\"\", 1 - -1 }");
    assert_eq!((stdout.as_str(), stderr.as_str()), ("1 2 0 0 1 2\n", ""));
    assert_eq!(status, Some(0));
}

// getline is an operand: `getline > 0` compares its result and `getline x
// y` concatenates y to it, where both were syntax errors (gawk's tests
// getline, getline2, getline3 and inputred).  As in gawk, `getline < f`
// takes no concatenation into the file name, and `cmd | getline` binds
// tighter than a comparison but looser than a concatenation.
#[test]
fn awk_getline_is_an_operand() {
    let cases = [
        (
            "NR == 1 { while (getline > 0) n++; print n }",
            "a\nb\nc\n",
            "2\n",
        ),
        (
            "NR == 1 { a = (getline x y); print a, x }",
            "l1\nl2\n",
            "1 l2\n",
        ),
        ("NR == 1 { print (getline x - 2), x }", "a\nb\n", "-1 b\n"),
        (
            "BEGIN { print (getline line < \"/dev/null\" > -1) }",
            "",
            "1\n",
        ),
        (
            "BEGIN { y = 7; print (\"echo 4\" | getline x y), x }",
            "",
            "17 4\n",
        ),
        ("BEGIN { \"echo a\" \"b\" | getline; print }", "", "ab\n"),
        (
            "BEGIN { while (\"echo z\" | getline line > 0) n++; print n, line }",
            "",
            "1 z\n",
        ),
        (
            "BEGIN { print (\"echo 5\" | getline x + 1), x }",
            "",
            "2 5\n",
        ),
    ];
    for (program, input, output) in cases {
        let (stdout, stderr, status) = awk_with_deadline_input(program, input);
        assert_eq!(
            (stdout.as_str(), stderr.as_str()),
            (output, ""),
            "{program}"
        );
        assert_eq!(status, Some(0), "{program}");
    }
}

// A print list can hold `==`, `!=`, `<` and `<=` without parentheses, as
// in gawk, mawk and busybox awk; `>` still redirects (gawk's test
// gsubtst3).
#[test]
fn awk_print_list_takes_comparisons() {
    let program = "BEGIN { x = 1; print x == 1, x != 1, x < 2, x <= 0, x == 1 ? \"y\" : \"n\"; printf \"%s %s\\n\", x == 1, x != 1 ? \"a\" : \"b\"; print x == 1 > \"/dev/stdout\" }";
    let (stdout, stderr, status) = awk_with_deadline(program);
    assert_eq!(
        (stdout.as_str(), stderr.as_str()),
        ("1 0 1 0 y\n1 b\n1\n", "")
    );
    assert_eq!(status, Some(0));
}

// `exit` in BEGIN skips the input but runs the END actions, `exit` with no
// expression keeps the status of an earlier `exit expr` (gawk's test
// exitval3), and `exit` in a function called from a pattern ends the
// program instead of panicking.
#[test]
fn awk_exit_skips_input_and_keeps_its_status() {
    let cases = [
        (
            "BEGIN { exit 3 } { print \"main\" } END { print \"end\" }",
            "end\n",
            3,
        ),
        ("BEGIN { exit 42 } END { exit }", "", 42),
        ("{ exit 4 } END { exit }", "", 4),
        (
            "function f() { exit 3 } f() { print \"matched\" } END { print \"end\" }",
            "end\n",
            3,
        ),
        (
            "BEGIN { exit 1 } END { print \"e1\"; exit 6; print \"no\" } END { print \"e2\" }",
            "e1\n",
            6,
        ),
    ];
    for (program, output, exit_status) in cases {
        let (stdout, stderr, status) = awk_with_deadline_input(program, "x\n");
        assert_eq!(
            (stdout.as_str(), stderr.as_str()),
            (output, ""),
            "{program}"
        );
        assert_eq!(status, Some(exit_status), "{program}");
    }
}

// Plain getline reads the main input: in BEGIN it opens the first file
// (standard input here), and at the end of a file it goes on to the next
// operand, performing assignments on the way; in END there is no more
// input (gawk's tests getline2 and gsubtst3).  FILENAME is empty until a
// file is opened.
#[test]
fn awk_getline_reads_the_main_input() {
    let cases = [
        (
            "BEGIN { while ((getline l) > 0) n++; print n, NR }",
            "3 3\n",
        ),
        (
            "BEGIN { getline; print \"begin\", $0, NR } { print \"main\", $0, NR }",
            "begin a 1\nmain b 2\nmain c 3\n",
        ),
        (
            "BEGIN { printf \"[%s]\", FILENAME; getline; print FILENAME }",
            "[]-\n",
        ),
    ];
    for (program, output) in cases {
        let (stdout, stderr, status) = awk_with_deadline_input(program, "a\nb\nc\n");
        assert_eq!(
            (stdout.as_str(), stderr.as_str()),
            (output, ""),
            "{program}"
        );
        assert_eq!(status, Some(0), "{program}");
    }
    test_awk(
        vec![
            "{ n = 0; while ((getline l) > 0) n++; print FILENAME, FNR, NR, n, x } END { print NR, (getline l) }".to_string(),
            "tests/awk/test_data2.txt".to_string(),
            "x=5".to_string(),
            "tests/awk/test_data3.txt".to_string(),
        ],
        "tests/awk/test_data3.txt 6 11 10 5\n11 0\n",
    );
}

// Deleting the element at the end of an array's storage, the last one
// added or a slot a `for (k in a)` loop emptied, panicked.
#[test]
fn awk_delete_the_last_stored_element() {
    let cases = [
        (
            "BEGIN { a[1]; a[2]; delete a[2]; print length(a); for (k in a) print k }",
            "1\n1\n",
        ),
        (
            "BEGIN { a[1]; a[2]; a[3]; for (k in a) if (k == 3) delete a[k]; delete a[1]; print length(a); for (k in a) print k }",
            "1\n2\n",
        ),
    ];
    for (program, output) in cases {
        let (stdout, stderr, status) = awk_with_deadline(program);
        assert_eq!(
            (stdout.as_str(), stderr.as_str()),
            (output, ""),
            "{program}"
        );
        assert_eq!(status, Some(0), "{program}");
    }
}

// `break` and `continue` work in a `for (k in a)` loop, and leaving one by
// `break`, `return` or `next` releases the array, which could then never
// be added to again.  Reading a missing element inside such a loop creates
// it instead of failing (gawk's test delarpm2); the loop does not visit
// it, as in gawk and mawk.
#[test]
fn awk_for_in_loop_can_be_left_and_its_array_added_to() {
    let cases = [
        (
            "BEGIN { a[1]; a[2]; a[3]; for (k in a) { n++; break }; for (k in a) { if (k == 2) continue; m++ }; a[4]; print n, m, length(a) }",
            "1 2 4\n",
        ),
        (
            "function f(arr, k) { for (k in arr) return k } BEGIN { a[1]; a[2]; f(a); a[3] = 1; print length(a) }",
            "3\n",
        ),
        (
            "{ for (k in seen) next } { seen[$0] } END { seen[\"z\"]; print length(seen) }",
            "2\n",
        ),
        (
            "BEGIN { a[1]; a[2]; for (k in a) if (a[k \"x\"] == \"\") n++; print n, length(a) }",
            "2 4\n",
        ),
        (
            "BEGIN { a[1]; a[2]; for (k in a) { delete a; a[\"q\"] }; print length(a) }",
            "1\n",
        ),
    ];
    for (program, output) in cases {
        let (stdout, stderr, status) = awk_with_deadline_input(program, "a\nb\nc\n");
        assert_eq!(
            (stdout.as_str(), stderr.as_str()),
            (output, ""),
            "{program}"
        );
        assert_eq!(status, Some(0), "{program}");
    }
}

// `break` and `continue` work in a do-while loop, where they were
// rejected as being outside a loop.
#[test]
fn awk_do_while_takes_break_and_continue() {
    let program = "BEGIN { do { n++; break } while (1); do { m++; if (m < 3) continue; break } while (1); i = 0; while (i < 2) { i++; do { break } while (1); k++ }; print n, m, k }";
    let (stdout, stderr, status) = awk_with_deadline(program);
    assert_eq!((stdout.as_str(), stderr.as_str()), ("1 3 2\n", ""));
    assert_eq!(status, Some(0));
}

// close() of a pipe returns the command's exit status, or 256 plus the
// signal that killed it, as gawk and mawk do (gawk's tests close_status
// and status-close); it returned 0 for any command.
#[test]
fn awk_close_of_a_pipe_returns_the_exit_status() {
    // Windows has no signal to end the command with, so no case of one.
    let program = "BEGIN { print \"x\" | \"cat >/dev/null; exit 9\"; print close(\"cat >/dev/null; exit 9\"); \"echo hi; exit 3\" | getline; print close(\"echo hi; exit 3\"); print close(\"echo hi; exit 3\") }";
    let (stdout, stderr, status) = awk_with_deadline(program);
    assert_eq!((stdout.as_str(), stderr.as_str()), ("9\n3\n-1\n", ""));
    assert_eq!(status, Some(0));
}

// The `#` flag follows printf(3): `%#x` of 0 has no 0x prefix, and `%#o`
// makes the first digit a zero even with a precision that leaves room for
// none (gawk's test printf1).
#[test]
fn awk_printf_alternative_form_of_zero() {
    let program =
        "BEGIN { printf \"%#.0o|%#x|%#X|%#o|%#.2o|%#x|%#5x|\\n\", 0, 0, 0, 0, 8, 255, 0 }";
    let (stdout, stderr, status) = awk_with_deadline(program);
    assert_eq!(
        (stdout.as_str(), stderr.as_str()),
        ("0|0|0|0|010|0xff|    0|\n", "")
    );
    assert_eq!(status, Some(0));
}

// printf `%s` counts its width and precision in characters, so a byte
// above 127 in the C locale is one column (gawk's test rebt8b1), and a
// precision that falls inside a UTF-8 character panicked.
#[test]
fn awk_printf_string_width_counts_characters() {
    let Some(locale) = plib::testing::utf8_locale() else {
        return;
    };
    let env = [("LC_ALL", locale.as_str())];
    let out = awk_bytes_with_env(
        &["BEGIN { printf \"%-5s|%.1s|%3s|\\n\", \"éa\", \"éab\", \"é\" }"],
        b"",
        &env,
    );
    assert_eq!(out, "éa   |é|  é|\n".as_bytes());
}

// The escape sequences of awk EREs, which regcomp does not know, are
// translated: `/\t/` never matched a tab, nor `/\141/` an "a", in an ERE
// token or a string used as one.  Inside a bracket expression a backslash
// escapes too, as in gawk and mawk (gawk's test regrange), and `\8` is a
// plain 8, not a back-reference (gawk's test back89).
#[test]
fn awk_ere_escape_sequences() {
    let cases = [
        (
            "/a\\tb/ { print \"tab\" } $0 ~ \"a\\\\tb\" { print \"dyntab\" } /[\\t]/ { print \"brtab\" } /\\141/ { print \"oct\" }",
            "a\tb\n",
            "tab\ndyntab\nbrtab\noct\n",
        ),
        ("/a\\8b/ { print \"a8b\" }", "a8b\n", "a8b\n"),
        ("{ gsub(/\\//, \"|\"); print }", "a/b\n", "a|b\n"),
        (
            "/a\\.b/ { print \"wrong\" } { print \"ok\" }",
            "axb\n",
            "ok\n",
        ),
        (
            "{ print (\"\\\\\" ~ /[\\\\]/), (\"]\" ~ /[\\]]/), (\"a\" ~ /[\\]a]/), (\"-\" ~ /[a\\-z]/), (\"b\" ~ /[a\\-z]/), (\"^\" ~ /[\\^x]/) }",
            "x\n",
            "1 1 1 1 0 1\n",
        ),
    ];
    for (program, input, output) in cases {
        let (stdout, stderr, status) = awk_with_deadline_input(program, input);
        assert_eq!(
            (stdout.as_str(), stderr.as_str()),
            (output, ""),
            "{program}"
        );
        assert_eq!(status, Some(0), "{program}");
    }
    // a range is the C locale's byte order only there
    let out = awk_bytes_with_env(
        &["{ print ($0 ~ /[\\[-\\]]/) }"],
        b"\\\n",
        &[("LC_ALL", "C")],
    );
    assert_eq!(out, b"1\n");
}

// A `/` inside a bracket expression does not end an ERE token, as in gawk
// and mawk (gawk's test regexpbrack).
#[test]
fn awk_slash_in_bracket_expression_of_ere_token() {
    let program = "/^[]+()0-9.,$%/'\"-]*$/ { print \"num\", $0 } /[/]/ { print \"m\" } /[[:alpha:]/]x/ { print \"no\" }";
    let (stdout, stderr, status) = awk_with_deadline_input(program, "a/b\n12/3\n");
    assert_eq!((stdout.as_str(), stderr.as_str()), ("m\nnum 12/3\nm\n", ""));
    assert_eq!(status, Some(0));
}

// sub and gsub leave their target alone when nothing matched: a field was
// assigned anyway, which rebuilt $0 with OFS (gawk's test gsubtst7), and a
// number became a string.
#[test]
fn awk_sub_without_a_match_assigns_nothing() {
    let cases = [
        (
            "{ gsub(\"foo\", \"bar\", $1); print; sub(/x/, \"y\", $2); print; gsub(/q/, \"z\"); print }",
            " aaa  b\n",
            " aaa  b\n aaa  b\n aaa  b\n",
        ),
        (
            "BEGIN { x = 10; gsub(/q/, \"\", x); print (x < 9) }",
            "",
            "0\n",
        ),
    ];
    for (program, input, output) in cases {
        let (stdout, stderr, status) = awk_with_deadline_input(program, input);
        assert_eq!(
            (stdout.as_str(), stderr.as_str()),
            (output, ""),
            "{program}"
        );
        assert_eq!(status, Some(0), "{program}");
    }
}

// Using a scalar as an array is a runtime error, where indexing it, `in`,
// delete and split panicked (gawk's tests prmarscl and scalar).
#[test]
fn awk_scalar_used_as_an_array_is_an_error() {
    let programs = [
        "function f(a) { print a[1] } BEGIN { j = 4; f(j) }",
        "BEGIN { x = 1; x[1] = 2 }",
        "BEGIN { x = 1; print x[1] }",
        "BEGIN { x = 1; print (1 in x) }",
        "BEGIN { x = 1; delete x[1] }",
        "BEGIN { x = 1; delete x }",
        "BEGIN { x = 1; split(\"a b\", x) }",
        "BEGIN { sub(/x/, \"\", a); a[1] }",
    ];
    for program in programs {
        let (stdout, stderr, status) = awk_with_deadline(program);
        assert_eq!(stdout, "", "{program}");
        // gawk's fatal error.
        assert!(
            stderr.starts_with("awk: cmd. line:1: fatal: "),
            "{program}: {stderr}"
        );
        assert_eq!(status, Some(2), "{program}");
    }
}

// A number used as a regular expression is its string value, as POSIX says
// for any expression on the right of `~`: `"a" ~ ("a" ~ "a")` failed with
// "expected extended regular expression".
#[test]
fn awk_number_used_as_a_regular_expression() {
    let program = "BEGIN { print (\"a\" ~ (\"a\" ~ \"a\")), (\"1\" ~ 1), (\"x10\" ~ 5*2), match(\"a.5\", 0.5), s = \"123\", sub(2, \"z\", s), s }";
    let (stdout, stderr, status) = awk_with_deadline(program);
    assert_eq!(
        (stdout.as_str(), stderr.as_str()),
        ("0 1 1 0 123 1 1z3\n", "")
    );
    assert_eq!(status, Some(0));
}

// A write error on output that does not end in a <newline> is reported: that
// output sat in stdout's line buffer until exit, where the error was lost.
#[test]
fn test_awk_reports_write_error_on_final_partial_line() {
    plib::testing::assert_write_error_on_full_device("awk", &["{ printf $0 }"], b"x", 1);
    plib::testing::assert_write_error_on_full_device("awk", &["BEGIN { printf \"x\" }"], b"", 1);
    plib::testing::assert_write_error_on_full_device(
        "awk",
        &["BEGIN { printf \"x\"; exit 3 }"],
        b"",
        3,
    );
}

// XBD 12.1: `-F=` is the field separator "=", not `-F` with an empty one
// (an empty FS split every character into its own field).
#[test]
fn awk_attached_field_separator_may_begin_with_equals() {
    for (args, input, out) in [
        (&["-F=", "{print $2}"][..], "a=b\n", "b\n"),
        (&["-F==", "{print $2}"][..], "a==b\n", "b\n"),
    ] {
        run_test(TestPlan {
            cmd: String::from("awk"),
            args: args.iter().map(|s| s.to_string()).collect(),
            stdin_data: String::from(input),
            expected_out: String::from(out),
            expected_err: String::new(),
            expected_exit_code: 0,
        });
    }
    plib::testing::assert_equals_option_argument("awk", "-F", &["{print $2}"], b"a=b\n");
}

#[cfg(target_os = "linux")]
#[test]
fn awk_write_error_on_full_device() {
    // A failed write to standard output is "awk: write error: REASON",
    // with the system's text for the error and exit status 1.
    plib::testing::assert_write_error_message_on_full_device("awk", &["{ print }"], b"x\n", 1);
}

#[cfg(target_os = "linux")]
#[test]
fn awk_begin_print_write_error_on_full_device() {
    // A failed write to standard output is "awk: write error: REASON",
    // with the system's text for the error and exit status 1.
    plib::testing::assert_write_error_message_on_full_device(
        "awk",
        &["BEGIN { print \"x\" }"],
        b"",
        1,
    );
}

#[test]
fn test_awk_open_errors_name_the_system_error() {
    // gawk's words for the error, without Rust's `(os error N)`.
    let missing = "/nonexistent/awk-file";
    for args in [
        vec!["1".to_string(), missing.to_string()],
        vec![format!("BEGIN {{ print \"x\" > \"{missing}\" }}")],
    ] {
        let out = plib::testing::run_test_base("awk", &args, b"");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_ne!(out.status.code(), Some(0), "{args:?}");
        assert!(
            stderr.contains("No such file or directory"),
            "{args:?}: {stderr:?}"
        );
        assert!(!stderr.contains("(os error"), "{args:?}: {stderr:?}");
    }
}

/// What posixutils-rs' tests take from its `plib`, for the tests above: the runner with an
/// environment, the locale, the option-argument checks. Windows has no `/dev/full`, so
/// those checks pass by skipping, as `plib` skips them on a host without one.
mod plib {
    pub mod testing {
        use std::io::Write;
        use std::path::PathBuf;
        use std::process::{Command, Output, Stdio};

        pub fn get_binary_path(_cmd: &str) -> PathBuf {
            PathBuf::from(env!("CARGO_BIN_EXE_awk"))
        }

        pub fn run_test_base_with_env(
            cmd: &str,
            args: &[String],
            stdin_data: &[u8],
            env_vars: &[(&str, &str)],
        ) -> Output {
            let mut command = Command::new(get_binary_path(cmd));
            command
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let test_controls_locale = env_vars
                .iter()
                .any(|(key, _)| key.starts_with("LC_") || *key == "LANG" || *key == "LANGUAGE");
            if !test_controls_locale {
                command.env("LC_ALL", "C");
            }
            for (key, value) in env_vars {
                command.env(key, value);
            }
            let mut child = command.spawn().expect("failed to spawn awk");
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(stdin_data);
            }
            child.wait_with_output().expect("failed to wait for awk")
        }

        pub fn run_test_base(cmd: &str, args: &[String], stdin_data: &[u8]) -> Output {
            run_test_base_with_env(cmd, args, stdin_data, &[])
        }

        #[expect(
            clippy::unnecessary_wraps,
            reason = "plib's signature, which the tests call"
        )]
        pub fn utf8_locale() -> Option<String> {
            Some("C.UTF-8".to_string())
        }

        pub fn assert_hyphen_option_argument(cmd: &str, args: &[&str]) -> Output {
            let args: Vec<String> = args.iter().map(ToString::to_string).collect();
            let output = run_test_base(cmd, &args, b"");
            let stderr = String::from_utf8_lossy(&output.stderr);
            for refusal in ["unexpected argument", "a value is required", "tip: to pass"] {
                assert!(
                    !stderr.contains(refusal),
                    "{cmd} {args:?}: an option-argument beginning with '-' was refused: {stderr}"
                );
            }
            output
        }

        pub fn assert_equals_option_argument(cmd: &str, opt: &str, rest: &[&str], stdin: &[u8]) {
            for value in ["=", "=x"] {
                let rest = rest.iter().map(ToString::to_string);
                let attached: Vec<String> = std::iter::once(format!("{opt}{value}"))
                    .chain(rest.clone())
                    .collect();
                let separate: Vec<String> = [opt.to_string(), value.to_string()]
                    .into_iter()
                    .chain(rest)
                    .collect();
                let a = run_test_base(cmd, &attached, stdin);
                let s = run_test_base(cmd, &separate, stdin);
                assert_eq!(
                    (a.status.code(), a.stdout, a.stderr),
                    (s.status.code(), s.stdout, s.stderr),
                    "{cmd} {attached:?} differs from {cmd} {separate:?}"
                );
            }
        }

        pub const fn assert_write_error_on_full_device(
            _cmd: &str,
            _args: &[&str],
            _stdin: &[u8],
            _status: i32,
        ) {
        }
    }

    pub mod tmp {
        pub type TempDir = tempfile::TempDir;
    }
}
