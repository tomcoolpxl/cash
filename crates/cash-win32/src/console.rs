//! Console control events and suspend — **D13**, **D19**, **D41**.
//!
//! # The trap that catches everyone
//!
//! `CTRL_C_EVENT` **cannot be delivered to a specific process group.**
//! `GenerateConsoleCtrlEvent` with a nonzero group id *succeeds* and the signal is never
//! received — a silent failure, which is the worst possible shape for the feature cash
//! is built around. Only `CTRL_BREAK_EVENT` is group-deliverable.
//!
//! So cash sends `CTRL_BREAK_EVENT` for targeted delivery. Go's runtime maps both events
//! to `os.Interrupt`, so Terraform, `gh` and `kubectl` — the toolchain in §1 — handle it
//! correctly. Tools that handle `CTRL_C_EVENT` but ignore `CTRL_BREAK_EVENT` remain an
//! open question that needs a survey against the real corpus, not reasoning.
//!
//! # Escalation (D13)
//!
//! The grace period is the user, not a timer:
//!
//! ```text
//! 1st Ctrl-C  -> console control event to the foreground job's process group
//! 2nd Ctrl-C  -> escalate
//! 3rd Ctrl-C  -> TerminateJobObject
//! ```
//!
//! Terraform's own interrupt handling is already two-stage — the first interrupt
//! finishes the *current* operation, which can legitimately take minutes. A fixed
//! timeout would either guillotine a valid apply or be long enough to feel broken.

use std::io;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use windows_sys::Win32::Foundation::{CloseHandle, FALSE, STILL_ACTIVE};
use windows_sys::Win32::System::Console::{
    CTRL_BREAK_EVENT, CTRL_C_EVENT, GenerateConsoleCtrlEvent, SetConsoleCP, SetConsoleCtrlHandler,
    SetConsoleOutputCP,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeThread, OpenThread, ResumeThread, SuspendThread, THREAD_QUERY_LIMITED_INFORMATION,
    THREAD_SUSPEND_RESUME,
};

/// UTF-8. Set for the console per D41.
pub const CODE_PAGE_UTF8: u32 = 65001;

/// What the next interrupt should do (D13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escalation {
    /// Ask politely: deliver a console control event and let the child clean up.
    Interrupt,
    /// Ask again, more firmly.
    Escalate,
    /// Stop asking: terminate the job object.
    Terminate,
}

/// Tracks how many interrupts the user has sent for the current foreground job.
///
/// Reset when a new foreground job starts, so that one Ctrl-C per command does not
/// accumulate into a termination three commands later.
#[derive(Debug, Default)]
pub struct InterruptState {
    count: AtomicU32,
}

impl InterruptState {
    /// A fresh state, as for a newly started foreground job.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: AtomicU32::new(0),
        }
    }

    /// Record an interrupt and report what should happen (D13).
    pub fn record(&self) -> Escalation {
        match self.count.fetch_add(1, Ordering::SeqCst) {
            0 => Escalation::Interrupt,
            1 => Escalation::Escalate,
            _ => Escalation::Terminate,
        }
    }

    /// Reset for a new foreground job.
    pub fn reset(&self) {
        self.count.store(0, Ordering::SeqCst);
    }

    /// How many interrupts have been recorded.
    pub fn count(&self) -> u32 {
        self.count.load(Ordering::SeqCst)
    }
}

/// Deliver an interrupt to a process group (D13 step 1).
///
/// Uses `CTRL_BREAK_EVENT` because `CTRL_C_EVENT` cannot be targeted at a group — see
/// this module's documentation. The target must have been created with
/// `CREATE_NEW_PROCESS_GROUP` for the group id to exist.
pub fn interrupt_process_group(group_id: u32) -> io::Result<()> {
    // Group 0 is not "no group" — it is *every process attached to this console*,
    // including cash itself, the terminal, and any unrelated program sharing it. A shell
    // never means that: `kill 0` means "my own process group", which on Windows is the
    // tree cash spawned, not the console. Refusing here makes the broadcast unreachable
    // from any caller rather than trusting each one to remember.
    if group_id == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to send a console control event to every process in the console",
        ));
    }

    // SAFETY: a plain Win32 call with a scalar argument.
    let ok = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, group_id) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Takes Ctrl-C handling back from a parent that turned it off (D13).
