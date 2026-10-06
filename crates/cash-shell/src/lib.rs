//! Create for brush, an executable bash-compatible shell.

pub mod args;
pub mod bundled;
mod cashctl;
pub mod config;
pub mod entry;
mod error_formatter;
pub mod events;
// The one-time offer a portable cash makes at its first interactive prompt.
mod portable_offer;
mod productinfo;
