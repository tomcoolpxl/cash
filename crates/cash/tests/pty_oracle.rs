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
//! Where cash differs on purpose, `NAME.cash.txt` holds the screen cash must leave
//! instead, and [`DELIBERATE`] says why. Differences not yet fixed are listed in
//! [`KNOWN_DIFFERENCES`].
//!
//! Both shells start without profile or rc files, with `PS1='$ '`, an empty `INPUTRC`
//! and no history file, in a scratch directory holding [`FIXTURE_FILES`] and
//! [`FIXTURE_DIRS`].
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
const FIXTURE_FILES: &[&str] = &["alpha beta.txt", "gamma.txt", "gamut.log", "it's here.txt"];

/// Directories in each case's scratch directory.
const FIXTURE_DIRS: &[&str] = &["my dir"];

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
    // A quoted command with escaped quotes inside: what runs, and what `bind -X` shows.
    Case {
        name: "1aa-bind-x-quoted-escapes",
        keys: &[
            "bind -x '\"\\C-t\" \"echo \\\"q\\\" x\"'\r",
            "\x14",
            "bind -X\r",
        ],
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
    Case {
        name: "1gg-bind-upper-P-name",
        keys: &["bind -P beginning-of-line\r"],
    },
    Case {
        name: "1gg-bind-p-unbound",
        keys: &[
            "bind -p vi-put; echo \"rc=$?\"\r",
            "bind -P vi-put; echo \"rc=$?\"\r",
        ],
    },
    Case {
        name: "1gg-bind-p-unknown",
        keys: &["bind -p no-such-thing; echo \"rc=$?\"\r"],
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
    // A directory: no space after it, so the path can go on.
    Case {
        name: "complete-dir-with-space",
        keys: &["cd my", "\t", "in", "\t"],
    },
    // The completed line runs as it stands.
    Case {
        name: "complete-dir-then-run",
        keys: &["cd my", "\t", "\r", "echo \"[${PWD##*/}]\"\r"],
    },
    // A backslash escape already typed continues as backslashes.
    Case {
        name: "complete-backslash-typed",
        keys: &["ls alpha\\ ", "\t"],
    },
    // A name holding a single quote.
    Case {
        name: "complete-apostrophe",
        keys: &["ls it", "\t"],
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
    // Without it, a function's completion is inserted as it is.
    Case {
        name: "1u-compopt-no-fullquote",
        keys: &[
            "f() { COMPREPLY=('x y'); }; complete -F f cmd\r",
            "cmd ",
            "\t",
        ],
    },
    Case {
        name: "1u-complete-p-fullquote",
        keys: &["complete -o fullquote -W 'a b' cmd; complete -p cmd\r"],
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
    // When a finished job is reported: after a foreground command, not only at the prompt.
    Case {
        name: "notify-after-foreground",
        keys: &["sleep 0.05 & sleep 0.2; echo after\r"],
    },
    // How `jobs` lays out running and finished jobs, with the current and previous marks.
    // The first two run throughout, so nothing depends on how long a step takes.
    Case {
        name: "jobs-layout",
        keys: &[
            "sleep 10 & sleep 10 &\r",
            "jobs\r",
            // The pid, however wide, masked.
            "jobs -l | sed -E 's/^(.{4}) +[0-9]+ /\\1 PID /'\r",
            "sleep 0.05 &\r",
            "sleep 0.3\r",
            // A real process, so that both shells have its pid.
            "bash -c 'sleep 0.1; exit 3' &\r",
            "sleep 0.3\r",
        ],
    },
    // `read -t` at the console times out; the Enter that ran it must not wake it early.
    Case {
        name: "read-t-console",
        keys: &["read -t 0.3; echo \"rc=$?\"\r"],
    },
    // A job that finishes during a builtin, with another started on the same line: its
    // `Done` is still reported, and the new job does not take its id.
    Case {
        name: "notice-not-lost",
        keys: &["sleep 0.05 & read -t 0.3; sleep 10 &\r"],
    },
    // 1.rr: a job finishing while a file is sourced is reported after it.
    Case {
        name: "1rr-sourcing-notify",
        keys: &[
            // Short sleeps: the harness moves on after 500 ms without output.
            "printf 'sleep 0.2\\n' > s.sh\r",
            "sleep 0.05 & . ./s.sh; echo after\r",
        ],
    },
];

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cash-pty-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for sub in FIXTURE_DIRS {
        std::fs::create_dir_all(dir.join(sub).join("inner")).unwrap();
    }
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

fn read_screen(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.replace("\r\n", "\n").trim_end().to_owned())
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

/// The cases where cash differs from Bash on purpose; each has a `NAME.cash.txt`.
const DELIBERATE: &[(&str, &str)] = &[
    (
        "complete-file-unquoted",
        "D40: with no quote typed, a name that needs quoting is completed in single \
         quotes, as PowerShell does: `'a b'`",
    ),
    ("1cc-read-E", "D40, as above"),
    (
        "complete-dir-with-space",
        "D40: `'my dir/'`, the cursor before the closing quote, so the path goes on",
    ),
    ("complete-dir-then-run", "D40, as above"),
    (
        "complete-apostrophe",
        "D40: a name holding `'` is completed in double quotes",
    ),
    (
        "complete-common-prefix",
        "D40: the first Tab inserts the shared part and the second shows the candidates; \
         Bash beeps on the second and lists on the third",
    ),
    (
        "1u-compopt-fullquote",
        "D40: `compopt -o fullquote` quotes as file names are quoted, `'x y'`",
    ),
];

/// The cases where cash differs from Bash, not yet fixed, each with the difference. A
/// case that starts to match fails the test until it is taken off this list.
const KNOWN_DIFFERENCES: &[(&str, &str)] = &[];

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
                    let bash = read_screen(&golden_path(case.name)).unwrap_or_default();
                    let wanted =
                        read_screen(&golden_path(&format!("{}.cash", case.name))).unwrap_or(bash);
                    (case.name, wanted, cash_screen(case))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut report = String::new();
    for (name, wanted, cash) in &results {
        let known = KNOWN_DIFFERENCES.iter().any(|(n, _)| n == name);
        assert_eq!(
            DELIBERATE.iter().any(|(n, _)| n == name),
            golden_path(&format!("{name}.cash")).exists(),
            "{name}: a deliberate difference and its .cash.txt go together"
        );
        if (wanted == cash) == known {
            let verdict = if known {
                "now matches (remove from KNOWN_DIFFERENCES)"
            } else {
                "differs"
            };
            let _ = write!(
                report,
                "\n=== {name}: {verdict}\n--- wanted\n{wanted}\n--- cash\n{cash}\n"
            );
        }
    }
    assert!(report.is_empty(), "{report}");
}
