use cash_core::timefmt::Zone;
use clap::Parser;
use std::{ffi::OsString, io::Write, ops::ControlFlow};
use uucore::format;

use cash_core::{Error, ErrorKind, ExecutionResult, builtins, escape, expansion};

mod numbers;

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

    /// Parsed by hand, as Bash's `internal_getopt` parses `-v NAME`: clap's parse was most
    /// of what a `printf` in a loop cost over `true`, and its arguments change from call
    /// to call, so it cannot be remembered as `read`'s is. A bad option is clap's error of
    /// the kind clap gave, which the builtins say in Bash's words: `printf: -x: invalid
    /// option`, `-v: option requires an argument`, and the usage line.
    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        use clap::error::{ContextKind, ContextValue, ErrorKind};

        let mut args = args.into_iter().skip(1).peekable();
        let mut output_variable = None;
        while let Some(option) = args.next_if(|arg| arg.len() > 1 && arg.starts_with('-')) {
            if option == "--" {
                break;
            }
            let letters = option.strip_prefix('-').unwrap_or_default();
            if let Some(attached) = letters.strip_prefix('v') {
                let name = if attached.is_empty() {
                    args.next().ok_or_else(|| {
                        let mut error = clap::Error::new(ErrorKind::InvalidValue);
                        error.insert(
                            ContextKind::InvalidArg,
                            ContextValue::String("-v".to_owned()),
                        );
                        error.insert(
                            ContextKind::InvalidValue,
                            ContextValue::String(String::new()),
                        );
                        error
                    })?
                } else {
                    attached.to_owned()
                };
                output_variable = Some(name);
            } else {
                // The first letter is the one Bash names; `--long` is said as `--`.
                let named = letters
                    .chars()
                    .next()
                    .filter(|&letter| letter != '-')
                    .map_or_else(|| option.clone(), |letter| format!("-{letter}"));
                let mut error = clap::Error::new(ErrorKind::UnknownArgument);
                error.insert(ContextKind::InvalidArg, ContextValue::String(named));
                return Err(error);
            }
        }
        let format_and_args: Vec<String> = args.collect();
        if format_and_args.is_empty() {
            return Err(clap::Error::new(ErrorKind::MissingRequiredArgument));
        }
        Ok(Self {
            output_variable,
            format_and_args,
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let Some((fmt, args)) = self.format_and_args.split_first() else {
            return Err(ErrorKind::PrintfInvalidUsage("missing operand".into()).into());
        };
        // The format's escapes as Bash reads them, before uucore reads the rest.
        let (format, warning) = bash_format(fmt);
        say_bad_conversion(
            &context,
            warning.map(str::to_owned),
            ExecutionResult::success(),
        )?;
        let fmt = &format;
        // `-v` names a variable or an array's element (`a[1]`), which Bash checks before
        // it formats anything; a bad name said "failed to parse parameter", status 1.
        if let Some(name) = &self.output_variable
            && !assignable(name)
        {
            writeln!(
                context.error_stream(),
                "{}: `{name}': not a valid identifier",
                context.command_name
            )?;
            return Ok(cash_core::ExecutionExitCode::InvalidUsage.into());
        }
        // Numbers are read here, as Bash reads them, and what is wrong with one is said
        // here, on the builtin's standard error (BI-09).
        let (args, complaints) = numbers::prepare(fmt, args);
        let mut say = |batch: usize| -> Result<(), Error> {
            for (_, complaint) in complaints.iter().filter(|(of, _)| *of == batch) {
                writeln!(
                    context.error_stream(),
                    "{}: {complaint}",
                    context.command_name
                )?;
            }
            Ok(())
        };
        let status = if complaints.is_empty() {
            ExecutionResult::success()
        } else {
            ExecutionResult::general_error()
        };

        // `%(…)T` shows times in the zone `TZ` names (the user, 2026-10-04).
        let zone = cash_core::timefmt::Zone::of_shell(context.shell);

        if has_count_spec(fmt) {
            let mut output = Vec::new();
            let mut counts = Vec::new();
            let bad = format_via_uucore_with_counts(
                fmt,
                &zone,
                args.iter().cloned(),
                &mut output,
                &mut counts,
                &mut say,
            )?;
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
            return say_bad_conversion(&context, bad, status);
        }
        let bad = if let Some(variable_name) = &self.output_variable {
            // Format to a u8 vector.
            let mut result: Vec<u8> = vec![];
            let bad = format_via_uucore(fmt, &zone, args.iter().cloned(), &mut result, &mut say)?;

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
            bad
        } else {
            let bad =
                print_formatted(fmt, &zone, args.iter().cloned(), context.stdout(), &mut say)?;
            context.stdout().flush()?;
            bad
        };

        say_bad_conversion(&context, bad, status)
    }
}

