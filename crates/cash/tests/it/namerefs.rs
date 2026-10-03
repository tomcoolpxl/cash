//! Name references — `declare -n`, and the `local -n` idiom scripts return values with.
//!
//! cash tracked the `-n` attribute and then ignored it, which made every use of a
//! reference a **silently wrong answer** rather than an error: `$ref` handed back the
//! name it was pointed at instead of that name's value, `ref=x` wrote to the reference
//! itself, and
//!
//! ```bash
//! fill() { local -n out=$1; out=filled; }
//! box=empty; fill box
//! ```
//!
//! — the way a bash function writes into a variable its caller owns — left `box` empty
//! and reported success. Every expectation below was read off real bash first.

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

use crate::common::run as cash;

// ---------------------------------------------------------------------------
// Reading and writing through a reference
// ---------------------------------------------------------------------------

#[test]
fn a_reference_reads_its_target() {
    let out = cash(r#"target=one; declare -n ref=target; echo "$ref""#);
    assert_eq!(out.stdout, "one", "stderr: {}", out.stderr);
}

#[test]
fn a_write_through_a_reference_lands_on_the_target() {
    let out = cash(r#"target=one; declare -n ref=target; ref=two; echo "$target""#);
    assert_eq!(out.stdout, "two", "stderr: {}", out.stderr);
}

#[test]
fn a_function_writes_back_through_local_n() {
    // The reason namerefs exist in scripts at all.
    let out = cash(r#"fill() { local -n out=$1; out=filled; }; box=empty; fill box; echo "$box""#);
    assert_eq!(out.stdout, "filled", "stderr: {}", out.stderr);
}

#[test]
fn a_reference_to_an_unset_name_creates_it_on_assignment() {
    // Which is what makes the `local -n` idiom work for a caller that never declared the
    // variable it is asking to be filled in.
    let out = cash(r#"declare -n miss=nothere; miss=created; echo "[$nothere]""#);
    assert_eq!(out.stdout, "[created]", "stderr: {}", out.stderr);
}

#[test]
fn reading_through_a_reference_to_an_unset_name_is_empty() {
    let out = cash(r#"declare -n miss=nothere; echo "[${miss-unset}]""#);
    assert_eq!(out.stdout, "[unset]", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// Chains
// ---------------------------------------------------------------------------

#[test]
fn a_chain_of_references_resolves_to_the_end() {
    let out = cash(r#"base=bottom; declare -n mid=base; declare -n top=mid; echo "$top""#);
    assert_eq!(out.stdout, "bottom", "stderr: {}", out.stderr);
}

#[test]
fn a_write_through_a_chain_reaches_the_end() {
    let out =
        cash(r#"base=bottom; declare -n mid=base; declare -n top=mid; top=changed; echo "$base""#);
    assert_eq!(out.stdout, "changed", "stderr: {}", out.stderr);
}

#[test]
fn a_self_reference_is_refused() {
    let out = cash(r#"declare -n s=s; echo "code: $?""#);
    assert_eq!(out.stdout, "code: 1", "stderr: {}", out.stderr);
    assert!(
        out.stderr
            .contains("s: nameref variable self references not allowed"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

#[test]
fn a_circular_pair_reads_as_unset_with_a_warning() {
    // Bash warns and reads nothing; the warning was missing (LANG-21).
    let out = cash(r#"declare -n p=q; declare -n q=p; echo "[${p-unset}]""#);
    assert_eq!(out.stdout, "[unset]", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("warning: p: circular name reference"),
        "{}",
        out.stderr
    );
}

#[test]
fn an_assignment_through_a_circular_pair_ends_the_script() {
    // As an assignment to a read-only variable does, in Bash; it was dropped in silence
    // and the script went on (LANG-21).
    let out = cash(r#"declare -n p=q; declare -n q=p; p=z; echo after"#);
    assert_eq!((out.stdout.as_str(), out.code), ("", 1), "{}", out.stderr);
    assert!(
        out.stderr.contains("warning: p: circular name reference"),
        "{}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// Arrays
// ---------------------------------------------------------------------------

#[test]
fn an_array_is_reachable_through_a_reference() {
    let out = cash(r#"arr=(a b c); declare -n aref=arr; echo "${aref[1]} ${#aref[@]} ${aref[*]}""#);
    assert_eq!(out.stdout, "b 3 a b c", "stderr: {}", out.stderr);
}

#[test]
fn an_element_written_through_a_reference_lands_in_the_array() {
    let out = cash(r#"arr=(a b c); declare -n aref=arr; aref[1]=B; echo "${arr[*]}""#);
    assert_eq!(out.stdout, "a B c", "stderr: {}", out.stderr);
}

#[test]
fn a_loop_iterates_the_referenced_array() {
    let out = cash(
        r#"arr=(a b c); declare -n fref=arr; for x in "${fref[@]}"; do printf '%s.' "$x"; done"#,
    );
    assert_eq!(out.stdout, "a.b.c.", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// The reference itself
// ---------------------------------------------------------------------------

#[test]
fn declare_p_shows_the_reference_and_its_target() {
    let out = cash(r#"target=two; declare -n ref=target; declare -p ref; declare -p target"#);
    assert_eq!(
        out.stdout, "declare -n ref=\"target\"\ndeclare -- target=\"two\"",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn the_indirection_operator_gives_the_name() {
    // bash's one exception to what `!` means: on a reference it yields the name, not a
    // second lookup.
    let out = cash(r#"target=one; declare -n ref=target; echo "${!ref}""#);
    assert_eq!(out.stdout, "target", "stderr: {}", out.stderr);
}

#[test]
fn dash_r_tests_the_reference_rather_than_the_target() {
    let out = cash(
        r#"base=1; declare -n r=base; [[ -R r ]] && echo "ref"; [[ -R base ]] || echo "not a ref""#,
    );
    assert_eq!(out.stdout, "ref\nnot a ref", "stderr: {}", out.stderr);
}

#[test]
fn dash_v_follows_the_reference() {
    let out = cash(r#"base=1; declare -n r=base; [[ -v r ]] && echo "set""#);
    assert_eq!(out.stdout, "set", "stderr: {}", out.stderr);
}

#[test]
fn retargeting_leaves_the_old_target_alone() {
    // Re-pointing a reference must not write through the reference it is replacing.
    let out = cash(
        r#"one=1; two=2; declare -n swap=one; declare -n swap=two; echo "$one $two $swap"; declare -p one"#,
    );
    assert_eq!(
        out.stdout, "1 2 2\ndeclare -- one=\"1\"",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn declare_without_n_assigns_through_the_reference() {
    // Only `-n` names the reference; a plain `declare` writes through it like anything
    // else.
    let out = cash(r#"t=1; declare -n ref=t; declare ref=5; echo "$t"; declare -p ref"#);
    assert_eq!(
        out.stdout, "5\ndeclare -n ref=\"t\"",
        "stderr: {}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// unset
// ---------------------------------------------------------------------------

#[test]
fn unset_through_a_reference_unsets_the_target() {
    let out = cash(r#"v=1; declare -n r=v; unset r; echo "[${v-unset}]""#);
    assert_eq!(out.stdout, "[unset]", "stderr: {}", out.stderr);
}

#[test]
fn the_reference_still_works_after_its_target_is_unset() {
    let out = cash(r#"v=1; declare -n r=v; unset r; r=again; echo "[${v-unset}]""#);
    assert_eq!(out.stdout, "[again]", "stderr: {}", out.stderr);
}

#[test]
fn unset_n_removes_the_reference_and_leaves_the_target() {
    // This refused outright with "not yet implemented", so a reference could not be
    // taken back once made.
    let out = cash(r#"v=1; declare -n r=v; unset -n r; echo "[${r-unset}] [${v-unset}]""#);
    assert_eq!(out.stdout, "[unset] [1]", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// The operators that read and write in one step
// ---------------------------------------------------------------------------

#[test]
fn arithmetic_through_a_reference_updates_the_target() {
    let out = cash(r#"n=1; declare -n nref=n; ((nref++)); echo "$n""#);
    assert_eq!(out.stdout, "2", "stderr: {}", out.stderr);
}

#[test]
fn append_through_a_reference_updates_the_target() {
    let out = cash(r#"n=2; declare -n nref=n; nref+=5; echo "$n""#);
    assert_eq!(out.stdout, "25", "stderr: {}", out.stderr);
}

#[test]
fn exporting_a_reference_is_accepted() {
    let out = cash(r#"base=1; declare -n eref=base; export eref; echo "code: $?""#);
    assert_eq!(out.stdout, "code: 0", "stderr: {}", out.stderr);
    assert_eq!(out.code, 0);
}
