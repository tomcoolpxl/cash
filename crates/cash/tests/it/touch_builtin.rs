//! `touch`, cash's own since it was taken from uutils 0.12.0 (`cash-uutils`).

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, run_in};

#[test]
fn an_operand_that_cannot_be_touched_is_said_and_the_rest_are_touched() {
    // As GNU touch: the first operand it could not touch ended the run, so `y` was never
    // made. A `"` is no character of a Windows name.
    let scratch = Scratch::new("touch-operands");
    let out = run_in(scratch.path(), r#"touch x 'a"b' y; echo "s $?"; ls"#);
    assert_eq!(out.stdout, "s 1\nx\ny", "{}", out.stderr);
    assert!(
        out.stderr.starts_with("touch: ") && out.stderr.contains("'a\"b'"),
        "{}",
        out.stderr
    );
}

#[test]
fn touch_sets_and_copies_times_and_names_the_uutils_release() {
    let scratch = Scratch::new("touch-times");
    let out = run_in(
        scratch.path(),
        "touch -d '2020-01-02 03:04' x; touch -r x y; touch -c nope; date -r y +%F; \
         [ -e nope ] || echo none; touch --version | head -1",
    );
    assert_eq!(
        out.stdout,
        "2020-01-02\nnone\ntouch (uutils coreutils) 0.12.0",
        "{}",
        out.stderr
    );
}
