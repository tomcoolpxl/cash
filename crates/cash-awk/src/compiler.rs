//
// Copyright (c) 2024-2026 Hemi Labs, Inc.
//
// This file is part of the posixutils-rs project covered under
// the MIT License.  For the full license text, please see the LICENSE
// file in the root directory of this project.
// SPDX-License-Identifier: MIT
//

#![allow(clippy::result_large_err)]

pub use crate::diagnostics::CompilerErrors;
use crate::diagnostics::{Diagnostic, Kind, syntax_error};
use crate::program::{
    Action, AwkRule, BuiltinFunction, Constant, DebugInfo, Function, OpCode, Pattern, Program,
    SourceLocation, SpecialVar, VarId,
};
use crate::regex::Regex;

use pest::Parser;
use pest::error::InputLocation;
use pest::iterators::{Pair, Pairs};
use pest::pratt_parser::{Assoc, Op, PrattParser};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;
use std::sync::LazyLock;

struct BuiltinFunctionInfo {
    function: BuiltinFunction,
    min_args: u16,
    max_args: u16,
}

static BUILTIN_FUNCTIONS: LazyLock<HashMap<Rule, BuiltinFunctionInfo>> = LazyLock::new(|| {
    HashMap::from([
        (
            Rule::atan2,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Atan2,
                min_args: 2,
                max_args: 2,
            },
        ),
        (
            Rule::cos,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Cos,
                min_args: 1,
                max_args: 1,
            },
        ),
        (
            Rule::sin,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Sin,
                min_args: 1,
                max_args: 1,
            },
        ),
        (
            Rule::exp,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Exp,
                min_args: 1,
                max_args: 1,
            },
        ),
        (
            Rule::log,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Log,
                min_args: 1,
                max_args: 1,
            },
        ),
        (
            Rule::sqrt,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Sqrt,
                min_args: 1,
                max_args: 1,
            },
        ),
        (
            Rule::int,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Int,
                min_args: 1,
                max_args: 1,
            },
        ),
        (
            Rule::rand,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Rand,
                min_args: 0,
                max_args: 0,
            },
        ),
        (
            Rule::srand,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Srand,
                min_args: 0,
                max_args: 1,
            },
        ),
        (
            Rule::gsub,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Gsub,
                min_args: 2,
                max_args: 3,
            },
        ),
        (
            Rule::index,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Index,
                min_args: 2,
                max_args: 2,
            },
        ),
        (
            Rule::length,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Length,
                min_args: 0,
                max_args: 1,
            },
        ),
        (
            Rule::match_fn,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Match,
                min_args: 2,
                // gawk's third argument, the array of the match and its groups.
                max_args: 3,
            },
        ),
        (
            Rule::split,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Split,
                min_args: 2,
                // gawk's fourth argument, the array of the separators.
                max_args: 4,
            },
        ),
        (
            Rule::sprintf,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Sprintf,
                // gawk takes `sprintf()`, and fails when it runs it.
                min_args: 0,
                max_args: u16::MAX,
            },
        ),
        (
            Rule::sub,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Sub,
                min_args: 2,
                max_args: 3,
            },
        ),
        (
            Rule::substr,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Substr,
                min_args: 2,
                max_args: 3,
            },
        ),
        (
            Rule::tolower,
            BuiltinFunctionInfo {
                function: BuiltinFunction::ToLower,
                min_args: 1,
                max_args: 1,
            },
        ),
        (
            Rule::toupper,
            BuiltinFunctionInfo {
                function: BuiltinFunction::ToUpper,
                min_args: 1,
                max_args: 1,
            },
        ),
        (
            Rule::close,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Close,
                min_args: 1,
                // gawk's `"to"` or `"from"`, the end of a two-way pipe to close.
                max_args: 2,
            },
        ),
        (
            Rule::fflush,
            BuiltinFunctionInfo {
                function: BuiltinFunction::FFlush,
                min_args: 0,
                max_args: 1,
            },
        ),
        (
            Rule::system,
            BuiltinFunctionInfo {
                function: BuiltinFunction::System,
                min_args: 1,
                max_args: 1,
            },
        ),
        (
            Rule::isarray,
            BuiltinFunctionInfo {
                function: BuiltinFunction::IsArray,
                min_args: 1,
                max_args: 1,
            },
        ),
        (
            Rule::asort,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Asort,
                min_args: 1,
                max_args: 3,
            },
        ),
        (
            Rule::asorti,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Asorti,
                min_args: 1,
                max_args: 3,
            },
        ),
        (
            Rule::gensub,
            BuiltinFunctionInfo {
                function: BuiltinFunction::Gensub,
                min_args: 3,
                max_args: 4,
            },
        ),
    ])
});

static PRATT_PARSER: LazyLock<PrattParser<Rule>> = LazyLock::new(|| {
    // Precedence is defined lowest to highest
    PrattParser::new()
        .op(Op::infix(Rule::or, Assoc::Left))
        .op(Op::infix(Rule::and, Assoc::Left))
        .op(Op::infix(Rule::in_op, Assoc::Left))
        .op(Op::infix(Rule::match_op, Assoc::Left) | Op::infix(Rule::not_match, Assoc::Left))
        .op(Op::infix(Rule::comp_op, Assoc::Left) | Op::infix(Rule::print_comp_op, Assoc::Left))
        .op(Op::infix(Rule::concat, Assoc::Left))
        .op(Op::infix(Rule::add, Assoc::Left) | Op::infix(Rule::binary_sub, Assoc::Left))
        .op(Op::infix(Rule::mul, Assoc::Left)
            | Op::infix(Rule::div, Assoc::Left)
            | Op::infix(Rule::modulus, Assoc::Left))
        .op(Op::prefix(Rule::not) | Op::prefix(Rule::negate) | Op::prefix(Rule::unary_plus))
        .op(Op::infix(Rule::pow, Assoc::Right))
        .op(Op::prefix(Rule::pre_inc) | Op::prefix(Rule::pre_dec))
        .op(Op::postfix(Rule::post_inc) | Op::postfix(Rule::post_dec))
});

#[derive(pest_derive::Parser, Default)]
#[grammar = "grammar.pest"]
struct AwkParser;

type PestError = pest::error::Error<Rule>;

fn pest_error_from_span(span: pest::Span, message: String) -> PestError {
    PestError::new_from_span(pest::error::ErrorVariant::CustomError { message }, span)
}

/// The marks at the start of an error's message that say how gawk reports it
/// (`diagnostics::Kind`): an error it goes on reading after (`awk: cmd. line:1: error:
/// division by zero attempted`), one it writes twice, a warning, the source line and a
/// caret under the place (`^ 0 is invalid as number of arguments for close`), and a fatal
/// error found once the program is read. An error without a mark is a syntax error.
const GAWK_ERROR: &str = "\0gawk error\0";
const GAWK_TWICE: &str = "\0gawk twice\0";
const GAWK_WARNING: &str = "\0gawk warning\0";
const GAWK_LATE_WARNING: &str = "\0gawk late warning\0";
const GAWK_ERROR_THEN_STOP: &str = "\0gawk error then stop\0";
const GAWK_CARET: &str = "\0gawk caret\0";
const GAWK_FATAL: &str = "\0gawk fatal\0";

/// An error at `pos` that is shown in gawk's form, `mark` saying which.
fn gawk_error(mark: &str, pos: pest::Position, message: &str) -> PestError {
    PestError::new_from_pos(
        pest::error::ErrorVariant::CustomError {
            message: format!("{mark}{message}"),
        },
        pos,
    )
}

/// The place of the token after `span`, past blanks, where gawk's parser meets it: the
/// caret of a syntax error found on reading the operator in `span`.
fn next_token(span: pest::Span) -> pest::Position {
    let input = span.get_input();
    let rest = input.get(span.end()..).unwrap_or_default();
    let blanks = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    pest::Position::new(input, span.end() + blanks).unwrap_or_else(|| span.end_pos())
}

/// `error` as gawk reports it, by its mark.
fn diagnostic(error: &PestError) -> Diagnostic {
    let offset = match error.location {
        InputLocation::Pos(offset) | InputLocation::Span((offset, _)) => offset,
    };
    let message = match &error.variant {
        pest::error::ErrorVariant::CustomError { message } => message.as_str(),
        pest::error::ErrorVariant::ParsingError { .. } => "",
    };
    for (mark, kind) in [
        (GAWK_ERROR, Kind::Error),
        (GAWK_TWICE, Kind::Twice),
        (GAWK_WARNING, Kind::Warning),
        (GAWK_LATE_WARNING, Kind::LateWarning),
        (GAWK_ERROR_THEN_STOP, Kind::ErrorThenStop),
        (GAWK_CARET, Kind::Caret),
        (GAWK_FATAL, Kind::Fatal),
    ] {
        if let Some(text) = message.strip_prefix(mark) {
            return Diagnostic::new(kind, offset, text);
        }
    }
    Diagnostic::new(Kind::Caret, offset, "syntax error")
}

/// The end of a match on a parse-tree node's rule that lists every alternative
/// `grammar.pest` allows where the node is (and, for an operator, every one the Pratt
/// parser was given): reaching it would mean the grammar and the compiler disagree, a
/// bug in cash and nothing a program can do. A target the grammar allows and the
/// compiler cannot use, as `sub(/a/, "b", length)` was, is an error the compiler reports.
fn not_in_grammar(node: &Pair<Rule>, compiling: &str) -> ! {
    unreachable!(
        "encountered {:?} while compiling {compiling}",
        node.as_rule()
    )
}

/// The opcode of a comparison operator, the child of a `comp_op` node.
fn comparison_opcode(op: &Pair<Rule>) -> OpCode {
    match op.as_rule() {
        Rule::lt => OpCode::Lt,
        Rule::le => OpCode::Le,
        Rule::gt => OpCode::Gt,
        Rule::ge => OpCode::Ge,
        Rule::eq => OpCode::Eq,
        Rule::ne => OpCode::Ne,
        _ => not_in_grammar(op, "comparison"),
    }
}

/// Where a variable is: a local of the function being compiled, or a global.
enum Variable {
    Local(VarId),
    Global(VarId),
}

/// Taking the children of a parse-tree node that its grammar rule requires.
trait GrammarChildren<'i> {
    /// The next child, one the node's rule always has.
    fn child(&mut self) -> Pair<'i, Rule>;
}

impl<'i> GrammarChildren<'i> for Pairs<'i, Rule> {
    #[expect(
        clippy::expect_used,
        reason = "callers take only the children their rule's grammar requires; optional \
                  parts are taken with `next`"
    )]
    fn child(&mut self) -> Pair<'i, Rule> {
        self.next()
            .expect("the grammar rule of a parsed node guarantees this child")
    }
}

fn first_child(pair: Pair<Rule>) -> Pair<Rule> {
    pair.into_inner().child()
}

fn distance(start: usize, end: usize) -> i32 {
    (end as i32) - (start as i32)
}

fn is_octal_digit(c: char) -> bool {
    ('0'..='7').contains(&c)
}

thread_local! {
    /// The escapes gawk has no meaning for in a string that it has warned about: once each.
    static WARNED_ESCAPES: RefCell<std::collections::HashSet<char>> = RefCell::default();
    /// gawk's warnings about strings since they were last taken (`take_escape_warnings`).
    static ESCAPE_WARNINGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// gawk's warnings about the escapes of the strings since the last call, for the caller to
/// write where the string is.
pub fn take_escape_warnings() -> Vec<String> {
    ESCAPE_WARNINGS.with_borrow_mut(std::mem::take)
}

fn escape_warning(warning: String) {
    ESCAPE_WARNINGS.with_borrow_mut(|warnings| warnings.push(warning));
}

/// The text of a string with its escapes made into what they stand for, as gawk makes
/// them, with gawk's warnings (`take_escape_warnings`): an escape it has no meaning for is
/// the character, and `\x` takes at most two hexadecimal digits.
pub fn escape_string_contents(s: &str) -> Result<Rc<str>, String> {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // A backslash at the end (`-v 'x=a\'`) stands for itself, as in gawk; it
                // was a fatal error. A string in the program cannot end in one.
                let Some(next_char) = chars.next() else {
                    result.push('\\');
                    break;
                };
                let escaped_char = match next_char {
                    '"' => '"',
                    'a' => '\x07',
                    'b' => '\x08',
                    'f' => '\x0C',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'v' => '\x0B',
                    '\\' => '\\',
                    n if is_octal_digit(n) => {
                        let mut char_code = u32::from(n) - u32::from('0');
                        for _ in 0..2 {
                            match chars.peek().and_then(|c| c.to_digit(8)) {
                                Some(digit) => {
                                    char_code = char_code * 8 + digit;
                                    chars.next();
                                }
                                None => break,
                            }
                        }
                        char::from_u32(char_code).ok_or("invalid character".to_string())?
                    }
                    'x' => {
                        let mut code = None;
                        for _ in 0..2 {
                            match chars.peek().and_then(|c| c.to_digit(16)) {
                                Some(digit) => {
                                    code = Some(code.unwrap_or(0) * 16 + digit);
                                    chars.next();
                                }
                                None => break,
                            }
                        }
                        match code {
                            Some(code) => {
                                char::from_u32(code).ok_or("invalid character".to_string())?
                            }
                            None => {
                                escape_warning("no hex digits in `\\x' escape sequence".into());
                                'x'
                            }
                        }
                    }
                    // A backslash before a newline continues the line, as in gawk.
                    '\n' => continue,
                    // An escape awk does not define stands for the character itself, as
                    // gawk has it (`"\."` is `.`, `"\&"` is `&`), with its warning, once for
                    // each character; it was a parse error, so `split(s, parts, "\.")`
                    // stopped the whole program (TXT-05).
                    other => {
                        if WARNED_ESCAPES.with_borrow_mut(|warned| warned.insert(other)) {
                            escape_warning(format!(
                                "escape sequence `\\{other}' treated as plain `{other}'"
                            ));
                        }
                        other
                    }
                };
                result.push(escaped_char);
            }
            other => result.push(other),
        }
    }
    Ok(result.into())
}

fn post_increment(val: &Cell<u32>) -> u32 {
    let result = val.get();
    val.set(result + 1);
    result
}

/// The value of a number literal, or `None` if it does not start with one.
fn parse_float(val: &str) -> Option<f64> {
    lexical::parse_partial_with_options::<f64, _, { lexical::format::C_LITERAL }>(
        val,
        &lexical::ParseFloatOptions::default(),
    )
    .ok()
    .map(|(value, _)| value)
}

/// Turn the instructions that read a variable, a field or an array element into those
/// that take a reference to it. Fails with `message` at `span` for anything else,
/// which cannot be assigned to: `sub(/a/, "b", length)` or `1 in 2`.
fn lvalue_to_scalar_ref(
    instructions: &mut [OpCode],
    span: pest::Span,
    message: &str,
) -> Result<(), PestError> {
    let reference = match instructions.last() {
        Some(OpCode::GetGlobal(id)) => OpCode::GlobalScalarRef(*id),
        Some(OpCode::GetLocal(id)) => OpCode::LocalScalarRef(*id),
        Some(OpCode::GetField) => OpCode::FieldRef,
        Some(OpCode::IndexArrayGetValue) => OpCode::IndexArrayGetRef,
        _ => return Err(pest_error_from_span(span, message.to_string())),
    };
    if let Some(last) = instructions.last_mut() {
        *last = reference;
    }
    Ok(())
}

/// Whether running `opcode` changes nothing a program can see but the stack: what may be
/// joined to a string that is appended to in place (`Compiler::appended`).
fn changes_nothing(opcode: &OpCode) -> bool {
    use BuiltinFunction as B;
    matches!(
        opcode,
        OpCode::PushConstant(_)
            | OpCode::PushZero
            | OpCode::PushOne
            | OpCode::PushUninitializedScalar
            | OpCode::GetGlobal(_)
            | OpCode::GetLocal(_)
            | OpCode::GetField
            | OpCode::IndexArrayGetValue
            | OpCode::IndexArraySubarray
            | OpCode::AsValue
            | OpCode::AsNumber
            | OpCode::Concat
            | OpCode::Add
            | OpCode::Sub
            | OpCode::Mul
            | OpCode::Div
            | OpCode::Mod
            | OpCode::Pow
            | OpCode::Negate
            | OpCode::Not
            | OpCode::Le
            | OpCode::Lt
            | OpCode::Ge
            | OpCode::Gt
            | OpCode::Eq
            | OpCode::Ne
            | OpCode::Jump(_)
            | OpCode::JumpIfFalse(_)
            | OpCode::JumpIfTrue(_)
            | OpCode::CallBuiltin {
                function: B::Length
                    | B::Substr
                    | B::Sprintf
                    | B::ToLower
                    | B::ToUpper
                    | B::Index
                    | B::Int,
                ..
            }
    )
}

/// The message for an operand that should be an lvalue and is not.
const NOT_AN_LVALUE: &str = "operand should be an lvalue";

/// Makes an argument that reads an element refer to it as a subarray, for a builtin that
/// fills it: `split(s, a[1])`.
fn refer_to_subarray(argument: &mut Instructions) {
    if let Some(last) = argument.opcodes.last_mut()
        && *last == OpCode::IndexArrayGetValue
    {
        *last = OpCode::IndexArraySubarray;
    }
}

