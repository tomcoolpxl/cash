//! How fast the line editor keeps up on a ConPTY, measured against PowerShell's.
//!
//! A measurement, not a pass/fail test: it prints what it finds, and fails only when a
//! shell leaves the screen wrong. It answered `open-issues.md` item 1 and is here to answer
//! it again, after a Reedline or Crossterm upgrade for instance (see their
//! `CASH-PATCHES.md`). Run it from the repository root:
//!
//! ```text
//! cargo test --release --target-dir target/bench -p cash --test pty-latency -- --ignored --nocapture
//! ```
//!
//! `--target-dir` keeps the build away from `target/release/cash.exe`, which may be running
//! as somebody's shell, and a debug build's numbers say little. `CASH_LATENCY_BIN` names a
//! `cash.exe` to measure instead of the one cargo built, for instance an older build.
//!
//! Run it on an idle machine. ConPTY draws its screen on its own schedule, and with the CPU
//! busy (a `cargo test --workspace` alongside, say) it merges many keys into one late frame:
//! latencies of seconds at a few bytes a key measure the machine, not the shell. Each
//! measurement runs [`RUNS`] times, so a single odd row, like the first launch of a fresh
//! build while Windows scans it, stands out as one.
//!
//! Each shell runs in this repository on a 120x30 ConPTY: cash and `pwsh -NoProfile`, each
//! with a plain prompt and, when `starship` is on `PATH`, with Starship's. Measured:
//!
//! - **Backspace** held at a fast key repeat over an 80-character line, with the prompt on
//!   an empty screen and at the bottom of a full one: how long each deletion takes to show,
//!   how many bytes each key sends the terminal, and for how long a character already
//!   deleted was still on screen.
//! - **Paste**: a 2000-character line written in one piece, as a terminal delivers one, and
//!   how long until all of it is drawn.
//!
//! What ConPTY sends is replayed onto a `Screen` after every chunk, and the moment a key's
//! effect first shows there is its latency.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "a measurement run by hand, which reports failures loudly"
)]

use cash_win32::conpty::{ConPty, ConPtyChild};
use cash_win32::vtscreen::Screen;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const COLS: u16 = 120;
const ROWS: u16 = 30;
/// The line typed and then deleted: short enough to share a row with a prompt's last line.
const LINE: usize = 80;
/// A fast key repeat: Windows' fastest setting is about 30 a second.
const REPEAT: Duration = Duration::from_millis(33);
/// The length of the pasted line.
const PASTE: usize = 2000;
/// Runs of each measurement, all printed: one slow run is the machine, not the shell.
const RUNS: usize = 3;
/// No output for this long means a shell has finished reacting.
const QUIET: Duration = Duration::from_millis(700);

/// What the ConPTY sent, each chunk with the moment it arrived.
type Chunks = Arc<Mutex<Vec<(Instant, Vec<u8>)>>>;

