use clap::Parser;
use itertools::Itertools;
use std::io::Write;

use cash_core::{
    ExecutionResult, builtins,
    env::{self, EnvironmentLookup, EnvironmentScope},
    parser::ast,
    variables::{self, ShellValue, ShellValueUnsetType, ShellVariable},
};

/// Add or update exported shell variables.
#[derive(Parser)]
pub(crate) struct ExportCommand {
    /// Names are treated as function names.
    #[arg(short = 'f')]
    names_are_functions: bool,

    /// Un-export the names.
    #[arg(short = 'n')]
    unexport: bool,

    /// Display all exported names.
    #[arg(short = 'p')]
    display_exported_names: bool,

    /// Taken and not acted on, as Bash takes them (`export -a name`): they were invalid
    /// options, status 2.
    #[arg(short = 'a', hide = true)]
    _indexed_array: bool,

    #[arg(short = 'A', hide = true)]
    _associative_array: bool,

    //
    // Declarations
    //
    // N.B. These are skipped by clap, but filled in by the BuiltinDeclarationCommand trait.
    #[clap(skip)]
    declarations: Vec<cash_core::CommandArg>,
}

impl builtins::DeclarationCommand for ExportCommand {
    fn set_declarations(&mut self, declarations: Vec<cash_core::CommandArg>) {
        self.declarations = declarations;
    }
}

impl builtins::Command for ExportCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        if self.declarations.is_empty() {
            if self.names_are_functions {
                display_exported_functions(&context)?;
            } else {
                display_all_exported_vars(&context)?;
            }
            return Ok(ExecutionResult::success());
        }

        let mut result = ExecutionResult::success();
        for decl in &self.declarations {
            let current_result = self.process_decl(&mut context, decl)?;
            if !current_result.is_success() {
                result = current_result;
            }
        }

        Ok(result)
    }
}

impl ExportCommand {
    fn process_decl(
        &self,
        context: &mut cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        decl: &cash_core::CommandArg,
    ) -> Result<ExecutionResult, cash_core::Error> {
        match decl {
            cash_core::CommandArg::String(s) => {
                // See if this is supposed to be a function name.
                if self.names_are_functions {
                    // Try to find the function already present; if we find it, then mark it
                    // exported.
                    if let Some(func) = context.shell.func_mut(s) {
                        if self.unexport {
                            func.unexport();
                        } else {
                            func.export();
                        }
                    } else {
                        writeln!(
                            context.error_stream(),
                            "{}: {s}: not a function",
                            context.command_name
                        )?;
                        return Ok(ExecutionResult::general_error());
                    }
                }
                // Bash refuses a word that is no name, `1bad=x` or `a b`, and goes on with
                // the rest; cash took it and did nothing. An array element, `a[1]=2`, is
                // named without its value, as Bash names it.
                else if let Some(refused) = refused_name(s) {
                    writeln!(
                        context.error_stream(),
                        "{}: `{refused}': not a valid identifier",
                        context.command_name
                    )?;
                    // It ends the shell in POSIX mode, as in Bash.
                    context.shell.note_special_builtin_error();
                    return Ok(ExecutionResult::general_error());
                }
                // A word such as `export "X=1 2"` or `export $spec` is still an
                // assignment; its value was already expanded.
                else if let Some((name, append, value)) = split_string_assignment(s) {
                    return self.assign(
                        context,
                        name,
                        variables::ShellValueLiteral::Scalar(value.to_owned()),
                        append,
                    );
                }
                // A name the command's own prefix assignment set (`var=60 export var`) is
                // that variable, as Bash moves it out of the temporary environment: the
                // export was made on it and lost with the prefix.
                else if let Some(mut variable) =
                    context.shell.env_mut().take_from_command_scope(s)
                {
                    if self.unexport {
                        variable.unexport();
                    }
                    context
                        .shell
                        .env_mut()
                        .add(s.as_str(), variable, EnvironmentScope::Global)?;
                }
                // Try to find the variable already present; if we find it, then mark it
                // exported.
                else if let Some((_, variable)) = context.shell.env_mut().get_mut(s) {
                    if self.unexport {
                        variable.unexport();
                    } else {
                        variable.export();
                    }
                }
                // A name not set yet is made, unset and exported, as Bash makes it
                // (`declare -x U`): a later `U=5` is passed on. cash made nothing, and
                // the later value stayed in the shell.
                else if !self.unexport {
                    let mut variable =
                        ShellVariable::new(ShellValue::Unset(ShellValueUnsetType::Untyped));
                    variable.export();
                    context
                        .shell
                        .env_mut()
                        .add(s.as_str(), variable, EnvironmentScope::Global)?;
                }
            }
            cash_core::CommandArg::Assignment(assignment) => {
                let name = match &assignment.name {
                    ast::AssignmentName::VariableName(name) => name,
                    ast::AssignmentName::ArrayElementName(name, index) => {
                        writeln!(
                            context.error_stream(),
                            "{}: `{name}[{index}]': not a valid identifier",
                            context.command_name
                        )?;
                        context.shell.note_special_builtin_error();
                        return Ok(ExecutionResult::general_error());
                    }
                };

                let value = match &assignment.value {
                    ast::AssignmentValue::Scalar(s) => {
                        variables::ShellValueLiteral::Scalar(s.flatten())
                    }
                    ast::AssignmentValue::Array(a) => {
                        variables::ShellValueLiteral::Array(variables::ArrayLiteral(
                            a.iter()
                                .map(|(k, v)| (k.as_ref().map(|(k, _)| k.flatten()), v.flatten()))
                                .collect(),
                        ))
                    }
                };

                return self.assign(context, name, value, assignment.append);
            }
        }

        Ok(ExecutionResult::success())
    }

