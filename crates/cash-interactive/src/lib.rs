//! Library implementing interactive command input and completion for the brush shell.

mod error;
pub use error::ShellError;

mod interactive_shell;
pub use interactive_shell::{InteractiveExecutionResult, InteractiveOptions, InteractiveShell};

mod zsh_hooks;
pub use zsh_hooks::init as init_zsh_style_hooks;

mod input_backend;
pub use input_backend::{InputBackend, InteractivePrompt, ReadResult};

mod options;
pub use options::UIOptions;

mod refs;
pub use refs::ShellRef;

mod completeness;
mod term_detection;
mod term_integration;

#[cfg(feature = "highlighting")]
pub mod highlighting;

#[cfg(feature = "completion")]
mod completion;

#[cfg(feature = "reedline")]
mod carapace;
#[cfg(feature = "reedline")]
mod history_words;

// Reedline-based shell
#[cfg(feature = "reedline")]
mod reedline;
#[cfg(feature = "reedline")]
pub use reedline::ReedlineInputBackend;

// Basic shell
#[cfg(feature = "basic")]
mod basic;
#[cfg(feature = "basic")]
pub use basic::BasicInputBackend;

// Minimal shell
#[cfg(feature = "minimal")]
mod minimal;
#[cfg(feature = "minimal")]
pub use minimal::MinimalInputBackend;
