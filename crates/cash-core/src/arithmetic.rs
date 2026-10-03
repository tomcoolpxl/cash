//! Arithmetic evaluation

use std::borrow::Cow;

use crate::{ExecutionParameters, Shell, expansion, extensions, variables};
use cash_parser::ast;

/// Maximum recursion depth for arithmetic variable dereference chains
/// (e.g., a=b, b=c, c=a would cycle through variable dereferences).
const MAX_VARIABLE_DEREF_DEPTH: u32 = 64;

/// Represents an error that occurs during evaluation of an arithmetic expression.
#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    /// Division by zero, at the token Bash names: the divisor and what follows it.
    #[error("division by 0 (error token is \"{0}\")")]
    DivideByZero(String),

    /// Negative exponent, at the token Bash names.
    #[error("exponent less than 0 (error token is \"{0}\")")]
    NegativeExponent(String),

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

    /// A syntax error, worded as Bash words it, at the token Bash names
    /// ([`syntax_error`]).
    #[error("{0} (error token is \"{1}\")")]
    Syntax(&'static str, String),

    /// An error in the expression `.0`, which Bash names first: `1/0: division by 0 (error
    /// token is "0")`. cash said `arithmetic evaluation error: division by zero`.
    #[error("{0}: {1}")]
    InExpression(String, Box<Self>),

    /// An error in `((` or `for ((`, which Bash names as `((: 1/0: division by 0 …`.
    #[error("((: {0}")]
    InArithmeticCommand(Box<Self>),

    /// Error expanding an unset variable.
    #[error("{0}: unbound variable")]
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

    // Traced as Bash traces it, the expanded text as it is, before it is parsed: it was
    // the parsed expression printed again, `((  x = 3  ))` as `(( x = 3 ))`.
    if trace_if_needed && shell.options().print_commands_and_arguments {
        shell
            .trace_command(params, std::format!("(( {expanded_self} ))"))
            .await;
    }

    // An expression that expands to nothing is 0, as in Bash: `$(( $empty ))` was a parse
    // error that abandoned the line (LANG-10).
    if expanded_self.trim().is_empty() {
        return Ok(0);
    }

    // Now parse.
    let expr = cash_parser::arithmetic::parse(&expanded_self)
        .map_err(|e| syntax_error(&expanded_self, e.arithmetic_offset()))?;

    // Now evaluate.
    expr.eval(shell)
        .map_err(|error| error.in_expression(&expanded_self))
}

impl EvalError {
    /// This error as one in `expression`, which Bash names before it, with the token a
    /// runtime error names found in its text. An error already in an expression (a
    /// variable's value that does not parse, `x=08`) keeps that one.
    #[must_use]
    pub fn in_expression(self, expression: &str) -> Self {
        let expression = expression.trim_start();
        let error = match self {
            Self::InExpression(..) | Self::InArithmeticCommand(_) => return self,
            Self::DivideByZero(divisor) => {
                Self::DivideByZero(after_operator(expression, &["/", "%"], &divisor, false))
            }
            Self::NegativeExponent(exponent) => {
                Self::NegativeExponent(after_operator(expression, &["**"], &exponent, true))
            }
            other @ (Self::Syntax(..)
            | Self::FailedToTokenizeExpression
            | Self::FailedToAccessArray
            | Self::FailedToUpdateEnvironment
            | Self::RecursionLimitExceeded) => other,
            other @ (Self::FailedToExpandExpression(_) | Self::ExpandingUnsetVariable(_)) => {
                return other;
            }
        };
        Self::InExpression(expression.to_owned(), Box::new(error))
    }
}

