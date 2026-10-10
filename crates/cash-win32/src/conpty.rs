//! Win32 ConPTY (Pseudo Console) support for interactive terminal testing and emulation.
//!
//! ConPTY was introduced in Windows 10 (version 1809) and allows hosting character-mode
//! command-line applications attached to a real pseudo console, with full VT100 / ANSI
//! escape sequences, raw input handling, and terminal geometry.

use std::cell::Cell;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::Path;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{FALSE, HANDLE, S_OK};
use windows_sys::Win32::System::Console::{COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON};
use windows_sys::Win32::System::Pipes::{CreatePipe, PeekNamedPipe};
use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, INFINITE, InitializeProcThreadAttributeList,
    PROCESS_INFORMATION, STARTUPINFOEXW, UpdateProcThreadAttribute, WaitForSingleObject,
};

/// Win32 thread attribute ID for pseudo console.
/// `ProcThreadAttributeValue(22, FALSE, TRUE, FALSE)` = `0x00020016`.
const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x0002_0016;

/// Process exit code reported while process is still active.
const STILL_ACTIVE: u32 = 259;

/// A child process spawned attached to a pseudo console.
pub struct ConPtyChild {
    process: OwnedHandle,
    /// Its first thread's handle, which `CreateProcessW` hands over too; held only to be
    /// closed with the process's.
    _thread: OwnedHandle,
    pid: u32,
}

impl ConPtyChild {
    /// Return the process ID.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.pid
    }

    /// Block until the process exits, returning its exit code.
    pub fn wait(&self) -> io::Result<u32> {
        // SAFETY: process handle is valid.
        unsafe { WaitForSingleObject(self.process.as_raw_handle(), INFINITE) };
        let mut code: u32 = 0;
        // SAFETY: handle is valid and code is valid out-param.
        let ok = unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &raw mut code) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(code)
    }

    /// Check if the process has exited without blocking.
    pub fn try_wait(&self) -> io::Result<Option<u32>> {
        let mut code: u32 = 0;
        // SAFETY: handle is valid and code is valid out-param.
        let ok = unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &raw mut code) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        if code == STILL_ACTIVE {
            Ok(None)
        } else {
            Ok(Some(code))
        }
    }
}

/// A native Win32 Pseudo Console (ConPTY).
pub struct ConPty {
    hpcon: HPCON,
    input_write: File,
    output_read: File,
    in_read: Cell<Option<OwnedHandle>>,
    out_write: Cell<Option<OwnedHandle>>,
}