/// A shell to measure.
struct Shell {
    name: &'static str,
    program: PathBuf,
    args: Vec<String>,
    env: Vec<(&'static str, String)>,
    /// Text on screen once the prompt is drawn.
    prompt: &'static str,
    /// A command printing `l1` to `l40`, which fills the screen.
    fill: &'static str,
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

/// cash, and PowerShell to compare it with; the Starship variants when it is installed.
fn shells(scratch: &Path) -> Vec<Shell> {
    let cash = std::env::var_os("CASH_LATENCY_BIN")
        .map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_cash")), PathBuf::from);
    let history = scratch.join("history").to_string_lossy().into_owned();
    let cash_fill = "for i in {1..40}; do echo l$i; done\r";
    let pwsh_fill = "1..40 | % { \"l$_\" }\r";
    let starship = which::which("starship").is_ok();

    let mut shells = vec![Shell {
        name: "cash",
        program: cash.clone(),
        args: strings(&["--norc", "--noprofile", "--no-config", "-i"]),
        env: vec![
            ("PS1", "PROMPT$ ".to_owned()),
            ("HISTFILE", history.clone()),
        ],
        prompt: "PROMPT$",
        fill: cash_fill,
    }];
    if starship {
        let rc = scratch.join("starship.rc");
        std::fs::write(&rc, "eval \"$(starship init bash)\"\n").unwrap();
        let rc = rc.to_string_lossy().into_owned();
        shells.push(Shell {
            name: "cash + Starship",
            program: cash,
            args: strings(&["--rcfile", &rc, "--noprofile", "--no-config", "-i"]),
            env: vec![("HISTFILE", history)],
            prompt: "\u{276f}",
            fill: cash_fill,
        });
    }
    if let Ok(pwsh) = which::which("pwsh") {
        shells.push(Shell {
            name: "pwsh",
            program: pwsh.clone(),
            args: strings(&["-NoLogo", "-NoProfile"]),
            env: vec![],
            prompt: "PS ",
            fill: pwsh_fill,
        });
        if starship {
            let init = "Invoke-Expression (& starship init powershell)";
            shells.push(Shell {
                name: "pwsh + Starship",
                program: pwsh,
                args: strings(&["-NoLogo", "-NoProfile", "-NoExit", "-Command", init]),
                env: vec![],
                prompt: "\u{276f}",
                fill: pwsh_fill,
            });
        }
    }
    shells
}

/// What the screen shows of the typed line: `q`s, which a prompt seldom has, so the
/// count on screen before typing is subtracted.
#[derive(Clone, Copy)]
struct View {
    typed: usize,
    /// Typed characters at or right of the cursor: shown, but already deleted.
    stale: usize,
}

fn view(screen: &Screen, before: usize) -> View {
    let text = screen.text();
    let (row, col) = screen.cursor();
    View {
        typed: count_q(&text).saturating_sub(before),
        stale: text.lines().nth(row).map_or(0, |line| {
            line.chars().skip(col).filter(|&c| c == 'q').count()
        }),
    }
}

fn count_q(text: &str) -> usize {
    text.chars().filter(|&c| c == 'q').count()
}

const fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// A shell on a ConPTY, with a thread collecting what the ConPTY sends.
struct Session {
    pty: ConPty,
    child: ConPtyChild,
    chunks: Chunks,
}

impl Session {
    fn start(shell: &Shell, dir: &Path) -> Self {
        let mut pty = ConPty::new(i16::try_from(COLS).unwrap(), i16::try_from(ROWS).unwrap())
            .expect("no ConPTY");
        // Colored as in a terminal: the shell running the tests may have asked for none.
        let mut env: Vec<(String, String)> = std::env::vars()
            .filter(|(key, _)| {
                !["NO_COLOR", "TERM"]
                    .iter()
                    .chain(shell.env.iter().map(|(name, _)| name))
                    .any(|name| key.eq_ignore_ascii_case(name))
            })
            .collect();
        env.extend(shell.env.iter().map(|(k, v)| ((*k).to_owned(), v.clone())));
        let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let args: Vec<&str> = shell.args.iter().map(String::as_str).collect();
        let child = pty
            .spawn_in(&shell.program, &args, Some(&env), Some(dir))
            .unwrap_or_else(|e| panic!("{}: could not start: {e}", shell.name));

        let chunks = Chunks::default();
        let mut output = pty.output_mut().try_clone().unwrap();
        let sink = Arc::clone(&chunks);
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 1 << 16];
            while let Ok(n @ 1..) = output.read(&mut buf) {
                sink.lock()
                    .unwrap()
                    .push((Instant::now(), buf[..n].to_vec()));
            }
        });
        Self { pty, child, chunks }
    }

    fn send(&mut self, bytes: &[u8]) {
        let input = self.pty.input_mut();
        input.write_all(bytes).unwrap();
        input.flush().unwrap();
    }

    fn received(&self) -> Vec<(Instant, Vec<u8>)> {
        self.chunks.lock().unwrap().clone()
    }

    fn screen(&self) -> Screen {
        let mut screen = Screen::new(usize::from(COLS), usize::from(ROWS));
        for (_, bytes) in self.received() {
            screen.feed(&bytes);
        }
        screen
    }

    /// Waits up to 30 s for the screen to show `what`.
    fn wait_for(&self, what: &str, done: impl Fn(&Screen) -> bool) {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(30) {
            if done(&self.screen()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("never showed {what}:\n{}", self.screen().text());
    }

    /// Waits until nothing has arrived for [`QUIET`], or 10 s.
    fn settle(&self) {
        let start = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(10));
            let last = self
                .received()
                .last()
                .map_or(start, |(at, _)| *at)
                .max(start);
            if last.elapsed() >= QUIET || start.elapsed() >= Duration::from_secs(10) {
                return;
            }
        }
    }

    /// The prompt drawn, and the screen filled first when `full`: the `q`s on it by then.
    fn ready(&mut self, shell: &Shell, full: bool) -> usize {
        self.wait_for("the prompt", |screen| screen.text().contains(shell.prompt));
        self.settle();
        if full {
            self.send(shell.fill.as_bytes());
            self.wait_for("the filled screen", |screen| screen.text().contains("l40"));
            self.settle();
        }
        count_q(&self.screen().text())
    }

    /// Everything after the first `from` chunks, as the screen looked after each, and the
    /// bytes they held.
    fn timeline(&self, from: usize, before: usize) -> (Vec<(Instant, View)>, usize) {
        let received = self.received();
        let mut screen = Screen::new(usize::from(COLS), usize::from(ROWS));
        for (_, bytes) in &received[..from] {
            screen.feed(bytes);
        }
        let mut bytes = 0;
        let mut timeline = Vec::new();
        for (at, chunk) in &received[from..] {
            screen.feed(chunk);
            bytes += chunk.len();
            timeline.push((*at, view(&screen, before)));
        }
        (timeline, bytes)
    }
}

