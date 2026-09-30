//! Process substitution streaming via Win32 Named Pipes (**D17**).
//!
//! Windows has no `/dev/fd` filesystem and child processes cannot inherit arbitrary file
//! descriptors beyond standard I/O (D26). Previously, cash fell back to materialising
//! subshell output into a temporary file on disk under `%TEMP%`.
//!
//! That approach had two severe flaws:
//! 1. **No streaming**: Subshells were executed to completion synchronously before the
//!    consuming command even started. Infinite streams (such as `tail -f` or `seq 1 1000000`)
//!    hung forever.
//! 2. **No write substitution**: `>(...)` was impossible because the subshell would need
//!    to consume output after the command started, which the temp-file model could not do.
//!
//! This module replaces temp files with native Win32 Named Pipes:
//! `\\.\pipe\cash-procsub-<pid>-<counter>`.
//!
//! Because Windows Named Pipes reside in the kernel object namespace (`\Device\NamedPipe`),
//! they stream in memory with zero disk I/O. Any native program (`cat`, `diff`, `grep`,
//! `python`, `git`) can open `\\.\pipe\...` paths via standard `CreateFileW` / `fopen`.
//!
//! Windows coreutils tools (`cat`, `diff`, `grep`) typically execute two opens:
//! an initial metadata probe via `FILE_READ_ATTRIBUTES` (to check `is_dir()`) followed
//! immediately by `GENERIC_READ`. We create dual server instances so both the metadata
//! probe and the actual data consumer connect without encountering `ERROR_PIPE_BUSY`.

use std::fs::File;
use std::io::{self, PipeReader, PipeWriter, Read, Write};
use std::os::windows::io::FromRawHandle;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_PIPE_CONNECTED, GetLastError, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND, PIPE_ACCESS_OUTBOUND,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
};

static COUNTER: AtomicU64 = AtomicU64::new(1);

/// Buffer size for the named pipe kernel buffer (64 KiB).
const PIPE_BUFFER_SIZE: u32 = 65536;

/// Generates a unique Win32 Named Pipe path for process substitution.
fn generate_pipe_path() -> (String, Vec<u16>) {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = format!(r"\\.\pipe\cash-procsub-{}-{}", std::process::id(), id);
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    (path, wide)
}

/// Helper that waits for a client to connect to an overlapped named pipe server handle.
///
/// Returns `true` if a client connected successfully, or `false` if the timeout elapsed,
/// an error occurred, or the operation was cancelled.
///
/// Helper that waits for a client to connect to a named pipe server handle.
///
/// # Safety
///
/// `server_handle` must be an open, valid named pipe server handle.
unsafe fn wait_for_client_connection(
    server_handle: windows_sys::Win32::Foundation::HANDLE,
) -> bool {
    // SAFETY: caller guarantees `server_handle` is an open, valid named pipe server handle.
    let connected = unsafe { ConnectNamedPipe(server_handle, std::ptr::null_mut()) };
    if connected != 0 {
        return true;
    }
    // SAFETY: GetLastError is always safe to call immediately following Win32 API failure.
    let err = unsafe { GetLastError() };
    err == ERROR_PIPE_CONNECTED || err == windows_sys::Win32::Foundation::ERROR_NO_DATA
}

/// Helper that creates a named pipe server instance.
unsafe fn create_pipe_instance(
    wide_path: &[u16],
    access: u32,
    is_first: bool,
    max_instances: u32,
) -> io::Result<windows_sys::Win32::Foundation::HANDLE> {
    let mut flags = access;
    if is_first {
        flags |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }

    // SAFETY: `wide_path` is null-terminated and valid for the call duration.
    let handle = unsafe {
        CreateNamedPipeW(
            wide_path.as_ptr(),
            flags,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            max_instances,
            PIPE_BUFFER_SIZE,
            PIPE_BUFFER_SIZE,
            0,
            std::ptr::null(),
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(handle)
    }
}

/// Spawns a background pump thread for a read substitution instance.
fn spawn_read_instance(
    handle: windows_sys::Win32::Foundation::HANDLE,
    reader: Arc<Mutex<PipeReader>>,
    replay_buffer: Arc<Mutex<Vec<u8>>>,
    reader_eof: Arc<AtomicBool>,
    name: &'static str,
) -> io::Result<()> {
    let handle_val = handle as usize;
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let handle = handle_val as windows_sys::Win32::Foundation::HANDLE;
            // SAFETY: `handle` is an open named pipe server handle transferred to this thread.
            let connected = unsafe { wait_for_client_connection(handle) };
            if connected {
                // SAFETY: `handle` is owned and connected.
                let mut server_file = unsafe { File::from_raw_handle(handle.cast()) };
                let mut cursor = 0;
                let mut streamed_any = false;

                loop {
                    let next_chunk = {
                        if let Ok(buf) = replay_buffer.lock() {
                            if cursor < buf.len() {
                                let chunk = buf[cursor..].to_vec();
                                cursor = buf.len();
                                Some(chunk)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };

                    if let Some(chunk) = next_chunk {
                        if server_file.write_all(&chunk).is_err() {
                            break;
                        }
                        streamed_any = true;
                        continue;
                    }

                    if reader_eof.load(Ordering::Relaxed) {
                        break;
                    }

                    if let Ok(mut r) = reader.lock() {
                        if reader_eof.load(Ordering::Relaxed) {
                            continue;
                        }

                        let mut buf = [0u8; 8192];
                        match r.read(&mut buf) {
                            Ok(0) => {
                                reader_eof.store(true, Ordering::Relaxed);
                            }
                            Ok(n) => {
                                let chunk = &buf[..n];
                                let to_send = if let Ok(mut shared) = replay_buffer.lock() {
                                    let old_cursor = cursor;
                                    shared.extend_from_slice(chunk);
                                    cursor = shared.len();
                                    shared[old_cursor..].to_vec()
                                } else {
                                    cursor += n;
                                    chunk.to_vec()
                                };
                                if server_file.write_all(&to_send).is_err() {
                                    break;
                                }
                                streamed_any = true;
                            }
                            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                            Err(_) => {
                                reader_eof.store(true, Ordering::Relaxed);
                            }
                        }
                    } else {
                        break;
                    }
                }

                if streamed_any {
                    let _ = server_file.flush();
                }
            } else {
                // SAFETY: Client never connected; close handle.
                unsafe { CloseHandle(handle) };
            }
        })?;
    Ok(())
}

/// Spawns a background pump thread for a write substitution instance.
fn spawn_write_instance(
    handle: windows_sys::Win32::Foundation::HANDLE,
    mut writer: PipeWriter,
    name: &'static str,
) -> io::Result<()> {
    let handle_val = handle as usize;
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let handle = handle_val as windows_sys::Win32::Foundation::HANDLE;
            // SAFETY: `handle` is an open named pipe server handle transferred to this thread.
            let connected = unsafe { wait_for_client_connection(handle) };
            if connected {
                // SAFETY: `handle` is owned and connected.
                let mut server_file = unsafe { File::from_raw_handle(handle.cast()) };
                let mut buf = [0u8; 8192];
                loop {
                    match server_file.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if writer.write_all(&buf[..n]).is_err() {
                                break;
                            }
                        }
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
                let _ = writer.flush();
            } else {
                // SAFETY: Client never connected; close handle.
                unsafe { CloseHandle(handle) };
            }
            // `writer` drops here, closing the pipe so subshell receives EOF immediately.
        })?;
    Ok(())
}

