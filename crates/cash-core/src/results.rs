//! Encapsulation of execution results.

use crate::{error, processes, sys};

/// Represents the result of executing a command or similar item.
#[derive(Default)]
pub struct ExecutionResult {
    /// The control flow transition to apply after execution.
    pub next_control_flow: ExecutionControlFlow,
    /// The exit code resulting from execution.
    pub exit_code: ExecutionExitCode,
}

impl ExecutionResult {
    /// Returns a new `ExecutionResult` with the given exit code.
    ///
    /// # Arguments
    ///
    /// * `exit_code` - The exit code of the command.
    pub fn new(exit_code: u8) -> Self {
        Self {
            exit_code: exit_code.into(),
            ..Self::default()
        }
    }

    /// Returns a new `ExecutionResult` reflecting a process that was stopped.
    pub fn stopped() -> Self {
        // TODO(jobs): Decide how to sort this out in a platform-independent way.
        const SIGTSTP: std::os::raw::c_int = 20;

        #[expect(clippy::cast_possible_truncation)]
        Self::new(128 + SIGTSTP as u8)
    }

    /// Returns a new `ExecutionResult` with an exit code of 0.
    pub const fn success() -> Self {
        Self {
            next_control_flow: ExecutionControlFlow::Normal,
            exit_code: ExecutionExitCode::Success,
        }
    }

    /// Returns a new `ExecutionResult` with a general error exit code.
    pub const fn general_error() -> Self {
        Self {
            next_control_flow: ExecutionControlFlow::Normal,
            exit_code: ExecutionExitCode::GeneralError,
        }
    }

    /// Returns whether the command was successful.
    pub const fn is_success(&self) -> bool {
        self.exit_code.is_success()
    }

    /// Returns whether the execution result indicates normal control flow.
    /// Returns `false` if there is any control flow transition requested.
    pub const fn is_normal_flow(&self) -> bool {
        matches!(self.next_control_flow, ExecutionControlFlow::Normal)
    }

    /// Returns whether the execution result indicates a loop break.
    pub const fn is_break(&self) -> bool {
        matches!(
            self.next_control_flow,
            ExecutionControlFlow::BreakLoop { .. }
        )
    }

    /// Returns whether the execution result indicates a loop continue.
    pub const fn is_continue(&self) -> bool {
        matches!(
            self.next_control_flow,
            ExecutionControlFlow::ContinueLoop { .. }
        )
    }

    /// Returns whether the execution result indicates an early return
    /// from a function or script, or an exit from the shell. Returns `false`
    /// otherwise, including loop breaks or continues.
    pub const fn is_return_or_exit(&self) -> bool {
        matches!(
            self.next_control_flow,
            ExecutionControlFlow::ReturnFromFunctionOrScript | ExecutionControlFlow::ExitShell
        )
    }

    /// Returns whether the execution result indicates an exit from the shell.
    pub const fn is_exit(&self) -> bool {
        matches!(self.next_control_flow, ExecutionControlFlow::ExitShell)
    }
}

impl From<ExecutionExitCode> for ExecutionResult {
    fn from(exit_code: ExecutionExitCode) -> Self {
        Self {
            next_control_flow: ExecutionControlFlow::Normal,
            exit_code,
        }
    }
}

impl From<ExecutionWaitResult> for ExecutionResult {
    fn from(wait_result: ExecutionWaitResult) -> Self {
        match wait_result {
            ExecutionWaitResult::Completed(result) => result,
            // TODO(jobs): We need to job-manage the stopped process.
            ExecutionWaitResult::Stopped(..) => Self::stopped(),
        }
    }
}

impl From<std::process::Output> for ExecutionResult {
    fn from(output: std::process::Output) -> Self {
        if let Some(code) = output.status.code() {
            // cash (D15): truncating to the low byte is right for ordinary exit codes
            // but unsafe on its own. A process killed by an exception reports an
            // NTSTATUS, and `0xC0000100 & 0xFF` is **zero** — a crash indistinguishable
            // from success, which would silently pass an `&&` chain. Known crash classes
            // map to bash's `128 + n` instead, so an access violation is `139`, exactly
            // what a segfault yields on Linux.
            #[expect(clippy::cast_sign_loss)]
            return Self::new(cash_win32::exit::from_windows(code as u32));
        }

        tracing::error!("unhandled process exit");
        Self::new(127)
    }
}

