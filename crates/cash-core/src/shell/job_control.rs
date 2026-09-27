//! Job management for shell instances.

use std::io::Write;

use crate::{error, extensions};

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Whether finished background jobs may be reported between commands: in the
    /// interactive shell itself, with job control on, and not while a file is being
    /// sourced (Bash 5.3 reports those when the file is done).
    pub(crate) fn may_report_jobs_now(&self) -> bool {
        self.options().interactive
            && self.options().enable_job_control
            && !self.is_subshell()
            && !self.in_sourced_script()
    }

    /// Checks for completed jobs in the shell, reporting any changes found.
    pub fn check_for_completed_jobs(&mut self) -> Result<(), error::Error> {
        let results = self.jobs.poll()?;

        if self.options.enable_job_control {
            for (job, _result) in results {
                writeln!(self.stderr(), "{job}")?;
            }
        }

        Ok(())
    }
}
