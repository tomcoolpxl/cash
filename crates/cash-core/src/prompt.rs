use crate::{
    ExecutionParameters, error, expansion, extensions,
    shell::Shell,
    sys::{self, users},
};
use std::path::Path;

const VERSION_MAJOR: &str = env!("CARGO_PKG_VERSION_MAJOR");
const VERSION_MINOR: &str = env!("CARGO_PKG_VERSION_MINOR");
const VERSION_PATCH: &str = env!("CARGO_PKG_VERSION_PATCH");

pub(crate) async fn expand_prompt(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    spec: &str,
) -> Result<String, error::Error> {
    // Parse the prompt spec into its pieces.
    let prompt_pieces = parse_prompt(spec)?;

    // Now, render each piece.
    let mut formatted_prompt = String::new();
    for piece in prompt_pieces {
        // Pieces that semantically represent a literal char (e.g. user wrote
        // `\$` meaning a literal `$`, not input for pass-2 expansion). We
        // prepend a `\` so pass 2 consumes it and leaves the char alone.
        let semantically_literal = matches!(piece, cash_parser::prompt::PromptPiece::DollarOrPound);

        let formatted_piece = format_prompt_piece(shell, piece)?;

        // Only useful when pass 2 actually consumes a `\` for that leading
        // byte; otherwise the `\` would leak through.
        if shell.options().expand_prompt_strings
            && semantically_literal
            && formatted_piece.starts_with(expansion::DOUBLE_QUOTED_ESCAPE_CHARS)
        {
            formatted_prompt.push('\\');
        }

        formatted_prompt.push_str(&formatted_piece);
    }

    if shell.options().expand_prompt_strings {
        // Now expand the result as bash does, as if it were inside double quotes: quote
        // characters stay literal (`PS1="it's \w"` keeps its `'`), and backslashes
        // emitted in the previous step survive unless they precede a character that is
        // escapable inside a double-quoted string.
        formatted_prompt =
            expansion::basic_expand_prompt_word(shell, params, &formatted_prompt).await?;
    }

    Ok(formatted_prompt)
}

#[cached::macros::cached(max_size = 64, key = "String", convert = r#"{ spec.to_owned() }"#)]
fn parse_prompt(
    spec: &str,
) -> Result<Vec<cash_parser::prompt::PromptPiece>, cash_parser::WordParseError> {
    cash_parser::prompt::parse(spec)
}

/// The shell's own name, as `\s` reports it.
///
/// cash: the default prompt is `\s-\v\$`, so with no rc file this is the first thing
/// anyone sees. On Windows the basename carries the extension, which made that prompt
/// read `cash.exe-0.5$`; bash shows `bash`, never `bash.exe`. Only an exact `.exe` is
/// stripped, so `sh` and `bash` still read as themselves.
fn shell_base_name(shell: &Shell<impl extensions::ShellExtensions>) -> String {
    let Some(shell_name) = shell.current_shell_name() else {
        return String::new();
    };

    let base = Path::new(shell_name.as_ref())
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();

    base.strip_suffix(".exe")
        .or_else(|| base.strip_suffix(".EXE"))
        .map_or_else(|| base.clone(), std::borrow::ToOwned::to_owned)
}

fn format_prompt_piece(
    shell: &Shell<impl extensions::ShellExtensions>,
    piece: cash_parser::prompt::PromptPiece,
) -> Result<String, error::Error> {
    let formatted = match piece {
        cash_parser::prompt::PromptPiece::EscapedSequence(s) => s,
        cash_parser::prompt::PromptPiece::Literal(l) => l,
        cash_parser::prompt::PromptPiece::AsciiCharacter(c) => {
            char::from_u32(c).map_or_else(String::new, |c| c.to_string())
        }
        cash_parser::prompt::PromptPiece::Backslash => "\\".to_owned(),
        cash_parser::prompt::PromptPiece::BellCharacter => "\x07".to_owned(),
        cash_parser::prompt::PromptPiece::CarriageReturn => "\r".to_owned(),
        // Both count the command about to be read, as Bash's `\#` and `\!` do.
        cash_parser::prompt::PromptPiece::CurrentCommandNumber => {
            (shell.commands_read() + 1).to_string()
        }
        cash_parser::prompt::PromptPiece::CurrentHistoryNumber => {
            (shell.history().map_or(0, |h| h.count()) + 1).to_string()
        }
        cash_parser::prompt::PromptPiece::CurrentUser => users::get_current_username()?,
        cash_parser::prompt::PromptPiece::CurrentWorkingDirectory {
            tilde_replaced,
            basename,
        } => format_current_working_directory(shell, tilde_replaced, basename),
        cash_parser::prompt::PromptPiece::Date(format) => {
            format_date(&chrono::Local::now(), &format)
        }
        cash_parser::prompt::PromptPiece::DollarOrPound => {
            if users::is_root() {
                "#".to_owned()
            } else {
                "$".to_owned()
            }
        }
        // NOTE: We mimic bash and convert \[ into \001, a.k.a. RL_PROMPT_START_IGNORE.
        // It will need to get removed before it's actually displayed. While present it
        // also has the important (compatible) side effect of ensuring the text on either
        // side of it is not concatenated together, potentially resulting in incompatible
        // variable expansions. Also, we *only* do this if the shell is interactive.
        cash_parser::prompt::PromptPiece::EndNonPrintingSequence => {
            if shell.options().interactive {
                "\x02".to_owned()
            } else {
                String::new()
            }
        }
        cash_parser::prompt::PromptPiece::EscapeCharacter => "\x1b".to_owned(),
        cash_parser::prompt::PromptPiece::Hostname {
            only_up_to_first_dot,
        } => {
            let hn = sys::network::get_hostname()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if only_up_to_first_dot && let Some((first, _)) = hn.split_once('.') {
                return Ok(first.to_owned());
            }
            hn
        }
        cash_parser::prompt::PromptPiece::Newline => "\n".to_owned(),
        cash_parser::prompt::PromptPiece::NumberOfManagedJobs => {
            shell.jobs().jobs.len().to_string()
        }
        cash_parser::prompt::PromptPiece::ShellBaseName => shell_base_name(shell),
        // cash: the version escapes report the *product's* version -- the one the shell
        // was handed at startup and publishes as $CASH_VERSION -- rather than the
        // version of whichever crate happens to hold this code. The default prompt
        // carries one, so this is the number on screen before anyone configures
        // anything, and it claimed 0.5 (cash-core's) while cash --version said something
        // else.
        cash_parser::prompt::PromptPiece::ShellRelease => shell.version().map_or_else(
            || std::format!("{VERSION_MAJOR}.{VERSION_MINOR}.{VERSION_PATCH}"),
            ToString::to_string,
        ),
        cash_parser::prompt::PromptPiece::ShellVersion => shell.version().map_or_else(
            || std::format!("{VERSION_MAJOR}.{VERSION_MINOR}"),
            |version| {
                // The short form: the first two components of whatever it is.
                let mut parts = version.split('.');
                match (parts.next(), parts.next()) {
                    (Some(major), Some(minor)) => std::format!("{major}.{minor}"),
                    _ => version.to_string(),
                }
            },
        ),
        // NOTE: See above note for EndNonPrintingSequence
        cash_parser::prompt::PromptPiece::StartNonPrintingSequence => {
            if shell.options().interactive {
                "\x01".to_owned()
            } else {
                String::new()
            }
        }
        cash_parser::prompt::PromptPiece::TerminalDeviceBaseName => {
            sys::terminal::try_get_terminal_device_path()
                .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
                .unwrap_or_default()
        }
        cash_parser::prompt::PromptPiece::Time(time_fmt) => {
            format_time(&chrono::Local::now(), &time_fmt)
        }
    };

    Ok(formatted)
}

