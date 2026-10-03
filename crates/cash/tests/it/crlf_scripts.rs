//! CRLF script files, end to end — **D7**.
//!
//! `cash-win32`'s `crlf_source.rs` proves the adapter. This proves the thing that
//! actually matters: a script file saved with Windows line endings runs, and produces
//! exactly what the LF-ending version of the same script produces.
//!
//! This is not an exotic case. `core.autocrlf` is `true` by default in Git for Windows,
//! so every `.sh` in a repository checked out here has CRLF endings unless somebody
//! wrote a `.gitattributes`. Before this worked, such a file failed to parse entirely,
//! and the reported position was wherever the parser finally gave up — nowhere near the
//! cause.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    clippy::literal_string_with_formatting_args,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result. Shell snippets are spelled with hashes \
              throughout, including where they are not strictly needed, because \
              alternating the two forms by accident of content reads worse."
)]

use std::path::Path;

use crate::common::{Output, Scratch, cash_command, output_of};

fn run(path: &Path) -> Output {
    output_of(cash_command().arg(path.to_string_lossy().replace('\\', "/")))
}

/// Write `source` twice — once with LF endings, once with CRLF — run both, and require
/// that they agree. Comparing the two spellings rather than a golden string means the
/// assertion is exactly D7's claim and nothing else.
fn both_line_endings_agree(name: &str, source: &str) -> Output {
    let scratch = Scratch::new(name);

    let lf_path = scratch.path().join("lf.sh");
    let crlf_path = scratch.path().join("crlf.sh");

    let lf = source.replace("\r\n", "\n");
    let crlf = lf.replace('\n', "\r\n");
    assert!(
        crlf.contains("\r\n"),
        "test source has no line breaks to convert"
    );

    std::fs::write(&lf_path, lf.as_bytes()).expect("write lf script");
    std::fs::write(&crlf_path, crlf.as_bytes()).expect("write crlf script");

    let lf_run = run(&lf_path);
    let crlf_run = run(&crlf_path);

    assert_eq!(
        crlf_run.code, lf_run.code,
        "exit status differed between line endings.\nLF: {} {}\nCRLF: {} {}",
        lf_run.stdout, lf_run.stderr, crlf_run.stdout, crlf_run.stderr
    );
    assert_eq!(
        crlf_run.stdout, lf_run.stdout,
        "stdout differed between line endings"
    );
    assert_eq!(
        crlf_run.stderr, lf_run.stderr,
        "stderr differed between line endings"
    );

    crlf_run
}

// ---------------------------------------------------------------------------
// The constructs where a stray \r is fatal
// ---------------------------------------------------------------------------

#[test]
fn a_crlf_script_runs_at_all() {
    let out = both_line_endings_agree("basic", "echo one\necho two\n");
    assert_eq!(out.stdout, "one\ntwo");
    assert_eq!(out.code, 0);
}

#[test]
fn block_keywords_are_recognised_with_a_return_after_them() {
    // `fi\r`, `done\r`, `esac\r` — the failure that started this. The parser would run
    // to end of input looking for a terminator it had already passed.
    let out = both_line_endings_agree(
        "keywords",
        "if [ 1 -eq 1 ]; then\n    echo if-taken\nfi\n\
         for i in a b; do\n    echo loop-$i\ndone\n\
         case x in\n    x) echo case-hit ;;\nesac\n\
         while [ -n \"${first:-}\" ]; do\n    break\ndone\n\
         echo end\n",
    );
    assert_eq!(out.stdout, "if-taken\nloop-a\nloop-b\ncase-hit\nend");
}

#[test]
fn a_here_document_delimiter_matches_with_a_return_after_it() {
    // `EOF\r` is not `EOF`, so the here-document would swallow the rest of the file.
    let out = both_line_endings_agree("heredoc", "cat <<'EOF'\nalpha\nbeta\nEOF\necho after\n");
    assert_eq!(out.stdout, "alpha\nbeta\nafter");
}

