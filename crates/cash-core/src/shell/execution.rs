//! Execution support for shell.

use std::io::{Read, Write as _};
use std::path::Path;

use super::parsing::Prefix;
use crate::{
    ExecutionControlFlow, ExecutionParameters, ExecutionResult, ProcessGroupPolicy, SourceInfo,
    arithmetic::Evaluatable as _, callstack, error, interp::Execute as _, openfiles,
    trace_categories,
};

impl<SE: crate::extensions::ShellExtensions> crate::Shell<SE> {
    /// Returns the default execution parameters for this shell.
    pub fn default_exec_params(&self) -> ExecutionParameters {
        let mut params = ExecutionParameters::default();

        params.process_group_policy = if self.options.enable_job_control {
            ProcessGroupPolicy::NewProcessGroup
        } else {
            ProcessGroupPolicy::SameProcessGroup
        };

        params
    }

    pub(super) async fn source_if_exists(
        &mut self,
        path: impl AsRef<Path>,
        params: &ExecutionParameters,
    ) -> Result<bool, error::Error> {
        let path = path.as_ref();
        if path.exists() {
            // cash: `$0` stays the shell's name while a startup file runs, as Git Bash
            // 5.3.15 keeps it for `~/.bashrc`, `~/.bash_profile` and `$BASH_ENV` alike
            // (checked 2026-09-29). It named the file, from a reading of Bash 5.2's notes
            // that bash does not bear out, and `coolfetch` in `~/.bashrc` then called the
            // shell `.bashrc`. The file is named as cash spells paths (D3): joined onto
            // the home folder it was `C:/Users/me\.bashrc` in `BASH_SOURCE` and in error
            // messages.
            let path = std::path::PathBuf::from(cash_win32::path::render(path));
            self.source_script(&path, std::iter::empty::<String>(), params)
                .await?;
            Ok(true)
        } else {
            tracing::debug!("skipping non-existent file: {}", path.display());
            Ok(false)
        }
    }

    /// Source the given file as a shell script, returning the execution result.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the file to source.
    /// * `args` - The arguments to pass to the script as positional parameters.
    /// * `params` - Execution parameters.
    pub async fn source_script<S: Into<String>, P: AsRef<Path>, I: Iterator<Item = S>>(
        &mut self,
        path: P,
        args: I,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        self.parse_and_execute_script_file(
            path.as_ref(),
            args,
            params,
            callstack::ScriptCallType::Source,
        )
        .await
    }

    /// Parse and execute the given file as a shell script, returning the execution result.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the file to source.
    /// * `args` - The arguments to pass to the script as positional parameters.
    /// * `params` - Execution parameters.
    /// * `call_type` - The type of script call being made.
    async fn parse_and_execute_script_file<
        S: Into<String>,
        P: AsRef<Path>,
        I: Iterator<Item = S>,
    >(
        &mut self,
        path: P,
        args: I,
        params: &ExecutionParameters,
        call_type: callstack::ScriptCallType,
    ) -> Result<ExecutionResult, error::Error> {
        let path = path.as_ref();
        tracing::debug!("sourcing: {}", path.display());

        let mut options = std::fs::File::options();
        options.read(true);

        let opened_file: openfiles::OpenFile = self
            .open_file(&options, crate::sys::fs::Access::Read, path, params)
            .map_err(|e| error::ErrorKind::FailedSourcingFile(path.to_owned(), e))?;

        if opened_file.is_dir() {
            return Err(error::ErrorKind::FailedSourcingFile(
                path.to_owned(),
                std::io::Error::from(std::io::ErrorKind::IsADirectory),
            )
            .into());
        }

        // Bash 5.3 `bash_source_fullpath`: `BASH_SOURCE` records the file's real path
        // rather than the name it was invoked by, when that path can be resolved.
        let source_path = if self.options().bash_source_full_path {
            std::fs::canonicalize(self.absolute_path(path)).map_or_else(
                |_| path.to_owned(),
                |real| std::path::PathBuf::from(cash_win32::path::render(&real)),
            )
        } else {
            path.to_owned()
        };
        let source_info = crate::SourceInfo::from(source_path);

        let mut contents = Vec::new();
        let mut opened_file = opened_file;
        opened_file
            .read_to_end(&mut contents)
            .map_err(|e| error::ErrorKind::FailedSourcingFile(path.to_owned(), e))?;

        // Bash refuses to run a script that looks binary (Bash 5.3 NEWS 1a): an ELF
        // header, or a NUL in the first line of the first 80 bytes — the first two
        // lines when it starts with `#!`. `source` does not check.
        if matches!(call_type, callstack::ScriptCallType::Run) && looks_binary(&contents) {
            return Err(error::ErrorKind::CannotExecuteBinaryFile(path.to_owned()).into());
        }

        // Like Bash, the shell's input reader discards NUL bytes.
        contents.retain(|&byte| byte != 0);

        let mut result = self
            .source_file(
                std::io::Cursor::new(contents),
                &source_info,
                args,
                params,
                call_type,
            )
            .await?;

        // Handle control flow at script execution boundary. If execution completed
        // with a `return`, we need to clear it since it's already been "used". All
        // other control flow types are preserved.
        if matches!(
            result.next_control_flow,
            ExecutionControlFlow::ReturnFromFunctionOrScript
        ) {
            result.next_control_flow = ExecutionControlFlow::Normal;
        }

        Ok(result)
    }

