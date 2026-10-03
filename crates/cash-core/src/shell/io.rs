//! I/O support for shell instances.

use std::io::Write;

use crate::{callstack, error, extensions, ioutils, openfiles};

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Returns a value that can be used to write to the shell's currently configured
    /// standard output stream using `write!` et al.
    pub fn stdout(&self) -> impl std::io::Write + 'static {
        self.open_files.try_stdout().cloned().unwrap_or_else(|| {
            ioutils::FailingReaderWriter::new("standard output not available").into()
        })
    }

    /// Returns a value that can be used to write to the shell's currently configured
    /// standard error stream using `write!` et al.; it says whether it is a terminal.
    pub fn stderr(&self) -> openfiles::OpenFile {
        self.open_files.try_stderr().cloned().unwrap_or_else(|| {
            ioutils::FailingReaderWriter::new("standard error not available").into()
        })
    }

    /// Whether what is written to `stream` may be coloured: only a terminal, and only
    /// while `NO_COLOR` is unset or empty. The one place an error stream's colour is
    /// decided (XC-8), where cash wrote ANSI colour into pipes and files.
    ///
    /// # Arguments
    ///
    /// * `stream` - The stream about to be written to.
    pub fn colours(&self, stream: &openfiles::OpenFile) -> bool {
        stream.is_terminal()
            && self
                .env_str("NO_COLOR")
                .is_none_or(|value| value.is_empty())
    }

    /// The start of an error message, as Bash's `get_name_for_error` and its `line N:`
    /// make it: `script.sh: line 3: ` in a shell that is not interactive, the name being
    /// the file the running code came from (`BASH_SOURCE[0]`), or `$0` where there is
    /// none (`cash: line 1: ` for `-c`); `cash: ` in an interactive one.
    pub fn error_prefix(&self) -> String {
        if self.options.interactive {
            return format!("{}: ", self.name_for_interactive_errors());
        }
        let line = self
            .call_stack
            .current_frame()
            .and_then(callstack::Frame::current_line)
            .unwrap_or(1);
        format!("{}: line {line}: ", self.name_for_errors())
    }

    /// `BASH_SOURCE[0]`, or `$0` where the running code came from no file.
    pub(crate) fn name_for_errors(&self) -> String {
        let source = self
            .call_stack
            .iter()
            .find_map(|frame| match &frame.frame_type {
                callstack::FrameType::Function(call) => Some(&call.function.source().source),
                callstack::FrameType::Script(script) => Some(&script.source_info.source),
                _ => None,
            });
        match source {
            Some(source) if !matches!(source.as_str(), "" | "-c" | "main") => source.clone(),
            _ => self
                .current_shell_name()
                .map_or_else(|| "cash".to_owned(), std::borrow::Cow::into_owned),
        }
    }

    /// The shell's own name, as Bash's `base_pathname (shell_name)`: `cash`.
    pub(crate) fn name_for_interactive_errors(&self) -> String {
        self.name
            .as_deref()
            .and_then(|name| std::path::Path::new(name).file_stem())
            .map_or_else(
                || "cash".to_owned(),
                |stem| stem.to_string_lossy().into_owned(),
            )
    }

    /// Outputs `set -x` style trace output for a command. Intentionally does not return
    /// a result or error to avoid risk that a caller treats an error as fatal. Tracing
    /// failure should generally always be ignored to avoid interfering with execution
    /// flows.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to trace.
    pub(crate) async fn trace_command<S: AsRef<str>>(
        &mut self,
        params: &crate::interp::ExecutionParameters,
        command: S,
    ) {
        // Expand the PS4 prompt variable to get our prefix.
        let mut prefix = self
            .as_mut()
            .expand_prompt_var("PS4", "")
            .await
            .unwrap_or_default();

        // Add additional depth-based prefixes using the first character of PS4.
        let additional_depth = self.call_stack.script_source_depth() + self.trace_level();
        if let Some(c) = prefix.chars().next() {
            for _ in 0..additional_depth {
                prefix.insert(0, c);
            }
        }

        // Resolve which file descriptor to use for tracing. We default to stderr,
        // but if BASH_XTRACEFD is set and refers to a valid file descriptor, use that instead.
        let trace_file = if let Some((_, xtracefd_var)) = self.env.get("BASH_XTRACEFD")
            && let Ok(fd) = xtracefd_var
                .value()
                .to_cow_str(self)
                .parse::<super::ShellFd>()
            && let Some(file) = self.open_files.try_fd(fd)
        {
            Some(file.clone())
        } else {
            params.try_stderr(self)
        };

        // If we have a valid trace file, write to it.
        // In one write: the stages of a pipeline and the background jobs trace from threads
        // of their own, and a line written in pieces could be split by another's.
        if let Some(mut trace_file) = trace_file {
            let line = format!("{prefix}{}\n", command.as_ref());
            let _ = trace_file.write_all(line.as_bytes());
        }
    }

    /// Displays the given error to the user, using the shell's error display mechanisms.
    ///
    /// # Arguments
    ///
    /// * `file` - The stream to write it to, coloured only if [`Self::colours`] says so.
    /// * `err` - The error to display.
    pub fn display_error(
        &self,
        file: &mut openfiles::OpenFile,
        err: &error::Error,
    ) -> Result<(), error::Error> {
        use crate::extensions::ErrorFormatter as _;
        let colour = self.colours(file);
        let str = self.error_formatter.format_error(err, self, colour);
        write!(file, "{str}")?;

        Ok(())
    }
}
