//! Completion on Windows — **D40**, with **D3** and **D16**.
//!
//! Completion is the one part of the shell with no command-line surface, so these drive
//! `Shell::get_completions` directly rather than the binary.
//!
//! D40 asks for three things, each for a Windows-specific reason:
//!
//! - **Case-insensitive**, because the filesystem is (D16) and a case-sensitive match on
//!   a case-insensitive volume can only ever produce a false negative.
//! - **Auto-quoted**, because `C:/Program Files` is the most common path on this platform
//!   and it breaks unquoted every single time.
//! - **Both spellings**, because D3 accepts `/c/...` as input and completion that only
//!   understood `C:/...` would make the accepted spelling unusable in practice.

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

use cash_builtins::ShellBuilderExt as _;
use cash_core::Shell;

struct Fixture {
    shell: Shell,
    dir: PathBuf,
}

impl Fixture {
    async fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cash-complete-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create fixture dir");

        let mut shell = Shell::builder()
            .profile(cash_core::ProfileLoadBehavior::Skip)
            .rc(cash_core::RcLoadBehavior::Skip)
            .default_builtins(cash_builtins::BuiltinSet::BashMode)
            .build()
            .await
            .expect("build shell");

        shell.set_working_dir(&dir).expect("set working dir");

        Self { shell, dir }
    }

    fn path(&self) -> &Path {
        &self.dir
    }

    fn touch(&self, relative: &str) {
        let path = self.dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(&path, b"x").expect("write file");
    }

    fn mkdir(&self, relative: &str) {
        std::fs::create_dir_all(self.dir.join(relative)).expect("create dir");
    }

    /// Complete with the cursor at the end of `input`, as pressing Tab would.
    async fn complete(&mut self, input: &str) -> Vec<String> {
        let position = input.len();
        let completions = self
            .shell
            .complete(input, position)
            .await
            .expect("completion failed");
        completions.candidates
    }

    /// The line a line editor would be left holding after accepting the first candidate.
    async fn completed_line(&mut self, input: &str) -> String {
        self.completed_line_at(input, input.len()).await
    }

    /// Like [`Self::completed_line`], with the cursor at `position`.
    async fn completed_line_at(&mut self, input: &str, position: usize) -> String {
        let completions = self
            .shell
            .complete(input, position)
            .await
            .expect("completion failed");
        let candidate = completions
            .candidates
            .first()
            .expect("no candidate to accept")
            .clone();

        let mut line = String::from(input.get(..completions.insertion_index).unwrap_or(""));
        line.push_str(candidate.trim_end());
        line.push_str(
            input
                .get(completions.insertion_index + completions.delete_count..)
                .unwrap_or(""),
        );
        line
    }

    /// Run `script` in the fixture's own shell, to define functions and compspecs.
    async fn define(&mut self, script: &str) {
        let params = self.shell.default_exec_params();
        self.shell
            .run_string(script, &cash_core::SourceInfo::default(), &params)
            .await
            .expect("run definitions");
    }

    /// Run a command line in the fixture's shell and return its standard output.
    fn run(&self, line: &str) -> String {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_cash"))
            .args(["-c", line])
            .current_dir(&self.dir)
            .output()
            .expect("failed to run cash");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn contains_ending_with(candidates: &[String], suffix: &str) -> bool {
    candidates
        .iter()
        .any(|c| c.trim_end_matches(['/', ' ']).ends_with(suffix))
}

// ---------------------------------------------------------------------------
// Case insensitivity (D40, D16)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_lowercase_prefix_completes_a_capitalised_name() {
    let mut fixture = Fixture::new("case").await;
    fixture.mkdir("Program Files");
    fixture.touch("README.md");

    let candidates = fixture.complete("cat READ").await;
    assert!(
        contains_ending_with(&candidates, "README.md"),
        "exact case failed: {candidates:?}"
    );

    let candidates = fixture.complete("cat read").await;
    assert!(
        contains_ending_with(&candidates, "README.md"),
        "a lowercase prefix did not find README.md: {candidates:?}"
    );
}

