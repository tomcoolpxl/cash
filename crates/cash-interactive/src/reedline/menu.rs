//! The completion menu: Reedline's columnar menu, leaving the cursor inside a quoted
//! directory's closing quote.
//!
//! cash (D40) completes a directory whose name needs quoting as `'my dir/'`. The quote is
//! closed, so Enter runs the line as it is; the cursor is put before the closing quote,
//! as PowerShell does, so typing and Tab carry on inside it and the path can go deeper.
//! Reedline has no cursor offset for a suggestion, so this wrapper moves the cursor after
//! it applies one. Everything else is the columnar menu's.

use reedline::{Completer, Editor, Menu, MenuEvent, Painter, Suggestion, UndoBehavior};

pub(crate) struct QuoteAwareMenu<M: Menu>(pub M);

/// After a completion: if the text before the cursor ends in `/'` or `/"`, a quoted
/// directory, step back over the closing quote.
fn step_inside_closing_quote(editor: &mut Editor) {
    editor.edit_buffer(
        |line| {
            let at = line.insertion_point();
            let before = line.get_buffer().get(..at).unwrap_or_default();
            if before.ends_with("/'") || before.ends_with("/\"") {
                line.set_insertion_point(at - 1);
            }
        },
        UndoBehavior::MoveCursor,
    );
}

impl<M: Menu> Menu for QuoteAwareMenu<M> {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn indicator(&self) -> &str {
        self.0.indicator()
    }

    fn is_active(&self) -> bool {
        self.0.is_active()
    }

    fn set_active(&mut self, active: bool) {
        self.0.set_active(active);
    }

    fn clear_input(&mut self) {
        self.0.clear_input();
    }

    fn on_activate(&mut self) {
        self.0.on_activate();
    }

    fn on_deactivate(&mut self) {
        self.0.on_deactivate();
    }

    fn handle_menu_event(&mut self, event: &MenuEvent) {
        self.0.handle_menu_event(event);
    }

    fn menu_event(&mut self, event: MenuEvent) {
        self.0.menu_event(event);
    }

    fn can_quick_complete(&self) -> bool {
        self.0.can_quick_complete()
    }

    fn can_partially_complete(
        &mut self,
        values_updated: bool,
        editor: &mut Editor,
        completer: &mut dyn Completer,
    ) -> bool {
        let completed = self
            .0
            .can_partially_complete(values_updated, editor, completer);
        if completed {
            step_inside_closing_quote(editor);
        }
        completed
    }

    fn update_values(&mut self, editor: &mut Editor, completer: &mut dyn Completer) {
        self.0.update_values(editor, completer);
    }

    fn reset_position(&mut self) {
        self.0.reset_position();
    }

    fn reload(&mut self, updated: bool, editor: &mut Editor, completer: &mut dyn Completer) {
        self.0.reload(updated, editor, completer);
    }

    fn update_working_details(
        &mut self,
        editor: &mut Editor,
        completer: &mut dyn Completer,
        painter: &Painter,
    ) {
        self.0.update_working_details(editor, completer, painter);
    }

    fn replace_in_buffer(&self, editor: &mut Editor) {
        self.0.replace_in_buffer(editor);
        step_inside_closing_quote(editor);
    }

    fn menu_required_lines(&self, terminal_columns: u16) -> u16 {
        self.0.menu_required_lines(terminal_columns)
    }

    fn menu_string(&self, available_lines: u16, use_ansi_coloring: bool) -> String {
        self.0.menu_string(available_lines, use_ansi_coloring)
    }

    fn min_rows(&self) -> u16 {
        self.0.min_rows()
    }

    fn get_values(&self) -> &[Suggestion] {
        self.0.get_values()
    }

    fn results_are_provisional(&self) -> bool {
        self.0.results_are_provisional()
    }

    fn is_awaiting_first_answer(&self) -> bool {
        self.0.is_awaiting_first_answer()
    }

    fn is_visible(&self) -> bool {
        self.0.is_visible()
    }

    fn set_cursor_pos(&mut self, pos: (u16, u16)) {
        self.0.set_cursor_pos(pos);
    }
}
