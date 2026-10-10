//! `z`, the folder jump (spec D80), run as a script on a record of folders written for
//! the test, in a `%LOCALAPPDATA%` of its own; and Tab after `z`, driven through
//! `Shell::complete`. The record an interactive cash keeps as it goes is in
//! `conpty_interactive_tests.rs`.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::fmt::Write as _;
use std::process::Stdio;

use cash_builtins::ShellBuilderExt as _;

use crate::common::{Scratch, cash_command};

/// A scratch folder holding `local`, the `%LOCALAPPDATA%` whose `cash\folders` is the
/// record, and the folders it names.
struct Record {
    dir: Scratch,
}

impl Record {
    /// Folders made under the scratch folder, each with its rank and the seconds since
    /// its last visit; a name starting with `gone/` is recorded but not made.
    fn new(name: &str, folders: &[(&str, f64, i64)]) -> Self {
        let dir = Scratch::new(&format!("z-{name}"));
        let now = cash_core::kept::now();
        let mut text = String::new();
        for (folder, rank, ago) in folders {
            let path = format!("{}/{folder}", dir.as_script_path());
            if !folder.starts_with("gone/") {
                std::fs::create_dir_all(&path).expect("create folder");
            }
            let _ = writeln!(text, "{rank}\t{}\t{path}", now - ago);
        }
        std::fs::create_dir_all(dir.join("local/cash")).expect("create the record's folder");
        std::fs::write(dir.join("local/cash/folders"), text).expect("write the record");
        Self { dir }
    }

    fn path(&self, folder: &str) -> String {
        format!("{}/{folder}", self.dir.as_script_path())
    }

    fn local(&self) -> String {
        self.path("local")
    }

    /// Runs `script` in the scratch folder with the record kept: standard output, standard
    /// error and the status.
    fn run(&self, script: &str) -> (String, String, i32) {
        let out = cash_command()
            .args(["-c", script])
            .current_dir(self.dir.path())
            .env("LOCALAPPDATA", self.local())
            .env("CASH_NO_RECORDS", "")
            .env("HOME", self.path("home"))
            .stdin(Stdio::null())
            .output()
            .expect("run cash");
        (
            String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
            String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
            out.status.code().unwrap_or(-1),
        )
    }

    /// The record as it is now: its folders, below the scratch folder.
    fn folders(&self) -> Vec<String> {
        let text = std::fs::read_to_string(self.dir.join("local/cash/folders")).unwrap();
        let prefix = format!("{}/", self.dir.as_script_path());
        text.lines()
            .filter_map(|line| line.split('\t').nth(2))
            .map(|path| path.strip_prefix(&prefix).unwrap_or(path).to_owned())
            .collect()
    }
}

const HOUR: i64 = 3_600;
const WEEK: i64 = 7 * 86_400;

#[test]
fn z_goes_to_the_best_ranked_match_as_zoxide_ranks_it() {
    // `old/proj` has the most visits, but its last was over a week ago (×¼): 10 × ¼ = 2.5
    // against `new/proj`'s 1 × 4 within the hour.
    let record = Record::new(
        "rank",
        &[
            ("old/proj", 10.0, 2 * WEEK),
            ("new/proj", 1.0, 60),
            ("home", 1.0, 60),
        ],
    );
    let (out, err, code) = record.run("z proj && pwd; z old proj && pwd");
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        out,
        format!("{}\n{}\n", record.path("new/proj"), record.path("old/proj"))
    );
}

#[test]
fn the_last_word_must_be_in_the_last_part_and_the_current_folder_is_left_out() {
    let record = Record::new(
        "words",
        &[
            ("src/cash/crates", 9.0, HOUR),
            ("src/cash", 1.0, HOUR),
            ("other/cash", 5.0, HOUR),
            ("home", 1.0, 60),
        ],
    );
    // `cash` is not in `crates`, the last part of the best ranked; from `other/cash`, the
    // next best is the other `cash`.
    let (out, err, code) = record.run("z cash && pwd; z cash && pwd; z CASH && pwd");
    assert_eq!(code, 0, "{err}");
    let (other, src) = (record.path("other/cash"), record.path("src/cash"));
    assert_eq!(out, format!("{other}\n{src}\n{other}\n"));
}

#[test]
fn z_lists_the_matches_with_their_scores_best_first() {
    let record = Record::new(
        "list",
        &[
            ("a/proj", 2.0, 60),
            ("b/proj", 3.0, 2 * HOUR),
            ("c/other", 1.0, 60),
        ],
    );
    let (out, err, code) = record.run("z -l proj");
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        out,
        format!(
            "   8.0 {}\n   6.0 {}\n",
            record.path("a/proj"),
            record.path("b/proj")
        )
    );
    let (out, _, code) = record.run("z -l");
    assert_eq!(code, 0);
    assert_eq!(out.lines().count(), 3, "{out}");
}

