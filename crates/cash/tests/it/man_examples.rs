//! Every example of every page written in full runs, and gives what Git Bash 5.3 with
//! the GNU tools gave for it: its standard output, its status, and the files it changed.
//!
//! Writing the pages' examples found three `cd` bugs and two stale claims, so each
//! example is a check (`research/man-browser-design.md`, "Every example is tested").
//! What Git Bash gave is frozen beside the examples' files, `tests/man-examples`, as the
//! corpus's oracle output is: `CASH_MAN_EXAMPLES_FREEZE=1` runs the examples under Git
//! Bash and writes it, for a page written in full or an example changed. A page of
//! cash's own commands (`man`, `z`) has no oracle: its examples run under cash with
//! status 0. A block marked parse-only is parsed (`cash -n`), not run.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a \
              failed set-up should abort it loudly, naming the file it could not read"
)]

use std::fmt::Write as _;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::common::{Scratch, cash_command};

/// The oracle: Git for Windows' Bash, with GNU sed and the rest.
const GIT_BASH: &str = "C:/Program Files/Git/bin/bash.exe";

/// The kind of a page whose command is cash's own, with no oracle to compare with.
const OWN_KIND: &str = "cash's shell commands";

/// Examples whose oracle output cash does not give, and why. They run too: one that
/// comes to match fails the test until it is taken off the list.
const KNOWN_DIFFERENT: &[(&str, &str)] = &[];

/// How long an example may run.
const TIMEOUT: Duration = Duration::from_secs(10);

#[test]
fn every_example_of_a_page_written_in_full_gives_git_bash_s_output() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/man-examples");
    let fixture = dir.join("fixture");
    let freeze = std::env::var_os("CASH_MAN_EXAMPLES_FREEZE").is_some();
    let mut failures = Vec::new();
    let mut ran = 0;
    let mut known = 0;
    for page in cash_man::manual().pages.iter().filter(|page| !page.draft) {
        for example in page.examples() {
            let name = format!("{}-{}", page.file, example.number);
            if example.parse_only.is_some() {
                let (_, status) = run(
                    cash_command().args(["--noprofile", "--norc", "-n", "-c", &example.script]),
                    &fixture,
                );
                if status != "0" {
                    failures.push(format!("{name}: does not parse (status {status})"));
                }
                continue;
            }
            ran += 1;
            let cash = record(
                cash_command().args(["--noprofile", "--norc", "-c", &example.script]),
                &fixture,
            );
            if page.kind == OWN_KIND {
                if !cash.contains("--- status 0\n") {
                    failures.push(format!("{name}: cash's own command failed\n{cash}"));
                }
                continue;
            }
            let golden = dir.join(format!("{name}.out"));
            if freeze {
                let bash = record(
                    Command::new(GIT_BASH)
                        .args([
                            "--noprofile",
                            "--norc",
                            "-c",
                            "s=$CORPUS_SCRIPT; unset CORPUS_SCRIPT; exec \"$BASH\" --noprofile --norc -c \"$s\"",
                        ])
                        .env("CORPUS_SCRIPT", &example.script),
                    &fixture,
                );
                std::fs::write(&golden, bash).expect("write what Git Bash gave");
            }
            let expected = std::fs::read_to_string(&golden).unwrap_or_default();
            let why = KNOWN_DIFFERENT
                .iter()
                .find(|(listed, _)| *listed == name)
                .map(|(_, why)| *why);
            known += usize::from(why.is_some());
            match (expected.is_empty(), cash == expected, why) {
                (true, _, _) => failures.push(format!(
                    "{name}: nothing frozen; run the test with CASH_MAN_EXAMPLES_FREEZE=1"
                )),
                (false, false, None) => failures.push(format!(
                    "{name}:\n{}\n--- cash\n{cash}--- Git Bash\n{expected}",
                    example.script
                )),
                (false, true, Some(why)) => failures.push(format!(
                    "{name}: matches Git Bash now; take it off KNOWN_DIFFERENT (\"{why}\")"
                )),
                _ => {}
            }
        }
    }
    assert!(ran >= 10, "only {ran} examples ran");
    assert_eq!(
        known,
        KNOWN_DIFFERENT.len(),
        "an example in KNOWN_DIFFERENT is not there"
    );
    assert!(
        failures.is_empty(),
        "{} of {ran} examples are not as Git Bash gave them:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// What an example did, run in a copy of the fixture: its standard output (CRLF read
/// as LF, as the corpus reads it), its status, and each file it made, changed or
/// removed, its carriage returns shown as `\r`.
fn record(command: &mut Command, fixture: &Path) -> String {
    let scratch = Scratch::new("man-example");
    let before = files_below(fixture);
    for (name, bytes) in &before {
        let path = scratch.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("make a fixture's directory");
        }
        std::fs::write(path, bytes).expect("copy a fixture");
    }
    let (stdout, status) = run(command, scratch.path());
    let mut out = format!("{stdout}--- status {status}\n");
    let after = files_below(scratch.path());
    for (name, bytes) in &after {
        if before.iter().all(|(was, old)| was != name || old != bytes) {
            let text = String::from_utf8_lossy(bytes).replace('\r', "\\r");
            let _ = write!(out, "--- file {name}\n{text}");
            if !text.ends_with('\n') {
                out.push('\n');
            }
        }
    }
    for (name, _) in &before {
        if after.iter().all(|(now, _)| now != name) {
            let _ = writeln!(out, "--- gone {name}");
        }
    }
    out
}

/// Every file in `dir` and below, by its path from `dir` with `/` between the parts,
/// and what it holds, in the order of the paths.
fn files_below(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![(dir.to_path_buf(), String::new())];
    while let Some((at, prefix)) = pending.pop() {
        for entry in std::fs::read_dir(&at).expect("read a directory").flatten() {
            let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
            if entry.path().is_dir() {
                pending.push((entry.path(), format!("{name}/")));
            } else {
                files.push((name, std::fs::read(entry.path()).unwrap_or_default()));
            }
        }
    }
    files.sort();
    files
}

/// Runs `command` in `dir` with no standard input: its standard output and status, or
/// `timeout`.
fn run(command: &mut Command, dir: &Path) -> (String, String) {
    let mut child = command
        .current_dir(dir)
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the shell");
    drop(child.stdin.take());
    let mut pipe = child.stdout.take().expect("its standard output");
    let (sender, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        let _ = sender.send(bytes);
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait().expect("wait for the shell") {
            Some(status) => break status.code().map_or_else(|| "-1".into(), |c| c.to_string()),
            None if started.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                break "timeout".into();
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    let stdout = received
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_default();
    (
        String::from_utf8_lossy(&stdout).replace("\r\n", "\n"),
        status,
    )
}
