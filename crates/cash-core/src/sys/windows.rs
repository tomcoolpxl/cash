pub use crate::sys::stubs::async_pipe;
pub use crate::sys::stubs::commands;
pub(crate) mod env;
pub use crate::sys::stubs::fd;
pub(crate) mod fs;
pub use crate::sys::stubs::input;
pub(crate) mod network;
pub use crate::sys::stubs::poll;
pub use crate::sys::stubs::resource;

/// Signal processing utilities
pub mod signal {
    pub(crate) use crate::sys::stubs::signal::*;
    pub(crate) use tokio::signal::ctrl_c as await_ctrl_c;
}

pub use crate::sys::stubs::terminal;
pub use crate::sys::tokio_process as process;
pub(crate) mod users;


/// Render a path string in cash's canonical spelling: drive letter, forward slashes (D3).
pub(crate) fn render_canonical_path(path: &str) -> String {
    cash_win32::path::render(std::path::Path::new(path))
}

/// Platform-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {}