/// Says what Bash says of a conversion that is none, `bad`, once the text before it is
/// out, and fails with 1; `status` otherwise.
fn say_bad_conversion<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    bad: Option<String>,
    status: ExecutionResult,
) -> Result<ExecutionResult, Error> {
    let Some(bad) = bad else {
        return Ok(status);
    };
    writeln!(context.error_stream(), "{}: {bad}", context.command_name)?;
    Ok(ExecutionResult::general_error())
}

/// `format_via_uucore` into `stdout`, each pass of the format held and written at once,
/// as bash's printf writes a line: a program reading a pipe a write at a time sees
/// `v\n` whole, not `v` and then an empty line.
fn print_formatted(
    format_string: &str,
    zone: &Zone,
    args: impl Iterator<Item = impl Into<OsString>>,
    stdout: impl Write,
    before_pass: &mut dyn FnMut(usize) -> Result<(), Error>,
) -> Result<Option<String>, cash_core::Error> {
    let mut out = std::io::BufWriter::with_capacity(64 * 1024, stdout);
    let bad = format_via_uucore(format_string, zone, args, &mut out, before_pass)?;
    out.flush()?;
    Ok(bad)
}

/// Whether `-v` may assign to `name`: a variable's name, or an array element's, a name
/// with a subscript in brackets after it.
fn assignable(name: &str) -> bool {
    match name.split_once('[') {
        Some((array, subscript)) => {
            subscript.ends_with(']') && cash_core::env::valid_variable_name(array)
        }
        None => cash_core::env::valid_variable_name(name),
    }
}

/// The format as Bash's printf reads its escapes, for uucore's parser, which reads
/// coreutils': `\'`, `\"`, `\?` and `\e` are their characters, `\u` and `\U` take one to
/// four or eight hex digits, `\c` is a backslash and a `c` (it ended all output), and a
/// `\x` without a hex digit is a backslash and an `x`, with the warning it comes back with.
/// `%ls` and `%lc` become `%S` and `%C`, wide strings and characters counted in characters
/// (they counted bytes). The rest is left to uucore, which reads it as Bash does, the bytes
/// of `\xff` and `\377` too.
fn bash_format(format: &str) -> (String, Option<&'static str>) {
    // A character an escape gave, which uucore is to take as it is.
    fn literal(out: &mut String, c: char) {
        match c {
            '%' => out.push_str("%%"),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    let mut out = String::with_capacity(format.len());
    let mut warning = None;
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                None => out.push_str("\\\\"),
                Some(quote @ ('\'' | '"' | '?')) => out.push(quote),
                Some('e' | 'E') => out.push('\x1b'),
                Some('c') => out.push_str("\\\\c"),
                Some(unicode @ ('u' | 'U')) => {
                    let most = if unicode == 'u' { 4 } else { 8 };
                    let mut digits = String::new();
                    while digits.len() < most
                        && let Some(digit) = chars.next_if(char::is_ascii_hexdigit)
                    {
                        digits.push(digit);
                    }
                    if let Some(ch) = u32::from_str_radix(&digits, 16)
                        .ok()
                        .and_then(char::from_u32)
                    {
                        literal(&mut out, ch);
                    } else {
                        out.push_str("\\\\");
                        out.push(unicode);
                        out.push_str(&digits);
                    }
                }
                Some('x') if !chars.peek().is_some_and(char::is_ascii_hexdigit) => {
                    warning = Some("missing hex digit for \\x");
                    out.push_str("\\\\x");
                }
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
            },
            '%' => {
                out.push('%');
                if let Some(percent) = chars.next_if_eq(&'%') {
                    out.push(percent);
                    continue;
                }
                while let Some(part) =
                    chars.next_if(|c| c.is_ascii_digit() || "-+ #.*$'".contains(*c))
                {
                    out.push(part);
                }
                if chars.peek() == Some(&'l') {
                    let mut ahead = chars.clone();
                    ahead.next();
                    match ahead.next() {
                        Some('s') => {
                            chars = ahead;
                            out.push('S');
                        }
                        Some('c') => {
                            chars = ahead;
                            out.push('C');
                        }
                        _ => {}
                    }
                }
            }
            c => out.push(c),
        }
    }
    (out, warning)
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

