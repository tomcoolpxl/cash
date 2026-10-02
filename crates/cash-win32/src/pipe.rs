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

/// Whether `path` is the named pipe of a process substitution:
/// `\\.\pipe\cash-procsub-…`, in either slash.
#[must_use]
pub fn is_substitution(path: &std::path::Path) -> bool {
    let path = path.to_string_lossy().replace('/', "\\");
    path.get(..PIPE_PREFIX.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(PIPE_PREFIX))
}

/// Where every process substitution's named pipe is.
const PIPE_PREFIX: &str = r"\\.\pipe\cash-procsub-";

/// Opens `path` for writing as `options` say; a process substitution's pipe, as a pipe
/// can be opened (D17).
///
/// A program that writes a file creates it, truncates it or appends to it, and a named
/// pipe allows none of that: the open fails with "The parameter is incorrect", or with
/// "Access is denied" for appending, and `tee >(cmd)` failed so (2026-09-30). A pipe is
/// opened for writing as it is, which is what each of those means for one. cash's own
/// opens and the bundled tools that write a file they are named go through here.
///
/// # Errors
///
/// Returns the error of the open.
pub fn open_output(options: &std::fs::OpenOptions, path: &std::path::Path) -> io::Result<File> {
    if is_substitution(path) {
        std::fs::OpenOptions::new().write(true).open(path)
    } else {
        options.open(path)
    }
}

/// Gives the end of its input to the write substitution listening at `path`.
///
/// If no program has opened the path, this opens it as a program would, writes nothing
/// and closes it. When one has opened it, the one instance the pipe has is taken, the
/// open fails, and nothing happens.
pub fn release_unclaimed(path: &str) {
    let _ = std::fs::OpenOptions::new().write(true).open(path);
}

/// A write process substitution (`>(cmd)`) handed to a program as a file: see
/// [`follow_file`].
pub struct FollowedFile {
    /// What the program writes into the file, for the substitution's standard input.
    pub reader: PipeReader,
    /// To be raised once the program is done with the file: what the file holds then is
    /// the rest of the substitution's input.
    pub done: Arc<AtomicBool>,
}

/// Sets up a write process substitution (`>(cmd)`) as the file at `path`, which the
/// substitution reads as it is written (D17).
///
/// A program opens a file it is to write as a new one, creating or truncating it
/// (`open(path, 'w')`), which the named pipe of [`create_write_substitution`] does not
/// allow, and a program on `PATH` cannot be made to ask differently. A file allows it.
/// What is written is read as it comes, a moment later, until [`FollowedFile::done`]
/// says the program is done; then the rest, and the substitution's input ends. The file
/// is deleted then.
///
/// The file is read through a handle that only reads, so that a program that lets
/// others read its file but not write it (.NET's default) can still open it. What has
/// been read is given back to the disk as the file grows, so that a program that writes
/// for hours does not fill it; also when the substitution has stopped reading.
///
/// # Errors
///
/// Returns an error if the file or the pipe cannot be made, or the thread started.
pub fn follow_file(path: &std::path::Path) -> io::Result<FollowedFile> {
    File::create(path)?;
    let file = File::open(path)?;
    let (reader, writer) = io::pipe()?;
    let done = Arc::new(AtomicBool::new(false));

    let follower = Follower {
        file,
        path: path.to_owned(),
        writer: Some(writer),
        done: Arc::clone(&done),
        read: 0,
        released: 0,
    };
    std::thread::Builder::new()
        .name("cash-psub-file".into())
        .spawn(move || follower.run())?;

    Ok(FollowedFile { reader, done })
}

/// The reading side of a [`FollowedFile`].
struct Follower {
    file: File,
    path: std::path::PathBuf,
    /// The substitution's input; `None` once it has stopped reading.
    writer: Option<PipeWriter>,
    done: Arc<AtomicBool>,
    /// How much of the file has been read.
    read: u64,
    /// How much of it has been given back to the disk.
    released: u64,
}

impl Follower {
    /// How long to wait for more when the file has been read to its end.
    const POLL: std::time::Duration = std::time::Duration::from_millis(15);

    /// How much is read between two releases of disk space.
    const RELEASE_EVERY: u64 = 1 << 20;