impl Drop for Session {
    /// Cancels the line and exits; a shell that ignores that goes with its console.
    fn drop(&mut self) {
        let _ = self.pty.input_mut().write_all(b"\x03exit\r");
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && matches!(self.child.try_wait(), Ok(None)) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// One held Backspace over the typed line.
struct Backspace {
    bytes_per_key: usize,
    /// Per key, from sending it to the screen showing one character fewer.
    latency: Vec<f64>,
    stale_ms: f64,
}

fn hold_backspace(shell: &Shell, dir: &Path, full: bool) -> Backspace {
    let mut session = Session::start(shell, dir);
    let before = session.ready(shell, full);
    session.send("q".repeat(LINE).as_bytes());
    session.wait_for("the typed line", |s| view(s, before).typed == LINE);
    session.settle();

    let from = session.received().len();
    let start = Instant::now();
    let mut sent = Vec::with_capacity(LINE);
    for key in 0..LINE {
        let due = start + REPEAT * u32::try_from(key).unwrap();
        std::thread::sleep(due.saturating_duration_since(Instant::now()));
        session.send(b"\x7f");
        sent.push(Instant::now());
    }
    session.wait_for("every character deleted", |s| view(s, before).typed == 0);
    session.settle();

    let (timeline, bytes) = session.timeline(from, before);
    let latency = sent
        .iter()
        .enumerate()
        .map(|(key, at)| {
            timeline
                .iter()
                .find(|(_, v)| v.typed < LINE - key)
                .map_or(f64::INFINITY, |(shown, _)| {
                    ms(shown.saturating_duration_since(*at))
                })
        })
        .collect();
    // Summed by hand: a float `sum` of nothing is -0.0, which prints as such.
    let mut stale_ms = 0.0;
    for pair in timeline.windows(2).filter(|pair| pair[0].1.stale > 0) {
        stale_ms += ms(pair[1].0 - pair[0].0);
    }
    Backspace {
        bytes_per_key: bytes / LINE,
        latency,
        stale_ms,
    }
}

/// A line of words, quotes and paths, `len` bytes or a little more.
fn paste_text(len: usize) -> String {
    let mut text = String::from("echo");
    let mut n = 0;
    while text.len() < len {
        write!(text, " qq{n} 'qq {n}' ./qq/{n}").unwrap();
        n += 1;
    }
    text
}

/// Milliseconds from writing a long line in one piece until all of it is drawn, and the
/// bytes that took.
fn paste(shell: &Shell, dir: &Path) -> (f64, usize) {
    let text = paste_text(PASTE);
    let expected = count_q(&text);
    let mut session = Session::start(shell, dir);
    let before = session.ready(shell, false);

    let from = session.received().len();
    let start = Instant::now();
    session.send(text.as_bytes());
    session.wait_for("the pasted line", |s| view(s, before).typed == expected);
    session.settle();

    let (timeline, bytes) = session.timeline(from, before);
    let drawn = timeline
        .iter()
        .find(|(_, v)| v.typed == expected)
        .map_or(f64::INFINITY, |(at, _)| {
            ms(at.saturating_duration_since(start))
        });
    (drawn, bytes)
}

fn percentile(values: &[f64], percent: usize) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[(sorted.len() - 1) * percent / 100]
}

#[test]
#[ignore = "a measurement, run by hand with --release and --nocapture; see the file's doc"]
fn line_editor_latency_against_powershell() {
    let scratch = tempfile::tempdir().unwrap();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();

    println!(
        "Backspace held at {} ms over {LINE} characters; latency is per key, until the screen \
         shows it. Paste: {PASTE} characters in one write.\n",
        REPEAT.as_millis()
    );
    for shell in shells(scratch.path()) {
        for _ in 0..RUNS {
            for (full, screen) in [(false, "empty"), (true, "full ")] {
                let run = hold_backspace(&shell, &repository, full);
                println!(
                    "{:<16} backspace, {screen} screen  {:>4} B/key  latency median {:>5.1} \
                     p95 {:>6.1} max {:>6.1} ms  deleted still shown {:>5.1} ms",
                    shell.name,
                    run.bytes_per_key,
                    percentile(&run.latency, 50),
                    percentile(&run.latency, 95),
                    percentile(&run.latency, 100),
                    run.stale_ms,
                );
            }
            let (drawn, bytes) = paste(&shell, &repository);
            println!(
                "{:<16} paste                  {bytes:>6} B    drawn after {drawn:>6.1} ms",
                shell.name
            );
        }
        println!();
    }
}
