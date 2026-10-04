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

    let mut child = command.spawn()?;
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
        let pid = child.id();
        let resumed = resume_with(
            || cash_win32::process::resume_process(handle.cast()),
            || pid.map_or(Ok(0), cash_win32::console::start_threads),
        );
        if let Err(error) = resumed {
            let _ = child.start_kill();
            return Err(std::io::Error::new(
                error.kind(),
                format!("the program could not be started: resuming it failed: {error}"),
            ));
        }
    }

    Ok(child)
}

/// Resumes a program created suspended: through `process` (`NtResumeProcess`), and if
/// that fails, through `threads`, which resumes its threads one by one with the
/// documented `ResumeThread` and says how many (D19). The result of the resume was not
/// looked at, so a program it failed for stayed suspended and the shell waited for it
/// for ever (`REVIEW_REPORT.md` EXE-14); the caller ends it instead.
fn resume_with(
    process: impl FnOnce() -> std::io::Result<()>,
    threads: impl FnOnce() -> std::io::Result<usize>,
) -> std::io::Result<()> {
    let Err(error) = process() else {
        return Ok(());
    };
    match threads() {
        Ok(resumed) if resumed > 0 => Ok(()),
        _ => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_resume_falls_back_to_the_threads_and_else_fails() {
        let failed = || Err(std::io::Error::other("NtResumeProcess failed"));
        assert!(resume_with(|| Ok(()), || unreachable!("not needed")).is_ok());
        assert!(resume_with(failed, || Ok(1)).is_ok());
        assert!(resume_with(failed, || Ok(0)).is_err());
        assert!(resume_with(failed, || Err(std::io::Error::other("no threads"))).is_err());
    }

    #[test]
    fn a_program_created_suspended_runs_once_its_threads_are_resumed() {
        use std::os::windows::process::CommandExt;
        const CREATE_SUSPENDED: u32 = 0x0000_0004;

        // process state: a test of the resume, in the test's own process.
        let mut child = std::process::Command::new("cmd")
            .args(["/c", "exit 7"])
            .creation_flags(CREATE_SUSPENDED)
            .spawn()
            .unwrap();
        assert!(cash_win32::console::start_threads(child.id()).unwrap() > 0);
        assert_eq!(child.wait().unwrap().code(), Some(7));
    }

    /// `cmd /c ping` that waits: a program with a child of its own.
    fn sleeper() -> std::process::Command {
        // process state: a test's child, with no shell whose state it should take.
        let mut command = std::process::Command::new("cmd");
        command.args(["/c", "ping -n 30 127.0.0.1 >nul"]);
        command
    }

    /// Whether `pid` still runs.
    fn alive(pid: u32) -> bool {
        cash_win32::process::is_pid_alive(pid)
    }

    /// The race spec §6 records: a program assigned to its job after it starts can start
    /// a child before then, outside the job. Created suspended, it is in its job when
    /// `spawn` returns, and so is everything it starts (D6). These were tests of
    /// `cash_win32::spawn::spawn`, which nothing ran (W32-11).
    #[tokio::test]
    async fn a_program_is_in_its_job_when_spawn_returns_and_so_are_its_children() {
        let mut child = spawn(sleeper(), true, false).unwrap();
        let pid = child.id().unwrap();
        assert!(
            cash_win32::jobreg::tree_pids(pid).contains(&pid),
            "{pid} was not in its job when spawn returned"
        );

        let started = std::time::Instant::now();
        let grandchild = loop {
            if let Some(&other) = cash_win32::jobreg::tree_pids(pid)
                .iter()
                .find(|&&p| p != pid)
            {
                break other;
            }
            assert!(
                started.elapsed() < std::time::Duration::from_secs(10),
                "ping never joined the job"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        };

        assert!(cash_win32::jobreg::terminate_tree(pid, 1).unwrap());
        let _ = child.wait().await;
        let started = std::time::Instant::now();
        while alive(grandchild) {
            assert!(
                started.elapsed() < std::time::Duration::from_secs(10),
                "{grandchild} outlived its job"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}
