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
