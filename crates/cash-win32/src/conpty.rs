//! Win32 ConPTY (Pseudo Console) support for interactive terminal testing and emulation.
//!
//! ConPTY was introduced in Windows 10 (version 1809) and allows hosting character-mode
//! command-line applications attached to a real pseudo console, with full VT100 / ANSI
//! escape sequences, raw input handling, and terminal geometry.

use std::cell::Cell;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::windows::io::FromRawHandle;
use std::path::Path;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE, INVALID_HANDLE_VALUE, S_OK};
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
    process: HANDLE,
    thread: HANDLE,
    pid: u32,
}

// SAFETY: Kernel handles are process-wide, carry no thread affinity, and are thread-safe.
unsafe impl Send for ConPtyChild {}
// SAFETY: Kernel handles are thread-safe Win32 synchronization objects.
unsafe impl Sync for ConPtyChild {}

impl ConPtyChild {
    /// Return the process ID.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.pid
    }

    /// Block until the process exits, returning its exit code.
    pub fn wait(&self) -> io::Result<u32> {
        // SAFETY: process handle is valid.
        unsafe { WaitForSingleObject(self.process, INFINITE) };
        let mut code: u32 = 0;
        // SAFETY: handle is valid and code is valid out-param.
        let ok = unsafe { GetExitCodeProcess(self.process, &raw mut code) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(code)
    }

    /// Check if the process has exited without blocking.
    pub fn try_wait(&self) -> io::Result<Option<u32>> {
        let mut code: u32 = 0;
        // SAFETY: handle is valid and code is valid out-param.
        let ok = unsafe { GetExitCodeProcess(self.process, &raw mut code) };
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

impl Drop for ConPtyChild {
    fn drop(&mut self) {
        if self.process != INVALID_HANDLE_VALUE && !self.process.is_null() {
            // SAFETY: Valid open process handle being closed.
            unsafe {
                CloseHandle(self.process);
            }
        }
        if self.thread != INVALID_HANDLE_VALUE && !self.thread.is_null() {
            // SAFETY: Valid open thread handle being closed.
            unsafe {
                CloseHandle(self.thread);
            }
        }
    }
}

/// A native Win32 Pseudo Console (ConPTY).
pub struct ConPty {
    hpcon: HPCON,
    input_write: File,
    output_read: File,
    in_read: Cell<Option<HANDLE>>,
    out_write: Cell<Option<HANDLE>>,
}

impl ConPty {
    /// Creates a new ConPTY with the specified dimensions (columns and rows).
    pub fn new(cols: i16, rows: i16) -> io::Result<Self> {
        let mut in_read: HANDLE = std::ptr::null_mut();
        let mut in_write: HANDLE = std::ptr::null_mut();
        let mut out_read: HANDLE = std::ptr::null_mut();
        let mut out_write: HANDLE = std::ptr::null_mut();

        // Create pipe for sending input to ConPTY.
        // SAFETY: standard CreatePipe call with valid out params.
        let ok_in = unsafe { CreatePipe(&raw mut in_read, &raw mut in_write, std::ptr::null(), 0) };
        if ok_in == 0 {
            return Err(io::Error::last_os_error());
        }

        // Create pipe for receiving output from ConPTY.
        // SAFETY: standard CreatePipe call with valid out params.
        let ok_out =
            unsafe { CreatePipe(&raw mut out_read, &raw mut out_write, std::ptr::null(), 0) };
        if ok_out == 0 {
            // SAFETY: Clean up handle on error.
            unsafe { CloseHandle(in_read) };
            // SAFETY: Clean up handle on error.
            unsafe { CloseHandle(in_write) };
            return Err(io::Error::last_os_error());
        }

        let size = COORD { X: cols, Y: rows };
        let mut hpcon: HPCON = 0;

        // SAFETY: CreatePseudoConsole takes in_read and out_write.
        let hr = unsafe { CreatePseudoConsole(size, in_read, out_write, 0, &raw mut hpcon) };

        if hr != S_OK {
            // SAFETY: Clean up handles on error.
            unsafe { CloseHandle(in_read) };
            // SAFETY: Clean up handles on error.
            unsafe { CloseHandle(in_write) };
            // SAFETY: Clean up handles on error.
            unsafe { CloseHandle(out_read) };
            // SAFETY: Clean up handles on error.
            unsafe { CloseHandle(out_write) };
            return Err(io::Error::from_raw_os_error(hr));
        }

        // SAFETY: in_write is a valid open pipe handle.
        let input_write = unsafe { File::from_raw_handle(in_write.cast()) };
        // SAFETY: out_read is a valid open pipe handle.
        let output_read = unsafe { File::from_raw_handle(out_read.cast()) };

        Ok(Self {
            hpcon,
            input_write,
            output_read,
            in_read: Cell::new(Some(in_read)),
            out_write: Cell::new(Some(out_write)),
        })
    }

    /// Spawn a command attached to this ConPTY.
    #[allow(clippy::cast_possible_truncation)]
    pub fn spawn(
        &self,
        program: &Path,
        args: &[&str],
        env: Option<&[(&str, &str)]>,
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
            // SAFETY: Clean up attribute list.
            unsafe { DeleteProcThreadAttributeList(attr_list) };
            return Err(io::Error::last_os_error());
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

        let mut cmd_line_wide: Vec<u16> =
            cmd_line.encode_utf16().chain(std::iter::once(0)).collect();

        // Prepare environment block if given
        let env_block: Option<Vec<u16>> = env.map(|vars| {
            let mut block = Vec::new();
            for (k, v) in vars {
                block.extend(format!("{k}={v}").encode_utf16());
                block.push(0);
            }
            block.push(0);
            block
        });

        let env_ptr = env_block
            .as_ref()
            .map_or(std::ptr::null(), |b| b.as_ptr().cast());

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
                std::ptr::null(),
                &raw mut si_ex.StartupInfo,
                &raw mut pi,
            )
        };

        // SAFETY: Clean up attribute list after process creation.
        unsafe {
            DeleteProcThreadAttributeList(attr_list);
        }

        // Now that the child process has been created attached to the pseudoconsole,
        // close the pseudoconsole pipe ends held by the parent.
        if let Some(in_r) = self.in_read.take() {
            // SAFETY: Close redundant in_read handle.
            unsafe { CloseHandle(in_r) };
        }
        if let Some(out_w) = self.out_write.take() {
            // SAFETY: Close redundant out_write handle.
            unsafe { CloseHandle(out_w) };
        }

        if create_ok == 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(ConPtyChild {
            process: pi.hProcess,
            thread: pi.hThread,
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
        if let Some(in_r) = self.in_read.take() {
            // SAFETY: Close redundant in_read handle.
            unsafe { CloseHandle(in_r) };
        }
        if let Some(out_w) = self.out_write.take() {
            // SAFETY: Close redundant out_write handle.
            unsafe { CloseHandle(out_w) };
        }
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
}

impl ConPtySession {
    /// Start a new interactive session running the given command on a 80x25 terminal.
    pub fn start(program: &Path, args: &[&str], env: Option<&[(&str, &str)]>) -> io::Result<Self> {
        let pty = ConPty::new(80, 25)?;
        let child = pty.spawn(program, args, env)?;
        Ok(Self {
            pty,
            child,
            accumulated_output: String::new(),
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

    /// Return the entire accumulated output seen so far.
    #[must_use]
    pub fn output(&self) -> &str {
        &self.accumulated_output
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
    #[allow(clippy::expect_used)]
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