/// Result of setting up a read process substitution (`<(cmd)`).
pub struct ReadSubstitution {
    /// The Win32 Named Pipe path to pass to the consuming command.
    pub path: String,
    /// The write end of an anonymous pipe to install as the subshell's standard output.
    pub writer: PipeWriter,
}

/// Sets up a streaming read process substitution (`<(cmd)`).
///
/// Returns the rendered pipe path (e.g. `\\.\pipe\cash-procsub-...`) and a `PipeWriter`
/// to be set as the subshell's `STDOUT_FD`. Output written by the subshell is streamed
/// live to whichever process opens the named pipe.
///
/// # Errors
///
/// Returns an error if the named pipe server or anonymous pipe cannot be created.
pub fn create_read_substitution() -> io::Result<ReadSubstitution> {
    let (path, wide_path) = generate_pipe_path();

    // SAFETY: Creating named pipe server instance 1.
    let h1 = unsafe { create_pipe_instance(&wide_path, PIPE_ACCESS_OUTBOUND, true, 2)? };
    // SAFETY: Creating named pipe server instance 2.
    let h2 = match unsafe { create_pipe_instance(&wide_path, PIPE_ACCESS_OUTBOUND, false, 2) } {
        Ok(h) => h,
        Err(e) => {
            // SAFETY: Clean up h1 on failure.
            unsafe { CloseHandle(h1) };
            return Err(e);
        }
    };

    let (pipe_reader, pipe_writer) = io::pipe()?;

    let reader_shared = Arc::new(Mutex::new(pipe_reader));
    let replay_buffer = Arc::new(Mutex::new(Vec::<u8>::new()));
    let reader_eof = Arc::new(AtomicBool::new(false));

    spawn_read_instance(
        h1,
        Arc::clone(&reader_shared),
        Arc::clone(&replay_buffer),
        Arc::clone(&reader_eof),
        "cash-psub-read-1",
    )?;
    spawn_read_instance(
        h2,
        Arc::clone(&reader_shared),
        Arc::clone(&replay_buffer),
        Arc::clone(&reader_eof),
        "cash-psub-read-2",
    )?;

    Ok(ReadSubstitution {
        path,
        writer: pipe_writer,
    })
}

/// Gives the end of its input to the write substitution listening at `path`.
///
/// If no program has opened the path, this opens it as a program would, writes nothing
/// and closes it. When one has opened it, the one instance the pipe has is taken, the
/// open fails, and nothing happens.
pub fn release_unclaimed(path: &str) {
    let _ = std::fs::OpenOptions::new().write(true).open(path);
}

/// Result of setting up a write process substitution (`>(cmd)`).
pub struct WriteSubstitution {
    /// The Win32 Named Pipe path to pass to the producing command.
    pub path: String,
    /// The read end of an anonymous pipe to install as the subshell's standard input.
    pub reader: PipeReader,
}

/// Sets up a streaming write process substitution (`>(cmd)`).
///
/// Returns the rendered pipe path (e.g. `\\.\pipe\cash-procsub-...`) and a `PipeReader`
/// to be set as the subshell's `STDIN_FD`. Data written to the named pipe is streamed
/// live into the subshell's standard input.
///
/// # Errors
///
/// Returns an error if the named pipe server or anonymous pipe cannot be created.
pub fn create_write_substitution() -> io::Result<WriteSubstitution> {
    let (path, wide_path) = generate_pipe_path();

    // SAFETY: Creating named pipe server instance (1 instance for write substitution).
    let h = unsafe { create_pipe_instance(&wide_path, PIPE_ACCESS_INBOUND, true, 1)? };

    let (pipe_reader, pipe_writer) = io::pipe()?;

    spawn_write_instance(h, pipe_writer, "cash-psub-write")?;

    Ok(WriteSubstitution {
        path,
        reader: pipe_reader,
    })
}
