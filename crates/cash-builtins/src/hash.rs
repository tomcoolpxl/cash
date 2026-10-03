use clap::Parser;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

use cash_core::{ExecutionResult, builtins, escape};

#[derive(Parser)]
pub(crate) struct HashCommand {
    /// Remove entries associated with the given names.
    #[arg(short = 'd')]
    remove: bool,

    /// Display paths in a format usable for input.
    #[arg(short = 'l')]
    display_as_usable_input: bool,

    /// The path to associate with the names.
    #[arg(short = 'p', value_name = "PATH")]
    path_to_use: Option<PathBuf>,

    /// Remove all entries.
    #[arg(short = 'r')]
    remove_all: bool,

    /// Display the paths associated with the names.
    #[arg(short = 't')]
    display_paths: bool,

    /// Names to process.
    names: Vec<String>,
}

impl builtins::Command for HashCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();
        let cmd = &context.command_name;

        if self.names.is_empty() && (self.remove || self.display_paths) {
            let option = if self.remove { "-d" } else { "-t" };
            writeln!(
                context.error_stream(),
                "{cmd}: {option}: option requires an argument"
            )?;
            return Ok(ExecutionResult::general_error());
        }

        if self.names.is_empty() && !self.remove_all {
            let entries: Vec<_> = context.shell.program_location_cache().entries().collect();

            if entries.is_empty() {
                // Bash 5.1 dropped the notice for `-l`, whose output is meant to be read back
                // in, and POSIX mode never printed it.
                if !self.display_as_usable_input && !context.shell.options().posix_mode {
                    writeln!(context.stdout(), "{cmd}: hash table empty")?;
                }
            } else if self.display_as_usable_input {
                // Unlike `-lt`, bash quotes here, so the listing reads back in even with the
                // space in `Program Files`.
                for (name, path, _) in entries {
                    writeln!(
                        context.stdout(),
                        "builtin hash -p {} {}",
                        escape::quote_if_needed(&render(path), escape::QuoteMode::SingleQuote),
                        escape::quote_if_needed(name, escape::QuoteMode::SingleQuote)
                    )?;
                }
            } else {
                writeln!(context.stdout(), "hits\tcommand")?;
                for (_, path, hits) in entries {
                    writeln!(context.stdout(), "{hits:4}\t{}", render(path))?;
                }
            }

            return Ok(result);
        }

        if self.remove_all {
            context.shell.program_location_cache_mut().reset();
        } else if self.remove {
            for name in &self.names {
                if !context.shell.program_location_cache_mut().unset(name) {
                    writeln!(context.error_stream(), "{cmd}: {name}: not found")?;
                    result = ExecutionResult::general_error();
                }
            }
        } else if self.display_paths {
            for name in &self.names {
                if let Some(path) = context.shell.program_location_cache().get(name) {
                    // Bash's lookup here counts as a hit, just as running the command does.
                    context.shell.program_location_cache_mut().record_hit(name);
                    let path = render(&path);

                    if self.display_as_usable_input {
                        // Bash prints this form unquoted, unlike the whole-table listing.
                        writeln!(context.stdout(), "builtin hash -p {path} {name}")?;
                    } else {
                        let mut prefix = String::new();

                        if self.names.len() > 1 {
                            prefix.push_str(name.as_str());
                            prefix.push('\t');
                        }

                        writeln!(context.stdout(), "{prefix}{path}")?;
                    }
                } else {
                    writeln!(context.error_stream(), "{cmd}: {name}: not found")?;
                    result = ExecutionResult::general_error();
                }
            }
        } else if let Some(path) = &self.path_to_use {
            // The shell's working directory is its own state, not the process's, so a
            // relative path is resolved against it before it's inspected -- but reported as given.
            let is_dir = context.shell.absolute_path(path).is_dir();

            for name in &self.names {
                if is_dir {
                    writeln!(
                        context.error_stream(),
                        "{cmd}: {}: Is a directory",
                        path.display()
                    )?;
                    result = ExecutionResult::general_error();
                    continue;
                }

                context
                    .shell
                    .program_location_cache_mut()
                    .set(name, path.clone());
            }
        } else {
            for name in &self.names {
                // Remove from the cache if already hashed.
                let _ = context.shell.program_location_cache_mut().unset(name);

                // Names with slashes are accepted silently
                if name.contains('/') {
                    continue;
                }

                // Hash the path
                if context
                    .shell
                    .find_first_executable_in_path_using_cache(name)
                    .is_none()
                {
                    writeln!(context.error_stream(), "{cmd}: {name}: not found")?;
                    result = ExecutionResult::general_error();
                }
            }
        }

        Ok(result)
    }
}

/// Renders a path with one canonical spelling (D3). A cached path is a `PATH` directory
/// joined to the file name, which otherwise shows up as `C:/Program Files/Git/usr/bin\ls.exe`.
fn render(path: &Path) -> String {
    cash_win32::path::render(path)
}
