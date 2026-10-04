//
// Copyright (c) 2024-2026 Hemi Labs, Inc.
//
// This file is part of the posixutils-rs project covered under
// the MIT License.  For the full license text, please see the LICENSE
// file in the root directory of this project.
// SPDX-License-Identifier: MIT
//

use array::Array;
use builtins::{builtin_match, builtin_sprintf, call_simple_builtin, print_to_string, sprintf};
use io::{
    EmptyRecordReader, FileStream, ReadFiles, ReadPipes, RecordReader, RecordSeparator,
    StdinRecordReader, WriteFiles, WritePipes,
};
use rand::rngs::SmallRng;
use rand::{RngExt as _, SeedableRng};
use record::{FieldSeparator, FieldsState, Record, ere_escape_char, record_index};
use stack::{ArrayIterator, ExecutionResult, Place, Stack, StackValue, compare_op, numeric_op};
use string::AwkString;
use value::{AwkRefType, AwkValue, AwkValueRef, AwkValueVariant};

use crate::compiler::take_escape_warnings;

/// A value given on the command line (`-v`, an operand, `-F`) with its escapes made, and
/// gawk's warnings about them written, unplaced, as gawk writes them.
fn escape_string_contents(value: &str) -> Result<Rc<str>, String> {
    let escaped = crate::compiler::escape_string_contents(value)?;
    let warnings = take_escape_warnings();
    if !warnings.is_empty() {
        let _ = io::flush_stdout();
        for warning in warnings {
            eprintln!("awk: warning: {warning}");
        }
    }
    Ok(escaped)
}
use crate::program::{
    Action, BuiltinFunction, Constant, Function, OpCode, Pattern, Program, SpecialVar,
};
use crate::regex::Regex;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::Write;
use std::iter;
use std::rc::Rc;
use std::time::SystemTime;

mod array;
mod builtins;
mod format;
mod io;
pub use io::set_shell;
pub(crate) use io::{flush_stdout, strerror};
mod record;
mod stack;
mod string;
mod value;

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed assumption in a test should abort it loudly"
)]
mod tests;

const STACK_SIZE: usize = 2048;

pub(crate) fn bool_to_f64(p: bool) -> f64 {
    if p { 1.0 } else { 0.0 }
}

/// The number at the start of input text, as gawk reads it (C's `strtod` without
/// hexadecimal, `inf` and `nan`): white space, then the longest decimal floating constant,
/// or `+inf`, `-inf`, `+nan` or `-nan` with only white space around it; 0 when there is
/// neither. `.5e` was 0 rather than 0.5, `inf` and `+infinity` were infinite, and `  -nan`
/// was 0 (`REVIEW_REPORT.md` TXT-16).
pub(crate) fn strtod(s: &str) -> f64 {
    match scan_number(s) {
        NumberText::Special(value) => value,
        NumberText::Decimal { number, .. } => number.parse().unwrap_or(0.0),
        NumberText::Nothing => 0.0,
    }
}

/// How input text reads as a number; see [`strtod`] and [`is_numeric_string`].
enum NumberText<'a> {
    /// `+inf`, `-inf`, `+nan` or `-nan`, in any case.
    Special(f64),
    /// The decimal floating constant at the start, with its sign, and whether nothing but
    /// white space follows it.
    Decimal {
        number: &'a str,
        whole: bool,
    },
    Nothing,
}

/// C's white space, which may surround a number in input text.
fn is_c_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

