/// What the hosting terminal is told, and in which dialect (D39).
///
/// It held 48 flags, one per OSC number some terminal understands, of which the
/// integration read one, 633, VS Code's; Windows Terminal, told 633 alone, never got the
/// OSC 133 marks and OSC 9;9 working folder D39 promises it (XC-18).
#[derive(Clone, Debug, Default)]
pub struct TerminalInfo {
    /// If applicable, a session nonce assigned by the terminal.
    pub session_nonce: Option<String>,

    /// How the prompt, the command line and the command's output are marked, if at all.
    pub marks: Option<Marks>,

    /// How the working folder is reported, if at all.
    pub cwd_report: Option<CwdReport>,
}

/// The sequences that mark a prompt's start and end, a command's start, and its end with
/// its status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marks {
    /// OSC 133, `FinalTerm`'s: Windows Terminal, iTerm2, `WezTerm`.
    Osc133,
    /// OSC 633, VS Code's, which also carries the command line.
    Osc633,
}

/// The sequence that reports the working folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CwdReport {
    /// OSC 7 with a `file://` URL: iTerm2, `WezTerm`, Ghostty.
    Osc7,
    /// OSC 9;9 with a Windows path: Windows Terminal, which opens a duplicated tab there.
    Osc9_9,
    /// OSC 633;P;Cwd=, VS Code's.
    Osc633,
}

/// Identifies a known terminal emulator hosting this process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownTerminal {
    /// Alacritty
    Alacritty,
    /// Apple terminal
    AppleTerminal,
    /// Ghostty
    Ghostty,
    /// GNOME Terminal
    GnomeTerminal,
    /// iTerm2
    ITerm2,
    /// Kitty
    Kitty,
    /// Konsole
    Konsole,
    /// `VSCode` Terminal
    VSCode,
    /// Other VTE-based terminal
    Vte,
    /// Warp Terminal
    WarpTerminal,
    /// `WezTerm`
    WezTerm,
    /// Windows Terminal
    WindowsTerminal,
}

/// Abstracts access to environment variables used for terminal detection.
pub(crate) trait TerminalEnvironment {
    /// Gets the value of an environment variable. Returns `None` if the variable is not set.
    fn get_env_var(&self, key: &str) -> Option<String>;
}

pub(crate) fn get_terminal_info(env: &impl TerminalEnvironment) -> TerminalInfo {
    let terminal = try_detect_terminal(env);
    let (marks, cwd_report) = match terminal {
        // https://code.visualstudio.com/docs/terminal/shell-integration
        Some(KnownTerminal::VSCode) => (Some(Marks::Osc633), Some(CwdReport::Osc633)),
        // https://learn.microsoft.com/en-us/windows/terminal/tutorials/shell-integration
        Some(KnownTerminal::WindowsTerminal) => (Some(Marks::Osc133), Some(CwdReport::Osc9_9)),
        // https://iterm2.com/documentation-escape-codes.html
        // https://wezterm.org/shell-integration.html
        Some(KnownTerminal::ITerm2 | KnownTerminal::WezTerm) => {
            (Some(Marks::Osc133), Some(CwdReport::Osc7))
        }
        // https://ghostty.org/docs/vt/osc/0
        Some(KnownTerminal::Ghostty) => (None, Some(CwdReport::Osc7)),
        _ => (None, None),
    };
    let session_nonce = if terminal == Some(KnownTerminal::VSCode) {
        env.get_env_var("VSCODE_NONCE")
    } else {
        None
    };
    TerminalInfo {
        session_nonce,
        marks,
        cwd_report,
    }
}

/// Tries to detect the hosting terminal.
///
/// # Arguments
///
/// * `env` - An implementation of `TerminalEnvironment` to access environment variables.
pub(crate) fn try_detect_terminal(env: &impl TerminalEnvironment) -> Option<KnownTerminal> {
    if let Some(detected) = try_detect_terminal_from_prog_var(env) {
        Some(detected)
    } else if env.get_env_var("WT_SESSION").is_some() {
        Some(KnownTerminal::WindowsTerminal)
    } else {
        None
    }
}

fn try_detect_terminal_from_prog_var(env: &impl TerminalEnvironment) -> Option<KnownTerminal> {
    let term_prog = env.get_env_var("TERM_PROGRAM")?;

    // Remove punctuation and normalize.
    let term_prog: String = term_prog
        .chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();

    match term_prog.as_str() {
        "alacritty" => Some(KnownTerminal::Alacritty),
        "appleterminal" => Some(KnownTerminal::AppleTerminal),
        "ghostty" => Some(KnownTerminal::Ghostty),
        "gnometerminal" => Some(KnownTerminal::GnomeTerminal),
        "iterm" | "iterm2" | "itermapp" => Some(KnownTerminal::ITerm2),
        "kitty" => Some(KnownTerminal::Kitty),
        "konsole" => Some(KnownTerminal::Konsole),
        "vscode" => Some(KnownTerminal::VSCode),
        "vte" => Some(KnownTerminal::Vte),
        "warp" | "warpterminal" => Some(KnownTerminal::WarpTerminal),
        "wezterm" => Some(KnownTerminal::WezTerm),
        "windowsterminal" => Some(KnownTerminal::WindowsTerminal),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_matches;
    use std::collections::HashMap;

    impl TerminalEnvironment for HashMap<&str, &str> {
        fn get_env_var(&self, key: &str) -> Option<String> {
            self.get(key).map(|v| (*v).to_string())
        }
    }

    #[test]
    fn no_term_program() {
        let test_env = HashMap::new();

        let term_info = get_terminal_info(&test_env);
        assert_matches!(try_detect_terminal(&test_env), None);
        assert_eq!(term_info.marks, None);
        assert_eq!(term_info.cwd_report, None);
    }

    #[test]
    fn unknown_term_program() {
        let test_env = HashMap::from([("TERM_PROGRAM", "unknown_terminal")]);

        let term_info = get_terminal_info(&test_env);
        assert_matches!(try_detect_terminal(&test_env), None);
        assert_eq!(term_info.marks, None);
    }

    #[test]
    fn vscode_recognition() {
        let test_env = HashMap::from([("TERM_PROGRAM", "vscode"), ("VSCODE_NONCE", "test_nonce")]);

        let term_info = get_terminal_info(&test_env);
        assert_matches!(try_detect_terminal(&test_env), Some(KnownTerminal::VSCode));
        assert_eq!(term_info.marks, Some(Marks::Osc633));
        assert_eq!(term_info.cwd_report, Some(CwdReport::Osc633));
        assert_eq!(term_info.session_nonce, Some("test_nonce".to_string()));
    }

    /// Windows Terminal is told what D39 says: OSC 133 marks and OSC 9;9 (XC-18).
    #[test]
    fn windows_terminal_recognition() {
        let test_env = HashMap::from([("WT_SESSION", "some_value"), ("VSCODE_NONCE", "x")]);

        let term_info = get_terminal_info(&test_env);
        assert_matches!(
            try_detect_terminal(&test_env),
            Some(KnownTerminal::WindowsTerminal)
        );
        assert_eq!(term_info.marks, Some(Marks::Osc133));
        assert_eq!(term_info.cwd_report, Some(CwdReport::Osc9_9));
        assert_eq!(term_info.session_nonce, None);
    }
}