    /// Source the given file as a shell script, returning the execution result.
    ///
    /// # Arguments
    ///
    /// * `file` - The file to source.
    /// * `source_info` - Information about the source of the script.
    /// * `args` - The arguments to pass to the script as positional parameters.
    /// * `params` - Execution parameters.
    /// * `call_type` - The type of script call being made.
    async fn source_file<F: Read, S: Into<String>, I: Iterator<Item = S>>(
        &mut self,
        file: F,
        source_info: &crate::SourceInfo,
        args: I,
        params: &ExecutionParameters,
        call_type: callstack::ScriptCallType,
    ) -> Result<ExecutionResult, error::Error> {
        use std::io::BufRead;

        let mut reader = std::io::BufReader::new(file);

        // cash (D41): strip a leading UTF-8 BOM before parsing.
        //
        // Windows editors write one, and it is otherwise invisible while breaking the
        // script completely: the shebang is not recognised and the first token carries
        // three phantom bytes, producing `command not found: ﻿echo`.
        let bom_len = match reader.fill_buf() {
            Ok(buffer) if buffer.starts_with(cash_win32::text::BOM) => cash_win32::text::BOM.len(),
            _ => 0,
        };
        if bom_len > 0 {
            reader.consume(bom_len);
        }

        // Read whole, so that a file that does not parse can be run a command at a time
        // ([`Self::run_text`]). CRLF script source parses as if it had LF endings (D7,
        // in `Shell::parse`): `core.autocrlf` is on by default in Git for Windows, so
        // every script in a checked-out repository has them, and `fi\r` is not `fi`.
        let mut text = Vec::new();
        reader.read_to_end(&mut text)?;

        tracing::debug!(target: trace_categories::PARSE, "Parsing sourced file: {}", source_info.source);
        let whole = self.parse(text.as_slice());

        let script_positional_args: Vec<String> = args.map(Into::into).collect();
        let source_was_given_args = matches!(call_type, callstack::ScriptCallType::Source)
            && !script_positional_args.is_empty();

        let extdebug = self.options.enable_debugger;
        self.call_stack
            .push_script(call_type, source_info, script_positional_args, extdebug);

        // A sourced file that does not parse fails `source` with 2, as in Bash, and the
        // script goes on, in POSIX mode too; a script that is run ends.
        let fatal = !matches!(call_type, callstack::ScriptCallType::Source);
        let result = self
            .run_text(&text, whole, TextOrigin::Whole, source_info, params, fatal)
            .await
            .map_err(|e| e.located_at(self.error_prefix()));

        if matches!(call_type, callstack::ScriptCallType::Source) && result.is_ok() {
            crate::commands::run_return_trap(self, params).await;
        } else {
            self.take_status_before_return();
        }

        let exited_frame = self.call_stack.pop();

        // Jobs that finished while the file was sourced are reported now that it is done
        // (Bash 5.3), when this was the outermost one.
        if matches!(call_type, callstack::ScriptCallType::Source) && self.may_report_jobs_now() {
            let _ = self.check_for_completed_jobs(params).await;
        }

        // Bash restores arguments temporarily supplied to `source` unless the sourced
        // file itself changes them with `set --`/`shift` at top level. In that case the
        // changed values become the caller's positional parameters. A function remains
        // its own argument scope, so sourced changes made inside one are restored.
        if source_was_given_args
            && !self.call_stack.in_function()
            && let Some(frame) = exited_frame
            && frame.positional_args_changed
        {
            self.mark_current_shell_args_set();
            *self.current_shell_args_mut() = frame.args;
        }

        result
    }

