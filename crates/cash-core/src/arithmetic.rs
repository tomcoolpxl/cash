//! Arithmetic evaluation

use std::borrow::Cow;

use crate::{ExecutionParameters, Shell, env, expansion, extensions, variables};
use cash_parser::ast;

/// Maximum recursion depth for arithmetic variable dereference chains
/// (e.g., a=b, b=c, c=a would cycle through variable dereferences).
const MAX_VARIABLE_DEREF_DEPTH: u32 = 64;

/// Represents an error that occurs during evaluation of an arithmetic expression.
#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    /// Division by zero.
    #[error("division by zero")]
    DivideByZero,

    /// Negative exponent.
    #[error("exponent less than 0")]
    NegativeExponent,

    /// Failed to tokenize an arithmetic expression.
    #[error("failed to tokenize expression")]
    FailedToTokenizeExpression,

    /// Failed to expand an arithmetic expression.
    #[error("failed to expand expression: {0}")]
    FailedToExpandExpression(String),

    /// Failed to access an element of an array.
    #[error("failed to access array")]
    FailedToAccessArray,

    /// Failed to update the shell environment in an assignment operator.
    #[error("failed to update environment")]
    FailedToUpdateEnvironment,

    /// Failed to parse an arithmetic expression.
    #[error("failed to parse expression: {0}")]
    ParseError(String),

    /// Error expanding an unset variable.
    #[error("expanding unset variable: {0}")]
    ExpandingUnsetVariable(String),

    /// Expression recursion level exceeded.
    #[error("expression recursion level exceeded")]
    RecursionLimitExceeded,
}

/// Trait implemented by arithmetic expressions that can be evaluated.
pub(crate) trait ExpandAndEvaluate {
    /// Evaluate the given expression, returning the resulting numeric value.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell to use for evaluation.
    /// * `trace_if_needed` - Whether to trace the evaluation.
    async fn eval(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
        trace_if_needed: bool,
    ) -> Result<i64, EvalError>;
}

impl ExpandAndEvaluate for ast::UnexpandedArithmeticExpr {
    async fn eval(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        params: &ExecutionParameters,
        trace_if_needed: bool,
    ) -> Result<i64, EvalError> {
        expand_and_eval(shell, params, self.value.as_str(), trace_if_needed).await
    }
}

/// Evaluate the given arithmetic expression, returning the resulting numeric value.
///
/// # Arguments
///
/// * `shell` - The shell to use for evaluation.
/// * `expr` - The unexpanded arithmetic expression to evaluate.
/// * `trace_if_needed` - Whether to trace the evaluation.
pub(crate) async fn expand_and_eval(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    expr: &str,
    trace_if_needed: bool,
) -> Result<i64, EvalError> {
    // Per documentation, first shell-expand it.
    let options = expansion::ExpanderOptions {
        tilde_expand: false,
        ..Default::default()
    };
    let expanded_self = expansion::basic_expand_word_with_options(shell, params, expr, &options)
        .await
        .map_err(|_e| EvalError::FailedToExpandExpression(expr.to_owned()))?;

    // Now parse.
    let expr = cash_parser::arithmetic::parse(&expanded_self)
        .map_err(|_e| EvalError::ParseError(expanded_self))?;

    // Trace if applicable.
    if trace_if_needed && shell.options().print_commands_and_arguments {
        shell
            .trace_command(params, std::format!("(( {expr} ))"))
            .await;
    }

    // Now evaluate.
    expr.eval(shell)
}

/// Trait implemented by evaluatable arithmetic expressions.
pub trait Evaluatable {
    /// Evaluate the given arithmetic expression, returning the resulting numeric value.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell to use for evaluation.
    fn eval(&self, shell: &mut Shell<impl extensions::ShellExtensions>) -> Result<i64, EvalError>;
}

impl Evaluatable for ast::ArithmeticExpr {
    fn eval(&self, shell: &mut Shell<impl extensions::ShellExtensions>) -> Result<i64, EvalError> {
        eval_expr_impl(self, shell, 0)
    }
}

