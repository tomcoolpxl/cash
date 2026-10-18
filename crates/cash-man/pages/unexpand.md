---
name: unexpand
summary: Convert spaces to tabs.
kind: Bundled coreutils, from uutils
see: expand
---

## Syntax

    unexpand [OPTION]... [FILE]...

    unexpand FILE        leading spaces to tabs, stops every 8
    unexpand -a FILE     all runs of spaces that reach a stop
    unexpand -t 4 FILE   stops every 4

## Examples

Leading spaces made a tab, and spaces inside the line too with `-a`:

    printf '        a\n' | unexpand | od -c
    printf 'a       b\n' | unexpand -a | od -c

Stops every 4:

    printf '    a\n' | unexpand -t 4 | od -c

## How it works

`unexpand` writes its files, or standard input, with runs of spaces that reach a tab
stop replaced by tabs: only at the start of a line, or everywhere with `-a`. The stops
are every 8 columns, or as `-t` gives them, which also means `-a`; `--first-only` keeps
to the start of the line even then. `expand` does the reverse.

## Options

    -a, --all              Every run of spaces, not only leading ones.
    -t, --tabs=N           Stops every N columns; implies -a.
    -t, --tabs=LIST        Stops at the columns listed.
    --first-only           Only leading spaces, even with -t.

Exit status: 0, or 1 for a file that cannot be read.
