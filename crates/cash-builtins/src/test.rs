use clap::Parser;
use std::io::Write;

use cash_core::{
    ErrorKind, ExecutionExitCode, ExecutionParameters, ExecutionResult, Shell, builtins, test_expr,
};

/// Evaluate test expression.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct TestCommand {
    #[clap(allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for TestCommand {
    type Error = cash_core::Error;

    /// Override the default [`builtins::Command::new`] function to handle clap's limitation related
    /// to `--`. See [`builtins::parse_known`] for more information
    /// TODO(test): we can safely remove this after the issue is resolved
    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        let (mut this, rest_args) = cash_core::builtins::try_parse_known::<Self>(args)?;
        if let Some(args) = rest_args {
            this.args.extend(args);
        }
        Ok(this)
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        let mut args = self.args.as_slice();

        if context.command_name == "[" {
            match args.last() {
                Some(s) if s == "]" => (),
                None | Some(_) => {
                    writeln!(context.error_stream(), "[: missing `]'")?;
                    return Ok(ExecutionExitCode::InvalidUsage.into());
                }
            }

            args = &args[0..args.len() - 1];
        }

        if execute_test(context.shell, &context.params, args).await? {
            Ok(ExecutionResult::success())
        } else {
            Ok(ExecutionResult::general_error())
        }
    }
}

async fn execute_test(
    shell: &mut Shell<impl cash_core::ShellExtensions>,
    params: &ExecutionParameters,
    args: &[String],
) -> Result<bool, cash_core::Error> {
    let test_command = cash_parser::test_command::parse(args)
        .map_err(|_| ErrorKind::TestError(complaint(args)))?;
    test_expr::eval_expr(&test_command, shell, params).await
}

/// Bash's complaint about arguments `test` cannot read, found as Bash finds it, by their
/// number (POSIX's rules): two that are not `! x` or a unary test are `1: unary operator
/// expected`, three without a binary operator `a: binary operator expected`, and more
/// `argument expected` when an operator ends them, `too many arguments` otherwise. cash
/// said `invalid test command`.
fn complaint(args: &[String]) -> String {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    complaint_for(&args)
}

fn complaint_for(args: &[&str]) -> String {
    match args {
        ["!", rest @ ..] if (2..=4).contains(&args.len()) && rest.len() >= 2 => complaint_for(rest),
        ["(", inner @ .., ")"] if args.len() == 4 => complaint_for(inner),
        [first, _] => format!("{first}: unary operator expected"),
        [_, second, _] => format!("{second}: binary operator expected"),
        [.., last] if is_operator(last) => "argument expected".to_owned(),
        _ => "too many arguments".to_owned(),
    }
}

/// Whether `word` is an operator of `test`'s that needs an operand after it.
fn is_operator(word: &str) -> bool {
    matches!(
        word,
        "!" | "-a" | "-o" | "(" | "=" | "==" | "!=" | "<" | ">"
    ) || (word.len() == 2 && word.starts_with('-'))
        || matches!(
            word,
            "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" | "-nt" | "-ot" | "-ef"
        )
}
