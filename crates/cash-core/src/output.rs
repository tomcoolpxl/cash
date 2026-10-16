//! A builtin's standard output, gathered into larger writes.
//!
//! Each write a builtin made went to its file at once: into a pipe or a file, every piece
//! of a `write!` was a system call of its own; to a console, `std::io::Stdout` wrote a
//! line at a time, and each line costs Windows Terminal's pseudo console about half a
//! millisecond. `ls -la` in a folder of 200 files took twice as long to appear as it
//! took to make.
//!
//! The output of a builtin now collects in a buffer its execution parameters hold for the
//! length of the call, which every handle [`ExecutionContext::stdout`] gives out shares.
//! What has collected is written:
//!
//! - once 64 KiB have collected, as C's standard output writes into a pipe or a file;
//! - before anything else could show or take its turn: whenever the builtin's standard
//!   error is written, its standard input read, or any of its descriptors asked for, as
//!   a program started or a command run asks for them;
//! - when the builtin asks for it (`flush`), and when it returns;
//! - once its oldest byte has waited 50 ms or so, by a thread that watches the buffers.
//!   A builtin that hangs, or works slowly, still shows what it wrote, on a terminal and
//!   through a pipe or a file alike.
//!
//! A builtin that runs until it is stopped, or draws a screen and waits for keys (`top`,
//! `watch`, `nc`, a pager), is registered as streaming its output
//! ([`Registration::streams_output`]) and writes straight through.
//!
//! [`ExecutionContext::stdout`]: crate::ExecutionContext::stdout
//! [`Registration::streams_output`]: crate::builtins::Registration::streams_output

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::time::Duration;

use crate::openfiles::OpenFile;

/// How much output collects before it is written.
pub(crate) const CAPACITY: usize = 64 * 1024;

/// How long output waits at most.
pub(crate) const DELAY: Duration = Duration::from_millis(50);

/// Milliseconds since Windows started, to the tick (10 to 16 ms): enough for a 50 ms
/// limit, and read from memory Windows keeps up to date, where `Instant::now` asks the
/// performance counter, for each builtin call that writes.
fn now() -> u64 {
    u64::try_from(cash_win32::sysinfo::uptime().as_millis()).unwrap_or(u64::MAX)
}

/// The output of the builtin running, and of the builtins it runs that write to the same
/// file, waiting to be written.
pub struct OutputBuffer {
    /// The file the output goes to, to tell whether a handle asked for is this one's.
    target: OpenFile,
    state: Mutex<State>,
    /// Whether output waits, to be looked at without the lock.
    pending: AtomicBool,
    /// Whether anything was written through the buffer: most calls of `:`, `[` or `cd`
    /// write nothing, and end without taking the lock.
    used: AtomicBool,
    /// Whether the call the buffer was made for has returned: what a handle that outlived
    /// it writes goes straight through.
    done: AtomicBool,
    /// How long output waits at most: [`DELAY`].
    delay: Duration,
}

struct State {
    file: OpenFile,
    bytes: Vec<u8>,
    /// When the oldest byte waiting came, by [`now`].
    since: Option<u64>,
    /// Whether the watching thread knows of this buffer.
    watched: bool,
    /// What went wrong writing where no one could be told, for the call's end to report.
    error: Option<std::io::Error>,
}

impl OutputBuffer {
    /// A buffer for output going to `file`.
    pub(crate) fn new(file: OpenFile) -> Arc<Self> {
        Self::with_delay(file, DELAY)
    }

    fn with_delay(file: OpenFile, delay: Duration) -> Arc<Self> {
        Arc::new(Self {
            target: file.clone(),
            state: Mutex::new(State {
                file,
                bytes: Vec::new(),
                since: None,
                watched: false,
                error: None,
            }),
            pending: AtomicBool::new(false),
            used: AtomicBool::new(false),
            done: AtomicBool::new(false),
            delay,
        })
    }

