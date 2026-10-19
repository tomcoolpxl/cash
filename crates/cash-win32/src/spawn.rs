//! Process creation - **D6**, D26, D45.
//!
//! Every program cash starts while a shell runs is started through [`spawn`], [`status`]
//! or [`prepare`]: the shell's commands (`cash_core::sys::tokio_process::spawn`, `exec`,
//! `sudo`) and the programs its tools start (awk's `system()`, sed's `e`, sort's
//! `--compress-program`, split's `--filter`, the editor, completion). They take one lock,
//! so that the handles [`pass`] lends a child cash for its start reach that child alone.
//!
//! The shell starts a program created suspended, contained in a job of its own
//! (`crate::jobreg::contain`), then resumed, so it cannot run an instruction, let alone
//! start a child, outside its job (spec §6, D6). What is here besides: whether cash itself
//! runs in a job, for `cash doctor`, and `detach`'s start of a program that leaves cash's
//! job on purpose (D45).
//!
//! # Descriptors above 2 for a child cash (D26)
//!
//! Windows hands a program three standard handles and no more; a shell's descriptor 3 has
//! nowhere to go in a native program. A cash started by cash is given them all, as a
//! child of Bash inherits them (`sh -c 'echo >&3' 3>&1`). The parent asks for it with
//! [`pass`], naming its own handles; [`prepare`], under the lock, makes an inheritable
//! copy of each, names the copies in the child's environment, and closes them once the
//! child has inherited them. The child takes them with [`receive`] first thing in `main`
//! and hands them to its shell through [`received`].
//!
//! Windows lets a child inherit every inheritable handle of its parent's, not only those
//! meant for it, so a program started at the same instant would inherit the copies too:
//! a pipe's writing end held there leaves its reader waiting for the end of a stranger.
//! That is what the lock is for.
//!
//! It held a `CreateProcessW` spawn of its own, written when tokio was thought unable to
//! start a program suspended; nothing ran it but its tests (W32-11).

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io;
use std::os::windows::io::{AsRawHandle as _, BorrowedHandle, FromRawHandle as _, OwnedHandle};
use std::path::Path;
use std::process::{Child, Command, ExitStatus};
use std::sync::{Mutex, MutexGuard, PoisonError};

use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, FALSE, HANDLE, TRUE};
use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, GetCurrentProcess, PROCESS_INFORMATION,
    STARTUPINFOW,
};

use crate::wide::to_wide_nul;

/// What a descriptor passed to a child cash is open on, so that the child makes the same
/// kind of open file of the handle it inherits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passed {
    /// A file, a device or a console: read and written as a file.
    File,
    /// The reading end of a pipe.
    PipeReader,
    /// The writing end of a pipe.
    PipeWriter,
    /// A TCP connection.
    Tcp,
    /// A UDP socket connected to one peer.
    Udp,
}

impl Passed {
    /// The letter the kind is named by in the environment.
    const fn letter(self) -> char {
        match self {
            Self::File => 'f',
            Self::PipeReader => 'r',
            Self::PipeWriter => 'w',
            Self::Tcp => 't',
            Self::Udp => 'u',
        }
    }

    /// The kind `letter` names.
    const fn from_letter(letter: &str) -> Option<Self> {
        Some(match letter.as_bytes() {
            b"f" => Self::File,
            b"r" => Self::PipeReader,
            b"w" => Self::PipeWriter,
            b"t" => Self::Tcp,
            b"u" => Self::Udp,
            _ => return None,
        })
    }
}

/// The variable [`pass`] names the parent's own handles in, `FD:KIND:HANDLE,...`, which
/// [`prepare`] replaces with [`PASSED`].
const REQUEST: &str = "CASH_PASS_FDS";

/// The variable a child cash finds its descriptors in: the parent's pid, then
/// `,FD:KIND:HANDLE` for each, the handle being the child's inherited copy.
const PASSED: &str = "CASH_FDS";

/// Taken by every start of a program; see the module's documentation.
static STARTS: Mutex<()> = Mutex::new(());

/// The descriptors [`receive`] took, until the shell asks for them.
static RECEIVED: Mutex<Vec<(i32, OwnedHandle, Passed)>> = Mutex::new(Vec::new());

