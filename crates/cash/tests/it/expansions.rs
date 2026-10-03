//! Expansions as Bash 5.3 performs them (TODO.md 12.3).

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
fn lengths_and_offsets_count_characters() {
    // They counted bytes: `${#x}` of `café` was 5 and `${x: -1}` empty (LANG-09).
    check(&[
        ("x=café; echo ${#x} \"${x: -1}\" \"${x: -2:1}\"", "4 é f"),
        (
            "a=(café ü); echo ${#a[0]} ${#a[1]} \"${a[0]: -1}\"",
            "4 1 é",
        ),
        ("y=日本語; echo ${#y} \"${y: -1}\"", "3 語"),
        ("set -- héllo; echo ${#1}", "5"),
    ]);
}

#[test]
fn an_arithmetic_expression_that_expands_to_nothing_is_zero() {
    // It was a parse error that abandoned the line (LANG-10).
    check(&[
        (
            "e=; echo $(( $e )) $(( $unset )) $[ $e ] $((  ))",
            "0 0 0 0",
        ),
        ("s='   '; echo $(( $s ))", "0"),
        ("e=; (( $e )); echo $?", "1"),
        ("e=; [[ $e -eq 0 ]] && echo yes", "yes"),
    ]);
}

#[test]
fn an_integer_variable_evaluates_its_value_however_it_is_assigned() {
    // `read`, `printf -v`, array literals and `for` read names as 0 (LANG-11).
    check(&[
        ("y=3; declare -i x; read x <<< 'y*2'; echo $x", "6"),
        ("y=3; declare -i x; printf -v x '%s' 'y+10'; echo $x", "13"),
        ("y=3; declare -ia a=(y+1 2*y); echo ${a[*]}", "4 6"),
        (
            "y=3; declare -ia b; b=(y+2 y*y); b+=(y+5); echo ${b[*]}",
            "5 9 8",
        ),
        ("y=3; declare -i x; for x in y+100; do echo $x; done", "103"),
        (
            "y=3; declare -i r; read -a r <<< 'y 2*y'; echo ${r[*]}",
            "3 6",
        ),
        ("y=3; declare -iA h; h+=([j]=y*3); echo ${h[j]}", "9"),
        ("y=3; declare -i t; declare -n ref=t; ref=y+4; echo $t", "7"),
        // `pid` is not `PID`, an integer found by the case-insensitive fallback (D31).
        (
            "read -r pid <<< PID; pid2=PID; declare pid3=PID; echo $pid $pid2 $pid3",
            "PID PID PID",
        ),
    ]);
}

#[test]
fn an_arithmetic_error_in_an_integer_assignment_is_reported() {
    // It was silently 0 (LANG-11); the rest of the line is abandoned, as in Bash.
    for script in [
        "declare -i w; w=1/0; echo \"after $w\"",
        "declare -i w; read w <<< 1/0; echo \"after $w\"",
    ] {
        let out = run(script);
        assert_eq!(out.stdout, "", "{script}");
        assert!(
            out.stderr.contains("division by zero"),
            "{script}: {}",
            out.stderr
        );
    }
}

#[test]
fn an_assigned_default_expands_to_the_value_stored() {
    // It expanded to the text given, before the variable's attributes (LANG-18).
    check(&[
        ("declare -i x; echo ${x:=1+2} $x", "3 3"),
        ("declare -u u; echo ${u:=abc} $u", "ABC ABC"),
        ("declare -ia a; echo ${a[2]:=2*4}", "8"),
        ("echo ${v:=plain} $v", "plain plain"),
    ]);
}

#[test]
fn a_backslash_in_backquotes_is_removed_before_dollar_backquote_and_backslash() {
    // `\$x` and `\"` were passed on, and `\\` stayed two backslashes (LANG-13).
    check(&[
        ("x=val; echo `echo \\$x` \"`echo \\$x`\"", "val val"),
        ("echo \"`echo \\\"q\\\"`\" `echo \\\"q\\\"`", "q \"q\""),
        ("echo `echo \\\\\\\\` \"`echo \\\\\\\\`\"", "\\ \\"),
        ("echo `echo a\\`echo b\\\\\\`echo c\\\\\\`\\`d`", "abcd"),
        ("x=val; cat <<EOF\n`echo \\\"q\\\" \\$x`\nEOF", "\"q\" val"),
    ]);
}

#[test]
fn a_quoted_ampersand_in_a_replacement_is_literal() {
    // Only the replacement text's own quotes were seen: `"$r"` with `r='&&'` and `"&"`
    // put the match in, and a quoted backslash escaped the `&` after it (LANG-14).
    check(&[
        (
            "x=abc; r='&&'; echo \"${x/b/\"$r\"}\" ${x/b/\"$r\"}",
            "a&&c a&&c",
        ),
        ("x=abc; r='&&'; echo ${x/b/$r} ${x/b/&&}", "abbc abbc"),
        (
            "x=abc; echo \"${x/b/\"&\"}\" ${x/b/'&'} ${x/b/\\&}",
            "a&c a&c a&c",
        ),
        ("x=abc; echo ${x/b/'\\'&} ${x/b/\\\\&}", "a\\bc a\\bc"),
        ("x=abc; q='\\&'; echo ${x/b/$q} ${x//?/&-}", "a&c a-b-c-"),
    ]);
}

#[test]
fn an_arithmetic_command_is_traced_as_its_expanded_text() {
    // It was the parsed expression printed again: `(( x + 5 ))` for `((x+$x))`.
    let out = run("x=5; set -x; ((x+$x)); for ((i=x; i<x; i++)); do :; done");
    assert_eq!(out.stderr, "+ (( x+5 ))\n+ (( i=x ))\n+ (( i<x ))");
}

#[test]
fn machtype_names_the_machine() {
    // It was "unknown"; `$HOSTTYPE-pc-$OSTYPE`, as Bash builds it.
    check(&[(
        "m=$HOSTTYPE-pc-windows; [[ $MACHTYPE == \"$m\" && ${BASH_VERSINFO[5]} == \"$m\" ]] && echo ok",
        "ok",
    )]);
}

#[test]
fn an_assignment_seeds_random_as_in_bash() {
    // It was dropped (LANG-23); the numbers are Git Bash 5.3's.
    check(&[
        (
            "RANDOM=42; echo $RANDOM $RANDOM $RANDOM",
            "17772 26794 1435",
        ),
        (
            "RANDOM=40+2; echo $RANDOM; RANDOM=-1; echo $RANDOM",
            "17772\n16807",
        ),
        ("unset RANDOM; RANDOM=5; echo $RANDOM", "5"),
    ]);
}

#[test]
fn transformations_u_and_k_do_what_bash_does() {
    // `@u` capitalised every word (LANG-19); `${a[@]@K}` gave the values only (LANG-20).
    check(&[
        ("x='hello big world'; echo \"${x@u}\"", "Hello big world"),
        ("w=(abc déf); echo \"${w[@]@u}\"", "Abc Déf"),
        (
            "a=(p 'q r'); printf '<%s>' \"${a[@]@K}\"",
            "<0 \"p\" 1 \"q r\">",
        ),
        (
            "declare -A m=([k]='$v'); printf '<%s>' \"${m[@]@K}\"",
            "<k \"\\$v\" >",
        ),
        (
            "a=(p 'q r'); printf '<%s>' ${a[@]@K}",
            "<0><\"p\"><1><\"q><r\">",
        ),
        (
            "x=hi; a=(p); printf '<%s>' \"${x@K}\" \"${a[0]@K}\"",
            "<'hi'><'p'>",
        ),
    ]);
}
