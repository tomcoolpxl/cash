pub use crate::sys::stubs::async_pipe;
pub use crate::sys::stubs::commands;
pub(crate) mod env;
pub use crate::sys::stubs::fd;
pub(crate) mod fs;
pub use crate::sys::stubs::input;
pub(crate) mod network;
// cash: Windows has no poll(2), but it does have PeekNamedPipe and WaitForSingleObject,
// which between them cover every handle kind `read -t` cares about.
pub mod poll;
pub use crate::sys::stubs::resource;

/// Signal processing utilities — cash's own (D13, D19, D21, D22), replacing the stub
/// whose `Signal` was an empty enum.
#[path = "windows/signal.rs"]
pub mod signal;

#[path = "windows/terminal.rs"]
pub mod terminal;
pub use crate::sys::tokio_process as process;
pub(crate) mod users;

/// Render a path string in cash's canonical spelling: drive letter, forward slashes (D3).
pub(crate) fn render_canonical_path(path: &str) -> String {
    cash_win32::path::render(std::path::Path::new(path))
}

/// Platform-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {}
