//
// Copyright (c) 2024-2026 Hemi Labs, Inc.
//
// This file is part of the posixutils-rs project covered under
// the MIT License.  For the full license text, please see the LICENSE
// file in the root directory of this project.
// SPDX-License-Identifier: MIT
//

use super::format::{
    FormatArgs, IntegerFormat, fmt_write_decimal_float, fmt_write_float_general,
    fmt_write_hex_float, fmt_write_scientific_float, fmt_write_signed_f64, fmt_write_special_float,
    fmt_write_string, fmt_write_unsigned, parse_conversion_specifier_args,
};
use super::record::{FieldSeparator, FieldsState, field_separators, split_record};
use super::stack::{Stack, array_place};
use super::string::AwkString;
use super::value::{AwkValue, AwkValueVariant};
use super::{GlobalEnv, compare_text, fold_case, maybe_numeric_string, strtod, swap_with_default};
use crate::program::BuiltinFunction;
use crate::regex::Regex;
use std::cmp::Ordering;
use std::rc::Rc;

pub(crate) fn sprintf(
    format_string: &str,
    values: &mut [AwkValue],
    float_format: &str,
) -> Result<AwkString, String> {
    let mut result = String::with_capacity(format_string.len());
    let mut iter = format_string.chars();
    let mut next = iter.next();
    let mut current_arg = values.len();
    while let Some(c) = next {
        match c {
            '%' => {
                let specifier_text = iter.as_str();
                let Ok((specifier, mut args)) = parse_conversion_specifier_args(&mut iter) else {
                    // The format ends inside the specifier, which is written out as it
                    // is, as gawk does.
                    result.push('%');
                    result.push_str(specifier_text);
                    break;
                };
                if specifier == '%' {
                    result.push('%');
                    next = iter.next();
                    continue;
                }
                let read = specifier_text.len() - iter.as_str().len();
                if !"diouxXcseEfFgGaA".contains(specifier) {
                    // No conversion: the specifier is written out as it is.
                    result.push('%');
                    result.push_str(specifier_text.get(..read).unwrap_or_default());
                    next = iter.next();
                    continue;
                }
                // Where the specifier starts in the format, for the error.
                let start = format_string.len() - specifier_text.len() - 1;
                let spec = specifier_text.get(..read).unwrap_or_default();

                // A '*' field width or precision consumes the next argument(s),
                // in order: width, then precision, then the conversion's value.
                if args.needs_width_arg() {
                    if current_arg == 0 {
                        let star = spec.find('*').unwrap_or_default();
                        return Err(not_enough_arguments(format_string, start + 1 + star));
                    }
                    current_arg -= 1;
                    args.set_width(
                        swap_with_default(&mut values[current_arg]).scalar_as_f64() as i64
                    );
                }
                if args.needs_precision_arg() {
                    if current_arg == 0 {
                        let star = spec.rfind('*').unwrap_or_default();
                        return Err(not_enough_arguments(format_string, start + 1 + star));
                    }
                    current_arg -= 1;
                    args.set_precision(
                        swap_with_default(&mut values[current_arg]).scalar_as_f64() as i64
                    );
                }

                if current_arg == 0 {
                    return Err(not_enough_arguments(format_string, start + read));
                }
                current_arg -= 1;
                let value = swap_with_default(&mut values[current_arg]);
                format_one_conversion(&mut result, specifier, value, &args, float_format)?;
                next = iter.next();
            }
            other => {
                result.push(other);
                next = iter.next();
            }
        }
    }
    Ok(result.into())
}

/// gawk's error for a format with more conversions than arguments: the format, and a
/// caret under the character at byte `at` that found none left. cash said "not enough
/// arguments for format string".
fn not_enough_arguments(format_string: &str, at: usize) -> String {
    format!(
        "not enough arguments to satisfy format string\n\t`{format_string}'\n\t{}^ ran out for this one",
        " ".repeat(at)
    )
}

