use clap::Parser;
use itertools::Itertools;
use std::{io::Write, sync::LazyLock};

use cash_core::{
    ErrorKind, ExecutionResult, builtins,
    env::{self, EnvironmentLookup, EnvironmentScope},
    parser::ast,
    variables::{
        self, ArrayLiteral, ShellValue, ShellValueLiteral, ShellValueUnsetType, ShellVariable,
        ShellVariableUpdateTransform,
    },
};

crate::minus_or_plus_flag_arg!(
    MakeIndexedArrayFlag,
    'a',
    "Make the variable an indexed array."
);
crate::minus_or_plus_flag_arg!(
    MakeAssociativeArrayFlag,
    'A',
    "Make the variable an associative array."
);
crate::minus_or_plus_flag_arg!(
    CapitalizeValueOnAssignmentFlag,
    'c',
    "Enable capitalize-on-assignment for the variable."
);
crate::minus_or_plus_flag_arg!(MakeIntegerFlag, 'i', "Mark the variable as integer-typed");
crate::minus_or_plus_flag_arg!(
    LowercaseValueOnAssignmentFlag,
    'l',
    "Enable lowercase-on-assignment for the variable."
);
crate::minus_or_plus_flag_arg!(
    MakeNameRefFlag,
    'n',
    "Mark the variable as a name reference"
);
crate::minus_or_plus_flag_arg!(MakeReadonlyFlag, 'r', "Mark the variable as read-only.");
crate::minus_or_plus_flag_arg!(MakeTracedFlag, 't', "Enable tracing for the variable.");
crate::minus_or_plus_flag_arg!(
    UppercaseValueOnAssignmentFlag,
    'u',
    "Enable uppercase-on-assignment for the variable."
);
crate::minus_or_plus_flag_arg!(MakeExportedFlag, 'x', "Mark the variable for export.");

/// An assignment spelled as an ordinary word: name, optional index, whether it
/// appends (`+=`), and value.
type StringAssignment = (String, Option<String>, bool, String);

/// Display or update variables and their attributes.
#[derive(Parser)]
#[clap(override_usage = "declare [OPTIONS] [DECLARATIONS]...")]
pub(crate) struct DeclareCommand {
    /// Constrain to function names or definitions.
    #[arg(short = 'f')]
    function_names_or_defs_only: bool,

    /// Constrain to function names only.
    #[arg(short = 'F')]
    function_names_only: bool,

    /// `+f` and `+F` are accepted and, as in Bash, ignored: turning the function
    /// attribute "off" means nothing, so `declare +ft f` acts on a variable `f`.
    #[arg(long = "+f", hide = true)]
    _plus_f: bool,
    #[arg(long = "+F", hide = true)]
    _plus_capital_f: bool,

    /// Create global variable, if applicable.
    #[arg(short = 'g')]
    create_global: bool,

    /// When creating a local variable that shadows another variable of the same name,
    /// then initialize it with the contents and attributes of the variable being shadowed.
    #[arg(short = 'I')]
    locals_inherit_from_prev_scope: bool,

    /// Display each item's attributes and values.
    #[arg(short = 'p')]
    print: bool,

    //
    // Attribute options
    #[clap(flatten)] // -a
    make_indexed_array: MakeIndexedArrayFlag,
    #[clap(flatten)] // -A
    make_associative_array: MakeAssociativeArrayFlag,
    #[clap(flatten)] // -c
    capitalize_value_on_assignment: CapitalizeValueOnAssignmentFlag,
    #[clap(flatten)] // -i
    make_integer: MakeIntegerFlag,
    #[clap(flatten)] // -l
    lowercase_value_on_assignment: LowercaseValueOnAssignmentFlag,
    #[clap(flatten)] // -n
    make_nameref: MakeNameRefFlag,
    #[clap(flatten)] // -r
    make_readonly: MakeReadonlyFlag,
    #[clap(flatten)] // -t
    make_traced: MakeTracedFlag,
    #[clap(flatten)] // -u
    uppercase_value_on_assignment: UppercaseValueOnAssignmentFlag,
    #[clap(flatten)] // -x
    make_exported: MakeExportedFlag,

    //
    // Declarations
    //
    // N.B. These are skipped by clap, but filled in by the BuiltinDeclarationCommand trait.
    #[clap(skip)]
    declarations: Vec<cash_core::CommandArg>,
}

#[derive(Clone, Copy)]
enum DeclareVerb {
    Declare,
    Local,
    Readonly,
}

