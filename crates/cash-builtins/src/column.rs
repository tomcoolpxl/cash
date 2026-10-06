//! `column`, util-linux's: a list laid out in columns, or a table aligned on a separator.
//!
//! Checked against util-linux 2.42.3 (`crates/cash/tests/oracle`). The fill modes keep
//! its arithmetic: items are padded with tabs to the widest item rounded up to a tab
//! stop, or with `-S` spaces, into as many columns as the output width holds; an item
//! as wide as the output is printed one per line. The table mode keeps libsmartcols'
//! rules: a column is as wide as its widest cell or its header, columns are joined by
//! `-o` (two spaces), the last visible column is not padded unless it is right-aligned,
//! `-m` widens the columns to the output width, and `-J` prints libsmartcols' JSON with
//! its three-space indentation and lower-cased keys. Widths are display cells
//! (`unicode-width`), so CJK text and combining marks line up, and an `ESC [ … m`
//! sequence has no width. The output width is the console's when standard output is
//! one, else `COLUMNS`, else 80.
//!
//! Deliberate differences: a line ending in CRLF has its CR taken off before it is
//! split and put back on its output line (util-linux keeps the CR inside the last
//! cell), a byte that is not UTF-8 is kept as one cell of width 1 (util-linux prints it
//! as `\xff`), and a file that cannot be read gives status 1 even when a table is still
//! printed (util-linux's status is the table's, 0). `-T`, `-W`, `-E`, `-C`, the tree
//! options and the colour scheme are refused by name.

use std::io::{Read, Write};

use cash_core::{ExecutionResult, builtins};
use clap::Parser;
use unicode_width::UnicodeWidthChar as _;

/// Columnate lists.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct ColumnCommand {
    /// Options and files, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

const HINT: &str = "Try 'column --help' for more information.";

const USAGE: &str = "
Usage:
 column [options] [<file>...]

Columnate lists.

Options:
 -t, --table                      create a table
 -n, --table-name <name>          table name for JSON output
 -O, --table-order <columns>      specify order of output columns
 -N, --table-columns <names>      comma separated columns names
 -l, --table-columns-limit <num>  maximal number of input columns
 -d, --table-noheadings           don't print header
 -m, --table-maxout               fill all available space
 -K, --table-header-as-columns    use the first row as table header
 -H, --table-hide <columns>       don't print the columns
 -R, --table-right <columns>      right align text in these columns
 -L, --keep-empty-lines           don't ignore empty lines
 -J, --json                       use JSON output format for table

 -c, --output-width <width>       width of output in number of characters
 -o, --output-separator <string>  columns separator for table output
                                    (default is two spaces)
 -s, --input-separator, --separator <string>
                                    possible table delimiters
 -x, --fillrows                   fill rows before columns
 -S, --use-spaces <number>        minimal whitespaces between columns (no tabs)

 -h, --help                       display this help
 -V, --version                    display version

Not supported: -C, -E, -T, -W, --wrap-separator, --table-colorscheme, and the tree
options -r, -i and -p.
";

/// A tab stop, as the fill modes pad with tabs.
const TAB_CELLS: usize = 8;

// ---------------------------------------------------------------------------------
// Characters and widths

/// One unit of a line of bytes: a character, or one byte that is not UTF-8.
struct Span {
    start: usize,
    end: usize,
    /// `None` for a byte that is not UTF-8.
    ch: Option<char>,
}

/// `text` as its characters and stray bytes, in order.
fn spans(text: &[u8]) -> Vec<Span> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    for chunk in text.utf8_chunks() {
        for c in chunk.valid().chars() {
            let len = c.len_utf8();
            out.push(Span {
                start: at,
                end: at + len,
                ch: Some(c),
            });
            at += len;
        }
        for _ in chunk.invalid() {
            out.push(Span {
                start: at,
                end: at + 1,
                ch: None,
            });
            at += 1;
        }
    }
    out
}

/// Whether `c` ends a control sequence (`ESC [ … c`).
const fn ends_control_sequence(c: char) -> bool {
    matches!(c, '\u{40}'..='\u{7e}')
}

/// The display cells `text` takes: each character's width, one for a byte that is not
/// UTF-8, none for a control character, and none for a complete `ESC [ … m` sequence.
fn display_width(text: &[u8]) -> usize {
    let spans = spans(text);
    let mut width = 0;
    let mut i = 0;
    while let Some(span) = spans.get(i) {
        i += 1;
        let Some(c) = span.ch else {
            width += 1;
            continue;
        };
        if c == '\u{1b}' && spans.get(i).is_some_and(|next| next.ch == Some('[')) {
            // Parameters, then the final byte; anything else leaves the sequence
            // incomplete, and only the ESC itself has no width.
            let rest = spans.get(i + 1..).unwrap_or_default();
            let params = rest
                .iter()
                .take_while(|s| s.ch.is_some_and(|c| matches!(c, '\u{30}'..='\u{3f}')))
                .count();
            if rest
                .get(params)
                .is_some_and(|s| s.ch.is_some_and(ends_control_sequence))
            {
                i += params + 2;
            }
            continue;
        }
        width += c.width().unwrap_or(0);
    }
    width
}

