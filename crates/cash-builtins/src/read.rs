use clap::{CommandFactory as _, FromArgMatches as _, Parser};
use itertools::Itertools;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use cash_core::{ErrorKind, builtins, env, variables};

use std::io::{Read, Write};
use utf8_chars::BufReadCharsExt;

/// Exit code returned when `read` times out.
/// This is 128 + SIGALRM (14) = 142, matching bash behavior.
const TIMEOUT_EXIT_CODE: u8 = 142;

/// ASCII control character for Ctrl+C (ETX - End of Text).
const CTRL_C: char = '\x03';
/// ASCII control character for Ctrl+D (EOT - End of Transmission).
const CTRL_D: char = '\x04';
/// Backslash character used for escape processing.
const BACKSLASH: char = '\\';
/// Default line delimiter (newline).
const DEFAULT_DELIMITER: char = '\n';
/// NUL character used as delimiter when `-d ''` is specified.
const NUL_DELIMITER: char = '\0';

/// Parse standard input.
#[derive(Parser)]
pub(crate) struct ReadCommand {
    /// Optionally, name of an array variable to receive read words
    /// of input.
    #[clap(short = 'a', value_name = "VAR_NAME")]
    array_variable: Option<String>,

    /// Optionally, a delimiter to use other than a newline character.
    #[clap(short = 'd')]
    delimiter: Option<String>,

    /// Use readline-like input.
    #[clap(short = 'e')]
    use_readline: bool,

    /// Use readline-like input with Bash completion.
    ///
    /// Cash's compact editor does not yet provide completion, so `-E` currently
    /// differs from `-e` only in accepting Bash's documented spelling.
    #[clap(short = 'E')]
    use_readline_with_bash_completion: bool,

    /// Provide text to use as initial input for readline.
    #[clap(short = 'i', value_name = "STR")]
    initial_text: Option<String>,

    /// Read only the first N characters or until a specified
    /// delimiter is reached, whichever happens first.
    #[clap(short = 'n', value_name = "COUNT")]
    return_after_n_chars: Vec<usize>,

    /// Read exactly N characters, ignoring any specified delimiter.
    #[clap(short = 'N', value_name = "COUNT")]
    return_after_n_chars_no_delimiter: Vec<usize>,

    /// Last `-n`/`-N` value in command-line order, populated by `Command::new`.
    #[clap(skip)]
    ordered_char_limit: Option<usize>,

    /// Whether any `-N` occurred. Bash keeps delimiter suppression enabled even
    /// when a later `-n` replaces the count.
    #[clap(skip)]
    saw_capital_n: bool,

    /// Prompt to display before reading.
    #[clap(short = 'p')]
    prompt: Option<String>,

    /// Read input in raw mode; no escape sequences.
    #[clap(short = 'r')]
    raw_mode: bool,

    /// Do not echo input.
    #[clap(short = 's')]
    silent: bool,

    /// Specify timeout in seconds; fail if the timeout elapses before
    /// input is completed.
    #[clap(short = 't', value_name = "SECONDS", allow_hyphen_values = true)]
    timeout_in_seconds: Option<f64>,

    /// File descriptor to read from instead of stdin.
    #[clap(short = 'u', name = "FD")]
    fd_num_to_read: Option<u8>,

    /// Optionally, names of variables to receive read input.
    variable_names: Vec<String>,
}

impl builtins::Command for ReadCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        let matches = Self::command().try_get_matches_from(args)?;
        let lower_n = last_indexed_value(&matches, "return_after_n_chars");
        let capital_n = last_indexed_value(&matches, "return_after_n_chars_no_delimiter");
        let ordered_char_limit = match (lower_n, capital_n) {
            (Some(lower), Some(capital)) => Some(if lower.0 > capital.0 {
                lower.1
            } else {
                capital.1
            }),
            (Some(lower), None) => Some(lower.1),
            (None, Some(capital)) => Some(capital.1),
            (None, None) => None,
        };
        let saw_capital_n = capital_n.is_some();
        let mut command = Self::from_arg_matches(&matches)?;
        command.ordered_char_limit = ordered_char_limit;
        command.saw_capital_n = saw_capital_n;
        Ok(command)
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        // Validate timeout value if provided.
        if let Some(result) = self.validate_timeout(&context)? {
            return Ok(result);
        }

        // Find the input stream to use.
        let fd_num = self.fd_num_to_read.map_or(
            cash_core::openfiles::OpenFiles::STDIN_FD,
            cash_core::ShellFd::from,
        );

        // Retrieve the file.
        let input_stream = context
            .try_fd(fd_num)
            .ok_or_else(|| ErrorKind::BadFileDescriptor(fd_num))?;

        // Retrieve effective value of IFS for splitting.
        // We convert to owned String to release the borrow before the mutable borrow
        // needed for variable assignment.
        let ifs = context.shell.ifs().into_owned();

        // An explicit -t wins. Otherwise Bash uses a positive, valid TMOUT value;
        // invalid or zero TMOUT values simply mean no default timeout.
        let timeout_seconds = self.timeout_in_seconds.or_else(|| {
            context
                .shell
                .env()
                .get_str("TMOUT", context.shell)
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite() && *value > 0.0)
        });
        let timeout = timeout_seconds.map(Duration::from_secs_f64);

        // Perform the read operation (potentially with timeout).
        let history: Vec<String> = context
            .shell
            .history()
            .map(|history| {
                history
                    .iter()
                    .map(|item| item.command_line.clone())
                    .collect()
            })
            .unwrap_or_default();
        let stderr = context.stderr();
        let read_result = self.read_line(input_stream, stderr, timeout, &history, context.shell)?;

        // Determine whether to skip IFS splitting (for -N option).
        let skip_ifs_splitting = self.ignores_delimiter();

        // Extract the input line and determine exit code based on result.
        let (input_line, result) = match &read_result {
            ReadResult::Line(line) => (Some(line.clone()), cash_core::ExecutionResult::success()),
            ReadResult::Eof(Some(line)) => (
                Some(line.clone()),
                cash_core::ExecutionResult::general_error(),
            ),
            ReadResult::Interrupted => (
                None,
                cash_core::ExecutionResult::from(cash_core::ExecutionExitCode::Interrupted),
            ),
            ReadResult::Eof(None) | ReadResult::InputNotReady => {
                (None, cash_core::ExecutionResult::general_error())
            }
            ReadResult::TimedOut(partial) => (
                partial.clone(),
                cash_core::ExecutionResult::new(TIMEOUT_EXIT_CODE),
            ),
            ReadResult::InputReady => (None, cash_core::ExecutionResult::success()),
        };

        // Bash consumes the record before diagnosing that `read -a` cannot replace an
        // associative array. Preserve the old array and return failure after the read.
        if let Some(array_variable) = &self.array_variable
            && context
                .shell
                .env()
                .get(array_variable)
                .is_some_and(|(_, var)| var.value().is_associative_array())
        {
            writeln!(
                context.stderr(),
                "{}: {array_variable}: not an indexed array",
                context.command_name
            )?;
            return Ok(cash_core::ExecutionResult::general_error());
        }

        // Assign input to variables based on options.
        assign_input_to_variables(
            context.shell,
            &context.params,
            input_line.as_deref(),
            &ifs,
            skip_ifs_splitting,
            self.array_variable.as_deref(),
            &self.variable_names,
        )
        .await?;

        Ok(result)
    }
}

