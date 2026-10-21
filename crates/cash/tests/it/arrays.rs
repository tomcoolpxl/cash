//! Indexed arrays as Bash 5.3 has them (TODO.md 12.1).

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
fn a_negative_index_counts_back_from_the_highest_index() {
    // It counted from the number of elements (LANG-03).
    check(&[
        (
            r#"a=(1 2 3); unset "a[1]"; echo "${a[-1]} [${a[-2]}]"; a[-1]=X; declare -p a"#,
            "3 []\ndeclare -a a=([0]=\"1\" [2]=\"X\")",
        ),
        (r#"a=([0]=x [5]=y [9]=z); echo "${a[-1]} ${a[-5]}""#, "z y"),
    ]);
}

#[test]
fn a_subscript_in_a_list_is_arithmetic_unless_the_array_is_associative() {
    // `2+1` and `i` were read as numbers, 0 (LANG-02).
    check(&[
        (
            "i=4; c=([2+1]=y z [i]=w); declare -p c",
            "declare -a c=([3]=\"y\" [4]=\"w\")",
        ),
        (
            "a=([1+1]=x); a+=([10/2]=y); declare -p a",
            "declare -a a=([2]=\"x\" [5]=\"y\")",
        ),
        (
            "declare -a d=([3*2]=z); declare -p d",
            "declare -a d=([6]=\"z\")",
        ),
        (
            "f() { local -a l=([1+2]=q); declare -p l; }; f",
            "declare -a l=([3]=\"q\")",
        ),
        (
            "declare -A h=([a+b]=1 [k]=v); declare -p h",
            "declare -A h=([a+b]=\"1\" [k]=\"v\" )",
        ),
        (
            "declare -A g; g=([x+1]=2); declare -p g",
            "declare -A g=([x+1]=\"2\" )",
        ),
    ]);
}

#[test]
fn a_reference_to_an_element_assigns_to_the_element() {
    // It created a variable named `a[1]` (LANG-12).
    check(&[
        (
            r#"a=(x y z); declare -n r="a[1]"; r=Z; declare -p a; echo "$r""#,
            "declare -a a=([0]=\"x\" [1]=\"Z\" [2]=\"z\")\nZ",
        ),
        (
            r#"f() { local -n out=$1; out=done; }; a=(1 2); f "a[1]"; declare -p a"#,
            "declare -a a=([0]=\"1\" [1]=\"done\")",
        ),
        (
            r#"declare -A h=([k]=v); declare -n r="h[k]"; r=W; declare -p h"#,
            "declare -A h=([k]=\"W\" )",
        ),
    ]);
}

/// Runs each script and checks its standard output, and its standard error with each
/// line's prefix up to `line ` taken off, against Bash's.
fn check_with_errors(cases: &[(&str, &str, &str)]) {
    for &(script, expected_out, expected_err) in cases {
        let out = run(script);
        let errors = out
            .stderr
            .lines()
            .map(|line| line.split_once(": line ").map_or(line, |(_, rest)| rest))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            (out.stdout.as_str(), errors.as_str()),
            (expected_out, expected_err),
            "{script}"
        );
    }
}

#[test]
fn a_bad_subscript_is_said_and_abandons_the_line() {
    // Each was element 0, or key "", in silence; an element before a command was set for
    // it. A bad subscript in a list keeps the elements before it; in a function it
    // abandons the command the function was called in.
    check_with_errors(&[
        (
            "b[]=x\necho \"after $?\"",
            "after 1",
            "1: b[]: bad array subscript",
        ),
        (
            "z[-1]=4\necho \"after $?\"",
            "after 1",
            "1: z[-1]: bad array subscript",
        ),
        (
            "y=(a b)\ny=(p [-65]=v w)\ndeclare -p y",
            "declare -a y=([0]=\"p\")",
            "2: [-65]=v: bad array subscript",
        ),
        (
            "x=(a b); x+=([-1]=v w); declare -p x",
            "declare -a x=([0]=\"a\" [1]=\"v\" [2]=\"w\")",
            "",
        ),
        (
            "declare -a x=([-1]=v)\necho \"after $?\"",
            "after 1",
            "1: [-1]=v: bad array subscript",
        ),
        (
            "f() { b[]=x; echo in; }; f; echo same\necho \"next $?\"",
            "next 1",
            "1: b[]: bad array subscript",
        ),
        (
            "declare -A A; A[\"\"]=x\nA=([\"\"]=y [k]=z)\nA=([k]=v one)\ndeclare -p A",
            "declare -A A=([k]=\"v\" )",
            "1: A[\"\"]: bad array subscript\n2: [\"\"]=y: bad array subscript\n\
             3: A: one: must use subscript when assigning associative array",
        ),
        (
            "arr[0]=1 echo hi; echo \"s $?\"",
            "hi\ns 0",
            "1: `arr[0]': not a valid identifier",
        ),
    ]);
}

#[test]
fn an_operator_that_reads_and_writes_an_element_evaluates_its_subscript_once() {
    // `i` was stepped twice and the next element written.
    check(&[(
        r#"a=(1 2 3); i=0; (( a[i++]++ )); echo "$i ${a[*]}"; (( a[i++] += 5 )); echo "$i ${a[*]}""#,
        "1 2 2 3\n2 2 7 3",
    )]);
}

#[test]
fn declare_takes_attributes_away_and_lists_with_them_as_bash_does() {
    // `+a` made an array; `-a` alone listed `name=(...)`; `+n` changed the target.
    check(&[
        ("s=1; declare +a s; declare -p s", "declare -- s=\"1\""),
        (
            r#"a=(1 2); declare -a | grep "^declare -a a=""#,
            "declare -a a=([0]=\"1\" [1]=\"2\")",
        ),
        (
            "x=1; declare -n r=x; declare +n r; declare -p r",
            "declare -- r=\"x\"",
        ),
    ]);
}

#[test]
fn a_reference_is_pointed_by_for_and_checked_and_writes_an_element_for_read() {
    // `for` assigned the second word to the first; `declare -n r=/` was taken; `read`
    // made a variable named `X[0]`; `${r=X}` was `X`'s value.
    check(&[
        (
            r#"declare -n r; for r in a b; do echo "[$r]"; done; declare -p r"#,
            "[]\n[]\ndeclare -n r=\"b\"",
        ),
        (
            r#"declare -n r; echo "${r=X}"; declare -p r"#,
            "X\ndeclare -n r=\"X\"",
        ),
        (
            "declare -n r=X[0]; read r <<< hi; declare -p X",
            "declare -a X=([0]=\"hi\")",
        ),
    ]);
    check_with_errors(&[(
        "declare -n r=/; echo \"s $?\"",
        "s 1",
        "1: declare: `/': invalid variable name for name reference",
    )]);
}
