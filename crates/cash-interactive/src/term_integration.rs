use std::borrow::Cow;
use std::fmt::Write;
use std::io::Write as _;

use crate::term_detection::{self, CwdReport, Marks};

/// Utility for integrating with terminal emulators.
pub(crate) struct TerminalIntegration {
    /// Info about the hosting terminal.
    term: term_detection::TerminalInfo,
}

impl TerminalIntegration {
    /// Starts terminal integration, emitting the sequence that announces it, and returns the
    /// utility the interactive loop reports events to.
    ///
    /// # Arguments
    ///
    /// * `term_info` - Information about the terminal capabilities.
    pub fn init(term_info: term_detection::TerminalInfo) -> std::io::Result<Self> {
        let integration = Self { term: term_info };
        Self::write(integration.initialize().as_ref())?;
        Ok(integration)
    }

    /// Returns a utility that integrates with nothing: it reports no capabilities, so every
    /// event handler below does nothing and every sequence it composes is empty. This is what
    /// a shell gets when integration is switched off, so it need not ask whether it has one.
    pub fn disabled() -> Self {
        Self {
            term: term_detection::TerminalInfo::default(),
        }
    }

    //
    // Event handlers: these are called at the points in the interactive loop that the
    // terminal wants to know about; each does what the event calls for. A terminal that
    // reports no support has nothing to say at any of them, so every handler is inert.
    //

    /// Called after a command line has been read and before it runs.
    ///
    /// # Arguments
    ///
    /// * `command` - The command that is about to be executed.
    pub fn on_pre_exec_command(&self, command: &str) -> std::io::Result<()> {
        Self::write(self.pre_exec_command(command).as_ref())
    }

    /// Called after a command has run.
    ///
    /// # Arguments
    ///
    /// * `exit_code` - The exit code the command left behind.
    pub fn on_post_exec_command(&self, exit_code: i32) -> std::io::Result<()> {
        Self::write(self.post_exec_command(exit_code).as_ref())
    }

    /// Writes a sequence to standard output, flushing so the terminal sees it before whatever
    /// the shell does next. Writing nothing still flushes; that costs nothing and keeps the
    /// inert case from being a separate path.
    fn write(seq: &str) -> std::io::Result<()> {
        let mut stdout = std::io::stdout();
        stdout.write_all(seq.as_bytes())?;
        stdout.flush()
    }

    /// Returns the terminal escape sequence that should be emitted to initialize terminal
    /// integration.
    fn initialize(&self) -> Cow<'_, str> {
        match self.term.marks {
            Some(Marks::Osc633) => "\x1b]633;P;HasRichCommandDetection=True\x1b\\".into(),
            Some(Marks::Osc133) | None => "".into(),
        }
    }

    /// Returns a composed prompt bracketed with the sequences that mark where a prompt starts
    /// and ends, and that report the working directory -- or the prompt as given, when there is
    /// nothing to add. Returned rather than composed in place so the untouched prompt costs
    /// nothing, not even the copy that concatenating three empty strings onto it would make.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The prompt as composed by the shell.
    /// * `working_dir` - The shell's current working directory.
    pub fn decorate_prompt(&self, prompt: String, working_dir: &std::path::Path) -> String {
        if self.term.marks.is_none() && self.term.cwd_report.is_none() {
            return prompt;
        }

        [
            self.mark('A').as_ref(),
            self.report_cwd(working_dir).as_ref(),
            prompt.as_str(),
            self.mark('B').as_ref(),
        ]
        .concat()
    }

    /// Returns a mark without arguments: `A` before the prompt, `B` after it, `C` before
    /// the command's output.
    fn mark(&self, mark: char) -> Cow<'_, str> {
        match self.term.marks {
            Some(Marks::Osc133) => format!("\x1b]133;{mark}\x1b\\").into(),
            Some(Marks::Osc633) => format!("\x1b]633;{mark}\x1b\\").into(),
            None => "".into(),
        }
    }

    /// Returns the terminal escape sequence to report the current working directory.
    fn report_cwd(&self, cwd: &std::path::Path) -> Cow<'_, str> {
        let cwd = cwd.to_string_lossy();
        match self.term.cwd_report {
            Some(CwdReport::Osc633) => {
                format!("\x1b]633;P;Cwd={}\x1b\\", osc_633_escape(&cwd)).into()
            }
            // Windows Terminal opens a duplicated tab where this says: a Windows path.
            Some(CwdReport::Osc9_9) => format!("\x1b]9;9;{}\x1b\\", cwd.replace('/', "\\")).into(),
            Some(CwdReport::Osc7) => format!("\x1b]7;{}\x1b\\", file_url(&cwd)).into(),
            None => "".into(),
        }
    }

    /// Returns the terminal escape sequence that should be emitted before executing a command,
    /// but after the prompt and the user has finished entering input.
    ///
    /// # Arguments
    ///
    /// * `command` - The command that is about to be executed.
    fn pre_exec_command(&self, command: &str) -> Cow<'_, str> {
        if self.term.marks != Some(Marks::Osc633) {
            return self.mark('C');
        }
        let mut escaped_command = osc_633_escape(command);
        escaped_command.insert_str(0, "\x1b]633;E;");

        if let Some(session_nonce) = &self.term.session_nonce {
            escaped_command.push(';');
            escaped_command.push_str(session_nonce);
        }

        escaped_command.push_str("\x1b\\\x1b]633;C\x1b\\");

        escaped_command.into()
    }

    /// Returns the terminal escape sequence that should be emitted after executing a command.
    fn post_exec_command(&self, exit_code: i32) -> Cow<'_, str> {
        match self.term.marks {
            Some(Marks::Osc133) => format!("\x1b]133;D;{exit_code}\x1b\\").into(),
            Some(Marks::Osc633) => format!("\x1b]633;D;{exit_code}\x1b\\").into(),
            None => "".into(),
        }
    }
}

