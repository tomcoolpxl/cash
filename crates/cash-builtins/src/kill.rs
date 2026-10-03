use clap::Parser;
use std::io::Write;

use cash_core::traps::TrapSignal;
use cash_core::{ExecutionExitCode, ExecutionResult, builtins, sys};

/// Signal a job or process.
#[derive(Parser)]
pub(crate) struct KillCommand {
    /// Name of the signal to send.
    #[arg(short = 's', value_name = "SIG_NAME")]
    signal_name: Option<String>,

    /// Number of the signal to send.
    #[arg(short = 'n', value_name = "SIG_NUM")]
    signal_number: Option<usize>,

    //
    // TODO(kill): implement -sigspec syntax
    /// List known signal names.
    #[arg(short = 'l', short_alias = 'L')]
    list_signals: bool,

    // Interpretation of these depends on whether -l is present.
    #[arg(allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for KillCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        let mut signal_zero = false;

        // The default signal is SIGTERM, as in Bash and POSIX. It was SIGKILL, so a plain
        // `kill $pid` gave the target no chance to exit on its own (D21's grace).
        //
        // cash: named rather than reached for through `nix`, which is Unix-only. That
        // one reference was the sole reason this builtin was gated to Unix, leaving
        // Windows with no `kill` at all — so D21 and D22 were unreachable and `kill`
        // fell through to whatever external kill.exe happened to be on PATH.
        let mut trap_signal = TrapSignal::try_from("TERM")?;

        // Try parsing the signal name (if specified).
        if let Some(signal_name) = &self.signal_name {
            if let Ok(parsed_trap_signal) = TrapSignal::try_from(signal_name.as_str()) {
                trap_signal = parsed_trap_signal;
            } else {
                writeln!(
                    context.error_stream(),
                    "{}: {}: invalid signal specification",
                    context.command_name,
                    signal_name
                )?;
                return Ok(ExecutionExitCode::InvalidUsage.into());
            }
        }

        // Try parsing the signal number (if specified).
        if let Some(signal_number) = &self.signal_number {
            if *signal_number == 0 {
                signal_zero = true;
            } else {
                #[expect(clippy::cast_possible_truncation)]
                #[expect(clippy::cast_possible_wrap)]
                if let Ok(parsed_trap_signal) = TrapSignal::try_from(*signal_number as i32) {
                    trap_signal = parsed_trap_signal;
                } else {
                    writeln!(
                        context.error_stream(),
                        "{}: invalid signal number: {}",
                        context.command_name,
                        signal_number
                    )?;
                    return Ok(ExecutionExitCode::InvalidUsage.into());
                }
            }
        }

        // Look through the remaining args for a pid/job spec or a -sigspec style option.
        //
        // Only the *first* argument may be a `-sigspec`, and only if no signal was given
        // with `-s`/`-n`. After that a leading `-` introduces a negative pid, which POSIX
        // reads as "the process group led by that pid" — `kill -KILL -1234` is a target,
        // not a second signal. Treating every hyphenated argument as a sigspec rejected
        // that spelling outright.
        let signal_already_given = self.signal_name.is_some() || self.signal_number.is_some();
        let mut targets = Vec::new();
        for (index, arg) in self.args.iter().enumerate() {
            let may_be_sigspec = index == 0 && !signal_already_given;

            // See if this is -sigspec syntax. The sigspec may be a signal name
            // (e.g., -TERM) or a signal number (e.g., -9, including -0).
            if let Some(possible_sigspec) = arg.strip_prefix("-").filter(|_| may_be_sigspec) {
                if Ok(0) == possible_sigspec.parse::<i32>() {
                    signal_zero = true;
                } else if let Ok(parsed_trap_signal) = possible_sigspec.parse::<TrapSignal>() {
                    signal_zero = false;
                    trap_signal = parsed_trap_signal;
                } else {
                    writeln!(
                        context.error_stream(),
                        "{}: {}: invalid signal specification",
                        context.command_name,
                        possible_sigspec
                    )?;
                    return Ok(ExecutionResult::general_error());
                }
            } else {
                targets.push(arg);
            }
        }

