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
    clippy::manual_assert,
    clippy::manual_string_new,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::panic,
    clippy::semicolon_if_nothing_returned,
    clippy::tests_outside_test_module,
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

// An escape awk does not define stands for the character, as in gawk (whose output this
// is, without its warnings): `split(s, parts, "\.")` was a parse error (TXT-05).
#[test]
fn test_awk_unknown_string_escapes_stand_for_the_character() {
    test_awk!(unknown_string_escapes_stand_for_the_character);
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
    test_awk!(infinite_and_nan_numbers_match_gawk);
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

// Regression: gsub with zero-width match must not panic on multi-byte UTF-8
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
        (r#"/\"hi\"/ { print "quote" }"#, "say \"hi\"\n", "quote\n"),
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
            r#"BEGIN { sub(/a/, "b", "x") }"#,
            "sub third parameter is not a changeable object",
        ),
        (
            "BEGIN { print 1 in 2 }",
            "the right side of 'in' should be an array",
        ),
        (
            r#"BEGIN { print 1 in "x" }"#,
            "the right side of 'in' should be an array",
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

// The precision of `%s` counts bytes; one that ends inside a character stops before it
// rather than panicking (TODO.md 14.6).
#[test]
fn test_awk_string_precision_inside_a_character() {
    run_test(plan(
        r#"BEGIN { printf "[%.1s][%.2s][%.3s]\n", "é", "éa", "éa" }"#,
        "",
        "[][é][éa]\n",
        0,
    ));
}