fn eval_expr_impl(
    expr: &ast::ArithmeticExpr,
    shell: &mut Shell<impl extensions::ShellExtensions>,
    depth: u32,
) -> Result<i64, EvalError> {
    let value = match expr {
        ast::ArithmeticExpr::Literal(l) => *l,
        ast::ArithmeticExpr::Reference(lvalue) => deref_lvalue(shell, lvalue, depth)?,
        ast::ArithmeticExpr::UnaryOp(op, operand) => apply_unary_op(shell, *op, operand, depth)?,
        ast::ArithmeticExpr::BinaryOp(op, left, right) => {
            apply_binary_op(shell, *op, left, right, depth)?
        }
        ast::ArithmeticExpr::Conditional(condition, then_expr, else_expr) => {
            let conditional_eval = eval_expr_impl(condition, shell, depth)?;

            // Ensure we only evaluate the branch indicated by the condition.
            if conditional_eval != 0 {
                eval_expr_impl(then_expr, shell, depth)?
            } else {
                eval_expr_impl(else_expr, shell, depth)?
            }
        }
        ast::ArithmeticExpr::Assignment(lvalue, rhs) => {
            let expr_eval = eval_expr_impl(rhs, shell, depth)?;
            assign(shell, lvalue, expr_eval, depth)?
        }
        ast::ArithmeticExpr::UnaryAssignment(op, lvalue) => {
            apply_unary_assignment_op(shell, lvalue, *op, depth)?
        }
        ast::ArithmeticExpr::BinaryAssignment(op, lvalue, operand) => {
            let value = apply_binary_op(
                shell,
                *op,
                &ast::ArithmeticExpr::Reference(lvalue.clone()),
                operand,
                depth,
            )?;
            assign(shell, lvalue, value, depth)?
        }
    };

    Ok(value)
}

fn get_var_value<'a>(
    shell: &'a Shell<impl extensions::ShellExtensions>,
    name: &str,
) -> Result<Cow<'a, str>, EvalError> {
    let value = shell.env_var(name).map(|var| var.resolve_value(shell));

    if let Some(value) = value
        && value.is_set()
    {
        return Ok(value.to_cow_str(shell).to_string().into());
    }

    if shell.options().treat_unset_variables_as_error {
        return Err(EvalError::ExpandingUnsetVariable(name.into()));
    }

    Ok("".into())
}

/// Turns an arithmetic array subscript into the element's key. For an associative
/// array that is the subscript's text, with `$name` and `${name}` expanded (`let
/// 'count[$word]++'`); for anything else it is the subscript evaluated as arithmetic.
fn resolve_subscript(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    name: &str,
    subscript: &ast::ArraySubscript,
    depth: u32,
) -> Result<String, EvalError> {
    let is_associative = shell
        .env()
        .get(name)
        .is_some_and(|(_, v)| v.value().is_associative_array());
    if is_associative {
        return expand_simple_references(shell, &subscript.text);
    }
    match &subscript.expr {
        Some(expr) => Ok(eval_expr_impl(expr, shell, depth)?.to_string()),
        None => Err(EvalError::ParseError(subscript.text.clone())),
    }
}

/// Expands `$name` and `${name}` in an associative subscript; other text is kept.
fn expand_simple_references(
    shell: &Shell<impl extensions::ShellExtensions>,
    text: &str,
) -> Result<String, EvalError> {
    let is_name_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let mut name = String::new();
        if chars.next_if_eq(&'{').is_some() {
            // `${name}`; anything else inside braces is kept literally.
            let mut literal = String::from("${");
            let mut closed = false;
            for c in chars.by_ref() {
                if c == '}' {
                    closed = true;
                    break;
                }
                literal.push(c);
                name.push(c);
            }
            if !closed || name.is_empty() || !name.chars().all(is_name_char) {
                out.push_str(&literal);
                if closed {
                    out.push('}');
                }
                continue;
            }
        } else {
            while let Some(c) = chars.next_if(|&c| is_name_char(c)) {
                name.push(c);
            }
            if name.is_empty() {
                out.push('$');
                continue;
            }
        }
        out.push_str(&get_var_value(shell, &name)?);
    }
    Ok(out)
}

