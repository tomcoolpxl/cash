//! F1: the one-screen help (spec D75), drawn on the alternate screen and put back the
//! way it was found; the line editor then redraws the prompt and the line as they were.

use cash_builtins::helpdocs::f1::{self, Palette};
use nu_ansi_term::{Color, Style};

use super::highlighter::styles;

/// F1 on `reedline`'s line: the screen, until a key closes it. `colour` is the shell's
/// colour setting; `NO_COLOR` in the shell's environment turns it off as well.
pub(crate) fn on_key(shell: &crate::ShellRef<impl cash_core::ShellExtensions>, colour: bool) {
    let (tools, no_color) = {
        let shell = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(shell.lock())
        });
        (
            shell.builtins().len(),
            shell
                .env_str("NO_COLOR")
                .is_some_and(|value| !value.is_empty()),
        )
    };
    let palette = if colour && !no_color {
        palette()
    } else {
        Palette::none()
    };
    if let Err(error) = f1::show(env!("CARGO_PKG_VERSION"), tools, &palette) {
        tracing::warn!("help: {error}");
    }
}

/// The prompt's colours: the title and section names in the accent the starter
/// `~/.bashrc` draws the folder in, examples and keys as the highlighter paints a
/// command and a quoted word, and the notes dimmed.
fn palette() -> Palette {
    Palette {
        accent: Style::new().bold().fg(Color::Blue),
        example: styles::builtin(),
        key: styles::quoted(),
        note: Style::new().dimmed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_is_the_prompts_and_the_highlighters() {
        let palette = palette();
        assert_eq!(palette.accent, Style::new().bold().fg(Color::Blue));
        assert_eq!(palette.example, styles::builtin());
        assert_eq!(palette.key, styles::quoted());
        assert!(palette.note.is_dimmed && palette.note.foreground.is_none());
    }
}
