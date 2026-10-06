//! How fast croot (Alt-E) shows its first frame, on a ConPTY (spec D73: within 30 ms).
//!
//! A measurement, not a pass/fail test: it prints what it finds, and fails only when the
//! picker does not open. Run it from the repository root on an idle machine:
//!
//! ```text
//! cargo test --profile dist -p cash --test croot-latency -- --ignored --nocapture
//! ```
//!
//! `CASH_LATENCY_BIN` names a `cash.exe` to measure instead of the one cargo built, such as
//! the released one Scoop installed. The time runs from writing Alt-E to the ConPTY until
//! the picker's header has come back from it, so it includes ConPTY's own frame delay, which
//! the same measurement of a plain key shows on its own.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "a measurement run by hand, which reports failures loudly"
)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cash_win32::conpty::ConPtySession;

const RUNS: usize = 10;

fn cash() -> PathBuf {
    // process state: the binary to measure, named by whoever runs the measurement.
    std::env::var_os("CASH_LATENCY_BIN")
        .map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_cash")), PathBuf::from)
}

/// Milliseconds from sending `keys` until `needle` arrives after them.
fn time_to(session: &mut ConPtySession, keys: &str, needle: &str) -> f64 {
    let before = session.output().len();
    let start = Instant::now();
    session.send(keys).unwrap();
    while start.elapsed() < Duration::from_secs(10) {
        session.read_available().unwrap();
        if session
            .output()
            .get(before..)
            .is_some_and(|new| new.contains(needle))
        {
            return start.elapsed().as_secs_f64() * 1000.0;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("{needle:?} did not appear after {keys:?}");
}

fn measure(folder: &Path, label: &str) {
    let mut session = ConPtySession::start_in(
        &cash(),
        &["--noprofile", "--norc", "--no-config", "-i"],
        Some(&[("HISTFILE", ""), ("PS1", "PROMPT$ ")]),
        Some(folder),
    )
    .unwrap();
    session.expect("PROMPT$", Duration::from_secs(20)).unwrap();
    let mut picker = Vec::new();
    let mut plain = Vec::new();
    for _ in 0..RUNS {
        picker.push(time_to(&mut session, "\x1be", "[folders]"));
        // Esc closes the picker; the prompt is drawn again.
        session.send("\x1b").unwrap();
        session
            .settle(Duration::from_millis(150), Duration::from_secs(5))
            .unwrap();
        // A plain key for comparison: ConPTY's own delay.
        plain.push(time_to(&mut session, "x", "x"));
        session.send("\x08").unwrap();
        session
            .settle(Duration::from_millis(150), Duration::from_secs(5))
            .unwrap();
    }
    picker.sort_by(f64::total_cmp);
    plain.sort_by(f64::total_cmp);
    println!(
        "{label}: Alt-E to the first frame: median {:.1} ms, worst {:.1} ms; a plain key: median {:.1} ms",
        picker[RUNS / 2],
        picker[RUNS - 1],
        plain[RUNS / 2]
    );
    session.send("exit\r").unwrap();
}

#[test]
#[ignore = "a measurement, run by hand with the dist profile and --nocapture; see the file's doc"]
fn alt_e_first_frame() {
    let scratch = tempfile::tempdir().unwrap();
    // A home folder's worth of folders, as the user's has.
    let home_like = scratch.path().join("home");
    for i in 0..44 {
        std::fs::create_dir_all(home_like.join(format!("folder{i:02}")).join("sub")).unwrap();
    }
    measure(&home_like, "44 folders");
    // A large folder: 2,000 entries.
    let large = scratch.path().join("large");
    for i in 0..1000 {
        std::fs::create_dir_all(large.join(format!("d{i:04}"))).unwrap();
        std::fs::write(large.join(format!("f{i:04}.txt")), "").unwrap();
    }
    measure(&large, "2,000 entries");
}
