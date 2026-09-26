use std::io::Write;

use clap::{CommandFactory as _, FromArgMatches as _, Parser, parser::ValueSource};

use cash_core::{ExecutionResult, builtins};

use crate::lookup::{self, Resolved};

/// Inspect the type of a named shell item.
#[derive(Parser)]
// Bash accepts `type -t -p -t`; a repeat replaces the earlier occurrence, so the index
// `new` compares is the last one.
#[command(args_override_self = true)]
pub(crate) struct TypeCommand {
    /// Display all locations of the specified name, not just the first.
    #[arg(short = 'a')]
    all_locations: bool,

    /// Don't consider functions when resolving the name.
    #[arg(short = 'f')]
    suppress_func_lookup: bool,

    /// Force searching by file path, even if the name is an alias, built-in
    /// command, or shell function.
    #[arg(short = 'P')]
    force_path_search: bool,

    /// Show file path only.
    #[arg(short = 'p')]
    show_path_only: bool,

    /// Only display the type of the specified name.
    #[arg(short = 't')]
    type_only: bool,

    /// Names to search for.
    names: Vec<String>,
}

impl builtins::Command for TypeCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        let matches = Self::command().try_get_matches_from(args)?;
        let mut command = Self::from_arg_matches(&matches)?;

        // Bash's `-t`, `-p` and `-P` each switch off the others' output form, so whichever
        // comes last decides between printing the type and printing the path: `-Pt` prints
        // `file`, `-tP` the path. `-P`'s forced path search is not switched off, though, so
        // `-Pt` still skips functions and builtins.
        let last = |id: &str| {
            (matches.value_source(id) == Some(ValueSource::CommandLine))
                .then(|| matches.indices_of(id)?.max())
                .flatten()
        };
        let type_at = last("type_only");
        let path_at = last("show_path_only").max(last("force_path_search"));
        if type_at.is_some() || path_at.is_some() {
            command.type_only = type_at > path_at;
            command.show_path_only = !command.type_only;
        }

        Ok(command)
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();
        let options = lookup::Options {
            force_path_search: self.force_path_search,
            suppress_func_lookup: self.suppress_func_lookup,
            all_locations: self.all_locations,
            path_dirs: None,
        };

        for name in &self.names {
            let mut resolved_types = lookup::resolve(context.shell, name, &options);

            // Bash's `-a` skips the hash table unless `-P` forces a path search, and even
            // then a hashed path is printed without counting as found. A name that is only
            // hashed -- say to a file that has since gone -- is therefore not found.
            if self.all_locations && !self.force_path_search {
                resolved_types.retain(|r| !matches!(r, Resolved::File { hashed: true, .. }));
            }
            let found = !self.all_locations
                || resolved_types
                    .iter()
                    .any(|r| !matches!(r, Resolved::File { hashed: true, .. }));

            if resolved_types.is_empty() {
                if !self.type_only && !self.show_path_only {
                    writeln!(context.stderr(), "type: {name}: not found")?;
                }

                result = ExecutionResult::general_error();
                continue;
            }

            // Bash consults its hash table through the same lookup that counts a hit when
            // running the command.
            let counts_a_hit = matches!(
                resolved_types.first(),
                Some(Resolved::File { hashed: true, .. })
            );

            for resolved_type in resolved_types {
                if self.show_path_only && !matches!(resolved_type, Resolved::File { .. }) {
                    // Do nothing.
                } else if self.type_only {
                    match &resolved_type {
                        Resolved::Alias(_) => {
                            writeln!(context.stdout(), "alias")?;
                        }
                        Resolved::Keyword => {
                            writeln!(context.stdout(), "keyword")?;
                        }
                        Resolved::Function(_) => {
                            writeln!(context.stdout(), "function")?;
                        }
                        Resolved::Builtin => {
                            writeln!(context.stdout(), "builtin")?;
                        }
                        Resolved::File { .. } => {
                            writeln!(context.stdout(), "file")?;
                        }
                    }
                } else {
                    match &resolved_type {
                        Resolved::File { path, .. } if self.show_path_only => {
                            #[cfg(windows)]
                            let rendered = cash_win32::path::render(path);
                            #[cfg(not(windows))]
                            let rendered = path.to_string_lossy();
                            writeln!(context.stdout(), "{rendered}")?;
                        }
                        _ => lookup::describe(context.stdout(), name, &resolved_type)?,
                    }
                }

                // If we only want the first, then break after the first.
                if !self.all_locations {
                    break;
                }
            }

            if counts_a_hit {
                context.shell.program_location_cache_mut().record_hit(name);
            }
            if !found {
                result = ExecutionResult::general_error();
            }
        }

        Ok(result)
    }
}
