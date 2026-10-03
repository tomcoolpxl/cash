//! Extended patterns, `!(…)` above all, as Bash 5.3 matches them (TODO.md 12.4).

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction, and a failed \
              setup should abort it loudly"
)]

use crate::common::{Scratch, run, run_in};

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
fn a_negation_matches_what_none_of_its_patterns_match() {
    // `!(…)` was a lookahead and an atomic group, which got these wrong (LANG-01).
    check(&[
        ("[[ ab == !(a|ab) ]] && echo yes || echo no", "no"),
        ("[[ abc == !(a|ab) ]] && echo yes || echo no", "yes"),
        (
            "case x.tar.gz in !(*.tar|*.tar.gz)) echo match;; *) echo no;; esac",
            "no",
        ),
        ("[[ aaab == +(a)!(b) ]] && echo yes", "yes"),
        ("[[ ab == @(!(a)|b)b ]] && echo yes || echo no", "no"),
        ("[[ '' == !(a) ]] && echo yes", "yes"),
        (
            "shopt -s nocasematch; [[ AB == !(a|ab) ]] && echo yes || echo no",
            "no",
        ),
    ]);
}

#[test]
fn pathname_expansion_leaves_out_what_a_negation_names() {
    let scratch = Scratch::new("extglob");
    for name in ["a.tar", "a.tar.gz", "b.txt", "c", "ab"] {
        std::fs::write(scratch.path().join(name), b"").expect("create file");
    }
    let out = run_in(
        scratch.path(),
        "echo !(*.tar|*.tar.gz); echo *.!(gz); echo @(!(a*)|ab)",
    );
    assert_eq!(
        out.stdout, "ab b.txt c\na.tar a.tar.gz b.txt\nab b.txt c",
        "{}",
        out.stderr
    );
}

#[test]
fn a_replacement_takes_the_longest_match_at_the_leftmost_place() {
    // The regex took the first alternative: `${x/@(a|ab)/X}` of `abc` was `Xbc`.
    check(&[
        (
            "x=abc; echo ${x/@(a|ab)/X} ${x//@(a|ab)/X} ${x/#@(a|ab)/X}",
            "Xc Xc Xc",
        ),
        ("z=xabcx; echo ${z/!(x)/_} ${z//!(x)/_}", "_ _"),
        // After an empty match the next character is kept; the end is not a place.
        (
            "x=abc; echo \"[${x//*(z)/_}] [${x//?(a)/_}] [${x//*(b)/_}]\"",
            "[_a_b_c] [__b_c] [_a__c]",
        ),
        // An empty value: only a pattern starting with `*` matches, but `/%` always.
        (
            "e=; echo \"[${e//*(z)/_}] [${e//!(z)/_}] [${e/%!(z)/_}]\"",
            "[_] [] [_]",
        ),
    ]);
}

#[test]
fn the_smallest_prefix_or_suffix_can_be_empty() {
    // `${f%*}` removed the last character: the empty suffix was never tried.
    check(&[(
        "f=foo.c; echo ${f%*} ${f#*} ${f%!(*.c)} ${f%%!(*.c)} ${f#!(foo)}",
        "foo.c foo.c foo.c foo. foo.c",
    )]);
}

#[test]
fn case_modification_tests_each_character() {
    // A match over several characters changed them all.
    check(&[(
        "x=abc; echo ${x^^@(ab)} ${x^^!(b)} ${x^@(a|b)}",
        "abc AbC Abc",
    )]);
}

#[test]
fn a_long_value_is_matched_in_reasonable_time() {
    // Each span tested on its own took minutes for 400 characters; Bash takes two
    // minutes for 2,000.
    check(&[(
        "y=$(printf 'ab%.0s' {1..500}); z=${y//!(*b*)/_}; echo ${#z}; [[ $y == !(x)*(ab) ]] && echo ok",
        "1500\nok",
    )]);
}
