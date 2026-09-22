//! `less` and `more` — **D48**, and the reason **D35** exists.
//!
//! `less` is not coreutils; it is its own GNU project, so D48's bundle has never carried
//! it. On a Windows machine `less` therefore comes from Git for Windows if it is
//! installed and not at all if it is not — and `more` resolves to `C:\Windows\System32\
//! more.com`, the DOS tool, which `cash doctor` already warns about because its syntax is
//! not the Unix one.
//!
//! The bundled uutils `more` filled part of the gap and got the most important case
//! wrong: piped into a file or another command it emitted `~` filler lines, so
//! `cmd | more > out` produced junk. A pager that corrupts a pipe is worse than no pager.
//!
//! So cash carries one pager, written once, exposed under both names:
//!
//! - **`less`** is the superset — backward movement, search, line numbers, follow.
//! - **`more`** is the same engine with `more`'s defaults: quit at end of file, and the
//!   `--More--(45%)` prompt rather than `:`.
//!
//! # The rule that matters most
//!
//! **If standard output is not a terminal, a pager is `cat`.** That is what GNU `less`
//! does and what every script depends on, and it is the case the bundled `more` broke.
//! Everything interactive below is gated behind that check.

use std::io::{IsTerminal, Read, Write};

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Which name the pager was invoked under.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flavor {
    /// `less`: stays open at end of file, prompts with `:`.
    Less,
    /// `more`: exits at end of file, prompts with `--More--(NN%)`.
    More,
}

/// Page through text.
#[derive(Parser)]
pub(crate) struct LessCommand {
    /// Quit if the entire file fits on one screen.
    #[arg(short = 'F', long = "quit-if-one-screen")]
    quit_if_one_screen: bool,

    /// Exit at end of file (the `more` default).
    #[arg(short = 'e', long = "quit-at-eof")]
    quit_at_eof: bool,

    /// Number output lines.
    #[arg(short = 'N', long = "LINE-NUMBERS")]
    line_numbers: bool,

    /// Chop long lines rather than wrapping them.
    #[arg(short = 'S', long = "chop-long-lines")]
    chop_long_lines: bool,

    /// Output raw control characters (accepted; cash never re-encodes them).
    #[arg(short = 'R', long = "RAW-CONTROL-CHARS")]
    raw_control_chars: bool,

    /// Do not clear the screen on exit (accepted; cash never clears it).
    #[arg(short = 'X', long = "no-init")]
    no_init: bool,

    /// Files to page. Standard input is used when none are given.
    files: Vec<String>,
}

impl builtins::Command for LessCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        self.run(&context, Flavor::Less)
    }
}

/// `more`: the same pager with `more`'s defaults.
#[derive(Parser)]
pub(crate) struct MoreCommand {
    /// Files to page. Standard input is used when none are given.
    files: Vec<String>,
}

impl builtins::Command for MoreCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let as_less = LessCommand {
            quit_if_one_screen: false,
            // `more` stops at the end; `less` waits there.
            quit_at_eof: true,
            line_numbers: false,
            chop_long_lines: false,
            raw_control_chars: false,
            no_init: false,
            files: self.files.clone(),
        };
        as_less.run(context_ref(&context), Flavor::More)
    }
}

/// Borrow the context without moving it, so both entry points share one body.
const fn context_ref<'a, SE: cash_core::ShellExtensions>(
    context: &'a cash_core::ExecutionContext<'a, SE>,
) -> &'a cash_core::ExecutionContext<'a, SE> {
    context
}

impl LessCommand {
    fn run<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        flavor: Flavor,
    ) -> Result<ExecutionResult, cash_core::Error> {
        let mut text = String::new();
        let mut failed = false;

        if self.files.is_empty() {
            let mut input = context.stdin();
            let mut raw = Vec::new();
            if input.read_to_end(&mut raw).is_ok() {
                text.push_str(&String::from_utf8_lossy(&raw));
            }
        } else {
            for file in &self.files {
                match read_file(context, file) {
                    Ok(contents) => text.push_str(&contents),
                    Err(e) => {
                        writeln!(context.stderr(), "{}: {file}: {e}", context.command_name)?;
                        failed = true;
                    }
                }
            }
        }

        let lines = split_lines(&text);
        let rendered = self.render(&lines);

        // The rule: a pager writing anywhere but a terminal is `cat`. Checked on the
        // *shell's* stdout, which is what a redirection or a pipe actually replaces.
        let interactive = std::io::stdout().is_terminal()
            && context
                .try_fd(cash_core::openfiles::OpenFiles::STDOUT_FD)
                .is_some_and(|f| f.is_terminal());

        if interactive {
            page(
                context,
                &rendered,
                flavor,
                self.quit_if_one_screen,
                self.quit_at_eof,
            )?;
        } else {
            let mut stdout = context.stdout();
            for line in &rendered {
                writeln!(stdout, "{line}")?;
            }
        }

        if failed {
            return Ok(ExecutionResult::general_error());
        }
        Ok(ExecutionResult::success())
    }

    /// Apply the presentation options that do not depend on the terminal.
    fn render(&self, lines: &[&str]) -> Vec<String> {
        lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                if self.line_numbers {
                    std::format!("{:>7} {line}", index + 1)
                } else {
                    (*line).to_string()
                }
            })
            .collect()
    }
}

