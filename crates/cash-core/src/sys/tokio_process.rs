//! Process management utilities

pub(crate) type ProcessId = i32;
pub(crate) use tokio::process::Child;

// `kill_on_drop`: see `CreateOptions::kill_external_commands_on_drop` (false for ordinary shells).
// `new_group`: on Windows, start the process as the leader of a process group of its own
// (D13): the keyboard's Ctrl-C then passes it by, and a Ctrl-Break can be aimed at it.
pub(crate) fn spawn(
    command: std::process::Command,
    kill_on_drop: bool,
    new_group: bool,
) -> std::io::Result<Child> {
    let mut command = {
        use std::os::windows::process::CommandExt;
        const CREATE_SUSPENDED: u32 = 0x0000_0004;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        let mut cmd = command;
        let group = if new_group {
            CREATE_NEW_PROCESS_GROUP
        } else {
            0
        };
        cmd.creation_flags(CREATE_SUSPENDED | group);
        let mut tokio_cmd = tokio::process::Command::from(cmd);
        tokio_cmd.kill_on_drop(kill_on_drop);
        tokio_cmd
    };

    let child = command.spawn()?;
    // A program on the console may leave it as the prompt cannot use it; the prompt puts
    // it back (`repair_before_prompt`).
    cash_win32::console::note_program_started();

    // cash (D6/D22): give the process its own nested job object so its descendants can
    // be reaped as a unit. Because it was created suspended, it cannot execute a single
    // instruction or fork before being contained in the job object.
    if let Some(pid) = child.id() {
        cash_win32::jobreg::sweep();
        cash_win32::jobreg::contain(pid);
    }
    if let Some(handle) = child.raw_handle() {
        let _ = cash_win32::process::resume_process(handle.cast());
    }

    Ok(child)
}
