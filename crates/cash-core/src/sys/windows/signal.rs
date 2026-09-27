//! Signals on Windows — **D13**, **D19**, **D21**, **D22**.
//!
//! Windows has no POSIX signals. The stub this replaces made `Signal` an empty enum, so
//! `trap INT` was rejected outright and `kill -TERM` could not name a target: nothing
//! signal-shaped worked at all.
//!
//! cash instead exposes the signals it can *honestly* implement, and each is backed by a
//! real Win32 mechanism rather than a pretence:
//!
//! | Signal | Mechanism |
//! |---|---|
//! | `INT` | `CTRL_BREAK_EVENT` to the process group (D13) |
//! | `TERM` | console event, then termination after a grace period (D21) |
//! | `KILL` | immediate `TerminateProcess` — cannot be caught, as on POSIX |
//! | `QUIT`, `HUP` | delivered like `TERM`; Windows has no distinct concept |
//! | `STOP`, `TSTP` | thread-enumeration suspend (D19) |
//! | `CONT` | resume (D19) |
//! | `CHLD` | cash's own: a trap runs once per child process the shell reaps |
//!
//! `CHLD` is not a signal Windows delivers, but its event is one cash sees: every child
//! it starts, it waits for. So `trap … CHLD` runs at the points Bash runs a pending trap
//! (after a command, in `wait`, when a job starts, at the prompt), once per child reaped
//! since. Sent to another process, it does what POSIX's default does: nothing.
//!
//! Deliberately absent: `SIGPIPE`, `SIGUSR1/2`, `SIGALRM` and the rest. There
//! is no Win32 mechanism behind them, and accepting a trap that can never fire would be
//! the silent-failure pattern D20 and D26 both reject. `trap USR1` therefore reports an
//! invalid signal rather than pretending.
//!
//! §4 divergence #9 records the consequence: `kill -STOP` suspends threads rather than
//! being a real `SIGSTOP`.

#![allow(
    clippy::should_implement_trait,
    clippy::unnecessary_wraps,
    clippy::missing_const_for_fn,
    reason = "these signatures mirror the Unix module's, because shared code calls both \
              through the same names. Narrowing a stub to `const`, or to a bare return \
              value, would break that symmetry — and several of these stubs are where a \
              real Win32 implementation may yet land."
)]

use crate::{error, sys, traps};

/// Ctrl-C arrives through tokio's console-control handler.
pub(crate) use tokio::signal::ctrl_c as await_ctrl_c;

/// The signals cash can implement on Windows.
///
/// Numbering follows the conventional POSIX values so that `kill -9` and `$?`'s `128 + n`
/// convention (D15) line up with what a script written on Linux expects.
// Re-exported through `sys::signal`, so it is reachable without being nameable by that
// path — the same shape as the stub this replaces.
#[allow(unnameable_types)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i32)]
pub enum Signal {
    /// Hangup. Delivered as a termination request.
    Hup = 1,
    /// Interrupt — what Ctrl-C raises (D13).
    Int = 2,
    /// Quit. Delivered as a termination request.
    Quit = 3,
    /// Kill. Immediate and uncatchable, as on POSIX.
    Kill = 9,
    /// Terminate, with the graceful-first escalation of D21.
    Term = 15,
    /// A child exited. cash's own event, from the children it reaps.
    Chld = 17,
    /// Continue a stopped process (D19).
    Cont = 18,
    /// Stop: suspend every thread of the target (D19).
    Stop = 19,
    /// Terminal stop — what Ctrl-Z raises (D19).
    Tstp = 20,
}

impl Signal {
    /// Every signal cash supports on Windows.
    pub fn iterator() -> impl Iterator<Item = Self> {
        [
            Self::Hup,
            Self::Int,
            Self::Quit,
            Self::Term,
            Self::Kill,
            Self::Stop,
            Self::Tstp,
            Self::Cont,
            Self::Chld,
        ]
        .into_iter()
    }

