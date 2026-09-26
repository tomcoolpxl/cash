use clap::Parser;
use std::{ffi::OsString, io::Write, ops::ControlFlow};
use uucore::format;

use cash_core::{Error, ErrorKind, ExecutionResult, builtins, escape, expansion};

/// Format a string.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct PrintfCommand {
    /// If specified, the output of the command is assigned to this variable.
    #[arg(short = 'v')]
    output_variable: Option<String>,

    /// Format string + arguments to the format string.
    ///
    /// N.B. We intentionally do *not* enable `allow_hyphen_values` here. Doing so would
    /// cause an attached short-option value such as `-va` (i.e. `-v a`) to be misparsed as
    /// a positional argument. With it disabled, a format string that genuinely needs to
    /// start with a hyphen must be preceded by `--`, matching other shells' behavior.
    #[arg(trailing_var_arg = true, required = true)]
    format_and_args: Vec<String>,
}

impl builtins::Command for PrintfCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self
            .format_and_args
            .first()
            .is_some_and(|fmt| has_count_spec(fmt))
        {
            let mut output = Vec::new();
            let mut counts = Vec::new();
            if let Some((fmt, args)) = self.format_and_args.split_first() {
                format_via_uucore_with_counts(fmt, args.iter(), &mut output, &mut counts)?;
            }
            for (name, count) in counts {
                if name.is_empty() {
                    continue;
                }
                expansion::assign_to_named_parameter_in_builtin(
                    context.shell,
                    &context.params,
                    &name,
                    count.to_string(),
                )
                .await?;
            }
            if let Some(variable_name) = &self.output_variable {
                let result = String::from_utf8(output).map_err(|_| {
                    cash_core::ErrorKind::PrintfInvalidUsage("invalid UTF-8 output".into())
                })?;
                expansion::assign_to_named_parameter_in_builtin(
                    context.shell,
                    &context.params,
                    variable_name,
                    result,
                )
                .await?;
            } else {
                context.stdout().write_all(&output)?;
                context.stdout().flush()?;
            }
            return Ok(ExecutionResult::success());
        }
        if let Some(variable_name) = &self.output_variable {
            // Format to a u8 vector.
            let mut result: Vec<u8> = vec![];
            format(self.format_and_args.as_slice(), &mut result)?;

            // Convert to a string.
            let result_str = String::from_utf8(result).map_err(|_| {
                cash_core::ErrorKind::PrintfInvalidUsage("invalid UTF-8 output".into())
            })?;

            // Assign to the selected variable.
            expansion::assign_to_named_parameter_in_builtin(
                context.shell,
                &context.params,
                variable_name,
                result_str,
            )
            .await?;
        } else {
            format(self.format_and_args.as_slice(), context.stdout())?;
            context.stdout().flush()?;
        }

        Ok(ExecutionResult::success())
    }
}

fn format(format_and_args: &[String], writer: impl Write) -> Result<(), cash_core::Error> {
    match format_and_args {
        // Handle format string with arguments using uucore
        [fmt, args @ ..] => format_via_uucore(fmt, args.iter(), writer),
        // Handle case with no format string (we shouldn't be able to get here since clap will
        // fail parsing when the format string is missing)
        [] => Err(ErrorKind::PrintfInvalidUsage("missing operand".into()).into()),
    }
}

fn has_count_spec(fmt: &str) -> bool {
    let bytes = fmt.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            index += 1;
            if bytes.get(index) == Some(&b'%') {
                index += 1;
                continue;
            }
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
            if bytes.get(index) == Some(&b'n') {
                return true;
            }
        }
        index += 1;
    }
    false
}

fn format_via_uucore(
    format_string: &str,
    args: impl Iterator<Item = impl Into<OsString>>,
    writer: impl Write,
) -> Result<(), cash_core::Error> {
    format_via_uucore_with_counts(format_string, args, writer, &mut Vec::new())
}

