// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! What `--pid` watches. cash: the process is held through `cash_win32::process::Held`,
//! where uutils opened and waited on a handle of its own.

use cash_win32::process::Held;

pub type Pid = u32;

pub struct ProcessChecker {
    /// The process, or `None` when there was none to open: it is dead.
    process: Option<Held>,
}

impl ProcessChecker {
    pub fn new(process_id: Pid) -> Self {
        Self {
            process: Held::open(process_id),
        }
    }

    pub fn is_dead(&self) -> bool {
        self.process
            .as_ref()
            .is_none_or(|process| !process.is_running())
    }
}

pub fn supports_pid_checks(_pid: Pid) -> bool {
    true
}