#[tokio::test]
async fn an_uppercase_prefix_completes_a_lowercase_name() {
    let mut fixture = Fixture::new("case-upper").await;
    fixture.touch("notes.txt");

    let candidates = fixture.complete("cat NOT").await;
    assert!(
        contains_ending_with(&candidates, "notes.txt"),
        "an uppercase prefix did not find notes.txt: {candidates:?}"
    );
}

// ---------------------------------------------------------------------------
// Auto-quoting (D40)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_completion_containing_a_space_is_quoted() {
    // The headline case. `C:/Program Files` unquoted is two arguments, and the failure
    // is silent: the command runs, against the wrong path. With no quote typed, it comes
    // back in single quotes, as PowerShell completes, a directory with its `/` inside.
    let mut fixture = Fixture::new("quote-space").await;
    fixture.mkdir("Program Files");
    fixture.touch("my file.txt");

    assert_eq!(fixture.complete("ls Prog").await, vec!["'Program Files/'"]);
    assert_eq!(fixture.complete("ls my").await, vec!["'my file.txt'"]);
}

#[tokio::test]
async fn typed_glob_characters_are_completed_as_they_are() {
    // `[draft]` was a bracket expression, so a name starting with it completed nothing
    // (LANG-16). Only `*` and `?` glob, as no Windows name can hold them (spec §4).
    let mut fixture = Fixture::new("glob-chars").await;
    fixture.touch("[draft] notes.txt");
    fixture.touch("(old) list.txt");
    fixture.touch("d.txt");

    for input in ["cat '[dr", "cat [dr", "cat \\[draft\\]"] {
        let candidates = fixture.complete(input).await;
        assert!(
            contains_ending_with(&candidates, "[draft] notes.txt'")
                || contains_ending_with(&candidates, "[draft] notes.txt")
                || contains_ending_with(&candidates, "\\[draft\\]\\ notes.txt"),
            "{input}: {candidates:?}"
        );
    }
    assert!(contains_ending_with(
        &fixture.complete("cat '(ol").await,
        "(old) list.txt'"
    ));
    assert!(contains_ending_with(
        &fixture.complete("cat ?.tx").await,
        "d.txt"
    ));
}

#[tokio::test]
async fn completing_before_a_closing_quote_replaces_it() {
    // After `'my dir/'` the cursor sits before the closing quote; typing and Tab there
    // must not leave the old quote behind (`'my dir/inner/''`).
    let mut fixture = Fixture::new("quote-inside").await;
    fixture.mkdir("my dir/inner");

    let input = "cd 'my dir/in'";
    let line = fixture.completed_line_at(input, input.len() - 1).await;
    assert_eq!(line, "cd 'my dir/inner/'");
}

#[tokio::test]
async fn fullquote_quotes_a_functions_completions_and_noquote_still_wins() {
    // Bash 5.3's `compopt -o fullquote` quotes completions that are not file names as if
    // they were; without it, or with `noquote`, they go in as they are.
    let mut fixture = Fixture::new("fullquote").await;
    fixture
        .define(concat!(
            "f() { compopt -o fullquote; COMPREPLY=('x y' plain); }; complete -F f fq\n",
            "g() { COMPREPLY=('x y'); }; complete -F g plain\n",
            "h() { compopt -o fullquote -o noquote; COMPREPLY=('x y'); }; complete -F h nq\n",
        ))
        .await;

    let mut quoted = fixture.complete("fq ").await;
    quoted.sort();
    assert_eq!(quoted, vec!["'x y'", "plain"]);
    assert_eq!(fixture.complete("plain ").await, vec!["x y"]);
    assert_eq!(fixture.complete("nq ").await, vec!["x y"]);
}

