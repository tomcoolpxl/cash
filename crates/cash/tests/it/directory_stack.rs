//! The directory stack's `+N` and `-N` forms — **D3**.
//!
//! `dirs`, `pushd` and `popd` each carried a `TODO` for the offsets, and all three
//! refused them. `dirs +1` and `popd +1` failed as usage errors, a bare `pushd` failed
//! for want of its "required" argument, and `pushd +1` did something worse than refuse:
//! it took `+1` for a path and reported that no such directory existed.
//!
//! The offsets are the reason `dirs -v` prints indices at all, and `pushd +1` is how a
//! script rotates between two trees it is working in. Every expectation here was read off
//! real bash first.

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

use std::path::{Path, PathBuf};
use std::process::Command;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// Pushes three directories, leaving the listing `[c, b, a, ~]`.
const BUILD: &str = "pushd a > /dev/null; pushd ../b > /dev/null; pushd ../c > /dev/null; ";

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

/// A directory tree to move around in, which is also the shell's `$HOME` — so cash
/// renders every path in it as `~/...` and the assertions can say what they mean instead
/// of carrying the machine's temp path around.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("cash-dirstack-{name}-{}", std::process::id()));
        // Named after the test and the run, and cleared on the way in as well as out, so a run that
        // died half way through does not change what the next one sees.
        let _ = std::fs::remove_dir_all(&root);
        for sub in ["a", "b", "c", "d e"] {
            std::fs::create_dir_all(root.join(sub)).expect("failed to create the sandbox");
        }
        Self { root }
    }

    fn run(&self, script: &str) -> Output {
        run_in(&self.root, script)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn run_in(dir: &Path, script: &str) -> Output {
    let out = Command::new(CASH)
        .current_dir(dir)
        .env("HOME", dir)
        .args(["-c", script])
        .output()
        .expect("failed to run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

// ---------------------------------------------------------------------------
// dirs +N / -N
// ---------------------------------------------------------------------------

#[test]
fn dirs_plus_n_prints_one_entry() {
    let sandbox = Sandbox::new("dirs-plus");
    let out = sandbox.run(&format!("{BUILD} dirs +1"));
    assert_eq!(out.stdout, "~/b", "stderr: {}", out.stderr);
}

#[test]
fn dirs_minus_n_counts_from_the_far_end() {
    let sandbox = Sandbox::new("dirs-minus");
    let out = sandbox.run(&format!(r#"{BUILD} dirs -0; dirs -1"#));
    assert_eq!(out.stdout, "~\n~/a", "stderr: {}", out.stderr);
}

#[test]
fn a_selector_keeps_its_absolute_index_under_v() {
    let sandbox = Sandbox::new("dirs-v");
    let out = sandbox.run(&format!("{BUILD} dirs -v +1"));
    assert_eq!(out.stdout, " 1  ~/b", "stderr: {}", out.stderr);
}

#[test]
fn the_last_selector_wins() {
    // bash reads them all and prints only the last.
    let sandbox = Sandbox::new("dirs-last");
    let out = sandbox.run(&format!("{BUILD} dirs +1 +2"));
    assert_eq!(out.stdout, "~/a", "stderr: {}", out.stderr);
}

#[test]
fn an_index_past_the_stack_is_reported_without_its_sign() {
    // bash drops the sign for `dirs`, though `pushd` and `popd` keep it.
    let sandbox = Sandbox::new("dirs-range");
    let out = sandbox.run(&format!("{BUILD} dirs +9"));
    assert_eq!(out.code, 1, "stdout: {}", out.stdout);
    assert!(
        out.stderr
            .contains("dirs: 9: directory stack index out of range"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

#[test]
fn with_nothing_pushed_the_stack_is_empty_rather_than_out_of_range() {
    let sandbox = Sandbox::new("dirs-empty");
    let out = sandbox.run("dirs +1");
    assert_eq!(out.code, 1, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("dirs: directory stack empty"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

#[test]
fn index_zero_is_the_current_directory_even_with_nothing_pushed() {
    let sandbox = Sandbox::new("dirs-zero");
    let out = sandbox.run("dirs +0");
    assert_eq!(out.stdout, "~", "stderr: {}", out.stderr);
    assert_eq!(out.code, 0);
}

#[test]
fn dirs_rejects_a_bare_word() {
    let sandbox = Sandbox::new("dirs-word");
    let out = sandbox.run("dirs foo");
    assert_eq!(out.code, 2, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("dirs: foo: invalid option")
            && out.stderr.contains("usage: dirs [-clpv] [+N] [-N]"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

#[test]
fn dirs_rejects_a_malformed_number() {
    let sandbox = Sandbox::new("dirs-number");
    let out = sandbox.run("dirs +1x");
    assert_eq!(out.code, 2, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("dirs: +1x: invalid number"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// pushd
// ---------------------------------------------------------------------------

#[test]
fn pushd_plus_n_rotates_the_stack() {
    // Not a lift: everything above the chosen entry comes round to the bottom.
    let sandbox = Sandbox::new("pushd-plus");
    let out = sandbox.run(&format!(
        r#"{BUILD} pushd +1; echo "pwd: $(basename "$PWD")""#
    ));
    assert_eq!(
        out.stdout, "~/b ~/a ~ ~/c\npwd: b",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn pushd_minus_n_rotates_from_the_far_end() {
    let sandbox = Sandbox::new("pushd-minus");
    let out = sandbox.run(&format!(
        r#"{BUILD} pushd -1; echo "pwd: $(basename "$PWD")""#
    ));
    assert_eq!(
        out.stdout, "~/a ~ ~/c ~/b\npwd: a",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn a_bare_pushd_exchanges_the_top_two() {
    // The shorthand people actually type; cash made it a usage error.
    let sandbox = Sandbox::new("pushd-bare");
    let out = sandbox.run(&format!(r#"{BUILD} pushd; echo "pwd: $(basename "$PWD")""#));
    assert_eq!(
        out.stdout, "~/b ~/c ~/a ~\npwd: b",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn a_bare_pushd_needs_something_to_exchange_with() {
    let sandbox = Sandbox::new("pushd-alone");
    let out = sandbox.run("pushd");
    assert_eq!(out.code, 1, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("pushd: no other directory"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

#[test]
fn rotating_by_zero_changes_nothing() {
    let sandbox = Sandbox::new("pushd-zero");
    let out = sandbox.run(&format!("{BUILD} pushd +0"));
    assert_eq!(out.stdout, "~/c ~/b ~/a ~", "stderr: {}", out.stderr);
}

#[test]
fn pushd_keeps_the_sign_when_the_index_is_out_of_range() {
    let sandbox = Sandbox::new("pushd-range");
    let out = sandbox.run(&format!("{BUILD} pushd +9"));
    assert_eq!(out.code, 1, "stdout: {}", out.stdout);
    assert!(
        out.stderr
            .contains("pushd: +9: directory stack index out of range"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

#[test]
fn pushd_n_rotates_without_moving_the_shell() {
    // bash's own oddity, matched deliberately: the entry that rotated into the current
    // slot is dropped, because the shell did not follow it. `[c b a ~]` becomes
    // `[c a ~ c]`, and nothing is printed.
    let sandbox = Sandbox::new("pushd-n-plus");
    let out = sandbox.run(&format!(
        r#"{BUILD} pushd -n +1; echo "printed: [$(true)]"; dirs; echo "pwd: $(basename "$PWD")""#
    ));
    assert_eq!(
        out.stdout, "printed: []\n~/c ~/a ~ ~/c\npwd: c",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn pushd_n_records_a_directory_without_checking_it() {
    // bash stores the argument as typed and only the later `popd` has to enter it.
    let sandbox = Sandbox::new("pushd-n-dir");
    let out = sandbox.run(&format!(
        r#"{BUILD} pushd -n nope; echo "pwd: $(basename "$PWD")""#
    ));
    assert_eq!(
        out.stdout, "~/c nope ~/b ~/a ~\npwd: c",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn pushd_still_pushes_a_directory() {
    let sandbox = Sandbox::new("pushd-dir");
    let out = sandbox.run(r#"pushd a; echo "pwd: $(basename "$PWD")""#);
    assert_eq!(out.stdout, "~/a ~\npwd: a", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// popd
// ---------------------------------------------------------------------------

#[test]
fn popd_plus_n_drops_one_entry_and_stays_put() {
    let sandbox = Sandbox::new("popd-plus");
    let out = sandbox.run(&format!(
        r#"{BUILD} popd +1; echo "pwd: $(basename "$PWD")""#
    ));
    assert_eq!(out.stdout, "~/c ~/a ~\npwd: c", "stderr: {}", out.stderr);
}

#[test]
fn popd_minus_n_counts_from_the_far_end() {
    let sandbox = Sandbox::new("popd-minus");
    let out = sandbox.run(&format!("{BUILD} popd -1"));
    assert_eq!(out.stdout, "~/c ~/b ~", "stderr: {}", out.stderr);
}

#[test]
fn popd_plus_zero_moves_the_shell() {
    let sandbox = Sandbox::new("popd-zero");
    let out = sandbox.run(&format!(
        r#"{BUILD} popd +0; echo "pwd: $(basename "$PWD")""#
    ));
    assert_eq!(out.stdout, "~/b ~/a ~\npwd: b", "stderr: {}", out.stderr);
}

#[test]
fn popd_n_drops_the_entry_above_the_current_one() {
    let sandbox = Sandbox::new("popd-n");
    let out = sandbox.run(&format!(
        r#"{BUILD} popd -n; echo "pwd: $(basename "$PWD")""#
    ));
    assert_eq!(out.stdout, "~/c ~/a ~\npwd: c", "stderr: {}", out.stderr);
}

#[test]
fn popd_keeps_the_sign_when_the_index_is_out_of_range() {
    let sandbox = Sandbox::new("popd-range");
    let out = sandbox.run(&format!("{BUILD} popd +9"));
    assert_eq!(out.code, 1, "stdout: {}", out.stdout);
    assert!(
        out.stderr
            .contains("popd: +9: directory stack index out of range"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

#[test]
fn popd_rejects_a_bare_word() {
    // bash calls it an argument rather than an option, because `popd` takes no operand
    // other than an offset.
    let sandbox = Sandbox::new("popd-word");
    let out = sandbox.run(&format!("{BUILD} popd foo"));
    assert_eq!(out.code, 2, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("popd: foo: invalid argument")
            && out.stderr.contains("usage: popd [-n] [+N | -N]"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

#[test]
fn an_empty_stack_is_reported_in_bashs_words() {
    // cash said "directory stack is empty", which is not what a script greps for.
    let sandbox = Sandbox::new("popd-empty");
    let out = sandbox.run("popd");
    assert_eq!(out.code, 1, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("popd: directory stack empty"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

#[test]
fn a_directory_it_cannot_enter_leaves_the_stack_alone() {
    // bash reports the failure and keeps the entry, so the stack still describes where
    // the shell has been.
    let sandbox = Sandbox::new("popd-missing");
    let out = sandbox.run(r#"pushd -n nope > /dev/null; popd; echo "code: $?"; dirs"#);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.first().copied(), Some("code: 1"), "stdout: {lines:?}");
    assert_eq!(
        lines.get(1).copied(),
        Some("~ nope"),
        "the entry was lost: {lines:?}"
    );
}

#[test]
fn popd_still_pops() {
    let sandbox = Sandbox::new("popd-plain");
    let out = sandbox.run(r#"pushd a > /dev/null; popd; echo "pwd: $(basename "$PWD")""#);
    assert_eq!(
        out.stdout,
        format!("~\npwd: cash-dirstack-popd-plain-{}", std::process::id()),
        "stderr: {}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// Edges
// ---------------------------------------------------------------------------

#[test]
fn a_selector_composes_with_the_other_options() {
    let sandbox = Sandbox::new("edge-options");
    let out = sandbox.run(&format!("{BUILD} dirs -p +1; dirs -l +1"));
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "stdout: {lines:?}");
    assert_eq!(lines[0], "~/b", "-p dropped the selector: {lines:?}");
    assert!(
        lines[1].ends_with("/b") && !lines[1].starts_with('~'),
        "-l should print the path unshortened: {lines:?}"
    );
}

#[test]
fn counting_from_the_far_end_still_reports_the_absolute_index() {
    // `-0` is the oldest entry, and `dirs -v` numbers it from the near end regardless.
    let sandbox = Sandbox::new("edge-minus-v");
    let out = sandbox.run(&format!("{BUILD} dirs -v -0"));
    assert_eq!(out.stdout, " 3  ~", "stderr: {}", out.stderr);
}

#[test]
fn both_far_ends_resolve() {
    let sandbox = Sandbox::new("edge-ends");
    let out = sandbox.run(&format!("{BUILD} dirs +3; dirs -3"));
    assert_eq!(out.stdout, "~\n~/c", "stderr: {}", out.stderr);
}

#[test]
fn one_step_past_either_end_is_out_of_range() {
    let sandbox = Sandbox::new("edge-past");
    let out = sandbox.run(&format!(
        r#"{BUILD} dirs +4; echo "a: $?"; dirs -4; echo "b: $?""#
    ));
    assert_eq!(out.stdout, "a: 1\nb: 1", "stderr: {}", out.stderr);
    assert_eq!(
        out.stderr
            .matches("dirs: 4: directory stack index out of range")
            .count(),
        2,
        "unexpected diagnostics: {}",
        out.stderr
    );
}

#[test]
fn rotating_to_the_last_entry_brings_everything_round() {
    let sandbox = Sandbox::new("edge-rotate-last");
    let out = sandbox.run(&format!(
        r#"{BUILD} pushd +3; echo "pwd: $(basename "$PWD")""#
    ));
    assert_eq!(
        out.stdout,
        format!(
            "~ ~/c ~/b ~/a\npwd: cash-dirstack-edge-rotate-last-{}",
            std::process::id()
        ),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn pushd_minus_zero_takes_the_oldest_entry() {
    // The same rotation as `+3` here, reached from the other end.
    let sandbox = Sandbox::new("edge-pushd-minus-zero");
    let out = sandbox.run(&format!("{BUILD} pushd -0"));
    assert_eq!(out.stdout, "~ ~/c ~/b ~/a", "stderr: {}", out.stderr);
}

#[test]
fn popd_minus_zero_drops_the_oldest_entry() {
    let sandbox = Sandbox::new("edge-popd-minus-zero");
    let out = sandbox.run(&format!(
        r#"{BUILD} popd -0; echo "pwd: $(basename "$PWD")""#
    ));
    assert_eq!(out.stdout, "~/c ~/b ~/a\npwd: c", "stderr: {}", out.stderr);
}

#[test]
fn popd_n_drops_the_same_entry_whatever_offset_it_is_given() {
    // bash performs the removal and then puts the unchanged working directory back at
    // the top, so `-n +0` and `-n +1` land in the same place.
    let sandbox = Sandbox::new("edge-popd-n");
    let out = sandbox.run(&format!(r#"{BUILD} popd -n +1 > /dev/null; dirs"#));
    let other = Sandbox::new("edge-popd-n-zero");
    let out_zero = other.run(&format!(r#"{BUILD} popd -n +0 > /dev/null; dirs"#));
    assert_eq!(out.stdout, "~/c ~/a ~", "stderr: {}", out.stderr);
    assert_eq!(out_zero.stdout, "~/c ~/a ~", "stderr: {}", out_zero.stderr);
}

#[test]
fn a_bare_pushd_n_does_nothing_at_all() {
    // There is no exchange that would not also move the shell, so bash declines silently
    // and successfully rather than reporting anything.
    let sandbox = Sandbox::new("edge-pushd-n-bare");
    let out = sandbox.run(&format!(
        r#"{BUILD} printed=$(pushd -n); echo "printed: [$printed] code: $?"; dirs"#
    ));
    assert_eq!(
        out.stdout, "printed: [] code: 0\n~/c ~/b ~/a ~",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn with_nothing_pushed_either_zero_names_the_current_directory() {
    let sandbox = Sandbox::new("edge-empty-zero");
    let out = sandbox.run(r#"dirs -0; dirs -v; pushd +0; pushd -0; echo "code: $?""#);
    assert_eq!(
        out.stdout, "~\n 0  ~\n~\n~\ncode: 0",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn with_nothing_pushed_popping_either_zero_is_an_empty_stack() {
    let sandbox = Sandbox::new("edge-empty-popd");
    let out = sandbox.run(r#"popd +0; echo "a: $?"; popd -0; echo "b: $?""#);
    assert_eq!(out.stdout, "a: 1\nb: 1", "stderr: {}", out.stderr);
    assert_eq!(
        out.stderr.matches("popd: directory stack empty").count(),
        2,
        "unexpected diagnostics: {}",
        out.stderr
    );
}

#[test]
fn a_directory_with_a_space_survives_the_stack() {
    // Not an exotic case on Windows, where `Program Files` is on everyone's machine.
    let sandbox = Sandbox::new("edge-space");
    let out = sandbox.run(&format!(
        r#"{BUILD} pushd "../d e" > /dev/null; dirs +0; echo "pwd: $(basename "$PWD")"; popd > /dev/null; dirs +0"#
    ));
    assert_eq!(out.stdout, "~/d e\npwd: d e\n~/c", "stderr: {}", out.stderr);
}

#[test]
fn pushing_a_directory_that_is_not_there_leaves_the_stack_alone() {
    let sandbox = Sandbox::new("edge-push-missing");
    let out = sandbox.run(&format!(r#"{BUILD} pushd nope; echo "code: $?"; dirs"#));
    assert_eq!(
        out.stdout, "code: 1\n~/c ~/b ~/a ~",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn popping_into_a_directory_that_was_removed_keeps_it_listed() {
    // The realistic version of the same rule: the entry was good when it was pushed. The
    // stack still describes where the shell has been, so the caller can see what went.
    let sandbox = Sandbox::new("edge-pop-removed");
    let out = sandbox.run(
        r#"mkdir gone; pushd gone > /dev/null; pushd ../a > /dev/null; rmdir ../gone; popd; echo "code: $?"; dirs"#,
    );
    assert_eq!(
        out.stdout, "code: 1\n~/a ~/gone ~",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn cd_moves_the_entry_at_the_top() {
    // Index 0 is the working directory itself, not a copy taken when it was pushed.
    let sandbox = Sandbox::new("edge-cd");
    let out = sandbox.run(r#"pushd a > /dev/null; cd ..; dirs"#);
    assert_eq!(out.stdout, "~ ~", "stderr: {}", out.stderr);
}