/// The token Bash names for an error at the operand after one of `operators`.
///
/// From that operand to the end of `expression` for a division (`0 + 5` in `1/0 + 5`), and
/// for an exponent from the token after it, or the last token when nothing follows (`+ 3`
/// in `2**-1 + 3`, `1` in `2**-1`): Bash reads as it evaluates, and that is where its
/// reading had got to. `operand` is the operand as the tree prints it; where it is not
/// found as written, it is the token.
fn after_operator(expression: &str, operators: &[&str], operand: &str, past_it: bool) -> String {
    for (at, _) in expression.char_indices() {
        let Some(tail) = expression.get(at..) else {
            continue;
        };
        let Some(after) = operators.iter().find_map(|op| tail.strip_prefix(op)) else {
            continue;
        };
        let rest = after.trim_start();
        let Some(following) = rest.strip_prefix(operand) else {
            continue;
        };
        if !past_it {
            return rest.to_owned();
        }
        let following = following.trim_start();
        if following.is_empty() {
            let last = last_token_start(expression);
            return expression.get(last..).unwrap_or_default().to_owned();
        }
        return following.to_owned();
    }
    operand.to_owned()
}

/// A parse error in the arithmetic expression `text`, worded as Bash words it.
///
/// `offset` is where the parser stopped: `1 + : arithmetic syntax error: operand expected
/// (error token is "+ ")`, `08: value too great for base (error token is "08")`. cash said
/// `failed to parse expression: 1 + `.
pub fn syntax_error(text: &str, offset: Option<usize>) -> EvalError {
    let expression = text.trim_start();
    let blanks = text.len() - expression.len();
    let offset = offset
        .unwrap_or(text.len())
        .saturating_sub(blanks)
        .min(expression.len());

    let at_end = expression
        .get(offset..)
        .is_none_or(|rest| rest.trim().is_empty());
    let start = if at_end {
        last_token_start(expression)
    } else {
        token_start(expression, offset)
    };
    let token = expression.get(start..).unwrap_or_default();
    let word: String = token.chars().take_while(|&c| is_word_char(c)).collect();

    let (message, token) = if let Some(message) = bad_number(&word) {
        (message, word)
    } else if expression.contains('?') && !expression.contains(':') {
        ("`:' expected for conditional expression", token.to_owned())
    } else if at_end {
        (
            "arithmetic syntax error: operand expected",
            token.to_owned(),
        )
    } else {
        ("arithmetic syntax error in expression", token.to_owned())
    };
    EvalError::InExpression(
        expression.to_owned(),
        Box::new(EvalError::Syntax(message, token)),
    )
}

/// Whether `c` belongs to a name or a number in an arithmetic expression.
const fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '#' | '@')
}

/// Where the token that `offset` falls in starts.
fn token_start(expression: &str, offset: usize) -> usize {
    let at = expression
        .get(offset..)
        .and_then(|rest| rest.chars().next());
    if !at.is_some_and(is_word_char) {
        return offset;
    }
    expression
        .get(..offset)
        .unwrap_or_default()
        .char_indices()
        .rev()
        .take_while(|&(_, c)| is_word_char(c))
        .last()
        .map_or(offset, |(start, _)| start)
}

/// Where the last token of `expression` starts: a name or a number, or a run of operator
/// characters.
fn last_token_start(expression: &str) -> usize {
    let trimmed = expression.trim_end();
    let mut chars = trimmed.char_indices().rev().peekable();
    let Some(&(_, last)) = chars.peek() else {
        return 0;
    };
    let word = is_word_char(last);
    let mut start = trimmed.len();
    for (at, c) in chars {
        if c.is_whitespace() || is_word_char(c) != word {
            break;
        }
        start = at;
    }
    // `++` and `--` are one token only before a name, as Bash reads them; at the end they
    // are two, and the last is a `+` or `-` of its own.
    if matches!(trimmed.get(start..), Some("++" | "--")) {
        start += 1;
    }
    start
}

