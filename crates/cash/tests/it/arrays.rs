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