fn normalize_builtin_function_arguments(
    function: BuiltinFunction,
    mut args: Vec<Instructions>,
    span: pest::Span,
    line_col: (usize, usize),
) -> Result<(Instructions, u16), PestError> {
    let flatten = |args: Vec<Instructions>| {
        args.into_iter()
            .fold(Instructions::default(), |mut acc, i| {
                acc.extend(i);
                acc
            })
    };
    let argc = args.len() as u16;
    let normalized = match function {
        BuiltinFunction::Length => match args.into_iter().next() {
            Some(arg) => (arg, 1),
            None => (
                Instructions::from_instructions_and_line_col(
                    vec![OpCode::PushZero, OpCode::GetField],
                    line_col,
                ),
                1,
            ),
        },
        BuiltinFunction::Split => {
            // A subarray (`split(s, a[1])`) is referred to, not read.
            refer_to_subarray(&mut args[1]);
            if let Some(seps) = args.get_mut(3) {
                refer_to_subarray(seps);
            }
            // put the array as the first argument
            args[0..2].rotate_right(1);
            (flatten(args), argc)
        }
        BuiltinFunction::Match => {
            if let Some(array) = args.get_mut(2) {
                refer_to_subarray(array);
            }
            (flatten(args), argc)
        }
        BuiltinFunction::Asort | BuiltinFunction::Asorti => {
            for array in args.iter_mut().take(2) {
                refer_to_subarray(array);
            }
            (flatten(args), argc)
        }
        BuiltinFunction::Gensub if argc == 3 => {
            // The target is `$0` by default, read and not changed.
            let mut instructions = flatten(args);
            instructions.extend(Instructions::from_instructions_and_line_col(
                vec![OpCode::PushZero, OpCode::GetField],
                line_col,
            ));
            (instructions, 4)
        }
        BuiltinFunction::Sub | BuiltinFunction::Gsub => {
            if argc == 2 {
                let mut instructions = Instructions::from_instructions_and_line_col(
                    vec![OpCode::PushZero, OpCode::FieldRef],
                    line_col,
                );
                instructions.extend(flatten(args));
                (instructions, 3)
            } else {
                args.rotate_right(1);
                // gawk's words for a target that cannot be assigned to.
                let message = if function == BuiltinFunction::Sub {
                    "sub third parameter is not a changeable object"
                } else {
                    "gsub third parameter is not a changeable object"
                };
                // A constant (gawk folds `1 + 1` into one) is replaced in a copy that
                // is then dropped, as in gawk; anything else is its error.
                let constant = args[0].opcodes.iter().all(|opcode| {
                    matches!(
                        opcode,
                        OpCode::PushConstant(_)
                            | OpCode::Add
                            | OpCode::Sub
                            | OpCode::Mul
                            | OpCode::Div
                            | OpCode::Mod
                            | OpCode::Pow
                            | OpCode::Negate
                            | OpCode::Not
                            | OpCode::AsNumber
                    )
                });
                if !constant && lvalue_to_scalar_ref(&mut args[0].opcodes, span, message).is_err() {
                    let close = pest::Position::new(span.get_input(), span.end() - 1)
                        .unwrap_or_else(|| span.start_pos());
                    return Err(gawk_error(GAWK_CARET, close, message));
                }
                (flatten(args), 3)
            }
        }
        _ => (flatten(args), argc),
    };
    Ok(normalized)
}

#[cfg_attr(test, derive(Debug))]
#[derive(PartialEq, Eq)]
enum ExprKind {
    LValue,
    Number,
    String,
    Regex,
    Comp,
}

struct Expr {
    kind: ExprKind,
    instructions: Instructions,
    /// The value of a numeric constant, as gawk folds one at parse time: a number, or
    /// `-`, `!` or `^` of constants (`!` of a string too), not in parentheses. A divisor
    /// that is a zero constant is an error before the program runs.
    constant: Option<f64>,
    /// A plain variable's name as gawk gives it in an error about an array, for the right
    /// side of `in`.
    array_name: Option<Rc<str>>,
}

impl Expr {
    fn new(kind: ExprKind, instructions: Instructions) -> Self {
        Expr {
            kind,
            instructions,
            constant: None,
            array_name: None,
        }
    }

    fn with_constant(self, constant: Option<f64>) -> Self {
        Self { constant, ..self }
    }
}

/// How gawk names the array `name` in an error: `` `a' ``, or `` parameter `p' `` for a
/// function's.
fn array_description(name: &str, locals: &LocalMap) -> Rc<str> {
    if locals.contains_key(name) {
        format!("parameter `{name}'").into()
    } else {
        format!("`{name}'").into()
    }
}

#[derive(Clone, Copy)]
pub enum GlobalName {
    Variable(VarId),
    SpecialVar(VarId),
    Function { id: u32, parameter_count: u32 },
}

type NameMap = HashMap<String, GlobalName>;
type LocalMap = HashMap<String, VarId>;

#[cfg_attr(test, derive(Debug))]
#[derive(Default)]
struct Instructions {
    opcodes: Vec<OpCode>,
    source_locations: Vec<SourceLocation>,
    /// The instructions that use an array, by index, with the array's name for an error
    /// (`DebugInfo::array_names`).
    array_names: Vec<(usize, Rc<str>)>,
}

impl Instructions {
    fn from_instructions_and_line_col(instructions: Vec<OpCode>, line_col: (usize, usize)) -> Self {
        let source_locations = vec![
            SourceLocation {
                line: line_col.0 as u32,
                column: line_col.1 as u32,
            };
            instructions.len()
        ];
        Instructions {
            opcodes: instructions,
            source_locations,
            array_names: Vec::new(),
        }
    }

    fn push(&mut self, instruction: OpCode, line_col: (usize, usize)) {
        self.opcodes.push(instruction);
        self.source_locations.push(SourceLocation {
            line: line_col.0 as u32,
            column: line_col.1 as u32,
        });
    }

    /// Push an instruction that uses the array `name` names, local or global, for an
    /// error that names it as gawk does.
    fn push_using_array(
        &mut self,
        instruction: OpCode,
        line_col: (usize, usize),
        name: &str,
        locals: &LocalMap,
    ) {
        self.push_named(instruction, line_col, array_description(name, locals));
    }

    /// Push an instruction that uses the array gawk names `description` in an error.
    fn push_named(&mut self, instruction: OpCode, line_col: (usize, usize), description: Rc<str>) {
        self.array_names.push((self.opcodes.len(), description));
        self.push(instruction, line_col);
    }

    fn extend(&mut self, instructions: Instructions) {
        let offset = self.opcodes.len();
        self.opcodes.extend(instructions.opcodes);
        self.source_locations.extend(instructions.source_locations);
        self.array_names.extend(
            instructions
                .array_names
                .into_iter()
                .map(|(index, name)| (index + offset, name)),
        );
    }

    /// The opcodes and their debug information.
    fn into_parts(self, file: Rc<str>) -> (Vec<OpCode>, DebugInfo) {
        let mut array_names = vec![None; self.opcodes.len()];
        for (index, name) in self.array_names {
            if let Some(slot) = array_names.get_mut(index) {
                *slot = Some(name);
            }
        }
        (
            self.opcodes,
            DebugInfo {
                source_locations: self.source_locations,
                file,
                array_names,
            },
        )
    }

    fn into_action(self, file: Rc<str>) -> Action {
        let (instructions, debug_info) = self.into_parts(file);
        Action {
            instructions,
            debug_info,
        }
    }

    /// The number of opcodes. Each has its source location: the two grow together
    /// (`push`, `extend`), and an opcode is otherwise only changed in place.
    fn len(&self) -> usize {
        self.opcodes.len()
    }
}

#[derive(Default)]
struct LoopStubs {
    break_stubs: Vec<usize>,
    continue_stubs: Vec<usize>,
}

struct Compiler {
    constants: RefCell<Vec<Constant>>,
    names: RefCell<NameMap>,
    last_global_var_id: Cell<u32>,
    last_global_function_id: Cell<u32>,
    in_function: bool,
    /// `"BEGIN"` or `"END"` while compiling one of those actions, where `next` and
    /// `nextfile` are errors.
    special_action: Option<&'static str>,
    loop_stack: Vec<LoopStubs>,
    /// Errors that do not stop the compiling, as gawk goes on parsing after them: a
    /// division by a zero constant. They are reported with the others.
    deferred_errors: RefCell<Vec<PestError>>,
}

impl Default for Compiler {
    fn default() -> Self {
        let default_globals = HashMap::from([
            (
                "ARGC".to_string(),
                GlobalName::SpecialVar(SpecialVar::Argc as u32),
            ),
            (
                "ARGV".to_string(),
                GlobalName::SpecialVar(SpecialVar::Argv as u32),
            ),
            (
                "CONVFMT".to_string(),
                GlobalName::SpecialVar(SpecialVar::Convfmt as u32),
            ),
            (
                "ENVIRON".to_string(),
                GlobalName::SpecialVar(SpecialVar::Environ as u32),
            ),
            (
                "FILENAME".to_string(),
                GlobalName::SpecialVar(SpecialVar::Filename as u32),
            ),
            (
                "FNR".to_string(),
                GlobalName::SpecialVar(SpecialVar::Fnr as u32),
            ),
            (
                "FS".to_string(),
                GlobalName::SpecialVar(SpecialVar::Fs as u32),
            ),
            (
                "NF".to_string(),
                GlobalName::SpecialVar(SpecialVar::Nf as u32),
            ),
            (
                "NR".to_string(),
                GlobalName::SpecialVar(SpecialVar::Nr as u32),
            ),
            (
                "OFMT".to_string(),
                GlobalName::SpecialVar(SpecialVar::Ofmt as u32),
            ),
            (
                "OFS".to_string(),
                GlobalName::SpecialVar(SpecialVar::Ofs as u32),
            ),
            (
                "ORS".to_string(),
                GlobalName::SpecialVar(SpecialVar::Ors as u32),
            ),
            (
                "RLENGTH".to_string(),
                GlobalName::SpecialVar(SpecialVar::Rlength as u32),
            ),
            (
                "RS".to_string(),
                GlobalName::SpecialVar(SpecialVar::Rs as u32),
            ),
            (
                "RSTART".to_string(),
                GlobalName::SpecialVar(SpecialVar::Rstart as u32),
            ),
            (
                "SUBSEP".to_string(),
                GlobalName::SpecialVar(SpecialVar::Subsep as u32),
            ),
            (
                "IGNORECASE".to_string(),
                GlobalName::SpecialVar(SpecialVar::IgnoreCase as u32),
            ),
        ]);
        Compiler {
            constants: RefCell::new(Vec::new()),
            names: RefCell::new(default_globals),
            last_global_var_id: Cell::new(SpecialVar::Count as u32),
            last_global_function_id: Cell::new(0),
            loop_stack: Vec::new(),
            in_function: false,
            special_action: None,
            deferred_errors: RefCell::default(),
        }
    }
}

impl Compiler {
    fn push_constant(&self, constant: Constant) -> u32 {
        let index = self.constants.borrow().len() as u32;
        self.constants.borrow_mut().push(constant);
        index
    }

    fn get_var(&self, name: &str, locals: &LocalMap) -> Result<OpCode, String> {
        Ok(match self.variable(name, locals)? {
            Variable::Local(id) => OpCode::GetLocal(id),
            Variable::Global(id) => OpCode::GetGlobal(id),
        })
    }

    /// The variable `name` is, a global made for it if it is new.
    fn variable(&self, name: &str, locals: &LocalMap) -> Result<Variable, String> {
        if let Some(local_id) = locals.get(name) {
            Ok(Variable::Local(*local_id))
        } else {
            let entry = self.names.borrow().get(name).copied();
            if let Some(var) = entry {
                match var {
                    GlobalName::Variable(id) => Ok(Variable::Global(id)),
                    GlobalName::SpecialVar(id) => Ok(Variable::Global(id)),
                    GlobalName::Function { .. } => Err(format!(
                        "{GAWK_ERROR}function `{name}' called with space between name and \
                             `(',\nor used as a variable or an array"
                    )),
                }
            } else {
                let id = post_increment(&self.last_global_var_id);
                self.names
                    .borrow_mut()
                    .insert(name.to_string(), GlobalName::Variable(id));
                Ok(Variable::Global(id))
            }
        }
    }

    fn compile_function_args(
        &self,
        args: Pairs<Rule>,
        instructions: &mut Instructions,
        call_span: pest::Span,
        locals: &LocalMap,
    ) -> Result<u16, PestError> {
        let mut argc: u32 = 0;
        for arg in args {
            let mut arg_instructions = Instructions::default();
            self.compile_expr(arg, &mut arg_instructions, locals)?;
            // An element may be a subarray, or become one in the function.
            if let Some(last) = arg_instructions.opcodes.last_mut()
                && *last == OpCode::IndexArrayGetValue
            {
                *last = OpCode::IndexArrayGetArgument;
            }
            instructions.extend(arg_instructions);
            argc += 1;
            if argc > u16::MAX as u32 {
                return Err(pest_error_from_span(
                    call_span,
                    "function call with too many arguments".to_string(),
                ));
            }
        }
        Ok(argc as u16)
    }

    fn map_primary(&self, primary: Pair<Rule>, locals: &LocalMap) -> Result<Expr, PestError> {
        match primary.as_rule() {
            // An assignment after a comparison, a match, `&&` or `||`: `a || b = 1`.
            Rule::assigned | Rule::print_assigned => {
                let mut instructions = Instructions::default();
                self.compile_expr(primary, &mut instructions, locals)?;
                Ok(Expr::new(ExprKind::Number, instructions))
            }
            Rule::expr => {
                let line_col = primary.line_col();
                let mut instructions = Instructions::default();
                self.compile_expr(primary, &mut instructions, locals)?;
                // A variable in parentheses is its value, as in gawk: no lvalue for `sub`,
                // no array for `split`, `length` or a function; it was the variable.
                if matches!(
                    instructions.opcodes.last(),
                    Some(
                        OpCode::GetGlobal(_)
                            | OpCode::GetLocal(_)
                            | OpCode::GetField
                            | OpCode::IndexArrayGetValue
                    )
                ) {
                    instructions.push(OpCode::AsValue, line_col);
                }
                Ok(Expr::new(ExprKind::Number, instructions))
            }
            Rule::ere => {
                // One slash off each end: `/a\//` ends in an escaped one.
                let text = primary.as_str();
                let text = text.strip_prefix('/').unwrap_or(text);
                let text = text.strip_suffix('/').unwrap_or(text);
                let text = join_continued_lines(text);
                let ere = translate_ere_escapes(&text);
                // What is wrong with the regex is gawk's error, after which it goes on
                // parsing; an empty regex stands in for it meanwhile.
                let made = Regex::new(&ere);
                // gawk's warnings, for an escape it has no meaning for.
                for warning in crate::regex::take_warnings() {
                    self.deferred_errors.borrow_mut().push(gawk_error(
                        GAWK_WARNING,
                        primary.as_span().start_pos(),
                        &warning,
                    ));
                }
                let regex = match made {
                    Ok(regex) => regex,
                    Err(error) => {
                        self.deferred_errors.borrow_mut().push(gawk_error(
                            GAWK_ERROR,
                            primary.as_span().start_pos(),
                            &format!("{error}: /{text}/"),
                        ));
                        Regex::new("").map_err(|e| pest_error_from_span(primary.as_span(), e))?
                    }
                };
                let index = self.push_constant(Constant::Regex(Rc::new(regex)));
                Ok(Expr::new(
                    ExprKind::Regex,
                    Instructions::from_instructions_and_line_col(
                        vec![OpCode::PushConstant(index)],
                        primary.line_col(),
                    ),
                ))
            }
            Rule::number => {
                let num = parse_float(primary.as_str()).ok_or_else(|| {
                    pest_error_from_span(primary.as_span(), "invalid number".to_string())
                })?;
                let index = self.push_constant(Constant::Number(num));
                Ok(Expr::new(
                    ExprKind::Number,
                    Instructions::from_instructions_and_line_col(
                        vec![OpCode::PushConstant(index)],
                        primary.line_col(),
                    ),
                )
                .with_constant(Some(num)))
            }
            Rule::string => {
                let span = primary.as_span();
                let string_line_col = primary.line_col();
                let str = escape_string_contents(first_child(primary).as_str())
                    .map_err(|e| pest_error_from_span(span, e))?;
                for warning in take_escape_warnings() {
                    self.deferred_errors.borrow_mut().push(gawk_error(
                        GAWK_WARNING,
                        span.start_pos(),
                        &warning,
                    ));
                }
                let index = self.push_constant(Constant::String(str));
                Ok(Expr::new(
                    ExprKind::String,
                    Instructions::from_instructions_and_line_col(
                        vec![OpCode::PushConstant(index)],
                        string_line_col,
                    ),
                ))
            }
            Rule::simple_getline => {
                let mut instructions = Instructions::default();
                self.compile_simple_getline(primary, &mut instructions, locals)?;
                Ok(Expr::new(ExprKind::Number, instructions))
            }
            Rule::lvalue => {
                let variable = first_child(primary.clone());
                let array_name = (variable.as_rule() == Rule::name)
                    .then(|| array_description(variable.as_str(), locals));
                let mut instructions = Instructions::default();
                self.compile_lvalue(primary, &mut instructions, locals)?;
                Ok(Expr {
                    array_name,
                    ..Expr::new(ExprKind::LValue, instructions)
                })
            }
            Rule::function_call => {
                let span = primary.as_span();
                let line_col = primary.line_col();
                let mut inner = primary.into_inner();
                let name = inner.child().as_str();
                let mut instructions = Instructions::default();
                let argc = self.compile_function_args(inner, &mut instructions, span, locals)?;
                match self.names.borrow().get(name) {
                    Some(GlobalName::Function {
                        id,
                        parameter_count,
                    }) => {
                        // I think this is a better way to structure the code
                        #[allow(clippy::comparison_chain)]
                        if argc > *parameter_count as u16 {
                            // gawk's warning, and the arguments the function has no
                            // parameters for are evaluated and dropped; it was an error.
                            self.deferred_errors.borrow_mut().push(gawk_error(
                                GAWK_LATE_WARNING,
                                span.start_pos(),
                                &format!(
                                    "function `{name}' called with more arguments than declared"
                                ),
                            ));
                            for _ in *parameter_count as u16..argc {
                                instructions.push(OpCode::Pop, line_col);
                            }
                        } else if argc < *parameter_count as u16 {
                            for _ in argc..*parameter_count as u16 {
                                instructions.push(OpCode::PushUninitialized, line_col);
                            }
                        }
                        instructions.push(OpCode::Call(*id), line_col);
                    }
                    // gawk's error, after which it goes on; the arguments are dropped and
                    // nothing called, as the program will not run.
                    Some(_) => {
                        self.deferred_errors.borrow_mut().push(gawk_error(
                            GAWK_ERROR,
                            span.start_pos(),
                            &format!("attempt to use non-function `{name}' in function call"),
                        ));
                        for _ in 0..argc {
                            instructions.push(OpCode::Pop, line_col);
                        }
                        instructions.push(OpCode::PushUninitializedScalar, line_col);
                    }
                    // gawk finds it once the whole program is read.
                    None => {
                        return Err(gawk_error(
                            GAWK_FATAL,
                            span.start_pos(),
                            &format!("function `{name}' not defined"),
                        ));
                    }
                }
                Ok(Expr::new(ExprKind::Number, instructions))
            }
            Rule::builtin_function_call => {
                let span = primary.as_span();
                let line_col = primary.line_col();
                let mut inner = primary.into_inner();
                let function = inner.child();
                let mut args = Vec::new();
                for arg in inner {
                    let mut arg_instructions = Instructions::default();
                    self.compile_expr(arg, &mut arg_instructions, locals)?;
                    args.push(arg_instructions);
                    if args.len() > u16::MAX as usize {
                        return Err(pest_error_from_span(
                            span,
                            "function call with too many arguments".to_string(),
                        ));
                    }
                }
                let argc = args.len() as u16;
                let fn_info = BUILTIN_FUNCTIONS.get(&function.as_rule()).ok_or_else(|| {
                    pest_error_from_span(
                        span,
                        format!("unknown builtin function '{}'", function.as_str()),
                    )
                })?;
                if (fn_info.min_args..=fn_info.max_args).contains(&argc) {
                    let (mut instructions, argc) = normalize_builtin_function_arguments(
                        fn_info.function,
                        args,
                        span,
                        line_col,
                    )?;
                    instructions.push(
                        OpCode::CallBuiltin {
                            function: fn_info.function,
                            argc,
                        },
                        line_col,
                    );
                    Ok(Expr::new(ExprKind::Number, instructions))
                } else if !span.as_str().trim_end().ends_with(')') {
                    // Only `length` goes without parentheses; gawk's caret is under what
                    // follows the name.
                    Err(gawk_error(GAWK_CARET, span.end_pos(), "syntax error"))
                } else {
                    // gawk's words, with its caret under the call's closing parenthesis.
                    let close = pest::Position::new(span.get_input(), span.end() - 1)
                        .unwrap_or_else(|| span.start_pos());
                    Err(gawk_error(
                        GAWK_CARET,
                        close,
                        &format!(
                            "{argc} is invalid as number of arguments for {}",
                            function.as_str().trim_end()
                        ),
                    ))
                }
            }
            _ => not_in_grammar(&primary, "primary"),
        }
    }