    /// Executes the given string as a shell program, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to execute.
    /// * `source_info` - Information about the source of the command text.
    /// * `params` - Execution parameters.
    pub async fn run_string<S: Into<String>>(
        &mut self,
        command: S,
        source_info: &crate::SourceInfo,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let command: String = command.into();
        let whole = self.parse_string(command.as_str());
        self.run_text(
            command.as_bytes(),
            whole,
            TextOrigin::Whole,
            source_info,
            params,
            true,
        )
        .await
    }

    /// Executes the given command, provided to a shell executable on the command
    /// line (i.e., via `-c`).
    ///
    /// It is expected that the shell will not be used for any further execution
    /// after this command; this function will perform any necessary shell exit
    /// handling.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to execute.
    pub async fn run_dash_c_command<S: Into<String>>(
        &mut self,
        command: S,
    ) -> Result<ExecutionResult, error::Error> {
        self.start_command_string_mode();

        // Execute the command string.
        let params = self.default_exec_params();
        let source_info = SourceInfo::from("-c");
        let result = self.run_string(command, &source_info, &params).await?;

        self.end_command_string_mode()?;

        // Give the shell a chance to run on-exit tasks, but ignore the result.
        let _ = self.on_exit().await;

        Ok(result)
    }

    /// Executes the given script file, returning the resulting exit status.
    ///
    /// It is expected that the shell will not be used for any further execution
    /// after this command; this function will perform any necessary shell exit
    /// handling.
    ///
    /// # Arguments
    ///
    /// * `script_path` - The path to the script file to execute.
    /// * `args` - The arguments to pass to the script as positional parameters.
    pub async fn run_script<S: Into<String>, P: AsRef<Path>, I: Iterator<Item = S>>(
        &mut self,
        script_path: P,
        args: I,
    ) -> Result<ExecutionResult, error::Error> {
        // `$0` is the script for as long as the shell runs, as in Bash: an `EXIT` trap
        // run once the script is done said cash's own path.
        self.name = Some(script_path.as_ref().to_string_lossy().into_owned());

        let params = self.default_exec_params();
        let result = self
            .parse_and_execute_script_file(
                script_path.as_ref(),
                args,
                &params,
                callstack::ScriptCallType::Run,
            )
            .await?;

        // Give the shell a chance to run on-exit tasks, but ignore the result.
        let _ = self.on_exit().await;

        Ok(result)
    }