/// `before_pass` is told the number of each pass over the format before it starts. What is
/// said of a conversion that is none comes back, once the text before it is written.
fn format_via_uucore(
    format_string: &str,
    zone: &Zone,
    args: impl Iterator<Item = impl Into<OsString>>,
    writer: impl Write,
    before_pass: &mut dyn FnMut(usize) -> Result<(), Error>,
) -> Result<Option<String>, cash_core::Error> {
    format_via_uucore_with_counts(
        format_string,
        zone,
        args,
        writer,
        &mut Vec::new(),
        before_pass,
    )
}

fn format_via_uucore_with_counts(
    format_string: &str,
    zone: &Zone,
    args: impl Iterator<Item = impl Into<OsString>>,
    mut writer: impl Write,
    counts: &mut Vec<(String, usize)>,
    before_pass: &mut dyn FnMut(usize) -> Result<(), Error>,
) -> Result<Option<String>, cash_core::Error> {
    // Convert string arguments to FormatArgument::Unparsed
    let format_args: Vec<_> = args
        .map(|s| format::FormatArgument::Unparsed(s.into()))
        .collect();

    // Parse format string once.
    let (format_items, bad) = parse_format_string(format_string)?;

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
    let mut pass = 0;
    while format_args.is_empty() || !format_args_wrapper.is_exhausted() {
        writer.flush()?;
        before_pass(pass)?;
        pass += 1;
        // Process all format items, in order. We'll bail when we're told to stop.
        for (item, backslash_quote, custom) in &format_items {
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
            if let Some(custom) = custom {
                let format::FormatItem::Spec(format::Spec::QuotedString { position }) = item else {
                    return Err(
                        ErrorKind::PrintfInvalidUsage("invalid quoted format".into()).into(),
                    );
                };
                // Where `$` cannot be, `position` is the next argument, a `*`'s too.
                let next = || {
                    format_args_wrapper
                        .next_string(*position)
                        .to_string_lossy()
                        .into_owned()
                };
                let rendered = custom.render(next, zone);
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

            // A `\c` ends all output, not only this pass over the format: the
            // arguments left over were formatted again (BI-10).
            if control_flow == ControlFlow::Break(()) {
                return Ok(None);
            }
        }

        // A conversion that is none ends the output there, as in Bash.
        if bad.is_some() {
            return Ok(bad);
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

    Ok(None)
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
    Option<Custom>,
);

/// A spec uucore does not format, rendered here from the argument it reads.
enum Custom {
    Quoted(QuotedFormat),
    Time(TimeFormat),
}

impl Custom {
    /// The item, from the arguments `next` hands out: a `*` width or precision first, as in
    /// Bash, then the value.
    fn render(&self, mut next: impl FnMut() -> String, zone: &Zone) -> String {
        match self {
            Self::Quoted(quoted) => {
                let mut star = || next().trim().parse::<i64>().unwrap_or(0);
                let mut quoted = *quoted;
                if quoted.width_star {
                    let width = star();
                    quoted.left_align |= width < 0;
                    quoted.width = usize::try_from(width.unsigned_abs()).ok();
                }
                if quoted.precision_star {
                    quoted.precision = usize::try_from(star()).ok();
                }
                quoted.render(&next())
            }
            Self::Time(time) => time.render(&next(), zone),
        }
    }
}

/// Bash's `%(FMT)T`: the argument, a number of seconds since 1970, as strftime formats it
/// by FMT; `-1`, or no argument, is now, and `-2` when the shell started. A width, `-`
/// and a precision apply to the text, as to `%s`.
struct TimeFormat {
    format: String,
    left_align: bool,
    width: Option<usize>,
    precision: Option<usize>,
}

impl TimeFormat {
    /// The spec between `%` and `(`: an optional `N$`, flags, a width and a precision.
    fn parse(spec: &[u8], format: &str) -> Option<(Self, Option<usize>)> {
        let text = std::str::from_utf8(spec).ok()?;
        let (position, text) = match text.split_once('$') {
            Some((digits, rest)) => (Some(digits.parse().ok()?), rest),
            None => (None, text),
        };
        let rest = text.trim_start_matches(['-', '+', ' ', '#', '0']);
        let left_align = text
            .strip_suffix(rest)
            .is_some_and(|flags| flags.contains('-'));
        let (width, precision) = match rest.split_once('.') {
            Some((width, precision)) => (width, Some(precision)),
            None => (rest, None),
        };
        let number = |digits: &str| -> Option<Option<usize>> {
            if digits.is_empty() {
                Some(None)
            } else {
                digits.parse().ok().map(Some)
            }
        };
        let width = number(width)?;
        let precision = match precision {
            Some(digits) => Some(number(digits)?.unwrap_or(0)),
            None => None,
        };
        Some((
            Self {
                format: format.to_owned(),
                left_align,
                width,
                precision,
            },
            position,
        ))
    }

    /// `argument` is a clean decimal (see `numbers`), or empty when there was none.
    fn render(&self, argument: &str, zone: &Zone) -> String {
        let when = match argument.parse::<i64>().unwrap_or(0) {
            _ if argument.is_empty() => Some(chrono::Utc::now()),
            -1 => Some(chrono::Utc::now()),
            -2 => Some(chrono::DateTime::<chrono::Utc>::from(
                cash_core::timefmt::shell_started(),
            )),
            seconds => chrono::DateTime::from_timestamp(seconds, 0),
        };
        // An empty format is the locale's time, as in Bash.
        let format = if self.format.is_empty() {
            "%X"
        } else {
            &self.format
        };
        let mut text = truncate_chars(
            &when.map_or_else(String::new, |when| zone.format(when, format)),
            self.precision,
        );
        let padding = self.width.unwrap_or(0).saturating_sub(text.chars().count());
        if self.left_align {
            text.push_str(&" ".repeat(padding));
        } else {
            text.insert_str(0, &" ".repeat(padding));
        }
        text
    }
}

/// Where a `%(FMT)T` that opens at `open`, the `(`, ends: past its `T`. `None` when its
/// `)` is missing or not followed by `T`. Parentheses in FMT nest, as Bash counts them
/// (`%(%Y (x))T`); the first `)` ended it.
fn time_format_end(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0_usize;
    for (at, &byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return (bytes.get(at + 1) == Some(&b'T')).then_some(at + 2);
                }
            }
            _ => {}
        }
    }
    None
}