    /// The text of a string constant that `instructions` push and do nothing else with.
    fn string_constant(&self, instructions: &Instructions) -> Option<Rc<str>> {
        let [OpCode::PushConstant(index)] = instructions.opcodes.as_slice() else {
            return None;
        };
        match self.constants.borrow().get(*index as usize) {
            Some(Constant::String(text)) => Some(text.clone()),
            _ => None,
        }
    }

    fn map_prefix(&self, op: Pair<Rule>, rhs: Expr) -> Result<Expr, PestError> {
        let kind = rhs.kind;
        let constant = rhs.constant;
        let mut instructions = rhs.instructions;
        match op.as_rule() {
            Rule::negate => {
                instructions.push(OpCode::Negate, op.line_col());
                Ok(Expr::new(ExprKind::Number, instructions).with_constant(constant.map(|n| -n)))
            }
            Rule::not => {
                let constant = constant
                    .map(|n| n == 0.0)
                    .or_else(|| self.string_constant(&instructions).map(|s| s.is_empty()))
                    .map(|truth| if truth { 1.0 } else { 0.0 });
                instructions.push(OpCode::Not, op.line_col());
                Ok(Expr::new(ExprKind::Number, instructions).with_constant(constant))
            }
            Rule::unary_plus => {
                instructions.push(OpCode::AsNumber, op.line_col());
                Ok(Expr::new(ExprKind::Number, instructions))
            }
            Rule::pre_inc | Rule::pre_dec => {
                // gawk's syntax error, under the operand.
                if kind != ExprKind::LValue {
                    return Err(gawk_error(
                        GAWK_CARET,
                        next_token(op.as_span()),
                        "syntax error",
                    ));
                }
                lvalue_to_scalar_ref(&mut instructions.opcodes, op.as_span(), NOT_AN_LVALUE)?;
                if op.as_rule() == Rule::pre_inc {
                    instructions.push(OpCode::PreInc, op.line_col());
                } else {
                    instructions.push(OpCode::PreDec, op.line_col());
                }
                Ok(Expr::new(ExprKind::Number, instructions))
            }
            _ => not_in_grammar(&op, "prefix operator"),
        }
    }

    fn map_postfix(&self, lhs: Expr, op: Pair<Rule>) -> Result<Expr, PestError> {
        // `post_inc` and `post_dec` are the grammar's only postfix operators.
        let opcode = if op.as_rule() == Rule::post_inc {
            OpCode::PostInc
        } else {
            OpCode::PostDec
        };
        let kind = lhs.kind;
        let mut instructions = lhs.instructions;
        // gawk's syntax error, under what follows the operator.
        if kind != ExprKind::LValue {
            return Err(gawk_error(
                GAWK_CARET,
                next_token(op.as_span()),
                "syntax error",
            ));
        }
        lvalue_to_scalar_ref(&mut instructions.opcodes, op.as_span(), NOT_AN_LVALUE)?;
        instructions.push(opcode, op.line_col());
        Ok(Expr::new(ExprKind::Number, instructions))
    }

    fn map_infix(&self, lhs: Expr, op: Pair<Rule>, rhs: Expr) -> Result<Expr, PestError> {
        let lhs_kind = lhs.kind;
        let rhs_kind = rhs.kind;
        let lhs_constant = lhs.constant;
        let rhs_constant = rhs.constant;
        let mut instructions = lhs.instructions;
        // gawk divides by a constant at parse time, and a zero one is an error it goes on
        // parsing after, where a zero that is computed is a fatal error when it is met.
        if matches!(op.as_rule(), Rule::div | Rule::modulus) && rhs_constant == Some(0.0) {
            let message = if op.as_rule() == Rule::div {
                "division by zero attempted"
            } else {
                "division by zero attempted in `%'"
            };
            self.deferred_errors.borrow_mut().push(gawk_error(
                GAWK_ERROR,
                op.as_span().start_pos(),
                message,
            ));
        }

        match op.as_rule() {
            Rule::and => {
                instructions.push(
                    OpCode::JumpIfFalse(rhs.instructions.len() as i32 + 2),
                    op.line_col(),
                );
                instructions.extend(rhs.instructions);
                instructions.push(OpCode::Jump(2), op.line_col());
                instructions.push(OpCode::PushZero, op.line_col());
                return Ok(Expr::new(ExprKind::Number, instructions));
            }
            Rule::or => {
                instructions.push(
                    OpCode::JumpIfTrue(rhs.instructions.len() as i32 + 2),
                    op.line_col(),
                );
                instructions.extend(rhs.instructions);
                instructions.push(OpCode::Jump(2), op.line_col());
                instructions.push(OpCode::PushOne, op.line_col());
                return Ok(Expr::new(ExprKind::Number, instructions));
            }
            Rule::in_op => {
                let lhs_instructions = instructions;
                let mut instructions = rhs.instructions;
                // A variable is taken as it is, which leaves a parameter linked to its
                // caller's variable; a subarray (`k in a[1]`) is referred to, not read.
                match instructions.opcodes.last_mut() {
                    Some(OpCode::GetGlobal(_) | OpCode::GetLocal(_)) => {}
                    Some(last @ OpCode::IndexArrayGetValue) => *last = OpCode::IndexArraySubarray,
                    // gawk's syntax error, under the right side.
                    _ => {
                        return Err(gawk_error(
                            GAWK_CARET,
                            next_token(op.as_span()),
                            "syntax error",
                        ));
                    }
                }
                instructions.extend(lhs_instructions);
                match rhs.array_name {
                    Some(name) => instructions.push_named(OpCode::In, op.line_col(), name),
                    None => instructions.push(OpCode::In, op.line_col()),
                }
                return Ok(Expr::new(ExprKind::Number, instructions));
            }
            _ => {}
        }

        instructions.extend(rhs.instructions);
        match op.as_rule() {
            Rule::add => {
                instructions.push(OpCode::Add, op.line_col());
                Ok(Expr::new(ExprKind::Number, instructions))
            }
            Rule::binary_sub => {
                instructions.push(OpCode::Sub, op.line_col());
                Ok(Expr::new(ExprKind::Number, instructions))
            }
            Rule::mul => {
                instructions.push(OpCode::Mul, op.line_col());
                Ok(Expr::new(ExprKind::Number, instructions))
            }
            Rule::div => {
                instructions.push(OpCode::Div, op.line_col());
                Ok(Expr::new(ExprKind::Number, instructions))
            }
            Rule::modulus => {
                instructions.push(OpCode::Mod, op.line_col());
                Ok(Expr::new(ExprKind::Number, instructions))
            }
            Rule::pow => {
                instructions.push(OpCode::Pow, op.line_col());
                let constant = lhs_constant.zip(rhs_constant).map(|(a, b)| a.powf(b));
                Ok(Expr::new(ExprKind::Number, instructions).with_constant(constant))
            }
            Rule::le => {
                instructions.push(OpCode::Le, op.line_col());
                Ok(Expr::new(ExprKind::Number, instructions))
            }
            Rule::comp_op | Rule::print_comp_op => {
                // gawk's syntax error, under the second comparison.
                if lhs_kind == ExprKind::Comp || rhs_kind == ExprKind::Comp {
                    return Err(gawk_error(
                        GAWK_CARET,
                        op.as_span().start_pos(),
                        "syntax error",
                    ));
                }
                let op = first_child(op);
                instructions.push(comparison_opcode(&op), op.line_col());
                Ok(Expr::new(ExprKind::Comp, instructions))
            }
            Rule::match_op => {
                instructions.push(OpCode::Match, op.line_col());
                Ok(Expr::new(ExprKind::Number, instructions))
            }
            Rule::not_match => {
                instructions.push(OpCode::Match, op.line_col());
                instructions.push(OpCode::Not, op.line_col());
                Ok(Expr::new(ExprKind::Number, instructions))
            }
            Rule::concat => {
                instructions.push(OpCode::Concat, op.line_col());
                Ok(Expr::new(ExprKind::String, instructions))
            }
            _ => not_in_grammar(&op, "infix operator"),
        }
    }