/// Format a single `%` conversion (`specifier` with `args`, using the
/// already-fetched argument `value`) and append it to `result`.
fn format_one_conversion(
    result: &mut String,
    specifier: char,
    value: AwkValue,
    args: &FormatArgs,
    float_format: &str,
) -> Result<(), String> {
    if matches!(
        specifier,
        'd' | 'i' | 'u' | 'o' | 'x' | 'X' | 'a' | 'A' | 'f' | 'F' | 'e' | 'E' | 'g' | 'G'
    ) {
        let number = value.scalar_as_f64();
        if !number.is_finite() {
            fmt_write_special_float(result, number, specifier.is_ascii_uppercase(), args);
            return Ok(());
        }
    }
    match specifier {
        'd' | 'i' => {
            fmt_write_signed_f64(result, value.scalar_as_f64(), args);
        }
        'u' | 'o' | 'x' | 'X' => {
            // A negative number is written as its 64-bit two's complement, as gawk and C
            // write it (`printf "%x", -1` is `ffffffffffffffff`); it was a fatal error.
            let value = value.scalar_as_f64() as i64;
            let format = match specifier {
                'u' => IntegerFormat::Decimal,
                'o' => IntegerFormat::Octal,
                'x' => IntegerFormat::HexLower,
                _ => IntegerFormat::HexUpper,
            };
            fmt_write_unsigned(result, value.cast_unsigned(), format, args);
        }
        'a' | 'A' => {
            let value = value.scalar_as_f64();
            fmt_write_hex_float(result, value, specifier == 'a', args);
        }
        'f' | 'F' => {
            let value = value.scalar_as_f64();
            fmt_write_decimal_float(result, value, specifier == 'f', args);
        }
        'e' | 'E' => {
            let value = value.scalar_as_f64();
            fmt_write_scientific_float(result, value, specifier == 'e', args);
        }
        'g' | 'G' => {
            let value = value.scalar_as_f64();
            fmt_write_float_general(result, value, specifier == 'g', args);
        }
        'c' => {
            let ch = match &value.value {
                AwkValueVariant::Number(n) => char::from_u32(*n as u32).unwrap_or('\0'),
                AwkValueVariant::String(s) if s.is_numeric => {
                    let code = value.scalar_as_f64() as u32;
                    char::from_u32(code).unwrap_or('\0')
                }
                // Not empty, so there is a first character.
                AwkValueVariant::String(s) if !s.is_empty() => s.chars().next().unwrap_or('\0'),
                _ => {
                    let code = value.scalar_as_f64() as u32;
                    char::from_u32(code).unwrap_or('\0')
                }
            };
            // A precision cuts a string, and the character is the whole of this one, so
            // gawk ignores it: `%.0c` writes the character, where it wrote nothing.
            let mut args = args.clone();
            args.set_precision(-1);
            fmt_write_string(result, &ch.to_string(), &args);
        }
        's' => {
            let value = value.scalar_to_string(float_format)?;
            fmt_write_string(result, &value, args);
        }
        _ => return Err(format!("unsupported format specifier '{}'", specifier)),
    }
    Ok(())
}

pub(crate) fn builtin_sprintf(
    stack: &mut Stack,
    argc: u16,
    global_env: &mut GlobalEnv,
) -> Result<AwkString, String> {
    // `printf` with nothing to print, gawk's fatal error.
    if argc == 0 {
        return Err("printf: no arguments".to_string());
    }
    let mut values = gather_values(stack, argc - 1)?;
    let format_string = stack
        .pop_scalar_value()?
        .scalar_to_string(&global_env.convfmt)?;
    sprintf(&format_string, &mut values, &global_env.convfmt)
}

/// Convert a byte offset within `s` to a character offset (the number of whole
/// characters that begin before `byte`). POSIX awk string functions operate on
/// characters, while the regex engine and `str` searches report byte offsets.
pub(crate) fn byte_offset_to_char_count(s: &str, byte: usize) -> usize {
    s.char_indices().take_while(|&(i, _)| i < byte).count()
}

