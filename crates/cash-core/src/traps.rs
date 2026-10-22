//! Facilities for configuring trap handlers.

use std::str::FromStr;
use std::{collections::HashMap, fmt::Display};

use itertools::Itertools as _;

use crate::{error, sys};

/// Signals that copies of the shell sent to its own process id (`( kill -USR1 $$ ) &`),
/// oldest first. A job, a subshell or a substitution runs inside the shell's process, so
/// the signal is left here for the shell, which takes it after the pipeline it runs and in
/// `wait` ([`crate::Shell::take_signals_sent`]), as Bash takes one a child of its sent it.
/// The copy took it itself: what its trap set was lost with the copy, and `kill $$` from a
/// watchdog job ended the job, not the script.
static SENT_TO_SHELL: std::sync::Mutex<Vec<sys::signal::Signal>> =
    std::sync::Mutex::new(Vec::new());
/// Whether [`SENT_TO_SHELL`] holds any, for the check after every pipeline.
static ANY_SENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Wakes a `wait` once a signal is sent ([`signal_sent_to_shell`]).
static SENT_NOTICE: std::sync::LazyLock<tokio::sync::Notify> =
    std::sync::LazyLock::new(tokio::sync::Notify::new);

/// Whether a signal sent to the shell ends it ([`send_to_shell`]): the shell stops waiting
/// for its foreground command, which goes on, as Bash, ended at once by a signal with no
/// trap, leaves it. A watchdog job's `kill $$` waited for the command it was to end.
static FATAL_SENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Wakes what waits for a foreground command once [`FATAL_SENT`] is set.
static FATAL_NOTICE: std::sync::LazyLock<tokio::sync::Notify> =
    std::sync::LazyLock::new(tokio::sync::Notify::new);

/// Leaves `signal`, which a copy of the shell sent the shell's own process id, for the
/// shell ([`SENT_TO_SHELL`]); `ends_it` when no trap takes it and it ends the shell.
pub(crate) fn send_to_shell(signal: sys::signal::Signal, ends_it: bool) {
    SENT_TO_SHELL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(signal);
    ANY_SENT.store(true, std::sync::atomic::Ordering::SeqCst);
    SENT_NOTICE.notify_one();
    if ends_it {
        FATAL_SENT.store(true, std::sync::atomic::Ordering::SeqCst);
        FATAL_NOTICE.notify_waiters();
    }
}

/// Whether a signal that ends the shell was sent it and not yet taken: a builtin that
/// asks its stop token stops ([`crate::commands::StopToken::requested`]).
pub(crate) fn fatal_signal_pending() -> bool {
    FATAL_SENT.load(std::sync::atomic::Ordering::SeqCst)
}

/// Waits until a signal that ends the shell is sent it ([`FATAL_SENT`]).
pub(crate) async fn fatal_signal_sent() {
    loop {
        let notified = FATAL_NOTICE.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if fatal_signal_pending() {
            return;
        }
        notified.await;
    }
}

/// Whether a signal with no trap ends the shell, as
/// [`crate::Shell::take_signals_sent`] acts on it: `KILL` always; at a prompt, neither
/// `INT` nor `TERM`; never one that leaves a process running, nor `QUIT`.
pub(crate) fn default_ends_shell(signal: sys::signal::Signal, at_prompt: bool) -> bool {
    let name = signal.as_str();
    name == "KILL"
        || !(signal.leaves_the_shell_running()
            || name == "QUIT"
            || at_prompt && matches!(name, "INT" | "TERM"))
}

/// Waits until a copy of the shell has sent it a signal ([`send_to_shell`]).
pub async fn signal_sent_to_shell() {
    while !signals_sent_to_shell() {
        SENT_NOTICE.notified().await;
    }
}

/// Whether a copy of the shell has sent it a signal not yet taken.
pub(crate) fn signals_sent_to_shell() -> bool {
    ANY_SENT.load(std::sync::atomic::Ordering::SeqCst)
}

/// Takes the signals copies of the shell sent it, oldest first.
pub(crate) fn take_signals_sent_to_shell() -> Vec<sys::signal::Signal> {
    let mut sent = SENT_TO_SHELL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    ANY_SENT.store(false, std::sync::atomic::Ordering::SeqCst);
    FATAL_SENT.store(false, std::sync::atomic::Ordering::SeqCst);
    std::mem::take(&mut *sent)
}