/// Bash's complaint about a number in a base it cannot be: `08`, `16#zz` (`value too
/// great for base`), `65#1` (`invalid arithmetic base`).
fn bad_number(word: &str) -> Option<&'static str> {
    if !word.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let hex = word.strip_prefix("0x").or_else(|| word.strip_prefix("0X"));
    let (base, digits) = match (word.split_once('#'), hex) {
        (Some((base, digits)), _) => {
            let base: u32 = base.parse().ok()?;
            if !(2..=64).contains(&base) {
                return Some("invalid arithmetic base");
            }
            (base, digits)
        }
        (None, Some(digits)) => (16, digits),
        (None, None) => match word.strip_prefix('0') {
            Some(digits) if !digits.is_empty() => (8, digits),
            _ => (10, word),
        },
    };
    let value_of = |c: char| match c {
        '0'..='9' => Some(u32::from(c) - u32::from('0')),
        'a'..='z' => Some(u32::from(c) - u32::from('a') + 10),
        'A'..='Z' if base <= 36 => Some(u32::from(c) - u32::from('A') + 10),
        'A'..='Z' => Some(u32::from(c) - u32::from('A') + 36),
        '@' => Some(62),
        '_' => Some(63),
        _ => None,
    };
    digits
        .chars()
        .any(|c| value_of(c).is_none_or(|value| value >= base))
        .then_some("value too great for base")
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
        .is_some_and(|(_, v)| v.is_associative(shell));
    if is_associative {
        return expand_simple_references(shell, &subscript.text);
    }
    match &subscript.expr {
        Some(expr) => Ok(eval_expr_impl(expr, shell, depth)?.to_string()),
        // A blank subscript is 0, as in Bash: `$((a[ ]))` is `a[0]`.
        None if subscript.text.trim().is_empty() => Ok("0".to_owned()),
        None => Err(syntax_error(&subscript.text, None)),
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
        // An empty subscript is said, `a[]: bad array subscript`, and is 0, as in Bash.
        ast::ArithmeticTarget::ArrayElement(name, subscript) if subscript.text.is_empty() => {
            let warning = crate::error::ErrorKind::ArrayIndexOutOfRange(format!("{name}[]")).into();
            let _ = shell.display_error(&mut shell.stderr(), &warning);
            return Ok(0);
        }
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
        .map_err(|e| syntax_error(&value_str, e.arithmetic_offset()))?;

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

    // The remaining operators unconditionally operate both operands. The right one is
    // kept as written, for the token an error names (`EvalError::in_expression`).
    let right_operand = right;
    let left = eval_expr_impl(left, shell, depth)?;
    let right = eval_expr_impl(right, shell, depth)?;

    #[expect(clippy::cast_possible_truncation)]
    #[expect(clippy::cast_sign_loss)]
    match op {
        ast::BinaryOperator::Power => {
            if right >= 0 {
                Ok(wrapping_pow_u64(left, right as u64))
            } else {
                Err(EvalError::NegativeExponent(right_operand.to_string()))
            }
        }
        ast::BinaryOperator::Multiply => Ok(left.wrapping_mul(right)),
        ast::BinaryOperator::Divide => {
            if right == 0 {
                Err(EvalError::DivideByZero(right_operand.to_string()))
            } else {
                Ok(left.wrapping_div(right))
            }
        }
        ast::BinaryOperator::Modulo => {
            if right == 0 {
                Err(EvalError::DivideByZero(right_operand.to_string()))
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
        // As any assignment: `(( RANDOM = 5 ))` seeds it.
        ast::ArithmeticTarget::Variable(name) => {
            shell
                .assign_variable(
                    name.as_str(),
                    None,
                    variables::ShellValueLiteral::Scalar(value.to_string()),
                )
                .map_err(|_err| EvalError::FailedToUpdateEnvironment)?;
        }
        ast::ArithmeticTarget::ArrayElement(name, subscript) => {
            let index_str = resolve_subscript(shell, name, subscript, depth)?;

            shell
                .assign_variable(
                    name.as_str(),
                    Some(index_str),
                    variables::ShellValueLiteral::Scalar(value.to_string()),
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
        .map_err(|e| syntax_error(expr_str, e.arithmetic_offset()))?;
    expr.eval(shell)
        .map_err(|error| error.in_expression(expr_str))
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