fn deref_lvalue(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    lvalue: &ast::ArithmeticTarget,
    depth: u32,
) -> Result<i64, EvalError> {
    let value_str: Cow<'_, str> = match lvalue {
        ast::ArithmeticTarget::Variable(name) => get_var_value(shell, name.as_str())?,
        ast::ArithmeticTarget::ArrayElement(name, subscript) => {
            let index_str = resolve_subscript(shell, name, subscript, depth)?;

            shell
                .env()
                .get(name)
                .map_or_else(
                    || Ok(None),
                    |(_, v)| v.value().get_at(index_str.as_str(), shell),
                )
                .map_err(|_err| EvalError::FailedToAccessArray)?
                .unwrap_or(Cow::Borrowed(""))
        }
    };

    let parsed_value = cash_parser::arithmetic::parse(value_str.as_ref())
        .map_err(|_err| EvalError::ParseError(value_str.to_string()))?;

    // Literals don't need depth tracking — they can't cause recursion.
    // Only increment depth when the parsed value requires further evaluation
    // (i.e., it references other variables), matching bash's behavior.
    if matches!(parsed_value, ast::ArithmeticExpr::Literal(_)) {
        return eval_expr_impl(&parsed_value, shell, depth);
    }

    let new_depth = depth + 1;
    if new_depth > MAX_VARIABLE_DEREF_DEPTH {
        return Err(EvalError::RecursionLimitExceeded);
    }

    eval_expr_impl(&parsed_value, shell, new_depth)
}

fn apply_unary_op(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    op: ast::UnaryOperator,
    operand: &ast::ArithmeticExpr,
    depth: u32,
) -> Result<i64, EvalError> {
    let operand_eval = eval_expr_impl(operand, shell, depth)?;

    match op {
        ast::UnaryOperator::UnaryPlus => Ok(operand_eval),
        ast::UnaryOperator::UnaryMinus => Ok(operand_eval.wrapping_neg()),
        ast::UnaryOperator::BitwiseNot => Ok(!operand_eval),
        ast::UnaryOperator::LogicalNot => Ok(bool_to_i64(operand_eval == 0)),
    }
}

fn apply_binary_op(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    op: ast::BinaryOperator,
    left: &ast::ArithmeticExpr,
    right: &ast::ArithmeticExpr,
    depth: u32,
) -> Result<i64, EvalError> {
    // First, special-case short-circuiting operators. For those, we need
    // to ensure we don't eagerly evaluate both operands. After we
    // get these out of the way, we can easily just evaluate operands
    // for the other operators.
    match op {
        ast::BinaryOperator::LogicalAnd => {
            let left = eval_expr_impl(left, shell, depth)?;
            if left == 0 {
                return Ok(bool_to_i64(false));
            }

            let right = eval_expr_impl(right, shell, depth)?;
            return Ok(bool_to_i64(right != 0));
        }
        ast::BinaryOperator::LogicalOr => {
            let left = eval_expr_impl(left, shell, depth)?;
            if left != 0 {
                return Ok(bool_to_i64(true));
            }

            let right = eval_expr_impl(right, shell, depth)?;
            return Ok(bool_to_i64(right != 0));
        }
        _ => (),
    }

    // The remaining operators unconditionally operate both operands.
    let left = eval_expr_impl(left, shell, depth)?;
    let right = eval_expr_impl(right, shell, depth)?;

    #[expect(clippy::cast_possible_truncation)]
    #[expect(clippy::cast_sign_loss)]
    match op {
        ast::BinaryOperator::Power => {
            if right >= 0 {
                Ok(wrapping_pow_u64(left, right as u64))
            } else {
                Err(EvalError::NegativeExponent)
            }
        }
        ast::BinaryOperator::Multiply => Ok(left.wrapping_mul(right)),
        ast::BinaryOperator::Divide => {
            if right == 0 {
                Err(EvalError::DivideByZero)
            } else {
                Ok(left.wrapping_div(right))
            }
        }
        ast::BinaryOperator::Modulo => {
            if right == 0 {
                Err(EvalError::DivideByZero)
            } else {
                Ok(left.wrapping_rem(right))
            }
        }
        ast::BinaryOperator::Comma => Ok(right),
        ast::BinaryOperator::Add => Ok(left.wrapping_add(right)),
        ast::BinaryOperator::Subtract => Ok(left.wrapping_sub(right)),
        ast::BinaryOperator::ShiftLeft => Ok(left.wrapping_shl(right as u32)),
        ast::BinaryOperator::ShiftRight => Ok(left.wrapping_shr(right as u32)),
        ast::BinaryOperator::LessThan => Ok(bool_to_i64(left < right)),
        ast::BinaryOperator::LessThanOrEqualTo => Ok(bool_to_i64(left <= right)),
        ast::BinaryOperator::GreaterThan => Ok(bool_to_i64(left > right)),
        ast::BinaryOperator::GreaterThanOrEqualTo => Ok(bool_to_i64(left >= right)),
        ast::BinaryOperator::Equals => Ok(bool_to_i64(left == right)),
        ast::BinaryOperator::NotEquals => Ok(bool_to_i64(left != right)),
        ast::BinaryOperator::BitwiseAnd => Ok(left & right),
        ast::BinaryOperator::BitwiseXor => Ok(left ^ right),
        ast::BinaryOperator::BitwiseOr => Ok(left | right),
        ast::BinaryOperator::LogicalAnd => unreachable!("LogicalAnd covered above"),
        ast::BinaryOperator::LogicalOr => unreachable!("LogicalOr covered above"),
    }
}

