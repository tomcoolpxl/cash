//! The tools cash took from uutils and made its own (`rm`, `sort`, `tee`, `uniq`,
//! `shuf`, in `crates/cash-uutils`) say their messages in English. A release printed
//! their message ids instead (`sort: sort-cannot-read`, `Usage: tee-usage`): uucore
//! finds a tool's messages in the cargo registry, where a tool that is not a crates.io
//! crate is not, on a clean machine. Each now carries its own.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::run;

#[test]
fn the_tools_taken_from_uutils_speak_english_not_message_ids() {
    for (script, want) in [
        ("sort /no/such/file", "sort: cannot read: /no/such/file:"),
        ("sort --help", "Usage: sort [OPTION]... [FILE]..."),
        ("tee --help", "Usage: tee [OPTION]... [FILE]..."),
        ("uniq --help", "Usage: uniq [OPTION]... [INPUT [OUTPUT]]"),
        (
            "shuf --help",
            "Shuffle the input by outputting a random permutation",
        ),
        ("shuf -r </dev/null", "shuf: no lines to repeat"),
        ("rm --help", "Usage: rm [OPTION]... FILE..."),
        (
            "rm /no/such",
            "rm: cannot remove '/no/such': No such file or directory",
        ),
    ] {
        let out = run(script);
        let text = format!("{}{}", out.stdout, out.stderr);
        assert!(text.contains(want), "{script}: {text}");
        // An id is the tool's name, a dash and more words: `sort-cannot-read`.
        for tool in ["rm", "sort", "tee", "uniq", "shuf"] {
            assert!(
                !text.contains(&format!("{tool}-usage"))
                    && !text.contains(&format!("{tool}-error")),
                "{script} printed a message id: {text}"
            );
        }
    }
}