    /// Assigns `value` to `name` (appending for `+=`) and applies the export flag.
    fn assign(
        &self,
        context: &mut cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        name: &str,
        value: variables::ShellValueLiteral,
        append: bool,
    ) -> Result<ExecutionResult, cash_core::Error> {
        // `export name+=value` appends to the existing value, exactly like a
        // bare `name+=value`. update_or_add always replaces, so when the
        // variable already exists honor the append here. A missing variable
        // falls through: appending to nothing is a plain assignment.
        if append && let Some((_, variable)) = context.shell.env_mut().get_mut(name) {
            variable.assign(value, true)?;
            if self.unexport {
                variable.unexport();
            } else {
                variable.export();
            }
            return Ok(ExecutionResult::success());
        }

        // Update the variable with the provided value and then mark it exported.
        context.shell.env_mut().update_or_add(
            name,
            value,
            |var| {
                if self.unexport {
                    var.unexport();
                } else {
                    var.export();
                }
                Ok(())
            },
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;

        Ok(ExecutionResult::success())
    }
}

/// What `export` names in refusing `word`, when the name in it is not valid: the
/// element for `a[1]=2`, the whole word otherwise.
fn refused_name(word: &str) -> Option<&str> {
    let lhs = word.split_once('=').map_or(word, |(lhs, _)| lhs);
    let name = lhs.strip_suffix('+').unwrap_or(lhs);
    if env::valid_variable_name(name) {
        return None;
    }
    let element = name
        .strip_suffix(']')
        .and_then(|rest| rest.split_once('['))
        .is_some_and(|(array, _)| env::valid_variable_name(array));
    Some(if element { name } else { word })
}

/// Splits `name=value` or `name+=value` into its parts, if `name` is a valid identifier.
fn split_string_assignment(s: &str) -> Option<(&str, bool, &str)> {
    let (lhs, value) = s.split_once('=')?;
    let (name, append) = lhs
        .strip_suffix('+')
        .map_or((lhs, false), |name| (name, true));
    let mut chars = name.chars();
    let valid = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then_some((name, append, value))
}

/// `export -p`: each exported variable as `declare -p` writes it, its value quoted to be
/// read back and an array whole (`declare -ax a=([0]="x")`), or `export NAME="v"` in
/// POSIX mode. cash wrote the value bare, `"` and `$` unescaped, and an array's first
/// element.
fn display_all_exported_vars(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<(), cash_core::Error> {
    let posix = context.shell.options().posix_mode;
    for (name, variable) in context.shell.env().iter().sorted_by_key(|v| v.0) {
        if !variable.is_exported() {
            continue;
        }
        let value = variable.value();
        let assigned = if matches!(value, ShellValue::Unset(_)) {
            String::new()
        } else {
            format!(
                "={}",
                value.format(variables::FormatStyle::DeclarePrint, context.shell)?
            )
        };
        if posix {
            writeln!(context.stdout(), "export {name}{assigned}")?;
        } else {
            let flags = variable.attribute_flags(context.shell);
            writeln!(context.stdout(), "declare -{flags} {name}{assigned}")?;
        }
    }

    Ok(())
}

/// `export -f` or `export -pf` with no names: each exported function's definition and a
/// `declare -fx NAME` after it, as Bash writes them. cash listed the variables.
fn display_exported_functions(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<(), cash_core::Error> {
    for (name, registration) in context.shell.funcs().iter().sorted_by_key(|v| v.0) {
        if registration.is_exported() {
            writeln!(context.stdout(), "{}", registration.definition())?;
            writeln!(context.stdout(), "declare -fx {name}")?;
        }
    }

    Ok(())
}