/// Assigns read input to shell variables based on the specified options.
///
/// This handles three modes:
/// - Array mode (`-a`): Split input by IFS and assign to array elements
/// - Named variables: Split input by IFS and assign to each variable, with remainder to last
/// - Default (`REPLY`): Assign entire input line to the `REPLY` variable
async fn assign_input_to_variables(
    shell: &mut cash_core::Shell<impl cash_core::ShellExtensions>,
    params: &cash_core::ExecutionParameters,
    input_line: Option<&str>,
    ifs: &str,
    skip_ifs_splitting: bool,
    array_variable: Option<&str>,
    variable_names: &[String],
) -> Result<(), cash_core::Error> {
    if let Some(array_variable) = array_variable {
        let literal_fields = build_array_fields(input_line, ifs, skip_ifs_splitting);
        shell.env_mut().update_or_add(
            array_variable,
            variables::ShellValueLiteral::Array(variables::ArrayLiteral(literal_fields)),
            |_| Ok(()),
            env::EnvironmentLookup::Anywhere,
            env::EnvironmentScope::Global,
        )?;
    } else if !variable_names.is_empty() {
        assign_to_named_variables(
            shell,
            params,
            input_line,
            ifs,
            skip_ifs_splitting,
            variable_names,
        )
        .await?;
    } else {
        shell.env_mut().update_or_add(
            "REPLY",
            variables::ShellValueLiteral::Scalar(input_line.unwrap_or_default().to_owned()),
            |_| Ok(()),
            env::EnvironmentLookup::Anywhere,
            env::EnvironmentScope::Global,
        )?;
    }
    Ok(())
}

/// Assigns split fields to named variables.
///
/// Fields are assigned one per variable, with any remaining fields joined by space
/// and assigned to the last variable. If there are more variables than fields,
/// the extra variables are set to empty strings.
async fn assign_to_named_variables(
    shell: &mut cash_core::Shell<impl cash_core::ShellExtensions>,
    params: &cash_core::ExecutionParameters,
    input_line: Option<&str>,
    ifs: &str,
    skip_ifs_splitting: bool,
    variable_names: &[String],
) -> Result<(), cash_core::Error> {
    let mut fields =
        build_variable_fields(input_line, ifs, skip_ifs_splitting, variable_names.len());

    for (i, name) in variable_names.iter().enumerate() {
        let is_last = i == variable_names.len() - 1;

        let value = if fields.is_empty() {
            String::new()
        } else if is_last {
            // Last variable gets all remaining fields joined by space.
            std::mem::take(&mut fields).into_iter().join(" ")
        } else {
            fields.pop_front().unwrap_or_default()
        };

        assign_read_value(shell, params, name, value).await?;

        if is_last {
            break;
        }
    }
    Ok(())
}

/// Assign one `read` result, including Bash's `read 'array[subscript]'` form.
async fn assign_read_value(
    shell: &mut cash_core::Shell<impl cash_core::ShellExtensions>,
    params: &cash_core::ExecutionParameters,
    target: &str,
    value: String,
) -> Result<(), cash_core::Error> {
    match cash_parser::word::parse_parameter(target, &shell.parser_options())? {
        cash_parser::word::Parameter::Named(name) => shell.env_mut().update_or_add(
            name,
            variables::ShellValueLiteral::Scalar(value),
            |_| Ok(()),
            env::EnvironmentLookup::Anywhere,
            env::EnvironmentScope::Global,
        ),
        cash_parser::word::Parameter::NamedWithIndex { name, index } => {
            let expand_once = shell.options().assoc_expand_once;
            let index = cash_core::expansion::resolve_subscript(
                shell,
                params,
                &name,
                &index,
                false,
                expand_once,
            )
            .await?;
            shell.env_mut().update_or_add_array_element(
                name,
                index,
                value,
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )
        }
        cash_parser::word::Parameter::Positional(_)
        | cash_parser::word::Parameter::Special(_)
        | cash_parser::word::Parameter::NamedWithAllIndices { .. } => {
            Err(ErrorKind::CannotAssignToSpecialParameter.into())
        }
    }
}

/// Builds array field values from input, optionally splitting by IFS.
fn build_array_fields(
    input_line: Option<&str>,
    ifs: &str,
    skip_ifs_splitting: bool,
) -> Vec<(Option<String>, String)> {
    match input_line {
        Some(line) if skip_ifs_splitting => {
            // With -N, don't split - put entire input as single element.
            vec![(None, line.to_string())]
        }
        Some(line) => {
            let fields: VecDeque<_> = split_line_by_ifs(ifs, line, None /* max_fields */);
            fields.into_iter().map(|f| (None, f)).collect()
        }
        None => vec![],
    }
}

/// Builds field values from input for assignment to named variables.
fn build_variable_fields(
    input_line: Option<&str>,
    ifs: &str,
    skip_ifs_splitting: bool,
    num_variables: usize,
) -> VecDeque<String> {
    match input_line {
        Some(line) if skip_ifs_splitting => {
            // With -N, don't split - put entire input in first variable.
            VecDeque::from([line.to_string()])
        }
        Some(line) => split_line_by_ifs(ifs, line, Some(num_variables)),
        None => VecDeque::new(),
    }
}

/// Result of a `read` operation.
///
/// This enum clearly represents all possible outcomes of `read_line()`,
/// making the contract with callers explicit.
enum ReadResult {
    /// Successfully read a complete line (delimiter or char limit reached).
    Line(String),
    /// Reached end of input. Contains any partial content read before EOF.
    Eof(Option<String>),
    /// Input was interrupted (e.g., Ctrl+C). No content is returned.
    Interrupted,
    /// The operation timed out. Contains any partial content read before timeout.
    TimedOut(Option<String>),
    /// For `-t 0`: input is immediately available (exit 0).
    InputReady,
    /// For `-t 0`: no input immediately available (exit 1).
    InputNotReady,
}

/// Helper struct that encapsulates the state for reading input character by character.
///
/// This separates the concerns of character-level I/O with timeout handling from the
/// higher-level logic of line building and escape processing.
struct InputReader {
    /// The input source.
    input: std::io::BufReader<PolledInput>,
    /// Bytes from a malformed UTF-8 sequence, still owed to the caller one at a time.
    pending: VecDeque<char>,
    /// Whether control bytes should have terminal meanings instead of being data.
    input_is_terminal: bool,
    /// Terminal mode guard - kept alive for RAII cleanup on drop.
    /// The guard restores original terminal settings when dropped, even though
    /// we don't access the field directly after construction.
    ///
    /// The leading underscore suppresses the "unused field" warning while making
    /// it explicit this field exists solely for its `Drop` implementation.
    _term_mode: Option<cash_core::terminal::AutoModeGuard>,
}

/// Events that can occur when reading input.
enum InputEvent {
    /// A regular character was read.
    Char(char),
    /// End of file was reached.
    Eof,
    /// The read operation timed out.
    Timeout,
    /// Ctrl+C was pressed.
    CtrlC,
    /// Ctrl+D was pressed.
    CtrlD,
}

