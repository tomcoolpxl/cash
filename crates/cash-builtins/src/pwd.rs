use cash_core::{ExecutionResult, builtins};
use clap::Parser;
use std::{borrow::Cow, io::Write, path::Path};

/// Display the current working directory.
#[derive(Parser)]
pub(crate) struct PwdCommand {
    /// Print the physical directory without any symlinks.
    #[arg(short = 'P', overrides_with = "allow_symlinks")]
    physical: bool,

    /// Print $PWD if it names the current working directory.
    #[arg(short = 'L', overrides_with = "physical")]
    allow_symlinks: bool,

    /// Print the directory in Windows form, as cash does unless `unixpaths` is on (MSYS2
    /// Bash's -W).
    #[arg(short = 'W')]
    windows: bool,

    /// Operands, which Bash's `pwd` takes and ignores; cash refused `pwd extra` as an
    /// invalid option.
    #[arg(hide = true)]
    ignored: Vec<String>,
}

impl builtins::Command for PwdCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        let mut cwd: Cow<'_, Path> = context.shell.working_dir().into();

        let should_canonicalize = self.physical
            || context
                .shell
                .options()
                .do_not_resolve_symlinks_when_changing_dir;

        if should_canonicalize {
            cwd = cwd.canonicalize()?.into();
        }

        // The resolved directory comes back as `\\?\C:\…`; cash prints `C:/…` (D3).
        // `/c/…` under `unixpaths` (D82), but for `-W`.
        let shown = if self.windows {
            cash_win32::path::render(&cwd)
        } else {
            context.shell.shown_path(&cwd)
        };
        writeln!(context.stdout(), "{shown}")?;

        Ok(ExecutionResult::success())
    }
}
