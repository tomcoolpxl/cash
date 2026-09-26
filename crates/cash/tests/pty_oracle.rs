//! Interactive behaviour against Git Bash 5.3, on a real console — ROADMAP item 15.
//!
//! Each case is a series of keystrokes. It runs in a ConPTY, and what the console shows
//! at the end is compared: the screen, not the byte stream, because ConPTY renders its
//! console as VT output with cursor jumps and redraws, and two shells showing the same
//! thing send different streams. `tests/oracle/pty/NAME.txt` holds Git Bash 5.3's
//! screen for each case, made by the ignored test `record_bash_screens`:
//!
//! ```text
//! cargo test -p cash --test pty-oracle -- --ignored record_bash_screens
//! ```
//!
//! Both shells start without profile or rc files, with `PS1='$ '`, an empty `INPUTRC`
//! and no history file, in a scratch directory holding the files in [`FIXTURE_FILES`].
//! Background job numbers and pids are masked.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction"
)]

use cash_win32::conpty::ConPtySession;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

const CASH: &str = env!("CARGO_BIN_EXE_cash");
const BASH: &str = "C:/Program Files/Git/usr/bin/bash.exe";

/// Files in each case's scratch directory, for completion.
const FIXTURE_FILES: &[&str] = &["alpha beta.txt", "gamma.txt", "gamut.log"];

/// A case: its name, the audit item it probes, and the keys, sent in chunks. After each
/// chunk the shell is given time to settle.
struct Case {
    name: &'static str,
    keys: &'static [&'static str],
}

const CASES: &[Case] = &[
    // Line editing, as a baseline.
    Case {
        name: "edit-insert-in-middle",
        keys: &["echo he", "\x1b[D\x1b[Dxx", "\r"],
    },
    // `bind -x`, the colon form every Bash accepts.
    Case {
        name: "bind-x-colon",
        keys: &["bind -x '\"\\C-t\": echo BOUND'\r", "\x14"],
    },
    // 1.aa: key sequence and command separated by whitespace, the command quoted.
    Case {
        name: "1aa-bind-x-whitespace",
        keys: &["bind -x '\"\\C-t\" \"echo BOUND\"'\r", "\x14"],
    },
    // 1.bb: `bind -X` prints `bind -x` bindings in the form `bind -x` reads.
    Case {
        name: "1bb-bind-X-output",
        keys: &["bind -x '\"\\C-t\": echo BOUND'\r", "bind -X\r"],
    },
    // 1.gg: `bind -p NAME` prints only the bindings of that command.
    Case {
        name: "1gg-bind-p-name",
        keys: &["bind -p beginning-of-line\r"],
    },
    // 1.dd: the vi-mode completion command has a Bash-specific name.
    Case {
        name: "1dd-bash-vi-complete",
        keys: &["bind -l | grep -c vi-complete\r"],
    },
    // Filename completion, unquoted and after an opening quote (1.b).
    Case {
        name: "complete-file-unquoted",
        keys: &["ls al", "\t"],
    },
    Case {
        name: "1b-complete-file-in-quotes",
        keys: &["ls \"al", "\t"],
    },
    Case {
        name: "complete-common-prefix",
        keys: &["ls ga", "\t", "\t"],
    },
    // 1.u: `compopt -o fullquote` quotes the whole completion.
    Case {
        name: "1u-compopt-fullquote",
        keys: &[
            "f() { compopt -o fullquote; COMPREPLY=('x y'); }; complete -F f cmd\r",
            "cmd ",
            "\t",
        ],
    },
    // 1.jj: a completion function returning 124 has its compspec reloaded.
    Case {
        name: "1jj-compfunc-124",
        keys: &[
            "_d() { complete -W dynamic cmd; return 124; }; complete -D -F _d\r",
            "cmd d",
            "\t",
        ],
    },
    // 1.cc: `read -E` reads with the line editor and the shell's completion.
    Case {
        name: "1cc-read-E",
        keys: &["read -E x\r", "cat al", "\t", "\r", "echo \"[$x]\"\r"],
    },
    // 1.rr: a job finishing while a file is sourced is reported after it.
    Case {
        name: "1rr-sourcing-notify",
        keys: &[
            "printf 'sleep 0.4\\n' > s.sh\r",
            "sleep 0.1 & . ./s.sh; echo after\r",
        ],
    },
];

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cash-pty-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for file in FIXTURE_FILES {
        std::fs::write(dir.join(file), "").unwrap();
    }
    std::fs::write(dir.join("inputrc"), "").unwrap();
    dir
}