/// The first `%(FMT)T` in `format`: what is before it, the spec between `%` and `(`, FMT,
/// and what is after it. `Err` names a time spec that does not end.
fn split_time_spec(format: &str) -> Result<Option<(&str, &str, &str, &str)>, String> {
    let bytes = format.as_bytes();
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'\\' => at += 2,
            b'%' if bytes.get(at + 1) == Some(&b'%') => at += 2,
            b'%' => {
                let start = at;
                at += 1;
                while bytes
                    .get(at)
                    .is_some_and(|b| b.is_ascii_digit() || b"$-+ #0.".contains(b))
                {
                    at += 1;
                }
                if bytes.get(at) == Some(&b'(') {
                    let open = at;
                    let pieces = time_format_end(bytes, open).and_then(|end| {
                        Some((
                            format.get(..start)?,
                            format.get(start + 1..open)?,
                            format.get(open + 1..end - 2)?,
                            format.get(end..)?,
                        ))
                    });
                    return pieces.map(Some).ok_or_else(|| {
                        format!(
                            "`{}': invalid time format specification",
                            format.get(start..).unwrap_or(format)
                        )
                    });
                }
            }
            _ => at += 1,
        }
    }
    Ok(None)
}

/// A `%q`/`%Q` spec with fixed modifiers, or a `%s` with the `0` flag, which uucore
/// rejects but Bash accepts: `%05s` pads the string with zeros, and `-` overrides `0`. Also
/// `%b` with a width or precision, and `%S` and `%C` (`%ls`, `%lc`), counted in characters,
/// which were a parsing error.
#[derive(Clone, Copy)]
struct QuotedFormat {
    /// The conversion: `q`, `Q`, `s`, `b`, `S` or `C`.
    conversion: u8,
    uppercase: bool,
    alternate: bool,
    /// A zero-flagged `%s`: printed as is, padded with `0` on the left.
    zero_padded_string: bool,
    left_align: bool,
    width: Option<usize>,
    precision: Option<usize>,
    /// `*`: the width is the argument before the value; `-` aligns left.
    width_star: bool,
    /// `.*`: the precision is the argument before the value; one below 0 is none.
    precision_star: bool,
}

