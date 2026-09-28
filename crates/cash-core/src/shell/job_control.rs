//! Job management for shell instances.

use std::io::Write;

use crate::{ExecutionParameters, error, extensions, sys, traps};

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Whether finished background jobs may be reported between commands: in the
    /// interactive shell itself, with job control on, and not while a file is being
    /// sourced (Bash 5.3 reports those when the file is done). In POSIX mode only the
    /// prompt reports them, as POSIX specifies and Bash 5.3 does (its item 1.qq).
    pub(crate) fn may_report_jobs_now(&self) -> bool {
        self.options().interactive
            && self.options().enable_job_control
            && !self.options().posix_mode
            && !self.is_subshell()
            && !self.in_sourced_script()
    }

    /// Checks for completed jobs in the shell, reporting any changes found. A `CHLD` trap
    /// runs for them first, and the notices follow it, as in Bash 5.3 (its item 1.ii).
    pub async fn check_for_completed_jobs(
        &mut self,
        params: &ExecutionParameters,
    ) -> Result<(), error::Error> {
        let results = self.jobs.poll()?;

        self.run_pending_chld_traps(params).await;

        if self.options.enable_job_control {
            for (job, _result) in results {
                writeln!(self.stderr(), "{job}")?;
            }
        }

        Ok(())
    }

    /// After a foreground command that ran processes: reports the background jobs that
    /// finished meanwhile where that is allowed ([`Self::may_report_jobs_now`]), and runs a
    /// `CHLD` trap for the children reaped.
    pub(crate) async fn after_foreground_children(
        &mut self,
        params: &ExecutionParameters,
    ) -> Result<(), error::Error> {
        if self.may_report_jobs_now() {
            return self.check_for_completed_jobs(params).await;
        }
        if self.chld_trap().is_some() {
            // A script reaps finished background jobs here only for a CHLD trap to
            // count them; at a prompt they wait for it, to be reported there.
            if !self.options().interactive {
                let _ = self.jobs.poll()?;
            }
            self.run_pending_chld_traps(params).await;
        }
        Ok(())
    }

    /// The `CHLD` trap, when the platform has the signal and a handler is set for it.
    fn chld_trap(&self) -> Option<traps::TrapSignal> {
        let chld = traps::TrapSignal::Signal(sys::signal::CHLD?);
        self.traps().handles(chld).then_some(chld)
    }

    /// Runs the `CHLD` trap once for each child reaped since it last ran: each process
    /// of a foreground command and each background job (cash's `CHLD`, see
    /// `sys::signal`). Children the handler itself starts and reaps are not counted, or a
    /// handler that runs a command would set itself off without end.
    pub async fn run_pending_chld_traps(&mut self, params: &ExecutionParameters) {
        let reaped = self.jobs.take_children_reaped();
        // A background job or a subshell runs in a copy of the shell; its children are
        // its own, and Bash resets a subshell's CHLD trap. Only the shell itself runs it,
        // once per job, when it reaps the job.
        if reaped == 0 || self.is_subshell() {
            return;
        }
        let Some(chld) = self.chld_trap() else {
            return;
        };
        for _ in 0..reaped {
            let _ = self.invoke_trap_handler(chld, params).await;
        }
        let _ = self.jobs.take_children_reaped();
    }
}