#[test]
fn z_alone_a_dash_or_a_folder_is_cds() {
    let record = Record::new(
        "cd",
        &[("home", 1.0, 60), ("proj", 1.0, 60), ("x", 1.0, 60)],
    );
    // `x` is a folder here, so `z x` is `cd x`; `z -` prints the folder, as `cd -` does.
    let (out, err, code) = record.run("z x && pwd; z && pwd; z - ; z .. && pwd");
    assert_eq!(code, 0, "{err}");
    let root = record.dir.as_script_path();
    assert_eq!(
        out,
        format!(
            "{}\n{}\n{}\n{}\n",
            record.path("x"),
            record.path("home"),
            record.path("x"),
            root
        )
    );
}

#[test]
fn a_folder_that_is_gone_is_skipped_and_forgotten_after_ninety_days() {
    let record = Record::new(
        "gone",
        &[
            ("gone/lately/proj", 50.0, 60),
            ("gone/long-ago/proj", 50.0, 91 * 86_400),
            ("here/proj", 1.0, 60),
        ],
    );
    let (out, err, code) = record.run("z proj && pwd");
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, format!("{}\n", record.path("here/proj")));
    assert_eq!(record.folders(), ["gone/lately/proj", "here/proj"]);
}

#[test]
fn no_match_no_record_and_a_wrong_option_say_so() {
    let record = Record::new("none", &[("proj", 1.0, 60)]);
    let (out, err, code) = record.run("z nothing-like-it");
    assert_eq!((out.as_str(), code), ("", 1));
    assert_eq!(err, "z: no folder matches 'nothing-like-it'\n");

    let (_, err, code) = record.run("z -q proj");
    assert_eq!(code, 2);
    assert!(err.starts_with("z: -q: unknown option\n"), "{err}");

    let out = cash_command()
        .args(["-c", "z proj"])
        .env("LOCALAPPDATA", record.local())
        .output()
        .expect("run cash");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("CASH_NO_RECORDS is set"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_z_function_comes_before_the_builtin() {
    let record = Record::new("function", &[("proj", 1.0, 60)]);
    let (out, _, code) = record.run("z() { echo mine \"$@\"; }; z proj");
    assert_eq!((out.as_str(), code), ("mine proj\n", 0));
}

/// The candidates Tab offers at the end of `line`, in a shell started in the record's
/// scratch folder with the record kept, and the length of the line they replace.
async fn complete(record: &Record, line: &str) -> (Vec<String>, usize) {
    let mut shell = cash_core::Shell::builder()
        .profile(cash_core::ProfileLoadBehavior::Skip)
        .rc(cash_core::RcLoadBehavior::Skip)
        .default_builtins(cash_builtins::BuiltinSet::BashMode)
        .build()
        .await
        .expect("build shell");
    shell.set_working_dir(record.dir.path()).unwrap();
    let params = shell.default_exec_params();
    let setup = format!("LOCALAPPDATA='{}'; CASH_NO_RECORDS=", record.local());
    shell
        .run_string(&setup, &cash_core::SourceInfo::default(), &params)
        .await
        .expect("set the record up");
    let completions = shell.complete(line, line.len()).await.expect("complete");
    (completions.candidates, completions.delete_count)
}

#[tokio::test]
async fn tab_after_z_offers_the_matching_folders_best_first() {
    // The words are ones the scratch folder's own path cannot hold.
    let record = Record::new(
        "tab",
        &[
            ("alpha/proj", 1.0, 60),
            ("beta/proj", 5.0, 60),
            ("gamma/other", 9.0, 60),
            ("my dir/proj two", 3.0, 60),
        ],
    );
    let (candidates, replaced) = complete(&record, "z beta proj").await;
    assert_eq!(candidates, [record.path("beta/proj")]);
    assert_eq!(replaced, "beta proj".len(), "every word typed is replaced");

    let (candidates, _) = complete(&record, "z proj").await;
    let my_dir = format!("'{}/'", record.path("my dir/proj two"));
    assert_eq!(
        candidates,
        [record.path("beta/proj"), my_dir, record.path("alpha/proj")]
    );

    // A path being typed completes as any path does.
    let (candidates, _) = complete(&record, "z ./gamma/oth").await;
    assert!(
        !candidates.is_empty() && candidates.iter().all(|c| c.starts_with("./gamma/oth")),
        "{candidates:?}"
    );
}
