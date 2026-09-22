//! Process management utilities

pub(crate) type ProcessId = i32;
pub(crate) use tokio::process::Child;

// `kill_on_drop`: see `CreateOptions::kill_external_commands_on_drop` (false for ordinary shells).
pub(crate) fn spawn(command: std::process::Command, kill_on_drop: bool) -> std::io::Result<Child> {
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(kill_on_drop);
    let child = command.spawn()?;

    // cash (D6/D22): give the process its own nested job object so its descendants can
    // be reaped as a unit. Without this, `kill %1` reaped only the process cash spawned
    // directly and left the grandchildren orphaned until the session job tore down.
    //
    // This is the post-spawn assignment, with the race §6 documents: tokio owns process
    // creation, so the CREATE_SUSPENDED path `cash_win32::spawn` uses is unavailable
    // here. The window is narrow, and the session job still catches anything through it.
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        cash_win32::jobreg::sweep();
        cash_win32::jobreg::contain(pid);
    }

    Ok(child)
}
