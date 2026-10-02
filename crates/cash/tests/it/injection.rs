//! Text that must never become a command.
//!
//! Each case here once ran something its author did not write: a URL handed to `start`,
//! whose `&` reached `cmd.exe` unquoted (`REVIEW_REPORT.md` BI-06); a key that `unset`,
//! `read` or `[[ -v ]]` expands a second time, spelt in a form the hardening of spec §4
//! row 37 did not recognise (LANG-04). Each test watches for a side effect — a file that
//! must not appear — rather than for output, which a command substitution would swallow.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result. Shell snippets are spelled with hashes \
              throughout, including where they are not strictly needed."
)]

use crate::common::{Scratch, run_in as cash_in};

#[test]
fn start_hands_an_ampersand_to_no_command_processor() {
    let scratch = Scratch::new("injection-start");
    let dir = scratch.path();
    // No such file, so nothing opens; with `cmd /c start` in between, `&` ran the rest.
    let out = cash_in(dir, "start 'no-such-file.txt&echo x>pwned'; echo \"rc=$?\"");
    assert!(
        !dir.join("pwned").exists(),
        "the text after & ran: {}",
        out.stderr
    );
    assert_eq!(out.stdout, "rc=1", "{}", out.stderr);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stderr
            .starts_with("start: no-such-file.txt&echo x>pwned:"),
        "{}",
        out.stderr
    );
}

#[test]
fn a_subscript_expanded_a_second_time_runs_no_command() {
    // Every spelling of a command substitution, held in a variable, then handed to each
    // builtin that expands a subscript again. Plain `$(...)` was refused already; the
    // other three ran.
    let keys = [
        "$(touch pwned)",
        "`touch pwned`",
        "$((touch pwned) )",
        "${ touch pwned; }",
        "${| touch pwned; }",
    ];
    let uses = [
        r#"a=(1 2); unset "a[$k]""#,
        r#"declare -A h=([x]=1); unset "h[$k]""#,
        r#"a=(1 2); [[ -v "a[$k]" ]]"#,
        r#"a=(1 2); read "a[$k]" <<< v"#,
        r#"a=(1 2); printf -v "a[$k]" x"#,
        r#"a=(1 2); declare "a[$k]=v""#,
    ];
    let scratch = Scratch::new("injection-subscript");
    let dir = scratch.path();
    for key in keys {
        for use_ in uses {
            let script = format!("k='{key}'; {use_}; echo after");
            let out = cash_in(dir, &script);
            assert!(
                !dir.join("pwned").exists(),
                "{script} ran the command: {}",
                out.stderr
            );
            assert_eq!(out.stdout, "after", "{script}: {}", out.stderr);
        }
    }
}
