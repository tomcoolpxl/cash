//! What D36's pooled prompt job would save (TODO 13.5, EXE-13): the job setup every
//! spawn pays, `jobreg::sweep` and `jobreg::contain`, against a whole Starship prompt
//! render, which is what the user waits for.
//!
//! Run with `cargo run --release -p cash-win32 --example prompt_job_cost`, on an idle
//! machine with Starship on `PATH`.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::expect_used,
    reason = "a measurement printed for a person to read, which may abort when it cannot run"
)]

use std::os::windows::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CREATE_SUSPENDED: u32 = 0x0000_0004;
const ROUNDS: u32 = 50;

fn main() {
    // The job setup, as every spawn pays it, on a program created suspended as cash
    // creates them.
    let mut setup = Duration::ZERO;
    for _ in 0..ROUNDS {
        let mut child = Command::new("cmd")
            .args(["/d", "/c", "exit"])
            .creation_flags(CREATE_SUSPENDED)
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn cmd");
        let started = Instant::now();
        cash_win32::jobreg::sweep();
        cash_win32::jobreg::contain(child.id());
        setup += started.elapsed();
        let _ = cash_win32::console::start_threads(child.id());
        let _ = child.wait();
    }

    // A Starship prompt, start to finish.
    let mut prompt = Duration::ZERO;
    for _ in 0..ROUNDS {
        let started = Instant::now();
        let status = Command::new("starship")
            .arg("prompt")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        prompt += started.elapsed();
        if status.is_err() {
            eprintln!("starship is not on PATH");
            return;
        }
    }

    let per = |total: Duration| total.as_secs_f64() * 1000.0 / f64::from(ROUNDS);
    println!("job setup per spawn: {:.3} ms", per(setup));
    println!("starship prompt:     {:.3} ms", per(prompt));
}