/// Read one input, honouring `-` as standard input the way every pager does.
fn read_file<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    file: &str,
) -> Result<String, std::io::Error> {
    if file == "-" {
        let mut raw = Vec::new();
        context.stdin().read_to_end(&mut raw)?;
        return Ok(String::from_utf8_lossy(&raw).into_owned());
    }

    let path = context.shell.absolute_path(std::path::Path::new(file));
    let raw = std::fs::read(path)?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

/// Split into display lines, treating `\r\n` and `\n` alike (D20) and not inventing a
/// trailing empty line for input that ends with a terminator.
fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let trimmed = text
        .strip_suffix('\n')
        .map_or(text, |t| t.strip_suffix('\r').unwrap_or(t));
    trimmed
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

/// The interactive half. Only ever reached when stdout is a terminal.
fn page<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    lines: &[String],
    flavor: Flavor,
    quit_if_one_screen: bool,
    quit_at_eof: bool,
) -> Result<(), cash_core::Error> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

    let (_, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let page_size = usize::from(rows.saturating_sub(1)).max(1);

    let mut stdout = context.stdout();

    // Everything fits and nobody asked to stay: just print it. `less -F` is the common
    // spelling, and `git` sets it in `LESS` for exactly this reason.
    if lines.len() <= page_size && (quit_if_one_screen || flavor == Flavor::More) {
        for line in lines {
            writeln!(stdout, "{line}")?;
        }
        return Ok(());
    }

    let mut top = 0usize;
    let mut search: Option<String> = None;

    loop {
        for line in lines.iter().skip(top).take(page_size) {
            writeln!(stdout, "{line}")?;
        }
        stdout.flush()?;

        let at_end = top + page_size >= lines.len();
        if at_end && (quit_at_eof || flavor == Flavor::More) {
            return Ok(());
        }

        write!(stdout, "{}", prompt(flavor, top, page_size, lines.len()))?;
        stdout.flush()?;

        // Raw mode only while waiting for a key, so a Ctrl-C during output still reaches
        // the shell's own handling (D13).
        let raw = crossterm::terminal::enable_raw_mode().is_ok();
        let key = loop {
            match event::read() {
                Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => break Some(key),
                Ok(_) => (),
                Err(_) => break None,
            }
        };
        if raw {
            let _ = crossterm::terminal::disable_raw_mode();
        }

        // Erase the prompt so the transcript reads as plain output.
        write!(stdout, "\r{: <30}\r", "")?;
        stdout.flush()?;

        let Some(key) = key else { return Ok(()) };

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q' | 'Q') | KeyCode::Esc => return Ok(()),
            KeyCode::Char('c' | 'C') if ctrl => return Ok(()),
            KeyCode::Char(' ' | 'f') | KeyCode::PageDown => {
                if at_end {
                    return Ok(());
                }
                top = (top + page_size).min(lines.len().saturating_sub(1));
            }
            KeyCode::Char('b') | KeyCode::PageUp => top = top.saturating_sub(page_size),
            KeyCode::Enter | KeyCode::Down | KeyCode::Char('j') => {
                if at_end {
                    return Ok(());
                }
                top = (top + 1).min(lines.len().saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => top = top.saturating_sub(1),
            KeyCode::Char('g') | KeyCode::Home => top = 0,
            KeyCode::Char('G') | KeyCode::End => top = lines.len().saturating_sub(page_size),
            KeyCode::Char('/') => {
                search = read_pattern(&mut stdout)?;
                if let Some(found) = search
                    .as_deref()
                    .and_then(|pattern| find_from(lines, top + 1, pattern))
                {
                    top = found;
                }
            }
            KeyCode::Char('n') => {
                if let Some(found) = search
                    .as_deref()
                    .and_then(|pattern| find_from(lines, top + 1, pattern))
                {
                    top = found;
                }
            }
            _ => (),
        }
    }
}

/// The prompt each flavour shows at the bottom of a screen.
fn prompt(flavor: Flavor, top: usize, page_size: usize, total: usize) -> String {
    match flavor {
        Flavor::Less => String::from(":"),
        Flavor::More => {
            let shown = (top + page_size).min(total);
            let percent = (shown * 100).checked_div(total).unwrap_or(100);
            std::format!("--More--({percent}%)")
        }
    }
}

/// Read a search pattern, echoed after a `/`, terminated by Enter.
fn read_pattern(stdout: &mut impl Write) -> Result<Option<String>, cash_core::Error> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};

    write!(stdout, "/")?;
    stdout.flush()?;

    let raw = crossterm::terminal::enable_raw_mode().is_ok();
    let mut pattern = String::new();

    loop {
        match event::read() {
            Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Enter => break,
                KeyCode::Esc => {
                    pattern.clear();
                    break;
                }
                KeyCode::Backspace => {
                    pattern.pop();
                }
                KeyCode::Char(c) => pattern.push(c),
                _ => (),
            },
            Ok(_) => (),
            Err(_) => break,
        }
    }

    if raw {
        let _ = crossterm::terminal::disable_raw_mode();
    }

    Ok((!pattern.is_empty()).then_some(pattern))
}

/// The next line at or after `from` containing `pattern`.
fn find_from(lines: &[String], from: usize, pattern: &str) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, line)| line.contains(pattern))
        .map(|(index, _)| index)
}