    /// The signal's name, without the `SIG` prefix, as `trap -l` and `kill -l` print it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hup => "HUP",
            Self::Int => "INT",
            Self::Quit => "QUIT",
            Self::Term => "TERM",
            Self::Kill => "KILL",
            Self::Stop => "STOP",
            Self::Tstp => "TSTP",
            Self::Cont => "CONT",
            Self::Chld => "CHLD",
        }
    }

    /// The conventional POSIX signal number.
    ///
    /// These are the enum's discriminants, deliberately: shared code converts a signal
    /// to a number with `s as i32`, which yields the discriminant. Declaration order
    /// would give 0, 1, 2... and `kill -l KILL` would answer 4 instead of 9.
    pub const fn number(self) -> i32 {
        self as i32
    }

    /// Parse a signal name, with or without the `SIG` prefix, or a signal number.
    pub fn from_str(s: &str) -> Result<Self, error::Error> {
        let trimmed = s.trim();

        if let Ok(number) = trimmed.parse::<i32>() {
            return Self::try_from(number);
        }

        let name = trimmed.to_ascii_uppercase();
        let name = name.strip_prefix("SIG").unwrap_or(&name);

        Self::iterator()
            .find(|candidate| candidate.as_str() == name)
            .ok_or_else(|| error::ErrorKind::InvalidSignal(s.into()).into())
    }
}

impl TryFrom<i32> for Signal {
    type Error = error::Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::iterator()
            .find(|candidate| candidate.number() == value)
            .ok_or_else(|| error::ErrorKind::InvalidSignal(std::format!("{value}")).into())
    }
}

/// Resume a suspended process (D19).
pub(crate) fn continue_process(pid: sys::process::ProcessId) -> Result<(), error::Error> {
    for target in resolve_targets(pid)? {
        cash_win32::console::resume_process(target)
            .map(|_| ())
            .map_err(|e| error::Error::from(error::ErrorKind::from(e)))?;
    }
    Ok(())
}

/// Whether a process exists and cash could signal it.
pub fn check_signalable(pid: sys::process::ProcessId) -> Result<(), error::Error> {
    let targets = resolve_targets(pid)?;
    if targets
        .iter()
        .any(|&t| cash_win32::process::is_pid_alive(t))
    {
        Ok(())
    } else {
        Err(error::ErrorKind::from(std::io::Error::from(std::io::ErrorKind::NotFound)).into())
    }
}

/// Turn a POSIX kill target into the concrete Windows process ids to act on.
///
/// POSIX gives the sign of the argument a meaning, and getting this wrong on Windows is
/// not a cosmetic error — it is how `kill 0` came to send a console control event to
/// *every process attached to the console*, terminal included.
///
/// | Target | POSIX meaning | What cash does |
/// |---|---|---|
/// | `pid > 0` | that one process | that one process |
/// | `0` | every process in my process group | every process tree cash spawned |
/// | `-pid` | every process in that group | the tree rooted at `pid` |
/// | `-1` | every process I may signal | refused |
///
/// `0` is the interesting one. Windows has no notion of "the shell's process group" that
/// excludes the terminal, so the console is the wrong answer by a wide margin; the set of
/// trees cash created is what a script writing `trap 'kill 0' EXIT` actually wants.
///
/// `-1` is refused rather than approximated. "Everything I am permitted to signal" on
/// Windows reaches well past anything a script could have intended, and guessing at a
/// destructive operation is worse than declining it.
fn resolve_targets(pid: sys::process::ProcessId) -> Result<Vec<u32>, error::Error> {
    match pid {
        p if p > 0 => Ok(u32::try_from(p).map_or_else(|_| Vec::new(), |raw| vec![raw])),
        0 => Ok(cash_win32::jobreg::roots()),
        -1 => Err(error::ErrorKind::from(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "kill -1 (every process) is not supported on Windows",
        ))
        .into()),
        negative => Ok(vec![negative.unsigned_abs()]),
    }
}

