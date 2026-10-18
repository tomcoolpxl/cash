---
name: expand
summary: Convert tabs to spaces.
kind: Bundled coreutils, from uutils
see: unexpand column
---

## Syntax

    expand [OPTION]... [FILE]...

    expand FILE          tabs to spaces, stops every 8 columns
    expand -t 4 FILE     stops every 4
    expand -i FILE       only the tabs that indent

## Examples

Tabs made spaces, to the default stops and to stops every 4:

    printf 'a\tb\tc\n' | expand | od -c
    printf 'a\tb\tc\n' | expand -t 4

Stops of your own, and only leading tabs:

    printf 'a\tb\tc\n' | expand -t 2,6
    printf '\tx\ty\n' | expand -i | od -c

## How it works

`expand` writes its files, or standard input, with each tab replaced by the spaces up
to the next tab stop: every 8 columns, or every N with `-t N`. `-t` with a list,
`-t 2,6`, puts the stops at those columns, and after the last every 1. `-i` replaces only
the tabs at the start of a line. `unexpand` does the reverse.

## Options

    -t, --tabs=N           Stops every N columns.
    -t, --tabs=LIST        Stops at the columns listed, a comma between.
    -i, --initial          Only the tabs before the first other character.

Exit status: 0, or 1 for a file that cannot be read.
