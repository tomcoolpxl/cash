//! Win32 semantics layer for cash.
//!
//! This crate holds the parts of cash that Windows makes genuinely different from
//! POSIX: process containment, signals, path canonicalisation and console handling.
//!
//! It deliberately lives *outside* `vendor/brush`. Writing this code directly into the
//! vendored fork would make every file of it permanent divergence; keeping it in its own
//! crate means the fork only needs thin wiring, which is what D9's diff discipline
//! depends on. It also leaves the door open to upstreaming the crate later, per §6.1.
//!
//! Decisions implemented here are referenced by their spec identifiers (`D6`, `D13`, …);
//! see `spec.md` at the repository root.

#[cfg(not(windows))]
compile_error!("cash builds only on Windows (spec D43)");

pub mod children;
pub mod cmd;
pub mod cmdline;
pub mod conin;
// A pseudo console and a screen to read it into, for the tests of programs on a terminal;
// not part of cash (W32-12).
#[cfg(any(test, feature = "pseudo-console"))]
pub mod conpty;
pub mod console;
pub mod ctrl_z;
pub mod devices;
pub mod endless;
pub mod env;
pub mod exit;
pub mod fs;
mod handle;
/// ICMP echo through the IP Helper API, for `ping`.
pub mod icmp;
mod imports;
pub mod job;
pub mod jobreg;
pub mod locale;
pub mod msys;
/// The machine's TCP/UDP sockets and their owning processes.
pub mod net;
pub mod path;
pub mod pipe;
pub mod poll;
pub mod process;
pub mod resolve;
/// Which processes hold a file open, through the Restart Manager.
pub mod restart;
pub mod scoop;
pub mod session;
pub mod shellopen;
pub mod spawn;
pub mod stdio;
/// Asking one process to stop, then making it: D21's `TERM`.
pub mod stop;
/// What the machine is, asked directly — the numbers `coolfetch` prints.
pub mod sysinfo;
pub mod terminal;
pub mod text;
pub mod userpath;
#[cfg(any(test, feature = "pseudo-console"))]
pub mod vtscreen;
pub mod wide;

pub use job::{JobConfig, JobObject};
