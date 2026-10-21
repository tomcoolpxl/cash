use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Evaluate the given string as script.
#[derive(Parser)]
pub(crate) struct EvalCommand {
    /// The script to evaluate.
    #[clap(allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for EvalCommand {
    type Error = cash_core::Error;

    /// `eval` takes no option, as Bash's `no_options` has it: a first word that is one
    /// is invalid, its first letter named (`--long` as `--`), status 2; `--` is passed
    /// over. `eval -x` ran `-x`.
    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        let args: Vec<String> = args.into_iter().collect();
        if let Some(first) = args.get(1)
            && first.len() > 1
            && first.starts_with('-')
            && first != "--"
            && first != "--help"
        {
            return Err(crate::invalid_option(first));
        }
        builtins::parse_args(args)
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        if !self.args.is_empty() {
            let args_concatenated = self.args.join(" ");

            tracing::debug!("Applying eval to: {:?}", args_concatenated);

            // Our new source context is relative to the current position because we are only
            // providing the raw string being eval'd.
            // TODO(source-info): Provide the location of the specific tokens that make up
            // `self.args`.
            let source_info = context.shell.call_stack().current_pos_as_source_info();

            // Return the direct result of running the string; we intentionally
            // pass through the result and honor its requested control flow. eval
            // executes in the current environment, so all control flow (return,
            // exit, break, continue) should propagate.
            context
                .shell
                .run_eval_string(args_concatenated, &source_info, &context.params)
                .await
        } else {
            Ok(ExecutionResult::success())
        }
    }
}