        if self.list_signals {
            return print_signals(&context, self.args.as_ref());
        }
        if targets.is_empty() {
            writeln!(
                context.error_stream(),
                "{}: invalid usage",
                context.command_name
            )?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        // Every target is signalled, and the status is 0 if one of them was, as in Bash;
        // a second one was "too many jobs or processes specified" (BI-03).
        let mut any_signalled = false;
        for target in targets {
            let result = signal_target(&mut context, target, signal_zero, trap_signal).await?;
            if !result.is_normal_flow() {
                return Ok(result);
            }
            any_signalled |= result.is_success();
        }
        Ok(if any_signalled {
            ExecutionResult::success()
        } else {
            ExecutionResult::general_error()
        })
    }
}

/// Signals one target of `kill`: a job spec, a pid or the shell itself.
async fn signal_target<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    target: &str,
    signal_zero: bool,
    trap_signal: TrapSignal,
) -> Result<ExecutionResult, cash_core::Error> {
    if target.starts_with('%') {
        return signal_job_spec(context, target, signal_zero, trap_signal);
    }
    let Ok(pid) = cash_core::int_utils::parse(target, 10) else {
        writeln!(
            context.error_stream(),
            "{}: `{target}': not a pid or valid job spec",
            context.command_name
        )?;
        return Ok(ExecutionResult::general_error());
    };
    if let Some(result) = signal_job_known_as(context, pid, signal_zero, trap_signal)? {
        return Ok(result);
    }
    if !signal_zero && u32::try_from(pid).is_ok_and(|pid| pid == std::process::id()) {
        return signal_self(context, trap_signal).await;
    }
    // A job ended by a signal to its pid is shown by the signal, as by `kill %1`.
    if !signal_zero && let Some(job) = context.shell.jobs_mut().resolve_pid(pid) {
        job.record_signal(trap_signal);
    }
    signal_pid(context, pid, signal_zero, trap_signal)
}

/// A signal the shell sends itself, `kill -TERM $$`: handled inside the shell, as Bash
/// handles it, rather than delivered through the OS. Delivered, `INT` and `TERM` ended
/// cash at once, trap or no trap.
///
/// `KILL` ends the shell. A trapped signal runs its trap and the shell goes on. Otherwise
/// the signal's default: a script ends with 128 + the signal's number; an interactive
/// shell ignores `TERM`, abandons the line on `INT` as Ctrl-C does, and exits on `HUP`.
/// `QUIT` (which Bash ignores in all cases), `STOP`, `TSTP`, `CONT` and `CHLD` are ignored.
async fn signal_self<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    signal: TrapSignal,
) -> Result<ExecutionResult, cash_core::Error> {
    let TrapSignal::Signal(raw) = signal else {
        return Ok(ExecutionResult::success());
    };
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let status = (128 + raw as i32) as u8;
    let exit = |status: u8| ExecutionResult {
        exit_code: ExecutionExitCode::from(status),
        next_control_flow: cash_core::ExecutionControlFlow::ExitShell,
    };

    let name = signal.as_str();
    if name == "KILL" {
        return Ok(exit(status));
    }
    if let Some(result) = context
        .shell
        .raise_signal_trap(signal, &context.params)
        .await
    {
        result?;
        return Ok(ExecutionResult::success());
    }
    match name {
        // Bash ignores QUIT in all cases, scripts included.
        "STOP" | "TSTP" | "CONT" | "CHLD" | "QUIT" => Ok(ExecutionResult::success()),
        _ if !context.shell.options().interactive => Ok(exit(status)),
        "INT" => {
            writeln!(context.error_stream())?;
            Err(cash_core::ErrorKind::Interrupted.into())
        }
        "HUP" => Ok(exit(status)),
        _ => Ok(ExecutionResult::success()),
    }
}