/// `substr(s, m[, n])`: the at most `n`-character substring of `s` that begins
/// at character position `m` (numbering from 1). Positions below 1 still consume
/// part of `n`, matching nawk/gawk, so e.g. `substr("hello", -1, 3) == "h"`.
/// `m` and `n` are assumed already truncated toward zero by the caller. The
/// character window is `[max(m, 1), m + n)`; `take` naturally stops at the end
/// of the string, so no length pre-scan or `Vec` is needed.
pub(crate) fn substr(s: &str, m: i64, n: Option<i64>) -> String {
    let start = m.max(1);
    let count = match n {
        None => usize::MAX,
        Some(n) => m.saturating_add(n).saturating_sub(start).max(0) as usize,
    };
    // `start >= 1`; a start past `usize::MAX` (only possible on a 32-bit `usize`)
    // skips the whole string, yielding the empty substring.
    let skip = usize::try_from(start - 1).unwrap_or(usize::MAX);
    s.chars().skip(skip).take(count).collect()
}

/// What gawk's `match(s, r, arr)` puts in `arr` for a group: its number, its text, and
/// where it starts and how long it is, in characters.
pub(crate) struct MatchedGroup {
    pub(crate) number: usize,
    pub(crate) text: String,
    pub(crate) start: f64,
    pub(crate) length: f64,
}

/// `match(s, r)`: pushes RSTART and returns RSTART and RLENGTH, and with `groups` the
/// match and its groups that took part in it, for gawk's third argument.
pub(crate) fn builtin_match(
    stack: &mut Stack,
    global_env: &mut GlobalEnv,
    groups: bool,
) -> Result<(f64, f64, Vec<MatchedGroup>), String> {
    let ere = stack.pop_scalar_value()?.into_ere(&global_env.convfmt)?;
    let string = stack
        .pop_scalar_value()?
        .scalar_to_string(&global_env.convfmt)?;
    let text = string.as_str().to_owned();
    // RSTART/RLENGTH are measured in characters, not bytes.
    let place = |found: &crate::regex::RegexMatch| {
        let cstart = byte_offset_to_char_count(&text, found.start);
        let cend = byte_offset_to_char_count(&text, found.end);
        (cstart as f64 + 1.0, (cend - cstart) as f64)
    };
    let mut matched = Vec::new();
    let (start, len) = if groups {
        match ere.captures(&text) {
            Some(captures) => {
                for (number, found) in captures.iter().enumerate() {
                    if let Some(found) = found {
                        let (start, length) = place(found);
                        matched.push(MatchedGroup {
                            number,
                            text: text
                                .get(found.start..found.end)
                                .unwrap_or_default()
                                .to_string(),
                            start,
                            length,
                        });
                    }
                }
                matched
                    .first()
                    .map_or((0.0, -1.0), |whole| (whole.start, whole.length))
            }
            None => (0.0, -1.0),
        }
    } else {
        ere.match_locations(&text)
            .next()
            .map_or((0.0, -1.0), |found| place(&found))
    };
    stack.push_value(start)?;
    Ok((start, len, matched))
}

pub(crate) fn gsub(
    ere: &Regex,
    repl: &str,
    in_str: &str,
    only_replace_first: bool,
) -> Result<(AwkString, usize), String> {
    let mut result = String::with_capacity(in_str.len());
    let mut last_match_end = 0;

    let mut repl_parts = Vec::new();
    let mut current_repl_part = String::new();
    let mut repl_iter = repl.chars();
    while let Some(c) = repl_iter.next() {
        if c == '\\' {
            match repl_iter.next() {
                Some('\\') => current_repl_part.push('\\'),
                Some('&') => current_repl_part.push('&'),
                Some(c) => {
                    current_repl_part.push('\\');
                    current_repl_part.push(c);
                }
                None => {
                    current_repl_part.push('\\');
                    break;
                }
            }
        } else if c == '&' {
            repl_parts.push(current_repl_part);
            current_repl_part = String::new();
        } else {
            current_repl_part.push(c);
        }
    }
    repl_parts.push(current_repl_part);

    let mut num_replacements = 0;
    // The regex engine reports matches at character boundaries, so `get` finds each.
    for m in ere.match_locations(in_str) {
        result.push_str(in_str.get(last_match_end..m.start).unwrap_or_default());
        let replaced_string = in_str.get(m.start..m.end).unwrap_or_default();
        result.push_str(&repl_parts[0]);
        for part in repl_parts.iter().skip(1) {
            result.push_str(replaced_string);
            result.push_str(part);
        }
        last_match_end = m.end;
        num_replacements += 1;
        if only_replace_first {
            break;
        }
    }
    result.push_str(in_str.get(last_match_end..).unwrap_or_default());
    Ok((result.into(), num_replacements))
}

