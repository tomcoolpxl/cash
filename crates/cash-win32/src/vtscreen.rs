//! A small terminal screen, for tests that compare what two shells leave on a ConPTY.
//!
//! ConPTY does not pass a program's output through: it renders the console it keeps and
//! sends that as VT sequences, with cursor jumps, erases and redraws. Two shells that show
//! the same thing send very different streams, so the tests replay the stream onto this
//! screen and compare the text on it. It implements what ConPTY emits: printing with
//! autowrap, cursor movement, erase in line and display, insert and delete of characters
//! and lines, scrolling and the scroll region. Colours, modes and titles are ignored.

/// A grid of characters with a cursor, fed with VT output by [`Screen::feed`].
pub struct Screen {
    parser: vte::Parser,
    grid: Grid,
}

struct Grid {
    rows: usize,
    cols: usize,
    cells: Vec<Vec<char>>,
    row: usize,
    col: usize,
    /// The cursor has written the last column; the next character wraps first.
    wrap_pending: bool,
    /// The scroll region, top and bottom rows inclusive.
    top: usize,
    bottom: usize,
    saved: (usize, usize),
}

impl Screen {
    /// A blank screen of `cols` by `rows`.
    #[must_use]
    pub fn new(cols: usize, rows: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            parser: vte::Parser::new(),
            grid: Grid {
                rows,
                cols,
                cells: vec![vec![' '; cols]; rows],
                row: 0,
                col: 0,
                wrap_pending: false,
                top: 0,
                bottom: rows - 1,
                saved: (0, 0),
            },
        }
    }

    /// Replays terminal output onto the screen.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.grid, bytes);
    }

    /// The screen's text: each row without trailing blanks, and without trailing blank
    /// rows.
    #[must_use]
    pub fn text(&self) -> String {
        let mut lines: Vec<String> = self
            .grid
            .cells
            .iter()
            .map(|row| row.iter().collect::<String>().trim_end().to_owned())
            .collect();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines.join("\n")
    }

    /// The cursor's row and column, from zero.
    #[must_use]
    pub const fn cursor(&self) -> (usize, usize) {
        (self.grid.row, self.grid.col)
    }
}

impl Grid {
    fn blank_row(&self) -> Vec<char> {
        vec![' '; self.cols]
    }

    fn line_feed(&mut self) {
        if self.row == self.bottom {
            self.scroll_up(1);
        } else if self.row + 1 < self.rows {
            self.row += 1;
        }
    }

    fn scroll_up(&mut self, n: usize) {
        for _ in 0..n.min(self.bottom + 1 - self.top) {
            self.cells.remove(self.top);
            self.cells.insert(self.bottom, self.blank_row());
        }
    }

    fn scroll_down(&mut self, n: usize) {
        for _ in 0..n.min(self.bottom + 1 - self.top) {
            self.cells.remove(self.bottom);
            self.cells.insert(self.top, self.blank_row());
        }
    }

    fn goto(&mut self, row: usize, col: usize) {
        self.row = row.min(self.rows - 1);
        self.col = col.min(self.cols - 1);
        self.wrap_pending = false;
    }

    fn erase(&mut self, row: usize, from: usize, to: usize) {
        if let Some(cells) = self.cells.get_mut(row) {
            for cell in cells.iter_mut().take(to).skip(from) {
                *cell = ' ';
            }
        }
    }
}

/// The first parameter, or `default` when it is absent or zero.
fn arg(params: &vte::Params, index: usize, default: usize) -> usize {
    params
        .iter()
        .nth(index)
        .and_then(|p| p.first().copied())
        .map_or(default, |n| if n == 0 { default } else { usize::from(n) })
}