    fn compile_simple_binary_expr<'i>(
        &self,
        expr: impl Iterator<Item = Pair<'i, Rule>>,
        locals: &LocalMap,
    ) -> Result<Expr, PestError> {
        PRATT_PARSER
            .map_primary(|primary| self.map_primary(primary, locals))
            .map_prefix(|op, rhs| self.map_prefix(op, rhs?))
            .map_postfix(|lhs, op| self.map_postfix(lhs?, op))
            .map_infix(|lhs, op, rhs| self.map_infix(lhs?, op, rhs?))
            .parse(expr)
    }

    /// Compile an lvalue into the instructions that take a reference to it.
    fn compile_lvalue_ref(
        &self,
        lvalue: Pair<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        let span = lvalue.as_span();
        self.compile_lvalue(lvalue, instructions, locals)?;
        lvalue_to_scalar_ref(&mut instructions.opcodes, span, NOT_AN_LVALUE)
    }

    /// The instructions of what `target = target a b ...` joins to `target`, when the
    /// assignment can append to it in place (`OpCode::AppendAssign`): `target` is a
    /// variable that is no special one, or an element of an array, and the value is the
    /// same text joined to operands whose evaluation changes nothing, so that appending
    /// after it is what assigning the whole would be.
    fn appended(
        &self,
        target: &Pair<Rule>,
        value: &Pair<Rule>,
        locals: &LocalMap,
    ) -> Result<Option<(Instructions, Instructions)>, PestError> {
        let variable = first_child(target.clone());
        let plain = match variable.as_rule() {
            Rule::name => {
                locals.contains_key(variable.as_str())
                    || !matches!(
                        self.names.borrow().get(variable.as_str()),
                        Some(GlobalName::SpecialVar(_))
                    )
            }
            Rule::array_element => true,
            _ => false,
        };
        let binary = first_child(value.clone());
        if !plain || binary.as_rule() != Rule::binary_expr {
            return Ok(None);
        }
        let joined = first_child(binary);
        if joined.as_rule() != Rule::simple_binary_expr {
            return Ok(None);
        }
        let parts: Vec<Pair<Rule>> = joined.into_inner().collect();
        let [first, join, ..] = parts.as_slice() else {
            return Ok(None);
        };
        // Only arithmetic binds tighter than the joining, so a comparison, a match, `in`,
        // `&&` or `||` would take the joined string as its operand.
        let lower = parts.iter().any(|part| {
            matches!(
                part.as_rule(),
                Rule::comp_op
                    | Rule::print_comp_op
                    | Rule::match_op
                    | Rule::not_match
                    | Rule::in_op
                    | Rule::and
                    | Rule::or
            )
        });
        if lower
            || first.as_rule() != Rule::lvalue
            || first.as_str() != target.as_str()
            || join.as_rule() != Rule::concat
        {
            return Ok(None);
        }
        // What is compiled here is used, or compiled again as a whole, with its errors.
        let errors_before = self.deferred_errors.borrow().len();
        let mut reference = Instructions::default();
        self.compile_lvalue_ref(target.clone(), &mut reference, locals)?;
        let (_, key) = reference
            .opcodes
            .split_last()
            .unwrap_or((&OpCode::Invalid, &[]));
        let key_changes_nothing = key.iter().all(changes_nothing);
        let tail = self.compile_simple_binary_expr(parts.into_iter().skip(2), locals)?;
        if key_changes_nothing && tail.instructions.opcodes.iter().all(changes_nothing) {
            Ok(Some((reference, tail.instructions)))
        } else {
            self.deferred_errors.borrow_mut().truncate(errors_before);
            Ok(None)
        }
    }

    fn compile_lvalue(
        &self,
        lvalue: Pair<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        let lvalue = first_child(lvalue);
        let line_col = lvalue.line_col();
        match lvalue.as_rule() {
            Rule::name => {
                let get_instruction = self
                    .get_var(lvalue.as_str(), locals)
                    .map_err(|msg| pest_error_from_span(lvalue.as_span(), msg))?;
                instructions.push(get_instruction, line_col);
            }
            Rule::array_element => {
                self.compile_subscripted(
                    lvalue.into_inner(),
                    OpCode::IndexArrayGetValue,
                    line_col,
                    instructions,
                    locals,
                )?;
            }
            Rule::field_var => {
                let expr = self.compile_field_var_expr(lvalue, locals)?;
                instructions.extend(expr.instructions);
                instructions.push(OpCode::GetField, line_col);
            }
            _ => not_in_grammar(&lvalue, "lvalue"),
        }
        Ok(())
    }

    fn compile_field_var_expr(
        &self,
        field_var: Pair<Rule>,
        locals: &LocalMap,
    ) -> Result<Expr, PestError> {
        let mut prefix_ops = Vec::new();
        let mut primary_pair = None;
        let mut postfix_ops = Vec::new();
        let span = field_var.as_span();

        for child in field_var.into_inner() {
            match child.as_rule() {
                Rule::pre_inc | Rule::pre_dec | Rule::not | Rule::unary_plus | Rule::negate => {
                    if primary_pair.is_none() {
                        prefix_ops.push(child);
                    }
                }
                Rule::post_inc | Rule::post_dec => {
                    postfix_ops.push(child);
                }
                _ => {
                    primary_pair = Some(child);
                }
            }
        }

        let primary = primary_pair.ok_or_else(|| {
            pest_error_from_span(span, "expected an expression after `$`".to_string())
        })?;
        let mut expr = self.map_primary(primary, locals)?;

        // Apply postfix ops first (they bind tighter to the primary)
        for op in postfix_ops {
            expr = self.map_postfix(expr, op)?;
        }

        // Apply prefix ops in reverse order (innermost first)
        for op in prefix_ops.into_iter().rev() {
            expr = self.map_prefix(op, expr)?;
        }

        Ok(expr)
    }

    /// Compile a name and its subscripts (`array_element`, `array_ref`): the variable,
    /// each subarray on the way, and `last` with the last subscript; with none, the
    /// variable alone. The first use of the variable carries its name for an error; a
    /// subarray's error names it from what the interpreter finds.
    fn compile_subscripted(
        &self,
        mut inner: Pairs<Rule>,
        last: OpCode,
        line_col: (usize, usize),
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        let name = inner.child();
        let get_instruction = self
            .get_var(name.as_str(), locals)
            .map_err(|msg| pest_error_from_span(name.as_span(), msg))?;
        instructions.push(get_instruction, line_col);
        let subscripts: Vec<Pair<Rule>> = inner.collect();
        let count = subscripts.len();
        if count == 0 {
            return Ok(());
        }
        for (index, subscript) in subscripts.into_iter().enumerate() {
            self.compile_array_index(subscript.into_inner(), instructions, locals)?;
            let opcode = if index + 1 == count {
                last
            } else {
                OpCode::IndexArraySubarray
            };
            if index == 0 {
                instructions.push_using_array(opcode, line_col, name.as_str(), locals);
            } else {
                instructions.push(opcode, line_col);
            }
        }
        Ok(())
    }

    fn compile_array_index(
        &self,
        mut index: Pairs<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        let first_index = index.child();
        self.compile_expr(first_index, instructions, locals)?;
        if let Some(second_index) = index.next() {
            let second_index_line_col = second_index.line_col();
            instructions.push(
                OpCode::GetGlobal(SpecialVar::Subsep as u32),
                second_index.line_col(),
            );
            instructions.push(OpCode::Concat, second_index.line_col());
            self.compile_expr(second_index, instructions, locals)?;
            for other_index in index {
                instructions.push(OpCode::Concat, other_index.line_col());
                instructions.push(
                    OpCode::GetGlobal(SpecialVar::Subsep as u32),
                    other_index.line_col(),
                );
                instructions.push(OpCode::Concat, other_index.line_col());
                self.compile_expr(other_index, instructions, locals)?;
            }
            instructions.push(OpCode::Concat, second_index_line_col);
        }
        Ok(())
    }

    fn compile_binary_expr(
        &self,
        expr: Pair<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        let expr = first_child(expr);
        match expr.as_rule() {
            Rule::simple_binary_expr | Rule::simple_binary_print_expr => {
                let expr = self.compile_simple_binary_expr(expr.into_inner(), locals)?;
                instructions.extend(expr.instructions);
            }
            Rule::multidimensional_in => {
                let mut inner = expr.into_inner();
                let index = inner.child();
                let array = inner.child();
                let line_col = array.line_col();
                let array = array.into_inner();
                let mut parts = array.clone();
                let name = parts.child();
                let is_subarray = parts.next().is_some();
                self.compile_subscripted(
                    array,
                    OpCode::IndexArraySubarray,
                    line_col,
                    instructions,
                    locals,
                )?;
                self.compile_array_index(index.into_inner(), instructions, locals)?;
                // gawk does not name a subarray that is a scalar here.
                if is_subarray {
                    instructions.push(OpCode::In, line_col);
                } else {
                    instructions.push_using_array(OpCode::In, line_col, name.as_str(), locals);
                }
            }
            _ => not_in_grammar(&expr, "binary expression"),
        }
        Ok(())
    }

    /// `getline` or `getline var`, from the main input.
    fn compile_simple_getline(
        &self,
        getline: Pair<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        let line_col = getline.line_col();
        if let Some(lvalue) = getline.into_inner().next() {
            self.compile_lvalue_ref(lvalue, instructions, locals)?;
        } else {
            instructions.extend(Instructions::from_instructions_and_line_col(
                vec![OpCode::PushZero, OpCode::FieldRef],
                line_col,
            ));
        }
        instructions.push(
            OpCode::CallBuiltin {
                function: BuiltinFunction::GetLine,
                argc: 1,
            },
            line_col,
        );
        Ok(())
    }

    fn compile_input_function(
        &self,
        expr: Pair<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        let input_function = first_child(expr);
        let line_col = input_function.line_col();
        match input_function.as_rule() {
            Rule::simple_getline => {
                self.compile_simple_getline(input_function, instructions, locals)?;
            }
            Rule::getline_from_file | Rule::getline_from_file_cmp => {
                let is_cmp = input_function.as_rule() == Rule::getline_from_file_cmp;
                let mut inner = input_function.into_inner();
                let lvalue = inner.child();
                let file = if lvalue.as_rule() == Rule::lvalue {
                    self.compile_lvalue_ref(lvalue, instructions, locals)?;
                    inner.child()
                } else {
                    instructions.extend(Instructions::from_instructions_and_line_col(
                        vec![OpCode::PushZero, OpCode::FieldRef],
                        line_col,
                    ));
                    lvalue
                };
                let file_expr = self.compile_simple_binary_expr(file.into_inner(), locals)?;
                instructions.extend(file_expr.instructions);
                instructions.push(
                    OpCode::CallBuiltin {
                        function: BuiltinFunction::GetLineFromFile,
                        argc: 2,
                    },
                    line_col,
                );
                if is_cmp {
                    let comp = first_child(inner.child());
                    self.compile_expr(inner.child(), instructions, locals)?;
                    instructions.push(comparison_opcode(&comp), comp.line_col());
                }
            }
            Rule::getline_from_pipe => {
                let mut inner = input_function.into_inner();
                let unpiped_expr = inner.child();
                let mut lvalues = Vec::new();
                for piped_getline in inner {
                    lvalues.push(piped_getline.into_inner().next());
                }
                let getline_count = lvalues.len();
                for lvalue in lvalues.into_iter().rev() {
                    if let Some(lvalue) = lvalue {
                        self.compile_lvalue_ref(lvalue.clone(), instructions, locals)?;
                    } else {
                        instructions.extend(Instructions::from_instructions_and_line_col(
                            vec![OpCode::PushZero, OpCode::FieldRef],
                            line_col,
                        ));
                    }
                }
                self.compile_expr(unpiped_expr, instructions, locals)?;
                for _ in 0..getline_count {
                    instructions.push(
                        OpCode::CallBuiltin {
                            function: BuiltinFunction::GetLineFromPipe,
                            argc: 2,
                        },
                        line_col,
                    );
                }
            }
            _ => not_in_grammar(&input_function, "input function"),
        }
        Ok(())
    }

    fn compile_expr(
        &self,
        expr: Pair<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        let expr = first_child(expr);
        match expr.as_rule() {
            Rule::assignment | Rule::print_assignment => {
                let mut inner = expr.into_inner();
                let target = inner.child();
                let assignment_op = first_child(inner.child());
                let value = inner.child();
                if assignment_op.as_rule() == Rule::assign
                    && let Some((reference, tail)) = self.appended(&target, &value, locals)?
                {
                    instructions.extend(reference);
                    instructions.extend(tail);
                    instructions.push(OpCode::AppendAssign, assignment_op.line_col());
                    return Ok(());
                }
                self.compile_lvalue_ref(target, instructions, locals)?;
                if assignment_op.as_rule() != Rule::assign {
                    instructions.push(OpCode::Dup, assignment_op.line_col());
                    self.compile_expr(value, instructions, locals)?;
                    match assignment_op.as_rule() {
                        Rule::add_assign => {
                            instructions.push(OpCode::Add, assignment_op.line_col())
                        }
                        Rule::sub_assign => {
                            instructions.push(OpCode::Sub, assignment_op.line_col())
                        }
                        Rule::mul_assign => {
                            instructions.push(OpCode::Mul, assignment_op.line_col())
                        }
                        Rule::div_assign => {
                            instructions.push(OpCode::DivAssign, assignment_op.line_col())
                        }
                        Rule::mod_assign => {
                            instructions.push(OpCode::ModAssign, assignment_op.line_col())
                        }
                        Rule::pow_assign => {
                            instructions.push(OpCode::Pow, assignment_op.line_col())
                        }
                        _ => not_in_grammar(&assignment_op, "assignment"),
                    }
                } else {
                    self.compile_expr(value, instructions, locals)?;
                }

                instructions.push(OpCode::Assign, assignment_op.line_col());
            }
            Rule::ternary_expr | Rule::ternary_print_expr => {
                let line_col = expr.line_col();
                let mut inner = expr.into_inner();
                self.compile_binary_expr(inner.child(), instructions, locals)?;
                let mut true_expr_instructions = Instructions::default();
                self.compile_expr(inner.child(), &mut true_expr_instructions, locals)?;
                instructions.push(
                    OpCode::JumpIfFalse(true_expr_instructions.len() as i32 + 2),
                    line_col,
                );
                instructions.extend(true_expr_instructions);
                let mut false_expr_instructions = Instructions::default();
                self.compile_expr(inner.child(), &mut false_expr_instructions, locals)?;
                instructions.push(
                    OpCode::Jump(false_expr_instructions.len() as i32 + 1),
                    line_col,
                );
                instructions.extend(false_expr_instructions);
            }
            Rule::binary_expr | Rule::binary_print_expr => {
                self.compile_binary_expr(expr, instructions, locals)?;
            }
            Rule::input_function | Rule::unpiped_input_function => {
                self.compile_input_function(expr, instructions, locals)?;
            }
            _ => not_in_grammar(&expr, "expression"),
        }
        Ok(())
    }

    fn compile_simple_statement(
        &mut self,
        simple_stmt: Pair<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        let stmt = first_child(simple_stmt);
        let stmt_line_col = stmt.line_col();
        match stmt.as_rule() {
            Rule::array_delete => {
                let mut inner = stmt.into_inner();
                if inner.clone().nth(1).is_some() {
                    self.compile_subscripted(
                        inner,
                        OpCode::DeleteElement,
                        stmt_line_col,
                        instructions,
                        locals,
                    )?;
                } else {
                    let name = inner.child();
                    let get_instruction = self
                        .get_var(name.as_str(), locals)
                        .map_err(|msg| pest_error_from_span(name.as_span(), msg))?;
                    instructions.push(get_instruction, stmt_line_col);
                    instructions.push_using_array(
                        OpCode::ClearArray,
                        stmt_line_col,
                        name.as_str(),
                        locals,
                    );
                }
            }
            Rule::expr => {
                self.compile_expr(stmt, instructions, locals)?;
                // A statement `s = s x` leaves nothing, so the string is not copied for a
                // value that would be thrown away.
                if let Some(last) = instructions.opcodes.last_mut()
                    && *last == OpCode::AppendAssign
                {
                    *last = OpCode::AppendAssignDiscard;
                } else {
                    instructions.push(OpCode::Pop, stmt_line_col);
                }
            }
            Rule::print_stmt => {
                let mut inner = stmt.into_inner();
                let print = inner.child();
                let mut print_function;
                let mut argc;
                match print.as_rule() {
                    Rule::simple_print
                    | Rule::print_call
                    | Rule::simple_printf
                    | Rule::printf_call => {
                        let is_printf =
                            matches!(print.as_rule(), Rule::printf_call | Rule::simple_printf);
                        let expressions = print.into_inner();
                        argc = expressions.len() as u16;
                        if expressions.is_empty() && is_printf {
                            // gawk's fatal error, when it runs (`builtin_sprintf`).
                            print_function = BuiltinFunction::Printf;
                            argc = 0;
                        } else if expressions.is_empty() {
                            print_function = BuiltinFunction::Print;
                            instructions.push(OpCode::PushZero, stmt_line_col);
                            instructions.push(OpCode::GetField, stmt_line_col);
                            argc = 1;
                        } else {
                            for expr in expressions {
                                self.compile_expr(expr, instructions, locals)?;
                            }
                            if is_printf {
                                print_function = BuiltinFunction::Printf;
                            } else {
                                print_function = BuiltinFunction::Print;
                            }
                        }
                    }
                    _ => not_in_grammar(&print, "print statement"),
                }
                if let Some(output_redirection) = inner.next() {
                    match output_redirection.as_rule() {
                        Rule::truncate => {
                            if print_function == BuiltinFunction::Print {
                                print_function = BuiltinFunction::RedirectedPrintTruncate
                            } else {
                                print_function = BuiltinFunction::RedirectedPrintfTruncate
                            }
                        }
                        Rule::append => {
                            if print_function == BuiltinFunction::Print {
                                print_function = BuiltinFunction::RedirectedPrintAppend
                            } else {
                                print_function = BuiltinFunction::RedirectedPrintfAppend
                            }
                        }
                        Rule::pipe => {
                            if print_function == BuiltinFunction::Print {
                                print_function = BuiltinFunction::RedirectedPrintPipe
                            } else {
                                print_function = BuiltinFunction::RedirectedPrintfPipe
                            }
                        }
                        _ => not_in_grammar(&output_redirection, "output redirection"),
                    }
                    let target = first_child(output_redirection);
                    let target = self.compile_simple_binary_expr(target.into_inner(), locals)?;
                    instructions.extend(target.instructions);
                    argc += 1;
                }
                instructions.push(
                    OpCode::CallBuiltin {
                        function: print_function,
                        argc,
                    },
                    stmt_line_col,
                );
            }
            _ => not_in_grammar(&stmt, "simple statement"),
        }
        Ok(())
    }

    fn compile_do_while(
        &mut self,
        do_while: Pair<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        let mut inner = do_while.into_inner();
        let start_index = instructions.len();

        self.loop_stack.push(LoopStubs::default());

        // An empty body (`do ; while (c)`) is an `empty_stmt` node.
        self.compile_stmt(inner.child(), instructions, locals)?;
        let condition = inner.child();

        let condition_start = instructions.len();
        let condition_line_col = condition.line_col();
        self.compile_expr(condition, instructions, locals)?;
        instructions.push(
            OpCode::JumpIfTrue(distance(instructions.len(), start_index)),
            condition_line_col,
        );

        // `continue` re-tests the condition, as in C.
        let loop_stubs = self.loop_stack.pop().unwrap_or_default();
        for stub in loop_stubs.break_stubs {
            instructions.opcodes[stub] = OpCode::Jump(distance(stub, instructions.len()));
        }
        for stub in loop_stubs.continue_stubs {
            instructions.opcodes[stub] = OpCode::Jump(distance(stub, condition_start));
        }

        Ok(())
    }

    fn compile_for_each(
        &mut self,
        for_each_stmt: Pair<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        let mut inner = for_each_stmt.into_inner();

        let iter_var = inner.child();
        let iter_var_get_stmt = self
            .get_var(iter_var.as_str(), locals)
            .map_err(|msg| pest_error_from_span(iter_var.as_span(), msg))?;
        instructions.push(iter_var_get_stmt, iter_var.line_col());
        lvalue_to_scalar_ref(&mut instructions.opcodes, iter_var.as_span(), NOT_AN_LVALUE)?;

        // skip in_op node from grammar
        let next = inner.child();
        let array_var = if next.as_rule() == Rule::in_op {
            inner.child()
        } else {
            next
        };
        let array_var_line_col = array_var.line_col();
        let array_ref = array_var.into_inner();
        let mut parts = array_ref.clone();
        let name = parts.child();
        let is_subarray = parts.next().is_some();
        self.compile_subscripted(
            array_ref,
            OpCode::IndexArraySubarray,
            array_var_line_col,
            instructions,
            locals,
        )?;
        // gawk does not name a subarray that is a scalar here.
        if is_subarray {
            instructions.push(OpCode::CreateIterator, array_var_line_col);
        } else {
            instructions.push_using_array(
                OpCode::CreateIterator,
                array_var_line_col,
                name.as_str(),
                locals,
            );
        }

        let iter_deref_location = instructions.len();
        instructions.push(OpCode::Invalid, array_var_line_col);

        self.loop_stack.push(LoopStubs::default());

        self.compile_stmt(inner.child(), instructions, locals)?;

        instructions.push(
            OpCode::Jump(distance(instructions.len(), iter_deref_location)),
            array_var_line_col,
        );

        // The iterator stays on the stack while the body runs; `AdvanceIterOrJump` pops
        // it when the keys run out. A `break` leaves early, so it lands on a `Pop` of
        // its own, which the exhausted iterator jumps over.
        let loop_stubs = self.loop_stack.pop().unwrap_or_default();
        if !loop_stubs.break_stubs.is_empty() {
            let break_landing = instructions.len();
            instructions.push(OpCode::Pop, array_var_line_col);
            for stub in loop_stubs.break_stubs {
                instructions.opcodes[stub] = OpCode::Jump(distance(stub, break_landing));
            }
        }
        for stub in loop_stubs.continue_stubs {
            instructions.opcodes[stub] = OpCode::Jump(distance(stub, iter_deref_location));
        }

        instructions.opcodes[iter_deref_location] =
            OpCode::AdvanceIterOrJump(distance(iter_deref_location, instructions.len()));

        Ok(())
    }

    fn compile_for(
        &mut self,
        for_stmt: Pair<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        let mut inner = for_stmt.into_inner();

        self.loop_stack.push(LoopStubs::default());

        // Any of the three clauses may be empty; an empty condition loops until `break`.
        let init = inner.child();
        if let Some(init) = init.into_inner().next() {
            self.compile_simple_statement(init, instructions, locals)?;
        }

        let condition_start = instructions.len();
        let condition = inner.child();
        let condition_line_col = condition.line_col();
        let for_jump_index = match condition.into_inner().next() {
            Some(condition) => {
                self.compile_expr(condition, instructions, locals)?;
                let index = instructions.len();
                instructions.push(OpCode::Invalid, condition_line_col);
                Some(index)
            }
            None => None,
        };

        let update = inner.child();
        // An empty body (`for (;;) ;`) is an `empty_stmt` node.
        self.compile_stmt(inner.child(), instructions, locals)?;
        let update_start = instructions.len();
        if let Some(update) = update.into_inner().next() {
            self.compile_simple_statement(update, instructions, locals)?;
        }
        instructions.push(
            OpCode::Jump(distance(instructions.len(), condition_start)),
            condition_line_col,
        );
        if let Some(for_jump_index) = for_jump_index {
            instructions.opcodes[for_jump_index] =
                OpCode::JumpIfFalse(distance(for_jump_index, instructions.len()));
        }

        let loop_stubs = self.loop_stack.pop().unwrap_or_default();
        for stub in loop_stubs.break_stubs {
            instructions.opcodes[stub] = OpCode::Jump(distance(stub, instructions.len()));
        }
        for stub in loop_stubs.continue_stubs {
            instructions.opcodes[stub] = OpCode::Jump(distance(stub, update_start));
        }

        Ok(())
    }

    fn compile_while(
        &mut self,
        while_stmt: Pair<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        let mut inner = while_stmt.into_inner();

        self.loop_stack.push(LoopStubs::default());

        let condition_start = instructions.len();
        let condition = inner.child();
        let condition_line_col = condition.line_col();
        self.compile_expr(condition, instructions, locals)?;
        let while_jump_index = instructions.len();
        instructions.push(OpCode::Invalid, condition_line_col);

        self.compile_stmt(inner.child(), instructions, locals)?;
        instructions.push(
            OpCode::Jump(distance(instructions.len(), condition_start)),
            condition_line_col,
        );

        instructions.opcodes[while_jump_index] =
            OpCode::JumpIfFalse(distance(while_jump_index, instructions.len()));

        let loop_stubs = self.loop_stack.pop().unwrap_or_default();
        for stub in loop_stubs.break_stubs {
            instructions.opcodes[stub] = OpCode::Jump(distance(stub, instructions.len()));
        }
        for stub in loop_stubs.continue_stubs {
            instructions.opcodes[stub] = OpCode::Jump(distance(stub, condition_start));
        }
        Ok(())
    }

    fn compile_if(
        &mut self,
        if_stmt: Pair<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        let mut inner = if_stmt.into_inner();

        let condition = inner.child();
        let condition_line_col = condition.line_col();
        self.compile_expr(condition, instructions, locals)?;

        let if_jump_index = instructions.len();
        instructions.push(OpCode::Invalid, condition_line_col);

        // An empty body (`if (c) ;`) is an `empty_stmt` node, so an `else` that follows
        // it is the third child, never the body.
        self.compile_stmt(inner.child(), instructions, locals)?;

        if let Some(else_body) = inner.next() {
            let else_jump_index = instructions.len();
            instructions.push(OpCode::Invalid, else_body.line_col());
            instructions.opcodes[if_jump_index] =
                OpCode::JumpIfFalse(distance(if_jump_index, instructions.len()));
            self.compile_stmt(else_body, instructions, locals)?;
            instructions.opcodes[else_jump_index] =
                OpCode::Jump(distance(else_jump_index, instructions.len()));
        } else {
            instructions.opcodes[if_jump_index] =
                OpCode::JumpIfFalse(distance(if_jump_index, instructions.len()));
        }

        Ok(())
    }

    fn compile_action(
        &mut self,
        action: Pair<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        for stmt in action.into_inner() {
            self.compile_stmt(stmt, instructions, locals)?;
        }
        Ok(())
    }

    fn compile_stmt(
        &mut self,
        stmt: Pair<Rule>,
        instructions: &mut Instructions,
        locals: &LocalMap,
    ) -> Result<(), PestError> {
        match stmt.as_rule() {
            Rule::action => self.compile_action(stmt, instructions, locals),
            Rule::t_if => self.compile_if(stmt, instructions, locals),
            Rule::t_while => self.compile_while(stmt, instructions, locals),
            Rule::t_for => self.compile_for(stmt, instructions, locals),
            Rule::t_foreach => self.compile_for_each(stmt, instructions, locals),
            Rule::ut_if => self.compile_if(stmt, instructions, locals),
            Rule::ut_while => self.compile_while(stmt, instructions, locals),
            Rule::ut_for => self.compile_for(stmt, instructions, locals),
            Rule::ut_foreach => self.compile_for_each(stmt, instructions, locals),
            Rule::simple_statement => self.compile_simple_statement(stmt, instructions, locals),
            Rule::empty_stmt => Ok(()),
            Rule::next | Rule::nextfile => {
                let is_next = stmt.as_rule() == Rule::next;
                // gawk's words: there is no record to go on from in BEGIN or END.
                if let Some(action) = self.special_action {
                    let keyword = if is_next { "next" } else { "nextfile" };
                    self.deferred_errors.borrow_mut().push(gawk_error(
                        GAWK_ERROR,
                        stmt.as_span().start_pos(),
                        &format!("`{keyword}' used in {action} action"),
                    ));
                    return Ok(());
                }
                let opcode = if is_next {
                    OpCode::Next
                } else {
                    OpCode::NextFile
                };
                instructions.push(opcode, stmt.line_col());
                Ok(())
            }
            Rule::break_stmt => {
                if let Some(loop_stubs) = self.loop_stack.last_mut() {
                    loop_stubs.break_stubs.push(instructions.len());
                    instructions.push(OpCode::Invalid, stmt.line_col());
                    Ok(())
                } else {
                    // gawk's error, which it writes twice and goes on after.
                    self.deferred_errors.borrow_mut().push(gawk_error(
                        GAWK_TWICE,
                        stmt.as_span().start_pos(),
                        "`break' is not allowed outside a loop or switch",
                    ));
                    Ok(())
                }
            }
            Rule::continue_stmt => {
                if let Some(loop_stubs) = self.loop_stack.last_mut() {
                    loop_stubs.continue_stubs.push(instructions.len());
                    instructions.push(OpCode::Invalid, stmt.line_col());
                    Ok(())
                } else {
                    self.deferred_errors.borrow_mut().push(gawk_error(
                        GAWK_TWICE,
                        stmt.as_span().start_pos(),
                        "`continue' is not allowed outside a loop",
                    ));
                    Ok(())
                }
            }
            Rule::exit_stmt => {
                let stmt_line_col = stmt.line_col();
                if let Some(expr) = stmt.into_inner().next() {
                    self.compile_expr(expr, instructions, locals)?;
                } else {
                    instructions.push(OpCode::PushZero, stmt_line_col);
                }
                instructions.push(OpCode::Exit, stmt_line_col);
                Ok(())
            }
            Rule::return_stmt => {
                if !self.in_function {
                    return Err(gawk_error(
                        GAWK_CARET,
                        stmt.as_span().start_pos(),
                        "`return' used outside function context",
                    ));
                }
                let stmt_line_col = stmt.line_col();
                if let Some(expr) = stmt.into_inner().next() {
                    self.compile_expr(expr, instructions, locals)?;
                } else {
                    instructions.push(OpCode::PushUninitializedScalar, stmt_line_col);
                }
                instructions.push(OpCode::Return, stmt_line_col);
                Ok(())
            }
            Rule::do_while => self.compile_do_while(stmt, instructions, locals),
            _ => not_in_grammar(&stmt, "statement"),
        }
    }

    fn compile_normal_pattern(
        &mut self,
        pattern: Pair<Rule>,
        file: Rc<str>,
    ) -> Result<Pattern, PestError> {
        let pattern = first_child(pattern);
        match pattern.as_rule() {
            Rule::expr => {
                let mut instructions = Instructions::default();
                self.compile_expr(pattern, &mut instructions, &HashMap::new())?;
                Ok(Pattern::Expr(instructions.into_action(file)))
            }
            Rule::range_pattern => {
                let mut inner = pattern.into_inner();

                let start = inner.child();
                let mut start_instructions = Instructions::default();
                self.compile_expr(start, &mut start_instructions, &HashMap::new())?;

                let end = inner.child();
                let mut end_instructions = Instructions::default();
                self.compile_expr(end, &mut end_instructions, &HashMap::new())?;

                Ok(Pattern::Range {
                    start: start_instructions.into_action(file.clone()),
                    end: end_instructions.into_action(file),
                })
            }
            _ => not_in_grammar(&pattern, "pattern"),
        }
    }

    fn compile_rule(&mut self, rule: Pair<Rule>, file: Rc<str>) -> Result<AwkRule, PestError> {
        let rule = first_child(rule);
        match rule.as_rule() {
            Rule::action => {
                let mut instructions = Instructions::default();
                self.compile_action(rule, &mut instructions, &HashMap::new())?;
                Ok(AwkRule {
                    pattern: Pattern::All,
                    action: instructions.into_action(file),
                })
            }
            Rule::pattern_and_action => {
                let mut inner = rule.into_inner();
                let pattern = self.compile_normal_pattern(inner.child(), file.clone())?;
                let action = inner.child();
                let mut instructions = Instructions::default();
                let locals = HashMap::new();
                self.compile_action(action, &mut instructions, &locals)?;
                Ok(AwkRule {
                    pattern,
                    action: instructions.into_action(file),
                })
            }
            Rule::normal_pattern => {
                let rule_line_col = rule.line_col();
                let pattern = self.compile_normal_pattern(rule, file.clone())?;
                let instructions = Instructions::from_instructions_and_line_col(
                    vec![
                        OpCode::PushZero,
                        OpCode::GetField,
                        OpCode::CallBuiltin {
                            function: BuiltinFunction::Print,
                            argc: 1,
                        },
                    ],
                    rule_line_col,
                );
                Ok(AwkRule {
                    pattern,
                    action: instructions.into_action(file),
                })
            }
            _ => not_in_grammar(&rule, "rule"),
        }
    }

    fn compile_function_definition(
        &mut self,
        function: Pair<Rule>,
        file: Rc<str>,
    ) -> Result<Function, PestError> {
        let mut inner = function.into_inner();
        let name = inner.child();
        let mut param_map = HashMap::new();
        let mut parameter_names = Vec::new();
        let mut parameters_count = 0;
        let maybe_param_list = inner.child();
        let body = if maybe_param_list.as_rule() == Rule::param_list {
            for param in maybe_param_list.into_inner() {
                // gawk's errors, after which it goes on; another function's name is a
                // parameter like any other, as in gawk.
                let function_name = name.as_str();
                let param_name = param.as_str();
                let problem = if matches!(
                    self.names.get_mut().get(param_name),
                    Some(GlobalName::SpecialVar(_))
                ) {
                    Some(format!(
                        "parameter `{param_name}': POSIX disallows using a special variable \
                         as a function parameter"
                    ))
                } else if param_name == function_name {
                    Some("cannot use function name as parameter name".to_string())
                } else {
                    parameter_names
                        .iter()
                        .position(|earlier: &Rc<str>| **earlier == *param_name)
                        .map(|earlier| {
                            format!(
                                "parameter #{}, `{param_name}', duplicates parameter #{}",
                                parameters_count + 1,
                                earlier + 1
                            )
                        })
                };
                if let Some(problem) = problem {
                    self.deferred_errors.borrow_mut().push(gawk_error(
                        GAWK_ERROR,
                        param.as_span().start_pos(),
                        &format!("function `{function_name}': {problem}"),
                    ));
                }
                param_map.insert(param.as_str().to_string(), parameters_count as u32);
                parameter_names.push(Rc::from(param.as_str()));
                parameters_count += 1;
            }
            inner.child()
        } else {
            maybe_param_list
        };
        let mut instructions = Instructions::default();
        self.in_function = true;
        self.compile_action(body, &mut instructions, &param_map)?;
        self.in_function = false;

        // ensure that functions always return
        if !matches!(instructions.opcodes.last(), Some(OpCode::Return)) {
            let name_line_col = name.line_col();
            instructions.push(OpCode::PushUninitializedScalar, name_line_col);
            instructions.push(OpCode::Return, name_line_col);
        }

        let (instructions, debug_info) = instructions.into_parts(file);
        Ok(Function {
            name: name.as_str().into(),
            parameters_count,
            parameter_names,
            instructions,
            debug_info,
        })
    }

    fn declare_program_functions(&mut self, program: Pairs<Rule>, errors: &mut Vec<PestError>) {
        for item in program {
            if item.as_rule() == Rule::function_definition {
                let mut inner = item.into_inner();
                let name = inner.child();
                let maybe_parameter_list = inner.child();
                let parameter_count = if maybe_parameter_list.as_rule() == Rule::param_list {
                    maybe_parameter_list.into_inner().count() as u32
                } else {
                    0
                };
                let function_id = post_increment(&self.last_global_function_id);
                let previous_value = self.names.borrow_mut().insert(
                    name.as_str().to_string(),
                    GlobalName::Function {
                        id: function_id,
                        parameter_count,
                    },
                );
                if previous_value.is_some() {
                    // gawk reports nothing after it.
                    errors.push(gawk_error(
                        GAWK_ERROR_THEN_STOP,
                        name.as_span().start_pos(),
                        &format!("function name `{name}' previously defined"),
                    ));
                }
            }
        }
    }
}