/// Type of signal that can be trapped in the shell.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TrapSignal {
    /// A system signal.
    Signal(sys::signal::Signal),
    /// The `DEBUG` trap.
    Debug,
    /// The `ERR` trap.
    Err,
    /// The `EXIT` trap.
    Exit,
    /// The `RETURN` trp.
    Return,
}

#[cfg(feature = "serde")]
impl serde::Serialize for TrapSignal {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for TrapSignal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::try_from(s.as_str()).map_err(serde::de::Error::custom)
    }
}

impl Display for TrapSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TrapSignal {
    /// Returns all possible values of [`TrapSignal`].
    pub fn iterator() -> impl Iterator<Item = Self> {
        const SIGNALS: &[TrapSignal] = &[TrapSignal::Debug, TrapSignal::Err, TrapSignal::Exit];

        let iter = itertools::chain!(
            SIGNALS.iter().copied(),
            sys::signal::Signal::iterator().map(TrapSignal::Signal)
        );

        iter
    }

    /// Converts [`TrapSignal`] into its corresponding signal name as a [`&'static str`](str)
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Signal(s) => s.as_str(),
            Self::Debug => "DEBUG",
            Self::Err => "ERR",
            Self::Exit => "EXIT",
            Self::Return => "RETURN",
        }
    }
}

/// Formats [`Iterator<Item = TrapSignal>`](TrapSignal)  to the provided writer.
///
/// # Arguments
///
/// * `f` - Any type that implements [`std::io::Write`].
/// * `it` - An iterator over the signals that will be formatted into the `f`.
pub fn format_signals(
    mut f: impl std::io::Write,
    it: impl Iterator<Item = TrapSignal>,
) -> Result<(), error::Error> {
    // Bash's `display_signal_list`: ` 1) SIGHUP`, a tab after each and a newline after
    // every fifth, and the last line ended. It was `1) HUP` a line, with no newline at
    // the end (BI-15).
    // Signals only, from 1, named `SIGHUP`: `trap -l` listed `0) EXIT`.
    let signals = it
        .filter_map(|s| i32::try_from(s).ok().map(|n| (s, n)))
        .filter(|&(_, number)| number > 0)
        .sorted_by(|a, b| Ord::cmp(&a.1, &b.1));
    let mut column = 0;
    for (signal, number) in signals {
        let name = signal.as_str();
        let sig = if name.starts_with("SIG") { "" } else { "SIG" };
        write!(f, "{number:2}) {sig}{name}")?;
        column += 1;
        if column < 5 {
            write!(f, "\t")?;
        } else {
            writeln!(f)?;
            column = 0;
        }
    }
    if column != 0 {
        writeln!(f)?;
    }
    Ok(())
}

// implement s.parse::<TrapSignal>()
impl FromStr for TrapSignal {
    type Err = error::Error;
    fn from_str(s: &str) -> Result<Self, <Self as FromStr>::Err> {
        if let Ok(n) = s.parse::<i32>() {
            Self::try_from(n)
        } else {
            Self::try_from(s)
        }
    }
}

// from a signal number
impl TryFrom<i32> for TrapSignal {
    type Error = error::Error;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        // NOTE: DEBUG and ERR are real-time signals, defined based on NSIG or SIGRTMAX (is not
        // available on bsd-like systems),
        // and don't have persistent numbers across platforms, so we skip them here.
        Ok(match value {
            0 => Self::Exit,
            value => Self::Signal(
                sys::signal::Signal::try_from(value)
                    .map_err(|_| error::ErrorKind::InvalidSignal(value.to_string()))?,
            ),
        })
    }
}

// from a signal name
impl TryFrom<&str> for TrapSignal {
    type Error = error::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        #[allow(unused_mut, reason = "only mutated on some platforms")]
        let mut s = value.to_ascii_uppercase();

        Ok(match s.as_str() {
            "DEBUG" => Self::Debug,
            "ERR" => Self::Err,
            "EXIT" => Self::Exit,
            "RETURN" => Self::Return,
            _ => {
                // Bash compatibility:
                // support for signal names without the `SIG` prefix, for example `HUP` -> `SIGHUP`
                if !s.starts_with("SIG") {
                    s.insert_str(0, "SIG");
                }
                sys::signal::Signal::from_str(s.as_str())
                    .map(TrapSignal::Signal)
                    .map_err(|_| error::ErrorKind::InvalidSignal(value.into()))?
            }
        })
    }
}