fn scan_number(text: &str) -> NumberText<'_> {
    let start = text.bytes().take_while(|&b| is_c_space(b)).count();
    // Only ASCII white space is skipped, so `start` is a character boundary.
    let text = text.get(start..).unwrap_or_default();
    let bytes = text.as_bytes();
    let core = text.trim_end_matches(|c: char| c.is_ascii() && is_c_space(c as u8));
    if let [sign @ (b'+' | b'-'), name @ ..] = core.as_bytes() {
        let value = if name.eq_ignore_ascii_case(b"inf") {
            Some(f64::INFINITY)
        } else if name.eq_ignore_ascii_case(b"nan") {
            Some(f64::NAN)
        } else {
            None
        };
        if let Some(value) = value {
            return NumberText::Special(if *sign == b'-' { -value } else { value });
        }
    }
    let digits = |from: usize| {
        bytes[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let mut at = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let whole = digits(at);
    at += whole;
    let mut fraction = 0;
    if bytes.get(at) == Some(&b'.') {
        fraction = digits(at + 1);
        at += 1 + fraction;
    }
    if whole + fraction == 0 {
        return NumberText::Nothing;
    }
    if let Some(b'e' | b'E') = bytes.get(at) {
        let sign = usize::from(matches!(bytes.get(at + 1), Some(b'+' | b'-')));
        let exponent = digits(at + 1 + sign);
        if exponent > 0 {
            at += 1 + sign + exponent;
        }
    }
    NumberText::Decimal {
        // Only ASCII is counted, so `at` is a character boundary.
        number: text.get(..at).unwrap_or_default(),
        whole: at == core.len(),
    }
}

/// `text` with its letters made small, as IGNORECASE compares it: one character for one,
/// so that places in it are those of `text`.
pub(crate) fn fold_case(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.chars().any(char::is_uppercase) {
        return std::borrow::Cow::Borrowed(text);
    }
    std::borrow::Cow::Owned(
        text.chars()
            .map(|c| {
                let mut lower = c.to_lowercase();
                match (lower.next(), lower.next()) {
                    (Some(small), None) => small,
                    _ => c,
                }
            })
            .collect(),
    )
}

/// The order of `a` and `b`, with case ignored when `fold`.
pub(crate) fn compare_text(a: &str, b: &str, fold: bool) -> std::cmp::Ordering {
    if fold {
        fold_case(a).cmp(&fold_case(b))
    } else {
        a.cmp(b)
    }
}

pub(crate) fn is_integer(num: f64) -> bool {
    num.is_finite() && num.fract() == 0.0
}

pub(crate) fn swap_with_default<T: Default>(value: &mut T) -> T {
    let mut result = T::default();
    std::mem::swap(&mut result, value);
    result
}

pub(crate) fn maybe_numeric_string<S: Into<AwkString>>(str: S) -> AwkString {
    let mut str = str.into();
    str.is_numeric = is_numeric_string(str.as_str());
    str
}

/// Whether input text (a field, `$0`, a `getline` var, an ARGV or `-v` value) is a
/// numeric string, which compares as a number, as gawk decides it: a decimal floating
/// constant with one optional sign and C white space around it, or `+inf`, `-inf`,
/// `+nan` or `-nan` in any case. An exponent needs its digits, and hexadecimal, a C
/// suffix such as `1f` and anything after the number make it a string.
///
/// It ran the program grammar's number rule on every field of every record, the most
/// of an awk run's time (`REVIEW_REPORT.md` TXT-16), and that rule matched only a
/// prefix, so `9abc` compared as the number 9.
pub(crate) fn is_numeric_string(text: &str) -> bool {
    match scan_number(text) {
        NumberText::Special(_) => true,
        NumberText::Decimal { whole, .. } => whole,
        NumberText::Nothing => false,
    }
}

struct GlobalEnv {
    convfmt: AwkString,
    fs: FieldSeparator,
    ofs: AwkString,
    ors: AwkString,
    ofmt: AwkString,
    rs: RecordSeparator,
    nr: u32,
    fnr: u32,
    nf: usize,
    /// Cached FS combined with newline for paragraph mode (RS="").
    /// Invalidated when FS or RS changes.
    paragraph_fs_cache: Option<FieldSeparator>,
    /// Hide the CR of CRLF-terminated input records (default line-ending
    /// mode). False when CR is ordinary data for the whole run: the program
    /// mentions a carriage return, or `CASH_EOL=lf` is set.
    strip_cr: bool,
    /// Whether the most recent main-input record (the current one; in END,
    /// the last one read) ended in CRLF. `print` then writes a default ORS
    /// ("\n") as "\r\n" so CRLF files round-trip unchanged.
    last_record_crlf: bool,
    /// gawk's IGNORECASE: regexes, `index` and the comparison of strings ignore case.
    ignore_case: bool,
}

impl GlobalEnv {
    fn set(&mut self, var: SpecialVar, value: &mut AwkValue) -> Result<(), String> {
        let as_string = |value: &mut AwkValue| value.clone().scalar_to_string(&self.convfmt);
        match var {
            SpecialVar::Convfmt => self.convfmt = as_string(value)?,
            SpecialVar::Fs => {
                self.fs = as_string(value)?.try_into()?;
                self.paragraph_fs_cache = None;
            }
            SpecialVar::Ofmt => self.ofmt = as_string(value)?,
            SpecialVar::Ofs => self.ofs = as_string(value)?,
            SpecialVar::Ors => self.ors = as_string(value)?,
            SpecialVar::Rs => {
                self.rs = as_string(value)?.try_into()?;
                self.paragraph_fs_cache = None;
            }
            SpecialVar::Nr => self.nr = value.scalar_as_f64() as u32,
            SpecialVar::Fnr => self.fnr = value.scalar_as_f64() as u32,
            // gawk's error; a negative NF was taken as 0.
            SpecialVar::Nf if value.scalar_as_f64().trunc() < 0.0 => {
                return Err("NF set to negative value".to_string());
            }
            SpecialVar::Nf => self.nf = value.scalar_as_f64() as usize,
            // gawk's: on when the value is true, `"0"` too, as a string is.
            SpecialVar::IgnoreCase => {
                self.ignore_case = value.scalar_as_bool();
                crate::regex::set_ignore_case(self.ignore_case);
            }
            _ => {
                // not needed
            }
        }

        Ok(())
    }
}

impl Default for GlobalEnv {
    fn default() -> Self {
        Self {
            convfmt: AwkString::from("%.6g"),
            fs: FieldSeparator::Default,
            ofs: AwkString::from(" "),
            ors: AwkString::from("\n"),
            ofmt: AwkString::from("%.6g"),
            rs: RecordSeparator::Char(b'\n'),
            nr: 1,
            fnr: 1,
            nf: 0,
            paragraph_fs_cache: None,
            strip_cr: true,
            last_record_crlf: false,
            ignore_case: false,
        }
    }
}

impl GlobalEnv {
    /// Returns the effective FS for the current record. In paragraph mode
    /// (RS=""), newline is always a field separator per POSIX, so the FS is
    /// combined with `\n`. The result is cached and only recomputed when
    /// FS or RS changes.
    fn effective_fs(&mut self) -> Result<&FieldSeparator, String> {
        if !matches!(self.rs, RecordSeparator::Null) {
            return Ok(&self.fs);
        }
        if let Some(ref cached) = self.paragraph_fs_cache {
            return Ok(cached);
        }
        let combined = match &self.fs {
            FieldSeparator::Default | FieldSeparator::Null => None,
            FieldSeparator::Char(c) => {
                let escaped = ere_escape_char(*c as char);
                let pattern = format!("\n|{}", escaped);
                Some(FieldSeparator::Ere(Rc::new(Regex::dynamic(&pattern)?)))
            }
            FieldSeparator::Ere(re) => {
                let pattern = format!("\n|{}", re.pattern());
                Some(FieldSeparator::Ere(Rc::new(Regex::dynamic(&pattern)?)))
            }
        };
        match combined {
            Some(fs) => Ok(self.paragraph_fs_cache.insert(fs)),
            None => Ok(&self.fs),
        }
    }
}

/// The argument for a value `value` of the element `index` of the array at `place`, to
/// a sort comparison function: the element itself when it is a subarray, else the value.
fn sort_argument(
    place: Option<&(*mut AwkValue, Vec<array::Key>)>,
    index: &array::Key,
    value: &AwkValue,
) -> StackValue {
    match (place, &value.value) {
        (Some((root, keys)), AwkValueVariant::Array(_)) => {
            StackValue::ArrayElementRef(stack::ArrayElementRef {
                array: *root,
                path: (!keys.is_empty()).then(|| keys.as_slice().into()),
                key: index.clone(),
            })
        }
        _ => StackValue::from(value.clone()),
    }
}

/// What running code needs besides the stack, for a builtin that runs a function.
struct CodeToRun<'a, 'r> {
    functions: &'a [Function],
    record: &'r Record,
    input: &'r mut MainInput,
}

struct Interpreter {
    globals: Vec<AwkValueRef>,
    /// The globals' names, by index, for errors
    global_names: Vec<Rc<str>>,
    constants: Vec<Constant>,
    write_files: WriteFiles,
    read_files: ReadFiles,
    write_pipes: WritePipes,
    read_pipes: ReadPipes,
    rand_seed: u64,
    rng: SmallRng,
    /// The program's file and the line of the instruction that ran last, for an error
    /// met outside the program's code; none before any has run.
    last_location: Option<(Rc<str>, u32)>,
}

impl Interpreter {
    /// The error that stopped `stack`'s code, as gawk reports a fatal one:
    /// `` awk: cmd. line:3: (FILENAME=data FNR=7) fatal: attempt to use scalar `x' as an
    /// array ``, the program's file in place of `cmd. line` when it has one, and the input
    /// place once a record has been read. cash wrote `runtime error:`, its own words for
    /// some errors, and the call stack.
    ///
    /// An instruction pointer can be past the last instruction it has a location for (a
    /// `return` from inside a loop leaves it there), so the last location stands in, and
    /// none leaves the place out, rather than be indexed for: the reporter panicked and
    /// hid the error it was reporting (`REVIEW_REPORT.md` TXT-24).
    fn fatal_message(&self, error: String, stack: &Stack) -> String {
        let ip = usize::try_from(stack.ip).ok();
        let error = match error.as_str() {
            stack::SCALAR_IN_ARRAY_CONTEXT => {
                let name = match &stack.error_place {
                    Some(Place::Variable(variable)) => self.scalar_name(*variable, stack),
                    Some(Place::Element(element)) => self
                        .element_name(element, stack)
                        .map(|name| format!("`{name}'")),
                    None => None,
                }
                .or_else(|| {
                    ip.and_then(|ip| stack.array_names.get(ip))
                        .and_then(|name| name.as_deref().map(str::to_string))
                });
                match name {
                    Some(name) => format!("attempt to use scalar {name} as an array"),
                    None => "attempt to use a scalar value as array".to_string(),
                }
            }
            stack::ARRAY_IN_SCALAR_CONTEXT => {
                let name = match &stack.error_place {
                    Some(Place::Variable(variable)) => self.array_name(*variable, stack),
                    Some(Place::Element(element)) => {
                        self.element_name(element, stack).map(|root| {
                            passed_name(stack, root, |slot| {
                                matches!(slot, StackValue::ArrayElementRef(e) if e == element)
                            })
                        })
                    }
                    None => None,
                };
                match name {
                    Some(name) => format!("attempt to use array `{name}' in a scalar context"),
                    None => "attempt to use array in a scalar context".to_string(),
                }
            }
            _ => error,
        };

        let location = ip
            .and_then(|ip| stack.source_locations.get(ip))
            .or(stack.source_locations.last())
            .map(|location| (stack.current_function_file.clone(), location.line));
        self.placed("fatal", &error, location.as_ref())
    }

    /// An error that stopped awk outside the program's code, as gawk reports one: placed
    /// at the code that ran last, when some has (`awk: cmd. line:2: fatal: cannot open
    /// file ...`), else bare (`awk: fatal: ...`). It was always bare.
    fn outside_error(&self, error: &str) -> String {
        self.placed("fatal", error, self.last_location.as_ref())
    }

    /// `error` as gawk reports a fatal one: `awk: `, the program's file (or `cmd. line`)
    /// and the line of `location`, the input place once a record has been read, `fatal: `.
    fn placed(&self, kind: &str, error: &str, location: Option<&(Rc<str>, u32)>) -> String {
        let mut message = String::from("awk: ");
        if let Some((file, line)) = location {
            let file = if file.is_empty() { "cmd. line" } else { file };
            let _ = write!(message, "{file}:{line}: ");
        }
        // SAFETY: a global's cell lives as long as the interpreter, and no reference to
        // one is held between instructions, nor once the code has stopped.
        let fnr = unsafe { &*self.globals[SpecialVar::Fnr as usize].get() }.clone();
        if fnr.scalar_as_f64() > 0.0 {
            // SAFETY: as above.
            let filename = unsafe { &*self.globals[SpecialVar::Filename as usize].get() }
                .clone()
                .scalar_to_string("%.6g")
                .unwrap_or_default();
            let _ = write!(
                message,
                "(FILENAME={filename} FNR={}) ",
                fnr.scalar_as_f64()
            );
        }
        let _ = write!(message, "{kind}: {error}");
        message
    }

    /// Writes gawk's warning `text`, placed at the current instruction of `stack` as an
    /// error is, after what awk has printed.
    fn warn(&self, text: &str, stack: &Stack) {
        let _ = io::flush_stdout();
        eprintln!(
            "{}",
            self.placed("warning", text, stack.location().as_ref())
        );
    }

    /// `log`, `sqrt` and `exp`, with gawk's warnings for an argument out of their range: a
    /// negative one for the first two, one whose result overflows or underflows to 0 for
    /// `exp`. The result was given in silence.
    fn math_builtin(&self, function: BuiltinFunction, stack: &mut Stack) -> Result<(), String> {
        let value = stack.pop_scalar_value()?.scalar_as_f64();
        let as_g = |number: f64| {
            sprintf("%g", &mut [number.into()], "%.6g")
                .map(|text| text.to_string())
                .unwrap_or_default()
        };
        let (result, name) = match function {
            BuiltinFunction::Log => (value.ln(), "log"),
            BuiltinFunction::Sqrt => (value.sqrt(), "sqrt"),
            _ => (value.exp(), "exp"),
        };
        if name == "exp" {
            if value.is_finite() && (result.is_infinite() || result == 0.0) {
                let argument = as_g(value);
                self.warn(&format!("exp: argument {argument} is out of range"), stack);
            }
        } else if value < 0.0 {
            let argument = as_g(value);
            self.warn(
                &format!("{name}: received negative argument {argument}"),
                stack,
            );
        }
        stack.push_value(result)
    }

    /// The global that `variable` is, if it is one.
    fn global_name(&self, variable: *const AwkValue) -> Option<Rc<str>> {
        let index = self
            .globals
            .iter()
            .position(|global| global.get().cast_const() == variable)?;
        self.global_names
            .get(index)
            .filter(|name| !name.is_empty())
            .cloned()
    }

    /// How gawk names the scalar `variable` used as an array: `` `x' ``, or
    /// `` parameter `p' ``.
    fn scalar_name(&self, variable: *const AwkValue, stack: &Stack) -> Option<String> {
        if let Some((index, _)) = stack.local_holding(variable) {
            let name = stack.parameter_names.get(index)?;
            return Some(format!("parameter `{name}'"));
        }
        Some(format!("`{}'", self.global_name(variable)?))
    }

    /// How gawk names the array `variable` read as a scalar: `a`, or `p (from a)` for a
    /// parameter that is the caller's array, `t (from s, from a)` when it was passed on.
    fn array_name(&self, variable: *mut AwkValue, stack: &Stack) -> Option<String> {
        let root = self
            .global_name(variable)
            .or_else(|| local_name(variable, stack))?;
        Some(passed_name(
            stack,
            root.to_string(),
            |slot| matches!(slot, StackValue::ValueRef(p) | StackValue::UninitializedRef(p) if *p == variable),
        ))
    }

    /// How gawk names an element: its variable's name and its keys, `a["1"]["x"]`.
    fn element_name(&self, element: &stack::ArrayElementRef, stack: &Stack) -> Option<String> {
        let mut name = self
            .global_name(element.array)
            .or_else(|| local_name(element.array, stack))?
            .to_string();
        for key in element.keys() {
            let _ = write!(name, "[\"{key}\"]");
        }
        Some(name)
    }
}

/// The local array `variable` is, in the current function or a caller, by name.
fn local_name(variable: *mut AwkValue, stack: &Stack) -> Option<Rc<str>> {
    stack.frames().find_map(|frame| {
        stack.parameter_where(
            frame,
            |slot| matches!(slot, StackValue::Value(cell) if cell.get() == variable),
        )
    })
}

/// `root`, an array's name, as gawk gives it when the array reached the current function
/// through parameters, which `refers` is true of: `t (from s, from a)`, innermost first.
fn passed_name(stack: &Stack, root: String, refers: impl Fn(&StackValue) -> bool) -> String {
    let names: Vec<Rc<str>> = stack
        .frames()
        .map_while(|frame| stack.parameter_where(frame, &refers))
        .collect();
    match names.split_first() {
        None => root,
        Some((first, rest)) => {
            let rest = rest.iter().fold(String::new(), |mut rest, name| {
                let _ = write!(rest, "{name}, from ");
                rest
            });
            format!("{first} (from {rest}{root})")
        }
    }
}

impl Interpreter {
    /// Close `name` in every I/O table (a name may have been opened for both
    /// reading and writing). POSIX: close shall return 0 if the close was
    /// successful and non-zero otherwise (e.g. the name was not open). Surface
    /// any error status, else 0, else -1 when nothing matched.
    fn close_streams(&mut self, name: &str) -> i32 {
        let results = [
            self.write_files.close_file(name),
            self.read_files.close_file(name),
            self.write_pipes.close_pipe(name),
            self.read_pipes.close_pipe(name),
        ];
        if results.iter().all(Option::is_none) {
            -1
        } else {
            results
                .iter()
                .flatten()
                .copied()
                .find(|&s| s != 0)
                .unwrap_or(0)
        }
    }

    /// Increment a special counter global (NR or FNR) by one, keeping its
    /// `AwkValue` and the cached `global_env` copy in sync via `assign`.
    /// Borrowing a global mutably while `global_env` is also borrowed mutably
    /// breaks the stacked-borrows rules, so the cell is reached via a raw pointer.
    /// Counts a record read from the main input, by the main loop or by `getline`.
    fn count_record(&mut self, global_env: &mut GlobalEnv) -> Result<(), String> {
        self.bump_counter(SpecialVar::Nr, global_env)?;
        self.bump_counter(SpecialVar::Fnr, global_env)
    }

    fn bump_counter(&mut self, var: SpecialVar, global_env: &mut GlobalEnv) -> Result<(), String> {
        let ptr = self.globals[var as usize].get();
        // SAFETY: a global's cell lives as long as the interpreter, and nothing else
        // refers to NR or FNR while the record is counted.
        let next = unsafe { (*ptr).scalar_as_f64() } + 1.0;
        // SAFETY: as above.
        unsafe { &mut *ptr }.assign(next, global_env)?;
        Ok(())
    }

    /// Execute a `CallBuiltin` opcode: dispatch to the I/O, getline, and other
    /// builtins that need interpreter state (`self`), the current input file, or
    /// the operand stack. Returns the resulting [`FieldsState`] so the caller can
    /// recompute `$0`/fields when a getline assignment changed them; builtins
    /// that don't touch fields return [`FieldsState::Ok`]. Builtins that need
    /// none of this state are delegated to [`call_simple_builtin`].
    #[expect(
        clippy::too_many_arguments,
        reason = "a builtin may run code: `asort`'s comparison function"
    )]
    fn call_builtin<'a>(
        &mut self,
        function: BuiltinFunction,
        argc: u16,
        stack: &mut Stack<'a, 'a>,
        global_env: &mut GlobalEnv,
        input: &mut MainInput,
        functions: &'a [Function],
        record: &Record,
    ) -> Result<FieldsState, String> {
        let mut fields_state = FieldsState::Ok;
        match function {
            BuiltinFunction::Asort | BuiltinFunction::Asorti => {
                let mut code = CodeToRun {
                    functions,
                    record,
                    input,
                };
                self.sort_array(function, argc, stack, global_env, &mut code)?;
            }
            BuiltinFunction::Gensub => {
                let target = stack
                    .pop_scalar_value()?
                    .scalar_to_string(&global_env.convfmt)?;
                let how = stack.pop_scalar_value()?;
                let repl = stack
                    .pop_scalar_value()?
                    .scalar_to_string(&global_env.convfmt)?;
                let ere = stack.pop_scalar_value()?.into_ere(&global_env.convfmt)?;
                // gawk's: a `how` that starts with `g` or `G` replaces every match, a number
                // the match it counts to, and one not above 0 is the first, with a warning.
                let text = how.clone().scalar_to_string(&global_env.convfmt)?;
                let only = if text.starts_with(['g', 'G']) {
                    None
                } else {
                    let number = how.scalar_as_f64();
                    if number <= 0.0 {
                        self.warn(
                            &format!("gensub: third argument `{text}' treated as 1"),
                            stack,
                        );
                    }
                    // A cast saturates: 1e300 counts to a match there is none of.
                    Some((number as usize).max(1))
                };
                stack.push_value(builtins::gensub(&ere, &repl, &target, only))?;
            }
            BuiltinFunction::Match => {
                let array = if argc == 3 {
                    Some(stack.pop().ok_or_else(|| "empty stack".to_string())?)
                } else {
                    None
                };
                let (start, len, groups) = builtin_match(stack, global_env, array.is_some())?;
                if let Some(array) = array {
                    // gawk's: `arr[n]` the text of group `n` (0 the whole match), and
                    // `arr[n, "start"]` and `arr[n, "length"]` its place.
                    // SAFETY: a global's cell lives as long as the interpreter.
                    let subsep = unsafe { &*self.globals[SpecialVar::Subsep as usize].get() }
                        .clone()
                        .scalar_to_string(&global_env.convfmt)?;
                    let array = stack
                        .resolve_array_to_empty(array, true)
                        .map_err(|error| builtins::not_an_array(error, "match: third argument"))?;
                    array.clear();
                    for group in groups {
                        let number = group.number;
                        array.set(number.to_string(), maybe_numeric_string(group.text))?;
                        array.set(format!("{number}{subsep}start"), group.start)?;
                        array.set(format!("{number}{subsep}length"), group.length)?;
                    }
                }
                // Update via `assign` so RSTART/RLENGTH keep their
                // `SpecialGlobalVar` ref_type; reach the cells through raw
                // pointers because borrowing `self.globals` mutably would break
                // the stacked borrows rules.
                // SAFETY: a global's cell lives as long as the interpreter, and nothing
                // else refers to RSTART while `match` returns.
                unsafe { &mut *self.globals[SpecialVar::Rstart as usize].get() }
                    .assign(start, global_env)?;
                // SAFETY: as above, for RLENGTH.
                unsafe { &mut *self.globals[SpecialVar::Rlength as usize].get() }
                    .assign(len, global_env)?;
            }
            BuiltinFunction::RedirectedPrintfTruncate
            | BuiltinFunction::RedirectedPrintfAppend
            | BuiltinFunction::RedirectedPrintTruncate
            | BuiltinFunction::RedirectedPrintAppend => {
                let filename = stack
                    .pop_scalar_value()?
                    .scalar_to_string(&global_env.convfmt)?;
                let is_append = matches!(
                    function,
                    BuiltinFunction::RedirectedPrintfAppend
                        | BuiltinFunction::RedirectedPrintAppend
                );
                no_null_redirection(&filename, if is_append { ">>" } else { ">" })?;
                let is_printf = matches!(
                    function,
                    BuiltinFunction::RedirectedPrintfTruncate
                        | BuiltinFunction::RedirectedPrintfAppend
                );
                let str = if is_printf {
                    builtin_sprintf(stack, argc - 1, global_env)?
                } else {
                    print_to_string(stack, argc - 1, global_env)?
                };
                self.write_files.write(&filename, &str, is_append)?;
            }
            BuiltinFunction::RedirectedPrintPipe | BuiltinFunction::RedirectedPrintfPipe => {
                let command = stack
                    .pop_scalar_value()?
                    .scalar_to_string(&global_env.convfmt)?;
                no_null_redirection(&command, "|")?;
                let str = if function == BuiltinFunction::RedirectedPrintPipe {
                    print_to_string(stack, argc - 1, global_env)?
                } else {
                    builtin_sprintf(stack, argc - 1, global_env)?
                };
                self.write_pipes.write(command, str)?;
            }
            BuiltinFunction::Close => {
                // gawk's second argument closes one end of a two-way pipe, which cash does
                // not have; for anything else, gawk closes it whole, as here.
                if argc == 2 {
                    let how = stack
                        .pop_scalar_value()?
                        .scalar_to_string(&global_env.convfmt)?;
                    let how = how.to_ascii_lowercase();
                    if how != "to" && how != "from" {
                        return Err("close: second argument must be `to' or `from'".to_string());
                    }
                }
                let filename = stack
                    .pop_scalar_value()?
                    .scalar_to_string(&global_env.convfmt)?;
                stack.push_value(self.close_streams(&filename) as f64)?;
            }
            BuiltinFunction::FFlush => {
                let expr_str = if argc == 1 {
                    stack
                        .pop_scalar_value()?
                        .scalar_to_string(&global_env.convfmt)?
                } else {
                    AwkString::default()
                };
                // Standard output that cannot be written is fatal, as in gawk. A file is
                // flushed if it is open, or a pipe; a name that is neither is gawk's
                // warning. It needed to be both a file and a pipe, so `fflush(file)`
                // failed.
                let result = if expr_str.is_empty() {
                    io::flush_stdout_for_fflush()?;
                    self.write_files.flush_all() && self.write_pipes.flush_all()
                } else if io::SpecialFile::named(&expr_str) == Some(io::SpecialFile::Stdout) {
                    io::flush_stdout_for_fflush()?;
                    true
                } else if io::SpecialFile::named(&expr_str) == Some(io::SpecialFile::Stderr) {
                    true
                } else if self.write_files.is_open(&expr_str) {
                    self.write_files.flush_file(&expr_str)
                } else if self.write_pipes.is_open(&expr_str) {
                    self.write_pipes.flush_file(&expr_str)
                } else {
                    self.warn(
                        &format!("fflush: `{expr_str}' is not an open file, pipe or co-process"),
                        stack,
                    );
                    false
                };
                stack.push_value(if result { 0.0 } else { -1.0 })?;
            }
            BuiltinFunction::GetLine => {
                let place = stack.location();
                let var = stack.pop_scalar_ref()?;
                if let Some((next_record, crlf)) =
                    input.next_record(&mut self.globals, global_env, place.as_ref())?
                {
                    // Main input: this record now decides how `print` ends lines.
                    global_env.last_record_crlf = crlf;
                    fields_state = var.assign(maybe_numeric_string(next_record), global_env)?;
                    self.count_record(global_env)?;
                    stack.push_value(1.0)?;
                } else {
                    stack.push_value(0.0)?;
                }
            }
            BuiltinFunction::GetLineFromFile | BuiltinFunction::GetLineFromPipe => {
                let filename = stack
                    .pop_scalar_value()?
                    .scalar_to_string(&global_env.convfmt)?;
                let is_file = function == BuiltinFunction::GetLineFromFile;
                no_null_redirection(&filename, if is_file { "<" } else { "|" })?;
                let var = stack.pop_scalar_ref()?;
                let maybe_next_record = if is_file {
                    self.read_files
                        .read_next_record(filename, &global_env.rs, global_env.strip_cr)
                } else {
                    self.read_pipes
                        .read_next_record(filename, &global_env.rs, global_env.strip_cr)
                };
                // `getline <file` and `cmd | getline` hide a CRLF's CR like
                // main input does, but they are not main input, so they do not
                // change how `print` ends lines.
                match maybe_next_record {
                    Ok(Some((next_record, _))) => {
                        fields_state = var.assign(maybe_numeric_string(next_record), global_env)?;
                        // `cmd | getline` advances NR (but not FNR), like
                        // historical awk; `getline < file` touches neither.
                        if function == BuiltinFunction::GetLineFromPipe {
                            self.bump_counter(SpecialVar::Nr, global_env)?;
                        }
                        stack.push_value(1.0)?;
                    }
                    Ok(None) => {
                        stack.push_value(0.0)?;
                    }
                    Err(_) => {
                        // Per POSIX, getline returns -1 on error
                        stack.push_value(-1.0)?;
                    }
                }
            }
            BuiltinFunction::Log | BuiltinFunction::Sqrt | BuiltinFunction::Exp => {
                self.math_builtin(function, stack)?;
            }
            BuiltinFunction::Rand => {
                let rand = self.rng.random_range(0.0..1.0);
                stack.push_value(rand)?;
            }
            BuiltinFunction::Srand => {
                let seed = if argc == 1 {
                    stack.pop_scalar_value()?.scalar_as_f64() as u64
                } else {
                    SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                };
                stack.push_value(self.rand_seed as f64)?;
                self.rand_seed = seed;
                self.rng = SmallRng::seed_from_u64(self.rand_seed);
            }
            other => fields_state = call_simple_builtin(other, argc, stack, global_env)?,
        }
        Ok(fields_state)
    }

    fn run(
        &mut self,
        action: &Action,
        functions: &[Function],
        record: &mut Record,
        stack: &mut [StackValue],
        global_env: &mut GlobalEnv,
        input: &mut MainInput,
    ) -> Result<ExecutionResult, String> {
        let mut stack = Stack::new(action, stack);
        let result = self.run_internal(functions, record, &mut stack, global_env, input);
        if let Some(location) = usize::try_from(stack.last_ip)
            .ok()
            .and_then(|ip| stack.source_locations.get(ip))
        {
            self.last_location = Some((stack.current_function_file.clone(), location.line));
        }
        result.map_err(|err| self.fatal_message(err, &stack))
    }

    /// The value the function `function` returns for `arguments`, run from a builtin.
    fn call_from_builtin<'a>(
        &mut self,
        function: &'a Function,
        arguments: Vec<StackValue>,
        stack: &mut Stack<'a, 'a>,
        global_env: &mut GlobalEnv,
        code: &mut CodeToRun<'a, '_>,
    ) -> Result<AwkValue, String> {
        let depth = stack.call_frames.len();
        let parameters = function.parameters_count;
        let given = arguments.len();
        for argument in arguments.into_iter().take(parameters) {
            // SAFETY: an argument is a value, or an element of the array being sorted,
            // whose variable outlives the call.
            unsafe { stack.push(argument)? };
        }
        for _ in given..parameters {
            stack.push_value(AwkValue::uninitialized())?;
        }
        stack.call_function(function)?;
        let outer = stack.stop_depth.replace(depth);
        let result = self.run_internal(code.functions, code.record, stack, global_env, code.input);
        stack.stop_depth = outer;
        match result? {
            ExecutionResult::Expression(value) => Ok(value),
            _ => Err("a sort comparison function ended the program".to_string()),
        }
    }

    /// gawk's `asort(src [, dest [, how]])` and `asorti`: `dest` (or `src`) becomes the
    /// values (or the indices) of `src` sorted as `how` says, at indices 1 to n; n is
    /// pushed. `how` is one of gawk's orders (`@val_type_asc` for `asort`, `@ind_str_asc`
    /// for `asorti`, by default) or a function of the program's that compares two
    /// elements.
    fn sort_array<'a>(
        &mut self,
        function: BuiltinFunction,
        argc: u16,
        stack: &mut Stack<'a, 'a>,
        global_env: &mut GlobalEnv,
        code: &mut CodeToRun<'a, '_>,
    ) -> Result<(), String> {
        let name = if function == BuiltinFunction::Asort {
            "asort"
        } else {
            "asorti"
        };
        let how = if argc == 3 {
            Some(
                stack
                    .pop_scalar_value()?
                    .scalar_to_string(&global_env.convfmt)?,
            )
        } else {
            None
        };
        let dest = if argc >= 2 {
            Some(stack.pop().ok_or_else(|| "empty stack".to_string())?)
        } else {
            None
        };
        let source = stack.pop().ok_or_else(|| "empty stack".to_string())?;
        let first = |error| builtins::not_an_array(error, &format!("{name}: first argument"));
        let second = |error| builtins::not_an_array(error, &format!("{name}: second argument"));
        // A subarray goes to a comparison function as itself, the element of the array
        // being sorted, as in gawk; it went as a copy.
        let source_place = stack::array_place(&source);
        let mut elements: Vec<(array::Key, AwkValue)> = {
            let array = stack
                .resolve_array(source.duplicate_place(), true)
                .map_err(first)?;
            array
                .key_iter()
                .map(|key| {
                    let value = array.get_value(key.clone()).map(|value| value.clone());
                    value.map(|value| (key, value))
                })
                .collect::<Result<_, _>>()?
        };
        if let Some(dest) = &dest
            && how.is_none()
            && stack::array_place(&source).is_some()
            && stack::array_place(&source) == stack::array_place(dest)
        {
            self.warn(
                "asort/asorti: using the same array as source and destination without a \
                 third argument is silly.",
                stack,
            );
        }
        let default = if function == BuiltinFunction::Asort {
            "@val_type_asc"
        } else {
            "@ind_str_asc"
        };
        let how = how.as_ref().map_or(default, |how| how.as_str());
        match builtins::SortOrder::named(how) {
            Some(order) => order.sort(&mut elements, &global_env.convfmt, global_env.ignore_case),
            None => {
                let compare = code
                    .functions
                    .iter()
                    .find(|candidate| &*candidate.name == how)
                    .ok_or_else(|| format!("sort comparison function `{how}' is not defined"))?;
                // Each element goes after those it does not come before, found by halves,
                // as the function says; a comparison can fail, which `sort_by` has no room
                // for. Equal elements keep their order.
                let mut sorted: Vec<(array::Key, AwkValue)> = Vec::with_capacity(elements.len());
                for element in elements {
                    let (mut low, mut high) = (0, sorted.len());
                    while low < high {
                        let middle = usize::midpoint(low, high);
                        let (index, value) = &sorted[middle];
                        let arguments = vec![
                            StackValue::from(AwkValue::from(index.to_string())),
                            sort_argument(source_place.as_ref(), index, value),
                            StackValue::from(AwkValue::from(element.0.to_string())),
                            sort_argument(source_place.as_ref(), &element.0, &element.1),
                        ];
                        let order = self
                            .call_from_builtin(compare, arguments, stack, global_env, code)?
                            .scalar_as_f64();
                        if order > 0.0 {
                            high = middle;
                        } else {
                            low = middle + 1;
                        }
                    }
                    sorted.insert(low, element);
                }
                elements = sorted;
            }
        }
        let count = elements.len();
        let target = dest.unwrap_or(source);
        let target = stack.resolve_array_to_empty(target, true).map_err(second)?;
        target.clear();
        for (index, (key, value)) in elements.into_iter().enumerate() {
            let value = if function == BuiltinFunction::Asort {
                value
            } else {
                AwkValue::from(key.to_string())
            };
            target.set((index + 1).to_string(), value)?;
        }
        stack.push_value(count as f64)
    }

    fn run_internal<'a>(
        &mut self,
        functions: &'a [Function],
        record: &Record,
        stack: &mut Stack<'a, 'a>,
        global_env: &mut GlobalEnv,
        input: &mut MainInput,
    ) -> Result<ExecutionResult, String> {
        // # Safety
        // To meat the requirements of stacked borrows (as checked by miri),
        // the `globals` member of `Interpreter` cannot be
        // borrowed mutably, otherwise dereferencing the pointers
        // to global values in the stack would be unsound.
        let mut fields_state = FieldsState::Ok;
        while let Some(instruction) = stack.next_instruction() {
            stack.last_ip = stack.ip;
            let mut ip_increment: isize = 1;
            match instruction {
                OpCode::Add => {
                    numeric_op!(stack, +);
                }
                OpCode::Sub => {
                    numeric_op!(stack,  -);
                }
                OpCode::Mul => {
                    numeric_op!(stack,  *);
                }
                OpCode::Div | OpCode::DivAssign => {
                    let rhs = stack.pop_scalar_value()?.scalar_as_f64();
                    let lhs = stack.pop_scalar_value()?.scalar_as_f64();
                    if rhs == 0.0 {
                        return Err(division_by_zero(instruction));
                    }
                    stack.push_value(lhs / rhs)?;
                }
                OpCode::Mod | OpCode::ModAssign => {
                    let rhs = stack.pop_scalar_value()?.scalar_as_f64();
                    let lhs = stack.pop_scalar_value()?.scalar_as_f64();
                    if rhs == 0.0 {
                        return Err(division_by_zero(instruction));
                    }
                    stack.push_value(lhs % rhs)?;
                }
                OpCode::Pow => {
                    let rhs = stack.pop_scalar_value()?.scalar_as_f64();
                    let lhs = stack.pop_scalar_value()?.scalar_as_f64();
                    stack.push_value(lhs.powf(rhs))?;
                }
                OpCode::Le => {
                    compare_op!(stack, &global_env.convfmt, global_env.ignore_case, <=);
                }
                OpCode::Lt => {
                    compare_op!(stack, &global_env.convfmt, global_env.ignore_case, <);
                }
                OpCode::Ge => {
                    compare_op!(stack, &global_env.convfmt, global_env.ignore_case, >=);
                }
                OpCode::Gt => {
                    compare_op!(stack, &global_env.convfmt, global_env.ignore_case, >);
                }
                OpCode::Eq => {
                    compare_op!(stack, &global_env.convfmt, global_env.ignore_case, ==);
                }
                OpCode::Ne => {
                    compare_op!(stack, &global_env.convfmt, global_env.ignore_case, !=);
                }
                OpCode::Match => {
                    let ere = stack.pop_scalar_value()?.into_ere(&global_env.convfmt)?;
                    let string = stack
                        .pop_scalar_value()?
                        .scalar_to_string(&global_env.convfmt)?;
                    let result = ere.matches(&string);
                    stack.push_value(bool_to_f64(result))?;
                }
                OpCode::Concat => {
                    let rhs = stack
                        .pop_scalar_value()?
                        .scalar_to_string(&global_env.convfmt)?;
                    let mut lhs = stack
                        .pop_scalar_value()?
                        .scalar_to_string(&global_env.convfmt)?;
                    lhs.concat(&rhs);
                    stack.push_value(lhs)?;
                }
                OpCode::In => {
                    let key = stack
                        .pop_scalar_value()?
                        .scalar_to_string(&global_env.convfmt)?;
                    // gawk does not name a subarray that is a scalar here.
                    let array = stack.pop_array(false)?;
                    let result = array.contains(&key);
                    stack.push_value(bool_to_f64(result))?;
                }
                OpCode::Negate => {
                    let value = stack.pop_scalar_value()?.scalar_as_f64();
                    stack.push_value(-value)?;
                }
                OpCode::Not => {
                    let value = stack.pop_scalar_value()?.scalar_as_bool();
                    stack.push_value(bool_to_f64(!value))?;
                }
                OpCode::PostInc => {
                    let lvalue = stack.pop_scalar_ref()?;
                    let expr_result = lvalue.scalar_as_f64();
                    fields_state = lvalue.assign(expr_result + 1.0, global_env)?;
                    stack.push_value(expr_result)?;
                }
                OpCode::PostDec => {
                    let lvalue = stack.pop_scalar_ref()?;
                    let expr_result = lvalue.scalar_as_f64();
                    fields_state = lvalue.assign(expr_result - 1.0, global_env)?;
                    stack.push_value(expr_result)?;
                }
                OpCode::PreInc => {
                    let lvalue = stack.pop_scalar_ref()?;
                    let expr_result = lvalue.scalar_as_f64() + 1.0;
                    fields_state = lvalue.assign(expr_result, global_env)?;
                    stack.push_value(expr_result)?;
                }
                OpCode::PreDec => {
                    let lvalue = stack.pop_scalar_ref()?;
                    let expr_result = lvalue.scalar_as_f64() - 1.0;
                    fields_state = lvalue.assign(expr_result, global_env)?;
                    stack.push_value(expr_result)?;
                }
                OpCode::CreateIterator => {
                    // The array, then the loop's variable, which is not given a type until
                    // a key is assigned to it: `for (a in b)` with `a` an array is an
                    // error only when `b` has a key, as in gawk; it was always one.
                    let array = match stack.pop().ok_or_else(|| "empty stack".to_string())? {
                        StackValue::ValueRef(array) | StackValue::UninitializedRef(array) => {
                            // SAFETY: a variable outlives the loop that goes through it.
                            unsafe { &mut *array }.as_array()?;
                            Place::Variable(array)
                        }
                        StackValue::ArrayElementRef(element) => {
                            // gawk does not name a subarray that is a scalar here.
                            let ptr = stack.element_ptr(&element)?;
                            // SAFETY: the element is in its variable's array.
                            unsafe { &mut *ptr }.as_array().inspect_err(|_| {
                                stack.error_place = None;
                            })?;
                            Place::Element(element)
                        }
                        // A variable that holds a scalar, named from the code.
                        _ => return Err(stack::SCALAR_IN_ARRAY_CONTEXT.to_string()),
                    };
                    let iter_var = match stack.pop().ok_or_else(|| "empty stack".to_string())? {
                        StackValue::ValueRef(variable) => {
                            // One with no type yet becomes a scalar, as in gawk; an array
                            // stays one, `for (b in b)` included.
                            // SAFETY: a variable outlives the loop that assigns to it.
                            let value = unsafe { &mut *variable };
                            if matches!(value.value, AwkValueVariant::Uninitialized) {
                                value.value = AwkValueVariant::UninitializedScalar;
                            }
                            Place::Variable(variable)
                        }
                        StackValue::ArrayElementRef(element) => Place::Element(element),
                        _ => return Err("expected lvalue".to_string()),
                    };
                    let key_iter = match &array {
                        // SAFETY: as above.
                        Place::Variable(array) => unsafe { &mut **array }.as_array()?.key_iter(),
                        Place::Element(element) => {
                            let ptr = stack.element_ptr(element)?;
                            // SAFETY: as above.
                            unsafe { &mut *ptr }.as_array()?.key_iter()
                        }
                    };
                    // SAFETY: the variable or element the iterator assigns to is of a
                    // variable that stays valid while the values below it are on the stack
                    // (`push`).
                    unsafe {
                        stack.push(StackValue::Iterator(ArrayIterator { iter_var, key_iter }))?
                    };
                }
                OpCode::AdvanceIterOrJump(offset) => {
                    // if the top of the stack is not an iterator
                    // the code is malformed
                    let mut iter = stack
                        .pop()
                        .ok_or_else(|| "empty stack".to_string())?
                        .unwrap_array_iterator()?;
                    // The keys the array had when the loop began, whatever has become of
                    // the array since (`array::KeyIterator`).
                    if let Some(key) = iter.key_iter.next() {
                        let variable = match &iter.iter_var {
                            Place::Variable(variable) => {
                                // SAFETY: the iterator's variable stays valid while the
                                // values below it are on the stack (`push`), and it was just
                                // popped.
                                let value = unsafe { &mut **variable };
                                if let Err(error) = value.ensure_value_is_scalar() {
                                    stack.error_place = Some(Place::Variable(*variable));
                                    return Err(error);
                                }
                                value
                            }
                            // A parameter that is a caller's subarray.
                            Place::Element(element) => {
                                stack.error_place = Some(Place::Element(element.clone()));
                                return Err(stack::ARRAY_IN_SCALAR_CONTEXT.to_string());
                            }
                        };
                        fields_state =
                            variable.assign(AwkValue::from(key.to_string()), global_env)?;
                        // SAFETY: only the key iterator changed, so the pointers are as
                        // valid as when the iterator was pushed.
                        unsafe { stack.push(StackValue::Iterator(iter))? };
                    } else {
                        ip_increment = offset as isize;
                    }
                }
                OpCode::AsNumber => {
                    let val = stack.pop_scalar_value()?;
                    stack.push_value(val.scalar_as_f64())?;
                }
                OpCode::AppendAssign | OpCode::AppendAssignDiscard => {
                    let tail = stack
                        .pop_scalar_value()?
                        .scalar_to_string(&global_env.convfmt)?;
                    let target = stack.pop_scalar_ref()?;
                    // In place: the string grows, where `s = s x` made a new one of the
                    // whole each time, and copied it twice besides, so that building a
                    // string a piece at a time took time in its square.
                    if let AwkValueVariant::String(text) = &mut target.value {
                        text.concat(&tail);
                    } else {
                        let mut text = target.clone().scalar_to_string(&global_env.convfmt)?;
                        text.concat(&tail);
                        target.value = AwkValueVariant::String(text);
                    }
                    if instruction == OpCode::AppendAssign {
                        let value = target.clone().into_ref(AwkRefType::None);
                        stack.push_value(value)?;
                    }
                }
                OpCode::AsValue => {
                    let val = stack.pop_scalar_value()?;
                    stack.push_value(val)?;
                }
                OpCode::GetGlobal(index) => {
                    let global = self.globals[index as usize].get();
                    // SAFETY: a global's cell outlives the stack, array or not.
                    let value = unsafe { StackValue::from_var(global) };
                    // SAFETY: as above.
                    unsafe { stack.push(value)? };
                }
                OpCode::GetLocal(index) if stack.local_element(index as usize).is_some() => {
                    // A parameter that is a caller's element is that element.
                    if let Some(element) = stack.local_element(index as usize) {
                        // SAFETY: the element's variable outlives the call.
                        unsafe { stack.push(StackValue::ArrayElementRef(element))? };
                    }
                }
                OpCode::GetLocal(index) => {
                    let value = stack
                        .get_mut_value_ptr(index as usize)
                        .ok_or_else(|| "invalid local index".to_string())?;
                    // SAFETY: the local is valid until the value at `index` is popped, which
                    // is below the one pushed here.
                    let value = unsafe { StackValue::from_var(value) };
                    // SAFETY: as above.
                    unsafe { stack.push(value)? };
                }
                OpCode::GetField => {
                    let index = record_index(stack.pop_scalar_value()?.scalar_as_f64())?;
                    stack.push_value(record.read_field(index))?;
                }
                OpCode::IndexArrayGetValue | OpCode::IndexArrayGetArgument => {
                    let key = stack
                        .pop_scalar_value()?
                        .scalar_to_string(&global_env.convfmt)?;
                    let element = stack.pop_element_ref(key.into())?;
                    let ptr = stack.element_ptr(&element)?;
                    // SAFETY: the element is in its variable's array.
                    let value = unsafe { &*ptr };
                    // A subarray is referred to, so that using it as a scalar names it; an
                    // argument with no type yet too, as the function may make it one.
                    let by_reference = match value.value {
                        AwkValueVariant::Array(_) => true,
                        AwkValueVariant::Uninitialized => {
                            instruction == OpCode::IndexArrayGetArgument
                        }
                        _ => false,
                    };
                    if by_reference {
                        // SAFETY: the element's variable outlives the reference.
                        unsafe { stack.push(StackValue::ArrayElementRef(element))? };
                    } else {
                        stack.push_value(value.clone().into_ref(AwkRefType::None))?;
                    }
                }
                OpCode::IndexArraySubarray => {
                    let key = stack
                        .pop_scalar_value()?
                        .scalar_to_string(&global_env.convfmt)?;
                    let element = stack.pop_element_ref(key.into())?;
                    // SAFETY: the element's variable outlives the reference.
                    unsafe { stack.push(StackValue::ArrayElementRef(element))? };
                }
                OpCode::GlobalScalarRef(index) => {
                    // SAFETY: a global's cell outlives the stack.
                    unsafe { stack.push_ref(self.globals[index as usize].get())? };
                }
                OpCode::LocalScalarRef(index) => {
                    let value = stack
                        .local_scalar_ref(index as usize)
                        .ok_or_else(|| "invalid local index".to_string())?;
                    // SAFETY: the local, or the caller's element it is, is valid until the
                    // value at `index` is popped, which is below the one pushed here.
                    unsafe { stack.push(value)? };
                }
                OpCode::FieldRef => {
                    let index = record_index(stack.pop_scalar_value()?.scalar_as_f64())?;
                    // SAFETY: the boxed cell keeps this pointer valid across later field
                    // growth, and fields outlive the stack.
                    unsafe { stack.push_ref(record.field_ref_ptr(index))? };
                }
                OpCode::IndexArrayGetRef => {
                    let key = stack
                        .pop_scalar_value()?
                        .scalar_to_string(&global_env.convfmt)?;
                    let element = stack.pop_element_ref(key.into())?;
                    // Referring to an element makes it, as in every awk.
                    stack.element_ptr(&element)?;
                    // SAFETY: the element's variable outlives the reference, which is
                    // pushed where the array's was.
                    unsafe { stack.push(StackValue::ArrayElementRef(element))? };
                }
                OpCode::Assign => {
                    let value = stack.pop_scalar_value()?;
                    let lvalue = stack.pop_scalar_ref()?;
                    fields_state = lvalue.assign(value.clone(), global_env)?;
                    stack.push_value(value)?;
                }
                OpCode::DeleteElement => {
                    let key = stack
                        .pop_scalar_value()?
                        .scalar_to_string(&global_env.convfmt)?;
                    let element = stack.pop_element_ref(key.into())?;
                    let keys: Vec<array::Key> = element.keys().cloned().collect();
                    stack.detach_elements(element.array, &keys, true);
                    // No subarray is made: `delete a[1][2]` leaves `a` empty, as in gawk,
                    // and an element with no type yet has nothing to delete. A variable with
                    // none becomes an array, as in gawk.
                    let make = element.path.is_none();
                    // SAFETY: the element's variable outlives the reference.
                    match unsafe { element.container(make) } {
                        // SAFETY: the array is in the element's variable's.
                        Ok(Some(array)) => unsafe { &mut *array }.delete(&element.key),
                        Ok(None) => {}
                        Err(error) => {
                            stack.error_place = Some(error.place);
                            return Err(error.error);
                        }
                    }
                }
                OpCode::ClearArray => {
                    let array = stack.pop_array_to_empty(true)?;
                    array.clear();
                }
                OpCode::JumpIfFalse(offset) => {
                    let condition = stack.pop_scalar_value()?.scalar_as_bool();
                    if !condition {
                        ip_increment = offset as isize;
                    }
                }
                OpCode::JumpIfTrue(offset) => {
                    let condition = stack.pop_scalar_value()?.scalar_as_bool();
                    if condition {
                        ip_increment = offset as isize;
                    }
                }
                OpCode::Jump(offset) => {
                    ip_increment = offset as isize;
                }
                OpCode::Call(id) => {
                    stack.call_function(&functions[id as usize])?;
                    ip_increment = 0;
                }
                OpCode::CallBuiltin { function, argc } => {
                    fields_state = self.call_builtin(
                        function, argc, stack, global_env, input, functions, record,
                    )?;
                }
                OpCode::PushConstant(index) => match self.constants[index as usize].clone() {
                    Constant::Number(num) => stack.push_value(num)?,
                    Constant::String(s) => stack.push_value(AwkString::from(s))?,
                    Constant::Regex(ere) => {
                        stack.push_value(AwkValue::from_ere(ere, &record.record.borrow()))?
                    }
                },
                OpCode::PushOne => {
                    stack.push_value(1.0)?;
                }
                OpCode::PushZero => {
                    stack.push_value(0.0)?;
                }
                OpCode::PushUninitialized => {
                    stack.push_value(AwkValue::uninitialized())?;
                }
                OpCode::PushUninitializedScalar => {
                    stack.push_value(AwkValue::uninitialized_scalar())?;
                }
                OpCode::Dup => {
                    // there has to be a value, otherwise the code is malformed
                    let mut val = stack.pop().ok_or_else(|| "empty stack".to_string())?;
                    // SAFETY: `val` is valid until the value pushed before it is popped
                    // (`push`), which is still on the stack below both copies.
                    unsafe { stack.push(val.duplicate())? };
                    // SAFETY: as above.
                    unsafe { stack.push(val)? };
                }
                OpCode::Pop => {
                    stack.pop();
                }
                OpCode::Next => return Ok(ExecutionResult::Next),
                OpCode::NextFile => return Ok(ExecutionResult::NextFile),
                OpCode::Exit => {
                    let exit_code = stack.pop_scalar_value()?.scalar_as_f64();
                    return Ok(ExecutionResult::Exit(exit_code as i32));
                }
                OpCode::Return => {
                    let return_value = stack.pop_scalar_value()?;
                    stack.restore_caller()?;
                    // A function a builtin called returns to the builtin.
                    if stack.stop_depth == Some(stack.call_frames.len()) {
                        return Ok(ExecutionResult::Expression(return_value));
                    }
                    // The call is the caller's code that ran last.
                    stack.last_ip = stack.ip;
                    stack.push_value(return_value)?;
                }
                // The compiler replaces every placeholder it emits.
                OpCode::Invalid => return Err("invalid opcode".to_string()),
            }
            // gawk's warnings about a regex the instruction made, placed at it.
            if crate::regex::has_warnings() {
                for warning in crate::regex::take_warnings() {
                    self.warn(&warning, stack);
                }
            }
            match fields_state {
                FieldsState::Ok => {
                    // no need to recompute anything
                }
                FieldsState::RecordChanged => {
                    // SAFETY: between instructions no reference to a field is held.
                    unsafe { record.recompute_fields(global_env)? };
                    // SAFETY: a global's cell lives as long as the interpreter, and nothing
                    // else refers to NF between instructions.
                    let nf = unsafe { &mut *self.globals[SpecialVar::Nf as usize].get() };
                    nf.assign(record.get_last_field() as f64, global_env)?;
                }
                FieldsState::FieldChanged { changed_field } => {
                    // SAFETY: between instructions no reference to a field is held.
                    unsafe { record.recompute_record(global_env, changed_field, false)? };
                    // SAFETY: as for `RecordChanged`.
                    let nf = unsafe { &mut *self.globals[SpecialVar::Nf as usize].get() };
                    nf.assign(record.get_last_field() as f64, global_env)?;
                }
                FieldsState::NfChanged => {
                    // SAFETY: between instructions no reference to a field is held.
                    unsafe { record.recompute_record(global_env, global_env.nf, true)? };
                }
            }
            fields_state = FieldsState::Ok;
            stack.ip += ip_increment;
        }
        // A pattern's value, a scalar: an array there is gawk's error, where its copy was
        // taken for a truth value and panicked.
        Ok(ExecutionResult::Expression(if stack.is_empty() {
            AwkValue::default()
        } else {
            stack.pop_scalar_value()?
        }))
    }

    fn new(args: Array, env: Array, constants: Vec<Constant>, program_globals: usize) -> Self {
        let mut globals = (0..SpecialVar::Count as usize + program_globals)
            .map(|_| AwkValueRef::new(AwkValue::uninitialized()))
            .collect::<Vec<AwkValueRef>>();

        *globals[SpecialVar::Argc as usize].get_mut() = AwkValue::from(args.len() as f64)
            .into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Argc));
        *globals[SpecialVar::Argv as usize].get_mut() =
            AwkValue::from(args).into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Argv));
        *globals[SpecialVar::Convfmt as usize].get_mut() = AwkValue::from("%.6g".to_string())
            .into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Convfmt));
        *globals[SpecialVar::Environ as usize].get_mut() =
            AwkValue::from(env).into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Environ));
        *globals[SpecialVar::Filename as usize].get_mut() = AwkValue::from("-".to_string())
            .into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Filename));
        *globals[SpecialVar::Fnr as usize].get_mut() =
            AwkValue::from(0.0).into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Fnr));
        *globals[SpecialVar::Fs as usize].get_mut() =
            AwkValue::from(" ").into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Fs));
        *globals[SpecialVar::Nf as usize].get_mut() =
            AwkValue::from(0.0).into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Nf));
        *globals[SpecialVar::Nr as usize].get_mut() =
            AwkValue::from(0.0).into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Nr));
        *globals[SpecialVar::Ofmt as usize].get_mut() = AwkValue::from("%.6g".to_string())
            .into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Ofmt));
        *globals[SpecialVar::Ofs as usize].get_mut() =
            AwkValue::from(" ".to_string()).into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Ofs));
        *globals[SpecialVar::Ors as usize].get_mut() = AwkValue::from("\n".to_string())
            .into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Ors));
        *globals[SpecialVar::Rlength as usize].get_mut() =
            AwkValue::from(0.0).into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Rlength));
        *globals[SpecialVar::Rs as usize].get_mut() =
            AwkValue::from("\n".to_string()).into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Rs));
        *globals[SpecialVar::Rstart as usize].get_mut() =
            AwkValue::from(0.0).into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Rstart));
        *globals[SpecialVar::Subsep as usize].get_mut() = AwkValue::from("\x1c".to_string())
            .into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::Subsep));
        *globals[SpecialVar::IgnoreCase as usize].get_mut() =
            AwkValue::from(0.0).into_ref(AwkRefType::SpecialGlobalVar(SpecialVar::IgnoreCase));

        Self {
            globals,
            global_names: Vec::new(),
            constants,
            write_files: WriteFiles::default(),
            read_files: ReadFiles::default(),
            write_pipes: WritePipes::default(),
            read_pipes: ReadPipes::default(),
            rand_seed: 0,
            rng: SmallRng::seed_from_u64(0),
            last_location: None,
        }
    }
}

