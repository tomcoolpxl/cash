---
name: pr
summary: Paginate or columnate files for printing.
kind: Bundled coreutils, from uutils
see: column fold nl
---

## Syntax

    pr [OPTION]... [FILE]...

    pr FILE              pages of 66 lines, a header on each
    pr -t -2 FILE        two columns, no header
    pr -m A B            two files side by side
    pr -t -s, -3 FILE    three columns apart by commas

## Examples

Numbers in columns, down and across:

    seq 9 | pr -t -3
    seq 9 | pr -t -3 -a

Two files side by side, and columns apart by a separator of your own:

    printf 'one\ntwo\n' > a.txt; printf 'uno\ndos\n' > b.txt
    pr -m -t a.txt b.txt
    printf 'the cat sat\non the mat\n' | pr -t -s, -2

Numbered lines, indented:

    printf 'alpha\nbeta\n' | pr -t -n -o 4

## How it works

`pr` lays a file out for printing: pages of 66 lines (`-l`), each with a header of the
date, the file's name and the page number, and blank lines at the foot; `-t` leaves
the header and the foot out, `-h` gives the header a title of your own. `-COLUMNS`
lays the lines out in that many columns, down each column first, `-a` across; `-m`
puts several files side by side, a column each. The columns share the page's width,
72 columns without `-w`, and are apart by white space, or by `-s CHAR` or `-S STRING`;
`-s` without `-w` leaves the columns as they are, unpadded and uncut. `-n` numbers the
lines, `-o` indents each line, `-d` double-spaces, and `+PAGE` starts at that page.

## Options

    -COLUMNS               That many columns.
    -a, --across           Fill the columns across, not down.
    -m, --merge            The files side by side.
    -t, --omit-header      No header and no foot.
    -T, --omit-pagination  No header, no foot, no page breaks.
    -h, --header=TITLE     The header's title: the file's name without.
    -l, --length=N         Pages of N lines: 66 without.
    -w, --width=N          Lines of N columns, with columns: 72 without.
    -W, --page-width=N     Lines of N columns, always.
    -s[CHAR]               Columns apart by CHAR: a tab without.
    -S[STRING]             Columns apart by STRING.
    -n[SEP[DIGITS]]        Number the lines.
    -o, --indent=N         Indent every line N columns.
    -d, --double-space     A blank line after each.
    -J, --join-lines       Whole lines, not cut to a column.
    -F, -f, --form-feed    Pages end with a form feed.
    +FIRST[:LAST]          Only these pages.

Exit status: 0, or 1 for a file that cannot be read.