#[test]
fn here_document_content_is_delivered_with_lf_endings() {
    // The content of a here-document in a CRLF file is the script's own text, and D20
    // says cash decides where those lines end. Delivering `alpha\r\n` would make
    // `[ "$(cat <<EOF ...)" = alpha ]` fail while printing identically.
    let scratch = Scratch::new("heredoc-bytes");
    let script = scratch.path().join("s.sh");
    std::fs::write(
        &script,
        b"cat <<'EOF' > out.txt\r\nalpha\r\nEOF\r\nod -c out.txt | head -1\r\n",
    )
    .expect("write");

    let out = cash_command()
        .arg(script.to_string_lossy().replace('\\', "/"))
        .current_dir(scratch.path())
        .output()
        .expect("failed to run cash");

    let written = std::fs::read(scratch.path().join("out.txt")).expect("heredoc output");
    assert_eq!(
        written,
        b"alpha\n",
        "here-document content kept a carriage return: {written:?} (stderr: {})",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_line_continuation_survives() {
    // `\` then `\r\n` is a backslash escaping a carriage return, not a continuation.
    let out = both_line_endings_agree("continuation", "echo one \\\n     two \\\n     three\n");
    assert_eq!(out.stdout, "one two three");
}

#[test]
fn a_trailing_pipe_or_operator_continues_onto_the_next_line() {
    let out = both_line_endings_agree(
        "operators",
        "printf 'b\\na\\n' |\n    sort |\n    tr '\\n' ' '\necho\ntrue &&\n    echo and-taken\nfalse ||\n    echo or-taken\n",
    );
    assert_eq!(out.stdout, "a b \nand-taken\nor-taken");
}

#[test]
fn a_function_definition_spanning_lines_parses() {
    let out = both_line_endings_agree(
        "function",
        "greet() {\n    local who=\"$1\"\n    echo \"hello $who\"\n}\ngreet world\n",
    );
    assert_eq!(out.stdout, "hello world");
}

#[test]
fn comments_do_not_swallow_the_following_line() {
    let out = both_line_endings_agree(
        "comments",
        "# a comment with `backticks` and 'quotes' and \"more\"\necho after-comment\n",
    );
    assert_eq!(out.stdout, "after-comment");
}

#[test]
fn a_multi_line_single_quoted_string_keeps_lf_inside() {
    // A literal newline inside quotes is data, and on Linux that data is one byte. A
    // CRLF checkout should not silently change a script's string constants.
    let out = both_line_endings_agree(
        "quoted",
        "s='first\nsecond'\nprintf '%s' \"$s\" | od -c | head -1\n",
    );
    assert!(
        !out.stdout.contains("\\r"),
        "a carriage return leaked into a quoted string: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("\\n"),
        "the newline was lost: {}",
        out.stdout
    );
}

#[test]
fn a_shebang_line_with_a_return_still_identifies_the_script() {
    let out = both_line_endings_agree(
        "shebang",
        "#!/usr/bin/env bash\nset -euo pipefail\necho shebang-ok\n",
    );
    assert_eq!(out.stdout, "shebang-ok");
    assert_eq!(out.code, 0);
}

#[test]
fn set_e_does_not_fire_spuriously_on_a_crlf_script() {
    // If `\r` reached the command line, almost any command would fail and `set -e` would
    // abort — a failure mode that looks like the script's own logic is wrong.
    let out = both_line_endings_agree(
        "set-e",
        "set -euo pipefail\nX=1\nif [ \"$X\" -eq 1 ]; then\n    echo guarded\nfi\necho done\n",
    );
    assert_eq!(out.stdout, "guarded\ndone");
    assert_eq!(out.code, 0);
}

#[test]
fn a_numeric_comparison_is_not_poisoned_by_a_return() {
    // `[ "$N" -eq 3 ]` with `N=3\r` is a syntax error from `[`, not a false comparison,
    // so this catches the case where the `\r` survives into a variable value.
    let out = both_line_endings_agree(
        "numeric",
        "N=3\nif [ \"$N\" -eq 3 ]; then\n    echo three\nelse\n    echo not-three\nfi\n",
    );
    assert_eq!(out.stdout, "three");
    assert_eq!(out.code, 0);
}

// ---------------------------------------------------------------------------
// The other ways source text reaches the parser
// ---------------------------------------------------------------------------

#[test]
fn a_sourced_file_may_have_crlf_independently_of_its_caller() {
    let scratch = Scratch::new("sourced");
    std::fs::write(
        scratch.path().join("lib.sh"),
        b"helper() {\r\n    echo from-lib\r\n}\r\nLIB_LOADED=yes\r\n",
    )
    .expect("write lib");
    std::fs::write(
        scratch.path().join("main.sh"),
        b"source ./lib.sh\nhelper\necho \"loaded=$LIB_LOADED\"\n",
    )
    .expect("write main");

    let out = cash_command()
        .arg("./main.sh")
        .current_dir(scratch.path())
        .output()
        .expect("failed to run cash");

    let stdout = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
    assert_eq!(
        stdout,
        "from-lib\nloaded=yes",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn dash_c_accepts_crlf() {
    // A `-c` string can be assembled from a CRLF file by a caller that never looked at
    // its bytes — a CI system reading a step out of a YAML file, for instance.
    let out = cash_command()
        .args(["-c", "if true; then\r\n    echo dash-c-ok\r\nfi\r\n"])
        .output()
        .expect("failed to run cash");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "dash-c-ok",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn eval_accepts_crlf() {
    let out = cash_command()
        .args([
            "-c",
            r#"eval "$(printf 'if true; then\r\n  echo eval-ok\r\nfi\r\n')""#,
        ])
        .output()
        .expect("failed to run cash");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "eval-ok",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_crlf_script_with_a_bom_works_too() {
    // D41 strips the BOM, D7 handles the endings; a Notepad-saved script has both, and
    // the two accommodations have to compose.
    let scratch = Scratch::new("bom");
    let script = scratch.path().join("s.sh");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"if true; then\r\n    echo bom-crlf-ok\r\nfi\r\n");
    std::fs::write(&script, &bytes).expect("write");

    let out = run(&script);
    assert_eq!(out.stdout, "bom-crlf-ok", "stderr: {}", out.stderr);
    assert_eq!(out.code, 0);
}

#[test]
fn a_script_with_mixed_endings_runs() {
    // Real files end up this way after an editor rewrites only the lines it touched.
    let scratch = Scratch::new("mixed");
    let script = scratch.path().join("s.sh");
    std::fs::write(
        &script,
        b"echo one\r\necho two\nif true; then\r\n    echo three\nfi\r\n",
    )
    .expect("write");

    let out = run(&script);
    assert_eq!(out.stdout, "one\ntwo\nthree", "stderr: {}", out.stderr);
}

#[test]
fn a_syntax_error_in_a_crlf_script_reports_the_real_line() {
    // The other half of the fix: before, *any* CRLF script failed at end of input, which
    // told the user nothing. A genuine error must still point at itself.
    let scratch = Scratch::new("error-position");
    let script = scratch.path().join("s.sh");
    std::fs::write(&script, b"echo one\r\necho two\r\nfor\r\necho four\r\n").expect("write");

    let out = run(&script);
    assert_ne!(out.code, 0, "a broken script exited cleanly");
    assert!(
        out.stderr.contains("line 3") || out.stderr.contains("line 4"),
        "the error did not point near the real line: {}",
        out.stderr
    );
}

#[test]
fn a_lone_carriage_return_in_a_crlf_script_is_still_data() {
    // `printf 'x\r'` — spelled with a real control byte in the source, as a progress
    // indicator would be. Normalising line endings must not eat it.
    let scratch = Scratch::new("lone-cr");
    let script = scratch.path().join("s.sh");
    std::fs::write(&script, b"printf 'a\rb' > out.txt\r\n").expect("write");

    let out = cash_command()
        .arg(script.to_string_lossy().replace('\\', "/"))
        .current_dir(scratch.path())
        .output()
        .expect("failed to run cash");

    let written = std::fs::read(scratch.path().join("out.txt")).expect("output");
    assert_eq!(
        written,
        b"a\rb",
        "a data carriage return was stripped (stderr: {})",
        String::from_utf8_lossy(&out.stderr)
    );
}
