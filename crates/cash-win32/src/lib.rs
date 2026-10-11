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

/// Accounts and their rights, for `sudo`, `su` and `cash doctor`.
pub mod account;
pub mod children;
// The clipboard's text, for `pbcopy` and `pbpaste`.
pub mod clipboard;
pub mod cmd;
pub mod cmdline;
// Windows' code pages, converted through, for `iconv`.
pub mod codepage;
pub mod conin;
/// Waking a thread that waits for a line at the console, for `nc`.
pub mod conwake;
// A pseudo console and a screen to read it into, for the tests of programs on a terminal;
// not part of cash (W32-12).
#[cfg(any(test, feature = "pseudo-console"))]
pub mod conpty;
pub mod console;
pub mod ctrl_z;
pub mod devices;
// `sudo` elevating in this terminal by itself: the caller and the elevated cash.
pub mod elevate;
pub mod endless;
pub mod env;
pub mod exit;
// Byte-range locks on an open file, for `flock`.
pub mod filelock;
pub mod fold;
pub mod fs;
mod handle;
pub mod handles;
/// ICMP echo through the IP Helper API, for `ping`.
pub mod icmp;
mod imports;
/// Scoop's, the installer's or a portable `cash.exe`, told by its path.
pub mod install_layout;
pub mod job;
pub mod jobreg;
/// Directory junctions, for the installer's `current` folder.
pub mod junction;
pub mod locale;
// Physical memory, the commit charge and the page files, for `free`.
pub mod memory;
pub mod msys;
/// The machine's TCP/UDP sockets and their owning processes.
pub mod net;
pub mod path;
pub mod pipe;
pub mod poll;
// A process's priority class, and the niceness it stands for, for `nice` and `renice`.
pub mod priority;
pub mod process;
pub mod reparse;
pub mod resolve;
/// Which processes hold a file open, through the Restart Manager.
pub mod restart;
pub mod scoop;
pub mod session;
pub mod shellopen;
/// Synchronous sockets, for the `/dev/tcp` and `/dev/udp` descriptors a child can use.
pub mod sockets;
pub mod spawn;
pub mod stdio;
/// Asking one process to stop, then making it: D21's `TERM`.
pub mod stop;
/// What the machine is, asked directly — the numbers `coolfetch` prints.
pub mod sysinfo;
pub mod terminal;
// A terminal's settings as `stty` sees them, over the console's four modes.
pub mod termios;
pub mod text;
pub mod unix;
pub mod userpath;
#[cfg(any(test, feature = "pseudo-console"))]
pub mod vtscreen;
pub mod wide;
// Letting another account onto this desktop for `sudo -u USER` with a windowed program.
pub mod winstation;

pub use job::{JobConfig, JobObject};
