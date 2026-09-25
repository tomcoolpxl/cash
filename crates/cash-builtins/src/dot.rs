use std::path::{Path, PathBuf};

use cash_core::builtins;
use cash_core::sys::fs::PathExt as _;
use clap::Parser;

/// Evaluate the provided script in the current shell environment.
#[derive(Parser)]
#[command(args_override_self = true)]
pub(crate) struct DotCommand {
    /// Colon-separated path to search instead of PATH (Bash 5.3).
    #[arg(short = 'p', value_name = "PATH")]
    search_path: Option<String>,

    /// Path to the script to evaluate.
    script_path: String,

    /// Any arguments to be passed as positional parameters to the script.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    script_args: Vec<String>,
}

impl builtins::Command for DotCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        let script_path = resolve_source_path(
            context.shell,
            &self.script_path,
            self.search_path.as_deref(),
        )
        .ok_or_else(|| {
            cash_core::Error::from(cash_core::ErrorKind::FailedSourcingFile(
                PathBuf::from(&self.script_path),
                std::io::Error::from(std::io::ErrorKind::NotFound),
            ))
        })?;

        context
            .shell
            .source_script(&script_path, self.script_args.iter(), &context.params)
            .await
    }
}

/// Resolve a `.`/`source` operand using Bash's lookup order.
///
/// An operand containing a path separator is used directly. Otherwise `-p` wins,
/// followed by `$PATH` when `shopt sourcepath` is enabled. Non-POSIX Bash falls back
/// to the current directory only when no explicit `-p` path was supplied.
fn resolve_source_path<SE: cash_core::ShellExtensions>(
    shell: &cash_core::Shell<SE>,
    script: &str,
    path_override: Option<&str>,
) -> Option<PathBuf> {
    if Path::new(script).is_absolute() || cash_core::sys::fs::contains_path_separator(script) {
        return Some(PathBuf::from(script));
    }

    let search_path: Option<std::borrow::Cow<'_, str>> = if let Some(path) = path_override {
        // Bash normalizes `source -p '' file` to a search of the current directory.
        Some(std::borrow::Cow::Borrowed(if path.is_empty() {
            "."
        } else {
            path
        }))
    } else if shell.options().source_builtin_searches_path {
        Some(shell.env_str("PATH").unwrap_or_default())
    } else {
        None
    };

    if let Some(search_path) = search_path {
        for directory in cash_core::sys::fs::split_paths_preserving_empty(search_path.as_ref()) {
            let candidate = shell.absolute_path(directory).join(script);
            if candidate.as_path().readable() {
                let candidate_text = candidate.to_string_lossy();
                let rendered =
                    cash_core::sys::fs::normalize_path_separators(candidate_text.as_ref());
                return Some(PathBuf::from(rendered.as_ref()));
            }
        }
    }

    if path_override.is_none() && !shell.options().posix_mode {
        Some(PathBuf::from(script))
    } else {
        None
    }
}