/// Whether a line is empty: nothing but white space, as util-linux's `skip_space` sees it.
fn is_blank_line(line: &[u8]) -> bool {
    line.iter()
        .all(|b| matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
}

// ---------------------------------------------------------------------------------
// Splitting a line into cells

/// What separates the cells of a table line.
enum Separator {
    /// Runs of spaces and tabs, with none at the ends: the default.
    Blanks,
    /// Each of these characters, every one of them: `-s`.
    Chars(Vec<char>),
}

impl Separator {
    fn matches(&self, span: &Span) -> bool {
        match self {
            Self::Blanks => matches!(span.ch, Some(' ' | '\t')),
            Self::Chars(set) => span.ch.is_some_and(|c| set.contains(&c)),
        }
    }
}

/// `line` split into cells; with `limit`, the last cell is the rest of the line as it
/// was, separators included.
fn split_cells(line: &[u8], separator: &Separator, limit: Option<usize>) -> Vec<Vec<u8>> {
    let spans = spans(line);
    let mut cells: Vec<Vec<u8>> = Vec::new();
    let at_limit = |cells: &Vec<Vec<u8>>| limit.is_some_and(|limit| cells.len() + 1 == limit);
    let mut i = 0;
    match separator {
        Separator::Blanks => loop {
            while spans.get(i).is_some_and(|s| separator.matches(s)) {
                i += 1;
            }
            let Some(first) = spans.get(i) else { break };
            if at_limit(&cells) {
                cells.push(line.get(first.start..).unwrap_or_default().to_vec());
                break;
            }
            let end = spans
                .get(i..)
                .unwrap_or_default()
                .iter()
                .position(|s| separator.matches(s))
                .map_or(spans.len(), |n| i + n);
            let stop = spans.get(end).map_or(line.len(), |s| s.start);
            cells.push(line.get(first.start..stop).unwrap_or_default().to_vec());
            i = end;
        },
        Separator::Chars(_) => {
            let mut start = 0;
            loop {
                if at_limit(&cells) {
                    cells.push(line.get(start..).unwrap_or_default().to_vec());
                    break;
                }
                let next = spans
                    .get(i..)
                    .unwrap_or_default()
                    .iter()
                    .position(|s| separator.matches(s))
                    .map(|n| i + n);
                let Some(sep) = next.and_then(|n| spans.get(n)) else {
                    cells.push(line.get(start..).unwrap_or_default().to_vec());
                    break;
                };
                cells.push(line.get(start..sep.start).unwrap_or_default().to_vec());
                start = sep.end;
                i = next.unwrap_or(i) + 1;
            }
        }
    }
    cells
}

// ---------------------------------------------------------------------------------
// The fill modes

/// Pads from display column `chcnt` up to `endcol`: with spaces, or with tabs to the tab
/// stops that fit.
fn pad_to(out: &mut Vec<u8>, chcnt: &mut usize, endcol: usize, spaces: bool) {
    if spaces {
        while *chcnt < endcol {
            out.push(b' ');
            *chcnt += 1;
        }
    } else {
        loop {
            let next = (*chcnt + TAB_CELLS) & !(TAB_CELLS - 1);
            if next > endcol {
                break;
            }
            out.push(b'\t');
            *chcnt = next;
        }
    }
}

/// `entries` laid out in columns of `termwidth` display cells, down the columns first
/// or, with `fill_rows`, along the rows; `spaces` is the `-S` gap, else tabs pad. Each
/// output line ends in `eol`.
fn columnate(
    entries: &[Vec<u8>],
    termwidth: usize,
    spaces: Option<usize>,
    fill_rows: bool,
    eol: &[u8],
) -> Vec<u8> {
    let widths: Vec<usize> = entries.iter().map(|e| display_width(e)).collect();
    let mut out = Vec::new();
    let widest = widths.iter().copied().max().unwrap_or(0);
    if widest >= termwidth {
        for entry in entries {
            out.extend_from_slice(entry);
            out.extend_from_slice(eol);
        }
        return out;
    }
    let maxlength = spaces.map_or_else(
        || (widest + TAB_CELLS) & !(TAB_CELLS - 1),
        |gap| widest + gap,
    );
    let mut numcols = (termwidth / maxlength).max(1);
    if let Some(gap) = spaces {
        if termwidth % maxlength + gap >= maxlength {
            numcols += 1;
        }
    }
    let count = entries.len();
    if fill_rows {
        let mut chcnt = 0;
        let mut col = 0;
        let mut endcol = maxlength;
        for (i, (entry, width)) in entries.iter().zip(&widths).enumerate() {
            out.extend_from_slice(entry);
            chcnt += width;
            if i + 1 == count {
                break;
            }
            col += 1;
            if col == numcols {
                chcnt = 0;
                col = 0;
                endcol = maxlength;
                out.extend_from_slice(eol);
            } else {
                pad_to(&mut out, &mut chcnt, endcol, spaces.is_some());
                endcol += maxlength;
            }
        }
        if chcnt > 0 {
            out.extend_from_slice(eol);
        }
        return out;
    }
    let numrows = count.div_ceil(numcols);
    for row in 0..numrows {
        let mut endcol = maxlength;
        let mut chcnt = 0;
        let mut base = row;
        for _ in 0..numcols {
            if let (Some(entry), Some(width)) = (entries.get(base), widths.get(base)) {
                out.extend_from_slice(entry);
                chcnt += width;
            }
            base += numrows;
            if base >= count {
                break;
            }
            pad_to(&mut out, &mut chcnt, endcol, spaces.is_some());
            endcol += maxlength;
        }
        out.extend_from_slice(eol);
    }
    out
}

// ---------------------------------------------------------------------------------
// The table

/// A column of the table: its header name, if any, and how it is shown.
#[derive(Default)]
struct Column {
    name: Option<String>,
    right: bool,
    hidden: bool,
}

/// A row of the table; a row from an empty line has no cells.
struct Row {
    cells: Vec<Vec<u8>>,
    crlf: bool,
}

/// How the table is read and shown.
struct TableOptions {
    json: bool,
    name: Option<String>,
    /// `-N`, or the first line with `-K` once it has been read.
    colnames: Option<Vec<String>>,
    header_as_columns: bool,
    noheadings: bool,
    hide_unnamed: bool,
    maxout: bool,
    output_separator: String,
}

/// The table as it is read: libsmartcols' table, with columns made on demand.
struct Table {
    columns: Vec<Column>,
    rows: Vec<Row>,
    /// Set when the first line (or empty line) arrives, as `init_table` is: whether a
    /// header is shown is decided then, from the names known at that moment.
    initialised: bool,
    show_header: bool,
}

impl Table {
    const fn new() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            initialised: false,
            show_header: false,
        }
    }

    fn init(&mut self, options: &TableOptions) {
        if self.initialised {
            return;
        }
        self.initialised = true;
        if let Some(names) = &options.colnames {
            self.columns = names
                .iter()
                .map(|name| Column {
                    name: Some(name.clone()),
                    ..Column::default()
                })
                .collect();
            self.show_header = !options.noheadings;
        }
    }

    /// Adds a line's cells as a row, making columns for cells beyond the known ones.
    fn add_line(
        &mut self,
        cells: Vec<Vec<u8>>,
        crlf: bool,
        options: &mut TableOptions,
    ) -> Result<(), String> {
        if options.header_as_columns {
            options.header_as_columns = false;
            options.colnames = Some(
                cells
                    .into_iter()
                    .map(|cell| String::from_utf8_lossy(&cell).into_owned())
                    .collect(),
            );
            return Ok(());
        }
        self.init(options);
        while self.columns.len() < cells.len() {
            if options.json && !options.hide_unnamed {
                // util-linux counts the line being added once it holds a cell.
                let line = self.rows.len() + 1 + usize::from(!self.columns.is_empty());
                return Err(std::format!(
                    "line {line}: for JSON the name of the column {} is required",
                    self.columns.len() + 1
                ));
            }
            self.columns.push(Column {
                hidden: options.hide_unnamed,
                ..Column::default()
            });
        }
        self.rows.push(Row { cells, crlf });
        Ok(())
    }

    fn add_empty_line(&mut self, crlf: bool, options: &TableOptions) {
        self.init(options);
        self.rows.push(Row {
            cells: Vec::new(),
            crlf,
        });
    }

    /// The visible column `n` counted from the end, 0 being the last.
    fn visible_from_end(&self, n: usize) -> Option<usize> {
        self.columns
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, c)| !c.hidden)
            .nth(n)
            .map(|(i, _)| i)
    }

    /// The column `text` names: a number from 1, `-1` for the last visible one, or a
    /// header name.
    fn column_named(&self, text: &str) -> Result<usize, String> {
        let found = if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
            text.parse::<usize>()
                .ok()
                .and_then(|n| n.checked_sub(1))
                .filter(|n| *n < self.columns.len())
        } else if text == "-1" {
            self.visible_from_end(0)
        } else {
            self.columns
                .iter()
                .position(|c| c.name.as_deref() == Some(text))
        };
        found.ok_or_else(|| std::format!("undefined column name '{text}'"))
    }

    /// Applies `set` to the columns `list` names: `0` for all, `-` for the unnamed ones,
    /// `N-M` for a range (silently clipped), `-N` counting from the end, else by number
    /// or name.
    fn apply_to_list(&mut self, list: &str, set: fn(&mut Column)) -> Result<(), String> {
        if list == "0" {
            self.columns.iter_mut().for_each(set);
            return Ok(());
        }
        let mut unnamed = false;
        for item in list.split(',').filter(|item| !item.is_empty()) {
            if item == "-" {
                unnamed = true;
                continue;
            }
            if let Some((low, up)) = item.contains('-').then(|| parse_range(item)).flatten() {
                for k in low..=up {
                    let index = match k {
                        k if k < 0 => usize::try_from(-k - 1)
                            .ok()
                            .and_then(|n| self.visible_from_end(n)),
                        0 => None,
                        k => usize::try_from(k - 1)
                            .ok()
                            .filter(|i| *i < self.columns.len()),
                    };
                    if let Some(column) = index.and_then(|i| self.columns.get_mut(i)) {
                        set(column);
                    }
                }
                continue;
            }
            let index = self.column_named(item)?;
            if let Some(column) = self.columns.get_mut(index) {
                set(column);
            }
        }
        if unnamed {
            self.columns
                .iter_mut()
                .filter(|c| c.name.is_none())
                .for_each(set);
        }
        Ok(())
    }

    /// Moves the columns `order` names to the front, in that order.
    fn reorder(&mut self, order: &str) -> Result<(), String> {
        if self.columns.is_empty() {
            return Ok(());
        }
        let mut wanted: Vec<usize> = Vec::new();
        for item in order.split(',').filter(|item| !item.is_empty()) {
            let index = self.column_named(item)?;
            if !wanted.contains(&index) {
                wanted.push(index);
            }
            if wanted.len() >= self.columns.len() {
                break;
            }
        }
        let order: Vec<usize> = wanted
            .iter()
            .copied()
            .chain((0..self.columns.len()).filter(|i| !wanted.contains(i)))
            .collect();
        let mut columns: Vec<Option<Column>> = self.columns.drain(..).map(Some).collect();
        self.columns = order
            .iter()
            .filter_map(|&i| columns.get_mut(i).and_then(Option::take))
            .collect();
        // The rows follow their columns; a cell a short row never had stays empty.
        for row in &mut self.rows {
            let mut cells: Vec<Option<Vec<u8>>> = row.cells.drain(..).map(Some).collect();
            row.cells = order
                .iter()
                .map(|&i| cells.get_mut(i).and_then(Option::take).unwrap_or_default())
                .collect();
        }
        Ok(())
    }

    /// Applies `-H`, `-R` and `-O`, in util-linux's order: hide, right-align, reorder.
    fn modify(&mut self, options: &Options) -> Result<(), String> {
        if let Some(list) = &options.hide {
            self.apply_to_list(list, |c| c.hidden = true)?;
        }
        if let Some(list) = &options.right {
            self.apply_to_list(list, |c| c.right = true)?;
        }
        if let Some(order) = &options.order {
            self.reorder(order)?;
        }
        Ok(())
    }

    /// The indices of the visible columns, in output order.
    fn visible(&self) -> Vec<usize> {
        (0..self.columns.len())
            .filter(|i| self.columns.get(*i).is_some_and(|c| !c.hidden))
            .collect()
    }

    /// Each visible column's width: its widest cell, at least its header's width and 1,
    /// or 0 for a column with neither header nor data; with `-m`, widened from the last
    /// column backwards until the row fills `termwidth`.
    fn widths(&self, options: &TableOptions, termwidth: usize) -> Vec<usize> {
        let visible = self.visible();
        let mut widths: Vec<usize> = visible
            .iter()
            .map(|&i| {
                let column = self.columns.get(i);
                let name = column.and_then(|c| c.name.as_deref());
                let data_max = self
                    .rows
                    .iter()
                    .map(|row| row.cells.get(i).map_or(0, |cell| display_width(cell)))
                    .max()
                    .unwrap_or(0);
                let header = if self.show_header {
                    name.map(|n| display_width(n.as_bytes()))
                } else {
                    None
                };
                let width_min = header.unwrap_or(0).max(1);
                if data_max == 0 && header.is_none() {
                    0
                } else {
                    data_max.max(width_min)
                }
            })
            .collect();
        if options.maxout && termwidth > 0 && !widths.is_empty() {
            // libsmartcols sorts the columns by `average + 3 × deviation` of their
            // cells' widths first, and widens them one cell at a time from the largest.
            let mut order: Vec<usize> = (0..visible.len()).collect();
            let keys: Vec<f64> = visible.iter().map(|&i| self.width_spread(i)).collect();
            order.sort_by(|&a, &b| {
                let (ka, kb) = (keys.get(a).copied(), keys.get(b).copied());
                ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
            });
            let separator = display_width(options.output_separator.as_bytes());
            let mut total: usize = widths.iter().sum::<usize>() + separator * (widths.len() - 1);
            'enlarge: while total < termwidth {
                for &n in order.iter().rev() {
                    if let Some(width) = widths.get_mut(n) {
                        *width += 1;
                        total += 1;
                    }
                    if total == termwidth {
                        break 'enlarge;
                    }
                }
            }
        }
        widths
    }

    /// `average + 3 × standard deviation` of column `i`'s cell widths over the rows,
    /// libsmartcols' measure of how spread out a column is.
    fn width_spread(&self, i: usize) -> f64 {
        fn to_f64(n: usize) -> f64 {
            u32::try_from(n).map_or(f64::MAX, f64::from)
        }
        let widths: Vec<f64> = self
            .rows
            .iter()
            .map(|row| to_f64(row.cells.get(i).map_or(0, |cell| display_width(cell))))
            .collect();
        let count = to_f64(widths.len());
        if widths.is_empty() {
            return 0.0;
        }
        let average = widths.iter().sum::<f64>() / count;
        let deviation = if widths.len() > 1 {
            let squares: f64 = widths.iter().map(|w| (w - average) * (w - average)).sum();
            (squares / (count - 1.0)).sqrt()
        } else {
            0.0
        };
        3.0f64.mul_add(deviation, average)
    }

    /// One cell as libsmartcols prints it: padded to `width` unless it is the last
    /// column, left-padded when right-aligned, then the separator unless last.
    fn write_cell(
        out: &mut Vec<u8>,
        data: &[u8],
        column: &Column,
        mut width: usize,
        is_last: bool,
        options: &TableOptions,
    ) {
        let mut len = display_width(data);
        if is_last && len < width && !options.maxout && !column.right {
            width = len;
        }
        if !data.is_empty() {
            if column.right {
                out.resize(out.len() + width.saturating_sub(len), b' ');
                len = width;
            }
            out.extend_from_slice(data);
        }
        if !options.maxout && is_last {
            return;
        }
        out.resize(out.len() + width.saturating_sub(len), b' ');
        if !is_last {
            out.extend_from_slice(options.output_separator.as_bytes());
        }
    }

    /// The table as text, or nothing when it has no rows or no columns.
    fn render(&self, options: &TableOptions, termwidth: usize) -> Vec<u8> {
        let mut out = Vec::new();
        if self.rows.is_empty() || self.columns.is_empty() {
            return out;
        }
        let visible = self.visible();
        let widths = self.widths(options, termwidth);
        let eol = |crlf: bool| if crlf { &b"\r\n"[..] } else { &b"\n"[..] };
        let write_row = |out: &mut Vec<u8>, cell_of: &dyn Fn(usize) -> Vec<u8>, crlf: bool| {
            for (n, (&i, &width)) in visible.iter().zip(&widths).enumerate() {
                if let Some(column) = self.columns.get(i) {
                    let is_last = n + 1 == visible.len();
                    Self::write_cell(out, &cell_of(i), column, width, is_last, options);
                }
            }
            out.extend_from_slice(eol(crlf));
        };
        if self.show_header {
            let crlf = self.rows.first().is_some_and(|row| row.crlf);
            write_row(
                &mut out,
                &|i| {
                    self.columns
                        .get(i)
                        .and_then(|c| c.name.as_deref())
                        .map(|n| n.as_bytes().to_vec())
                        .unwrap_or_default()
                },
                crlf,
            );
        }
        for row in &self.rows {
            write_row(
                &mut out,
                &|i| row.cells.get(i).cloned().unwrap_or_default(),
                row.crlf,
            );
        }
        out
    }

    /// The table as libsmartcols' JSON, or nothing when it has no rows or no columns.
    fn render_json(&self, options: &TableOptions) -> Vec<u8> {
        let mut out = Vec::new();
        if self.rows.is_empty() || self.columns.is_empty() {
            return out;
        }
        let visible = self.visible();
        out.extend_from_slice(b"{\n   ");
        json_string(
            &mut out,
            options.name.as_deref().unwrap_or("table").as_bytes(),
            true,
        );
        out.extend_from_slice(b": [\n");
        for (n, row) in self.rows.iter().enumerate() {
            out.extend_from_slice(if n == 0 { b"      {\n" } else { b"},{\n" });
            for (k, &i) in visible.iter().enumerate() {
                if k > 0 {
                    out.extend_from_slice(b",\n");
                }
                out.extend_from_slice(b"         ");
                let name = self
                    .columns
                    .get(i)
                    .and_then(|c| c.name.as_deref())
                    .unwrap_or("");
                json_string(&mut out, name.as_bytes(), true);
                out.extend_from_slice(b": ");
                match row.cells.get(i) {
                    Some(cell) if !cell.is_empty() => json_string(&mut out, cell, false),
                    _ => out.extend_from_slice(b"null"),
                }
            }
            out.extend_from_slice(b"\n      ");
        }
        out.extend_from_slice(b"}\n   ]\n}\n");
        out
    }
}