    fn run(mut self) {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            // Whether the program was done before this read: all it wrote is in the file
            // then, and a read that finds nothing more is the end.
            let done = self.done.load(Ordering::SeqCst);
            match self.file.read(&mut buf) {
                Ok(0) if done => break,
                Ok(0) => std::thread::sleep(Self::POLL),
                Ok(n) => {
                    if let Some(writer) = &mut self.writer
                        && writer.write_all(&buf[..n]).is_err()
                    {
                        self.writer = None;
                    }
                    self.read += n as u64;
                    if self.read - self.released >= Self::RELEASE_EVERY {
                        release_space(&self.path, self.read);
                        self.released = self.read;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        let Self {
            file, path, writer, ..
        } = self;
        drop(file);
        let _ = std::fs::remove_file(path);
        // The substitution's input ends here, once the file is gone: a shell waiting for
        // the substitution finds nothing of it left.
        drop(writer);
    }
}

/// Gives the disk space of the first `upto` bytes of the file at `path` back, keeping
/// the file's size: it is made sparse, and that range a hole.
///
/// Through a handle of its own, which writes: if the program writing the file lets no one
/// else write it, nothing is given back.
fn release_space(path: &std::path::Path, upto: u64) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::{FILE_ZERO_DATA_INFORMATION, FSCTL_SET_ZERO_DATA};

    let Ok(file) = std::fs::OpenOptions::new().write(true).open(path) else {
        return;
    };
    let Ok(upto) = i64::try_from(upto) else {
        return;
    };
    // The file is set sparse again each time, as a program truncating it may have
    // cleared that.
    if make_sparse(&file).is_err() {
        return;
    }
    let mut returned = 0u32;
    let range = FILE_ZERO_DATA_INFORMATION {
        FileOffset: 0,
        BeyondFinalZero: upto,
    };
    // SAFETY: the handle is open for writing; `range` is valid for the call, and no
    // output buffer is asked for.
    unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_SET_ZERO_DATA,
            (&raw const range).cast(),
            u32::try_from(size_of::<FILE_ZERO_DATA_INFORMATION>()).unwrap_or(0),
            std::ptr::null_mut(),
            0,
            &raw mut returned,
            std::ptr::null_mut(),
        )
    };
}

/// Makes `file`, open for writing, sparse: a range never written, or made a hole, takes no
/// disk space and reads as zeros.
///
/// # Errors
///
/// Returns the error of the call, as on a volume that keeps no sparse files (FAT).
pub(crate) fn make_sparse(file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;

    let mut returned = 0u32;
    // SAFETY: the handle is open; no buffer is passed or asked for.
    let done = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_SET_SPARSE,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &raw mut returned,
            std::ptr::null_mut(),
        )
    };
    if done == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use super::*;

    /// How much of the disk the file at `path` takes.
    fn allocated(path: &Path) -> u64 {
        use windows_sys::Win32::Storage::FileSystem::GetCompressedFileSizeW;

        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut high = 0u32;
        // SAFETY: `wide` is null-terminated and `high` a valid out-param.
        let low = unsafe { GetCompressedFileSizeW(wide.as_ptr(), &raw mut high) };
        (u64::from(high) << 32) | u64::from(low)
    }

    #[test]
    fn a_followed_file_is_read_as_it_is_written_and_to_its_end_once_done() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("substitution");
        let followed = follow_file(&path).unwrap();
        let mut reader = followed.reader;

        // The program opens it as a new file, truncating it, while it is followed.
        let mut program = File::create(&path).unwrap();
        program.write_all(b"first\n").unwrap();
        let mut first = [0u8; 6];
        reader.read_exact(&mut first).unwrap();
        assert_eq!(&first, b"first\n", "not read before the program was done");

        program.write_all(b"last\n").unwrap();
        drop(program);
        followed.done.store(true, Ordering::SeqCst);
        let mut rest = String::new();
        reader.read_to_string(&mut rest).unwrap();
        assert_eq!(rest, "last\n");
        assert!(!path.exists(), "the file was left behind");
    }

    #[test]
    fn what_has_been_read_of_a_followed_file_is_given_back_to_the_disk() {
        const MIB: usize = 1 << 20;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("substitution");
        let followed = follow_file(&path).unwrap();
        let mut reader = followed.reader;

        let mut program = File::create(&path).unwrap();
        let chunk = vec![b'x'; MIB];
        let mut buf = vec![0u8; MIB];
        for _ in 0..8 {
            program.write_all(&chunk).unwrap();
            reader.read_exact(&mut buf).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while allocated(&path) > 2 * MIB as u64 {
            assert!(
                Instant::now() < deadline,
                "8 MiB read, and {} bytes still on the disk",
                allocated(&path)
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            program.metadata().unwrap().len(),
            8 * MIB as u64,
            "the file's size changed under the program"
        );

        drop(program);
        followed.done.store(true, Ordering::SeqCst);
        let mut rest = Vec::new();
        reader.read_to_end(&mut rest).unwrap();
        assert!(rest.is_empty());
    }
}