/// Asks for `handles` to reach the cash `command` starts, each as the descriptor it is
/// paired with.
///
/// The handles are the parent's own, and must stay open until the command is started
/// through [`prepare`], [`spawn`] or [`status`]; a start that is not passes nothing.
pub fn pass(command: &mut Command, handles: &[(i32, BorrowedHandle<'_>, Passed)]) {
    if handles.is_empty() {
        return;
    }
    let request = handles
        .iter()
        .map(|(fd, handle, kind)| {
            format!(
                "{fd}:{}:{:x}",
                kind.letter(),
                handle.as_raw_handle() as usize
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    command.env(REQUEST, request);
}

/// Held while a program is created: the lock every start takes, and the inheritable
/// copies of the handles passed to it, which are closed before the lock is let go.
#[must_use = "the program is to be created while this is held"]
pub struct Starting {
    // Declared before the lock, so dropped before it.
    _copies: Vec<OwnedHandle>,
    _lock: MutexGuard<'static, ()>,
}

/// Takes the lock for starting `command`, and makes the copies of the handles [`pass`]
/// asked for, named in the child's environment.
///
/// Create the program while the value is held, and drop it as soon as the program is
/// created.
///
/// # Errors
///
/// A handle that could not be copied, as when cash has run out of handles.
pub fn prepare(command: &mut Command) -> io::Result<Starting> {
    let lock = STARTS.lock().unwrap_or_else(PoisonError::into_inner);
    let request = command
        .get_envs()
        .find(|(name, _)| *name == OsStr::new(REQUEST))
        .and_then(|(_, value)| value)
        .map(|value| value.to_string_lossy().into_owned());
    command.env_remove(REQUEST);
    // Not from a cash's own environment either: its handles are not the child's.
    command.env_remove(PASSED);

    let mut copies = Vec::new();
    if let Some(request) = request {
        let mut passed = std::process::id().to_string();
        for (fd, kind, handle) in request.split(',').filter_map(parse_entry) {
            let copy = inheritable_copy(handle)?;
            write!(
                passed,
                ",{fd}:{}:{:x}",
                kind.letter(),
                copy.as_raw_handle() as usize
            )
            .map_err(io::Error::other)?;
            copies.push(copy);
        }
        command.env(PASSED, passed);
    }
    Ok(Starting {
        _copies: copies,
        _lock: lock,
    })
}

/// Starts `command`, as [`Command::spawn`] does, under the lock.
///
/// # Errors
///
/// As [`prepare`] and [`Command::spawn`].
pub fn spawn(command: &mut Command) -> io::Result<Child> {
    let starting = prepare(command)?;
    let child = command.spawn();
    drop(starting);
    child
}

/// Runs `command` and waits for it, as [`Command::status`] does, holding the lock only
/// while it is started.
///
/// # Errors
///
/// As [`prepare`], [`Command::spawn`] and [`Child::wait`].
pub fn status(command: &mut Command) -> io::Result<ExitStatus> {
    spawn(command)?.wait()
}

/// Takes the descriptors the cash that started this one passed to it out of the
/// environment, so that nothing this process starts sees them.
///
/// [`received`] hands them over. Called first in `main`, while cash has a single thread.
pub fn receive() {
    let Some(value) = std::env::var_os(PASSED) else {
        return;
    };
    // SAFETY: called first in `main`, before any other thread exists, so nothing reads
    // the environment while it changes.
    unsafe { std::env::remove_var(PASSED) };

    let value = value.to_string_lossy();
    let mut entries = value.split(',');
    // Only from the cash that started this one: the handles are this process's only if
    // its parent made them so. A program that keeps its environment for its children
    // could otherwise hand the variable on to a cash it starts.
    let parent = entries.next().and_then(|pid| pid.parse::<u32>().ok());
    if parent.is_none() || parent != crate::facts::parent_pid() {
        return;
    }
    let received = entries
        .filter_map(parse_entry)
        // SAFETY: the parent made the handle inheritable for this process's start, and
        // named it only here; nothing else in this process owns it.
        .map(|(fd, kind, handle)| (fd, unsafe { OwnedHandle::from_raw_handle(handle) }, kind))
        .collect();
    *RECEIVED.lock().unwrap_or_else(PoisonError::into_inner) = received;
}

/// The descriptors [`receive`] took, handed over once: each with its number and what it
/// is open on.
#[must_use]
pub fn received() -> Vec<(i32, OwnedHandle, Passed)> {
    std::mem::take(&mut *RECEIVED.lock().unwrap_or_else(PoisonError::into_inner))
}

/// `FD:KIND:HANDLE`, the handle in hexadecimal.
fn parse_entry(entry: &str) -> Option<(i32, Passed, HANDLE)> {
    let mut parts = entry.split(':');
    let fd = parts.next()?.parse::<i32>().ok().filter(|&fd| fd > 2)?;
    let kind = Passed::from_letter(parts.next()?)?;
    let handle = usize::from_str_radix(parts.next()?, 16).ok()?;
    if parts.next().is_some() || handle == 0 {
        return None;
    }
    Some((fd, kind, handle as HANDLE))
}

/// A copy of `handle` that a child started now inherits.
fn inheritable_copy(handle: HANDLE) -> io::Result<OwnedHandle> {
    let mut copy: HANDLE = std::ptr::null_mut();
    // SAFETY: the pseudo-handle of the current process, valid without being opened.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: `handle` is open, as `pass`'s caller promises, and `copy` is a live,
    // correctly typed out-parameter.
    let copied = unsafe {
        DuplicateHandle(
            process,
            handle,
            process,
            &raw mut copy,
            0,
            TRUE,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if copied == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `DuplicateHandle` made the handle, which nothing else owns.
    Ok(unsafe { OwnedHandle::from_raw_handle(copy) })
}

/// Whether the current process is inside a job object.
///
/// Relevant to D6: cash running inside Windows Terminal's or VS Code's own job is fine,
/// because nested jobs have worked since Windows 8 — but it is worth being able to say
/// so in `cash doctor` (D35).
#[must_use]
pub fn in_any_job() -> bool {
    use windows_sys::Win32::System::JobObjects::IsProcessInJob;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut result: i32 = FALSE;
    // SAFETY: returns a pseudo-handle for the current process; it reads no memory.
    let this = unsafe { GetCurrentProcess() };
    // SAFETY: a null job handle asks "is this process in *any* job", and `result` is a
    // live, correctly-typed BOOL out-parameter.
    let ok = unsafe { IsProcessInJob(this, std::ptr::null_mut(), &raw mut result) };
    ok != 0 && result != 0
}

/// Start a process that deliberately leaves cash's job object (D45's `detach`), in the
/// directory `cwd` and with the environment `env` (the shell's, which are not the process's:
/// D5, D10).
///
/// `CREATE_BREAKAWAY_FROM_JOB` only succeeds if the enclosing job permits it, which is
/// why the session job is created with `JOB_OBJECT_LIMIT_BREAKAWAY_OK`
/// ([`crate::job::JobConfig::session`]). D45 records the cost of that: once breakaway is
/// permitted, *any* child can request it, so this escape hatch weakens D6's guarantee
/// slightly for everything.
///
/// `DETACHED_PROCESS` additionally gives the child no console, so it does not die with
/// the terminal and does not scribble on cash's output. And it inherits no handle at all,
/// which the standard library cannot promise: a child it starts inherits every
/// inheritable handle of cash's, among them a pipe cash's own output goes to, which then
/// stays open for as long as the detached program runs.
pub fn spawn_detached(command_line: &str, cwd: &Path, env: &[(String, String)]) -> io::Result<u32> {
    use windows_sys::Win32::System::Threading::{CREATE_BREAKAWAY_FROM_JOB, DETACHED_PROCESS};

    let mut command = to_wide_nul(command_line);
    let cwd = to_wide_nul(crate::path::process_directory(cwd)?);
    let environment = crate::env::environment_block(env);

    // SAFETY: `STARTUPINFOW` is plain old data — integers, pointers and a handle triple —
    // for which all-zero is the documented "use the defaults" value. `cb` is set below,
    // which is the only field the API requires.
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    // The structure is a fixed ~104 bytes and `cb` is a `u32` by ABI, so the saturating
    // fallback is unreachable; it keeps this a function that returns rather than panics.
    startup.cb = u32::try_from(size_of::<STARTUPINFOW>()).unwrap_or(u32::MAX);

    // SAFETY: `PROCESS_INFORMATION` is four integers-or-handles that CreateProcessW fills
    // in; all-zero is a valid starting state.
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    // SAFETY: command is a live, writable UTF-16 buffer; the directory and the environment
    // block are NUL-terminated (the block doubly) and outlive the call; every other
    // pointer is null or points at a correctly-sized local that outlives the call.
    let created = unsafe {
        CreateProcessW(
            std::ptr::null(),
            command.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            FALSE, // do not inherit handles: a detached process should hold none of ours
            CREATE_BREAKAWAY_FROM_JOB | DETACHED_PROCESS | CREATE_UNICODE_ENVIRONMENT,
            environment.as_ptr().cast(),
            cwd.as_ptr(),
            &raw const startup,
            &raw mut info,
        )
    };

    if created == 0 {
        return Err(io::Error::last_os_error());
    }

    // cash neither waits for the program nor ends it, so it keeps no handle to it: both
    // are closed here.
    // SAFETY: the process was made, so both are open handles that are ours.
    drop(unsafe { OwnedHandle::from_raw_handle(info.hThread) });
    // SAFETY: as above.
    drop(unsafe { OwnedHandle::from_raw_handle(info.hProcess) });
    Ok(info.dwProcessId)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_names_a_descriptor_above_2_its_kind_and_a_handle() {
        assert_eq!(
            parse_entry("3:w:1a4"),
            Some((3, Passed::PipeWriter, 0x1a4 as HANDLE))
        );
        assert_eq!(parse_entry("10:t:8"), Some((10, Passed::Tcp, 8 as HANDLE)));
        // 0, 1 and 2 are the standard handles, given otherwise.
        assert_eq!(parse_entry("2:f:1a4"), None);
        assert_eq!(parse_entry("3:x:1a4"), None);
        assert_eq!(parse_entry("3:f:0"), None);
        assert_eq!(parse_entry("3:f:1a4:9"), None);
        assert_eq!(parse_entry("3:f"), None);
        for kind in [
            Passed::File,
            Passed::PipeReader,
            Passed::PipeWriter,
            Passed::Tcp,
            Passed::Udp,
        ] {
            let letter = kind.letter().to_string();
            assert_eq!(Passed::from_letter(&letter), Some(kind));
        }
    }

    #[test]
    fn a_start_names_copies_of_the_handles_asked_for_and_not_the_request() {
        let (reader, writer) = std::io::pipe().unwrap();
        let mut command = Command::new("cmd.exe");
        pass(
            &mut command,
            &[(
                3,
                std::os::windows::io::AsHandle::as_handle(&writer),
                Passed::PipeWriter,
            )],
        );
        let starting = prepare(&mut command).unwrap();
        let value = |name: &str| {
            command
                .get_envs()
                .find(|(key, _)| *key == OsStr::new(name))
                .and_then(|(_, value)| value.map(|v| v.to_string_lossy().into_owned()))
        };
        assert_eq!(value(REQUEST), None);
        let passed = value(PASSED).unwrap();
        let (pid, entry) = passed.split_once(',').unwrap();
        assert_eq!(pid, std::process::id().to_string());
        let (fd, kind, copy) = parse_entry(entry).unwrap();
        assert_eq!((fd, kind), (3, Passed::PipeWriter));
        assert_ne!(
            copy,
            writer.as_raw_handle(),
            "a copy, not the handle itself"
        );
        drop(starting);
        drop(reader);
    }

    #[test]
    fn a_start_with_nothing_to_pass_names_nothing() {
        let mut command = Command::new("cmd.exe");
        command.env(PASSED, "1,3:f:1a4");
        drop(prepare(&mut command).unwrap());
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == OsStr::new(PASSED) && value.is_none()),
            "removed from the child's environment"
        );
    }
}