/// `text` as a JSON string, escaped as libsmartcols does, lower-cased when it is a key.
fn json_string(out: &mut Vec<u8>, text: &[u8], key: bool) {
    out.push(b'"');
    for &b in text {
        match b {
            b'"' | b'\\' => {
                out.push(b'\\');
                out.push(b);
            }
            0x20.. => out.push(if key { b.to_ascii_lowercase() } else { b }),
            b'\x08' => out.extend_from_slice(b"\\b"),
            b'\t' => out.extend_from_slice(b"\\t"),
            b'\n' => out.extend_from_slice(b"\\n"),
            0x0c => out.extend_from_slice(b"\\f"),
            b'\r' => out.extend_from_slice(b"\\r"),
            _ => out.extend_from_slice(std::format!("\\u00{b:02x}").as_bytes()),
        }
    }
    out.push(b'"');
}

/// `N-M`, or a single signed number, as `strtol` and util-linux's range parser read it.
fn parse_range(text: &str) -> Option<(i64, i64)> {
    fn leading_int(text: &str) -> Option<(i64, &str)> {
        let (sign, rest) = match text.strip_prefix('-') {
            Some(rest) => (-1, rest),
            None => (1, text.strip_prefix('+').unwrap_or(text)),
        };
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits == 0 {
            return None;
        }
        let value: i64 = rest.get(..digits)?.parse().ok()?;
        Some((sign * value, rest.get(digits..).unwrap_or_default()))
    }
    let (low, rest) = leading_int(text)?;
    if rest.is_empty() {
        return Some((low, low));
    }
    let rest = rest.strip_prefix('-').or_else(|| rest.strip_prefix(':'))?;
    let (up, rest) = leading_int(rest)?;
    rest.is_empty().then_some((low, up))
}

