//! Here-documents as Bash 5.3 reads and writes them (TODO.md 12.2).

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::run;

/// Runs each script and checks its standard output and status against Bash's.
fn check(cases: &[(&str, &str)]) {
    for &(script, expected) in cases {
        let out = run(script);
        assert_eq!(
            (out.stdout.as_str(), out.code),
            (expected, 0),
            "{script}: {}",
            out.stderr
        );
    }
}

#[test]
fn a_function_with_a_here_document_is_written_so_it_reads_back() {
    // `declare -f` indented the body and the end of the here-document, so the end was
    // not found when the text was read back (PI-01).
    check(&[(
        "g() { cat <<EOF\nhello $1\nEOF\n}\ndeclare -f g; eval \"$(declare -f g)\"; g world",
        "g () \n{ \n    cat <<EOF\nhello $1\nEOF\n\n}\nhello world",
    )]);
}

#[test]
fn a_shift_after_a_nested_arithmetic_expansion_is_a_shift() {
    // The inner `$((1))` ended the outer one's arithmetic, and `<< 2` was read as a
    // here-document (PI-05).
    check(&[
        ("echo $(( $((1)) << 2 ))", "4"),
        ("echo $(( $((1)) << $((2)) ))", "4"),
        ("echo $[ $[1] << 2 ]", "4"),
    ]);
}

#[test]
fn a_backslash_newline_joins_lines_in_an_unquoted_here_document() {
    // Both were kept (PI-04); a quoted here-document keeps them, as in Bash.
    check(&[
        ("cat <<EOF\none \\\ntwo\nEOF", "one two"),
        ("cat <<'EOF'\nquoted \\\nstays\nEOF", "quoted \\\nstays"),
        ("x=$(cat <<EOF\nin \\\nsub\nEOF\n)\necho \"$x\"", "in sub"),
    ]);
}