/// Represents an exit code from execution.
#[derive(Clone, Copy, Default)]
pub enum ExecutionExitCode {
    /// Indicates successful execution.
    #[default]
    Success,
    /// Indicates a general error.
    GeneralError,
    /// Indicates invalid usage.
    InvalidUsage,
    /// Cannot execute the command.
    CannotExecute,
    /// Indicates a command or similar item was not found.
    NotFound,
    /// Indicates execution was interrupted.
    Interrupted,
    /// Indicates a broken pipe (SIGPIPE) was encountered.
    BrokenPipe,
    /// Indicates unimplemented functionality was encountered.
    Unimplemented,
    /// A custom exit code.
    Custom(u8),
}

impl ExecutionExitCode {
    /// Returns whether the exit code indicates success.
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }
}

impl From<u8> for ExecutionExitCode {
    fn from(code: u8) -> Self {
        match code {
            0 => Self::Success,
            1 => Self::GeneralError,
            2 => Self::InvalidUsage,
            99 => Self::Unimplemented,
            126 => Self::CannotExecute,
            127 => Self::NotFound,
            130 => Self::Interrupted,
            141 => Self::BrokenPipe,
            code => Self::Custom(code),
        }
    }
}

impl From<ExecutionExitCode> for u8 {
    fn from(code: ExecutionExitCode) -> Self {
        Self::from(&code)
    }
}

impl From<&ExecutionExitCode> for u8 {
    fn from(code: &ExecutionExitCode) -> Self {
        match code {
            ExecutionExitCode::Success => 0,
            ExecutionExitCode::GeneralError => 1,
            ExecutionExitCode::InvalidUsage => 2,
            ExecutionExitCode::Unimplemented => 99,
            ExecutionExitCode::CannotExecute => 126,
            ExecutionExitCode::NotFound => 127,
            ExecutionExitCode::Interrupted => 130,
            ExecutionExitCode::BrokenPipe => 141,
            ExecutionExitCode::Custom(code) => *code,
        }
    }
}

/// Represents a control flow transition to apply.
#[derive(Clone, Copy, Default)]
pub enum ExecutionControlFlow {
    /// Continue normal execution.
    #[default]
    Normal,
    /// Break out of an enclosing loop.
    BreakLoop {
        /// Identifies which level of nested loops to break out of. 0 indicates the innermost loop,
        /// 1 indicates the next outer loop, and so on.
        levels: usize,
    },
    /// Continue to the next iteration of an enclosing loop.
    ContinueLoop {
        /// Identifies which level of nested loops to continue. 0 indicates the innermost loop,
        /// 1 indicates the next outer loop, and so on.
        levels: usize,
    },
    /// Return from the current function or script.
    ReturnFromFunctionOrScript,
    /// Exit the shell.
    ExitShell,
}

impl ExecutionControlFlow {
    /// Attempts to decrement the loop levels for `BreakLoop` or `ContinueLoop`.
    /// If the levels reach zero, transitions to `Normal`. If the control flow is not
    /// a loop break or continue, no changes are made.
    #[must_use]
    pub const fn try_decrement_loop_levels(&self) -> Self {
        match self {
            Self::BreakLoop { levels: 0 } | Self::ContinueLoop { levels: 0 } => Self::Normal,
            Self::BreakLoop { levels } => Self::BreakLoop {
                levels: *levels - 1,
            },
            Self::ContinueLoop { levels } => Self::ContinueLoop {
                levels: *levels - 1,
            },
            control_flow => *control_flow,
        }
    }
}

/// Represents the result of spawning an execution; captures both execution
/// that immediately returns as well as execution that starts a process
/// asynchronously.
pub enum ExecutionSpawnResult {
    /// Indicates that the execution completed.
    Completed(ExecutionResult),
    /// Indicates that a process was started and had not yet completed.
    StartedProcess(processes::ChildProcess),
    /// Indicates that a task was started to handle the execution asynchronously.
    StartedTask(tokio::task::JoinHandle<Result<ExecutionResult, error::Error>>),
}

impl From<ExecutionResult> for ExecutionSpawnResult {
    fn from(result: ExecutionResult) -> Self {
        Self::Completed(result)
    }
}

impl ExecutionSpawnResult {
    /// Waits for the command to complete.
    pub async fn wait(self) -> Result<ExecutionWaitResult, error::Error> {
        Ok(self.wait_or_stop(false).await?.0)
    }