    /// Whether this collects the output of `file`.
    pub(crate) fn is_for(&self, file: &OpenFile) -> bool {
        self.target.same_file(file)
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Writes what waits, and drops it whether or not that worked: a writer that failed
    /// once is told so, not handed the same bytes again. Flushed too, as the console's
    /// `std::io::Stdout` keeps a line that has no end yet, a prompt.
    fn write_out(&self, state: &mut State) -> std::io::Result<()> {
        state.since = None;
        self.pending.store(false, Ordering::Release);
        if state.bytes.is_empty() {
            return Ok(());
        }
        let result = state
            .file
            .write_all(&state.bytes)
            .and_then(|()| state.file.flush());
        state.bytes.clear();
        if state.bytes.capacity() > CAPACITY {
            state.bytes = Vec::new();
        }
        result
    }

    /// Writes what waits, before something else takes its turn; what goes wrong is kept
    /// for the call's end to report.
    pub(crate) fn write_pending(&self) {
        if !self.pending.load(Ordering::Acquire) {
            return;
        }
        let mut state = self.lock();
        if let Err(error) = self.write_out(&mut state) {
            state.error.get_or_insert(error);
        }
    }

    /// Writes what waits as the call `made_by_this_call` ends: when that call made the
    /// buffer, what comes after goes straight through. Says what went wrong writing since
    /// the call began.
    pub(crate) fn finish(&self, made_by_this_call: bool) -> std::io::Result<()> {
        // `done` before `used`: a write that set `used` too late for this look sees
        // `done`, and goes straight through.
        if made_by_this_call {
            self.done.store(true, Ordering::SeqCst);
        }
        if !self.used.load(Ordering::SeqCst) {
            return Ok(());
        }
        let mut state = self.lock();
        let written = self.write_out(&mut state).and_then(|()| state.file.flush());
        match state.error.take() {
            Some(error) => Err(error),
            None => written,
        }
    }

    /// Writes what has waited too long, unless the builtin is writing right now; whether
    /// this is still worth watching.
    fn tick(&self) -> bool {
        let Ok(mut state) = self.state.try_lock() else {
            return true;
        };
        let waited = |since: u64| u128::from(now().saturating_sub(since));
        if state
            .since
            .is_some_and(|since| waited(since) >= self.delay.as_millis())
        {
            if let Err(error) = self.write_out(&mut state) {
                state.error.get_or_insert(error);
            }
        }
        state.watched = !state.bytes.is_empty();
        state.watched
    }
}

impl Drop for OutputBuffer {
    fn drop(&mut self) {
        let state = self.state.get_mut().unwrap_or_else(PoisonError::into_inner);
        if !state.bytes.is_empty() {
            let _ = state.file.write_all(&state.bytes);
            let _ = state.file.flush();
        }
    }
}

/// A handle to a builtin's standard output ([`ExecutionContext::stdout`]).
///
/// [`ExecutionContext::stdout`]: crate::ExecutionContext::stdout
pub enum Stdout {
    /// Into the buffer of the builtin running.
    Buffered(Arc<OutputBuffer>),
    /// Straight to the file: no builtin's output is gathered, or this is another file.
    Direct(OpenFile),
}

impl Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let buffer = match self {
            Self::Direct(file) => return file.write(buf),
            Self::Buffered(buffer) => buffer,
        };
        if !buffer.used.load(Ordering::Relaxed) {
            buffer.used.store(true, Ordering::SeqCst);
        }
        let mut state = buffer.lock();
        if buffer.done.load(Ordering::SeqCst) {
            buffer.write_out(&mut state)?;
            return state.file.write(buf);
        }
        if state.bytes.len() + buf.len() > CAPACITY {
            buffer.write_out(&mut state)?;
            if buf.len() >= CAPACITY {
                return state.file.write(buf);
            }
        }
        if state.bytes.is_empty() {
            state.since = Some(now());
            buffer.pending.store(true, Ordering::Release);
        }
        state.bytes.extend_from_slice(buf);

        // The watching thread writes what waits too long.
        if !state.watched {
            state.watched = true;
            drop(state);
            watch(Arc::downgrade(buffer));
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Direct(file) => file.flush(),
            Self::Buffered(buffer) => {
                let mut state = buffer.lock();
                buffer.write_out(&mut state)?;
                state.file.flush()
            }
        }
    }
}

/// A builtin's standard input, or its standard error, used after its output.
///
/// Whatever the builtin's standard output has gathered is written before each read or
/// write, so that a prompt shows before the builtin waits for its answer, and a message
/// after the output that came before it.
pub struct AfterOutput {
    file: OpenFile,
    output: Option<Arc<OutputBuffer>>,
}

impl AfterOutput {
    pub(crate) const fn new(file: OpenFile, output: Option<Arc<OutputBuffer>>) -> Self {
        Self { file, output }
    }

    fn write_output(&self) {
        if let Some(output) = &self.output {
            output.write_pending();
        }
    }
}

impl Read for AfterOutput {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.write_output();
        self.file.read(buf)
    }
}

impl Write for AfterOutput {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write_output();
        self.file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

/// The buffers whose output waits, and the thread that writes what has waited too long.
struct Watcher {
    buffers: Mutex<Vec<Weak<OutputBuffer>>>,
    wake: Condvar,
}

fn watch(buffer: Weak<OutputBuffer>) {
    static WATCHER: OnceLock<Option<&'static Watcher>> = OnceLock::new();
    let watcher = WATCHER.get_or_init(|| {
        let watcher: &'static Watcher = Box::leak(Box::new(Watcher {
            buffers: Mutex::new(Vec::new()),
            wake: Condvar::new(),
        }));
        std::thread::Builder::new()
            .name("output-watcher".to_owned())
            .spawn(move || watcher.run())
            .ok()
            .map(|_| watcher)
    });
    // Without the thread, output that waits shows when something else would take its
    // turn, or when the builtin returns.
    if let Some(watcher) = watcher {
        let mut buffers = watcher.lock();
        buffers.push(buffer);
        // The thread sleeps only while it has nothing to watch; otherwise it looks
        // again soon by itself, and a loop of `echo` does not wake it each time.
        if buffers.len() == 1 {
            watcher.wake.notify_one();
        }
    }
}

