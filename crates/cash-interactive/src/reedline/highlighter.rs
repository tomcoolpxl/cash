use crate::{highlighting, refs};
use nu_ansi_term::{Color, Style};

mod styles {
    use super::{Color, Style};

    pub fn default() -> Style {
        Style::new().fg(Color::Default)
    }

    pub fn comment() -> Style {
        Style::new().fg(Color::DarkGray)
    }

    pub fn arithmetic() -> Style {
        Style::new().fg(Color::LightBlue)
    }

    pub fn parameter() -> Style {
        Style::new().fg(Color::LightMagenta)
    }

    pub fn command_substitution() -> Style {
        Style::new().fg(Color::LightBlue)
    }

    pub fn quoted() -> Style {
        Style::new().fg(Color::Yellow)
    }

    pub fn operator() -> Style {
        Style::new().fg(Color::Default).italic()
    }

    pub fn assignment() -> Style {
        Style::new().fg(Color::LightGray).dimmed()
    }

    pub fn hyphen_option() -> Style {
        Style::new().fg(Color::Default).italic()
    }

    pub fn function() -> Style {
        Style::new().bold().fg(Color::Yellow)
    }

    pub fn keyword() -> Style {
        Style::new().bold().fg(Color::LightYellow).italic()
    }

    pub fn builtin() -> Style {
        Style::new().bold().fg(Color::Green)
    }

    pub fn alias() -> Style {
        Style::new().bold().fg(Color::Cyan)
    }

    pub fn external_command() -> Style {
        Style::new().bold().fg(Color::Green)
    }

    pub fn not_found_command() -> Style {
        Style::new().bold().fg(Color::Red)
    }

    pub fn unknown_command() -> Style {
        Style::new().bold().fg(Color::Default)
    }
}

/// The line editor's highlighter, installed whether or not colours are wanted: it also
/// decides where abbreviations expand (D60).
pub(crate) struct ReedlineHighlighter<SE: cash_core::ShellExtensions> {
    pub shell: refs::ShellRef<SE>,
    /// Colour the line by syntax; otherwise it is shown in the terminal's default colour.
    pub syntax: bool,
}

pub(crate) struct PlainTextHighlighter;

impl reedline::Highlighter for PlainTextHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> reedline::StyledText {
        let mut styled = reedline::StyledText::new();
        styled.push((Style::new(), line.to_owned()));
        styled
    }
}

impl<SE: cash_core::ShellExtensions> reedline::Highlighter for ReedlineHighlighter<SE> {
    #[expect(clippy::significant_drop_tightening)]
    fn highlight(&self, line: &str, cursor: usize) -> reedline::StyledText {
        if !self.syntax {
            return PlainTextHighlighter.highlight(line, cursor);
        }

        let shell = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.shell.lock())
        });

        let highlighted = highlighting::highlight_command(shell.as_ref(), line, cursor);

        let mut styled = reedline::StyledText::new();
        for (kind, text) in highlighted.iter() {
            styled.push((kind_to_style(kind), text.to_owned()));
        }

        styled
    }

    /// Reedline expands a word it has an abbreviation for on Space and Enter, and asks
    /// here first. Its table is only ever added to (see `input_backend`), so this also
    /// refuses a name `abbr -e` removed since.
    fn should_expand_abbr(
        &self,
        line: &str,
        word_start: usize,
        context: reedline::AbbrExpandContext,
    ) -> bool {
        if context != reedline::AbbrExpandContext::WordAbbreviation {
            return false;
        }
        let shell = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.shell.lock())
        });
        highlighting::abbreviation_applies(shell.as_ref(), line, word_start)
    }
}

fn kind_to_style(kind: highlighting::HighlightKind) -> Style {
    match kind {
        highlighting::HighlightKind::Default => styles::default(),
        highlighting::HighlightKind::Comment => styles::comment(),
        highlighting::HighlightKind::Arithmetic => styles::arithmetic(),
        highlighting::HighlightKind::Parameter => styles::parameter(),
        highlighting::HighlightKind::CommandSubstitution => styles::command_substitution(),
        highlighting::HighlightKind::Quoted => styles::quoted(),
        highlighting::HighlightKind::Operator => styles::operator(),
        highlighting::HighlightKind::Assignment => styles::assignment(),
        highlighting::HighlightKind::HyphenOption => styles::hyphen_option(),
        highlighting::HighlightKind::Function => styles::function(),
        highlighting::HighlightKind::Keyword => styles::keyword(),
        highlighting::HighlightKind::Builtin => styles::builtin(),
        highlighting::HighlightKind::Alias => styles::alias(),
        highlighting::HighlightKind::ExternalCommand => styles::external_command(),
        highlighting::HighlightKind::NotFoundCommand => styles::not_found_command(),
        highlighting::HighlightKind::UnknownCommand => styles::unknown_command(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reedline::Highlighter;

    #[test]
    fn plain_text_highlighter_uses_terminal_default_colors() {
        let styled = PlainTextHighlighter.highlight("echo hello", 0);

        assert_eq!(styled.buffer.len(), 1);
        assert_eq!(styled.buffer[0].0.foreground, None);
        assert_eq!(styled.buffer[0].0.background, None);
        assert_eq!(styled.buffer[0].1, "echo hello");
    }
}