fn apply_unary_assignment_op(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    lvalue: &ast::ArithmeticTarget,
    op: ast::UnaryAssignmentOperator,
    depth: u32,
) -> Result<i64, EvalError> {
    let value = deref_lvalue(shell, lvalue, depth)?;

    match op {
        ast::UnaryAssignmentOperator::PrefixIncrement => {
            let new_value = value.wrapping_add(1);
            assign(shell, lvalue, new_value, depth)?;
            Ok(new_value)
        }
        ast::UnaryAssignmentOperator::PrefixDecrement => {
            let new_value = value.wrapping_sub(1);
            assign(shell, lvalue, new_value, depth)?;
            Ok(new_value)
        }
        ast::UnaryAssignmentOperator::PostfixIncrement => {
            let new_value = value.wrapping_add(1);
            assign(shell, lvalue, new_value, depth)?;
            Ok(value)
        }
        ast::UnaryAssignmentOperator::PostfixDecrement => {
            let new_value = value.wrapping_sub(1);
            assign(shell, lvalue, new_value, depth)?;
            Ok(value)
        }
    }
}

fn assign(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    lvalue: &ast::ArithmeticTarget,
    value: i64,
    depth: u32,
) -> Result<i64, EvalError> {
    match lvalue {
        ast::ArithmeticTarget::Variable(name) => {
            shell
                .env_mut()
                .update_or_add(
                    name.as_str(),
                    variables::ShellValueLiteral::Scalar(value.to_string()),
                    |_| Ok(()),
                    env::EnvironmentLookup::Anywhere,
                    env::EnvironmentScope::Global,
                )
                .map_err(|_err| EvalError::FailedToUpdateEnvironment)?;
        }
        ast::ArithmeticTarget::ArrayElement(name, subscript) => {
            let index_str = resolve_subscript(shell, name, subscript, depth)?;

            shell
                .env_mut()
                .update_or_add_array_element(
                    name.as_str(),
                    index_str,
                    value.to_string(),
                    |_| Ok(()),
                    env::EnvironmentLookup::Anywhere,
                    env::EnvironmentScope::Global,
                )
                .map_err(|_err| EvalError::FailedToUpdateEnvironment)?;
        }
    }

    Ok(value)
}

const fn bool_to_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

// N.B. We implement our own version of wrapping_pow that takes a 64-bit exponent.
// This seems to be the best way to guarantee that we handle overflow cases
// with exponents correctly.
const fn wrapping_pow_u64(mut base: i64, mut exponent: u64) -> i64 {
    let mut result: i64 = 1;

    while exponent > 0 {
        if exponent % 2 == 1 {
            result = result.wrapping_mul(base);
        }

        base = base.wrapping_mul(base);
        exponent /= 2;
    }

    result
}

/// Evaluate an arithmetic expression string with access to the shell's environment.
pub fn evaluate_str<SE: extensions::ShellExtensions>(
    shell: &mut Shell<SE>,
    expr_str: &str,
) -> Result<i64, EvalError> {
    let expr = cash_parser::arithmetic::parse(expr_str)
        .map_err(|_e| EvalError::ParseError(expr_str.to_owned()))?;
    expr.eval(shell)
}