/// A `file://` URL for a Windows path, as OSC 7 takes it: `C:\a b` is `file:///C:/a%20b`.
fn file_url(path: &str) -> String {
    let mut url = String::from("file://");
    if !path.starts_with(['/', '\\']) {
        url.push('/');
    }
    for byte in path.bytes() {
        match byte {
            b'\\' => url.push('/'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                url.push(char::from(byte));
            }
            _ => {
                let _ = write!(url, "%{byte:02X}");
            }
        }
    }
    url
}

/// Escapes a string for safe inclusion in an OSC 633 escape sequence.
/// Reference: <https://github.com/microsoft/vscode/blob/main/src/vs/workbench/contrib/terminal/common/scripts/shellIntegration-bash.sh>
fn osc_633_escape(command: &str) -> String {
    let mut result = String::new();

    for c in command.chars() {
        match c {
            // Escape ASCII control characters (< 0x1f, i.e., < 31)
            '\x00'..='\x1e' => {
                let _ = write!(result, r"\x{:02x}", c as u8);
            }
            // Escape backslash with an extra prefixed backslash
            '\\' => result.push_str(r"\\"),
            // Escape semicolon via \xNN syntax (like control chars)
            ';' => result.push_str(r"\x3b"),
            // Keep other characters as-is
            _ => result.push(c),
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term_detection::TerminalInfo;

    fn integration(marks: Option<Marks>, cwd_report: Option<CwdReport>) -> TerminalIntegration {
        TerminalIntegration {
            term: TerminalInfo {
                marks,
                cwd_report,
                ..TerminalInfo::default()
            },
        }
    }

    /// Windows Terminal gets OSC 133 marks and OSC 9;9 with a Windows path, as D39 says;
    /// it was sent VS Code's 633 alone (XC-18).
    #[test]
    fn windows_terminal_gets_osc_133_and_9_9() {
        let wt = integration(Some(Marks::Osc133), Some(CwdReport::Osc9_9));
        assert_eq!(wt.initialize(), "");
        assert_eq!(
            wt.decorate_prompt("$ ".into(), std::path::Path::new("C:/Users/me")),
            "\x1b]133;A\x1b\\\x1b]9;9;C:\\Users\\me\x1b\\$ \x1b]133;B\x1b\\"
        );
        assert_eq!(wt.pre_exec_command("ls; pwd"), "\x1b]133;C\x1b\\");
        assert_eq!(wt.post_exec_command(3), "\x1b]133;D;3\x1b\\");
    }

    #[test]
    fn vs_code_gets_osc_633_with_the_command_line() {
        let code = integration(Some(Marks::Osc633), Some(CwdReport::Osc633));
        assert_eq!(
            code.decorate_prompt("$ ".into(), std::path::Path::new("C:/a;b")),
            "\x1b]633;A\x1b\\\x1b]633;P;Cwd=C:/a\\x3bb\x1b\\$ \x1b]633;B\x1b\\"
        );
        assert_eq!(
            code.pre_exec_command("ls; pwd"),
            "\x1b]633;E;ls\\x3b pwd\x1b\\\x1b]633;C\x1b\\"
        );
        assert_eq!(code.post_exec_command(0), "\x1b]633;D;0\x1b\\");
    }

    #[test]
    fn osc_7_reports_a_file_url() {
        let wez = integration(Some(Marks::Osc133), Some(CwdReport::Osc7));
        assert_eq!(
            wez.report_cwd(std::path::Path::new(r"C:\Program Files\x%")),
            "\x1b]7;file:///C:/Program%20Files/x%25\x1b\\"
        );
    }

    #[test]
    fn an_unknown_terminal_gets_nothing() {
        let none = integration(None, None);
        assert_eq!(
            none.decorate_prompt("$ ".into(), std::path::Path::new("C:/")),
            "$ "
        );
        assert_eq!(none.pre_exec_command("ls"), "");
        assert_eq!(none.post_exec_command(1), "");
    }

    #[test]
    fn osc_633_escape_basic() {
        // Test simple alphanumeric string
        assert_eq!(osc_633_escape("echo hello"), "echo hello");
        assert_eq!(osc_633_escape("ls -la"), "ls -la");
    }

    #[test]
    fn osc_633_escape_semicolon() {
        // Semicolons should be escaped
        assert_eq!(osc_633_escape("cmd1; cmd2"), r"cmd1\x3b cmd2");
        assert_eq!(osc_633_escape(";"), r"\x3b");
        assert_eq!(osc_633_escape("a;b;c"), r"a\x3bb\x3bc");
    }

    #[test]
    fn osc_633_escape_backslash() {
        // Backslashes should be escaped
        assert_eq!(osc_633_escape(r"echo \n"), r"echo \\n");
        assert_eq!(osc_633_escape(r"\"), r"\\");
        assert_eq!(osc_633_escape(r"C:\path\to\file"), r"C:\\path\\to\\file");
    }

    #[test]
    fn osc_633_escape_control_chars() {
        // ASCII control characters (0x00-0x1e, i.e., 0-30) should be escaped
        assert_eq!(osc_633_escape("\x00"), r"\x00");
        assert_eq!(osc_633_escape("\x01"), r"\x01");
        assert_eq!(osc_633_escape("\t"), r"\x09"); // tab
        assert_eq!(osc_633_escape("\n"), r"\x0a"); // newline
        assert_eq!(osc_633_escape("\r"), r"\x0d"); // carriage return
        assert_eq!(osc_633_escape("\x1e"), r"\x1e"); // last control char (30)

        // 0x1f (31) should NOT be escaped as a control char (not < 31)
        assert_eq!(osc_633_escape("\x1f"), "\x1f");

        // Space (0x20, 32) should NOT be escaped
        assert_eq!(osc_633_escape(" "), " ");
    }

    #[test]
    fn osc_633_escape_mixed() {
        // Test combinations of different escape scenarios
        assert_eq!(
            osc_633_escape("echo\nhello; world\\n"),
            r"echo\x0ahello\x3b world\\n"
        );

        assert_eq!(osc_633_escape("cmd\t\t; \\path"), r"cmd\x09\x09\x3b \\path");

        // Test with null bytes
        assert_eq!(osc_633_escape("a\x00b\x01c"), r"a\x00b\x01c");

        // Test all three special cases together
        assert_eq!(osc_633_escape("\\;\n"), r"\\\x3b\x0a");
    }

    #[test]
    fn osc_633_escape_empty() {
        assert_eq!(osc_633_escape(""), "");
    }

    #[test]
    fn osc_633_escape_unicode() {
        // Unicode characters should pass through unchanged
        assert_eq!(osc_633_escape("echo 你好"), "echo 你好");
        assert_eq!(osc_633_escape("café"), "café");
        assert_eq!(osc_633_escape("🦀"), "🦀");

        // But should still escape special chars
        assert_eq!(osc_633_escape("你好;世界"), r"你好\x3b世界");
    }
}
