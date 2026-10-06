#[derive(Debug, Default, Clone)]
pub(crate) struct Formatter {
    pub use_color: bool,
}

impl cash_core::extensions::ErrorFormatter for Formatter {
    /// Bash's wording, as the default has it; `--disable-color` turns colour off even on
    /// a terminal.
    ///
    /// At an interactive prompt, a command that is not found and that `help tools` lists
    /// gets one more line, `install it: winget install ID, or scoop install NAME`, in the
    /// same plain style. A script, `-c` and a shell with a `command_not_found_handle`
    /// function get Bash's line only, so nothing changes for them.
    fn format_error(
        &self,
        err: &cash_core::error::Error,
        shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
        colour: bool,
    ) -> String {
        let mut text = cash_core::extensions::DefaultErrorFormatter.format_error(
            err,
            shell,
            colour && self.use_color,
        );
        if let cash_core::error::ErrorKind::CommandNotFound(name) = err.kind()
            && shell.options().interactive
            && shell.funcs().get("command_not_found_handle").is_none()
            && let Some(hint) = cash_builtins::helpdocs::install_hint(name)
        {
            text.push_str("install it: ");
            text.push_str(&hint);
            text.push('\n');
        }
        text
    }
}