impl vte::Perform for Grid {
    fn print(&mut self, c: char) {
        if self.wrap_pending {
            self.col = 0;
            self.line_feed();
            self.wrap_pending = false;
        }
        if let Some(cell) = self
            .cells
            .get_mut(self.row)
            .and_then(|r| r.get_mut(self.col))
        {
            *cell = c;
        }
        if self.col + 1 == self.cols {
            self.wrap_pending = true;
        } else {
            self.col += 1;
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => {
                self.col = 0;
                self.wrap_pending = false;
            }
            b'\n' | 0x0b | 0x0c => {
                self.line_feed();
                self.wrap_pending = false;
            }
            0x08 => {
                self.col = self.col.saturating_sub(1);
                self.wrap_pending = false;
            }
            b'\t' => self.col = ((self.col / 8 + 1) * 8).min(self.cols - 1),
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        // Private modes (`?25l`, `?2004h`) and the like change nothing on the screen.
        if !intermediates.is_empty() {
            return;
        }
        let n = arg(params, 0, 1);
        match action {
            'H' | 'f' => self.goto(n - 1, arg(params, 1, 1) - 1),
            'A' => self.goto(self.row.saturating_sub(n), self.col),
            'B' | 'e' => self.goto(self.row + n, self.col),
            'C' | 'a' => self.goto(self.row, self.col + n),
            'D' => self.goto(self.row, self.col.saturating_sub(n)),
            'E' => self.goto(self.row + n, 0),
            'F' => self.goto(self.row.saturating_sub(n), 0),
            'G' | '`' => self.goto(self.row, n - 1),
            'd' => self.goto(n - 1, self.col),
            'K' => match arg(params, 0, 0) {
                0 => self.erase(self.row, self.col, self.cols),
                1 => self.erase(self.row, 0, self.col + 1),
                _ => self.erase(self.row, 0, self.cols),
            },
            'J' => {
                let (row, col) = (self.row, self.col);
                match arg(params, 0, 0) {
                    0 => {
                        self.erase(row, col, self.cols);
                        for r in row + 1..self.rows {
                            self.erase(r, 0, self.cols);
                        }
                    }
                    1 => {
                        for r in 0..row {
                            self.erase(r, 0, self.cols);
                        }
                        self.erase(row, 0, col + 1);
                    }
                    _ => {
                        for r in 0..self.rows {
                            self.erase(r, 0, self.cols);
                        }
                    }
                }
            }
            'X' => self.erase(self.row, self.col, self.col + n),
            'P' => {
                let (col, cols) = (self.col, self.cols);
                if let Some(cells) = self.cells.get_mut(self.row) {
                    for _ in 0..n.min(cols - col) {
                        cells.remove(col);
                        cells.push(' ');
                    }
                }
            }
            '@' => {
                let (col, cols) = (self.col, self.cols);
                if let Some(cells) = self.cells.get_mut(self.row) {
                    for _ in 0..n.min(cols - col) {
                        cells.insert(col, ' ');
                        cells.pop();
                    }
                }
            }
            'L' | 'M' if (self.top..=self.bottom).contains(&self.row) => {
                let saved_top = self.top;
                self.top = self.row;
                if action == 'L' {
                    self.scroll_down(n);
                } else {
                    self.scroll_up(n);
                }
                self.top = saved_top;
            }
            'S' => self.scroll_up(n),
            'T' => self.scroll_down(n),
            'r' => {
                let top = arg(params, 0, 1) - 1;
                let bottom = arg(params, 1, self.rows).min(self.rows) - 1;
                if top < bottom {
                    self.top = top;
                    self.bottom = bottom;
                }
                self.goto(0, 0);
            }
            's' => self.saved = (self.row, self.col),
            'u' => self.goto(self.saved.0, self.saved.1),
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        if !intermediates.is_empty() {
            return;
        }
        match byte {
            b'7' => self.saved = (self.row, self.col),
            b'8' => self.goto(self.saved.0, self.saved.1),
            b'D' => self.line_feed(),
            b'E' => {
                self.col = 0;
                self.line_feed();
            }
            b'M' => {
                if self.row == self.top {
                    self.scroll_down(1);
                } else {
                    self.row = self.row.saturating_sub(1);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Screen;

    fn screen(input: &str) -> Screen {
        let mut screen = Screen::new(10, 4);
        screen.feed(input.as_bytes());
        screen
    }

    #[test]
    fn prints_wraps_and_scrolls() {
        assert_eq!(screen("abc\r\ndef").text(), "abc\ndef");
        assert_eq!(screen("0123456789ab").text(), "0123456789\nab");
        assert_eq!(screen("1\r\n2\r\n3\r\n4\r\n5").text(), "2\n3\n4\n5");
    }

    #[test]
    fn moves_the_cursor_and_erases() {
        assert_eq!(screen("hello\x1b[3Dxy").text(), "hexyo");
        assert_eq!(screen("hello\x1b[1;3H\x1b[K").text(), "he");
        assert_eq!(screen("a\r\nb\x1b[2J\x1b[Hc").text(), "c");
        assert_eq!(screen("abcdef\x1b[1;2H\x1b[2P").text(), "adef");
        assert_eq!(screen("abc\x1b[1;2H\x1b[@").text(), "a bc");
        assert_eq!(screen("ab\x1b]0;title\x07\x1b[?25lc").text(), "abc");
        assert_eq!(screen("x\x08y").cursor(), (0, 1));
    }
}