fn format_via_uucore_with_counts(
    format_string: &str,
    args: impl Iterator<Item = impl Into<OsString>>,
    mut writer: impl Write,
    counts: &mut Vec<(String, usize)>,
) -> Result<(), cash_core::Error> {
    // Convert string arguments to FormatArgument::Unparsed
    let format_args: Vec<_> = args
        .map(|s| format::FormatArgument::Unparsed(s.into()))
        .collect();

    // Parse format string once.
    let format_items = parse_format_string(format_string)?;

    // Wrap the format arguments.
    let mut format_args_wrapper = format::FormatArguments::new(&format_args);

    // Determine whether the format string contains any specifiers that consume arguments. If it
    // doesn't, then we must only run through it once -- even when extra arguments are provided --
    // since otherwise we'd loop forever waiting for arguments that will never be consumed. This
    // matches the behavior of other shells, which print such a format string exactly once.
    let format_consumes_args = format_items.iter().any(|(item, _, _)| {
        item.as_ref()
            .is_none_or(|item| matches!(item, format::FormatItem::Spec(_)))
    });

    let mut written = 0usize;

    // Keep going until we've exhausted all format arguments. Also make sure to run at least once
    // even if there's no format arguments.
    while format_args.is_empty() || !format_args_wrapper.is_exhausted() {
        // Process all format items, in order. We'll bail when we're told to stop.
        for (item, backslash_quote, quoted_format) in &format_items {
            if item.is_none() {
                let mut string_spec = b"s".as_slice();
                let format::Spec::String { position, .. } =
                    format::Spec::parse(&mut string_spec)
                        .map_err(|_| ErrorKind::PrintfInvalidUsage("invalid %n".into()))?
                else {
                    return Err(ErrorKind::PrintfInvalidUsage("invalid %n".into()).into());
                };
                let name = format_args_wrapper
                    .next_string(position)
                    .to_string_lossy()
                    .to_string();
                counts.push((name, written));
                continue;
            }
            let Some(item) = item else { continue };
            if let Some(quoted_format) = quoted_format {
                let format::FormatItem::Spec(format::Spec::QuotedString { position }) = item else {
                    return Err(
                        ErrorKind::PrintfInvalidUsage("invalid quoted format".into()).into(),
                    );
                };
                let arg = format_args_wrapper.next_string(*position).to_string_lossy();
                let rendered = quoted_format.render(&arg);
                write!(writer, "{rendered}")?;
                written += rendered.len();
                continue;
            }
            if let (format::FormatItem::Spec(format::Spec::QuotedString { position }), true) =
                (item, *backslash_quote)
            {
                let arg = format_args_wrapper.next_string(*position).to_string_lossy();
                let quoted = quote_printf_q(&arg);
                write!(writer, "{quoted}")?;
                written += quoted.len();
                continue;
            }

            let mut counted = CountWriter {
                inner: &mut writer,
                written: &mut written,
            };
            let control_flow = item
                .write(&mut counted, &mut format_args_wrapper)
                .map_err(|e| match e {
                    // Propagate I/O errors directly so they can be handled appropriately
                    format::FormatError::IoError(io_err) => Error::from(io_err),
                    // Wrap other format errors
                    other => Error::from(ErrorKind::PrintfInvalidUsage(std::format!(
                        "printf formatting error: {other}"
                    ))),
                })?;

            if control_flow == ControlFlow::Break(()) {
                break;
            }
        }

        // If the format string doesn't consume any arguments, stop now; otherwise we'd reprocess
        // it forever since no arguments will ever be consumed.
        if !format_consumes_args {
            break;
        }

        // Start next batch if not exhausted
        if !format_args_wrapper.is_exhausted() {
            format_args_wrapper.start_next_batch();
        }

        if format_args.is_empty() {
            break;
        }
    }

    Ok(())
}