/// An anonymous pipe's two ends, read and write.
fn anonymous_pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut read: HANDLE = std::ptr::null_mut();
    let mut write: HANDLE = std::ptr::null_mut();
    // SAFETY: standard CreatePipe call with valid out params.
    if unsafe { CreatePipe(&raw mut read, &raw mut write, std::ptr::null(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the call succeeded, so both are open handles that are ours.
    let read = unsafe { OwnedHandle::from_raw_handle(read) };
    // SAFETY: as above.
    let write = unsafe { OwnedHandle::from_raw_handle(write) };
    Ok((read, write))
}

impl ConPty {
    /// Creates a new ConPTY with the specified dimensions (columns and rows).
    pub fn new(cols: i16, rows: i16) -> io::Result<Self> {
        // A pipe for sending input to ConPTY, and one for receiving output from it.
        let (in_read, in_write) = anonymous_pipe()?;
        let (out_read, out_write) = anonymous_pipe()?;

        let size = COORD { X: cols, Y: rows };
        let mut hpcon: HPCON = 0;

        // SAFETY: CreatePseudoConsole takes in_read and out_write, which are open.
        let hr = unsafe {
            CreatePseudoConsole(
                size,
                in_read.as_raw_handle(),
                out_write.as_raw_handle(),
                0,
                &raw mut hpcon,
            )
        };

        if hr != S_OK {
            // An HRESULT is no Win32 error code; one that wraps a Win32 error carries it in
            // its low 16 bits (FACILITY_WIN32), and any other is reported as itself.
            return Err(if (hr.cast_unsigned() >> 16) == 0x8007 {
                io::Error::from_raw_os_error(hr & 0xFFFF)
            } else {
                io::Error::other(format!("CreatePseudoConsole failed: HRESULT {hr:#010x}"))
            });
        }

        Ok(Self {
            hpcon,
            input_write: File::from(in_write),
            output_read: File::from(out_read),
            in_read: Cell::new(Some(in_read)),
            out_write: Cell::new(Some(out_write)),
        })
    }

    /// Spawn a command attached to this ConPTY.
    pub fn spawn(
        &self,
        program: &Path,
        args: &[&str],
        env: Option<&[(&str, &str)]>,
    ) -> io::Result<ConPtyChild> {
        self.spawn_in(program, args, env, None)
    }

    /// Spawn a command attached to this ConPTY, in `cwd` when given.
    #[allow(clippy::cast_possible_truncation)]
    pub fn spawn_in(
        &self,
        program: &Path,
        args: &[&str],
        env: Option<&[(&str, &str)]>,
        cwd: Option<&Path>,
    ) -> io::Result<ConPtyChild> {
        let mut attr_size: usize = 0;
        // Query required attribute list size.
        // SAFETY: Calling with null list to query required size.
        unsafe {
            InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &raw mut attr_size);
        }

        let mut attr_storage = vec![0u8; attr_size];
        let attr_list = attr_storage.as_mut_ptr().cast();

        // SAFETY: attr_list points to allocated buffer of size attr_size.
        let init_ok =
            unsafe { InitializeProcThreadAttributeList(attr_list, 1, 0, &raw mut attr_size) };
        if init_ok == 0 {
            return Err(io::Error::last_os_error());
        }

        // SAFETY: Set the PSEUDOCONSOLE attribute to self.hpcon (passed as pointer/handle value).
        let update_ok = unsafe {
            UpdateProcThreadAttribute(
                attr_list,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
                self.hpcon as *mut std::ffi::c_void,
                std::mem::size_of::<HPCON>(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };

        if update_ok == 0 {
            // Read before the clean-up, which may set the last error itself (W32-15).
            let error = io::Error::last_os_error();
            // SAFETY: Clean up attribute list.
            unsafe { DeleteProcThreadAttributeList(attr_list) };
            return Err(error);
        }

        // Prepare STARTUPINFOEXW
        // SAFETY: Zeroed memory is a valid initial state for STARTUPINFOEXW before population.
        let mut si_ex: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
        si_ex.StartupInfo.cb = u32::try_from(std::mem::size_of::<STARTUPINFOEXW>()).unwrap_or(0);
        si_ex.StartupInfo.dwFlags = windows_sys::Win32::System::Threading::STARTF_USESTDHANDLES;
        si_ex.lpAttributeList = attr_list;

        // Build command line: program + args
        let mut cmd_line = format!("\"{}\"", program.display());
        for arg in args {
            cmd_line.push(' ');
            if arg.contains(' ') || arg.contains('\t') || arg.is_empty() {
                cmd_line.push('"');
                cmd_line.push_str(arg);
                cmd_line.push('"');
            } else {
                cmd_line.push_str(arg);
            }
        }

        let mut cmd_line_wide = crate::wide::to_wide_nul(&cmd_line);

        // Prepare environment block if given
        let env_block: Option<Vec<u16>> = env.map(crate::env::environment_block);

        let env_ptr = env_block
            .as_ref()
            .map_or(std::ptr::null(), |b| b.as_ptr().cast());

        let cwd_wide = cwd.map(crate::wide::to_wide_nul);
        let cwd_ptr = cwd_wide.as_ref().map_or(std::ptr::null(), |w| w.as_ptr());

        let mut creation_flags = EXTENDED_STARTUPINFO_PRESENT;
        if env.is_some() {
            creation_flags |= CREATE_UNICODE_ENVIRONMENT;
        }

        // SAFETY: Zeroed memory is valid for PROCESS_INFORMATION out-param.
        let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

        // SAFETY: CreateProcessW called with valid parameters and STARTUPINFOEXW.
        let create_ok = unsafe {
            CreateProcessW(
                std::ptr::null(),
                cmd_line_wide.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                FALSE,
                creation_flags,
                env_ptr,
                cwd_ptr,
                &raw mut si_ex.StartupInfo,
                &raw mut pi,
            )
        };
        // Read before the clean-up below, which may set the last error itself (W32-15).
        let create_error = (create_ok == 0).then(io::Error::last_os_error);

        // SAFETY: Clean up attribute list after process creation.
        unsafe {
            DeleteProcThreadAttributeList(attr_list);
        }

        // Now that the child process has been created attached to the pseudoconsole,
        // close the pseudoconsole pipe ends held by the parent.
        drop(self.in_read.take());
        drop(self.out_write.take());

        create_error.map_or(Ok(()), Err)?;

        // SAFETY: the process was made, so both are open handles that are ours.
        let process = unsafe { OwnedHandle::from_raw_handle(pi.hProcess) };
        // SAFETY: as above.
        let thread = unsafe { OwnedHandle::from_raw_handle(pi.hThread) };
        Ok(ConPtyChild {
            process,
            _thread: thread,
            pid: pi.dwProcessId,
        })
    }

    /// Borrow the input pipe file to write keystrokes.
    pub const fn input_mut(&mut self) -> &mut File {
        &mut self.input_write
    }

    /// Borrow the output pipe file to read console output.
    pub const fn output_mut(&mut self) -> &mut File {
        &mut self.output_read
    }
}

impl Drop for ConPty {
    fn drop(&mut self) {
        // The pseudo console's own ends go before it, as after a spawn.
        drop(self.in_read.take());
        drop(self.out_write.take());
        if self.hpcon != 0 {
            // SAFETY: ClosePseudoConsole shuts down the terminal host.
            unsafe {
                ClosePseudoConsole(self.hpcon);
            }
        }
    }
}

/// An interactive test session driving a ConPTY.
pub struct ConPtySession {
    pty: ConPty,
    child: ConPtyChild,
    accumulated_output: String,
    /// The terminal's columns and rows.
    size: (i16, i16),
}

impl ConPtySession {
    /// Start a new interactive session running the given command on a 80x25 terminal.
    pub fn start(program: &Path, args: &[&str], env: Option<&[(&str, &str)]>) -> io::Result<Self> {
        Self::start_in(program, args, env, None)
    }

    /// Like [`Self::start`], in the directory `cwd` when given.
    pub fn start_in(
        program: &Path,
        args: &[&str],
        env: Option<&[(&str, &str)]>,
        cwd: Option<&Path>,
    ) -> io::Result<Self> {
        Self::start_sized(program, args, env, cwd, 80, 25)
    }

    /// Like [`Self::start_in`], on a terminal of `cols` by `rows`.
    pub fn start_sized(
        program: &Path,
        args: &[&str],
        env: Option<&[(&str, &str)]>,
        cwd: Option<&Path>,
        cols: i16,
        rows: i16,
    ) -> io::Result<Self> {
        let pty = ConPty::new(cols, rows)?;
        let child = pty.spawn_in(program, args, env, cwd)?;
        Ok(Self {
            pty,
            child,
            accumulated_output: String::new(),
            size: (cols, rows),
        })
    }

    /// Send raw string into the pseudo terminal.
    pub fn send(&mut self, text: &str) -> io::Result<()> {
        self.pty.input_mut().write_all(text.as_bytes())?;
        self.pty.input_mut().flush()
    }

    /// Send a line into the pseudo terminal (appends `\r\n`).
    pub fn send_line(&mut self, line: &str) -> io::Result<()> {
        let mut bytes = line.as_bytes().to_vec();
        bytes.extend_from_slice(b"\r\n");
        self.pty.input_mut().write_all(&bytes)?;
        self.pty.input_mut().flush()
    }

    /// Poll and read any available bytes from the pseudo console without blocking indefinitely.
    pub fn read_available(&mut self) -> io::Result<String> {
        use std::os::windows::io::AsRawHandle;
        let handle = self.pty.output_mut().as_raw_handle();

        let mut avail: u32 = 0;
        // SAFETY: PeekNamedPipe checks available byte count safely.
        let ok = unsafe {
            PeekNamedPipe(
                handle.cast(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &raw mut avail,
                std::ptr::null_mut(),
            )
        };

        if ok == 0 {
            return Err(io::Error::last_os_error());
        }

        if avail == 0 {
            return Ok(String::new());
        }

        let mut buf = vec![0u8; avail as usize];
        let bytes_read = self.pty.output_mut().read(&mut buf)?;
        buf.truncate(bytes_read);
        let text = String::from_utf8_lossy(&buf).to_string();
        self.accumulated_output.push_str(&text);
        Ok(text)
    }

    /// Wait until `needle` appears in the output, or timeout expires.
    pub fn expect(&mut self, needle: &str, timeout: Duration) -> io::Result<()> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            let _ = self.read_available()?;
            if self.accumulated_output.contains(needle) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(15));
        }

        if self.accumulated_output.contains(needle) {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "Timed out after {timeout:?} waiting for '{needle}'. Accumulated output:\n{}",
                    self.accumulated_output
                ),
            ))
        }
    }

    /// Waits until what the console shows satisfies `shows`, or `timeout` passes: whether
    /// it did, and the screen's text. A drawing that lands in pieces is read whole, where
    /// [`Self::settle`] can stop at a pause in the middle of it on a busy machine.
    pub fn wait_for_screen(
        &mut self,
        shows: impl Fn(&str) -> bool,
        timeout: Duration,
    ) -> io::Result<(bool, String)> {
        let start = Instant::now();
        loop {
            let _ = self.read_available()?;
            let text = self.screen().text();
            if shows(&text) {
                return Ok((true, text));
            }
            if start.elapsed() >= timeout {
                return Ok((false, text));
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    /// Return the entire accumulated output seen so far.
    #[must_use]
    pub fn output(&self) -> &str {
        &self.accumulated_output
    }

    /// Reads until no output has arrived for `quiet`, or `limit` has passed: the point at
    /// which a shell has finished reacting to what was sent.
    pub fn settle(&mut self, quiet: Duration, limit: Duration) -> io::Result<()> {
        let start = Instant::now();
        let mut last = Instant::now();
        while start.elapsed() < limit {
            if self.read_available()?.is_empty() {
                if last.elapsed() >= quiet {
                    break;
                }
            } else {
                last = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        Ok(())
    }

    /// What the console shows now: the output so far, replayed onto a
    /// [`crate::vtscreen::Screen`] of the terminal's size.
    #[must_use]
    pub fn screen(&self) -> crate::vtscreen::Screen {
        let (cols, rows) = self.size;
        let mut screen = crate::vtscreen::Screen::new(
            usize::try_from(cols).unwrap_or(80),
            usize::try_from(rows).unwrap_or(25),
        );
        screen.feed(self.accumulated_output.as_bytes());
        screen
    }

    /// Wait for the child process to exit and return its exit code while draining output.
    pub fn wait(&mut self) -> io::Result<u32> {
        loop {
            let _ = self.read_available()?;
            if let Some(code) = self.child.try_wait()? {
                // Drain any final output after child termination.
                let _ = self.read_available()?;
                return Ok(code);
            }
            std::thread::sleep(Duration::from_millis(15));
        }
    }

    /// Check if child process has exited without blocking.
    pub fn try_wait(&self) -> io::Result<Option<u32>> {
        self.child.try_wait()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conpty_echo() {
        let cmd = Path::new("C:\\Windows\\System32\\cmd.exe");
        let mut session = ConPtySession::start(cmd, &["/c", "echo", "HELLO_CONPTY"], None)
            .expect("ConPtySession start failed");

        session
            .expect("HELLO_CONPTY", Duration::from_secs(5))
            .expect("did not find HELLO_CONPTY in output");

        let code = session.wait().expect("wait failed");
        assert_eq!(code, 0);
    }
}