/// Deliver a signal to a process (D21, D22).
///
/// D22's scope rule — job specs reap trees, bare PIDs hit one process — is applied by
/// the caller, which knows which spelling was used. This acts on the single process it
/// is given.
pub fn kill_process(
    pid: sys::process::ProcessId,
    signal: traps::TrapSignal,
) -> Result<(), error::Error> {
    let traps::TrapSignal::Signal(signal) = signal else {
        // DEBUG/ERR/EXIT/RETURN are shell-internal traps, not deliverable to a process.
        return Err(error::ErrorKind::InvalidSignal(signal.to_string()).into());
    };

    let targets = resolve_targets(pid)?;
    if targets.is_empty() {
        // Nothing to signal. `kill 0` in a shell that has spawned nothing is a no-op in
        // bash too, so this is not an error.
        return Ok(());
    }

    // A target named explicitly must report if it is gone. A target that came from a
    // *set* — `kill 0` — may have exited between resolving the set and signalling it,
    // and that is a race, not a failure of the command.
    let named = pid != 0;
    let mut first_error = None;

    for target in targets {
        if let Err(e) = deliver(target, signal) {
            let vanished = e
                .as_io_error()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound);
            if named || !vanished {
                first_error.get_or_insert(e);
            }
        }
    }

    first_error.map_or(Ok(()), Err)
}

/// The status a process a signal terminates exits with: POSIX's 128 + the signal's
/// number, which Windows lets the terminating side choose.
#[expect(
    clippy::cast_sign_loss,
    reason = "signal numbers are small and positive"
)]
const fn exit_status(signal: Signal) -> u32 {
    (128 + signal.number()) as u32
}

/// Deliver one signal to one process.
fn deliver(raw: u32, signal: Signal) -> Result<(), error::Error> {
    // Check first, so that a target that does not exist reports as such. Windows answers
    // a signal aimed at nothing with `ERROR_INVALID_PARAMETER`, and "The parameter is
    // incorrect. (os error 87)" tells the user nothing about what went wrong.
    if !cash_win32::process::is_pid_alive(raw) {
        return Err(
            error::ErrorKind::from(std::io::Error::from(std::io::ErrorKind::NotFound)).into(),
        );
    }

    let result = match signal {
        // D19: Windows has no SIGSTOP, so suspend every thread of the target. §4
        // divergence #9.
        Signal::Stop | Signal::Tstp => cash_win32::console::suspend_process(raw).map(|_| ()),
        Signal::Cont => cash_win32::console::resume_process(raw).map(|_| ()),
        // POSIX's default for SIGCHLD is to ignore it; there is nothing to deliver.
        Signal::Chld => Ok(()),

        // D21: TERM and friends ask first, through the target's windows, and terminate
        // it if it has none to ask or has not exited when the grace period ends. Not a
        // console control event: aimed at a pid that leads no process group — and cash
        // starts none as leaders — that reaches every process on the console, so
        // `kill -TERM $pid` used to kill the shell. KILL does not ask at all, matching
        // POSIX where it cannot be caught.
        Signal::Int | Signal::Term | Signal::Hup | Signal::Quit => {
            cash_win32::stop::request_stop(raw, cash_win32::stop::GRACE, exit_status(signal))
        }
        // D22: reap the whole tree when this pid roots one, falling back to the single
        // process otherwise. That is what makes `kill %1` reap a pipeline's descendants
        // rather than orphaning them.
        Signal::Kill => match cash_win32::jobreg::terminate_tree(raw, exit_status(signal)) {
            Ok(true) => Ok(()),
            Ok(false) => cash_win32::process::terminate(raw, exit_status(signal)),
            Err(e) => Err(e),
        },
    };

    result.map_err(|e| error::ErrorKind::from(e).into())
}

pub(crate) fn lead_new_process_group() -> Result<(), error::Error> {
    // Windows process groups are established at creation time via
    // CREATE_NEW_PROCESS_GROUP, not by a call after the fact.
    Ok(())
}

pub(crate) struct FakeSignal {}

impl FakeSignal {
    const fn new() -> Self {
        Self {}
    }

    pub async fn recv(&self) {
        futures::future::pending::<()>().await;
    }
}

/// Ctrl-Z arrives through the console input path rather than as a signal, so there is
/// nothing to listen for here.
pub(crate) fn tstp_signal_listener() -> Result<FakeSignal, error::Error> {
    Ok(FakeSignal::new())
}

/// Windows has no `SIGCHLD`; child exits are observed by waiting on handles.
pub(crate) fn chld_signal_listener() -> Result<FakeSignal, error::Error> {
    Ok(FakeSignal::new())
}

pub(crate) fn mask_sigttou() -> Result<(), error::Error> {
    // No SIGTTOU on Windows; background reads from the console fail rather than signal.
    Ok(())
}

pub(crate) fn poll_for_stopped_children() -> Result<bool, error::Error> {
    Ok(false)
}
