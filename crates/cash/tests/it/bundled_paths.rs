//! Bundled utilities render paths cash's way — **D3** via **D48**.
//!
//! D48 puts uutils and findutils inside the cash binary. They are portable Rust, so the
//! few that *construct* an absolute path spell it the way Windows does, and `mktemp -d`
//! printing `C:\Users\me\AppData\Local\Temp\tmp.AbCdEf` is how that spelling gets into a
//! script in the first place: almost every use is `d=$(mktemp -d)`.
//!
//! The consequence is not cosmetic. `xargs` reads `\` as an escape, so a plain
//! `find "$d" | xargs grep` silently turns `C:\Users\me\...` into `C:Usersme...` and then
//! reports that nothing matched — a wrong answer, not an error, which is exactly what
//! D20 and D26 exist to prevent elsewhere.

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

/// Assert that every line of `stdout` is a canonically spelled path.
fn assert_canonical(out: &Output, what: &str) {
    assert!(
        !out.stdout.contains('\\'),
        "{what} emitted a backslash path: {} (stderr: {})",
        out.stdout,
        out.stderr
    );
    assert!(
        !out.stdout.contains("//?/"),
        "{what} leaked the extended-length prefix: {}",
        out.stdout
    );
}

// ---------------------------------------------------------------------------
// The sources of constructed paths
// ---------------------------------------------------------------------------

