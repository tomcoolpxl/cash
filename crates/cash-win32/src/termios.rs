//! A terminal's settings as `stty` sees them, kept over the Windows console.
//!
//! A Unix terminal driver has some sixty settings and seventeen special characters; the
//! console has four modes that mean the same thing. Those four live on the console and are
//! read and set there each time: echo (`ENABLE_ECHO_INPUT`), line collection
//! (`ENABLE_LINE_INPUT`, `icanon`), Ctrl-C as a signal (`ENABLE_PROCESSED_INPUT`, `isig`)
//! and output processing (`ENABLE_PROCESSED_OUTPUT`, `opost`). The rest is remembered here,
//! for the whole process, so that `stty -a` prints back what a script set and `stty -g`
//! round-trips. The console is one per process, so the table is too.
//!
//! The settings are kept in the shape of a Linux `struct termios`, with Linux's bit
//! values: four flag words, a line discipline and [`NCCS`] special characters. `stty -g`
//! then prints the string GNU's does on Linux, and a saved string from either restores.

use std::io;
use std::os::windows::io::AsRawHandle as _;
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::System::Console::{
    CONSOLE_SCREEN_BUFFER_INFO, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_PROCESSED_OUTPUT, GetConsoleMode, GetConsoleScreenBufferInfo, SetConsoleMode,
};

/// How many special characters a Linux `termios` holds.
pub const NCCS: usize = 32;

/// `ISIG` in the local flags: Ctrl-C and its kin are signals rather than characters.
pub const ISIG: u32 = 0o000_001;
/// `ICANON` in the local flags: input is collected a line at a time.
pub const ICANON: u32 = 0o000_002;
/// `ECHO` in the local flags: what is typed is shown.
pub const ECHO: u32 = 0o000_010;
/// `OPOST` in the output flags: output is processed on its way to the screen.
pub const OPOST: u32 = 0o000_001;

/// A terminal's settings, laid out as Linux lays out `struct termios`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Termios {
    /// Input flags (`c_iflag`).
    pub iflag: u32,
    /// Output flags (`c_oflag`).
    pub oflag: u32,
    /// Control flags (`c_cflag`), the speed among them.
    pub cflag: u32,
    /// Local flags (`c_lflag`): echo, line collection, signals.
    pub lflag: u32,
    /// The line discipline (`c_line`).
    pub line: u8,
    /// The special characters (`c_cc`), by Linux's indices.
    pub cc: [u8; NCCS],
}

impl Default for Termios {
    /// A Linux terminal as GNU's `stty sane` leaves it, at 38400 baud with 8-bit
    /// characters: what a Linux shell finds on a fresh pseudo terminal, and what `stty`
    /// with no arguments measures its report against.
    fn default() -> Self {
        // intr ^C, quit ^\, erase ^?, kill ^U, eof ^D, time 0, min 1, swtch undef,
        // start ^Q, stop ^S, susp ^Z, eol undef, rprnt ^R, discard ^O, werase ^W,
        // lnext ^V, eol2 undef.
        let mut cc = [0u8; NCCS];
        let set = [
            3, 0o34, 0o177, 0o25, 4, 0, 1, 0, 0o21, 0o23, 0o32, 0, 0o22, 0o17, 0o27, 0o26, 0,
        ];
        for (slot, value) in cc.iter_mut().zip(set) {
            *slot = value;
        }
        Self {
            // brkint icrnl ixon imaxbel
            iflag: 0o022_402,
            // opost onlcr
            oflag: 0o000_005,
            // 38400 baud, cs8, cread
            cflag: 0o000_277,
            // isig icanon iexten echo echoe echok echoctl echoke
            lflag: 0o105_073,
            line: 0,
            cc,
        }
    }
}

/// The settings the console does not hold, as last set in this process.
static REMEMBERED: OnceLock<Mutex<Termios>> = OnceLock::new();

fn remembered() -> &'static Mutex<Termios> {
    REMEMBERED.get_or_init(|| Mutex::new(Termios::default()))
}

/// The console's input and output, or an error when the process has none.
fn console() -> io::Result<(std::fs::File, std::fs::File)> {
    let open = |name: &str| {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(name)
    };
    Ok((open("CONIN$")?, open("CONOUT$")?))
}