/// A key understood by the small Readline-compatible editor used by `read -e`.
enum EditingKey {
    Char(char),
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Tab,
    PreviousHistory,
    NextHistory,
    KillBefore,
    KillAfter,
    KillWordBefore,
    #[cfg(not(windows))]
    Eof,
    CtrlD,
    Timeout,
    Interrupt,
    Ignore,
}

impl InputReader {
    /// Creates a new input reader with optional timeout.
    fn new(
        input: cash_core::openfiles::OpenFile,
        timeout: Option<Duration>,
        term_mode: Option<cash_core::terminal::AutoModeGuard>,
    ) -> Self {
        let input_is_terminal = input.is_terminal();
        Self {
            input: std::io::BufReader::with_capacity(
                1,
                PolledInput {
                    input,
                    deadline: timeout.map(|t| Instant::now() + t),
                },
            ),
            pending: VecDeque::new(),
            input_is_terminal,
            _term_mode: term_mode,
        }
    }

    /// Checks if input is immediately available (for `-t 0`). Returns `false` if an error
    /// occurs while checking for available input.
    fn check_input_available(&self) -> bool {
        cash_core::sys::poll::poll_for_input(&self.input.get_ref().input, Duration::ZERO)
            .unwrap_or(false)
    }

    /// Reads the next input event, handling timeout and control characters.
    fn read_event(&mut self) -> Result<InputEvent, cash_core::Error> {
        let ch = loop {
            if let Some(ch) = self.pending.pop_front() {
                break ch;
            }

            match self.input.read_char_raw() {
                Ok(Some(ch)) => break ch,
                Ok(None) => return Ok(InputEvent::Eof),
                Err(e) => {
                    // Hand back the bytes it consumed, one at a time; the byte that broke
                    // the sequence was left unconsumed, for the next call to re-read.
                    //
                    // TODO(utf-8): `line` is a `String`, so an invalid byte can't
                    // round-trip; bash preserves it verbatim, we re-encode it.
                    if e.as_bytes().is_empty() {
                        if e.as_io_error().kind() == std::io::ErrorKind::TimedOut {
                            return Ok(InputEvent::Timeout);
                        }
                        return Err(e.into_io_error().into());
                    }
                    self.pending
                        .extend(e.as_bytes().iter().copied().map(char::from));
                }
            }
        };

        // Map control characters to events.
        Ok(match ch {
            CTRL_C if self.input_is_terminal => InputEvent::CtrlC,
            CTRL_D if self.input_is_terminal => InputEvent::CtrlD,
            _ => InputEvent::Char(ch),
        })
    }

    /// Decode native Windows console key events. Crossterm enables this code to see
    /// navigation keys before the console's cooked input layer consumes them.
    #[cfg(windows)]
    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "the Unix implementation consumes buffered bytes through the same API"
    )]
    fn read_editing_key(&mut self) -> Result<EditingKey, cash_core::Error> {
        use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

        loop {
            let event_available = if let Some(deadline) = self.input.get_ref().deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                !remaining.is_zero() && crossterm::event::poll(remaining)?
            } else {
                true
            };
            if !event_available {
                return Ok(EditingKey::Timeout);
            }

            let event = crossterm::event::read()?;
            let Event::Key(event) = event else {
                continue;
            };
            if !matches!(event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                continue;
            }

            return Ok(match (event.modifiers, event.code) {
                (modifiers, KeyCode::Char('c')) if modifiers.contains(KeyModifiers::CONTROL) => {
                    EditingKey::Interrupt
                }
                (modifiers, KeyCode::Char('d')) if modifiers.contains(KeyModifiers::CONTROL) => {
                    EditingKey::CtrlD
                }
                (modifiers, KeyCode::Char('a')) if modifiers.contains(KeyModifiers::CONTROL) => {
                    EditingKey::Home
                }
                (modifiers, KeyCode::Char('e')) if modifiers.contains(KeyModifiers::CONTROL) => {
                    EditingKey::End
                }
                (modifiers, KeyCode::Char('k')) if modifiers.contains(KeyModifiers::CONTROL) => {
                    EditingKey::KillAfter
                }
                (modifiers, KeyCode::Char('u')) if modifiers.contains(KeyModifiers::CONTROL) => {
                    EditingKey::KillBefore
                }
                (modifiers, KeyCode::Char('w')) if modifiers.contains(KeyModifiers::CONTROL) => {
                    EditingKey::KillWordBefore
                }
                (modifiers, KeyCode::Char('p')) if modifiers.contains(KeyModifiers::CONTROL) => {
                    EditingKey::PreviousHistory
                }
                (modifiers, KeyCode::Char('n')) if modifiers.contains(KeyModifiers::CONTROL) => {
                    EditingKey::NextHistory
                }
                (_, KeyCode::Enter) => EditingKey::Enter,
                (_, KeyCode::Backspace) => EditingKey::Backspace,
                (_, KeyCode::Delete) => EditingKey::Delete,
                (_, KeyCode::Left) => EditingKey::Left,
                (_, KeyCode::Right) => EditingKey::Right,
                (_, KeyCode::Home) => EditingKey::Home,
                (_, KeyCode::End) => EditingKey::End,
                (_, KeyCode::Tab) => EditingKey::Tab,
                (_, KeyCode::Up) => EditingKey::PreviousHistory,
                (_, KeyCode::Down) => EditingKey::NextHistory,
                (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(ch)) => {
                    EditingKey::Char(ch)
                }
                _ => EditingKey::Ignore,
            });
        }
    }

    /// Decode the common ANSI and control-key sequences emitted by Unix terminals.
    #[cfg(not(windows))]
    fn read_editing_key(&mut self) -> Result<EditingKey, cash_core::Error> {
        let event = self.read_event()?;
        Ok(match event {
            InputEvent::Eof => EditingKey::Eof,
            InputEvent::Timeout => EditingKey::Timeout,
            InputEvent::CtrlC => EditingKey::Interrupt,
            InputEvent::CtrlD => EditingKey::CtrlD,
            InputEvent::Char('\r' | '\n') => EditingKey::Enter,
            InputEvent::Char('\x08' | '\x7f') => EditingKey::Backspace,
            InputEvent::Char('\x01') => EditingKey::Home,
            InputEvent::Char('\x05') => EditingKey::End,
            InputEvent::Char('\x0b') => EditingKey::KillAfter,
            InputEvent::Char('\x15') => EditingKey::KillBefore,
            InputEvent::Char('\x17') => EditingKey::KillWordBefore,
            InputEvent::Char('\x10') => EditingKey::PreviousHistory,
            InputEvent::Char('\x0e') => EditingKey::NextHistory,
            InputEvent::Char('\t') => EditingKey::Tab,
            InputEvent::Char('\x1b') => self.read_escape_sequence()?,
            InputEvent::Char(ch) if ch.is_ascii_control() => EditingKey::Ignore,
            InputEvent::Char(ch) => EditingKey::Char(ch),
        })
    }

    #[cfg(not(windows))]
    fn read_escape_sequence(&mut self) -> Result<EditingKey, cash_core::Error> {
        let InputEvent::Char(prefix) = self.read_event()? else {
            return Ok(EditingKey::Ignore);
        };
        if prefix != '[' && prefix != 'O' {
            return Ok(EditingKey::Ignore);
        }

        let InputEvent::Char(code) = self.read_event()? else {
            return Ok(EditingKey::Ignore);
        };
        Ok(match code {
            'A' => EditingKey::PreviousHistory,
            'B' => EditingKey::NextHistory,
            'C' => EditingKey::Right,
            'D' => EditingKey::Left,
            'H' => EditingKey::Home,
            'F' => EditingKey::End,
            '1' | '3' | '4' | '7' | '8' => {
                let InputEvent::Char(terminator) = self.read_event()? else {
                    return Ok(EditingKey::Ignore);
                };
                if terminator != '~' {
                    EditingKey::Ignore
                } else {
                    match code {
                        '1' | '7' => EditingKey::Home,
                        '3' => EditingKey::Delete,
                        '4' | '8' => EditingKey::End,
                        _ => EditingKey::Ignore,
                    }
                }
            }
            _ => EditingKey::Ignore,
        })
    }
}