impl builtins::DeclarationCommand for DeclareCommand {
    fn set_declarations(&mut self, declarations: Vec<cash_core::CommandArg>) {
        self.declarations = declarations;
    }
}

impl builtins::Command for DeclareCommand {
    fn takes_plus_options() -> bool {
        true
    }

    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        let verb = match context.command_name.as_str() {
            "local" => DeclareVerb::Local,
            "readonly" => DeclareVerb::Readonly,
            _ => DeclareVerb::Declare,
        };

        if matches!(verb, DeclareVerb::Local) && !context.shell.in_function() {
            writeln!(context.stderr(), "can only be used in a function")?;
            return Ok(ExecutionResult::general_error());
        }

        let mut result = ExecutionResult::success();
        if !self.declarations.is_empty() {
            for declaration in &self.declarations {
                if self.print && !matches!(verb, DeclareVerb::Readonly) {
                    if !self.try_display_declaration(&context, declaration, verb)? {
                        result = ExecutionResult::general_error();
                    }
                } else {
                    if !self.process_declaration(&mut context, declaration, verb)? {
                        result = ExecutionResult::general_error();
                    }
                }
            }
        } else {
            // Display matching declarations from the variable environment.
            if !self.function_names_only && !self.function_names_or_defs_only {
                self.display_matching_env_declarations(&context, verb)?;
            }

            // Do the same for functions.
            if !matches!(verb, DeclareVerb::Local | DeclareVerb::Readonly)
                && (!self.print || self.function_names_only || self.function_names_or_defs_only)
            {
                self.display_matching_functions(&context)?;
            }
        }

        Ok(result)
    }
}

impl DeclareCommand {
    fn try_display_declaration(
        &self,
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        declaration: &cash_core::CommandArg,
        verb: DeclareVerb,
    ) -> Result<bool, cash_core::Error> {
        let name = match declaration {
            cash_core::CommandArg::String(s) => s,
            cash_core::CommandArg::Assignment(_) => {
                writeln!(context.stderr(), "declare: {declaration}: not found")?;
                return Ok(false);
            }
        };

        let lookup = if matches!(verb, DeclareVerb::Local) {
            EnvironmentLookup::OnlyInCurrentLocal
        } else {
            EnvironmentLookup::Anywhere
        };

        if self.function_names_only || self.function_names_or_defs_only {
            if let Some(func_registration) = context.shell.funcs().get(name) {
                if self.function_names_only {
                    if self.print {
                        writeln!(context.stdout(), "declare -f {name}")?;
                    } else {
                        writeln!(context.stdout(), "{name}")?;
                    }
                } else {
                    writeln!(context.stdout(), "{}", func_registration.definition())?;
                }
                Ok(true)
            } else {
                // For some reason, bash does not print an error message in this case.
                Ok(false)
            }
        } else if let Some(variable) = context.shell.env().get_using_policy(name, lookup) {
            let mut cs = variable.attribute_flags(context.shell);
            if cs.is_empty() {
                cs.push('-');
            }

            let resolved_value = variable.resolve_value(context.shell);
            let separator_str = if matches!(resolved_value, ShellValue::Unset(_)) {
                ""
            } else {
                "="
            };

            writeln!(
                context.stdout(),
                "declare -{cs} {name}{separator_str}{}",
                resolved_value.format(variables::FormatStyle::DeclarePrint, context.shell)?
            )?;

            Ok(true)
        } else {
            writeln!(context.stderr(), "declare: {name}: not found")?;
            Ok(false)
        }
    }

    /// `declare -f` with attribute flags applies them to the named function rather than
    /// displaying it (e.g. `declare -ft name`, `declare -fx name`).
    fn apply_function_attributes(
        &self,
        context: &mut cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        declaration: &cash_core::CommandArg,
    ) -> bool {
        let func = match declaration {
            cash_core::CommandArg::String(name) => context.shell.func_mut(name),
            cash_core::CommandArg::Assignment(_) => None,
        };

        // As with display, bash reports failure without printing an error message here.
        let Some(func) = func else {
            return false;
        };

        match self.make_exported.to_bool() {
            Some(true) => func.export(),
            Some(false) => func.unexport(),
            None => (),
        }

        if let Some(traced) = self.make_traced.to_bool() {
            func.set_traced(traced);
        }

        true
    }