/// `split(s, arr[, fs[, seps]])`: split `s` into `arr` on the field separator (the
/// optional third argument, else `FS`) and return the number of fields. When the
/// `fs` argument is a regex value it is used directly; otherwise it is
/// interpreted like `FS` (e.g. `" "` means whitespace). gawk's `seps` gets the
/// separators (`field_separators`); it was an error.
pub(crate) fn builtin_split(
    stack: &mut Stack,
    global_env: &mut GlobalEnv,
    argc: u16,
) -> Result<FieldsState, String> {
    let seps = if argc == 4 {
        Some(stack.pop().ok_or_else(|| "empty stack".to_string())?)
    } else {
        None
    };
    let separator = if argc == 2 {
        None
    } else {
        let sep_val = stack.pop_scalar_value()?;
        if matches!(&sep_val.value, AwkValueVariant::Regex { .. }) {
            let ere = sep_val.into_ere(&global_env.convfmt)?;
            // A regex of one character is that character, as in gawk: `/./` splits at
            // dots, `/ /` at each space, and IGNORECASE leaves it be; it was the regex.
            match ere.pattern().as_bytes() {
                [byte] => Some(FieldSeparator::Char(*byte)),
                _ => Some(FieldSeparator::Ere(ere)),
            }
        } else {
            let sep_str = sep_val.scalar_to_string(&global_env.convfmt)?;
            Some(FieldSeparator::try_from(sep_str)?)
        }
    };
    let s = stack
        .pop_scalar_value()?
        .scalar_to_string(&global_env.convfmt)?;
    let array_value = stack.pop().ok_or_else(|| "empty stack".to_string())?;
    if let Some(seps) = &seps
        && array_place(&array_value).is_some()
        && array_place(&array_value) == array_place(seps)
    {
        return Err("split: cannot use the same array for second and fourth args".to_string());
    }
    let field_separator = separator.as_ref().unwrap_or(&global_env.fs);
    let mut fields = Vec::new();
    let mut separators = Vec::new();
    if !s.is_empty() {
        if seps.is_some() {
            separators = field_separators(&s, field_separator);
        }
        split_record(s, field_separator, |_, field| {
            fields.push(field);
            Ok(())
        })?;
    }
    let n = fields.len();
    // Each array is found when it is filled, so that making one does not move the other.
    let array = stack
        .resolve_array_to_empty(array_value, true)
        .map_err(|error| not_an_array(error, "split: second argument"))?;
    array.clear();
    for (index, field) in fields.into_iter().enumerate() {
        array.set((index + 1).to_string(), field)?;
    }
    if let Some(seps) = seps {
        let seps = stack
            .resolve_array_to_empty(seps, true)
            .map_err(|error| not_an_array(error, "split: fourth argument"))?;
        seps.clear();
        for (index, separator) in separators {
            seps.set(index.to_string(), maybe_numeric_string(separator))?;
        }
    }
    stack.push_value(n as f64)?;
    Ok(FieldsState::Ok)
}

