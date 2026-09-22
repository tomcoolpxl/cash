use clap::Parser;
use std::io::Write;

use cash_core::builtins::Command as _;
use cash_core::{ExecutionResult, builtins};

use crate::dirs;

/// Push a path onto the current directory stack.
#[derive(Parser)]
pub(crate) struct PushdCommand {
    /// Push the path without changing the current working directory.
    #[clap(short = 'n')]
    no_directory_change: bool,

    /// Directory to push, or `+N`/`-N` to rotate the stack instead.
    #[arg(allow_hyphen_values = true)]
    dir: Option<String>,
}

impl builtins::Command for PushdCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        // cash: `+N` used to reach the filesystem — `pushd +1` tried to enter a directory
        // literally named `+1` and reported that it could not be found. A wrong answer to
        // a spelling bash has always rotated the stack with.
        if let Some(arg) = self.dir.as_deref()
            && let Some(selector) = dirs::Selector::parse(arg)
        {
            let listing = dirs::listing(context.shell);
            let Some(index) = selector.index_in(listing.len()) else {
                return dirs::report_bad_index(&context, listing.len(), arg);
            };

            let rotated = rotate(listing, index);
            dirs::store(context.shell, rotated, !self.no_directory_change)?;

            // bash says nothing for a rotation it was told not to follow.
            if self.no_directory_change {
                return Ok(ExecutionResult::success());
            }

            return print_stack(context).await;
        }

        if let Some(dir) = self.dir.as_deref() {
            if self.no_directory_change {
                // bash records the directory as typed and does not check it — it is the
                // later `popd` that has to be able to enter it.
                context
                    .shell
                    .directory_stack_mut()
                    .push(std::path::PathBuf::from(dir));
            } else {
                let prev_working_dir = context.shell.working_dir().to_path_buf();
                context.shell.set_working_dir(std::path::Path::new(dir))?;
                context.shell.directory_stack_mut().push(prev_working_dir);
            }

            return print_stack(context).await;
        }

        // cash: a bare `pushd` was a usage error for want of its required argument,
        // although bash's is the shorthand people actually type: exchange the top two
        // entries and follow the swap.
        if self.no_directory_change {
            // Nothing to exchange that would not also move the shell, so bash does
            // nothing at all — silently, and successfully.
            return Ok(ExecutionResult::success());
        }

        let mut listing = dirs::listing(context.shell);
        if listing.len() < 2 {
            writeln!(
                context.stderr(),
                "{}: no other directory",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        }

        listing.swap(0, 1);
        dirs::store(context.shell, listing, true)?;

        print_stack(context).await
    }
}

/// Brings the entry at `index` to the top, carrying everything above it round to the
/// bottom — bash rotates the stack rather than lifting one entry out of it.
fn rotate(listing: Vec<std::path::PathBuf>, index: usize) -> Vec<std::path::PathBuf> {
    let mut rotated = listing;
    rotated.rotate_left(index);
    rotated
}

/// Both `pushd` and `popd` report the stack they leave behind.
async fn print_stack<SE: cash_core::ShellExtensions>(
    context: cash_core::ExecutionContext<'_, SE>,
) -> Result<ExecutionResult, cash_core::Error> {
    let dirs_cmd = dirs::DirsCommand::default();
    dirs_cmd.execute(context).await?;
    Ok(ExecutionResult::success())
}