/// A number as `strtou32_or_err` reads one: leading blanks and a sign allowed, nothing
/// after the digits, within `u32`.
fn parse_u32(text: &str, what: &str) -> Result<u32, String> {
    let trimmed = text.trim_start();
    let (negative, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(std::format!("{what}: '{text}'"));
    }
    match digits.parse::<u32>() {
        Ok(0) => Ok(0),
        Ok(v) if !negative => Ok(v),
        _ => Err(std::format!(
            "{what}: '{text}': Numerical result out of range"
        )),
    }
}

// ---------------------------------------------------------------------------------
// Options

/// Whether an option takes an argument.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Takes {
    Nothing,
    Required,
    Optional,
}

/// util-linux's long options, in its order (which the ambiguity message lists).
const LONG_OPTIONS: &[(&str, Takes, &str)] = &[
    ("columns", Takes::Required, "c"),
    ("color", Takes::Optional, "color"),
    ("fillrows", Takes::Nothing, "x"),
    ("help", Takes::Nothing, "h"),
    ("input-separator", Takes::Required, "s"),
    ("json", Takes::Nothing, "J"),
    ("keep-empty-lines", Takes::Nothing, "L"),
    ("output-separator", Takes::Required, "o"),
    ("output-width", Takes::Required, "c"),
    ("separator", Takes::Required, "s"),
    ("table", Takes::Nothing, "t"),
    ("table-colorscheme", Takes::Required, "colorscheme"),
    ("table-columns", Takes::Required, "N"),
    ("table-column", Takes::Required, "C"),
    ("table-columns-limit", Takes::Required, "l"),
    ("table-hide", Takes::Required, "H"),
    ("table-name", Takes::Required, "n"),
    ("table-maxout", Takes::Nothing, "m"),
    ("table-noextreme", Takes::Required, "E"),
    ("table-noheadings", Takes::Nothing, "d"),
    ("table-order", Takes::Required, "O"),
    ("table-right", Takes::Required, "R"),
    ("table-truncate", Takes::Required, "T"),
    ("table-wrap", Takes::Required, "W"),
    ("table-empty-lines", Takes::Nothing, "L"),
    ("table-header-repeat", Takes::Nothing, "e"),
    ("table-header-as-columns", Takes::Nothing, "K"),
    ("tree", Takes::Required, "r"),
    ("tree-id", Takes::Required, "i"),
    ("tree-parent", Takes::Required, "p"),
    ("use-spaces", Takes::Required, "S"),
    ("version", Takes::Nothing, "V"),
    ("wrap-separator", Takes::Required, "wrap-separator"),
];