///
/// Windows passes "ignore Ctrl-C" down to children: a process started in a new process
/// group, or by one that called `SetConsoleCtrlHandler(NULL, TRUE)` (build tools and task
/// runners do), ignores Ctrl-C, and so does everything it starts. An interactive shell owns
/// its terminal, so it clears the flag at startup; without that, Ctrl-C would stop
/// nothing the user runs. Scripts keep what they inherited, as POSIX has a
/// non-interactive shell keep signals that were ignored on entry.
pub fn enable_ctrl_c() {
    // SAFETY: a plain call; a null handler with FALSE restores default Ctrl-C handling.
    unsafe { SetConsoleCtrlHandler(None, FALSE) };
}

/// A Ctrl-C the console delivered while the shell was running commands of its own, and
/// which the shell has not acted on yet.
static PENDING_INTERRUPT: AtomicBool = AtomicBool::new(false);

/// Keeps a Ctrl-C that arrives while the shell runs its own commands, for the shell to
/// act on before its next command ([`take_interrupt`]) (D13).
///
/// Windows ends a process that has no handler for Ctrl-C where it stands. A script in a
/// loop of builtins was ended so, and an interactive shell running one as well: no
/// `EXIT` trap, no trap on `INT`, no prompt to come back to. Bash runs the traps.
///
/// Handlers are asked newest first, and this one is installed when the shell starts, so
/// anything that listens later is asked before it: the wait for a foreground program,
/// which has its own rule for a Ctrl-C, and `ping`. It therefore hears only the Ctrl-C
/// nothing else took. A second one that arrives before the shell has acted on the first
/// is left to Windows, which ends the process: a shell stuck in a command that never
/// returns can still be ended from the keyboard.
///
/// A shell started to ignore Ctrl-C hears nothing here, and goes on ignoring it.
pub fn keep_interrupts() {
    unsafe extern "system" fn handler(kind: u32) -> i32 {
        i32::from(kind == CTRL_C_EVENT && !PENDING_INTERRUPT.swap(true, Ordering::SeqCst))
    }

    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        // SAFETY: registers a handler that only touches an atomic.
        unsafe { SetConsoleCtrlHandler(Some(handler), 1) };
    });
}

/// Whether a Ctrl-C is waiting to be acted on, which this call takes.
pub fn take_interrupt() -> bool {
    PENDING_INTERRUPT.swap(false, Ordering::SeqCst)
}

/// Input modes of a console waiting for a line: processed, line-buffered and echoed
/// input, with insert and quick-edit, as a new console window starts.
const COOKED_INPUT: u32 = 0x0001 | 0x0002 | 0x0004 | 0x0010 | 0x0020 | 0x0040 | 0x0080 | 0x0100;
/// Output modes cash relies on: processed output, wrapping, and VT sequences.
const VT_OUTPUT: u32 = 0x0001 | 0x0002 | 0x0004;

/// Puts back the console state cash relies on between prompts, for `reset`.
///
/// That is cooked input, VT-processing output and the UTF-8 code page (D41). A program
/// that died in raw mode, with echo off, VT processing off or after `chcp` leaves the
/// console unusable, and escape sequences cannot undo that because they are not
/// interpreted then.
///
/// Returns `false` when the process has no console to restore.
pub fn restore_modes() -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::System::Console::SetConsoleMode;

    let open = |name: &str| {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(name)
    };
    let (Ok(input), Ok(output)) = (open("CONIN$"), open("CONOUT$")) else {
        return Ok(false);
    };
    // SAFETY: the handles are open console handles owned by `input` and `output`, alive
    // for the call.
    let input_ok = unsafe { SetConsoleMode(input.as_raw_handle(), COOKED_INPUT) };
    // SAFETY: as above.
    let output_ok = unsafe { SetConsoleMode(output.as_raw_handle(), VT_OUTPUT) };
    if input_ok == 0 || output_ok == 0 {
        return Err(io::Error::last_os_error());
    }
    set_utf8_code_page()?;
    Ok(true)
}

/// Whether cash has started a program since the last prompt: then the terminal may need
/// putting back ([`repair_before_prompt`]).
static PROGRAM_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Notes that a program was started, for [`repair_before_prompt`].
pub fn note_program_started() {
    PROGRAM_STARTED.store(true, Ordering::Relaxed);
}

