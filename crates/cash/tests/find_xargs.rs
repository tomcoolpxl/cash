//! `find` and `xargs` — **D48**, **D3**, **D32**.
//!
//! On a clean Windows machine `find` is `C:\Windows\System32\find.exe`, a DOS tool that
//! searches for text *inside* files, and there is no `xargs` at all. `find | xargs grep`
//! is in cash's own acceptance corpus, so the pair had to be carried rather than left to
//! whatever answers the name.
//!
//! Every expectation here was read off GNU find first. The two that are deliberately not
//! GNU's are called out where they are asserted: matching is case-insensitive (D16), and
//! paths are rendered with forward slashes (D3) — which is also what makes the pipeline
//! safe, since a backslash is an escape to `xargs`.

#![cfg(windows)]
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

/// A small tree to search, rebuilt per test so the runs do not see each other.
///
/// ```text
/// a.txt          hello
/// readme.md      # hi
/// zero.txt       (empty)
/// has space.txt  spaced
/// sub/b.txt      x
/// sub/deep/c.txt deep
/// skip/d.txt     no
/// empty/         (empty directory)
/// ```
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("cash-find-{name}"));
        let _ = std::fs::remove_dir_all(&root);

        for dir in ["sub/deep", "skip", "empty"] {
            std::fs::create_dir_all(root.join(dir)).expect("failed to create the sandbox");
        }
        for (path, contents) in [
            ("a.txt", "hello\n"),
            ("readme.md", "# hi\n"),
            ("zero.txt", ""),
            ("has space.txt", "spaced\n"),
            ("sub/b.txt", "x\n"),
            ("sub/deep/c.txt", "deep\n"),
            ("skip/d.txt", "no\n"),
        ] {
            std::fs::write(root.join(path), contents).expect("failed to write a sandbox file");
        }

        Self { root }
    }

    fn run(&self, script: &str) -> Output {
        let out = Command::new(CASH)
            .current_dir(&self.root)
            .args(["-c", script])
            .output()
            .expect("failed to run cash");
        Output {
            stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
            code: out.status.code().unwrap_or(-1),
        }
    }

    fn exists(&self, path: &str) -> bool {
        Path::new(&self.root).join(path).exists()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The lines of a listing, sorted, so a test says what it means rather than depending on
/// directory order.
fn lines(stdout: &str) -> Vec<String> {
    let mut found: Vec<String> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect();
    found.sort();
    found
}

// ---------------------------------------------------------------------------
// It is ours
// ---------------------------------------------------------------------------

#[test]
fn both_are_builtins() {
    // If `find` resolves to a file, DOS's has won and `find . -name x` answers
    // "FIND: Parameter format not correct".
    let sandbox = Sandbox::new("builtin");
    let out = sandbox.run("type find; type xargs");
    assert_eq!(
        out.stdout.matches("shell builtin").count(),
        2,
        "not both builtins: {}",
        out.stdout
    );
}

// ---------------------------------------------------------------------------
// Walking
// ---------------------------------------------------------------------------

#[test]
fn the_default_listing_is_the_whole_tree() {
    let sandbox = Sandbox::new("default");
    let out = sandbox.run("find .");
    assert_eq!(
        lines(&out.stdout),
        vec![
            ".",
            "./a.txt",
            "./empty",
            "./has space.txt",
            "./readme.md",
            "./skip",
            "./skip/d.txt",
            "./sub",
            "./sub/b.txt",
            "./sub/deep",
            "./sub/deep/c.txt",
            "./zero.txt",
        ],
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn paths_are_rendered_with_forward_slashes() {
    // D3, and the reason `find | xargs` is safe: a backslash is an escape to `xargs`, so
    // a Windows-spelled path would arrive with its separators eaten (§4's near-miss).
    // Quoted, because an unquoted `.\sub` is the shell escaping `s` — bash reads it the
    // same way, and what `find` receives is `.sub`.
    let sandbox = Sandbox::new("render");
    let out = sandbox.run(r#"find '.\sub'"#);
    assert!(
        !out.stdout.contains('\\'),
        "a backslash survived into the listing: {}",
        out.stdout
    );
    assert_eq!(
        lines(&out.stdout),
        vec!["./sub", "./sub/b.txt", "./sub/deep", "./sub/deep/c.txt"],
        "a Windows-spelled start path was not rendered: {}",
        out.stderr
    );
}

#[test]
fn it_walks_the_shells_working_directory() {
    // `cd` moves the shell's own working directory without moving the process (D3/D10).
    // Reading `.` from the process's directory made `cd sub; find .` walk the tree above.
    let sandbox = Sandbox::new("cwd");
    let out = sandbox.run("cd sub; find .");
    assert_eq!(
        lines(&out.stdout),
        vec![".", "./b.txt", "./deep", "./deep/c.txt"],
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn maxdepth_and_mindepth_bound_the_walk() {
    let sandbox = Sandbox::new("depth");
    let shallow = sandbox.run("find . -maxdepth 1 -type d");
    assert_eq!(
        lines(&shallow.stdout),
        vec![".", "./empty", "./skip", "./sub"],
        "stderr: {}",
        shallow.stderr
    );

    let deeper = sandbox.run("find . -mindepth 2 -maxdepth 2 -type f");
    assert_eq!(
        lines(&deeper.stdout),
        vec!["./skip/d.txt", "./sub/b.txt"],
        "stderr: {}",
        deeper.stderr
    );
}

// ---------------------------------------------------------------------------
// Predicates
// ---------------------------------------------------------------------------

#[test]
fn name_matches_a_glob() {
    let sandbox = Sandbox::new("name");
    let out = sandbox.run(r#"find . -name "*.txt""#);
    assert_eq!(
        lines(&out.stdout),
        vec![
            "./a.txt",
            "./has space.txt",
            "./skip/d.txt",
            "./sub/b.txt",
            "./sub/deep/c.txt",
            "./zero.txt",
        ],
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn matching_is_case_insensitive() {
    // Deliberately not GNU's behaviour, and for D16's reason: on a case-insensitive
    // volume a case-sensitive match can only produce false negatives, and `-name '*.TXT'`
    // finding nothing on a disk full of `.txt` is exactly that.
    let sandbox = Sandbox::new("case");
    let out = sandbox.run(r#"find . -name "*.TXT" | wc -l"#);
    assert_eq!(out.stdout.trim(), "6", "stderr: {}", out.stderr);
}

#[test]
fn type_selects_files_or_directories() {
    let sandbox = Sandbox::new("type");
    let files = sandbox.run("find . -type f | wc -l");
    let dirs = sandbox.run("find . -type d | wc -l");
    assert_eq!(files.stdout.trim(), "7", "stderr: {}", files.stderr);
    assert_eq!(dirs.stdout.trim(), "5", "stderr: {}", dirs.stderr);
}

#[test]
fn not_or_and_parentheses_compose() {
    let sandbox = Sandbox::new("logic");
    let negated = sandbox.run(r#"find . -type f ! -name "*.txt""#);
    assert_eq!(
        lines(&negated.stdout),
        vec!["./readme.md"],
        "stderr: {}",
        negated.stderr
    );

    let either = sandbox.run(r#"find . -maxdepth 1 \( -name "*.md" -o -name "zero*" \)"#);
    assert_eq!(
        lines(&either.stdout),
        vec!["./readme.md", "./zero.txt"],
        "stderr: {}",
        either.stderr
    );
}

#[test]
fn prune_skips_a_subtree() {
    let sandbox = Sandbox::new("prune");
    let out = sandbox.run(r#"find . -name skip -prune -o -type f -print"#);
    assert_eq!(
        lines(&out.stdout),
        vec![
            "./a.txt",
            "./has space.txt",
            "./readme.md",
            "./sub/b.txt",
            "./sub/deep/c.txt",
            "./zero.txt",
        ],
        "the pruned subtree leaked: {}",
        out.stderr
    );
}

#[test]
fn empty_finds_empty_files_and_directories() {
    let sandbox = Sandbox::new("empty");
    let out = sandbox.run("find . -empty");
    assert_eq!(
        lines(&out.stdout),
        vec!["./empty", "./zero.txt"],
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn size_filters_by_bytes() {
    let sandbox = Sandbox::new("size");
    let out = sandbox.run(r#"find . -type f -size -1c"#);
    assert_eq!(
        lines(&out.stdout),
        vec!["./zero.txt"],
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn an_unknown_predicate_is_refused_by_name() {
    // A `find` that quietly ignored `-regex` would return the wrong file list and nothing
    // would say so — the failure mode this project rejects everywhere.
    let sandbox = Sandbox::new("unknown");
    let out = sandbox.run(r#"find . -regex ".*""#);
    assert_eq!(out.code, 1, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("-regex"),
        "the diagnostic does not name the predicate: {}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

#[test]
fn exec_runs_once_per_match() {
    let sandbox = Sandbox::new("exec");
    let out = sandbox.run(r#"find . -name "b.txt" -exec echo FOUND {} \;"#);
    assert_eq!(
        out.stdout.trim(),
        "FOUND ./sub/b.txt",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn exec_plus_batches_the_matches() {
    let sandbox = Sandbox::new("execplus");
    let out = sandbox.run(r#"find . -maxdepth 1 -name "*.md" -exec echo BATCH {} +"#);
    assert_eq!(
        out.stdout.trim(),
        "BATCH ./readme.md",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn delete_removes_what_it_matched() {
    let sandbox = Sandbox::new("delete");
    let out = sandbox.run(r#"find . -name "zero.txt" -delete; echo "code: $?""#);
    assert_eq!(out.stdout.trim(), "code: 0", "stderr: {}", out.stderr);
    assert!(!sandbox.exists("zero.txt"), "the file is still there");
    assert!(sandbox.exists("a.txt"), "it deleted more than it matched");
}

// ---------------------------------------------------------------------------
// xargs
// ---------------------------------------------------------------------------

#[test]
fn xargs_batches_its_input_into_one_command() {
    let sandbox = Sandbox::new("xargs-batch");
    let out = sandbox.run(r#"printf '%s\n' a b c | xargs echo"#);
    assert_eq!(out.stdout.trim(), "a b c", "stderr: {}", out.stderr);
}

#[test]
fn n_limits_the_arguments_per_run() {
    let sandbox = Sandbox::new("xargs-n");
    let out = sandbox.run(r#"printf '%s\n' a b c | xargs -n 1 echo"#);
    assert_eq!(
        lines(&out.stdout),
        vec!["a", "b", "c"],
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn print0_and_null_carry_a_name_with_a_space() {
    // The pairing that exists precisely for names like this one, which Windows has more
    // of than Linux does.
    let sandbox = Sandbox::new("xargs-null");
    let out = sandbox.run(r#"find . -name "has space.txt" -print0 | xargs -0 -n 1 echo ITEM"#);
    assert_eq!(
        out.stdout.trim(),
        "ITEM ./has space.txt",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn replace_substitutes_each_item() {
    let sandbox = Sandbox::new("xargs-i");
    let out = sandbox.run(r#"printf '%s\n' one two | xargs -I @ echo "[@]""#);
    assert_eq!(
        lines(&out.stdout),
        vec!["[one]", "[two]"],
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn replace_mode_treats_each_line_as_one_item() {
    let sandbox = Sandbox::new("xargs-i-lines");
    let out = sandbox
        .run(r#"printf '  one two  \r\n\r\nthree four\r\n' | xargs -I @ printf '<%s>\n' '@'"#);
    assert_eq!(
        lines(&out.stdout),
        vec!["<one two  >", "<three four>"],
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn r_declines_to_run_on_empty_input() {
    let sandbox = Sandbox::new("xargs-r");
    let with_r = sandbox.run(r#"printf '' | xargs -r echo EMPTY; echo "code: $?""#);
    assert_eq!(with_r.stdout.trim(), "code: 0", "stderr: {}", with_r.stderr);
    assert!(
        !with_r.stdout.contains("EMPTY"),
        "-r still ran the command: {}",
        with_r.stdout
    );

    // Without `-r`, GNU runs the command once with no arguments.
    let without = sandbox.run(r#"printf '' | xargs echo EMPTY"#);
    assert_eq!(without.stdout.trim(), "EMPTY", "stderr: {}", without.stderr);
}

#[test]
fn a_failing_command_is_reported_in_the_exit_code() {
    // GNU's 123 for "a command exited non-zero", which is what a script testing `xargs`
    // is looking for.
    let sandbox = Sandbox::new("xargs-fail");
    let out = sandbox.run(r#"printf '%s\n' x | xargs cash -c 'exit 3'; echo "code: $?""#);
    assert_eq!(out.stdout.trim(), "code: 123", "stderr: {}", out.stderr);
}

#[test]
fn xargs_runs_in_the_shells_working_directory() {
    let sandbox = Sandbox::new("xargs-cwd");
    let out = sandbox.run(r#"cd sub; find . -name "b.txt" | xargs -n 1 echo GOT"#);
    assert_eq!(out.stdout.trim(), "GOT ./b.txt", "stderr: {}", out.stderr);
}