    /// Waits for the command to complete or, when `ctrl_z`, for the keyboard's Ctrl-Z to
    /// stop it (D19). Also says what the keyboard's Ctrl-C did to it meanwhile (D13).
    pub(crate) async fn wait_or_stop(
        self,
        ctrl_z: bool,
    ) -> Result<(ExecutionWaitResult, Interrupt), error::Error> {
        let result = match self {
            Self::StartedProcess(mut child) => {
                // Wait for the process to exit or for a relevant signal, whichever happens
                // first.
                match child.wait_or_stop(ctrl_z).await? {
                    processes::ProcessWaitResult::Completed(output) => {
                        let interrupt = Interrupt::of(&output, child.saw_ctrl_c());
                        (
                            ExecutionWaitResult::Completed(ExecutionResult::from(output)),
                            interrupt,
                        )
                    }
                    processes::ProcessWaitResult::Stopped => {
                        (ExecutionWaitResult::Stopped(child), Interrupt::None)
                    }
                }
            }
            Self::Completed(result) => (ExecutionWaitResult::Completed(result), Interrupt::None),
            Self::StartedTask(join_handle) => {
                let result = join_handle.await?;
                (ExecutionWaitResult::Completed(result?), Interrupt::None)
            }
        };

        Ok(result)
    }

    pub(crate) async fn poll(self) -> Result<ExecutionWaitResult, error::Error> {
        let result = match self {
            Self::StartedProcess(child) => {
                // cash (D19): stopped with the pipeline stage before it. Unix stops a job's
                // whole process group at once; Windows suspends each tree by hand.
                if let Some(pid) = child.pid() {
                    let _ = sys::signal::suspend_trees(&[pid]);
                }
                ExecutionWaitResult::Stopped(child)
            }
            Self::Completed(result) => ExecutionWaitResult::Completed(result),
            Self::StartedTask(join_handle) => {
                // TODO(jobs): This isn't right.
                let result = join_handle.await?;
                ExecutionWaitResult::Completed(result?)
            }
        };

        Ok(result)
    }
}

/// What the keyboard's Ctrl-C did to a foreground program while the shell waited for it
/// (D13).
///
/// Windows sends the event to every process on the console, the program and the shell
/// alike, and the program decides for itself what it does with it. What the shell then
/// does follows Bash, which looks at how the program ended: a script ends when its
/// command died of the interrupt, and goes on when the command took it in its stride (a
/// REPL that stays, `terraform apply` finishing its step).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Interrupt {
    /// None arrived.
    None,
    /// One arrived, and the program lived on or ended in a way of its own.
    Survived,
    /// The program died of it.
    Fatal,
}

impl Interrupt {
    /// What a program that ended with `output` made of the interrupt, given whether one
    /// arrived while the shell waited (`saw_ctrl_c`).
    ///
    /// A program that Ctrl-C ended exits with `STATUS_CONTROL_C_EXIT`, whether or not
    /// the shell heard the event itself. A shell (cash, Bash) that an interrupt ended
    /// says so as shells do, with status 130; that counts when the interrupt was heard
    /// here, since 130 on its own is a status like any other.
    fn of(output: &std::process::Output, saw_ctrl_c: bool) -> Self {
        #[expect(clippy::cast_sign_loss, reason = "an exit code is a DWORD")]
        let code = output.status.code().map(|code| code as u32);
        let interrupted_shell =
            saw_ctrl_c && code.is_some_and(|code| cash_win32::exit::from_windows(code) == 130);
        if code == Some(cash_win32::exit::CONTROL_C_EXIT) || interrupted_shell {
            Self::Fatal
        } else if saw_ctrl_c {
            Self::Survived
        } else {
            Self::None
        }
    }

    /// The graver of two: what a pipeline made of the interrupt, from what its programs
    /// did.
    pub(crate) const fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::Fatal, _) | (_, Self::Fatal) => Self::Fatal,
            (Self::Survived, _) | (_, Self::Survived) => Self::Survived,
            (Self::None, Self::None) => Self::None,
        }
    }
}

/// Represents the result of waiting for an execution to complete.
pub enum ExecutionWaitResult {
    /// Indicates that the execution completed.
    Completed(ExecutionResult),
    /// Indicates that the execution was stopped.
    Stopped(processes::ChildProcess),
}