/// Reads the input file one byte at a time, enforcing the deadline on every byte.
///
/// Wrapped in a 1-byte `BufReader`, which gives us both the pushback a byte that turns
/// out not to belong to the current UTF-8 sequence needs, and the guarantee that `read`
/// never consumes more of the descriptor than it asked for -- whatever runs next may
/// want the rest.
struct PolledInput {
    /// The input source.
    input: cash_core::openfiles::OpenFile,
    /// Optional deadline for timeout.
    deadline: Option<Instant>,
}

impl Read for PolledInput {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(deadline) = self.deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() || !cash_core::sys::poll::poll_for_input(&self.input, remaining)?
            {
                return Err(std::io::ErrorKind::TimedOut.into());
            }
        }
        self.input.read(buf)
    }
}

/// Configuration for line reading behavior.
struct LineReaderConfig {
    /// Character that terminates input (None for -N mode).
    delimiter: Option<char>,
    /// Maximum characters to read (for -n or -N).
    char_limit: Option<usize>,
    /// Whether to process backslash escapes (false for -r mode).
    process_escapes: bool,
}

/// Reads a complete line of input using the given reader and configuration.
///
/// Returns a `ReadResult` indicating success, EOF, timeout, or interruption.
///
/// Note on character counting for `-n` limit:
/// Bash counts OUTPUT characters (after escape processing) toward the limit.
/// For example, with `-n 3` and input `a\bc` (4 bytes):
/// - Bash processes: 'a' (output 1), '\b' → 'b' (output 2), 'c' (output 3) → "abc"
/// - The backslash is consumed but doesn't count toward the limit
fn read_line_with_reader(
    reader: &mut InputReader,
    config: &LineReaderConfig,
) -> Result<ReadResult, cash_core::Error> {
    let mut line = String::new();
    // Tracked alongside `line`, which is a `String`: the limit counts characters, and
    // recounting a growing string on every one of them is quadratic.
    let mut output_chars = 0usize;
    let mut pending_backslash = false;

    loop {
        let event = reader.read_event()?;

        match event {
            InputEvent::Eof => {
                // Bash discards pending backslash on EOF.
                return Ok(ReadResult::Eof(if line.is_empty() {
                    None
                } else {
                    Some(line)
                }));
            }

            InputEvent::Timeout => {
                // Include pending backslash on timeout (different from EOF).
                if pending_backslash {
                    line.push(BACKSLASH);
                }
                return Ok(ReadResult::TimedOut(if line.is_empty() {
                    None
                } else {
                    Some(line)
                }));
            }

            InputEvent::CtrlC => {
                return Ok(ReadResult::Interrupted);
            }

            InputEvent::CtrlD => {
                // At line start = EOF, mid-input = flush current input.
                // Bash discards pending backslash here too.
                return Ok(if line.is_empty() && !pending_backslash {
                    ReadResult::Eof(None)
                } else {
                    ReadResult::Line(line)
                });
            }

            InputEvent::Char(ch) => {
                // Handle backslash escape processing (when enabled).
                if config.process_escapes {
                    if pending_backslash {
                        pending_backslash = false;

                        // Bash removes backslash-newline and backslash-NUL pairs.
                        // Other escaped delimiters are retained as literal data.
                        if ch == DEFAULT_DELIMITER || ch == NUL_DELIMITER {
                            continue;
                        }

                        // For other chars, add char literally (backslash consumed).
                        line.push(ch);
                        output_chars += 1;

                        // Check character limit (based on output length).
                        if let Some(limit) = config.char_limit
                            && output_chars >= limit
                        {
                            return Ok(ReadResult::Line(line));
                        }
                        continue;
                    }

                    if ch == BACKSLASH {
                        pending_backslash = true;
                        continue;
                    }
                }

                // Check for delimiter.
                if let Some(delim) = config.delimiter
                    && ch == delim
                {
                    // cash (D20): a CRLF terminator loses its `\r` as well, so
                    // `while read -r l` over a CRLF file yields the same values it
                    // would on Linux. Only when the delimiter is the default newline —
                    // an explicit `-d` means the caller has its own framing, and a lone
                    // `\r` with no `\n` is data, not a terminator.
                    #[cfg(windows)]
                    if delim == DEFAULT_DELIMITER
                        && let Some(stripped) = line.strip_suffix('\r')
                    {
                        let stripped_len = stripped.len();
                        line.truncate(stripped_len);
                    }

                    return Ok(ReadResult::Line(line));
                }

                // Bash discards NUL unless it is the requested delimiter. Other
                // control bytes from redirected input are ordinary data.
                if ch == NUL_DELIMITER && config.delimiter != Some(NUL_DELIMITER) {
                    continue;
                }

                line.push(ch);
                output_chars += 1;

                // Check character limit (based on output length).
                if let Some(limit) = config.char_limit
                    && output_chars >= limit
                {
                    return Ok(ReadResult::Line(line));
                }
            }
        }
    }
}

