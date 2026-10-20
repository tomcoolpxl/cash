//! The differential corpus, frozen: every case in `tests/corpus/{sed,awk,bash}` against the
//! output Git Bash 5.3 with GNU sed and gawk gave for it (BIN-05).
//!
//! The 209 cases ran only by hand, through `tests/corpus/run.ps1`, against an oracle on
//! the developer's machine; `results.json` was days old. `run.ps1 -Freeze` writes the
//! oracle's standard output and status beside each case (`case.sh.out`,
//! `case.sh.status`), and this runs each case under cash as the runner does and checks
//! it against them, on every machine and in CI. Re-freeze when a case is added or the
//! oracle changes.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a \
              failed set-up should abort it loudly, naming the file it could not read"
)]

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::common::{Scratch, cash_command};

/// Cases whose oracle output cash does not give, and why. They run too: one that comes to
/// match the oracle fails the test until it is taken off the list.
const KNOWN_DIFFERENT: &[(&str, &str)] = &[
    (
        "bash/bashdb.sh",
        "D3: bashdb takes a file for absolute only when it starts with `/`, which `C:/…` \
         does not, and reads no lines of it; bashdb-unixpaths.sh runs it under `unixpaths` \
         (D82)",
    ),
    (
        "bash/bats-file.sh",
        "D3: bats' `load` takes a path for absolute only when it starts with `/`, which \
         `C:/…` does not; bats-file-unixpaths.sh runs it under `unixpaths` (D82)",
    ),
    (
        "bash/direnv.sh",
        "D3: `$PWD` is `C:/…`, whose colon splits PATH_rm's colon-separated list and which \
         expand_path takes for relative; direnv-unixpaths.sh runs it under `unixpaths` (D82)",
    ),
    (
        "bash/bats-assert.sh",
        "D3: bats' `load` takes a path for absolute only when it starts with `/`, which \
         `C:/…` does not; bats-assert-unixpaths.sh runs it under `unixpaths` (D82)",
    ),
    (
        "bash/nvm-helpers.sh",
        "D3: `$PWD` is `C:/…`, whose colon splits `nvm_strip_path`'s colon-separated list",
    ),
    (
        "bash/pbb-internal-variables.sh",
        "spec §4 row 25: `$OSTYPE` is `windows`, which the case's `cygwin | msys | win32` misses",
    ),
    (
        "bash/pitfall-read-arith-injection.sh",
        "spec §4 row 27: arithmetic never runs `$(...)` from a variable's value",
    ),
    (
        "sed/pement-dos2unix-dot.sh",
        "D49: `s/.$//` names no CR, so cash's sed removes the last visible character",
    ),
];

/// Cases whose result depends on the machine, not on cash: they call GNU awk by name,
/// which the oracle has in `/usr/bin` and Windows has only where someone installed it.
/// Not run; said, with the reason.
const NOT_RUN: &[(&str, &str)] = &[
    (
        "awk/pement-gensub-gawk.sh",
        "calls `gawk` by name, there only where it is installed",
    ),
    (
        "awk/pement-re-interval-gawk.sh",
        "calls `gawk` by name, there only where it is installed",
    ),
];

/// How long one case may take, as `run.ps1` allows, unless its header says otherwise
/// ([`timeout_of`]).
const TIMEOUT: Duration = Duration::from_secs(10);

/// How many times a case's own time cash gets here. The test runs a debug build, about
/// five times slower than a release on getoptions' case, eight cases at once, and CI's
/// machines are slower again: getoptions ended its output in time and its status after
/// 10 s. No golden expects a timeout, so here the limit only stops a hang.
const DEBUG_SLOWER: u32 = 3;

/// How long the case `script` may take: its header's `# timeout: N` seconds, as in
/// `run.ps1`, or [`TIMEOUT`].
fn timeout_of(script: &str) -> Duration {
    script
        .lines()
        .take_while(|line| line.starts_with('#'))
        .find_map(|line| {
            let rest = line.trim_start_matches('#').trim_start();
            rest.strip_prefix("timeout:")?.trim().parse().ok()
        })
        .map_or(TIMEOUT, Duration::from_secs)
}

/// How many cases run at once. The test takes as many of nextest's slots
/// (`.config/nextest.toml`).
const WORKERS: usize = 8;