/// Signal one process id, reporting failure the way bash reports it.
/// Signals the job a job spec (`%1`) names.
fn signal_job_spec<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    job_spec: &str,
    signal_zero: bool,
    trap_signal: TrapSignal,
) -> Result<ExecutionResult, cash_core::Error> {
    let Some(job) = context.shell.jobs_mut().resolve_job_spec(job_spec) else {
        writeln!(
            context.error_stream(),
            "{}: {}: no such job",
            context.command_name,
            job_spec
        )?;
        return Ok(ExecutionResult::general_error());
    };
    if signal_zero {
        job.check_signalable()?;
    } else {
        job.kill(trap_signal)?;
    }
    Ok(ExecutionResult::success())
}

/// Signals the job `pid` is the number of, when it is the number of a job that started
/// no program (D70): it names that job, which has no process of its own.
fn signal_job_known_as<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    pid: i32,
    signal_zero: bool,
    trap_signal: TrapSignal,
) -> Result<Option<ExecutionResult>, cash_core::Error> {
    let Some(job) = context
        .shell
        .jobs_mut()
        .resolve_pid(pid)
        .filter(|job| job.is_known_as(pid))
    else {
        return Ok(None);
    };
    if !signal_zero {
        job.kill(trap_signal)?;
    } else if !job.runs_inside_the_shell() {
        writeln!(
            context.error_stream(),
            "{}: ({pid}) - No such process",
            context.command_name
        )?;
        return Ok(Some(ExecutionResult::general_error()));
    }
    Ok(Some(ExecutionResult::success()))
}

fn signal_pid<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    pid: i32,
    signal_zero: bool,
    trap_signal: TrapSignal,
) -> Result<ExecutionResult, cash_core::Error> {
    let result = if signal_zero {
        sys::signal::check_signalable(pid)
    } else {
        sys::signal::kill_process(pid, trap_signal)
    };

    if let Err(e) = result {
        // bash's wording, because scripts and users both read it: a target that is not
        // there is "No such process", not an OS error number.
        if e.as_io_error()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
        {
            writeln!(
                context.error_stream(),
                "{}: ({pid}) - No such process",
                context.command_name
            )?;
        } else {
            let e = e.worded();
            writeln!(context.error_stream(), "{}: {e}", context.command_name)?;
        }
        return Ok(ExecutionResult::general_error());
    }

    Ok(ExecutionResult::success())
}

fn print_signals(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    signals: &[String],
) -> Result<ExecutionResult, cash_core::Error> {
    let mut exit_code = ExecutionResult::success();
    if !signals.is_empty() {
        for s in signals {
            // If the user gives us a code, we print the name; if they give a name, we print its
            // code.
            enum PrintSignal {
                Name(&'static str),
                Num(i32),
            }

            let signal = if let Ok(n) = s.parse::<i32>() {
                // bash compatibility. `SIGHUP` -> `HUP`
                TrapSignal::try_from(n).map(|s| {
                    PrintSignal::Name(s.as_str().strip_prefix("SIG").unwrap_or(s.as_str()))
                })
            } else {
                TrapSignal::try_from(s.as_str()).map(|sig| {
                    i32::try_from(sig).map_or(PrintSignal::Name(sig.as_str()), PrintSignal::Num)
                })
            };

            match signal {
                Ok(PrintSignal::Num(n)) => {
                    writeln!(context.stdout(), "{n}")?;
                }
                Ok(PrintSignal::Name(s)) => {
                    writeln!(context.stdout(), "{s}")?;
                }
                Err(e) => {
                    writeln!(context.error_stream(), "{e}")?;
                    exit_code = ExecutionResult::general_error();
                }
            }
        }
    } else {
        return cash_core::traps::format_signals(
            context.stdout(),
            TrapSignal::iterator().filter(|s| !matches!(s, TrapSignal::Exit)),
        )
        .map(|()| ExecutionResult::success());
    }

    Ok(exit_code)
}
