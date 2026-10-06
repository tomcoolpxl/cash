---
see: paste pr crlf
---
## Description

`column` is util-linux's. Without options it lays the input lines out in columns that
fit the output width, down the columns first (`-x` along the rows first), padded with
tabs or with `-S` spaces. `-t` makes a table: each line is split on blanks (or on the
characters of `-s`), and every column is padded to its widest cell, two spaces (`-o`)
between columns. `-N` names the columns, `-K` takes the names from the first line, `-R`
right-aligns, `-H` hides, `-O` reorders, `-l` limits the columns, `-d` drops the header,
`-L` keeps empty lines, `-m` fills the width, and `-J` prints the table as JSON.

Widths are display cells, so CJK text and accents line up. The output width is the
console's, else `COLUMNS`, else 80; `-c` sets it (`-c 0` is unlimited).

## Difference from util-linux

A CRLF line has its CR taken off before it is split and put back on its output line;
util-linux keeps it inside the last cell. A byte that is not UTF-8 is kept, one cell
wide, where util-linux prints `\xff`. A file that cannot be read gives status 1 even
when the table is printed. `-T`, `-W`, `-E`, `-C`, the tree options and the colour
scheme are not supported, and say so.

## Examples

```
ls | column                              # the names in columns
mount | column -t                        # aligned on blanks
column -t -s : /etc/passwd               # aligned on colons
column -t -N NAME,SIZE -R SIZE sizes.txt # named columns, sizes right-aligned
column -J -N name,size sizes.txt         # the same table as JSON
```