/// Runs `case` in `program` and returns the screen it leaves.
fn screen(program: &str, args: &[&str], case: &Case) -> String {
    let dir = fixture(case.name);
    let inputrc = dir.join("inputrc").to_string_lossy().replace('\\', "/");
    let home = dir.to_string_lossy().replace('\\', "/");
    let overrides = [
        ("PS1", "$ "),
        ("PS2", "> "),
        ("HISTFILE", ""),
        ("TERM", "xterm-256color"),
        ("INPUTRC", inputrc.as_str()),
        ("HOME", home.as_str()),
    ];
    let skip = ["BASH_ENV", "ENV", "PROMPT_COMMAND", "PS0", "PS4", "CDPATH"];
    let inherited: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| {
            !skip.iter().any(|s| k.eq_ignore_ascii_case(s))
                && !overrides.iter().any(|(o, _)| k.eq_ignore_ascii_case(o))
        })
        .collect();
    let mut env: Vec<(&str, &str)> = inherited
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    env.extend(overrides.iter().copied());

    let mut session =
        ConPtySession::start_in(Path::new(program), args, Some(&env), Some(&dir)).unwrap();
    session
        .settle(Duration::from_millis(700), Duration::from_secs(10))
        .unwrap();
    for chunk in case.keys {
        session.send(chunk).unwrap();
        session
            .settle(Duration::from_millis(500), Duration::from_secs(10))
            .unwrap();
    }
    let text = session.screen().text();
    drop(session);
    let _ = std::fs::remove_dir_all(&dir);
    mask(&text)
}

/// Masks what differs from run to run: a background job's pid, `[1] 1234`.
fn mask(text: &str) -> String {
    text.lines()
        .map(|line| {
            match line
                .split_once("] ")
                .filter(|(job, _)| job.starts_with('['))
            {
                Some((job, rest))
                    if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    format!("{job}] PID")
                }
                _ => line.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn bash_screen(case: &Case) -> String {
    screen(BASH, &["--noprofile", "--norc", "-i"], case)
}

fn cash_screen(case: &Case) -> String {
    screen(CASH, &["--noprofile", "--norc", "--no-config", "-i"], case)
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/oracle/pty")
        .join(format!("{name}.txt"))
}

#[test]
#[ignore = "records Git Bash 5.3's screens; run by hand when cases change"]
fn record_bash_screens() {
    std::fs::create_dir_all(golden_path("x").parent().unwrap()).unwrap();
    std::thread::scope(|scope| {
        for case in CASES {
            scope.spawn(move || {
                let text = bash_screen(case);
                std::fs::write(golden_path(case.name), format!("{text}\n")).unwrap();
            });
        }
    });
}

/// The cases where cash differs from Bash, each with the reason. A case that starts to
/// match fails the test until it is taken off this list.
const KNOWN_DIFFERENCES: &[(&str, &str)] = &[
    (
        "complete-file-unquoted",
        "not yet fixed: a completed name with a space is wrapped in escaped quotes",
    ),
    ("1b-complete-file-in-quotes", "not yet fixed: as above"),
    (
        "1cc-read-E",
        "not yet fixed: completion quotes with \"\" where Bash escapes",
    ),
    (
        "1u-compopt-fullquote",
        "not yet fixed: the completion is inserted unquoted",
    ),
    (
        "complete-common-prefix",
        "not yet fixed: candidates are listed before the common prefix is inserted",
    ),
    (
        "bind-x-colon",
        "not yet fixed: a bind -x binding never runs",
    ),
    (
        "1aa-bind-x-whitespace",
        "not yet fixed: the whitespace form does not parse",
    ),
    (
        "1gg-bind-p-name",
        "not yet fixed: bind -p takes no command name",
    ),
    (
        "1rr-sourcing-notify",
        "not yet fixed: job notices are worded differently and come after the line",
    ),
];

#[test]
fn cash_leaves_the_screen_bash_leaves() {
    let results: Vec<(&str, String, String)> = std::thread::scope(|scope| {
        #[allow(
            clippy::needless_collect,
            reason = "every case starts before any is joined, so they run side by side"
        )]
        let handles: Vec<_> = CASES
            .iter()
            .map(|case| {
                scope.spawn(move || {
                    let golden = std::fs::read_to_string(golden_path(case.name))
                        .unwrap_or_default()
                        .replace("\r\n", "\n");
                    (case.name, golden.trim_end().to_owned(), cash_screen(case))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut report = String::new();
    for (name, bash, cash) in &results {
        let known = KNOWN_DIFFERENCES.iter().any(|(n, _)| n == name);
        if (bash == cash) == known {
            let verdict = if known {
                "now matches (remove from KNOWN_DIFFERENCES)"
            } else {
                "differs"
            };
            let _ = write!(
                report,
                "\n=== {name}: {verdict}\n--- bash\n{bash}\n--- cash\n{cash}\n"
            );
        }
    }
    assert!(report.is_empty(), "{report}");
}