    /// Runs `text` as Bash runs a script, a sourced file, a `-c` string or `eval`'s:
    /// whole when `whole`, its parse, succeeded, which is the common case; when it did
    /// not, a complete command at a time, as Bash reads it, so the commands before the
    /// error run and a payload after an `exit` is never read (EXE-07). A command a line
    /// before it changed, as `shopt -s extglob` does, parses as that command left it.
    ///
    /// The syntax error is reported where it is reached, as [`Self::report_syntax_error`]
    /// reports it with `fatal`.
    pub(crate) async fn run_text(
        &mut self,
        text: &[u8],
        whole: Result<cash_parser::ast::Program, cash_parser::ParseError>,
        origin: TextOrigin,
        source_info: &crate::SourceInfo,
        params: &ExecutionParameters,
        fatal: bool,
    ) -> Result<ExecutionResult, error::Error> {
        if let Ok(program) = whole {
            return self
                .run_parsed_result(Ok(program), source_info, params)
                .await;
        }

        // A last line ends as the others do, as Bash ends a `-c` or `eval` string: `echo (`
        // is wrong at its newline, not at the end of the text.
        let mut ended;
        let text = if text.last().is_some_and(|&byte| byte != b'\n') {
            ended = text.to_vec();
            ended.push(b'\n');
            ended.as_slice()
        } else {
            text
        };

        let mut result = ExecutionResult::success();
        let mut pending: Vec<u8> = Vec::new();
        // The lines before `pending`, which its own line numbers do not count.
        let mut lines_before = 0;
        let mut pending_lines = 0;
        let mut lines = text.split_inclusive(|&byte| byte == b'\n').peekable();
        while let Some(line) = lines.next() {
            pending.extend_from_slice(line);
            pending_lines += 1;
            let mut parsed = self.parse_prefix(&pending);
            // A here-document still open at the end is closed there, with Bash's warning,
            // and the command runs with what it holds, as in Bash; cash refused it.
            if lines.peek().is_none()
                && let Prefix::NeedsMore(Err(cash_parser::ParseError::Tokenizing {
                    inner: cash_parser::TokenizerError::UnterminatedHereDocuments(tags, positions),
                    ..
                })) = &parsed
            {
                let tags: Vec<String> = tags
                    .split(", ")
                    .map(|tag| tag.replace(['\'', '"', '\\'], ""))
                    .collect();
                let opened = positions
                    .split(',')
                    .next()
                    .and_then(|line| line.trim().parse::<usize>().ok())
                    .unwrap_or(1);
                let warning = format!(
                    "{}: line {}: warning: here-document at line {} delimited by end-of-file \
                     (wanted `{}')\n",
                    self.name_for_errors(),
                    first_line(origin) + lines_before + pending_lines - 1,
                    first_line(origin) + lines_before + opened - 1,
                    tags.first().map_or("", String::as_str),
                );
                let _ = params.stderr(self).write_all(warning.as_bytes());
                for tag in &tags {
                    pending.extend_from_slice(tag.as_bytes());
                    pending.push(b'\n');
                }
                parsed = self.parse_prefix(&pending);
            }
            let program = match parsed {
                Prefix::NeedsMore(_) if lines.peek().is_some() => continue,
                Prefix::Complete(program) | Prefix::NeedsMore(Ok(program)) => program,
                Prefix::NeedsMore(Err(err)) | Prefix::Wrong(err) => {
                    let err = counted_from_the_start(err, lines_before);
                    return Ok(self.report_syntax_error(
                        err,
                        text,
                        origin,
                        source_info,
                        params,
                        fatal,
                    ));
                }
            };

            self.call_stack.increment_current_line_offset(lines_before);
            let ran = self
                .run_parsed_result(Ok(program), source_info, params)
                .await;
            self.call_stack.decrement_current_line_offset(lines_before);
            result = ran?;
            if matches!(
                result.next_control_flow,
                ExecutionControlFlow::ExitShell | ExecutionControlFlow::ReturnFromFunctionOrScript
            ) {
                break;
            }

            lines_before += pending_lines;
            pending_lines = 0;
            pending.clear();
        }
        Ok(result)
    }

    /// A syntax error in the text a script, `eval` or `source` was given: reported, and
    /// with `fatal` it ends a shell that is not interactive, as for a script, a `-c`
    /// string, and `eval` in POSIX mode. Otherwise the builtin fails with 2 and the
    /// script goes on, as Bash has it for `eval` and `source`.
    pub(crate) fn report_syntax_error(
        &mut self,
        parse_err: cash_parser::ParseError,
        text: &[u8],
        origin: TextOrigin,
        source_info: &crate::SourceInfo,
        params: &ExecutionParameters,
        fatal: bool,
    ) -> ExecutionResult {
        let mut stderr = params.stderr(self);
        let colour = self.colours(&stderr);
        let message = self.syntax_error_message(&parse_err, text, origin, source_info, colour);
        let _ = stderr.write_all(message.as_bytes());

        let mut err =
            error::Error::from(error::ErrorKind::ParseError(parse_err, source_info.clone()));
        if fatal {
            err = err.into_fatal();
        }
        let result = err.into_result(self);
        self.set_last_exit_status(result.exit_code.into());
        result
    }

