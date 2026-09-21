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

#![cfg(windows)]

pub mod cmd;
pub mod console;
pub mod env;
pub mod exit;
pub mod job;
pub mod path;
pub mod process;
pub mod resolve;
pub mod spawn;
pub mod text;

pub use job::{JobConfig, JobObject};