/// `ENABLE_VIRTUAL_TERMINAL_INPUT`: Windows then hands over Enter as `\r` and Backspace as
/// `\x7f` rather than as keys. Letters still arrive, but the line editor reads keys, so
/// Enter and Backspace do nothing. `k3d cluster create` left it on (2026-09-29).
const VT_INPUT: u32 = 0x0200;
/// The input modes the line editor turns off for its raw mode and back on for a command:
/// processed, line and echo input. Its business, so never touched here.
const COOKED: u32 = 0x0001 | 0x0002 | 0x0004;
/// The input modes that are the console's settings rather than a program's: window and
/// mouse input, insert mode, quick edit and auto-position. A program that takes the mouse
/// turns quick edit off, and a console with mouse input and no quick edit has Windows
/// Terminal send it the mouse, so text no longer selects with a plain drag.
const SETTINGS: u32 = 0x0008 | 0x0010 | 0x0020 | 0x0040 | 0x0100;
/// `ENABLE_EXTENDED_FLAGS`: without it, setting a mode leaves insert mode and quick edit as
/// they are.
const EXTENDED_FLAGS: u32 = 0x0080;

/// The console's input settings ([`SETTINGS`]) as cash started, for
/// [`repair_before_prompt`] to put back.
static STARTING_INPUT: std::sync::OnceLock<u32> = std::sync::OnceLock::new();

/// Remembers the console's input settings as cash starts, before any program can change
/// them, for [`repair_before_prompt`]. Without a console, nothing is remembered.
pub fn remember_starting_modes() {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::System::Console::GetConsoleMode;

    let Ok(input) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONIN$")
    else {
        return;
    };
    let mut mode = 0u32;
    // SAFETY: an open console handle owned by `input`, and a valid out-parameter.
    if unsafe { GetConsoleMode(input.as_raw_handle(), &raw mut mode) } != 0 {
        let _ = STARTING_INPUT.set(mode & SETTINGS);
    }
}

/// The input mode to have at a prompt, given the `current` one and the settings cash
/// started with, if known: the line editor's raw-mode bits as they are, the starting
/// settings (or the current ones), and no VT input.
const fn wanted_input(current: u32, starting: Option<u32>) -> u32 {
    let settings = match starting {
        Some(starting) => starting,
        None => current,
    };
    ((current & COOKED) | (settings & SETTINGS) | EXTENDED_FLAGS) & !VT_INPUT
}
/// Output modes the prompt needs: processed output, wrapping at the edge, VT sequences.
const NEEDED_OUTPUT: u32 = 0x0001 | 0x0002 | 0x0004;
/// `DISABLE_NEWLINE_AUTO_RETURN`: a line feed then stays in its column, and output steps
/// down the screen diagonally.
const NO_AUTO_RETURN: u32 = 0x0008;

/// The terminal state a program may leave behind, put back before the prompt after one ran.
///
/// First the screen: the main screen rather than the alternate one a full-screen program
/// died in, and scrolling over the whole screen rather than a region, both between saving
/// the cursor and restoring it. Leaving the alternate screen restores the cursor saved on
/// entering it, and setting the region moves it to the top; on a healthy screen the bare
/// sequences would move the prompt into earlier output, and the save and restore keep it
/// in place (tried on a ConPTY, 2026-09-29). Terminals keep a saved cursor per screen, so
/// after a full-screen program the restore puts back the cursor from before it started.
///
/// Then attributes reset, cursor shown, mouse tracking (1000, 1002, 1003, 1006, 1015) and
/// focus reporting off, cursor keys and keypad in their normal modes, the cursor shape the
/// terminal's profile gives, lines wrapping at the edge, and ASCII in G0: a curses program
/// that dies while drawing boxes leaves letters showing as line pieces. These come after
/// the restore, which also puts back the attributes and character set that were saved.
pub const TERMINAL_RESET: &str = "\x1b7\x1b[?1049l\x1b[r\x1b8\
                                  \x1b[0m\x1b[?25h\x1b[?1000l\x1b[?1002l\x1b[?1003l\
                                  \x1b[?1006l\x1b[?1015l\x1b[?1004l\x1b[?1l\x1b>\x1b[0 q\
                                  \x1b[?7h\x1b(B\x0f";