/// util-linux's short options.
const SHORT_OPTIONS: &str = "C:c:dE:eH:hi:JKl:LN:n:mO:o:p:R:r:S:s:T:tVW:x";

/// Options that cannot be combined, each with the long name its message uses.
const EXCLUSIVE: &[&[(&str, &str)]] = &[
    &[
        ("C", "table-column"),
        ("K", "table-header-as-columns"),
        ("N", "table-columns"),
    ],
    &[("J", "json"), ("x", "fillrows")],
    &[("t", "table"), ("x", "fillrows")],
];

/// How the input is laid out.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    FillColumns,
    FillRows,
    Table,
}

/// Everything the command line said.
struct Options {
    mode: Mode,
    /// `-c`: `Some(0)` is unlimited; `None` asks the console or `COLUMNS`.
    termwidth: Option<u32>,
    spaces: Option<usize>,
    keep_empty_lines: bool,
    separator: Separator,
    limit: Option<usize>,
    order: Option<String>,
    right: Option<String>,
    hide: Option<String>,
    table: TableOptions,
    files: Vec<String>,
}

/// What parsing the command line ends in, other than options to act on.
enum Early {
    Help,
    Version,
    /// A message for standard error (and the hint, when `hint`), status 1.
    Error {
        message: String,
        hint: bool,
    },
}

impl Options {
    fn new() -> Self {
        Self {
            mode: Mode::FillColumns,
            termwidth: None,
            spaces: None,
            keep_empty_lines: false,
            separator: Separator::Blanks,
            limit: None,
            order: None,
            right: None,
            hide: None,
            table: TableOptions {
                json: false,
                name: None,
                colnames: None,
                header_as_columns: false,
                noheadings: false,
                hide_unnamed: false,
                maxout: false,
                output_separator: "  ".to_owned(),
            },
            files: Vec::new(),
        }
    }

    /// Applies one option; `shown` is how it was typed, for a refusal.
    fn apply(
        &mut self,
        id: &str,
        value: Option<String>,
        shown: &str,
        seen: &mut Vec<String>,
    ) -> Result<(), Early> {
        let fail = |message: String| Early::Error {
            message,
            hint: false,
        };
        for group in EXCLUSIVE {
            if let Some((_, this)) = group.iter().find(|(c, _)| *c == id) {
                if let Some((_, other)) = group
                    .iter()
                    .find(|(c, _)| *c != id && seen.iter().any(|s| s == c))
                {
                    return Err(fail(std::format!(
                        "options --{other} and --{this} cannot be combined"
                    )));
                }
            }
        }
        seen.push(id.to_owned());
        let value = value.unwrap_or_default();
        match id {
            "h" => return Err(Early::Help),
            "V" => return Err(Early::Version),
            "t" => self.mode = Mode::Table,
            "x" => self.mode = Mode::FillRows,
            "J" => {
                self.table.json = true;
                self.mode = Mode::Table;
            }
            "K" => {
                self.table.header_as_columns = true;
                self.mode = Mode::Table;
            }
            "c" => {
                self.termwidth = Some(if value == "unlimited" {
                    0
                } else {
                    parse_u32(&value, "invalid columns argument").map_err(fail)?
                });
            }
            "d" => self.table.noheadings = true,
            "e" => {}
            "H" => {
                self.table.hide_unnamed =
                    value == "-" || (value.contains(',') && value.split(',').any(|v| v == "-"));
                self.hide = Some(value);
            }
            "L" => self.keep_empty_lines = true,
            "l" => {
                let limit = parse_u32(&value, "invalid columns limit argument").map_err(fail)?;
                if limit == 0 {
                    return Err(fail("columns limit must be greater than zero".to_owned()));
                }
                self.limit = usize::try_from(limit).ok();
            }
            "N" => {
                self.table.colnames = Some(
                    value
                        .split(',')
                        .filter(|n| !n.is_empty())
                        .map(str::to_owned)
                        .collect(),
                );
            }
            "n" => self.table.name = Some(value),
            "m" => self.table.maxout = true,
            "O" => self.order = Some(value),
            "o" => self.table.output_separator = value,
            "R" => self.right = Some(value),
            "S" => {
                let gap = parse_u32(&value, "invalid spaces argument").map_err(fail)?;
                self.spaces = Some(usize::try_from(gap).unwrap_or(usize::MAX));
            }
            "s" => self.separator = Separator::Chars(value.chars().collect()),
            "color" => {
                if !matches!(value.as_str(), "" | "auto" | "always" | "never") {
                    return Err(fail(std::format!("unsupported color mode: {value}")));
                }
            }
            _ => return Err(fail(std::format!("{shown} is not supported"))),
        }
        Ok(())
    }