/// What gawk's `asort` and `asorti` sort by, `how` names it: `@val_type_asc` and the
/// others.
#[derive(Clone, Copy)]
pub(crate) struct SortOrder {
    by: SortBy,
    descending: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortBy {
    IndexString,
    IndexNumber,
    ValueType,
    ValueString,
    ValueNumber,
    /// The order of `for (k in a)`.
    Unsorted,
}

impl SortOrder {
    /// The order gawk names `how`, if it is one of gawk's own.
    pub(crate) fn named(how: &str) -> Option<Self> {
        if how == "@unsorted" {
            return Some(Self {
                by: SortBy::Unsorted,
                descending: false,
            });
        }
        let (by, direction) = how.strip_prefix('@')?.rsplit_once('_')?;
        let by = match by {
            "ind_str" => SortBy::IndexString,
            "ind_num" => SortBy::IndexNumber,
            "val_type" => SortBy::ValueType,
            "val_str" => SortBy::ValueString,
            "val_num" => SortBy::ValueNumber,
            _ => return None,
        };
        let descending = match direction {
            "asc" => false,
            "desc" => true,
            _ => return None,
        };
        Some(Self { by, descending })
    }

    /// Sorts `elements`, each an index and its value. Elements that compare equal are
    /// in the order of their indices as strings, as in gawk, and a descending order is
    /// the ascending one turned round.
    pub(crate) fn sort(self, elements: &mut [(Rc<str>, AwkValue)], convfmt: &str, fold: bool) {
        if self.by == SortBy::Unsorted {
            return;
        }
        elements.sort_by(|a, b| {
            let order = self.compare(a, b, convfmt, fold);
            if self.descending {
                order.reverse()
            } else {
                order
            }
        });
    }

    fn compare(
        self,
        (a_index, a): &(Rc<str>, AwkValue),
        (b_index, b): &(Rc<str>, AwkValue),
        convfmt: &str,
        fold: bool,
    ) -> Ordering {
        // Text, with case ignored under IGNORECASE, as in gawk.
        let text = |a: &str, b: &str| compare_text(a, b, fold);
        let by_index = || text(a_index, b_index);
        match self.by {
            SortBy::IndexString | SortBy::Unsorted => by_index(),
            SortBy::IndexNumber => strtod(a_index)
                .total_cmp(&strtod(b_index))
                .then_with(by_index),
            SortBy::ValueType => {
                let (a, b) = (Sorted::of(a, convfmt), Sorted::of(b, convfmt));
                a.rank()
                    .cmp(&b.rank())
                    .then_with(|| match (&a, &b) {
                        (Sorted::Number(x, _), Sorted::Number(y, _)) => x.total_cmp(y),
                        (Sorted::String(x), Sorted::String(y)) => text(x, y),
                        (Sorted::Array(x), Sorted::Array(y)) => x.cmp(y),
                        _ => Ordering::Equal,
                    })
                    .then_with(by_index)
            }
            SortBy::ValueString => {
                let (a, b) = (Sorted::of(a, convfmt), Sorted::of(b, convfmt));
                let arrays = matches!(a, Sorted::Array(_)).cmp(&matches!(b, Sorted::Array(_)));
                arrays
                    .then_with(|| text(a.text(), b.text()))
                    .then_with(by_index)
            }
            SortBy::ValueNumber => {
                let (a, b) = (Sorted::of(a, convfmt), Sorted::of(b, convfmt));
                let arrays = matches!(a, Sorted::Array(_)).cmp(&matches!(b, Sorted::Array(_)));
                arrays
                    .then_with(|| a.number().total_cmp(&b.number()))
                    .then_with(|| text(a.text(), b.text()))
                    .then_with(by_index)
            }
        }
    }
}

/// A value as `asort` compares it: a number (a numeric string too) with its text, a
/// string, or an array, by its size.
enum Sorted {
    Number(f64, String),
    String(String),
    Array(usize),
}

impl Sorted {
    fn of(value: &AwkValue, convfmt: &str) -> Self {
        match &value.value {
            AwkValueVariant::Array(array) => Sorted::Array(array.len()),
            AwkValueVariant::String(text) if !text.is_numeric => {
                Sorted::String(text.as_str().to_string())
            }
            _ => Sorted::Number(
                value.scalar_as_f64(),
                value
                    .clone()
                    .scalar_to_string(convfmt)
                    .map(|text| text.to_string())
                    .unwrap_or_default(),
            ),
        }
    }

