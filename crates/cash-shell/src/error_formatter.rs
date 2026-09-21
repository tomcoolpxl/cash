#[derive(Debug, Default, Clone)]
pub(crate) struct Formatter {
    pub use_color: bool,
}

impl cash_core::extensions::ErrorFormatter for Formatter {
    fn format_error(
        &self,
        err: &cash_core::error::Error,
        _shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
    ) -> String {
        let prefix = if self.use_color {
            color_print::cstr!("<red>error:</red> ")
        } else {
            "error: "
        };

        std::format!("{prefix}{err:#}\n")
    }
}
