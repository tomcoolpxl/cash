//! Execution support for shell.

use std::{io::Read, path::Path};

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

        // cash (D7): CRLF script source parses as if it had LF endings. `core.autocrlf`
        // is on by default in Git for Windows, so every script in a checked-out
        // repository looks like this; without it `fi\r` is not `fi` and the whole file
        // fails to parse, at a line number nowhere near the real problem.
        let mut reader = std::io::BufReader::new(cash_win32::text::NormalizeCrlf::new(reader));

        let mut parser = cash_parser::Parser::new(&mut reader, &self.parser_options());

        tracing::debug!(target: trace_categories::PARSE, "Parsing sourced file: {}", source_info.source);
        let parse_result = parser.parse_program();

        let script_positional_args: Vec<String> = args.map(Into::into).collect();
        let source_was_given_args = matches!(call_type, callstack::ScriptCallType::Source)
            && !script_positional_args.is_empty();

        let extdebug = self.options.enable_debugger;
        self.call_stack
            .push_script(call_type, source_info, script_positional_args, extdebug);

        let result = self
            .run_parsed_result(parse_result, source_info, params)
            .await;

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
        let parse_result = self.parse_string(command);
        self.run_parsed_result(parse_result, source_info, params)
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