#[tokio::test]
async fn a_name_holding_a_single_quote_gets_double_quotes() {
    let mut fixture = Fixture::new("quote-apostrophe").await;
    fixture.touch("it's here.txt");
    fixture.touch("it's $5.txt");

    let mut candidates = fixture.complete("ls it").await;
    candidates.sort();
    // Inside double quotes `$` still expands, so it is escaped.
    assert_eq!(candidates, vec![r#""it's \$5.txt""#, r#""it's here.txt""#]);
}

#[tokio::test]
async fn a_backslash_escape_already_typed_continues_as_backslashes() {
    // As Bash completes: the user chose escaping, so the rest of the word is escaped.
    let mut fixture = Fixture::new("quote-backslash").await;
    fixture.mkdir("my dir");
    fixture.touch("my dir/a (b).txt");

    assert_eq!(fixture.complete(r"ls my\ d").await, vec![r"my\ dir/"]);
    assert_eq!(
        fixture.complete(r"ls my\ dir/a\ ").await,
        vec![r"my\ dir/a\ \(b\).txt"]
    );
    // A drive path's backslash is a separator, not an escape: no style is started.
    let windows = fixture.path().to_string_lossy().replace('/', "\\");
    let candidates = fixture.complete(&format!(r"ls {windows}\my")).await;
    assert!(
        candidates.iter().all(|c| c.starts_with('\'')),
        "a drive path's backslash was taken for an escape: {candidates:?}"
    );
}

#[tokio::test]
async fn a_completion_without_a_space_is_not_quoted() {
    // Quoting everything would be safe and unreadable. Only quote what needs it.
    let mut fixture = Fixture::new("quote-plain").await;
    fixture.touch("plain.txt");

    let candidates = fixture.complete("cat plai").await;
    assert!(!candidates.is_empty(), "nothing completed");
    for candidate in &candidates {
        assert!(
            !candidate.starts_with('"') && !candidate.starts_with('\''),
            "an ordinary name was needlessly quoted: {candidate:?}"
        );
    }
}

#[tokio::test]
async fn completing_inside_an_open_quote_keeps_exactly_one_quote() {
    // D40's open question, resolved by the shape of the replacement: the span being
    // replaced *includes* the opening quote the user typed. So the candidate has to
    // carry the quote — dropping it would silently delete what they typed, and adding a
    // second would produce `""Program Files"`.
    let mut fixture = Fixture::new("quote-open").await;
    fixture.mkdir("Program Files");

    let candidates = fixture.complete(r#"ls "Prog"#).await;
    assert!(!candidates.is_empty(), "nothing completed inside a quote");
    for candidate in &candidates {
        assert!(
            candidate.starts_with('"') && candidate.ends_with('"'),
            "the typed quote was not carried through: {candidate:?}"
        );
        assert!(
            !candidate.starts_with(r#"\"\""#),
            "a second quote was added: {candidate:?}"
        );
    }
}

#[tokio::test]
async fn a_single_quote_already_typed_is_honoured() {
    let mut fixture = Fixture::new("quote-single").await;
    fixture.mkdir("Program Files");

    let candidates = fixture.complete("ls 'Prog").await;
    assert!(!candidates.is_empty(), "nothing completed inside a quote");
    for candidate in &candidates {
        assert!(
            candidate.starts_with('\'') && candidate.ends_with('\''),
            "the single quote was not carried through: {candidate:?}"
        );
    }
}

#[tokio::test]
async fn a_quote_the_user_typed_survives_even_when_unnecessary() {
    // Someone who typed a quote meant it; silently removing it changes their line.
    let mut fixture = Fixture::new("quote-kept").await;
    fixture.touch("plain.txt");

    let candidates = fixture.complete(r#"ls "plai"#).await;
    assert_eq!(candidates, vec![r#""plain.txt""#.to_string()]);
}

#[tokio::test]
async fn the_quoted_candidate_round_trips_through_the_shell() {
    // The real test of quoting is whether the resulting line runs. Build it the way a
    // line editor would — replace the completed span with the candidate — and execute it.
    let mut fixture = Fixture::new("quote-roundtrip").await;
    fixture.mkdir("Program Files");
    fixture.touch("Program Files/inside.txt");

    // A directory's candidate ends in `/`, the cursor left before its closing quote; the
    // path typed after the quote still joins the word.
    let line = fixture.completed_line("ls Prog").await;
    let output = fixture.run(&format!("{line}inside.txt"));
    assert_eq!(
        output.trim(),
        "Program Files/inside.txt",
        "the completed line did not run: {line:?}"
    );
}

#[tokio::test]
async fn a_completion_containing_other_shell_metacharacters_is_quoted() {
    // Spaces are the common case, not the only one. `$`, `&`, `;`, `(` and `)` all
    // appear in real Windows paths — `Program Files (x86)` most of all.
    let mut fixture = Fixture::new("quote-meta").await;
    fixture.mkdir("Program Files (x86)");

    let candidates = fixture.complete("ls Prog").await;
    assert!(!candidates.is_empty(), "nothing completed");
    for candidate in &candidates {
        assert!(
            candidate.starts_with('\''),
            "parentheses were left unquoted: {candidate:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Both spellings (D40, D3)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_absolute_drive_path_completes() {
    let mut fixture = Fixture::new("abs-drive").await;
    fixture.mkdir("sub/deeper");

    let prefix = format!(
        "ls {}/su",
        fixture.path().to_string_lossy().replace('\\', "/")
    );
    let candidates = fixture.complete(&prefix).await;
    assert!(
        contains_ending_with(&candidates, "sub"),
        "an absolute C:/ path did not complete: {candidates:?}"
    );
}

#[tokio::test]
async fn a_unix_spelled_drive_path_completes() {
    // D3 accepts `/c/Users/...` as input, so completion has to understand it too —
    // otherwise the accepted spelling is only usable if you type every character.
    let mut fixture = Fixture::new("abs-unix").await;
    fixture.mkdir("sub");

    let windows = fixture.path().to_string_lossy().replace('\\', "/");
    let unix = cash_win32_unix_spelling(&windows);

    let candidates = fixture.complete(&format!("ls {unix}/su")).await;
    assert!(
        contains_ending_with(&candidates, "sub"),
        "the Unix spelling {unix}/su did not complete: {candidates:?}"
    );
}

/// `C:/x` -> `/c/x`, without depending on `cash-win32` from this crate.
fn cash_win32_unix_spelling(windows: &str) -> String {
    let bytes = windows.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let letter = (bytes[0] as char).to_ascii_lowercase();
        let rest = windows.get(2..).unwrap_or_default();
        format!("/{letter}{rest}")
    } else {
        windows.to_string()
    }
}

// ---------------------------------------------------------------------------
// Ordinary behaviour that must survive the Windows accommodations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_directory_completes_by_name() {
    // Deliberately no trailing separator: the absorbed suite asserts bare names, and
    // D40 does not ask for one. Appending `/` is readline's job, not the shell's.
    let mut fixture = Fixture::new("dir-sep").await;
    fixture.mkdir("subdir");

    let candidates = fixture.complete("ls subd").await;
    assert_eq!(candidates, vec!["subdir".to_string()]);
}

#[tokio::test]
async fn globstar_completion_obeys_shopt_and_skips_hidden_directories() {
    let mut fixture = Fixture::new("globstar").await;
    fixture.touch("one/needle.txt");
    fixture.touch("one/two/needle.txt");
    fixture.touch(".hidden/deep/needle.txt");

    let disabled = fixture.complete("cat **/nee").await;
    assert_eq!(disabled, vec!["one/needle.txt".to_string()]);

    fixture.shell.options_mut().enable_star_star_glob = true;
    let enabled = fixture.complete("cat **/nee").await;
    assert_eq!(
        enabled,
        vec![
            "one/needle.txt".to_string(),
            "one/two/needle.txt".to_string()
        ]
    );
}

#[tokio::test]
async fn a_command_in_first_position_completes_from_builtins() {
    let mut fixture = Fixture::new("command").await;

    let candidates = fixture.complete("expor").await;
    assert!(
        candidates.iter().any(|c| c.trim_end() == "export"),
        "a builtin did not complete in command position: {candidates:?}"
    );
}

#[tokio::test]
async fn a_variable_completes() {
    let mut fixture = Fixture::new("variable").await;

    let candidates = fixture.complete("echo $PAT").await;
    assert!(
        candidates.iter().any(|c| c.contains("PATH")),
        "a variable did not complete: {candidates:?}"
    );
}

#[tokio::test]
async fn nothing_matching_completes_to_nothing() {
    let mut fixture = Fixture::new("empty").await;
    fixture.touch("a.txt");

    let candidates = fixture.complete("cat zzzz").await;
    assert!(
        candidates.is_empty(),
        "invented a candidate: {candidates:?}"
    );
}
