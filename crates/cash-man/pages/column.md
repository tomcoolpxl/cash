---
name: column
summary: Lay a list out in columns, or align a table on a separator.
kind: Text and file tools
see: paste pr crlf
---

## Syntax

    column [OPTION]... [FILE]...

    column FILE            a list in as many columns as fit
    column -t FILE         a table, aligned on blanks
    column -t -s, FILE     a table, aligned on commas
    column -t -J -N a,b F  the table as JSON, its columns named

## Examples

A table aligned on blanks, and on commas:

    printf 'name age\nann 31\nbob 7\n' | column -t
    printf 'a,b\nccc,d\n' | column -t -s,

Named columns, the second right-aligned, with a separator of your own:

    printf 'ann 31\nbob 7\n' | column -t -N NAME,AGE -R AGE -o ' | '

A list in columns, down then across:

    seq 6 | column -c 20
    seq 6 | column -x -c 20

## How it works

`column` is util-linux's. Without options it lays the input lines out in columns that
fit the output width, down the columns first (`-x` along the rows first), padded with
tabs or with `-S` spaces. `-t` makes a table: each line is split on blanks (or on the
characters of `-s`), and every column is padded to its widest cell, two spaces (`-o`)
between columns. `-N` names the columns, `-K` takes the names from the first line, `-R`
right-aligns, `-H` hides, `-O` reorders, `-l` limits the columns, `-d` drops the header,
`-L` keeps empty lines, `-m` fills the width, and `-J` prints the table as JSON.

Widths are display cells, so CJK text and accents line up. The output width is the
console's, else `COLUMNS`, else 80; `-c` sets it (`-c 0` is unlimited).

## Options

    -t, --table                  A table: split each line into cells.
    -s, --separator=CHARS        Split on CHARS, not blanks.
    -o, --output-separator=STR   STR between columns: two spaces without.
    -N, --table-columns=NAMES    Name the columns, a comma between.
    -K, --table-column-names     Take the names from the first line.
    -R, --table-right=COLUMNS    Right-align these columns.
    -H, --table-hide=COLUMNS     Leave these columns out.
    -O, --table-order=COLUMNS    Print the columns in this order.
    -l, --table-columns-limit=N  At most N columns; the rest in the last.
    -d, --table-noheadings       No header line.
    -L, --keep-empty-lines       Keep empty lines.
    -J, --json                   The table as JSON; needs -N.
    -c, --output-width=WIDTH     The width to fill: 0 for no limit.
    -x, --fillrows               Fill rows before columns.
    -S, --use-spaces=N           Pad with spaces, at least N.

Exit status: 0, or 1 for a file that cannot be read.

## Different in cash

### Difference from util-linux

A CRLF line has its CR taken off before it is split and put back on its output line;
util-linux keeps it inside the last cell. A byte that is not UTF-8 is kept, one cell
wide, where util-linux prints `\xff`. A file that cannot be read gives status 1 even
when the table is printed. `-T`, `-W`, `-E`, `-C`, the tree options and the colour
scheme are not supported, and say so.
