---
name: wc
summary: Count lines, words and bytes.
kind: Bundled coreutils, from uutils
see: cat grep uniq
---

## Syntax

    wc [OPTION]... [FILE]...

    wc FILE           lines, words and bytes
    wc -l FILE        lines only
    wc -w FILE        words only
    wc -c FILE        bytes only
    COMMAND | wc -l   how many lines a command prints

## Examples

Lines, words and bytes of a file:

    wc notes.txt

Lines only, the count most often wanted:

    wc -l app.log

How many lines a command prints:

    seq 42 | wc -l

The number alone, without the file's name, from standard input:

    wc -l < app.log

Several files, with a total:

    wc -l notes.txt pairs.txt

## How it works

`wc` counts the newlines, words and bytes in each FILE, or in its standard input, and
writes them in that order, then the file's name. A word is a run of characters
between blanks or newlines. With more than one FILE a line of totals comes last.

The options choose what is counted; they are always written in the order lines,
words, characters, bytes, longest line. Read from standard input (`< file`, or a
pipe), `wc` writes no name, which is what a script that wants the number needs.

`wc -l` counts newlines: a last line without one is not counted.

## Options

    -l, --lines           Count newlines.
    -w, --words           Count words.
    -c, --bytes           Count bytes.
    -m, --chars           Count characters, as UTF-8 decodes them.
    -L, --max-line-length Write the length of the longest line.
    --files0-from=F       Read the names of the files from F, NUL-separated.
    --total=WHEN          A line of totals: auto, always, only, never.

Exit status: 0 when every file was read; 1 when one could not be.

## Different in cash

- It is uutils' `wc`, carried inside cash, so it is the same on every machine, with or
  without Git or MSYS2.