/// The alternate form of `%q` (Bash 5.3 `%#q`): always single-quoted, even when no
/// quoting is needed. Strings with control characters still need `$'...'`.
fn force_single_quote(s: &str) -> String {
    let dollar_quoted = quote_printf_q(s);
    if dollar_quoted.starts_with("$'") {
        return dollar_quoted;
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn quote_printf_q(s: &str) -> String {
    let quoted = escape::quote_if_needed(s, escape::QuoteMode::BackslashEscape);
    if quoted.starts_with("$'") {
        return quoted.into_owned();
    }

    let mut quoted = quoted.replace(":~", ":\\~").replace("=~", "=\\~");
    if matches!(quoted.as_bytes().first(), Some(b'~' | b'#')) {
        quoted.insert(0, '\\');
    }
    quoted
}

type ParsedFormatItem = (
    Option<format::FormatItem<format::EscapedChar>>,
    bool,
    Option<QuotedFormat>,
);

/// A `%q`/`%Q` spec with fixed modifiers, or a `%s` with the `0` flag, which uucore
/// rejects but Bash accepts: `%05s` pads the string with zeros, and `-` overrides `0`.
#[derive(Clone, Copy)]
struct QuotedFormat {
    uppercase: bool,
    alternate: bool,
    /// A zero-flagged `%s`: printed as is, padded with `0` on the left.
    zero_padded_string: bool,
    left_align: bool,
    width: Option<usize>,
    precision: Option<usize>,
}

impl QuotedFormat {
    fn parse(spec: &[u8]) -> Option<Self> {
        let (&conversion, body) = spec.split_last()?;
        if !matches!(conversion, b'q' | b'Q' | b's') || body.contains(&b'*') || body.contains(&b'$')
        {
            return None;
        }

        let mut index = 0;
        let mut alternate = false;
        let mut left_align = false;
        while let Some(flag) = body.get(index).copied().filter(|c| b"-+ #0".contains(c)) {
            alternate |= flag == b'#';
            left_align |= flag == b'-';
            index += 1;
        }

        let width_start = index;
        while body.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        let width = (index > width_start)
            .then(|| {
                std::str::from_utf8(&body[width_start..index])
                    .ok()?
                    .parse()
                    .ok()
            })
            .flatten();

        let precision = if body.get(index) == Some(&b'.') {
            index += 1;
            let start = index;
            while body.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
            Some(if index == start {
                0
            } else {
                std::str::from_utf8(&body[start..index])
                    .ok()?
                    .parse()
                    .ok()?
            })
        } else {
            None
        };

        (index == body.len()).then_some(Self {
            uppercase: conversion == b'Q',
            alternate,
            zero_padded_string: conversion == b's',
            left_align,
            width,
            precision,
        })
    }

    fn render(self, argument: &str) -> String {
        if self.zero_padded_string {
            let mut text = truncate_chars(argument, self.precision);
            let padding = self.width.unwrap_or(0).saturating_sub(text.chars().count());
            if self.left_align {
                text.push_str(&" ".repeat(padding));
            } else {
                text.insert_str(0, &"0".repeat(padding));
            }
            return text;
        }

        let source = if self.uppercase {
            truncate_chars(argument, self.precision)
        } else {
            argument.to_owned()
        };
        let mut quoted = if self.alternate {
            force_single_quote(&source)
        } else {
            quote_printf_q(&source)
        };
        if !self.uppercase {
            quoted = truncate_chars(&quoted, self.precision);
        }

        if let Some(width) = self.width {
            let padding = width.saturating_sub(quoted.chars().count());
            if padding > 0 {
                let spaces = " ".repeat(padding);
                if self.left_align {
                    quoted.push_str(&spaces);
                } else {
                    quoted.insert_str(0, &spaces);
                }
            }
        }
        quoted
    }
}

fn truncate_chars(value: &str, precision: Option<usize>) -> String {
    precision.map_or_else(
        || value.to_owned(),
        |limit| value.chars().take(limit).collect(),
    )
}

struct CountWriter<'a, W: Write> {
    inner: &'a mut W,
    written: &'a mut usize,
}

impl<W: Write> Write for CountWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let count = self.inner.write(buf)?;
        *self.written += count;
        Ok(count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn parse_format_string(format_string: &str) -> Result<Vec<ParsedFormatItem>, cash_core::Error> {
    let format_items: Result<Vec<_>, _> = format::parse_spec_and_escape(format_string.as_bytes())
        .map(|result| match result {
            Ok(item @ format::FormatItem::Spec(format::Spec::QuotedString { .. })) => {
                Ok((Some(item), true, None))
            }
            Ok(item) => Ok((Some(item), false, None)),
            Err(format::FormatError::SpecError(spec, _))
                if spec.last() == Some(&b'n')
                    && spec[..spec.len() - 1].iter().all(u8::is_ascii_digit) =>
            {
                Ok((None, false, None))
            }
            // Fixed q/Q modifiers are deliberately ignored; dynamic modifiers remain unsupported
            // until uucore exposes quoted-string metadata.
            Err(format::FormatError::SpecError(spec, span))
                if (matches!(spec.last(), Some(b'q' | b'Q'))
                    || (spec.last() == Some(&b's') && spec.contains(&b'0')))
                    && !spec.contains(&b'*')
                    && !spec.contains(&b'$') =>
            {
                let quoted_format = QuotedFormat::parse(&spec)
                    .ok_or_else(|| format::FormatError::SpecError(spec.clone(), span.clone()))?;
                let mut bare_q: &[u8] = b"q";
                let item = format::Spec::parse(&mut bare_q)
                    .map(format::FormatItem::Spec)
                    .map_err(|spec| format::FormatError::SpecError(spec.to_vec(), span))?;
                Ok((Some(item), false, Some(quoted_format)))
            }
            Err(error) => Err(error),
        })
        .collect();

    // Observe any errors we encountered along the way.
    let format_items = format_items
        .map_err(|e| ErrorKind::PrintfInvalidUsage(format!("printf parsing error: {e}")))?;

    Ok(format_items)
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use anyhow::Result;

    fn sprintf_via_uucore(
        format_string: &str,
        args: impl Iterator<Item = impl Into<OsString>>,
    ) -> Result<String> {
        let mut result = vec![];
        format_via_uucore(format_string, args, &mut result)?;

        Ok(String::from_utf8(result)?)
    }

    #[test]
    fn test_basic_sprintf() -> Result<()> {
        assert_eq!(sprintf_via_uucore("%s", std::iter::once(&"xyz"))?, "xyz");
        assert_eq!(sprintf_via_uucore(r"%d\n", std::iter::once(&"1"))?, "1\n");

        Ok(())
    }

    #[test]
    fn test_sprintf_without_args() -> Result<()> {
        let empty: [&str; 0] = [];

        assert_eq!(sprintf_via_uucore("xyz", empty.iter())?, "xyz");
        assert_eq!(sprintf_via_uucore("%s|", empty.iter())?, "|");

        Ok(())
    }

    #[test]
    fn test_sprintf_with_cycles() -> Result<()> {
        assert_eq!(sprintf_via_uucore("%s|", ["x", "y"].iter())?, "x|y|");

        Ok(())
    }
}
