---
name: tac
summary: Print files last line first.
kind: Bundled coreutils, from uutils
see: cat rev sort
---

## Syntax

    tac [OPTION]... [FILE]...

    tac FILE         FILE's lines, the last first
    tac -s, FILE     records ending in commas, the last first

## Examples

The last line first:

    tac report.txt

A log's newest lines, newest first:

    tac app.log | head -3

Records that end in something else than a newline:

    printf 'one,two,three,' | tac -s,
    echo

## How it works

`tac` writes the lines of its files, or standard input, in reverse: `cat` backwards.
`-s SEP` ends each record with SEP instead of a newline, `-b` puts the separator before
each record instead of after, and `-r` reads SEP as a regular expression.

## Options

    -s, --separator=STRING    Records end in STRING, not a newline.
    -b, --before              The separator comes before each record.
    -r, --regex               STRING is a regular expression.

Exit status: 0, or 1 for a file that cannot be read.
