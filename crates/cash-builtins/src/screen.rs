//! `clear` and `reset`, as VT sequences.
//!
//! ConPTY and Windows Terminal are the target, and both interpret VT, so the sequences
//! are written directly instead of being looked up in a terminfo database for `TERM`.
//! The bytes are ncurses 6.6's for `xterm-256color`:
//!
//! * `clear` writes `ESC[H ESC[2J ESC[3J` to standard output (home, erase the screen,
//!   erase the scrollback); `-x` leaves the scrollback;
//! * `reset` writes `ESC c` (a full reset), `ESC]104 BEL` (the colour palette) and the
//!   mode resets to standard error, as `tset` does, and also puts back the console modes
//!   cash relies on (cooked input, VT output, UTF-8), which a program that crashed in raw
//!   mode leaves wrong and no escape sequence can fix. `-q` only prints the terminal type.
//!
//! `reset` shadows `C:\Windows\System32\reset.exe`, the Remote Desktop session command
//! (`reset session`); `command reset` or its full path still reaches it.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Home, erase the display, erase the scrollback.
const CLEAR: &[u8] = b"\x1b[H\x1b[2J\x1b[3J";
/// Home and erase the display, keeping the scrollback (`clear -x`).
const CLEAR_KEEP_SCROLLBACK: &[u8] = b"\x1b[H\x1b[2J";
/// ncurses' `reset` for xterm: full reset, palette, soft reset, then the modes it clears.
const RESET: &[u8] = b"\x1bc\x1b]104\x07\x1b[!p\x1b[?3;4l\x1b[4l\x1b>\x1b[?69l\r";

/// Clear the terminal screen.
#[derive(Parser)]
#[clap(disable_version_flag = true)]
pub(crate) struct ClearCommand {
    /// Do not try to clear the scrollback.
    #[arg(short = 'x')]
    keep_scrollback: bool,

    /// The terminal type; only VT-compatible ones are supported.
    #[arg(short = 'T', value_name = "TERM")]
    term: Option<String>,

    /// Print version information.
    #[arg(short = 'V')]
    version: bool,
}

/// Whether `term` names a terminal these sequences are right for.
fn is_vt(term: &str) -> bool {
    term.starts_with("xterm") || term.starts_with("vt") || term.starts_with("ms-terminal")
}

impl builtins::Command for ClearCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.version {
            writeln!(
                context.stdout(),
                "clear (cash): VT sequences, as ncurses 6.6 writes them"
            )?;
            return Ok(ExecutionResult::success());
        }
        if let Some(term) = self.term.as_deref().filter(|t| !is_vt(t)) {
            writeln!(
                context.stderr(),
                "clear: -T {term} is not supported: cash writes VT sequences, for xterm-like terminals"
            )?;
            return Ok(ExecutionResult::general_error());
        }
        let bytes = if self.keep_scrollback {
            CLEAR_KEEP_SCROLLBACK
        } else {
            CLEAR
        };
        let mut stdout = context.stdout();
        stdout.write_all(bytes)?;
        stdout.flush()?;
        Ok(ExecutionResult::success())
    }
}

/// Reset the terminal and the console modes cash relies on.
#[derive(Parser)]
#[clap(disable_version_flag = true)]
pub(crate) struct ResetCommand {
    /// Print the terminal type and do nothing else.
    #[arg(short = 'q')]
    print_type_only: bool,

    /// Do not report the erase, interrupt and kill characters (there are none to report).
    #[arg(short = 'Q')]
    quiet: bool,

    /// Erase character; the Windows console has none to set.
    #[arg(short = 'e', value_name = "CH")]
    erase: Option<String>,

    /// Interrupt character; the Windows console has none to set.
    #[arg(short = 'i', value_name = "CH")]
    interrupt: Option<String>,

    /// Line-kill character; the Windows console has none to set.
    #[arg(short = 'k', value_name = "CH")]
    kill: Option<String>,

    /// Do not initialise the terminal, only restore the console modes.
    #[arg(short = 'I')]
    no_init: bool,

    /// Print version information.
    #[arg(short = 'V')]
    version: bool,

    /// The terminal type; only VT-compatible ones are supported.
    #[arg(value_name = "TERMINAL")]
    term: Option<String>,
}

impl builtins::Command for ResetCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.version {
            writeln!(
                context.stdout(),
                "reset (cash): VT sequences, as ncurses 6.6 writes them"
            )?;
            return Ok(ExecutionResult::success());
        }
        let term = self.term.clone().unwrap_or_else(|| {
            context
                .shell
                .env()
                .get_str("TERM", context.shell)
                .map_or_else(|| "xterm-256color".to_owned(), |t| t.into_owned())
        });
        if !is_vt(&term) {
            writeln!(
                context.stderr(),
                "reset: {term} is not supported: cash writes VT sequences, for xterm-like terminals"
            )?;
            return Ok(ExecutionResult::general_error());
        }
        if self.print_type_only {
            writeln!(context.stdout(), "{term}")?;
            return Ok(ExecutionResult::success());
        }
        // `-e`, `-i` and `-k` set characters a Unix tty driver has and a Windows console
        // does not; they are accepted so scripts written for `tset` run.
        let _ = (&self.erase, &self.interrupt, &self.kill, self.quiet);

        if let Err(error) = cash_win32::console::restore_modes() {
            writeln!(
                context.stderr(),
                "reset: could not restore the console modes: {error}"
            )?;
        }
        if !self.no_init {
            let mut stderr = context.stderr();
            stderr.write_all(RESET)?;
            stderr.flush()?;
        }
        Ok(ExecutionResult::success())
    }
}
