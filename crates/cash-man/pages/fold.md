---
name: fold
summary: Wrap each line to a width.
kind: Bundled coreutils, from uutils
see: fmt cut
---

## Syntax

    fold [OPTION]... [FILE]...

    fold -w 40 FILE       lines of 40 columns at most
    fold -s -w 40 FILE    broken at spaces, words kept whole

## Examples

Cut at the width, or at the last space before it:

    echo "the quick brown fox jumps over the lazy dog" | fold -w 12
    echo "the quick brown fox jumps over the lazy dog" | fold -s -w 12

A long string in pieces:

    echo abcdefghijkl | fold -w 4

## How it works

`fold` breaks each line longer than the width, 80 without `-w`, into lines of that
width. `-s` breaks at the last space before the width instead, so words stay whole,
and a line with no space is cut where it must be. `-b` counts bytes instead of
columns. A tab counts to the next tab stop.

`fmt` fills paragraphs instead: it joins short lines as well as breaking long ones.

## Options

    -w, --width=WIDTH    The width: 80 without.
    -s, --spaces         Break at spaces.
    -b, --bytes          Count bytes, not columns.

Exit status: 0, or 1 for a file that cannot be read.
