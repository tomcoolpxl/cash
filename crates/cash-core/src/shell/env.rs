//! Environment support for shell.

use std::borrow::Cow;

use crate::{ShellVariable, error};

impl<SE: crate::extensions::ShellExtensions> crate::Shell<SE> {
    /// Tries to retrieve a variable from the shell's environment, converting it into its
    /// string form.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn env_str(&self, name: &str) -> Option<Cow<'_, str>> {
        self.env.get_str(name, self)
    }

    /// Tries to retrieve a variable from the shell's environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn env_var(&self, name: &str) -> Option<&ShellVariable> {
        self.env.get(name).map(|(_, var)| var)
    }

    /// The value an assignment to `name` stores: for a variable with the integer attribute
    /// each value is evaluated as arithmetic, with the shell's variables, as Bash does
    /// however it is assigned. `read`, `printf -v`, `for` and array literals read names
    /// as 0, and `1/0` was silently 0 (LANG-11). An array literal's subscripts are left
    /// as they are. Any other variable's value is returned unchanged.
    ///
    /// # Errors
    ///
    /// The value's arithmetic error, such as division by zero.
    pub fn value_for_assignment(
        &mut self,
        name: &str,
        value: crate::variables::ShellValueLiteral,
    ) -> Result<crate::variables::ShellValueLiteral, crate::arithmetic::EvalError> {
        use crate::variables::{ArrayLiteral, ShellValueLiteral};

        Ok(match value {
            ShellValueLiteral::Scalar(text) => {
                ShellValueLiteral::Scalar(self.scalar_for_assignment(name, text)?)
            }
            ShellValueLiteral::Array(ArrayLiteral(elements)) => {
                ShellValueLiteral::Array(ArrayLiteral(
                    elements
                        .into_iter()
                        .map(|(key, text)| Ok((key, self.scalar_for_assignment(name, text)?)))
                        .collect::<Result<_, crate::arithmetic::EvalError>>()?,
                ))
            }
        })
    }

    /// [`Self::value_for_assignment`] for a scalar, or one element of an array.
    ///
    /// # Errors
    ///
    /// The value's arithmetic error, such as division by zero.
    pub fn scalar_for_assignment(
        &mut self,
        name: &str,
        value: String,
    ) -> Result<String, crate::arithmetic::EvalError> {
        let is_integer = self
            .env
            .assignment_target(name)
            .is_some_and(ShellVariable::is_treated_as_integer);
        if !is_integer {
            return Ok(value);
        }
        if value.trim().is_empty() {
            return Ok(String::from("0"));
        }
        Ok(crate::arithmetic::evaluate_str(self, &value)?.to_string())
    }

    /// An assignment to one of the variables the shell keeps itself (a dynamic one, while
    /// it is), given the meaning Bash gives it: `RANDOM` seeds, `SECONDS` restarts,
    /// `BASH_ARGV0` renames `$0`, `BASH_SUBSHELL` sets the level, `DIRSTACK[n]` replaces a
    /// folder of the stack, `BASH_ALIASES[name]` and `BASH_CMDS[name]` add an alias and a
    /// remembered program. To any other, as to `DIRSTACK[0]`, it does nothing, as in
    /// Bash. Whether `name` was such a variable; the value has been through
    /// [`Self::value_for_assignment`].
    ///
    /// Only `NAME=value` reached `RANDOM` and `SECONDS`, and nothing reached the others:
    /// `read`, `printf -v`, `declare` and `(( ))` dropped the value, and `DIRSTACK[1]=x`
    /// was "not yet implemented".
    ///
    /// # Errors
    ///
    /// None yet; the result is for a variable whose assignment can fail.
    pub fn assign_special(
        &mut self,
        name: &str,
        index: Option<&str>,
        value: &crate::variables::ShellValueLiteral,
        append: bool,
    ) -> Result<bool, error::Error> {
        use crate::variables::{ShellValue, ShellValueLiteral};

        let Some(resolved) = self
            .env
            .resolved_name(name)
            .map(std::borrow::Cow::into_owned)
        else {
            return Ok(false);
        };
        let is_special = self
            .env
            .assignment_target(&resolved)
            .is_some_and(|var| matches!(var.value(), ShellValue::Dynamic { .. }));
        if !is_special {
            return Ok(false);
        }
        let ShellValueLiteral::Scalar(value) = value else {
            return Ok(true);
        };
        let number = || value.trim().parse::<i64>().unwrap_or(0);

        match (resolved.as_str(), index) {
            ("RANDOM", None) => {
                let seed = if append {
                    i64::from(self.next_random()).wrapping_add(number())
                } else {
                    number()
                };
                #[expect(clippy::cast_sign_loss, reason = "Bash reads the seed as unsigned")]
                self.seed_random(seed as u64);
            }
            ("SECONDS", None) => {
                let current = || {
                    self.env_str("SECONDS")
                        .and_then(|now| now.parse::<i64>().ok())
                        .unwrap_or(0)
                };
                let seconds = if append {
                    current().saturating_add(number())
                } else {
                    number()
                };
                self.set_stopwatch_seconds(seconds);
            }
            ("BASH_ARGV0", None) => self.renamed_zero = Some(value.clone()),
            ("BASH_SUBSHELL", None) => {
                self.subshell_level = usize::try_from(number()).unwrap_or(0);
            }
            ("DIRSTACK", Some(index)) => {
                // `${DIRSTACK[0]}` is the working folder; 1 is the last one pushed.
                let stack_len = self.directory_stack.len();
                if let Ok(index) = index.trim().parse::<usize>()
                    && (1..=stack_len).contains(&index)
                    && let Some(entry) = self.directory_stack.get_mut(stack_len - index)
                {
                    *entry = std::path::PathBuf::from(value);
                }
            }
            ("BASH_ALIASES", Some(alias)) => {
                self.aliases.insert(alias.to_owned(), value.clone());
            }
            ("BASH_CMDS", Some(command)) => {
                self.program_location_cache
                    .set(command, std::path::PathBuf::from(value));
            }
            _ => (),
        }
        Ok(true)
    }

    /// Assigns `value` to `name`, or to its element `index`, as `read`, `printf -v`, `for`
    /// and an arithmetic assignment do: evaluated for an integer variable
    /// ([`Self::value_for_assignment`]), given its meaning for one the shell keeps
    /// ([`Self::assign_special`]), and otherwise stored where the name is found, or as a
    /// global.
    ///
    /// # Errors
    ///
    /// An arithmetic error in the value, or the error of the assignment itself, such as a
    /// read-only variable's.
    pub fn assign_variable(
        &mut self,
        name: &str,
        index: Option<String>,
        value: crate::variables::ShellValueLiteral,
    ) -> Result<(), error::Error> {
        use crate::env::{EnvironmentLookup, EnvironmentScope};
        use crate::variables::ShellValueLiteral;

        let value = self.value_for_assignment(name, value)?;
        if self.assign_special(name, index.as_deref(), &value, false)? {
            return Ok(());
        }
        match (index, value) {
            (Some(index), ShellValueLiteral::Scalar(value)) => {
                self.env.update_or_add_array_element(
                    name,
                    index,
                    value,
                    |_| Ok(()),
                    EnvironmentLookup::Anywhere,
                    EnvironmentScope::Global,
                )
            }
            (_, value) => self.env.update_or_add(
                name,
                value,
                |_| Ok(()),
                EnvironmentLookup::Anywhere,
                EnvironmentScope::Global,
            ),
        }
    }

    /// Tries to set a global variable in the shell's environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to add.
    /// * `var` - The variable contents to add.
    pub fn set_env_global(&mut self, name: &str, var: ShellVariable) -> Result<(), error::Error> {
        self.env.set_global(name, var)
    }
}
