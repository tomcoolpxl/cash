//! Prompt handling for shell instances.

use std::borrow::Cow;

use crate::{Shell, error, extensions, prompt};

/// The variable holding the collapsed prompt (spec D61).
const TRANSIENT_PROMPT_VAR: &str = "CASH_TRANSIENT_PS1";

impl<SE: extensions::ShellExtensions> Shell<SE> {
    /// Returns the default prompt string for the shell.
    const fn default_prompt(&self) -> &'static str {
        if self.options.sh_mode {
            "$ "
        } else {
            "brush$ "
        }
    }

    /// Composes the shell's post-input, pre-command prompt, applying all appropriate expansions.
    pub async fn compose_precmd_prompt(&mut self) -> Result<String, error::Error> {
        self.expand_prompt_var("PS0", "").await
    }

    /// Composes the shell's prompt, applying all appropriate expansions.
    pub async fn compose_prompt(&mut self) -> Result<String, error::Error> {
        self.expand_prompt_var("PS1", self.default_prompt()).await
    }

    /// Composes the shell's alternate-side prompt, applying all appropriate expansions.
    pub async fn compose_alt_side_prompt(&mut self) -> Result<String, error::Error> {
        // This is a brush extension.
        self.expand_prompt_var("BRUSH_PS_ALT", "").await
    }

    /// Composes the shell's continuation prompt.
    pub async fn compose_continuation_prompt(&mut self) -> Result<String, error::Error> {
        self.expand_prompt_var("PS2", "> ").await
    }

    /// Composes the prompt a finished command's prompt collapses to (spec D61), from
    /// `CASH_TRANSIENT_PS1`; `None` when that is unset, so the prompt stays as it was.
    ///
    /// Expanded with the prompt it will replace, so `\$`, `$?` and anything they depend on
    /// agree with it.
    pub async fn compose_transient_prompt(&mut self) -> Result<Option<String>, error::Error> {
        if self.env_str(TRANSIENT_PROMPT_VAR).is_none() {
            return Ok(None);
        }
        self.expand_prompt_var(TRANSIENT_PROMPT_VAR, "")
            .await
            .map(Some)
    }

    pub(super) async fn expand_prompt_var(
        &mut self,
        var_name: &str,
        default: &str,
    ) -> Result<String, error::Error> {
        //
        // TODO(prompt): bash appears to do this in a subshell; we need to investigate
        // if that's required.
        //

        // Retrieve the spec.
        let prompt_spec = self.parameter_or_default(var_name, default);
        if prompt_spec.is_empty() {
            return Ok(String::new());
        }

        // Save (and later restore) the state the last command left behind, so that
        // expanding here is invisible to it.
        let saved_status = self.save_command_status();

        // Expand it. We must own the spec here: it borrows `self` (via the
        // returned `Cow`), and `expand_prompt` needs `&mut self`.
        let prompt_spec = prompt_spec.into_owned();
        let params = self.default_exec_params();
        let result = prompt::expand_prompt(self, &params, &prompt_spec).await;

        self.restore_command_status(saved_status);

        // Strip out special characters that readline would typically drop:
        // \001 and \002 (start and end of non-printing sequences).
        let mut expanded = result?;
        expanded.retain(|c| c != '\x01' && c != '\x02');

        Ok(expanded)
    }

    fn parameter_or_default<'a>(&'a self, name: &str, default: &'a str) -> Cow<'a, str> {
        self.env_str(name).unwrap_or_else(|| default.into())
    }
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]
mod tests {
    use crate::{ProfileLoadBehavior, RcLoadBehavior, Shell, ShellVariable, error};

    /// Bash expands the prompt as if inside double quotes, so quote characters in PS1
    /// stay literal: `PS1="it's \w"` must not lose its `'`.
    #[tokio::test]
    async fn prompt_keeps_quote_characters() -> Result<(), error::Error> {
        let mut shell = Shell::builder()
            .profile(ProfileLoadBehavior::Skip)
            .rc(RcLoadBehavior::Skip)
            .build()
            .await?;

        shell.env.set_global("v", ShellVariable::new("hi"))?;
        shell.env.set_global(
            "PS1",
            ShellVariable::new(r#"a"b"c 'd' it's \[x\] $v "$v" '$v' q\"r \\ s\x ${v:-"z"} "#),
        )?;
        assert_eq!(
            shell.compose_prompt().await?,
            r#"a"b"c 'd' it's x hi "hi" 'hi' q"r \ s\x hi "#
        );

        Ok(())
    }
}