/// Evaluates a purely arithmetic expression string without requiring access to a shell.
///
/// Supports literals, binary operators (+, -, *, /, %, <<, >>, &, |, ^, &&, ||, ==, !=, <, >, <=, >=),
/// unary operators (+, -, ~, !), and ternary conditionals (? :).
/// Variable references evaluate to 0 if not present.
#[must_use]
pub fn eval_pure_str(expr_str: &str) -> Option<i64> {
    let expr = cash_parser::arithmetic::parse(expr_str).ok()?;
    eval_pure_expr(&expr)
}

fn eval_pure_expr(expr: &ast::ArithmeticExpr) -> Option<i64> {
    match expr {
        ast::ArithmeticExpr::Literal(l) => Some(*l),
        ast::ArithmeticExpr::Reference(_) => Some(0),
        ast::ArithmeticExpr::UnaryOp(op, operand) => {
            let val = eval_pure_expr(operand)?;
            match op {
                ast::UnaryOperator::UnaryPlus => Some(val),
                ast::UnaryOperator::UnaryMinus => Some(val.wrapping_neg()),
                ast::UnaryOperator::BitwiseNot => Some(!val),
                ast::UnaryOperator::LogicalNot => Some(bool_to_i64(val == 0)),
            }
        }
        ast::ArithmeticExpr::BinaryOp(op, left, right) => {
            let l = eval_pure_expr(left)?;
            let r = eval_pure_expr(right)?;
            match op {
                ast::BinaryOperator::Add => Some(l.wrapping_add(r)),
                ast::BinaryOperator::Subtract => Some(l.wrapping_sub(r)),
                ast::BinaryOperator::Multiply => Some(l.wrapping_mul(r)),
                ast::BinaryOperator::Divide => {
                    if r == 0 {
                        None
                    } else {
                        Some(l.wrapping_div(r))
                    }
                }
                ast::BinaryOperator::Modulo => {
                    if r == 0 {
                        None
                    } else {
                        Some(l.wrapping_rem(r))
                    }
                }
                ast::BinaryOperator::Power => {
                    let exp = u64::try_from(r).ok()?;
                    Some(wrapping_pow_u64(l, exp))
                }
                ast::BinaryOperator::ShiftLeft => {
                    let shift = u32::try_from(r).ok()?;
                    Some(l.wrapping_shl(shift))
                }
                ast::BinaryOperator::ShiftRight => {
                    let shift = u32::try_from(r).ok()?;
                    Some(l.wrapping_shr(shift))
                }
                ast::BinaryOperator::BitwiseAnd => Some(l & r),
                ast::BinaryOperator::BitwiseOr => Some(l | r),
                ast::BinaryOperator::BitwiseXor => Some(l ^ r),
                ast::BinaryOperator::LogicalAnd => Some(bool_to_i64(l != 0 && r != 0)),
                ast::BinaryOperator::LogicalOr => Some(bool_to_i64(l != 0 || r != 0)),
                ast::BinaryOperator::Equals => Some(bool_to_i64(l == r)),
                ast::BinaryOperator::NotEquals => Some(bool_to_i64(l != r)),
                ast::BinaryOperator::LessThan => Some(bool_to_i64(l < r)),
                ast::BinaryOperator::LessThanOrEqualTo => Some(bool_to_i64(l <= r)),
                ast::BinaryOperator::GreaterThan => Some(bool_to_i64(l > r)),
                ast::BinaryOperator::GreaterThanOrEqualTo => Some(bool_to_i64(l >= r)),
                ast::BinaryOperator::Comma => Some(r),
            }
        }
        ast::ArithmeticExpr::Conditional(condition, then_expr, else_expr) => {
            let c = eval_pure_expr(condition)?;
            if c != 0 {
                eval_pure_expr(then_expr)
            } else {
                eval_pure_expr(else_expr)
            }
        }
        ast::ArithmeticExpr::Assignment(_, rhs) => eval_pure_expr(rhs),
        ast::ArithmeticExpr::UnaryAssignment(op, _) => match op {
            ast::UnaryAssignmentOperator::PrefixIncrement
            | ast::UnaryAssignmentOperator::PrefixDecrement
            | ast::UnaryAssignmentOperator::PostfixIncrement
            | ast::UnaryAssignmentOperator::PostfixDecrement => Some(0),
        },
        ast::ArithmeticExpr::BinaryAssignment(_, _, operand) => eval_pure_expr(operand),
    }
}