    /// Bash's words for a syntax error: after `script.sh: line 2: `, the error, and on a
    /// second line the line it is on, quoted; `cash: -c: line 2: ` for a `-c` string,
    /// `script.sh: eval: line 7: ` for `eval`'s, counted from the line the `eval` is on;
    /// at the end of the text, the line after its last, and no quote. What was typed at
    /// the prompt gets `cash: ` and one line.
    fn syntax_error_message(
        &self,
        err: &cash_parser::ParseError,
        text: &[u8],
        origin: TextOrigin,
        source_info: &crate::SourceInfo,
        colour: bool,
    ) -> String {
        use cash_parser::ParseError;

        use std::fmt::Write as _;

        let paint = |prefix: String| {
            if colour {
                format!("\x1b[31m{prefix}\x1b[39m")
            } else {
                prefix
            }
        };
        if self.options.interactive && source_info.source == "main" {
            let prefix = paint(format!("{}: ", self.name_for_interactive_errors()));
            return format!("{prefix}{err}\n");
        }

        let (name, first_line) = match origin {
            TextOrigin::Eval { line } => (format!("{}: eval", self.name_for_errors()), line),
            TextOrigin::Whole if source_info.source == "-c" => {
                let zero = self
                    .current_shell_name()
                    .map_or_else(|| "cash".to_owned(), std::borrow::Cow::into_owned);
                (format!("{zero}: -c"), 1)
            }
            TextOrigin::Whole => (source_info.source.clone(), 1),
        };
        // The line a compound command began on is counted as the others are.
        let shown = match err {
            ParseError::UnterminatedCompound { keyword, line } => {
                ParseError::UnterminatedCompound {
                    keyword: keyword.clone(),
                    line: line + first_line - 1,
                }
                .to_string()
            }
            other => other.to_string(),
        };
        // Input that ended inside a quote or a substitution, as Bash words it.
        if let ParseError::Tokenizing { inner, .. } = err
            && let Some((closer, opened)) = inner.bash_eof()
        {
            // `${` and `$((` name the line they opened on, which the tokenizer does not
            // keep; a command substitution, the line after the last.
            let last_opened = |opener: &[u8]| {
                text.windows(opener.len())
                    .rposition(|window| window == opener)
                    .map(|at| first_line + text[..at].split(|&b| b == b'\n').count() - 1)
            };
            let last_substitution = text.windows(2).rposition(|pair| pair == b"$(");
            let line = match (closer, opened) {
                (_, Some(line)) => first_line + line - 1,
                ('}', None) => last_opened(b"${").unwrap_or(first_line),
                (_, None) if last_substitution == text.windows(3).rposition(|w| w == b"$((") => {
                    last_opened(b"$((").unwrap_or(first_line)
                }
                (_, None) => first_line + count_lines(text),
            };
            let prefix = paint(format!("{name}: line {line}: "));
            return format!("{prefix}unexpected EOF while looking for matching `{closer}'\n");
        }
        if let ParseError::UnterminatedArray { line } = err {
            let prefix = paint(format!("{name}: line {}: ", first_line + line - 1));
            return format!("{prefix}{err}\n");
        }
        let (line, quoted) = match err {
            ParseError::ParsingNear(position, _) => (
                first_line + position.line.saturating_sub(1),
                line_of(text, position.line),
            ),
            ParseError::Tokenizing {
                inner,
                position: Some(position),
            } if !inner.is_incomplete() => (first_line + position.line.saturating_sub(1), None),
            _ => (first_line + count_lines(text), None),
        };

        let prefix = paint(format!("{name}: line {line}: "));
        let mut message = format!("{prefix}{shown}\n");
        if let Some(quoted) = quoted {
            let _ = writeln!(message, "{prefix}`{quoted}'");
        }
        message
    }