    fn rank(&self) -> u8 {
        match self {
            Sorted::Number(..) => 0,
            Sorted::String(_) => 1,
            Sorted::Array(_) => 2,
        }
    }

    fn text(&self) -> &str {
        match self {
            Sorted::Number(_, text) | Sorted::String(text) => text,
            Sorted::Array(_) => "",
        }
    }

    fn number(&self) -> f64 {
        match self {
            Sorted::Number(number, _) => *number,
            Sorted::String(text) => strtod(text),
            Sorted::Array(_) => 0.0,
        }
    }
}

/// gawk's words for an argument that should be an array and is a scalar: `what` is not
/// an array.
pub(crate) fn not_an_array(error: String, what: &str) -> String {
    if error == super::stack::SCALAR_IN_ARRAY_CONTEXT {
        format!("{what} is not an array")
    } else {
        error
    }
}

pub(crate) fn builtin_gsub(
    stack: &mut Stack,
    global_env: &mut GlobalEnv,
    is_sub: bool,
) -> Result<FieldsState, String> {
    let repl = stack
        .pop_scalar_value()?
        .scalar_to_string(&global_env.convfmt)?;
    let ere = stack.pop_scalar_value()?.into_ere(&global_env.convfmt)?;
    // A constant target is a value, replaced in a copy that is dropped.
    if stack.top_is_value() {
        let text = stack
            .pop_scalar_value()?
            .scalar_to_string(&global_env.convfmt)?;
        let (_, count) = gsub(&ere, &repl, &text, is_sub)?;
        stack.push_value(count as f64)?;
        return Ok(FieldsState::Ok);
    }
    let in_str = stack.pop_scalar_ref()?;
    let (result, count) = gsub(
        &ere,
        &repl,
        &in_str.clone().scalar_to_string(&global_env.convfmt)?,
        is_sub,
    )?;
    let result = in_str.assign(result, global_env);
    stack.push_value(count as f64)?;
    result
}

pub(crate) fn call_simple_builtin(
    function: BuiltinFunction,
    argc: u16,
    stack: &mut Stack,
    global_env: &mut GlobalEnv,
) -> Result<FieldsState, String> {
    match function {
        BuiltinFunction::Atan2 => {
            let x = stack.pop_scalar_value()?.scalar_as_f64();
            let y = stack.pop_scalar_value()?.scalar_as_f64();
            stack.push_value(y.atan2(x))?;
        }
        BuiltinFunction::Cos => {
            let value = stack.pop_scalar_value()?.scalar_as_f64();
            stack.push_value(value.cos())?;
        }
        BuiltinFunction::Sin => {
            let value = stack.pop_scalar_value()?.scalar_as_f64();
            stack.push_value(value.sin())?;
        }
        BuiltinFunction::Int => {
            let value = stack.pop_scalar_value()?.scalar_as_f64();
            stack.push_value(value.trunc())?;
        }
        BuiltinFunction::Index => {
            let t = stack
                .pop_scalar_value()?
                .scalar_to_string(&global_env.convfmt)?;
            let s = stack
                .pop_scalar_value()?
                .scalar_to_string(&global_env.convfmt)?;
            // index() returns a character position, numbering from 1; str::find
            // reports a byte offset, so convert it to a character count.
            // With IGNORECASE, in the text with case folded, which has the same characters
            // in the same places.
            let (s, t) = if global_env.ignore_case {
                (fold_case(&s).into_owned(), fold_case(&t).into_owned())
            } else {
                (s.to_string(), t.to_string())
            };
            let index = s
                .find(&t)
                .map(|i| byte_offset_to_char_count(&s, i) as f64 + 1.0)
                .unwrap_or(0.0);
            stack.push_value(index)?;
        }
        BuiltinFunction::Length => match stack.pop_array_or_scalar(true)? {
            Ok(len) => stack.push_value(len as f64)?,
            Err(value) => {
                // length() counts characters, not bytes.
                let value_str = value.scalar_to_string(&global_env.convfmt)?;
                stack.push_value(value_str.chars().count() as f64)?;
            }
        },
        BuiltinFunction::IsArray => {
            let is_array = stack.pop_array_or_scalar(false)?.is_ok();
            stack.push_value(if is_array { 1.0 } else { 0.0 })?;
        }
        BuiltinFunction::Split => return builtin_split(stack, global_env, argc),
        BuiltinFunction::Sprintf => {
            // gawk's fatal error, met when the call runs.
            if argc == 0 {
                return Err("sprintf: no arguments".to_string());
            }
            let str = builtin_sprintf(stack, argc, global_env)?;
            stack.push_value(str)?;
        }
        BuiltinFunction::Substr => {
            let n = if argc == 2 {
                None
            } else {
                Some(stack.pop_scalar_value()?.scalar_as_f64().trunc() as i64)
            };
            let m = stack.pop_scalar_value()?.scalar_as_f64().trunc() as i64;
            let s = stack
                .pop_scalar_value()?
                .scalar_to_string(&global_env.convfmt)?;
            stack.push_value(substr(s.as_str(), m, n))?;
        }
        BuiltinFunction::ToLower => {
            // POSIX: case mapping follows the LC_CTYPE category of the locale.
            let value = stack
                .pop_scalar_value()?
                .scalar_to_string(&global_env.convfmt)?;
            let lowered: String = value.to_lowercase();
            stack.push_value(lowered)?;
        }
        BuiltinFunction::ToUpper => {
            let value = stack
                .pop_scalar_value()?
                .scalar_to_string(&global_env.convfmt)?;
            let uppered: String = value.to_uppercase();
            stack.push_value(uppered)?;
        }
        BuiltinFunction::Gsub | BuiltinFunction::Sub => {
            return builtin_gsub(stack, global_env, function == BuiltinFunction::Sub);
        }
        BuiltinFunction::System => {
            let command = stack
                .pop_scalar_value()?
                .scalar_to_string(&global_env.convfmt)?;
            stack.push_value(run_system(&command)? as f64)?;
        }
        BuiltinFunction::Print => {
            super::io::write_stdout(&print_to_string(stack, argc, global_env)?, "print")?;
        }
        BuiltinFunction::Printf => {
            super::io::write_stdout(&builtin_sprintf(stack, argc, global_env)?, "printf")?;
        }
        // `call_builtin` takes the functions that need the interpreter's state before it
        // passes the rest here; one of those would be malformed code, an error rather
        // than a panic.
        _ => return Err("a builtin function called out of place".to_string()),
    }
    Ok(FieldsState::Ok)
}

/// Run `command` via shell process and translate its status into awk's `system()` return code.
fn run_system(command: &str) -> Result<i32, String> {
    let mut command_proc = super::io::create_shell_command(command)?;

    Ok(match command_proc.status() {
        Ok(status) => status.code().unwrap_or(-1),
        Err(_) => -1,
    })
}

pub(crate) fn gather_values(stack: &mut Stack, count: u16) -> Result<Vec<AwkValue>, String> {
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        values.push(stack.pop_scalar_value()?);
    }
    Ok(values)
}

pub(crate) fn print_to_string(
    stack: &mut Stack,
    argc: u16,
    global_env: &GlobalEnv,
) -> Result<AwkString, String> {
    let mut values = Vec::with_capacity(argc as usize);
    for _ in 0..argc {
        values.push(
            stack
                .pop_scalar_value()?
                .scalar_to_string(&global_env.ofmt)?,
        );
    }
    let mut output = String::new();
    for elem in values.iter().skip(1).rev() {
        output.push_str(elem);
        output.push_str(&global_env.ofs);
    }
    // The compiler gives print at least one argument, `$0` when it has none.
    output.push_str(
        values
            .first()
            .ok_or_else(|| "print called without arguments".to_string())?,
    );
    // Restore the CRLF of the most recent main-input record, but only for the
    // default ORS; an explicit ORS (and all printf output) is written verbatim.
    if global_env.last_record_crlf && global_env.ors.as_str() == "\n" {
        output.push_str("\r\n");
    } else {
        output.push_str(&global_env.ors);
    }
    Ok(output.into())
}