fn mode_of(file: &std::fs::File) -> io::Result<u32> {
    let mut mode = 0u32;
    // SAFETY: an open console handle owned by `file`, and a valid out-parameter.
    if unsafe { GetConsoleMode(file.as_raw_handle(), &raw mut mode) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(mode)
}

fn set_mode(file: &std::fs::File, mode: u32) -> io::Result<()> {
    // SAFETY: an open console handle owned by `file`, alive for the call.
    if unsafe { SetConsoleMode(file.as_raw_handle(), mode) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// `flags` with `bit` set or cleared as `on` says.
const fn with(flags: u32, bit: u32, on: bool) -> u32 {
    if on { flags | bit } else { flags & !bit }
}

/// The settings now: the remembered table, with echo, `icanon`, `isig` and `opost` as the
/// console has them.
///
/// # Errors
///
/// Returns an error when the process has no console, or the console will not say its
/// modes.
pub fn current() -> io::Result<Termios> {
    let (input, output) = console()?;
    let input_mode = mode_of(&input)?;
    let output_mode = mode_of(&output)?;
    let mut settings = *remembered()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    settings.lflag = with(settings.lflag, ECHO, input_mode & ENABLE_ECHO_INPUT != 0);
    settings.lflag = with(settings.lflag, ICANON, input_mode & ENABLE_LINE_INPUT != 0);
    settings.lflag = with(
        settings.lflag,
        ISIG,
        input_mode & ENABLE_PROCESSED_INPUT != 0,
    );
    settings.oflag = with(
        settings.oflag,
        OPOST,
        output_mode & ENABLE_PROCESSED_OUTPUT != 0,
    );
    Ok(settings)
}

/// Makes `settings` the terminal's: echo, `icanon`, `isig` and `opost` on the console,
/// whose other modes are left as they are, and the rest remembered for [`current`].
///
/// # Errors
///
/// Returns an error when the process has no console, or the console refuses a mode.
pub fn apply(settings: &Termios) -> io::Result<()> {
    let (input, output) = console()?;
    let mut input_mode = mode_of(&input)?;
    input_mode = with(input_mode, ENABLE_ECHO_INPUT, settings.lflag & ECHO != 0);
    input_mode = with(input_mode, ENABLE_LINE_INPUT, settings.lflag & ICANON != 0);
    input_mode = with(
        input_mode,
        ENABLE_PROCESSED_INPUT,
        settings.lflag & ISIG != 0,
    );
    let output_mode = with(
        mode_of(&output)?,
        ENABLE_PROCESSED_OUTPUT,
        settings.oflag & OPOST != 0,
    );
    set_mode(&input, input_mode)?;
    set_mode(&output, output_mode)?;
    *remembered()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = *settings;
    Ok(())
}

/// The console window's rows and columns.
///
/// # Errors
///
/// Returns an error when the process has no console, or the console will not say.
pub fn window_size() -> io::Result<(u16, u16)> {
    let (_, output) = console()?;
    // SAFETY: an all-zero CONSOLE_SCREEN_BUFFER_INFO is a valid out-parameter.
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: an open console handle owned by `output`, and a valid out-parameter.
    if unsafe { GetConsoleScreenBufferInfo(output.as_raw_handle(), &raw mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let window = info.srWindow;
    let extent = |low: i16, high: i16| u16::try_from(i32::from(high) - i32::from(low) + 1);
    match (
        extent(window.Top, window.Bottom),
        extent(window.Left, window.Right),
    ) {
        (Ok(rows), Ok(cols)) => Ok((rows, cols)),
        _ => Err(io::Error::other("the console window has no size")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_a_linux_terminal_at_38400_baud() {
        let settings = Termios::default();
        assert_eq!(settings.iflag, 0x2502);
        assert_eq!(settings.oflag, 0x5);
        assert_eq!(settings.cflag, 0xbf);
        assert_eq!(settings.lflag, 0x8a3b);
        assert_eq!(settings.cc[0], 3, "intr is ^C");
        assert_eq!(settings.cc[2], 0x7f, "erase is ^?");
        assert_eq!(settings.cc[6], 1, "min is 1");
        assert_eq!(&settings.cc[17..], &[0u8; NCCS - 17]);
    }

    #[test]
    fn a_bit_is_set_or_cleared() {
        assert_eq!(with(0o5, ECHO, true), 0o15);
        assert_eq!(with(0o15, ECHO, false), 0o5);
        assert_eq!(with(0o5, ECHO, false), 0o5);
    }
}