/// Read a line with the editing behavior users expect from `read -e`.
///
/// This intentionally implements the portable, high-value part of Readline rather
/// than importing the interactive shell editor into the builtin layer. Unix uses ANSI
/// key sequences; Windows uses native console events through crossterm.
#[expect(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    reason = "the exhaustive key dispatch is clearer when kept in one editor loop, which \
              needs the reader, its configuration and the completion shell together"
)]
fn read_line_with_editor<SE: cash_core::ShellExtensions>(
    reader: &mut InputReader,
    config: &LineReaderConfig,
    prompt: &str,
    initial_text: &str,
    silent: bool,
    output: &mut impl Write,
    history: &[String],
    completion_shell: Option<&mut cash_core::Shell<SE>>,
) -> Result<ReadResult, cash_core::Error> {
    let _editor_mode = EditorModeGuard::new()?;
    let mut committed = String::new();
    let mut line: Vec<char> = initial_text.chars().collect();
    let mut cursor = line.len();
    let scratch = line.clone();
    let mut history_index = history.len();
    let mut completion_shell = completion_shell;

    if !silent && !line.is_empty() {
        write!(output, "{}", line.iter().collect::<String>())?;
        output.flush()?;
    }

    loop {
        match reader.read_editing_key()? {
            #[cfg(not(windows))]
            EditingKey::Eof => {
                finish_editor_output(output)?;
                committed.push_str(&edited_line(&line, config));
                return Ok(ReadResult::Eof(
                    (!committed.is_empty()).then_some(committed),
                ));
            }
            EditingKey::Timeout => {
                finish_editor_output(output)?;
                committed.push_str(&edited_line(&line, config));
                return Ok(ReadResult::TimedOut(
                    (!committed.is_empty()).then_some(committed),
                ));
            }
            EditingKey::Interrupt => {
                if !silent {
                    write!(output, "^C")?;
                }
                finish_editor_output(output)?;
                return Ok(ReadResult::Interrupted);
            }
            EditingKey::Enter => {
                if config.delimiter != Some(DEFAULT_DELIMITER) {
                    line.insert(cursor, DEFAULT_DELIMITER);
                    cursor += 1;
                    repaint_editor(output, prompt, &line, cursor, silent)?;
                    if editor_limit_reached(&committed, &line, config) {
                        committed.push_str(&edited_line(&line, config));
                        finish_editor_output(output)?;
                        return Ok(ReadResult::Line(committed));
                    }
                } else if accept_editor_segment(
                    &mut committed,
                    &mut line,
                    &mut cursor,
                    DEFAULT_DELIMITER,
                    config,
                    prompt,
                    silent,
                    output,
                )? {
                    return Ok(ReadResult::Line(committed));
                }
            }
            EditingKey::CtrlD => {
                if line.is_empty() {
                    finish_editor_output(output)?;
                    return Ok(ReadResult::Eof(
                        (!committed.is_empty()).then_some(committed),
                    ));
                }
                if cursor < line.len() {
                    line.remove(cursor);
                    repaint_editor(output, prompt, &line, cursor, silent)?;
                }
            }
            EditingKey::Char(ch) => {
                if config.delimiter == Some(ch) {
                    if accept_editor_segment(
                        &mut committed,
                        &mut line,
                        &mut cursor,
                        ch,
                        config,
                        prompt,
                        silent,
                        output,
                    )? {
                        return Ok(ReadResult::Line(committed));
                    }
                    continue;
                }
                line.insert(cursor, ch);
                cursor += 1;
                repaint_editor(output, prompt, &line, cursor, silent)?;

                if editor_limit_reached(&committed, &line, config) {
                    committed.push_str(&edited_line(&line, config));
                    finish_editor_output(output)?;
                    return Ok(ReadResult::Line(committed));
                }
            }
            EditingKey::Backspace if cursor > 0 => {
                cursor -= 1;
                line.remove(cursor);
                repaint_editor(output, prompt, &line, cursor, silent)?;
            }
            EditingKey::Delete if cursor < line.len() => {
                line.remove(cursor);
                repaint_editor(output, prompt, &line, cursor, silent)?;
            }
            EditingKey::Left if cursor > 0 => {
                cursor -= 1;
                repaint_editor(output, prompt, &line, cursor, silent)?;
            }
            EditingKey::Right if cursor < line.len() => {
                cursor += 1;
                repaint_editor(output, prompt, &line, cursor, silent)?;
            }
            EditingKey::Home => {
                cursor = 0;
                repaint_editor(output, prompt, &line, cursor, silent)?;
            }
            EditingKey::End => {
                cursor = line.len();
                repaint_editor(output, prompt, &line, cursor, silent)?;
            }
            EditingKey::PreviousHistory if !history.is_empty() && history_index > 0 => {
                history_index -= 1;
                line = history[history_index].chars().collect();
                cursor = line.len();
                repaint_editor(output, prompt, &line, cursor, silent)?;
            }
            EditingKey::NextHistory if history_index < history.len() => {
                history_index += 1;
                line = if history_index == history.len() {
                    scratch.clone()
                } else {
                    history[history_index].chars().collect()
                };
                cursor = line.len();
                repaint_editor(output, prompt, &line, cursor, silent)?;
            }
            EditingKey::Tab => {
                if let Some(shell) = completion_shell.as_deref_mut() {
                    apply_editor_completion(shell, &mut line, &mut cursor)?;
                    repaint_editor(output, prompt, &line, cursor, silent)?;
                }
            }
            EditingKey::KillBefore if cursor > 0 => {
                line.drain(..cursor);
                cursor = 0;
                repaint_editor(output, prompt, &line, cursor, silent)?;
            }
            EditingKey::KillAfter if cursor < line.len() => {
                line.truncate(cursor);
                repaint_editor(output, prompt, &line, cursor, silent)?;
            }
            EditingKey::KillWordBefore if cursor > 0 => {
                let mut start = cursor;
                while start > 0 && line[start - 1].is_whitespace() {
                    start -= 1;
                }
                while start > 0 && !line[start - 1].is_whitespace() {
                    start -= 1;
                }
                line.drain(start..cursor);
                cursor = start;
                repaint_editor(output, prompt, &line, cursor, silent)?;
            }
            EditingKey::Backspace
            | EditingKey::Delete
            | EditingKey::Left
            | EditingKey::Right
            | EditingKey::KillBefore
            | EditingKey::KillAfter
            | EditingKey::KillWordBefore
            | EditingKey::PreviousHistory
            | EditingKey::NextHistory
            | EditingKey::Ignore => {}
        }
    }
}

fn apply_editor_completion(
    shell: &mut cash_core::Shell<impl cash_core::ShellExtensions>,
    line: &mut Vec<char>,
    cursor: &mut usize,
) -> Result<(), cash_core::Error> {
    let input: String = line.iter().collect();
    let cursor_byte = input
        .char_indices()
        .nth(*cursor)
        .map_or(input.len(), |(index, _)| index);
    let completions = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(shell.complete(&input, cursor_byte))
    })?;
    if completions.candidates.is_empty() {
        return Ok(());
    }

    let replacement = if completions.candidates.len() == 1 {
        let mut candidate = completions.candidates[0].clone();
        if cursor_byte == input.len()
            && !completions.options.no_trailing_space_at_end_of_line
            && !candidate.ends_with(['/', '\\'])
        {
            candidate.push(' ');
        }
        candidate
    } else {
        common_completion_prefix(&completions.candidates)
    };
    if replacement.is_empty() {
        return Ok(());
    }

    let start_byte = completions.insertion_index;
    let end_byte = start_byte.saturating_add(completions.delete_count);
    if !input.is_char_boundary(start_byte) || !input.is_char_boundary(end_byte) {
        return Ok(());
    }
    let (Some(before_start), Some(before_end)) = (input.get(..start_byte), input.get(..end_byte))
    else {
        return Ok(());
    };
    let start_char = before_start.chars().count();
    let end_char = before_end.chars().count();
    line.splice(start_char..end_char, replacement.chars());
    *cursor = start_char + replacement.chars().count();
    Ok(())
}

fn common_completion_prefix(candidates: &[String]) -> String {
    let Some(first) = candidates.first() else {
        return String::new();
    };
    let mut prefix: Vec<char> = first.chars().collect();
    for candidate in &candidates[1..] {
        let common = prefix
            .iter()
            .zip(candidate.chars())
            .take_while(|(left, right)| **left == *right)
            .count();
        prefix.truncate(common);
    }
    prefix.into_iter().collect()
}