    /// Parses the command line as `getopt_long` does: options and files in any order,
    /// clusters, `--name=value`, unique prefixes, `--` ending the options.
    fn parse(args: &[String]) -> Result<Self, Early> {
        let mut options = Self::new();
        let mut seen = Vec::new();
        let mut index = 0;
        while let Some(arg) = args.get(index) {
            index += 1;
            if arg == "--" {
                options.files.extend(args.iter().skip(index).cloned());
                break;
            }
            if let Some(text) = arg.strip_prefix("--") {
                let (id, value, shown) = long_option(text, args, &mut index)?;
                options.apply(id, value, &shown, &mut seen)?;
            } else if arg.len() > 1 && arg.starts_with('-') {
                let body = arg.get(1..).unwrap_or_default();
                options.short_cluster(body, args, &mut index, &mut seen)?;
            } else {
                options.files.push(arg.clone());
            }
        }
        Ok(options)
    }

    /// Applies a cluster of short options (`-ts,`); the first that takes an argument
    /// takes the rest of the cluster, or the next word.
    fn short_cluster(
        &mut self,
        body: &str,
        args: &[String],
        index: &mut usize,
        seen: &mut Vec<String>,
    ) -> Result<(), Early> {
        for (at, c) in body.char_indices() {
            let takes = match SHORT_OPTIONS.find(c) {
                Some(pos) if c != ':' => {
                    if SHORT_OPTIONS
                        .get(pos + 1..)
                        .is_some_and(|r| r.starts_with(':'))
                    {
                        Takes::Required
                    } else {
                        Takes::Nothing
                    }
                }
                _ => return Err(hinted(std::format!("invalid option -- '{c}'"))),
            };
            let id = c.to_string();
            if takes == Takes::Nothing {
                self.apply(&id, None, &std::format!("-{c}"), seen)?;
                continue;
            }
            let rest = body.get(at + c.len_utf8()..).unwrap_or_default();
            let value = if rest.is_empty() {
                let Some(value) = args.get(*index) else {
                    return Err(hinted(std::format!("option requires an argument -- '{c}'")));
                };
                *index += 1;
                value.clone()
            } else {
                rest.to_owned()
            };
            self.apply(&id, Some(value), &std::format!("-{c}"), seen)?;
            break;
        }
        Ok(())
    }

    /// What `--table-*` options need: the table mode; and what `-J` needs: names.
    fn check(&self) -> Result<(), String> {
        let table_only = self.order.is_some()
            || self.table.name.is_some()
            || self.hide.is_some()
            || self.right.is_some()
            || self.table.colnames.is_some();
        if self.mode != Mode::Table && table_only {
            return Err("option --table required for all --table-*".to_owned());
        }
        if self.table.json && self.table.colnames.is_none() && !self.table.header_as_columns {
            return Err("option --table-columns or --table-column required for --json".to_owned());
        }
        Ok(())
    }
}

/// An error that gets the `--help` hint after it.
const fn hinted(message: String) -> Early {
    Early::Error {
        message,
        hint: true,
    }
}

/// A long option `text` (after its `--`): its id, its value (taken from `args` at
/// `index` when it needs one), and how to name it.
fn long_option(
    text: &str,
    args: &[String],
    index: &mut usize,
) -> Result<(&'static str, Option<String>, String), Early> {
    let (name, inline) = text
        .split_once('=')
        .map_or((text, None), |(n, v)| (n, Some(v.to_owned())));
    let exact = LONG_OPTIONS.iter().find(|(n, _, _)| *n == name).copied();
    let candidates: Vec<(&str, Takes, &str)> = LONG_OPTIONS
        .iter()
        .filter(|(n, _, _)| n.starts_with(name))
        .copied()
        .collect();
    let found = exact.or(match candidates.as_slice() {
        [one] => Some(*one),
        _ => None,
    });
    let Some((full, takes, id)) = found else {
        if candidates.is_empty() {
            return Err(hinted(std::format!("unrecognized option '--{text}'")));
        }
        let listed: Vec<String> = candidates
            .iter()
            .map(|(n, _, _)| std::format!("'--{n}'"))
            .collect();
        return Err(hinted(std::format!(
            "option '--{text}' is ambiguous; possibilities: {}",
            listed.join(" ")
        )));
    };
    let value = match (takes, inline) {
        (Takes::Nothing, Some(_)) => {
            return Err(hinted(std::format!(
                "option '--{full}' doesn't allow an argument"
            )));
        }
        (Takes::Nothing | Takes::Optional, None) => None,
        (_, Some(value)) => Some(value),
        (Takes::Required, None) => {
            let Some(value) = args.get(*index) else {
                return Err(hinted(std::format!(
                    "option '--{full}' requires an argument"
                )));
            };
            *index += 1;
            Some(value.clone())
        }
    };
    Ok((id, value, std::format!("--{full}")))
}

/// The output width: `-c`, else the console's when standard output is one, else
/// `COLUMNS`, else 80.
fn output_width<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    requested: Option<u32>,
) -> usize {
    if let Some(width) = requested {
        return usize::try_from(width).unwrap_or(usize::MAX);
    }
    let terminal = context
        .try_fd(cash_core::openfiles::OpenFiles::STDOUT_FD)
        .is_some_and(|fd| fd.is_terminal());
    terminal
        .then(|| crossterm::terminal::size().ok())
        .flatten()
        .map(|(columns, _)| usize::from(columns))
        .or_else(|| {
            context
                .shell
                .env_str("COLUMNS")
                .and_then(|value| positive_number(&value))
        })
        .unwrap_or(80)
}

/// The inputs: standard input, or each file that can be read, a missing one reported;
/// with how many could not be read.
fn read_inputs<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    files: &[String],
) -> Result<(Vec<Vec<u8>>, usize), cash_core::Error> {
    let mut inputs = Vec::new();
    let mut failures = 0;
    if files.is_empty() {
        let mut input = Vec::new();
        context.stdin().read_to_end(&mut input)?;
        inputs.push(input);
        return Ok((inputs, 0));
    }
    for file in files {
        let path = context.shell.absolute_path(std::path::Path::new(file));
        match std::fs::read(&path) {
            Ok(input) => inputs.push(input),
            Err(error) => {
                let reason = cash_core::error::os_error_text(&error);
                writeln!(context.stderr(), "column: {file}: {reason}")?;
                failures += 1;
            }
        }
    }
    Ok((inputs, failures))
}

/// A positive number as `COLUMNS` carries one, else nothing.
fn positive_number(text: &str) -> Option<usize> {
    let text = text.trim_start();
    let text = text.strip_prefix('+').unwrap_or(text);
    text.parse::<u32>()
        .ok()
        .filter(|n| *n > 0)
        .and_then(|n| usize::try_from(n).ok())
}