impl QuotedFormat {
    fn parse(spec: &[u8]) -> Option<Self> {
        let (&conversion, body) = spec.split_last()?;
        if !matches!(conversion, b'q' | b'Q' | b's' | b'b' | b'S' | b'C') || body.contains(&b'$') {
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

        let width_star = body.get(index) == Some(&b'*');
        if width_star {
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

        let mut precision_star = false;
        let precision = if body.get(index) == Some(&b'.') {
            index += 1;
            precision_star = body.get(index) == Some(&b'*');
            if precision_star {
                index += 1;
            }
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
            width_star,
            precision_star,
            conversion,
            uppercase: conversion == b'Q',
            alternate,
            zero_padded_string: conversion == b's',
            left_align,
            width,
            precision,
        })
    }

    fn render(self, argument: &str) -> String {
        // `%b`'s escapes are `echo -e`'s; then it is a string as `%S` is, in characters.
        let text = match self.conversion {
            b'b' => Some(
                cash_core::escape::expand_backslash_escapes(
                    argument,
                    cash_core::escape::EscapeExpansionMode::EchoBuiltin,
                )
                .map_or_else(
                    |_| argument.to_owned(),
                    |(bytes, _)| String::from_utf8_lossy(&bytes).into_owned(),
                ),
            ),
            b'S' => Some(argument.to_owned()),
            b'C' => Some(argument.chars().take(1).collect()),
            _ => None,
        };
        if let Some(text) = text {
            let mut text = truncate_chars(&text, self.precision);
            let padding = self.width.unwrap_or(0).saturating_sub(text.chars().count());
            if self.left_align {
                text.push_str(&" ".repeat(padding));
            } else {
                text.insert_str(0, &" ".repeat(padding));
            }
            return text;
        }
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

/// The format's items: a `%(FMT)T` is cut out of it here, as uucore knows no such spec,
/// and the text around it is uucore's to parse.
/// The format's items, up to a conversion that is none, and what is to be said of that
/// one, as Bash says it once the text before it is printed; the text was lost with it.
fn parse_format_string(
    format_string: &str,
) -> Result<(Vec<ParsedFormatItem>, Option<String>), cash_core::Error> {
    let mut items = Vec::new();
    let mut rest = format_string;
    while let Some((before, spec, time_format, after)) =
        split_time_spec(rest).map_err(ErrorKind::PrintfInvalidUsage)?
    {
        let (before, bad) = parse_plain_format(before);
        items.extend(before);
        if bad.is_some() {
            return Ok((items, bad));
        }
        let invalid =
            || ErrorKind::PrintfInvalidUsage(format!("`{spec}({time_format})T': invalid format"));
        let (time, position) =
            TimeFormat::parse(spec.as_bytes(), time_format).ok_or_else(invalid)?;
        let bare_q = match position {
            Some(position) => format!("{position}$q"),
            None => "q".to_owned(),
        };
        let item = format::Spec::parse(&mut bare_q.as_bytes())
            .map(format::FormatItem::Spec)
            .map_err(|_| invalid())?;
        items.push((Some(item), false, Some(Custom::Time(time))));
        rest = after;
    }
    let (rest, bad) = parse_plain_format(rest);
    items.extend(rest);
    Ok((items, bad))
}

/// What Bash says of the conversion `spec` that is none: a length with nothing after it is
/// missing its character, anything else is an invalid one.
fn bad_conversion(error: &format::FormatError) -> String {
    match error {
        format::FormatError::SpecError(spec, _) => {
            let spec = String::from_utf8_lossy(spec);
            match spec.chars().last() {
                Some(last) if !"hlLqjzt".contains(last) => {
                    format!("`{last}': invalid format character")
                }
                _ => format!("`%{spec}': missing format character"),
            }
        }
        other => format!("{other}"),
    }
}

fn parse_plain_format(format_string: &str) -> (Vec<ParsedFormatItem>, Option<String>) {
    let mut parsed = Vec::new();
    let items =
        format::parse_spec_and_escape(format_string.as_bytes()).map(|result| match result {
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
            // What uucore refuses and Bash takes, rendered by cash: a `*` width or precision
            // is read from the arguments as the item is printed.
            Err(format::FormatError::SpecError(spec, span))
                if (matches!(spec.last(), Some(b'q' | b'Q' | b'b' | b'S' | b'C'))
                    || (spec.last() == Some(&b's') && spec.contains(&b'0')))
                    && !spec.contains(&b'$') =>
            {
                let quoted_format = QuotedFormat::parse(&spec)
                    .ok_or_else(|| format::FormatError::SpecError(spec.clone(), span.clone()))?;
                let mut bare_q: &[u8] = b"q";
                let item = format::Spec::parse(&mut bare_q)
                    .map(format::FormatItem::Spec)
                    .map_err(|spec| format::FormatError::SpecError(spec.to_vec(), span))?;
                Ok((Some(item), false, Some(Custom::Quoted(quoted_format))))
            }
            Err(error) => Err(error),
        });
    for item in items {
        match item {
            Ok(item) => parsed.push(item),
            Err(error) => return (parsed, Some(bad_conversion(&error))),
        }
    }
    (parsed, None)
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
        format_via_uucore(format_string, &Zone::Local, args, &mut result, &mut |_| {
            Ok(())
        })?;

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

    /// The writes a writer was given, each kept apart.
    struct Writes(Vec<Vec<u8>>);

    impl Write for &mut Writes {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.push(buf.to_vec());
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_pass_of_the_format_is_one_write() -> Result<()> {
        let mut writes = Writes(Vec::new());
        print_formatted(
            r"v\n",
            &Zone::Local,
            std::iter::empty::<&str>(),
            &mut writes,
            &mut |_| Ok(()),
        )?;
        assert_eq!(writes.0, [b"v\n".to_vec()]);
        // Each pass apart, as the messages of a pass come before its output.
        let mut writes = Writes(Vec::new());
        print_formatted(
            r"%s=%d\n",
            &Zone::Local,
            ["a", "1", "b", "2"].iter(),
            &mut writes,
            &mut |_| Ok(()),
        )?;
        assert_eq!(writes.0, [b"a=1\n".to_vec(), b"b=2\n".to_vec()]);

        Ok(())
    }
}