/// The program `source`, cut where a statement may end before byte `error_at` of a syntax
/// error, with the blocks it opened closed, so that it parses: the code gawk has read when
/// it meets the error, for the errors it reports there first (`BEGIN { print 1/0; x( }`).
fn parsable_prefix(source: &str, error_at: usize) -> Option<String> {
    let head = source.get(..error_at)?;
    let ends = head
        .char_indices()
        .rev()
        .filter(|(_, c)| matches!(c, ';' | '\n' | '}'))
        .map(|(at, _)| at + 1);
    std::iter::once(error_at)
        .chain(ends)
        .take(32)
        .flat_map(|cut| (0..=4).map(move |closers| (cut, closers)))
        .map(|(cut, closers)| {
            format!(
                "{}\n{}",
                source.get(..cut).unwrap_or_default(),
                "}".repeat(closers)
            )
        })
        .find(|text| AwkParser::parse(Rule::program, text).is_ok())
}

/// gawk's errors for `sources`, the one of index `failed` not parsing: those in the code
/// before its syntax error, which gawk reports as it reads, then the syntax error, after
/// which gawk reads no further.
fn read_until_syntax_error(sources: &[SourceFile], failed: usize) -> CompilerErrors {
    let source = sources
        .get(failed)
        .map(|source| source.contents.as_str())
        .unwrap_or_default();
    // gawk's lexer ends the program with a newline.
    let error_at = match AwkParser::parse(Rule::program, &format!("{source}\n")) {
        Err(error) => match error.location {
            InputLocation::Pos(at) | InputLocation::Span((at, _)) => at,
        },
        Ok(_) => source.len(),
    };
    let from_file = sources
        .get(failed)
        .is_some_and(|source| !source.filename.is_empty());
    let syntax = syntax_error(source, error_at, from_file);
    let mut read: Vec<SourceFile> = sources.iter().take(failed).cloned().collect();
    if let Some(prefix) = parsable_prefix(source, error_at.min(source.len())) {
        read.push(SourceFile {
            filename: sources
                .get(failed)
                .map(|source| source.filename.clone())
                .unwrap_or_default(),
            contents: prefix,
        });
    }
    let mut diagnostics: Vec<(usize, Diagnostic)> = match compile_program(&read) {
        Ok(_) => Vec::new(),
        Err(errors) => errors
            .diagnostics
            .into_iter()
            // A function defined after the error is not declared yet.
            .filter(|(_, diagnostic)| !matches!(diagnostic.kind, Kind::Fatal | Kind::LateWarning))
            .collect(),
    };
    diagnostics.push((failed, syntax));
    CompilerErrors::new(diagnostics, &source_texts(sources))
}

/// Each source's file name and text, for `CompilerErrors`.
fn source_texts(sources: &[SourceFile]) -> Vec<(&str, &str)> {
    sources
        .iter()
        .map(|source| (source.filename.as_str(), source.contents.as_str()))
        .collect()
}

#[derive(Clone)]
pub struct SourceFile {
    pub filename: String,
    pub contents: String,
}

impl SourceFile {
    pub fn stdin(contents: String) -> Self {
        Self {
            filename: "".to_string(),
            contents,
        }
    }
}

pub fn compile_program(sources: &[SourceFile]) -> Result<Program, CompilerErrors> {
    let mut parsed_sources = Vec::new();
    // Each error with the index of its source.
    let mut errors: Vec<(usize, PestError)> = Vec::new();
    // gawk's lexer ends a program given as an argument with a newline, so that a backslash
    // last on it continues the line; a program file ends where it ends.
    let texts: Vec<Cow<str>> = sources
        .iter()
        .map(|source| {
            let contents = source.contents.as_str();
            if source.filename.is_empty() && contents.ends_with('\\') {
                Cow::Owned(format!("{contents}\n"))
            } else {
                Cow::Borrowed(contents)
            }
        })
        .collect();
    for (index, source_file) in sources.iter().enumerate() {
        let filename: Rc<str> = source_file.filename.clone().into();
        let text = texts.get(index).map_or("", |text| text.as_ref());
        match AwkParser::parse(Rule::program, text) {
            Ok(mut program) => {
                let program = program.child();
                parsed_sources.push((index, filename, program.into_inner()));
            }
            // gawk reads no further than its first syntax error.
            Err(_) => return Err(read_until_syntax_error(sources, index)),
        };
    }

    let mut compiler = Compiler::default();
    for (index, _, program_iter) in &parsed_sources {
        let mut declared = Vec::new();
        compiler.declare_program_functions(program_iter.clone(), &mut declared);
        errors.extend(declared.into_iter().map(|error| (*index, error)));
    }

    let mut begin_actions = Vec::new();
    let mut rules = Vec::new();
    let mut end_actions = Vec::new();
    let mut functions = Vec::new();
    for (index, filename, program_iter) in parsed_sources {
        for item in program_iter {
            let errors_before = errors.len();
            match item.as_rule() {
                Rule::begin_action | Rule::end_action => {
                    let is_begin_action = item.as_rule() == Rule::begin_action;
                    let mut instructions = Instructions::default();
                    compiler.special_action = Some(if is_begin_action { "BEGIN" } else { "END" });
                    let result = compiler.compile_action(
                        first_child(item),
                        &mut instructions,
                        &HashMap::new(),
                    );
                    compiler.special_action = None;
                    if let Err(err) = result {
                        errors.push((index, err));
                    }
                    if is_begin_action {
                        begin_actions.push(instructions.into_action(filename.clone()));
                    } else {
                        end_actions.push(instructions.into_action(filename.clone()));
                    }
                }
                Rule::rule => match compiler.compile_rule(item, filename.clone()) {
                    Ok(rule) => rules.push(rule),
                    Err(err) => errors.push((index, err)),
                },
                Rule::function_definition => {
                    match compiler.compile_function_definition(item, filename.clone()) {
                        Ok(function) => functions.push(function),
                        Err(err) => errors.push((index, err)),
                    }
                }
                Rule::EOI => {}
                _ => not_in_grammar(&item, "program"),
            }
            // The item's errors that did not stop it came before the one that did.
            let deferred = compiler.deferred_errors.get_mut().drain(..);
            let deferred: Vec<(usize, PestError)> = deferred.map(|error| (index, error)).collect();
            errors.splice(errors_before..errors_before, deferred);
        }
    }

    let globals = compiler
        .names
        .into_inner()
        .into_iter()
        .filter_map(|(k, v)| match v {
            GlobalName::Variable(id) => Some((k, id)),
            GlobalName::SpecialVar(id) => Some((k, id)),
            _ => None,
        })
        .collect();

    let diagnostics: Vec<(usize, Diagnostic)> = errors
        .iter()
        .map(|(index, error)| (*index, diagnostic(error)))
        .collect();
    let texts = source_texts(sources);
    if diagnostics.iter().all(|(_, d)| d.kind.is_warning()) {
        let mut warnings = Vec::new();
        let mut diagnostics = diagnostics;
        diagnostics.sort_by_key(|(index, warning)| (*index, warning.offset));
        for (index, warning) in diagnostics {
            let (file, text) = texts.get(index).copied().unwrap_or_default();
            warnings.extend(warning.render(text, file));
        }
        Ok(Program {
            constants: compiler.constants.into_inner(),
            begin_actions,
            rules,
            end_actions,
            functions,
            globals_count: compiler.last_global_var_id.get() as usize,
            globals,
            warnings,
        })
    } else {
        Err(CompilerErrors::new(diagnostics, &texts))
    }
}

/// The text of a regex literal with each backslash and newline taken out: gawk's lexer
/// joins the lines there, as in a string. An escaped backslash before a newline is kept.
fn join_continued_lines(text: &str) -> Cow<'_, str> {
    if !text.contains("\\\n") {
        return Cow::Borrowed(text);
    }
    let mut joined = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            joined.push(c);
            continue;
        }
        match chars.next() {
            Some('\n') => {}
            Some(next) => {
                joined.push('\\');
                joined.push(next);
            }
            None => joined.push('\\'),
        }
    }
    Cow::Owned(joined)
}

/// Rewrites the escapes POSIX awk defines in a regex literal into the syntax of the Rust
/// regex engine: `\ddd` (one to three octal digits) and `\b`, which is a backspace in
/// awk but a word boundary in Rust, become `\x{..}`. Everything else,
/// including `\\` and `\/`, is passed through.
fn translate_ere_escapes(ere: &str) -> String {
    let mut out = String::with_capacity(ere.len());
    let mut chars = ere.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some(d @ '0'..='7') => {
                let mut value = d.to_digit(8).unwrap_or(0);
                chars.next();
                for _ in 0..2 {
                    match chars.peek().and_then(|c| c.to_digit(8)) {
                        Some(digit) => {
                            value = value * 8 + digit;
                            chars.next();
                        }
                        None => break,
                    }
                }
                out.push_str(&format!("\\x{{{value:x}}}"));
            }
            Some('b') => {
                chars.next();
                out.push_str("\\x{8}");
            }
            Some(other) => {
                chars.next();
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failed assumption in a test should abort it loudly"
)]
mod test {

