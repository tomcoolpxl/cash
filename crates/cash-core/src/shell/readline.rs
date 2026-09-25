//! Readline edit buffer support for shell instances.

use crate::{error, extensions, variables::ShellVariable};

impl<SE: extensions::ShellExtensions> crate::Shell<SE> {
    /// Updates the shell state to reflect the given edit buffer contents.
    ///
    /// # Arguments
    ///
    /// * `contents` - The contents of the edit buffer.
    /// * `cursor` - The cursor position in the edit buffer.
    pub fn set_edit_buffer(&mut self, contents: String, cursor: usize) -> Result<(), error::Error> {
        let mut line = ShellVariable::new(contents);
        line.export();
        self.env.set_global("READLINE_LINE", line)?;

        let mut point = ShellVariable::new(cursor.to_string());
        point.export();
        self.env.set_global("READLINE_POINT", point)?;

        Ok(())
    }

    /// Sets the explicit numeric argument for a `bind -x` command. Bash only
    /// creates this variable when the user supplied an argument prefix.
    pub fn set_readline_argument(&mut self, argument: Option<i64>) -> Result<(), error::Error> {
        let _ = self.env.unset("READLINE_ARGUMENT")?;
        if let Some(argument) = argument {
            let mut variable = ShellVariable::new(argument.to_string());
            variable.export();
            self.env.set_global("READLINE_ARGUMENT", variable)?;
        }
        Ok(())
    }

    /// Returns the contents of the shell's edit buffer, if any. The buffer
    /// state is cleared from the shell.
    pub fn pop_edit_buffer(&mut self) -> Result<Option<(String, usize)>, error::Error> {
        let line = self
            .env
            .unset("READLINE_LINE")?
            .map(|line| line.value().to_cow_str(self).to_string());

        let point = self
            .env
            .unset("READLINE_POINT")?
            .and_then(|point| point.value().to_cow_str(self).parse::<usize>().ok())
            .unwrap_or(0);

        if let Some(line) = line {
            Ok(Some((line, point)))
        } else {
            Ok(None)
        }
    }
}