/// Reads `input` line by line into the fill entries or the table.
struct Reader<'a> {
    options: &'a mut Options,
    entries: Vec<Vec<u8>>,
    table: Table,
    /// Whether every line so far ended in CRLF, for the fill modes' output lines.
    all_crlf: bool,
    lines: usize,
}

impl Reader<'_> {
    fn read(&mut self, input: &[u8]) -> Result<(), String> {
        let mut lines = input.split(|&b| b == b'\n').peekable();
        while let Some(line) = lines.next() {
            if lines.peek().is_none() && line.is_empty() {
                break;
            }
            self.lines += 1;
            let (line, crlf) = match line.split_last() {
                Some((b'\r', body)) => (body, true),
                _ => (line, false),
            };
            self.all_crlf &= crlf;
            if is_blank_line(line) {
                if self.options.keep_empty_lines {
                    if self.options.mode == Mode::Table {
                        self.table.add_empty_line(crlf, &self.options.table);
                    } else {
                        self.entries.push(Vec::new());
                    }
                }
                continue;
            }
            if self.options.mode == Mode::Table {
                let cells = split_cells(line, &self.options.separator, self.options.limit);
                self.table.add_line(cells, crlf, &mut self.options.table)?;
            } else {
                self.entries.push(line.to_vec());
            }
        }
        Ok(())
    }
}

