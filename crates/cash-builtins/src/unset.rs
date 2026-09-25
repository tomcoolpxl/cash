use std::borrow::Cow;

use clap::Parser;

use cash_core::{ExecutionResult, Shell, builtins};

/// Unset a variable.
#[derive(Parser)]
pub(crate) struct UnsetCommand {
    #[clap(flatten)]
    name_interpretation: UnsetNameInterpretation,

    /// Names of variables to unset.
    names: Vec<String>,
}

#[derive(Parser)]
#[clap(group = clap::ArgGroup::new("name-interpretation").multiple(false).required(false))]
pub(crate) struct UnsetNameInterpretation {
    /// Treat each name as a shell function.
    #[arg(short = 'f', group = "name-interpretation")]
    shell_functions: bool,

    /// Treat each name as a shell variable.
    #[arg(short = 'v', group = "name-interpretation")]
    shell_variables: bool,

    /// Treat each name as a name reference.
    #[arg(short = 'n', group = "name-interpretation")]
    name_references: bool,
}

impl UnsetNameInterpretation {
    pub const fn unspecified(&self) -> bool {
        !self.shell_functions && !self.shell_variables && !self.name_references
    }
}

impl builtins::Command for UnsetCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        // cash: `unset -n ref` removes the reference itself, where a plain `unset ref`
        // now removes what it stands for. This refused outright, so the only way to get
        // rid of a reference was to leave it lying around.
        if self.name_interpretation.name_references {
            for name in &self.names {
                context.shell.env_mut().unset_raw(name)?;
            }

            return Ok(ExecutionResult::success());
        }

        let unspecified = self.name_interpretation.unspecified();

        #[expect(clippy::needless_continue)]
        for name in &self.names {
            if unspecified || self.name_interpretation.shell_variables {
                // Try to parse the name as a parameter. If we can't, don't bail; it may not be a
                // valid variable name/parameter but could still be a function name.
                if let Ok(parameter) =
                    cash_parser::word::parse_parameter(name, &context.shell.parser_options())
                {
                    let result = match parameter {
                        cash_parser::word::Parameter::Positional(_) => continue,
                        cash_parser::word::Parameter::Special(_) => continue,
                        cash_parser::word::Parameter::Named(name) => {
                            context.shell.env_mut().unset(name.as_str())?.is_some()
                        }
                        cash_parser::word::Parameter::NamedWithIndex { name, index } => {
                            unset_array_index(
                                context.shell,
                                &context.params,
                                name.as_str(),
                                index.as_str(),
                            )
                            .await?
                        }
                        cash_parser::word::Parameter::NamedWithAllIndices { name, concatenate } => {
                            unset_all_indices(
                                context.shell,
                                name.as_str(),
                                if concatenate { "*" } else { "@" },
                            )?
                        }
                    };

                    if result {
                        continue;
                    }
                }
            }

            // TODO(unset): Deal with readonly functions
            if unspecified || self.name_interpretation.shell_functions {
                if context.shell.undefine_func(name) {
                    continue;
                }
            }
        }

        Ok(ExecutionResult::success())
    }
}

fn unset_all_indices(
    shell: &mut Shell<impl cash_core::ShellExtensions>,
    name: &str,
    spelling: &str,
) -> Result<bool, cash_core::Error> {
    let Some((_, variable)) = shell.env_mut().get_mut(name) else {
        return Ok(false);
    };

    if variable.value().is_associative_array() {
        return variable.unset_index(spelling);
    }

    if variable.value().is_indexed_array() {
        let had_elements = matches!(
            variable.value(),
            cash_core::variables::ShellValue::IndexedArray(elements) if !elements.is_empty()
        );
        variable.assign(
            cash_core::variables::ShellValueLiteral::Array(cash_core::variables::ArrayLiteral(
                Vec::new(),
            )),
            false,
        )?;
        return Ok(had_elements);
    }

    Ok(false)
}

async fn unset_array_index(
    shell: &mut Shell<impl cash_core::ShellExtensions>,
    params: &cash_core::ExecutionParameters,
    name: &str,
    index: &str,
) -> Result<bool, cash_core::Error> {
    // First check to see if it's an associative array.
    let is_assoc_array = shell
        .env()
        .get(name)
        .is_some_and(|(_, var)| var.value().is_associative_array());

    // Compute which index we should actually use. For indexed arrays, we need to evaluate
    // the index string as an arithmetic expression first.
    // The word was already expanded once as an argument. `assoc_expand_once` (Bash 5.3:
    // `array_expand_once`) keeps it from being expanded a second time here.
    let index_to_use: Cow<'_, str> = if is_assoc_array {
        if shell.options().assoc_expand_once {
            index.into()
        } else {
            shell.basic_expand_string(params, index).await?.into()
        }
    } else {
        // First evaluate the index expression.
        let index_as_expr = cash_parser::arithmetic::parse(index)?;
        let evaluated_index = shell.eval_arithmetic(&index_as_expr)?;
        evaluated_index.to_string().into()
    };

    // Now we can try to unset, and return the result.
    shell.env_mut().unset_index(name, index_to_use.as_ref())
}