    /// Whether `declare -n name=name` may stand.
    ///
    /// cash: a reference that stands for itself can never resolve, so bash refuses it —
    /// except for a local, where `local -n out=$1` called as `fill out` is an ordinary
    /// mistake rather than nonsense. There it warns, leaves the reference circular and
    /// carries on, so the function fails to write rather than dying.
    fn allow_self_reference(
        &self,
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        name: &str,
        initial_value: Option<&ShellValueLiteral>,
        create_var_local: bool,
    ) -> Result<bool, cash_core::Error> {
        if self.make_nameref.to_bool() != Some(true) {
            return Ok(true);
        }

        let Some(ShellValueLiteral::Scalar(target)) = initial_value else {
            return Ok(true);
        };

        if target != name {
            return Ok(true);
        }

        if create_var_local {
            writeln!(
                context.stderr(),
                "{}: warning: {name}: circular name reference",
                context.command_name
            )?;
            return Ok(true);
        }

        writeln!(
            context.stderr(),
            "{}: {name}: nameref variable self references not allowed",
            context.command_name
        )?;
        Ok(false)
    }

    /// Finds the variable a declaration is about.
    ///
    /// cash: with `-n` this is the reference itself rather than what it stands for.
    /// `declare -n ref=other` re-points an existing reference, where following it would
    /// assign `other` to the old target and quietly turn *that* into a reference too.
    fn look_up<'a, SE: cash_core::ShellExtensions>(
        &self,
        context: &'a mut cash_core::ExecutionContext<'_, SE>,
        name: &str,
        lookup: EnvironmentLookup,
    ) -> Option<&'a mut ShellVariable> {
        if self.make_nameref.to_bool() == Some(true) {
            context
                .shell
                .env_mut()
                .get_mut_using_policy_raw(name, lookup)
        } else {
            context.shell.env_mut().get_mut_using_policy(name, lookup)
        }
    }

    #[allow(clippy::too_many_lines)]
    fn process_declaration(
        &self,
        context: &mut cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        declaration: &cash_core::CommandArg,
        verb: DeclareVerb,
    ) -> Result<bool, cash_core::Error> {
        let create_var_local = matches!(verb, DeclareVerb::Local)
            || (matches!(verb, DeclareVerb::Declare)
                && context.shell.in_function()
                && !self.create_global);

        if (self.function_names_or_defs_only || self.function_names_only)
            && (self.make_traced.to_bool().is_some() || self.make_exported.to_bool().is_some())
        {
            return Ok(self.apply_function_attributes(context, declaration));
        }

        if self.function_names_or_defs_only || self.function_names_only {
            return self.try_display_declaration(context, declaration, verb);
        }

        // Extract the variable name and the initial value being assigned (if any).
        let (name, assigned_index, mut initial_value, name_is_array, append) =
            Self::declaration_to_name_and_value(declaration)?;

        let is_int_decl = self.make_integer.to_bool() == Some(true)
            || context
                .shell
                .env()
                .get(name.as_str())
                .is_some_and(|(_, v)| v.is_treated_as_integer());
        if is_int_decl {
            if let Some(ShellValueLiteral::Scalar(ref mut s)) = initial_value {
                if let Ok(eval_val) = cash_core::arithmetic::evaluate_str(context.shell, s.as_str())
                {
                    *s = eval_val.to_string();
                }
            }
        }

        // Special-case: `local -`
        if name == "-" && matches!(verb, DeclareVerb::Local) {
            context.shell.save_local_options();
            return Ok(true);
        }

        // Make sure it's a valid name.
        if !env::valid_variable_name(name.as_str()) {
            writeln!(
                context.stderr(),
                "{}: {name}: not a valid variable name",
                context.command_name
            )?;
            return Ok(false);
        }

        if !self.allow_self_reference(context, &name, initial_value.as_ref(), create_var_local)? {
            return Ok(false);
        }

        // Figure out where we should look.
        let lookup = if create_var_local {
            EnvironmentLookup::OnlyInCurrentLocal
        } else {
            EnvironmentLookup::Anywhere
        };

        // `local -I x[=v]` / `declare -I` (bash 5.0+): the new local inherits
        // value and attributes from the nearest same-name variable in an
        // enclosing scope instead of starting unset; `+=` appends to the
        // inherited value. With no same-name variable anywhere, fall through
        // to ordinary creation.
        if self.locals_inherit_from_prev_scope && create_var_local {
            let inherited = context
                .shell
                .env()
                .get_using_policy(name.as_str(), EnvironmentLookup::Anywhere)
                .cloned();

            if let Some(mut var) = inherited {
                self.apply_attributes_before_update(&mut var)?;

                if let Some(initial_value) = initial_value {
                    var.assign(initial_value, append || assigned_index.is_some())?;
                }

                if context.shell.options().export_variables_on_modification
                    && !var.value().is_array()
                {
                    var.export();
                }

                self.apply_attributes_after_update(&mut var, verb)?;

                context
                    .shell
                    .env_mut()
                    .add(name, var, EnvironmentScope::Local)?;
                return Ok(true);
            }
        }

        if let Some(var) = self.look_up(context, name.as_str(), lookup) {
            if self.make_associative_array.is_some() {
                var.convert_to_associative_array()?;
            }
            if self.make_indexed_array.is_some() {
                var.convert_to_indexed_array()?;
            }

            self.apply_attributes_before_update(var)?;

            if let Some(initial_value) = initial_value {
                // We append for `name+=value`, or if the declaration included
                // an explicit index.
                var.assign(initial_value, append || assigned_index.is_some())?;
            }

            self.apply_attributes_after_update(var, verb)?;
        } else {
            let unset_type = if self.make_indexed_array.is_some() {
                ShellValueUnsetType::IndexedArray
            } else if self.make_associative_array.is_some() {
                ShellValueUnsetType::AssociativeArray
            } else if name_is_array {
                ShellValueUnsetType::IndexedArray
            } else {
                ShellValueUnsetType::Untyped
            };

            let mut var = ShellVariable::new(ShellValue::Unset(unset_type));

            self.apply_attributes_before_update(&mut var)?;

            if let Some(initial_value) = initial_value {
                var.assign(initial_value, append)?;
            }

            if context.shell.options().export_variables_on_modification && !var.value().is_array() {
                var.export();
            }

            self.apply_attributes_after_update(&mut var, verb)?;

            let scope = if create_var_local {
                EnvironmentScope::Local
            } else {
                EnvironmentScope::Global
            };

            context.shell.env_mut().add(name, var, scope)?;
        }

        Ok(true)
    }

    /// Splits a word of the form `name=value`, `name+=value` or `name[index]=value` into
    /// its name, index, append flag and value. Returns `None` for anything else.
    fn string_assignment(s: &str) -> Option<StringAssignment> {
        #[allow(clippy::unwrap_used, reason = "regex is valid and should not fail")]
        static ASSIGNMENT_RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
            fancy_regex::Regex::new(r"(?s)^([A-Za-z_][A-Za-z0-9_]*)(?:\[(.*?)\])?(\+?)=(.*)$")
                .unwrap()
        });

        let captures = ASSIGNMENT_RE.captures(s).ok()??;
        Some((
            captures.get(1)?.as_str().to_owned(),
            captures.get(2).map(|m| m.as_str().to_owned()),
            captures.get(3).is_some_and(|m| !m.as_str().is_empty()),
            captures.get(4)?.as_str().to_owned(),
        ))
    }

    #[expect(clippy::type_complexity)]
    fn declaration_to_name_and_value(
        declaration: &cash_core::CommandArg,
    ) -> Result<
        (
            String,
            Option<String>,
            Option<ShellValueLiteral>,
            bool,
            bool,
        ),
        cash_core::Error,
    > {
        let name;
        let assigned_index;
        let initial_value;
        let name_is_array;
        let append;

        // An assignment that reached us as an ordinary word — `declare "a=x y"` or
        // `declare $spec` — whose value was already expanded. It is used as is, without
        // field splitting or another round of expansion.
        if let cash_core::CommandArg::String(s) = declaration
            && let Some((name, index, append, value)) = Self::string_assignment(s)
        {
            return Ok(match index {
                Some(index) => (
                    name,
                    Some(index.clone()),
                    Some(ShellValueLiteral::Array(ArrayLiteral(vec![(
                        Some(index),
                        value,
                    )]))),
                    true,
                    append,
                ),
                None => (
                    name,
                    None,
                    Some(ShellValueLiteral::Scalar(value)),
                    false,
                    append,
                ),
            });
        }

        match declaration {
            cash_core::CommandArg::String(s) => {
                // We need to handle the case of someone invoking `declare array[index]`.
                // In such case, we ignore the index and treat it as a declaration of
                // the array.
                #[allow(
                    clippy::unwrap_in_result,
                    clippy::unwrap_used,
                    reason = "regex is valid and should not fail"
                )]
                static ARRAY_AND_INDEX_RE: LazyLock<fancy_regex::Regex> =
                    LazyLock::new(|| fancy_regex::Regex::new(r"^(.*?)\[(.*?)\]$").unwrap());

                if let Some(captures) = ARRAY_AND_INDEX_RE.captures(s)? {
                    name = captures
                        .get(1)
                        .ok_or_else(|| {
                            cash_core::ErrorKind::InternalError("declaration parse error".into())
                        })?
                        .as_str()
                        .to_owned();

                    assigned_index = captures.get(2).map(|m| m.as_str().to_owned());
                    name_is_array = true;
                } else {
                    name = s.clone();
                    assigned_index = None;
                    name_is_array = false;
                }
                initial_value = None;
                append = false;
            }
            cash_core::CommandArg::Assignment(assignment) => {
                match &assignment.name {
                    ast::AssignmentName::VariableName(var_name) => {
                        name = var_name.to_owned();
                        assigned_index = None;
                    }
                    ast::AssignmentName::ArrayElementName(var_name, index) => {
                        if matches!(assignment.value, ast::AssignmentValue::Array(_)) {
                            return Err(ErrorKind::AssigningListToArrayMember.into());
                        }

                        name = var_name.to_owned();
                        assigned_index = Some(index.to_owned());
                    }
                }

                append = assignment.append;

                match &assignment.value {
                    ast::AssignmentValue::Scalar(s) => {
                        if let Some(index) = &assigned_index {
                            initial_value = Some(ShellValueLiteral::Array(ArrayLiteral(vec![(
                                Some(index.to_owned()),
                                s.value.clone(),
                            )])));
                            name_is_array = true;
                        } else {
                            initial_value = Some(ShellValueLiteral::Scalar(s.value.clone()));
                            name_is_array = false;
                        }
                    }
                    ast::AssignmentValue::Array(a) => {
                        initial_value = Some(ShellValueLiteral::Array(ArrayLiteral(
                            a.iter()
                                .map(|(i, v)| {
                                    (i.as_ref().map(|w| w.value.clone()), v.value.clone())
                                })
                                .collect(),
                        )));
                        name_is_array = true;
                    }
                }
            }
        }

        Ok((name, assigned_index, initial_value, name_is_array, append))
    }

    fn display_matching_env_declarations(
        &self,
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        verb: DeclareVerb,
    ) -> Result<(), cash_core::Error> {
        if matches!(verb, DeclareVerb::Local) && context.shell.has_saved_local_options() {
            writeln!(context.stdout(), "local -")?;
        }

        //
        // Dump all declarations. Use attribute flags to filter which variables are dumped.
        //

        // We start by excluding all variables that are not enumerable.
        #[expect(clippy::type_complexity)]
        let mut filters: Vec<Box<dyn Fn((&String, &ShellVariable)) -> bool>> =
            vec![Box::new(|(_, v)| v.is_enumerable())];

        // Add filters depending on verb.
        if matches!(verb, DeclareVerb::Readonly) {
            filters.push(Box::new(|(_, v)| v.is_readonly()));
        }

        // Add filters depending on attribute flags.
        if let Some(value) = self.make_indexed_array.to_bool() {
            filters.push(Box::new(move |(_, v)| {
                matches!(v.value(), ShellValue::IndexedArray(_)) == value
            }));
        }
        if let Some(value) = self.make_associative_array.to_bool() {
            filters.push(Box::new(move |(_, v)| {
                matches!(v.value(), ShellValue::AssociativeArray(_)) == value
            }));
        }
        if let Some(value) = self.make_integer.to_bool() {
            filters.push(Box::new(move |(_, v)| v.is_treated_as_integer() == value));
        }
        if let Some(value) = self.capitalize_value_on_assignment.to_bool() {
            filters.push(Box::new(move |(_, v)| {
                matches!(
                    v.get_update_transform(),
                    ShellVariableUpdateTransform::Capitalize
                ) == value
            }));
        }
        if let Some(value) = self.lowercase_value_on_assignment.to_bool() {
            filters.push(Box::new(move |(_, v)| {
                matches!(
                    v.get_update_transform(),
                    ShellVariableUpdateTransform::Lowercase
                ) == value
            }));
        }
        if let Some(value) = self.make_nameref.to_bool() {
            filters.push(Box::new(move |(_, v)| v.is_treated_as_nameref() == value));
        }
        if let Some(value) = self.make_readonly.to_bool() {
            filters.push(Box::new(move |(_, v)| v.is_readonly() == value));
        }
        if let Some(value) = self.make_traced.to_bool() {
            filters.push(Box::new(move |(_, v)| v.is_trace_enabled() == value));
        }
        if let Some(value) = self.uppercase_value_on_assignment.to_bool() {
            filters.push(Box::new(move |(_, v)| {
                matches!(
                    v.get_update_transform(),
                    ShellVariableUpdateTransform::Uppercase
                ) == value
            }));
        }
        if let Some(value) = self.make_exported.to_bool() {
            filters.push(Box::new(move |(_, v)| v.is_exported() == value));
        }

        let iter_policy = if matches!(verb, DeclareVerb::Local) {
            EnvironmentLookup::OnlyInCurrentLocal
        } else {
            EnvironmentLookup::Anywhere
        };

        // Iterate through an ordered list of all matching declarations tracked in the
        // environment.
        for (name, variable) in context
            .shell
            .env()
            .iter_using_policy(iter_policy)
            .filter(|pair| filters.iter().all(|f| f(*pair)))
            .sorted_by_key(|v| v.0)
        {
            if self.print {
                let mut cs = variable.attribute_flags(context.shell);
                if cs.is_empty() {
                    cs.push('-');
                }

                let separator_str = if matches!(variable.value(), ShellValue::Unset(_)) {
                    ""
                } else {
                    "="
                };

                writeln!(
                    context.stdout(),
                    "declare -{cs} {name}{separator_str}{}",
                    variable
                        .value()
                        .format(variables::FormatStyle::DeclarePrint, context.shell)?
                )?;
            } else {
                writeln!(
                    context.stdout(),
                    "{name}={}",
                    variable
                        .value()
                        .format(variables::FormatStyle::Basic, context.shell)?
                )?;
            }
        }

        Ok(())
    }

    fn display_matching_functions(
        &self,
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    ) -> Result<(), cash_core::Error> {
        for (name, registration) in context.shell.funcs().iter().sorted_by_key(|v| v.0) {
            if self.function_names_only {
                writeln!(context.stdout(), "declare -f {name}")?;
            } else {
                writeln!(context.stdout(), "{}", registration.definition())?;
            }
        }

        Ok(())
    }

    #[expect(clippy::unnecessary_wraps)]
    const fn apply_attributes_before_update(
        &self,
        var: &mut ShellVariable,
    ) -> Result<(), cash_core::Error> {
        if let Some(value) = self.make_integer.to_bool() {
            if value {
                var.treat_as_integer();
            } else {
                var.unset_treat_as_integer();
            }
        }
        if let Some(value) = self.capitalize_value_on_assignment.to_bool() {
            if value {
                var.set_update_transform(ShellVariableUpdateTransform::Capitalize);
            } else if matches!(
                var.get_update_transform(),
                ShellVariableUpdateTransform::Capitalize
            ) {
                var.set_update_transform(ShellVariableUpdateTransform::None);
            }
        }
        if let Some(value) = self.lowercase_value_on_assignment.to_bool() {
            if value {
                var.set_update_transform(ShellVariableUpdateTransform::Lowercase);
            } else if matches!(
                var.get_update_transform(),
                ShellVariableUpdateTransform::Lowercase
            ) {
                var.set_update_transform(ShellVariableUpdateTransform::None);
            }
        }
        if let Some(value) = self.make_nameref.to_bool() {
            if value {
                var.treat_as_nameref();
            } else {
                var.unset_treat_as_nameref();
            }
        }
        if let Some(value) = self.make_traced.to_bool() {
            if value {
                var.enable_trace();
            } else {
                var.disable_trace();
            }
        }
        if let Some(value) = self.uppercase_value_on_assignment.to_bool() {
            if value {
                var.set_update_transform(ShellVariableUpdateTransform::Uppercase);
            } else if matches!(
                var.get_update_transform(),
                ShellVariableUpdateTransform::Uppercase
            ) {
                var.set_update_transform(ShellVariableUpdateTransform::None);
            }
        }
        if let Some(value) = self.make_exported.to_bool() {
            if value {
                var.export();
            } else {
                var.unexport();
            }
        }

        Ok(())
    }

    fn apply_attributes_after_update(
        &self,
        var: &mut ShellVariable,
        verb: DeclareVerb,
    ) -> Result<(), cash_core::Error> {
        if matches!(verb, DeclareVerb::Readonly) {
            var.set_readonly();
        } else if let Some(value) = self.make_readonly.to_bool() {
            if value {
                var.set_readonly();
            } else {
                var.unset_readonly()?;
            }
        }

        Ok(())
    }
}