/// Process an editor buffer when Readline accepts it. Returns `true` when the
/// delimiter completed the read, or `false` when a trailing backslash escaped
/// that delimiter and another edited line is required.
#[expect(
    clippy::too_many_arguments,
    reason = "keeps editor state and display continuation in one operation"
)]
fn accept_editor_segment(
    committed: &mut String,
    line: &mut Vec<char>,
    cursor: &mut usize,
    delimiter: char,
    config: &LineReaderConfig,
    prompt: &str,
    silent: bool,
    output: &mut impl Write,
) -> Result<bool, cash_core::Error> {
    let trailing_backslashes = line.iter().rev().take_while(|ch| **ch == BACKSLASH).count();
    let delimiter_was_escaped = config.process_escapes && trailing_backslashes % 2 == 1;

    committed.push_str(&edited_line(line, config));
    if delimiter_was_escaped && delimiter != DEFAULT_DELIMITER && delimiter != NUL_DELIMITER {
        committed.push(delimiter);
    }

    finish_editor_output(output)?;
    if !delimiter_was_escaped {
        return Ok(true);
    }

    line.clear();
    *cursor = 0;
    if !silent {
        write!(output, "{prompt}")?;
        output.flush()?;
    }
    Ok(false)
}

fn editor_limit_reached(committed: &str, line: &[char], config: &LineReaderConfig) -> bool {
    config.char_limit.is_some_and(|limit| {
        committed.chars().count() + edited_line(line, config).chars().count() >= limit
    })
}

/// Windows needs an explicit raw-console guard because its cash-core terminal
/// configuration is currently a stub. Unix was already placed in raw mode by
/// `setup_terminal_settings`.
struct EditorModeGuard;