/// What [`repair_before_prompt`] found wrong and put back.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Repaired {
    /// VT input was on, or an input setting differed from cash's start.
    pub input: bool,
    /// An output mode the prompt needs was off, or line feeds had stopped returning.
    pub output: bool,
    /// The code page was not UTF-8.
    pub code_page: bool,
    /// A program ran, and the terminal reset was sent.
    pub terminal: bool,
}

/// Puts back, before a prompt, the console state a program may have left wrong (D68).
///
/// Wrong is where the line editor could not read keys, or the prompt could not be drawn
/// (decided with the user, 2026-09-29, after `k3d cluster create` left VT input on). Only
/// what is wrong is changed, silently: VT input off, and the input settings as cash
/// started ([`remember_starting_modes`]); processed output, wrapping and VT processing on,
/// and line feeds returning to the margin; the UTF-8 code page (D41); and, when a program
/// ran since the last prompt, [`TERMINAL_RESET`]. The line editor's own raw mode is its
/// business and is left to it. Without a console, nothing is done.
pub fn repair_before_prompt() -> Repaired {
    use std::io::Write as _;
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::System::Console::{
        GetConsoleCP, GetConsoleMode, GetConsoleOutputCP, SetConsoleMode,
    };

    let mut repaired = Repaired::default();
    let open = |name: &str| {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(name)
            .ok()
    };
    let mode = |file: &std::fs::File| {
        let mut mode = 0u32;
        // SAFETY: an open console handle owned by `file`, and a valid out-parameter.
        (unsafe { GetConsoleMode(file.as_raw_handle(), &raw mut mode) } != 0).then_some(mode)
    };
    let set = |file: &std::fs::File, mode: u32| {
        // SAFETY: an open console handle owned by `file`, alive for the call.
        unsafe { SetConsoleMode(file.as_raw_handle(), mode) != 0 }
    };

    let input = open("CONIN$");
    if let Some(input) = &input
        && let Some(current) = mode(input)
    {
        let wanted = wanted_input(current, STARTING_INPUT.get().copied());
        // The console need not report the extended-flags bit back, so it is not compared.
        if (wanted ^ current) & !EXTENDED_FLAGS != 0 {
            repaired.input = set(input, wanted);
        }
    }
    let output = open("CONOUT$");
    if let Some(output) = &output
        && let Some(current) = mode(output)
    {
        let wanted = (current | NEEDED_OUTPUT) & !NO_AUTO_RETURN;
        if wanted != current {
            repaired.output = set(output, wanted);
        }
    }
    if input.is_none() && output.is_none() {
        return repaired;
    }

    // SAFETY: plain calls without arguments.
    let input_page = unsafe { GetConsoleCP() };
    // SAFETY: as above.
    let output_page = unsafe { GetConsoleOutputCP() };
    if input_page != CODE_PAGE_UTF8 || output_page != CODE_PAGE_UTF8 {
        repaired.code_page = set_utf8_code_page().is_ok();
    }

    if PROGRAM_STARTED.swap(false, Ordering::Relaxed)
        && let Some(mut output) = output
    {
        repaired.terminal = output
            .write_all(TERMINAL_RESET.as_bytes())
            .and_then(|()| output.flush())
            .is_ok();
    }
    repaired
}

/// Set the console to UTF-8 (D41).
///
/// Fixes console *display* and console-attached children. It deliberately does not claim
/// to fix pipeline bytes: when cash pipes a child's output it reads raw bytes and the
/// console code page is never consulted.
pub fn set_utf8_code_page() -> io::Result<()> {
    // SAFETY: a plain Win32 call taking a scalar code page; it touches no memory of ours.
    let out = unsafe { SetConsoleOutputCP(CODE_PAGE_UTF8) };
    // SAFETY: as above, for the input code page.
    let inp = unsafe { SetConsoleCP(CODE_PAGE_UTF8) };
    if out == 0 || inp == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// `ENABLE_LINE_INPUT`: off, a program reads keys one at a time, as a full-screen one does.
const LINE_INPUT: u32 = 0x0002;

/// The console as a program left it: input and output modes, and code pages.
///
/// Saved when Ctrl-Z stops a job and put back when `fg` resumes it (D19), as zsh keeps
/// each stopped job's terminal modes. The prompt in between sets the console as it needs
/// it ([`repair_before_prompt`]), and a program resumed in modes it did not choose would
/// find, in raw mode, its keys echoed and held back until Enter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsoleState {
    input: Option<u32>,
    output: Option<u32>,
    input_code_page: u32,
    output_code_page: u32,
}