impl TrapSignal {
    /// The number Bash reports in `$BASH_TRAPSIG` while this trap runs.
    ///
    /// Signals use their own numbers and `EXIT` is 0. Bash numbers its pseudo-signals
    /// from `NSIG`, which is 65 on Linux and Cygwin, so `DEBUG`, `ERR` and `RETURN`
    /// are 65, 66 and 67 — the values scripts written against Bash observe.
    pub const fn trap_number(self) -> i32 {
        const NSIG: i32 = 65;
        match self {
            Self::Signal(s) => s as i32,
            Self::Exit => 0,
            Self::Debug => NSIG,
            Self::Err => NSIG + 1,
            Self::Return => NSIG + 2,
        }
    }
}

/// Error type used when failing to convert a `TrapSignal` to a number.
#[derive(Debug, Clone, Copy)]
pub struct TrapSignalNumberError;

impl TryFrom<TrapSignal> for i32 {
    type Error = TrapSignalNumberError;
    fn try_from(value: TrapSignal) -> Result<Self, Self::Error> {
        Ok(match value {
            TrapSignal::Signal(s) => s as Self,
            TrapSignal::Exit => 0,
            _ => return Err(TrapSignalNumberError),
        })
    }
}

/// A handler for a trap signal.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrapHandler {
    /// The source text of the command to invoke.
    pub command: String,
    /// Source information for where the trap handler was defined.
    pub source_info: crate::SourceInfo,
    /// The depth of the copy of the shell that set the trap ([`crate::Shell::depth`]): a
    /// subshell runs the traps it set itself, and of its parent's, which the copy carries,
    /// only those Bash passes on (`set -E`, `set -T`); its `EXIT` trap, as it ends.
    pub depth: usize,
}

/// Configuration for trap handlers in the shell.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrapHandlerConfig {
    /// Registered handlers for traps; maps signal type to command.
    handlers: HashMap<TrapSignal, TrapHandler>,
}

impl TrapHandlerConfig {
    /// Iterates over the registered handlers for trap signals.
    pub fn iter_handlers(&self) -> impl Iterator<Item = (TrapSignal, &TrapHandler)> {
        self.handlers
            .iter()
            .map(|(signal, handler)| (*signal, handler))
    }

    /// Tries to find the handler associated with the given signal.
    ///
    /// # Arguments
    ///
    /// * `signal_type` - The type of signal to get the handler for.
    pub fn get_handler(&self, signal_type: TrapSignal) -> Option<&TrapHandler> {
        self.handlers.get(&signal_type)
    }

    /// Returns whether a handler is registered for the given signal.
    pub fn handles(&self, signal_type: TrapSignal) -> bool {
        self.handlers.contains_key(&signal_type)
    }

    /// Returns whether a handler is registered for any signal.
    pub fn any(&self) -> bool {
        !self.handlers.is_empty()
    }

    /// Registers a handler for a trap signal.
    ///
    /// # Arguments
    ///
    /// * `signal_type` - The type of signal to register a handler for.
    /// * `command` - The command to execute when the signal is trapped.
    /// * `source_info` - The source info for where the trap handler was defined.
    /// * `depth` - The depth of the copy of the shell setting it ([`TrapHandler::depth`]).
    pub fn register_handler(
        &mut self,
        signal_type: TrapSignal,
        command: String,
        source_info: crate::SourceInfo,
        depth: usize,
    ) {
        let _ = self.handlers.insert(
            signal_type,
            TrapHandler {
                command,
                source_info,
                depth,
            },
        );
    }

    /// Puts back a handler that was set aside, as it was.
    pub fn restore_handler(&mut self, signal_type: TrapSignal, handler: TrapHandler) {
        let _ = self.handlers.insert(signal_type, handler);
    }

    /// Removes handlers for a trap signal.
    ///
    /// # Arguments
    ///
    /// * `signal_type` - The type of signal to remove handlers for.
    pub fn remove_handlers(&mut self, signal_type: TrapSignal) {
        self.handlers.remove(&signal_type);
    }
}
