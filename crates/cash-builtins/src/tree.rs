//! A small native `tree` for Windows.
//!
//! The useful part of `tree` is its shape, not filesystem statistics.  This walker only
//! reads directory entries and the metadata needed to distinguish directories from
//! files and reparse points.  Reparse points are printed but never followed, so a
//! junction cannot make the walk escape its root or loop forever.

use std::io::Write;
use std::path::{Path, PathBuf};

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Display a directory hierarchy.
#[derive(Parser)]
pub(crate) struct TreeCommand {
    /// Include entries whose names begin with a dot.
    #[arg(short = 'a')]
    all: bool,

    /// List directories only.
    #[arg(short = 'd')]
    directories_only: bool,

    /// Descend at most LEVEL directory levels.
    #[arg(short = 'L', value_name = "LEVEL", value_parser = parse_depth)]
    max_depth: Option<usize>,

    /// Print the path prefix for every entry.
    #[arg(short = 'f')]
    full_path: bool,

    /// Put directories before files at each level.
    #[arg(long)]
    dirsfirst: bool,

    /// Do not print the final directory and file count.
    #[arg(long)]
    noreport: bool,

    /// Directory to inspect (the current directory by default).
    #[arg(value_name = "DIRECTORY", default_value = ".")]
    path: PathBuf,
}

impl builtins::Command for TreeCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let actual = context.shell.absolute_path(&self.path);
        let shown = self.path.clone();
        let mut stdout = context.stdout();

        writeln!(stdout, "{}", render(&shown))?;

        // The root is looked at through a link, as GNU's tree lists the folder a link it
        // is given names.
        let metadata = match std::fs::metadata(&actual) {
            Ok(metadata) => metadata,
            Err(error) => {
                let error = cash_core::error::os_error_text(&error);
                writeln!(context.stderr(), "tree: {}: {error}", render(&shown))?;
                return Ok(ExecutionResult::general_error());
            }
        };

        if !metadata.is_dir() {
            if !self.noreport {
                writeln!(stdout, "\n0 directories, 1 file")?;
            }
            return Ok(ExecutionResult::success());
        }

        let mut state = WalkState {
            out: &mut stdout,
            directories: 0,
            files: 0,
            failed: false,
        };
        self.walk(&actual, &shown, "", 0, &mut state, &context)?;
        if context.stop_token().requested() {
            drop(stdout);
            return context.stopped().await;
        }

        if !self.noreport {
            writeln!(
                state.out,
                "\n{} {}, {} {}",
                state.directories,
                plural(state.directories, "directory", "directories"),
                state.files,
                plural(state.files, "file", "files")
            )?;
        }

        if state.failed {
            Ok(ExecutionResult::general_error())
        } else {
            Ok(ExecutionResult::success())
        }
    }
}

impl TreeCommand {
    fn walk<SE: cash_core::ShellExtensions, W: Write>(
        &self,
        actual: &Path,
        shown: &Path,
        prefix: &str,
        depth: usize,
        state: &mut WalkState<'_, W>,
        context: &cash_core::ExecutionContext<'_, SE>,
    ) -> Result<(), cash_core::Error> {
        // A Ctrl-C, or `kill` of a background job, stops the walk.
        if self.max_depth.is_some_and(|limit| depth >= limit) || context.stop_token().requested() {
            return Ok(());
        }

        let entries = match std::fs::read_dir(actual) {
            Ok(entries) => entries,
            Err(error) => {
                let error = cash_core::error::os_error_text(&error);
                writeln!(context.stderr(), "tree: {}: {error}", render(shown))?;
                state.failed = true;
                return Ok(());
            }
        };

        let mut children = Vec::new();
        for result in entries {
            let entry = match result {
                Ok(entry) => entry,
                Err(error) => {
                    let error = cash_core::error::os_error_text(&error);
                    writeln!(context.stderr(), "tree: {}: {error}", render(shown))?;
                    state.failed = true;
                    continue;
                }
            };
            let name = entry.file_name();
            if !self.all && name.to_string_lossy().starts_with('.') {
                continue;
            }
            // What it is comes free with the listing; a link is not gone into, and a
            // cloud placeholder or a deduplicated folder is the folder it holds.
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(error) => {
                    let error = cash_core::error::os_error_text(&error);
                    writeln!(
                        context.stderr(),
                        "tree: {}: {error}",
                        render(&shown.join(&name))
                    )?;
                    state.failed = true;
                    continue;
                }
            };
            let directory = crate::walk::kind(file_type) == crate::walk::Kind::Folder;
            if self.directories_only && !directory {
                continue;
            }
            children.push((name, entry.path(), directory));
        }

        children.sort_by(|left, right| {
            if self.dirsfirst {
                right.2.cmp(&left.2).then_with(|| left.0.cmp(&right.0))
            } else {
                left.0.cmp(&right.0)
            }
        });

        let child_count = children.len();
        for (index, (name, child_actual, directory)) in children.into_iter().enumerate() {
            let last = index + 1 == child_count;
            let branch = if last { "└── " } else { "├── " };
            let child_shown = shown.join(&name);
            let label = if self.full_path {
                render(&child_shown)
            } else {
                name.to_string_lossy().into_owned()
            };
            writeln!(state.out, "{prefix}{branch}{label}")?;

            if directory {
                state.directories += 1;
                let continuation = if last { "    " } else { "│   " };
                self.walk(
                    &child_actual,
                    &child_shown,
                    &format!("{prefix}{continuation}"),
                    depth + 1,
                    state,
                    context,
                )?;
            } else {
                state.files += 1;
            }
        }
        Ok(())
    }
}

struct WalkState<'a, W> {
    out: &'a mut W,
    directories: usize,
    files: usize,
    failed: bool,
}

const fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}

fn render(path: &Path) -> String {
    cash_win32::path::render(path)
}

fn parse_depth(value: &str) -> Result<usize, String> {
    match value.parse::<usize>() {
        Ok(depth) if depth > 0 => Ok(depth),
        _ => Err("depth must be a positive integer".to_owned()),
    }
}