impl ConsoleState {
    /// The console's state now; `None` without a console.
    #[must_use]
    pub fn save() -> Option<Self> {
        use windows_sys::Win32::System::Console::{GetConsoleCP, GetConsoleOutputCP};

        let input = console_mode("CONIN$");
        let output = console_mode("CONOUT$");
        if input.is_none() && output.is_none() {
            return None;
        }
        Some(Self {
            input,
            output,
            // SAFETY: plain calls without arguments.
            input_code_page: unsafe { GetConsoleCP() },
            // SAFETY: as above.
            output_code_page: unsafe { GetConsoleOutputCP() },
        })
    }

    /// Puts the saved state back, as far as the console accepts it.
    pub fn restore(&self) {
        use std::os::windows::io::AsRawHandle as _;
        use windows_sys::Win32::System::Console::SetConsoleMode;

        for (name, mode) in [("CONIN$", self.input), ("CONOUT$", self.output)] {
            if let (Some(mode), Ok(file)) = (mode, open_console(name)) {
                // SAFETY: an open console handle owned by `file`, alive for the call.
                unsafe { SetConsoleMode(file.as_raw_handle(), mode) };
            }
        }
        // SAFETY: a plain call taking a code page; it touches no memory of ours.
        unsafe { SetConsoleCP(self.input_code_page) };
        // SAFETY: as above.
        unsafe { SetConsoleOutputCP(self.output_code_page) };
    }

    /// Whether the program was reading keys one at a time, as a full-screen program does.
    #[must_use]
    pub const fn raw_input(&self) -> bool {
        matches!(self.input, Some(mode) if mode & LINE_INPUT == 0)
    }
}

fn open_console(name: &str) -> io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(name)
}

fn console_mode(name: &str) -> Option<u32> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::System::Console::GetConsoleMode;

    let file = open_console(name).ok()?;
    let mut mode = 0u32;
    // SAFETY: an open console handle owned by `file`, and a valid out-parameter.
    (unsafe { GetConsoleMode(file.as_raw_handle(), &raw mut mode) } != 0).then_some(mode)
}

/// Asks the program reading the console to draw its screen again, as `fg` resumes a
/// full-screen program (D19).
///
/// Unix sends a resumed program `SIGCONT`, and a full-screen one redraws on it; Windows
/// tells it nothing, and the prompt has drawn over its screen meanwhile. Such a program
/// redraws when the window changes size, so the console is handed a resize record with
/// the size it already has. Best effort: a program that ignores resizes shows its old
/// screen once it draws again.
pub fn request_redraw() {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::System::Console::{
        CONSOLE_SCREEN_BUFFER_INFO, GetConsoleScreenBufferInfo, INPUT_RECORD,
        WINDOW_BUFFER_SIZE_EVENT, WriteConsoleInputW,
    };

    let (Ok(input), Ok(output)) = (open_console("CONIN$"), open_console("CONOUT$")) else {
        return;
    };
    // SAFETY: an all-zero CONSOLE_SCREEN_BUFFER_INFO is a valid out-parameter.
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: an open console handle owned by `output`, and a valid out-parameter.
    if unsafe { GetConsoleScreenBufferInfo(output.as_raw_handle(), &raw mut info) } == 0 {
        return;
    }
    // SAFETY: an all-zero INPUT_RECORD is valid; the resize fields follow.
    let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
    record.EventType = u16::try_from(WINDOW_BUFFER_SIZE_EVENT).unwrap_or_default();
    record.Event.WindowBufferSizeEvent.dwSize = info.dwSize;
    let mut written = 0u32;
    // SAFETY: an open console handle owned by `input`, one initialised record, and a valid
    // out-parameter.
    unsafe {
        WriteConsoleInputW(
            input.as_raw_handle(),
            &raw const record,
            1,
            &raw mut written,
        )
    };
}