    use super::*;
    use crate::regex::regex_from_str;

    const FIRST_GLOBAL_VAR: u32 = SpecialVar::Count as u32;

    fn compile_expr(expr: &str) -> (Vec<OpCode>, Vec<Constant>) {
        let mut program = compile_program(&[SourceFile::stdin(format!("BEGIN {{ {} }}", expr))])
            .expect("error compiling expression");
        // remove OpCode::Pop
        program.begin_actions[0].instructions.pop();
        (
            program.begin_actions[0].instructions.clone(),
            program.constants,
        )
    }

    fn compile_stmt(stmt: &str) -> (Vec<OpCode>, Vec<Constant>) {
        let program = compile_program(&[SourceFile::stdin(format!("BEGIN {{ {} }}", stmt))])
            .expect("error compiling statement");
        (
            program.begin_actions[0].instructions.clone(),
            program.constants,
        )
    }

    fn compile_correct_program(text: &str) -> Program {
        compile_program(&[SourceFile::stdin(text.to_string())]).expect("error compiling program")
    }

    fn does_not_compile(text: &str) {
        compile_program(&[SourceFile::stdin(text.to_string())])
            .expect_err("expected error compiling program");
    }

    #[test]
    fn appending_to_a_variable_is_done_in_place() {
        // `s = s x` appends to `s`; it read `s`, copied it into a new string with `x` and
        // copied that again into `s`, so a string built a piece at a time took time in its
        // square (TODO.md phase 15).
        let (instructions, _) = compile_stmt("s = s \" w\" i");
        assert_eq!(
            instructions,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::GetGlobal(FIRST_GLOBAL_VAR + 1),
                OpCode::Concat,
                OpCode::AppendAssignDiscard,
            ]
        );
        // Not when what is appended could change `s` first.
        let (instructions, _) = compile_stmt("s = s (x = 1)");
        assert!(!instructions.contains(&OpCode::AppendAssignDiscard));
    }

    #[test]
    fn test_compile_empty_program() {
        let program = compile_correct_program("");
        assert!(program.constants.is_empty(), "{:?}", program.constants);
        assert!(
            program.begin_actions.is_empty(),
            "{:?}",
            program.begin_actions
        );
        assert!(program.rules.is_empty(), "{:?}", program.rules);
        assert!(program.end_actions.is_empty(), "{:?}", program.end_actions);
        assert!(program.functions.is_empty(), "{:?}", program.functions);
    }

    #[test]
    fn test_compile_empty_begin() {
        let program = compile_correct_program("BEGIN {}");
        assert!(program.constants.is_empty(), "{:?}", program.constants);
        assert_eq!(program.begin_actions.len(), 1);
        assert!(
            program.begin_actions[0].instructions.is_empty(),
            "{:?}",
            program.begin_actions[0].instructions
        );
        assert!(program.rules.is_empty(), "{:?}", program.rules);
        assert!(program.end_actions.is_empty(), "{:?}", program.end_actions);
        assert!(program.functions.is_empty(), "{:?}", program.functions);
    }

    #[test]
    fn test_compile_empty_end() {
        let program = compile_correct_program("END {}");
        assert!(program.constants.is_empty(), "{:?}", program.constants);
        assert!(
            program.begin_actions.is_empty(),
            "{:?}",
            program.begin_actions
        );
        assert!(program.rules.is_empty(), "{:?}", program.rules);
        assert_eq!(program.end_actions.len(), 1);
        assert!(
            program.end_actions[0].instructions.is_empty(),
            "{:?}",
            program.end_actions[0].instructions
        );
        assert!(program.functions.is_empty(), "{:?}", program.functions);
    }

    #[test]
    fn test_compile_numbers() {
        let (_, constants) = compile_expr("123");
        assert_eq!(constants, vec![Constant::Number(123.0)]);

        let (_, constants) = compile_expr("1.23");
        assert_eq!(constants, vec![Constant::Number(1.23)]);

        let (_, constants) = compile_expr(".154");
        assert_eq!(constants, vec![Constant::Number(0.154)]);

        let (_, constants) = compile_expr("1.");
        assert_eq!(constants, vec![Constant::Number(1.0)]);

        let (_, constants) = compile_expr("1.0e3");
        assert_eq!(constants, vec![Constant::Number(1.0e3)]);

        let (_, constants) = compile_expr("1.0e-3");
        assert_eq!(constants, vec![Constant::Number(1.0e-3)]);

        let (_, constants) = compile_expr("1.0e+3");
        assert_eq!(constants, vec![Constant::Number(1.0e+3)]);

        let (_, constants) = compile_expr("1e5");
        assert_eq!(constants, vec![Constant::Number(1e5)]);

        let (_, constants) = compile_expr("5.f");
        assert_eq!(constants, vec![Constant::Number(5.0)]);

        let (_, constants) = compile_expr("5.34F");
        assert_eq!(constants, vec![Constant::Number(5.34)]);

        let (_, constants) = compile_expr("5.34l");
        assert_eq!(constants, vec![Constant::Number(5.34)]);

        let (_, constants) = compile_expr("5.34L");
        assert_eq!(constants, vec![Constant::Number(5.34)]);
    }

    #[test]
    fn test_compile_string() {
        let (_, constants) = compile_expr(r#""""#);
        assert_eq!(constants, vec!["".into()]);

        let (_, constants) = compile_expr(r#""hello""#);
        assert_eq!(constants, vec!["hello".into()]);

        let (_, constants) = compile_expr(r#""\"""#);
        assert_eq!(constants, vec![Constant::from("\"")]);

        let (_, constants) = compile_expr(r#""\/""#);
        assert_eq!(constants, vec![Constant::from("/")]);

        let (_, constants) = compile_expr(r#""\a""#);
        assert_eq!(constants, vec![Constant::from("\x07")]);

        let (_, constants) = compile_expr(r#""\b""#);
        assert_eq!(constants, vec![Constant::from("\x08")]);

        let (_, constants) = compile_expr(r#""\f""#);
        assert_eq!(constants, vec![Constant::from("\x0C")]);

        let (_, constants) = compile_expr(r#""\n""#);
        assert_eq!(constants, vec![Constant::from("\n")]);

        let (_, constants) = compile_expr(r#""\r""#);
        assert_eq!(constants, vec![Constant::from("\r")]);

        let (_, constants) = compile_expr(r#""\t""#);
        assert_eq!(constants, vec![Constant::from("\t")]);

        let (_, constants) = compile_expr(r#""\v""#);
        assert_eq!(constants, vec![Constant::from("\x0B")]);

        let (_, constants) = compile_expr(r#""\\""#);
        assert_eq!(constants, vec![Constant::from("\\")]);

        let (_, constants) = compile_expr(r#""\7""#);
        assert_eq!(constants, vec![Constant::from("\x07")]);

        let (_, constants) = compile_expr(r#""\41""#);
        assert_eq!(constants, vec![Constant::from("!")]);

        let (_, constants) = compile_expr(r#""\142""#);
        assert_eq!(constants, vec![Constant::from("b")]);

        let (_, constants) = compile_expr(r#""hello\nworld""#);
        assert_eq!(constants, vec![Constant::from("hello\nworld")]);

        let (_, constants) = compile_expr(r#""hello\tworld""#);
        assert_eq!(constants, vec!["hello\tworld".into()]);

        let (_, constants) = compile_expr(r#""hello\\world""#);
        assert_eq!(constants, vec![Constant::from("hello\\world")]);

        let (_, constants) = compile_expr(r#""hello\"world""#);
        assert_eq!(constants, vec![Constant::from(r#"hello"world"#)]);

        let (_, constants) = compile_expr(r#""hello\41world""#);
        assert_eq!(constants, vec![Constant::from("hello!world")]);

        let (_, constants) = compile_expr(r#""hello\141world""#);
        assert_eq!(constants, vec![Constant::from("helloaworld")]);
    }

    #[test]
    fn test_compile_unary_numeric_ops() {
        let (instructions, _) = compile_expr("-1");
        assert_eq!(instructions, vec![OpCode::PushConstant(0), OpCode::Negate]);

        let (instructions, _) = compile_expr("+1");
        assert_eq!(
            instructions,
            vec![OpCode::PushConstant(0), OpCode::AsNumber]
        );

        let (instructions, _) = compile_expr("!1");
        assert_eq!(instructions, vec![OpCode::PushConstant(0), OpCode::Not]);

        let (instructions, _) = compile_expr("++a");
        assert_eq!(
            instructions,
            vec![OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR), OpCode::PreInc]
        );

        let (instructions, _) = compile_expr("--a");
        assert_eq!(
            instructions,
            vec![OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR), OpCode::PreDec]
        );

        let (instructions, _) = compile_expr("a++");
        assert_eq!(
            instructions,
            vec![OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR), OpCode::PostInc]
        );

        let (instructions, _) = compile_expr("a--");
        assert_eq!(
            instructions,
            vec![OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR), OpCode::PostDec]
        );

        let (instructions, _) = compile_expr("++a[0]");
        assert_eq!(
            instructions,
            vec![
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::IndexArrayGetRef,
                OpCode::PreInc
            ]
        );

        let (instructions, _) = compile_expr("++$3");
        assert_eq!(
            instructions,
            vec![OpCode::PushConstant(0), OpCode::FieldRef, OpCode::PreInc]
        );
    }

    #[test]
    fn test_compile_binary_numeric_ops() {
        let (instructions, _) = compile_expr("1 + 2");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Add,
            ]
        );

        let (instructions, _) = compile_expr("1 - 2");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Sub,
            ]
        );

        let (instructions, _) = compile_expr("1 * 2");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Mul,
            ]
        );

        let (instructions, _) = compile_expr("1 / 2");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Div,
            ]
        );

        let (instructions, _) = compile_expr("1 % 2");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Mod,
            ]
        );

        let (instructions, _) = compile_expr("1 ^ 2");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Pow,
            ]
        );
    }

    #[test]
    fn test_exp_is_right_associative() {
        let (instructions, _) = compile_expr("1 ^ 2 ^ 3");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::PushConstant(2),
                OpCode::Pow,
                OpCode::Pow,
            ]
        );
    }

    #[test]
    fn test_compile_binary_numeric_exprs_with_correct_precedence() {
        let (instructions, constants) = compile_expr("1 + 2 * 3");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::PushConstant(2),
                OpCode::Mul,
                OpCode::Add,
            ]
        );
        assert_eq!(
            constants,
            vec![
                Constant::Number(1.0),
                Constant::Number(2.0),
                Constant::Number(3.0),
            ]
        );

        let (instructions, constants) = compile_expr("3 * 8 + 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Mul,
                OpCode::PushConstant(2),
                OpCode::Add,
            ]
        );
        assert_eq!(
            constants,
            vec![
                Constant::Number(3.0),
                Constant::Number(8.0),
                Constant::Number(1.0),
            ]
        );

        let (instructions, constants) = compile_expr("34 + 7 / 3.45");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::PushConstant(2),
                OpCode::Div,
                OpCode::Add,
            ]
        );
        assert_eq!(
            constants,
            vec![
                Constant::Number(34.0),
                Constant::Number(7.0),
                Constant::Number(3.45),
            ]
        );

        let (instructions, _) = compile_expr("1 / 2 + 3");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Div,
                OpCode::PushConstant(2),
                OpCode::Add,
            ]
        );

        let (instructions, _) = compile_expr("1 + 2 % 3");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::PushConstant(2),
                OpCode::Mod,
                OpCode::Add,
            ]
        );

        let (instructions, _) = compile_expr("1 % 2 + 3");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Mod,
                OpCode::PushConstant(2),
                OpCode::Add,
            ]
        );

        let (instructions, _) = compile_expr("1 + 2 ^ 3");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::PushConstant(2),
                OpCode::Pow,
                OpCode::Add,
            ]
        );

        let (instructions, _) = compile_expr("1 ^ 2 * 3");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Pow,
                OpCode::PushConstant(2),
                OpCode::Mul,
            ]
        );
    }

    #[test]
    fn compile_concat() {
        let (instructions, constants) = compile_expr(r#""hello" "world""#);
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Concat,
            ]
        );
        assert_eq!(
            constants,
            vec![Constant::from("hello"), Constant::from("world"),]
        );

        let (instructions, constants) = compile_expr(r#""hello" 1 "world""#);
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Concat,
                OpCode::PushConstant(2),
                OpCode::Concat,
            ]
        );
        assert_eq!(
            constants,
            vec![
                Constant::from("hello"),
                Constant::Number(1.0),
                Constant::from("world"),
            ]
        );

        let (instructions, constants) = compile_expr(r#""hello"1"#);
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Concat,
            ]
        );
        assert_eq!(
            constants,
            vec![Constant::from("hello"), Constant::Number(1.0)]
        );

        let (instructions, constants) = compile_expr(r#"1"hello""#);
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Concat,
            ]
        );
        assert_eq!(
            constants,
            vec![Constant::Number(1.0), Constant::from("hello")]
        );
    }

    #[test]
    fn test_compile_comp_op() {
        let (instructions, constants) = compile_expr(r#"1 < "x""#);
        assert_eq!(
            instructions,
            vec![OpCode::PushConstant(0), OpCode::PushConstant(1), OpCode::Lt]
        );
        assert_eq!(constants, vec![Constant::Number(1.0), Constant::from("x")]);

        let (instructions, constants) = compile_expr("1 > 2");
        assert_eq!(
            instructions,
            vec![OpCode::PushConstant(0), OpCode::PushConstant(1), OpCode::Gt]
        );
        assert_eq!(
            constants,
            vec![Constant::Number(1.0), Constant::Number(2.0)]
        );

        let (instructions, constants) = compile_expr("1 <= 2");
        assert_eq!(
            instructions,
            vec![OpCode::PushConstant(0), OpCode::PushConstant(1), OpCode::Le]
        );
        assert_eq!(
            constants,
            vec![Constant::Number(1.0), Constant::Number(2.0)]
        );

        let (instructions, constants) = compile_expr(r#" "str" >= 2"#);
        assert_eq!(
            instructions,
            vec![OpCode::PushConstant(0), OpCode::PushConstant(1), OpCode::Ge]
        );
        assert_eq!(
            constants,
            vec![Constant::from("str"), Constant::Number(2.0)]
        );

        let (instructions, constants) = compile_expr("1 == 2");
        assert_eq!(
            instructions,
            vec![OpCode::PushConstant(0), OpCode::PushConstant(1), OpCode::Eq]
        );
        assert_eq!(
            constants,
            vec![Constant::Number(1.0), Constant::Number(2.0)]
        );

        let (instructions, constants) = compile_expr("1 != 2");
        assert_eq!(
            instructions,
            vec![OpCode::PushConstant(0), OpCode::PushConstant(1), OpCode::Ne]
        );
        assert_eq!(
            constants,
            vec![Constant::Number(1.0), Constant::Number(2.0)]
        );
    }

    #[test]
    fn test_compile_in_expr() {
        let (instructions, constants) = compile_expr(r#""a" in map"#);
        assert_eq!(
            instructions,
            vec![
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::In
            ]
        );
        assert_eq!(constants, vec![Constant::from("a")]);
    }

    #[test]
    fn test_compile_multidimensional_in_operator() {
        let (instructions, constants) = compile_expr(r#"(1, 2) in map"#);
        assert_eq!(
            instructions,
            vec![
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::GetGlobal(SpecialVar::Subsep as u32),
                OpCode::Concat,
                OpCode::PushConstant(1),
                OpCode::Concat,
                OpCode::In
            ]
        );
        assert_eq!(
            constants,
            vec![Constant::Number(1.0), Constant::Number(2.0)]
        );

        let (instructions, constants) = compile_expr(r#"(1, "str", 3) in map"#);
        assert_eq!(
            instructions,
            vec![
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::GetGlobal(SpecialVar::Subsep as u32),
                OpCode::Concat,
                OpCode::PushConstant(1),
                OpCode::Concat,
                OpCode::GetGlobal(SpecialVar::Subsep as u32),
                OpCode::Concat,
                OpCode::PushConstant(2),
                OpCode::Concat,
                OpCode::In
            ]
        );
        assert_eq!(
            constants,
            vec![
                Constant::Number(1.0),
                Constant::from("str"),
                Constant::Number(3.0)
            ]
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_compile_match() {
        let (instructions, constants) = compile_expr(r#" "hello" ~ /hello/ "#);
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Match
            ]
        );
        assert_eq!(
            constants,
            vec![
                Constant::from("hello"),
                Constant::Regex(Rc::new(regex_from_str("hello")))
            ]
        )
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_compile_not_match() {
        let (instructions, constants) = compile_expr(r#" "test" !~ /te?s+t*/"#);
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Match,
                OpCode::Not
            ]
        );
        assert_eq!(
            constants,
            vec![
                Constant::from("test"),
                Constant::Regex(Rc::new(regex_from_str("te?s+t*")))
            ]
        )
    }

    #[test]
    fn test_compile_and() {
        let (instructions, constants) = compile_expr("1 && 2");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::JumpIfFalse(3),
                OpCode::PushConstant(1),
                OpCode::Jump(2),
                OpCode::PushZero,
            ]
        );
        assert_eq!(
            constants,
            vec![Constant::Number(1.0), Constant::Number(2.0)]
        );

        let (instructions, constants) = compile_expr(r#"1 < 2 && "x" >= "y""#);
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Lt,
                OpCode::JumpIfFalse(5),
                OpCode::PushConstant(2),
                OpCode::PushConstant(3),
                OpCode::Ge,
                OpCode::Jump(2),
                OpCode::PushZero,
            ]
        );
        assert_eq!(
            constants,
            vec![
                Constant::Number(1.0),
                Constant::Number(2.0),
                Constant::from("x"),
                Constant::from("y"),
            ]
        );
    }

    #[test]
    fn test_compile_or() {
        let (instructions, constants) = compile_expr("1 || 2");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::JumpIfTrue(3),
                OpCode::PushConstant(1),
                OpCode::Jump(2),
                OpCode::PushOne,
            ]
        );
        assert_eq!(
            constants,
            vec![Constant::Number(1.0), Constant::Number(2.0)]
        );

        let (instructions, constants) = compile_expr(r#"1 < 2 || "x" >= "y""#);
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Lt,
                OpCode::JumpIfTrue(5),
                OpCode::PushConstant(2),
                OpCode::PushConstant(3),
                OpCode::Ge,
                OpCode::Jump(2),
                OpCode::PushOne,
            ]
        );
        assert_eq!(
            constants,
            vec![
                Constant::Number(1.0),
                Constant::Number(2.0),
                Constant::from("x"),
                Constant::from("y"),
            ]
        );
    }

    #[test]
    fn test_compile_ternary_expression() {
        let (instructions, constants) = compile_expr("1 ? 2 : 3");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::JumpIfFalse(3),
                OpCode::PushConstant(1),
                OpCode::Jump(2),
                OpCode::PushConstant(2),
            ]
        );
        assert_eq!(
            constants,
            vec![
                Constant::Number(1.0),
                Constant::Number(2.0),
                Constant::Number(3.0),
            ]
        );

        let (instructions, constants) = compile_expr(r#"a == 1 ? "one" : a == 2 ? "two" : "many""#);
        assert_eq!(
            instructions,
            vec![
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::Eq,
                OpCode::JumpIfFalse(3),
                OpCode::PushConstant(1),
                OpCode::Jump(8),
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(2),
                OpCode::Eq,
                OpCode::JumpIfFalse(3),
                OpCode::PushConstant(3),
                OpCode::Jump(2),
                OpCode::PushConstant(4),
            ]
        );
        assert_eq!(
            constants,
            vec![
                Constant::Number(1.0),
                Constant::from("one"),
                Constant::Number(2.0),
                Constant::from("two"),
                Constant::from("many"),
            ]
        );
    }

    #[test]
    fn test_compile_simple_variable_assignment() {
        let (instructions, constants) = compile_expr("a = 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::Assign,
            ]
        );
        assert_eq!(constants, vec![Constant::Number(1.0)]);

        let (instructions, constants) = compile_expr("a = b = 2");
        assert_eq!(
            instructions,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR + 1),
                OpCode::PushConstant(0),
                OpCode::Assign,
                OpCode::Assign,
            ]
        );
        assert_eq!(constants, vec![Constant::Number(2.0)]);
    }

    #[test]
    fn test_compile_compound_variable_assignment() {
        let (instructions, constants) = compile_expr("a += 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::Dup,
                OpCode::PushConstant(0),
                OpCode::Add,
                OpCode::Assign,
            ]
        );
        assert_eq!(constants, vec![Constant::Number(1.0)]);

        let (instructions, constants) = compile_expr("a -= 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::Dup,
                OpCode::PushConstant(0),
                OpCode::Sub,
                OpCode::Assign,
            ]
        );
        assert_eq!(constants, vec![Constant::Number(1.0)]);

        let (instructions, constants) = compile_expr("a *= 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::Dup,
                OpCode::PushConstant(0),
                OpCode::Mul,
                OpCode::Assign,
            ]
        );
        assert_eq!(constants, vec![Constant::Number(1.0)]);

        let (instructions, constants) = compile_expr("a /= 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::Dup,
                OpCode::PushConstant(0),
                OpCode::DivAssign,
                OpCode::Assign,
            ]
        );
        assert_eq!(constants, vec![Constant::Number(1.0)]);

        let (instructions, constants) = compile_expr("a %= 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::Dup,
                OpCode::PushConstant(0),
                OpCode::ModAssign,
                OpCode::Assign,
            ]
        );
        assert_eq!(constants, vec![Constant::Number(1.0)]);

        let (instructions, constants) = compile_expr("a ^= 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::Dup,
                OpCode::PushConstant(0),
                OpCode::Pow,
                OpCode::Assign,
            ]
        );
        assert_eq!(constants, vec![Constant::Number(1.0)]);

        let (instructions, constants) = compile_expr("a += b += 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::Dup,
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR + 1),
                OpCode::Dup,
                OpCode::PushConstant(0),
                OpCode::Add,
                OpCode::Assign,
                OpCode::Add,
                OpCode::Assign,
            ]
        );
        assert_eq!(constants, vec![Constant::Number(1.0)]);
    }

    #[test]
    fn compile_array_element_assignment() {
        let (instructions, constants) = compile_expr("a[1] = 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::IndexArrayGetRef,
                OpCode::PushConstant(1),
                OpCode::Assign,
            ]
        );
        assert_eq!(
            constants,
            vec![Constant::Number(1.0), Constant::Number(1.0)]
        );
    }

    #[test]
    fn compile_multidimensional_array_index() {
        let (instructions, constants) = compile_expr("a[1, 2]");
        assert_eq!(
            instructions,
            vec![
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::GetGlobal(SpecialVar::Subsep as u32),
                OpCode::Concat,
                OpCode::PushConstant(1),
                OpCode::Concat,
                OpCode::IndexArrayGetValue,
            ]
        );
        assert_eq!(
            constants,
            vec![Constant::Number(1.0), Constant::Number(2.0)]
        );

        let (instructions, constants) = compile_expr("a[1, 2, \"test\"]");
        assert_eq!(
            instructions,
            vec![
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::GetGlobal(SpecialVar::Subsep as u32),
                OpCode::Concat,
                OpCode::PushConstant(1),
                OpCode::Concat,
                OpCode::GetGlobal(SpecialVar::Subsep as u32),
                OpCode::Concat,
                OpCode::PushConstant(2),
                OpCode::Concat,
                OpCode::IndexArrayGetValue,
            ]
        );
        assert_eq!(
            constants,
            vec![
                Constant::Number(1.0),
                Constant::Number(2.0),
                Constant::from("test")
            ]
        );
    }

    #[test]
    fn compile_field_var_assignment() {
        let (instructions, constants) = compile_expr("$1 = 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::FieldRef,
                OpCode::PushConstant(1),
                OpCode::Assign,
            ]
        );
        assert_eq!(
            constants,
            vec![Constant::Number(1.0), Constant::Number(1.0)]
        );
    }

    #[test]
    fn compile_field_var_compairisons() {
        let (instructions, constants) = compile_expr("$1 == 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::GetField,
                OpCode::PushConstant(1),
                OpCode::Eq,
            ]
        );
        assert_eq!(
            constants,
            vec![Constant::Number(1.0), Constant::Number(1.0)]
        );
    }

    #[test]
    fn compile_terminated_if() {
        let (instructions, constant) = compile_stmt("if (1) \n 1;");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::JumpIfFalse(3),
                OpCode::PushConstant(1),
                OpCode::Pop,
            ]
        );
        assert_eq!(constant, vec![Constant::Number(1.0), Constant::Number(1.0)]);

        let (instructions, constant) = compile_stmt("if (1) \n 1; \n else \n 2;");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::JumpIfFalse(4),
                OpCode::PushConstant(1),
                OpCode::Pop,
                OpCode::Jump(3),
                OpCode::PushConstant(2),
                OpCode::Pop,
            ]
        );
        assert_eq!(
            constant,
            vec![
                Constant::Number(1.0),
                Constant::Number(1.0),
                Constant::Number(2.0)
            ]
        );
    }

    #[test]
    fn compile_terminated_while() {
        let (instructions, constant) = compile_stmt("while (1) \n 1;");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::JumpIfFalse(4),
                OpCode::PushConstant(1),
                OpCode::Pop,
                OpCode::Jump(-4),
            ]
        );
        assert_eq!(constant, vec![Constant::Number(1.0), Constant::Number(1.0)]);

        let (instructions, constant) = compile_stmt("while (1) {1; 2; 3;}");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::JumpIfFalse(8),
                OpCode::PushConstant(1),
                OpCode::Pop,
                OpCode::PushConstant(2),
                OpCode::Pop,
                OpCode::PushConstant(3),
                OpCode::Pop,
                OpCode::Jump(-8),
            ]
        );
        assert_eq!(
            constant,
            vec![
                Constant::Number(1.0),
                Constant::Number(1.0),
                Constant::Number(2.0),
                Constant::Number(3.0),
            ]
        );
    }

    #[test]
    fn test_compile_terminated_for() {
        let (instructions, constant) = compile_stmt("for (i = 0; i < 10; i++) 1;");
        assert_eq!(
            instructions,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::Assign,
                OpCode::Pop,
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(1),
                OpCode::Lt,
                OpCode::JumpIfFalse(7),
                OpCode::PushConstant(2),
                OpCode::Pop,
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::PostInc,
                OpCode::Pop,
                OpCode::Jump(-9),
            ]
        );
        assert_eq!(
            constant,
            vec![
                Constant::Number(0.0),
                Constant::Number(10.0),
                Constant::Number(1.0),
            ]
        );
    }

    #[test]
    fn compile_unterminated_if() {
        let (instructions, constant) = compile_stmt("if (1) \n 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::JumpIfFalse(3),
                OpCode::PushConstant(1),
                OpCode::Pop,
            ]
        );
        assert_eq!(constant, vec![Constant::Number(1.0), Constant::Number(1.0)]);

        let (instructions, constant) = compile_stmt("if (1) \n 1; \n else \n 2");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::JumpIfFalse(4),
                OpCode::PushConstant(1),
                OpCode::Pop,
                OpCode::Jump(3),
                OpCode::PushConstant(2),
                OpCode::Pop,
            ]
        );
        assert_eq!(
            constant,
            vec![
                Constant::Number(1.0),
                Constant::Number(1.0),
                Constant::Number(2.0)
            ]
        );
    }

    #[test]
    fn compile_unterminated_while() {
        let (instructions, constant) = compile_stmt("while (1) \n 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::JumpIfFalse(4),
                OpCode::PushConstant(1),
                OpCode::Pop,
                OpCode::Jump(-4),
            ]
        );
        assert_eq!(constant, vec![Constant::Number(1.0), Constant::Number(1.0)]);
    }

    #[test]
    fn test_compile_unterminated_for() {
        let (instructions, constant) = compile_stmt("for (i = 0; i < 10; i++) 1");
        assert_eq!(
            instructions,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::Assign,
                OpCode::Pop,
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(1),
                OpCode::Lt,
                OpCode::JumpIfFalse(7),
                OpCode::PushConstant(2),
                OpCode::Pop,
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::PostInc,
                OpCode::Pop,
                OpCode::Jump(-9),
            ]
        );
        assert_eq!(
            constant,
            vec![
                Constant::Number(0.0),
                Constant::Number(10.0),
                Constant::Number(1.0),
            ]
        );
    }

    #[test]
    fn test_compile_break_inside_while_loop() {
        let (instructions, _) = compile_stmt("while (1) { 1; break; }");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::JumpIfFalse(5),
                OpCode::PushConstant(1),
                OpCode::Pop,
                OpCode::Jump(2),
                OpCode::Jump(-5),
            ]
        );
    }

    #[test]
    fn test_compile_continue_in_while_loop() {
        let (instructions, _) = compile_stmt("while (1) { 1; continue; }");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::JumpIfFalse(5),
                OpCode::PushConstant(1),
                OpCode::Pop,
                OpCode::Jump(-4),
                OpCode::Jump(-5),
            ]
        );
    }

    #[test]
    fn test_compile_break_inside_for_loop() {
        let (instructions, _) = compile_stmt("for (i = 0; i < 10; i++) { 1; break; }");
        assert_eq!(
            instructions,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::Assign,
                OpCode::Pop,
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(1),
                OpCode::Lt,
                OpCode::JumpIfFalse(8),
                OpCode::PushConstant(2),
                OpCode::Pop,
                OpCode::Jump(5),
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::PostInc,
                OpCode::Pop,
                OpCode::Jump(-10),
            ]
        );
    }

    #[test]
    fn test_compile_continue_in_for_loop() {
        let (instructions, _) = compile_stmt("for (i = 0; i < 10; i++) { 1; continue; }");
        assert_eq!(
            instructions,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::Assign,
                OpCode::Pop,
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(1),
                OpCode::Lt,
                OpCode::JumpIfFalse(8),
                OpCode::PushConstant(2),
                OpCode::Pop,
                OpCode::Jump(1),
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::PostInc,
                OpCode::Pop,
                OpCode::Jump(-10),
            ]
        );
    }

    #[test]
    fn test_compile_next() {
        let program = compile_correct_program("{ next; }");
        assert_eq!(program.rules[0].action.instructions, vec![OpCode::Next]);
    }

    #[test]
    fn test_compile_nextfile() {
        let program = compile_correct_program("{ nextfile; }");
        assert_eq!(program.rules[0].action.instructions, vec![OpCode::NextFile]);
    }

    #[test]
    fn test_next_and_nextfile_in_begin_or_end_do_not_compile() {
        does_not_compile("BEGIN { next }");
        does_not_compile("BEGIN { nextfile }");
        does_not_compile("END { while (1) next }");
        does_not_compile("END { nextfile }");
        compile_correct_program("function f() { next } BEGIN { f() }");
    }

    #[test]
    fn test_compile_exit() {
        let (instructions, _) = compile_stmt("exit;");
        assert_eq!(instructions, vec![OpCode::PushZero, OpCode::Exit]);

        let (instructions, _) = compile_stmt("exit 1;");
        assert_eq!(instructions, vec![OpCode::PushConstant(0), OpCode::Exit]);
    }

    #[test]
    fn test_compile_do_while() {
        let (instructions, constant) = compile_stmt("do 1; while (1);");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::Pop,
                OpCode::PushConstant(1),
                OpCode::JumpIfTrue(-3),
            ]
        );
        assert_eq!(constant, vec![Constant::Number(1.0), Constant::Number(1.0)]);
    }

    #[test]
    fn test_compile_delete_element() {
        let (instructions, _) = compile_stmt("delete a[1];");
        assert_eq!(
            instructions,
            vec![
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::DeleteElement,
            ]
        );
    }

    #[test]
    fn test_compile_clear_array() {
        let (instructions, _) = compile_stmt("delete a");
        assert_eq!(
            instructions,
            vec![OpCode::GetGlobal(FIRST_GLOBAL_VAR), OpCode::ClearArray]
        );
    }

    #[test]
    fn test_compile_simple_print() {
        let (instructions, constant) = compile_stmt("print 1;");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Print,
                    argc: 1
                },
            ]
        );
        assert_eq!(constant, vec![Constant::Number(1.0),]);

        let (instructions, constant) = compile_stmt(r#"print "number", 1, 2, "and", 3;"#);
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::PushConstant(2),
                OpCode::PushConstant(3),
                OpCode::PushConstant(4),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Print,
                    argc: 5
                },
            ]
        );
        assert_eq!(
            constant,
            vec![
                Constant::from("number"),
                Constant::Number(1.0),
                Constant::Number(2.0),
                Constant::from("and"),
                Constant::Number(3.0),
            ]
        );
    }

    #[test]
    fn test_compile_print_call() {
        let (instructions, constant) = compile_stmt("print (\"hello\");");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Print,
                    argc: 1
                },
            ]
        );
        assert_eq!(constant, vec![Constant::from("hello")]);

        let (instructions, constants) = compile_stmt(r#"print ("hello", 1, 2, "and", 3);"#);
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::PushConstant(2),
                OpCode::PushConstant(3),
                OpCode::PushConstant(4),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Print,
                    argc: 5
                },
            ]
        );
        assert_eq!(
            constants,
            vec![
                Constant::from("hello"),
                Constant::Number(1.0),
                Constant::Number(2.0),
                Constant::from("and"),
                Constant::Number(3.0),
            ]
        );
    }

    #[test]
    fn test_compile_redirected_simple_print() {
        let (instructions, constant) = compile_stmt("print 1 > \"file\";");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::RedirectedPrintTruncate,
                    argc: 2
                },
            ]
        );
        assert_eq!(
            constant,
            vec![Constant::Number(1.0), Constant::from("file"),]
        );

        let (instructions, constant) = compile_stmt("print 1 >> \"file\";");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::RedirectedPrintAppend,
                    argc: 2
                },
            ]
        );
        assert_eq!(
            constant,
            vec![Constant::Number(1.0), Constant::from("file"),]
        );

        let (instructions, constant) = compile_stmt(r#"print 1 | "bash";"#);
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::RedirectedPrintPipe,
                    argc: 2
                },
            ]
        );
        assert_eq!(
            constant,
            vec![Constant::Number(1.0), Constant::from("bash"),]
        );
    }

    #[test]
    fn test_compile_simple_printf() {
        let (instructions, constant) = compile_stmt("printf \"test\";");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Printf,
                    argc: 1
                },
            ]
        );
        assert_eq!(constant, vec![Constant::from("test"),]);

        let (instructions, constant) =
            compile_stmt(r#"printf "%d, %d, %s, %.6f", 1, 2, "and", 3;"#);
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::PushConstant(2),
                OpCode::PushConstant(3),
                OpCode::PushConstant(4),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Printf,
                    argc: 5
                },
            ]
        );
        assert_eq!(
            constant,
            vec![
                Constant::from("%d, %d, %s, %.6f"),
                Constant::Number(1.0),
                Constant::Number(2.0),
                Constant::from("and"),
                Constant::Number(3.0),
            ]
        );
    }

    #[test]
    fn test_compile_printf_call() {
        let (instructions, constant) = compile_stmt("printf (\"hello\");");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Printf,
                    argc: 1
                },
            ]
        );
        assert_eq!(constant, vec![Constant::from("hello")]);

        let (instructions, constants) =
            compile_stmt(r#"printf ("%g, %x, %s, %.6f", 1, 2, "and", 3);"#);
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::PushConstant(2),
                OpCode::PushConstant(3),
                OpCode::PushConstant(4),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Printf,
                    argc: 5
                },
            ]
        );
        assert_eq!(
            constants,
            vec![
                Constant::from("%g, %x, %s, %.6f"),
                Constant::Number(1.0),
                Constant::Number(2.0),
                Constant::from("and"),
                Constant::Number(3.0),
            ]
        );
    }

    #[test]
    fn test_compile_redirected_simple_printf() {
        let (instructions, constant) = compile_stmt("printf \"test\" > \"file\";");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::RedirectedPrintfTruncate,
                    argc: 2
                },
            ]
        );
        assert_eq!(
            constant,
            vec![Constant::from("test"), Constant::from("file"),]
        );

        let (instructions, constant) = compile_stmt("printf \"test\" >> \"file\";");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::RedirectedPrintfAppend,
                    argc: 2
                },
            ]
        );
        assert_eq!(
            constant,
            vec![Constant::from("test"), Constant::from("file"),]
        );

        let (instructions, constant) = compile_stmt(r#"printf "test" | "bash";"#);
        assert_eq!(
            instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::RedirectedPrintfPipe,
                    argc: 2
                },
            ]
        );
        assert_eq!(
            constant,
            vec![Constant::from("test"), Constant::from("bash"),]
        );
    }

    #[test]
    fn test_compile_empty_function() {
        let program = compile_correct_program(
            r#"
            function fun() {}
            "#,
        );
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].parameters_count, 0);
    }

    #[test]
    fn test_compile_function_with_no_parameters() {
        let program = compile_correct_program(
            r#"
            function fun() {
                x + 2;
            }
            "#,
        );
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].parameters_count, 0);
        assert_eq!(
            program.functions[0].instructions,
            vec![
                OpCode::GetGlobal(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::Add,
                OpCode::Pop,
                OpCode::PushUninitializedScalar,
                OpCode::Return,
            ]
        );
    }

    #[test]
    fn test_compile_function_with_parameters() {
        let program = compile_correct_program(
            r#"
            function fun(a, b, c) {
                a["1"] = b + c;
            }
            "#,
        );
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].parameters_count, 3);
        assert_eq!(
            program.functions[0].instructions,
            vec![
                OpCode::GetLocal(0),
                OpCode::PushConstant(0),
                OpCode::IndexArrayGetRef,
                OpCode::GetLocal(1),
                OpCode::GetLocal(2),
                OpCode::Add,
                OpCode::Assign,
                OpCode::Pop,
                OpCode::PushUninitializedScalar,
                OpCode::Return,
            ]
        );
    }

    #[test]
    fn test_compile_function_call_no_params() {
        let program = compile_correct_program(
            r#"
            function fun() {
            }
            BEGIN {fun()}
            "#,
        );
        assert_eq!(
            program.begin_actions[0].instructions,
            vec![OpCode::Call(0), OpCode::Pop]
        );
    }

    #[test]
    fn test_compile_function_call() {
        let program = compile_correct_program(
            r#"
            function fun(a, b) {
                a + b;
            }
            BEGIN {fun(1, 2)}
            "#,
        );
        assert_eq!(
            program.functions[0].instructions,
            vec![
                OpCode::GetLocal(0),
                OpCode::GetLocal(1),
                OpCode::Add,
                OpCode::Pop,
                OpCode::PushUninitializedScalar,
                OpCode::Return,
            ]
        );
        assert_eq!(
            program.begin_actions[0].instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::Call(0),
                OpCode::Pop,
            ]
        );
    }

    #[test]
    fn test_compile_function_call_with_too_few_arguments() {
        let program = compile_correct_program(
            r#"
            function fun(a, b) {
                a + b;
            }
            BEGIN {fun(1)}
            "#,
        );
        assert_eq!(
            program.functions[0].instructions,
            vec![
                OpCode::GetLocal(0),
                OpCode::GetLocal(1),
                OpCode::Add,
                OpCode::Pop,
                OpCode::PushUninitializedScalar,
                OpCode::Return,
            ]
        );
        assert_eq!(
            program.begin_actions[0].instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushUninitialized,
                OpCode::Call(0),
                OpCode::Pop,
            ]
        );
    }

    #[test]
    fn test_compile_empty_return_statement() {
        let program = compile_correct_program(
            r#"
            function fun() {
                return;
            }
            "#,
        );
        assert_eq!(
            program.functions[0].instructions,
            vec![OpCode::PushUninitializedScalar, OpCode::Return]
        );
    }

    #[test]
    fn test_compile_return_statement_with_expression() {
        let program = compile_correct_program(
            r#"
            function fun() {
                return 1;
            }
            "#,
        );
        assert_eq!(
            program.functions[0].instructions,
            vec![OpCode::PushConstant(0), OpCode::Return]
        );
    }

    #[test]
    fn test_return_statement_outside_of_function_is_err() {
        does_not_compile("BEGIN { return 1; }");
    }

    #[test]
    fn compile_rule_with_expression_pattern() {
        let program = compile_correct_program(
            r#"
            1 {
                1 + 2;
            }
            "#,
        );
        assert_eq!(program.rules.len(), 1);
        assert!(matches!(
            &program.rules[0].pattern,
            Pattern::Expr(Action { instructions, .. }) if *instructions == [OpCode::PushConstant(0)]
        ));
        assert_eq!(
            program.rules[0].action.instructions,
            vec![
                OpCode::PushConstant(1),
                OpCode::PushConstant(2),
                OpCode::Add,
                OpCode::Pop,
            ]
        );
        assert_eq!(
            program.constants,
            vec![
                Constant::Number(1.0),
                Constant::Number(1.0),
                Constant::Number(2.0)
            ]
        );
    }

    #[test]
    fn compile_rule_with_range_pattern() {
        let program = compile_correct_program(
            r#"
            1, 2 {
                1 + 2;
            }
            "#,
        );
        assert_eq!(program.rules.len(), 1);
        assert!(matches!(
            &program.rules[0].pattern,
            Pattern::Range { start, end } if start.instructions == [OpCode::PushConstant(0)] && end.instructions == [OpCode::PushConstant(1)]
        ));
        assert_eq!(
            program.rules[0].action.instructions,
            vec![
                OpCode::PushConstant(2),
                OpCode::PushConstant(3),
                OpCode::Add,
                OpCode::Pop,
            ]
        );
        assert_eq!(
            program.constants,
            vec![
                Constant::Number(1.0),
                Constant::Number(2.0),
                Constant::Number(1.0),
                Constant::Number(2.0)
            ]
        );
    }

    #[test]
    fn compile_rule_without_pattern() {
        let program = compile_correct_program(
            r#"
            {1}
            "#,
        );
        assert_eq!(program.rules.len(), 1);
        assert_eq!(program.rules[0].pattern, Pattern::All);
        assert_eq!(
            program.rules[0].action.instructions,
            vec![OpCode::PushConstant(0), OpCode::Pop,]
        );
        assert_eq!(program.constants, vec![Constant::Number(1.0)]);
    }

    #[test]
    fn compile_rule_without_action() {
        let program = compile_correct_program("1");
        assert_eq!(program.rules.len(), 1);
        assert!(matches!(
            &program.rules[0].pattern,
            Pattern::Expr(Action { instructions, .. }) if *instructions == [OpCode::PushConstant(0)]
        ));
        assert_eq!(
            program.rules[0].action.instructions,
            vec![
                OpCode::PushZero,
                OpCode::GetField,
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Print,
                    argc: 1
                }
            ]
        );
        assert_eq!(program.constants, vec![Constant::Number(1.0)]);
    }

    #[test]
    fn test_compile_print_with_no_args() {
        let (instructions, _) = compile_stmt("print;");
        assert_eq!(
            instructions,
            vec![
                OpCode::PushZero,
                OpCode::GetField,
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Print,
                    argc: 1
                }
            ]
        );
    }

    #[test]
    fn compile_builtin_arithmetic_functions() {
        let program = compile_correct_program(
            r#"
            BEGIN {
                atan2(1, 2);
                cos(1);
                sin(1);
                exp(1);
                log(1);
                sqrt(1);
                int(1);
                rand();
                srand();
                srand(1);
            }
            "#,
        );
        assert_eq!(
            program.begin_actions[0].instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Atan2,
                    argc: 2
                },
                OpCode::Pop,
                OpCode::PushConstant(2),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Cos,
                    argc: 1
                },
                OpCode::Pop,
                OpCode::PushConstant(3),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Sin,
                    argc: 1
                },
                OpCode::Pop,
                OpCode::PushConstant(4),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Exp,
                    argc: 1
                },
                OpCode::Pop,
                OpCode::PushConstant(5),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Log,
                    argc: 1
                },
                OpCode::Pop,
                OpCode::PushConstant(6),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Sqrt,
                    argc: 1
                },
                OpCode::Pop,
                OpCode::PushConstant(7),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Int,
                    argc: 1
                },
                OpCode::Pop,
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Rand,
                    argc: 0
                },
                OpCode::Pop,
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Srand,
                    argc: 0
                },
                OpCode::Pop,
                OpCode::PushConstant(8),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Srand,
                    argc: 1
                },
                OpCode::Pop,
            ]
        );
    }

    #[test]
    fn test_compile_builtin_string_functions() {
        let program = compile_correct_program(
            r#"
            BEGIN {
                gsub(/test/, "x");
                gsub(/y/, "x", s);
                index("a", "b");
                match("a", "b");
                split("test string", a);
                split("test string", a, /ere/);
                length;
                length();
                length("a");
                sprintf("a", 1);
                sprintf("a", 1, 2, 3, 4, 5);
                sub(/test/, "x");
                sub(/y/, "x", s);
                substr("a", 1);
                substr("a", 1, 2);
                tolower("A");
                toupper("a");
            }
        "#,
        );
        assert_eq!(
            program.begin_actions[0].instructions,
            vec![
                OpCode::PushZero,
                OpCode::FieldRef,
                OpCode::PushConstant(0),
                OpCode::PushConstant(1),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Gsub,
                    argc: 3,
                },
                OpCode::Pop,
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(2),
                OpCode::PushConstant(3),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Gsub,
                    argc: 3,
                },
                OpCode::Pop,
                OpCode::PushConstant(4),
                OpCode::PushConstant(5),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Index,
                    argc: 2,
                },
                OpCode::Pop,
                OpCode::PushConstant(6),
                OpCode::PushConstant(7),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Match,
                    argc: 2,
                },
                OpCode::Pop,
                OpCode::GetGlobal(FIRST_GLOBAL_VAR + 1),
                OpCode::PushConstant(8),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Split,
                    argc: 2,
                },
                OpCode::Pop,
                OpCode::GetGlobal(FIRST_GLOBAL_VAR + 1),
                OpCode::PushConstant(9),
                OpCode::PushConstant(10),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Split,
                    argc: 3,
                },
                OpCode::Pop,
                OpCode::PushZero,
                OpCode::GetField,
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Length,
                    argc: 1,
                },
                OpCode::Pop,
                OpCode::PushZero,
                OpCode::GetField,
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Length,
                    argc: 1,
                },
                OpCode::Pop,
                OpCode::PushConstant(11),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Length,
                    argc: 1,
                },
                OpCode::Pop,
                OpCode::PushConstant(12),
                OpCode::PushConstant(13),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Sprintf,
                    argc: 2,
                },
                OpCode::Pop,
                OpCode::PushConstant(14),
                OpCode::PushConstant(15),
                OpCode::PushConstant(16),
                OpCode::PushConstant(17),
                OpCode::PushConstant(18),
                OpCode::PushConstant(19),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Sprintf,
                    argc: 6,
                },
                OpCode::Pop,
                OpCode::PushZero,
                OpCode::FieldRef,
                OpCode::PushConstant(20),
                OpCode::PushConstant(21),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Sub,
                    argc: 3,
                },
                OpCode::Pop,
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(22),
                OpCode::PushConstant(23),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Sub,
                    argc: 3,
                },
                OpCode::Pop,
                OpCode::PushConstant(24),
                OpCode::PushConstant(25),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Substr,
                    argc: 2,
                },
                OpCode::Pop,
                OpCode::PushConstant(26),
                OpCode::PushConstant(27),
                OpCode::PushConstant(28),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Substr,
                    argc: 3,
                },
                OpCode::Pop,
                OpCode::PushConstant(29),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::ToLower,
                    argc: 1,
                },
                OpCode::Pop,
                OpCode::PushConstant(30),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::ToUpper,
                    argc: 1,
                },
                OpCode::Pop,
            ],
        );
    }

    #[test]
    fn test_compile_builtin_io_functions() {
        let program = compile_correct_program(
            r#"
            BEGIN {
                close("file");
                fflush("file");
                fflush();
                system("ls");
            }
        "#,
        );
        assert_eq!(
            program.begin_actions[0].instructions,
            vec![
                OpCode::PushConstant(0),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::Close,
                    argc: 1,
                },
                OpCode::Pop,
                OpCode::PushConstant(1),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::FFlush,
                    argc: 1
                },
                OpCode::Pop,
                OpCode::CallBuiltin {
                    function: BuiltinFunction::FFlush,
                    argc: 0
                },
                OpCode::Pop,
                OpCode::PushConstant(2),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::System,
                    argc: 1,
                },
                OpCode::Pop,
            ]
        );
    }

    #[test]
    fn test_compile_special_variables() {
        let program = compile_correct_program(
            r#"
            BEGIN {
                ARGC
                ARGV
                CONVFMT
                ENVIRON
                FILENAME
                FNR
                FS
                NF
                NR
                OFMT
                OFS
                ORS
                RLENGTH
                RS
                RSTART
                SUBSEP
            }
        "#,
        );
        assert_eq!(
            program.begin_actions[0].instructions,
            vec![
                OpCode::GetGlobal(SpecialVar::Argc as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Argv as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Convfmt as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Environ as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Filename as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Fnr as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Fs as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Nf as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Nr as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Ofmt as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Ofs as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Ors as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Rlength as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Rs as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Rstart as u32),
                OpCode::Pop,
                OpCode::GetGlobal(SpecialVar::Subsep as u32),
                OpCode::Pop,
            ]
        );
    }

    #[test]
    fn test_compile_for_each_stmt() {
        let (program, constants) = compile_stmt("for (a in array) {1;}");
        assert_eq!(
            program,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::GetGlobal(FIRST_GLOBAL_VAR + 1),
                OpCode::CreateIterator,
                OpCode::AdvanceIterOrJump(4),
                OpCode::PushConstant(0),
                OpCode::Pop,
                OpCode::Jump(-3)
            ]
        );
        assert_eq!(constants, vec![Constant::Number(1.0)]);
    }

    #[test]
    fn test_compile_recursive_function() {
        let program = compile_correct_program(
            r#"
            function fib(n) {
                if (n <= 1) {
                    return n;
                }
                return fib(n - 1) + fib(n - 2);
            }
        "#,
        );
        assert_eq!(
            program.functions[0].instructions,
            vec![
                OpCode::GetLocal(0),
                OpCode::PushConstant(0),
                OpCode::Le,
                OpCode::JumpIfFalse(3),
                OpCode::GetLocal(0),
                OpCode::Return,
                OpCode::GetLocal(0),
                OpCode::PushConstant(1),
                OpCode::Sub,
                OpCode::Call(0),
                OpCode::GetLocal(0),
                OpCode::PushConstant(2),
                OpCode::Sub,
                OpCode::Call(0),
                OpCode::Add,
                OpCode::Return,
            ]
        );
    }

    #[test]
    fn test_compile_mutually_recursive_functions() {
        let program = compile_correct_program(
            r#"
            function is_even(n) {
                if (n == 0) {
                    return 1;
                }
                return is_odd(n - 1);
            }

            function is_odd(n) {
                if (n == 0) {
                    return 0;
                }
                return is_even(n - 1);
            }
        "#,
        );
        assert_eq!(
            program.functions[0].instructions,
            vec![
                OpCode::GetLocal(0),
                OpCode::PushConstant(0),
                OpCode::Eq,
                OpCode::JumpIfFalse(3),
                OpCode::PushConstant(1),
                OpCode::Return,
                OpCode::GetLocal(0),
                OpCode::PushConstant(2),
                OpCode::Sub,
                OpCode::Call(1),
                OpCode::Return,
            ]
        );
        assert_eq!(
            program.functions[1].instructions,
            vec![
                OpCode::GetLocal(0),
                OpCode::PushConstant(3),
                OpCode::Eq,
                OpCode::JumpIfFalse(3),
                OpCode::PushConstant(4),
                OpCode::Return,
                OpCode::GetLocal(0),
                OpCode::PushConstant(5),
                OpCode::Sub,
                OpCode::Call(0),
                OpCode::Return,
            ]
        );
    }

    #[test]
    fn test_compile_simple_getline() {
        let (expr, _) = compile_expr("getline");
        assert_eq!(
            expr,
            vec![
                OpCode::PushZero,
                OpCode::FieldRef,
                OpCode::CallBuiltin {
                    function: BuiltinFunction::GetLine,
                    argc: 1
                }
            ]
        );

        let (expr, _) = compile_expr("getline var");
        assert_eq!(
            expr,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::GetLine,
                    argc: 1
                }
            ]
        )
    }

    #[test]
    fn test_compile_getline_from_file() {
        let (expr, _) = compile_expr("getline < \"file\"");
        assert_eq!(
            expr,
            vec![
                OpCode::PushZero,
                OpCode::FieldRef,
                OpCode::PushConstant(0),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::GetLineFromFile,
                    argc: 2
                }
            ]
        );

        let (expr, _) = compile_expr("getline var < \"file\"");
        assert_eq!(
            expr,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::GetLineFromFile,
                    argc: 2
                }
            ]
        );
    }

    #[test]
    fn test_compile_piped_getline() {
        let (expr, _) = compile_expr("\"command\" | getline");
        assert_eq!(
            expr,
            vec![
                OpCode::PushZero,
                OpCode::FieldRef,
                OpCode::PushConstant(0),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::GetLineFromPipe,
                    argc: 2
                }
            ]
        );

        let (expr, _) = compile_expr("\"command\" | getline var");
        assert_eq!(
            expr,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::PushConstant(0),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::GetLineFromPipe,
                    argc: 2
                }
            ]
        );

        let (expr, _) = compile_expr("getline var | getline var2");
        assert_eq!(
            expr,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR + 1),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::GetLine,
                    argc: 1
                },
                OpCode::CallBuiltin {
                    function: BuiltinFunction::GetLineFromPipe,
                    argc: 2
                }
            ]
        );

        let (expr, _) = compile_expr("\"test\" | getline var | getline var2 | getline var3");
        assert_eq!(
            expr,
            vec![
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR),
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR + 1),
                OpCode::GlobalScalarRef(FIRST_GLOBAL_VAR + 2),
                OpCode::PushConstant(0),
                OpCode::CallBuiltin {
                    function: BuiltinFunction::GetLineFromPipe,
                    argc: 2
                },
                OpCode::CallBuiltin {
                    function: BuiltinFunction::GetLineFromPipe,
                    argc: 2
                },
                OpCode::CallBuiltin {
                    function: BuiltinFunction::GetLineFromPipe,
                    argc: 2
                }
            ]
        );
    }
}