fn is_valid_variable(s: &str) -> bool {
    s.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The main input: the files ARGV names, in turn, with the assignments between them made
/// as they are reached, or standard input when ARGV names none.
///
/// The main loop and plain `getline` both read from it, so `getline` goes on into the
/// next file and, in `BEGIN`, opens the first, as POSIX has it. It read only from the file
/// the main loop had open, and from nothing in `BEGIN` (`REVIEW_REPORT.md` TXT-06).
struct MainInput {
    reader: Box<dyn RecordReader>,
    /// The ARGV index to look at next.
    next_arg: usize,
    /// Whether a file (or standard input) has been opened, which is what decides whether
    /// standard input is read when ARGV runs out.
    opened: bool,
    exhausted: bool,
    /// The program's global variables by name, for `var=value` operands.
    globals_by_name: HashMap<String, u32>,
}

impl MainInput {
    fn new(globals_by_name: HashMap<String, u32>) -> Self {
        Self {
            reader: Box::new(EmptyRecordReader::default()),
            next_arg: 1,
            opened: false,
            exhausted: false,
            globals_by_name,
        }
    }

    /// A main input with nothing to read, for running code with no ARGV behind it.
    #[cfg(test)]
    fn without_input() -> Self {
        Self {
            exhausted: true,
            ..Self::new(HashMap::new())
        }
    }

    /// The next record of the main input, opening the files ARGV names as each runs out.
    /// Counting it in NR and FNR is the caller's, through `Interpreter::count_record`.
    /// `place` is the code that ran last, for a warning.
    fn next_record(
        &mut self,
        globals: &mut [AwkValueRef],
        global_env: &mut GlobalEnv,
        place: Option<&(Rc<str>, u32)>,
    ) -> Result<Option<(String, bool)>, String> {
        while !self.exhausted {
            if let Some(record) = self
                .reader
                .read_next_record(&global_env.rs, global_env.strip_cr)?
            {
                return Ok(Some(record));
            }
            self.open_next(globals, global_env, place)?;
        }
        Ok(None)
    }

    /// Leaves the file being read (`nextfile`).
    fn skip_file(&mut self) {
        self.reader = Box::new(EmptyRecordReader::default());
    }

    /// Opens the next file ARGV names, making the assignments before it; standard input
    /// when ARGV names no file at all. FILENAME names it, and FNR starts over.
    fn open_next(
        &mut self,
        globals: &mut [AwkValueRef],
        global_env: &mut GlobalEnv,
        place: Option<&(Rc<str>, u32)>,
    ) -> Result<(), String> {
        loop {
            let argc = globals[SpecialVar::Argc as usize].get_mut().scalar_as_f64() as usize;
            let arg = if self.next_arg >= argc {
                if self.opened {
                    self.exhausted = true;
                    self.reader = Box::new(EmptyRecordReader::default());
                    return Ok(());
                }
                AwkString::from("-")
            } else {
                globals[SpecialVar::Argv as usize]
                    .get_mut()
                    .as_array()?
                    .get_value(self.next_arg.to_string().into())?
                    .clone()
                    .scalar_to_string(&global_env.convfmt)?
            };
            self.next_arg += 1;

            if arg.is_empty() {
                continue;
            }
            if let Some((var, value)) = parse_assignment(&arg) {
                if let Some(&global_index) = self.globals_by_name.get(var) {
                    globals[global_index as usize].get_mut().assign(
                        maybe_numeric_string(escape_string_contents(value)?),
                        global_env,
                    )?;
                }
                continue;
            }

            // gawk passes over a directory with a warning, placed as an error is; reading
            // one failed. It still counts as a file, so standard input is not read.
            if arg.as_str() != "-" && std::path::Path::new(arg.as_str()).is_dir() {
                let _ = io::flush_stdout();
                let place = place
                    .map(|(file, line)| {
                        let file = if file.is_empty() { "cmd. line" } else { file };
                        format!("{file}:{line}: ")
                    })
                    .unwrap_or_default();
                eprintln!(
                    "awk: {place}warning: command line argument `{arg}' is a directory: skipped"
                );
                self.opened = true;
                continue;
            }
            globals[SpecialVar::Filename as usize].get_mut().value =
                AwkValueVariant::String(maybe_numeric_string(arg.clone()));
            // FNR starts over before the file is opened, so that a file that cannot be
            // opened is reported without the last one's place, as gawk reports it.
            globals[SpecialVar::Fnr as usize]
                .get_mut()
                .assign(0.0, global_env)?;
            self.reader = if arg.as_str() == "-"
                || io::SpecialFile::named(&arg) == Some(io::SpecialFile::Stdin)
            {
                Box::new(StdinRecordReader::default())
            } else {
                Box::new(FileStream::open(&arg)?)
            };
            self.opened = true;
            return Ok(());
        }
    }
}

fn parse_assignment(s: &str) -> Option<(&str, &str)> {
    let (lhs, rhs) = s.split_once('=')?;
    if is_valid_variable(lhs) {
        Some((lhs, rhs))
    } else {
        None
    }
}

fn set_globals_with_assignment_arguments(
    interpreter: &mut Interpreter,
    globals: &HashMap<String, u32>,
    global_env: &mut GlobalEnv,
    assignments: &[String],
) -> Result<(), String> {
    assignments
        .iter()
        .filter_map(|s| parse_assignment(s))
        .filter_map(|(var, value)| globals.get(var).copied().map(|index| (index, value)))
        .try_for_each(|(global_index, value)| {
            let value = escape_string_contents(value)?;
            interpreter.globals[global_index as usize]
                .get_mut()
                .assign(maybe_numeric_string(value), global_env)?;
            Ok(())
        })
}

pub fn interpret(
    program: Program,
    args: &[String],
    assignments: &[String],
    separator: Option<String>,
) -> Result<i32, String> {
    interpret_with_eol(program, args, assignments, separator, false)
}

/// Like [`interpret`], but with an explicit line-ending mode: when
/// `cr_is_data` is true, CR bytes are ordinary data (never hidden on input,
/// never restored on output), exactly like awk on Linux. Otherwise the CR of
/// a CRLF-terminated record is hidden from `$0` and restored by `print`.
pub fn interpret_with_eol(
    program: Program,
    args: &[String],
    assignments: &[String],
    separator: Option<String>,
    cr_is_data: bool,
) -> Result<i32, String> {
    let args = iter::once(("0".to_string(), AwkValue::from("awk")))
        .chain(args.iter().enumerate().map(|(index, s)| {
            (
                (index + 1).to_string(),
                maybe_numeric_string(s.as_str()).into(),
            )
        }))
        .collect();

    let env = std::env::vars()
        .map(|(k, v)| (k, maybe_numeric_string(v)))
        .collect();

    let mut stack = iter::repeat_with(|| StackValue::Invalid)
        .take(STACK_SIZE)
        .collect::<Vec<StackValue>>();
    let mut current_record = Record::default();
    let mut interpreter = Interpreter::new(args, env, program.constants, program.globals_count);
    interpreter.global_names = vec![Rc::from(""); interpreter.globals.len()];
    for (name, &index) in &program.globals {
        if let Some(slot) = interpreter.global_names.get_mut(index as usize) {
            *slot = Rc::from(name.as_str());
        }
    }
    // IGNORECASE starts off, whatever a run before on this thread left.
    crate::regex::set_ignore_case(false);
    let mut global_env = GlobalEnv {
        strip_cr: !cr_is_data,
        ..GlobalEnv::default()
    };
    let mut range_pattern_started = vec![false; program.rules.len()];
    let mut return_value = 0;

    set_globals_with_assignment_arguments(
        &mut interpreter,
        &program.globals,
        &mut global_env,
        assignments,
    )?;

    if let Some(separator) = separator {
        // POSIX: `-F sepstring` is equivalent to `-v FS=sepstring`, so the
        // separator undergoes the same escape processing (e.g. `-F '\t'` is a
        // tab), matching the -v path above.
        let separator = escape_string_contents(&separator)?;
        interpreter.globals[SpecialVar::Fs as usize]
            .get_mut()
            .assign(AwkString::from(separator), &mut global_env)?;
    }

    let mut input = MainInput::new(program.globals.clone());

    for action in program.begin_actions {
        let begin_result = interpreter.run(
            &action,
            &program.functions,
            &mut current_record,
            &mut stack,
            &mut global_env,
            &mut input,
        )?;
        match begin_result {
            ExecutionResult::Exit(val) => {
                return_value = val;
                break;
            }
            other => no_next_in_special_action(&other, "BEGIN")
                .map_err(|e| interpreter.outside_error(&e))?,
        }
    }

    if program.rules.is_empty() && program.end_actions.is_empty() {
        return Ok(return_value);
    }

    'record_loop: while let Some((record, crlf)) = input
        .next_record(
            &mut interpreter.globals,
            &mut global_env,
            interpreter.last_location.as_ref(),
        )
        .map_err(|e| interpreter.outside_error(&e))?
    {
        interpreter
            .count_record(&mut global_env)
            .map_err(|e| interpreter.outside_error(&e))?;
        global_env.last_record_crlf = crlf;
        let fs = global_env
            .effective_fs()
            .map_err(|e| interpreter.outside_error(&e))?;
        current_record
            .reset(record, fs)
            .map_err(|e| interpreter.outside_error(&e))?;
        interpreter.globals[SpecialVar::Nf as usize].get_mut().value =
            AwkValue::from(current_record.get_last_field() as f64).value;
        global_env.nf = current_record.get_last_field();

        for (i, rule) in program.rules.iter().enumerate() {
            // Whether the rule runs, or the `next`, `nextfile` or `exit` of a function its
            // pattern called, which ends the pattern as it would the action. That used to
            // panic: `function f() { exit 3 } f() { print }`.
            let pattern: Result<bool, ExecutionResult> = 'pattern: {
                match &rule.pattern {
                    Pattern::All => Ok(true),
                    Pattern::Expr(expr) => interpreter
                        .run(
                            expr,
                            &program.functions,
                            &mut current_record,
                            &mut stack,
                            &mut global_env,
                            &mut input,
                        )?
                        .pattern_matches(),
                    Pattern::Range { start, end } => {
                        if range_pattern_started[i] {
                            let end_matches = match interpreter
                                .run(
                                    end,
                                    &program.functions,
                                    &mut current_record,
                                    &mut stack,
                                    &mut global_env,
                                    &mut input,
                                )?
                                .pattern_matches()
                            {
                                Ok(matches) => matches,
                                Err(other) => break 'pattern Err(other),
                            };
                            if end_matches {
                                range_pattern_started[i] = false;
                            }
                            // range is inclusive
                            Ok(true)
                        } else {
                            let should_start = match interpreter
                                .run(
                                    start,
                                    &program.functions,
                                    &mut current_record,
                                    &mut stack,
                                    &mut global_env,
                                    &mut input,
                                )?
                                .pattern_matches()
                            {
                                Ok(matches) => matches,
                                Err(other) => break 'pattern Err(other),
                            };
                            if should_start {
                                // Check if end also matches on the same line
                                let end_matches = match interpreter
                                    .run(
                                        end,
                                        &program.functions,
                                        &mut current_record,
                                        &mut stack,
                                        &mut global_env,
                                        &mut input,
                                    )?
                                    .pattern_matches()
                                {
                                    Ok(matches) => matches,
                                    Err(other) => break 'pattern Err(other),
                                };
                                // If end matches on the same line, don't keep range open
                                range_pattern_started[i] = !end_matches;
                            }
                            Ok(should_start)
                        }
                    }
                }
            };
            let rule_result = match pattern {
                Ok(false) => continue,
                Ok(true) => interpreter.run(
                    &rule.action,
                    &program.functions,
                    &mut current_record,
                    &mut stack,
                    &mut global_env,
                    &mut input,
                )?,
                Err(other) => other,
            };
            match rule_result {
                ExecutionResult::Next => break,
                ExecutionResult::NextFile => {
                    input.skip_file();
                    break;
                }
                ExecutionResult::Exit(val) => {
                    return_value = val;
                    break 'record_loop;
                }
                ExecutionResult::Expression(_) => {}
            }
        }
    }

    for action in program.end_actions {
        let end_result = interpreter.run(
            &action,
            &program.functions,
            &mut current_record,
            &mut stack,
            &mut global_env,
            &mut input,
        )?;
        match end_result {
            ExecutionResult::Exit(val) => {
                return_value = val;
                break;
            }
            other => no_next_in_special_action(&other, "END")
                .map_err(|e| interpreter.outside_error(&e))?,
        }
    }

    Ok(return_value)
}

/// gawk's fatal error for a redirection (`operator`) to or from an empty name; the name
/// was opened, and failed or did nothing.
fn no_null_redirection(name: &str, operator: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(format!(
            "expression for `{operator}' redirection has null string value"
        ));
    }
    Ok(())
}

/// gawk's fatal error for a division by zero, which names the operator unless it is a
/// plain `/`. The quotient was infinite or not a number, and printed as `+inf` or `-nan`.
fn division_by_zero(operator: OpCode) -> String {
    let operator = match operator {
        OpCode::Mod => " in `%'",
        OpCode::DivAssign => " in `/='",
        OpCode::ModAssign => " in `%='",
        _ => "",
    };
    format!("division by zero attempted{operator}")
}

/// The error of a `next` or `nextfile` that a function called from a BEGIN or END action
/// ran, in gawk's words; the compiler rejects one written in the action itself.
fn no_next_in_special_action(result: &ExecutionResult, action: &str) -> Result<(), String> {
    let keyword = match result {
        ExecutionResult::Next => "next",
        ExecutionResult::NextFile => "nextfile",
        _ => return Ok(()),
    };
    Err(format!(
        "`{keyword}' cannot be called from a `{action}' rule"
    ))
}
