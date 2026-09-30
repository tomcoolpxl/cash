//! The processes of cash's jobs, held open so that their pids stay theirs — **D22**.
//!
//! A script keeps a background job's pid and uses it later: `kill "$pid"` in a cleanup
//! trap, `while kill -0 "$pid"; do …`. On Linux that is safe after the job has ended,
//! because the kernel hands pids out in ascending order and comes back to one only after
//! millions of others; Bash calls `kill(2)` on the number and gets "No such process".
//! Windows hands a pid out again within a second on a busy machine, so the number alone
//! named a stranger: `kill -0` said the job was running, and `kill` ended some other
//! program.
//!
//! So cash keeps a handle on each process of a job ([`Held`]). While it does, Windows
//! cannot give the pid to another process, and asking by pid gets the truth: a signal
//! reaches the job's process, or reports that there is no such process. It is the
//! reservation a zombie holds on Linux until its parent reaps it.
//!
//! Running processes are held for as long as they run. Of those that have ended, the
//! most recent [`MAX_ENDED`] are held and the oldest let go, as `wait` remembers the
//! statuses of that many finished jobs; past that a pid is free again, and cash knows
//! nothing of it, as Bash knows nothing of a pid the kernel has reused. Each ended
//! process held costs the kernel about 15 KB (measured 2026-09-30), is not listed, and
//! does not keep its executable open.

use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};

use crate::process::Held;

/// How many ended processes stay held before the oldest is let go: as many as the
/// finished jobs `wait PID` remembers.
pub const MAX_ENDED: usize = 1024;

/// The held processes, oldest first.
static CHILDREN: Mutex<VecDeque<Held>> = Mutex::new(VecDeque::new());

/// Holds the process cash has just started as `pid`, while its own handle to it is still
/// open, so the pid cannot have changed hands. Returns whether it is held.
pub fn hold(pid: u32) -> bool {
    let Some(process) = Held::open(pid) else {
        return false;
    };
    let mut children = CHILDREN.lock().unwrap_or_else(PoisonError::into_inner);
    admit(&mut children, process, MAX_ENDED);
    true
}

/// Whether `pid` is a held process that is still running.
///
/// `false` for a pid that is not held: a job's process is held from its start and let
/// go only after it has ended, so one that is no longer held has ended, whatever process
/// has the number now.
#[must_use]
pub fn is_running(pid: u32) -> bool {
    CHILDREN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .any(|child| child.pid() == pid && child.is_running())
}

/// Adds `process`, then lets go of the oldest ended ones beyond `max_ended`.
fn admit(children: &mut VecDeque<Held>, process: Held, max_ended: usize) {
    children.push_back(process);
    let ended = children.iter().filter(|child| !child.is_running()).count();
    let mut excess = ended.saturating_sub(max_ended);
    if excess > 0 {
        children.retain(|child| {
            let let_go = excess > 0 && !child.is_running();
            if let_go {
                excess -= 1;
            }
            !let_go
        });
    }
}

#[cfg(test)]
mod tests {
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    use super::*;
    use crate::process::{is_pid_alive, started};

    /// A process that has ended, with its pid and start time; nothing holds it.
    fn ended() -> (u32, u64) {
        let mut child = Command::new("cmd.exe")
            .args(["/d", "/c", "exit"])
            .spawn()
            .unwrap();
        let identity = (child.id(), started(child.id()).unwrap());
        child.wait().unwrap();
        identity
    }

    /// As [`ended`], held from before it ended.
    fn ended_and_held() -> (Held, u32, u64) {
        let mut child = Command::new("cmd.exe")
            .args(["/d", "/c", "exit"])
            .spawn()
            .unwrap();
        let held = Held::open(child.id()).unwrap();
        let identity = (child.id(), started(child.id()).unwrap());
        child.wait().unwrap();
        (held, identity.0, identity.1)
    }

    fn running() -> Child {
        Command::new("ping.exe")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
    }

    /// Whether the process that had `pid` and started at `at` stops being a process
    /// within `patience`.
    ///
    /// Windows frees a process once the last handle to it is closed. Others (the console
    /// host, a virus scanner) may hold one a moment longer than the test does: about
    /// 5 ms when measured. So "let go" and "kept" are both told by watching for a while.
    fn is_let_go(pid: u32, at: u64, patience: Duration) -> bool {
        let deadline = Instant::now() + patience;
        while started(pid) == Some(at) {
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        true
    }

    /// Long enough to see a process let go.
    const LET_GO: Duration = Duration::from_secs(10);
    /// Long enough that one still there is kept.
    const KEPT: Duration = Duration::from_secs(1);

    #[test]
    fn a_process_nothing_holds_is_let_go_when_it_ends() {
        // What the tests below rely on to tell held from not held.
        let (pid, at) = ended();
        assert!(is_let_go(pid, at, LET_GO));
    }

    #[test]
    fn a_held_process_keeps_its_pid_after_it_ends() {
        let mut child = Command::new("cmd.exe")
            .args(["/d", "/c", "exit"])
            .spawn()
            .unwrap();
        let (pid, at) = (child.id(), started(child.id()).unwrap());
        assert!(hold(pid));
        child.wait().unwrap();
        drop(child);

        // No other process can have the pid, so asking by pid is asking about this one.
        assert!(!is_let_go(pid, at, KEPT));
        assert!(!is_pid_alive(pid));
        assert!(!is_running(pid));
    }

    #[test]
    fn a_pid_that_is_not_held_is_not_running() {
        // This process runs, and cash did not start it as a job's.
        assert!(is_pid_alive(std::process::id()));
        assert!(!is_running(std::process::id()));
    }

    #[test]
    fn a_held_process_is_running_until_it_ends() {
        let mut child = running();
        assert!(hold(child.id()));
        assert!(is_running(child.id()));
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(!is_running(child.id()));
    }

    #[test]
    fn the_oldest_ended_processes_are_let_go_past_the_limit() {
        let mut children = VecDeque::new();
        let mut live = running();
        admit(&mut children, Held::open(live.id()).unwrap(), 2);

        let (first, first_pid, first_at) = ended_and_held();
        let (second, second_pid, second_at) = ended_and_held();
        admit(&mut children, first, 2);
        admit(&mut children, second, 2);
        assert!(!is_let_go(first_pid, first_at, KEPT), "two may have ended");

        let (third, third_pid, third_at) = ended_and_held();
        admit(&mut children, third, 2);
        assert!(is_let_go(first_pid, first_at, LET_GO), "the oldest goes");
        assert!(!is_let_go(second_pid, second_at, KEPT));
        assert_eq!(started(third_pid), Some(third_at));

        // A running process is never let go, however many have ended since it started.
        let held: Vec<u32> = children.iter().map(Held::pid).collect();
        assert_eq!(held, [live.id(), second_pid, third_pid]);
        live.kill().unwrap();
        live.wait().unwrap();
    }
}