    pub(crate) async fn run_parsed_result(
        &mut self,
        parse_result: Result<cash_parser::ast::Program, cash_parser::ParseError>,
        source_info: &crate::SourceInfo,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // If parsing succeeded, run the program. If there's a parse error, it's fatal (per spec).
        let result = match parse_result {
            Ok(prog) => self.run_program(prog, params).await,
            Err(parse_err) => Err(error::Error::from(error::ErrorKind::ParseError(
                parse_err,
                source_info.clone(),
            ))
            .into_fatal()),
        };

        // Report any errors.
        match result {
            Ok(result) => Ok(result),
            // An interrupt a program passes on is for the program that runs it to act on.
            Err(err) if err.is_silent_interrupt() => Err(err),
            Err(err) => {
                let _ = self.display_error(&mut params.stderr(self), &err);

                let result = err.into_result(self);
                self.set_last_exit_status(result.exit_code.into());

                Ok(result)
            }
        }
    }

    /// Executes the given parsed shell program, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `program` - The program to execute.
    /// * `params` - Execution parameters.
    pub async fn run_program(
        &mut self,
        program: cash_parser::ast::Program,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        program.execute(self, params).await
    }

    /// Evaluate the given arithmetic expression, returning the result.
    pub fn eval_arithmetic(
        &mut self,
        expr: &cash_parser::ast::ArithmeticExpr,
    ) -> Result<i64, error::Error> {
        Ok(expr.eval(self)?)
    }
}

/// Where a text [`crate::Shell::run_text`] runs came from, for the line numbers its
/// syntax errors name.
#[derive(Clone, Copy, Debug)]
pub(crate) enum TextOrigin {
    /// A file, a `-c` string or what was typed: its first line is line 1.
    Whole,
    /// `eval`'s string: its first line is the line the `eval` is on.
    Eval {
        /// That line.
        line: usize,
    },
}

/// The line a text's first line is, by where it came from.
const fn first_line(origin: TextOrigin) -> usize {
    match origin {
        TextOrigin::Whole => 1,
        TextOrigin::Eval { line } => line,
    }
}

/// The lines in `text`, a last one without a newline included.
fn count_lines(text: &[u8]) -> usize {
    text.split_inclusive(|&byte| byte == b'\n').count()
}

/// Line `number` of `text`, counted from 1, without its line ending.
fn line_of(text: &[u8], number: usize) -> Option<String> {
    let line = text
        .split(|&byte| byte == b'\n')
        .nth(number.checked_sub(1)?)?;
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    Some(String::from_utf8_lossy(line).into_owned())
}

/// `err`, from a parse of text that began `lines` lines into the whole, with its lines
/// counted from the start of the whole.
fn counted_from_the_start(err: cash_parser::ParseError, lines: usize) -> cash_parser::ParseError {
    use cash_parser::ParseError;
    match err {
        ParseError::ParsingNear(mut position, token) => {
            position.line += lines;
            ParseError::ParsingNear(position, token)
        }
        ParseError::UnterminatedArray { line } => {
            ParseError::UnterminatedArray { line: line + lines }
        }
        ParseError::UnterminatedCompound { keyword, line } => ParseError::UnterminatedCompound {
            keyword,
            line: line + lines,
        },
        ParseError::Tokenizing { inner, position } => ParseError::Tokenizing {
            inner: inner.later_by(lines),
            position: position.map(|mut position| {
                position.line += lines;
                position
            }),
        },
        other @ ParseError::ParsingAtEndOfInput => other,
    }
}

/// Bash's `check_binary_file`: an ELF header, or a NUL before the end of the first
/// line (the second when the file starts with `#!`) within the first 80 bytes.
fn looks_binary(contents: &[u8]) -> bool {
    let sample = &contents[..contents.len().min(80)];
    if sample.starts_with(b"\x7fELF") {
        return true;
    }
    let lines = if sample.starts_with(b"#!") { 2 } else { 1 };
    sample
        .split_inclusive(|&byte| byte == b'\n')
        .take(lines)
        .any(|line| line.contains(&0))
}