/// Suspend every thread of a process (D19).
///
/// Windows has no `SIGSTOP` for arbitrary executables, so `Ctrl-Z` and `kill -STOP`
/// enumerate threads and suspend each — what Process Explorer's Suspend does, using only
/// documented APIs rather than the undocumented `NtSuspendProcess`.
///
/// Accepted costs, recorded in D19: racy against thread creation during the sweep, and a
/// process could in principle resume itself.
///
/// A process cash has already stopped is left as it is: a thread's suspend count adds up,
/// so a second `kill -STOP` took a second `kill -CONT` to undo, where a stopped Unix
/// process is simply stopped (W32-05). What cash has stopped is known by pid and start
/// time, so a pid handed out again is another process.
pub fn suspend_process(pid: u32) -> io::Result<usize> {
    suspend_processes(&[pid])
}

/// [`suspend_process`] for each of `pids`, from one thread snapshot of the system: a job's
/// tree took one per process (W32-16).
pub fn suspend_processes(pids: &[u32]) -> io::Result<usize> {
    let mut stopped = STOPPED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    stopped.retain(|&(other, _)| crate::process::is_pid_alive(other));
    let identities: Vec<(u32, Option<u64>)> = pids
        .iter()
        .map(|&pid| (pid, crate::process::started(pid)))
        .filter(|identity| !stopped.contains(identity))
        .collect();
    if identities.is_empty() {
        return Ok(0);
    }
    let targets: Vec<u32> = identities.iter().map(|&(pid, _)| pid).collect();
    let threads = for_each_thread(&targets, |handle| {
        // SAFETY: handle is a valid thread handle with THREAD_SUSPEND_RESUME.
        unsafe { SuspendThread(handle) };
    })?;
    stopped.extend(identities);
    drop(stopped);
    Ok(threads)
}

/// Resume every thread of a process (D19), if cash stopped it; a process cash did not
/// stop is left running, or stopped by whoever stopped it, as `SIGCONT` leaves a running
/// one.
pub fn resume_process(pid: u32) -> io::Result<usize> {
    resume_processes(&[pid])
}

/// [`resume_process`] for each of `pids`, from one thread snapshot of the system.
pub fn resume_processes(pids: &[u32]) -> io::Result<usize> {
    let mut stopped = STOPPED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let targets: Vec<u32> = pids
        .iter()
        .copied()
        .filter(|&pid| {
            let identity = (pid, crate::process::started(pid));
            let at = stopped.iter().position(|&known| known == identity);
            at.map(|at| stopped.swap_remove(at)).is_some()
        })
        .collect();
    drop(stopped);
    if targets.is_empty() {
        return Ok(0);
    }
    for_each_thread(&targets, |handle| {
        // SAFETY: handle is a valid thread handle with THREAD_SUSPEND_RESUME.
        unsafe { ResumeThread(handle) };
    })
}

/// Resume every thread of a process once, whoever suspended it: for a process cash
/// created suspended, which job control did not stop.
pub fn start_threads(pid: u32) -> io::Result<usize> {
    for_each_thread(&[pid], |handle| {
        // SAFETY: handle is a valid thread handle with THREAD_SUSPEND_RESUME.
        unsafe { ResumeThread(handle) };
    })
}

/// The processes cash has stopped, by pid and start time.
static STOPPED: std::sync::Mutex<Vec<(u32, Option<u64>)>> = std::sync::Mutex::new(Vec::new());

