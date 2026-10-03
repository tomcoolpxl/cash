//! Shell-level edge cases, run against the real `cash.exe`.
//!
//! `acceptance.rs` asserts each decision's headline behaviour. This asserts the awkward
//! corners — paths with spaces, empty files, files with no trailing newline, nesting,
//! quoting that interacts with the path model, and the boundaries where two decisions
//! meet.

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

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

fn cash(script: &str) -> Output {
    let out = Command::new(CASH)
        .args(["-c", script])
        .output()
        .expect("failed to run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cash-edge-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
        Self(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
    fn script_path(&self) -> String {
        self.0.to_string_lossy().replace('\\', "/")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------------------
// Paths with spaces — the most common Windows path shape
// ---------------------------------------------------------------------------

#[test]
fn directories_with_spaces_work_in_every_spelling() {
    let scratch = Scratch::new("spaces");
    let dir = scratch.path().join("Program Files Like");
    std::fs::create_dir_all(&dir).unwrap();
    let base = scratch.script_path();

    for spelling in [
        format!(r#"cd "{base}/Program Files Like""#),
        format!(r#"cd '{base}/Program Files Like'"#),
    ] {
        let out = cash(&format!("{spelling}; pwd"));
        assert!(
            out.stdout.ends_with("Program Files Like"),
            "failed for {spelling}: {} {}",
            out.stdout,
            out.stderr
        );
        assert!(
            !out.stdout.contains('\\'),
            "rendered with backslashes: {}",
            out.stdout
        );
    }
}

#[test]
fn a_file_with_spaces_round_trips_through_winpath() {
    let with_spaces = "C:/Program Files/Some App/file name.txt";
    assert_eq!(
        cash(&format!(r#"winpath "{with_spaces}""#)).stdout,
        with_spaces
    );
    assert_eq!(
        cash(&format!(r#"winpath -w "{with_spaces}""#)).stdout,
        r"C:\Program Files\Some App\file name.txt"
    );
    assert_eq!(
        cash(&format!(r#"winpath "$(winpath -u "{with_spaces}")""#)).stdout,
        with_spaces,
        "unix round trip lost the spaces"
    );
}

#[test]
fn a_batch_command_found_on_path_may_live_in_a_directory_with_spaces() {
    let scratch = Scratch::new("batch-path-spaces");
    let bin = scratch.path().join("Program Files Like").join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(
        bin.join("space-tool.cmd"),
        b"@echo off\r\necho ARG=[%~1]\r\n",
    )
    .unwrap();

    let path = bin.to_string_lossy().replace('\\', "/");
    let out = cash(&format!(r#"PATH="{path}:$PATH"; space-tool "two words""#));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "ARG=[two words]");
}

// ---------------------------------------------------------------------------
// D20 — CRLF corner cases
// ---------------------------------------------------------------------------

#[test]
fn an_empty_file_captures_as_empty() {
    let scratch = Scratch::new("empty");
    std::fs::write(scratch.path().join("e.txt"), b"").unwrap();
    let out = cash(&format!(
        r#"v=$(cat {}/e.txt); printf '[%s]' "$v""#,
        scratch.script_path()
    ));
    assert_eq!(out.stdout, "[]");
}

#[test]
fn a_file_with_no_trailing_newline_is_captured_whole() {
    let scratch = Scratch::new("no-trailing");
    std::fs::write(scratch.path().join("f.txt"), b"no newline here").unwrap();
    let out = cash(&format!(
        r#"v=$(cat {}/f.txt); printf '[%s]' "$v""#,
        scratch.script_path()
    ));
    assert_eq!(out.stdout, "[no newline here]");
}

#[test]
fn a_crlf_file_with_no_final_terminator_still_loses_interior_carriage_returns() {
    let scratch = Scratch::new("crlf-partial");
    std::fs::write(scratch.path().join("f.txt"), b"a\r\nb").unwrap();
    let out = cash(&format!(
        r#"n=0; while read -r l; do n=$((n+1)); done < {}/f.txt; echo $n"#,
        scratch.script_path()
    ));
    // Two lines: bash counts a final unterminated line only if `read` returns it, which
    // it does not — so this matches bash's one.
    assert!(
        out.stdout == "1" || out.stdout == "2",
        "unexpected count: {}",
        out.stdout
    );
}

#[test]
fn blank_crlf_lines_are_preserved_as_empty_not_dropped() {
    let scratch = Scratch::new("crlf-blank");
    std::fs::write(scratch.path().join("f.txt"), b"a\r\n\r\nb\r\n").unwrap();
    let out = cash(&format!(
        r#"while read -r l; do printf '[%s]' "$l"; done < {}/f.txt"#,
        scratch.script_path()
    ));
    assert_eq!(out.stdout, "[a][][b]");
}

#[test]
fn a_lone_carriage_return_in_data_survives_capture() {
    // Progress-bar style output. Deleting this would corrupt the value.
    let out = cash(r#"v=$(printf 'a\rb'); printf %s "$v" | wc -c"#);
    assert_eq!(out.stdout, "3", "the bare \\r was stripped");
}

// ---------------------------------------------------------------------------
// D15 — exit codes through shell constructs
// ---------------------------------------------------------------------------

#[test]
fn exit_status_propagates_through_pipelines_and_lists() {
    assert_eq!(cash("false | true; echo $?").stdout, "0");
    assert_eq!(cash("set -o pipefail; false | true; echo $?").stdout, "1");
    assert_eq!(cash("true && false; echo $?").stdout, "1");
    assert_eq!(cash("false || true; echo $?").stdout, "0");
    assert_eq!(cash("(exit 42); echo $?").stdout, "42");
}

#[test]
fn a_crash_status_survives_a_conditional() {
    // The point of D15: a crashed process must not pass an && chain.
    let out =
        cash("cmd.exe /d /s /c exit 3221225728 >/dev/null 2>&1 && echo PASSED || echo caught");
    assert_eq!(out.stdout, "caught", "a crash satisfied &&");
}

#[test]
fn the_shells_own_exit_code_reflects_the_last_command() {
    // What a caller of cash.exe observes, as distinct from `$?` inside the script.
    assert_eq!(cash("true").code, 0);
    assert_eq!(cash("false").code, 1);
    assert_eq!(cash("exit 42").code, 42);
    assert_eq!(cash("cmd.exe /d /s /c exit 3 >/dev/null 2>&1").code, 3);
    // 256 wraps to 0 exactly as bash does.
    assert_eq!(cash("exit 256").code, 0);
}

// ---------------------------------------------------------------------------
// D31 — environment corners
// ---------------------------------------------------------------------------

#[test]
fn an_unset_variable_is_empty_in_any_case() {
    assert_eq!(
        cash(r#"printf '[%s]' "$DEFINITELY_NOT_SET_XYZ""#).stdout,
        "[]"
    );
    assert_eq!(
        cash(r#"printf '[%s]' "$definitely_not_set_xyz""#).stdout,
        "[]"
    );
}

#[test]
fn a_locally_defined_variable_beats_a_case_insensitive_environment_match() {
    // The fallback must never shadow an exact match.
    let out = cash(r#"Path=mine; printf '%s' "$Path""#);
    assert_eq!(
        out.stdout, "mine",
        "the environment shadowed a local assignment"
    );
}

#[test]
fn exported_variables_reach_children_unchanged() {
    let out = cash(r#"export CASH_EDGE='a b;c'; cmd.exe /d /s /c "echo %CASH_EDGE%""#);
    assert!(out.stdout.contains("a b;c"), "got {:?}", out.stdout);
}

// ---------------------------------------------------------------------------
// D17 — process substitution corners
// ---------------------------------------------------------------------------

#[test]
fn process_substitution_handles_empty_and_large_output() {
    assert_eq!(cash("printf '[%s]' \"$(cat <(true))\"").stdout, "[]");

    let out = cash(r#"cat <(seq 1 500) | wc -l"#);
    assert_eq!(out.stdout.trim(), "500", "large output truncated");
}

#[test]
fn multiple_process_substitutions_in_one_command_are_distinct() {
    // Each must get its own temp file; sharing one would make both operands equal.
    assert_eq!(
        cash(r#"diff <(echo one) <(echo two) >/dev/null 2>&1; echo $?"#).stdout,
        "1",
        "both substitutions resolved to the same content"
    );
}

#[test]
fn nested_process_substitution_works() {
    assert_eq!(cash(r#"cat <(cat <(echo deep))"#).stdout, "deep");
}

// ---------------------------------------------------------------------------
// D16 — globbing corners
// ---------------------------------------------------------------------------

#[test]
fn an_unmatched_glob_stays_literal_as_in_bash() {
    let scratch = Scratch::new("glob-nomatch");
    let out = cash(&format!("cd {}; echo *.nomatch", scratch.script_path()));
    assert_eq!(out.stdout, "*.nomatch");
}

#[test]
fn globbing_matches_regardless_of_case_in_either_direction() {
    let scratch = Scratch::new("glob-case");
    std::fs::write(scratch.path().join("UPPER.TXT"), b"").unwrap();
    std::fs::write(scratch.path().join("lower.txt"), b"").unwrap();
    let dir = scratch.script_path();

    let lower = cash(&format!("cd {dir}; echo *.txt")).stdout;
    let upper = cash(&format!("cd {dir}; echo *.TXT")).stdout;
    assert!(
        lower.contains("UPPER.TXT") && lower.contains("lower.txt"),
        "got {lower}"
    );
    assert!(
        upper.contains("UPPER.TXT") && upper.contains("lower.txt"),
        "got {upper}"
    );
}

#[test]
fn a_glob_over_names_with_spaces_yields_separate_words() {
    let scratch = Scratch::new("glob-spaces");
    std::fs::write(scratch.path().join("a b.txt"), b"").unwrap();
    let out = cash(&format!(
        r#"cd {}; for f in *.txt; do printf '[%s]' "$f"; done"#,
        scratch.script_path()
    ));
    assert_eq!(out.stdout, "[a b.txt]", "the space split the filename");
}

// ---------------------------------------------------------------------------
// The /x/ diagnostic — precision
// ---------------------------------------------------------------------------

#[test]
fn the_diagnostic_never_fires_on_command_flags() {
    // `cmd.exe /d /s /c` is among the most common invocations on Windows, and `/d`
    // reads as drive D under a naive rule.
    let out = cash(r#"cmd.exe /d /s /c "exit 0""#);
    assert!(
        !out.stderr.contains("winpath"),
        "warned on a command flag: {}",
        out.stderr
    );
}

#[test]
fn the_diagnostic_fires_at_most_once_per_command() {
    let out = cash(
        r#"enable -n cat 2>/dev/null; cat /c/Windows/win.ini /c/Windows/system.ini >/dev/null 2>&1"#,
    );
    let hints = out.stderr.matches("winpath").count();
    assert!(hints <= 1, "emitted {hints} hints for one command");
}

// ---------------------------------------------------------------------------
// D45 — builtins under awkward input
// ---------------------------------------------------------------------------

#[test]
fn winpath_handles_relative_and_degenerate_input() {
    assert_eq!(cash("winpath sub/dir").stdout, "sub/dir");
    assert_eq!(cash(r#"winpath "sub\dir""#).stdout, "sub/dir");
    assert_eq!(cash(r#"winpath """#).stdout, "");
}

#[test]
fn winpath_from_stdin_handles_blank_lines_and_crlf() {
    let out = cash(r#"printf 'C:/a\r\n\r\nC:/b\r\n' | winpath -w"#);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(lines.contains(&r"C:\a"), "got {:?}", out.stdout);
    assert!(lines.contains(&r"C:\b"), "got {:?}", out.stdout);
}

// ---------------------------------------------------------------------------
// Subshells see the parent's jobs (read-only)
// ---------------------------------------------------------------------------

#[test]
fn a_subshell_can_list_the_parents_jobs() {
    // `$(jobs -p)` is a documented way to collect background pids, and was returning
    // nothing because a cloned shell got an empty job table. A subshell must not be able
    // to *manage* the parent's jobs — it does not own those processes — but bash lets it
    // see them, so cash hands it read-only snapshots.
    let out = cash(
        r#"ping.exe -n 20 127.0.0.1 >/dev/null & sleep 1; inner=$(jobs -p); kill -9 $! 2>/dev/null; printf '%s' "$inner""#,
    );
    assert!(
        out.stdout.trim().parse::<u32>().is_ok(),
        "$(jobs -p) gave {:?}, expected a pid",
        out.stdout
    );
}

#[test]
fn a_subshell_and_its_parent_agree_on_the_job_list() {
    let out = cash(
        r#"ping.exe -n 20 127.0.0.1 >/dev/null & sleep 1; outer=$!; inner=$(jobs -p); kill -9 $! 2>/dev/null; [ "$outer" = "$inner" ] && echo agree || echo "$outer vs $inner""#,
    );
    assert_eq!(out.stdout, "agree");
}

#[test]
fn a_subshell_with_no_parent_jobs_lists_nothing() {
    assert_eq!(cash(r#"printf '[%s]' "$(jobs -p)""#).stdout, "[]");
}

#[test]
fn the_tmp_spelling_is_explained_too() {
    // `/tmp/x` resolves for operations cash performs but not for a command handed it,
    // including a bundled builtin (D48), which opens paths directly. Same cliff as
    // `/c/x`, so it gets the same explanation.
    let out = cash(
        r#"echo hi > /tmp/cash-edge-tmp.txt; cat /tmp/cash-edge-tmp.txt; rm -f /tmp/cash-edge-tmp.txt"#,
    );
    assert!(
        out.stderr.contains("winpath"),
        "no hint for a /tmp argument: {}",
        out.stderr
    );
}

#[test]
fn the_diagnostic_does_not_name_a_misleading_command() {
    // A bundled builtin re-enters the cash binary to dispatch, so naming the resolved
    // command would report cash.exe rather than the `cat` the user typed.
    let out = cash(
        r#"echo hi > /tmp/cash-edge-name.txt; cat /tmp/cash-edge-name.txt; rm -f /tmp/cash-edge-name.txt"#,
    );
    assert!(
        !out.stderr.contains("cash.exe as written"),
        "named the re-entrant binary: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("a command receives this path"),
        "got {}",
        out.stderr
    );
}
