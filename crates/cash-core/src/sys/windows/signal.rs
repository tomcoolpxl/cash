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
//!
//! Deliberately absent: `SIGPIPE`, `SIGCHLD`, `SIGUSR1/2`, `SIGALRM` and the rest. There
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
    cash_win32::console::resume_process(pid_as_u32(pid))
        .map(|_| ())
        .map_err(|e| error::ErrorKind::from(e).into())
}

/// Whether a process exists and cash could signal it.
pub fn check_signalable(pid: sys::process::ProcessId) -> Result<(), error::Error> {
    if cash_win32::process::is_pid_alive(pid_as_u32(pid)) {
        Ok(())
    } else {
        Err(error::ErrorKind::from(std::io::Error::from(std::io::ErrorKind::NotFound)).into())
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
    let raw = pid_as_u32(pid);

    let traps::TrapSignal::Signal(signal) = signal else {
        // DEBUG/ERR/EXIT/RETURN are shell-internal traps, not deliverable to a process.
        return Err(error::ErrorKind::InvalidSignal(signal.to_string()).into());
    };

    let result = match signal {
        // D19: Windows has no SIGSTOP, so suspend every thread of the target. §4
        // divergence #9.
        Signal::Stop | Signal::Tstp => cash_win32::console::suspend_process(raw).map(|_| ()),
        Signal::Cont => cash_win32::console::resume_process(raw).map(|_| ()),

        // D13: CTRL_C_EVENT cannot be delivered to a specific process group —
        // GenerateConsoleCtrlEvent succeeds and the signal is never received — so
        // targeted delivery must use CTRL_BREAK_EVENT.
        Signal::Int => cash_win32::console::interrupt_process_group(raw),

        // D21: TERM and friends ask first. The caller escalates if the target ignores
        // it; KILL does not ask at all, matching POSIX where it cannot be caught.
        Signal::Term | Signal::Hup | Signal::Quit => {
            cash_win32::console::interrupt_process_group(raw)
        }
        // D22: reap the whole tree when this pid roots one, falling back to the single
        // process otherwise. That is what makes `kill %1` reap a pipeline's descendants
        // rather than orphaning them.
        Signal::Kill => match cash_win32::jobreg::terminate_tree(raw) {
            Ok(true) => Ok(()),
            Ok(false) => cash_win32::process::terminate(raw),
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

/// Narrow a platform process id to the `DWORD` Win32 expects.
fn pid_as_u32(pid: sys::process::ProcessId) -> u32 {
    u32::try_from(pid).unwrap_or(0)
}
