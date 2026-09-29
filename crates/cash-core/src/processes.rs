//! Process management

use futures::FutureExt;

use crate::{error, sys};

/// A waitable future that will yield the results of a child process's execution.
pub(crate) type WaitableChildProcess = std::pin::Pin<
    Box<dyn futures::Future<Output = Result<std::process::Output, std::io::Error>> + Send + Sync>,
>;

/// Tracks a child process being awaited.
pub struct ChildProcess {
    /// A waitable future that will yield the results of a child process's execution.
    exec_future: WaitableChildProcess,
    /// If available, the process ID of the child.
    pid: Option<sys::process::ProcessId>,
    /// If available, the process group ID of the child.
    pgid: Option<sys::process::ProcessId>,
    /// The console as the process left it when Ctrl-Z stopped it, for `fg` to put back.
    console_at_stop: Option<cash_win32::console::ConsoleState>,
}

impl ChildProcess {
    /// Wraps a child process and its future.
    pub fn new(
        child: sys::process::Child,
        pid: Option<sys::process::ProcessId>,
        pgid: Option<sys::process::ProcessId>,
    ) -> Self {
        Self {
            exec_future: Box::pin(child.wait_with_output()),
            pid,
            pgid,
            console_at_stop: None,
        }
    }

    /// Takes the console state saved when Ctrl-Z stopped the process (D19).
    pub(crate) const fn take_console_at_stop(
        &mut self,
    ) -> Option<cash_win32::console::ConsoleState> {
        self.console_at_stop.take()
    }

    /// Returns the process's ID.
    pub const fn pid(&self) -> Option<sys::process::ProcessId> {
        self.pid
    }

    /// Returns the process's group ID.
    pub const fn pgid(&self) -> Option<sys::process::ProcessId> {
        self.pgid
    }

    /// Waits for the process to exit.
    pub async fn wait(&mut self) -> Result<ProcessWaitResult, error::Error> {
        self.wait_or_stop(false).await
    }

    /// Waits for the process to exit or, when `ctrl_z`, for the keyboard's Ctrl-Z to stop
    /// it.
    ///
    /// cash (D19): Windows stops nothing on Ctrl-Z. cash takes the key when no program
    /// reads it and suspends the process's tree itself. Only the interactive shell's
    /// foreground job listens, as only its job table can resume a stopped process.
    pub async fn wait_or_stop(&mut self, ctrl_z: bool) -> Result<ProcessWaitResult, error::Error> {
        let mut sigtstp = sys::signal::tstp_signal_listener(ctrl_z)?;
        #[allow(unused_mut, reason = "only mutated on some platforms")]
        let mut sigchld = sys::signal::chld_signal_listener()?;

        #[allow(clippy::ignored_unit_patterns)]
        loop {
            tokio::select! {
                output = &mut self.exec_future => {
                    break Ok(ProcessWaitResult::Completed(output?))
                },
                () = sigtstp.recv() => {
                    let pids: Vec<_> = self.pid.into_iter().collect();
                    self.console_at_stop = sys::signal::stop_for_ctrl_z(&pids);
                    break Ok(ProcessWaitResult::Stopped)
                },
                _ = sigchld.recv() => {
                    if sys::signal::poll_for_stopped_children()? {
                        break Ok(ProcessWaitResult::Stopped);
                    }
                },
                _ = sys::signal::await_ctrl_c() => {
                    // SIGINT got thrown. Handle it and continue looping. The child should
                    // have received it as well, and either handled it or ended up getting
                    // terminated (in which case we'll see the child exit).
                },
            }
        }
    }

    pub(crate) fn poll(&mut self) -> Option<Result<std::process::Output, error::Error>> {
        let checkable_future = &mut self.exec_future;
        checkable_future
            .now_or_never()
            .map(|result| result.map_err(Into::into))
    }
}

/// Represents the result of waiting for an executing process.
pub enum ProcessWaitResult {
    /// The process completed.
    Completed(std::process::Output),
    /// The process stopped and has not yet completed.
    Stopped,
}
