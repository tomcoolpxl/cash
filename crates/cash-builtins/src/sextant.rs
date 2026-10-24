//! `sextant`: a shooter drawn in the terminal with text (decisions.md D88), on
//! `cash-sextant`.
//!
//! It takes the console as `top` does: raw keys and the alternate screen until the player
//! quits, then the prompt as it was. The cell patterns come from a DEC soft font the game
//! sends the terminal, so any font does; `--unicode` takes them from the font's sextants
//! instead, for a terminal without soft fonts. `--glyphs` shows both, to see which works.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Play sextant, a shooter drawn with text.
#[derive(Parser)]
#[clap(disable_version_flag = true)]
pub(crate) struct SextantCommand {
    /// Let the game play by itself.
    #[arg(long)]
    demo: bool,

    /// Draw with the font's Unicode sextants, not a soft font.
    #[arg(long)]
    unicode: bool,

    /// Frames a second, 1 to 240.
    #[arg(long, value_name = "N", default_value_t = 60,
          value_parser = clap::value_parser!(u32).range(1..=240))]
    fps: u32,

    /// Print the 64 cell patterns both ways, and play nothing.
    #[arg(long)]
    glyphs: bool,
}

impl builtins::Command for SextantCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.glyphs {
            context
                .stdout()
                .write_all(cash_sextant::glyph_table().as_bytes())?;
            return Ok(ExecutionResult::success());
        }
        let terminal = context
            .try_fd(cash_core::openfiles::OpenFiles::STDOUT_FD)
            .is_some_and(|fd| fd.is_terminal());
        if !terminal {
            writeln!(
                context.stderr(),
                "{}: standard output is not a terminal",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        }
        let options = cash_sextant::Options {
            demo: self.demo,
            soft_font: !self.unicode,
            fps: self.fps,
        };
        let outcome = cash_sextant::play(&mut context.stdout(), options)?;
        writeln!(
            context.stdout(),
            "{}: score {}, wave {}",
            context.command_name,
            outcome.score,
            outcome.wave
        )?;
        Ok(ExecutionResult::success())
    }
}