impl EditorModeGuard {
    fn new() -> Result<Self, cash_core::Error> {
        #[cfg(windows)]
        crossterm::terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for EditorModeGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

// Kept as a tiny helper so the editor's conversion rules can be unit tested without
// a terminal. Backslashes quote the following character unless `read -r` was used.
fn edited_line(chars: &[char], config: &LineReaderConfig) -> String {
    if !config.process_escapes {
        return chars.iter().collect();
    }

    let mut result = String::new();
    let mut quoted = false;
    for &ch in chars {
        if quoted {
            if ch != DEFAULT_DELIMITER && ch != NUL_DELIMITER {
                result.push(ch);
            }
            quoted = false;
        } else if ch == BACKSLASH {
            quoted = true;
        } else {
            result.push(ch);
        }
    }
    result
}

fn repaint_editor(
    output: &mut impl Write,
    prompt: &str,
    line: &[char],
    cursor: usize,
    silent: bool,
) -> Result<(), cash_core::Error> {
    if silent {
        return Ok(());
    }

    write!(
        output,
        "\r\x1b[2K{prompt}{}",
        line.iter().collect::<String>()
    )?;
    let chars_after_cursor = line.len().saturating_sub(cursor);
    if chars_after_cursor > 0 {
        write!(output, "\x1b[{chars_after_cursor}D")?;
    }
    output.flush()?;
    Ok(())
}

fn finish_editor_output(output: &mut impl Write) -> Result<(), cash_core::Error> {
    write!(output, "\r\n")?;
    output.flush()?;
    Ok(())
}

impl ReadCommand {
    /// Reads a line of input, optionally with a timeout.
    ///
    /// Handles backslash escape processing:
    /// - Without `-r`: backslash-newline is line continuation, other backslashes escape the next
    ///   char
    /// - With `-r`: backslash is treated as a literal character
    fn read_line<SE: cash_core::ShellExtensions>(
        &self,
        input_file: cash_core::openfiles::OpenFile,
        mut stderr_file: impl std::io::Write,
        mut timeout: Option<Duration>,
        history: &[String],
        shell: &mut cash_core::Shell<SE>,
    ) -> Result<ReadResult, cash_core::Error> {
        let input_file_is_terminal = input_file.is_terminal();
        let term_mode = self.setup_terminal_settings(&input_file)?;

        // Like Bash, a positive timeout has no effect on regular files. Keep
        // explicit `-t 0`, which is a readiness query even for a file.
        if matches!(&input_file, cash_core::openfiles::OpenFile::File(_))
            && timeout != Some(Duration::ZERO)
        {
            timeout = None;
        }

        // Determine delimiter based on options.
        let delimiter = if self.ignores_delimiter() {
            None
        } else if let Some(delimiter_str) = &self.delimiter {
            if delimiter_str.is_empty() {
                Some(NUL_DELIMITER)
            } else {
                delimiter_str.chars().next()
            }
        } else {
            Some(DEFAULT_DELIMITER)
        };

        let char_limit = self.character_limit();

        // Create the input reader.
        let mut reader = InputReader::new(input_file, timeout, term_mode);

        // Handle -t 0 special case: just check if input is available without reading.
        if timeout == Some(Duration::ZERO) {
            return Ok(if reader.check_input_available() {
                ReadResult::InputReady
            } else {
                ReadResult::InputNotReady
            });
        }

        // Bash treats both `read -n 0` and `read -N 0` as successful empty
        // reads and does not consume input or display a prompt.
        if char_limit == Some(0) {
            return Ok(ReadResult::Line(String::new()));
        }

        // Display prompt on stderr, but only if input is from a terminal (per bash behavior).
        if let Some(prompt) = &self.prompt
            && input_file_is_terminal
        {
            write!(stderr_file, "{prompt}")?;
            stderr_file.flush()?;
        }

        // Configure and perform the read.
        let config = LineReaderConfig {
            delimiter,
            char_limit,
            process_escapes: !self.raw_mode,
        };

        if input_file_is_terminal && self.editing_requested() {
            read_line_with_editor(
                &mut reader,
                &config,
                self.prompt.as_deref().unwrap_or_default(),
                self.initial_text.as_deref().unwrap_or_default(),
                self.silent,
                &mut stderr_file,
                history,
                self.use_readline_with_bash_completion.then_some(shell),
            )
        } else {
            read_line_with_reader(&mut reader, &config)
        }
    }

    fn setup_terminal_settings(
        &self,
        file: &cash_core::openfiles::OpenFile,
    ) -> Result<Option<cash_core::terminal::AutoModeGuard>, cash_core::Error> {
        let mode = cash_core::terminal::AutoModeGuard::new(file.to_owned()).ok();
        if let Some(mode) = &mode {
            let editing = self.editing_requested();
            let config = cash_core::terminal::Settings::builder()
                .line_input(false)
                .interrupt_signals(false)
                .echo_input(!self.silent && !editing)
                .build();

            mode.apply_settings(&config)?;
        }

        Ok(mode)
    }

    /// Validates the timeout value and returns an error result if invalid.
    ///
    /// Returns `Ok(Some(result))` if the timeout is invalid (caller should return early),
    /// `Ok(None)` if the timeout is valid or not specified.
    ///
    /// TODO(read): Bash uses $TMOUT as a default timeout for `read` when -t is not specified.
    fn validate_timeout(
        &self,
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    ) -> Result<Option<cash_core::ExecutionResult>, cash_core::Error> {
        if let Some(timeout) = self.timeout_in_seconds {
            if !timeout.is_finite() || timeout < 0.0 {
                writeln!(
                    context.stderr(),
                    "{}: -t: invalid timeout specification",
                    context.command_name
                )?;
                return Ok(Some(cash_core::ExecutionResult::general_error()));
            }
        }
        Ok(None)
    }

    const fn editing_requested(&self) -> bool {
        self.use_readline || self.use_readline_with_bash_completion
    }

    fn character_limit(&self) -> Option<usize> {
        if self.ordered_char_limit.is_some() {
            self.ordered_char_limit
        } else {
            self.return_after_n_chars_no_delimiter
                .last()
                .copied()
                .or_else(|| self.return_after_n_chars.last().copied())
        }
    }

    const fn ignores_delimiter(&self) -> bool {
        self.saw_capital_n || !self.return_after_n_chars_no_delimiter.is_empty()
    }
}

/// Return the final value for an option together with Clap's command-line index.
/// Comparing these indices preserves Bash's sequential `-n`/`-N` semantics.
fn last_indexed_value(matches: &clap::ArgMatches, id: &str) -> Option<(usize, usize)> {
    matches
        .indices_of(id)?
        .zip(matches.get_many::<usize>(id)?)
        .map(|(index, value)| (index, *value))
        .max_by_key(|(index, _)| *index)
}

/// Splits a line by IFS (Internal Field Separator) according to shell rules.
///
/// Shell IFS splitting has special rules:
/// - Whitespace IFS chars (space, tab, newline) are "IFS whitespace"
/// - Leading/trailing IFS whitespace is trimmed from the input
/// - Consecutive IFS whitespace chars act as a single delimiter
/// - Non-whitespace IFS chars each act as individual delimiters
/// - Trailing non-whitespace delimiter does NOT create an empty final field
///
/// # Arguments
/// * `ifs` - The IFS string (typically " \t\n")
/// * `line` - The input line to split
/// * `max_fields` - Optional limit on number of fields (for `read var1 var2`)
fn split_line_by_ifs(ifs: &str, line: &str, max_fields: Option<usize>) -> VecDeque<String> {
    let ifs_chars: Vec<char> = ifs.chars().collect();

    // Helper to check if a char is IFS whitespace (space, tab, or newline AND in IFS).
    let is_ifs_whitespace =
        |c: char| -> bool { (c == ' ' || c == '\t' || c == '\n') && ifs_chars.contains(&c) };

    // Trim leading/trailing IFS whitespace from the input.
    let trimmed_line = line.trim_matches(&is_ifs_whitespace);
    if trimmed_line.is_empty() {
        return VecDeque::new();
    }

    let max_fields = max_fields.unwrap_or(usize::MAX);

    // State machine for splitting:
    // - `consuming_whitespace_run`: Currently skipping consecutive IFS whitespace
    // - `prev_was_non_ws_delim`: Previous char was a non-whitespace delimiter
    // - `collecting_remainder`: We've hit max_fields, collect everything into last field
    let mut fields = VecDeque::new();
    let mut current_field = String::new();
    let mut consuming_whitespace_run = false;
    let mut prev_was_non_ws_delim = false;
    let mut collecting_remainder = false;

    for c in trimmed_line.chars() {
        // Skip consecutive IFS whitespace (they act as single delimiter).
        if consuming_whitespace_run && is_ifs_whitespace(c) {
            continue;
        }
        consuming_whitespace_run = false;

        let is_delimiter = ifs_chars.contains(&c);
        let at_field_limit = fields.len() + 1 >= max_fields;

        if !at_field_limit && is_delimiter {
            // Normal case: delimiter ends current field, start new one.
            fields.push_back(std::mem::take(&mut current_field));
            consuming_whitespace_run = is_ifs_whitespace(c);
            prev_was_non_ws_delim = !consuming_whitespace_run;
        } else if at_field_limit && !collecting_remainder && is_delimiter {
            // At field limit but haven't started last field content yet.
            // Skip leading IFS whitespace for the final field.
            if is_ifs_whitespace(c) {
                consuming_whitespace_run = true;
            } else {
                // Non-whitespace delimiters at boundary: include in remainder.
                // e.g., "x::y" with IFS=":" and 2 vars gives ["x", ":y"]
                collecting_remainder = true;
                current_field.push(c);
            }
        } else {
            // Regular character: add to current field.
            collecting_remainder = at_field_limit;
            current_field.push(c);
            prev_was_non_ws_delim = false;
        }
    }

    // Finalize: push last field unless it's empty AND we ended with non-ws delimiter.
    // e.g., "a,b,c," with IFS="," gives ["a", "b", "c"], not ["a", "b", "c", ""].
    if !current_field.is_empty() || !prev_was_non_ws_delim {
        fields.push_back(current_field);
    }

    // The last variable takes the rest of the line. As in Bash's `read`, when that rest is
    // a single word, it gets the word alone, without the delimiter that ended it: with
    // IFS=: `foo:bar:` gives `bar`, while `x:y:z:` keeps `y:z:`.
    if fields.len() == max_fields
        && let Some(last) = fields.back_mut()
        && let Some(word) = sole_word(last, &ifs_chars, &is_ifs_whitespace)
    {
        *last = word;
    }

    fields
}

/// Returns the word `rest` consists of, if it is one word followed by at most one
/// delimiter (and IFS whitespace).
fn sole_word(
    rest: &str,
    ifs_chars: &[char],
    is_ifs_whitespace: &impl Fn(char) -> bool,
) -> Option<String> {
    let word_end = rest.find(|c| ifs_chars.contains(&c)).unwrap_or(rest.len());
    let (word, tail) = rest.split_at(word_end);
    let mut after = tail.chars().peekable();
    while after.next_if(|&c| is_ifs_whitespace(c)).is_some() {}
    after.next_if(|&c| ifs_chars.contains(&c) && !is_ifs_whitespace(c));
    while after.next_if(|&c| is_ifs_whitespace(c)).is_some() {}
    after.peek().is_none().then(|| word.to_owned())
}

#[cfg(test)]
mod tests {
    use itertools::assert_equal;

    use super::*;

    // ==================== UTF-8 decoding tests ====================

    // Decoding is otherwise covered end-to-end by the compat suite; what can't be
    // expressed there is a partial sequence that never completes, since it needs a
    // writer held open while `read` gives up on it.
    //
    // `-t` needs `poll_for_input`, which only the unix backend implements; elsewhere it
    // reports `Unsupported`, so there's no timeout to observe.
    #[cfg(unix)]
    #[test]
    fn test_read_times_out_mid_sequence_without_losing_the_partial_bytes() {
        let (rx, mut tx) = std::io::pipe().unwrap();
        tx.write_all(b"\xc3").unwrap();

        let mut reader = InputReader::new(rx.into(), Some(Duration::from_millis(100)), None);
        let config = LineReaderConfig {
            delimiter: Some(DEFAULT_DELIMITER),
            char_limit: None,
            process_escapes: false,
        };

        // Holding `tx` means the continuation byte never arrives, so the deadline has to
        // be enforced on it and not just on the byte that started the sequence.
        let result = read_line_with_reader(&mut reader, &config).unwrap();
        drop(tx);

        assert!(matches!(result, ReadResult::TimedOut(Some(line)) if line == "\u{c3}"));
    }

    #[test]
    fn test_edited_line_applies_read_backslash_rules() {
        let escaped: Vec<char> = r"one\ two\\three".chars().collect();
        let cooked = LineReaderConfig {
            delimiter: Some(DEFAULT_DELIMITER),
            char_limit: None,
            process_escapes: true,
        };
        let raw = LineReaderConfig {
            delimiter: Some(DEFAULT_DELIMITER),
            char_limit: None,
            process_escapes: false,
        };

        assert_eq!(edited_line(&escaped, &cooked), r"one two\three");
        assert_eq!(edited_line(&escaped, &raw), r"one\ two\\three");
    }

    #[test]
    fn test_common_completion_prefix_handles_unicode_and_empty_inputs() {
        assert_eq!(
            common_completion_prefix(&["alpha".into(), "alpine".into(), "alps".into()]),
            "alp"
        );
        assert_eq!(
            common_completion_prefix(&["café".into(), "caféine".into()]),
            "café"
        );
        assert_eq!(common_completion_prefix(&[]), "");
    }

    // ==================== split_line_by_ifs tests ====================

    #[test]
    fn test_split_line_by_ifs_basic() {
        let result = split_line_by_ifs(",", "a,b,c", None);
        assert_equal(result, VecDeque::from(vec!["a", "b", "c"]));
    }

    #[test]
    fn test_split_line_by_ifs_leading_or_trailing_space() {
        let result = split_line_by_ifs(" ", "  a b c ", None);
        assert_equal(result, VecDeque::from(vec!["a", "b", "c"]));
    }

    #[test]
    fn test_split_line_by_ifs_extra_interior_space() {
        let result = split_line_by_ifs(" ", "a  b c", None);
        assert_equal(result, VecDeque::from(vec!["a", "b", "c"]));
    }

    #[test]
    fn test_split_line_by_ifs_leading_non_space_delimiter() {
        let result = split_line_by_ifs(",", ",a,b,c", None);
        assert_equal(result, VecDeque::from(vec!["", "a", "b", "c"]));
    }

    #[test]
    fn test_split_line_by_ifs_trailing_non_space_delimiter() {
        // Bash does NOT include empty trailing field when input ends with non-ws delimiter.
        let result = split_line_by_ifs(",", "a,b,c,", None);
        assert_equal(result, VecDeque::from(vec!["a", "b", "c"]));
    }

    #[test]
    fn test_split_line_by_ifs_max_fields() {
        // With max_fields=2, remainder goes into second field.
        let result = split_line_by_ifs(" ", "a b c d", Some(2));
        assert_equal(result, VecDeque::from(vec!["a", "b c d"]));
    }

    #[test]
    fn test_split_line_by_ifs_max_fields_with_non_ws_delimiter() {
        // With max_fields and non-whitespace delimiter.
        let result = split_line_by_ifs(",", "a,b,c,d", Some(2));
        assert_equal(result, VecDeque::from(vec!["a", "b,c,d"]));
    }

    #[test]
    fn test_split_line_by_ifs_consecutive_delimiters_at_boundary() {
        // Consecutive non-whitespace delimiters at field boundary should be preserved.
        // e.g., "x::y" with IFS=":" and 2 vars gives ["x", ":y"]
        let result = split_line_by_ifs(":", "x::y", Some(2));
        assert_equal(result, VecDeque::from(vec!["x", ":y"]));

        // Triple delimiter at boundary.
        let result = split_line_by_ifs(":", "x:::y", Some(2));
        assert_equal(result, VecDeque::from(vec!["x", "::y"]));

        // Delimiter in middle of remainder is also preserved.
        let result = split_line_by_ifs(":", "x:y:z:w", Some(2));
        assert_equal(result, VecDeque::from(vec!["x", "y:z:w"]));
    }

    #[test]
    fn test_split_line_by_ifs_last_variable_drops_a_lone_trailing_delimiter() {
        // Observed with Bash 5.3: one word left over loses the delimiter that ended it;
        // more than one keeps the rest verbatim.
        for (line, max, expected) in [
            ("foo:bar:", 2, vec!["foo", "bar"]),
            ("x:y:z:", 2, vec!["x", "y:z:"]),
            ("x:y::", 2, vec!["x", "y::"]),
            ("x::", 2, vec!["x", ""]),
            ("x:", 1, vec!["x"]),
            ("x::", 1, vec!["x::"]),
        ] {
            let result = split_line_by_ifs(":", line, Some(max));
            assert_equal(result, VecDeque::from(expected));
        }
        let result = split_line_by_ifs(": ", "x:y: ", Some(2));
        assert_equal(result, VecDeque::from(vec!["x", "y"]));
    }

    #[test]
    fn test_split_line_by_ifs_mixed_delimiters() {
        // Mixed whitespace and non-whitespace in IFS.
        let result = split_line_by_ifs(": ", "a:b  c:d", None);
        assert_equal(result, VecDeque::from(vec!["a", "b", "c", "d"]));
    }

    #[test]
    fn test_split_line_by_ifs_empty_input() {
        let result = split_line_by_ifs(" ", "", None);
        assert_equal(result, VecDeque::<String>::new());
    }

    #[test]
    fn test_split_line_by_ifs_whitespace_only() {
        let result = split_line_by_ifs(" ", "   ", None);
        assert_equal(result, VecDeque::<String>::new());
    }

    #[test]
    fn test_split_line_by_ifs_consecutive_non_ws_delimiters() {
        // Consecutive non-whitespace delimiters create empty fields.
        let result = split_line_by_ifs(",", "a,,b", None);
        assert_equal(result, VecDeque::from(vec!["a", "", "b"]));
    }

    // ==================== build_array_fields tests ====================

    #[test]
    fn test_build_array_fields_basic() {
        let result = build_array_fields(Some("a b c"), " ", false);
        assert_eq!(
            result,
            vec![
                (None, "a".to_string()),
                (None, "b".to_string()),
                (None, "c".to_string())
            ]
        );
    }

    #[test]
    fn test_build_array_fields_skip_splitting() {
        // With -N option, entire input goes as single element.
        let result = build_array_fields(Some("a b c"), " ", true);
        assert_eq!(result, vec![(None, "a b c".to_string())]);
    }

    #[test]
    fn test_build_array_fields_none_input() {
        let result = build_array_fields(None, " ", false);
        assert!(result.is_empty());
    }

    // ==================== build_variable_fields tests ====================

    #[test]
    fn test_build_variable_fields_basic() {
        let result = build_variable_fields(Some("a b c"), " ", false, 3);
        assert_equal(result, VecDeque::from(vec!["a", "b", "c"]));
    }

    #[test]
    fn test_build_variable_fields_fewer_vars_than_fields() {
        // Last variable gets remainder.
        let result = build_variable_fields(Some("a b c d"), " ", false, 2);
        assert_equal(result, VecDeque::from(vec!["a", "b c d"]));
    }

    #[test]
    fn test_build_variable_fields_skip_splitting() {
        // With -N option, entire input goes to first variable.
        let result = build_variable_fields(Some("a b c"), " ", true, 3);
        assert_equal(result, VecDeque::from(vec!["a b c"]));
    }

    #[test]
    fn test_build_variable_fields_none_input() {
        let result = build_variable_fields(None, " ", false, 3);
        assert!(result.is_empty());
    }
}
