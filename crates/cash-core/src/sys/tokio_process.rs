//! Process management utilities

pub(crate) type ProcessId = i32;
pub(crate) use tokio::process::Child;

// `kill_on_drop`: see `CreateOptions::kill_external_commands_on_drop` (false for ordinary shells).
pub(crate) fn spawn(command: std::process::Command, kill_on_drop: bool) -> std::io::Result<Child> {
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut cmd = command;
        cmd.creation_flags(0x0000_0004); // CREATE_SUSPENDED
        let mut tokio_cmd = tokio::process::Command::from(cmd);
        tokio_cmd.kill_on_drop(kill_on_drop);
        tokio_cmd
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut tokio_cmd = tokio::process::Command::from(command);
        tokio_cmd.kill_on_drop(kill_on_drop);
        tokio_cmd
    };

    let child = command.spawn()?;

    // cash (D6/D22): give the process its own nested job object so its descendants can
    // be reaped as a unit. Because it was created suspended, it cannot execute a single
    // instruction or fork before being contained in the job object.
    #[cfg(windows)]
    {
        if let Some(pid) = child.id() {
            cash_win32::jobreg::sweep();
            cash_win32::jobreg::contain(pid);
        }
        if let Some(handle) = child.raw_handle() {
            let _ = cash_win32::process::resume_process(handle.cast());
        }
    }

    Ok(child)
}