/// Apply an operation to every thread of the processes `pids`, returning how many were
/// affected.
fn for_each_thread<F>(pids: &[u32], mut action: F) -> io::Result<usize>
where
    F: FnMut(windows_sys::Win32::Foundation::HANDLE),
{
    // A process that has exited has nothing to suspend or resume, whatever the thread
    // snapshot still lists for it: on GitHub's runner an exited process kept reporting a
    // thread that the per-thread exit-code check below did not rule out.
    let pids: Vec<u32> = pids
        .iter()
        .copied()
        .filter(|&pid| crate::process::is_pid_alive(pid))
        .collect();
    if pids.is_empty() {
        return Ok(0);
    }

    // SAFETY: TH32CS_SNAPTHREAD ignores the pid argument and snapshots all threads.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    // SAFETY: `THREADENTRY32` is a plain-old-data Win32 struct of integers, for which an
    // all-zero bit pattern is valid; `dwSize` is filled in immediately below, which is the
    // only field the API requires before the first call.
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    // The struct is 28 bytes and `dwSize` is a `u32` by ABI, so the fallback is
    // unreachable — but saturating beats panicking in a function that returns a Result.
    entry.dwSize = u32::try_from(size_of::<THREADENTRY32>()).unwrap_or(u32::MAX);

    let mut affected = 0usize;

    // SAFETY: entry is correctly sized and the snapshot handle is valid.
    let mut ok = unsafe { Thread32First(snapshot, &raw mut entry) };
    while ok != 0 {
        if pids.contains(&entry.th32OwnerProcessID) {
            let access = THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION;
            // SAFETY: opening a thread by id; null is returned on failure.
            let handle = unsafe { OpenThread(access, FALSE, entry.th32ThreadID) };
            if !handle.is_null() {
                // A thread that has finished stays in the snapshot for as long as anyone
                // holds a handle to it (an antivirus scanner, say), and suspending it
                // "succeeds". It is not running, so it is not counted or touched.
                let mut code = 0u32;
                // SAFETY: handle is a valid thread handle with query access, and `code`
                // outlives the call.
                let queried = unsafe { GetExitCodeThread(handle, &raw mut code) };
                #[allow(clippy::cast_sign_loss, reason = "STILL_ACTIVE is 259")]
                let running = queried == 0 || code == STILL_ACTIVE as u32;
                if running {
                    action(handle);
                    affected += 1;
                }
                // SAFETY: closing a handle we just opened, exactly once.
                unsafe { CloseHandle(handle) };
            }
        }
        // SAFETY: as above.
        ok = unsafe { Thread32Next(snapshot, &raw mut entry) };
    }

    // SAFETY: closing the snapshot handle, exactly once.
    unsafe { CloseHandle(snapshot) };

    Ok(affected)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree's processes stop and start together, from one snapshot (W32-16), and a
    /// second stop or start of each does nothing, as for one process (W32-05).
    #[test]
    fn processes_stop_and_start_together() {
        let mut children: Vec<std::process::Child> = (0..2)
            .map(|_| {
                std::process::Command::new("ping")
                    .args(["-n", "30", "127.0.0.1"])
                    .stdout(std::process::Stdio::null())
                    .spawn()
                    .unwrap()
            })
            .collect();
        let pids: Vec<u32> = children.iter().map(std::process::Child::id).collect();
        // A process still starting makes threads while it is stopped, a race D19
        // accepts; so the threads started again are at least those stopped.
        std::thread::sleep(std::time::Duration::from_millis(300));

        let stopped = suspend_processes(&pids).unwrap();
        let again = suspend_processes(&pids).unwrap();
        let started = resume_processes(&pids).unwrap();
        let after = resume_processes(&pids).unwrap();
        for child in &mut children {
            let _ = child.kill();
            let _ = child.wait();
        }

        assert!(stopped >= 2, "{stopped} threads stopped");
        assert_eq!(again, 0);
        assert!(started >= stopped, "{started} started, {stopped} stopped");
        assert_eq!(after, 0);
    }

    /// Windows Terminal's console as cash starts: cooked, with mouse input, insert mode,
    /// quick edit and auto-position, and extended flags.
    const FRESH: u32 = 0x01F7;

    #[test]
    fn a_healthy_prompt_is_left_alone() {
        assert_eq!(wanted_input(FRESH, Some(FRESH & SETTINGS)), FRESH);
        // In the line editor's raw mode.
        assert_eq!(wanted_input(0x01F0, Some(FRESH & SETTINGS)), 0x01F0);
    }

    #[test]
    fn vt_input_goes_and_nothing_else_changes() {
        // `k3d cluster create` left 0x03F0 (2026-09-29).
        assert_eq!(wanted_input(0x03F0, Some(FRESH & SETTINGS)), 0x01F0);
        assert_eq!(wanted_input(0x03F0, None), 0x01F0);
    }

    #[test]
    fn a_program_that_took_the_mouse_gives_back_quickedit_and_insert_mode() {
        // What crossterm's mouse capture sets: window and mouse input, extended flags.
        assert_eq!(wanted_input(0x0098, Some(FRESH & SETTINGS)), 0x01F0);
    }

    #[test]
    fn settings_the_user_started_with_are_kept() {
        // quick edit off in the console's properties stays off.
        let started = (FRESH & !0x0040) & SETTINGS;
        assert_eq!(wanted_input(0x01B7, Some(started)), 0x01B7);
    }
}
