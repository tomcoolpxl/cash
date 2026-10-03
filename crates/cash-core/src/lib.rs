//! Core implementation of the brush shell. Implements the shell's abstraction, its interpreter, and
//! various facilities used internally by the shell.

pub mod abbreviations;
pub mod arithmetic;
mod bash_synopses;
mod braceexpansion;
pub mod builtins;
pub mod callstack;
pub mod commands;
pub mod completion;
pub mod dirhistory;
pub mod env;
pub mod error;
pub mod escape;
pub mod expansion;
mod extendedtests;
pub mod extensions;
pub mod functions;
mod globsort;
pub mod history;
pub mod int_utils;
pub mod interfaces;
mod interp;
mod ioutils;
pub mod jobs;
mod keywords;
pub mod namedoptions;
pub mod openfiles;
pub mod options;
pub mod pathcache;
pub mod pathindex;
pub mod pathsearch;
pub mod patterns;
pub mod processes;
mod prompt;
mod random;
mod regex;
pub mod results;
mod shebang_env;
mod shell;
pub mod sourceinfo;
pub mod sys;
pub mod terminal;
pub mod tests;
pub mod timing;

/// The numeric identity `$UID`, `$EUID` and the `id` builtin share.
///
/// On Windows this is 0 in an elevated shell (the root convention that
/// `[ "$EUID" -eq 0 ]` and `[ "$(id -u)" -eq 0 ]` test for) and the account's RID
/// otherwise; spec §4 row 20.
pub mod identity {
    /// The effective user id.
    #[must_use]
    pub fn effective_uid() -> Option<u32> {
        crate::sys::users::get_effective_uid().ok()
    }

    /// The effective group id.
    #[must_use]
    pub fn effective_gid() -> Option<u32> {
        crate::sys::users::get_effective_gid().ok()
    }
}
/// The stack each thread that runs shell code reserves: the thread that drives the
/// shell, the runtime's workers and blocking threads, and a process substitution's thread.
///
/// Every level of a function call, `$(...)` or pipeline stage is several large futures
/// deep, and a stack overflow is an abort that no panic recovery can catch. Windows
/// reserves the address space and commits pages only as they are used, so a thread costs
/// no more memory for a large reservation; a 64-bit process has address space to spare.
pub const SHELL_THREAD_STACK_SIZE: usize = 256 * 1024 * 1024;

pub mod trace_categories;
pub mod traps;
pub mod variables;
mod wellknownvars;

/// Re-export parser types used in core definitions.
pub mod parser {
    pub use cash_parser::{
        BindingParseError, ParseError, ParserImpl, SourcePosition, SourcePositionOffset,
        SourceSpan, TestCommandParseError, WordParseError, ast,
    };
}

pub use commands::{CommandArg, ExecutionContext};
pub use error::{BuiltinError, Error, ErrorKind};
pub use extensions::ShellExtensions;
pub use interp::{ExecutionParameters, ProcessGroupPolicy, finish_output_substitutions};
pub use parser::{SourcePosition, SourcePositionOffset, SourceSpan};
pub use results::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, ExecutionSpawnResult};
pub use shell::{
    CreateOptions, ProfileLoadBehavior, RcLoadBehavior, SavedCommandStatus, Shell, ShellBuilder,
    ShellBuilderState, ShellFd, ShellState,
};
pub use sourceinfo::SourceInfo;
pub use variables::{ShellValue, ShellVariable};