impl Watcher {
    fn lock(&self) -> MutexGuard<'_, Vec<Weak<OutputBuffer>>> {
        self.buffers.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn run(&self) {
        loop {
            let mut buffers = self.lock();
            while buffers.is_empty() {
                buffers = self
                    .wake
                    .wait(buffers)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            drop(buffers);
            std::thread::sleep(DELAY / 2);
            // Looked at without the list's lock, which a builtin's first write takes:
            // most are gone by now, one for each `echo` of a loop.
            let watched = std::mem::take(&mut *self.lock());
            let still: Vec<_> = watched
                .into_iter()
                .filter(|buffer| buffer.upgrade().is_some_and(|buffer| buffer.tick()))
                .collect();
            if !still.is_empty() {
                self.lock().extend(still);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn file_in(dir: &tempfile::TempDir, name: &str) -> (std::path::PathBuf, OpenFile) {
        let path = dir.path().join(name);
        let file = std::fs::File::create(&path).unwrap();
        (path, file.into())
    }

    /// A buffer that keeps its output until it is told, whatever the machine's load.
    fn held(file: OpenFile) -> Arc<OutputBuffer> {
        OutputBuffer::with_delay(file, Duration::from_secs(3600))
    }

    #[test]
    fn output_waits_until_the_call_ends() {
        let dir = tempfile::tempdir().unwrap();
        let (path, file) = file_in(&dir, "out");
        let buffer = held(file);
        let mut stdout = Stdout::Buffered(Arc::clone(&buffer));
        write!(stdout, "one ").unwrap();
        write!(Stdout::Buffered(Arc::clone(&buffer)), "two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
        buffer.finish(true).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one two");
        // A handle that outlived the call writes straight through.
        write!(stdout, " three").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one two three");
    }

    #[test]
    fn output_waits_no_longer_than_the_delay() {
        let dir = tempfile::tempdir().unwrap();
        let (path, file) = file_in(&dir, "out");
        let buffer = OutputBuffer::new(file);
        write!(Stdout::Buffered(Arc::clone(&buffer)), "working...").unwrap();
        // The builtin writes nothing more for now: the watching thread writes it.
        let deadline = Instant::now() + Duration::from_secs(10);
        while std::fs::read_to_string(&path).unwrap().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "working...");
        assert_eq!(buffer.lock().bytes.len(), 0);
    }

    #[test]
    fn a_message_or_a_read_comes_after_the_output_before_it() {
        let dir = tempfile::tempdir().unwrap();
        let (path, file) = file_in(&dir, "out");
        let buffer = held(file.clone());
        writeln!(Stdout::Buffered(Arc::clone(&buffer)), "out").unwrap();
        let mut stderr = AfterOutput::new(file, Some(Arc::clone(&buffer)));
        writeln!(stderr, "err").unwrap();
        writeln!(Stdout::Buffered(Arc::clone(&buffer)), "more").unwrap();
        let mut input = AfterOutput::new(OpenFile::from(std::io::stdin()), Some(buffer));
        // Reading an empty buffer reads nothing, but writes the output first.
        let _ = input.read(&mut []);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "out\nerr\nmore\n");
    }

    #[test]
    fn a_full_buffer_is_written_and_a_large_write_goes_straight_through() {
        let dir = tempfile::tempdir().unwrap();
        let (path, file) = file_in(&dir, "out");
        let buffer = held(file);
        let mut stdout = Stdout::Buffered(Arc::clone(&buffer));
        stdout.write_all(&[b'a'; 10]).unwrap();
        stdout.write_all(&vec![b'b'; CAPACITY]).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            u64::try_from(10 + CAPACITY).unwrap()
        );
        stdout.write_all(b"c").unwrap();
        stdout.flush().unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            u64::try_from(11 + CAPACITY).unwrap()
        );
    }

    #[test]
    fn a_failed_write_is_reported_at_the_end() {
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);
        let buffer = OutputBuffer::new(writer.into());
        write!(Stdout::Buffered(Arc::clone(&buffer)), "lost").unwrap();
        buffer.write_pending();
        let error = buffer.finish(true).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn a_buffer_knows_its_file_and_no_other() {
        let dir = tempfile::tempdir().unwrap();
        let (_, file) = file_in(&dir, "out");
        let buffer = OutputBuffer::new(file.clone());
        assert!(buffer.is_for(&file));
        let (_, other) = file_in(&dir, "other");
        assert!(!buffer.is_for(&other));
        assert!(!buffer.is_for(&OpenFile::from(std::io::stdout())));
    }
}
