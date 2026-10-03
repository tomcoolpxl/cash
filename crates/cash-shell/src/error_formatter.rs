#[derive(Debug, Default, Clone)]
pub(crate) struct Formatter {
    pub use_color: bool,
}

impl cash_core::extensions::ErrorFormatter for Formatter {
    /// Bash's wording, as the default has it; `--disable-color` turns colour off even on
    /// a terminal.
    fn format_error(
        &self,
        err: &cash_core::error::Error,
        shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
        colour: bool,
    ) -> String {
        cash_core::extensions::DefaultErrorFormatter.format_error(
            err,
            shell,
            colour && self.use_color,
        )
    }
}