#[test]
fn mktemp_d_prints_a_canonical_path() {
    let out = cash(r#"d=$(mktemp -d); echo "$d"; [ -d "$d" ] && echo exists; cd /; rm -rf "$d""#);
    assert_canonical(&out, "mktemp -d");
    assert!(
        out.stdout.contains("exists"),
        "the directory was not created: {}",
        out.stdout
    );
    assert!(
        out.stdout.starts_with("C:/") || out.stdout.starts_with("//"),
        "not an absolute path: {}",
        out.stdout
    );
}

#[test]
fn mktemp_prints_a_canonical_path_for_a_file_too() {
    let out = cash(r#"f=$(mktemp); echo "$f"; [ -f "$f" ] && echo exists; rm -f "$f""#);
    assert_canonical(&out, "mktemp");
    assert!(
        out.stdout.contains("exists"),
        "the file was not created: {}",
        out.stdout
    );
}

#[test]
fn mktemp_u_prints_a_canonical_path_it_did_not_create() {
    let out = cash(r#"p=$(mktemp -u); echo "$p"; [ -e "$p" ] || echo absent"#);
    assert_canonical(&out, "mktemp -u");
    assert!(
        out.stdout.contains("absent"),
        "-u created something: {}",
        out.stdout
    );
}

#[test]
fn realpath_prints_a_canonical_path() {
    let out =
        cash(r#"d=$(mktemp -d); mkdir -p "$d/a/b"; cd "$d/a"; realpath ./b; cd /; rm -rf "$d""#);
    assert_canonical(&out, "realpath");
    assert!(
        out.stdout.ends_with("/b"),
        "unexpected output: {}",
        out.stdout
    );
    assert_eq!(out.code, 0);
}

#[test]
fn readlink_f_prints_a_canonical_path() {
    let out = cash(r#"d=$(mktemp -d); : > "$d/f"; readlink -f "$d/f"; cd /; rm -rf "$d""#);
    assert_canonical(&out, "readlink -f");
    assert!(
        out.stdout.ends_with("/f"),
        "unexpected output: {}",
        out.stdout
    );
}

#[test]
fn a_constructed_path_with_spaces_stays_one_argument() {
    let out = cash(
        r#"d=$(mktemp -d); mkdir -p "$d/Program Files"; p=$(realpath "$d/Program Files"); echo "[$p]"; [ -d "$p" ] && echo ok; cd /; rm -rf "$d""#,
    );
    assert_canonical(&out, "realpath with spaces");
    assert!(
        out.stdout.contains("Program Files"),
        "the space was lost: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("ok"),
        "the rendered path did not resolve: {}",
        out.stdout
    );
}

#[test]
fn a_constructed_path_with_unicode_survives() {
    let out =
        cash(r#"d=$(mktemp -d); mkdir -p "$d/Ünïcodé"; realpath "$d/Ünïcodé"; cd /; rm -rf "$d""#);
    assert_canonical(&out, "realpath with unicode");
    assert!(
        out.stdout.contains("Ünïcodé"),
        "unicode was mangled: {}",
        out.stdout
    );
}

// ---------------------------------------------------------------------------
// The failure this prevents
// ---------------------------------------------------------------------------

#[test]
fn find_piped_to_xargs_survives_a_temp_directory() {
    // The §4 entry this fix retired. `xargs` reads `\` as an escape, so a backslash path
    // arrives at the next command with its separators eaten — and `grep` then reports no
    // matches rather than failing.
    let out = cash(
        r#"
        set -euo pipefail
        d=$(mktemp -d)
        mkdir -p "$d/src"
        printf 'TODO: x\n' > "$d/src/a.rs"
        printf 'nothing\n'  > "$d/src/b.rs"
        n=$(find "$d" -name '*.rs' | xargs grep -l 'TODO' | wc -l | tr -d ' ')
        echo "matched=$n"
        cd /; rm -rf "$d"
        "#,
    );
    assert_eq!(out.stdout, "matched=1", "stderr: {}", out.stderr);
    assert_eq!(out.code, 0);
}

#[test]
fn the_null_delimited_form_works_as_well() {
    // The idiom that is correct on Linux too, because it survives filenames with spaces.
    let out = cash(
        r#"
        set -euo pipefail
        d=$(mktemp -d)
        mkdir -p "$d/a dir"
        : > "$d/a dir/f.txt"
        find "$d" -name '*.txt' -print0 | xargs -0 wc -l | tr -s ' ' | cut -d' ' -f1
        cd /; rm -rf "$d"
        "#,
    );
    assert_eq!(out.stdout, "0", "stderr: {}", out.stderr);
}

#[test]
fn realpath_z_output_feeds_xargs_0() {
    let out = cash(
        r#"d=$(mktemp -d); : > "$d/f"; realpath -z "$d/f" | xargs -0 basename; cd /; rm -rf "$d""#,
    );
    assert_eq!(out.stdout, "f", "stderr: {}", out.stderr);
}

#[test]
fn a_temp_directory_can_be_entered_and_reported() {
    // `cd "$(mktemp -d)"; pwd` must agree with itself. Before, `cd` accepted the
    // backslash spelling (D3 accepts every spelling) and `pwd` rendered the canonical
    // one, so a script comparing the two saw a mismatch.
    let out = cash(
        r#"d=$(mktemp -d); cd "$d"; [ "$(pwd)" = "$d" ] && echo agree || echo "differ: [$d] [$(pwd)]"; cd /; rm -rf "$d""#,
    );
    assert_eq!(out.stdout, "agree", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// What must NOT change
// ---------------------------------------------------------------------------

#[test]
fn help_and_version_stay_prose() {
    // Those outputs are not paths, and some argument parsers exit the process while
    // printing them, which would strand the text in the capture file.
    let help = cash("mktemp --help");
    assert!(
        help.stdout.contains("Usage"),
        "help was lost: {}",
        help.stdout
    );

    let version = cash("realpath --version");
    assert!(
        version.stdout.contains("coreutils"),
        "version was lost: {}",
        version.stdout
    );
}

#[test]
fn an_error_still_reaches_stderr_with_its_exit_code() {
    // stderr is deliberately not captured, so diagnostics keep their ordering.
    let out = cash("realpath /definitely/not/here/at/all; echo rc=$?");
    assert!(
        out.stdout.contains("rc=1"),
        "exit code lost: {}",
        out.stdout
    );
    assert!(
        !out.stderr.is_empty(),
        "the diagnostic was swallowed by the capture"
    );
}

#[test]
fn utilities_that_echo_their_input_are_left_alone() {
    // `dirname`, `basename` and `find` return a spelling that came from the script.
    // Rewriting those would silently change data the user chose.
    let out = cash(r#"dirname 'C:\keep\this'; basename 'C:\keep\this'"#);
    assert_eq!(
        out.stdout, "C:\\keep\nthis",
        "an echoing utility was rewritten: {:?}",
        out.stdout
    );
}

#[test]
fn ordinary_output_containing_backslashes_is_untouched() {
    // Only the path-emitting allowlist is captured; `cat` and friends stay byte
    // transparent, which D20 also promises.
    let out = cash(r#"printf 'a\\b\n' | cat"#);
    assert_eq!(
        out.stdout, "a\\b",
        "a non-path utility's output was rewritten"
    );
}

#[test]
fn mktemp_output_is_a_single_line_with_no_stray_bytes() {
    // The capture reassembles the output byte for byte; a doubled or missing newline
    // would show up as an empty first element here.
    let out = cash(r#"mktemp -u | wc -l | tr -d ' '"#);
    assert_eq!(out.stdout, "1", "stderr: {}", out.stderr);
}

#[test]
fn several_paths_in_one_invocation_are_each_rendered() {
    let out = cash(
        r#"d=$(mktemp -d); : > "$d/a"; : > "$d/b"; realpath "$d/a" "$d/b" | wc -l | tr -d ' '; realpath "$d/a" "$d/b" | grep -c '\\' || true; cd /; rm -rf "$d""#,
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("2"),
        "wrong line count: {}",
        out.stdout
    );
    assert_eq!(
        lines.get(1).copied(),
        Some("0"),
        "a backslash survived in multi-argument output: {}",
        out.stdout
    );
}

/// A bundled tool runs as `cash.exe --invoke-bundled cut`, and named itself after the first
/// word of that command line: `cash.exe: you must specify a list of bytes, characters, or
/// fields` and `Try 'C:/…/cash.exe --help'` (TODO 4.5). GNU's names itself `cut`.
#[test]
fn a_bundled_tool_names_itself_in_its_messages() {
    let out = cash("cut");
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr.replace("\r\n", "\n"),
        "cut: you must specify a list of bytes, characters, or fields\n\
         Try 'cut --help' for more information."
    );

    let out = cash("cat /nonexistent; wc /nonexistent; head -n x");
    for (line, tool) in out.stderr.lines().zip(["cat: ", "wc: ", "head: "]) {
        assert!(line.starts_with(tool), "{line}");
    }
    assert!(!out.stderr.contains("cash"), "{}", out.stderr);

    // The arguments the tool reads are its own, quotes and patterns as cash passed them.
    let out = cash(r#"echo "a*b c" | cut -d " " -f1; cut -c1-2 nothing*here 2>&1"#);
    assert!(
        out.stdout.starts_with("a*b\ncut: 'nothing*here': "),
        "{}",
        out.stdout
    );
}