#[test]
fn every_corpus_case_gives_the_oracles_output() {
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut cases = Vec::new();
    let mut not_run = Vec::new();
    for group in ["sed", "awk", "bash"] {
        let mut files: Vec<PathBuf> = std::fs::read_dir(corpus.join(group))
            .expect("read the corpus")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "sh"))
            .collect();
        files.sort();
        for case in files {
            let file = case
                .file_name()
                .expect("a case file")
                .to_string_lossy()
                .into_owned();
            let name = format!("{group}/{file}");
            if let Some((_, why)) = NOT_RUN.iter().find(|(skipped, _)| *skipped == name) {
                not_run.push(format!("{name}: {why}"));
                continue;
            }
            let why = KNOWN_DIFFERENT
                .iter()
                .find(|(known, _)| *known == name)
                .map(|(_, why)| *why);
            cases.push((name, case, why));
        }
    }
    // The longest first, by the time their headers allow: a real program's case takes
    // many times a snippet's, and one started last kept the test running on one worker
    // after the others had finished (shellspec's 18 s, 2026-10-19).
    cases.sort_by_cached_key(|(_, case, _)| {
        std::cmp::Reverse(std::fs::read_to_string(case).map_or(TIMEOUT, |s| timeout_of(&s)))
    });

    let next = std::sync::atomic::AtomicUsize::new(0);
    let fixtures = corpus.join("fixtures");
    let mut failures: Vec<(usize, String)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..WORKERS)
            .map(|_| {
                scope.spawn(|| {
                    let mut failures = Vec::new();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((name, case, why)) = cases.get(index) else {
                            return failures;
                        };
                        let difference = check_case(name, case, &fixtures);
                        match (why, difference) {
                            (None, Some(difference)) => failures.push((index, difference)),
                            (Some(why), None) => failures.push((
                                index,
                                format!(
                                    "{name}: matches the oracle now; take it off \
                                     KNOWN_DIFFERENT, and its record (\"{why}\")"
                                ),
                            )),
                            _ => {}
                        }
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("a worker"))
            .collect()
    });
    failures.sort();
    let failures: Vec<String> = failures.into_iter().map(|(_, failure)| failure).collect();
    let known: Vec<String> = cases
        .iter()
        .filter_map(|(name, _, why)| why.map(|why| format!("{name}: {why}")))
        .collect();
    let ran = cases.len();

    assert!(ran > 200, "only {ran} corpus cases ran");
    assert_eq!(
        known.len(),
        KNOWN_DIFFERENT.len(),
        "a case in KNOWN_DIFFERENT is not in the corpus; found only:\n{}",
        known.join("\n")
    );
    assert_eq!(
        not_run.len(),
        NOT_RUN.len(),
        "a case in NOT_RUN is not in the corpus; found only:\n{}",
        not_run.join("\n")
    );
    assert!(
        failures.is_empty(),
        "{} of {ran} corpus cases are not as frozen (re-freeze with \
         `pwsh tests/corpus/run.ps1 -Freeze` if the oracle is right to differ):\n\n{}",
        failures.len(),
        failures.join("\n"),
    );
    eprintln!(
        "{} corpus cases match; known to differ:\n{}\nnot run:\n{}",
        ran - known.len(),
        known.join("\n"),
        not_run.join("\n")
    );
}

/// Runs one case in a scratch directory of its own, beside the fixtures. What differs from
/// the oracle, if anything.
fn check_case(name: &str, case: &Path, fixtures: &Path) -> Option<String> {
    let file = case.file_name()?.to_string_lossy();
    let expected = read(&case.with_file_name(format!("{file}.out")));
    let expected_status = read(&case.with_file_name(format!("{file}.status")));
    let expected_status = expected_status.trim();

    let script = read(case);
    let scratch = Scratch::new("corpus");
    copy_named_fixtures(fixtures, scratch.path(), &script);
    let (stdout, status) = run_case(&script, scratch.path());
    (stdout != expected || status != expected_status).then(|| {
        format!(
            "{name}: status {status} (oracle {expected_status})\n--- cash\n{stdout}--- oracle\n{expected}"
        )
    })
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
        .replace("\r\n", "\n")
}

/// The fixtures every case runs beside, as `run.ps1` copies them: directories too, for a
/// program of many files (bats-core, rbenv, git-flow).
/// The fixtures `script` names, copied into `to`, as `run.ps1` copies them when it freezes
/// a golden. Each case copied all of them, 389 files for each of 318 cases: two thirds of
/// the test's time, 93 s against 30 s (2026-10-20). A case that uses one names it, and
/// a case that lists its directory sees the same names in both.
fn copy_named_fixtures(from: &Path, to: &Path, script: &str) {
    for entry in std::fs::read_dir(from)
        .expect("read the fixtures")
        .flatten()
    {
        let name = entry.file_name();
        if !script.contains(name.to_string_lossy().as_ref()) {
            continue;
        }
        let path = entry.path();
        let target = to.join(&name);
        if path.is_dir() {
            std::fs::create_dir_all(&target).expect("make a fixture directory");
            copy_fixtures(&path, &target);
        } else if path.is_file() {
            std::fs::copy(&path, &target).expect("copy a fixture");
        }
    }
}

fn copy_fixtures(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from)
        .expect("read the fixtures")
        .flatten()
    {
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            std::fs::create_dir_all(&target).expect("make a fixture directory");
            copy_fixtures(&path, &target);
        } else if path.is_file() {
            std::fs::copy(&path, &target).expect("copy a fixture");
        }
    }
}

/// Runs `script` as `run.ps1` runs it under cash: `-c`, `LC_ALL=C`, standard input a
/// pipe already closed, killed after its [`timeout_of`] times [`DEBUG_SLOWER`]. Its
/// standard output with CRLF as LF, and its status.
fn run_case(script: &str, dir: &Path) -> (String, String) {
    let timeout = timeout_of(script) * DEBUG_SLOWER;
    let mut child = cash_command()
        .args(["--noprofile", "--norc", "-c", script])
        .current_dir(dir)
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start cash");
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
        match child.try_wait().expect("wait for cash") {
            Some(status) => break status.code().map_or_else(|| "-1".into(), |c| c.to_string()),
            None if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                break "timeout".into();
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    // A child the case started may hold the pipe after cash is gone, as `run.ps1` allows.
    let stdout = received
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_default();
    (
        String::from_utf8_lossy(&stdout).replace("\r\n", "\n"),
        status,
    )
}