impl builtins::Command for ColumnCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut options = match Options::parse(&self.args) {
            Ok(options) => options,
            Err(Early::Help) => {
                write!(context.stdout(), "{USAGE}")?;
                return Ok(ExecutionResult::success());
            }
            Err(Early::Version) => {
                writeln!(
                    context.stdout(),
                    "column (cash): util-linux 2.42.3's options"
                )?;
                return Ok(ExecutionResult::success());
            }
            Err(Early::Error { message, hint }) => {
                writeln!(context.stderr(), "column: {message}")?;
                if hint {
                    writeln!(context.stderr(), "{HINT}")?;
                }
                return Ok(ExecutionResult::general_error());
            }
        };

        if let Err(message) = options.check() {
            writeln!(context.stderr(), "column: {message}")?;
            return Ok(ExecutionResult::general_error());
        }
        let termwidth = output_width(&context, options.termwidth);
        let files = std::mem::take(&mut options.files);
        let (inputs, failures) = read_inputs(&context, &files)?;
        let mut reader = Reader {
            options: &mut options,
            entries: Vec::new(),
            table: Table::new(),
            all_crlf: true,
            lines: 0,
        };
        let read = inputs.iter().try_for_each(|input| reader.read(input));
        let Reader {
            entries,
            mut table,
            all_crlf,
            lines,
            ..
        } = reader;
        let modified = read.and_then(|()| match options.mode {
            Mode::Table => table.modify(&options),
            Mode::FillColumns | Mode::FillRows => Ok(()),
        });
        if let Err(message) = modified {
            writeln!(context.stderr(), "column: {message}")?;
            return Ok(ExecutionResult::general_error());
        }
        let status = if failures > 0 {
            ExecutionResult::general_error()
        } else {
            ExecutionResult::success()
        };
        let output = match options.mode {
            Mode::Table => {
                if options.table.json {
                    table.render_json(&options.table)
                } else {
                    table.render(&options.table, termwidth)
                }
            }
            Mode::FillColumns | Mode::FillRows => {
                if entries.is_empty() {
                    return Ok(status);
                }
                let eol: &[u8] = if all_crlf && lines > 0 {
                    b"\r\n"
                } else {
                    b"\n"
                };
                columnate(
                    &entries,
                    termwidth,
                    options.spaces,
                    options.mode == Mode::FillRows,
                    eol,
                )
            }
        };
        context.stdout().write_all(&output)?;
        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Column, Row, Separator, Table, TableOptions, columnate, display_width, json_string,
        parse_range, parse_u32, split_cells,
    };

    fn cells(line: &str, separator: &Separator, limit: Option<usize>) -> Vec<String> {
        split_cells(line.as_bytes(), separator, limit)
            .into_iter()
            .map(|c| String::from_utf8_lossy(&c).into_owned())
            .collect()
    }

    #[test]
    fn widths_are_display_cells() {
        assert_eq!(display_width(b"abc"), 3);
        assert_eq!(display_width("日本".as_bytes()), 4);
        assert_eq!(display_width("e\u{301}".as_bytes()), 1);
        assert_eq!(display_width("\u{1F468}\u{200D}\u{1F469}".as_bytes()), 4);
        assert_eq!(display_width(b"a\x01b\x7f"), 2);
        assert_eq!(display_width(b"a\x1b[1;31mb"), 2);
        assert_eq!(display_width(b"a\x1b[1 c"), 5);
        assert_eq!(display_width(b"a\x1bMb"), 3);
        assert_eq!(display_width(b"a\xffb"), 3);
        assert_eq!(display_width(b""), 0);
    }

    #[test]
    fn blanks_split_greedily_and_separators_do_not() {
        assert_eq!(
            cells("  a   bb\t\tccc  ", &Separator::Blanks, None),
            ["a", "bb", "ccc"]
        );
        let comma = Separator::Chars(vec![',']);
        assert_eq!(cells("a,,ccc", &comma, None), ["a", "", "ccc"]);
        assert_eq!(cells("a,b,", &comma, None), ["a", "b", ""]);
        assert_eq!(cells(",a", &comma, None), ["", "a"]);
        assert_eq!(cells("abc", &comma, None), ["abc"]);
        let set = Separator::Chars(vec![',', '→']);
        assert_eq!(cells("a→b,c", &set, None), ["a", "b", "c"]);
    }

    #[test]
    fn a_limit_keeps_the_rest_of_the_line() {
        assert_eq!(
            cells("a   bb  ccc dd", &Separator::Blanks, Some(2)),
            ["a", "bb  ccc dd"]
        );
        let comma = Separator::Chars(vec![',']);
        assert_eq!(cells("a,,ccc,dd", &comma, Some(2)), ["a", ",ccc,dd"]);
        assert_eq!(cells("a b c", &Separator::Blanks, Some(1)), ["a b c"]);
    }

    fn fill(items: &[&str], width: usize, spaces: Option<usize>, rows: bool) -> String {
        let entries: Vec<Vec<u8>> = items.iter().map(|i| i.as_bytes().to_vec()).collect();
        String::from_utf8_lossy(&columnate(&entries, width, spaces, rows, b"\n")).into_owned()
    }

    #[test]
    fn fill_pads_with_tabs_to_the_rounded_width() {
        let items = ["a", "bb", "ccc", "dddd", "eeeee", "ffffff"];
        assert_eq!(
            fill(&items, 20, None, false),
            "a\tdddd\nbb\teeeee\nccc\tffffff\n"
        );
        assert_eq!(
            fill(&items, 20, None, true),
            "a\tbb\nccc\tdddd\neeeee\tffffff\n"
        );
        // An item of 8 cells rounds up to 16, so 32 columns hold two.
        assert_eq!(
            fill(&["aaaaaaaa", "b", "c"], 32, None, false),
            "aaaaaaaa\tc\nb\n"
        );
        // As wide as the output: one per line.
        assert_eq!(
            fill(&["aaaaaaaaaaaa", "b"], 10, None, false),
            "aaaaaaaaaaaa\nb\n"
        );
        assert_eq!(fill(&["a", "b"], 0, None, false), "a\nb\n");
    }

    #[test]
    fn fill_with_spaces_counts_the_gap() {
        assert_eq!(
            fill(&["a", "bb", "ccc"], 20, Some(1), false),
            "a   bb  ccc\n"
        );
        assert_eq!(fill(&["aaaa", "bbbb"], 5, Some(2), false), "aaaa  bbbb\n");
        assert_eq!(
            fill(&["aaa", "bbb", "ccc", "ddd", "eee"], 15, Some(2), true),
            "aaa  bbb  ccc\nddd  eee\n"
        );
        assert_eq!(
            fill(&["日本", "ab", "c", "d"], 16, Some(2), false),
            "日本  c\nab    d\n"
        );
    }

    fn options() -> TableOptions {
        TableOptions {
            json: false,
            name: None,
            colnames: None,
            header_as_columns: false,
            noheadings: false,
            hide_unnamed: false,
            maxout: false,
            output_separator: "  ".to_owned(),
        }
    }

    fn build(lines: &[&str], mut opts: TableOptions) -> (Table, TableOptions) {
        let mut table = Table::new();
        for line in lines {
            let cells = split_cells(line.as_bytes(), &Separator::Blanks, None);
            assert_eq!(table.add_line(cells, false, &mut opts), Ok(()));
        }
        (table, opts)
    }

    fn text(table: &Table, opts: &TableOptions) -> String {
        String::from_utf8_lossy(&table.render(opts, 80)).into_owned()
    }

    #[test]
    fn table_pads_all_but_the_last_column() {
        let (table, opts) = build(&["a bb ccc", "dddd e f"], options());
        assert_eq!(text(&table, &opts), "a     bb  ccc\ndddd  e   f\n");
        let (table, opts) = build(&["a bb ccc", "d"], options());
        assert_eq!(text(&table, &opts), "a  bb  ccc\nd      \n");
    }

    #[test]
    fn headers_right_alignment_and_hiding() {
        let mut opts = options();
        opts.colnames = Some(vec!["X".to_owned(), "LONGNAME".to_owned()]);
        let (mut table, opts) = build(&["aaa b"], opts);
        assert_eq!(text(&table, &opts), "X    LONGNAME\naaa  b\n");
        assert_eq!(table.apply_to_list("2", |c| c.right = true), Ok(()));
        assert_eq!(text(&table, &opts), "X    LONGNAME\naaa         b\n");
        assert_eq!(table.apply_to_list("X", |c| c.hidden = true), Ok(()));
        assert_eq!(text(&table, &opts), "LONGNAME\n       b\n");
        assert_eq!(
            table.apply_to_list("nope", |c| c.right = true),
            Err("undefined column name 'nope'".to_owned())
        );
    }

    #[test]
    fn maxout_widens_from_the_last_column() {
        let mut opts = options();
        opts.maxout = true;
        let (table, opts) = build(&["a b c", "ccc d e"], opts);
        let out = String::from_utf8_lossy(&table.render(&opts, 20)).into_owned();
        // The first column's widths vary most, so it grows first: 7, 4 and 5 cells.
        assert_eq!(out, "a        b     c    \nccc      d     e    \n");
    }

    #[test]
    fn reorder_moves_the_named_columns_first() {
        let (mut table, opts) = build(&["a bb c", "ccc d e"], options());
        assert_eq!(table.reorder("3,1"), Ok(()));
        assert_eq!(text(&table, &opts), "c  a    bb\ne  ccc  d\n");
    }

    #[test]
    fn json_escapes_as_libsmartcols_does() {
        let mut out = Vec::new();
        json_string(&mut out, b"x\"y\\z\t\x01", false);
        assert_eq!(out, b"\"x\\\"y\\\\z\\t\\u0001\"");
        let mut out = Vec::new();
        json_string(&mut out, "NAME é".as_bytes(), true);
        assert_eq!(String::from_utf8_lossy(&out), "\"name é\"");
        let mut opts = options();
        opts.json = true;
        opts.colnames = Some(vec!["a".to_owned(), "b".to_owned()]);
        let (table, opts) = build(&["x", "y z"], opts);
        let json = String::from_utf8_lossy(&table.render_json(&opts)).into_owned();
        assert_eq!(
            json,
            "{\n   \"table\": [\n      {\n         \"a\": \"x\",\n         \"b\": null\n      },{\n         \"a\": \"y\",\n         \"b\": \"z\"\n      }\n   ]\n}\n"
        );
    }

    #[test]
    fn an_empty_row_and_an_unnamed_empty_column() {
        let mut table = Table::new();
        let opts = options();
        table.columns.push(Column::default());
        table.columns.push(Column::default());
        table.columns.push(Column::default());
        table.rows.push(Row {
            cells: vec![b"a".to_vec(), b"b".to_vec()],
            crlf: false,
        });
        table.rows.push(Row {
            cells: Vec::new(),
            crlf: true,
        });
        // The third column has neither header nor data: width 0.
        assert_eq!(text(&table, &opts), "a  b  \n      \r\n");
    }

    #[test]
    fn ranges_and_numbers_parse_as_strtol_does() {
        assert_eq!(parse_range("2-3"), Some((2, 3)));
        assert_eq!(parse_range("-1"), Some((-1, -1)));
        assert_eq!(parse_range("2-"), None);
        assert_eq!(parse_range("x-1"), None);
        assert_eq!(parse_u32(" 20", "bad"), Ok(20));
        assert_eq!(parse_u32("+20", "bad"), Ok(20));
        assert_eq!(parse_u32("20 ", "bad"), Err("bad: '20 '".to_owned()));
        assert_eq!(parse_u32("", "bad"), Err("bad: ''".to_owned()));
        assert_eq!(
            parse_u32("-5", "bad"),
            Err("bad: '-5': Numerical result out of range".to_owned())
        );
        assert_eq!(
            parse_u32("4294967296", "bad"),
            Err("bad: '4294967296': Numerical result out of range".to_owned())
        );
    }
}