fn format_current_working_directory(
    shell: &Shell<impl extensions::ShellExtensions>,
    tilde_replaced: bool,
    basename: bool,
) -> String {
    let mut working_dir_str = shell.working_dir().to_string_lossy().to_string();

    if tilde_replaced {
        working_dir_str = shell.tilde_shorten(working_dir_str);
    }

    if basename && let Some(filename) = Path::new(&working_dir_str).file_name() {
        working_dir_str = filename.to_string_lossy().to_string();
    }

    working_dir_str = working_dir_str.replace('\\', "/");

    working_dir_str
}

fn format_time<Tz: chrono::TimeZone>(
    datetime: &chrono::DateTime<Tz>,
    format: &cash_parser::prompt::PromptTimeFormat,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let formatted = match format {
        cash_parser::prompt::PromptTimeFormat::TwelveHourAM => datetime.format("%I:%M %p"),
        cash_parser::prompt::PromptTimeFormat::TwelveHourHHMMSS => datetime.format("%I:%M:%S"),
        cash_parser::prompt::PromptTimeFormat::TwentyFourHourHHMM => datetime.format("%H:%M"),
        cash_parser::prompt::PromptTimeFormat::TwentyFourHourHHMMSS => datetime.format("%H:%M:%S"),
    };

    formatted.to_string()
}

fn format_date<Tz: chrono::TimeZone>(
    datetime: &chrono::DateTime<Tz>,
    format: &cash_parser::prompt::PromptDateFormat,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    match format {
        cash_parser::prompt::PromptDateFormat::WeekdayMonthDate => {
            datetime.format("%a %b %d").to_string()
        }
        // An empty format is the locale's time, as in Bash.
        cash_parser::prompt::PromptDateFormat::Custom(fmt) => {
            let fmt = if fmt.is_empty() { "%X" } else { fmt.as_str() };
            crate::timefmt::strftime(datetime, fmt)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_time() {
        // Create a well-known test date/time.
        let dt = chrono::DateTime::parse_from_rfc3339("2024-12-25T13:34:56.789Z").unwrap();

        assert_eq!(
            format_time(&dt, &cash_parser::prompt::PromptTimeFormat::TwelveHourAM),
            "01:34 PM"
        );

        assert_eq!(
            format_time(
                &dt,
                &cash_parser::prompt::PromptTimeFormat::TwentyFourHourHHMMSS
            ),
            "13:34:56"
        );

        assert_eq!(
            format_time(
                &dt,
                &cash_parser::prompt::PromptTimeFormat::TwelveHourHHMMSS
            ),
            "01:34:56"
        );
    }

    #[test]
    fn test_format_date() {
        // Create a well-known test date/time.
        let dt = chrono::DateTime::parse_from_rfc3339("2024-12-25T12:34:56.789Z").unwrap();

        assert_eq!(
            format_date(
                &dt,
                &cash_parser::prompt::PromptDateFormat::WeekdayMonthDate
            ),
            "Wed Dec 25"
        );

        assert_eq!(
            format_date(
                &dt,
                &cash_parser::prompt::PromptDateFormat::Custom(String::from("%Y-%m-%d"))
            ),
            "2024-12-25"
        );

        assert_eq!(
            format_date(
                &dt,
                &cash_parser::prompt::PromptDateFormat::Custom(String::from(
                    "%Y-%m-%d %H:%M:%S.%f"
                ))
            ),
            "2024-12-25 12:34:56.789000000"
        );
    }
}
